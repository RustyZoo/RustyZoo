//! The units in a tank (tank fauna): the tank position check that sets the tank mode and the move
//! kind after every move step (4), the terrain mode of an animal (5), the level a destination in
//! a tank gets (6.2), the 3D move along a path of (tile, level) nodes (8) and the swimmer's
//! animation names (9).
//!
//! Kept as the original has it: the segment of the 3D move is as long as its truncated true
//! length, so a flat diagonal step of a tile takes 90 units. Not here yet: the ladder and the
//! platform (the marine specialist's), the water mode destination, the amphibious neighbours'
//! links, and objects that block only some levels of a tank (7.3).

use super::{Kind, Part, Unit, movement};
use crate::behaviour::{Place, Target};
use crate::place::unit::gate_inside;
use crate::render::order::ground_height;
use crate::sim::Rng;
use crate::world::World;

#[cfg(test)]
mod tests;

/// The segment of the 3D move from one node to the next: where it started, its extent in
/// movement units, its truncated length and the distance gone along it, and the cell it goes to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Segment {
    to: (i32, i32, i32),
    start: (i32, i32, i32),
    delta: (i32, i32, i32),
    length: i32,
    travelled: i32,
}

/// The tank test: the type lives in tanks (`cUnderwater` 1) and the unit is not an egg.
fn diver(unit: &Unit) -> bool {
    unit.mover.underwater && !matches!(&unit.part, Part::Animal(a) if a.egg)
}

/// 4: the tank mode and the move kind of the unit at position `at` of the entity list from the
/// tank it stands in, its height and its type. A unit in a tank with water is under water (4)
/// below the surface, unless its type stays at the surface, else it is put on the surface (3); a
/// dry tank holds it on the floor. A change of mode restarts its animation.
pub fn check(unit: &mut Unit, world: &mut World, at: usize) {
    let Some(c) = world.zoo.entities.get(at).and_then(|e| e.common.as_ref()) else {
        return;
    };
    let (x, y, z0, rotation) = (c.x, c.y, c.z, c.rotation);
    let tile = world.tile_index(x.div_euclid(64), y.div_euclid(64));
    let tank = tile.and_then(|t| {
        let id = world.tank_at(t)?;
        Some((t, id, world.tank(id)?))
    });
    let only = unit.mover.only_underwater;
    let (kind, mode, z) = match tank {
        None => (2, if only { 4 } else { 0 }, z0),
        Some((t, id, k)) => {
            let ground = ground_height(&world.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64));
            let (height, level) = (16 * k.steps().0, 16 * k.steps().1);
            // The tile inside the gate keeps a unit on the platform it is on.
            let platform = gate_inside(world, id) == Some(t)
                && (unit.tank == 2
                    || unit.kind != Kind::Animal && z0.saturating_sub(ground) == height);
            let (mode, h) = if platform {
                (2, height)
            } else if level < 16 {
                (if only { 4 } else { 0 }, 0)
            } else if z0.saturating_sub(ground) < level && !unit.mover.surface {
                (4, z0.saturating_sub(ground))
            } else {
                (3, level)
            };
            (3, mode, ground + h)
        }
    };
    if let Some(c) = world.zoo.entities[at].common.as_mut() {
        c.movement_mode = kind;
    }
    if z != z0 {
        world.place_unit(at, (x, y, z), rotation);
    }
    if mode != unit.tank {
        unit.tank = mode;
        // A moving unit goes level, and its move or idle animation is picked again.
        unit.vertical = 0;
        unit.flip = true;
    }
}

/// The unit `id`, just put on the map, takes the tank mode of where it stands and, for an animal,
/// its terrain mode (4, 5); the world moves it on at its next step.
pub fn put(world: &mut World, id: u32) {
    let (Some(at), Some(slot)) = (world.find(id), world.units.get_mut(id)) else {
        return;
    };
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    check(&mut unit, world, at);
    add_to_map(&mut unit, world, at);
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
}

