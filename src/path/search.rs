//! The A* search over tiles with the original's open list (pathfinding 3.3 to 3.7).

use super::cost::{clear, step};
use super::{Kind, MAX, Mover, Settings};
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
const FOUR: [(i32, i32); 4] = [(-1, 0), (0, -1), (1, 0), (0, 1)];

#[derive(Clone, Copy, PartialEq)]
enum State {
    New,
    Open,
    Closed,
}

struct Node {
    tile: usize,
    g: f64,
    h: f64,
    f: f64,
    parent: Option<usize>,
    state: State,
}

/// What a search found: the tiles from the start, and whether the last one is the goal.
pub struct Found {
    pub path: Vec<usize>,
    pub reached: bool,
}

struct Search<'a> {
    w: &'a World,
    m: &'a Mover,
    s: &'a Settings,
    goal: usize,
    nodes: Vec<Node>,
    at: HashMap<usize, usize>,
    heap: Vec<usize>,
    /// A node was refused because the cap was used up.
    capped: bool,
}

/// Whether a unit moves on the diagonals (3.2).
pub fn diagonal(m: &Mover, s: &Settings) -> bool {
    s.diagonal && m.kind != Kind::Guest && !(m.test && m.kind == Kind::Animal)
}

/// The step cost of going from `a` to its neighbour `b` in direction `d` when the step is allowed
/// (4.2), else `None`.
pub fn open(
    w: &World,
    m: &Mover,
    s: &Settings,
    (a, b, d): (usize, usize, usize),
    goal: usize,
) -> Option<u32> {
    let cost = step(w, m, a, b, d, goal);
    (clear(w, m, a, b, d, diagonal(m, s)) && w.tank_at(b).is_none() && cost < MAX).then_some(cost)
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
    fn estimate(&self, t: usize) -> f64 {
        let ((x, y), (gx, gy)) = (self.xy(t), self.xy(self.goal));
        let (dx, dy) = (f64::from(gx - x), f64::from(gy - y));
        let k = f64::from(self.s.min_cost);
        match self.s.estimate {
            0 => (dx * dx + dy * dy) * k,
            1 => (dx.abs() + dy.abs()) * k,
            2 => dx.abs().max(dy.abs()) * k,
            _ => f64::from(((dx * dx + dy * dy) as f32).sqrt() * k as f32),
        }
    }

    /// The node of a tile, made when the tile is met for the first time and the cap allows.
    fn node(&mut self, tile: usize) -> Option<usize> {
        if let Some(&i) = self.at.get(&tile) {
            return Some(i);
        }
        if self.nodes.len() >= self.s.node_max {
            self.capped = true;
            return None;
        }
        let h = self.estimate(tile);
        self.nodes.push(Node {
            tile,
            g: 0.0,
            h,
            f: h,
            parent: None,
            state: State::New,
        });
        self.at.insert(tile, self.nodes.len() - 1);
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
        let from = self.nodes[c].tile;
        let (x, y) = self.xy(from);
        for &(dx, dy) in list {
            let Some(to) = self.w.tile_index(x + dx, y + dy) else {
                continue;
            };
            let d = STEP.iter().position(|&s| s == (dx, dy)).unwrap_or(0);
            let Some(cost) = open(self.w, self.m, self.s, (from, to, d), self.goal) else {
                continue;
            };
            let Some(n) = self.node(to) else {
                continue;
            };
            let length = if d.is_multiple_of(2) {
                1.0
            } else {
                std::f64::consts::SQRT_2
            };
            let g = self.nodes[c].g + length * f64::from(cost);
            if self.nodes[n].state == State::New || g < self.nodes[n].g {
                let node = &mut self.nodes[n];
                (node.g, node.parent, node.f) = (g, Some(c), g + node.h);
                if node.state == State::Open {
                    self.rebuild();
                } else {
                    self.push(n);
                }
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
    let mut f = Search {
        w,
        m,
        s,
        goal,
        nodes: Vec::new(),
        at: HashMap::new(),
        heap: Vec::new(),
        capped: false,
    };
    let Some(start) = f.node(from) else {
        return Found {
            path: Vec::new(),
            reached: false,
        };
    };
    f.nodes[start].state = State::Closed;
    let (mut c, mut best, mut reached) = (start, start, false);
    loop {
        if f.nodes[c].h < f.nodes[best].h {
            best = c;
        }
        if f.nodes[c].tile == goal {
            (best, reached) = (c, true);
            break;
        }
        f.expand(c);
        match f.pop() {
            Some(next) if !f.capped => c = next,
            _ => break,
        }
    }
    let mut path = Vec::new();
    if reached || partial {
        let mut at = Some(best);
        while let Some(i) = at {
            path.push(f.nodes[i].tile);
            at = f.nodes[i].parent;
        }
        path.reverse();
        if path.len() == 1 {
            path.clear();
        }
    }
    Found { path, reached }
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
            nodes,
            at,
            heap: Vec::new(),
            capped: false,
        };
        for (tile, &h) in fs.iter().enumerate() {
            f.nodes.push(Node {
                tile,
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
}
