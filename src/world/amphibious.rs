//! The amphibious neighbours (tank fauna 12): a tank and a land exhibit are neighbours when a line
//! of their common boundary holds one tank wall and no second exhibit fence, neither is a show
//! tank, and the two corners of the land tile on that line are level with the top of the tank's
//! walls. The wall is then a connector, which an amphibious species crosses.
//!
//! The links are made again after every edit that can move one (12.2 lists the triggers; one that
//! is made again after each edit gets the same links, O3).

use super::World;
use crate::exhibit::{EXTERIOR, GROUNDS};
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

    /// 12.2: the links of every exhibit and the connectors, made again; the neighbours of an
    /// exhibit keep the order they became so in.
    pub(super) fn relink(&mut self) {
        let ids: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        let mut connectors = Vec::new();
        let mut found = Vec::new();
        for &x in &ids {
            let mut mine: Vec<u32> = Vec::new();
            for (a, b) in self.exhibits.boundary(self, x) {
                let Some((y, wall)) = b.and_then(|b| self.link(x, a, b)) else {
                    continue;
                };
                connectors.extend(self.zoo.entities[wall].common.as_ref().map(|c| c.id));
                if !mine.contains(&y) {
                    mine.push(y);
                }
            }
            found.push(mine);
        }
        connectors.sort_unstable();
        connectors.dedup();
        self.connectors = connectors;
        for (&x, mine) in ids.iter().zip(&found) {
            if let Some(e) = self.exhibits.get_mut(x) {
                let mut next: Vec<u32> = (e.amphibious.iter().copied())
                    .filter(|n| mine.contains(n))
                    .collect();
                for n in mine {
                    if !next.contains(n) {
                        next.push(*n);
                    }
                }
                e.amphibious = next;
            }
        }
    }
}

#[cfg(test)]
mod tests;
