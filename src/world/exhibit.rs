//! The exhibits as fences change: a new exhibit when a piece cuts off a part of the grounds, a
//! split when it cuts an exhibit, deletion and merging when a piece goes, the choice of the gate
//! and the default names (exhibits and money 3.2 to 3.6).
//!
//! After an edit of an exhibit fence the regions are found again from the fences and matched
//! with the exhibits there were: that gives the same result as the original's check of the two
//! tiles beside the piece. What the player has to be told or asked comes out as [`Event`]s.

use super::World;
use crate::exhibit::{self, EXTERIOR, Exhibit, Exhibits, GROUNDS, Part};
use crate::place::path_shape;
use std::collections::{HashMap, HashSet};

/// The message that the exhibit is inaccessible to the staff.
pub const INACCESSIBLE: i32 = 11000;
/// The message that the entrance of the exhibit is blocked.
pub const BLOCKED: i32 = 11001;

/// What happened to the exhibits, for the game to log, to ask the name and to tell the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A new exhibit with its default name; the player names it unless `ask` is false (an undo).
    New { id: u32, ask: bool },
    /// An exhibit was cut in two: this part kept its name and gate.
    Split(u32),
    /// An exhibit opened to the zoo grounds and is gone.
    Removed(u32),
    /// Two exhibits became one: `gone` into `kept`.
    Merged { kept: u32, gone: u32 },
    /// An exhibit got a new gate.
    Gate(u32),
    /// The gate went where no keeper can pass, or none could be put: the message is about the
    /// exhibit (its name goes in the `%s`).
    Blocked { id: u32, message: i32 },
}

/// A pair of tiles of the boundary of an exhibit: one in it, one outside it, side by side.
type Pair = (usize, usize);

/// What the walk round the boundary found.
enum Walk {
    /// A good pair: the gate goes there.
    Good(Pair),
    /// The walk ended; the pair to use and the message to tell.
    Ended(Option<(Pair, Option<i32>)>),
}

impl World {
    /// The tile and side (0, 2, 4, 6) of the fence piece at position `at` of the entity list.
    pub(super) fn slot_of(&self, at: usize) -> Option<(usize, usize)> {
        let c = self.zoo.entities.get(at)?.common.as_ref()?;
        let t = self.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
        (0..4)
            .map(|d| d * 2)
            .find(|&d| self.edge(t, d) == Some(at))
            .map(|d| (t, d))
    }

    /// Whether the entity at `at` is the gate of an exhibit fence (its subtype starts with `g`).
    pub(super) fn is_door(&self, at: usize) -> bool {
        let e = &self.zoo.entities[at];
        matches!(e.class.as_str(), "fences" | "tankwall") && e.subtype.starts_with('g')
    }

    /// The tile and side of the gate of exhibit `id`, if it has one.
    pub fn gate_slot(&self, id: u32) -> Option<(usize, usize)> {
        let gate = self.exhibits.get(id)?.gate?;
        self.slot_of(self.find(gate)?)
    }

    /// The exhibit whose gate the fence piece at `at` could become with the gate tool (placing
    /// 6.4 items 1 to 3): the piece is of an exhibit fence type that has a gate, the tile across
    /// is on the map and its slot on the line is empty, one tile of the line is in the zoo
    /// grounds (the exterior is part of the original's grounds region) and the other in an
    /// exhibit, and the piece is not already that exhibit's gate. The tank test (item 4) waits
    /// for tank exhibits.
    pub fn gate_for(&self, at: usize) -> Option<u32> {
        self.shapes
            .get(at)
            .copied()
            .flatten()
            .filter(|s| s.fence && s.gated && !s.wall)?;
        let (t, d) = self.slot_of(at)?;
        let n = self.neighbour(t, d)?;
        if self.edge(n, (d + 4) % 8).is_some() {
            return None;
        }
        let grounds = |r: u32| r == GROUNDS || r == EXTERIOR;
        let (a, b) = (self.exhibits.region(t)?, self.exhibits.region(n)?);
        let exhibit = match (grounds(a), grounds(b)) {
            (true, false) => b,
            (false, true) => a,
            _ => return None,
        };
        let id = self.zoo.entities[at].common.as_ref()?.id;
        (self.exhibits.get(exhibit)?.gate != Some(id)).then_some(exhibit)
    }

