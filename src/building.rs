//! Guest buildings (guest buildings 3, 6 to 9): what a building type declares ([`Kind`]: slots,
//! entrances, exits, capacity, prices, the items it sells, the needs it satisfies) and the live
//! state of a building ([`Live`]: its users, price and monthly figures), kept by the units table.
//! The goals of the guests that use them are [`crate::goal`]'s; a shelter for animals (guest
//! buildings 12.1) uses the same users and slots.
//!
//! Only the buildings that have been built in play, had a user, a month change or a price change,
//! or were loaded with a zoo ([`load`]: the figures, the price and the can's fill its record
//! stores, and [`users::restore`] its users) have a live entry; a missing one is a building at its
//! default price with no users. A trash can counts
//! the litter its users put in ([`Live::fill`], [`set_fill`]; the workers empty it, a deleted can
//! spills it).
//! The panels that set the price and show the figures are [`crate::panel::building`]'s.
//! A building that is deleted or picked up pushes its users out ([`leave::push_out`]).
//! Left for later issues: the random use animation and the sounds, `cUseRandomSlot`.
//!
//! The log family `building`: `building use <id> <guest>` when a guest becomes a user and
//! `building buy <id> <item> <price>` when it buys an item, and `building price <id> <price>` when
//! the player sets the price in the panel.

use crate::config::{to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::guest::litter::CAN_FILLING;
use crate::place::path_shape;
use crate::sim::Rng;
use crate::timing::Date;
use crate::world::{Shape, World};
use crate::zoo::Entity;
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub mod items;
pub mod leave;
pub mod users;

/// The economy keys buildings use (guest buildings 13): the multiples of the default price from
/// which a price is expensive and cheap, and the default price of a type that names none; and the
/// compost's pay for dung.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prices {
    pub expensive: f32,
    pub cheap: f32,
    pub default: f32,
    /// The ceiling of a price set in the panel (`cBuildingUseCostMax`).
    pub max: f32,
    /// What the compost pays for a pile of dung (`cZooDooRecyclingAmount`, staff 8.6).
    pub recycling: f32,
}

