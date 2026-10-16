//! The simulation step of the original (simulation 3 and 4, main loop 6): six managers in a fixed
//! order, the clocks they keep, the removal pass, the loop over the entities and the event queue,
//! and the game's one random generator.
//!
//! What a unit does with its step is the [`unit`] module's: [`Sim::step`] runs [`unit::update`]
//! for each entity in list order, then hands the entity to the caller's `update` (for the kinds
//! of entity that have no live state yet). The zoo status checks of the game manager are the
//! [`crate::status::zoo`] module's. Missing so far: the new guest check,
//! the task pool, the shows, the water ripples, the ambient creatures, and the scenario, research and
//! marketing managers, which are empty until M7.
//!
//! [`hash::state`] is the hash of the whole state that the `sim` command prints, and [`drive`]
//! the commands such a run applies at a step.
//!
//! The log family `sim`: `sim seed N` at each seeding of the generator in a test run, `sim step MS` for each
//! step, `sim month M Y` when a step acts on the month flag of the one before, and the zoo
//! status' `sim rating N`, `sim message ID` and `sim donation AMOUNT`.

pub mod drive;
mod events;
pub mod hash;
mod rng;
#[cfg(test)]
mod tests;

pub use events::{Event, Events, KEEPER_CHANNEL, UNIT_CHANNEL};
pub use rng::{Rng, Seconds};

use crate::behaviour::{Data, NoData};
use crate::config::{self, Config};
use crate::entity::Catalogue;
use crate::exhibit::rating;
use crate::money::Money;
use crate::status::zoo::{Say, Settings, Status, Sums};
use crate::timing::{Calendar, ini_value, ini_whole};
use crate::unit;
use crate::world::World;
use crate::zoo::Clocks;

/// The `[AI]` keys of the ini file that change the game, with the original's fallbacks
/// (simulation 9.1). Those that belong to the path finder, the habitat manager or the goals are
/// read here once and used by them.
#[derive(Debug, Clone, PartialEq)]
pub struct Ai {
    /// Milliseconds of an AI tick; the original crashes on 0, so 0 keeps 1000. A negative value
    /// reads as the huge unsigned number the original divides by.
    pub ai_tick: u32,
    pub status_period: i32,
    pub clear_unreachable_freq: i32,
    pub min_unreachable_count: u8,
    pub variable_unit_speeds: i32,
    pub allow_diagonal_moves: bool,
    pub animal_reset_interval: i32,
    pub guest_reset_interval: i32,
    pub animated_water: bool,
    pub max_guests: i32,
    pub escaped_animal_change: i32,
    pub escaped_animal_time: i32,
    pub exhibit_service_interval: i32,
    pub use_attraction_attractiveness: i32,
    pub map_size: i32,
    pub min_cost: i32,
    pub node_max: i32,
    pub cost_max: i32,
    pub vector_increment: i32,
    pub heuristic: f32,
    pub estimate_func: i32,
    pub popularity_scale_factor: f32,
    pub max_dist_to_go_to_show: i32,
}

impl Ai {
    /// The keys of the `[AI]` section; a missing or unreadable one keeps its fallback.
    pub fn from_ini(ini: &[u8]) -> Ai {
        let config = Config::parse(ini).ok();
        let whole = |key: &str, fallback: i32| {
            config
                .as_ref()
                .and_then(|c| ini_whole(c, "AI", key))
                .unwrap_or(fallback)
        };
        let decimal = |key: &str, fallback: f32| {
            config
                .as_ref()
                .and_then(|c| ini_value(c, "AI", key))
                .map_or(fallback, config::to_float)
        };
        Ai {
            ai_tick: match whole("mAITick", 1000) {
                0 => 1000,
                n => n as u32,
            },
            status_period: whole("mStatusPeriod", 1000),
            clear_unreachable_freq: whole("ClearUnreachableFreq", 5),
            min_unreachable_count: whole("MinUnreachableCount", 5) as u8,
            variable_unit_speeds: whole("VariableUnitSpeeds", 1),
            allow_diagonal_moves: whole("AllowDiagonalMoves", 1) == 1,
            animal_reset_interval: whole("AnimalResetInterval", 0).max(0),
            guest_reset_interval: whole("GuestResetInterval", 0).max(0),
            animated_water: whole("animatedwater", 1) != 0,
            max_guests: whole("maxGuests", 1000),
            escaped_animal_change: whole("cEscapedAnimalChange", 50),
            escaped_animal_time: whole("cEscapedAnimalTime", 2),
            exhibit_service_interval: whole("exhibitServiceInterval", 90),
            use_attraction_attractiveness: whole("useAttractionAttractiveness", 0),
            map_size: whole("MapSize", 20),
            min_cost: whole("MinCost", 2),
            node_max: whole("NodeMax", 500),
            cost_max: whole("CostMax", 300),
            vector_increment: whole("VectorIncrement", 20),
            heuristic: decimal("Heuristic", 1.0),
            estimate_func: whole("EstimateFunc", 1),
            popularity_scale_factor: decimal("popularityScaleFactor", 15.0),
            max_dist_to_go_to_show: whole("maxDistToGoToShow", 150),
        }
    }
}

