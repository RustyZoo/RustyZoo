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
//! The random use animation is [`random_use`]'s. Left for later issues: the sounds, `cUseRandomSlot`.
//!
//! The log family `building`: `building use <id> <guest>` when a guest becomes a user and
//! `building buy <id> <item> <price>` when it buys an item, and `building price <id> <price>` when
//! the player sets the price in the panel, `building upgrade <id> <n>` when the player picks the
//! upgrade at position `n` of the Programs tab.

use crate::config::{to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::sim::Rng;
use crate::world::{Shape, World};
use crate::zoo::{BuildingRecord, Entity};
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub mod birth;
pub mod dead;
pub mod drawn;
pub mod items;
pub mod leave;
pub mod random_use;
pub mod ret;
pub mod upgrade;
pub mod users;

mod lifecycle;
mod slots;
pub(crate) use lifecycle::{built, load, loaded_events, reset_all};
pub(crate) use slots::{can_add, entrances, exit_of, has_open_slot};

/// The type whose users are not held to their animal's exhibit (guest buildings 6.3 point 5a).
const SHELTER_OF_ANY_EXHIBIT: i32 = 7059;

/// The economy keys buildings use (guest buildings 13): the multiples of the default price from
/// which a price is expensive and cheap, and the default price of a type that names none; and the
/// compost's pay for dung.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Prices {
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
pub(crate) enum Mood {
    Angry,
    Fair,
    Happy,
}

impl Prices {
    pub(crate) fn mood(&self, price: f32, default: f32) -> Mood {
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
pub(crate) type Point = (i32, i32);

/// One slot of a building type (guest buildings 3.2), the positions relative to the building.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Slot {
    pub at: Point,
    /// The height of `at` (`slotposz`).
    pub height: i32,
    pub entrances: Vec<Point>,
    pub exit: Option<Point>,
    /// The height of the exit (`exitposz`).
    pub exit_height: i32,
    pub capacity: i32,
    pub facing: u32,
    /// Whether the exit may be on land and on water (`exitonland`, `exitonwater`; 6.3).
    pub exit_land: bool,
    pub exit_water: bool,
    /// The species that may use the slot (`id`, empty: any) and the subtype letters it refuses
    /// (`filter`; guest buildings 6.1 point 7).
    pub species: Vec<i32>,
    pub filter: String,
    /// Only a returning unit may use the slot (`isreturnslot`; guest buildings 12.2 point 11).
    pub return_slot: bool,
    /// The name id of the unit type made in the slot when the building is placed (`birthUser`,
    /// unit placing 7.4), 0 for none.
    pub birth: i32,
    /// The draw places of a slot with the drawn-users flag (`cHasDrawSlots`; shows 4.3): the
    /// positions relative to the building, as the slot's, and their heights, `p / 2` of them;
    /// `None` for a slot without the flag.
    pub places: Option<Vec<(i32, i32, i32)>>,
}

/// The type id and subtype of a guest or an animal, which the slots of a building test (guest
/// buildings 6.1 point 7), and what an animal needs of its place (6.3 point 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Species {
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
    /// A guest whose need being served is `grandstand`: a grandstand with no show soon refuses it
    /// (6.1 point 6).
    pub grandstand: bool,
    /// The unit's id, and the name id of the return building it means to come back to.
    pub unit: u32,
    pub returning: Option<i32>,
}

impl Species {
    /// The species of the animal or guest entity `e`, none for another entity; `egg` is the
    /// animal's egg flag, which an egg never swims by.
    pub(crate) fn of(cat: &Catalogue, e: &Entity, egg: bool) -> Option<Species> {
        let ty = crate::entity::type_of(cat, e)
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
            grandstand: false,
            unit: e.common.as_ref().map_or(0, |c| c.id),
            returning: None,
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
            && self.return_slot == who.returning.is_some()
            && (self.filter.is_empty() || !self.filter.contains(&who.subtype))
    }
}

/// The draw places of a slot section (shows 4.3): `cHasDrawSlots` names a section of `p` values read
/// in triples (x, y, height). The table has `p / 2` places, the first `p / 3` from the values and
/// the rest (0, 0, 0); the order lists play no part. A section that is missing gives a flag with
/// no places, and a slot of no capacity has no flag.
fn draw_places(
    config: Option<&crate::config::Config>,
    slot: &crate::config::Section,
) -> Option<Vec<(i32, i32, i32)>> {
    let name = slot.get("cHasDrawSlots")?;
    if slot.get("capacity").and_then(to_int).unwrap_or(0) <= 0 {
        return None;
    }
    let section = config.and_then(|c| c.section(name));
    let values: Vec<i32> = (section.map(|s| s.get_all("p")).unwrap_or_default())
        .iter()
        .map(|v| to_int(v).unwrap_or(0))
        .collect();
    let mut places = vec![(0, 0, 0); values.len() / 2];
    for (place, &[x, y, z]) in places.iter_mut().zip(values.as_chunks::<3>().0) {
        *place = (x, y, z);
    }
    Some(places)
}

/// What a building type declares (guest buildings 3.1).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Kind {
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
    /// The building draws its users itself (`cDrawUser`, map drawing 9.5.1).
    pub draw_user: bool,
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
    /// The keys that decide the dead state (11.1.1).
    pub dead: dead::Keys,
    /// The keys of a return building (12.2).
    pub ret: ret::Keys,
}