impl Default for Prices {
    fn default() -> Prices {
        Prices {
            expensive: 1.2,
            cheap: 0.8,
            default: 29.0,
            max: 100.0,
            recycling: 50.0,
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

/// What one press of the panel's up or down button moves the price by.
const STEP: f32 = 0.25;

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
    /// Whether the exit may be on land and on water (`exitonland`, `exitonwater`; 6.3).
    pub exit_land: bool,
    pub exit_water: bool,
    /// The species that may use the slot (`id`, empty: any) and the subtype letters it refuses
    /// (`filter`; guest buildings 6.1 point 7).
    pub species: Vec<i32>,
    pub filter: String,
}

/// The type id and subtype of a guest or an animal, which the slots of a building test (guest
/// buildings 6.1 point 7), and what an animal needs of its place (6.3 point 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Species {
    /// An animal, not a guest: the place test of 6.3 point 5 applies.
    pub animal: bool,
    pub id: i32,
    pub subtype: String,
    /// `cSwims` is 1 and the animal is no egg: it may stand on water.
    pub swims: bool,
    /// Its footprint (`cFootprintX` times `cFootprintY`) is above 3.
    pub large: bool,
    /// The tile it stands on (`x / 64`, `y / 64`): its exhibit is the one its place must be in.
    pub tile: Option<(i32, i32)>,
}

impl Species {
    /// The species of the animal or guest entity `e`, none for another entity; `egg` is the
    /// animal's egg flag, which an egg never swims by.
    pub fn of(cat: &Catalogue, e: &Entity, egg: bool) -> Option<Species> {
        let ty = crate::place::bulldozer::type_of(cat, e)
            .filter(|_| ["animals", "guests"].contains(&e.class.as_str()))?;
        Some(Species {
            animal: e.class == "animals",
            id: ty.name_id,
            subtype: e.subtype.clone(),
            swims: !egg && crate::build::number(ty, "cSwims") == Some(1.0),
            large: ty.footprint[0].saturating_mul(ty.footprint[1]) > 3,
            tile: e
                .common
                .as_ref()
                .map(|c| (c.x.div_euclid(64), c.y.div_euclid(64))),
        })
    }
}

impl Slot {
    /// Whether the slot's species list and filter let `who` use it.
    fn allows(&self, who: Option<&Species>) -> bool {
        let Some(who) = who else {
            return true;
        };
        (self.species.is_empty() || self.species.contains(&who.id))
            && (self.filter.is_empty() || !self.filter.contains(&who.subtype))
    }
}

/// What a building type declares (guest buildings 3.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Kind {
    /// The type's name id (`cNameID`).
    pub name_id: i32,
    pub capacity: i32,
    pub time_inside: i32,
    pub commerce: bool,
    pub price: f32,
    /// The bounds the panel keeps the price between (`cLowCost`, `cHighCost`; guest buildings 7.2).
    pub low: f32,
    pub high: f32,
    pub factor: f32,
    /// The panel's price field is read only (`cHideCostChange`).
    pub fixed_price: bool,
    /// The panel leaves its money figures and counts empty (`cHideCommerceInfo`).
    pub hide_info: bool,
    /// The change record given at leaving (guests 5.4).
    pub change: [i32; 6],
    pub hide: bool,
    pub outside: bool,
    /// An outside user turns towards the building (`cSetUserFacing`).
    pub faces_user: bool,
    /// A user is put inside without a walk (`cUserTeleportsInside`).
    pub teleports: bool,
    pub uses_exit: bool,
    pub direct: bool,
    pub holds: bool,
    pub sells: Vec<String>,
    pub satisfies: Vec<String>,
    /// The items it takes from a user it enters (guest buildings 7.6), and the thoughts a user
    /// may think there (`cUsedThought`).
    pub removes: Vec<String>,
    pub used: Vec<i32>,
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
            // A slot flag is on only when its value is exactly 1.
            let flag = |key: &str, fallback: bool| {
                s.get(key).and_then(to_int).map_or(fallback, |v| v == 1)
            };
            Slot {
                at: points("slotpos").first().copied().unwrap_or((0, 0)),
                entrances: points("entrpos"),
                exit: points("exitpos").first().copied(),
                capacity: number("capacity"),
                facing: number("facing") as u32,
                exit_land: flag("exitonland", true),
                exit_water: flag("exitonwater", false),
                species: s.get_all("id").iter().filter_map(|v| to_int(v)).collect(),
                filter: String::from_utf8_lossy(s.get("filter").unwrap_or_default()).into(),
            }
        })
        .collect();
        Kind {
            name_id: ty.name_id,
            capacity: int("cCapacity", if config.is_some() { 1 } else { i32::MAX }),
            time_inside: int("cTimeInside", 0),
            commerce: flag("cCommerce", false),
            price: float("cDefaultCost", default),
            low: float("cLowCost", 1.0),
            high: float("cHighCost", 1.0),
            factor: float("cPriceFactor", 1.0),
            fixed_price: flag("cHideCostChange", false),
            hide_info: flag("cHideCommerceInfo", false),
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
            faces_user: flag("cSetUserFacing", true),
            teleports: flag("cUserTeleportsInside", false),
            uses_exit: flag("cUserUsesExit", false),
            direct: flag("cDirectEntrance", false),
            holds: flag("cHoldsOntoUser", false),
            sells: names("Sells"),
            satisfies: names("Satisfies"),
            removes: names("Removes"),
            used: (ints.map(|s| s.get_all("cUsedThought")).unwrap_or_default())
                .iter()
                .filter_map(|v| to_int(v))
                .collect(),
            slots,
        }
    }

    pub fn satisfies(&self, need: &str) -> bool {
        self.satisfies.iter().any(|n| n == need)
    }

    /// The price after the up step by `amount` (guest buildings 7.2; the button's is 0.25): within
    /// the economy's ceiling and the type's, which are applied whatever they are (a `cHighCost`
    /// of 0 gives 0).
    pub fn raised(&self, price: f32, max: f32, amount: f32) -> f32 {
        (price + amount).min(max).min(self.high)
    }

    /// The price after the down step by `amount`: not below 0 or the type's floor.
    pub fn lowered(&self, price: f32, amount: f32) -> f32 {
        (price - amount).max(0.0).max(self.low)
    }

    /// The price after the up or down button (the step of 0.25).
    pub fn pressed(&self, price: f32, max: f32, up: bool) -> f32 {
        match up {
            true => self.raised(price, max, STEP),
            false => self.lowered(price, STEP),
        }
    }

    /// The price after the player typed the number `typed` into the price field (7.2): the step up
    /// or down by the difference, so the bounds hold as for a button press; the same number is no
    /// change.
    pub fn typed(&self, price: f32, max: f32, typed: f64) -> f32 {
        let at = f64::from(price);
        match typed.partial_cmp(&at) {
            Some(Ordering::Less) => self.lowered(price, (at - typed) as f32),
            Some(Ordering::Greater) => self.raised(price, max, (typed - at) as f32),
            _ => price,
        }
    }

    /// Whether the up button is enabled at this price (at exactly the economy's ceiling it is).
    pub fn can_raise(&self, price: f32, max: f32) -> bool {
        !(self.high != 0.0 && price >= self.high || price > max)
    }

    /// Whether the down button is enabled at this price.
    pub fn can_lower(&self, price: f32) -> bool {
        !(self.low != 0.0 && price <= self.low || price <= 0.0)
    }
}