/// 9.4: where the unit is for its ambient table: under water or at the surface of a tank for a
/// diver, on water for a swimmer, else on land.
pub fn place(unit: &Unit, world: &World, at: usize) -> Place {
    let wet = (world.zoo.entities.get(at).and_then(|e| e.common.as_ref()))
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)))
        .is_some_and(|t| world.is_water(t));
    let egg = matches!(&unit.part, Part::Animal(a) if a.egg);
    match (diver(unit), unit.tank) {
        (true, 4) => Place::Under,
        (true, 3) => Place::Surface,
        _ if unit.mover.swims && !egg && wet => Place::Water,
        _ => Place::Land,
    }
}

/// 9.1: the idle animation of the unit by its tank mode.
pub fn idle_name(unit: &Unit) -> &'static str {
    match (diver(unit), unit.tank) {
        (true, 4) => "subidle",
        (true, 3) => "surfidle",
        _ => "idle",
    }
}

/// 9.2: the move animation a unit in a tank shows, whatever the speed band; none out of one.
pub fn move_name(unit: &Unit) -> Option<&'static str> {
    match (diver(unit), unit.tank, unit.vertical) {
        (true, 4, -1) => Some("dive"),
        (true, 4, 1) => Some("rise"),
        (true, 4, _) => Some("subswim"),
        (true, 3, _) => Some("surfswim"),
        _ => None,
    }
}

/// 5: the terrain mode of an animal (0 land, 1 water, 2 tank), set without sending the animal
/// anywhere. A species that lives only under water is always in the tank mode.
pub fn set_terrain(unit: &mut Unit, mode: u8) {
    let only = unit.mover.only_underwater;
    if let Part::Animal(a) = &mut unit.part {
        a.terrain = if only { 2 } else { mode };
    }
}

/// 5: the terrain mode an animal gets where it is put on the map (a placement, a drop, the first
/// turn): water for a swimmer on a water tile, the tank mode for a diver on a tank tile, else
/// land; an animal at its minimum health keeps its mode on water or a tank tile.
pub fn add_to_map(unit: &mut Unit, world: &World, at: usize) {
    let Part::Animal(a) = &unit.part else {
        return;
    };
    let tile = (world.zoo.entities.get(at).and_then(|e| e.common.as_ref()))
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let (water, tank) = tile.map_or((false, false), |t| {
        (world.is_water(t), world.tank_at(t).is_some())
    });
    let (minimum, mode) = (a.at_minimum_health, a.terrain);
    let m = &unit.mover;
    let next = if m.swims && !a.egg && water && !minimum {
        1
    } else if m.underwater && !a.egg && tank && !minimum {
        2
    } else if minimum && (water || tank) {
        mode
    } else {
        0
    };
    set_terrain(unit, next);
}

/// 6.2: the level of a destination in tile `tile`, which has the floor at level `floor`, for a
/// unit now at level `here`: in a tank the unit's 3D target and the tank's water decide, with one
/// draw for a dive, a rise or a plain destination; out of a tank the floor. The type's `cDepth` is
/// used as it is: a species deeper than the water, or a negative depth, gets a level outside it,
/// which the depth search refuses. The numbers are the original's 32-bit ones, so a wild depth
/// wraps instead of stopping.
pub fn level_of(unit: &Unit, world: &World, rng: &mut Rng, tile: usize, here: i32) -> i32 {
    let floor = world.zoo.tiles[tile].elevation;
    let Some(top) = (world.tank_at(tile).and_then(|id| world.tank(id))).map(|k| k.steps().1) else {
        return floor;
    };
    let depth = unit.mover.depth;
    let current = here.saturating_sub(floor).max(0);
    let lev = if current > top {
        top
    } else {
        current.max(depth)
    };
    // `rand() % n` of the span between two levels, which is at least 1 where it is drawn.
    let mut draw = |from: i32, to: i32| rng.pick((i64::from(to) - i64::from(from)) as u32) as i32;
    let level = match unit.target {
        _ if unit.mover.surface => top,
        Target::Surface => top,
        Target::Dive if depth < lev => depth.wrapping_add(draw(depth, lev)),
        Target::Rise if lev < top => top.wrapping_sub(draw(lev, top)),
        Target::Dive | Target::Rise | Target::Level => lev,
        Target::Floor => depth,
        _ if depth.saturating_add(1) < top => depth.wrapping_add(draw(depth, top.wrapping_add(1))),
        _ => lev,
    };
    floor.wrapping_add(level)
}

