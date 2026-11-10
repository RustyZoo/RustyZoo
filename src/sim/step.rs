//! The step of the six managers, called from a fixed table with one [`Step`].

use super::{Env, Game, Report, Sim, TASK_CHANNEL, guest, rating, unit};
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::money::Money;
use crate::world::World;

/// What one step reads and writes: the game, the length of the step and the two hooks.
pub(crate) struct Step<'a> {
    pub data: &'a dyn Data,
    pub cat: &'a Catalogue,
    pub world: &'a mut World,
    pub money: &'a mut Money,
    /// Milliseconds of the step.
    pub ms: u32,
    /// Called for each entity of the list, by position, after the unit's own turn.
    pub update: &'a mut dyn FnMut(&mut World, usize, u32),
    /// Runs inside the guest creation, once the new guest is in the world and its admission
    /// booked, so what it adds to the world moves in this very step and its draws come before
    /// those of the entities (ambients 3.1).
    pub created: &'a mut dyn FnMut(&mut Sim, &mut World),
}

/// The managers in the original's order.
const MANAGERS: [fn(&mut Sim, &mut Step); 6] = [
    Sim::game_manager,
    Sim::ai_manager,
    Sim::world_manager,
    Sim::scenario_manager,
    Sim::research_manager,
    Sim::marketing_manager,
];

impl Sim {
    /// One step: each manager in turn, then the guests that arrived.
    pub(crate) fn step_hooked(&mut self, mut ctx: Step) -> Report {
        self.report = Report::default();
        self.lines.push(format!("step {}", ctx.ms));
        for manager in MANAGERS {
            manager(self, &mut ctx);
        }
        self.report.arrived = std::mem::take(&mut self.arrived);
        self.report
    }

    fn game_manager(&mut self, ctx: &mut Step) {
        let (data, cat, step) = (ctx.data, ctx.cat, ctx.ms);
        let (world, money) = (&mut *ctx.world, &mut *ctx.money);
        // The objects on the map now start their clocks, and advance by this step.
        self.draw_ripples(world, cat, data);
        self.idles.register(world, cat, data, &mut self.rng);
        let made = world.units.take_exploded();
        self.idles.queue_explode(world, &made, data);
        // The time, the calendar, whose month flag of the step before is acted
        // on here, and the zoo status (zoo status 4).
        self.total = self.total.wrapping_add(step);
        self.play_time = self.play_time.saturating_add(step);
        let month_end = self.calendar.advance(step);
        self.report.month_end = month_end;
        guest::mega::update(world, cat, step);
        guest::gawk::update(world, cat);
        self.report.bars = self.zoo_status(world, cat, money, step, month_end, &mut *ctx.created);
        // The ambient soundscape follows the zoo status and the bars (simulation 3 point 1).
        if let Some(soundscape) = &mut self.soundscape {
            let guests = self.status.sums.guests;
            soundscape.update(step, guests, &mut self.rng, &mut self.lines);
        }
        world.units.now = self.calendar.elapsed_ms();
    }

    fn ai_manager(&mut self, ctx: &mut Step) {
        let (data, cat, step) = (ctx.data, ctx.cat, ctx.ms);
        let (world, money) = (&mut *ctx.world, &mut *ctx.money);
        // First the task pool reads the events of the buffer (simulation 4.2).
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
        self.report.clear_unreachable = clear_unreachable;
        if clear_unreachable {
            self.next_clear = self
                .ai_ticks
                .wrapping_add(self.ai.clear_unreachable_freq as u32);
        }
        world.units.finder.tick(u64::from(self.ai_ticks));
        // Each animal's update ages its reactions (animals 10).
        world.units.fade(step);
        for id in self.removal_pass(world) {
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
                let mut uctx = unit::Ctx {
                    rng: &mut self.rng,
                    events: &self.events,
                    removals: &mut self.removals,
                    step,
                    cat,
                };
                unit::update(&mut unit::Standard { data }, world, &mut uctx, at);
                // A keeper's cleaning or a dung may have made the water clean.
                self.draw_ripples(world, cat, data);
                self.start_mended(world, cat, data);
                self.random_use(world, cat, data, at);
                unit::gate::advance(world, cat, data, at, step);
                crate::flight::advance(world, at, step, &mut self.removals);
                let calls = &mut self.calls;
                crate::flight::call(calls, world, cat, data, at, step, &mut self.rng);
                crate::show::advance_portals(world, data, at, step);
                (ctx.update)(world, at, step);
                // A grandstand reads the events of its show (shows 6.3).
                crate::show::listen_grandstand(world, cat, &mut self.events, &mut self.rng, at);
                // A fence piece wears at its own place in the list.
                self.wear_fence(world, cat, data, at, step);
                // So does a tank filter.
                self.filter_update(world, cat, data, (at, step));
            }
        }
        unit::gate::forget(world);
        self.idles.advance(step);
        self.use_sounds(world);
        for event in std::mem::take(&mut world.units.posts) {
            self.events.post(event);
        }
        self.events.age();
        self.book_units(world, money);
    }

    fn world_manager(&mut self, ctx: &mut Step) {
        let (data, cat, step) = (ctx.data, ctx.cat, ctx.ms);
        let world = &mut *ctx.world;
        // The zoo grounds' timers, then the exhibits' timers and ratings, the
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
        self.report.water_moved = self.tank_updates(world, cat, data, step);
        self.water_effects(world, cat, step);
        self.tick_world(data, cat, world, step);
        // The shows' events reach the tank and the grandstands (shows 6.2).
        crate::show::listen_world(world, cat, &mut self.events, &mut self.rng);
    }

    fn scenario_manager(&mut self, ctx: &mut Step) {
        let (cat, step) = (ctx.cat, ctx.ms);
        let (world, money) = (&mut *ctx.world, &mut *ctx.money);
        if let Some(scenario) = &mut self.scenario {
            let game = Game {
                world,
                cat,
                money,
                status: &self.status,
                research: self.research.as_ref(),
            };
            let env = Env::new(money.month as i32, money.year as i32, Some(game));
            scenario.update(step, &env, &mut self.lines);
            self.pass_ms = scenario.gathered();
        }
    }

    fn research_manager(&mut self, ctx: &mut Step) {
        let step = ctx.ms;
        let money = &mut *ctx.money;
        if let Some(tree) = &mut self.research {
            let done = tree.step(step, money, &mut || self.rng.rand());
            for d in &done {
                self.lines.push(format!("research done {}", d.name));
            }
            self.research_done.extend(done);
        }
    }

    fn marketing_manager(&mut self, ctx: &mut Step) {
        self.marketing.step(ctx.ms, ctx.money);
    }
}
