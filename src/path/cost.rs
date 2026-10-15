//! What a step costs a unit: the edge cost of leaving a tile, the terrain cost of a tile, and
//! whether objects let the unit through (pathfinding 4 to 6).

use super::{Kind, MAX, Mover};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::place::path_shape;
use crate::world::World;

/// The quarter of a tile in the spec's numbering (`(lx / 32) * 2 + ly / 32`) as the world numbers
/// it (`qy * 2 + qx`).
const SLOT: [usize; 4] = [0, 2, 1, 3];

fn on_path(w: &World, t: usize) -> bool {
    w.path(t).is_some()
}

/// A tile without a path whose shape is neither flat nor a path shape.
fn steep(w: &World, t: usize) -> bool {
    !on_path(w, t) && !path_shape(w.zoo.tiles[t].shape)
}

/// Per quarter (the spec's numbering): whether it holds an entity, and whether that entity stops
/// the unit (4.3).
fn quarters(w: &World, m: &Mover, t: usize) -> [(bool, bool); 4] {
    SLOT.map(|q| match w.quadrant(t, q) {
        None => (false, false),
        Some(e) => {
            let walk = w.shape(e).map(|s| s.walkable);
            (true, !walk.is_some_and(|k| if m.tall { k.1 } else { k.0 }))
        }
    })
}

/// 6.1: the tiles nobody but a swimmer or a diver uses.
fn closed(w: &World, m: &Mover, t: usize) -> bool {
    let water = w.is_water(t) && !m.swims;
    let tank = w.tank_at(t).is_some() && !m.underwater;
    !w.in_zoo(t) || water || tank
}

/// The terrain cost of tile `t` for the unit (6.1 to 6.6); `goal` is the tile the search under
/// way is going to.
pub fn terrain(w: &World, m: &Mover, t: usize, goal: Option<usize>) -> u32 {
    if m.at == Some(t) {
        return 1;
    }
    if closed(w, m, t) {
        return MAX;
    }
    let (path, steep, tank) = (on_path(w, t), steep(w, t), w.tank_at(t).is_some());
    let blocked = quarters(w, m, t).iter().all(|q| q.1);
    let at_path = m.at.is_some_and(|s| on_path(w, s));
    match m.kind {
        Kind::Keeper if !tank && blocked => MAX,
        Kind::Keeper => match (steep, path || tank) {
            (true, _) => [16, 1][usize::from(m.relaxed)],
            (_, false) => [2, 1][usize::from(m.relaxed)],
            _ => 1,
        },
        Kind::Worker if steep => 16,
        Kind::Worker => [2, 1][usize::from(path)],
        Kind::Guide if steep => 16,
        Kind::Guide => match (path, at_path) {
            (true, _) => 1,
            (_, false) => 17,
            _ => MAX,
        },
        Kind::Guest => {
            let on_tank = m.at.is_some_and(|s| w.tank_at(s).is_some());
            let both = goal.is_some_and(|g| on_path(w, g)) && at_path;
            let base = match (path, both && !m.leaving) {
                (true, _) => 1,
                (_, true) => MAX,
                _ if on_tank || w.is_water(t) => 1,
                _ => m.non_path_cost,
            };
            if steep { base.max(16) } else { base }
        }
        // 6.6 without the rules that need the other units, the shows and the avoided edges.
        Kind::Animal if m.only_underwater && !tank => MAX,
        Kind::Animal
            if steep
                && m.footprint >= 4
                && m.at.is_none_or(|s| path_shape(w.zoo.tiles[s].shape)) =>
        {
            MAX
        }
        Kind::Animal if m.water_mode && !(w.is_water(t) || tank) => 17,
        Kind::Animal if !m.water_mode && w.is_water(t) => 17,
        Kind::Animal => [1, 16][usize::from(steep && !m.climbs || path)],
    }
}

/// The edge cost of leaving tile `t` in map direction `d` (5.1, 5.2).
pub fn edge(w: &World, m: &Mover, t: usize, d: usize) -> u32 {
    if d.is_multiple_of(2) {
        return straight(w, m, t, d);
    }
    let (a, b) = ((d + 7) % 8, (d + 1) % 8);
    let (ta, tb, to) = (w.neighbour(t, a), w.neighbour(t, b), w.neighbour(t, d));
    let (Some(ta), Some(tb), Some(to)) = (ta, tb, to) else {
        return MAX;
    };
    let worst = [(t, a), (t, b), (ta, b), (tb, a)]
        .iter()
        .map(|&(t, d)| straight(w, m, t, d))
        .max()
        .unwrap_or(MAX);
    let corner = on_path(w, t) && on_path(w, to) && !(on_path(w, ta) && on_path(w, tb));
    worst + if corner { 13 } else { 0 }
}

