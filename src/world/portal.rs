//! The portal wall pair of the show neighbours (shows 31.3 points 5 to 8, 31.5): of the walls a
//! show tank shares with a plain tank, the first straight pair of its boundary becomes the
//! portal, flag set on both pieces; the flags of the other pairs of the link are cleared.
//!
//! The original recomputes the links and portals of the show tanks an edit reaches, and only at
//! the edits of shows 34.4; each recompute closes every portal of the show tank (34.5). The
//! world notes those edits with [`World::reach`].

use super::World;
use crate::exhibit::Exhibits;
use std::collections::HashMap;

/// A show tank's portal towards a show neighbour: the neighbour and the ids of the two wall
/// pieces, the show tank's and the neighbour's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Portal {
    pub to: u32,
    pub mine: u32,
    pub theirs: u32,
}

impl World {
    /// The portal flag setter (31.3 point 7): the flag of the tank wall at position `at` takes `v`.
    /// Nothing when it has it already or the piece is no tank wall; else the map is marked
    /// changed and, unless a saved zoo is being read, the open byte goes back to 0.
    pub(crate) fn set_flag(&mut self, at: usize, v: bool) {
        if !self.shapes[at].is_some_and(|s| s.tank.wall) {
            return;
        }
        let fence = self.zoo.entities[at]
            .common
            .as_mut()
            .and_then(|c| c.fence.as_mut());
        let Some(fence) = fence.filter(|f| f.portal != v) else {
            return;
        };
        fence.portal = v;
        if !self.reading {
            self.close_portal(at);
        }
        self.dirty = true;
    }

    /// What the flag setter does in play: the open byte goes back to 0 and the run stops (shows
    /// 32.6 point 7).
    fn close_portal(&mut self, at: usize) {
        let Some(c) = self.zoo.entities[at].common.as_mut() else {
            return;
        };
        c.fence.iter_mut().for_each(|f| f.open = false);
        let id = c.id;
        self.portal_reset(id);
    }

    /// The id of the entity at position `at`.
    fn piece_id(&self, at: usize) -> Option<u32> {
        self.zoo.entities[at].common.as_ref().map(|c| c.id)
    }

    /// The pair `(a, b)` of a show tank's boundary is straight (31.3 point 6): `a` and `b` each
    /// hold one edge piece in all, on the line they share, both fences and neither a gate. Gives
    /// the two pieces.
    fn straight(&self, a: usize, b: usize) -> Option<(usize, usize)> {
        let slots = |t: usize| (0..4).filter_map(|d| self.edge(t, 2 * d)).count();
        let (p, q) = (
            self.edge(a, self.side_to(a, b)?)?,
            self.edge(b, self.side_to(b, a)?)?,
        );
        let ok = |e: usize| self.shapes[e].is_some_and(|s| s.fence) && !self.is_door(e);
        (slots(a) == 1 && slots(b) == 1 && ok(p) && ok(q)).then_some((p, q))
    }

    /// Notes that exhibit `id` is updated by the edit in progress (shows 34.4): the next
    /// [`World::update_shows`] recomputes the show tanks it reaches.
    pub(super) fn reach(&mut self, id: u32) {
        self.reach.push(id);
    }

    /// Every show tank is updated by the next recompute (a load).
    pub(super) fn reach_all(&mut self) {
        self.reach_all = true;
    }

    /// The show-link recompute of the exhibits noted by [`World::reach`] (shows 31.4, 34.5): each
    /// show tank they reach has its links dropped and made again, in the order of its boundary,
    /// and the flags of its portals go to 0 and to 1 again. A show tank not reached keeps all.
    /// `before` is the amphibious sets from before the rebuild when the edit updates first (shows
    /// 34.2 point 5).
    pub(super) fn update_shows(&mut self, before: Option<&HashMap<u32, Vec<u32>>>) {
        let ids: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        // An exhibit that is gone is forgotten by its neighbours (31.5 point 1).
        for &x in &ids {
            if let Some(e) = self.exhibits.get_mut(x) {
                e.show_near.retain(|n| ids.contains(n));
                e.portals.retain(|p| ids.contains(&p.to));
            }
        }
        let reach = std::mem::take(&mut self.reach);
        let from = if std::mem::take(&mut self.reach_all) {
            ids
        } else {
            reach
        };
        let is_show = |w: &World, x: u32| {
            w.exhibits
                .get(x)
                .is_some_and(|e| e.tank.is_some_and(|t| t.show))
        };
        let mut shows: Vec<u32> = Vec::new();
        for x in from {
            let across = match is_show(self, x) {
                true => vec![x],
                false if self.exhibits.get(x).is_none() => continue,
                false => (self.exhibits.boundary(self, x).into_iter())
                    .filter_map(|(_, b)| self.exhibits.region(b?))
                    .filter(|&y| is_show(self, y))
                    .collect(),
            };
            for y in across {
                if !shows.contains(&y) {
                    shows.push(y);
                }
            }
        }
        for s in shows {
            self.update_show(s, before);
        }
    }

