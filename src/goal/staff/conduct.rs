//! The marine specialist's job 19, conducting a show (shows 22.1 points 4 to 6): he stands on the
//! platform of the show tank from the announce to the stop and is in the show's unit list while
//! he does. What brings him there is the duties' decide ([`Cx::duties_decide`]); his cues are
//! `show::cue`.

use super::{Cx, tank::TO_INSIDE_LADDER};
use crate::animal::food::nearest_direction;
use crate::goal::Sub;
use crate::show::{self, CONDUCT};
use crate::staff::ladder::Ladder;
use crate::unit::movement;

impl Cx<'_, '_> {
    /// Duties 3.1: the job of a keeper whom the show of his exhibit `x` needs: the show job on the
    /// platform, else the way to the inside ladder. False when it needs no keeper of his type.
    pub(super) fn show_job(&mut self, x: u32, key: u32) -> bool {
        if !show::needs_keeper(self.world, x, key) {
            return false;
        }
        let job = if self.unit.tank == 2 {
            CONDUCT
        } else {
            TO_INSIDE_LADDER
        };
        self.set_job(job, 0, None);
        true
    }

    /// Init: the exhibit under him is the show's. In a tank he is moved to the platform, turned
    /// into the tank, and his idle animation starts again; then he joins the show.
    pub(super) fn conduct_init(&mut self) -> bool {
        let Some(x) = self.exhibit() else {
            return false;
        };
        if let Some(l) = Ladder::of(self.world, x) {
            let w = self.world.zoo.width as usize;
            let (i, o) = (l.inside, l.outside);
            let (dx, dy) = (
                (o % w) as i32 - (i % w) as i32,
                (o / w) as i32 - (i / w) as i32,
            );
            let rotation = self.world.zoo.entities[self.at].common.as_ref();
            let facing = rotation.map_or(0, |c| c.rotation);
            let facing = nearest_direction(dx, dy).map_or(facing, |d| (d + 4) % 8);
            self.world.place_unit(self.at, l.platform, facing);
            movement::idle(self.unit, self.world, self.data, self.at);
        }
        if let Some(ty) = self.who() {
            show::join(self.world, self.ctx.cat, x, self.unit.id, ty);
        }
        self.unit.show = show::info_of(self.world, x).map_or(0, |i| i.id);
        true
    }

    /// Check done: done when the show of his exhibit is not his, is stopped, or he is off the
    /// platform; with no exhibit his goal is reset.
    pub(super) fn conduct_check_done(&mut self) {
        let Some(x) = self.exhibit() else {
            return self.reset();
        };
        let info = show::info_of(self.world, x);
        let other = info.map_or(0, |i| i.id) != self.unit.show;
        let stopped = info.is_none_or(|i| !(i.show.ready || i.is_started()));
        if other || stopped || self.unit.tank != 2 {
            self.record().job_done = true;
        }
    }

    /// The job object is dropped, completed or not (a completed one at the next goal update): he
    /// leaves the list of his show and has no show.
    pub(in crate::goal) fn sub_dropped(&mut self) {
        let old = self.unit.goal.sub.take();
        self.unit.goal.held.1 = None;
        if old != Some(Sub::Keeper(CONDUCT)) {
            return;
        }
        let (show, id) = (self.unit.show, self.unit.id);
        if let Some(ty) = self.who() {
            show::leave(self.world, show, id, ty.name_id as u32);
        }
        self.unit.show = 0;
    }
}

#[cfg(test)]
mod tests;