/// What a step did that the caller acts on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Report {
    /// The previous step changed the month: this step runs the month-end work (the finance
    /// check), after the generator was seeded again.
    pub month_end: bool,
    /// The AI clock passed the time to empty the list of unreachable places.
    pub clear_unreachable: bool,
    /// The water of a tank moved.
    pub water_moved: bool,
    /// The bars of the status bar are to be written: animal happiness, guest happiness, rating.
    pub bars: Option<[i32; 3]>,
}

/// The managers' state: the clocks, the generator and the event queue of one program run.
#[derive(Debug)]
pub struct Sim {
    pub ai: Ai,
    pub rng: Rng,
    pub events: Events,
    /// The zoo status of the game manager, and the economy file it starts from.
    pub status: Status,
    economy: Option<Config>,
    messages: Vec<Say>,
    seconds: Seconds,
    /// The tile count of the last map set up, which decides whether a new one seeds again.
    tiles: usize,
    calendar: Calendar,
    /// Simulated time since the zoo began, in milliseconds.
    total: u32,
    /// The AI clock: the milliseconds left over and the whole AI ticks.
    ai_rem: u32,
    ai_ticks: u32,
    /// The AI tick at which the unreachable list is next emptied; a value of the session, not
    /// of the zoo (simulation 4.3).
    next_clear: u32,
    /// The world clock: whole seconds and the milliseconds left over.
    world_secs: u32,
    world_rem: u32,
    /// The entities that asked to be removed, by id.
    removals: Vec<u32>,
    lines: Vec<String>,
    /// The sex of the staff the Hire panel lists: drawn at the start of the program, after the
    /// first seeding, and after every hire (unit placing 4.3).
    pub hire_male: bool,
}

impl Sim {
    /// The managers at the start of the program: the `[AI]` keys read once, the generator
    /// seeded from the wall clock (the first seeding of main loop 9.2).
    pub fn new(ini: &[u8]) -> Sim {
        let mut sim = Sim {
            ai: Ai::from_ini(ini),
            rng: Rng::default(),
            events: Events::default(),
            status: Status::default(),
            economy: None,
            messages: Vec::new(),
            seconds: Seconds::Clock,
            tiles: 0,
            calendar: Calendar::new(),
            total: 0,
            ai_rem: 0,
            ai_ticks: 0,
            next_clear: 0,
            world_secs: 0,
            world_rem: 0,
            removals: Vec::new(),
            lines: Vec::new(),
            hire_male: false,
        };
        sim.reseed();
        sim.draw_hire();
        sim
    }

    /// Seeds the generator again from the seconds source.
    fn reseed(&mut self) {
        let seconds = self.seconds.now();
        self.rng.seed(seconds);
        // The log tells the seconds, so that a replay seeds the same.
        self.lines.push(format!("seed {seconds}"));
    }

    /// Makes a replay: the seconds of the seedings are `seeds` in order (those a log told), and
    /// the seedings that already happened are made again with them.
    pub fn set_seeds(&mut self, seeds: Vec<u32>) {
        self.restart(Seconds::Replay(seeds.into()));
    }

    /// Makes a test run: the seconds are `seed + n` from now on, and the seedings that already
    /// happened (the start of the program, and the map when a game is open) are made again with
    /// them, so that the order is the same as when the seed is known from the start.
    pub fn set_seed(&mut self, seed: u32) {
        self.restart(Seconds::Test { seed, n: 0 });
    }

