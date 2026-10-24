//! The exhibits of a zoo: the regions its exhibit fences make (exhibits and money 3.1).
//!
//! Tiles joined through sides that hold no exhibit fence form one region. The region that holds
//! the zoo entrance is the zoo grounds, the tiles beyond the zoo wall are the exterior, and
//! every other region is an exhibit, from one tile up. The map edge closes a region like a fence.

use crate::config::{Config, to_int};
use crate::timing::Date;
use crate::vfs::Vfs;
use crate::world::World;

pub mod rating;
pub mod viewing;

/// The region of the zoo grounds.
pub const GROUNDS: u32 = 0;
/// The region of the tiles beyond the zoo wall.
pub const EXTERIOR: u32 = u32::MAX;

/// An exhibit: its number (kept while it exists), name, gate and size.
#[derive(Debug, Clone, PartialEq)]
pub struct Exhibit {
    pub id: u32,
    /// The first tile of its region (row by row).
    pub first: usize,
    pub tiles: usize,
    /// Its tiles in the original's order (exhibits and money 3.1), as the last fill left them.
    pub order: Vec<usize>,
    /// The name; empty for an exhibit found in a map of another language (zoo format 2 rule 5).
    pub name: String,
    /// The id of the fence piece that is its gate.
    pub gate: Option<u32>,
    /// What a tank has besides: set when every wall of its boundary is a tank wall.
    pub tank: Option<Tank>,
    /// The milliseconds the water of a tank has held its purity (exhibits and money 3.10).
    pub purity_ms: u32,
    /// The id of the filter that serves a tank, as the last filter added or updated left it
    /// (tank staff 7.2, 7.3); not saved.
    pub filter: Option<u32>,
    /// The exhibits it is linked to as amphibious neighbours, in the order they became so
    /// (tank fauna 12); the company checks count their animals and tiles with its own. The world
    /// makes them again after each edit (`World::relink`).
    pub amphibious: Vec<u32>,
    /// The tanks it is linked to as show neighbours (exhibits and money 3.11), in the order they
    /// became so: a show tank and a plain tank that touch, both filled and at least 4 high, with
    /// wall tops at one level. Made again with the amphibious links.
    pub show_near: Vec<u32>,
    /// Its records, species list and timers (habitat rating).
    pub rating: rating::State,
    /// What its panel shows besides the rating.
    pub stats: Stats,
}

/// The statistics of an exhibit (habitat rating 13.2 and 13.3). The simulation writes them: the
/// guests' viewing the popularity and the donations, the month change the shift of the triples.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    /// The calendar date it was made.
    pub created: Option<Date>,
    /// 0 to 100.
    pub popularity: u32,
    /// The donations: last month, this month, in total.
    pub donations: [f32; 3],
    /// The exhibit's spending (the keepers' food and healing costs; the panel calls it upkeep),
    /// in the same order.
    pub upkeep: [f32; 3],
    /// The calendar time guests spent watching it, in units of 100 ns (habitat rating 13.2).
    pub viewing: u64,
}

impl Stats {
    /// The month ends (habitat rating 13.3): this month's figures become last month's and this
    /// month starts from 0; the totals keep running.
    pub fn enter_new_month(&mut self) {
        for t in [&mut self.donations, &mut self.upkeep] {
            *t = [t[1], 0.0, t[2]];
        }
    }
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
    /// The tank is filled (not drained): the flag of exhibits and money 4.7.
    pub filled: bool,
    /// The tile the gate's platform is drawn on, (-1, -1) for none.
    pub platform: (i32, i32),
}

impl Tank {
    /// The wall height and the water level in levels, cut to 1000 so that a sum with a floor or a
    /// product with 16 never overflows (a file can store any number).
    pub fn steps(&self) -> (i32, i32) {
        let cut = |v: u32| v.min(1000) as i32;
        (cut(self.height), cut(self.level))
    }
}

/// The numbers of `tanks.cfg` that make a new tank (exhibits and money 3.9 and 4.7); the first of
/// a pair is for a tank, the second for a show tank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TankConfig {
    /// How far the ground of a new tank is lowered (`initialSink`, `initialSinkShow`).
    pub sink: [i32; 2],
    /// The wall height (`initialHeight`, `initialHeightShow`).
    pub height: [u32; 2],
    /// `initialWaterLevel`, `initialFillState` above 0, `initialSalinity` above 0 (salt) and
    /// `initialWaterPurity`.
    pub level: u32,
    pub filled: bool,
    pub salt: bool,
    pub purity: u32,
    /// The terrain types of the floor of a tank and of a show tank (`tankTerrain`,
    /// `tankTerrainShow`).
    pub terrain: [u8; 2],
    /// `waterOffset`: the full water level is the wall height plus this (3.10).
    pub offset: i32,
    /// `maximumTankHeight`: the highest wall height.
    pub limit: u32,
    /// `murkyWaterPurity`: the purity below which the water is not clean.
    pub murky: i32,
    /// `waterPurityDecayTime`: the milliseconds before the purity of the water drops one point.
    pub decay: u32,
}

