//! One shaping step over the brush and the contiguous spreading (terrain editing 5.2 to 5.4).
//!
//! Pure: the step reads the [`World`] and says which tiles take which corners. A tile's corners
//! are kept in the order 7, 1, 3, 5, so the next one shares a side with it and the one two on is
//! the opposite corner.

use super::Type;
use crate::render::corner_elevation;
use crate::world::World;
use crate::zoo::Tile;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The corners stay between these elevations.
pub const LIMIT: i32 = 12;
/// The shape bytes there are (map drawing 3.3).
const LEGAL: [u8; 19] = [
    0x00, 0x01, 0x04, 0x05, 0x10, 0x11, 0x14, 0x15, 0x19, 0x40, 0x41, 0x44, 0x45, 0x46, 0x50, 0x51,
    0x54, 0x64, 0x91,
];
/// The offset from the tile's top left point of the point of each corner, in the order 7, 1, 3, 5.
const POINT: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];
/// The order the corners are looked at in (7, 1, 5, 3).
const ORDER: [usize; 4] = [0, 1, 3, 2];

/// What a step asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Raise,
    Lower,
    /// Bring the brush to this elevation.
    Level(i32),
}

/// What a step does to a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Raise,
    Lower,
    Fill,
    Cut,
}

pub type Corners = [i32; 4];

/// The elevations of the corners 7, 1, 3, 5 of a tile.
pub fn corners(tile: &Tile) -> Corners {
    [7, 1, 3, 5].map(|c| corner_elevation(tile, c))
}

/// The elevation and shape byte of corners; `None` when they are not one of the 19 shapes.
fn tile_of(c: &Corners) -> Option<(i32, u8)> {
    let low = *c.iter().min()?;
    let field = |k: usize| u8::try_from(c[k] - low).ok().filter(|&f| f <= 3);
    let shape = field(0)? | field(3)? << 2 | field(2)? << 4 | field(1)? << 6;
    LEGAL.contains(&shape).then_some((c[0], shape))
}

struct Step<'a> {
    world: &'a World,
    types: &'a [Type],
    brush: &'a [usize],
    /// The corners of the tiles that changed so far.
    now: BTreeMap<usize, Corners>,
}

impl Step<'_> {
    fn get(&self, t: usize) -> Corners {
        self.now
            .get(&t)
            .copied()
            .unwrap_or_else(|| corners(&self.world.zoo.tiles[t]))
    }

    fn water(&self, t: usize) -> bool {
        let kind = self.world.zoo.tiles[t].terrain;
        self.types.iter().any(|ty| ty.kind == kind && ty.water)
    }

    /// A tile that keeps its shape and moves as a whole: water, or under an object that is
    /// not a quarter tile (5.1).
    fn rigid(&self, t: usize) -> bool {
        self.water(t) || self.world.sized(t)
    }

    /// The tile `t` is changed by `action` toward `goal`; the corners that moved.
    fn tile(&mut self, t: usize, action: Action, goal: i32) -> Corners {
        let old = self.get(t);
        let low = old.into_iter().min().unwrap_or(0);
        let high = old.into_iter().max().unwrap_or(0);
        let mut c = old;
        if self.rigid(t) {
            let by = match action {
                Action::Raise if goal > high => 1,
                Action::Lower if goal < low => -1,
                Action::Fill if goal > high => goal.saturating_sub(old[0]),
                Action::Cut if goal < low => goal.saturating_sub(old[0]),
                _ => 0,
            };
            if c.iter()
                .all(|&e| (-LIMIT..=LIMIT).contains(&e.saturating_add(by)))
            {
                c = c.map(|e| e.saturating_add(by));
            }
        } else {
            match action {
                Action::Raise if low < goal => c = c.map(|e| e.saturating_add(i32::from(e == low))),
                Action::Lower if high > goal => {
                    c = c.map(|e| e.saturating_sub(i32::from(e == high)))
                }
                Action::Fill if low < goal => c = [goal; 4],
                Action::Cut if high > goal => c = [goal; 4],
                _ => {}
            }
        }
        if c != old {
            self.now.insert(t, c);
        }
        c
    }

    /// The ground around the brush follows the corners that moved by `d` (5.4).
    fn spread(&mut self, mut queue: VecDeque<(usize, usize)>, d: i32) {
        let width = self.world.zoo.width as i32;
        while let Some((t, i)) = queue.pop_front() {
            let v = self.get(t)[i];
            let (x, y) = (t as i32 % width, t as i32 / width);
            for j in (0..4).filter(|&j| j != i) {
                let at = (x + POINT[i].0 - POINT[j].0, y + POINT[i].1 - POINT[j].1);
                let Some(b) = self.world.tile_index(at.0, at.1) else {
                    continue;
                };
                // Water is not moved, unless a large object stands on it (5.1).
                if self.brush.contains(&b)
                    || !self.world.in_zoo(b)
                    || (self.water(b) && !self.world.big(b))
                {
                    continue;
                }
                let old = self.get(b);
                if old[j] != v.saturating_sub(d) {
                    continue;
                }
                let mut c = old;
                if self.rigid(b) {
                    if c.iter()
                        .any(|&e| !(-LIMIT..=LIMIT).contains(&e.saturating_add(d)))
                    {
                        continue;
                    }
                    c = c.map(|e| e.saturating_add(d));
                } else {
                    c[j] = v;
                    for k in [(j + 1) % 4, (j + 3) % 4] {
                        if c[k].abs_diff(v) > 1 {
                            c[k] = c[k].saturating_add(d);
                        }
                    }
                    let o = (j + 2) % 4;
                    if c[o].abs_diff(v) > 2 {
                        c[o] = c[o].saturating_add(d);
                    }
                }
                self.now.insert(b, c);
                queue.extend((0..4).filter(|&k| c[k] != old[k]).map(|k| (b, k)));
            }
        }
    }
}