    fn restart(&mut self, seconds: Seconds) {
        self.seconds = seconds;
        self.lines.retain(|l| !l.starts_with("seed "));
        self.reseed();
        self.draw_hire();
        if self.tiles != 0 {
            self.reseed();
        }
    }

    /// Draws the sex of the next hire from the generator: bit 16 of the state set is male (unit
    /// placing 4.3), the lowest bit of the number.
    pub fn draw_hire(&mut self) {
        self.hire_male = self.rng.rand() & 1 == 1;
    }

    /// A new game on a map of `tiles` tiles that stores these clocks: the total time and the
    /// calendar start again, the AI and world clocks start from the map's (simulation 4.3, 8.3),
    /// the queue is empty, and the generator is seeded again if the map has another number of
    /// tiles than the last one (main loop 9.2, the second moment).
    pub fn start_game(&mut self, tiles: usize, clocks: Clocks) {
        self.status = Status::start(&clocks.status, self.economy.as_ref());
        self.calendar = Calendar::new();
        self.total = 0;
        (self.ai_rem, self.ai_ticks) = (clocks.ai_rem, clocks.ai_ticks);
        (self.world_rem, self.world_secs) = (clocks.world_rem, clocks.world_secs);
        self.removals.clear();
        self.events.restore(&clocks.queue);
        if tiles != self.tiles {
            self.tiles = tiles;
            self.reseed();
        }
    }

    /// Gives the economy file that every later game's zoo status starts from.
    pub fn set_economy(&mut self, file: &[u8]) {
        self.economy = Config::parse(file).ok();
    }

    /// A zoo has just been loaded (Continue, Load, the command line, or the file of a new game
    /// before its set-up): the load ends with a rating check with the stored cash and the entities
    /// as loaded, research at 0 points, which restarts the rating timer (zoo status 16).
    pub fn loaded(&mut self, world: &World, cat: &Catalogue, money: &mut Money) {
        let sums = Sums::of(world, cat, money.cash, 0);
        self.status.rating_check(sums, money, &mut self.lines);
    }

    /// A new game (not a saved one) has just been set up: the timers and the donation counter
    /// start again and the rating check of the set-up runs, with no cash, the built-in settings and
    /// every research program counted as done (zoo status 6.4).
    pub fn new_game(&mut self, world: &World, cat: &Catalogue, money: &mut Money) {
        self.status.timers = [0; 3];
        self.status.given = 0.0;
        let set = std::mem::replace(&mut self.status.settings, Settings::builtin());
        let sums = Sums::of(world, cat, 0.0, 100);
        self.status.rating_check(sums, money, &mut self.lines);
        self.status.settings = set;
    }

    /// Creates the animals of the map just loaded (animals 3.5), with the types of `cat` and the
    /// class keys in `world.units.class`. The draws come before every draw of the first step.
    pub fn load(&mut self, world: &mut World, cat: &Catalogue) {
        crate::animal::load(world, cat, &mut self.rng);
    }

    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub fn total_ms(&self) -> u32 {
        self.total
    }

    /// The AI clock: whole AI ticks and the milliseconds left over.
    pub fn ai_clock(&self) -> (u32, u32) {
        (self.ai_ticks, self.ai_rem)
    }

    /// The world clock: whole seconds and the milliseconds left over.
    pub fn world_clock(&self) -> (u32, u32) {
        (self.world_secs, self.world_rem)
    }

    /// Asks for an entity to be removed at the next step's removal pass.
    pub fn remove(&mut self, id: u32) {
        self.removals.push(id);
    }

    /// The advice and donation messages since the last call, as text numbers and the amount of a
    /// donation.
    pub fn take_messages(&mut self) -> Vec<Say> {
        std::mem::take(&mut self.messages)
    }

    /// The `sim` log lines since the last call, without the family word.
    pub fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// One step of `step` milliseconds: the managers in the original's order. `update` is called
    /// for each entity of the list, by position, in list order, with the step, after the unit's
    /// own turn ([`unit::update`]). This is the AI manager's loop: the count is taken after the
    /// removal pass, so an entity added by `update` waits for the next step.
    pub fn step(
        &mut self,
        world: &mut World,
        step: u32,
        update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_in(world, &Catalogue::default(), step, update)
    }

    /// [`Sim::step`] in a game whose types are `cat`, with accounts nobody reads: the units read
    /// their species' keys from it and the exhibits are rated for the species they hold.
    pub fn step_in(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        step: u32,
        update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_with(&NoData, cat, world, step, update)
    }

