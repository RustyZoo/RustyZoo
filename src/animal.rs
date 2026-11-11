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
//! Sleep, health, old age and the dung meter are [`life`]; the dung pile and the keeper's heal are
//! [`dirt`].
//!
//! Mates, births, young animals growing up and eggs are [`breed`]: a male in good spirits with a
//! mate of its species in the exhibit rolls for young at the reproduction check, which puts them
//! (or eggs, for a species that lays them) on free tiles of the exhibit.
//!
//! The animals that live in tanks are [`tank`]: the breath check, the murky water check and the
//! water check, which moves an animal between the land, the water and a tank (and so across the
//! wall to the land exhibit beside its tank, the amphibious neighbours).
//!
//! The shelter check picks a building to rest in; the goals walk the animal there and the building
//! holds it for its time inside (`crate::goal`, `crate::building`).
//!
//! An animal that stands on the zoo grounds when its escape counter fires is escaped; the checks
//! have their escaped cases (eating, the sleep, health and water checks, thirst). How it leaves
//! its exhibit and the set of an escaped animal are [`escape`]. Not here yet: the effects of the
//! rampage and zap checks (M7: only their counters and draws run). The ambient choice is taken at
//! the end of the checks by the behaviour runner ([`Animal::ambient`]).
//!
//! The log family `animal`: `animal status <id> <happiness> <hunger> <energy> <health>` right
//! after each update of the values, `animal message <string> <id>` for a message posted,
//! `animal trample <id> <x>,<y>` for a tile trampled, `animal sleep|wake|ill|infected|dying <id>`
//! for the changes of state, `animal dung <id> <x>,<y> <size>` for a pile put down,
//! `animal shelter <id> <building>` when the check takes a shelter, `animal heal <id>`,
//! `animal species <id> <type>` when the rename cheat changes it,
//! `animal mate <id> <id>`, `animal birth <father> <id> <subtype> <x>,<y>`,
//! `animal egg <father> <id> <x>,<y>`, `animal grow <id> <subtype>`, `animal hatch <id>`,
//! `animal breath <id> <breath>` when the breath rises, `animal terrain <id> <mode>` when the
//! water check changes the terrain mode (0 land, 1 water, 2 tank) and `animal escape <id>` when
//! the animal escapes.

use crate::behaviour::Data;
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating::Record;
use crate::exhibit::{EXTERIOR, GROUNDS, rating};
use crate::research::Overlay;
use crate::sim::Rng;
use crate::single;
use crate::timing::{Date, file_time};
use crate::unit::{Ctx, Part, Unit};
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::{AnimalRecord, Entity};

pub mod boxed;
pub mod breed;
mod check;
mod class;
pub mod dirt;
pub mod escape;
pub mod fence;
pub mod food;
pub mod home;
mod hook;
pub mod life;
pub mod rampage;
pub mod tank;
#[cfg(test)]
mod tests;

use check::{bored, captivity, habitat, other, shelter, social, zap};
pub use class::Class;
pub(crate) use class::Keys;
use food::Meal;
use hook::find;
#[cfg(test)]
pub(crate) use hook::shelters;
#[cfg(test)]
use hook::{census, read_exhibit, trample};
pub(crate) use hook::{checks, exhibit_animals, keys, lengthen, load, needs, start};
use life::Sets;

/// The messages "not happy" and "dislikes its exhibit": string numbers whose text holds the name.
pub(crate) const NOT_HAPPY: i32 = 10028;
/// The message and alert of an animal that got out of its exhibit (animals 11.4).
pub(crate) const ESCAPED: i32 = 10007;
pub(crate) const DISLIKES_EXHIBIT: i32 = 10031;
/// "Can't find any food".
pub(crate) const NO_FOOD: i32 = 10030;
/// "Taken away due to poor treatment", "very sick" and "has died of old age".
pub(crate) const TAKEN_AWAY: i32 = 10010;
pub(crate) const VERY_SICK: i32 = 10029;
pub(crate) const DIED: i32 = 10242;
/// "Is ill" and "has caught an illness from another animal".
pub(crate) const ILL: i32 = 15000;
pub(crate) const CAUGHT_ILL: i32 = 15001;

