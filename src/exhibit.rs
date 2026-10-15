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

/// An exhibit: its number (kept while it exists), name, gate and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exhibit {
    pub id: u32,
    /// The first tile of its region (row by row).
    pub first: usize,
    pub tiles: usize,
    /// The name; empty for an exhibit found in a map, whose stored name is not read yet.
    pub name: String,
    /// The id of the fence piece that is its gate.
    pub gate: Option<u32>,
    /// What a tank has besides: set when every wall of its boundary is a tank wall.
    pub tank: Option<Tank>,
}

/// The state of a tank exhibit (exhibits and money 3.9, 4.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tank {
    /// A show tank: its walls are show walls (the test is `World::retank`'s).
    pub show: bool,
    /// The wall height and the water level, in elevation steps above the tank floor.
    pub height: u32,
    pub level: u32,
    /// Salt water; the purity of the water.
    pub salt: bool,
    pub purity: u32,
    /// The tile the gate's platform is drawn on, (-1, -1) for none.
    pub platform: (i32, i32),
}

impl Default for Tank {
    /// A tank with no walls up, as a new tank is before its creation sets the water (3.9).
    fn default() -> Tank {
        Tank {
            show: false,
            height: 0,
            level: 0,
            salt: true,
            purity: 100,
            platform: (-1, -1),
        }
    }
}

/// A set of tiles joined without an exhibit fence.
pub struct Part {
    pub tiles: Vec<usize>,
    /// The region holds the zoo entrance or a tile beyond the zoo wall.
    pub grounds: bool,
}

/// The region of every tile and the exhibits.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Exhibits {
    region: Vec<u32>,
    list: Vec<Exhibit>,
    /// The number the next new exhibit gets.
    pub next: u32,
    /// The number of the next default name (exhibits and money 3.6).
    pub counter: u32,
}

/// The regions of a world, the one of the entrance first and then the others by their first tile.
pub fn parts(world: &World) -> Vec<Part> {
    let tiles = world.zoo.tiles.len();
    let mut seen = vec![false; tiles];
    let mut parts = Vec::new();
    let (ex, ey) = world.zoo.entrance;
    let entrance = i32::try_from(ex)
        .ok()
        .zip(i32::try_from(ey).ok())
        .and_then(|(x, y)| world.tile_index(x, y));
    for start in entrance.into_iter().chain(0..tiles) {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut members = vec![start];
        let mut at = 0;
        while let Some(&t) = members.get(at) {
            at += 1;
            for d in [0, 2, 4, 6] {
                if let Some(n) = world.neighbour(t, d)
                    && !seen[n]
                    && !world.closed(t, d)
                {
                    seen[n] = true;
                    members.push(n);
                }
            }
        }
        let grounds = Some(start) == entrance || members.iter().any(|&t| !world.in_zoo(t));
        parts.push(Part {
            tiles: members,
            grounds,
        });
    }
    parts
}

impl Exhibits {
    /// Finds the regions of a world. The exhibits are numbered from 1 in the order of their first
    /// tile. A region that holds the entrance tile or a tile beyond the zoo wall is the zoo
    /// grounds (the gate buildings join the exterior to the grounds), and the tiles beyond the
    /// wall are marked [`EXTERIOR`] in it.
    pub fn new(world: &World) -> Exhibits {
        let mut found = Exhibits {
            region: vec![GROUNDS; world.zoo.tiles.len()],
            next: 1,
            counter: 1,
            ..Exhibits::default()
        };
        for part in parts(world).iter().filter(|p| !p.grounds) {
            found.add(part, String::new());
        }
        found.exterior(world);
        found
    }

    /// An empty set for a map of `tiles` tiles, with this one's numbers.
    pub fn blank(&self, tiles: usize) -> Exhibits {
        Exhibits {
            region: vec![GROUNDS; tiles],
            list: Vec::new(),
            next: self.next,
            counter: self.counter,
        }
    }

    /// Adds an exhibit that goes on, with the tiles it holds now.
    pub fn keep(&mut self, exhibit: Exhibit, part: &Part) {
        let id = exhibit.id;
        self.list.push(exhibit);
        self.assign(id, part);
    }

    /// Adds the part as a new exhibit with the next number and a name.
    pub fn add(&mut self, part: &Part, name: String) -> u32 {
        let id = self.next;
        self.next += 1;
        self.list.push(Exhibit {
            id,
            first: 0,
            tiles: 0,
            name,
            gate: None,
            tank: None,
        });
        self.assign(id, part);
        id
    }

    /// The tiles of the part belong to exhibit `id`.
    pub fn assign(&mut self, id: u32, part: &Part) {
        for &t in &part.tiles {
            self.region[t] = id;
        }
        if let Some(e) = self.list.iter_mut().find(|e| e.id == id) {
            e.first = part.tiles.iter().copied().min().unwrap_or(0);
            e.tiles = part.tiles.len();
        }
    }

    /// Marks the tiles beyond the zoo wall.
    pub fn exterior(&mut self, world: &World) {
        for (t, r) in self.region.iter_mut().enumerate() {
            if !world.in_zoo(t) {
                *r = EXTERIOR;
            }
        }
    }

    /// The exhibit numbered `id`.
    pub fn get(&self, id: u32) -> Option<&Exhibit> {
        self.list.iter().find(|e| e.id == id)
    }

    pub fn list_mut(&mut self) -> &mut [Exhibit] {
        &mut self.list
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Exhibit> {
        self.list.iter_mut().find(|e| e.id == id)
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

    /// The exhibits, in the order they were made.
    pub fn list(&self) -> &[Exhibit] {
        &self.list
    }
}

#[cfg(test)]
mod tests;
