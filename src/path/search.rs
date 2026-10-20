//! The A* search over tiles with the original's open list (pathfinding 3.3 to 3.8), over (tile,
//! level) nodes in a tank.

use super::cost::{clear, edge, other_building, step};
use super::{Kind, MAX, Mover, Settings};
use crate::render::corner_elevation;
use crate::world::World;
use std::collections::HashMap;

/// The map directions as offsets, and the neighbours in the order the search tries them.
const STEP: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];
const EIGHT: [(i32, i32); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];
/// The length factor of a diagonal step: the decimal constant of the program's table (3.6), not
/// the square root of 2.
#[allow(clippy::approx_constant)]
pub(super) const DIAGONAL: f64 = 1.414_213_562_373;
const FOUR: [(i32, i32); 4] = [(-1, 0), (0, -1), (1, 0), (0, 1)];

#[derive(Clone, Copy, PartialEq)]
enum State {
    New,
    Open,
    Closed,
}

/// The length factor of a step that changes `n` of the three coordinates by one (3.6).
#[allow(clippy::approx_constant)]
const LENGTH: [f64; 3] = [1.0, DIAGONAL, 1.732_050_807_569];

struct Node {
    tile: usize,
    level: i32,
    /// The direction and the level change this node was entered with (depth search).
    entered: (usize, i32),
    g: f64,
    h: f64,
    f: f64,
    parent: Option<usize>,
    state: State,
}

/// What a search found: the tiles from the start, and whether the last one is the goal.
pub struct Found {
    pub path: Vec<usize>,
    /// The level of each tile of the path in a depth search, else empty.
    pub levels: Vec<i32>,
    pub reached: bool,
}

struct Search<'a> {
    w: &'a World,
    m: &'a Mover,
    s: &'a Settings,
    goal: usize,
    /// A depth search, and the level it ends at.
    deep: bool,
    goal_level: i32,
    nodes: Vec<Node>,
    at: HashMap<(usize, i32), usize>,
    heap: Vec<usize>,
    /// A node was refused because the cap was used up.
    capped: bool,
}

/// Whether a unit moves on the diagonals (3.2).
pub fn diagonal(m: &Mover, s: &Settings) -> bool {
    s.diagonal && m.kind != Kind::Guest && !(m.test && m.kind == Kind::Animal)
}

/// The step cost of going from `a` to its neighbour `b` in direction `d` when the step is allowed
/// (4.2), else `None`. A unit at a building (`building`) is held to the edge cost alone and passes
/// every quarter but those of another building (4.3 point 1).
pub fn open(
    w: &World,
    m: &Mover,
    s: &Settings,
    (a, b, d): (usize, usize, usize),
    goal: Option<usize>,
    building: bool,
) -> Option<u32> {
    let cost = step(w, m, a, b, d, goal);
    let fits = if building { edge(w, m, a, d) } else { cost };
    (w.tank_at(b).is_none()
        && fits < MAX
        && (building && !other_building(w, m, b) || clear(w, m, (a, b), d, diagonal(m, s), None)))
    .then_some(cost)
}

/// 4.2 in a depth search: the step cost from level `za` of `a` to level `zb` of `b` when the unit's
/// elevation check (tank fauna 7.1) lets it, else `None`.
pub fn open_deep(
    w: &World,
    m: &Mover,
    s: &Settings,
    ((a, za), (b, zb), d): ((usize, i32), (usize, i32), usize),
    goal: Option<usize>,
    building: bool,
) -> Option<u32> {
    let cost = step(w, m, a, b, d, goal);
    let fits = if building { edge(w, m, a, d) } else { cost };
    (clear(w, m, (a, b), d, diagonal(m, s), Some((za, zb)))
        && elevated(w, m, (a, za), (b, zb))
        && fits < MAX)
        .then_some(cost)
}

/// Tank fauna 7.1: whether the unit may be at level `zb` of tile `b` after level `za` of `a`.
fn elevated(w: &World, m: &Mover, (a, za): (usize, i32), (b, zb): (usize, i32)) -> bool {
    let Some(id) = w.tank_at(b) else {
        // On land the level is one of the four corners of the tile.
        return [1, 7, 3, 5]
            .iter()
            .any(|&c| corner_elevation(&w.zoo.tiles[b], c) == zb);
    };
    let (Some(floor), Some(tank)) = (w.floor(id), w.tank(id)) else {
        return false;
    };
    let top = floor + tank.steps().1;
    match w.tank_at(a) {
        Some(from) if from != id => zb == top && zb == za,
        _ if m.surface => zb == top,
        _ => floor <= zb && zb <= top,
    }
}

