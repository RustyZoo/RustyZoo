//! Guests: their arrival at the zoo entrance and the state they start with (guests 3 to 5, 13).
//!
//! The new guest check of the zoo status ([`crate::status::zoo::Status::new_guest_check`]) decides
//! whether a guest arrives; [`arrive`] makes it: the type with equal chance, the status values and
//! the check countdowns it starts with ([`Guest::create`], every draw in the original's order),
//! the place at the centre of the entrance tile, the name and the admission booked (a child pays
//! half). The way out is the leave goal ([`crate::goal`]): a guest whose `leave` flag is up walks
//! to the entrance and is removed there.
//!
//! A guest of a loaded map is made by [`load`] with the draws of a creation and then takes the
//! fields its record stores; a start with a reset sets every arrival to the game date
//! ([`arrive_all`]).
//!
//! At each status tick [`checks::needs`] applies the pending changes and [`checks::checks`] runs
//! the checks of guests 6 in the original's order: the needs grow, the trash check costs
//! happiness, the leave check raises the flag of an unhappy guest, the viewing check
//! ([`view`]) changes happiness by the animals in sight. The site choice, the walk to a viewing
//! area and viewing are the goals' ([`crate::goal`]); what a guest sees from an area is [`view`].
//! The seek parts of the need checks send a guest to a building that satisfies the need
//! ([`crate::goal`], [`crate::building`]); the items it buys are held in [`Guest::items`].
//! The surroundings check ([`mega`], guests 6.11) reads the megatile under the guest: a beautiful
//! area pleases, a crowd annoys, a smell makes it sick. The thoughts ([`thought`]) a guest thinks
//! at its checks are kept newest first for the panels. Litter ([`litter`]) is dropped on the ground
//! when no trash can will take it.
//! Gawk scenery ([`gawk`]) is a site like an exhibit: the list of scenery worth a look, the weight
//! of what a guest has not seen and the choice of the nearest object.
//! Missing so far (other issues): the Halloween colours; the entrance tiles other than the zoo
//! entrance of the map.
//!
//! The log family `guest`: `guest add <id> <adult|child>` for each guest made, `guest status <id>
//! <happiness> <hunger> <thirst> <bathroom> <energy>` after each update of the values, `guest
//! view <id> <exhibit>` when a guest starts viewing an exhibit, `guest donate <id> <exhibit>
//! <amount>` for a donation, `guest thought <id> <text id>` for each thought, `guest litter <id>
//! <x>,<y>` for a piece of litter dropped on a tile and `guest leave <id>` for each one that left
//! and was removed. The buildings a guest uses log the family `building` ([`crate::building`]).

use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating;
use crate::money::{self, Money};
use crate::sim::Rng;
use crate::timing::{Date, file_time};
use crate::unit::Part;
use crate::world::{Shape, TankKind, World};
use crate::zoo::{Common, Entity, GuestRecord};

pub mod checks;
pub mod gawk;
pub mod litter;
pub mod mega;
#[cfg(test)]
mod tests;
pub mod thought;
pub mod view;

pub use checks::{checks, needs};

/// The status values of a guest, in the order of [`Guest::values`].
pub const HAPPINESS: usize = 0;
pub const HUNGER: usize = 1;
pub const THIRST: usize = 2;
pub const ENERGY: usize = 3;
pub const BATHROOM: usize = 4;

/// The need list of the shipped building types, by the index a unit record stores for the need
/// being served (guests 7.4).
pub const NEED_LIST: [&str; 16] = [
    "bathroom",
    "building",
    "compost",
    "drink",
    "eat",
    "energy",
    "familybathroom",
    "food",
    "fun",
    "gift",
    "trash",
    "animalrest",
    "animalsex",
    "grandstand",
    "showtrick",
    "fungift",
];
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
/// The keys of the four needs in the order of [`Guest::values`] from hunger: increment, threshold
/// and the happiness change at the top, with the program's value for a missing key.
#[rustfmt::skip]
const NEEDS: [[(&str, i32); 3]; 4] = [
    [("cHungerIncrement", 5), ("cHungerThreshold", 70), ("cAngryFoodChange", -20)],
    [("cThirstIncrement", 10), ("cThirstThreshold", 60), ("cAngryThirstChange", -20)],
    [("cEnergyIncrement", 10), ("cEnergyThreshold", 90), ("cAngryEnergyChange", -10)],
    [("cBathroomIncrement", 5), ("cBathroomThreshold", 70), ("cAngryBathroomChange", -30)],
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
    /// Hunger, thirst, energy and bathroom: increment, threshold and the change at the top.
    pub needs: [[i32; 3]; 4],
    /// `cLeaveChanceLow`, `Med`, `High` and `Done`, and `cAngryTrashChange`.
    pub leave: [i32; 4],
    pub trash: i32,
    /// `cBuySouvenirChanceMed` and `High`.
    pub souvenir: [i32; 2],
    pub view: View,
    /// `cMimicChance`, `cTestFenceChance`, `cTapWallChance`.
    pub reactions: [i32; 3],
    /// `cGawkTime` (five) and `cGawkAttrTime` (six): seconds of viewing by happiness, then more
    /// by attractiveness.
    pub gawk: Vec<i32>,
    pub gawk_attr: Vec<i32>,
    /// `cSearchRadius` (tiles), and `cPriceHappy1Change` and `cPriceAngry1Change`.
    pub search_radius: i32,
    pub price: [i32; 2],
    /// The type's name id, which picks its esthetic bonus (guests 6.11).
    pub name_id: i32,
    /// The surroundings check: `cObjectEstheticThreshold`, `cHappyEstheticChange`,
    /// `cStinkThreshold`, `cSickChange` and `cSickChance`.
    pub env: [i32; 5],
}

