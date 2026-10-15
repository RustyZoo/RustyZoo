//! The undo button and the Backspace key (placing 8): the last press and release's
//! operations are taken back by the world, the cash and the accounts follow, and a removed
//! object that cannot be put back is told in a message box.

use super::*;
use crate::money::{CONSTRUCTION, CONSTRUCTION_REFUND};
use crate::place::bulldozer::type_of;
use crate::text::format::whole_units;
use crate::world::{Spot, Step, Undone};

/// The button of the game screen, grey until something can be undone, and the string that tells
/// an object could not be put back (the object's name goes in its `%s`).
pub(super) const UNDO: i32 = 1075;
const BLOCKED: i32 = 159;

/// The log words of an undone step: what the world did (`remove NAME 12,40 id 7 undo`).
fn told(word: &str, spot: &Spot, end: &str) -> String {
    let side = spot.side.map_or(String::new(), |s| format!(" side {s}"));
    format!(
        "{word} {} {},{}{side} id {}{end}",
        spot.name, spot.tile.0, spot.tile.1, spot.id
    )
}

impl App<'_> {
    /// The button follows the undo list: enabled while there is something to take back.
    pub(super) fn undo_button(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let can = game.world.can_undo();
        if can != game.undoable {
            game.undoable = can;
            ui.set_enabled(UNDO, can);
        }
    }

    /// Takes back the last operation. Each action tells a line (`world remove`, `world add`,
    /// `world retype`, `world skip` with `undo` at the end, `terrain undo N` with the price of
    /// the tiles given back, `tool undo N` with the number of actions) and moves its price in
    /// the cash. A removed object that cannot be put back stops the undo (`world blocked`) and
    /// shows string 159 with its name.
    pub(super) fn undo(&mut self) {
        let Some(game) = &mut self.game else { return };
        let done = game.world.undo();
        if !done.is_empty() {
            self.undone(done);
        }
    }

    /// Moves the prices of the steps of an undo in the cash and tells what was done.
    pub(super) fn undone(&mut self, done: Undone) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let cat = self.renderer.catalogue();
        let mut tiles = None;
        for step in &done.steps {
            let line = match step {
                Step::Removed(spot, price) => {
                    game.money.undo_charge(CONSTRUCTION, *price);
                    told("remove", spot, " undo")
                }
                Step::Retyped(spot, price) => {
                    game.money.undo_charge(CONSTRUCTION, *price);
                    told("retype", spot, " undo")
                }
                Step::Back(spot, price) => {
                    game.money.undo_refund(CONSTRUCTION_REFUND, *price);
                    told("add", spot, " undo")
                }
                Step::Skipped(spot) => told("skip", spot, " undo"),
                Step::Tiles(_, price) => {
                    game.money.undo_charge(CONSTRUCTION, *price);
                    *tiles.get_or_insert(0.0) += price;
                    continue;
                }
            };
            self.world_lines.push(line);
        }
        self.tool_lines
            .push(format!("tool undo {}", done.steps.len()));
        if let Some(price) = tiles {
            self.terrain_lines
                .push(format!("undo {}", whole_units(f64::from(price))));
            if game.terrain.mode().is_some() {
                let habits = |e: &Entity| habit(cat, e);
                game.terrain.refresh(&game.world, &habits);
                game.terrain_marks(cat);
            }
        }
        let blocked = done.blocked.map(|entity| {
            let name = type_of(cat, &entity).map_or(String::new(), |t| ui.string(t.name_id));
            let spot = game.world.spot_of(&entity);
            self.world_lines.push(told("blocked", &spot, ""));
            ui.string(BLOCKED).replacen("%s", &name, 1)
        });
        self.exhibits_told();
        self.terrain_refresh();
        self.aim(false);
        if let Some(text) = blocked {
            self.say(&text);
        }
    }
}
