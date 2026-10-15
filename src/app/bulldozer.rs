//! The bulldozer in the game (placing 7): what it would remove under the pointer or in the
//! dragged rectangle (tinted red and priced next to the pointer), the click or release that
//! removes it, and the questions asked on the way, one at a time. The rules are in
//! [`crate::place::bulldozer`], the button and the press in [`crate::build`].

use super::*;
use crate::entity::Catalogue;
use crate::place::bulldozer::{self, Check, Question};
use crate::text::format::{self, Arg, expand, whole_units};
use std::collections::VecDeque;

/// The strings of the questions: the sell question (it ends with "Refund: "), the merge of two
/// exhibits, the escape of the animals of an exhibit; and the default name of an exhibit.
const SELL: i32 = 154;
const MERGE: i32 = 156;
const ESCAPE: i32 = 157;
const EXHIBIT: i32 = 6208;

/// What the bulldozer would remove now: the entities (positions in `zoo.entities`, valid while
/// the world stands still), their refunds, and the rectangle an area drag outlines.
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Dozer {
    pub(super) at: Vec<usize>,
    refund: f32,
    rect: Option<((i32, i32), (i32, i32))>,
    /// Whether the cost text is shown: over something removable, and in a drag from the start
    /// of the drag (if shown then) or from the first sum that is not 0.
    shown: bool,
}

/// A removal under way: the entities still to go (ids), the pairs of regions answered No, the
/// question that is open, and what has gone so far.
pub(super) struct Sweep {
    queue: VecDeque<u32>,
    declined: Vec<(usize, usize)>,
    asking: Option<(u32, Question)>,
    area: bool,
    removed: usize,
    refund: f32,
}

/// What the removal does next.
enum Next {
    /// Nothing is left.
    Done,
    /// The front of the queue stays: it is gone, cannot be removed, or its pair was declined.
    Skip,
    Remove(u32),
    Ask(u32, Question, String),
}

/// The templates have no `%r`.
struct Plain;

impl format::Strings for Plain {
    fn string(&self, _: u32) -> &str {
        ""
    }
}

impl Game {
    /// The amount next to the pointer while the bulldozer is the tool: what it would refund, or
    /// nothing over anything it cannot remove (a drag shows its sum, even 0, once it is shown).
    pub(super) fn dozer_amount(&self) -> Option<i64> {
        let d = &self.dozer;
        d.shown.then(|| whole_units(f64::from(d.refund)))
    }

    /// The transient items of the bulldozer: what it would remove, tinted, and the outline of
    /// the rectangle of a drag.
    pub(super) fn dozer_extra(&self, extra: &mut Extra) {
        extra
            .tinted
            .extend(self.dozer.at.iter().map(|&e| (e, self.tints.delete)));
        if let Some((a, b)) = self.dozer.rect {
            for ty in a.1.min(b.1)..=a.1.max(b.1) {
                for tx in a.0.min(b.0)..=a.0.max(b.0) {
                    if let Some(tile) = self.world.tile_index(tx, ty) {
                        extra.marks.push(Mark {
                            tile,
                            color: crate::terrain::SELECTION,
                        });
                    }
                }
            }
        }
    }
}

