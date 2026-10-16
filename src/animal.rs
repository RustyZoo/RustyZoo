//! The status variables of an animal and the conditional checks that judge its company and its
//! exhibit (animals 3 to 5, 8).
//!
//! An animal has happiness, hunger, health, energy and a dung meter, each with a pending change
//! that the checks add to; at the next status tick [`needs`] applies it, clamps the value to the
//! species' limits and clears it. [`checks`] then runs the checks of the animal's turn in the
//! original's order: the reset countdown, keeper visit and mourning, the escape check's draws,
//! captivity (which may trample the grass), social, keeper, habitat, bored, other animals, the
//! zap draw, the "not happy" message and the ambient choice. The checks are plain functions of
//! the animal, an [`Env`] that describes where it stands, the check periods and the generator
//! ([`run`]); the hook reads the environment from the world, posts the messages and paints the
//! trampled tile.
//!
//! An animal of a loaded map is created by [`load`] before the first step, with the draws of a
//! new one, and then takes what its record stores (animals 3.5); it draws nothing at its turn.
//!
//! Hunger, the food entity and the eating are [`food`]: the hunger check and `fEat` run among the
//! checks, their effects on the food and on the goal are applied after them.
//!
//! Not here yet: the shelter check, the other checks of part B, the swimmers and the effects of the
//! escape, rampage and zap checks (M7: only their counters and draws run). The ambient choice is
//! taken at the end of the checks by the behaviour runner ([`Animal::ambient`]).
//!
//! The log family `animal`: `animal status <id> <happiness> <hunger> <energy> <health>` right
//! after each update of the values, `animal message <string> <id>` for a message posted and
//! `animal trample <id> <x>,<y>` for a tile trampled.

use crate::behaviour::Data;
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating::Record;
use crate::exhibit::{EXTERIOR, GROUNDS, rating};
use crate::sim::Rng;
use crate::unit::{Ctx, Part, Unit};
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::{AnimalRecord, Entity};

pub mod food;
#[cfg(test)]
mod tests;

use food::Meal;

/// The messages "not happy" and "dislikes its exhibit": string numbers whose text holds the name.
pub const NOT_HAPPY: i32 = 10028;
pub const DISLIKES_EXHIBIT: i32 = 10031;
/// "Can't find any food".
pub const NO_FOOD: i32 = 10030;

/// The counters of the checks, in the order the creation draws them (animals 3.2).
pub const HUNGER: usize = 0;
pub const BREATH: usize = 1;
pub const HEALTH: usize = 2;
pub const ENERGY: usize = 3;
pub const ESCAPE: usize = 4;
pub const OTHER: usize = 5;
pub const KEEPER: usize = 6;
pub const CAPTIVITY: usize = 7;
pub const HABITAT: usize = 8;
pub const BORED: usize = 9;
pub const SOCIAL: usize = 10;
pub const REPRODUCTION: usize = 11;
pub const MESSAGE: usize = 12;
pub const SHELTER: usize = 13;
pub const WATER: usize = 14;
pub const MURKY: usize = 15;
const COUNTERS: usize = 16;

/// The recommendation bits this part owns (animals 8).
/// The bit of "hungry" (9901): no food for it.
const HUNGRY: u32 = 0;
const ANGRY: u32 = 3;
const FEW: u32 = 4;
const MANY: u32 = 5;
const STIMULATION: u32 = 6;
const INCOMPATIBLE: u32 = 10;
const CRAMPED: u32 = 16;
const CROWDED: u32 = 56;
const NEEDS_EXHIBIT: u32 = 62;

/// The terrain type of trampled ground.
const TRAMPLED: u8 = 16;

/// The check periods in the order of the counters: key and the program's default (animals 3.4).
const PERIODS: [(&str, i32); COUNTERS] = [
    ("cHungerCheck", 5),
    ("cBreathCheck", 5),
    ("cHealthCheck", 10),
    ("cEnergyCheck", 3),
    ("cEscapedCheck", 1),
    ("cOtherCheck", 5),
    ("cKeeperCheck", 10),
    ("cCaptivityCheck", 5),
    ("cHabitatCheck", 3),
    ("cBoredCheck", 5),
    ("cSocialCheck", 5),
    ("cReproductionCheck", 10),
    ("", 10),
    ("cBuildingUseCheck", 5),
    ("cWaterCheck", 30),
    ("cMurkyWaterCheck", 5),
];

