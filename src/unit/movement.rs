//! The move of a unit by the step (movement 3 to 5, 7.2, 8.2): the distance of the step, the
//! walk along the tiles of the path with its corners, the entry of a new tile, the end of a walk
//! and the height above the ground.
//!
//! Kept as the original has it: the remainder of the distance is lost at every step (engine issue
//! 301) and a long diagonal step of a unit that may not go diagonally does not move it (issue 302).
//! Not here yet: the reactions to a gate and to water on entering a tile, the move animation of
//! a speed band (they need the unit's type and the behaviour calls), the units in a tank (the 3D
//! move), the index block of a unit larger than a tile (the map index holds the unit on one tile).

use super::{Kind, Unit};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::path;
use crate::render::order::ground_height;
use crate::world::World;

#[cfg(test)]
mod tests;

/// The eight directions as steps of a tile (movement 2).
const DIR: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// The rate of a guest (7.2): its type's rate less 5 plus its individual `offset` (0 to 10), then
/// 10 % up for an energy of 75 or more and 10 % down for 19 or less, truncated. With `variable`
/// off (`[AI] VariableUnitSpeeds` is 0) the rate as it is.
pub fn guest_speed(rate: u32, offset: u32, energy: i32, variable: bool) -> u32 {
    if !variable {
        return rate;
    }
    let s = f64::from(rate) - 5.0 + f64::from(offset);
    let s = match energy {
        75.. => (s * 1.1) as i32,
        ..=19 => (s * 0.9) as i32,
        _ => s as i32,
    };
    s.max(0) as u32
}

/// Starts a walk along `path` at `speed` (8.2, first run): a path of fewer than two tiles is a
/// unit already in its last tile, which stops at once. Returns whether the unit goes.
pub fn start(unit: &mut Unit, path: Vec<usize>, speed: u32, anim_speed: u16) -> bool {
    if path.len() < 2 {
        stop(unit);
        return false;
    }
    (unit.path, unit.speed, unit.anim_speed, unit.moving) = (path, speed, anim_speed, true);
    true
}

/// Stops the walk: speed 0, no path, not moving.
pub fn stop(unit: &mut Unit) {
    (unit.speed, unit.moving) = (0, false);
    unit.path.clear();
}

/// The tile of a point, if it is on the map.
fn tile_of(world: &World, (x, y): (i32, i32)) -> Option<usize> {
    world.tile_index(x.div_euclid(64), y.div_euclid(64))
}

/// The direction from tile `a` to tile `b` by the signs of the differences, -1 for the same tile.
fn direction(world: &World, a: usize, b: usize) -> i32 {
    let width = world.zoo.width as usize;
    let d = |a: usize, b: usize| (b as i32 - a as i32).signum();
    let step = (d(a % width, b % width), d(a / width, b / width));
    DIR.iter().position(|&s| s == step).map_or(-1, |i| i as i32)
}

/// The margin of the corners: 20 movement units, 5 for a unit whose footprint covers more than
/// four cells (2).
fn margin(world: &World, at: usize) -> i32 {
    match world
        .shape(at)
        .is_some_and(|s| s.footprint.0 * s.footprint.1 > 4)
    {
        true => 5,
        false => 20,
    }
}

/// 4.3: the direction a unit that travelled in `travel` takes towards `wanted` at the sub-tile
/// point `(px, py)`: a right-angle turn slides diagonally until the unit is `m` units inside the
/// tile it should follow.
fn turning(travel: i32, wanted: i32, (px, py): (i32, i32), m: i32) -> i32 {
    if wanted % 2 != 0 {
        return wanted;
    }
    let side = |c: i32, low: i32, mid: i32, high: i32| match c {
        c if c > 64 - m => high,
        c if c >= m => mid,
        _ => low,
    };
    match (travel, wanted) {
        (0 | 4, 2) => side(py, 3, 2, 1),
        (0 | 4, 6) => side(py, 5, 6, 7),
        (2 | 6, 0) => side(px, 1, 0, 7),
        (2 | 6, 4) => side(px, 3, 4, 5),
        _ => wanted,
    }
}

/// 4.4: the displacement of a step of `distance` in direction `dir`. The shortening of a long
/// diagonal step of a unit that may not go diagonally multiplies the direction by a factor the
/// original truncates to 0: such a step does not move (issue 302).
fn displacement(dir: i32, distance: i32, diagonals: bool, m: i32) -> (i32, i32) {
    let Some(&(ux, uy)) = usize::try_from(dir).ok().and_then(|d| DIR.get(d)) else {
        return (0, 0);
    };
    if dir % 2 == 1 && !diagonals && distance >= (32 - m) * 2 {
        return (0, 0);
    }
    (ux * distance, uy * distance)
}

/// The move of one step of `ms` milliseconds for the unit at position `at` of the entity list
/// (3), then the check of its height (5).
pub fn step(unit: &mut Unit, world: &mut World, ms: u32, at: usize) {
    if unit.moving && unit.speed > 0 {
        // The remainder of the division is lost (issue 301).
        let distance = (unit.speed.wrapping_mul(ms) / 1000).max(1);
        if progress(unit, world, distance, at) {
            unit.through_building = false;
        }
    }
    snap(world, at);
}