/// A user of a building: the unit, the slot it uses and the AI clock when it entered.
pub type User = (u32, usize, u32);

/// The live state of a building (guest buildings 4).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Live {
    /// In entering order.
    pub users: Vec<User>,
    /// Users counted against the capacity that are not in the list: a loaded building keeps its
    /// stored count when a listed unit is gone (guest buildings 4.2).
    pub extra: u32,
    pub price: f32,
    /// The pieces of litter a trash can holds (guest buildings 8).
    pub fill: u32,
    /// Guests and income this month, last month and since construction; the upkeep paid since.
    pub guests: [u32; 3],
    pub income: [f32; 3],
    pub upkeep: f32,
    /// The calendar time the building was built at (ms of game time since the calendar's start):
    /// the stored construction date for one that was there when the zoo was loaded, so it may be
    /// before the start or after now.
    pub built: i64,
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

/// Building `id` of type `ty` was built at calendar time `now` (ms of game time): a guest
/// building starts its live state, with the date its months open are counted from (guest
/// buildings 4).
pub fn built(world: &mut World, ty: &EntityType, id: u32, now: i64) {
    if crate::month::is_guest_building(ty) {
        let live = Live::new(&Kind::of(ty, world.units.prices.default));
        world
            .units
            .buildings
            .insert(id, Live { built: now, ..live });
    }
}

/// The building at position `at` of the entity list of a zoo just loaded takes the figures its
/// record stores (zoo format 6.9, guest buildings 4): the guests and the income of this month,
/// last month and since construction, the upkeep paid since construction (from version 68), the
/// trash can's fill, the construction date and the price. The users come after the guests are
/// made ([`users::restore`]).
pub fn load(world: &mut World, cat: &Catalogue, at: usize) {
    let e = &world.zoo.entities[at];
    let Some(record) = e.common.as_ref().and_then(|c| c.building.clone()) else {
        return;
    };
    let Some(ty) = crate::place::bulldozer::type_of(cat, e) else {
        return;
    };
    if !crate::month::is_guest_building(ty) {
        return;
    }
    let kind = Kind::of(ty, world.units.prices.default);
    let id = e.common.as_ref().map_or(0, |c| c.id);
    let built = Date::from_file_time(record.built).game_ms();
    let live = Live {
        guests: record.guests,
        income: record.income.map(f32::from_bits),
        upkeep: record.upkeep.map_or(0.0, f32::from_bits),
        price: f32::from_bits(record.price),
        built,
        ..Live::new(&kind)
    };
    world.units.buildings.insert(id, live);
    if kind.satisfies("trash") {
        set_fill(world, id, record.fill);
    }
}

