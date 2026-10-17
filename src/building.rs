//! Guest buildings (guest buildings 3, 6 to 9): what a building type declares ([`Kind`]: slots,
//! entrances, exits, capacity, prices, the items it sells, the needs it satisfies) and the live
//! state of a building ([`Live`]: its users, price and monthly figures), kept by the units table.
//! The goals of the guests that use them are [`crate::goal`]'s; a shelter for animals (guest
//! buildings 12.1) uses the same users and slots.
//!
//! Only the buildings that have had a user or a month change have a live entry; a missing one is
//! a building at its default price with no users. Left for later issues: the trash can's fill and
//! litter, the random use animation and the sounds, the building panel, users pushed out of a
//! building that is moved or removed, the slot filters of a user type, `cUseRandomSlot`.
//!
//! The log family `building`: `building use <id> <guest>` when a guest becomes a user and
//! `building buy <id> <item> <price>` when it buys an item.

use crate::config::{to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::world::World;
use crate::zoo::Entity;
use std::collections::BTreeMap;

pub mod items;

/// The economy keys buildings use (guest buildings 13): the multiples of the default price from
/// which a price is expensive and cheap, and the default price of a type that names none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    pub expensive: f32,
    pub cheap: f32,
    pub default: f32,
}

impl Default for Prices {
    fn default() -> Prices {
        Prices {
            expensive: 1.2,
            cheap: 0.8,
            default: 29.0,
        }
    }
}

/// How a price strikes a guest (guest buildings 7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Angry,
    Fair,
    Happy,
}

impl Prices {
    pub fn mood(&self, price: f32, default: f32) -> Mood {
        if price >= self.expensive * default {
            Mood::Angry
        } else if price <= self.cheap * default {
            Mood::Happy
        } else {
            Mood::Fair
        }
    }
}

/// A point in world units: 64 to a tile.
pub type Point = (i32, i32);

/// One slot of a building type (guest buildings 3.2), the positions relative to the building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub at: Point,
    pub entrances: Vec<Point>,
    pub exit: Option<Point>,
    pub capacity: i32,
    pub facing: u32,
}

/// What a building type declares (guest buildings 3.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Kind {
    pub capacity: i32,
    pub time_inside: i32,
    pub commerce: bool,
    pub price: f32,
    pub factor: f32,
    /// The change record given at leaving (guests 5.4).
    pub change: [i32; 6],
    pub hide: bool,
    pub outside: bool,
    pub uses_exit: bool,
    pub direct: bool,
    pub holds: bool,
    pub sells: Vec<String>,
    pub satisfies: Vec<String>,
    pub slots: Vec<Slot>,
}

impl Kind {
    /// The declarations of a building type; `default` is the economy's default price.
    pub fn of(ty: &EntityType, default: f32) -> Kind {
        let config = ty.config.as_deref();
        let section = |name: &str| config.and_then(|c| c.section(name));
        let (ints, floats) = (
            section("Characteristics/Integers"),
            section("Characteristics/Floats"),
        );
        let int = |key: &str, fallback: i32| {
            (ints.and_then(|s| s.get(key)).and_then(to_int)).unwrap_or(fallback)
        };
        // A flag is on only when its value is exactly 1.
        let flag = |key: &str, fallback: bool| {
            (ints.and_then(|s| s.get(key)).and_then(to_int)).map_or(fallback, |v| v == 1)
        };
        let float = |key: &str, fallback: f32| {
            (floats.and_then(|s| s.get(key)).map(to_float)).unwrap_or(fallback)
        };
        let names = |name: &str| -> Vec<String> {
            (section(name).into_iter().flat_map(|s| s.keys()))
                .map(|k| String::from_utf8_lossy(k).to_ascii_lowercase())
                .collect()
        };
        let slots = (section("slots")
            .map(|s| s.get_all("name"))
            .unwrap_or_default())
        .iter()
        .filter_map(|name| config?.section(name))
        .map(|s| {
            let points = |key: &str| -> Vec<Point> {
                (s.get_all(key).as_chunks::<2>().0.iter())
                    .map(|[x, y]| (to_int(x).unwrap_or(0), to_int(y).unwrap_or(0)))
                    .collect()
            };
            let number = |key: &str| s.get(key).and_then(to_int).unwrap_or(0);
            Slot {
                at: points("slotpos").first().copied().unwrap_or((0, 0)),
                entrances: points("entrpos"),
                exit: points("exitpos").first().copied(),
                capacity: number("capacity"),
                facing: number("facing") as u32,
            }
        })
        .collect();
        Kind {
            capacity: int("cCapacity", if config.is_some() { 1 } else { i32::MAX }),
            time_inside: int("cTimeInside", 0),
            commerce: flag("cCommerce", false),
            price: float("cDefaultCost", default),
            factor: float("cPriceFactor", 1.0),
            change: [
                "cAdultChange",
                "cChildChange",
                "cHungerChange",
                "cThirstChange",
                "cBathroomChange",
                "cEnergyChange",
            ]
            .map(|k| int(k, 0)),
            hide: flag("cHideUser", false),
            outside: flag("cUserStaysOutside", true),
            uses_exit: flag("cUserUsesExit", false),
            direct: flag("cDirectEntrance", false),
            holds: flag("cHoldsOntoUser", false),
            sells: names("Sells"),
            satisfies: names("Satisfies"),
            slots,
        }
    }

