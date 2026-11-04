//! What an animal does with the units its escape check sees (animals 11.7, shows 30.2 point 4.5).
//! An animal in a show runs no escape check, so it never gets here.

use crate::show::abort_show_for;
use crate::unit::{Ctx, Unit, predator};
use crate::world::World;

/// The predator search that follows a firing of the escape check: a threat seen while the animal is
/// not afraid aborts the show of its show id, then raises its afraid flag; none seen lowers the
/// flag and forgets the threat. The playmate and prey searches wait for the chase.
pub(super) fn run(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    if predator::find(world, ctx.cat, ctx.removals, unit, at, 0) == 0 {
        (unit.afraid, unit.predator) = (false, 0);
    } else if !unit.afraid {
        let show = unit.show;
        abort_show_for(world, ctx.cat, unit, show);
        unit.afraid = true;
    }
}
