//! The entry points the other modules call to run a piece of a unit's goals on the unit taken out
//! of the table: the send of an animal, the calls of a use set and a guide's status tick.

use super::Cx;
use crate::behaviour::Data;
use crate::unit::{Ctx, Unit};
use crate::world::World;

/// Tank fauna 5: the animal at position `at` is sent to tile `to`.
pub(crate) fn send(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    to: usize,
) {
    Cx {
        unit,
        world,
        ctx,
        at,
        data,
    }
    .send_to(to);
}

/// The animal at position `at` runs `fKillPrey` when `kill`, else `fCaughtPrey` (animals 21.5).
pub(crate) fn meal(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    kill: bool,
) {
    let mut cx = Cx {
        unit,
        world,
        ctx,
        at,
        data,
    };
    match kill {
        true => cx.kill_prey(),
        false => cx.caught_prey(),
    }
}

/// A use set reaches the building (guest buildings 6.4.1): `fEnterBuilding` when `enter`, else
/// `fExitBuilding`.
pub(crate) fn use_call(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    enter: bool,
) {
    let mut cx = Cx {
        unit,
        world,
        ctx,
        at,
        data,
    };
    match enter {
        true => cx.set_enters(),
        false => cx.set_exits(),
    }
}

/// A guide's status tick: the countdown of its gathering (tour guide 4).
pub(crate) fn crowd(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    Cx {
        unit,
        world,
        ctx,
        at,
        data,
    }
    .crowd_check();
}