/// The class keys of `animals.ai` (animals 3.4), read once for every animal type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub periods: [i32; COUNTERS],
    pub death_change: i32,
    pub keeper_arrives: i32,
    pub keeper_arrives_amphibious: i32,
    /// `cZapCheck`: the period of the zap counter, which creation does not draw.
    pub zap: i32,
    /// `cUIThreshold`, `cDirtChanceChance` and `cDirtChance` (animals 4.5).
    pub ui_threshold: i32,
    pub dirt_chance_chance: i32,
    pub dirt_chance: i32,
    /// The terrain type of trampled ground: 16, or 0 when the terrain table has fewer than 17.
    pub trample: u8,
    /// `AnimalResetInterval` of the ini file (0 for never); the hosts set it.
    pub reset_interval: i32,
}

impl Default for Class {
    /// The program's values for a missing file or key.
    fn default() -> Class {
        Class::read(&|_, fallback| fallback)
    }
}

impl Class {
    /// The keys of `animals.ai`; a file that is missing or unreadable keeps the defaults. The
    /// terrain types are those of the install.
    pub fn load(vfs: &Vfs) -> Class {
        let file = vfs
            .read("animals.ai")
            .ok()
            .and_then(|b| Config::parse(&b).ok());
        let class = Class::read(&|key, fallback| {
            (file.as_ref())
                .and_then(|c| c.section("Characteristics/Integers")?.get(key))
                .and_then(to_int)
                .unwrap_or(fallback)
        });
        let trample = if crate::terrain::types(vfs).len() > TRAMPLED.into() {
            TRAMPLED
        } else {
            0
        };
        Class { trample, ..class }
    }

    fn read(int: &dyn Fn(&str, i32) -> i32) -> Class {
        Class {
            periods: PERIODS.map(|(key, period)| {
                if key.is_empty() {
                    period
                } else {
                    int(key, period)
                }
            }),
            death_change: int("cDeathChange", -21),
            keeper_arrives: int("cKeeperArrivesChange", 100),
            keeper_arrives_amphibious: int("cKeeperArrivesChangeAmphibious", 100),
            zap: int("cZapCheck", 5),
            ui_threshold: int("cUIThreshold", 50),
            dirt_chance_chance: int("cDirtChanceChance", 50),
            dirt_chance: int("cDirtChance", 10),
            trample: TRAMPLED,
            reset_interval: 0,
        }
    }
}

/// Declares [`Keys`]: the keys of one species with the program's default for an absent one.
macro_rules! keys {
    ($($field:ident: $key:literal = $default:expr,)*) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Keys {
            $(pub $field: i32,)*
            /// `cKeeperArrivesChange`, which falls back to the class file's.
            pub keeper_arrives: i32,
            /// `cDirtChance`, which falls back to the class file's.
            pub dirt_chance: i32,
            /// `cUnderwater` 1 and `cOnlyUnderwater` 0.
            pub amphibious: bool,
        }

        impl Keys {
            /// The keys through `int`, which gives the value of a key or the fallback it is handed.
            pub fn read(int: &dyn Fn(&str, i32) -> i32, class: &Class) -> Keys {
                let amphibious = int("cUnderwater", 0) == 1 && int("cOnlyUnderwater", 0) == 0;
                Keys {
                    $($field: int($key, $default),)*
                    keeper_arrives: match amphibious {
                        true => int("cKeeperArrivesChangeAmphibious", class.keeper_arrives_amphibious),
                        false => int("cKeeperArrivesChange", class.keeper_arrives),
                    },
                    dirt_chance: int("cDirtChance", class.dirt_chance),
                    amphibious,
                }
            }
        }
    };
}