impl Default for TankConfig {
    /// What there is without a `tanks.cfg`: the heights of 1 the original has built in, the rest
    /// empty, and both floors 0 (grass): the original gives `tankTerrain` and `tankTerrainShow` no
    /// default.
    fn default() -> TankConfig {
        TankConfig {
            sink: [0; 2],
            height: [1; 2],
            level: 0,
            filled: false,
            salt: false,
            purity: 100,
            terrain: [0; 2],
            offset: 0,
            limit: 30,
            murky: 60,
            decay: 30_000,
        }
    }
}

impl TankConfig {
    /// The `tanks.cfg` of the game, or the default without one (or one that cannot be read).
    pub fn load(vfs: &Vfs) -> TankConfig {
        vfs.read("tanks.cfg")
            .ok()
            .and_then(|bytes| Config::parse(&bytes).ok())
            .map_or_else(TankConfig::default, |c| TankConfig::read(&c))
    }

    /// Reads the `[tank]` section of a `tanks.cfg`; a key that is missing keeps the default, and
    /// a number is cut to what a tank can hold (a height or level 0 to 255, a sink -24 to 24, a
    /// purity 0 to 100, a terrain type 0 to 255).
    pub fn read(config: &Config) -> TankConfig {
        let tank = config.section("tank");
        let number = |key: &str| tank.and_then(|s| s.get(key)).and_then(to_int);
        let mut found = TankConfig::default();
        let (mut sink, mut height) = (found.sink, found.height);
        for (i, (s, h)) in [
            ("initialSink", "initialHeight"),
            ("initialSinkShow", "initialHeightShow"),
        ]
        .into_iter()
        .enumerate()
        {
            sink[i] = number(s).map_or(sink[i], |n| n.clamp(-24, 24));
            height[i] = number(h).map_or(height[i], |n| n.clamp(0, 255) as u32);
        }
        found.sink = sink;
        found.height = height;
        found.level = number("initialWaterLevel").map_or(found.level, |n| n.clamp(0, 255) as u32);
        found.filled = number("initialFillState").map_or(found.filled, |n| n > 0);
        found.salt = number("initialSalinity").map_or(found.salt, |n| n > 0);
        found.purity =
            number("initialWaterPurity").map_or(found.purity, |n| n.clamp(0, 100) as u32);
        for (i, key) in ["tankTerrain", "tankTerrainShow"].into_iter().enumerate() {
            found.terrain[i] = number(key).map_or(found.terrain[i], |n| n.clamp(0, 255) as u8);
        }
        found.offset = number("waterOffset").map_or(found.offset, |n| n.clamp(-255, 255));
        found.limit = number("maximumTankHeight").map_or(found.limit, |n| n.clamp(0, 255) as u32);
        found.murky = number("murkyWaterPurity").unwrap_or(found.murky);
        found.decay = number("waterPurityDecayTime").map_or(found.decay, |n| n.max(0) as u32);
        found
    }