    /// As [`Sim::step_in`], with the data the units' sets and animations read.
    pub fn step_with(
        &mut self,
        data: &dyn Data,
        cat: &Catalogue,
        world: &mut World,
        step: u32,
        update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_game(data, cat, world, &mut Money::default(), step, update)
    }

    /// [`Sim::step_with`] in a game whose cash and accounts are `money`: the zoo status checks
    /// read and book into them.
    pub fn step_game(
        &mut self,
        data: &dyn Data,
        cat: &Catalogue,
        world: &mut World,
        money: &mut Money,
        step: u32,
        mut update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.lines.push(format!("step {step}"));
        // The game manager: the time, the calendar, whose month flag of the step before is acted
        // on here, and the zoo status (zoo status 4).
        self.total = self.total.wrapping_add(step);
        let month_end = self.calendar.advance(step);
        let bars = self.zoo_status(world, cat, money, step, month_end);
        // The AI manager: the clock in whole ticks with the remainder kept, the removal pass,
        // the entities, the event queue.
        let total = self.ai_rem.wrapping_add(step);
        self.ai_rem = total % self.ai.ai_tick;
        self.ai_ticks = self.ai_ticks.wrapping_add(total / self.ai.ai_tick);
        let clear_unreachable = self.ai_ticks >= self.next_clear;
        if clear_unreachable {
            self.next_clear = self
                .ai_ticks
                .wrapping_add(self.ai.clear_unreachable_freq as u32);
        }
        for id in std::mem::take(&mut self.removals) {
            world.unrecorded(|w| w.remove(id));
        }
        let count = world.zoo.entities.len();
        for at in 0..count {
            // Read again at each index, as the original does.
            if at < world.zoo.entities.len() {
                let mut ctx = unit::Ctx {
                    rng: &mut self.rng,
                    events: &self.events,
                    removals: &mut self.removals,
                    step,
                    cat,
                };
                unit::update(&mut unit::Standard { data }, world, &mut ctx, at);
                update(world, at, step);
            }
        }
        self.events.age();
        // The world manager: the zoo grounds' timers, then the exhibits' timers and ratings, the
        // water of the tanks, the world clock.
        let mut grounds = std::mem::take(&mut world.exhibits_mut().grounds);
        grounds.update(step, &mut || self.rng.rand(), rating::Input::default);
        world.exhibits_mut().grounds = grounds;
        for at in 0..world.exhibits().list().len() {
            let id = world.exhibits().list()[at].id;
            let mut state = std::mem::take(&mut world.exhibits_mut().list_mut()[at].rating);
            let rand = &mut || self.rng.rand();
            state.update(step, rand, || rating::input(world, cat, id));
            world.exhibits_mut().list_mut()[at].rating = state;
        }
        let water_moved = world.step_water();
        let total = self.world_rem.wrapping_add(step);
        self.world_rem = total % 1000;
        self.world_secs = self.world_secs.wrapping_add(total / 1000);
        // The scenario, research and marketing managers have nothing to do before M7.
        Report {
            month_end,
            clear_unreachable,
            water_moved,
            bars,
        }
    }

    /// The zoo status of a step: the due checks in the original's order, the donation and the
    /// bars. The generator is seeded again inside the finance check, after the other checks.
    fn zoo_status(
        &mut self,
        world: &World,
        cat: &Catalogue,
        money: &mut Money,
        step: u32,
        month_end: bool,
    ) -> Option<[i32; 3]> {
        let due = self.status.due(step, month_end);
        if due.rating {
            let sums = Sums::of(world, cat, money.cash, 0);
            self.status.rating_check(sums, money, &mut self.lines);
        }
        if due.message {
            let (say, log) = (&mut self.messages, &mut self.lines);
            self.status.message_check(money.cash, say, log);
        }
        if due.new_guest {
            // The new guest check creates the guests (guest arrival); its timer restarts all the same.
            self.status.timers[2] = 0;
        }
        if month_end {
            self.reseed();
            let date = self.calendar.date();
            self.lines
                .push(format!("month {} {}", date.month, date.year));
            // The endowment goes before the accounts move on, the wages and the upkeep after.
            money.month_change();
        }
        let (say, log) = (&mut self.messages, &mut self.lines);
        self.status.donation(&mut self.rng, money, say, log);
        self.status.bars(step)
    }
}