impl Kind {
    /// The declarations of a building type; `default` is the economy's default price.
    pub(crate) fn of(ty: &EntityType, default: f32) -> Kind {
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
                height: number("slotposz"),
                entrances: points("entrpos"),
                exit: points("exitpos").first().copied(),
                exit_height: number("exitposz"),
                capacity: number("capacity"),
                facing: number("facing") as u32,
                exit_land: flag("exitonland", true),
                exit_water: flag("exitonwater", false),
                species: s.get_all("id").iter().filter_map(|v| to_int(v)).collect(),
                filter: String::from_utf8_lossy(s.get("filter").unwrap_or_default()).into(),
                return_slot: flag("isreturnslot", false),
                birth: number("birthUser"),
                places: draw_places(config, s),
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
            draw_user: flag("cDrawUser", false),
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
            dead: dead::Keys {
                on_land: flag("cDeadOnLand", false),
                on_flat_water: flag("cDeadOnFlatWater", false),
                underwater: flag("cDeadUnderwater", false),
                surface: flag("cSurface", false),
                height: int("cHeight", 0),
            },
            ret: ret::Keys {
                building: flag("cReturnBuilding", false),
                tracker: flag("cUserTracker", false),
                strict: int("cStrictBuildingCapacity", 0),
                idler: flag("cIdler", false),
                viewer: flag("cExhibitViewer", false),
            },
        }
    }

    pub(crate) fn satisfies(&self, need: &str) -> bool {
        self.satisfies.iter().any(|n| n == need)
    }

    /// The price after the up step by `amount` (guest buildings 7.2; the button's is 0.25): within
    /// the economy's ceiling and the type's, which are applied whatever they are (a `cHighCost`
    /// of 0 gives 0).
    pub(crate) fn raised(&self, price: f32, max: f32, amount: f32) -> f32 {
        (price + amount).min(max).min(self.high)
    }

    /// The price after the down step by `amount`: not below 0 or the type's floor.
    pub(crate) fn lowered(&self, price: f32, amount: f32) -> f32 {
        (price - amount).max(0.0).max(self.low)
    }

    /// The price after the up or down button (the step of 0.25).
    pub(crate) fn pressed(&self, price: f32, max: f32, up: bool) -> f32 {
        match up {
            true => self.raised(price, max, STEP),
            false => self.lowered(price, STEP),
        }
    }

    /// The price after the player typed the number `typed` into the price field (7.2): the step up
    /// or down by the difference, so the bounds hold as for a button press; the same number is no
    /// change.
    pub(crate) fn typed(&self, price: f32, max: f32, typed: f64) -> f32 {
        let at = f64::from(price);
        match typed.partial_cmp(&at) {
            Some(Ordering::Less) => self.lowered(price, (at - typed) as f32),
            Some(Ordering::Greater) => self.raised(price, max, (typed - at) as f32),
            _ => price,
        }
    }

    /// Whether the up button is enabled at this price (at exactly the economy's ceiling it is).
    pub(crate) fn can_raise(&self, price: f32, max: f32) -> bool {
        !(self.high != 0.0 && price >= self.high || price > max)
    }

    /// Whether the down button is enabled at this price.
    pub(crate) fn can_lower(&self, price: f32) -> bool {
        !(self.low != 0.0 && price <= self.low || price <= 0.0)
    }
}

/// A user of a building: the unit, the slot it uses and the AI clock when it entered.
pub(crate) type User = (u32, usize, u32);

