//! Guests: their arrival at the zoo entrance and the state they start with (guests 3 to 5, 13).
//!
//! The new guest check of the zoo status ([`crate::status::zoo::Status::new_guest_check`]) decides
//! whether a guest arrives; [`arrive`] makes it: the type with equal chance, the status values and
//! the check countdowns it starts with ([`Guest::create`], every draw in the original's order),
//! the place at the centre of the entrance tile, the name and the admission booked (a child pays
//! half). The way out is the leave goal ([`crate::goal`]): a guest whose `leave` flag is up walks
//! to the entrance and is removed there. Missing so far: the needs and checks that use the values
//! and countdowns (they are what raises the flag), the Halloween colours, and the entrance tiles
//! other than the zoo entrance of the map.
//!
//! The log family `guest`: `guest add <id> <adult|child>` for each guest made and
//! `guest leave <id>` for each one that left and was removed.

use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating;
use crate::money::{self, Money};
use crate::sim::Rng;
use crate::timing::Date;
use crate::unit::Part;
use crate::world::{Shape, TankKind, World};
use crate::zoo::{Common, Entity};

#[cfg(test)]
mod tests;

/// The status values of a guest, in the order of [`Guest::values`].
pub const HAPPINESS: usize = 0;
pub const HUNGER: usize = 1;
pub const THIRST: usize = 2;
pub const ENERGY: usize = 3;
pub const BATHROOM: usize = 4;
/// The replaceable colours of a guest type, one draw each at creation.
const COLOURS: usize = 4;
/// The checks, in the order of their countdowns: hunger, thirst, bathroom, leave, gift, energy,
/// chase, trash, like animals, viewing area, surroundings, with the key of the period and the
/// program's value for a missing key.
#[rustfmt::skip]
const PERIODS: [(&str, i32); 11] = [
    ("cHungerCheck", 5), ("cThirstyCheck", 5), ("cBathroomCheck", 10), ("cLeaveZooCheck", 20),
    ("cBuySouvenirCheck", 20), ("cEnergyCheck", 10), ("cChaseCheck", 2), ("cTrashCheck", 20),
    ("cLikeAnimalsCheck", 10), ("cViewingAreaCheck", 10), ("cEnvironmentEffectCheck", 10),
];
/// The category of the entity type that makes the name's counter, shared by all four types.
const COUNTER: &str = "guests";

/// The keys of a guest type that its creation reads (guests 5): its own section, the default
/// subtype's and the plain one, which also holds the class keys of the random start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    pub initial_happiness: i32,
    pub max_energy: i32,
    pub periods: [i32; PERIODS.len()],
    pub saw_animal_reset: i32,
    /// `cRandomStatNth` and the start ranges of hunger, thirst, energy and bathroom.
    pub random_nth: i32,
    pub random: [(i32, i32); 4],
}

impl Keys {
    pub fn of(ty: &EntityType) -> Keys {
        let config = ty.config.as_deref();
        let default = (config.and_then(|c| c.section("Global")))
            .and_then(|g| g.get("DefaultSubtype"))
            .unwrap_or_default();
        let int = |key: &str, fallback: i32| {
            let found = config.and_then(|config| {
                [&ty.type_name[..], default, b""]
                    .into_iter()
                    .find_map(|prefix| {
                        let name = match prefix.is_empty() {
                            true => b"Characteristics/Integers".to_vec(),
                            false => [prefix, b"/Characteristics/Integers"].concat(),
                        };
                        config.section(name)?.get(key).and_then(to_int)
                    })
            });
            found.unwrap_or(fallback)
        };
        let range = |stat: &str| {
            (
                int(&format!("cRandom{stat}Min"), 0),
                int(&format!("cRandom{stat}Max"), 0),
            )
        };
        Keys {
            initial_happiness: int("cInitialHappiness", 40),
            max_energy: int("cMaxEnergy", 100),
            periods: PERIODS.map(|(key, period)| int(key, period)),
            saw_animal_reset: int("cSawAnimalReset", 20),
            random_nth: int("cRandomStatNth", 0),
            random: ["Hunger", "Thirst", "Energy", "Bathroom"].map(range),
        }
    }
}

/// The live state of a guest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guest {
    /// Happiness, hunger, thirst, energy and bathroom, and the changes added at the next status
    /// tick.
    pub values: [i32; 5],
    pub pending: [i32; 5],
    /// The countdowns of the checks in the order of the table of periods; -1 never fires.
    pub countdowns: [i32; PERIODS.len()],
    /// The saw-animal and reset countdowns.
    pub saw_animal: i32,
    pub reset: i32,
    /// The name id of the animal type the guest likes best, if the game has animal types.
    pub favourite: Option<i32>,
    pub female: bool,
    pub child: bool,
    /// The game date of the arrival.
    pub arrived: Option<Date>,
    /// The personal part of the walking speed, 0 to 10.
    pub speed_offset: u32,
    /// Feeling sick: a guest created on 1 January is.
    pub sick: bool,
    /// The guest decided to leave: the ladder then gives it the leave goal.
    pub leave: bool,
}