keys! {
    initial_happiness: "cInitialHappiness" = 60,
    max_hits: "cMaxHits" = 500,
    max_energy: "cMaxEnergy" = 100,
    min_dirty: "cMinDirty" = 0,
    max_dirty: "cMaxDirty" = 100,
    habitat_preference: "cHabitatPreference" = 70,
    pct_habitat: "cPctHabitat" = 10,
    happy_habitat: "cHappyHabitatChange" = 20,
    angry_habitat: "cAngryHabitatChange" = -10,
    very_angry_habitat: "cVeryAngryHabitatChange" = -20,
    captivity: "cCaptivity" = 5,
    number_min: "cNumberAnimalsMin" = 1,
    number_max: "cNumberAnimalsMax" = 10,
    habitat_size: "cHabitatSize" = 0,
    number_min_change: "cNumberMinChange" = -5,
    number_max_change: "cNumberMaxChange" = -5,
    density: "cAnimalDensity" = 30,
    all_crowded_change: "cAllCrowdedChange" = -10,
    crowd: "cCrowd" = 40,
    crowd_change: "cCrowdHappinessChange" = -1,
    energy_threshold: "cEnergyThreshold" = 70,
    hunger_threshold: "cHungerThreshold" = 100,
    drink: "cDrinkWaterChance" = 0,
    chase: "cChaseAnimalChance" = 0,
    other_angry: "cOtherAnimalAngryChange" = -30,
    rampage_threshold: "cRampageThreshold" = -100,
    rampage_chance: "cRampageChance" = 0,
    rampage_min: "cRampageTimeMin" = 0,
    rampage_max: "cRampageTimeMax" = 0,
    test_fence: "cTestFenceChance" = 0,
    hunger_increment: "cHungerIncrement" = 10,
    hungry_health: "cHungryHealthChange" = 5,
    no_food_change: "cNoFoodChange" = -20,
    food_kind: "cKeeperFoodType" = 0,
    bite: "cKeeperFoodUnitsEaten" = 20,
    food_value: "cFoodUnitValue" = 1,
}

impl Keys {
    /// The keys of an animal type: the section of its subtype, of the default subtype, then
    /// the plain one (entity types 6.2).
    pub fn of(ty: &EntityType, class: &Class) -> Keys {
        let Some(config) = ty.config.as_deref() else {
            return Keys::read(&|_, fallback| fallback, class);
        };
        let default = (config.section("Global")).and_then(|g| g.get("DefaultSubtype"));
        let int = |key: &str, fallback: i32| {
            [&ty.subtype[..], default.unwrap_or_default(), b""]
                .into_iter()
                .find_map(|prefix| {
                    let name = match prefix.is_empty() {
                        true => b"Characteristics/Integers".to_vec(),
                        false => [prefix, b"/Characteristics/Integers"].concat(),
                    };
                    config.section(name)?.get(key).and_then(to_int)
                })
                .unwrap_or(fallback)
        };
        Keys::read(&int, class)
    }
}

/// The live state of an animal: the values, their pending changes, the counters, the flags that
/// the checks read and raise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Animal {
    pub happiness: i32,
    pub hunger: i32,
    pub health: i32,
    pub energy: i32,
    pub dung: i32,
    pub happiness_change: i32,
    pub hunger_change: i32,
    pub health_change: i32,
    pub energy_change: i32,
    pub dung_change: i32,
    /// The counters of the checks, indexed by [`HUNGER`] and the other names.
    pub counters: [i32; COUNTERS],
    pub female: bool,
    /// The game milliseconds of the calendar when it last ate and last slept (animals 13.2).
    pub fed: u64,
    pub slept: u64,
    /// The recommendation bits shown and the ones among them that are important.
    pub advice: u64,
    pub important: u64,
    /// The guests' pressure on the animal: the total and its x and y parts (viewing areas).
    pub pressure: [i32; 3],
    /// The direction away from the pressing guests, 0 to 7, or -1 for none.
    pub crowd_direction: i8,
    pub egg: bool,
    pub crated: bool,
    pub dying: bool,
    pub in_show: bool,
    pub very_unhappy: bool,
    pub at_minimum_health: bool,
    /// Raised by a keeper starting its duties in the exhibit, and by the death of an animal.
    pub keeper_visited: bool,
    pub mourning: bool,
    /// Raised by the bored check (9.2); the drink goal of a later issue lowers it.
    pub thirsty: bool,
    /// Hungry for keeper food (the ladder starts the keeper food goal), eating, and "hungry and
    /// no food": the first hungry check that finds no food only notes it (animals-needs 4.3).
    pub wants_food: bool,
    pub eating: bool,
    pub no_food: bool,
    /// The food entity it goes to and eats from; 0 for none.
    pub food: u32,
    /// The rampage flag and its counter (animals 11): they gate the rampage draws and the bored
    /// check; nothing else follows from them before M7.
    pub rampaging: bool,
    pub rampage: i32,
    /// The zap counter (animals 4.1): it starts at 0 and fires when it was below 1.
    pub zap: i32,
    /// Status ticks until the creation runs again (`AnimalResetInterval`).
    pub reset: i32,
    /// The end of the checks asks for the ambient choice (behaviours 6.4): the behaviour runner
    /// takes the request and starts the set its table gives for the happiness.
    pub ambient: bool,
}