    /// The level of a full tank of this wall height, `H + waterOffset` cut to 0 and 255 (3.10).
    pub fn top(&self, height: u32) -> u32 {
        (i64::from(height) + i64::from(self.offset)).clamp(0, 255) as u32
    }
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
            filled: false,
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
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Exhibits {
    region: Vec<u32>,
    list: Vec<Exhibit>,
    /// The number the next new exhibit gets.
    pub next: u32,
    /// The number of the next default name (exhibits and money 3.6).
    pub counter: u32,
    /// The zoo grounds' own timers and record (habitat rating 4): the update is the exhibits',
    /// and its recomputation only clears the record.
    pub grounds: rating::State,
}

/// The tiles of the region of `start` in the original's order (exhibits and money 3.1): a stack of
/// seed tiles, each popped tile that is not in yet puts in its row run (the tile, then the tiles
/// to its west, then those to its east), and pushes the tiles above and below the run from west
/// to east. The list is the reverse of the order the tiles were put in.
pub fn fill(world: &World, start: usize) -> Vec<usize> {
    let mut put = vec![false; world.zoo.tiles.len()];
    let mut list = Vec::new();
    let mut seeds = vec![start];
    while let Some(t) = seeds.pop() {
        if std::mem::replace(&mut put[t], true) {
            continue;
        }
        list.push(t);
        let mut ends = [t; 2];
        for (end, d) in ends.iter_mut().zip([6, 2]) {
            while !world.closed(*end, d)
                && let Some(n) = world.neighbour(*end, d)
            {
                if !std::mem::replace(&mut put[n], true) {
                    list.push(n);
                }
                *end = n;
            }
        }
        let mut at = ends[0];
        loop {
            for d in [0, 4] {
                if let Some(n) = world.neighbour(at, d)
                    && !put[n]
                    && !world.closed(at, d)
                {
                    seeds.push(n);
                }
            }
            match world.neighbour(at, 2) {
                Some(n) if at != ends[1] => at = n,
                _ => break,
            }
        }
    }
    list.reverse();
    list
}

/// The line of the boundary of the region `tiles` that the file closes last (exhibits and money
/// 3.8): a line is closed by the earlier in the file of the exhibit fence pieces on its two slots.
/// The tile inside, the tile across and the tile of the closing piece.
pub fn closed_last(world: &World, tiles: &[usize]) -> Option<(usize, usize, usize)> {
    let inside: std::collections::HashSet<usize> = tiles.iter().copied().collect();
    let mut last: Option<(usize, (usize, usize, usize))> = None;
    for &t in tiles {
        for d in [0, 2, 4, 6] {
            let Some(n) = world.neighbour(t, d).filter(|n| !inside.contains(n)) else {
                continue;
            };
            let pieces = [(t, world.edge(t, d)), (n, world.edge(n, (d + 4) % 8))];
            let closing = (pieces.into_iter())
                .filter_map(|(tile, p)| p.map(|p| (p, tile)))
                .filter(|&(p, _)| world.shape(p).is_some_and(|s| s.fence))
                .min();
            if let Some((p, tile)) = closing
                && last.is_none_or(|(l, _)| p > l)
            {
                last = Some((p, (t, n, tile)));
            }
        }
    }
    last.map(|l| l.1)
}

/// The boundary pairs of the tile list `order` (3.1): for each tile, for the sides 0, 2, 4, 6, the
/// pair (tile, neighbour) when `inside` says the neighbour is not in the exhibit; no neighbour at
/// the map edge.
pub fn boundary_of(
    world: &World,
    order: &[usize],
    inside: &dyn Fn(usize) -> bool,
) -> Vec<(usize, Option<usize>)> {
    let mut pairs = Vec::new();
    for &t in order {
        for d in [0, 2, 4, 6] {
            let n = world.neighbour(t, d);
            if n.is_none_or(|n| !inside(n)) {
                pairs.push((t, n));
            }
        }
    }
    pairs
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
            grounds: rating::State::new(),
            ..Exhibits::default()
        };
        // A loaded exhibit is filled from its tile on the line the file closes last (3.8).
        for part in parts(world).into_iter().filter(|p| !p.grounds) {
            let start =
                closed_last(world, &part.tiles)
                    .map(|l| l.0)
                    .or(part.tiles.iter().copied().min());
            let size = part.tiles.len();
            let tiles = start.map_or(part.tiles, |s| fill(world, s));
            debug_assert_eq!(tiles.len(), size, "the fill finds the region");
            found.add(&Part { tiles, ..part }, String::new());
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
            grounds: self.grounds.clone(),
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
            order: Vec::new(),
            name,
            gate: None,
            tank: None,
            purity_ms: 0,
            filter: None,
            amphibious: Vec::new(),
            show_near: Vec::new(),
            rating: rating::State::new(),
            stats: Stats::default(),
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
            e.order.clone_from(&part.tiles);
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

    /// The month ends: every exhibit shifts its monthly figures.
    pub fn enter_new_month(&mut self) {
        for e in &mut self.list {
            e.stats.enter_new_month();
        }
    }

    /// Marks exhibit `id` to be rated again (habitat rating 4), and its amphibious and show
    /// neighbours with it, theirs in turn, up to those already marked; the zoo grounds have no
    /// rating.
    pub fn mark(&mut self, id: u32) {
        let Some(e) = self.get_mut(id) else {
            return;
        };
        e.rating.dirty = true;
        for n in e
            .amphibious
            .iter()
            .chain(&e.show_near)
            .copied()
            .collect::<Vec<_>>()
        {
            if self.get(n).is_some_and(|n| !n.rating.dirty) {
                self.mark(n);
            }
        }
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

    /// The boundary of exhibit `id` in the original's order (3.1): for each tile of its list, for
    /// the sides 0, 2, 4, 6, the pair (tile, neighbour outside it); no neighbour at the map edge.
    pub fn boundary(&self, world: &World, id: u32) -> Vec<(usize, Option<usize>)> {
        let tiles = self.get(id).map_or(&[][..], |e| &e.order);
        boundary_of(world, tiles, &|n| self.region(n) == Some(id))
    }

    /// Gives exhibit `id` a new tile list: the same tiles, filled again.
    pub fn reorder(&mut self, id: u32, order: Vec<usize>) {
        if let Some(e) = self.get_mut(id) {
            e.order = order;
        }
    }

    /// The exhibits, in the order they were made.
    pub fn list(&self) -> &[Exhibit] {
        &self.list
    }
}

#[cfg(test)]
mod tests;
