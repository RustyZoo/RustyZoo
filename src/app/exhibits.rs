//! The exhibits in the game: what the world tells about them is logged, a new one asks the player
//! for its name before anything else (a modal dialog, so the game stands still), and the message
//! about a gate no keeper can reach comes after it (exhibits and money 3.4, 3.6).

use super::*;
use crate::config::Config;
use crate::text::from_cp1252;
use crate::timing::ini_whole;
use crate::world::Event;

/// The layout of the name dialog, its message, its field and the string of its message.
const NAME_DIALOG: i32 = 45300;
const NAME_FIELD: i32 = 45308;
const NAME_TEXT: i32 = 158;
/// The word of a default name (string 6208 is "Exhibit").
pub(super) const WORD: i32 = 6208;
/// The number of characters of a name field when the ini file has no `defaultEditCharLimit`.
const NAME_LIMIT: i32 = 17;

/// An exhibit waiting for its name, and what to tell once it has one.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Naming {
    id: u32,
    message: Option<i32>,
}

/// `[UI] defaultEditCharLimit` of the ini file; a limit below 1 is no limit, which a name field
/// would not have, so it falls back to the built-in one.
fn name_limit(ini: &[u8]) -> i32 {
    Config::parse(ini)
        .ok()
        .and_then(|c| ini_whole(&c, "UI", "defaultEditCharLimit"))
        .filter(|&n| n > 0)
        .unwrap_or(NAME_LIMIT)
}

impl App<'_> {
    /// Tells what happened to the exhibits since the last call: one `exhibit` line each (`new 1
    /// "Exhibit 1" tiles 9 gate 13,41 side 2`, `split 1 tiles 6`, `removed 1`, `merged 2 into 3
    /// tiles 12`, `gate 1 13,41 side 2`), the gate button follows their number, and each new
    /// exhibit queues its name dialog.
    pub(super) fn exhibits_told(&mut self) {
        let Some(game) = &mut self.game else { return };
        let events = game.world.take_events();
        let world = &game.world;
        let slot = |id: u32| match world.gate_slot(id) {
            Some((t, d)) => format!(
                "gate {},{} side {d}",
                t % world.zoo.width as usize,
                t / world.zoo.width as usize
            ),
            None => "gate none".to_string(),
        };
        let tiles = |id: u32| world.exhibits().get(id).map_or(0, |e| e.tiles);
        let new: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                Event::New { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        for event in &events {
            let line = match event {
                Event::New { id, .. } => {
                    let name = world.exhibits().get(*id).map_or("", |e| &e.name);
                    format!("new {id} {name:?} tiles {} {}", tiles(*id), slot(*id))
                }
                Event::Split(id) => format!("split {id} tiles {}", tiles(*id)),
                Event::Removed(id) => format!("removed {id}"),
                Event::Merged { kept, gone } => {
                    format!("merged {gone} into {kept} tiles {}", tiles(*kept))
                }
                Event::Gate(id) if !new.contains(id) => format!("{} {id}", slot(*id)),
                _ => continue,
            };
            self.exhibit_lines.push(line);
        }
        let count = world.exhibits().list().len();
        for event in events {
            match event {
                Event::New { id, ask: true } => game.naming.push(Naming { id, message: None }),
                Event::Blocked { id, message } => match game.naming.iter_mut().find(|n| n.id == id)
                {
                    Some(n) => n.message = Some(message),
                    None => self.exhibit_messages.push((id, message)),
                },
                _ => {}
            }
        }
        if let Some(ui) = &mut self.ui {
            game.build.set_exhibits(ui, count);
        }
        self.name_next();
        self.exhibit_message();
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
        let Some(first) = game.naming.first() else {
            return;
        };
        let name = game.world.exhibits().get(first.id).map_or("", |e| &e.name);
        let text = to_cp1252(&ui.string(NAME_TEXT));
        if ui.ask(NAME_DIALOG, &text, Buttons::OK) {
            ui.set_char_limit(NAME_FIELD, name_limit(self.renderer.vfs().ini()));
            ui.set_text(NAME_FIELD, &to_cp1252(name));
            ui.focus(NAME_FIELD);
            game.naming_open = true;
        } else {
            // No dialog in this interface: the default name stays.
            game.naming.clear();
        }
    }

    /// The name dialog was answered (OK, Enter or Escape alike): the field's text is the name.
    pub(super) fn named(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        if !std::mem::take(&mut game.naming_open) || game.naming.is_empty() {
            return;
        }
        let first = game.naming.remove(0);
        let text: String = ui
            .text(NAME_FIELD)
            .unwrap_or_default()
            .iter()
            .map(|&b| from_cp1252(b))
            .collect();
        if !text.is_empty() {
            game.world.rename(first.id, text.clone());
            self.exhibit_lines
                .push(format!("name {} {text:?}", first.id));
        }
        if let Some(message) = first.message {
            self.tell_gate(first.id, message);
        }
        self.name_next();
    }
}

#[cfg(test)]
mod tests {
    use super::name_limit;

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
