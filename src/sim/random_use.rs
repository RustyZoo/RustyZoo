//! The random use of the buildings, wired to the simulation (guest buildings 10.1): the time
//! drawn when a building is made or placed, and the `used` animation queued when it is up.

use super::Sim;
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::world::World;

impl Sim {
    /// A building of random use plays `used` when its time is up (guest buildings 10.1).
    pub(super) fn random_use(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        data: &dyn Data,
        at: usize,
    ) {
        let now = self.ai_ticks;
        if let Some(times) = crate::building::random_use::due(world, cat, at, now, &mut self.rng) {
            self.idles.queue_used(world, at, times, data);
        }
    }

    /// The random use time of a pending building of type `ty`, drawn when it is made (guest
    /// buildings 10.1 point 2.5): none for a type that is not of random use.
    pub(crate) fn use_time(&mut self, ty: &crate::entity::EntityType) -> Option<u32> {
        crate::building::random_use::time(ty, self.ai_ticks, &mut self.rng)
    }

    /// The building `id` just placed keeps the time `kept` of its pending object; without one
    /// (a tool not chosen in the list) it draws now (guest buildings 10.1 point 2.5).
    pub(crate) fn keep_use(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        id: u32,
        kept: Option<u32>,
    ) {
        let Some(at) = world.find(id) else { return };
        match (kept, world.zoo.entities[at].common.as_mut()) {
            (Some(time), Some(c)) => c.stamp = Some(time),
            _ => crate::building::random_use::draw(world, cat, at, self.ai_ticks, &mut self.rng),
        }
    }

    /// A start with `reset` draws the random use time of every building again, in list order,
    /// from the restored clock (guest buildings 10.1 point 2.5).
    pub(crate) fn draw_use_all(&mut self, world: &mut World, cat: &Catalogue) {
        for at in 0..world.zoo.entities.len() {
            crate::building::random_use::draw(world, cat, at, self.ai_ticks, &mut self.rng);
        }
    }
}
