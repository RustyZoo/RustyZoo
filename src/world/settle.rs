//! The new list of exhibits after the edit of an exhibit fence piece (exhibits and money 3.1 to
//! 3.9): the parts of the fences, the merges and splits, the kinds, the morph, then the tanks.

use super::exhibit::{Event, Pair};
use super::{Draw, World};
use crate::exhibit::{self, EXTERIOR, Exhibits, GROUNDS, Part, Tank};
use std::collections::{HashMap, HashSet};

/// What `settle` decides about the parts of the new fences before it builds the list.
struct Plan {
    parts: Vec<Part>,
    /// The old exhibits each part holds tiles of.
    olds: Vec<Vec<u32>>,
    /// The old exhibit each part keeps.
    winner: Vec<Option<u32>>,
    /// The (gone, kept) pairs of the merges.
    lost: Vec<(u32, u32)>,
    /// The exhibit a part left by a split comes from.
    split_from: HashMap<usize, u32>,
    /// The tanks that kept a part of a split.
    split_kept: Vec<u32>,
    /// The kind of each part: none for land, else whether a show tank.
    kinds: Vec<Option<bool>>,
    /// The parts of a cut land exhibit named after it, with no number.
    silent: Vec<usize>,
    /// The exhibits whose kind no longer fits their walls, by part.
    morphed: HashMap<usize, u32>,
}

/// What `settle` builds: the new list and what is left to do for the exhibits in it.
#[derive(Default)]
struct Built {
    next: Exhibits,
    fresh: Vec<u32>,
    tanks: Vec<(u32, Option<Tank>)>,
    starts: Vec<(u32, Pair)>,
    regate: Vec<u32>,
    kept_ids: HashSet<u32>,
    /// The exhibits the original resizes: the ones made and the ones whose region changed.
    resized: Vec<u32>,
    /// The resizes a split makes before those: when the survivor keeps the side across the piece,
    /// each exhibit is first resized over the side it does not keep (habitat rating 10.1).
    early: Vec<(u32, Vec<usize>)>,
}

/// What a split gives: where each part comes from, the land exhibits cut, the tanks that kept a part.
type Splits = (HashMap<usize, u32>, Vec<Vec<usize>>, Vec<u32>);

/// The old exhibits each part holds tiles of, A's side (`ra`) first, then B's (`rb`).
fn held_by(old: &Exhibits, parts: &[Part], ra: Option<u32>, rb: Option<u32>) -> Vec<Vec<u32>> {
    parts
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
        .collect()
}

/// The morph: an exhibit whose kind no longer fits its walls is replaced by a new one of the other
/// kind on the same tiles (3.9). Gives the exhibit each such part had.
fn morph(
    old: &Exhibits,
    winner: &mut [Option<u32>],
    kinds: &[Option<bool>],
) -> HashMap<usize, u32> {
    let mut morphed: HashMap<usize, u32> = HashMap::new();
    for (i, w) in winner.iter_mut().enumerate() {
        let tank = |id: u32| old.get(id).is_some_and(|e| e.tank.is_some());
        if let Some(id) = w.filter(|&id| tank(id) != kinds[i].is_some()) {
            morphed.insert(i, id);
            *w = None;
        }
    }
    morphed
}

impl World {
    /// The exhibits after the edit of an exhibit fence piece on side `d` of tile `a` (`hint`; none
    /// for an undo, which also makes `quiet`: no gate is chosen and the counter stays).
    pub(super) fn settle(&mut self, hint: Option<(usize, usize)>, quiet: bool) {
        let old = std::mem::take(&mut self.exhibits);
        let plan = self.plan(&old, hint, quiet);
        let mut built = self.rebuild(&old, &plan, hint, quiet);
        self.leave(&old, &built.kept_ids, &plan.lost);
        built.next.exterior(self);
        self.refill(&mut built);
        let gone = self.gone_tanks(&old, &built.kept_ids, quiet);
        self.commit(&old, &plan, &mut built, &gone, quiet);
        if let Some((a, d)) = hint.filter(|_| !quiet) {
            self.announce(&built, a, d);
        }
        // A new tank with no gate to choose ends its split at once; one that is gone ends nothing.
        self.tails.retain(|t| self.exhibits.get(t.0).is_some());
        let waiting: Vec<u32> = self.waiting.iter().map(|w| w.0).collect();
        let (wait, now): (Vec<_>, Vec<_>) =
            (std::mem::take(&mut self.tails).into_iter()).partition(|t| waiting.contains(&t.0));
        self.tails = wait;
        for (_, tanks, again) in now {
            self.finish_split(&tanks, again);
        }
    }

