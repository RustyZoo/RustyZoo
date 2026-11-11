//! The simulation step of the original (simulation 3 and 4, main loop 6): six managers in a fixed
//! order, the clocks they keep, the removal pass, the loop over the entities and the event queue,
//! and the game's one random generator.
//!
//! What a unit does with its step is the [`unit`] module's: [`Sim::step`] runs [`unit::update`]
//! for each entity in list order, then hands the entity to the caller's `update` (for the kinds
//! of entity that have no live state yet). The zoo status checks of the game manager are the
//! [`crate::status::zoo`] module's, and the guests it lets in the [`crate::guest`] module's. The
//! shows are [`crate::show`]'s, the spawn list of the ambient creatures [`crate::flight`]'s
//! and the scenario manager [`crate::scenario`]'s.
//!
//! [`hash::state`] is the hash of the whole state that the `sim` command prints, and [`drive`]
//! the commands such a run applies at a step.
//!
//! The log family `sim`: `sim seed N` at each seeding of the generator in a test run, `sim step MS` for each
//! step, `sim scenario ...` ([`crate::scenario`]), `sim month M Y` when a step acts on the month
//! flag of the one before, and the zoo status' `sim fly ID [COUNT]` for a flying object a date hook launches ([`crate::flight`]; `sim flight ID missing` when its type is absent), `sim rating N`, `sim message ID` and `sim donation AMOUNT`, `sim rate ID [KEY SHOWN]...`
//! when exhibit `ID` is rated at its update (habitat rating 4), with the suitability its record of
//! each species gives, and at a month end the
//! `sim wages ID AMOUNT` of each staff member and `sim upkeep AMOUNT` of the buildings ([`crate::month`]).

pub mod drive;
#[cfg(test)]
mod escape_tests;
mod events;
pub mod hash;
pub(crate) mod idle;
mod inspect;
mod load;
mod programs;
mod random_use;
mod ripple;
mod rng;
mod sparkle;
mod status;
mod step;
#[cfg(test)]
mod tests;
mod wear;
#[cfg(test)]
mod wear_tests;

pub(crate) use events::{
    BUILDING_CHANNEL, Event, Events, KEEPER_CHANNEL, SHOW_CHANNEL, TANK_CHANNEL, TASK_CHANNEL,
    UNIT_CHANNEL, WORLD_CHANNEL,
};
pub(crate) use rng::{Rng, Seconds};
pub(crate) use step::Step;

use crate::behaviour::Data;
#[cfg(test)]
use crate::behaviour::NoData;
use crate::config::{self, Config};
use crate::entity::Catalogue;
use crate::exhibit::rating;
use crate::guest;
use crate::marketing::Marketing;
use crate::money::{self, Money};
use crate::research::{Done, Tree};
use crate::scenario::{Env, Game, Scenario};
use crate::soundscape::{Calls, SceneLoop, Shot, Soundscape};
use crate::status::zoo::{Say, Status};
use crate::timing::{Calendar, ini_value, ini_whole};
use crate::unit;
use crate::world::World;
use crate::zoo::Clocks;