impl Animal {
    /// The animal as creation leaves it (animals 3.2): the starts, every counter drawn from
    /// `rng` (16 draws, 17 for a young animal whose sex is drawn first).
    pub fn create(k: &Keys, class: &Class, subtype: &str, rng: &mut Rng) -> Animal {
        let female = match subtype {
            "m" => false,
            "f" | "p" => true,
            _ => rng.rand() & 1 == 0,
        };
        let mut counters = [-1; COUNTERS];
        for (c, &p) in counters.iter_mut().zip(&class.periods) {
            // A negative period draws too: the remainder keeps the sign of the number drawn.
            if p != 0 {
                *c = (rng.rand() as i32) % p;
            }
        }
        Animal {
            happiness: k.initial_happiness,
            health: k.max_hits,
            counters,
            female,
            reset: class.reset_interval,
            crowd_direction: -1,
            ..Animal::default()
        }
    }

    /// Overwrites what the record of a loaded map stores (animals 3.5); the rest keeps the value
    /// creation gave.
    pub fn restore(&mut self, saved: &AnimalRecord) {
        let r = saved;
        (self.happiness, self.hunger, self.health, self.energy) =
            (r.happiness, r.hunger, r.health, r.energy);
        (self.dung, self.female) = (r.dung, r.female);
        for (c, saved) in self.counters.iter_mut().zip(&r.counters) {
            *c = saved.unwrap_or(*c);
        }
        self.at_minimum_health = r.at_minimum_health;
        self.very_unhappy = r.very_unhappy;
        self.keeper_visited = r.keeper_visited;
        self.mourning = r.mourning;
        self.dying = r.dying;
        self.crated = r.crated;
        self.egg = r.egg;
        self.thirsty = r.thirsty;
        self.rampaging = r.rampaging;
        self.rampage = r.rampage;
    }

    /// Applies the pending changes (animals 3.3).
    pub fn update(&mut self, k: &Keys) {
        // The high limit first, then the low one: a low limit above the high one wins.
        fn apply(v: &mut i32, d: &mut i32, lo: i32, hi: i32) {
            *v = v.saturating_add(*d).min(hi).max(lo);
            *d = 0;
        }
        if self.egg {
            self.health_change = self.health_change.saturating_sub(1);
        } else {
            apply(&mut self.happiness, &mut self.happiness_change, -100, 100);
            apply(&mut self.hunger, &mut self.hunger_change, 0, 100);
            apply(&mut self.energy, &mut self.energy_change, 0, k.max_energy);
            apply(
                &mut self.dung,
                &mut self.dung_change,
                k.min_dirty,
                k.max_dirty,
            );
            self.very_unhappy = self.happiness < -80;
        }
        apply(&mut self.health, &mut self.health_change, 0, k.max_hits);
        self.at_minimum_health = self.health == 0;
    }

    fn change(&mut self, by: i32) {
        self.happiness_change = self.happiness_change.saturating_add(by);
    }

    fn mark(&mut self, bit: u32, important: bool) {
        self.advice |= 1 << bit;
        self.important |= u64::from(important) << bit;
    }

    fn unmark(&mut self, bit: u32) {
        self.advice &= !(1 << bit);
        self.important &= !(1 << bit);
    }

    /// Whether it is doing something (9.3). The set and the task are the environment's.
    fn occupied(&self, env: &Env) -> bool {
        self.thirsty || self.in_show || self.wants_food || self.dying || env.busy
    }

    /// Clears the bits, then sets "needs a suitable exhibit" when `set`.
    fn exhibit_bit(&mut self, set: bool) {
        (self.advice, self.important) = (0, 0);
        if set {
            self.mark(NEEDS_EXHIBIT, true);
        }
    }
}

/// A counter with period `period` (animals 4.1): it runs down, and the check acts when it was 0.
fn fire(counter: &mut i32, period: i32) -> bool {
    let old = *counter;
    *counter = old.wrapping_sub(1);
    if old == 0 {
        *counter = period.wrapping_sub(1);
    }
    old == 0
}

/// The animals the social, bored and other checks count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Census {
    /// Animals of its species in its exhibit, itself included.
    pub species: u32,
    /// Adults (subtype `m` or `f`) of its species, and of every species, in its exhibit and its
    /// amphibious neighbours.
    pub adults: u32,
    pub all_adults: u32,
    /// The other animals of its exhibit that are very unhappy.
    pub unhappy: u32,
}