    /// What the parts of the new fences are made of: for each, the old exhibits it holds tiles of
    /// and the exhibit it keeps (merges, splits), its kind and its morph.
    fn plan(&mut self, old: &Exhibits, hint: Option<(usize, usize)>, quiet: bool) -> Plan {
        let near = hint.and_then(|(a, d)| exhibit::parts_near(self, old, a, d));
        #[cfg(debug_assertions)]
        if let Some(near) = &near {
            exhibit::assert_same_parts(self, near);
        }
        let parts = near.unwrap_or_else(|| {
            let all = exhibit::parts(self);
            all.into_iter().filter(|p| !p.grounds).collect()
        });
        let ra = hint.and_then(|(a, _)| old.region(a));
        let rb = hint
            .and_then(|(a, d)| self.neighbour(a, d))
            .and_then(|b| old.region(b));
        let olds = held_by(old, &parts, ra, rb);
        let (mut winner, lost) = self.merges(old, &olds);
        let (split_from, split_land, split_kept) = self.splits(old, &parts, &mut winner, hint, ra);
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
        let morphed = morph(old, &mut winner, &kinds);
        Plan {
            parts,
            olds,
            winner,
            lost,
            split_from,
            split_kept,
            kinds,
            silent,
            morphed,
        }
    }

    /// Merges: each part with several old exhibits keeps the survivor. Gives the exhibit each
    /// part keeps and the pairs (gone, kept).
    fn merges(&self, old: &Exhibits, olds: &[Vec<u32>]) -> (Vec<Option<u32>>, Vec<(u32, u32)>) {
        let mut winner: Vec<Option<u32>> = Vec::new();
        let mut lost: Vec<(u32, u32)> = Vec::new();
        for list in olds {
            let mut it = list.iter().copied();
            let mut w = it.next();
            for y in it {
                let x = w.unwrap_or(y);
                let kept = self.survivor(old, x, y);
                lost.push((if kept == x { y } else { x }, kept));
                w = Some(kept);
            }
            winner.push(w);
        }
        (winner, lost)
    }

    /// Splits: an old exhibit held by several parts stays in one of them. The parts that leave
    /// it remember where they come from, and a tank that kept a part has its walls to bring to
    /// its side again. Gives where each part comes from, the land exhibits that were cut and the
    /// tanks that kept a part.
    fn splits(
        &mut self,
        old: &Exhibits,
        parts: &[Part],
        winner: &mut [Option<u32>],
        hint: Option<(usize, usize)>,
        ra: Option<u32>,
    ) -> Splits {
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
        (split_from, split_land, split_kept)
    }