    /// Moves the gate of exhibit `id` to the fence piece with id `piece` (placing 6.4): the old
    /// gate becomes a fence piece again, the piece a gate made healthy (`life` and `strength` of
    /// its new type), and the undo list is cleared. Whether the piece could become a gate.
    pub fn move_gate(&mut self, id: u32, piece: u32, life: u32, strength: u32) -> bool {
        let Some(at) = self.find(piece) else {
            return false;
        };
        let Some(old) = self.exhibits.get(id).map(|e| e.gate) else {
            return false;
        };
        if !self.make_door(at) {
            return false;
        }
        if let Some(old) = old.and_then(|g| self.find(g)).filter(|&o| o != at) {
            self.make_fence(old);
        }
        if let Some(f) = self.zoo.entities[at]
            .common
            .as_mut()
            .and_then(|c| c.fence.as_mut())
        {
            (f.life, f.strength) = (life, strength);
        }
        if let Some(e) = self.exhibits.get_mut(id) {
            e.gate = Some(piece);
        }
        self.undo.clear();
        self.dirty = true;
        self.events.push(Event::Gate(id));
        true
    }

    /// Gives exhibit `id` this name.
    pub fn rename(&mut self, id: u32, name: String) {
        if let Some(e) = self.exhibits.get_mut(id) {
            e.name = name;
        }
    }

    /// The word before the number of a default name (string 6208 of the language library).
    pub fn set_exhibit_word(&mut self, word: String) {
        self.word = word;
    }

    /// Which terrain types are water, for the gate walk: a keeper does not swim.
    pub fn set_water(&mut self, water: Vec<bool>) {
        self.water = water;
    }

    /// Makes the piece with id `piece` the gate of exhibit `id`, for a test.
    #[cfg(test)]
    pub fn set_gate(&mut self, id: u32, piece: u32) {
        let at = self.find(piece).unwrap();
        assert!(self.make_door(at));
        self.exhibits.get_mut(id).unwrap().gate = Some(piece);
    }

    /// The new exhibits that wait for their gate: the name dialog is open for them (exhibits and
    /// money 3.6).
    pub fn waiting(&self) -> Vec<u32> {
        self.waiting.iter().map(|w| w.0).collect()
    }

    /// The new exhibit `id` was named: its gate is chosen now (exhibits and money 3.2, 3.4).
    pub fn give_gate(&mut self, id: u32) {
        if let Some(i) = self.waiting.iter().position(|w| w.0 == id) {
            let (id, start, placed) = self.waiting.remove(i);
            self.choose_gate(id, start, placed);
        }
    }

    /// What happened to the exhibits since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Turns the piece at `at` back into an ordinary fence of its type.
    fn make_fence(&mut self, at: usize) {
        if self.is_door(at) {
            self.zoo.entities[at].subtype = "f".into();
            self.dirty = true;
        }
    }

    /// Turns the piece at `at` into the gate of its type, if the type has one; whether it is one.
    fn make_door(&mut self, at: usize) -> bool {
        let gated = self.shapes[at].is_some_and(|s| s.gated && !s.wall);
        if gated && !self.is_door(at) {
            self.zoo.entities[at].subtype = "g".into();
            self.dirty = true;
        }
        gated || self.is_door(at)
    }

    /// Whether the exhibit gate entity `gate` has gone from the map.
    fn missing(&self, e: &Exhibit) -> bool {
        e.gate.is_some_and(|g| self.find(g).is_none())
    }

