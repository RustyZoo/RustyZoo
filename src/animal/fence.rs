//! The fence test start of an animal (animals 11.10): when the draw of the zap check succeeds, a
//! side with a working electrified fence is looked for, and the animal stops what it does, turns to
//! it and starts `bTestFence`.

use super::{Keys, escape};
use crate::behaviour::Data;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::path::SLOT;
use crate::render::order::Kind;
use crate::unit::{Ctx, Part, Unit};
use crate::world::World;

const TEST: &str = "bTestFence";
/// The two quarters along side `d / 2` (the spec's numbering, `pathfinding.md` 4.3).
const ALONG: [[usize; 2]; 4] = [[0, 2], [2, 3], [1, 3], [0, 1]];

/// The region of tile `t`, the exterior counting as the grounds.
fn region(world: &World, t: usize) -> Option<u32> {
    let r = world.exhibits().region(t)?;
    Some(if r == EXTERIOR { GROUNDS } else { r })
}

/// Whether the fence-class piece `p` is a working electrified fence: its type is electrified, the
/// piece is not broken and its life is above the `cDecayedLife` of the type.
fn working(world: &World, p: usize) -> bool {
    let (Some(s), Some(c)) = (world.shape(p), world.zoo.entities[p].common.as_ref()) else {
        return false;
    };
    let life = c.fence.map_or(s.tank.life, |f| f.life);
    s.tank.electrified && life > 0 && life > s.tank.decayed
}

/// 11.10 point 1, the edge test of the side `d` of tile `t`: the piece of the slot, else the
/// piece the neighbour holds on its side `d + 4`; only a fence-class piece (a fence or a tank
/// wall) counts, and the first one found decides.
fn electrified(world: &World, t: usize, d: usize) -> bool {
    let fence = |p: &usize| {
        world
            .shape(*p)
            .is_some_and(|s| matches!(s.kind, Kind::Edge | Kind::Wall))
    };
    let own = world.edge(t, d).filter(fence);
    let across = || {
        let n = world.neighbour(t, d)?;
        world.edge(n, (d + 4) % 8).filter(fence)
    };
    own.or_else(across).is_some_and(|p| working(world, p))
}

/// 11.10 point 1: the first side `d` of 0, 2, 4, 6 whose probe point, `offset` tiles away, lies on
/// the map, in the region of tile `own`, with its two quarters along the side empty and a working
/// electrified fence on the side or across it. `(x, y)` is the animal's position.
fn side(world: &World, own: usize, (x, y): (i32, i32), offset: i32) -> Option<usize> {
    let o = i64::from(offset) * 64;
    let (x, y) = (i64::from(x), i64::from(y));
    let probes = [(x, y - o), (x + o, y), (x, y + o), (x - o, y)];
    (0..4).find_map(|i| {
        let (px, py) = probes[i];
        let tile = |v: i64| i32::try_from(v / 64).ok();
        let t = (px >= 0 && py >= 0)
            .then(|| world.tile_index(tile(px)?, tile(py)?))
            .flatten()?;
        let empty = ALONG[i]
            .iter()
            .all(|&q| world.quadrant(t, SLOT[q]).is_none());
        (region(world, t) == region(world, own) && empty && electrified(world, t, 2 * i))
            .then_some(2 * i)
    })
}

/// 11.10: the animal whose zap draw succeeded searches a side. With one: it wakes, stops everything
/// without the idle animation, faces it and starts `bTestFence` forced; when the set starts (or is
/// the top set already) the chance drops to 0 under `cMinTestFenceChance` and the happiness
/// changes by `cZapHappinessChange`.
pub fn test(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    k: &Keys,
) {
    let Some(c) = world.zoo.entities[at].common.as_ref() else {
        return;
    };
    let (point, own) = (
        (c.x, c.y, c.z),
        world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)),
    );
    let Some(d) = own.and_then(|t| side(world, t, (point.0, point.1), k.fence_offset)) else {
        return;
    };
    if let Part::Animal(a) = &mut unit.part {
        a.sleeping = false;
    }
    crate::goal::stop_all(unit);
    world.place_unit(at, point, d as u32);
    if !escape::start_new(unit, world, data, ctx.rng, at, (TEST, false)) {
        return;
    }
    if let Part::Animal(a) = &mut unit.part {
        let lowered = a
            .fence_chance
            .unwrap_or(k.test_fence)
            .wrapping_sub(k.fence_drop);
        a.fence_chance = Some(if lowered < k.fence_min { 0 } else { lowered });
        a.change(k.zap_change);
    }
}