    /// The new list: the exhibits kept (their region found again when it changed) and the new
    /// ones, with their name and the events they post.
    fn rebuild(
        &mut self,
        old: &Exhibits,
        plan: &Plan,
        hint: Option<(usize, usize)>,
        quiet: bool,
    ) -> Built {
        let Plan {
            parts,
            olds,
            winner,
            split_from,
            kinds,
            silent,
            morphed,
            ..
        } = plan;
        let mut built = Built {
            next: old.blank(self.zoo.tiles.len()),
            ..Built::default()
        };
        // The tiles beside the piece: the fill of a region that changed starts at the one it
        // holds (3.1).
        let beside: Vec<usize> = hint
            .into_iter()
            .flat_map(|(a, d)| [Some(a), self.neighbour(a, d)])
            .flatten()
            .collect();
        for (i, ((part, w), kind)) in parts.iter().zip(winner).zip(kinds).enumerate() {
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
                    built.kept_ids.insert(e.id);
                    let same = e.order.len() == part.tiles.len()
                        && part.tiles.iter().all(|&t| old.region(t) == Some(e.id));
                    if !same {
                        built.resized.push(e.id);
                    }
                    let part = match same {
                        true => &Part {
                            tiles: e.order.clone(),
                            grounds: false,
                        },
                        false => &filled(None, Some(e.id)),
                    };
                    built.next.keep(e.clone(), part);
                    if self.missing(e) {
                        built.regate.push(e.id);
                    }
                }
                None => {
                    let merged = (olds[i].len() > 1)
                        .then(|| morphed.get(&i).map(|&x| filled(None, Some(x)).tiles))
                        .flatten();
                    let start = morphed
                        .get(&i)
                        .and_then(|&x| self.morph_start(old, x, part, &beside, merged.as_deref()));
                    let part = &filled(start.map(|s| s.0), None);
                    // The land part of such a cut is the exhibit's own: its name, no number.
                    let silent = kind.is_none() && silent.contains(&i);
                    let number = built.next.counter;
                    built.next.counter += u32::from(!quiet && !silent);
                    let word = &self.words[kind.map_or(0, |show| 1 + usize::from(show))];
                    let name = match split_from.get(&i).and_then(|&x| old.get(x)) {
                        Some(x) if silent => x.name.clone(),
                        _ => format!("{word} {number}"),
                    };
                    let id = built.next.add(part, name);
                    built.fresh.push(id);
                    built.resized.push(id);
                    match kind {
                        // A tank has its ground, walls and objects to deal with before its name.
                        Some(_) => {
                            let from = split_from.get(&i).and_then(|&x| old.get(x));
                            built.tanks.push((id, from.and_then(|e| e.tank)));
                        }
                        None => self.events.push(Event::New {
                            id,
                            ask: !quiet && !silent,
                        }),
                    }
                    if let Some(start) = start {
                        built.starts.push((id, start));
                    }
                }
            }
        }
        built.next.list_mut().sort_by_key(|e| e.id);
        self.split_twice(plan, hint, &mut built);
        built
    }

    /// A split whose survivor keeps the side across the piece resizes both exhibits twice, the
    /// survivor first each time (habitat rating 10.1): the survivor over the piece's side, the
    /// new exhibit over the side across, then both over the side they keep.
    fn split_twice(&self, plan: &Plan, hint: Option<(usize, usize)>, built: &mut Built) {
        let Some((a, b)) = hint.and_then(|(a, d)| Some((a, self.neighbour(a, d)?))) else {
            return;
        };
        let holds = |i: usize, t: usize| plan.parts[i].tiles.contains(&t);
        for i in 0..plan.parts.len() {
            let Some(&x) = plan.split_from.get(&i) else {
                continue;
            };
            let new = plan.parts[i]
                .tiles
                .first()
                .and_then(|&t| built.next.region(t));
            let Some(new) = new.filter(|n| built.fresh.contains(n)) else {
                continue;
            };
            // The survivor may have been morphed: its part then has no winner.
            let kept = |j: usize| plan.winner[j] == Some(x) || plan.morphed.get(&j) == Some(&x);
            let Some(j) = (0..plan.parts.len()).find(|&j| kept(j)) else {
                continue;
            };
            if !holds(i, a) || !holds(j, b) || holds(j, a) {
                continue;
            }
            // A survivor that becomes a tank goes with its resizes (10.1.1).
            if plan.morphed.get(&j) != Some(&x) {
                built.early.push((x, exhibit::fill(self, a)));
            }
            // A new part that becomes a tank is made by the morph, resized by its constructor only.
            if plan.kinds[i].is_none() {
                built.early.push((new, exhibit::fill(self, b)));
            }
            let at = |built: &Built, id: u32| built.resized.iter().position(|&r| r == id);
            if let (Some(p), Some(q)) = (at(built, x), at(built, new))
                && q < p
            {
                built.resized.swap(p, q);
            }
        }
    }

    /// The exhibits that are gone: their gate is a fence again, and the events say where they went.
    fn leave(&mut self, old: &Exhibits, kept_ids: &HashSet<u32>, lost: &[(u32, u32)]) {
        let into: HashMap<u32, u32> = lost.iter().copied().collect();
        for e in old.list().iter().filter(|e| !kept_ids.contains(&e.id)) {
            self.units.thoughts.drop_exhibit(e.id);
            self.unmake_gone(old, e.id);
            self.drop_portals(old, e.id);
            if let Some(at) = e.gate.and_then(|g| self.find(g)) {
                self.make_fence(at);
            }
            self.events.push(Event::Left(e.order.clone()));
            self.events.push(match into.get(&e.id) {
                Some(&kept) => Event::Merged { kept, gone: e.id },
                None => Event::Removed(e.id),
            });
        }
    }

    /// A morph refills the region across its start pair from that tile (3.1), and resizes its
    /// exhibit (habitat rating 10.1): once more when the split or merge resized it already.
    fn refill(&self, built: &mut Built) {
        for &(id, (_, out)) in &built.starts {
            if let Some(r) = built
                .next
                .region(out)
                .filter(|&r| r != GROUNDS && r != EXTERIOR && r != id)
            {
                built.next.reorder(r, exhibit::fill(self, out));
                built.resized.push(r);
            }
        }
    }

    /// The tiles of the tanks that are gone, which keep their ground as sand (3.9).
    fn gone_tanks(&self, old: &Exhibits, kept_ids: &HashSet<u32>, quiet: bool) -> Vec<usize> {
        (old.list().iter())
            .filter(|e| e.tank.is_some() && !kept_ids.contains(&e.id) && !quiet)
            .flat_map(|e| (0..self.zoo.tiles.len()).filter(|&t| old.region(t) == Some(e.id)))
            .collect()
    }

    /// Puts the new list in place, then the tanks (ground, walls, water) and the gates that went.
    fn commit(
        &mut self,
        old: &Exhibits,
        plan: &Plan,
        built: &mut Built,
        gone: &[usize],
        quiet: bool,
    ) {
        let Plan {
            lost, split_kept, ..
        } = plan;
        self.exhibits = std::mem::take(&mut built.next);
        self.reshaped = true;
        self.resize_areas(&built.early, &built.resized);
        self.edits += 1;
        let mut changed = self.retank(quiet);
        if !quiet {
            self.paint(gone, false);
            changed.extend(lost.iter().map(|l| l.1).chain(split_kept.iter().copied()));
            self.floors(&changed);
        }
        if !quiet {
            let made = built.tanks.iter().filter(|t| t.1.is_none());
            self.spend_temporary_ids(made.map(|t| t.0));
        }
        self.decide(&changed, false);
        // A split updates the wall information of the tank it cuts first (map drawing 9.7.8
        // point 3.2.4 and 3.4).
        let cuts: Vec<u32> = built
            .tanks
            .iter()
            .filter_map(|t| t.1.map(|_| t.0))
            .collect();
        // The tank cut is first resized to the side of the piece, whatever the side of its gate:
        // when it keeps the other side, the two tanks stand on each other's tiles until the new
        // tank is made, and the set-ups they queue keep the walls they held then.
        let across = match (&split_kept[..], &cuts[..]) {
            (&[x], &[y]) if !quiet && built.early.iter().any(|e| e.0 == x) => Some((x, y)),
            _ => None,
        };
        if let Some((x, y)) = across {
            self.exhibits.swap_tiles(x, y);
            self.swapped = true;
        }
        if !quiet {
            // The tank cut: its walls are brought to its side, then it updates.
            for &x in split_kept {
                self.wall_step(x);
            }
            for &x in split_kept {
                self.push_set_up(Draw::Walls(x));
            }
        }
        for (id, inherited) in std::mem::take(&mut built.tanks) {
            self.make_tank(id, inherited, !quiet);
        }
        if let Some((x, y)) = across {
            self.exhibits.swap_tiles(x, y);
            self.swapped = false;
        }
        if !quiet && !split_kept.is_empty() {
            let mut changed = self.retank(false);
            changed.extend(split_kept.iter().copied());
            self.floors(&changed);
            self.decide(&changed, true);
            // The last updates, X then the new tank Y, come after the gate of Y; when the sides
            // were swapped, the walls are brought inwards again first.
            for &y in &cuts {
                let tanks = split_kept.iter().copied().chain([y]).collect();
                self.tails.push((y, tanks, across.is_some()));
            }
        }
        for &id in &built.regate {
            if let Some(e) = self.exhibits.get_mut(id) {
                e.gate = None;
            }
        }
        if !quiet {
            self.merge_tanks(old, lost);
            // A tank left by a merge was decided again (shows 31.2 point 11).
            if !lost.is_empty() {
                self.note_decision();
            }
        }
        self.relink();
    }

    /// The end of a split, after the gate of the new tank: the tanks `tanks` (the tank cut, then the
    /// new one) bring their walls inwards again when their sides were swapped, then update.
    pub(super) fn finish_split(&mut self, tanks: &[u32], again: bool) {
        for &t in tanks.iter().filter(|_| again) {
            self.wall_step(t);
        }
        self.draws.extend(tanks.iter().map(|&t| Draw::Walls(t)));
    }

    /// Tells the game where the new exhibits start, and gives a new gate to an exhibit whose gate
    /// went but whose line is still closed, after the piece on side `d` of tile `a`.
    fn announce(&mut self, built: &Built, a: usize, d: usize) {
        let Some(b) = self.neighbour(a, d) else {
            return;
        };
        for &id in &built.fresh {
            let start = built.starts.iter().find(|s| s.0 == id).map_or_else(
                || match self.exhibits.region(b) == Some(id) {
                    true => (b, a),
                    false => (a, b),
                },
                |s| s.1,
            );
            self.waiting.push((id, start, a));
        }
        if self.closed(a, d) {
            for &id in &built.regate {
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
}
