//! The wear of the fence pieces in the simulation: their step in the entity loop, and the repair
//! (placing 7.4).

use super::Sim;
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::world::World;

impl Sim {
    /// The fence pieces the workers just mended start their animation at the time they drew.
    pub(super) fn start_mended(&mut self, world: &mut World, cat: &Catalogue, data: &dyn Data) {
        for mended in std::mem::take(&mut world.units.mended) {
            self.idles.start_fence(world, cat, data, mended);
        }
    }

    /// The wear step of the fence piece at `at` (placing 7.4), run at its place in the entity
    /// loop: its events are posted; a step that wears chooses the picture again, which draws a
    /// start time for its animation, and a break draws three times (the break sets the picture, its
    /// validation sets it again and the step sets it a last time).
    pub(super) fn wear_fence(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        data: &dyn Data,
        at: usize,
        ms: u32,
    ) {
        let life = world.fence_life(at);
        for event in world.wear(cat, at, ms, &mut self.lines) {
            self.events.post(event);
        }
        if let Some(now) = world.fence_life(at).filter(|&l| Some(l) != life) {
            // Only a piece that broke draws three times.
            let draws = if now < 1 && world.breakable(cat, at).is_some() {
                3
            } else {
                1
            };
            let rng = &mut self.rng;
            self.idles.rewind_fence(world, cat, data, rng, (at, draws));
        }
    }

    /// Makes the fence piece at `at` healthy and draws its wear interval, and, for a piece that is
    /// not a gate, its start time right after (sprite drawing 3.6 item 5); a line tells of a
    /// piece that wears.
    pub(super) fn heal(&mut self, world: &mut World, cat: &Catalogue, at: usize) {
        self.heal_with(world, cat, at, true);
    }

    /// The same, with or without the start time (a gate's copy draws its interval only).
    pub(super) fn heal_with(&mut self, world: &mut World, cat: &Catalogue, at: usize, start: bool) {
        let Some(id) = world.zoo.entities[at].common.as_ref().map(|c| c.id) else {
            return;
        };
        if let Some(interval) = world.heal(cat, id, &mut || self.rng.rand()) {
            if start {
                self.draw_start(world, id);
            }
            if world.wears(cat, at) {
                self.lines.push(format!("fence {id} healthy {interval}"));
            }
        }
    }

    /// The piece `id` was made healthy: when it is not a gate it draws its start time, which its
    /// animation takes at the next register.
    pub(crate) fn draw_start(&mut self, world: &World, id: u32) {
        let gate = (world.find(id)).is_none_or(|e| world.zoo.entities[e].subtype.starts_with('g'));
        if !gate {
            self.idles.start_next(id, self.rng.pick(2000));
        }
    }

    /// The break cheat (cheats 4.2 C): every piece that can deteriorate breaks and draws two start times at
    /// its place (sprite drawing 3.6 item 4).
    pub(crate) fn break_fences(&mut self, world: &mut World, cat: &Catalogue) {
        let (events, broke) = world.break_fences(cat, &mut self.lines);
        events.into_iter().for_each(|e| self.events.post(e));
        for id in broke {
            let start = (0..2).map(|_| self.rng.pick(2000)).last().unwrap_or(0);
            self.idles.start_next(id, start);
        }
    }

    /// The `$` key's second effect (input 4.8): one candidate piece is drawn (one draw, none when
    /// there is no candidate); a tank wall halves its tank's purity, any other piece breaks like
    /// the break cheat when it can.
    pub(crate) fn dollar_break(&mut self, world: &mut World, cat: &Catalogue) {
        let found = world.dollar_candidates();
        if found.is_empty() {
            return;
        }
        let at = found[self.rng.pick(found.len() as u32) as usize];
        if world.halve_tank_water(at) {
            return;
        }
        let mut events = Vec::new();
        if world.break_piece(cat, at, &mut self.lines, &mut events) && world.loops(at) {
            let id = world.zoo.entities[at].common.as_ref().map_or(0, |c| c.id);
            let start = (0..2).map(|_| self.rng.pick(2000)).last().unwrap_or(0);
            self.idles.start_next(id, start);
        }
        events.into_iter().for_each(|e| self.events.post(e));
    }

    /// The repair cheat (placing 7.4): every fence piece that is broken or at or below its type's
    /// decayed life is made healthy, with no wear test.
    pub(crate) fn repair_fences(&mut self, world: &mut World, cat: &Catalogue) {
        for at in 0..world.zoo.entities.len() {
            if world.repairable(cat, at) {
                self.heal(world, cat, at);
            }
        }
    }
}