    /// The exhibits after the edit of an exhibit fence piece on side `d` of tile `a` (`hint`; none
    /// for an undo, which also makes `quiet`: no gate is chosen and the counter stays).
    pub(super) fn settle(&mut self, hint: Option<(usize, usize)>, quiet: bool) {
        let old = std::mem::take(&mut self.exhibits);
        let parts: Vec<Part> = exhibit::parts(self)
            .into_iter()
            .filter(|p| !p.grounds)
            .collect();
        let ra = hint.and_then(|(a, _)| old.region(a));
        let rb = hint
            .and_then(|(a, d)| self.neighbour(a, d))
            .and_then(|b| old.region(b));
        // The old exhibits each part holds tiles of, A's side first.
        let olds: Vec<Vec<u32>> = parts
            .iter()
            .map(|p| {
                let mut v: Vec<u32> = p
                    .tiles
                    .iter()
                    .filter_map(|&t| old.region(t))
                    .filter(|&r| r != GROUNDS && r != EXTERIOR)
                    .collect();
                v.sort_unstable();
                v.dedup();
                v.sort_by_key(|&x| match Some(x) {
                    s if s == ra => 0,
                    s if s == rb => 1,
                    _ => 2,
                });
                v
            })
            .collect();
        // Merges: each part with several old exhibits keeps the survivor.
        let mut winner: Vec<Option<u32>> = Vec::new();
        let mut lost: Vec<(u32, u32)> = Vec::new();
        for list in &olds {
            let mut it = list.iter().copied();
            let mut w = it.next();
            for y in it {
                let x = w.unwrap_or(y);
                let kept = self.survivor(&old, x, y);
                lost.push((if kept == x { y } else { x }, kept));
                w = Some(kept);
            }
            winner.push(w);
        }
        // Splits: an old exhibit held by several parts stays in one of them.
        for e in old.list() {
            let held: Vec<usize> = (0..parts.len())
                .filter(|&i| winner[i] == Some(e.id))
                .collect();
            if held.len() < 2 {
                continue;
            }
            let gate = e.gate.and_then(|g| self.slot_of(self.find(g)?));
            let side = gate.map(|(t, d)| match old.region(t) == Some(e.id) {
                true => t,
                false => self.neighbour(t, d).unwrap_or(t),
            });
            let a = hint.map(|h| h.0).filter(|_| ra == Some(e.id));
            let keeper = [side, a]
                .into_iter()
                .flatten()
                .find_map(|t| held.iter().copied().find(|&i| parts[i].tiles.contains(&t)))
                .unwrap_or(held[0]);
            for &i in held.iter().filter(|&&i| i != keeper) {
                winner[i] = None;
            }
            self.events.push(Event::Split(e.id));
        }
        // The new list.
        let mut next = old.blank(self.zoo.tiles.len());
        let mut fresh = Vec::new();
        let mut regate = Vec::new();
        let mut kept_ids = HashSet::new();
        for (part, w) in parts.iter().zip(&winner) {
            match w.and_then(|id| old.get(id)) {
                Some(e) => {
                    kept_ids.insert(e.id);
                    next.keep(e.clone(), part);
                    if self.missing(e) {
                        regate.push(e.id);
                    }
                }
                None => {
                    let number = next.counter;
                    next.counter += u32::from(!quiet);
                    let name = format!("{} {number}", self.word);
                    let id = next.add(part, name);
                    fresh.push(id);
                    self.events.push(Event::New { id, ask: !quiet });
                }
            }
        }
        next.list_mut().sort_by_key(|e| e.id);
        // The exhibits that are gone: their gate is a fence again.
        let into: HashMap<u32, u32> = lost.iter().copied().collect();
        for e in old.list().iter().filter(|e| !kept_ids.contains(&e.id)) {
            if let Some(at) = e.gate.and_then(|g| self.find(g)) {
                self.make_fence(at);
            }
            self.events.push(match into.get(&e.id) {
                Some(&kept) => Event::Merged { kept, gone: e.id },
                None => Event::Removed(e.id),
            });
        }
        next.exterior(self);
        self.exhibits = next;
        for &id in &regate {
            if let Some(e) = self.exhibits.get_mut(id) {
                e.gate = None;
            }
        }
        let Some((a, d)) = hint.filter(|_| !quiet) else {
            return;
        };
        let Some(b) = self.neighbour(a, d) else {
            return;
        };
        for id in fresh {
            let start = match self.exhibits.region(b) == Some(id) {
                true => (b, a),
                false => (a, b),
            };
            self.waiting.push((id, start, a));
        }
        // The gate of an exhibit went but its line is still closed: a new one on that line.
        if self.closed(a, d) {
            for id in regate {
                let line = [(a, b), (b, a)];
                if let Some(&(me, other)) = line
                    .iter()
                    .find(|(me, _)| self.exhibits.region(*me) == Some(id))
                {
                    self.regate(id, me, other);
                }
            }
        }
    }

    /// Which of two exhibits that became one stays (exhibits and money 3.3).
    fn survivor(&self, old: &Exhibits, x: u32, y: u32) -> u32 {
        let (ex, ey) = (old.get(x), old.get(y));
        if ex.is_some_and(|e| self.missing(e)) {
            return y;
        }
        if ey.is_some_and(|e| self.missing(e)) {
            return x;
        }
        match (self.opens_into(old, x, y), self.opens_into(old, y, x)) {
            (true, false) => return y,
            (false, true) => return x,
            _ => {}
        }
        let size = |e: Option<&Exhibit>| e.map_or(0, |e| e.tiles);
        if size(ex) > size(ey) { x } else { y }
    }

    /// Whether going out of `p` through its gate, then out of the exhibit reached through its
    /// gate and so on, reaches `q` before the zoo grounds, an exhibit without a gate or one
    /// already visited.
    fn opens_into(&self, old: &Exhibits, p: u32, q: u32) -> bool {
        let mut seen = vec![p];
        let mut at = p;
        loop {
            let across = old.get(at).and_then(|e| e.gate).and_then(|g| {
                let (t, d) = self.slot_of(self.find(g)?)?;
                let (here, there) = (old.region(t)?, old.region(self.neighbour(t, d)?)?);
                Some(if here == at { there } else { here })
            });
            match across {
                Some(next) if next == q => return true,
                Some(next) if next != GROUNDS && next != EXTERIOR && !seen.contains(&next) => {
                    seen.push(next);
                    at = next;
                }
                _ => return false,
            }
        }
    }