    /// `update` of show tank `s` (31.4 point 2, 34.5): all its links dropped, then `check` for
    /// each pair of its boundary (31.3 points 4 to 6).
    fn update_show(&mut self, s: u32, before: Option<&HashMap<u32, Vec<u32>>>) {
        let Some(e) = self.exhibits.get_mut(s) else {
            return;
        };
        let (old, portals) = (
            std::mem::take(&mut e.show_near),
            std::mem::take(&mut e.portals),
        );
        // The set a plain tank's joins read (34.2 point 5).
        let near = |w: &World, n: u32| match before.and_then(|b| b.get(&n)) {
            Some(old) => old.clone(),
            None => w
                .exhibits
                .get(n)
                .map_or_else(Vec::new, |e| e.amphibious.clone()),
        };
        for n in old {
            if let Some(m) = self.exhibits.get_mut(n) {
                m.show_near.retain(|&y| y != s);
            }
            let near = near(self, n);
            self.show_moves.push(crate::show::Move::Link {
                plain: n,
                show: s,
                on: false,
                near,
            });
        }
        for piece in portals.iter().flat_map(|p| [p.mine, p.theirs]) {
            if let Some(at) = self.find(piece) {
                self.set_flag(at, false);
            }
        }
        let mut now: Vec<Portal> = Vec::new();
        for (a, b) in self.exhibits.boundary(self, s) {
            let Some(b) = b else { continue };
            let Some(y) = (self.exhibits.region(b)).filter(|&y| self.show_pair(s, y)) else {
                continue;
            };
            if let Some(e) = self
                .exhibits
                .get_mut(s)
                .filter(|e| !e.show_near.contains(&y))
            {
                e.show_near.push(y);
                if let Some(m) = self.exhibits.get_mut(y) {
                    m.show_near.push(s);
                }
                let near = near(self, y);
                self.show_moves.push(crate::show::Move::Link {
                    plain: y,
                    show: s,
                    on: true,
                    near,
                });
            }
            let line = [
                self.side_to(a, b).and_then(|d| self.edge(a, d)),
                self.side_to(b, a).and_then(|d| self.edge(b, d)),
            ];
            for e in line.into_iter().flatten() {
                self.set_flag(e, false);
            }
            if now.iter().any(|p| p.to == y) {
                continue;
            }
            if let Some((p, q)) = self.straight(a, b)
                && let (Some(mine), Some(theirs)) = (self.piece_id(p), self.piece_id(q))
            {
                self.set_flag(p, true);
                self.set_flag(q, true);
                now.push(Portal {
                    to: y,
                    mine,
                    theirs,
                });
            }
        }
        if let Some(e) = self.exhibits.get_mut(s) {
            e.portals = now;
        }
    }

    /// Exhibit `id` of `old` is gone (31.5 point 1): the flags of its portals, and of the portals
    /// the other exhibits hold towards it, go to 0 when the pieces are still on the map.
    pub(super) fn drop_portals(&mut self, old: &Exhibits, id: u32) {
        let portals = old.list().iter().flat_map(|e| {
            let mine = e.id == id;
            e.portals
                .iter()
                .filter(move |p| mine || p.to == id)
                .copied()
        });
        for p in portals.collect::<Vec<_>>() {
            for piece in [p.mine, p.theirs] {
                if let Some(at) = self.find(piece) {
                    self.set_flag(at, false);
                }
            }
        }
    }
}
