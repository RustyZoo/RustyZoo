//! The building panels (information panels 3, 4.1; guest buildings 7.2, 10.3): the charging panel
//! with the price field, its up and down buttons, the income, upkeep and profit since the
//! building was built and per month, the guests and the items sold, and the panel of a building
//! that charges nothing, with the guests and the upkeep; both show the type's information image
//! and the building's name; the sell button is on for every building and the pick-up button shows
//! for a moveable type only. A type with `cHideCommerceInfo` has nothing filled below the price.
//! The charging panel is refreshed every 5 s of the interface clock while shown and the game
//! runs; the other one is filled when shown, never by the clock.
//!
//! [`Panel`] holds the selected building and the clock; [`Panel::refresh`] writes the layout from
//! the world and returns the `panel` log line when what it shows changed.

use crate::building::{Kind, Live, moveable};
use crate::config::to_double;
use crate::entity::{Catalogue, EntityType};
use crate::money::{SYMBOL, Symbol};
use crate::month::upkeep;
use crate::panel::exhibit::{Clock, type_at};
use crate::text::format;
use crate::text::to_cp1252;
use crate::timing::DAY_MS;
use crate::ui::interface::Interface;
use crate::vfs::Vfs;
use crate::world::World;

/// The layout of a charging building and of another one.
pub const CHARGING: i32 = 21;
pub const PLAIN: i32 = 20;
/// The up and down buttons of the price, and the tab buttons of each panel (each forces a
/// refresh).
pub const UP: i32 = 4557;
pub const DOWN: i32 = 4556;
pub const TABS: [i32; 6] = [4575, 4576, 4577, 4675, 4678, 4677];
/// The period of the interface clock between two refreshes of the charging panel (ms; guest
/// buildings 10.3).
pub const PERIOD: u64 = 5000;

/// The elements of the charging panel.
pub const NAME: i32 = 4502;
const PICTURE: i32 = 4550;
pub const PRICE: i32 = 4555;
pub const SELL: i32 = 4516;
pub const PICK_UP: i32 = 4580;
/// Income, upkeep and profit since construction, and per month.
const TOTALS: [i32; 3] = [4531, 4533, 4535];
const MONTHLY: [i32; 3] = [4559, 4561, 4591];
const MONTHS: i32 = 4573;
/// The guests last month, this month and since construction. The shipped layout of the charging
/// panel has none of them; the other one has its own.
const GUESTS: [i32; 3] = [4563, 4565, 4592];
const ITEMS: [i32; 5] = [4567, 4568, 4569, 4570, 4571];
/// The elements of the panel of a building that charges nothing: the guests last month, this
/// month and since construction, and the months open; the layout of the full game also has an
/// upkeep field.
pub const PLAIN_NAME: i32 = 4602;
const PLAIN_PICTURE: i32 = 4650;
pub const PLAIN_SELL: i32 = 4616;
const PLAIN_GUESTS: [i32; 3] = [4664, 4666, 4662];
const PLAIN_MONTHS: i32 = 4668;
const PLAIN_UPKEEP: i32 = 4696;

/// The status and finance tab buttons, hidden when the type hides its commerce info.
const HIDDEN_TABS: [i32; 2] = [4575, 4576];

/// The name of the animation of the panel's picture, `cInfoImageName` of the type: none if the type
/// names none.
fn info_image(ty: &EntityType) -> Option<String> {
    let value = ty
        .config
        .as_deref()?
        .section("Characteristics/Strings")?
        .get("cInfoImageName")?;
    let name = crate::vfs::name_from_bytes(value.trim_ascii());
    (!name.is_empty()).then_some(name)
}

/// The months a building has been open at game time `now` when it was built at `built` (ms):
/// `(d + 29) / 30` (the ceiling of `d / 30`) for `d` whole days, at least 1 (guest buildings 10.3).
pub fn months(built: i64, now: i64) -> i64 {
    let days = now.saturating_sub(built) / DAY_MS as i64;
    ((days.max(0) + 29) / 30).max(1)
}

