//! The goals of guests (goals 10.2, 11): the way out, the choice of what to see and viewing it.
//! A guest whose leave flag is up and that is not busy gets the leave task (0x71) from its
//! ladder, walks to the zoo entrance tile and is removed there; the step books it as gone and, with
//! some chance, as a new endowment member. A guest with an exhibit it has not seen gets the site
//! task, which picks one and sends it to a tile it can look at it from ([`sites`]), where it
//! views the animals. The leave flag is raised by the unhappy-leave check
//! ([`crate::guest::checks`]) and by `checkLeaveZoo`, which the ladder runs when nothing is left to
//! see. The log lines are those of the `guest` family of [`crate::guest`].

use super::Cx;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::unit::Part;

mod sites;

/// The leave task.
pub const LEAVE: u8 = 113;
/// The avoid and exit tasks, which keep a guest from leaving, and the drowning task.
const AVOID: u8 = 117;
const EXIT: u8 = 127;
const DROWN: u8 = 132;
/// The tasks of using a building: entering, leaving and using it.
const IN_BUILDING: std::ops::RangeInclusive<u8> = 104..=106;

impl Cx<'_, '_> {
    /// Ladder point 2: a guest that decided to leave and is not leaving, avoiding or exiting, not
    /// using a building, not on water and not in an exhibit or a tank gets the leave task, mode
    /// 1. Whether it did (no other rung runs then). Nobody is afraid before M7.
    pub(super) fn leave_ladder(&mut self) -> bool {
        let task = self.record().task;
        let wants = matches!(&self.unit.part, Part::Guest(g) if g.leave);
        let busy = [LEAVE, AVOID, EXIT].contains(&task) || IN_BUILDING.contains(&task);
        if !wants || busy || self.mode() == 3 {
            return false;
        }
        let Some(tile) = self.tile() else {
            return false;
        };
        let inside = self.world.exhibits().region(tile);
        if self.world.is_water(tile) || inside.is_some_and(|r| r != GROUNDS && r != EXTERIOR) {
            return false;
        }
        self.set_task(LEAVE, 1, 0, None);
        true
    }

    /// Ladder point 1: a guest standing in an exhibit or a tank (put there, or fallen in) gets the
    /// exit task, mode 0, unless it has it already or is drowning or avoiding (the thought is the
    /// thoughts issue's). Whether it did.
    pub(super) fn exit_ladder(&mut self) -> bool {
        let task = self.record().task;
        let inside = self.tile().and_then(|t| self.world.exhibits().region(t));
        if [EXIT, DROWN, AVOID].contains(&task)
            || !inside.is_some_and(|r| r != GROUNDS && r != EXTERIOR)
        {
            return false;
        }
        self.set_task(EXIT, 0, 0, None);
        true
    }

    /// Ladder point 10.3 and `checkLeaveZoo` (guests 11.1): a guest that is neither leaving nor
    /// on water leaves with the chance `cLeaveChanceDone`. One draw when it can.
    pub(super) fn check_leave_zoo(&mut self) -> bool {
        let Some(k) = crate::guest::keys(self.world, self.ctx.cat, self.at) else {
            return false;
        };
        let wet = self.tile().is_some_and(|t| self.world.is_water(t));
        if self.record().task == LEAVE || wet || !self.ctx.rng.chance(k.leave[3]) {
            return false;
        }
        if let Part::Guest(g) = &mut self.unit.part {
            g.leave = true;
        }
        true
    }

    /// The tile of the zoo entrance.
    fn entrance(&self) -> Option<usize> {
        let (x, y) = self.world.zoo.entrance;
        self.world
            .tile_index(i32::try_from(x).ok()?, i32::try_from(y).ok()?)
    }

    /// 11.6, check done: on the target tile, done if that is the entrance, else the target goes.
    pub(super) fn leave_check_done(&mut self) {
        if self.reached(false) {
            match self.tile() == self.entrance() {
                true => self.record().done = true,
                false => self.record().target.clear(),
            }
        }
    }

    /// 11.6, decide: only a goal without a target tile looks for the exit, and the path is not
    /// emptied first. The exit is a path to the entrance tile that may end short of it (a guest
    /// on the entrance asks like any other) and the walk started; the leave flag goes down when
    /// the destination is the entrance. Without an exit the guest walks at random, not forced.
    pub(super) fn leave_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let entrance = self.entrance();
        if let Some(entrance) = entrance
            && self.path_to(entrance, true)
            && self.start_walk(false, false, true)
        {
            if self.destination_tile() == Some(entrance)
                && let Part::Guest(g) = &mut self.unit.part
            {
                g.leave = false;
            }
            return;
        }
        let (unit, world) = (&mut *self.unit, &mut *self.world);
        if crate::unit::walk::random_walk(unit, world, self.ctx.rng, self.at) {
            self.start_walk(false, false, false);
        }
    }

    /// The tile of the destination the unit holds.
    fn destination_tile(&self) -> Option<usize> {
        let (x, y, _) = self.unit.destination?;
        self.world.tile_index(x.div_euclid(64), y.div_euclid(64))
    }

    /// 11.6, complete: a happy guest may join the endowment (one draw when it is happy enough),
    /// the guest asks for its removal and is counted gone, and the record is cleared.
    pub(super) fn leave_complete(&mut self) {
        let id = self.unit.id;
        let happiness = match &mut self.unit.part {
            Part::Guest(g) => {
                g.leave = false;
                g.values[crate::guest::HAPPINESS]
            }
            _ => 0,
        };
        let units = &mut self.world.units;
        let (above, chance) = units.guest_join;
        if happiness > above && self.ctx.rng.chance(chance) {
            units.members_joined += 1;
        }
        units.guests_gone += 1;
        units.guest_note(format!("leave {id}"));
        self.ctx.remove(self.world, id);
        self.reset();
    }
}

#[cfg(test)]
mod tests;