/// The cell of a point: its tile and its level.
fn cell((x, y, z): (i32, i32, i32)) -> (i32, i32, i32) {
    (x.div_euclid(64), y.div_euclid(64), z.div_euclid(16))
}

/// The next step is refused: the unit's goal has to give up (movement 4.7); the move is done.
fn abort(unit: &mut Unit, world: &mut World) -> bool {
    if !std::mem::replace(&mut unit.aborted, true) {
        world.units.lines.push(format!("abort {}", unit.id));
    }
    true
}

/// 8.2: one step of the 3D move by `distance` along the path of (tile, level) nodes; true when
/// the move is done. A node is reached as soon as the unit's cell is the node's; the next
/// segment starts where the unit is.
pub fn progress(unit: &mut Unit, world: &mut World, mut distance: u32, at: usize) -> bool {
    if distance > 64 {
        distance = 16;
    }
    let Some(c) = world.zoo.entities.get(at).and_then(|e| e.common.as_ref()) else {
        return true;
    };
    let (point, rotation) = ((c.x, c.y, c.z), c.rotation);
    let width = world.zoo.width as usize;
    // The first node of a path that has its levels.
    let node = |u: &Unit| {
        let (t, level) = (u.path.first()?, u.levels.first()?);
        (u.path.len() == u.levels.len()).then_some(((t % width) as i32, (t / width) as i32, *level))
    };
    let here = cell(point);
    let Some(mut next) = node(unit) else {
        unit.path.clear();
        return true;
    };
    if here == next {
        unit.pop_node();
        let Some(n) = node(unit) else {
            unit.path.clear();
            return true;
        };
        (next, unit.segment) = (n, None);
    }
    let segment = match unit.segment.filter(|s| s.to == next) {
        Some(s) => s,
        None => {
            let (a, b) = (here.1 as usize * width + here.0 as usize, unit.path[0]);
            let d = movement::direction(world, a, b);
            let allowed = usize::try_from(d).is_ok_and(|d| {
                movement::allowed_deep(unit, world, at, ((a, here.2), (b, next.2)), d)
            });
            if !allowed {
                return abort(unit, world);
            }
            let delta = (next.0 - here.0, next.1 - here.1, next.2 - here.2);
            // A step changes the level by one at most.
            if delta.2.abs() > 1 {
                return abort(unit, world);
            }
            let flat = (64 * delta.0, 64 * delta.1, 16 * delta.2);
            let squares = flat.0 * flat.0 + flat.1 * flat.1 + flat.2 * flat.2;
            if delta.2 != unit.vertical {
                (unit.vertical, unit.flip) = (delta.2, true);
            }
            Segment {
                to: next,
                start: point,
                delta: flat,
                length: f64::from(squares).sqrt() as i32,
                travelled: 0,
            }
        }
    };
    let travelled = segment.travelled + distance as i32;
    let off = |d: i32| match segment.length {
        0 => d.saturating_mul(travelled),
        l => (i64::from(d) * i64::from(travelled) / i64::from(l)) as i32,
    };
    let mut p = (
        segment.start.0 + off(segment.delta.0),
        segment.start.1 + off(segment.delta.1),
        segment.start.2 + off(segment.delta.2),
    );
    unit.segment = Some(Segment {
        travelled,
        ..segment
    });
    // Past the end of the segment and not in the node's cell: the unit is put on the node.
    if travelled > segment.length && cell(p) != next {
        p = (
            segment.start.0 + segment.delta.0,
            segment.start.1 + segment.delta.1,
            segment.start.2 + segment.delta.2,
        );
        unit.segment = None;
    }
    let facing = movement::DIR
        .iter()
        .position(|&d| d == (segment.delta.0.signum(), segment.delta.1.signum()))
        .map_or(rotation, |d| d as u32);
    let old = world.tile_index(point.0.div_euclid(64), point.1.div_euclid(64));
    let new = world.tile_index(p.0.div_euclid(64), p.1.div_euclid(64));
    world.place_unit(at, p, facing);
    if facing != rotation {
        world.units.lines.push(format!("face {} {facing}", unit.id));
    }
    if let Some((old, new)) = old.zip(new).filter(|(a, b)| a != b) {
        movement::enter(unit, world, old, new);
    }
    false
}