/// A start with `reset` or `resetdate` resets every building (guest buildings 4.1): the fill, the
/// users and the figures go to 0 and the construction date becomes the game time `now` (ms since
/// the calendar's start); the price is kept. It runs after the load events, so a can that called a
/// worker is empty with its task still in the pool. Every unit a building listed as a user is
/// removed from the world, not released: the ids are returned for the caller to ask the removal.
pub fn reset_all(world: &mut World, now: i64) -> Vec<u32> {
    let ids: Vec<u32> = world.units.buildings.keys().copied().collect();
    let mut removed = Vec::new();
    for id in ids {
        set_fill(world, id, 0);
        if let Some(live) = world.units.buildings.get_mut(&id) {
            removed.extend(live.users.iter().map(|u| u.0));
            *live = Live {
                price: live.price,
                built: now,
                ..Live::default()
            };
        }
    }
    removed
}

/// The events a zoo just loaded posts for its trash cans (staff 8.2): a can above 12 pieces calls
/// a worker as it does when it reaches 13, with its tile, in entity list order.
pub fn loaded_events(world: &World) -> Vec<crate::sim::Event> {
    let mut events = Vec::new();
    for e in &world.zoo.entities {
        let Some(c) = e.common.as_ref() else {
            continue;
        };
        let full = world
            .units
            .buildings
            .get(&c.id)
            .is_some_and(|l| l.fill > 12);
        let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        if e.class == "building" && full {
            let tile = tile.unwrap_or(0) as u32;
            events.push(crate::world::piece_event(c.id, CAN_FILLING, tile));
        }
    }
    events
}

/// Whether a building of the type can be picked up: `cMoveable` 1 (default 0; unit placing 8.7).
pub fn moveable(ty: &EntityType) -> bool {
    crate::build::number(ty, "cMoveable") == Some(1.0)
}

/// A building the player holds (unit placing 8.7): taken off the map with its users pushed out,
/// it keeps its id, its place in the entity list and its figures until it is set down again.
#[derive(Debug, Clone)]
pub struct Held {
    at: usize,
    entity: Entity,
    shape: Option<Shape>,
    live: Option<Live>,
}

/// Picks building `id` up (guest buildings 11.1): its users are pushed out as a forced leave, a
/// trash can spills its fill, and the building leaves the world, nothing recorded for the undo.
/// `None` when there is no such building.
pub fn lift(world: &mut World, cat: &Catalogue, id: u32, rng: &mut Rng) -> Option<Held> {
    let at = world.find(id)?;
    leave::push_out(world, cat, id);
    crate::guest::litter::spill(world, cat, id, rng);
    let (entity, shape) = (world.zoo.entities[at].clone(), world.shape(at));
    let live = world.units.buildings.get(&id).cloned();
    world.unrecorded(|w| w.remove(id));
    Some(Held {
        at,
        entity,
        shape,
        live,
    })
}

/// Sets the held building down at `point` facing `facing`, with the figures it had.
pub fn set_down(world: &mut World, held: Held, point: (i32, i32, i32), facing: u32) {
    let Held {
        at,
        mut entity,
        shape,
        live,
    } = held;
    let id = entity.common.as_mut().map(|c| {
        (c.x, c.y, c.z, c.rotation) = (point.0, point.1, point.2, facing);
        c.id
    });
    world.put_back(at, entity, shape);
    if let Some((id, live)) = id.zip(live) {
        world.units.buildings.insert(id, live);
    }
}

