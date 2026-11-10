//! The choice of the gate of an exhibit, and the walks of the keeper between its tiles.

use super::*;

impl World {
    /// Whether the relaxed keeper of the gate choice cannot stand on tile `t` (pathfinding 8):
    /// outside the zoo, on water or in a tank, which the marine specialist of a tank (`diver`)
    /// swims in, or on a land tile whose four quarters objects hold.
    fn dear(&self, t: usize, diver: bool) -> bool {
        path::terrain(self, &Mover::keeper(diver, true), t, None) >= path::MAX
    }

    /// Whether tile `t` has a terrain type that is water.
    pub(crate) fn is_water(&self, t: usize) -> bool {
        let terrain = usize::from(self.zoo.tiles[t].terrain);
        self.water.get(terrain).copied().unwrap_or(false)
    }

    /// The side of tile `from` towards tile `to`.
    pub(in crate::world) fn side_to(&self, from: usize, to: usize) -> Option<usize> {
        (0..4)
            .map(|d| d * 2)
            .find(|&d| self.neighbour(from, d) == Some(to))
    }

    /// The pieces on the line of a pair: the one on the slot of each tile.
    pub(in crate::world) fn line(&self, (i, o): Pair) -> [Option<usize>; 2] {
        [
            self.side_to(i, o).and_then(|d| self.edge(i, d)),
            self.side_to(o, i).and_then(|d| self.edge(o, d)),
        ]
    }

    /// The code of a pair (exhibits and money 3.4).
    pub(super) fn code(&self, pair: Pair, diver: bool) -> u8 {
        let pieces: Vec<usize> = self.line(pair).into_iter().flatten().collect();
        let code = if pieces
            .iter()
            .any(|&p| self.shapes[p].is_some_and(|s| s.wall))
        {
            4
        } else if pieces.iter().any(|&p| self.is_door(p)) {
            2
        } else {
            u8::from(pieces.len() == 2)
        };
        // Water, the outside of the zoo and objects count only while nothing else has.
        if code != 0 {
            code
        } else if self.dear(pair.0, diver) || self.dear(pair.1, diver) {
            4
        } else if self.quadrant(pair.0, 3).is_some()
            || self.quadrant(pair.1, 3).is_some()
            || (diver && (0..4).any(|q| self.quadrant(pair.0, q).is_some()))
        {
            3
        } else {
            0
        }
    }

    /// Whether the search of the path finder, from the zoo entrance to tile `out` with the relaxed
    /// temporary keeper and no partial path, finds it (pathfinding 8): no request, no unreachable
    /// list, and the node cap applies.
    fn reaches(&self, diver: bool, out: usize) -> bool {
        let (ex, ey) = self.zoo.entrance;
        i32::try_from(ex)
            .ok()
            .zip(i32::try_from(ey).ok())
            .and_then(|(x, y)| self.tile_index(x, y))
            .is_some_and(|from| {
                path::search(
                    self,
                    &Mover::keeper(diver, true),
                    &self.units.finder.last(),
                    from,
                    out,
                    false,
                )
                .reached
            })
    }

    /// The step of the walk round the boundary of exhibit `id` from a pair; `None` at a pinch.
    pub(in crate::world) fn step(&self, id: u32, pair: Pair) -> Option<Pair> {
        self.step_in(&|t| self.exhibits.region(t) == Some(id), pair)
    }

    /// The same step for the exhibit whose tiles `inside` tells.
    pub(in crate::world) fn step_in(
        &self,
        inside: &dyn Fn(usize) -> bool,
        (i, o): Pair,
    ) -> Option<Pair> {
        let d = self.side_to(o, i)?;
        let s = (d + 6) % 8;
        let (i2, o2) = (self.neighbour(i, s), self.neighbour(o, s));
        let Some(o2) = o2 else {
            // The map edge closes the exhibit like a fence: go on along it, turning at the
            // corners (at most four times).
            let (mut t, mut dir) = (i, d);
            for _ in 0..5 {
                loop {
                    match self.neighbour(t, dir) {
                        Some(n) if inside(n) => t = n,
                        Some(n) => return Some((t, n)),
                        None => break,
                    }
                }
                dir = (dir + 2) % 8;
            }
            return None;
        };
        match (i2.map(inside)?, inside(o2)) {
            (true, false) => Some((i2?, o2)),
            (true, true) => Some((o2, o)),
            (false, false) => Some((i, i2?)),
            (false, true) => None,
        }
    }

    /// Puts the gate of exhibit `id`, the walk starting at `start`; `placed` is the tile the
    /// player placed the piece on (exhibits and money 3.4).
    pub(super) fn choose_gate(&mut self, id: u32, start: Pair, placed: usize) {
        let diver = self.exhibits.get(id).is_some_and(|e| e.tank.is_some());
        let good = |w: &World, pair: Pair| {
            w.exhibits.region(pair.1) == Some(GROUNDS)
                && [pair.0, pair.1]
                    .iter()
                    .all(|&t| path_shape(w.zoo.tiles[t].shape))
                && w.reaches(diver, pair.1)
        };
        let reachable =
            |w: &World, pair: Pair| w.code(pair, diver) == 0 && w.reaches(diver, pair.1);
        let mut seen = HashSet::new();
        let (mut last, mut zero, mut first_code) = (None, None, None);
        let mut cur = start;
        let walk = loop {
            if !seen.insert(cur) {
                break Walk::Ended(match (last, zero, first_code) {
                    (Some(p), _, _) => Some((p, None)),
                    (_, Some(p), _) => Some((p, Some(INACCESSIBLE))),
                    (_, _, Some(1 | 3)) => Some((start, Some(BLOCKED))),
                    (_, _, Some(4)) => Some((start, Some(INACCESSIBLE))),
                    (_, _, Some(_)) => Some((start, None)),
                    _ => None,
                });
            }
            let here = self.in_zoo(cur.1);
            if here {
                let code = self.code(cur, diver);
                if cur == start {
                    first_code = Some(code);
                }
                if code == 0 && good(self, cur) {
                    break Walk::Good(cur);
                }
                if reachable(self, cur) {
                    last = Some(cur);
                } else if code == 0 && zero.is_none() {
                    zero = Some(cur);
                }
            }
            match self.step(id, cur) {
                Some(next) => cur = next,
                // At a pinch the fallbacks are dropped: only the pair just left, if it was
                // reachable, gets the gate.
                None => {
                    break Walk::Ended((here && reachable(self, cur)).then_some((cur, None)));
                }
            }
        };
        let (pair, message) = match walk {
            Walk::Good(pair) => (pair, None),
            Walk::Ended(Some(found)) => found,
            Walk::Ended(None) => return,
        };
        let [mine, other] = self.line(pair);
        let piece = match (mine, other) {
            (Some(m), Some(o)) => Some(if placed == pair.1 { o } else { m }),
            (m, o) => m.or(o),
        };
        let Some(at) = piece.filter(|&p| self.make_door(p)) else {
            return;
        };
        let gate = self.zoo.entities[at].common.as_ref().map(|c| c.id);
        if let Some(e) = self.exhibits.get_mut(id) {
            e.gate = gate;
        }
        gate.into_iter().for_each(|g| self.update_walls_of(g));
        self.events.push(Event::Gate(id));
        if let Some(message) = message {
            self.events.push(Event::Blocked { id, message });
        }
    }
}
