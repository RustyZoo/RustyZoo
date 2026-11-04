//! The recover task (131) of a guest: climb out of the water (tank fauna 13). The guest walks to a
//! dry tile near it (or the first one east of it along the water); each failed search costs one
//! of its tries, and with none left it takes the drowning task (`drown.rs`).

use super::{Cx, DROWN};

/// Rounds of searches, tries in a round, and the reach of a try in tiles on each side.
const ROUNDS: u32 = 10;
const TRIES: u32 = 30;
const REACH: i32 = 5;

impl Cx<'_, '_> {
    /// Init: the guest has `2 + rand() % 3` tries.
    pub(super) fn recover_init(&mut self) -> bool {
        self.unit.goal.tries = Some(2 + self.ctx.rng.pick(3) as u8);
        true
    }

    /// Check done: done on a dry tile; with no try left, the drowning task.
    pub(super) fn recover_check_done(&mut self) {
        let Some(here) = self.tile() else { return };
        if !self.world.is_water(here) {
            return self.record().done = true;
        }
        if self.reached(false) {
            self.record().target.clear();
        }
        if self.unit.goal.tries == Some(0) {
            self.set_task(DROWN, 0, 0, None);
        }
    }

    /// Decide, while the guest is not on its way: a dry tile it may enter and reach, drawn near
    /// it, else the first one east along the water; without one a try is spent and, with
    /// tries left, the guest walks at random.
    pub(super) fn recover_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let Some(here) = self.tile() else { return };
        // A guest loaded in the middle of its recovery has no tries yet.
        if self.unit.goal.tries.is_none() {
            self.recover_init();
        }
        let width = self.world.zoo.width as i32;
        let (x, y) = (here as i32 % width, here as i32 / width);
        let mut found = None;
        'rounds: for _ in 0..ROUNDS {
            for _ in 0..TRIES {
                let dx = self.ctx.rng.pick(2 * REACH as u32) as i32 - REACH;
                let dy = self.ctx.rng.pick(2 * REACH as u32) as i32 - REACH;
                let Some(t) = self.world.tile_index(x + dx, y + dy) else {
                    continue;
                };
                // The first enterable dry tile ends the round, reachable or not.
                if !self.world.is_water(t)
                    && crate::unit::walk::fits(self.unit, self.world, here, t, true)
                {
                    if self.path_to(t, false) {
                        found = Some(t);
                        break 'rounds;
                    }
                    break;
                }
            }
        }
        if found.is_none() {
            let east: Vec<usize> = (x + 1..width)
                .map_while(|tx| {
                    let t = self.world.tile_index(tx, y)?;
                    self.world.in_zoo(t).then_some(t)
                })
                .collect();
            let dry = east.into_iter().find(|&t| !self.world.is_water(t));
            found = dry.filter(|&t| self.path_to(t, false));
        }
        if found.is_some() && self.return_walk(true, true) {
            return;
        }
        let tries = self.unit.goal.tries.as_mut().map(|t| {
            *t = t.saturating_sub(1);
            *t
        });
        self.unit.path.clear();
        // With no try left it does not walk at random (the drowning task follows).
        if tries == Some(0) {
            return;
        }
        let (unit, world) = (&mut *self.unit, &mut *self.world);
        if crate::unit::walk::random_walk(unit, world, self.ctx.cat, self.ctx.rng, self.at) {
            self.return_walk(true, true);
        }
    }
}
