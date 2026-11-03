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
/// the unit (4.3). In a depth search `level` is the level the unit would be at: an object on a
/// tank tile stops only the levels from its own minus its depth to its own plus its height.
fn quarters(w: &World, m: &Mover, t: usize, level: Option<i32>) -> [(bool, bool); 4] {
    SLOT.map(|q| match w.quadrant(t, q) {
        None => (false, false),
        Some(e) => {
            let walk = w.shape(e).map(|s| s.walkable);
            let stops = !walk.is_some_and(|k| if m.tall { k.1 } else { k.0 });
            (true, stops && level.is_none_or(|l| in_reach(w, e, t, l)))
        }
    })
}

/// Whether level `level` is one the entity `e` on tile `t` blocks (tank fauna 7.3): every level
/// out of a tank.
pub fn in_reach(w: &World, e: usize, t: usize, level: i32) -> bool {
    let (Some(tank), Some(c)) = (
        w.shape(e).map(|s| s.tank),
        w.zoo.entities[e].common.as_ref(),
    ) else {
        return true;
    };
    let own = c.z.div_euclid(16);
    w.tank_at(t).is_none()
        || (own.saturating_sub(tank.depth)..=own.saturating_add(tank.height)).contains(&level)
}

/// The units on tile `t` other than `m`: their ids, and whether they let it through (an animal
/// walks through others of a type with `walkable` 1).
fn units(w: &World, m: &Mover, t: usize) -> Vec<(u32, bool)> {
    let id = |e: usize| w.zoo.entities[e].common.as_ref().map(|c| c.id);
    let walk = |e: usize| w.shape(e).is_some_and(|s| s.walkable.0);
    (w.units_on(t))
        .filter_map(|e| id(e).map(|id| (id, walk(e))))
        .filter(|&(id, _)| Some(id) != m.id)
        .collect()
}

/// 6.6 item 5: whether another unit that does not let the animal through stands on tile `t` and
/// the animal's own tile holds none. An animal chasing a unit that stands on `t` is never closed
/// out of it, whoever else is there.
fn crowded(w: &World, m: &Mover, t: usize) -> bool {
    let all = units(w, m, t);
    let stops = |t: usize| units(w, m, t).iter().any(|u| !u.1);
    all.iter().any(|u| !u.1)
        && m.at.is_none_or(|s| !stops(s))
        && !m.chasing.is_some_and(|c| all.iter().any(|u| u.0 == c))
}

/// Whether any of the edge slots of the tiles within `r - 1` tiles of `t` (a square; `t` itself
/// for `r` 1) holds a piece.
fn near_edge(w: &World, t: usize, r: u32) -> bool {
    let width = w.zoo.width as i32;
    let r = r.min(w.zoo.width.max(w.zoo.height)) as i32;
    let (x, y) = (t as i32 % width, t as i32 / width);
    (1 - r..r)
        .flat_map(|dy| (1 - r..r).map(move |dx| (dx, dy)))
        .filter_map(|(dx, dy)| w.tile_index(x + dx, y + dy))
        .any(|n| (0..4).any(|d| w.edge(n, d * 2).is_some()))
}

/// 6.6 item 4: whether the animal keeps off tile `t` for the fences near it. An animal with no
/// tile (a young one not yet placed) stands on no tile that is near an edge piece.
fn avoids(w: &World, m: &Mover, t: usize) -> bool {
    let tank = |t: usize| w.tank_at(t).is_some();
    let same = m
        .dest
        .is_none_or(|d| m.at.is_some_and(|s| tank(d) == tank(s)));
    m.avoid_edges > 0
        && !m.test
        && !m.show
        && near_edge(w, t, m.avoid_edges)
        && m.at.is_none_or(|s| !near_edge(w, s, m.avoid_edges))
        && same
}

