//! The exhibits as fences change: a new exhibit when a piece cuts off a part of the grounds, a
//! split when it cuts an exhibit, deletion and merging when a piece goes, the choice of the gate
//! and the default names (exhibits and money 3.2 to 3.6).
//!
//! After an edit of an exhibit fence the regions are found again from the fences and matched
//! with the exhibits there were: that gives the same result as the original's check of the two
//! tiles beside the piece. What the player has to be told or asked comes out as [`Event`]s.

use super::World;
use crate::exhibit::{self, EXTERIOR, Exhibit, Exhibits, GROUNDS, Part, Tank};
use crate::path::{self, Mover};
use crate::place::path_shape;
use crate::timing::Date;
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
    /// An exhibit (a tank too) is gone with these tiles: what it leaves calls a worker (staff 8.2).
    Left(Vec<usize>),
    /// Two exhibits became one: `gone` into `kept`.
    Merged { kept: u32, gone: u32 },
    /// An exhibit got a new gate.
    Gate(u32),
    /// A new tank has objects it does not allow: the player is asked (string 10552) and the tank
    /// is named after the answer.
    Objects(u32),
    /// A tank got its water values.
    Tank(u32),
    /// A filled tank with a gate got a new wall height: its animals react to the new depth
    /// (habitat rating 4). The levels are the full levels of the old and of the new height.
    Depth { id: u32, from: u32, to: u32 },
    /// The gate went where no keeper can pass, or none could be put: the message is about the
    /// exhibit (its name goes in the `%s`).
    Blocked { id: u32, message: i32 },
}