/// The changes of the viewing check (guests visit 6.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub sick: i32,
    /// `cHappyAnimalChange1`, `2`; `cAngryAnimalChange1` to `3`.
    pub happy: [i32; 2],
    pub angry: [i32; 3],
    /// Threshold and change of the variety of species, and of the crowding.
    pub species: (i32, i32),
    pub crowd: (i32, i32),
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
        let list = |key: &str| {
            let found = config.and_then(|config| {
                [&ty.type_name[..], default, b""]
                    .into_iter()
                    .find_map(|prefix| {
                        let name = match prefix.is_empty() {
                            true => b"Characteristics/Integers".to_vec(),
                            false => [prefix, b"/Characteristics/Integers"].concat(),
                        };
                        let all = config.section(name)?.get_all(key);
                        (!all.is_empty()).then(|| all.iter().filter_map(|v| to_int(v)).collect())
                    })
            });
            found.unwrap_or_default()
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
            needs: NEEDS.map(|row| row.map(|(key, fallback)| int(key, fallback))),
            leave: [
                int("cLeaveChanceLow", 10),
                int("cLeaveChanceMed", 15),
                int("cLeaveChanceHigh", 20),
                int("cLeaveChanceDone", 5),
            ],
            trash: int("cAngryTrashChange", -10),
            souvenir: [
                int("cBuySouvenirChanceMed", 60),
                int("cBuySouvenirChanceHigh", 90),
            ],
            view: View {
                sick: int("cSickAnimalChange", -10),
                happy: [
                    int("cHappyAnimalChange1", 10),
                    int("cHappyAnimalChange2", 5),
                ],
                angry: [
                    int("cAngryAnimalChange1", -5),
                    int("cAngryAnimalChange2", -10),
                    int("cAngryAnimalChange3", -15),
                ],
                species: (
                    int("cDifferentSpeciesThreshold", 4),
                    int("cDifferentSpeciesChange", 10),
                ),
                crowd: (
                    int("cCrowdedViewingThreshold", 70),
                    int("cCrowdedViewingChange", -10),
                ),
            },
            reactions: [
                int("cMimicChance", 10),
                int("cTestFenceChance", 0),
                int("cTapWallChance", 0),
            ],
            gawk: list("cGawkTime"),
            gawk_attr: list("cGawkAttrTime"),
            search_radius: int("cSearchRadius", 11),
            price: [int("cPriceHappy1Change", 5), int("cPriceAngry1Change", -5)],
            name_id: ty.name_id,
            env: [
                int("cObjectEstheticThreshold", 60),
                int("cHappyEstheticChange", 10),
                int("cStinkThreshold", 2),
                int("cSickChange", -20),
                int("cSickChance", 20),
            ],
        }
    }
}

/// The live state of a guest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guest {
    /// The colour choices drawn at the creation, one per colour group (save games 11.2).
    pub colours: Vec<u8>,
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
    /// The tiles of the last 10 sites seen, oldest first, and how many sites it has seen since the
    /// list was emptied (guests visit 3.5).
    pub seen: Vec<usize>,
    pub seen_count: u32,
    /// The species (name ids) it saw while viewing, the last 5, oldest first.
    pub recent: Vec<i32>,
    /// The direction in which it looks for its viewing area (guests visit 4.2).
    pub va: Option<u8>,
    /// The calendar time viewing began, in milliseconds.
    pub viewed_at: i64,
    /// The items held: food, drink, fun gift, gift and litter, each an item name id (guests 9.2).
    pub items: [Option<i32>; crate::building::items::SLOTS],
    /// The need being served and the building target (guests 7.3): the building's id, 0 for none,
    /// and the slot and entrance it was chosen with.
    pub need: Option<&'static str>,
    pub building: u32,
    pub slot: usize,
    pub entrance: crate::building::Point,
    /// The eat-standing flag, and the wish for a gift (guests 6.9, 8).
    pub stand: bool,
    pub gift: bool,
    /// The tour guide it follows, 0 for none, and whether it heard the guide's talk (tour guide 10).
    pub guide: u32,
    pub listening: bool,
    /// The last drop of litter failed (guests 10).
    pub littered: bool,
    /// The name id of the return building the guest means to come back to; while set, no
    /// attraction or scenery object counts as seen (guests visit 3.2). Nothing sets it before M7.
    pub returning: Option<i32>,
}

