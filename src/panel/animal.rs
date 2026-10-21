//! The animal panel (information panels 3.2, 6; animals 13): the name, the four bars of
//! happiness, hunger, health and the suitability of the exhibit, the sex and the two time rows,
//! refreshed every 5 s of the interface clock while shown and not while the game is paused.
//!
//! [`Panel`] holds the selected animal and the clock; [`Panel::refresh`] writes the layout from
//! the world and returns the `panel` log line when what it shows changed. The thoughts list is the
//! five newest thoughts about the animal, newest first (their ids close the line); the sell and
//! pick-up buttons are enabled as their rules say and their actions are other issues'.

use crate::animal::{self, Animal, Keys};
use crate::build::number;
use crate::entity::Catalogue;
use crate::exhibit::rating;
use crate::panel::exhibit::{Clock, record_exhibit, type_at};
use crate::panel::follow::set_follow_icon;
use crate::panel::guest::{thought_rows, thought_texts};
use crate::status::Words;
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::unit::Part;
use crate::vfs::Vfs;
use crate::world::World;

/// The layout of the panel, the name field, the buttons that follow the animal, show its exhibit
/// and its recommendations, and the tab buttons (each tab forces a refresh).
pub const LAYOUT: i32 = 6;
pub const NAME: i32 = 3102;
pub const FOLLOW: i32 = 3114;
pub const EXHIBIT: i32 = 3197;
pub const RECOMMENDATIONS: i32 = 3122;
pub const TABS: [i32; 3] = [3162, 3164, 3165];
const SELL: i32 = 3111;
const PICK_UP: i32 = 3115;
const THOUGHTS: i32 = 3125;
/// The bars: happiness, hunger, health and suitability.
const BARS: [i32; 4] = [3104, 3108, 3106, 3110];
const SUITABILITY: i32 = 3110;
/// The sex image set, and the labels and texts of the last fed and last slept rows. A text shows
/// the string of its base plus the band of the time ago.
const SEX: i32 = 3172;
const FED: [i32; 2] = [3176, 3177];
const SLEPT: [i32; 2] = [3178, 3179];
const FED_STRING: i32 = 3116;
const SLEPT_STRING: i32 = 3119;
/// The interface clock time between two refreshes (ms).
pub const PERIOD: u64 = 5000;
/// Game milliseconds in an hour, and the hours up to which a time ago is "recently" and "a short
/// time ago".
const HOUR: u64 = 3_600_000;
const RECENT: u64 = 1440;
const SHORT: u64 = 8640;