/// Sets the price of building `id` of type `ty` to what `new` makes of the type and the price it
/// has (guest buildings 7.2): the guests that come later pay it, nothing is booked. Returns the
/// new price when it is another.
pub fn set_price(
    world: &mut World,
    ty: &EntityType,
    id: u32,
    new: impl FnOnce(&Kind, f32) -> f32,
) -> Option<f32> {
    let kind = Kind::of(ty, world.units.prices.default);
    let live = (world.units.buildings.entry(id)).or_insert_with(|| Live::new(&kind));
    let price = new(&kind, live.price);
    (price != live.price).then(|| {
        live.price = price;
        price
    })
}

/// Sets the pieces trash can `id` holds. Its picture follows the count (guest buildings 8): the
/// entity keeps it in its counter, which the picture reads.
pub fn set_fill(world: &mut World, id: u32, fill: u32) {
    if let Some(live) = world.units.buildings.get_mut(&id) {
        live.fill = fill;
    }
    if let Some(at) = world.find(id) {
        world.set_food(at, i32::try_from(fill).unwrap_or(i32::MAX));
    }
}

/// The live states by building id.
pub type Table = BTreeMap<u32, Live>;

/// How the price of building `id` of kind `kind` strikes a guest (guest buildings 7.3).
pub fn mood(world: &World, id: u32, kind: &Kind) -> Mood {
    let price = world
        .units
        .buildings
        .get(&id)
        .map_or(kind.price, |l| l.price);
    world.units.prices.mood(price, kind.price)
}

/// Guests 7.1: the buildings that satisfy `need`, and among them the ones nearest to tile `here`
/// within `radius` tiles that lie in its area and have an open slot, the expensive ones farther
/// and the cheap ones nearer; the draw among the ties is the caller's.
pub fn satisfiers(
    world: &World,
    cat: &Catalogue,
    here: usize,
    need: &str,
    radius: i64,
    who: Option<&Species>,
) -> (Vec<u32>, Vec<u32>) {
    let width = world.zoo.width as usize;
    let area = world.exhibits().region(here);
    let prices = world.units.prices;
    let (mut all, mut best, mut ties) = (Vec::new(), i64::MAX, Vec::new());
    for e in &world.zoo.entities {
        let (Some(kind), Some(c)) = (kind_of(cat, e, prices.default), e.common.as_ref()) else {
            continue;
        };
        let Some(t) = world
            .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
            .filter(|_| kind.satisfies(need))
        else {
            continue;
        };
        all.push(c.id);
        let live = world.units.buildings.get(&c.id);
        if world.exhibits().region(t) != area || !has_open_slot(world, e, &kind, live, who) {
            continue;
        }
        let dx = (t % width) as i64 - (here % width) as i64;
        let dy = (t / width) as i64 - (here / width) as i64;
        let mut d = dx * dx + dy * dy;
        let price = live.map_or(kind.price, |l| l.price);
        let scale = match prices.mood(price, kind.price) {
            _ if !kind.commerce => None,
            Mood::Angry => Some(prices.expensive),
            Mood::Happy => Some(prices.cheap),
            Mood::Fair => None,
        };
        if let Some(f) = scale {
            let far = ((d as f64).sqrt() * f64::from(f)).round_ties_even() as i64;
            d = far * far;
        }
        if d > radius * radius || d > best {
            continue;
        }
        if d < best {
            (best, ties) = (d, Vec::new());
        }
        ties.push(c.id);
    }
    (all, ties)
}

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

/// The type whose users are not held to their animal's exhibit (guest buildings 6.3 point 5a).
const SHELTER_OF_ANY_EXHIBIT: i32 = 7059;