/// Where the animal stands and what the checks read of it.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// It stands in an exhibit and not on the grounds.
    pub exhibit: bool,
    /// The grounds or beyond the zoo wall.
    pub grounds: bool,
    /// Its subtype is neither `m`, `f` nor `p`: a creation draws its sex.
    pub young: bool,
    /// The exhibit's number of tiles, its amphibious neighbours' included.
    pub size: usize,
    /// What the exhibit's record says: the animals of the species and the rating shown.
    pub species: i32,
    pub rating: i32,
    /// What the species think of each other, the record's other-animals sum (habitat rating
    /// 5.5.7), rounded.
    pub other: i32,
    pub census: Census,
    /// The ground under it can be trampled: terrain 0, 1, 4, 11 or 13 and nothing in the four
    /// slots of the tile.
    pub grass: bool,
    /// The tile holds water or the exhibit is a tank: it does not look for a drink here.
    pub wet: bool,
    /// The game milliseconds of the calendar, for the date of its last meal.
    pub now: u64,
    /// Its set or its task occupies it (9.3).
    pub busy: bool,
    /// Its task is the keeper food or the other food task.
    pub food_task: bool,
    /// The food of its species' kind in its exhibit, in units (hunger check 4.1).
    pub food_total: i32,
    /// What eating reads of the food it aims at and of its type.
    pub meal: Meal,
}

/// What a tick of checks asks of the world besides the animal's own state.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Out {
    /// The messages posted, by string number.
    pub messages: Vec<i32>,
    /// The tile under the animal is painted as trampled ground.
    pub trample: bool,
    /// What the eating of this tick asks of the world.
    pub meal: Meal,
}

/// The checks of one status tick, in the original's order (animals 4.2); `rng` is the shared
/// generator, drawn from in that order.
pub fn run(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng) -> Out {
    let mut out = Out {
        meal: env.meal.clone(),
        ..Out::default()
    };
    if a.crated || a.egg {
        a.exhibit_bit(a.crated || env.grounds);
        return out;
    }
    if class.reset_interval > 0 {
        a.reset = a.reset.saturating_sub(1);
        if a.reset <= 0 {
            // The creation again; the subtype only matters for a young animal's drawn sex.
            let subtype = match (env.young, a.female) {
                (true, _) => "y",
                (_, true) => "f",
                _ => "m",
            };
            // The zap counter keeps running down across the reset.
            let zap = a.zap;
            *a = Animal::create(k, class, subtype, rng);
            a.zap = zap;
        }
    }
    if a.dying {
        return out;
    }
    // The keeper's visit and the mourning, at most once each.
    if std::mem::take(&mut a.keeper_visited) {
        a.change(k.keeper_arrives);
    }
    if std::mem::take(&mut a.mourning) {
        a.change(class.death_change);
    }
    if a.eating && a.hunger <= 0 {
        a.stop_eating(false, &mut out.meal);
        a.fed = env.now;
    }
    escape(a, k, class, rng);
    if a.eating {
        food::eat(a, k, &mut out.meal);
    }
    food::hunger(a, k, class, env, &mut out);
    captivity(a, k, class, env, rng, &mut out);
    social(a, k, class, env);
    fire(&mut a.counters[KEEPER], class.periods[KEEPER]);
    habitat(a, k, class, env, &mut out.messages);
    bored(a, k, class, env, rng);
    other(a, k, class, env);
    zap(a, k, class, env, rng);
    if fire(&mut a.counters[MESSAGE], class.periods[MESSAGE]) && a.happiness < -60 {
        out.messages.push(NOT_HAPPY);
    }
    a.ambient = true;
    match env.grounds {
        true => a.exhibit_bit(true),
        false => a.unmark(NEEDS_EXHIBIT),
    }
    out
}

/// The escape check (animals 11): the rampage, then the counter. Nothing follows from either in
/// M6 but the flag and the counter of the rampage: the escape and the sets are M7's.
fn escape(a: &mut Animal, k: &Keys, class: &Class, rng: &mut Rng) {
    if a.in_show {
        return;
    }
    if a.rampaging {
        a.rampage = a.rampage.wrapping_add(1);
        if a.happiness > k.rampage_threshold || a.rampage > k.rampage_max {
            (a.rampaging, a.rampage) = (false, 0);
        }
    } else {
        a.rampage = 0;
        if k.rampage_chance >= 1
            && k.rampage_min >= 1
            && a.happiness <= k.rampage_threshold
            && rng.chance(k.rampage_chance)
        {
            let len = k.rampage_max.wrapping_sub(k.rampage_min);
            if len >= 0 {
                a.rampaging = true;
                a.rampage = len - rng.rand() as i32 % len.wrapping_add(1);
            }
        }
    }
    fire(&mut a.counters[ESCAPE], class.periods[ESCAPE]);
}

