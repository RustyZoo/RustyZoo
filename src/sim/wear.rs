//! The wear of the fence pieces in the simulation: their step in the entity loop, and the repair
//! (placing 7.4).

use super::Sim;
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::world::World;

impl Sim {
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
            let draws = if now < 1 { 3 } else { 1 };
            let rng = &mut self.rng;
            self.idles.rewind_fence(world, cat, data, rng, (at, draws));
        }
    }

    /// Makes the fence piece at `at` healthy and draws its wear interval, and, for a piece that is
    /// not a gate, its start time right after (sprite drawing 3.6 item 5); a line tells of a
    /// piece that wears.
    pub(super) fn heal(&mut self, world: &mut World, cat: &Catalogue, at: usize) {
        let Some(id) = world.zoo.entities[at].common.as_ref().map(|c| c.id) else {
            return;
        };
        if let Some(interval) = world.heal(cat, id, &mut || self.rng.rand()) {
            self.draw_start(world, id);
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

    /// The break cheat (cheats 4.2 C): every piece that wears breaks and draws two start times at
    /// its place (sprite drawing 3.6 item 4).
    pub(crate) fn break_fences(&mut self, world: &mut World, cat: &Catalogue) {
        let (events, broke) = world.break_fences(cat, &mut self.lines);
        events.into_iter().for_each(|e| self.events.post(e));
        for id in broke {
            let start = (0..2).map(|_| self.rng.pick(2000)).last().unwrap_or(0);
            self.idles.start_next(id, start);
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
}