/// The place a user of slot `slot` of the building is put at when it leaves by force or by the
/// exit (guest buildings 6.3, 6.7): the slot's exit for a type whose users use it, else the first
/// entrance, whose place passes [`fits`]. `who` is the animal that leaves, which its own place
/// rules apply to too (6.3 point 5), its exhibit being the one of the tile it stands on; `None`
/// for a guest. `None` when there is none, and the slot cannot be used.
pub fn exit_of(
    world: &World,
    e: &Entity,
    kind: &Kind,
    slot: usize,
    who: Option<&Species>,
) -> Option<Point> {
    let s = kind.slots.get(slot)?;
    let home = who
        .and_then(|w| w.tile)
        .and_then(|(x, y)| world.tile_index(x, y))
        .and_then(|t| world.exhibits().region(t));
    let exempt = kind.name_id == SHELTER_OF_ANY_EXHIBIT;
    let animal = |t: usize| who.is_none_or(|w| animal_place(world, t, w, home, exempt));
    match kind.uses_exit {
        true => (s.exit.map(|p| place(e, p))).filter(|&p| fits(world, p, Some(s), &animal)),
        false => {
            (s.entrances.iter().map(|&p| place(e, p))).find(|&p| fits(world, p, None, &animal))
        }
    }
}

/// Guest buildings 6.3, the place test for a guest: the tile of `p` is on the map, in the zoo and
/// has a quarter free or held by a walkable object; an exit place (`exit` given) must also be on
/// the terrain the slot allows, water or land. `animal` is the animal's part of the test (point 5),
/// which passes a guest.
fn fits(world: &World, (x, y): Point, exit: Option<&Slot>, animal: &dyn Fn(usize) -> bool) -> bool {
    let Some(t) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return false;
    };
    let walkable = |e: usize| world.shape(e).is_some_and(|s| s.walkable.0);
    let open = (0..4).any(|q| world.quadrant(t, q).is_none_or(walkable));
    let terrain = exit.is_none_or(|s| match world.is_water(t) {
        true => s.exit_water,
        false => s.exit_land,
    });
    world.in_zoo(t) && open && terrain && animal(t)
}

/// Guest buildings 6.3 point 5, the place of an animal on tile `t`: in its own exhibit, which is
/// the exhibit of the tile it stands on (`home`) unless the type is `exempt`; not a large animal on
/// a tile that is not flat or raised along one side (the path shapes of placing 5.3); and not water
/// for a non-swimmer.
fn animal_place(world: &World, t: usize, who: &Species, home: Option<u32>, exempt: bool) -> bool {
    if !who.animal {
        return true;
    }
    let own = exempt || world.exhibits().region(t) == home;
    own && !(who.large && !path_shape(world.zoo.tiles[t].shape))
        && (who.swims || !world.is_water(t))
}

/// Whether the unit (`who`) may use slot `slot` of a building (guest buildings 6.1):
/// the building and the slot have room, the slot's species and filter let it in, and an exit exists
/// (6.3: an entrance, or the slot's exit for a type that uses it, that passes the place test).
pub fn can_add(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    slot: usize,
    who: Option<&Species>,
) -> bool {
    let users = live.map_or(0, |l| l.users.len() + l.extra as usize);
    let Some(s) = kind.slots.get(slot) else {
        return false;
    };
    let in_slot = live.map_or(0, |l| l.users.iter().filter(|u| u.1 == slot).count());
    users < kind.capacity.max(0) as usize
        && in_slot < s.capacity.max(0) as usize
        && s.allows(who)
        && exit_of(world, e, kind, slot, who).is_some()
}

/// Whether some slot of the building is open (guest buildings 6.1).
pub fn has_open_slot(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    who: Option<&Species>,
) -> bool {
    (0..kind.slots.len()).any(|s| can_add(world, e, kind, live, s, who))
}

/// The entrances of the open slots with their squared distance from `from`, the nearest first and
/// in slot order among equals (guest buildings 6.2).
pub fn entrances(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    from: Point,
    who: Option<&Species>,
) -> Vec<(i64, usize, Point)> {
    let mut found = Vec::new();
    for (i, s) in kind.slots.iter().enumerate() {
        if !can_add(world, e, kind, live, i, who) {
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
