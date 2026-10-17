//! Staff (staff 3 to 5): the types the Hire panel lists, what hiring books and where, the monthly
//! cost, and the two numbers a new staff member draws when its first turn starts its state.
//!
//! The keeper class (the zookeeper, the scientist and the marine specialist), the tour guide and
//! the maintenance worker are staff; the helicopter comes with the scenarios.

use crate::build::{cost, number};
use crate::entity::{Catalogue, EntityType};
use crate::money::{GUIDE_WAGES, KEEPER_WAGES, MAINT_WAGES};
use crate::place::bulldozer::type_of;
use crate::unit::{Ctx, Kind, Part, Unit};
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

/// The category the type's monthly wage is booked in (staff 5.2): the keeper class and the
/// helicopter in keeper wages, then the guide and the worker; `None` for a type of no staff class.
pub fn monthly_wages(ty: &EntityType) -> Option<usize> {
    wages(ty).or((ty.class == "helicopter").then_some(KEEPER_WAGES))
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

/// The duty kinds the game allows a maintenance worker (staff 8.1, O6): the trash cans, the litter
/// and dung and the fences, and the tank filters when the aqua expansion data is installed (a
/// marine specialist is in the catalogue).
pub fn duty_kinds(cat: &Catalogue) -> usize {
    let marine = (cat.types.iter()).any(|t| t.class == "keeper" && t.type_name == b"trainer");
    3 + usize::from(marine)
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
    if let Part::Staff(s) = &mut unit.part {
        (s.work, s.chase) = (work, chase);
    }
}

/// A keeper's status tick (staff 4.2): the assigned exhibits with fewer than 2 tiles go.
pub fn checks(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let Part::Staff(s) = &unit.part else {
        return;
    };
    let who = type_of(ctx.cat, &world.zoo.entities[at]);
    let small = |x: &u32| {
        who.is_some_and(|w| assign::block_service(world, ctx.cat, *x, w, (false, false)) == 1)
    };
    let kept: Vec<u32> = s.assigned.iter().copied().filter(|x| !small(x)).collect();
    if let (true, Part::Staff(s)) = (unit.kind == Kind::Keeper, &mut unit.part) {
        s.assigned = kept;
    }
}

/// The keeper `id` stops going to the exhibit of his target tile, which is lowered by one, and
/// closes the gate he remembers (movement 6.2): he is picked up or removed. Only a keeper does.
pub fn leaves(world: &mut World, id: u32) {
    let Some(keeper) = world.units.get_mut(id).filter(|u| u.kind == Kind::Keeper) else {
        return;
    };
    let tile = keeper.goal.record.target.tile;
    keeper.goal.record.target.clear();
    let exhibit = tile.and_then(|t| crate::animal::food::exhibit_of(world, t));
    if let Some(x) = exhibit.and_then(|e| world.exhibits_mut().get_mut(e)) {
        x.rating.coming = x.rating.coming.saturating_sub(1);
    }
    world.units.release_gate_of(id);
}

pub mod assign;
pub mod service;

#[cfg(test)]
mod tests;