/// The captivity tick (animals 4.5): the change, and below the threshold two draws that may
/// trample the ground under the animal.
fn captivity(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng, out: &mut Out) {
    if !fire(&mut a.counters[CAPTIVITY], class.periods[CAPTIVITY]) {
        return;
    }
    a.change(k.captivity.saturating_neg());
    let low = class.ui_threshold.saturating_mul(2).saturating_sub(201);
    if k.dirt_chance > 0
        && a.happiness <= low
        && rng.chance(class.dirt_chance_chance)
        && rng.chance(k.dirt_chance)
    {
        out.trample = env.grass;
    }
}

/// The social check (animals 5.1): the bits at every tick, the changes when the counter fires.
fn social(a: &mut Animal, k: &Keys, class: &Class, env: &Env) {
    [FEW, MANY, CRAMPED, CROWDED]
        .into_iter()
        .for_each(|b| a.unmark(b));
    let adults = f64::from(env.census.adults);
    let size = if env.exhibit { env.size } else { 0 };
    let (mut few, mut many) = (false, false);
    if env.species < k.number_min {
        few = true;
        a.mark(FEW, k.number_min_change < -5);
    } else {
        let f = match size {
            0 => 1.0,
            n => (n as f64 / f64::from(k.habitat_size)).max(1.0),
        };
        if adults > f * f64::from(k.number_max) {
            many = true;
            a.mark(MANY, k.number_max_change < -5);
        }
    }
    let density = match env.census.all_adults {
        0 => k.density,
        n => match (size as f32 / n as f32).round_ties_even() as i32 {
            0 => k.density,
            d => d,
        },
    };
    let cramped = density < k.density;
    if cramped {
        a.mark(CRAMPED, k.all_crowded_change < -5);
    }
    let crowded = !a.egg && !a.in_show && a.pressure[0] > k.crowd;
    if crowded {
        a.mark(CROWDED, k.crowd_change < -5);
    }
    // The counter runs down at every tick but restarts only outside a show (the original's flaw,
    // issue 317: an animal whose counter ends during a show never checks again).
    let old = a.counters[SOCIAL];
    a.counters[SOCIAL] = old.wrapping_sub(1);
    if old != 0 || a.egg || a.in_show {
        return;
    }
    a.counters[SOCIAL] = class.periods[SOCIAL].wrapping_sub(1);
    if few {
        a.change(k.number_min_change);
    } else if many {
        a.change(k.number_max_change);
    }
    if cramped {
        a.change(k.all_crowded_change);
    }
    if crowded {
        a.change(k.crowd_change);
        let [_, px, py] = a.pressure;
        a.crowd_direction = match (px.signum(), py.signum()) {
            (0, 1) => 4,
            (0, -1) => 0,
            (1, 0) => 6,
            (-1, 0) => 2,
            (1, 1) => 5,
            (1, -1) => 7,
            (-1, 1) => 3,
            (-1, -1) => 1,
            _ => a.crowd_direction,
        };
    }
}

/// The habitat check (animals 5.4): the bit at every tick, the change when the counter fires.
fn habitat(a: &mut Animal, k: &Keys, class: &Class, env: &Env, messages: &mut Vec<i32>) {
    let other = env.other.clamp(-500, 150) / 10;
    a.unmark(INCOMPATIBLE);
    if other < 0 {
        a.mark(INCOMPATIBLE, other < -9);
    }
    if !fire(&mut a.counters[HABITAT], class.periods[HABITAT]) || a.egg || a.in_show {
        return;
    }
    let r = env.rating;
    if r >= k.habitat_preference {
        a.change(k.happy_habitat);
    } else if i64::from(r) * 100 < i64::from(k.pct_habitat) * i64::from(k.habitat_preference) {
        a.change(k.very_angry_habitat);
        if env.exhibit {
            messages.push(DISLIKES_EXHIBIT);
        }
    } else {
        a.change(k.angry_habitat);
    }
    a.change(other);
}