/// The direction from tile `a` to its neighbour `b`.
pub fn direction(w: &World, a: usize, b: usize) -> usize {
    (0..8).find(|&d| w.neighbour(a, d) == Some(b)).unwrap_or(0)
}

impl Search<'_> {
    fn xy(&self, t: usize) -> (i32, i32) {
        let width = self.w.zoo.width as usize;
        ((t % width) as i32, (t / width) as i32)
    }

    /// The estimate `h` (3.5).
    fn estimate(&self, t: usize, level: i32) -> f64 {
        let ((x, y), (gx, gy)) = (self.xy(t), self.xy(self.goal));
        let (dx, dy) = (f64::from(gx - x), f64::from(gy - y));
        // Twice the level difference, in a depth search only.
        let dz = if self.deep {
            2.0 * (f64::from(self.goal_level) - f64::from(level))
        } else {
            0.0
        };
        let k = f64::from(self.s.min_cost);
        match self.s.estimate {
            0 => (dx * dx + dy * dy + dz * dz) * k,
            1 => (dx.abs() + dy.abs() + dz.abs()) * k,
            2 => dx.abs().max(dy.abs()).max(dz.abs()) * k,
            _ => (dx * dx + dy * dy + dz * dz).sqrt() * k,
        }
    }

    /// The start node: closed, entered with the unit's facing and vertical direction (3.6), so
    /// that a first step straight ahead at the same vertical direction pays no turn.
    fn start(&mut self, tile: usize, level: i32) -> Option<usize> {
        let start = self.node(tile, level)?;
        self.nodes[start].state = State::Closed;
        self.nodes[start].entered = self.m.start;
        Some(start)
    }

    /// The node of a tile, made when the tile is met for the first time and the cap allows.
    fn node(&mut self, tile: usize, level: i32) -> Option<usize> {
        if let Some(&i) = self.at.get(&(tile, level)) {
            return Some(i);
        }
        if self.nodes.len() >= self.s.node_max {
            self.capped = true;
            return None;
        }
        let h = self.estimate(tile, level);
        self.nodes.push(Node {
            tile,
            level,
            entered: (usize::MAX, 0),
            g: 0.0,
            h,
            f: h,
            parent: None,
            state: State::New,
        });
        self.at.insert((tile, level), self.nodes.len() - 1);
        Some(self.nodes.len() - 1)
    }

    fn f(&self, slot: usize) -> f64 {
        self.nodes[self.heap[slot]].f
    }

    /// Moves `v` up from `hole` while its parent's `f` is greater (not above `top`).
    fn up(&mut self, mut hole: usize, top: usize, v: usize) {
        let f = self.nodes[v].f;
        while hole > top && self.f((hole - 1) / 2) > f {
            self.heap[hole] = self.heap[(hole - 1) / 2];
            hole = (hole - 1) / 2;
        }
        self.heap[hole] = v;
    }

    /// Moves the hole at `hole` down to a leaf of the first `n` slots, taking the right child on
    /// equal `f`; the new hole.
    fn down(&mut self, mut hole: usize, n: usize) -> usize {
        let mut child = 2 * hole + 2;
        while child < n {
            if self.f(child - 1) < self.f(child) {
                child -= 1;
            }
            self.heap[hole] = self.heap[child];
            hole = child;
            child = 2 * child + 2;
        }
        if child == n {
            self.heap[hole] = self.heap[child - 1];
            hole = child - 1;
        }
        hole
    }

    fn push(&mut self, v: usize) {
        self.heap.push(v);
        self.up(self.heap.len() - 1, 0, v);
        self.nodes[v].state = State::Open;
    }

    fn pop(&mut self) -> Option<usize> {
        let top = *self.heap.first()?;
        let v = self.heap.pop()?;
        let n = self.heap.len();
        if n > 0 {
            let hole = self.down(0, n);
            self.up(hole, 0, v);
        }
        self.nodes[top].state = State::Closed;
        Some(top)
    }

    /// The classic make-heap, after an open node got a smaller `f`.
    fn rebuild(&mut self) {
        let n = self.heap.len();
        for p in (0..n / 2).rev() {
            let v = self.heap[p];
            let hole = self.down(p, n);
            self.up(hole, p, v);
        }
    }

    /// Relaxes every allowed neighbour of node `c` (3.3, 3.4).
    fn expand(&mut self, c: usize) {
        let list: &[(i32, i32)] = if diagonal(self.m, self.s) {
            &EIGHT
        } else {
            &FOUR
        };
        // A depth search tries the same neighbours for a level change of -1, 0 and +1 (3.4).
        let levels: &[i32] = if self.deep { &[-1, 0, 1] } else { &[0] };
        for &dz in levels {
            for &(dx, dy) in list {
                self.relax(c, (dx, dy, dz));
            }
        }
    }

    /// Relaxes the neighbour of node `c` at the tile offset `(dx, dy)` and the level change `dz`.
    fn relax(&mut self, c: usize, (dx, dy, dz): (i32, i32, i32)) {
        let Node {
            tile: from,
            level: z,
            entered,
            ..
        } = self.nodes[c];
        let (x, y) = self.xy(from);
        let Some(to) = self.w.tile_index(x + dx, y + dy) else {
            return;
        };
        let d = STEP.iter().position(|&s| s == (dx, dy)).unwrap_or(0);
        let (goal, building) = (Some(self.goal), self.m.building);
        let cost = match self.deep {
            true => open_deep(
                self.w,
                self.m,
                self.s,
                ((from, z), (to, z + dz), d),
                goal,
                building,
            ),
            false => open(self.w, self.m, self.s, (from, to, d), goal, building),
        };
        let Some(cost) = cost else {
            return;
        };
        let Some(n) = self.node(to, z + dz) else {
            return;
        };
        let mut length = LENGTH[(dx.abs() + dy.abs() + dz.abs()) as usize - 1];
        if self.deep {
            // A turn costs 0.1 more, and so does a change of the level change.
            length += 0.1 * f64::from(u8::from(entered.0 != d));
            length += 0.1 * f64::from(u8::from(entered.1 != dz));
        }
        let g = self.nodes[c].g + length * f64::from(cost);
        if self.nodes[n].state == State::New || g < self.nodes[n].g {
            let node = &mut self.nodes[n];
            (node.g, node.parent, node.f, node.entered) = (g, Some(c), g + node.h, (d, dz));
            if node.state == State::Open {
                self.rebuild();
            } else {
                self.push(n);
            }
        }
    }
}

