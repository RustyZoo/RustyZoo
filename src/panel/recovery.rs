//! The recovery team panel (staff 18; Dinosaur Digs, layout 1400): what the player gets by
//! selecting the helicopter. The name, the follow button, the monthly cost, the current job, the
//! number of recoveries and the duty icons, refreshed every second of the interface clock while
//! shown and not while the game is paused.
//!
//! [`Panel`] holds the selected helicopter and the clock; [`Panel::refresh`] writes the layout
//! from the world and returns the `panel` log line when what it shows changed.

use crate::entity::Catalogue;
use crate::exhibit::members::type_at;
use crate::money::{SYMBOL, Symbol};
use crate::panel::exhibit::Clock;
use crate::panel::follow::set_follow_icon;
use crate::panel::staff::{job_text, training};
use crate::staff;
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::unit::Part;
use crate::vfs::Vfs;
use crate::world::World;

/// The layout of the panel and its name field, follow button and fire button.
pub(crate) const LAYOUT: i32 = 1400;
pub(crate) const NAME: i32 = 1403;
pub(crate) const FOLLOW: i32 = 1405;
pub(crate) const FIRE: i32 = 1419;
/// The monthly cost, the current job, the recoveries and the five duty icons.
const COST: i32 = 1407;
const DUTY: i32 = 1411;
const RECOVERIES: i32 = 1409;
const ICONS: [i32; 5] = [1413, 1414, 1415, 1416, 1417];
/// The interface clock time between two refreshes (ms).
pub(crate) const PERIOD: u64 = 1000;

/// The selected helicopter and the refresh clock.
#[derive(Debug, Default)]
pub(crate) struct Panel {
    /// The selected helicopter, by entity id.
    pub selected: Option<u32>,
    pub clock: Clock,
    /// Just selected: the next refresh also sets the name field and the follow button.
    fresh: bool,
    told: String,
}

impl Panel {
    /// Selects the helicopter with entity id `id`: the panel is filled at once.
    pub(crate) fn select(&mut self, id: u32) {
        self.selected = Some(id);
        self.fresh = true;
        self.clock.force = true;
        self.told.clear();
    }

    /// Fills the panel again from the start: the name field shows the helicopter's name.
    pub(crate) fn reselect(&mut self) {
        if let Some(id) = self.selected {
            self.select(id);
        }
    }

    pub(crate) fn deselect(&mut self) {
        self.selected = None;
        self.told.clear();
    }

    /// Writes the panel for the selected helicopter and returns the log line when something it
    /// shows changed. The recoveries are the darts of its base, which count the animals it
    /// brought down (staff 7.11.1 point 4.5).
    pub(crate) fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
    ) -> Option<String> {
        let id = self.selected?;
        let (at, unit) = (world.find(id)?, world.units.get(id)?);
        let Part::Staff(team) = &unit.part else {
            return None;
        };
        let ty = type_at(world, cat, at);
        let money = Symbol::new(ui.string(SYMBOL));
        let cost = money.text(ty.map_or(0.0, staff::monthly).round() as i64);
        ui.set_text(COST, &to_cp1252(&cost));
        let duty = job_text(ui, world, cat, unit, ty.map_or(0, |t| t.name_id));
        ui.set_text(DUTY, &to_cp1252(&duty));
        let base = world.find(team.heli.base);
        let base = base.and_then(|b| world.zoo.entities[b].common.as_ref()?.building.as_ref());
        let recoveries = base.map_or(0, |b| b.fill);
        ui.set_text(RECOVERIES, recoveries.to_string().as_bytes());
        let trained = training(ui, vfs, ty, world.research.training_of(ty), &ICONS);
        let name = world.zoo.entities[at].common.as_ref();
        let name = name.map_or("", |c| &c.name);
        let fresh = std::mem::take(&mut self.fresh);
        if fresh {
            ui.set_text(NAME, &to_cp1252(name));
            ui.set_button(FOLLOW, false, true);
        }
        if fresh || self.clock.forced {
            set_follow_icon(ui, vfs, FOLLOW, ty, "idle");
        }
        ui.set_enabled(FOLLOW, true);
        let told = format!(
            "team {id} {name:?} cost {cost} duty {duty:?} recoveries {recoveries}{trained}"
        );
        match told == self.told {
            true => None,
            false => {
                self.told.clone_from(&told);
                Some(told)
            }
        }
    }
}

#[cfg(test)]
mod tests;
