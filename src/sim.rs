//! The simulation step of the original (simulation 3 and 4, main loop 6): six managers in a fixed
//! order, the clocks they keep, the removal pass, the loop over the entities and the event queue,
//! and the game's one random generator.
//!
//! What a unit does with its step is the [`unit`] module's: [`Sim::step`] runs [`unit::update`]
//! for each entity in list order, then hands the entity to the caller's `update` (for the kinds
//! of entity that have no live state yet). The zoo status checks of the game manager are the
//! [`crate::status::zoo`] module's, and the guests it lets in the [`crate::guest`] module's. Missing
//! so far: the shows, the spawn list of the ambient creatures, and the research and marketing
//! managers, which are empty until M7. The scenario manager is [`crate::scenario`]'s (most of its rules and
//! triggers are stubs).
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
mod events;
pub mod hash;
pub(crate) mod idle;
mod inspect;
mod ripple;
mod rng;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod wear_tests;

pub(crate) use events::{
    BUILDING_CHANNEL, Event, Events, KEEPER_CHANNEL, SHOW_CHANNEL, TANK_CHANNEL, TASK_CHANNEL,
    UNIT_CHANNEL, WORLD_CHANNEL,
};
pub(crate) use rng::{Rng, Seconds};

use crate::behaviour::Data;
#[cfg(test)]
use crate::behaviour::NoData;
use crate::config::{self, Config};
use crate::entity::Catalogue;
use crate::exhibit::rating;
use crate::guest;
use crate::money::{self, Money};
use crate::research::{Done, Life, Overlay, Tree};
use crate::scenario::{Env, Game, Scenario};
use crate::soundscape::{Shot, Soundscape};
use crate::status::zoo::{Say, Settings, Status, Sums};
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
    /// The AI tick at which the unreachable list is next emptied; a value of the session, not
    /// of the zoo (simulation 4.3).
    next_clear: u32,
    /// The world clock: whole seconds and the milliseconds left over.
    world_secs: u32,
    world_rem: u32,
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
    /// The research tree of a scenario or freeform game, or of a saved game that names a
    /// scenario; none otherwise, which counts as no research done.
    research: Option<Tree>,
    /// The programs the research step completed, for the caller to apply.
    research_done: Vec<Done>,
    arrived: bool,
    /// The ambient sounds of the game being played, if it has them (sound 8.1).
    soundscape: Option<Soundscape>,
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
            next_clear: 0,
            world_secs: 0,
            world_rem: 0,
            ripples: ripple::Ripples::default(),
            idles: idle::Idles::default(),
            removals: Vec::new(),
            bulldozed: Vec::new(),
            lines: Vec::new(),
            scenario: None,
            research: None,
            research_done: Vec::new(),
            arrived: false,
            soundscape: None,
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
        (self.ai_rem, self.ai_ticks) = (clocks.ai_rem, clocks.ai_ticks);
        (self.world_rem, self.world_secs) = (clocks.world_rem, clocks.world_secs);
        self.removals.clear();
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
        self.soundscape
            .as_mut()
            .map_or_else(Vec::new, Soundscape::take_shots)
    }

    /// The research tree of the game just opened is built (research 7.4): the zoo status counts
    /// its programs from the next check on.
    pub(crate) fn set_research(&mut self, tree: Tree) {
        self.research = Some(tree);
    }

    /// The research tree, for the research panel.
    pub(crate) fn research(&self) -> Option<&Tree> {
        self.research.as_ref()
    }

    /// The research tree the player's clicks change.
    pub(crate) fn research_mut(&mut self) -> Option<&mut Tree> {
        self.research.as_mut()
    }

    /// The research section of a saved game for the writer; none when the game has no tree
    /// (research 7.5).
    pub(crate) fn research_section(&self) -> Option<Vec<u8>> {
        self.research.as_ref().map(Tree::save)
    }

    /// The programs completed since the last call (research 5.4).
    pub(crate) fn take_research_done(&mut self) -> Vec<Done> {
        std::mem::take(&mut self.research_done)
    }

    /// A program the research step completed, as the step reports it.
    #[cfg(test)]
    pub(crate) fn push_research_done(&mut self, done: Done) {
        self.research_done.push(done);
    }

    /// The building upgrade the completed program named `name` gives, with the name id of the
    /// building type it is for (research 6.3).
    pub(crate) fn research_upgrade(
        &self,
        name: i32,
    ) -> Option<(i32, crate::building::upgrade::Upgrade)> {
        self.research.as_ref()?.upgrade_of(name)
    }

    /// The characteristic effect of a program the step completed (research 6.4, 6.7), laid on
    /// `over`. The changes of the life counter come back for the live animals.
    pub(crate) fn research_effect(
        &mut self,
        cat: &Catalogue,
        over: &mut Overlay,
        d: &Done,
    ) -> Vec<Life> {
        let tree = self.research.as_mut();
        tree.map_or_else(Vec::new, |t| t.complete(cat, over, d))
    }

    /// Completes every research program of the game (research 6.8). The programs forced come back
    /// for the caller to apply their effects 0 and 1, with the changes of the life counter.
    pub(crate) fn force_research(
        &mut self,
        cat: &Catalogue,
        over: &mut Overlay,
        progress: bool,
    ) -> (Vec<Done>, Vec<Life>) {
        let Some(tree) = &mut self.research else {
            return Default::default();
        };
        let out = tree.force(cat, over, progress, &mut || self.rng.rand());
        self.lines.push(format!("research forced {}", out.0.len()));
        out
    }

    /// Completes every program of the research categories named `name` as a cheat does (research
    /// 6.8); the programs come back as for [`Sim::force_research`].
    pub(crate) fn complete_category(
        &mut self,
        cat: &Catalogue,
        over: &mut Overlay,
        name: i32,
    ) -> (Vec<Done>, Vec<Life>) {
        let Some(tree) = &mut self.research else {
            return Default::default();
        };
        let out = tree.complete_category(cat, over, name);
        self.lines
            .push(format!("research category {name} {}", out.0.len()));
        out
    }

    /// The research share of the zoo status: the tree's, 0 while a game has none.
    pub(crate) fn research_share(&self) -> i32 {
        self.research.as_ref().map_or(0, Tree::share)
    }

    /// The scenario of the game just opened starts: its start triggers fire.
    pub(crate) fn start_scenario(&mut self, scenario: Scenario) {
        scenario.start(&mut self.lines);
        self.scenario = Some(scenario);
    }

    /// A saved game that holds `scenario` opened (scenarios 5.3): its goals read the stored
    /// `words`, `groups` when the file's version is 86 or later.
    pub(crate) fn resume_scenario(&mut self, mut scenario: Scenario, words: &[u32], groups: bool) {
        scenario.resume(words, groups, &mut self.lines);
        self.scenario = Some(scenario);
    }

    /// The scenario being played, if the game is one.
    pub(crate) fn scenario(&self) -> Option<&Scenario> {
        self.scenario.as_ref()
    }

    /// Posts an event to the queue, as a unit's change does.
    pub(crate) fn post(&mut self, event: Event) {
        self.events.post(event);
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

    /// A zoo has just been loaded (Continue, Load, the command line, or the file of a new game
    /// before its set-up): the load ends with a rating check with the stored cash and the entities
    /// as loaded, research as the tree counts it, which restarts the rating timer (zoo status 16).
    pub(crate) fn loaded(&mut self, world: &World, cat: &Catalogue, money: &mut Money) {
        let sums = Sums::of(world, cat, money.cash, self.research_share());
        self.status
            .rating_check(sums, self.calendar.elapsed_ms(), money, &mut self.lines);
    }

    /// The trash cans, fence pieces and filters of a zoo just loaded post their events, after the
    /// events the file stored, in one pass in entity list order (guest buildings 4.1): a can above
    /// 12 pieces calls a worker (staff 8.2), a broken or decayed piece (placing 7.4) or filter
    /// asks for a mender, and dung or food on the grounds or in an empty exhibit calls a worker.
    pub(crate) fn fences_loaded(&mut self, world: &World, cat: &Catalogue) {
        let mut events = crate::building::loaded_events(world);
        events.extend(world.loaded_events(cat));
        events.extend(world.loaded_filter_events(cat));
        events.extend(crate::staff::chores::loaded_leftovers(world, cat));
        // One event per entity, so the sort by the sender's place is the file's order.
        let place = |id| {
            world
                .zoo
                .entities
                .iter()
                .position(|e| e.common.as_ref().is_some_and(|c| c.id == id))
        };
        events.sort_by_key(|e| place(e.sender));
        for event in events {
            self.events.post(event);
        }
    }

    /// Makes the fence piece at `at` healthy and draws its wear interval (placing 7.4); a line
    /// tells of a piece that wears.
    fn heal(&mut self, world: &mut World, cat: &Catalogue, at: usize) {
        let Some(id) = world.zoo.entities[at].common.as_ref().map(|c| c.id) else {
            return;
        };
        if let Some(interval) = world.heal(cat, id, &mut || self.rng.rand())
            && world.wears(cat, at)
        {
            self.lines.push(format!("fence {id} healthy {interval}"));
        }
    }

    /// The repair cheat (placing 7.4): every fence piece that is broken or at or below its type's
    /// decayed life is made healthy.
    pub(crate) fn repair_fences(&mut self, world: &mut World, cat: &Catalogue) {
        for at in 0..world.zoo.entities.len() {
            if world.decayed(cat, at) {
                self.heal(world, cat, at);
            }
        }
    }

    /// A new game (not a saved one) has just been set up: with `fences` (the map file's
    /// `resetfences` above 0) every fence piece is made healthy and draws its wear interval,
    /// otherwise the pieces stay as the map stores them (placing 7.4); the timers, the donation
    /// counter and the date of the last escape start again and the rating check of the set-up runs, with no cash, the built-in
    /// settings and every research program counted as done (zoo status 6.4).
    pub(crate) fn new_game(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        money: &mut Money,
        fences: bool,
    ) {
        for at in (0..world.zoo.entities.len()).filter(|_| fences) {
            self.heal(world, cat, at);
        }
        self.status.timers = [0; 3];
        self.status.given = 0.0;
        // The defaults also reset the date of the last escape, over the one the map stored.
        self.status.escape = crate::zoo::NEVER;
        let mut built = Settings::builtin();
        let old = &self.status.settings;
        (built.escaped_change, built.escaped_time) = (old.escaped_change, old.escaped_time);
        let set = std::mem::replace(&mut self.status.settings, built);
        let sums = Sums::of(world, cat, 0.0, 100);
        self.status
            .rating_check(sums, self.calendar.elapsed_ms(), money, &mut self.lines);
        self.status.settings = set;
    }

    /// Creates the animals of the map just loaded (animals 3.5) and its guests (guests 5.2, zoo
    /// format 6.12) with their saved state, and its staff (staff 4.2) with the draws of a new one,
    /// in file order, with the types of `cat` and the class keys in
    /// `world.units.class`; the guest buildings take their figures, price and fill, and then, once
    /// every guest is made, their users (guest buildings 4.1). The fence pieces of a file older than
    /// version 55 have no condition: they are made healthy (placing 7.4). The draws come before
    /// every draw of the first step.
    pub(crate) fn load(&mut self, world: &mut World, cat: &Catalogue) {
        let (date, reset) = (self.calendar.date(), self.ai.guest_reset_interval);
        let old = world.zoo.version < 55;
        for at in 0..world.zoo.entities.len() {
            if old && world.is_fence(at) {
                self.heal(world, cat, at);
            }
            crate::animal::load(world, cat, &mut self.rng, at);
            crate::guest::load(world, cat, &mut self.rng, at, date, reset);
            crate::staff::make(world, cat, &mut self.rng, at);
            crate::building::load(world, cat, at);
            crate::show::loaded(world, cat, at);
        }
        // The units go on with their saved sets; the users come once every guest is made,
        // whatever the order of the file.
        crate::unit::resume::load(world, self.ai_ticks);
        crate::goal::rides_loaded(world, cat, self.ai_ticks);
        crate::show::rejoin(world, cat);
        crate::show::associate(world, cat);
        crate::building::users::restore(world, cat, self.ai_ticks);
        crate::building::users::seat_loaded(world, cat, &mut self.rng);
        // The dead state is decided with no user put out: a zoo file is being loaded.
        crate::building::dead::decide_all(world, cat);
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

    /// The date of an escape (animals 11.1), then the money the units collected.
    fn book_units(&mut self, world: &mut World, money: &mut Money) {
        if std::mem::take(&mut world.units.loose) {
            self.status.escape = crate::timing::file_time(world.units.now);
        }
        settle_money(world, money);
    }

    /// [`Sim::step_with`] in a game whose cash and accounts are `money`: the zoo status checks
    /// read and book into them.
    pub(crate) fn step_game(
        &mut self,
        data: &dyn Data,
        cat: &Catalogue,
        world: &mut World,
        money: &mut Money,
        step: u32,
        mut update: impl FnMut(&mut World, usize, u32),
    ) -> Report {
        self.lines.push(format!("step {step}"));
        // The objects on the map now start their clocks, and advance by this step.
        self.idles.register(world, cat, data);
        // The game manager: the time, the calendar, whose month flag of the step before is acted
        // on here, and the zoo status (zoo status 4).
        self.total = self.total.wrapping_add(step);
        self.play_time = self.play_time.saturating_add(step);
        let month_end = self.calendar.advance(step);
        guest::mega::update(world, cat, step);
        guest::gawk::update(world, cat);
        let bars = self.zoo_status(world, cat, money, step, month_end);
        // The ambient soundscape follows the zoo status and the bars (simulation 3 point 1).
        if let Some(soundscape) = &mut self.soundscape {
            let guests = self.status.sums.guests;
            soundscape.update(step, guests, &mut self.rng, &mut self.lines);
        }
        world.units.now = self.calendar.elapsed_ms();
        // The AI manager: first the task pool reads the events of the buffer (simulation 4.2).
        let tiles = world.zoo.tiles.len();
        (world.units.pool).listen(self.events.listen(0, TASK_CHANNEL), tiles);
        // The show manager updates the shows, before the AI clock moves (shows 5.1).
        let turn = (&mut self.events, &mut self.rng, data);
        crate::show::update(world, cat, turn, self.ai_ticks);
        // The clock in whole ticks with the remainder kept, the removal pass,
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
        world.units.finder.tick(u64::from(self.ai_ticks));
        // Each animal's update ages its reactions (animals 10).
        world.units.fade(step);
        for id in std::mem::take(&mut self.removals) {
            crate::show::removed(world, cat, id);
            world.unrecorded(|w| w.remove(id));
        }
        for id in std::mem::take(&mut self.bulldozed) {
            world.remove_filter(cat, id);
        }
        crate::show::settle(world, cat);
        world.units.ai_ticks = self.ai_ticks;
        let settings = &self.status.settings;
        world.units.guest_join = (settings.join_happiness, settings.join_chance);
        world.units.donation_factor = settings.donation_factor;
        world.units.variable_speeds = self.ai.variable_unit_speeds != 0;
        world.units.attraction_score = self.ai.use_attraction_attractiveness != 0;
        world.units.show.distance = self.ai.max_dist_to_go_to_show;
        world.units.prices = settings.prices;
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
                unit::gate::advance(world, cat, data, at, step);
                crate::flight::advance(world, at, step, &mut self.removals);
                crate::show::advance_portals(world, data, at, step);
                update(world, at, step);
                // A grandstand reads the events of its show (shows 6.3).
                crate::show::listen_grandstand(world, cat, &mut self.events, &mut self.rng, at);
                // A fence piece wears at its own place in the list.
                for event in world.wear(cat, at, step, &mut self.lines) {
                    self.events.post(event);
                }
                // So does a tank filter.
                let (events, gone) = world.filter_step(cat, at, step);
                for event in events {
                    self.events.post(event);
                }
                self.bulldozed.extend(gone);
            }
        }
        unit::gate::forget(world);
        self.idles.advance(step);
        for event in std::mem::take(&mut world.units.posts) {
            self.events.post(event);
        }
        self.events.age();
        self.book_units(world, money);
        // The world manager: the zoo grounds' timers, then the exhibits' timers and ratings, the
        // water of the tanks, the world clock.
        let mut grounds = std::mem::take(&mut world.exhibits_mut().grounds);
        grounds.update(step, &mut || self.rng.rand(), rating::Input::default);
        world.exhibits_mut().grounds = grounds;
        for at in 0..world.exhibits().list().len() {
            let id = world.exhibits().list()[at].id;
            let mut state = std::mem::take(&mut world.exhibits_mut().list_mut()[at].rating);
            let rand = &mut || self.rng.rand();
            let rated = state.update(step, rand, || rating::input(world, cat, id));
            world.units.cheat.lions_tigers_bears |= state.trio;
            let shown: Vec<String> = (state.records.iter())
                .filter(|_| rated)
                .map(|r| format!(" {} {}", r.key, r.shown()))
                .collect();
            world.exhibits_mut().list_mut()[at].rating = state;
            // An exhibit whose last animal went loses its viewing time (13.2) and calls the workers
            // for its dung and food (5.1).
            let gone = std::mem::take(&mut world.exhibits_mut().list_mut()[at].rating.emptied);
            if gone {
                world.exhibits_mut().list_mut()[at].stats.viewing = 0;
            }
            if gone && world.exhibits().get(id).is_some_and(|e| e.tank.is_none()) {
                let tiles = world
                    .exhibits()
                    .get(id)
                    .map_or(Vec::new(), |e| e.order.clone());
                for (piece, tile) in crate::staff::chores::leftovers(world, cat, &tiles) {
                    self.events.post(crate::world::piece_event(
                        piece,
                        crate::animal::dirt::DUNG,
                        tile as u32,
                    ));
                }
            }
            if rated {
                rating::finish(world, cat, id, &mut || self.rng.rand());
                crate::animal::home::settle(world, cat, id);
                self.lines.push(format!("rate {id}{}", shown.concat()));
            }
            crate::show::open_portals(world, cat, id);
        }
        let water_moved = world.step_water();
        // Buildings on a tank whose water moved are validated again (guest buildings 11.1.1).
        crate::building::dead::wet_changed(world, cat, &mut self.rng);
        world.decay_water(step);
        self.water_effects(world, cat, step);
        let total = self.world_rem.wrapping_add(step);
        self.world_rem = total % 1000;
        self.world_secs = self.world_secs.wrapping_add(total / 1000);
        // The shows' events reach the tank and the grandstands (shows 6.2).
        crate::show::listen_world(world, cat, &mut self.events, &mut self.rng);
        // The scenario manager.
        if let Some(scenario) = &mut self.scenario {
            let game = Game {
                world,
                cat,
                money,
                status: &self.status,
            };
            let env = Env::new(money.month as i32, money.year as i32, Some(game));
            scenario.update(step, &env, &mut self.lines);
        }
        // The research manager; marketing has nothing to do before M7.
        if let Some(tree) = &mut self.research {
            let done = tree.step(step, money, &mut || self.rng.rand());
            for d in &done {
                self.lines.push(format!("research done {}", d.name));
            }
            self.research_done.extend(done);
        }
        Report {
            arrived: std::mem::take(&mut self.arrived),
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
        world: &mut World,
        cat: &Catalogue,
        money: &mut Money,
        step: u32,
        month_end: bool,
    ) -> Option<[i32; 3]> {
        let due = self.status.due(step, month_end);
        if due.rating {
            let sums = Sums::of(world, cat, money.cash, self.research_share());
            self.status
                .rating_check(sums, self.calendar.elapsed_ms(), money, &mut self.lines);
        }
        if due.message {
            let (say, log) = (&mut self.messages, &mut self.lines);
            self.status.message_check(money.cash, say, log);
        }
        if due.new_guest {
            let occupied = guest::occupied(world, cat);
            // The effective rating: a cheat adds to it (zoo status 8).
            let bonus = world.units.cheat.rating_points();
            if let Some(advice) = self.status.new_guest_check(occupied, bonus, &mut self.rng) {
                let s = &self.status;
                let arrival = guest::Arrival {
                    price: s.price,
                    multiple: s.settings.admission_multiple,
                    guests: s.sums.guests,
                    max_guests: self.ai.max_guests,
                    date: self.calendar.date(),
                    reset: self.ai.guest_reset_interval,
                    real_halloween: (self.real_day)() == (10, 31),
                };
                if guest::arrive(world, cat, &mut self.rng, money, &arrival) {
                    self.arrived = true;
                    if advice {
                        self.status.advise(&mut self.messages, &mut self.lines);
                    }
                }
            }
        }
        if month_end {
            self.reseed();
            let date = self.calendar.date();
            self.lines
                .push(format!("month {} {}", date.month, date.year));
            // The endowment goes before the accounts move on, the wages and the upkeep after, in
            // the new month's column.
            self.status.endowment(&mut self.rng, money, &mut self.lines);
            money.month_change();
            world.exhibits_mut().enter_new_month();
            crate::show::new_month(world, cat);
            crate::month::charge(world, cat, money, &mut self.lines);
        }
        let (say, log) = (&mut self.messages, &mut self.lines);
        self.status.donation(&mut self.rng, money, say, log);
        self.status.bars(step)
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