    pub fn satisfies(&self, need: &str) -> bool {
        self.satisfies.iter().any(|n| n == need)
    }
}

/// A user of a building: the unit, the slot it uses and the AI clock when it entered.
pub type User = (u32, usize, u32);

/// The live state of a building (guest buildings 4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    /// In entering order.
    pub users: Vec<User>,
    pub price: f32,
    /// Guests and income this month, last month and since construction; the upkeep paid since.
    pub guests: [u32; 3],
    pub income: [f32; 3],
    pub upkeep: f32,
}

impl Live {
    pub fn new(kind: &Kind) -> Live {
        Live {
            price: kind.price,
            ..Live::default()
        }
    }

    /// The month changes (guest buildings 9): this month's figures become last month's.
    pub fn roll(&mut self, upkeep: f32) {
        self.guests[1] = std::mem::take(&mut self.guests[0]);
        self.income[1] = std::mem::take(&mut self.income[0]);
        self.upkeep += upkeep;
    }
}

/// The live states by building id.
pub type Table = BTreeMap<u32, Live>;

/// The type of the building entity `e`.
pub fn kind_of(cat: &Catalogue, e: &Entity, default: f32) -> Option<Kind> {
    if e.class != "building" {
        return None;
    }
    crate::place::bulldozer::type_of(cat, e).map(|ty| Kind::of(ty, default))
}

/// A slot, entrance or exit position `rel` of the file on the map: turned by the building's
/// rotation and added to its position (guest buildings 3.2). Rotation 0 subtracts the offset, 4
/// adds it, 2 and 6 turn it a quarter; an odd rotation ignores it.
pub fn place(e: &Entity, rel: Point) -> Point {
    let Some(c) = e.common.as_ref() else {
        return rel;
    };
    let ((x, y), (sx, sy)) = ((c.x, c.y), rel);
    match c.rotation % 8 {
        0 => (x - sx, y - sy),
        2 => (x + sy, y - sx),
        4 => (x + sx, y + sy),
        6 => (x - sy, y + sx),
        _ => (x, y),
    }
}

/// The direction of a user put at a slot with the file's `facing`, turned by the building's
/// rotation (guest buildings 3.2).
pub fn facing(e: &Entity, facing: u32) -> u32 {
    (e.common.as_ref().map_or(0, |c| c.rotation) + facing) % 8
}

/// Whether the unit may use slot `slot` of a building (guest buildings 6.1): the building and the
/// slot have room and an exit exists (6.3: an entrance, or the slot's exit for a type that uses
/// it, on the map).
pub fn can_add(world: &World, e: &Entity, kind: &Kind, live: Option<&Live>, slot: usize) -> bool {
    let users = live.map_or(0, |l| l.users.len());
    let Some(s) = kind.slots.get(slot) else {
        return false;
    };
    let in_slot = live.map_or(0, |l| l.users.iter().filter(|u| u.1 == slot).count());
    let on_map = |p: &Point| {
        let (x, y) = place(e, *p);
        world
            .tile_index(x.div_euclid(64), y.div_euclid(64))
            .is_some()
    };
    let exit = match kind.uses_exit {
        true => s.exit.iter().any(on_map),
        false => s.entrances.iter().any(on_map),
    };
    users < kind.capacity.max(0) as usize && in_slot < s.capacity.max(0) as usize && exit
}

/// Whether some slot of the building is open (guest buildings 6.1).
pub fn has_open_slot(world: &World, e: &Entity, kind: &Kind, live: Option<&Live>) -> bool {
    (0..kind.slots.len()).any(|s| can_add(world, e, kind, live, s))
}

/// The entrances of the open slots with their squared distance from `from`, the nearest first and
/// in slot order among equals (guest buildings 6.2).
pub fn entrances(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    from: Point,
) -> Vec<(i64, usize, Point)> {
    let mut found = Vec::new();
    for (i, s) in kind.slots.iter().enumerate() {
        if !can_add(world, e, kind, live, i) {
            continue;
        }
        for &rel in &s.entrances {
            let p = place(e, rel);
            let (dx, dy) = (i64::from(from.0 - p.0), i64::from(from.1 - p.1));
            found.push((dx * dx + dy * dy, i, p));
        }
    }
    found.sort_by_key(|f| f.0);
    found
}

#[cfg(test)]
mod tests;
