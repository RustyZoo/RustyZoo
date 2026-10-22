//! The bulldozer in the game (placing 7): what it would remove under the pointer or in the
//! dragged rectangle (tinted red and priced next to the pointer), the click or release that
//! removes it, and the questions asked on the way, one at a time. The rules are in
//! [`crate::place::bulldozer`], the button and the press in [`crate::build`].

use super::*;
use crate::entity::Catalogue;
use crate::place::bulldozer::{self, Check, Question};
use crate::staff;
use crate::text::format::{self, Arg, expand, whole_units};
use std::collections::VecDeque;

/// The strings of the questions: the sell question (it ends with the label of the refund), the
/// fire question (it ends with the label of the monthly salary), the merge of two exhibits, the
/// escape of the animals of an exhibit, the same for a tank (drained, its animals boxed); and the
/// default name of an exhibit.
const SELL: i32 = 154;
const FIRE: i32 = 155;
const MERGE: i32 = 156;
const ESCAPE: i32 = 157;
const DRAIN: i32 = 160;
/// The message of a click on a tank wall between a show tank and another tank.
const NOT_MERGED: i32 = 10571;
const EXHIBIT: i32 = 6208;

/// What the bulldozer would remove now: the entities (positions in `zoo.entities`, valid while
/// the world stands still), their refunds, and the rectangle an area drag outlines.
#[derive(Debug, Default, Clone, PartialEq)]
pub(super) struct Dozer {
    pub(super) at: Vec<usize>,
    /// The object under the pointer (the first of `at` unless a tied wall goes before it).
    head: Option<usize>,
    refund: f32,
    rect: Option<((i32, i32), (i32, i32))>,
    /// Whether the cost text is shown: over something removable, and in a drag from the start
    /// of the drag (if shown then) or from the first sum that is not 0.
    shown: bool,
    /// The pointer is over a tank wall that stays (placing 7.1): a click says why.
    mixed: bool,
}

/// A removal under way: the entities still to go (ids), the pairs of regions asked about, the
/// entity whose question is open, and what has gone so far.
pub(super) struct Sweep {
    queue: VecDeque<u32>,
    asked: Vec<(u32, u32)>,
    asking: Option<u32>,
    area: bool,
    /// A panel's button: the bulldozer's tests (`cDeletable`, the zoo wall) are not made.
    panel: bool,
    removed: usize,
    refund: f32,
}