/// The bored check (animals 5.5): a fed, rested animal with company may get thirsty. The
/// predator and playmate searches and the chase are M7's and find nothing.
fn bored(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng) {
    if a.egg || a.in_show || !fire(&mut a.counters[BORED], class.periods[BORED]) {
        return;
    }
    a.unmark(STIMULATION);
    // A tired animal tries to sleep (part B).
    if a.energy >= k.energy_threshold || a.at_minimum_health || a.occupied(env) || a.rampaging {
        return;
    }
    if a.hunger >= k.hunger_threshold {
        return;
    }
    let alone = env.census.species <= 1;
    let (drink, play) = (k.drink, if alone { 0 } else { k.chase });
    let total = drink.wrapping_add(play);
    if total > 0 && rng.rand() as i32 % total < drink {
        a.thirsty |= !env.wet;
    }
}

/// The check of other animals (animals 5.2): bit 3 and the change while another animal of the
/// exhibit is very unhappy.
fn other(a: &mut Animal, k: &Keys, class: &Class, env: &Env) {
    if !fire(&mut a.counters[OTHER], class.periods[OTHER]) {
        return;
    }
    a.unmark(ANGRY);
    if env.exhibit && env.census.unhappy > 0 && k.other_angry < 0 {
        a.change(k.other_angry);
        a.mark(ANGRY, k.other_angry < -9);
    }
}

/// The zap check (animals 11): nothing runs for an animal in a show; its counter fires when it
/// was below 1, and the draw of a fence test is made when nothing occupies the animal and its
/// chance is not 0. The test itself is M7's.
fn zap(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng) {
    if a.in_show {
        return;
    }
    let old = a.zap;
    a.zap = old.wrapping_sub(1);
    if old < 1 {
        a.zap = class.zap.wrapping_sub(1);
        if !a.occupied(env) && k.test_fence != 0 {
            rng.chance(k.test_fence);
        }
    }
}

fn find<'c>(cat: &'c Catalogue, e: &Entity) -> Option<&'c EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}

/// The record of species `key` in exhibit `x`; a dirty exhibit is rated first.
fn record(world: &mut World, cat: &Catalogue, x: u32, key: &str) -> Option<Record> {
    if world.exhibits().get(x)?.rating.dirty {
        let input = rating::input(world, cat, x);
        world.exhibits_mut().get_mut(x)?.rating.compute(&input);
    }
    (world.exhibits().get(x)?.rating.records.iter())
        .find(|r| r.key == key)
        .cloned()
}

/// Counts the animals of the regions `near` (an exhibit and its amphibious neighbours) for an
/// animal of species `key`; the species and the very unhappy ones only in the exhibit `x`.
fn census(world: &World, x: u32, near: &[u32], key: &str) -> Census {
    let mut c = Census::default();
    for e in &world.zoo.entities {
        let Some(com) = e.common.as_ref().filter(|_| e.class == "animals") else {
            continue;
        };
        if world.is_carried(com.id) {
            continue;
        }
        let tile = world.tile_index(com.x.div_euclid(64), com.y.div_euclid(64));
        let Some(region) = tile.and_then(|t| world.exhibits().region(t)) else {
            continue;
        };
        if !near.contains(&region) {
            continue;
        }
        let adult = matches!(e.subtype.as_str(), "m" | "f");
        let same = e.type_name == key;
        c.adults += u32::from(same && adult);
        c.all_adults += u32::from(adult);
        if region == x {
            c.species += u32::from(same);
            let unhappy = world
                .units
                .get(com.id)
                .is_some_and(|u| matches!(&u.part, Part::Animal(a) if a.very_unhappy));
            c.unhappy += u32::from(unhappy);
        }
    }
    c
}

/// The keys of the animal at position `at` of the entity list.
pub fn keys(world: &World, cat: &Catalogue, at: usize) -> Keys {
    let class = &world.units.class;
    match find(cat, &world.zoo.entities[at]) {
        Some(ty) => Keys::of(ty, class),
        None => Keys::read(&|_, fallback| fallback, class),
    }
}

/// Creates the animals of a loaded map as the loader does (animals 3.5): each gets its creation
/// in file order, with its draws, and then the values its record stores. They are done before
/// the first step, and their first turn draws nothing.
pub fn load(world: &mut World, cat: &Catalogue, rng: &mut Rng) {
    for at in 0..world.zoo.entities.len() {
        let entity = &world.zoo.entities[at];
        let Some((id, saved)) = (entity.common.as_ref())
            .and_then(|c| Some((c.id, c.animal.clone()?)))
            .filter(|&(id, _)| world.find(id) == Some(at))
        else {
            continue;
        };
        let (k, subtype) = (keys(world, cat, at), entity.subtype.clone());
        let mut animal = Animal::create(&k, &world.units.class, &subtype, rng);
        animal.restore(&saved);
        if let Some(unit) = world.units.get_mut(id) {
            unit.part = Part::Animal(animal);
            unit.started = true;
        }
    }
}