/// The end of the move call (8.2 step 3), which runs after the move in the same step: a walk
/// ends when the path is empty or the unit stands in its destination tile.
pub fn finish(unit: &mut Unit, world: &World, at: usize) {
    if !unit.moving {
        return;
    }
    let here = point(world, at).and_then(|(x, y, _)| tile_of(world, (x, y)));
    let target = unit
        .destination
        .and_then(|(x, y, _)| tile_of(world, (x, y)));
    if unit.path.is_empty() || here.is_some() && here == target {
        stop(unit);
    }
}

fn point(world: &World, at: usize) -> Option<(i32, i32, i32)> {
    let c = world.zoo.entities.get(at)?.common.as_ref()?;
    Some((c.x, c.y, c.z))
}

/// A walking unit on the ground stands on it (5): its height is the ground's at its point. Not
/// done in a tank, where the unit swims.
fn snap(world: &mut World, at: usize) {
    let Some(c) = world.zoo.entities.get(at).and_then(|e| e.common.as_ref()) else {
        return;
    };
    let (x, y, rotation) = (c.x, c.y, c.rotation);
    let Some(t) = tile_of(world, (x, y)) else {
        return;
    };
    if c.movement_mode == 3 || world.tank_at(t).is_some() {
        return;
    }
    let ground = ground_height(&world.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64));
    world.place_unit(at, (x, y, ground), rotation);
}

/// Whether the tile step from `here` to `next` is still allowed (4.7).
fn allowed(unit: &Unit, world: &World, at: usize, here: usize, next: usize) -> bool {
    let Ok(d) = usize::try_from(direction(world, here, next)) else {
        return false;
    };
    if world.neighbour(here, d) != Some(next) {
        return false;
    }
    let mover = mover(unit, world, at, here);
    path::allowed(
        world,
        &mover,
        &world.units.finder.settings,
        (here, next, d),
        unit.through_building,
    )
}

/// The unit as the costs of the path finder see it, standing on tile `here`.
pub(super) fn mover(unit: &Unit, world: &World, at: usize, here: usize) -> path::Mover {
    let mut m = unit.mover.clone();
    m.at = Some(here);
    m.footprint = world
        .shape(at)
        .map_or(1, |s| (s.footprint.0 * s.footprint.1).clamp(1, 4) as u32);
    m
}

/// 4.2: one step of the walk along the path by `distance`; true when the move is done.
fn progress(unit: &mut Unit, world: &mut World, mut distance: u32, at: usize) -> bool {
    if distance > 64 {
        distance = 16;
    }
    if unit.path.is_empty() {
        return true;
    }
    let Some((x, y, z)) = point(world, at) else {
        return true;
    };
    let here = tile_of(world, (x, y));
    let dest = unit
        .destination
        .and_then(|(x, y, _)| tile_of(world, (x, y)));
    let (Some(here), Some(dest)) = (here, dest) else {
        unit.path.clear();
        return true;
    };
    let first = unit.path[0];
    if first >= world.zoo.tiles.len() {
        unit.path.clear();
        return true;
    }
    let arrived = here == dest;
    // A guest goes diagonally only in a tank (4.3).
    let diagonals = world.units.finder.settings.diagonal
        && (unit.kind != Kind::Guest || world.tank_at(here).is_some());
    let wanted = if arrived {
        unit.travel
    } else if diagonals && here != first {
        direction(world, here, first)
    } else if unit.path.len() == 1 {
        -1
    } else {
        let next = unit.path[1];
        if !(next < world.zoo.tiles.len() && allowed(unit, world, at, here, next)) {
            if !std::mem::replace(&mut unit.aborted, true) {
                world.units.lines.push(format!("abort {}", unit.id));
            }
            return true;
        }
        direction(world, here, next)
    };
    let m = margin(world, at);
    let dir = turning(unit.travel, wanted, (x.rem_euclid(64), y.rem_euclid(64)), m);
    let (dx, dy) = displacement(dir, distance as i32, diagonals, m);
    let (nx, ny) = (x + dx, y + dy);
    let entered = tile_of(world, (nx, ny));
    // The unit does not leave its last tile.
    if entered != Some(here) && (unit.path.len() == 1 || entered.is_none()) {
        unit.path.clear();
        return true;
    }
    let old = world.zoo.entities[at]
        .common
        .as_ref()
        .map_or(0, |c| c.rotation);
    let facing = u32::try_from(dir).unwrap_or(old);
    world.place_unit(at, (nx, ny, z), facing);
    if facing != old {
        world.units.lines.push(format!("face {} {facing}", unit.id));
    }
    if let Some(new) = entered.filter(|&n| n != here) {
        enter(unit, world, here, new);
    }
    if arrived {
        unit.path.clear();
    }
    arrived
}

/// 4.5: the unit crossed from tile `old` into tile `new`: its travel direction, the old tile off
/// the path, and the exhibit it entered, if it is another one.
fn enter(unit: &mut Unit, world: &mut World, old: usize, new: usize) {
    unit.travel = direction(world, old, new);
    if unit.path.first() == Some(&old) {
        unit.path.remove(0);
    }
    let width = world.zoo.width as usize;
    world
        .units
        .lines
        .push(format!("tile {} {},{}", unit.id, new % width, new / width));
    let (before, after) = (world.exhibits().region(old), world.exhibits().region(new));
    if let Some(r) = after.filter(|&r| r != GROUNDS && r != EXTERIOR && after != before) {
        world.units.lines.push(format!("enter {} {r}", unit.id));
    }
}
