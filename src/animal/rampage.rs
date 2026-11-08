//! The rampage's building target (animals 11.11 points 4 and 5): the call `fTargetBuilding` of
//! the rampage sets, which sends a rampaging animal to the nearest building that serves the need
//! `building` and ends in a fast move there (the unit runs that move after this choice).

use super::NEED_BUILDING;
use crate::behaviour::{Data, Flow};
use crate::building::{self, Species};
use crate::goal::{Cx, TARGET_BUILDING};
use crate::unit::{Ctx, Part, Unit, movement};
use crate::world::World;

/// The radius of the search, in tiles (guests 7.1).
const RADIUS: i64 = 46_340;

/// The call, each time its set runs it (11.11 point 4). `Some(flow)` is the call's answer: done
/// at once when the animal does not rampage or finds no target (its flag is down then), failed
/// for a unit that is not an animal. `None`: the animal has its target (the flag is up) and the
/// call goes on as the move to it.
pub(crate) fn target(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
) -> Option<Flow> {
    let Part::Animal(a) = &mut unit.part else {
        return Some(Flow::Failed);
    };
    if !a.rampaging {
        a.rampage_target = false;
        return Some(Flow::Done);
    }
    if a.rampage_target {
        return None;
    }
    let found = choose(unit, world, ctx, at, data);
    if let Part::Animal(a) = &mut unit.part {
        a.rampage_target = found;
    }
    (!found).then_some(Flow::Done)
}

/// The target choice (point 5): the nearest `building` satisfier with an open slot for the
/// animal, one draw among the ties, then a path to its tile. Found, the animal heads for it:
/// destination and path, building target, task 0x7b with mode 1, the need, and the building's
/// audience put out when it is a grandstand.
fn choose(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) -> bool {
    let Some(here) = movement::here(world, at) else {
        return false;
    };
    let egg = matches!(&unit.part, Part::Animal(a) if a.egg);
    let who = Species::of(ctx.cat, &world.zoo.entities[at], egg);
    let ties = building::satisfiers(world, ctx.cat, here, "building", RADIUS, who.as_ref()).1;
    if ties.is_empty() {
        return false;
    }
    let id = ties[ctx.rng.pick(ties.len() as u32) as usize];
    let tile = world.find(id).and_then(|b| {
        let c = world.zoo.entities[b].common.as_ref()?;
        world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
    });
    let cat = ctx.cat;
    let mut cx = Cx {
        unit,
        world,
        ctx,
        at,
        data,
    };
    let Some(tile) = tile.filter(|&t| cx.path_to(t, false)) else {
        return false;
    };
    if let Part::Animal(a) = &mut cx.unit.part {
        (a.building, a.need) = (id, NEED_BUILDING);
    }
    cx.set_task(TARGET_BUILDING, 1, 0, Some(tile));
    crate::show::empty_grandstands_for(world, cat, unit, id);
    true
}