/// A pair of tiles of the boundary of an exhibit: one in it, one outside it, side by side.
pub(super) type Pair = (usize, usize);

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
    pub fn is_door(&self, at: usize) -> bool {
        let e = &self.zoo.entities[at];
        matches!(e.class.as_str(), "fences" | "tankwall") && e.subtype.starts_with('g')
    }

    /// The tile and side of the gate of exhibit `id`, if it has one.
    pub fn gate_slot(&self, id: u32) -> Option<(usize, usize)> {
        let gate = self.exhibits.get(id)?.gate?;
        self.slot_of(self.find(gate)?)
    }

    /// Takes the gate of each exhibit from its record in the file (exhibits and money 3.8, zoo
    /// format 4.1): the piece in the stored tile's edge slot of the stored side, if any. Its
    /// statistics, creation date and time last serviced come with it, and its name when the file was written in the
    /// language of the running game (zoo format 2 rule 5).
    pub(super) fn load_gates(&mut self) {
        let records = self.zoo.habitat_tiles.iter().zip(&self.zoo.habitat_gates);
        for ((&tile, &((x, y), side)), info) in records.zip(&self.zoo.habitat_info) {
            let piece = usize::try_from(side)
                .ok()
                .filter(|d| d % 2 == 0 && *d < 8)
                .zip(self.tile_index(x, y))
                .and_then(|(d, t)| self.edge(t, d))
                .and_then(|at| self.zoo.entities[at].common.as_ref().map(|c| c.id));
            let id = self
                .tile_index(tile.0, tile.1)
                .and_then(|t| self.exhibits.region(t))
                .filter(|&r| r != GROUNDS && r != EXTERIOR);
            let stat = |k: usize| f32::from_bits(info.stats[k]);
            let (this, last, total) = (0, 1, 2);
            if let Some(e) = id.and_then(|id| self.exhibits.get_mut(id)) {
                e.gate = piece;
                if self.zoo.locale == self.language.number {
                    e.name.clone_from(&info.name);
                }
                // Stored this month, last month, total; kept last month, this month, total.
                e.stats.donations = [stat(last), stat(this), stat(total)];
                e.stats.upkeep = [stat(3 + last), stat(3 + this), stat(3 + total)];
                let count = |k: usize| info.stats[6 + k];
                e.stats.count = [count(last), count(this), count(total)];
                e.stats.created = Some(Date::from_file_time(info.created));
                e.rating.serviced = u64::from(info.serviced);
            }
        }
    }

    /// While a zoo loads, the gate walk runs for an exhibit whose record stores no gate (3.8),
    /// from the line its file closes last. Nothing is told about it.
    pub(super) fn walk_loaded(&mut self) {
        let records: Vec<(i32, i32)> = (self.zoo.habitat_tiles.iter())
            .zip(&self.zoo.habitat_gates)
            .filter(|(_, ((x, _), _))| *x < 0)
            .map(|(&tile, _)| tile)
            .collect();
        for (x, y) in records {
            let id = self
                .tile_index(x, y)
                .and_then(|t| self.exhibits.region(t))
                .filter(|&r| r != GROUNDS && r != EXTERIOR);
            let line = id
                .and_then(|id| self.exhibits.get(id))
                .and_then(|e| exhibit::closed_last(self, &e.order));
            if let Some((id, (inside, across, placed))) = id.zip(line) {
                let told = self.events.len();
                self.choose_gate(id, (inside, across), placed);
                self.events.truncate(told);
            }
        }
    }

    /// The exhibit whose gate the fence piece at `at` could become with the gate tool (placing
    /// 6.4 items 1 to 3; item 4 is [`World::tank_holds_object`]): the piece is of an exhibit fence type that has a gate, the tile across
    /// is on the map and its slot on the line is empty, one tile of the line is in the zoo
    /// grounds (the exterior is part of the original's grounds region) and the other in an
    /// exhibit, and the piece is not already that exhibit's gate.
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

    /// Whether a keeper passes where the fence piece at `at` could be the gate: the pair test of
    /// the piece's tile and the tile across gives code 0 and a cost below 10 (placing 6.4, exhibits
    /// and money 3.4), for the keeper of the path finder that is not relaxed (pathfinding 8): the
    /// marine specialist when the piece's tile is in a tank, else the zookeeper.
    pub fn keeper_passes(&self, at: usize) -> bool {
        let Some((t, d)) = self.slot_of(at) else {
            return false;
        };
        let Some(n) = self.neighbour(t, d) else {
            return false;
        };
        let diver = self.tank_at(t).is_some();
        let keeper = Mover::keeper(diver, false);
        self.code((t, n), diver) == 0 && path::pair_cost(self, &keeper, t, n) < 10
    }

    /// Whether a quadrant of the tank tile of the fence piece at `at` holds an object: item 4 of
    /// the gate tool's validity (placing 6.4), which refuses the gate and hides its cost text.
    pub fn tank_holds_object(&self, at: usize) -> bool {
        self.slot_of(at).is_some_and(|(t, _)| {
            self.tank_at(t).is_some() && (0..4).any(|q| self.quadrant(t, q).is_some())
        })
    }

    /// Moves the gate of exhibit `id` to the fence piece with id `piece` (placing 6.4): the old
    /// gate becomes a fence piece again, the piece a gate made healthy (`life` and `strength` of
    /// its new type), and the undo list is cleared. Whether the piece could become a gate.
    pub fn move_gate(&mut self, id: u32, piece: u32, life: i32, strength: u32) -> bool {
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

    /// A loaded zoo whose stored locale is not the install language keeps no exhibit name: each
    /// exhibit that has a record gets the default name of its kind in the install language, the
    /// word and the counter (exhibits and money 3.6). Made after the tanks and their shows are known.
    pub(super) fn name_loaded(&mut self) {
        if self.zoo.locale == self.language.number {
            return;
        }
        let stored: HashSet<u32> = (self.zoo.habitat_tiles.iter())
            .filter_map(|&(x, y)| self.tile_index(x, y))
            .filter_map(|t| self.exhibits.region(t))
            .filter(|&r| r != GROUNDS && r != EXTERIOR)
            .collect();
        let named = (self.exhibits.list_mut().iter_mut()).filter(|e| stored.contains(&e.id));
        for (counter, e) in (1..).zip(named) {
            let kind = match &e.tank {
                Some(t) => 1 + usize::from(t.show),
                None => 0,
            };
            e.name = format!("{} {counter}", self.words[kind]);
        }
    }

    /// Records that exhibit `id` was made on this date.
    pub fn set_created(&mut self, id: u32, date: Date) {
        if let Some(e) = self.exhibits.get_mut(id) {
            e.stats.created = Some(date);
        }
    }

    /// The words before the number of a default name: exhibit, tank and show tank (strings 6208
    /// to 6210 of the language library); a missing one (empty) stays as it was.
    pub fn set_exhibit_words(&mut self, words: [String; 3]) {
        for (word, new) in self.words.iter_mut().zip(words) {
            if !new.is_empty() {
                *word = new;
            }
        }
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
    pub(super) fn make_fence(&mut self, at: usize) {
        if self.is_door(at) {
            self.zoo.entities[at].subtype = "f".into();
            self.dirty = true;
        }
    }

    /// Turns the piece at `at` into the gate of its type, if the type has one; whether it is one.
    pub(super) fn make_door(&mut self, at: usize) -> bool {
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
        // Splits: an old exhibit held by several parts stays in one of them. The parts that leave
        // it remember where they come from, and a tank that kept a part has its walls to bring
        // to its side again.
        let mut split_from: HashMap<usize, u32> = HashMap::new();
        let mut split_land: Vec<Vec<usize>> = Vec::new();
        let mut split_kept: Vec<u32> = Vec::new();
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
                split_from.insert(i, e.id);
            }
            split_kept.extend(e.tank.map(|_| e.id));
            if e.tank.is_none() {
                split_land.push(held);
            }
            self.events.push(Event::Split(e.id));
        }
        // The kind of each part: a tank (a show tank or not) or land.
        let other = |t: usize| {
            old.region(t)
                .and_then(|r| old.get(r))
                .is_some_and(|e| e.tank.is_some())
        };
        let kinds: Vec<Option<bool>> = parts
            .iter()
            .map(|p| self.tank_test(&p.tiles, &other, quiet))
            .collect();
        // A land exhibit cut in two with a tank for a part names no land part (3.2 step 4): the
        // tank is named when it is made.
        let silent: Vec<usize> = (split_land.iter())
            .filter(|held| held.iter().any(|&i| kinds[i].is_some()))
            .flatten()
            .copied()
            .collect();
        // The morph: an exhibit whose kind no longer fits its walls is replaced by a new one of
        // the other kind on the same tiles (3.9).
        let mut morphed: HashMap<usize, u32> = HashMap::new();
        for (i, w) in winner.iter_mut().enumerate() {
            let tank = |id: u32| old.get(id).is_some_and(|e| e.tank.is_some());
            if let Some(id) = w.filter(|&id| tank(id) != kinds[i].is_some()) {
                morphed.insert(i, id);
                *w = None;
            }
        }
        // The new list.
        let mut next = old.blank(self.zoo.tiles.len());
        let mut fresh = Vec::new();
        let mut tanks = Vec::new();
        let mut starts: Vec<(u32, Pair)> = Vec::new();
        let mut regate = Vec::new();
        let mut kept_ids = HashSet::new();
        // The tiles beside the piece: the fill of a region that changed starts at the one it
        // holds (3.1).
        let beside: Vec<usize> = hint
            .into_iter()
            .flat_map(|(a, d)| [Some(a), self.neighbour(a, d)])
            .flatten()
            .collect();
        for (i, ((part, w), kind)) in parts.iter().zip(&winner).zip(&kinds).enumerate() {
            let filled = |from: Option<usize>, was: Option<u32>| {
                let start = from.or_else(|| {
                    let held = |t: &usize| part.tiles.contains(t);
                    let mine = |t: &usize| was.is_some() && old.region(*t) == was;
                    let side = beside.iter().copied().filter(held);
                    side.clone().find(mine).or(side.clone().next())
                });
                let start = start.or(part.tiles.iter().copied().min());
                Part {
                    tiles: start.map_or(Vec::new(), |s| exhibit::fill(self, s)),
                    grounds: false,
                }
            };
            match w.and_then(|id| old.get(id)) {
                Some(e) => {
                    kept_ids.insert(e.id);
                    let same = e.order.len() == part.tiles.len()
                        && part.tiles.iter().all(|&t| old.region(t) == Some(e.id));
                    let part = match same {
                        true => &Part {
                            tiles: e.order.clone(),
                            grounds: false,
                        },
                        false => &filled(None, Some(e.id)),
                    };
                    next.keep(e.clone(), part);
                    if self.missing(e) {
                        regate.push(e.id);
                    }
                }
                None => {
                    let merged = (olds[i].len() > 1)
                        .then(|| morphed.get(&i).map(|&x| filled(None, Some(x)).tiles))
                        .flatten();
                    let start = morphed
                        .get(&i)
                        .and_then(|&x| self.morph_start(&old, x, part, &beside, merged.as_deref()));
                    let part = &filled(start.map(|s| s.0), None);
                    // The land part of such a cut is the exhibit's own: its name, no number.
                    let silent = kind.is_none() && silent.contains(&i);
                    let number = next.counter;
                    next.counter += u32::from(!quiet && !silent);
                    let word = &self.words[kind.map_or(0, |show| 1 + usize::from(show))];
                    let name = match split_from.get(&i).and_then(|&x| old.get(x)) {
                        Some(x) if silent => x.name.clone(),
                        _ => format!("{word} {number}"),
                    };
                    let id = next.add(part, name);
                    fresh.push(id);
                    match kind {
                        // A tank has its ground, walls and objects to deal with before its name.
                        Some(_) => {
                            let from = split_from.get(&i).and_then(|&x| old.get(x));
                            tanks.push((id, from.and_then(|e| e.tank)));
                        }
                        None => self.events.push(Event::New {
                            id,
                            ask: !quiet && !silent,
                        }),
                    }
                    if let Some(start) = start {
                        starts.push((id, start));
                    }
                }
            }
        }
        next.list_mut().sort_by_key(|e| e.id);
        // The exhibits that are gone: their gate is a fence again.
        let into: HashMap<u32, u32> = lost.iter().copied().collect();
        for e in old.list().iter().filter(|e| !kept_ids.contains(&e.id)) {
            self.units.thoughts.drop_exhibit(e.id);
            if let Some(at) = e.gate.and_then(|g| self.find(g)) {
                self.make_fence(at);
            }
            self.events.push(Event::Left(e.order.clone()));
            self.events.push(match into.get(&e.id) {
                Some(&kept) => Event::Merged { kept, gone: e.id },
                None => Event::Removed(e.id),
            });
        }
        next.exterior(self);
        // A morph refills the region across its start pair from that tile (3.1).
        for &(id, (_, out)) in &starts {
            if let Some(r) = next
                .region(out)
                .filter(|&r| r != GROUNDS && r != EXTERIOR && r != id)
            {
                next.reorder(r, exhibit::fill(self, out));
            }
        }
        // The tiles of the tanks that are gone, which keep their ground as sand (3.9).
        let gone: Vec<usize> = old
            .list()
            .iter()
            .filter(|e| e.tank.is_some() && !kept_ids.contains(&e.id) && !quiet)
            .flat_map(|e| (0..self.zoo.tiles.len()).filter(|&t| old.region(t) == Some(e.id)))
            .collect();
        self.exhibits = next;
        self.edits += 1;
        let mut changed = self.retank(quiet);
        if !quiet {
            self.paint(&gone, false);
            changed.extend(lost.iter().map(|l| l.1).chain(split_kept.iter().copied()));
            self.floors(&changed);
        }
        for (id, inherited) in tanks {
            self.make_tank(id, inherited, !quiet);
        }
        if !quiet && !split_kept.is_empty() {
            for &id in &split_kept {
                self.rewall(id);
            }
            let mut changed = self.retank(false);
            changed.extend(split_kept);
            self.floors(&changed);
        }
        for &id in &regate {
            if let Some(e) = self.exhibits.get_mut(id) {
                e.gate = None;
            }
        }
        if !quiet {
            self.merge_tanks(&old, &lost);
        }
        self.relink();
        let Some((a, d)) = hint.filter(|_| !quiet) else {
            return;
        };
        let Some(b) = self.neighbour(a, d) else {
            return;
        };
        for id in fresh {
            let start = starts.iter().find(|s| s.0 == id).map_or_else(
                || match self.exhibits.region(b) == Some(id) {
                    true => (b, a),
                    false => (a, b),
                },
                |s| s.1,
            );
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

    /// The water of the tanks that merged (3.10): `lost` pairs each exhibit that went with the one
    /// that took it; a merged tank without a gate keeps its own values.
    fn merge_tanks(&mut self, old: &Exhibits, lost: &[(u32, u32)]) {
        let mut now: HashMap<u32, (Tank, usize)> = HashMap::new();
        for &(gone, kept) in lost {
            let value = |now: &HashMap<u32, (Tank, usize)>, id: u32| {
                let tank = |e: &Exhibit| e.tank.map(|t| (t, e.tiles));
                now.get(&id).copied().or_else(|| old.get(id).and_then(tank))
            };
            let (Some(a), Some(b)) = (value(&now, kept), value(&now, gone)) else {
                continue;
            };
            let gate = self.exhibits.get(kept).is_some_and(|e| e.gate.is_some());
            if gate {
                self.merge_water(kept, a, b);
            }
            let tiles = a.1 + b.1;
            let merged = self.exhibits.get(kept).and_then(|e| e.tank);
            now.extend(merged.map(|t| (kept, (t, tiles))));
        }
    }

    /// The tank exhibit that tile `t` belongs to (its number).
    pub fn tank_at(&self, t: usize) -> Option<u32> {
        let id = self.exhibits.region(t)?;
        self.exhibits.get(id)?.tank.map(|_| id)
    }

    /// The tank of exhibit `id`, if it is one.
    pub fn tank(&self, id: u32) -> Option<Tank> {
        self.exhibits.get(id)?.tank
    }

    /// The floor level of tank `id`: the level of the first tile of its list (tank fauna 2).
    pub fn floor(&self, id: u32) -> Option<i32> {
        let first = *self.exhibits.get(id)?.order.first()?;
        Some(self.zoo.tiles[first].elevation)
    }

    /// The tiles of exhibit `id`, in the original's order (3.1).
    pub fn tiles_of(&self, id: u32) -> Vec<usize> {
        self.exhibits
            .get(id)
            .map_or_else(Vec::new, |e| e.order.clone())
    }

    /// Whether the region `tiles` is a tank, and then whether it is a show tank (exhibits and
    /// money 3.9): every line of its boundary holds a tank wall, on the slot of the tile inside
    /// or else on the slot across; `other` tells whether a tile belongs to another tank. A show
    /// tank has a show wall on at least one line and every line passes: a piece on the inside
    /// slot passes when it is a show wall or the line holds a second piece (not while `strict`,
    /// an undo); a line with a piece across only, when that is a show wall or stands on a tile
    /// of another tank.
    fn tank_test(
        &self,
        tiles: &[usize],
        other: &dyn Fn(usize) -> bool,
        strict: bool,
    ) -> Option<bool> {
        let mut inside = vec![false; self.zoo.tiles.len()];
        tiles.iter().for_each(|&t| inside[t] = true);
        let kind = |p: usize| self.shapes[p].map(|s| s.tank);
        let (mut lines, mut any, mut every) = (0, false, true);
        for &t in tiles {
            for d in [0, 2, 4, 6] {
                let Some(n) = self.neighbour(t, d).filter(|&n| !inside[n]) else {
                    continue;
                };
                let (mine, across) = (self.edge(t, d), self.edge(n, (d + 4) % 8));
                let piece = kind(mine.or(across)?)?;
                if !piece.wall {
                    return None;
                }
                lines += 1;
                any |= piece.show;
                every &= match mine {
                    Some(_) => piece.show || (!strict && across.is_some()),
                    None => piece.show || other(n),
                };
            }
        }
        (lines > 0).then_some(any && every)
    }

    /// Decides again which exhibits are tanks and which of them show tanks. A land exhibit
    /// closed by tank walls only becomes a tank and the other way round (the morph of 3.9 is
    /// the game's business); the tank list of the zoo, which the picture reads, follows. The
    /// tanks that are new or changed their show state are returned.
    pub(super) fn retank(&mut self, strict: bool) -> Vec<u32> {
        let mut by_region: HashMap<u32, Vec<usize>> = HashMap::new();
        for t in 0..self.zoo.tiles.len() {
            if let Some(r) = self
                .exhibits
                .region(t)
                .filter(|&r| r != GROUNDS && r != EXTERIOR)
            {
                by_region.entry(r).or_default().push(t);
            }
        }
        let ids: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        let tanks: Vec<(u32, bool)> = ids
            .iter()
            .filter_map(|&id| Some((id, self.tank_test(by_region.get(&id)?, &|_| false, strict)?)))
            .collect();
        let mut changed = Vec::new();
        for &id in &ids {
            let found = tanks.iter().any(|t| t.0 == id);
            if let Some(e) = self.exhibits.get_mut(id) {
                match (found, e.tank) {
                    (true, None) => {
                        e.tank = Some(Tank::default());
                        changed.push(id);
                    }
                    (false, _) => e.tank = None,
                    _ => {}
                }
            }
        }
        // The show test of a piece across looks at the other tanks, which are known now.
        for &(id, _) in &tanks {
            let Some(mine) = by_region.get(&id) else {
                continue;
            };
            let show = self.tank_test(mine, &|t| self.tank_at(t).is_some(), strict);
            if let Some(tank) = self.exhibits.get_mut(id).and_then(|e| e.tank.as_mut()) {
                if tank.show != (show == Some(true)) && !changed.contains(&id) {
                    changed.push(id);
                }
                tank.show = show == Some(true);
            }
        }
        self.sync_tanks();
        changed
    }

    /// The tank records of the zoo, which the picture reads, from the tank exhibits.
    pub(super) fn sync_tanks(&mut self) {
        let width = self.zoo.width as usize;
        let at = |t: usize| ((t % width) as i32, (t / width) as i32);
        let tanks = self
            .exhibits
            .list()
            .iter()
            .filter_map(|e| {
                let k = e.tank?;
                // The platform of the gate stands on the tank tile beside the gate.
                let gate = self.gate_slot(e.id).map(|(t, d)| {
                    match self.exhibits.region(t) == Some(e.id) {
                        true => at(t),
                        false => self.neighbour(t, d).map_or(k.platform, at),
                    }
                });
                Some(crate::zoo::Tank {
                    tile: at(e.first),
                    gate: gate.unwrap_or(k.platform),
                    height: k.height,
                    level: k.level,
                    salt: k.salt,
                    purity: k.purity,
                })
            })
            .collect();
        if self.zoo.tanks != tanks {
            self.zoo.tanks = tanks;
            self.dirty = true;
        }
    }

    /// The tank state of the exhibits a loaded zoo stores a tank record for (3.10 load): height
    /// and level go through the wall height and level rules, then the tank is filled when it
    /// holds water and drained when it does not (the file keeps no flag, so the level tells), and
    /// goes on moving from there.
    pub(super) fn load_tanks(&mut self) {
        let records = self.zoo.tanks.clone();
        let mut loaded = Vec::new();
        for r in records {
            let Some(id) = self
                .tile_index(r.tile.0, r.tile.1)
                .and_then(|t| self.exhibits.region(t))
            else {
                continue;
            };
            if let Some(e) = self.exhibits.get_mut(id) {
                e.tank = Some(Tank {
                    show: false,
                    height: 0,
                    level: 0,
                    salt: r.salt,
                    purity: r.purity,
                    filled: false,
                    platform: r.gate,
                });
                loaded.push((id, r.height, r.level));
            }
        }
        let told = self.events.len();
        for (id, height, level) in loaded {
            self.set_height(id, height);
            self.edit_tank(id, |t, c| t.level = level.min(c.top(t.height)));
            match level > 0 {
                true => self.fill(id),
                false => self.drain(id),
            }
        }
        self.events.truncate(told);
    }

    /// The pair the gate walk and the fill of a new exhibit that replaces the old exhibit `x`
    /// start from (3.9 morph): the old gate's inside tile and the tile across; without a gate the
    /// old exhibit's tile on the changed line `beside` and the tile across, or after a merge the
    /// first pair of the boundary list of the survivor's new tile list `merged`, stepped by the
    /// gate walk while the tile outside is not in the zoo.
    fn morph_start(
        &self,
        old: &Exhibits,
        x: u32,
        part: &Part,
        beside: &[usize],
        merged: Option<&[usize]>,
    ) -> Option<Pair> {
        let gate = old.get(x)?.gate.and_then(|g| self.slot_of(self.find(g)?));
        let own: HashSet<usize> = part.tiles.iter().copied().collect();
        if let Some((t, d)) = gate {
            let n = self.neighbour(t, d)?;
            return Some(match own.contains(&t) {
                true => (t, n),
                false => (n, t),
            });
        }
        let inside = |t: usize| own.contains(&t);
        let mut pair = match merged {
            Some(order) => {
                let (t, n) = *exhibit::boundary_of(self, order, &inside).first()?;
                (t, n?)
            }
            None => {
                let t = beside.iter().copied().find(|&t| old.region(t) == Some(x))?;
                let n = beside.iter().copied().find(|&n| n != t)?;
                (t, n)
            }
        };
        let mut seen = HashSet::new();
        while !self.in_zoo(pair.1) && seen.insert(pair) {
            match self.step_in(&inside, pair) {
                Some(next) => pair = next,
                None => break,
            }
        }
        Some(pair)
    }

    /// Which of two exhibits that became one stays (exhibits and money 3.3); a tank that joins a
    /// land exhibit goes into it whatever the rest says.
    fn survivor(&self, old: &Exhibits, x: u32, y: u32) -> u32 {
        let (ex, ey) = (old.get(x), old.get(y));
        let tank = |e: Option<&Exhibit>| e.is_some_and(|e| e.tank.is_some());
        if tank(ex) != tank(ey) {
            return if tank(ex) { y } else { x };
        }
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
    pub(super) fn opens_into(&self, old: &Exhibits, p: u32, q: u32) -> bool {
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

    /// Whether the relaxed keeper of the gate choice cannot stand on tile `t` (pathfinding 8):
    /// outside the zoo, on water or in a tank, which the marine specialist of a tank (`diver`)
    /// swims in, or on a land tile whose four quarters objects hold.
    fn dear(&self, t: usize, diver: bool) -> bool {
        path::terrain(self, &Mover::keeper(diver, true), t, None) >= path::MAX
    }

    /// Whether tile `t` has a terrain type that is water.
    pub fn is_water(&self, t: usize) -> bool {
        let terrain = usize::from(self.zoo.tiles[t].terrain);
        self.water.get(terrain).copied().unwrap_or(false)
    }

    /// The side of tile `from` towards tile `to`.
    pub(super) fn side_to(&self, from: usize, to: usize) -> Option<usize> {
        (0..4)
            .map(|d| d * 2)
            .find(|&d| self.neighbour(from, d) == Some(to))
    }

    /// The pieces on the line of a pair: the one on the slot of each tile.
    pub(super) fn line(&self, (i, o): Pair) -> [Option<usize>; 2] {
        [
            self.side_to(i, o).and_then(|d| self.edge(i, d)),
            self.side_to(o, i).and_then(|d| self.edge(o, d)),
        ]
    }

    /// The code of a pair (exhibits and money 3.4).
    fn code(&self, pair: Pair, diver: bool) -> u8 {
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
    pub(super) fn step(&self, id: u32, pair: Pair) -> Option<Pair> {
        self.step_in(&|t| self.exhibits.region(t) == Some(id), pair)
    }

    /// The same step for the exhibit whose tiles `inside` tells.
    fn step_in(&self, inside: &dyn Fn(usize) -> bool, (i, o): Pair) -> Option<Pair> {
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
        self.events.push(Event::Gate(id));
        if let Some(message) = message {
            self.events.push(Event::Blocked { id, message });
        }
    }
}