    /// Exhibit `id` lost its gate on the line between its tile `me` and `other`.
    fn regate(&mut self, id: u32, me: usize, other: usize) {
        if self.exhibits.region(other) == Some(GROUNDS) {
            // The piece left on the line is the gate, whichever of the two slots it is on.
            let slot =
                |from: usize, to: usize| self.side_to(from, to).and_then(|d| self.edge(from, d));
            let piece = slot(other, me).or(slot(me, other));
            if let Some(at) =
                piece.filter(|&p| self.shapes[p].is_some_and(|s| s.fence) && self.make_door(p))
            {
                let gate = self.zoo.entities[at].common.as_ref().map(|c| c.id);
                if let Some(e) = self.exhibits.get_mut(id) {
                    e.gate = gate;
                }
                self.events.push(Event::Gate(id));
            }
        } else {
            self.choose_gate(id, (me, other), me);
        }
    }

    /// Whether keepers cannot stand on tile `t`: outside the zoo, or water.
    fn dear(&self, t: usize) -> bool {
        !self.in_zoo(t)
            || self
                .water
                .get(usize::from(self.zoo.tiles[t].terrain))
                .copied()
                .unwrap_or(false)
    }

    /// The side of tile `from` towards tile `to`.
    fn side_to(&self, from: usize, to: usize) -> Option<usize> {
        (0..4)
            .map(|d| d * 2)
            .find(|&d| self.neighbour(from, d) == Some(to))
    }

    /// The pieces on the line of a pair: the one on the slot of each tile.
    fn line(&self, (i, o): Pair) -> [Option<usize>; 2] {
        [
            self.side_to(i, o).and_then(|d| self.edge(i, d)),
            self.side_to(o, i).and_then(|d| self.edge(o, d)),
        ]
    }

    /// The code of a pair (exhibits and money 3.4).
    fn code(&self, pair: Pair) -> u8 {
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
        } else if self.dear(pair.0) || self.dear(pair.1) {
            4
        } else if self.quadrant(pair.0, 3).is_some() || self.quadrant(pair.1, 3).is_some() {
            3
        } else {
            0
        }
    }

    /// The tiles a keeper can walk to from the zoo entrance: not through an exhibit fence or a
    /// zoo wall, unless the piece is a gate or broken.
    fn reach(&self) -> Vec<bool> {
        let mut seen = vec![false; self.zoo.tiles.len()];
        let (ex, ey) = self.zoo.entrance;
        let start = i32::try_from(ex)
            .ok()
            .zip(i32::try_from(ey).ok())
            .and_then(|(x, y)| self.tile_index(x, y))
            .filter(|&t| !self.dear(t));
        let open = |piece: Option<usize>| {
            piece.is_none_or(|p| {
                self.is_door(p)
                    || self.zoo.entities[p]
                        .common
                        .as_ref()
                        .and_then(|c| c.fence.as_ref())
                        .is_some_and(|f| f.life < 1)
                    || self.shapes[p].is_some_and(|s| !s.fence && !s.wall)
            })
        };
        let mut todo: Vec<usize> = start.into_iter().collect();
        todo.iter().for_each(|&t| seen[t] = true);
        while let Some(t) = todo.pop() {
            for d in [0, 2, 4, 6] {
                let Some(n) = self.neighbour(t, d) else {
                    continue;
                };
                if !seen[n]
                    && !self.dear(n)
                    && open(self.edge(t, d))
                    && open(self.edge(n, (d + 4) % 8))
                {
                    seen[n] = true;
                    todo.push(n);
                }
            }
        }
        seen
    }

    /// The step of the walk round the boundary of exhibit `id` from a pair; `None` at a pinch.
    fn step(&self, id: u32, (i, o): Pair) -> Option<Pair> {
        let inside = |t: usize| self.exhibits.region(t) == Some(id);
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
    fn choose_gate(&mut self, id: u32, start: Pair, placed: usize) {
        let reach = self.reach();
        let good = |w: &World, pair: Pair| {
            reach[pair.1]
                && w.exhibits.region(pair.1) == Some(GROUNDS)
                && [pair.0, pair.1]
                    .iter()
                    .all(|&t| path_shape(w.zoo.tiles[t].shape))
        };
        let reachable = |w: &World, pair: Pair| w.code(pair) == 0 && reach[pair.1];
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
                let code = self.code(cur);
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
        self.events.push(Event::Gate(id));
        if let Some(message) = message {
            self.events.push(Event::Blocked { id, message });
        }
    }
}