/// The counters of the checks, in the order the creation draws them (animals 3.2).
pub(crate) const HUNGER: usize = 0;
pub(crate) const BREATH: usize = 1;
pub(crate) const HEALTH: usize = 2;
pub(crate) const ENERGY: usize = 3;
pub(crate) const ESCAPE: usize = 4;
pub(crate) const OTHER: usize = 5;
pub(crate) const KEEPER: usize = 6;
pub(crate) const CAPTIVITY: usize = 7;
pub(crate) const HABITAT: usize = 8;
pub(crate) const BORED: usize = 9;
pub(crate) const SOCIAL: usize = 10;
pub(crate) const REPRODUCTION: usize = 11;
pub(crate) const MESSAGE: usize = 12;
pub(crate) const SHELTER: usize = 13;
pub(crate) const WATER: usize = 14;
pub(crate) const MURKY: usize = 15;
const COUNTERS: usize = 16;

/// The recommendation bits this part owns (animals 8).
/// The bit of "hungry" (9901): no food for it.
const HUNGRY: u32 = 0;
/// The bits of "sick" (9902) and "other animals are sick" (9903).
const SICK: u32 = 1;
const SICK_NEAR: u32 = 2;
const ANGRY: u32 = 3;
const FEW: u32 = 4;
const MANY: u32 = 5;
const STIMULATION: u32 = 6;
const INCOMPATIBLE: u32 = 10;
const CRAMPED: u32 = 16;
const CROWDED: u32 = 56;
const NEEDS_EXHIBIT: u32 = 62;

/// The need a shelter serves, and the radius of the search for one (the square root of the largest
/// whole number: no limit in practice).
pub(crate) const NEED: &str = "animalrest";
/// The index of the need `building` (guests 7.4), which a rampage serves.
pub(crate) const NEED_BUILDING: u8 = 1;
const SEARCH: i64 = 46_340;

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

