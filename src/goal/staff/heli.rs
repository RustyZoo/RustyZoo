//! The helicopter's goals (staff 7.11.1): the ladder that sends it after an escaped animal or home,
//! 0x6a (wait at the base), 0x69 (take off), 0x67 (return) and 0x68 (land), and the pieces of the
//! capture that differ for a flier (the flight to the animal and the dart on its base).

use super::super::{CAPTURE, Cx, ENTER_BUILDING, ENTRANCE, EXIT_BUILDING, Goal, NONE, USE};
use crate::staff::heli;
use crate::unit::{Kind, Part, movement};

impl Cx<'_, '_> {
    /// The entity list position of the base, when the base id names a building.
    pub(super) fn base_entity(&self) -> Option<usize> {
        match &self.unit.part {
            Part::Staff(s) => heli::base_at(self.world, s.heli.base),
            _ => None,
        }
    }

    fn flying(&mut self, on: bool) {
        if let Part::Staff(s) = &mut self.unit.part {
            s.heli.set_flying(on);
        }
    }

    /// The helicopter's destination setter (point 8.1).
    fn fly_dest(&mut self, to: (i32, i32, i32)) {
        heli::set_destination(self.unit, self.world, self.at, to);
    }

    /// The path request of a helicopter (point 4.2): no search, the destination is the corner of
    /// tile `t` at its base height.
    pub(super) fn fly_to(&mut self, t: usize) -> bool {
        let width = self.world.zoo.width as usize;
        let level = self.world.zoo.tiles.get(t).map_or(0, |t| t.elevation);
        self.fly_dest(((t % width) as i32 * 64, (t / width) as i32 * 64, level * 16));
        true
    }

    /// `goToDest` with the set the goal names: it starts when the unit may move and has a
    /// destination, and the destination becomes the target.
    fn go_dest(&mut self, set: &str, force: bool) -> bool {
        let free = !self.unit.suspended() && !self.unit.held && self.unit.destination.is_some();
        let started = free && self.start_with(set, false, force);
        if started {
            let (world, dest) = (&*self.world, self.unit.destination);
            self.unit.goal.record.target.set_pos(world, dest);
        }
        started
    }

    /// Whether the set `name` is the one running now.
    fn running(&self, name: &str) -> bool {
        self.unit.runner.top() == Some(name) && self.unit.runner.running()
    }

    /// The position equals the destination, x, y and z.
    fn at_destination(&self) -> bool {
        let c = self.world.zoo.entities[self.at].common.as_ref();
        self.unit.destination == c.map(|c| (c.x, c.y, c.z))
    }

    /// Point 1: no base, no helicopter; with no task it goes after an escaped animal, else home.
    pub(in crate::goal) fn heli_ladder(&mut self, task: u8) -> bool {
        if self.base_entity().is_none() {
            self.ctx.remove(self.world, self.unit.id);
            return false;
        }
        if task == NONE {
            match self.found_escaped() {
                true => {
                    let animal = self.staff().map_or(0, |s| s.captured);
                    self.set_task(CAPTURE, 3, animal, None);
                }
                false => self.set_task(ENTRANCE, 0, 0, None),
            }
        }
        true
    }

    /// Points 3.1, 5.1 and 6.1: the destination and the first set of 0x69, 0x67 and 0x68. False
    /// clears the task.
    pub(in crate::goal) fn heli_init(&mut self, goal: Goal) -> bool {
        let points = self
            .base_entity()
            .and_then(|b| Some((b, heli::points(self.world, self.ctx.cat, b, 0)?)));
        let Some((base, (slot, exit))) = points else {
            return false;
        };
        match goal {
            Goal::HeliExit => {
                self.fly_dest(exit);
                self.unit.runner.top() == Some("bSpinup") || self.start_with("bSpinup", false, true)
            }
            Goal::HeliReturn => {
                self.fly_dest(exit);
                self.go_dest("bFly", true);
                true
            }
            _ => {
                heli::add_user(self.world, self.ctx.cat, base, self.unit.id);
                self.fly_dest(slot);
                self.go_dest("bDescend", true);
                true
            }
        }
    }

    /// Points 2, 3.2, 5.2 and 6.2.
    pub(in crate::goal) fn heli_check_done(&mut self, goal: Goal) {
        let done = match goal {
            Goal::HeliBase => self.found_escaped(),
            Goal::HeliReturn => {
                let dest = self.unit.destination;
                let to = dest.and_then(|(x, y, _)| {
                    self.world.tile_index(x.div_euclid(64), y.div_euclid(64))
                });
                to.is_some() && to == self.tile()
            }
            _ if self.at_destination() => {
                movement::stop(self.unit);
                match goal {
                    Goal::HeliExit => self.flying(true),
                    _ if self.unit.runner.top() != Some("bSpindown") => {
                        self.start_with("bSpindown", false, true);
                    }
                    _ => {}
                }
                true
            }
            _ => false,
        };
        self.record().done |= done;
    }

    /// Points 3.3 and 5.3.
    pub(in crate::goal) fn heli_decide(&mut self, goal: Goal) {
        if goal == Goal::HeliReturn {
            if self.found_escaped() {
                let animal = self.staff().map_or(0, |s| s.captured);
                self.set_task(CAPTURE, 0, animal, None);
            }
            return;
        }
        let flying = matches!(&self.unit.part, Part::Staff(s) if s.heli.flying);
        if !self.running("bSpinup") && !flying {
            self.flying(true);
            self.go_dest("bAscend", false);
        }
    }

    /// Points 2, 3.4, 5.4 and 6.3.
    pub(in crate::goal) fn heli_complete(&mut self, goal: Goal) {
        match goal {
            Goal::HeliBase => self.set_task(EXIT_BUILDING, 0, 0, None),
            Goal::HeliReturn => self.set_task(ENTER_BUILDING, 0, 0, None),
            Goal::HeliExit => {
                if let Some(base) = self.base_entity() {
                    heli::remove_user(self.world, base, self.unit.id);
                }
                let animal = self.staff().map_or(0, |s| s.captured);
                self.set_task(CAPTURE, 0, animal, None);
            }
            _ if self.running("bSpindown") => {}
            _ => {
                self.flying(false);
                self.set_task(USE, 0, 0, None);
            }
        }
    }

    /// The dart of job 14 counts on the base of a helicopter (point 4.5).
    pub(super) fn dart_hook(&mut self) {
        if let (Kind::Heli, Some(base)) = (self.unit.kind, self.base_entity()) {
            heli::add_dart(self.world, base);
        }
    }
}