/// The live state of a building (guest buildings 4).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Live {
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
    /// The show a grandstand belongs to, 0 for none (shows 3.5, 4.2); not saved, made again at
    /// load. The flag "show soon" of a grandstand is `fill`.
    pub show_id: u16,
    /// The draw place each user of a slot with draw places sits on (shows 4.3); not saved.
    pub seats: Vec<Seat>,
    /// The building is in its dead state: it refuses new users (6.1 point 1) and, found dead
    /// outside a load, it has put its users out (11.1.1); not saved, decided again at the load.
    pub dead: bool,
    /// The guests let out on the water, for a `cUserTracker` type (12.2 point 6); not saved.
    pub members: Vec<u32>,
    /// The water capacity (12.2 point 13), 0 until the first leave; not saved.
    pub water: u32,
}

/// A user on a draw place of a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Seat {
    pub unit: u32,
    pub slot: usize,
    pub place: usize,
}

impl Live {
    /// The users now (guest buildings 4.2).
    pub(crate) fn users_now(&self) -> u32 {
        (self.users.len() as u32).saturating_add(self.extra)
    }

    pub(crate) fn new(kind: &Kind) -> Live {
        Live {
            price: kind.price,
            ..Live::default()
        }
    }

    /// The record a save writes (zoo format 6.9): the figures, the price and the users now, over
    /// `base`, which keeps the fill of a type that is not a trash can (a grandstand's flag is not
    /// modelled) and the members.
    pub(crate) fn record(&self, mut base: BuildingRecord, trash: bool) -> BuildingRecord {
        let users: Vec<(u32, u32)> = (self.users.iter())
            .map(|&(u, s, _)| (u, s as u32))
            .collect();
        base.users_now = (users.len() as u32).saturating_add(self.extra);
        base.users = users;
        base.guests = self.guests;
        base.income = self.income.map(f32::to_bits);
        base.upkeep = Some(self.upkeep.to_bits());
        base.price = self.price.to_bits();
        base.built = crate::timing::file_time(self.built);
        if trash {
            base.fill = self.fill;
        }
        base
    }

    /// Seats `unit` on a free draw place of slot `slot` and returns the place and its position
    /// relative to the building (shows 4.3): `pick(capacity)` is drawn until the place is free (a
    /// draw per try), over the `capacity` seats of the slot. Nothing is drawn when the slot has no
    /// free seat. A seat past the place table sits at (0, 0, 0). The seats of users that left are
    /// dropped first.
    pub(crate) fn seat(
        &mut self,
        slot: usize,
        capacity: i32,
        places: &[(i32, i32, i32)],
        unit: u32,
        pick: &mut dyn FnMut(u32) -> u32,
    ) -> Option<(i32, i32, i32)> {
        let users = &self.users;
        self.seats
            .retain(|s| users.iter().any(|u| u.0 == s.unit) && s.unit != unit);
        let range = usize::try_from(capacity).unwrap_or(0);
        let taken = |place| {
            self.seats
                .iter()
                .any(|s| s.slot == slot && s.place == place)
        };
        if (0..range).all(taken) {
            return None;
        }
        let place = loop {
            let place = pick(capacity as u32) as usize;
            if place < range && !taken(place) {
                break place;
            }
        };
        self.seats.push(Seat { unit, slot, place });
        Some(places.get(place).copied().unwrap_or_default())
    }

    /// The month changes (guest buildings 9): this month's figures become last month's.
    pub(crate) fn roll(&mut self, upkeep: f32) {
        self.guests[1] = std::mem::take(&mut self.guests[0]);
        self.income[1] = std::mem::take(&mut self.income[0]);
        self.upkeep += upkeep;
    }
}

/// Whether a building of the type can be picked up: `cMoveable` 1 (default 0; unit placing 8.7).
pub(crate) fn moveable(ty: &EntityType) -> bool {
    crate::build::number(ty, "cMoveable") == Some(1.0)
}

/// A building the player holds (unit placing 8.7): taken off the map with its users pushed out,
/// it keeps its id, its place in the entity list ([`World::held_at`]) and its figures until it is
/// set down again.
#[derive(Debug, Clone)]
pub(crate) struct Held {
    entity: Entity,
    shape: Option<Shape>,
    live: Option<Live>,
    upgrade: u32,
}

impl Held {
    /// The id of the building.
    fn id(&self) -> u32 {
        self.entity.common.as_ref().map_or(0, |c| c.id)
    }

    /// Writes the building into the list of a zoo about to be saved, where it stands in the list.
    pub(crate) fn put_in(&self, zoo: &mut crate::zoo::Zoo, place: usize) {
        zoo.entities
            .insert(place.min(zoo.entities.len()), self.entity.clone());
    }

    /// Puts the figures of the building in the tables of the world, for a save to read them as for
    /// a building on the map; [`Held::unlend`] takes them out again.
    pub(crate) fn lend(&self, world: &mut World) {
        let id = self.id();
        if let Some(live) = &self.live {
            world.units.buildings.insert(id, live.clone());
        }
        world.units.upgrade_index.insert(id, self.upgrade);
    }