/// The live state of an animal: the values, their pending changes, the counters, the flags that
/// the checks read and raise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Animal {
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
    /// The breath, 0 to 100 (tank fauna 10): it rises under water and an animal at its
    /// threshold surfaces.
    pub breath: i32,
    /// The counters of the checks, indexed by [`HUNGER`] and the other names.
    pub counters: [i32; COUNTERS],
    pub female: bool,
    /// The game milliseconds of the calendar when it last ate and last slept (animals 13.2).
    pub fed: i64,
    pub slept: i64,
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
    /// In a show as of the start of the status checks and of the ambient choice, for the checks
    /// that read it; elsewhere it is computed where it is read (shows 32.3).
    pub in_show: bool,
    pub very_unhappy: bool,
    pub at_minimum_health: bool,
    /// Raised by a keeper starting its duties in the exhibit, and by the death of an animal.
    pub keeper_visited: bool,
    pub mourning: bool,
    /// Raised by the bored check (9.2); the drink goal lowers it.
    pub thirsty: bool,
    /// Hungry for keeper food (the ladder starts the keeper food goal), eating, and "hungry and
    /// no food": the first hungry check that finds no food only notes it (animals-needs 4.3).
    pub wants_food: bool,
    pub eating: bool,
    pub no_food: bool,
    /// Asleep (the sleep check, animals-needs 6) and drinking at the water (the drink goal).
    pub sleeping: bool,
    pub drinking: bool,
    /// Status ticks left before old age rolls death, and the health checks in a row at health 0
    /// with full hunger (animals-needs 9, 10.1).
    pub life: i32,
    pub starving: i32,
    /// The mate's id (0 for none), the reproduction checks left before a monogamous animal that
    /// lost its mate looks for another, the status ticks since the last birth (ready at
    /// `cReproductionInterval`), the ones left before a young animal grows up, the ones left
    /// before an egg hatches, and the ids of its parents (animals-needs 11, 12, 13).
    pub mate: u32,
    pub mate_delay: i32,
    pub interval: i32,
    pub baby: i32,
    pub hatch: i32,
    pub mother: u32,
    pub father: u32,
    /// The food entity it goes to and eats from; 0 for none.
    pub food: u32,
    /// The rampage flag and its counter (animals 11): they gate the rampage draws and the bored
    /// check; nothing else follows from them before M7.
    pub rampaging: bool,
    pub rampage: i32,
    /// The rampage's building target flag (animals 21.4): one of the "doing something" flags of
    /// 9.3, saved from version 78. The call `fTargetBuilding` raises it ([`rampage`]), the goal 0x7b lowers it.
    pub rampage_target: bool,
    /// It stands loose on the zoo grounds (animals 11.1): set by the escape check on the grounds,
    /// lowered by the escape check in an exhibit and when it is picked up.
    pub escaped: bool,
    /// The keeper's dart put it to sleep: it has no goals and chooses no ambient set until the
    /// sleep check wakes it or it is crated (animals 9.1, 11.6).
    pub tranq: bool,
    /// The id of the staff member who has taken it as his capture job, 0 for none (animals 11.6).
    pub claim: u32,
    /// The escape walk (animals 11.3): its record while the flag is up.
    pub walk: Option<crate::zoo::Escape>,
    /// The crossing animation the escape walk forces (11.3), for the turn to play.
    pub force: Option<&'static str>,
    /// The zap counter (animals 4.1): it starts at 0 and fires when it was below 1.
    pub zap: i32,
    /// The current fence-test chance (animals 11): `None` is the species' `cTestFenceChance`, as at
    /// creation; a test that starts lowers it (11.10). The file does not hold it.
    pub fence_chance: Option<i32>,
    /// The crate counter (`cBoxedCheck`), which runs only while the animal is crated.
    pub crate_count: i32,
    /// Status ticks until the creation runs again (`AnimalResetInterval`).
    pub reset: i32,
    /// The end of the checks asks for the ambient choice (behaviours 6.4): the behaviour runner
    /// takes the request and starts the set its table gives for the happiness.
    pub ambient: bool,
    /// The terrain mode: 0 land, 1 water, 2 tank (tank fauna 5); it decides where the animal looks
    /// for a destination.
    pub terrain: u8,
    /// The shelter it aims at (animals 6): the building's id, 0 for none, and the slot and entrance
    /// it was given by the goals (guest buildings 6.2). The need served is [`Animal::need`]. The
    /// slot is the unit record's byte, [`crate::guest::NO_SLOT`] from creation; a reset keeps it
    /// (goals 11.3.1 point 2).
    pub building: u32,
    pub slot: usize,
    pub entrance: crate::building::Point,
    /// The need being served, as the raw saved byte (shows 32.1): an index of
    /// [`crate::guest::NEED_LIST`], [`crate::guest::NO_NEED`] for none. A toy trick writes `showtrick`,
    /// a shelter choice `animalrest`; a goal reset leaves it, so it outlives the trick.
    pub need: u8,
    /// Playing (it hunts, or plays the friendly chase), caught prey (its hunt got its catch), the
    /// kept prey and the chase stamp (an AI clock value); animals 20. A goal reset lowers the first two and the kept prey.
    pub playing: bool,
    pub caught_prey: bool,
    pub prey: u32,
    pub stamp: u32,
    /// The goal flags of the preattack and of the defence, started and over (animals 20.2, 20.4);
    /// the chase goal and the avoid goal each start them down.
    pub pre: [bool; 2],
}