/// The figures the charging panel shows (guest buildings 10.3): income, upkeep and profit since
/// construction, the first two cut toward zero and the profit their difference, and the same
/// per month (whole division).
pub fn figures(live: &Live, months: i64) -> ([i64; 3], [i64; 3]) {
    let (income, upkeep) = (
        f64::from(live.income[2]) as i64,
        f64::from(live.upkeep) as i64,
    );
    let total = [income, upkeep, income.saturating_sub(upkeep)];
    (total, total.map(|v| v / months))
}

/// The number the player typed into the price field (guest buildings 7.2): the first currency
/// `symbol` and then the first thousands separator cut out, the rest read as `atof` does.
pub fn typed(text: &[u8], symbol: &[u8]) -> f64 {
    let mut text = text.to_vec();
    let cut = |text: &mut Vec<u8>, part: &[u8]| {
        if let Some(at) = text.windows(part.len()).position(|w| w == part) {
            text.drain(at..at + part.len());
        }
    };
    if !symbol.is_empty() {
        cut(&mut text, symbol);
    }
    cut(&mut text, b",");
    to_double(&text)
}

/// The selected building and the refresh clock.
#[derive(Debug, Default)]
pub struct Panel {
    /// The selected building, by entity id, and the layout of its panel.
    pub selected: Option<u32>,
    pub layout: i32,
    pub clock: Clock,
    /// Just selected: the next refresh also sets the name field and the buttons.
    fresh: bool,
    told: String,
}

impl Panel {
    /// Selects the building with entity id `id`, shown in `layout`: the panel is filled at once.
    pub fn select(&mut self, id: u32, layout: i32) {
        self.selected = Some(id);
        self.layout = layout;
        self.fresh = true;
        self.clock.force = true;
        self.told.clear();
    }

    /// Fills the panel again from the start: the name field shows the building's name.
    pub fn reselect(&mut self) {
        if let Some(id) = self.selected {
            self.select(id, self.layout);
        }
    }

    pub fn deselect(&mut self) {
        self.selected = None;
        self.told.clear();
    }

    /// Writes the panel for the selected building at game time `now` (ms) and returns the log
    /// line when something it shows changed.
    pub fn refresh(
        &mut self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        now: i64,
    ) -> Option<String> {
        let id = self.selected?;
        let at = world.find(id)?;
        let ty = type_at(world, cat, at)?;
        let kind = Kind::of(ty, world.units.prices.default);
        let live = (world.units.buildings.get(&id).cloned()).unwrap_or_else(|| Live::new(&kind));
        let charging = self.layout == CHARGING;
        let given = world.zoo.entities[at].common.as_ref().map(|c| &c.name);
        let name = match given.filter(|n| !n.is_empty()) {
            Some(n) => n.clone(),
            None => ui.string(ty.name_id),
        };
        let fresh = std::mem::take(&mut self.fresh);
        let name_field = if charging { NAME } else { PLAIN_NAME };
        if fresh {
            ui.set_text(name_field, &to_cp1252(&name));
            // The type's information image at the top left of the element, set only now; a type
            // that names none shows no picture (guest buildings 10.3.1).
            let picture = info_image(ty).and_then(|name| ui.tree.picture(vfs, &name));
            ui.set_picture(if charging { PICTURE } else { PLAIN_PICTURE }, picture);
            if charging {
                for tab in HIDDEN_TABS {
                    ui.set_hidden(tab, kind.hide_info);
                    ui.set_enabled(tab, !kind.hide_info);
                }
            }
            // A building can always be sold; the pick-up button shows for a moveable type only
            // (guest buildings 10.3).
            for button in [SELL, PLAIN_SELL] {
                ui.set_enabled(button, true);
            }
            ui.set_hidden(PICK_UP, !moveable(ty));
            ui.set_enabled(PICK_UP, moveable(ty));
        }
        let told = if charging {
            self.charging(ui, world, cat, vfs, (&kind, &live), now)
        } else {
            let counts = [live.guests[1], live.guests[0], live.guests[2]];
            let shown = counts.map(|n| n.to_string());
            let open = months(live.built, now);
            let texts = PLAIN_GUESTS.iter().zip(&shown);
            for (&element, text) in texts.chain([(&PLAIN_MONTHS, &open.to_string())]) {
                ui.set_text(element, text.as_bytes());
            }
            let mut told = format!(
                "guests last {} this {} total {} months {open}",
                shown[0], shown[1], shown[2]
            );
            // The full game's layout also shows the monthly upkeep, money with two decimals.
            if ui.tree.find(PLAIN_UPKEEP).is_some() {
                let symbol = Symbol::new(ui.string(SYMBOL));
                let upkeep = format::money_cents(&symbol, f64::from(upkeep(ty)), false);
                ui.set_text(PLAIN_UPKEEP, &to_cp1252(&upkeep));
                told += &format!(" upkeep {upkeep}");
            }
            told
        };
        let told = format!("building {id} {name:?} {told}");
        match told == self.told {
            true => None,
            false => {
                self.told.clone_from(&told);
                Some(told)
            }
        }
    }

