//! Return buildings (guest buildings 12.2): the guest goes in by a normal slot, comes out on the
//! water, rides about ([`crate::goal`], task 0x82) and comes back in by the return slot. What the
//! guest keeps is the type's name id (`Guest::returning`) and the set name of its walks
//! (`Guest::return_set`), set at its first leave and cleared at the return leave; the building
//! keeps the guests it let out ([`Live::members`], for a `cUserTracker` type) and its water
//! capacity.

use super::{Kind, Live, Species};
use crate::unit::{Part, Unit};
use crate::world::World;
use crate::zoo::Entity;

/// What a type declares of a return building (guest buildings 12.2): `cReturnBuilding`,
/// `cUserTracker`, `cStrictBuildingCapacity` (an integer), `cIdler` and `cExhibitViewer`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Keys {
    pub building: bool,
    pub tracker: bool,
    pub strict: i32,
    pub idler: bool,
    pub viewer: bool,
}

/// The size `N` of the water area behind the building at position `at` (12.2 point 13, open point
/// 11): the tiles of water reached from the exit tile of the first slot that exits on water, over
/// the sides that hold no fence, through tiles in the zoo with no object in any quarter. 0 when
/// that tile is not such a tile, or the building's own facing edge is fenced.
pub(crate) fn area(world: &World, at: usize, kind: &Kind) -> usize {
    let e = &world.zoo.entities[at];
    let Some(exit) = (kind.slots.iter()).find_map(|s| s.exit.filter(|_| s.exit_water)) else {
        return 0;
    };
    let (x, y) = super::place(e, exit);
    let free = |t: usize| world.is_water(t) && (0..4).all(|q| world.quadrant(t, q).is_none());
    let Some(start) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
        return 0;
    };
    let facing = e.common.as_ref().map_or(1, |c| c.rotation as usize % 8);
    let own = world.footprint(at).map_or_else(Vec::new, |f| f.1);
    let behind = facing.is_multiple_of(2)
        && world
            .neighbour(start, facing)
            .is_some_and(|n| own.contains(&n))
        && world.closed(start, facing);
    if !free(start) || behind {
        return 0;
    }
    let mut found = vec![start];
    let mut next = 0;
    while let Some(&t) = found.get(next) {
        next += 1;
        for d in [0, 2, 4, 6] {
            let n = world.neighbour(t, d);
            if let Some(n) = n.filter(|&n| world.in_zoo(n) && free(n) && !world.closed(t, d))
                && !found.contains(&n)
            {
                found.push(n);
            }
        }
    }
    found.len()
}

/// The legs of a ride (goals 11.13 point 2), and the water capacity without the strict override.
pub(crate) fn legs(world: &World, at: usize, kind: &Kind) -> i32 {
    ((area(world, at, kind) + 7) / 10).min(10) as i32
}

/// The water capacity `W` of the building at position `at` (12.2 point 13).
pub(crate) fn capacity(world: &World, at: usize, kind: &Kind) -> u32 {
    match kind.ret.strict {
        s if s >= 1 => s as u32,
        _ => legs(world, at, kind) as u32,
    }
}

/// Guest buildings 6.1 point 9: a guest that is not returning is refused when the members reach
/// the water capacity, or the users now plus one pass a strict capacity; a returning guest only
/// when the type is not the one it left, or, for a tracker, when it is not a member. Any other
/// unit, and any other type, passes.
pub(crate) fn may_enter(
    world: &World,
    e: &Entity,
    kind: &Kind,
    live: Option<&Live>,
    who: Option<&Species>,
) -> bool {
    let Some(w) = who.filter(|w| kind.ret.building && !w.animal) else {
        return true;
    };
    if let Some(name) = w.returning {
        let member = live.is_some_and(|l| l.members.contains(&w.unit));
        return name == kind.name_id && (!kind.ret.tracker || member);
    }
    let users = live.map_or(0, |l| l.users_now() as usize);
    let water = match live.map_or(0, |l| l.water) {
        0 => (e.common.as_ref())
            .and_then(|c| world.find(c.id))
            .map_or(0, |at| capacity(world, at, kind)),
        n => n,
    };
    let held = live.map_or(0, |l| l.members.len());
    held < water as usize && (kind.ret.strict == 0 || (users as i64) < kind.ret.strict.into())
}

/// Guest buildings 12.2 points 5 to 7: the guest `unit` has left building `id` (entity `e`, type
/// `kind`). A first leave by the normal slot sets its return name id and its set name (`bReturn`,
/// the type's key, the building's upgrade index), lists a tracked guest as a member and computes
/// the water capacity again; the return leave and a forced leave clear the two fields. Only a
/// guest and a return building.
pub(crate) fn left(
    world: &mut World,
    unit: &mut Unit,
    id: u32,
    e: &Entity,
    kind: &Kind,
    forced: bool,
) {
    let Part::Guest(g) = &mut unit.part else {
        return;
    };
    if !kind.ret.building {
        return;
    }
    if forced || g.returning.is_some() {
        (g.returning, g.return_set) = (None, None);
        return;
    }
    let index = world.units.upgrade_index.get(&id).copied().unwrap_or(0);
    g.returning = Some(kind.name_id).filter(|&n| n != 0);
    g.return_set = Some(format!("bReturn{}{index}", e.subtype));
    let water = world.find(id).map_or(0, |at| capacity(world, at, kind));
    let live = (world.units.buildings.entry(id)).or_insert_with(|| Live::new(kind));
    if kind.ret.tracker {
        live.members.push(unit.id);
    }
    live.water = water;
}

/// The guest `unit` leaves the member list of every building (unit placing 8.5 point 1).
pub(crate) fn forget(world: &mut World, unit: u32) {
    for live in world.units.buildings.values_mut() {
        live.members.retain(|&m| m != unit);
    }
}