impl Animal {
    /// The animal as creation leaves it (animals 3.2): the starts, every counter drawn from
    /// `rng` (16 draws, 17 for a young animal whose sex is drawn first).
    pub(crate) fn create(k: &Keys, class: &Class, subtype: &str, rng: &mut Rng) -> Animal {
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
            life: k.time_death.wrapping_sub(1),
            mate_delay: class.mate_pick_delay.wrapping_sub(1),
            interval: k.reproduction_interval,
            baby: k.baby_to_adult.wrapping_sub(1),
            reset: class.reset_interval,
            crate_count: class.boxed.wrapping_sub(1),
            crowd_direction: -1,
            need: crate::guest::NO_NEED,
            slot: crate::guest::NO_SLOT,
            ..Animal::default()
        }
    }

    /// Overwrites what the record of a loaded map stores (animals 3.5); the rest keeps the value
    /// creation gave. The life and the mate delay are clamped to the class's limits (a record
    /// below version 43 has neither: they keep creation's values), the dates are read as game
    /// milliseconds. The sleeping, eating and drinking flags come back as saved; [`load`] starts
    /// the behaviour that matches them.
    pub(crate) fn restore(&mut self, saved: &AnimalRecord, k: &Keys, class: &Class) {
        let r = saved;
        (self.life, self.mate_delay) = (
            r.life
                .map_or(self.life, |life| life.min(k.time_death.wrapping_sub(1))),
            r.mate_delay.map_or(self.mate_delay, |delay| {
                delay.min(class.mate_pick_delay.wrapping_sub(1))
            }),
        );
        (self.baby, self.interval) = (r.baby, r.interval);
        (self.fed, self.slept) = (
            Date::from_file_time(r.dates[0]).game_ms(),
            Date::from_file_time(r.dates[1]).game_ms(),
        );
        (self.happiness, self.hunger, self.health, self.energy) =
            (r.happiness, r.hunger, r.health, r.energy);
        (self.dung, self.female) = (r.dung, r.female);
        for (c, saved) in self.counters.iter_mut().zip(&r.counters) {
            *c = saved.unwrap_or(*c);
        }
        self.breath = r.breath.unwrap_or(self.breath);
        self.at_minimum_health = r.at_minimum_health;
        self.very_unhappy = r.very_unhappy;
        self.keeper_visited = r.keeper_visited;
        self.mourning = r.mourning;
        self.dying = r.dying;
        (self.crated, self.crate_count) = (r.crated, r.boxed);
        self.egg = r.egg;
        self.thirsty = r.thirsty;
        self.rampaging = r.rampaging;
        self.rampage = r.rampage;
        self.rampage_target = r.rampage_target;
        self.escaped = r.escaped;
        (self.playing, self.caught_prey, self.prey) = (r.playing, r.caught_prey, r.prey);
        (self.claim, self.tranq) = (r.claim, r.tranq);
        // The flags are rebuilt from their words (animals 11.3).
        self.walk = r.escape.map(|e| crate::zoo::Escape {
            reached: u32::from(e.reached == 1),
            across: u32::from(e.across == 1),
            broken: u32::from(e.broken != 0),
            ..e
        });
        (self.sleeping, self.eating, self.drinking) = (r.sleeping, r.eating, r.drinking);
    }

    /// The record of this animal: `base` with the live values written over it, so what the engine
    /// does not model stays as it was.
    pub(crate) fn record(&self, base: AnimalRecord) -> AnimalRecord {
        let mut counters = base.counters;
        // The shelter and murky water counters are not stored.
        for at in (0..=12).chain([WATER]) {
            counters[at] = Some(self.counters[at]);
        }
        // A date that did not change keeps its file time, which the game date would round.
        let kept = |live: i64, date: u64| match live == Date::from_file_time(date).game_ms() {
            true => date,
            false => file_time(live),
        };
        AnimalRecord {
            escaped: self.escaped,
            playing: self.playing,
            caught_prey: self.caught_prey,
            prey: self.prey,
            escape: self.walk,
            happiness: self.happiness,
            hunger: self.hunger,
            health: self.health,
            energy: self.energy,
            dung: self.dung,
            female: self.female,
            counters,
            breath: Some(self.breath),
            at_minimum_health: self.at_minimum_health,
            very_unhappy: self.very_unhappy,
            keeper_visited: self.keeper_visited,
            mourning: self.mourning,
            dying: self.dying,
            crated: self.crated,
            boxed: self.crate_count,
            claim: self.claim,
            tranq: self.tranq,
            egg: self.egg,
            thirsty: self.thirsty,
            rampaging: self.rampaging,
            rampage: self.rampage,
            rampage_target: self.rampage_target,
            sleeping: self.sleeping,
            eating: self.eating,
            drinking: self.drinking,
            life: Some(self.life),
            baby: self.baby,
            interval: self.interval,
            mate_delay: Some(self.mate_delay),
            dates: [
                kept(self.fed, base.dates[0]),
                kept(self.slept, base.dates[1]),
            ],
            ..base
        }
    }

    /// Applies the pending changes (animals 3.3).
    pub(crate) fn update(&mut self, k: &Keys) {
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
        let flags = [
            self.thirsty,
            self.in_show,
            self.wants_food,
            self.dying,
            self.drinking,
        ];
        flags.contains(&true) || self.caught_prey || self.rampage_target || env.busy
    }

    /// The need flags of a goal reset (animals 9.2): drinking, hungry for keeper food, thirsty, the
    /// escape walk, playing, the caught and the kept prey.
    pub(crate) fn reset_needs(&mut self) {
        (self.drinking, self.wants_food, self.thirsty, self.walk) = (false, false, false, None);
        (self.playing, self.caught_prey, self.prey) = (false, false, 0);
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
pub(crate) struct Census {
    /// Animals of its species in its exhibit, itself included.
    pub species: u32,
    /// Adults (subtype `m` or `f`) of its species, and of every species, in its exhibit and its
    /// amphibious neighbours.
    pub adults: u32,
    pub all_adults: u32,
    /// The very unhappy animals of its exhibit at its last rating pass, itself included when it
    /// was one then.
    pub unhappy: u32,
}

/// Where the animal stands and what the checks read of it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Env {
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
    /// The animal cannot act here (animals-needs 2): an egg, in a show, or a species that does not live
    /// only under water in water or a tank.
    pub cannot_act: bool,
    /// The game milliseconds of the calendar, for the date of its last meal.
    pub now: i64,
    /// Its set, its task or a caught state occupies it (9.3).
    pub busy: bool,
    /// What the bored check reads of the units round it (animals 21.1).
    pub sight: Sight,
    /// Its task is the keeper food or the other food task.
    pub food_task: bool,
    /// The food of its species' kind in its exhibit, in units (hunger check 4.1).
    pub food_total: i32,
    /// The dung pile of the hunger check was put down.
    pub piled: bool,
    /// What eating reads of the food it aims at and of its type.
    pub meal: Meal,
    /// What the sleep, health and death checks read of its sets.
    pub sets: Sets,
    /// The other sick animals of its exhibit and its amphibious neighbours (animals-needs 9).
    pub sick_near: u32,
    /// A keeper is at work in its exhibit.
    pub keeper_working: bool,
    /// The breeding cheat is up and the species is one it works for (cheats 4.5).
    pub boost: bool,
    /// Its subtype is `m` or `f` (an adult, animals-needs 11.2).
    pub adult: bool,
    /// Its subtype is `y` (it grows up, 12).
    pub baby: bool,
    /// Its subtype is `m` (it gives birth, 11.2).
    pub male: bool,
    /// It stands in a tank that holds a show.
    pub show_tank: bool,
    /// The tank mode of the unit (tank fauna 4) and the tank it stands in.
    pub tank_mode: u8,
    pub tank: Option<crate::exhibit::Tank>,
    /// The land, water and tank tiles of its exhibit and its amphibious neighbours (indexed by
    /// the terrain mode), counted for the water check when its counter is due, and the kind of
    /// tile it stands on.
    pub kinds: [usize; 3],
    pub stands: u8,
    /// What the shelter check reads of the buildings (animals 6), listed only when its counter is
    /// due.
    pub shelter: Shelter,
    /// Where its mate is, when it has one.
    pub mate: breed::Mate,
    /// The animals it could pick as a mate, in the exhibit's order (animals-needs 11.3); only
    /// listed for an animal whose reproduction check is due.
    pub candidates: Vec<u32>,
}