/// The `[AI]` keys of the ini file that change the game, with the original's fallbacks
/// (simulation 9.1). Those that belong to the path finder, the habitat manager or the goals are
/// read here once and used by them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ai {
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
    pub(crate) fn from_ini(ini: &[u8]) -> Ai {
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
pub(crate) struct Report {
    /// The previous step changed the month: this step runs the month-end work (the finance
    /// check), after the generator was seeded again.
    pub month_end: bool,
    /// The AI clock passed the time to empty the list of unreachable places.
    pub clear_unreachable: bool,
    /// The water of a tank moved.
    pub water_moved: bool,
    /// The bars of the status bar are to be written: animal happiness, guest happiness, rating.
    pub bars: Option<[i32; 3]>,
    /// A guest was created this step: the date hooks run (cheats 6).
    pub arrived: bool,
}

/// The managers' state: the clocks, the generator and the event queue of one program run.
#[derive(Debug)]
pub(crate) struct Sim {
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
    /// The play time of the file plus the steps since the load, in simulated milliseconds.
    play_time: u32,
    /// The AI clock: the milliseconds left over and the whole AI ticks.
    ai_rem: u32,
    ai_ticks: u32,
    /// The AI clock held before the file of the game just started set its own: 0 in a fresh
    /// process, the previous game's clock when a zoo is loaded inside a running game. A stored 0
    /// in an entity's base word takes this (zoo format 6.4).
    held_ticks: u32,
    /// The AI tick at which the unreachable list is next emptied; a value of the session, not
    /// of the zoo (simulation 4.3).
    next_clear: u32,
    /// The world clock: whole seconds and the milliseconds left over.
    world_secs: u32,
    world_rem: u32,
    /// The spawn list of the world (ambients 7).
    pub(crate) spawns: crate::flight::Spawns,
    /// The slots of the animated water (7.2).
    ripples: ripple::Ripples,
    /// The idle animation clocks of the fixed objects (sprite drawing 6.2).
    idles: idle::Idles,
    /// The entities that asked to be removed, by id.
    removals: Vec<u32>,
    /// The entities the step removes as the bulldozer does, with an undo record (a flooded or
    /// doubled tank filter, tank staff 7.3), by id.
    bulldozed: Vec<u32>,
    lines: Vec<String>,
    /// The scenario being played, if the game is one.
    scenario: Option<Scenario>,
    /// The milliseconds the manager has gathered towards its next pass: one value for the program,
    /// so a game left between two passes brings its share to the next scenario (scenarios 5.2).
    pass_ms: u32,
    /// The research tree of a scenario or freeform game, or of a saved game that names a
    /// scenario; none otherwise, which counts as no research done.
    research: Option<Tree>,
    /// The programs the research step completed, for the caller to apply.
    research_done: Vec<Done>,
    /// The marketing of the game: it outlives a game, only a scenario set-up rebuilds it
    /// (marketing 3.3).
    marketing: Marketing,
    arrived: bool,
    /// What the step in progress did, filled in by the managers.
    report: Report,
    /// The ambient sounds of the game being played, if it has them (sound 8.1).
    soundscape: Option<Soundscape>,
    /// The calls of the ambient creatures: the groups outlive a game (ambients 6.4 point 4).
    calls: Calls,
    /// The computer's (month, day) in UTC; a test sets another (cheats 6.1 point 2).
    pub(crate) real_day: fn() -> (u32, u32),
    /// The sex of the staff the Hire panel lists: drawn at the start of the program, after the
    /// first seeding, and after every hire (unit placing 4.3).
    pub hire_male: bool,
}

impl Sim {
    /// The managers at the start of the program: the `[AI]` keys read once, the generator
    /// seeded from the wall clock (the first seeding of main loop 9.2).
    pub(crate) fn new(ini: &[u8]) -> Sim {
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
            play_time: 0,
            ai_rem: 0,
            ai_ticks: 0,
            held_ticks: 0,
            next_clear: 0,
            world_secs: 0,
            world_rem: 0,
            spawns: Default::default(),
            ripples: ripple::Ripples::default(),
            idles: idle::Idles::default(),
            removals: Vec::new(),
            bulldozed: Vec::new(),
            lines: Vec::new(),
            scenario: None,
            pass_ms: 0,
            research: None,
            research_done: Vec::new(),
            marketing: Marketing::default(),
            arrived: false,
            report: Report::default(),
            soundscape: None,
            calls: Calls::default(),
            real_day: crate::timing::utc_day,
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
    /// the seedings that already happened are made again with them (the lines they gave are
    /// dropped).
    pub(crate) fn set_seeds(&mut self, seeds: Vec<u32>) {
        self.restart(Seconds::Replay(seeds.into()));
    }

    /// Makes a test run: the seconds are `seed + n` from now on, and the seedings that already
    /// happened (the start of the program, and the map when a game is open) are made again with
    /// them, so that the order is the same as when the seed is known from the start.
    pub(crate) fn set_seed(&mut self, seed: u32) {
        self.restart(Seconds::Test { seed, n: 0 });
    }

    fn restart(&mut self, seconds: Seconds) {
        self.seconds = seconds;
        // What the lines told so far came from the earlier seeding: the load that follows tells it
        // again.
        self.lines.clear();
        self.reseed();
        self.draw_hire();
        if self.tiles != 0 {
            self.reseed();
        }
    }

    /// Draws the sex of the next hire from the generator: bit 16 of the state set is male (unit
    /// placing 4.3), the lowest bit of the number.
    pub(crate) fn draw_hire(&mut self) {
        self.hire_male = self.rng.rand() & 1 == 1;
    }

    /// A game on a map of `tiles` tiles that stores these clocks: the total time starts again, the
    /// calendar starts from the stored date (a new game restarts it, `restart_calendar`), the AI
    /// and world clocks start from the map's (simulation 4.3, 8.3), the queue is empty,
    /// and the generator is seeded again if the map has another number of tiles than the last one
    /// (main loop 9.2, the second moment).
    pub(crate) fn start_game(&mut self, tiles: usize, clocks: Clocks) {
        self.status = Status::start(&clocks.status, None);
        let s = &mut self.status.settings;
        (s.escaped_change, s.escaped_time) =
            (self.ai.escaped_animal_change, self.ai.escaped_animal_time);
        if let Some(file) = &self.economy {
            self.status.economy(file);
        }
        self.calendar = Calendar::at(clocks.calendar);
        self.total = 0;
        self.play_time = clocks.play_time;
        self.held_ticks = self.ai_ticks;
        (self.ai_rem, self.ai_ticks) = (clocks.ai_rem, clocks.ai_ticks);
        (self.world_rem, self.world_secs) = (clocks.world_rem, clocks.world_secs);
        self.removals.clear();
        self.spawns.clear();
        self.ripples.clear();
        self.idles.clear();
        self.bulldozed.clear();
        self.scenario = None;
        self.soundscape = None;
        self.research = None;
        self.research_done.clear();
        self.events.restore(&clocks.queue);
        if tiles != self.tiles {
            self.tiles = tiles;
            self.reseed();
        }
    }

    /// The game just opened has this soundscape: its draws join every step (sound 8.1).
    pub(crate) fn set_soundscape(&mut self, soundscape: Soundscape) {
        self.soundscape = Some(soundscape);
    }

    /// The ambient one-shots the steps since the last call asked for.
    pub(crate) fn take_shots(&mut self) -> Vec<Shot> {
        let mut shots = (self.soundscape.as_mut()).map_or_else(Vec::new, Soundscape::take_shots);
        shots.extend(self.calls.take());
        shots
    }

    /// The loops of the soundscape playing now (sound 8.1).
    pub(crate) fn scene_loops(&self) -> Vec<SceneLoop> {
        (self.soundscape.as_ref()).map_or_else(Vec::new, Soundscape::loops)
    }

    /// The scenario of the game just opened starts: its start triggers fire.
    pub(crate) fn start_scenario(&mut self, mut scenario: Scenario) {
        scenario.set_gathered(self.pass_ms);
        scenario.start(&mut self.lines);
        self.scenario = Some(scenario);
    }

    /// The world clock advances by `step` ms, then the ambient creatures whose period passed are
    /// made (ambients 7 point 2).
    fn tick_world(&mut self, data: &dyn Data, cat: &Catalogue, world: &mut World, step: u32) {
        let total = self.world_rem.wrapping_add(step);
        self.world_rem = total % 1000;
        self.world_secs = self.world_secs.wrapping_add(total / 1000);
        for ty in self.spawns.due(cat, self.world_secs) {
            crate::flight::launch(world, cat, ty, data.set_files(ty), &mut self.rng);
        }
    }

    /// The scenario's ambients join the spawn list, after its start hook (scenarios 5.2, ambients 7).
    pub(crate) fn add_scenario_ambients(&mut self) {
        let ids = self.scenario.as_ref().map_or(&[][..], |s| s.ambients());
        ids.iter().for_each(|&id| self.spawns.add(id));
    }

    /// A saved game that holds `scenario` opened (scenarios 5.3): its goals read the stored
    /// `words`, `groups` when the file's version is 86 or later.
    pub(crate) fn resume_scenario(&mut self, mut scenario: Scenario, words: &[u32], groups: bool) {
        scenario.set_gathered(self.pass_ms);
        scenario.resume(words, groups, &mut self.lines);
        self.scenario = Some(scenario);
    }

    /// The scenario being played, if the game is one.
    pub(crate) fn scenario(&self) -> Option<&Scenario> {
        self.scenario.as_ref()
    }

    /// Moves the events the world has posted since the last step into the queue at once, so a save
    /// or a hash taken before the next step holds them, ahead of that step's own posts
    /// (guest buildings 6.6 point 4).
    pub(crate) fn absorb(&mut self, world: &mut World) {
        for event in std::mem::take(&mut world.units.posts) {
            self.events.post(event);
        }
    }

    /// Adds a line to the `sim` log.
    pub(crate) fn note(&mut self, line: String) {
        self.lines.push(line);
    }

    /// How the scenario ended, once, when it has.
    pub(crate) fn scenario_ended(&mut self) -> Option<crate::scenario::Ended> {
        self.scenario.as_mut()?.ended()
    }

    /// A scenario or freeform map is a new game: its calendar starts on 1 January 2001 whatever
    /// date the file stores (main loop 7.2).
    pub(crate) fn restart_calendar(&mut self) {
        self.calendar = Calendar::new();
    }

    /// A scenario's own economy file over the settings of the game just started (zoo status 3.1).
    pub(crate) fn scenario_economy(&mut self, file: &[u8]) {
        if let Ok(file) = Config::parse(file) {
            self.status.economy(&file);
        }
    }

    /// Gives the economy file that every later game's zoo status starts from.
    pub(crate) fn set_economy(&mut self, file: &[u8]) {
        self.economy = Config::parse(file).ok();
    }

    /// The clocks, the status and the event queue a file stores (the inverse of
    /// [`Sim::start_game`]).
    pub(crate) fn clocks(&self) -> Clocks {
        Clocks {
            status: self.status.record(),
            ai_rem: self.ai_rem,
            ai_ticks: self.ai_ticks,
            world_rem: self.world_rem,
            world_secs: self.world_secs,
            play_time: self.play_time,
            queue: self.events.queue(),
            calendar: self.calendar.elapsed_ms(),
        }
    }

    pub(crate) fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    #[cfg(test)]
    pub(crate) fn total_ms(&self) -> u32 {
        self.total
    }

    /// The AI clock: whole AI ticks and the milliseconds left over.
    #[cfg(test)]
    pub(crate) fn ai_clock(&self) -> (u32, u32) {
        (self.ai_ticks, self.ai_rem)
    }

    /// The world clock: whole seconds and the milliseconds left over.
    #[cfg(test)]
    pub(crate) fn world_clock(&self) -> (u32, u32) {
        (self.world_secs, self.world_rem)
    }

    /// Asks for an entity to be removed at the next step's removal pass.
    pub(crate) fn remove(&mut self, id: u32) {
        self.removals.push(id);
    }

    /// The advice and donation messages since the last call, as text numbers and the amount of a
    /// donation.
    pub(crate) fn take_messages(&mut self) -> Vec<Say> {
        std::mem::take(&mut self.messages)
    }

    /// The `sim` log lines since the last call, without the family word.
    pub(crate) fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// One step of `step` milliseconds: the managers in the original's order. `update` is called
    /// for each entity of the list, by position, in list order, with the step, after the unit's
    /// own turn ([`unit::update`]). This is the AI manager's loop: the count is taken after the
    /// removal pass, so an entity added by `update` waits for the next step.
    #[cfg(test)]
    pub(crate) fn step(
        &mut self,
        world: &mut World,
        step: u32,
        update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_in(world, &Catalogue::default(), step, update)
    }

    /// [`Sim::step`] in a game whose types are `cat`, with accounts nobody reads: the units read
    /// their species' keys from it and the exhibits are rated for the species they hold.
    #[cfg(test)]
    pub(crate) fn step_in(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        step: u32,
        update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_with(&NoData, cat, world, step, update)
    }

    /// As [`Sim::step_in`], with the data the units' sets and animations read.
    #[cfg(test)]
    pub(crate) fn step_with(
        &mut self,
        data: &dyn Data,
        cat: &Catalogue,
        world: &mut World,
        step: u32,
        update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_game(data, cat, world, &mut Money::default(), step, update)
    }

    /// The frame each animated fixed object shows (sprite drawing 6.2), for the picture.
    pub(crate) fn idle_frames(&self) -> std::collections::HashMap<u32, u32> {
        self.idles.frames()
    }

    /// The animation of each building that shows another than `idle`: `used`, `idledead` or a rubble piece's `explode`.
    pub(crate) fn idle_looks(&self) -> std::collections::HashMap<u32, &'static str> {
        self.idles.looks()
    }

    /// The date of an escape (animals 11.1), then the money the units collected.
    fn book_units(&mut self, world: &mut World, money: &mut Money) {
        if std::mem::take(&mut world.units.loose) {
            self.status.escape = crate::timing::file_time(world.units.now);
        }
        settle_money(world, money);
    }

    /// [`Sim::step_with`] in a game whose cash and accounts are `money`: the zoo status checks
    /// read and book into them.
    #[cfg(test)]
    pub(crate) fn step_game(
        &mut self,
        data: &dyn Data,
        cat: &Catalogue,
        world: &mut World,
        money: &mut Money,
        step: u32,
        mut update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.step_hooked(Step {
            data,
            cat,
            world,
            money,
            ms: step,
            update: &mut update,
            created: &mut |_, _| {},
        })
    }
}

/// Moves the money the units collected in this step into the books.
fn settle_money(world: &mut World, money: &mut Money) {
    for (category, cost) in std::mem::take(&mut world.units.spent) {
        money.spend(category, cost);
    }
    for d in std::mem::take(&mut world.units.donated) {
        money.refund(money::DONATIONS, d);
    }
    for d in std::mem::take(&mut world.units.concessions) {
        money.refund(money::CONCESSIONS, d);
    }
    for d in std::mem::take(&mut world.units.show_income) {
        money.refund(money::SHOW_INCOME, d);
    }
    for d in std::mem::take(&mut world.units.recycled) {
        money.refund(money::CONSTRUCTION_REFUND, d);
    }
    for _ in 0..std::mem::take(&mut world.units.guests_gone) {
        money.count(money::GUESTS_GONE);
    }
    for _ in 0..std::mem::take(&mut world.units.members_joined) {
        money.count(money::MEMBERS);
        money.count(money::MEMBERS_JOINED);
    }
}