impl App<'_> {
    /// Works out what the bulldozer would remove from the pointer (and the press, if one is
    /// held), tells it when it changed (`tool bulldoze none`, `tool bulldoze CLASS/TYPE/SUBTYPE
    /// id N refund R name=NAME`, `tool bulldoze area X,Y X,Y objects N refund R`) and has the
    /// first hovered object's name shown as a tooltip.
    pub(super) fn dozer_aim(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let before = std::mem::take(&mut game.dozer);
        if game.build.bulldozing() && game.hover {
            let world = &game.world;
            let pointer = ui.pointer();
            let tile = game
                .camera
                .tile_under(&world.zoo, pointer)
                .and_then(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)));
            let rect = game.build.sweep_area(tile, pointer);
            let found: Vec<usize> = match rect {
                Some((a, b)) => bulldozer::area(world, a, b),
                None => {
                    let (x, y, _) = game.camera.pick(&world.zoo, pointer);
                    bulldozer::under(world, (x, y)).into_iter().collect()
                }
            };
            let at: Vec<usize> = found
                .into_iter()
                .filter(|&e| bulldozer::removable(world, cat, e))
                .collect();
            let refund = at
                .iter()
                .map(|&e| bulldozer::refund(cat, &world.zoo.entities[e]))
                .sum();
            let shown = match rect {
                Some(_) => before.shown || refund != 0.0,
                None => !at.is_empty(),
            };
            game.dozer = Dozer {
                at,
                refund,
                rect,
                shown,
            };
        }
        if before == game.dozer {
            return;
        }
        let d = &game.dozer;
        let whole = whole_units(f64::from(d.refund));
        let single = d.at.first().filter(|_| d.rect.is_none());
        let entity = single.map(|&e| &game.world.zoo.entities[e]);
        let ty = entity.and_then(|e| bulldozer::type_of(cat, e));
        ui.help_tip(ty.map_or(0, |t| t.name_id));
        let line = match (d.rect, entity) {
            (Some((a, b)), _) => format!(
                "bulldoze area {},{} {},{} objects {} refund {whole}",
                a.0,
                a.1,
                b.0,
                b.1,
                d.at.len()
            ),
            (None, Some(e)) => format!(
                "bulldoze {}/{}/{} id {} refund {whole} name={}",
                e.class,
                e.type_name,
                e.subtype,
                e.common.as_ref().map_or(0, |c| c.id),
                ty.map_or(String::new(), |t| ui.string(t.name_id))
            ),
            (None, None) => "bulldoze none".to_string(),
        };
        self.tool_lines.push(format!("tool {line}"));
        ui.touch();
    }

    /// The release of a bulldozer press on the map: the hovered object, or the objects of the
    /// dragged rectangle, are removed one after the other, each after its question if it has one.
    pub(super) fn bulldoze_release(&mut self) {
        let Some(game) = &mut self.game else { return };
        game.build.end_sweep();
        let queue: VecDeque<u32> = game
            .dozer
            .at
            .iter()
            .filter_map(|&e| {
                game.world
                    .zoo
                    .entities
                    .get(e)?
                    .common
                    .as_ref()
                    .map(|c| c.id)
            })
            .collect();
        game.sweep = Some(Sweep {
            queue,
            declined: Vec::new(),
            asking: None,
            area: game.dozer.rect.is_some(),
            removed: 0,
            refund: 0.0,
        });
        self.sweep_step();
    }

    /// What the removal does next: the front of the queue, looked at.
    fn sweep_next(&mut self) -> Next {
        let (Some(game), Some(ui)) = (&self.game, &self.ui) else {
            return Next::Done;
        };
        let cat = self.renderer.catalogue();
        let Some(sweep) = &game.sweep else {
            return Next::Done;
        };
        let Some(&id) = sweep.queue.front() else {
            return Next::Done;
        };
        let world = &game.world;
        let Some(at) = world
            .find(id)
            .filter(|&at| bulldozer::removable(world, cat, at))
        else {
            return Next::Skip;
        };
        match bulldozer::check(world, cat, at, game.confirm, &sweep.declined) {
            Check::Go => Next::Remove(id),
            Check::Keep => Next::Skip,
            Check::Ask(q) => Next::Ask(id, q, question_text(ui, cat, world, at, q)),
        }
    }

    /// Goes on with the removal until a question is open or nothing is left.
    fn sweep_step(&mut self) {
        loop {
            match self.sweep_next() {
                Next::Done => {
                    if let Some(sweep) = self.game.as_mut().and_then(|g| g.sweep.take())
                        && sweep.area
                        && sweep.removed > 0
                    {
                        self.tool_lines.push(format!(
                            "tool bulldoze removed {} refund {}",
                            sweep.removed,
                            whole_units(f64::from(sweep.refund))
                        ));
                    }
                    return self.aim(false);
                }
                Next::Remove(id) => self.bulldoze_remove(id),
                Next::Skip => {}
                Next::Ask(id, question, text) => {
                    let asked = self
                        .ui
                        .as_mut()
                        .is_some_and(|ui| ui.ask(QUESTION, &to_cp1252(&text), Buttons::YES_NO));
                    if asked {
                        if let Some(sweep) = self.game.as_mut().and_then(|g| g.sweep.as_mut()) {
                            sweep.asking = Some((id, question));
                        }
                        self.asked = Some(Asked::Sweep);
                        self.tool_lines
                            .push(format!("tool bulldoze ask {question:?}"));
                        return;
                    }
                }
            }
            if let Some(sweep) = self.game.as_mut().and_then(|g| g.sweep.as_mut()) {
                sweep.queue.pop_front();
            }
        }
    }

    /// Removes the entity with this id, books its refund and logs it (`world remove CLASS/TYPE/
    /// SUBTYPE X,Y [side S] id N refund R`, and `world gate id N` for the piece that became the
    /// exhibit's gate).
    fn bulldoze_remove(&mut self, id: u32) {
        let Some(game) = &mut self.game else { return };
        let (renderer, cat) = (&self.renderer, self.renderer.catalogue());
        let removed = bulldozer::remove(&mut game.world, &mut game.money, cat, id, |e| {
            renderer.shape(e)
        });
        let Some(r) = removed else { return };
        let side = r.side.map_or(String::new(), |s| format!(" side {s}"));
        self.world_lines.push(format!(
            "remove {} {},{}{side} id {} refund {}",
            r.name,
            r.tile.0,
            r.tile.1,
            r.id,
            whole_units(f64::from(r.refund))
        ));
        if let Some(gate) = r.gate {
            self.world_lines.push(format!("gate id {gate}"));
        }
        if let Some(sweep) = &mut game.sweep {
            sweep.removed += 1;
            sweep.refund += r.refund;
        }
    }

    /// The open question was answered: Yes removes the object, No keeps it (and keeps the other
    /// pieces between the same two exhibits), anything else stops the removal.
    pub(super) fn sweep_answered(&mut self, code: i32) {
        let Some(game) = &mut self.game else { return };
        let Some((id, question)) = game.sweep.as_mut().and_then(|s| s.asking.take()) else {
            return;
        };
        self.tool_lines.push(format!(
            "tool bulldoze answer {}",
            match code {
                6 => "yes",
                7 => "no",
                _ => "cancel",
            }
        ));
        match code {
            6 => self.bulldoze_remove(id),
            7 => {
                if let Some(pair) = question.pair(&game.world)
                    && let Some(sweep) = &mut game.sweep
                {
                    sweep.declined.push(pair);
                }
            }
            _ => {
                game.sweep = None;
                return self.aim(false);
            }
        }
        if let Some(sweep) = self.game.as_mut().and_then(|g| g.sweep.as_mut()) {
            sweep.queue.pop_front();
        }
        self.sweep_step();
    }
}

/// The text of a question (placing 7.1). An exhibit has no name of its own yet: its default
/// name is the word and its number.
fn question_text(
    ui: &Interface,
    cat: &Catalogue,
    world: &World,
    at: usize,
    question: Question,
) -> String {
    let template = |id| ui.string(id);
    let exhibit = |n: u32| format!("{} {n}", ui.string(EXHIBIT));
    match question {
        Question::Sell => {
            let entity = world.zoo.entities.get(at);
            let name = entity
                .and_then(|e| bulldozer::type_of(cat, e))
                .map_or(String::new(), |t| ui.string(t.name_id));
            let refund = entity.map_or(0.0, |e| bulldozer::refund(cat, e));
            let text = expand(&Plain, &template(SELL), &[Arg::Text(&name)]);
            // The dialog shows the refund cut to whole units; the cash gets the exact amount.
            format!(
                "{text}{}",
                Symbol::new(ui.string(SYMBOL)).text(refund as i64)
            )
        }
        Question::Merge(x, y) => expand(
            &Plain,
            &template(MERGE),
            &[Arg::Text(&exhibit(x)), Arg::Text(&exhibit(y))],
        ),
        Question::Escape(_) => template(ESCAPE),
    }
}
