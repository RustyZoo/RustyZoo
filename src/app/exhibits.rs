//! The exhibits in the game: what the world tells about them is logged, and a new one asks the
//! player for its name inside the purchase of the piece that closed it (a modal dialog, so the
//! game stands still and the rest of the drag waits). After the answer come the exhibit in the
//! list, its gate with the message about a gate no keeper can reach, and the rest of the drag
//! (exhibits and money 3.2, 3.4, 3.6).

use super::*;
use crate::config::Config;
use crate::render::order::Kind;
use crate::text::from_cp1252;
use crate::timing::ini_whole;
use crate::world::Event;

/// The layout of the name dialog, its message, its field and the string of its message.
const NAME_DIALOG: i32 = 45300;
const NAME_FIELD: i32 = 45308;
const NAME_TEXT: i32 = 158;
/// The question about the objects a new tank does not allow.
const TANK_OBJECTS: i32 = 10552;
/// The words of a default name, from this string on: exhibit, tank and show tank.
pub(super) const WORD: i32 = 6208;
/// The number of characters of a name field when the ini file has no `defaultEditCharLimit`.
const NAME_LIMIT: i32 = 17;

/// `[UI] defaultEditCharLimit` of the ini file; a limit below 1 is no limit, which a name field
/// would not have, so it falls back to the built-in one.
fn name_limit(ini: &[u8]) -> i32 {
    Config::parse(ini)
        .ok()
        .and_then(|c| ini_whole(&c, "UI", "defaultEditCharLimit"))
        .filter(|&n| n > 0)
        .unwrap_or(NAME_LIMIT)
}

/// Whether a name holds a letter, digit or punctuation mark (exhibits and money 3.6).
fn visible(bytes: &[u8]) -> bool {
    bytes.iter().any(u8::is_ascii_graphic)
}

