//! The portal wall pair of the show neighbours (shows 31.3 points 5 to 8, 31.5): of the walls a
//! show tank shares with a plain tank, the first straight pair of its boundary becomes the
//! portal, flag set on both pieces; the flags of the other pairs of the link are cleared.
//!
//! The original recomputes a show tank's links and portals at each edit (31.4); the world does
//! it for every show tank after each edit. The flags of the old portals are cleared and the new
//! ones set in one pass, so a portal that stays keeps its open byte.

use super::World;
use crate::exhibit::Exhibits;

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
            fence.open = false;
        }
        self.dirty = true;
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

    /// The portals of every show tank, made again (31.3 points 5 and 6, 31.5 point 2): the flags of
    /// its old portals and of the pieces of every qualifying pair go to 0, then the first
    /// straight pair of each link, in the tank's boundary order, becomes its portal.
    pub(super) fn pair_portals(&mut self) {
        let list = self.exhibits.list().iter();
        let shows: Vec<u32> = list
            .filter(|e| e.tank.is_some_and(|t| t.show))
            .map(|e| e.id)
            .collect();
        for s in shows {
            let mut flags: Vec<(u32, bool)> = Vec::new();
            let old = self
                .exhibits
                .get_mut(s)
                .map(|e| std::mem::take(&mut e.portals));
            for p in old.iter().flatten() {
                flags.extend([(p.mine, false), (p.theirs, false)]);
            }
            let mut now: Vec<Portal> = Vec::new();
            for (a, b) in self.exhibits.boundary(self, s) {
                let Some(b) = b else { continue };
                let Some(y) = (self.exhibits.region(b)).filter(|&y| self.show_pair(s, y)) else {
                    continue;
                };
                let line = [
                    self.side_to(a, b).and_then(|d| self.edge(a, d)),
                    self.side_to(b, a).and_then(|d| self.edge(b, d)),
                ];
                flags.extend(
                    line.into_iter()
                        .flatten()
                        .filter_map(|e| Some((self.piece_id(e)?, false))),
                );
                if now.iter().any(|p| p.to == y) {
                    continue;
                }
                if let Some((p, q)) = self.straight(a, b)
                    && let (Some(mine), Some(theirs)) = (self.piece_id(p), self.piece_id(q))
                {
                    flags.extend([(mine, true), (theirs, true)]);
                    now.push(Portal {
                        to: y,
                        mine,
                        theirs,
                    });
                }
            }
            // The last word on a piece wins; a piece that is gone from the map is skipped.
            for (i, &(id, _)) in flags.iter().enumerate() {
                let last = flags.iter().rposition(|f| f.0 == id) == Some(i);
                if let (true, Some(at)) = (last, self.find(id)) {
                    self.set_flag(at, flags[i].1);
                }
            }
            if let Some(e) = self.exhibits.get_mut(s) {
                e.portals = now;
            }
        }
    }

    /// Exhibit `id` of `old` is gone (31.5 point 1): the flags of its portals go to 0 when the
    /// pieces are still on the map.
    pub(super) fn drop_portals(&mut self, old: &Exhibits, id: u32) {
        let portals = old.get(id).map(|e| e.portals.clone()).unwrap_or_default();
        for p in portals {
            for piece in [p.mine, p.theirs] {
                if let Some(at) = self.find(piece) {
                    self.set_flag(at, false);
                }
            }
        }
    }
}