/// The percentage of `v` between `lo` and `hi`, truncated; 0 when the limits meet.
pub(crate) fn percent(v: i32, lo: i32, hi: i32) -> i32 {
    let span = i64::from(hi) - i64::from(lo);
    let p = (i64::from(v) - i64::from(lo)) * 100 / span.max(1);
    p.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// What the bars show: the values of happiness, hunger, health and suitability, and the picture
/// of the suitability bar (0 red, 1 yellow, 2 green).
#[derive(Debug, PartialEq, Eq)]
pub struct Bars {
    pub values: [i32; 4],
    pub level: usize,
}

/// The bars of an animal with the suitability `rating` of its exhibit (animals 13.1). The colour
/// is picked by the transitions of the habitat check (very unhappy first, then unhappy, never for an
/// egg or a carried animal) against the value shown. An egg shows happiness, hunger and
/// suitability full.
pub fn bars(a: &Animal, k: &Keys, rating: i32, carried: bool) -> Bars {
    let full = |v: i32| if a.egg { 100 } else { v };
    let very = i64::from(rating) * 100 < i64::from(k.pct_habitat) * i64::from(k.habitat_preference);
    let unhappy = !a.egg && !carried && rating < k.habitat_preference;
    // The picture is the first transition above the value shown (0 red, 1 yellow, 2 green), so an
    // egg, which shows 100, is green for any rating below 99.
    let r = i64::from(rating);
    let t = if very {
        (r + 1, r + 2)
    } else if unhappy {
        (r - 1, r + 1)
    } else {
        (r - 2, r - 1)
    };
    let shown = if a.egg { 100 } else { i64::from(rating) }.clamp(0, 100);
    Bars {
        values: [
            full(percent(a.happiness, -100, 100)),
            full(100i32.saturating_sub(percent(a.hunger, 0, 100))),
            percent(a.health, 0, k.max_hits),
            full(rating),
        ],
        level: usize::from(t.0 <= shown) + usize::from(t.1 <= shown),
    }
}

/// The band of a time ago (animals 13.2): 0 below 1440 hours of calendar time, 1 up to 8640, 2
/// above.
pub fn band(now: u64, then: u64) -> usize {
    let hours = now.saturating_sub(then) / HOUR;
    usize::from(hours >= RECENT) + usize::from(hours > SHORT)
}

/// The rating the animal at `at` shows: its species' in the exhibit of its record (its home for an
/// amphibious animal in a tank), 0 on the grounds.
fn suitability(world: &World, cat: &Catalogue, at: usize) -> i32 {
    let Some(exhibit) = record_exhibit(world, cat, at) else {
        return 0;
    };
    let records = rating::rate(&rating::input(world, cat, exhibit));
    let key = &world.zoo.entities[at].type_name;
    (records.iter().find(|r| &r.key == key)).map_or(0, |r| r.shown().min(i32::MAX as u32) as i32)
}

/// The selected animal and the refresh clock.
#[derive(Debug, Default)]
pub struct Panel {
    /// The selected animal, by entity id.
    pub selected: Option<u32>,
    pub clock: Clock,
    /// The recommendations plaque was shown at the last look.
    pub plaque_shown: bool,
    /// Just selected: the next refresh also sets the name, the icon and the follow button.
    fresh: bool,
    told: String,
}

impl Panel {
    /// Selects the animal with entity id `id`: the panel is filled at once (4.3).
    pub fn select(&mut self, id: u32) {
        self.selected = Some(id);
        self.fresh = true;
        self.clock.force = true;
        self.told.clear();
    }

    /// Fills the panel again from the start: the name field shows the animal's name.
    pub fn reselect(&mut self) {
        if let Some(id) = self.selected {
            self.select(id);
        }
    }

    pub fn deselect(&mut self) {
        self.selected = None;
        self.told.clear();
    }

    /// Writes the panel for the selected animal at game time `now` (ms of the calendar) and
    /// returns its log line when something it shows changed.
    pub fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        now: u64,
    ) -> Option<String> {
        let id = self.selected?;
        let (at, unit) = (world.find(id)?, world.units.get(id)?);
        let Part::Animal(a) = &unit.part else {
            return None;
        };
        let ty = type_at(world, cat, at);
        let (k, carried) = (animal::keys(world, cat, at), unit.carried);
        let shown = bars(a, &k, suitability(world, cat, at), carried);
        let none = Words::new(|_| String::new());
        for (&bar, value) in BARS.iter().zip(shown.values) {
            ui.set_status(bar, value);
            ui.set_level(bar, (bar == SUITABILITY).then_some(shown.level));
            let tip = format::expand(&none, &ui.template(bar), &[Arg::Int(i64::from(value))]);
            ui.set_string(bar, tip);
        }
        ui.set_image(SEX, usize::from(a.female));
        let rows = [(FED, FED_STRING, a.fed), (SLEPT, SLEPT_STRING, a.slept)];
        let ago = rows.map(|(ids, base, then)| {
            let text = ui.string(base + band(now, then) as i32);
            ui.set_text(ids[1], &to_cp1252(&text));
            for element in ids {
                ui.set_hidden(element, a.egg);
            }
            text
        });
        let name = world.zoo.entities[at]
            .common
            .as_ref()
            .map_or("", |c| &c.name);
        let fresh = std::mem::take(&mut self.fresh);
        if fresh {
            ui.set_text(NAME, &to_cp1252(name));
            ui.set_button(FOLLOW, false, !carried);
        }
        if fresh || self.clock.forced {
            set_follow_icon(ui, vfs, FOLLOW, ty);
        }
        let moveable = ty.is_none_or(|t| number(t, "cMoveable").is_none_or(|v| v != 0.0));
        ui.set_enabled(SELL, !carried);
        ui.set_enabled(PICK_UP, moveable && !carried);
        ui.set_enabled(EXHIBIT, !carried);
        let list = world.units.thoughts.about(id);
        ui.refill_rows(
            THOUGHTS,
            thought_rows(&thought_texts(ui, world, cat, &list)),
        );
        let thoughts: Vec<i32> = list.iter().map(|t| t.text).collect();
        let [fed, slept] = ago;
        let [happiness, hunger, health, suitability] = shown.values;
        let mut told = format!(
            "animal {id} {name:?} happiness {happiness} hunger {hunger} health {health} \
             suitability {suitability} fed {fed} slept {slept}"
        );
        if !thoughts.is_empty() {
            told += &format!(" thoughts {thoughts:?}");
        }
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