/// What the bored check reads of the units round the animal, found when its counter is due: the
/// afraid flag and the task now, a threat, a playmate that chases it, prey for a playmate of its
/// own and whether it is in a friendly chase (animals 21.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Sight {
    pub afraid: bool,
    pub task: u8,
    pub threat: bool,
    pub chaser: bool,
    pub prey: bool,
    pub friendly: bool,
}

/// The shelters around an animal when its shelter check is due: whether the building it aims at
/// still serves the need `animalrest`, and the nearest buildings that do, in the list order
/// (guests 7.1); the draw among them is the check's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Shelter {
    pub held: bool,
    pub nearest: Vec<u32>,
}

/// What a tick of checks asks of the world besides the animal's own state.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Out {
    /// The messages posted, by string number.
    pub messages: Vec<i32>,
    /// The tile under the animal is painted as trampled ground.
    pub trample: bool,
    /// What the eating of this tick asks of the world.
    pub meal: Meal,
    /// The sets to start, in order.
    pub sets: Vec<&'static str>,
    /// A dung pile is due (the meter was at its threshold).
    pub dung: bool,
    /// The bored check saw a threat or a playmate that chases it: the afraid flag goes up.
    pub afraid: bool,
    /// The bored check ended a lonely friendly chase: the unit's goal resets (animals 21.1 point 7).
    pub lonely: bool,
    /// The animal is taken away or removed without a death set, and the animals of its exhibit
    /// mourn.
    pub remove: bool,
    pub mourn: bool,
    /// The changes of state for the log, one word each.
    pub log: Vec<&'static str>,
    /// It took the animal with this id as its mate; it lost the mate with this id, whose own id
    /// of it goes too (animals-needs 11.2, 11.3).
    pub pair: Option<u32>,
    pub unpair: Option<u32>,
    /// The breeding cheat was used by the roll: the flag goes down.
    pub boost_used: bool,
    /// A young animal grows up.
    pub grow: bool,
    /// The water check asks for this terrain mode, and the draw that picks the tile it sends the
    /// animal to (tank fauna 5).
    pub terrain: Option<(u8, Option<usize>)>,
    /// The shelter it took as its target.
    pub shelter: Option<u32>,
    /// A rampage ended: a running rampage set is cleared (animals 11).
    pub calm: bool,
    /// The draw of the zap check succeeded: the fence test starts (animals 11.10).
    pub fence: bool,
    /// The escape check fired with the animal on the zoo grounds: the zoo status dates the
    /// escape (animals 11.1).
    pub loose: bool,
    /// The escape check fired: the predator search follows (animals 11.7).
    pub fired: bool,
    /// The draw that picks the land tile an escaped animal on water is sent to (animals 11.5).
    pub sent: Option<usize>,
}

