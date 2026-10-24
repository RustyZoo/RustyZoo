//! The amphibious neighbours (tank fauna 12): a tank and a land exhibit are neighbours when a line
//! of their common boundary holds one tank wall and no second exhibit fence, neither is a show
//! tank, and the two corners of the land tile on that line are level with the top of the tank's
//! walls. The wall is then a connector, which an amphibious species crosses.
//!
//! The links are made again after every edit that can move one (12.2 lists the triggers; one that
//! is made again after each edit gets the same links, O3).
//!
//! The show neighbours (exhibits and money 3.11) are made here too, by the same rebuild: a show tank
//! and a plain tank that touch, both filled and at least 4 high, with their wall tops at one level.
//! The portal wall pair is not made; a saved portal flag is read and drawn as it is.

use super::World;
use crate::exhibit::{EXTERIOR, GROUNDS, Tank};
use crate::render::corner_elevation;

impl World {
    /// Whether the fence piece at position `at` of the entity list is a connector (12.1).
    pub fn connector(&self, at: usize) -> bool {
        let common = self.zoo.entities.get(at).and_then(|e| e.common.as_ref());
        common.is_some_and(|c| self.connectors.binary_search(&c.id).is_ok())
    }

    /// 12.1: the exhibit across the pair `(a, b)` of the boundary of exhibit `x` and the wall
    /// piece that links them, when they are amphibious neighbours.
    fn link(&self, x: u32, a: usize, b: usize) -> Option<(u32, usize)> {
        let y = self
            .exhibits
            .region(b)
            .filter(|&y| y != GROUNDS && y != EXTERIOR)?;
        let (kx, ky) = (self.exhibits.get(x)?.tank, self.exhibits.get(y)?.tank);
        // One tank and one land exhibit, neither a show tank.
        let tank = match (kx, ky) {
            (Some(k), None) => (x, a, b, k),
            (None, Some(k)) => (y, b, a, k),
            _ => return None,
        };
        let (id, t, land, k) = tank;
        if k.show {
            return None;
        }
        // The pieces on the line: the wall is the tank wall with a height, the first of the two;
        // a second exhibit fence on the line shuts it.
        let (da, db) = (self.side_to(a, b)?, self.side_to(b, a)?);
        let pieces = [self.edge(a, da), self.edge(b, db)];
        let fence = |e: usize| self.shapes[e].is_some_and(|s| s.fence);
        let wall = pieces
            .into_iter()
            .flatten()
            .find(|&e| self.shapes[e].is_some_and(|s| s.tank.wall) && self.wall_height(e) != 0)?;
        if let [Some(p), Some(q)] = pieces
            && fence(p)
            && fence(q)
        {
            return None;
        }
        // The land tile's two corners on the line are level with the top of the walls.
        let top = self.floor(id)? + k.steps().0;
        let to = self.side_to(land, t)?;
        let level = [(to + 1) % 8, (to + 7) % 8]
            .into_iter()
            .all(|c| corner_elevation(&self.zoo.tiles[land], c) == top);
        level.then_some((y, wall))
    }

    /// 3.11: whether tanks `x` and `y`, which touch, are show neighbours.
    fn show_pair(&self, x: u32, y: u32) -> bool {
        let tank = |id: u32| self.exhibits.get(id).and_then(|e| e.tank);
        let top = |id: u32, k: &Tank| Some(i64::from(self.floor(id)?) + i64::from(k.height));
        let (Some(a), Some(b)) = (tank(x), tank(y)) else {
            return false;
        };
        a.show != b.show
            && [a, b].iter().all(|k| k.filled && k.height >= 4)
            && top(x, &a).is_some_and(|t| Some(t) == top(y, &b))
    }

    /// 12.2: the links of every exhibit and the connectors, made again; the neighbours of an
    /// exhibit keep the order they became so in.
    pub(super) fn relink(&mut self) {
        let ids: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        let mut connectors = Vec::new();
        let mut found = Vec::new();
        for &x in &ids {
            let (mut mine, mut shows): (Vec<u32>, Vec<u32>) = (Vec::new(), Vec::new());
            for (a, b) in self.exhibits.boundary(self, x) {
                let Some(b) = b else { continue };
                if let Some(y) = (self.exhibits.region(b)).filter(|&y| self.show_pair(x, y))
                    && !shows.contains(&y)
                {
                    shows.push(y);
                }
                let Some((y, wall)) = self.link(x, a, b) else {
                    continue;
                };
                connectors.extend(self.zoo.entities[wall].common.as_ref().map(|c| c.id));
                if !mine.contains(&y) {
                    mine.push(y);
                }
            }
            found.push((mine, shows));
        }
        connectors.sort_unstable();
        connectors.dedup();
        self.connectors = connectors;
        // A set keeps the order its members became so in.
        let keep = |old: &[u32], now: &[u32]| {
            let mut next: Vec<u32> = old.iter().copied().filter(|n| now.contains(n)).collect();
            let added: Vec<u32> = now.iter().filter(|n| !next.contains(n)).copied().collect();
            next.extend(added);
            next
        };
        for (&x, (mine, shows)) in ids.iter().zip(&found) {
            if let Some(e) = self.exhibits.get_mut(x) {
                e.amphibious = keep(&e.amphibious, mine);
                e.show_near = keep(&e.show_near, shows);
            }
        }
    }
}

#[cfg(test)]
mod tests;
