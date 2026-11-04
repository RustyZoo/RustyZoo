//! What an animal does with the units its escape check sees (animals 11.7, 20.5, shows 30.2 point
//! 4.5). An animal in a show runs no escape check, so it never gets here.

use crate::behaviour::Data;
use crate::goal::Cx;
use crate::unit::{Ctx, Unit};
use crate::world::World;

/// The predator, playmate and prey searches that follow a firing of the escape check; the rules are
/// [`Cx::escape_dispatch`]'s, which reads the task as it was before this check.
pub(super) fn run(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let task = unit.goal.record.task;
    let mut cx = Cx {
        unit,
        world,
        ctx,
        at,
        data,
    };
    cx.escape_dispatch(task);
}