/// The checks of one status tick, in the original's order (animals 4.2); `rng` is the shared
/// generator, drawn from in that order. No young animal is ever born.
#[cfg(test)]
pub(crate) fn run(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng) -> Out {
    run_with(a, k, class, env, rng, &mut |_, _| false)
}

/// The same, with `birth` making the young when the reproduction check succeeds, among the
/// draws (animals-needs 11.4).
#[cfg(test)]
pub(crate) fn run_with(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
    birth: breed::Birth,
) -> Out {
    run_read(
        a,
        k,
        class,
        &mut env.clone(),
        rng,
        birth,
        &mut |_, _, _, _, _| {},
    )
}

/// What a check reads of its exhibit, which is recomputed first when it is dirty (habitat rating
/// 4, "Reading"): the check asks for it where it reads, so the recomputation and its draws happen
/// at that point of the tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Read {
    /// The stored animal list, for the mate search (animals-needs 11.3).
    Mates,
    /// The food sums of the exhibit and its amphibious neighbours (hunger check).
    Food,
    /// The keepers at work in the exhibit (health check).
    Keeper,
    /// The sick animals of the exhibit and its neighbours (health check).
    Sick,
    /// The species record, the animal counts and the rating (social check).
    Exhibit,
    /// The animal list, read by the bite that emptied the pile (animals-needs 5.1).
    Emptied,
    /// Not a read: the hunger check puts a dung pile down (animals-needs 8.1); `Env::piled` says
    /// whether it is done.
    Pile,
    /// Not a read: the captivity tick paints the ground under the animal as trampled (animals 4.5).
    Trample,
}

