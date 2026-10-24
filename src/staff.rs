//! Staff (staff 3 to 5): the types the Hire panel lists, what hiring books and where, the monthly
//! cost, and the colours and counters a new staff member draws when it is created (the pending one
//! when an item of the Hire panel is chosen, the staff of a loaded map at the load); a keeper's
//! status tick and what leaving does to the exhibit he was going to.
//!
//! The keeper class (the zookeeper, the scientist and the marine specialist), the tour guide and
//! the maintenance worker are staff; the helicopter comes with the scenarios.

use crate::behaviour::Data;
use crate::build::{cost, number};
use crate::entity::{Catalogue, EntityType};
use crate::money::{GUIDE_WAGES, KEEPER_WAGES, MAINT_WAGES};
use crate::place::bulldozer::type_of;
use crate::sim::Rng;
use crate::unit::{Ctx, Kind, Part, Staff, Unit};
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

/// The keys of a tour guide type (tour guide 3) that its group reads, with the program's value
/// for a missing key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crowd {
    /// `cTourGuideBonus`, the happiness a viewing area under the guide gives.
    pub bonus: i32,
    /// `cCrowdCheck`: status ticks between two gatherings.
    pub check: i32,
    /// `cCrowdRadius`: half side of the square the guide gathers from, in tiles.
    pub radius: i32,
    /// `cFollowChance` in per cent and `cMaxGroupSize`.
    pub chance: i32,
    pub max: i32,
}

impl Crowd {
    pub fn of(ty: Option<&EntityType>) -> Crowd {
        let key =
            |k: &str, fallback: i32| ty.and_then(|t| number(t, k)).map_or(fallback, |v| v as i32);
        Crowd {
            bonus: key("cTourGuideBonus", 5),
            check: key("cCrowdCheck", 10),
            radius: key("cCrowdRadius", 2),
            chance: key("cFollowChance", 50),
            max: key("cMaxGroupSize", 12),
        }
    }
}

/// `rand % n` of a counter key, as the C runtime reduces it: -1 without a draw when the key is 0.
fn counter(ty: Option<&EntityType>, key: &str, fallback: f32, rng: &mut Rng) -> i32 {
    let n = ty.and_then(|t| number(t, key)).unwrap_or(fallback) as i32;
    match n {
        0 => -1,
        n => rng.pick(n.unsigned_abs()) as i32,
    }
}

/// The duty kinds the game allows a maintenance worker (staff 8.1, O6): the trash cans, the litter
/// and dung and the fences, and the tank filters when the aqua expansion data is installed (a
/// marine specialist is in the catalogue).
pub fn duty_kinds(cat: &Catalogue) -> usize {
    let marine = (cat.types.iter()).any(|t| t.class == "keeper" && t.type_name == b"trainer");
    3 + usize::from(marine)
}

/// The replaceable colours of every shipped staff type (hair and skin), one draw each at creation.
const COLOURS: usize = 2;

/// The creation of a staff member of type `ty` and kind `kind`: one draw per replaceable colour,
/// the work counter and then the chase counter, one draw each (staff 4.2), and a tour guide's crowd
/// countdown (tour guide 4). The counters are kept; only the guide reads them.
pub fn create(ty: Option<&EntityType>, kind: Kind, rng: &mut Rng) -> Staff {
    for _ in 0..COLOURS {
        rng.rand();
    }
    let work = counter(ty, "cWorkCheck", 5.0, rng);
    let chase = counter(ty, "cChaseCheck", 2.0, rng);
    let crowd = match kind {
        Kind::Guide => counter(ty, "cCrowdCheck", 10.0, rng),
        _ => 0,
    };
    // A worker's countdown to the clearing of his unreachable lists (staff 4.2).
    let clear = ty.and_then(|t| number(t, "cClearInvalidListInterval"));
    let clear = (kind == Kind::Worker).then_some(clear.map_or(10_000, |n| n as i32));
    Staff {
        work,
        chase,
        crowd,
        clear: clear.unwrap_or(0),
        ..Staff::default()
    }
}

/// Creates the staff member of the entity at position `at` of the entity list, as the loader of a
/// map does (staff 4.2; the `sim` command hires through it too): the draws of [`create`], in file
/// order with the animals and the guests.
/// Nothing for an entity that is not staff. The unit counts as started, so its first turn draws
/// nothing.
pub fn make(world: &mut World, cat: &Catalogue, rng: &mut Rng, at: usize) {
    let Some(entity) = world.zoo.entities.get(at) else {
        return;
    };
    let kind = Kind::of_class(&entity.class).filter(|k| !matches!(k, Kind::Animal | Kind::Guest));
    let (Some(kind), Some(common)) = (kind, entity.common.as_ref()) else {
        return;
    };
    let (id, staff) = (common.id, create(type_of(cat, entity), kind, rng));
    if let Some(unit) = world.units.get_mut(id) {
        unit.part = Part::Staff(staff);
        unit.started = true;
    }
}

/// The first turn of a staff member that was not created by [`make`] or the Hire panel (one a test
/// world added): the same draws as [`create`].
pub fn start(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    if !matches!(unit.part, Part::Staff(_)) {
        return;
    }
    let ty = type_of(ctx.cat, &world.zoo.entities[at]);
    let new = create(ty, unit.kind, ctx.rng);
    if let Part::Staff(s) = &mut unit.part {
        (s.work, s.chase, s.crowd, s.members, s.clear) =
            (new.work, new.chase, new.crowd, Vec::new(), new.clear);
    }
}

/// A staff member's status tick: a guide's gathering countdown (tour guide 4); a keeper's
/// assigned exhibits with fewer than 2 tiles go (staff 4.2).
pub fn checks(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    if unit.kind == Kind::Guide {
        return crate::goal::crowd(unit, world, ctx, at, data);
    }
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

/// The keeper `id` stops going to the exhibit of his own target tile, which is lowered by one, and
/// closes the gate he remembers (movement 6.2): he is picked up or removed. A guide lets its group
/// go, a worker gives his task back.
pub fn leaves(world: &mut World, id: u32) {
    // A guide picked up or removed lets its group go: the guests notice at their next update.
    if let Some(Part::Staff(s)) = (world.units.get_mut(id))
        .filter(|u| u.kind == Kind::Guide)
        .map(|u| &mut u.part)
    {
        (s.members, s.leg) = (Vec::new(), false);
    }
    // A worker removed or picked up gives his task back (staff 8.9, O8).
    if let Some(worker) = world.units.get_mut(id).filter(|u| u.kind == Kind::Worker) {
        crate::goal::lose_task(worker);
        world.units.pool.unlink(id);
    }
    let Some(keeper) = world.units.get_mut(id).filter(|u| u.kind == Kind::Keeper) else {
        world.units.release_gate_of(id);
        return;
    };
    let (job, held) = (keeper.goal.record.job, keeper.goal.record.job_target.id);
    let tile = match &mut keeper.part {
        Part::Staff(s) => s.target.take(),
        _ => None,
    };
    // The animal he was to heal waits no more.
    if let (3 | 4, Some(a)) = (job, world.units.get_mut(held)) {
        a.wait = 0;
    }
    let exhibit = tile.and_then(|t| crate::animal::food::exhibit_of(world, t));
    if let Some(x) = exhibit.and_then(|e| world.exhibits_mut().get_mut(e)) {
        x.rating.coming = x.rating.coming.saturating_sub(1);
    }
    world.units.release_gate_of(id);
}

pub mod assign;
pub mod chores;
pub mod ladder;
pub mod pool;
pub mod service;

#[cfg(test)]
mod tests;
