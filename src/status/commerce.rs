//! The commerce building list of the zoo status window (zoo status 21.4): the guest buildings
//! that sell, one row each with the months open, the guests, the profit and the average profit,
//! and the five sort keys.

use crate::building::{Kind, Live};
use crate::entity::{Catalogue, type_of};
use crate::money::Symbol;
use crate::panel::building::months;
use crate::text::format::money_cents;
use crate::text::{Justify, to_cp1252};
use crate::ui::widgets::{Cell, Row as ListRow};
use crate::world::World;
use std::cmp::Ordering;

/// The layout of the tab, its list and the sort buttons; each sort button has a header text with
/// the same place in [`HEADERS`].
pub(crate) const LAYOUT: i32 = 810;
pub(crate) const LIST: i32 = 4172;
pub(crate) const BUTTONS: [i32; 5] = [4175, 4176, 4177, 4178, 4179];
pub(crate) const HEADERS: [i32; 5] = [4184, 4185, 4186, 4187, 4188];

/// The colour of the header of the sort key, and of the others (the layout's own).
pub(crate) const CURRENT: [u8; 3] = [255, 217, 90];
pub(crate) const OTHER: [u8; 3] = [255, 228, 173];
/// The colours of a loss and of a gain.
const LOSS: [u8; 3] = [255, 0, 0];
const GAIN: [u8; 3] = [0, 255, 0];

/// One building of the list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub id: u32,
    /// The name as the list writes it, and the subtype of the building's type.
    pub name: Vec<u8>,
    pub subtype: Vec<u8>,
    pub months: i64,
    pub guests: u32,
    pub profit: f32,
    pub average: f32,
}

/// The sort key (0 to 4: type then name, months, guests, profit, average) and the direction each
/// key last had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sort {
    pub key: usize,
    pub ascending: [bool; 5],
}

impl Default for Sort {
    fn default() -> Sort {
        Sort {
            key: 0,
            ascending: [true, false, false, false, false],
        }
    }
}

impl Sort {
    /// A click on sort button `button`: the current key's direction flips, another key becomes
    /// the current one with the direction it last had. Whether the button is a sort button.
    pub(crate) fn press(&mut self, button: i32) -> bool {
        let Some(key) = BUTTONS.iter().position(|&b| b == button) else {
            return false;
        };
        if key == self.key {
            self.ascending[key] = !self.ascending[key];
        }
        self.key = key;
        true
    }
}

/// The rows of the buildings that sell, in the order of the world's entities. `name` gives the
/// text of a string number, `now` is the game time in ms.
pub(crate) fn rows(
    world: &World,
    cat: &Catalogue,
    name: impl Fn(i32) -> String,
    now: i64,
) -> Vec<Row> {
    let mut out = Vec::new();
    for e in &world.zoo.entities {
        let (Some(ty), Some(c)) = (type_of(cat, e), e.common.as_ref()) else {
            continue;
        };
        let kind = Kind::of(ty, world.units.prices.default);
        if !crate::month::is_guest_building(ty) || !kind.commerce {
            continue;
        }
        let live = (world.units.buildings.get(&c.id).cloned()).unwrap_or_else(|| Live::new(&kind));
        let open = months(live.built, now);
        let profit = live.income[2] - live.upkeep;
        let text = match c.name.is_empty() {
            true => name(ty.name_id),
            false => c.name.clone(),
        };
        out.push(Row {
            id: c.id,
            name: to_cp1252(&text),
            subtype: ty.subtype.clone(),
            months: open,
            guests: live.guests[2],
            profit,
            average: profit / open as f32,
        });
    }
    out
}

/// Puts the rows in the order of the sort: a stable sort, so equal keys keep the order they came
/// in, in either direction. The type then the name compare byte by byte, a prefix first.
pub(crate) fn order(rows: &mut [Row], sort: &Sort) {
    let by = |a: &Row, b: &Row| match sort.key {
        0 => (&a.subtype, &a.name).cmp(&(&b.subtype, &b.name)),
        1 => a.months.cmp(&b.months),
        2 => a.guests.cmp(&b.guests),
        3 => a.profit.partial_cmp(&b.profit).unwrap_or(Ordering::Equal),
        _ => a.average.partial_cmp(&b.average).unwrap_or(Ordering::Equal),
    };
    match sort.ascending[sort.key.min(4)] {
        true => rows.sort_by(by),
        false => rows.sort_by(|a, b| by(b, a)),
    }
}

/// The rows of the list: the name, then the months, the guests, the profit and the average, all
/// right-aligned; the months and the guests take the list's colour, the money is red below 0 and
/// green else.
pub(crate) fn list(rows: &[Row], symbol: &Symbol) -> Vec<ListRow> {
    let money = |v: f32| {
        let colour = if v < 0.0 { LOSS } else { GAIN };
        let text = to_cp1252(&money_cents(symbol, f64::from(v), true));
        (text, Some(colour))
    };
    let cell = |(text, color): (Vec<u8>, Option<[u8; 3]>)| Cell {
        text,
        color,
        justify: Justify::Right,
    };
    rows.iter()
        .map(|r| {
            let number = |n: String| (n.into_bytes(), None);
            ListRow {
                text: r.name.clone(),
                cells: vec![
                    cell(number(r.months.to_string())),
                    cell(number(r.guests.to_string())),
                    cell(money(r.profit)),
                    cell(money(r.average)),
                ],
                ..ListRow::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