/// The tiles that objects larger than one tile standing on the tiles of `brush` (inside the zoo)
/// cover.
fn under(world: &World, brush: &[usize]) -> BTreeSet<usize> {
    brush
        .iter()
        .filter(|&&t| world.in_zoo(t))
        .flat_map(|&t| world.carried(t))
        .collect()
}

/// The tiles outside the brush that those objects take along in a non-contiguous step, inside the
/// zoo, in index order (terrain editing 5.5).
pub fn carried(world: &World, brush: &[usize]) -> Vec<usize> {
    let mut tiles = under(world, brush);
    tiles.retain(|t| !brush.contains(t) && world.in_zoo(*t));
    tiles.into_iter().collect()
}

/// The tiles a non-contiguous step adds to the snapshot unchanged, so that they are drawn again:
/// the ring of one tile around the brush and the rectangle that bounds the tiles of `carried`'s
/// objects, grown by one tile (terrain editing 5.5), clipped to the map, in index order.
pub fn ring(world: &World, brush: &[usize]) -> Vec<usize> {
    let width = world.zoo.width as usize;
    let mut rings = BTreeSet::new();
    let mut grow = |tiles: &mut dyn Iterator<Item = usize>| {
        let cells: Vec<(i32, i32)> = tiles
            .map(|t| ((t % width) as i32, (t / width) as i32))
            .collect();
        let (Some(x0), Some(x1), Some(y0), Some(y1)) = (
            cells.iter().map(|c| c.0).min(),
            cells.iter().map(|c| c.0).max(),
            cells.iter().map(|c| c.1).min(),
            cells.iter().map(|c| c.1).max(),
        ) else {
            return;
        };
        for y in y0 - 1..=y1 + 1 {
            for x in x0 - 1..=x1 + 1 {
                rings.extend(world.tile_index(x, y));
            }
        }
    };
    grow(&mut brush.iter().copied().filter(|&t| world.in_zoo(t)));
    grow(&mut under(world, brush).into_iter());
    rings.into_iter().collect()
}

/// One step of `request` over the `brush` tiles, in rows (terrain editing 5.2). The tiles that
/// changed, in index order, with their new elevation and shape byte; none when the step does
/// nothing. Only tiles inside the zoo change.
pub fn run(
    world: &World,
    types: &[Type],
    brush: &[usize],
    contiguous: bool,
    request: Request,
) -> Vec<(usize, i32, u8)> {
    let brush: Vec<usize> = brush.iter().copied().filter(|&t| world.in_zoo(t)).collect();
    let mut step = Step {
        world,
        types,
        brush: &brush,
        now: BTreeMap::new(),
    };
    let all = || brush.iter().flat_map(|&t| corners(&world.zoo.tiles[t]));
    let (Some(low), Some(high)) = (all().min(), all().max()) else {
        return Vec::new();
    };
    let (action, goal) = match request {
        Request::Raise => (Action::Raise, low.saturating_add(1)),
        Request::Lower => (Action::Lower, high.saturating_sub(1)),
        Request::Level(t) if low < t => match contiguous {
            true => (Action::Raise, low.saturating_add(1)),
            false => (Action::Fill, t),
        },
        Request::Level(t) if high > t => match contiguous {
            true => (Action::Lower, high.saturating_sub(1)),
            false => (Action::Cut, t),
        },
        Request::Level(_) => return Vec::new(),
    };
    if !(-LIMIT..=LIMIT).contains(&goal) {
        return Vec::new();
    }
    let mut queue = VecDeque::new();
    for &t in &brush {
        let old = step.get(t);
        let new = step.tile(t, action, goal);
        queue.extend(ORDER.iter().filter(|&&k| new[k] != old[k]).map(|&k| (t, k)));
    }
    if contiguous {
        step.spread(queue, if action == Action::Raise { 1 } else { -1 });
    } else {
        for t in carried(world, &brush) {
            step.tile(t, action, goal);
        }
    }
    step.now
        .iter()
        .filter(|&(&t, c)| corners(&world.zoo.tiles[t]) != *c)
        .filter_map(|(&t, c)| tile_of(c).map(|(elevation, shape)| (t, elevation, shape)))
        .collect()
}
