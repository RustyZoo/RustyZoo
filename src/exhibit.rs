//! The exhibits of a zoo: the regions its exhibit fences make (exhibits and money 3.1).
//!
//! Tiles joined through sides that hold no exhibit fence form one region. The region that holds
//! the zoo entrance is the zoo grounds, the tiles beyond the zoo wall are the exterior, and
//! every other region is an exhibit, from one tile up. The map edge closes a region like a fence.

use crate::world::World;

/// The region of the zoo grounds.
pub const GROUNDS: u32 = 0;
/// The region of the tiles beyond the zoo wall.
pub const EXTERIOR: u32 = u32::MAX;

/// An exhibit: the first tile of its region (row by row) and its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhibit {
    pub first: usize,
    pub tiles: usize,
}

/// The region of every tile and the exhibits.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Exhibits {
    region: Vec<u32>,
    list: Vec<Exhibit>,
}

impl Exhibits {
    /// Finds the regions of a world. The exhibits are numbered from 1 in the order of their first
    /// tile. An entrance with no building on its tile has no exterior: the world marks none.
    pub fn new(world: &World) -> Exhibits {
        let tiles = world.zoo.tiles.len();
        let none = u32::MAX - 1;
        let mut region: Vec<u32> = (0..tiles)
            .map(|t| if world.in_zoo(t) { none } else { EXTERIOR })
            .collect();
        let mut list = Vec::new();
        let (ex, ey) = world.zoo.entrance;
        let entrance = i32::try_from(ex)
            .ok()
            .zip(i32::try_from(ey).ok())
            .and_then(|(x, y)| world.tile_index(x, y));
        for start in entrance.into_iter().chain(0..tiles) {
            if region[start] != none {
                continue;
            }
            let id = if Some(start) == entrance {
                GROUNDS
            } else {
                list.len() as u32 + 1
            };
            let mut size = 0;
            region[start] = id;
            let mut todo = vec![start];
            while let Some(t) = todo.pop() {
                size += 1;
                for d in [0, 2, 4, 6] {
                    if let Some(n) = world.neighbour(t, d)
                        && region[n] == none
                        && !world.closed(t, d)
                    {
                        region[n] = id;
                        todo.push(n);
                    }
                }
            }
            if id != GROUNDS {
                list.push(Exhibit {
                    first: start,
                    tiles: size,
                });
            }
        }
        Exhibits { region, list }
    }

    /// How many of the tiles `(x, y)` lie in an exhibit that no earlier one lies in: a tile off the
    /// map, in the zoo grounds or in the exterior does not count, nor does a second one in the same
    /// exhibit. Equal to the number of exhibits and of tiles when the tiles are the first tiles
    /// the file stores for its habitats.
    pub fn distinct(&self, world: &World, tiles: &[(i32, i32)]) -> usize {
        let mut seen = std::collections::HashSet::new();
        for &(x, y) in tiles {
            if let Some(id) = world
                .tile_index(x, y)
                .and_then(|t| self.region(t))
                .filter(|&id| id != GROUNDS && id != EXTERIOR)
            {
                seen.insert(id);
            }
        }
        seen.len()
    }

    /// The region of tile `t`: [`GROUNDS`], [`EXTERIOR`] or the number of an exhibit (from 1).
    pub fn region(&self, t: usize) -> Option<u32> {
        self.region.get(t).copied()
    }

    /// The exhibits, the one numbered `n` at position `n - 1`.
    pub fn list(&self) -> &[Exhibit] {
        &self.list
    }
}

#[cfg(test)]
mod tests;