/// Searches from tile `from` to tile `goal`. A partial path (to the node nearest the goal) is
/// returned when `partial` is set; else the path is empty when the goal is not reached.
pub fn search(
    w: &World,
    m: &Mover,
    s: &Settings,
    from: usize,
    goal: usize,
    partial: bool,
) -> Found {
    let deep = m.deep || w.tank_at(from).is_some() || w.tank_at(goal).is_some();
    let (z0, z1) = if deep { m.levels } else { (0, 0) };
    let mut f = Search {
        w,
        m,
        s,
        goal,
        deep,
        goal_level: z1,
        nodes: Vec::new(),
        at: HashMap::new(),
        heap: Vec::new(),
        capped: false,
    };
    let Some(start) = f.start(from, z0) else {
        return Found {
            path: Vec::new(),
            levels: Vec::new(),
            reached: false,
        };
    };
    let (mut c, mut best, mut reached) = (start, start, false);
    loop {
        if f.nodes[c].h < f.nodes[best].h {
            best = c;
        }
        if f.nodes[c].tile == goal && (!deep || f.nodes[c].level == z1) {
            (best, reached) = (c, true);
            break;
        }
        f.expand(c);
        match f.pop() {
            Some(next) if !f.capped => c = next,
            _ => break,
        }
    }
    let (mut path, mut levels) = (Vec::new(), Vec::new());
    if reached || partial {
        let mut at = Some(best);
        while let Some(i) = at {
            path.push(f.nodes[i].tile);
            levels.push(f.nodes[i].level);
            at = f.nodes[i].parent;
        }
        path.reverse();
        levels.reverse();
        if path.len() == 1 {
            path.clear();
            levels.clear();
        }
    }
    if !deep {
        levels.clear();
    }
    Found {
        path,
        levels,
        reached,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nodes of these estimates pushed in order, the `lowered` ones given a smaller `f` and the
    /// list rebuilt, then popped: the order of the nodes.
    fn order(fs: &[f64], lowered: &[(usize, f64)]) -> Vec<usize> {
        let w = crate::path::tests::world(|_, _| {});
        let (m, s) = (Mover::default(), Settings::default());
        let (at, nodes) = (HashMap::new(), Vec::new());
        let mut f = Search {
            w: &w,
            m: &m,
            s: &s,
            goal: 0,
            deep: false,
            goal_level: 0,
            nodes,
            at,
            heap: Vec::new(),
            capped: false,
        };
        for (tile, &h) in fs.iter().enumerate() {
            f.nodes.push(Node {
                tile,
                level: 0,
                entered: (0, 0),
                g: 0.0,
                h,
                f: h,
                parent: None,
                state: State::New,
            });
            f.push(tile);
        }
        for &(i, h) in lowered {
            f.nodes[i].f = h;
        }
        if !lowered.is_empty() {
            f.rebuild();
        }
        std::iter::from_fn(|| f.pop()).collect()
    }

    #[test]
    fn the_open_list_pops_by_f_and_takes_the_right_child_on_equal_f() {
        assert_eq!(order(&[1.0; 4], &[]), [0, 2, 1, 3]);
        let mut x = 7u32;
        let fs: Vec<f64> = (0..200)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                f64::from((x >> 16) % 40)
            })
            .collect();
        let lowered = [(3, -5.0), (50, -4.0), (199, -9.0), (100, 0.5)];
        let popped = order(&fs, &lowered);
        let f = |i: usize| lowered.iter().find(|l| l.0 == i).map_or(fs[i], |l| l.1);
        assert_eq!(popped.len(), 200);
        assert!(popped.windows(2).all(|p| f(p[0]) <= f(p[1])));
    }

    #[test]
    fn the_search_uses_the_original_length_factors_and_a_double_estimate() {
        let w = crate::path::tests::world(|_, _| {});
        let at = |x: usize, y: usize| y * 10 + x;
        let m = Mover::new(Kind::Keeper, Some(at(5, 5)));
        let s = Settings {
            estimate: 3,
            ..Settings::default()
        };
        let mut f = Search {
            w: &w,
            m: &m,
            s: &s,
            goal: at(0, 0),
            deep: false,
            goal_level: 0,
            nodes: Vec::new(),
            at: HashMap::new(),
            heap: Vec::new(),
            capped: false,
        };
        // The square root of 2 times `MinCost`, in double precision.
        assert_eq!(f.estimate(at(1, 1), 0), 2f64.sqrt() * 2.0);
        // A diagonal step of step cost 3 (edge 1, open ground 2) adds the table's constant times 3.
        let start = f.node(at(5, 5), 0).unwrap();
        f.nodes[start].state = State::Closed;
        f.expand(start);
        let n = f.at[&(at(6, 6), 0)];
        assert_eq!(DIAGONAL.to_bits(), 0x3FF6_A09E_667F_3A21);
        assert_eq!(f.nodes[n].g, DIAGONAL * 3.0);
        assert_ne!(f.nodes[n].g, std::f64::consts::SQRT_2 * 3.0);
    }

    #[test]
    fn the_first_step_of_a_depth_search_is_penalised_against_the_unit_s_facing_and_vertical_move() {
        let w = crate::path::tests::world(|_, _| {});
        let at = |x: usize, y: usize| y * 10 + x;
        let s = Settings::default();
        // The cost of the first step east (direction 2) and south-east (3), a step of cost `c`.
        let first = |facing: usize, vertical: i32| {
            let mut m = Mover::new(Kind::Animal, Some(at(5, 5)));
            m.start = (facing, vertical);
            let mut f = Search {
                w: &w,
                m: &m,
                s: &s,
                goal: at(9, 9),
                deep: true,
                goal_level: 0,
                nodes: Vec::new(),
                at: HashMap::new(),
                heap: Vec::new(),
                capped: false,
            };
            let start = f.start(at(5, 5), 0).unwrap();
            f.expand(start);
            let g = |tile| f.nodes[f.at[&(tile, 0)]].g;
            (g(at(6, 5)), g(at(6, 6)))
        };
        // Straight ahead at the unit's vertical direction: nothing is added.
        let (step, diagonal) = (first(2, 0).0, first(3, 0).1);
        // Another facing pays 0.1 times the step's cost, another vertical direction the same.
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(first(3, 0).0, step + 0.1 * step));
        assert!(close(first(2, 1).0, step + 0.1 * step));
        assert!(close(first(0, -1).0, step + 0.2 * step));
        assert!(close(first(2, 0).1, diagonal + 0.1 * step));
    }
}
