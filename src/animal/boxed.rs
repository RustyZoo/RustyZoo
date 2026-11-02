//! The crate (animals 7, unit placing 9): an escaped animal the player picks up is crated, and the
//! crate's own check opens it in an exhibit or shakes it. The crate's pictures are not drawn yet.

use super::{Animal, fire};
use crate::behaviour::{Animation, Runner};
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::place::{Class, snap as grid, unit_footprint};
use crate::unit::{Ctx, Part, Unit, movement, swim};
use crate::world::World;

/// Crates the animal (unit placing 9.1): it is no longer escaped, and the status reset drops its
/// escape walk and its need flags.
pub fn crate_up(a: &mut Animal) {
    a.crated = true;
    a.escaped = false;
    (a.walk, a.drinking, a.wants_food, a.thirsty) = (None, false, false, false);
}

/// Snaps the unit at `at` to the tile grid where it stands (unit placing 9.1, 9.2): the position
/// of `placing.md` 4.2 for its footprint now (turned by the facing), the crate's while crated, and the ground under it.
pub fn snap(world: &mut World, cat: &Catalogue, at: usize) {
    let Some(shape) = world.shape(at) else {
        return;
    };
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return;
    };
    let (point, z, rotation) = ((c.x, c.y), c.z, c.rotation);
    let (fx, fy) = unit_footprint(world, cat, at, shape);
    // The footprint is turned by the facing (unit placing 9.1.1).
    let fp = if rotation % 4 == 0 {
        (fx, fy)
    } else {
        (fy, fx)
    };
    let (x, y) = grid(Class::Fixed, point, fp, 0);
    world.place_unit(at, (x, y, z), rotation);
    movement::snap(world, at);
}

/// The crate's own check (unit placing 9.2), first of the status checks and only for a crated
/// animal: every `cBoxedCheck` ticks the crate opens where it stands in an exhibit (a tank holding
/// water, else a land exhibit for a species that does not live only under water); otherwise one
/// draw queues `bRattle` with the chance `cRattleChance`.
pub fn check(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let period = world.units.class.boxed;
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    if !a.crated || !fire(&mut a.crate_count, period) {
        return;
    }
    let tile = (world.zoo.entities[at].common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let exhibit = (tile.and_then(|t| world.exhibits().region(t)))
        .filter(|&r| r != GROUNDS && r != EXTERIOR)
        .and_then(|x| world.exhibits().get(x));
    let opens = exhibit.is_some_and(|e| match e.tank {
        Some(t) => t.filled && t.level >= 1,
        None => !unit.mover.only_underwater,
    });
    if !opens {
        if ctx.rng.chance(world.units.class.rattle) {
            unit.runner.queue_if_new("bRattle", false, true);
        }
        return;
    }
    // Opened, it stops everything and is put on the map again as a placed animal.
    a.crated = false;
    unit.runner = Runner::default();
    unit.animation = Animation::default();
    (unit.moving, unit.destination) = (false, None);
    unit.path.clear();
    snap(world, ctx.cat, at);
    swim::check(unit, world, at);
    swim::add_to_map(unit, world, at);
    if let Some(t) = tile {
        world.mark_rating(t);
    }
    world.units.tell(format!("open {}", unit.id));
}