/// 6.1: the tiles nobody but a swimmer or a diver uses. A guest is closed out of the tiles
/// outside the zoo only: its water and tank rules are its own (6.5).
fn closed(w: &World, m: &Mover, t: usize) -> bool {
    let water = w.is_water(t) && !m.swims;
    let tank = w.tank_at(t).is_some() && !m.underwater;
    !w.in_zoo(t) || m.kind != Kind::Guest && (water || tank)
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
    let blocked = quarters(w, m, t, None).iter().all(|q| q.1);
    // The food a keeper put down: scenery in quarter 0 with food units left.
    let food = w.quadrant(t, SLOT[0]).is_some_and(|e| {
        w.zoo.entities[e]
            .common
            .as_ref()
            .is_some_and(|c| c.food > 0)
    });
    let at_path = m.at.is_some_and(|s| on_path(w, s));
    match m.kind {
        Kind::Keeper if !tank && blocked && !food => MAX,
        Kind::Keeper => match (steep || food, path || tank) {
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
            let wet = |t: usize| w.is_water(t);
            let hurt = m.recovering || m.avoiding;
            if wet(t) && !m.at.is_some_and(wet) && !hurt
                || m.in_water && !wet(t) && !m.building && !m.recovering
            {
                return MAX;
            }
            let on_tank = m.at.is_some_and(|s| w.tank_at(s).is_some());
            let both = goal.is_some_and(|g| on_path(w, g)) && at_path;
            let base = match (path, both && !m.leaving && !m.avoiding) {
                (true, _) => 1,
                (_, true) => MAX,
                _ if on_tank || wet(t) => 1,
                _ if m.avoiding || m.emergency => m.emergency_cost,
                _ => m.non_path_cost,
            };
            let afraid = units(w, m, t).iter().any(|u| m.predators.contains(&u.0));
            if steep || afraid { base.max(16) } else { base }
        }
        Kind::Animal if m.only_underwater && !tank => MAX,
        Kind::Animal if avoids(w, m, t) || crowded(w, m, t) => MAX,
        Kind::Animal
            if steep
                && m.footprint >= 4
                && m.at.is_none_or(|s| path_shape(w.zoo.tiles[s].shape)) =>
        {
            MAX
        }
        Kind::Animal if m.show => 1,
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

/// Whether the ground closes the edge between `t` and its neighbour `n` in direction `d` (5.4,
/// tank fauna 7.2): at a tank the walls decide, else the edge state and the climbers' rule.
fn stepped(w: &World, m: &Mover, t: usize, n: usize, d: usize) -> bool {
    let elevation = |t: usize| w.zoo.tiles[t].elevation;
    // The top of the walls of tank `id`, and whether the two corners of `land` towards `to` are
    // level with it.
    let top = |id: u32| w.floor(id).zip(w.tank(id)).map(|(f, k)| f + k.steps().0);
    let level = |land: usize, to: usize, id: u32| {
        [(to + 1) % 8, (to + 7) % 8]
            .iter()
            .all(|&c| top(id) == Some(crate::render::corner_elevation(&w.zoo.tiles[land], c)))
    };
    match (w.tank_at(t), w.tank_at(n)) {
        (None, None) => w.cliff(t, d) && !(m.climbs && (elevation(t) - elevation(n)).abs() <= 1),
        (Some(a), Some(b)) => a != b && top(a) != top(b),
        (Some(id), None) => !level(n, (d + 4) % 8, id),
        (None, Some(id)) => !level(t, d, id),
    }
}

fn straight(w: &World, m: &Mover, t: usize, d: usize) -> u32 {
    let Some(n) = w.neighbour(t, d) else {
        return MAX;
    };
    let step = matches!(m.kind, Kind::Guest | Kind::Animal) && stepped(w, m, t, n, d);
    let pieces: Vec<usize> = [w.edge(t, d), w.edge(n, (d + 4) % 8)]
        .into_iter()
        .flatten()
        .collect();
    if step || !passes(w, m, t, n, &pieces) {
        return MAX;
    }
    let low = |p: &usize| w.shape(*p).is_some_and(|s| !s.fence && !s.wall);
    // The escape search sets no mark.
    let marked = |p: &usize| !m.test && w.shape(*p).is_some_and(|s| !s.tank.wall) && !low(p);
    // A random walk turns only from the unit's own tile: a quarter turn costs 2, a turn back 4.
    let turn = match m.turn.filter(|_| m.at == Some(t)) {
        Some(f) => match (d + 8 - f % 8) % 8 {
            2 | 6 => 2,
            3..=5 => 4,
            _ => 1,
        },
        None => 1,
    };
    match m.kind {
        Kind::Animal if pieces.iter().any(marked) => 8,
        Kind::Keeper | Kind::Worker if pieces.iter().any(low) => turn + 15,
        _ => turn,
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
    // The gate of a show tank on this line.
    let show_gate = |p: usize| {
        let show = |t: usize| {
            w.tank_at(t)
                .and_then(|id| w.tank(id))
                .is_some_and(|k| k.show)
        };
        w.is_door(p) && (show(t) || show(n))
    };
    let grounds = |t: usize| {
        w.exhibits()
            .region(t)
            .is_some_and(|r| r == GROUNDS || r == EXTERIOR)
    };
    let every = |ok: &dyn Fn(usize) -> bool| pieces.iter().all(|&p| ok(p));
    let amphibious = m.underwater && !m.only_underwater;
    match m.kind {
        _ if pieces.is_empty() => true,
        Kind::Keeper | Kind::Worker => every(&|p| broken(p) || low(p) || gate(p, true)),
        Kind::Guest => every(&|p| gate(p, false)) && !(grounds(t) && !grounds(n)),
        Kind::Guide => every(&|p| broken(p) || gate(p, false)),
        // An amphibious species crosses a connector wall (tank fauna 12.3), but not on an
        // escape walk.
        Kind::Animal => every(&|p| {
            low(p)
                || broken(p)
                || m.show && show_gate(p)
                || amphibious && !m.test && w.connector(p)
                || m.test && !grounds(t) && escapes(w, m, (t, n), p)
        }),
    }
}

/// 5.3 points 2, 6 and 7: an animal with its escape flag up, in an exhibit, gets past the piece at
/// `p` on the line between `t` and `n`: never a wall, a zoo gate, a gate, a powered electric fence
/// or a connector wall; always a tank wall of a tank with no height; else a piece it jumps,
/// climbs, bashes (its strength below the bash) or crushes (its height at most the crush).
pub(super) fn escapes(w: &World, m: &Mover, (t, n): (usize, usize), p: usize) -> bool {
    let (Some(s), Some(c)) = (w.shape(p), w.zoo.entities[p].common.as_ref()) else {
        return false;
    };
    let k = s.tank;
    let (life, strength) = c
        .fence
        .map_or((k.life, k.strength), |f| (f.life, f.strength));
    if s.wall || s.gate || w.is_door(p) || k.electrified && life > 0 && life > k.decayed {
        return false;
    }
    if k.wall && w.connector(p) {
        return false;
    }
    let tank = w.tank_at(t).or(w.tank_at(n)).and_then(|id| w.tank(id));
    k.wall && tank.is_some_and(|k| k.height == 0)
        || m.climber && k.climbable
        || m.jumper && k.jumpable
        || i64::from(strength) < i64::from(m.bash)
        || m.crush != 0 && k.height <= m.crush
}

/// 4.3 point 1: whether quarter 3 of tile `t` holds a building other than the unit's building
/// target. A unit without a target meets none.
pub fn other_building(w: &World, m: &Mover, t: usize) -> bool {
    let Some(target) = m.target_building else {
        return false;
    };
    w.quadrant(t, SLOT[3]).is_some_and(|e| {
        let e = &w.zoo.entities[e];
        e.class == "building" && e.common.as_ref().is_some_and(|c| c.id != target)
    })
}

/// 4.3: whether the step from `a` to `b` in direction `d` clears the objects.
pub fn clear(
    w: &World,
    m: &Mover,
    (a, b): (usize, usize),
    d: usize,
    diagonal: bool,
    levels: Option<(i32, i32)>,
) -> bool {
    if d % 2 == 1 && diagonal {
        let (x, y) = ((d + 7) % 8, (d + 1) % 8);
        let (ax, ay) = (w.neighbour(a, x), w.neighbour(a, y));
        let Some((ax, ay)) = ax.zip(ay) else {
            return false;
        };
        let sides = [(a, ax, x), (a, ay, y), (ax, b, y), (ay, b, x)];
        if !sides
            .iter()
            .all(|&(p, q, d)| clear_one(w, m, (p, q), d, levels))
        {
            return false;
        }
    }
    clear_one(w, m, (a, b), d, levels)
}

fn clear_one(
    w: &World,
    m: &Mover,
    (a, b): (usize, usize),
    d: usize,
    levels: Option<(i32, i32)>,
) -> bool {
    let (qa, qb) = (
        quarters(w, m, a, levels.map(|l| l.0)),
        quarters(w, m, b, levels.map(|l| l.1)),
    );
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
