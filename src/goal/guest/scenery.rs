//! Walking to a scenery object (0x80) and gawking at it (0x81) (goals 11.12; guests visit 3.5, 8.7).

use super::super::{Cx, DO_SCENERY};
use crate::guest::{gawk, view};
use crate::unit::Part;

impl Cx<'_, '_> {
    /// 0x80, check done: a scenery object that is gone ends the goal; at the viewing place the guest
    /// faces the object and the object joins its seen list, and the goal is done.
    pub(in crate::goal) fn scenery_check_done(&mut self) {
        let id = self.record().target.id;
        let Some(at) = self.world.find(id) else {
            return self.reset();
        };
        if !self.reached(false) {
            return;
        }
        if let Some(tile) = view::tile_of(self.world, at) {
            self.face(tile);
            if let Part::Guest(g) = &mut self.unit.part {
                view::see(g, tile);
            }
        }
        self.record().done = true;
    }

    /// 0x80, complete: gawk at the object.
    pub(in crate::goal) fn scenery_arrived(&mut self) {
        let id = self.record().target.id;
        self.set_task(DO_SCENERY, 0, id, None);
    }

    /// 0x81, init: the object must still exist. A guest not yet playing a gawk set starts the one
    /// its esthetic bonus picks, and then thinks one of the object's thoughts, chosen by one draw.
    pub(in crate::goal) fn do_scenery_init(&mut self) -> bool {
        let id = self.record().target.id;
        let Some(at) = self.world.find(id) else {
            return false;
        };
        if self.gawking() {
            return true;
        }
        let (Some(ty), Some(k)) = (
            gawk::kind(self.world, self.ctx.cat, at),
            crate::guest::keys(self.world, self.ctx.cat, self.at),
        ) else {
            return true;
        };
        let thoughts = gawk::thoughts(ty);
        if self.start_set(gawk::gawk_set(ty, k.name_id), false) && !thoughts.is_empty() {
            let pick = self.ctx.rng.pick(thoughts.len() as u32) as usize;
            self.think(thoughts[pick], None);
        }
        true
    }

    /// 0x81, check done: the set has ended.
    pub(in crate::goal) fn do_scenery_check_done(&mut self) {
        self.record().done = !self.gawking();
    }

    /// Whether the guest is playing a gawk set.
    fn gawking(&self) -> bool {
        let runner = &self.unit.runner;
        runner.running() && runner.top().is_some_and(|s| s.starts_with("bGawkObject"))
    }
}