impl Guest {
    /// What a record stores of this guest, `base` being the one it was loaded with or made as: the
    /// afraid flag, which nothing raises, and a favourite or arrival date that did not change are
    /// kept as they were. The guest follows a guide exactly when it is in a group.
    pub fn record(&self, base: &GuestRecord, wandered: bool) -> GuestRecord {
        let arrived = match self.arrived {
            Some(d) if d != Date::from_file_time(base.arrived) => file_time(d.game_ms()),
            _ => base.arrived,
        };
        GuestRecord {
            values: self.values,
            countdowns: self.countdowns,
            arrived,
            favourite: self.favourite.unwrap_or(base.favourite),
            sick: self.sick,
            saw_animal: self.saw_animal,
            female: self.female,
            child: self.child,
            items: self.items.map(|i| i.map_or(0, |n| n as u32)),
            leave: self.leave,
            gift: self.gift,
            wandered,
            stand: self.stand,
            in_group: self.guide != 0,
            listening: self.listening,
            guide: self.guide,
            ..*base
        }
    }

    /// The guest forgets the sites it has seen (guests visit 3.6).
    pub fn forget(&mut self) {
        self.seen.clear();
        self.seen_count = 0;
    }

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
        let colours = crate::unit::draw_colours(Some(ty), COLOURS, rng);
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
            colours,
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

/// The keys of the guest at position `at` of the entity list; none when its type is unknown.
pub fn keys(world: &World, cat: &Catalogue, at: usize) -> Option<Keys> {
    type_of(world, cat, at).map(Keys::of)
}

/// The type of the entity at position `at` of the list, if the catalogue has it.
fn type_of<'a>(world: &World, cat: &'a Catalogue, at: usize) -> Option<&'a EntityType> {
    let e = world.zoo.entities.get(at)?;
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
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

/// Whether an animal is loose on the zoo grounds.
fn loose(world: &World) -> bool {
    (world.units.iter()).any(|u| matches!(&u.part, crate::unit::Part::Animal(a) if a.escaped))
}

/// Whether an exhibit holds an animal and none is loose (guests 3.1 steps 1 to 3: a guest
/// arrives in neither case otherwise).
pub fn occupied(world: &World, cat: &Catalogue) -> bool {
    let list = world.exhibits().list();
    let held = (list.iter()).any(|x| !rating::input(world, cat, x.id).animals.is_empty());
    held && !loose(world)
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

/// Creates the guest at position `at` of a loaded map as the loader does (guests 5.2, zoo format
/// 6.12): through its type, with all the draws of a creation, and then the fields its record
/// stores: the five status values, the sick flag, female and child, the check countdowns, the
/// saw-animal countdown, the item slots, the leave, wants-a-gift, wandered and eat-standing flags,
/// the arrival date, the favourite species, the listening flag and the guide id. A favourite that
/// is not a loaded species is drawn anew among the species that are not special animals, as at
/// arrival; with none, the guest has no favourite. `date` is the game date of the load and `reset`
/// is `[AI] GuestResetInterval`. The caller goes in file order before the first step; a guest
/// whose type is unknown is left as it is.
pub fn load(world: &mut World, cat: &Catalogue, rng: &mut Rng, at: usize, date: Date, reset: i32) {
    let Some((id, record)) =
        (world.zoo.entities[at].common.as_ref()).and_then(|c| Some((c.id, *c.guest.as_deref()?)))
    else {
        return;
    };
    let Some(ty) = type_of(world, cat, at) else {
        return;
    };
    let mut guest = Guest::create(&Keys::of(ty), ty, &favourites(cat), date, reset, rng);
    let known = |name| {
        cat.types
            .iter()
            .any(|t| t.class == "animals" && t.name_id == name)
    };
    guest.favourite = if known(record.favourite) {
        Some(record.favourite)
    } else {
        let list = favourites(cat);
        (!list.is_empty()).then(|| list[rng.pick(list.len() as u32) as usize])
    };
    guest.values = record.values;
    guest.countdowns = record.countdowns;
    guest.arrived = Some(Date::from_file_time(record.arrived));
    (guest.sick, guest.saw_animal) = (record.sick, record.saw_animal);
    (guest.female, guest.child) = (record.female, record.child);
    guest.items = record.items.map(|n| (n != 0).then_some(n as i32));
    (guest.leave, guest.gift, guest.stand) = (record.leave, record.gift, record.stand);
    (guest.guide, guest.listening) = (record.guide, record.listening);
    if let Some(unit) = world.units.get_mut(id) {
        unit.goal.wandered = record.wandered;
        unit.part = Part::Guest(guest);
    }
}

/// Sets the arrival date of every guest to `date`: a start with `reset` or `resetdate` (zoo format
/// 6.12).
pub fn arrive_all(world: &mut World, date: Date) {
    let ids: Vec<u32> = (world.zoo.entities.iter())
        .filter(|e| e.class == "guests")
        .filter_map(|e| e.common.as_ref().map(|c| c.id))
        .collect();
    for id in ids {
        if let Some(Part::Guest(g)) = world.units.get_mut(id).map(|u| &mut u.part) {
            g.arrived = Some(date);
        }
    }
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
            colors: guest.colours.clone(),
            fence: None,
            ..Default::default()
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