/// What the removal does next.
enum Next {
    /// Nothing is left.
    Done,
    /// The front of the queue stays: it is gone, cannot be removed, or its pair was asked already.
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
            let mut head = None;
            let found: Vec<usize> = match rect {
                Some((a, b)) => bulldozer::area(world, a, b),
                // The last drawn entity with a pixel under the pointer (placing 7.1), and the
                // tied wall that goes with it.
                None => {
                    head = game
                        .staged
                        .pick(game.camera.view_point(pointer), bulldozer::WIDEN, |id| {
                            world.find(id).is_some()
                        })
                        .and_then(|id| world.find(id));
                    (head.into_iter())
                        .flat_map(|e| bulldozer::with_tied(world, e))
                        .collect()
                }
            };
            let mixed = rect.is_none() && found.iter().any(|&e| bulldozer::mixed_tanks(world, e));
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
            let head = head.filter(|e| at.contains(e)).or(at.first().copied());
            game.dozer = Dozer {
                at,
                head,
                refund,
                rect,
                shown,
                mixed,
            };
        }
        if before == game.dozer {
            return;
        }
        let d = &game.dozer;
        let whole = whole_units(f64::from(d.refund));
        let single = d.head.as_ref().filter(|_| d.rect.is_none());
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
        if game.dozer.mixed {
            if let Some(ui) = &mut self.ui {
                ui.message(&to_cp1252(&ui.string(NOT_MERGED)));
            }
            self.tool_lines.push(format!("tool message {NOT_MERGED}"));
        }
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
            asked: Vec::new(),
            asking: None,
            area: game.dozer.rect.is_some(),
            panel: false,
            removed: 0,
            refund: 0.0,
        });
        self.sweep_step();
    }

    /// The fire button of the staff panel and the sell button of the building panel: the entity
    /// with this id is the one entity of a removal, which asks its question like the bulldozer's
    /// (unit placing 10.1, placing 7) but is not tested for `cDeletable` or the zoo wall. The
    /// selection is cleared first, which closes the panel, so the entity a No keeps is no longer
    /// selected (placing 7.1).
    pub(super) fn remove_one(&mut self, id: u32) {
        let Some(game) = &mut self.game else { return };
        game.selected = None;
        game.sweep = Some(Sweep {
            queue: VecDeque::from([id]),
            asked: Vec::new(),
            asking: None,
            area: false,
            panel: true,
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
            .filter(|&at| sweep.panel || bulldozer::removable(world, cat, at))
        else {
            return Next::Skip;
        };
        match bulldozer::check(world, cat, at, game.confirm, &sweep.asked) {
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
                Next::Remove(id) => {
                    self.bulldoze_remove(id);
                    // A removal that makes a tank with objects in it asks about them: the rest
                    // of the removal is dropped.
                    if let Some(game) = self.game.as_mut().filter(|g| g.refused.is_some()) {
                        game.sweep = None;
                        return;
                    }
                }
                Next::Skip => {}
                Next::Ask(id, question, text) => {
                    let asked = self
                        .ui
                        .as_mut()
                        .is_some_and(|ui| ui.ask(QUESTION, &to_cp1252(&text), Buttons::YES_NO));
                    if asked {
                        if let Some(sweep) = self.game.as_mut().and_then(|g| g.sweep.as_mut()) {
                            sweep.asking = Some(id);
                            // The pair is stored when the question is reached, before the
                            // answer: reaching it again in this click is a silent No.
                            sweep.asked.extend(question.pair());
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
    /// SUBTYPE X,Y [side S] id N refund R`), then tells what happened to the exhibits.
    pub(super) fn bulldoze_remove(&mut self, id: u32) {
        let Some(game) = &mut self.game else { return };
        let cat = self.renderer.catalogue();
        // The animals of the exhibit react to an object, an animal or a staff member removed
        // (habitat rating 9.4); an object or an animal decides an unchanged sum by its value, negated.
        let object = game.world.find(id).and_then(|at| {
            use crate::render::order::Kind;
            let e = &game.world.zoo.entities[at];
            let c = e.common.as_ref()?;
            let tile = game
                .world
                .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
            let kind = Kind::of_class(&e.class);
            let ty = bulldozer::type_of(cat, e)
                .filter(|_| matches!(kind, Kind::Fixed | Kind::Path | Kind::Unit));
            ty.map(|ty| {
                let (habitat, name) = rating::object(ty);
                let change = match kind {
                    Kind::Unit if place::unit::is_animal(ty) => Some(rating::Change::Animal {
                        ids: rating::Likes::of(ty).ids,
                        sign: -1,
                    }),
                    Kind::Unit => None,
                    _ => Some(rating::Change::Object {
                        habitat,
                        name,
                        sign: -1,
                    }),
                };
                (rating::before(&game.world, cat, &[tile]), change)
            })
        });
        let removed =
            bulldozer::remove(&mut game.world, &mut game.money, cat, id, &mut self.sim.rng);
        let Some(r) = removed else { return };
        if let Some((before, change)) = object {
            let rand = &mut || self.sim.rng.rand();
            rating::react(&mut game.world, cat, before, change, rand);
        }
        let side = r.side.map_or(String::new(), |s| format!(" side {s}"));
        self.world_lines.push(format!(
            "remove {} {},{}{side} id {} refund {}",
            r.name,
            r.tile.0,
            r.tile.1,
            r.id,
            whole_units(f64::from(r.refund))
        ));
        if let Some(sweep) = &mut game.sweep {
            sweep.removed += 1;
            sweep.refund += r.refund;
        }
        self.exhibits_told();
    }

    /// The open question was answered: Yes removes the object, No keeps it (the other pieces
    /// between the same two exhibits stay, their pair being stored already), anything else stops
    /// the removal.
    pub(super) fn sweep_answered(&mut self, code: i32) {
        let Some(game) = &mut self.game else { return };
        let Some(id) = game.sweep.as_mut().and_then(|s| s.asking.take()) else {
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
            7 => {}
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

/// The text of a question (placing 7.1). An exhibit is named by its name; one without (found in a
/// map) by the word and its number.
pub(super) fn question_text(
    ui: &Interface,
    cat: &Catalogue,
    world: &World,
    at: usize,
    question: Question,
) -> String {
    let template = |id| ui.string(id);
    let exhibit = |n: u32| match world.exhibits().get(n) {
        Some(e) if !e.name.is_empty() => e.name.clone(),
        _ => format!("{} {n}", ui.string(EXHIBIT)),
    };
    match question {
        Question::Sell | Question::Fire => {
            let entity = world.zoo.entities.get(at);
            let ty = entity.and_then(|e| bulldozer::type_of(cat, e));
            let name = ty.map_or(String::new(), |t| ui.string(t.name_id));
            // The refund of a sale, or the monthly salary of the staff member fired; the dialog
            // shows it cut to whole units, the cash gets the exact amount.
            let (id, amount) = match question {
                Question::Fire => (FIRE, ty.map_or(0.0, staff::monthly)),
                _ => (SELL, entity.map_or(0.0, |e| bulldozer::refund(cat, e))),
            };
            let text = expand(&Plain, &template(id), &[Arg::Text(&name)]);
            format!(
                "{text}{}",
                Symbol::new(ui.string(SYMBOL)).text(amount as i64)
            )
        }
        Question::Merge(x, y) => expand(
            &Plain,
            &template(MERGE),
            &[Arg::Text(&exhibit(x)), Arg::Text(&exhibit(y))],
        ),
        Question::Escape(x) => {
            let tank = world.exhibits().get(x).is_some_and(|e| e.tank.is_some());
            template(if tank { DRAIN } else { ESCAPE })
        }
    }
}