/// The first turn of an animal that was not on the map at its load (adopted, born): its values
/// start and its counters are drawn.
pub fn start(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    let k = keys(world, ctx.cat, at);
    let subtype = world.zoo.entities[at].subtype.clone();
    *a = Animal::create(&k, &world.units.class, &subtype, ctx.rng);
    (a.fed, a.slept) = (world.units.now, world.units.now);
}

/// The update of the values at a status tick (animals 3.3).
pub fn needs(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let k = keys(world, ctx.cat, at);
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    a.update(&k);
    let line = format!(
        "status {} {} {} {} {}",
        unit.id, a.happiness, a.hunger, a.energy, a.health
    );
    world.units.tell(line);
}

/// The conditional checks of a status tick (animals 4.2).
pub fn checks(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let task = unit.goal.record.task;
    let meal = food::meal(unit, world, data, at);
    let busy = matches!(
        unit.runner.top(),
        Some("bSick" | "bSleep" | "bEat" | "bTestFence")
    ) || matches!(
        task,
        102..=106 | 114 | 115 | 122 | 124 | 128 | 129 | 133 | 134
    );
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    let entity = &world.zoo.entities[at];
    let Some(com) = entity.common.as_ref() else {
        return;
    };
    let key = entity.type_name.clone();
    let young = !matches!(entity.subtype.as_str(), "m" | "f" | "p");
    let (tx, ty_) = (com.x.div_euclid(64), com.y.div_euclid(64));
    let tile = world.tile_index(tx, ty_);
    let region = tile
        .and_then(|t| world.exhibits().region(t))
        .unwrap_or(GROUNDS);
    let x = Some(region).filter(|&r| r != GROUNDS && r != EXTERIOR);
    let k = keys(world, ctx.cat, at);
    let class = world.units.class.clone();
    let mut env = Env {
        exhibit: x.is_some(),
        grounds: x.is_none(),
        young,
        grass: tile.is_some_and(|t| {
            matches!(world.zoo.tiles[t].terrain, 0 | 1 | 4 | 11 | 13)
                && (0..4).all(|q| world.quadrant(t, q).is_none())
        }),
        wet: tile.is_some_and(|t| world.is_water(t)),
        now: world.units.now,
        busy,
        food_task: matches!(task, 114 | 115),
        meal,
        ..Env::default()
    };
    if let Some(x) = x {
        let exhibit = world.exhibits().get(x);
        let near: Vec<u32> = (exhibit.iter())
            .flat_map(|e| std::iter::once(x).chain(e.amphibious.iter().copied()))
            .collect();
        env.wet |= exhibit.is_some_and(|e| e.tank.is_some());
        env.size = (near.iter())
            .filter_map(|&n| world.exhibits().get(n))
            .map(|e| e.tiles)
            .sum();
        env.census = census(world, x, &near, &key);
        let regions = food::near(world, x, &k);
        let pieces = food::pieces(world, ctx.cat, &regions, k.food_kind);
        env.food_total = pieces.iter().map(|p| p.left).fold(0, i32::saturating_add);
        if let Some(r) = record(world, ctx.cat, x, &key) {
            env.species = r.animals as i32;
            env.rating = r.shown() as i32;
            env.other = r.other.round() as i32;
        }
    }
    let mut out = run(a, &k, &class, &env, ctx.rng);
    let meal = std::mem::take(&mut out.meal);
    if let Some(t) = tile.filter(|_| out.trample) {
        let tile = world.zoo.tiles[t];
        let painted =
            world.unrecorded(|w| w.set_tile(tx, ty_, tile.elevation, tile.shape, class.trample));
        // The ground of the exhibit changed: it is rated again.
        if let Some(e) = x
            .filter(|_| painted)
            .and_then(|x| world.exhibits_mut().get_mut(x))
        {
            e.rating.dirty = true;
        }
        world.units.tell(format!("trample {} {tx},{ty_}", unit.id));
    }
    for m in out.messages {
        world.units.post(m, unit.id);
        world.units.tell(format!("message {m} {}", unit.id));
    }
    food::apply(unit, world, ctx, at, data, meal);
}
