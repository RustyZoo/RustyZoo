//! The simulation's zoo loading: the checks and draws of a zoo just opened or loaded.

use super::*;
use crate::status::zoo::{Settings, Sums};

impl Sim {
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
    pub(crate) fn fences_loaded(&mut self, world: &mut World, cat: &Catalogue) {
        // The pieces broken again at load come first: life 0, the break sound.
        let broken = world.break_loaded(cat);
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
        // The pieces broken again at load draw their two start times, in list order.
        for id in broken {
            let start = (0..2).map(|_| self.rng.pick(2000)).last().unwrap_or(0);
            self.idles.start_next(id, start);
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
        // Every entity base word stored as 0 is the AI clock held before the file's own was read
        // (zoo format 6.4, from version 35).
        if world.zoo.version >= 35 {
            for c in (world.zoo.entities.iter_mut()).filter_map(|e| e.common.as_mut()) {
                c.stamp.get_or_insert(self.held_ticks);
            }
        }
        for at in 0..world.zoo.entities.len() {
            if old && world.is_fence(at) {
                self.heal(world, cat, at);
            }
            crate::animal::load(world, cat, &mut self.rng, at);
            crate::guest::load(world, cat, &mut self.rng, at, date, reset);
            crate::staff::make(world, cat, &mut self.rng, at);
            crate::building::load(world, cat, at);
            crate::building::random_use::loaded(world, cat, at, self.held_ticks, &mut self.rng);
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
}