    /// The charging panel's fields; returns the part of the log line.
    fn charging(
        &self,
        ui: &mut Interface,
        world: &World,
        cat: &Catalogue,
        vfs: &Vfs,
        (kind, live): (&Kind, &Live),
        now: i64,
    ) -> String {
        let max = world.units.prices.max;
        let symbol = Symbol::new(ui.string(SYMBOL));
        let price = format::money_cents(&symbol, f64::from(live.price), true);
        ui.set_text(PRICE, &to_cp1252(&price));
        // With `cHideCostChange` the field is disabled and the two buttons are hidden.
        ui.set_enabled(PRICE, !kind.fixed_price);
        for (button, enabled) in [
            (UP, kind.can_raise(live.price, max)),
            (DOWN, kind.can_lower(live.price)),
        ] {
            ui.set_enabled(button, enabled);
            ui.set_hidden(button, kind.fixed_price);
        }
        let open = months(live.built, now);
        let (total, monthly) = figures(live, open);
        let shown = |v: [i64; 3]| v.map(|n| format::money(&symbol, n, true));
        let (total, monthly) = (shown(total), shown(monthly));
        let guests = [live.guests[1], live.guests[0], live.guests[2]];
        let counts = guests.map(|n| n.to_string());
        // With `cHideCommerceInfo` the money figures, counts and months are not filled at all: the
        // layout's text stays.
        if !kind.hide_info {
            let pairs = (TOTALS.iter().zip(&total)).chain(MONTHLY.iter().zip(&monthly));
            for (&element, text) in pairs.chain(GUESTS.iter().zip(&counts)) {
                ui.set_text(element, &to_cp1252(text));
            }
            ui.set_text(MONTHS, &to_cp1252(&open.to_string()));
        }
        // The item icons are part of the block `cHideCommerceInfo` leaves alone.
        if !kind.hide_info {
            // One picture per item sold, from the left, each with its name as tooltip.
            for (n, &element) in ITEMS.iter().enumerate() {
                let item = kind.sells.get(n).and_then(|name| cat.items.get(name));
                let picture = item
                    .filter(|i| !i.icon.is_empty())
                    .and_then(|i| ui.tree.picture(vfs, &i.icon));
                ui.set_picture(element, picture);
                if let Some(i) = ui.tree.find(element) {
                    ui.tree.elements[i].help_id = item.map(|i| i.id).into_iter().collect();
                }
            }
        }
        // The line says when the buttons are hidden (` fixed`) and the figures are not filled.
        let price = if kind.fixed_price {
            format!("{price} fixed")
        } else {
            price
        };
        if kind.hide_info {
            return format!("price {price}");
        }
        let [t, u, p] = total;
        let [mt, mu, mp] = monthly;
        format!(
            "price {price} income {t} upkeep {u} profit {p} month {mt} {mu} {mp} months {open} \
             guests last {} this {} total {} sells {:?}",
            guests[0], guests[1], guests[2], kind.sells
        )
    }
}

#[cfg(test)]
mod tests;
