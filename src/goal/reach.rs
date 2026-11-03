//! Arrival (goals 8.2): whether a unit reached its target.

use super::Cx;

impl Cx<'_, '_> {
    /// 8.2: whether the unit reached the target of the task, or of the job.
    pub(super) fn reached(&self, job: bool) -> bool {
        let r = &self.unit.goal.record;
        let target = if job { &r.job_target } else { &r.target };
        self.reached_at(target.tile, target.pos)
    }

    /// The same for a tile and a position the caller names.
    pub(super) fn reached_at(&self, tile: Option<usize>, pos: Option<(i32, i32, i32)>) -> bool {
        let Some(c) = self
            .world
            .zoo
            .entities
            .get(self.at)
            .and_then(|e| e.common.as_ref())
        else {
            return false;
        };
        let grid =
            |(x, y, z): (i32, i32, i32)| (x.div_euclid(64), y.div_euclid(64), z.div_euclid(16));
        match (c.movement_mode, pos) {
            (1, pos) => pos == Some((c.x, c.y, c.z)),
            (3, pos) => pos.map(grid) == Some(grid((c.x, c.y, c.z))),
            _ => tile.is_some() && tile == self.tile(),
        }
    }
}