    /// Takes the figures lent out of the tables again.
    pub(crate) fn unlend(&self, world: &mut World) {
        let id = self.id();
        world.units.buildings.remove(&id);
        world.units.upgrade_index.remove(&id);
    }
}

/// Picks building `id` up (guest buildings 11.1): its users are pushed out as a forced leave, a
/// trash can spills its fill, and the building leaves the world, nothing recorded for the undo.
/// `None` when there is no such building.
pub(crate) fn lift(world: &mut World, cat: &Catalogue, id: u32, rng: &mut Rng) -> Option<Held> {
    let at = world.find(id)?;
    leave::push_out(world, cat, id);
    crate::guest::litter::spill(world, cat, id, rng);
    let (entity, shape) = (world.zoo.entities[at].clone(), world.shape(at));
    let live = world.units.buildings.get(&id).cloned();
    let upgrade = world.units.upgrade_index.get(&id).copied().unwrap_or(0);
    world.unrecorded(|w| w.remove(id));
    world.held_at = Some(at);
    Some(Held {
        entity,
        shape,
        live,
        upgrade,
    })
}

/// Sets the held building down at `point` facing `facing`, with the figures it had.
pub(crate) fn set_down(
    world: &mut World,
    ty: &EntityType,
    held: Held,
    point: (i32, i32, i32),
    facing: u32,
) {
    let at = world.held_at.take().unwrap_or(usize::MAX);
    let Held {
        mut entity,
        shape,
        live,
        upgrade,
    } = held;
    let id = entity.common.as_mut().map(|c| {
        (c.x, c.y, c.z, c.rotation) = (point.0, point.1, point.2, facing);
        c.id
    });
    world.put_back(at, entity, shape);
    if let Some(id) = id {
        if let Some(live) = live {
            world.units.buildings.insert(id, live);
        }
        if upgrade != 0 {
            world.units.upgrade_index.insert(id, upgrade);
        }
        dead::decide(world, ty, id);
    }
}

/// Sets the price of building `id` of type `ty` to what `new` makes of the type and the price it
/// has (guest buildings 7.2): the guests that come later pay it, nothing is booked. Returns the
/// new price when it is another.
pub(crate) fn set_price(
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
pub(crate) fn set_fill(world: &mut World, id: u32, fill: u32) {
    if let Some(live) = world.units.buildings.get_mut(&id) {
        live.fill = fill;
    }
    if let Some(at) = world.find(id) {
        world.set_food(at, i32::try_from(fill).unwrap_or(i32::MAX));
    }
}

/// The live states by building id.
pub(crate) type Table = BTreeMap<u32, Live>;

/// How the price of building `id` of kind `kind` strikes a guest (guest buildings 7.3).
pub(crate) fn mood(world: &World, id: u32, kind: &Kind) -> Mood {
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
pub(crate) fn satisfiers(
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
pub(crate) fn kind_of(cat: &Catalogue, e: &Entity, default: f32) -> Option<Kind> {
    if e.class != "building" {
        return None;
    }
    crate::entity::type_of(cat, e).map(|ty| Kind::of(ty, default))
}

/// A slot, entrance or exit position `rel` of the file on the map: turned by the building's
/// rotation and added to its position (guest buildings 3.2). Rotation 0 subtracts the offset, 4
/// adds it, 2 and 6 turn it a quarter; an odd rotation ignores it.
pub(crate) fn place(e: &Entity, rel: Point) -> Point {
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

/// Where a user of slot `slot` is put inside the building `e`: the slot's position, or the draw
/// place `seat` it sits on (shows 4.3), turned by the building's rotation; the height is the
/// building's, plus the place's height unless the rotation is odd.
pub(crate) fn seat_at(e: &Entity, slot: &Slot, seat: Option<(i32, i32, i32)>) -> (i32, i32, i32) {
    let (x, y) = place(e, seat.map_or(slot.at, |p| (p.0, p.1)));
    let c = e.common.as_ref();
    let z = c.map_or(0, |c| c.z);
    let lift = seat.filter(|_| c.is_some_and(|c| c.rotation % 2 == 0));
    (x, y, z + lift.map_or(0, |p| p.2))
}

/// The direction of a user put at a slot with the file's `facing`, turned by the building's
/// rotation (guest buildings 3.2).
pub(crate) fn facing(e: &Entity, facing: u32) -> u32 {
    (e.common.as_ref().map_or(0, |c| c.rotation) + facing) % 8
}

#[cfg(test)]
mod tests;