impl Guest {
    /// A new guest of type `ty` with its keys `k` (guests 5.2, 13.2): the draws are four colours,
    /// the random start stat, female and child, one per check, the favourite among `favourites`
    /// and the speed offset. `reset` is `[AI] GuestResetInterval`.
    pub fn create(
        k: &Keys,
        ty: &EntityType,
        favourites: &[i32],
        date: Date,
        reset: i32,
        rng: &mut Rng,
    ) -> Guest {
        for _ in 0..COLOURS {
            rng.rand();
        }
        let mut values = [k.initial_happiness, 0, 0, 0, 0];
        if k.random_nth >= 1 && rng.pick(k.random_nth as u32) == 0 {
            let stat = (rng.rand() & 3) as usize;
            let (min, max) = k.random[stat];
            let upper = if stat == 2 { k.max_energy } else { 100 };
            if min < max && min >= 0 && max <= upper {
                values[stat + 1] = min + rng.rand() as i32 % (max - min);
            }
        }
        let (mut female, mut child) = (rng.rand() & 1 == 1, rng.rand() & 1 == 1);
        match &ty.type_name[..] {
            b"woman" => (female, child) = (true, false),
            b"girl" => (female, child) = (true, true),
            b"man" => (female, child) = (false, false),
            b"boy" => (female, child) = (false, true),
            _ => {}
        }
        let countdowns = k.periods.map(|p| match p {
            0 => -1,
            p => rng.rand() as i32 % p,
        });
        let favourite = match favourites.len() {
            0 => None,
            n => Some(favourites[rng.pick(n as u32) as usize]),
        };
        Guest {
            values,
            countdowns,
            saw_animal: k.saw_animal_reset,
            reset,
            favourite,
            female,
            child,
            arrived: Some(date),
            speed_offset: rng.pick(11),
            sick: date.month == 1 && date.day == 1,
            ..Guest::default()
        }
    }
}

/// What a creation needs besides the world.
#[derive(Debug, Clone, Copy)]
pub struct Arrival {
    /// The adult admission price and what the economy multiplies it by.
    pub price: f32,
    pub multiple: f32,
    /// The guests of the last sums, and `[AI] maxGuests`.
    pub guests: i32,
    pub max_guests: i32,
    pub date: Date,
    /// `[AI] GuestResetInterval`.
    pub reset: i32,
}

/// Whether an exhibit holds an animal (guests 3.1 step 1).
pub fn occupied(world: &World, cat: &Catalogue) -> bool {
    let list = world.exhibits().list();
    list.iter()
        .any(|x| !rating::input(world, cat, x.id).animals.is_empty())
}

/// The name ids of the species that are not special animals, in order, one per name id: the
/// subtypes of a species share it and the last one loaded decides (guests 5.2 step 9).
fn favourites(cat: &Catalogue) -> Vec<i32> {
    let mut species = std::collections::BTreeMap::new();
    for t in cat.types.iter().filter(|t| t.class == "animals") {
        species.insert(
            t.name_id,
            crate::build::number(t, "cIsSpecialAnimal") != Some(1.0),
        );
    }
    (species.into_iter()).filter(|s| s.1).map(|s| s.0).collect()
}

/// Creates a guest at the zoo entrance (guests 3.2), the draws in the original's order, and books
/// its admission. Whether one was made: not with the last sums at the limit, without an entrance
/// tile or without a guest type.
pub fn arrive(
    world: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    money: &mut Money,
    a: &Arrival,
) -> bool {
    let (ex, ey) = world.zoo.entrance;
    let entrance = match (ex, ey) {
        (0, 0) => None,
        (x, y) => world.tile_index(
            i32::try_from(x).unwrap_or(-1),
            i32::try_from(y).unwrap_or(-1),
        ),
    };
    let types: Vec<&EntityType> = cat.types.iter().filter(|t| t.class == "guests").collect();
    let (Some(tile), false) = (entrance, types.is_empty() || a.guests >= a.max_guests) else {
        return false;
    };
    let ty = types[rng.pick(types.len() as u32) as usize];
    let guest = Guest::create(&Keys::of(ty), ty, &favourites(cat), a.date, a.reset, rng);
    // The tile is drawn from the entrance tiles, one draw with a single tile.
    rng.pick(1);
    let width = world.zoo.width as usize;
    let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
    let counter = world.zoo.names.entry(COUNTER.into()).or_insert(0);
    *counter = counter.saturating_add(1);
    let label = match world.units.guest_name.is_empty() {
        true => text(&ty.type_name),
        false => world.units.guest_name.clone(),
    };
    let entity = Entity {
        class: "guests".into(),
        type_name: text(&ty.type_name),
        subtype: String::new(),
        body_size: None,
        common: Some(Common {
            x: (tile % width) as i32 * 64 + 32,
            y: (tile / width) as i32 * 64 + 32,
            z: world.zoo.tiles[tile].elevation * 16,
            rotation: 0,
            id: 0,
            name: format!("{label} {}", world.zoo.names[COUNTER]),
            colors: Vec::new(),
            fence: None,
            food: 0,
            movement_mode: 0,
            animal: None,
        }),
    };
    let shape = Shape {
        kind: crate::render::order::Kind::of_class("guests"),
        footprint: (ty.footprint[0], ty.footprint[1]),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: TankKind::NONE,
        walkable: (false, false),
    };
    let child = guest.child;
    let Some(id) = world.unrecorded(|w| w.add(entity, Some(shape))) else {
        return false;
    };
    if let Some(unit) = world.units.get_mut(id) {
        unit.part = Part::Guest(guest);
        unit.started = true;
    }
    let fee = match child {
        true => a.price * 0.5 * a.multiple,
        false => a.price * a.multiple,
    };
    money.refund(money::ADMISSIONS_INCOME, fee);
    money.count(money::ADMISSIONS);
    money.count(money::GUESTS_CREATED);
    let who = if child { "child" } else { "adult" };
    world.units.guest_note(format!("add {id} {who}"));
    true
}
