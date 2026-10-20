//! The guest panel (information panels 3.3, 8; guests 5.5): the name, the five bars of happiness,
//! drink, food, restroom and energy, the time in the park, the favourite species, the items held
//! and the guest's five newest thoughts, refreshed every 10 s of the interface clock while shown
//! and not while the game is paused. There is no mood line (13.1).
//!
//! [`Panel`] holds the selected guest and the clock; [`Panel::refresh`] writes the layout from the
//! world and returns the `panel` log line when what it shows changed.

use crate::entity::Catalogue;
use crate::guest::{self, Guest};
use crate::panel::animal::percent;
use crate::panel::exhibit::{Clock, type_at};
use crate::panel::follow::set_follow_icon;
use crate::status::Words;
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::timing::Date;
use crate::ui::interface::Interface;
use crate::ui::widgets::Row;
use crate::unit::Part;
use crate::vfs::Vfs;
use crate::world::World;

/// The layout of the panel, the name field, the buttons that follow the guest and pick it up, and
/// the tab buttons (each tab forces a refresh).
pub const LAYOUT: i32 = 9;
pub const NAME: i32 = 3402;
pub const FOLLOW: i32 = 3415;
pub const PICK_UP: i32 = 3416;
pub const TABS: [i32; 3] = [3462, 3464, 3465];
/// The bars: happiness, drink, food, restroom and energy.
const BARS: [i32; 5] = [3404, 3406, 3408, 3410, 3412];
/// The texts of the time in the park and of the favourite species, the five item pictures and the
/// thoughts list.
const TIME: i32 = 3475;
const FAVOURITE: i32 = 3476;
const ITEMS: [i32; 5] = [3481, 3482, 3483, 3484, 3485];
const THOUGHTS: i32 = 3469;
/// The strings of the four times in the park.
const TIME_STRING: i32 = 3420;
/// The interface clock time between two refreshes (ms).
pub const PERIOD: u64 = 10_000;
/// The hours of calendar time up to which a guest has "just arrived", stayed "a short time" and
/// "a while".
const BANDS: [i64; 3] = [1440, 5760, 8640];
const HOUR: i64 = 3_600_000;

/// What the bars show (8.2): happiness as a percentage of its range, and each need as 100 minus
/// its percentage, so a full bar is a satisfied guest. The energy's upper limit is `max_energy`.
pub fn bars(g: &Guest, max_energy: i32) -> [i32; 5] {
    let need = |v: i32, hi: i32| 100i32.saturating_sub(percent(v, 0, hi));
    let v = &g.values;
    [
        percent(v[guest::HAPPINESS], -100, 100),
        need(v[guest::THIRST], 100),
        need(v[guest::HUNGER], 100),
        need(v[guest::BATHROOM], 100),
        need(v[guest::ENERGY], max_energy),
    ]
}

/// The time in the park (8.3): the index of its text, from the hours of calendar time since the
/// arrival date; a guest with no arrival date has just arrived.
pub fn stay(arrived: Option<Date>, now: Date) -> usize {
    let hours = arrived.map_or(0, |a| (now.millis() - a.millis()) / HOUR);
    BANDS.iter().filter(|&&b| hours >= b).count()
}

/// The selected guest and the refresh clock.
#[derive(Debug, Default)]
pub struct Panel {
    /// The selected guest, by entity id.
    pub selected: Option<u32>,
    pub clock: Clock,
    /// Just selected: the next refresh also sets the name field and the follow button.
    fresh: bool,
    told: String,
}

impl Panel {
    /// Selects the guest with entity id `id`: the panel is filled at once (4.3).
    pub fn select(&mut self, id: u32) {
        self.selected = Some(id);
        self.fresh = true;
        self.clock.force = true;
        self.told.clear();
    }

    /// Fills the panel again from the start: the name field shows the guest's name.
    pub fn reselect(&mut self) {
        if let Some(id) = self.selected {
            self.select(id);
        }
    }

    pub fn deselect(&mut self) {
        self.selected = None;
        self.told.clear();
    }

    /// Writes the panel for the selected guest at calendar date `now` and returns the log line
    /// when something it shows changed.
    pub fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        now: Date,
    ) -> Option<String> {
        let id = self.selected?;
        let (at, unit) = (world.find(id)?, world.units.get(id)?);
        let Part::Guest(g) = &unit.part else {
            return None;
        };
        let ty = type_at(world, cat, at);
        let max_energy = guest::keys(world, cat, at).map_or(100, |k| k.max_energy);
        let shown = bars(g, max_energy);
        let none = Words::new(|_| String::new());
        for (&bar, value) in BARS.iter().zip(shown) {
            ui.set_status(bar, value);
            let tip = format::expand(&none, &ui.template(bar), &[Arg::Int(i64::from(value))]);
            ui.set_string(bar, tip);
        }
        let time = ui.string(TIME_STRING + stay(g.arrived, now) as i32);
        ui.set_text(TIME, &to_cp1252(&time));
        let favourite = g.favourite.map(|f| ui.string(f)).unwrap_or_default();
        ui.set_text(FAVOURITE, &to_cp1252(&favourite));
        // The items held fill the pictures from the left, each with its name as tooltip.
        let held: Vec<i32> = g.items.iter().flatten().copied().collect();
        for (n, &element) in ITEMS.iter().enumerate() {
            let item = held.get(n).copied();
            let icon = item
                .and_then(|id| cat.items.values().find(|i| i.id == id))
                .filter(|i| !i.icon.is_empty());
            let picture = icon.and_then(|i| ui.tree.picture(vfs, &i.icon));
            ui.set_picture(element, picture);
            if let Some(i) = ui.tree.find(element) {
                ui.tree.elements[i].help_id = item.into_iter().collect();
            }
        }
        let name_of = |id: u32| {
            let at = world.find(id)?;
            let named = world.zoo.entities[at]
                .common
                .as_ref()
                .map(|c| c.name.clone());
            named
                .filter(|n| !n.is_empty())
                .or_else(|| type_at(world, cat, at).map(|t| ui.string(t.name_id)))
        };
        let texts: Vec<String> = (world.units.thoughts.of(id).into_iter())
            .map(|t| {
                let template = ui.string(t.text);
                match t.object.and_then(name_of) {
                    Some(name) => format::expand(&none, &template, &[Arg::Text(&name)]),
                    None => template,
                }
            })
            .collect();
        let rows = (texts.iter())
            .map(|t| Row {
                text: to_cp1252(t),
                ..Row::default()
            })
            .collect();
        ui.refill_rows(THOUGHTS, rows);
        let name = world.zoo.entities[at]
            .common
            .as_ref()
            .map_or("", |c| &c.name);
        let fresh = std::mem::take(&mut self.fresh);
        if fresh {
            ui.set_text(NAME, &to_cp1252(name));
            ui.set_button(FOLLOW, false, !unit.carried);
        }
        if fresh || self.clock.forced {
            set_follow_icon(ui, vfs, FOLLOW, ty);
        }
        let moveable =
            ty.is_none_or(|t| crate::build::number(t, "cMoveable").is_none_or(|v| v != 0.0));
        ui.set_enabled(PICK_UP, moveable && !unit.carried);
        let [happiness, drink, food, restroom, energy] = shown;
        let told = format!(
            "guest {id} {name:?} happiness {happiness} drink {drink} food {food} \
             restroom {restroom} energy {energy} time {time:?} favourite {favourite:?} \
             thoughts {texts:?}"
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
