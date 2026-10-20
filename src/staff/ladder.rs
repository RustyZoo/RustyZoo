//! The ladders and the platform of a tank's gate (tank staff 3): the tiles `I` (inside, in the
//! tank) and `O` (outside), the points the marine keeper climbs between and the facings he takes
//! there. A point is `(x, y, z)` in world units.

use super::service::gate_tiles;
use crate::render::order::ground_height;
use crate::world::World;

pub type Point = (i32, i32, i32);

/// The points of the gate of one tank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ladder {
    pub inside: usize,
    pub outside: usize,
    /// The directions from `O` to `I` and from `I` to `O`.
    pub toward_inside: u32,
    pub toward_outside: u32,
    /// The platform point `P`, on top of the wall above the centre of `I`.
    pub platform: Point,
    /// The outside ladder: its foot `F` and its top.
    pub out_foot: Point,
    pub out_top: Point,
    /// The inside ladder: its foot on the tank floor, the foot raised to the water surface and
    /// its top.
    pub in_foot: Point,
    pub in_surface: Point,
    pub in_top: Point,
}

/// The point of tile `t` on its edge toward direction `d`, snapped inside the tile.
fn edge(world: &World, t: usize, d: usize) -> (i32, i32) {
    let w = world.zoo.width as usize;
    let (dx, dy) = crate::unit::movement::DIR[d];
    let snap = |c: i32| (32 + 32 * c).clamp(0, 63);
    (
        (t % w) as i32 * 64 + snap(dx),
        (t / w) as i32 * 64 + snap(dy),
    )
}

/// The height of the ground of the tile at the point.
fn ground(world: &World, (x, y): (i32, i32)) -> i32 {
    match world.tile_index(x.div_euclid(64), y.div_euclid(64)) {
        Some(t) => ground_height(&world.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64)),
        None => 0,
    }
}

/// The direction from tile `a` to its neighbour `b`.
fn toward(world: &World, a: usize, b: usize) -> Option<u32> {
    (0..8)
        .find(|&d| world.neighbour(a, d) == Some(b))
        .map(|d| d as u32)
}

impl Ladder {
    /// The gate of tank `x`, if it has one with both tiles on the map.
    pub fn of(world: &World, x: u32) -> Option<Ladder> {
        let tank = world.tank(x)?;
        let (inside, outside) = gate_tiles(world, x)?;
        let (to_in, to_out) = (
            toward(world, outside, inside)?,
            toward(world, inside, outside)?,
        );
        let w = world.zoo.width as usize;
        let centre = ((inside % w) as i32 * 64 + 32, (inside / w) as i32 * 64 + 32);
        let (height, level) = (16 * tank.steps().0, 16 * tank.steps().1);
        let top = ground(world, centre) + height;
        let platform = (centre.0, centre.1, top);
        // The foot of the outside ladder: the ground of `O` there, raised by the water of the tank
        // it may be in, but at least the floor of `I` and at most the platform.
        let foot = edge(world, outside, to_in as usize);
        let raised = world
            .tank_at(outside)
            .and_then(|t| world.tank(t))
            .map_or(0, |t| 16 * t.steps().1);
        let z = (ground(world, foot) + raised)
            .max(ground(world, centre))
            .min(top);
        let inner = edge(world, inside, to_out as usize);
        let floor = ground(world, inner);
        Some(Ladder {
            inside,
            outside,
            toward_inside: to_in,
            toward_outside: to_out,
            platform,
            out_foot: (foot.0, foot.1, z),
            out_top: (foot.0, foot.1, top),
            in_foot: (inner.0, inner.1, floor),
            in_surface: (inner.0, inner.1, floor + level),
            in_top: (inner.0, inner.1, top),
        })
    }
}