impl App<'_> {
    /// Tells what happened to the exhibits since the last call: one `exhibit` line each (`new 1
    /// "Exhibit 1" tiles 9 gate none`, `split 1 tiles 6`, `removed 1`, `merged 2 into 3 tiles 12`,
    /// `gate 1 13,41 side 2`), the gate button follows the number of exhibits that are named, and
    /// each new exhibit queues its name dialog.
    pub(super) fn exhibits_told(&mut self) {
        let Some(game) = &mut self.game else { return };
        let events = game.world.take_events();
        let world = &game.world;
        let slot = |id: u32| match world.gate_slot(id) {
            Some((t, d)) => format!(
                "{},{} side {d}",
                t % world.zoo.width as usize,
                t / world.zoo.width as usize
            ),
            None => "none".to_string(),
        };
        let tiles = |id: u32| world.exhibits().get(id).map_or(0, |e| e.tiles);
        for event in &events {
            let line = match event {
                Event::New { id, .. } => {
                    let name = world.exhibits().get(*id).map_or("", |e| &e.name);
                    format!("new {id} {name:?} tiles {} gate {}", tiles(*id), slot(*id))
                }
                Event::Split(id) => format!("split {id} tiles {}", tiles(*id)),
                Event::Removed(id) => format!("removed {id}"),
                Event::Merged { kept, gone } => {
                    format!("merged {gone} into {kept} tiles {}", tiles(*kept))
                }
                Event::Gate(id) => format!("gate {id} {}", slot(*id)),
                Event::Tank(id) => match world.tank(*id) {
                    Some(t) => format!(
                        "tank {id} height {} level {} show {} salt {} purity {} filled {}",
                        t.height,
                        t.level,
                        u8::from(t.show),
                        u8::from(t.salt),
                        t.purity,
                        u8::from(t.filled)
                    ),
                    None => continue,
                },
                _ => continue,
            };
            self.exhibit_lines.push(line);
        }
        for event in events {
            match event {
                Event::New { id, ask: true } => game.naming.push(id),
                Event::Blocked { id, message } => self.exhibit_messages.push((id, message)),
                Event::Objects(id) => game.refused = Some(id),
                _ => {}
            }
        }
        // An exhibit that is being named, or a tank that is being asked about, is not in the list yet.
        let count = (game.world.exhibits().list().iter())
            .filter(|e| !game.naming.contains(&e.id) && game.refused != Some(e.id))
            .count();
        if let Some(ui) = &mut self.ui {
            game.build.set_exhibits(ui, count, &mut self.tool_lines);
        }
        self.tank_ask();
        self.name_next();
        self.exhibit_message();
    }

    /// Asks about the objects of a new tank, if it has some and no question is open (3.9).
    fn tank_ask(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if game.refused.is_none() || self.asked.is_some() {
            return;
        }
        let text = to_cp1252(&ui.string(TANK_OBJECTS));
        if ui.ask(QUESTION, &text, Buttons::YES_NO) {
            self.asked = Some(Asked::Tank);
            self.tool_lines.push("tool tank ask".into());
        } else {
            // No dialog in this interface: the objects go.
            self.tank_answered(6);
        }
    }

    /// The question was answered: Yes sells the scenery, moves the animals and the staff out
    /// and goes on to the name of the tank; anything else undoes the operation that made it.
    pub(super) fn tank_answered(&mut self, code: i32) {
        match code {
            6 => self.tank_yes(),
            _ => self.tank_no(),
        }
    }

    fn tank_yes(&mut self) {
        let Some(game) = &mut self.game else { return };
        let Some(id) = game.refused.take() else {
            return;
        };
        self.tool_lines.push("tool tank answer yes".into());
        let world = &game.world;
        let outside = world.outside(id);
        // (id, a unit, a unit that moves): an animal or a staff member goes to the tile outside,
        // a guest stays, the rest is sold.
        let objects: Vec<(u32, bool, bool)> = (world.refused(id).into_iter())
            .filter_map(|e| {
                let at = world.find(e)?;
                let unit = world.shape(at)?.kind == Kind::Unit;
                Some((e, unit, unit && world.zoo.entities[at].class != "guests"))
            })
            .collect();
        for (e, unit, moves) in objects {
            match (unit, moves, outside) {
                (true, true, Some(t)) => {
                    if let Some(game) = &mut self.game {
                        game.world.move_unit(e, t);
                    }
                }
                (true, ..) => {}
                _ => self.bulldoze_remove(e),
            }
        }
        if let Some(game) = &mut self.game {
            game.world.accept_tank(id);
        }
        self.exhibits_told();
    }

    /// The operation that made the tank is undone and what it cost comes back (placing 8): every
    /// piece it placed gives its whole price back, every piece it removed takes its refund back.
    pub(super) fn tank_no(&mut self) {
        let Some(game) = &mut self.game else { return };
        let Some(id) = game.refused.take() else {
            return;
        };
        self.tool_lines.push("tool tank answer no".into());
        let done = game.world.decline_tank(id);
        game.rest.clear();
        self.undone(done);
    }

    /// Tells the messages about gates of exhibits that have no dialog.
    fn exhibit_message(&mut self) {
        let messages = std::mem::take(&mut self.exhibit_messages);
        for (id, message) in messages {
            self.tell_gate(id, message);
        }
    }

    /// Shows message `message`, with the name of exhibit `id` in it, in the message line.
    fn tell_gate(&mut self, id: u32, message: i32) {
        let (Some(game), Some(ui)) = (&self.game, &mut self.ui) else {
            return;
        };
        let name = game.world.exhibits().get(id).map_or("", |e| &e.name);
        let text = ui.string(message).replacen("%s", name, 1);
        ui.message(&to_cp1252(&text));
        self.tool_lines.push(format!("tool message {message}"));
    }

    /// Opens the name dialog of the first exhibit waiting, if none is open: its default name in
    /// the field, which has the focus.
    pub(super) fn name_next(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        if game.naming_open {
            return;
        }
        let Some(&first) = game.naming.first() else {
            return;
        };
        let name = game.world.exhibits().get(first).map_or("", |e| &e.name);
        let text = to_cp1252(&ui.string(NAME_TEXT));
        if ui.ask(NAME_DIALOG, &text, Buttons::OK) {
            ui.set_char_limit(NAME_FIELD, name_limit(self.renderer.vfs().ini()));
            ui.set_text(NAME_FIELD, &to_cp1252(name));
            ui.focus(NAME_FIELD);
            game.naming_open = true;
        } else {
            // No dialog in this interface: the default names stay.
            for id in std::mem::take(&mut game.naming) {
                game.world.give_gate(id);
            }
            self.exhibits_told();
        }
    }

    /// The name dialog was answered (OK, Enter or Escape alike): the field's text is the name
    /// when it holds a visible character. Then the exhibit gets its gate, and the rest of the
    /// drag is bought.
    pub(super) fn named(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        if !std::mem::take(&mut game.naming_open) || game.naming.is_empty() {
            return;
        }
        let id = game.naming.remove(0);
        let bytes = ui.text(NAME_FIELD).unwrap_or_default();
        if visible(&bytes) {
            let text: String = bytes.iter().map(|&b| from_cp1252(b)).collect();
            self.exhibit_lines.push(format!("name {id} {text:?}"));
            game.world.rename(id, text);
        }
        game.world.give_gate(id);
        self.exhibits_told();
        let Some(game) = &mut self.game else { return };
        if !game.naming_open {
            let rest = std::mem::take(&mut game.rest);
            self.buy(rest, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{name_limit, visible};

    #[test]
    fn a_name_needs_a_visible_ascii_character() {
        assert!(visible(b"Lion"));
        assert!(visible(b"  7 "));
        assert!(visible(b"-"));
        assert!(!visible(b""), "empty");
        assert!(!visible(b"   "), "spaces");
        assert!(!visible(b"\t\xa0\xe9"), "controls and bytes above 127");
    }

    #[test]
    fn the_name_limit_is_the_ini_value_or_the_built_in_one() {
        let limit = |text: &str| name_limit(text.as_bytes());
        assert_eq!(limit(""), 17, "missing");
        assert_eq!(limit("[UI]\ndefaultEditCharLimit=22\n"), 22);
        assert_eq!(
            limit("[ui]\nDEFAULTEDITCHARLIMIT = 30\n"),
            30,
            "case does not count"
        );
        assert_eq!(limit("[UI]\ndefaultEditCharLimit=0\n"), 17);
        assert_eq!(limit("[UI]\ndefaultEditCharLimit=-4\n"), 17);
        assert_eq!(limit("[UI]\ndefaultEditCharLimit=abc\n"), 17);
        assert!(
            limit("[UI]\ndefaultEditCharLimit=99999999999999\n") > 0,
            "huge"
        );
    }
}