/// Fills the part of the [`Env`] that `Read` names from the world, given the animal, the meal
/// of the tick so far and the generator.
pub(crate) type Reader<'a> = &'a mut dyn FnMut(Read, &Animal, &Meal, &mut Env, &mut Rng);

#[cfg(test)]
/// [`run_with`] with the reads of the exhibit made by `read` where the checks need them.
pub(crate) fn run_read(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &mut Env,
    rng: &mut Rng,
    birth: breed::Birth,
    read: Reader,
) -> Out {
    let (mut out, go) = run_head(a, k, class, env, rng, birth, read);
    if go {
        run_tail(a, k, class, env, rng, &mut out, read);
    }
    out
}

/// The checks of a tick up to and with the escape check (animals 4.2); the flag says the tick goes
/// on with [`run_tail`], after the predator dispatch that the escape check asks for.
pub(crate) fn run_head(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &mut Env,
    rng: &mut Rng,
    birth: breed::Birth,
    read: Reader,
) -> (Out, bool) {
    let mut out = Out {
        meal: env.meal.clone(),
        ..Out::default()
    };
    breed::egg_check(a, k, env, &mut out);
    if a.crated || a.egg {
        if a.egg && !a.crated {
            life::health(a, k, class, env, rng, &mut out, read);
        }
        a.exhibit_bit(a.crated || env.grounds);
        return (out, false);
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
            // The zap counter keeps running down across the reset, and the mate and the parents
            // are not status values.
            let (zap, mate, mother, father) = (a.zap, a.mate, a.mother, a.father);
            *a = Animal::create(k, class, subtype, rng);
            (a.zap, a.mate, a.mother, a.father) = (zap, mate, mother, father);
            (a.fed, a.slept) = (env.now, env.now);
        }
    }
    breed::grow(a, k, env, &mut out);
    breed::reproduction(a, k, class, env, rng, &mut out, (birth, &mut *read));
    if a.dying {
        // Removed unless its death set still runs.
        out.remove |= !(env.sets.dying_now || out.sets.contains(&"bDie"));
        return (out, false);
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
    escape::check(a, k, class, env, rng, &mut out);
    (out, true)
}

/// The checks after the escape check and its dispatch (animals 4.2), the bored check among them
/// with `env.sight` read after the dispatch (21.1 point 10).
pub(crate) fn run_tail(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &mut Env,
    rng: &mut Rng,
    out: &mut Out,
    read: Reader,
) {
    if a.eating {
        food::bite(a, k, env, rng, out, read);
    }
    food::hunger(a, k, class, env, rng, out, read);
    tank::breath(a, k, class, env);
    tank::murky(a, k, class, env);
    life::health(a, k, class, env, rng, out, read);
    life::sleep(a, k, class, env, out);
    captivity(a, k, class, env, rng, out, read);
    social(a, k, class, env, rng, &out.meal, read);
    fire(&mut a.counters[KEEPER], class.periods[KEEPER]);
    habitat(a, k, class, env, &mut out.messages);
    bored(a, k, class, env, rng, out);
    out.terrain = tank::water(a, k, class, env, rng);
    shelter(a, k, class, env, rng, out);
    other(a, k, class, env);
    zap(a, k, class, env, rng, out);
    if fire(&mut a.counters[MESSAGE], class.periods[MESSAGE]) && a.happiness < -60 {
        out.messages.push(NOT_HAPPY);
    }
    a.ambient = true;
    match env.grounds {
        true => a.exhibit_bit(true),
        false => a.unmark(NEEDS_EXHIBIT),
    }
}
