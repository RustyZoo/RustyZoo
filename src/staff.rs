//! Staff (staff 3 to 5): the types the Hire panel lists, what hiring books and where, the monthly
//! cost, and the two numbers a new staff member draws when its first turn starts its state.
//!
//! The keeper class (the zookeeper, the scientist and the marine specialist), the tour guide and
//! the maintenance worker are staff; the helicopter comes with the scenarios.

use crate::build::{cost, number};
use crate::entity::EntityType;
use crate::money::{GUIDE_WAGES, KEEPER_WAGES, MAINT_WAGES};
use crate::place::bulldozer::type_of;
use crate::unit::{Ctx, Part, Unit};
use crate::world::World;

/// Whether the type is a staff member the player hires.
pub fn is_staff(ty: &EntityType) -> bool {
    wages(ty).is_some()
}

/// The category the type's hiring charge is booked in (staff 5.1), `None` for a type that is not
/// staff.
pub fn wages(ty: &EntityType) -> Option<usize> {
    match ty.class {
        "keeper" => Some(KEEPER_WAGES),
        "tour" => Some(GUIDE_WAGES),
        "maint" => Some(MAINT_WAGES),
        _ => None,
    }
}

/// The monthly cost of the type: `cMonthlyCost`, which is `cPurchaseCost` when the file has none
/// (staff 3).
pub fn monthly(ty: &EntityType) -> f32 {
    number(ty, "cMonthlyCost").unwrap_or_else(|| cost(ty))
}

/// The string of the duties the Hire panel shows, 0 when the type has none.
pub fn duties(ty: &EntityType) -> i32 {
    number(ty, "cDutiesTextID").map_or(0, |n| n as i32)
}

/// `rand % n` of a counter key, as the C runtime reduces it: -1 without a draw when the key is 0.
fn counter(ty: Option<&EntityType>, key: &str, fallback: f32, ctx: &mut Ctx) -> i32 {
    let n = ty.and_then(|t| number(t, key)).unwrap_or(fallback) as i32;
    match n {
        0 => -1,
        n => ctx.rng.pick(n.unsigned_abs()) as i32,
    }
}

/// The first turn of a staff member: the work counter and then the chase counter, one draw each
/// (staff 4.2). The values are kept; nothing reads them yet.
pub fn start(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    if !matches!(unit.part, Part::Staff(_)) {
        return;
    }
    let ty = type_of(ctx.cat, &world.zoo.entities[at]);
    let work = counter(ty, "cWorkCheck", 5.0, ctx);
    let chase = counter(ty, "cChaseCheck", 2.0, ctx);
    unit.part = Part::Staff(crate::unit::Staff { work, chase });
}

#[cfg(test)]
mod tests;