fn straight(w: &World, m: &Mover, t: usize, d: usize) -> u32 {
    let Some(n) = w.neighbour(t, d) else {
        return MAX;
    };
    let elevation = |t: usize| w.zoo.tiles[t].elevation;
    let climbs = m.climbs && (elevation(t) - elevation(n)).abs() <= 1;
    let step = matches!(m.kind, Kind::Guest | Kind::Animal) && w.cliff(t, d) && !climbs;
    let pieces: Vec<usize> = [w.edge(t, d), w.edge(n, (d + 4) % 8)]
        .into_iter()
        .flatten()
        .collect();
    if step || !passes(w, m, t, n, &pieces) {
        return MAX;
    }
    let low = |p: &usize| w.shape(*p).is_some_and(|s| !s.fence && !s.wall);
    let marked = |p: &usize| w.shape(*p).is_some_and(|s| !s.tank.wall) && !low(p);
    match m.kind {
        Kind::Animal if pieces.iter().any(marked) => 8,
        Kind::Keeper | Kind::Worker if pieces.iter().any(low) => 16,
        _ => 1,
    }
}

/// The fence test of a unit kind (5.3): whether it may cross the line between `t` and `n`.
fn passes(w: &World, m: &Mover, t: usize, n: usize, pieces: &[usize]) -> bool {
    let broken = |p: usize| {
        w.zoo.entities[p]
            .common
            .as_ref()
            .and_then(|c| c.fence.as_ref())
            .is_some_and(|f| f.life < 1)
    };
    let low = |p: usize| w.shape(p).is_some_and(|s| !s.fence && !s.wall);
    // A gate of a tank wall is open only while the wall has no height.
    let gate = |p: usize, flat: bool| {
        let tank = w.tank_at(t).or(w.tank_at(n)).and_then(|id| w.tank(id));
        let wall = w.shape(p).is_some_and(|s| s.tank.wall);
        w.is_door(p) && (!flat || !wall || tank.is_none_or(|k| k.height == 0))
    };
    let grounds = |t: usize| {
        w.exhibits()
            .region(t)
            .is_some_and(|r| r == GROUNDS || r == EXTERIOR)
    };
    let every = |ok: &dyn Fn(usize) -> bool| pieces.iter().all(|&p| ok(p));
    match m.kind {
        _ if pieces.is_empty() => true,
        Kind::Keeper | Kind::Worker => every(&|p| broken(p) || low(p) || gate(p, true)),
        Kind::Guest => every(&|p| gate(p, false)) && !(grounds(t) && !grounds(n)),
        Kind::Guide => every(&|p| broken(p) || gate(p, false)),
        Kind::Animal => every(&|p| low(p) || broken(p)),
    }
}

/// 4.3: whether the step from `a` to `b` in direction `d` clears the objects.
pub fn clear(w: &World, m: &Mover, a: usize, b: usize, d: usize, diagonal: bool) -> bool {
    if d % 2 == 1 && diagonal {
        let (x, y) = ((d + 7) % 8, (d + 1) % 8);
        let (ax, ay) = (w.neighbour(a, x), w.neighbour(a, y));
        let Some((ax, ay)) = ax.zip(ay) else {
            return false;
        };
        let sides = [(a, ax, x), (a, ay, y), (ax, b, y), (ay, b, x)];
        if !sides.iter().all(|&(p, q, d)| clear_one(w, m, p, q, d)) {
            return false;
        }
    }
    clear_one(w, m, a, b, d)
}

fn clear_one(w: &World, m: &Mover, a: usize, b: usize, d: usize) -> bool {
    let (qa, qb) = (quarters(w, m, a), quarters(w, m, b));
    if !qa.iter().chain(&qb).any(|q| q.0) {
        return true;
    }
    let blocked = |q: &[(bool, bool); 4], i: &[usize]| i.iter().all(|&i| q[i].1);
    let count = qb.iter().filter(|q| q.1).count();
    let s = m.footprint.clamp(1, 4) as usize;
    let into = [[1, 3], [0, 1], [0, 2], [2, 3]];
    let out = [[0, 2], [2, 3], [1, 3], [0, 1]];
    if 4 - count < s
        || s > 1 && (blocked(&qb, &[0, 3]) || blocked(&qb, &[1, 2]))
        || d.is_multiple_of(2) && blocked(&qb, &into[d / 2])
    {
        return false;
    }
    if m.at == Some(a) && qa[3].0 {
        return true;
    }
    qa.iter().filter(|q| q.1).count() < 2 || d % 2 == 1 || !blocked(&qa, &out[d / 2])
}

/// The step cost `P(a, b)` (4.1) for two neighbouring tiles in direction `d`; `goal` is the tile
/// of the search under way, none outside a search.
pub fn step(w: &World, m: &Mover, a: usize, b: usize, d: usize, goal: Option<usize>) -> u32 {
    let cost = edge(w, m, a, d) + terrain(w, m, b, goal);
    if cost > MAX { MAX } else { cost }
}
