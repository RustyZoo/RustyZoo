//! The viewing areas of an exhibit (habitat rating 10) and its deterioration state (12).
//!
//! A viewing area is a group of path tiles near a fenced side of an exhibit; it sees the exhibit
//! tiles within 10 tiles in line of sight. The grouping works on a plain [`Plan`] and the line of
//! sight on a plain [`Ground`], so both run without a world; [`Plan::of`] and [`Ground::of`] read
//! them from one. The pressure of the guests on the animals seen is [`Area::pressure`]: a caller
//! with units draws it for each animal and adds the result to the animal's totals.
//!
//! Nothing is cached: the callers build the areas again when a path or a fence changes and call
//! [`wear`] when a fence changes and after a game is loaded (rule 12).

use std::collections::HashMap;

use crate::build::number;
use crate::entity::{Catalogue, EntityType};
use crate::render::corner_elevation;
use crate::world::World;
use crate::zoo::Entity;

/// The map direction `d` (0 to 7, 0 is -y, then clockwise) as a step in x and y.
const STEP: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];
/// A tile joins an area when the area covers at most this many rows or columns (inclusive).
const SPAN: i32 = 3;
/// How far a viewing tile looks along a direction for its exhibit, and how far it sees (squared).
const LOOK: i32 = 3;
const RANGE: i32 = 100;
/// A tank is murky for the see-through walls when its purity is at most this. The original's
/// constant is not known; this is the murky threshold of the animals' files.
const MURKY: u32 = 60;

/// The exhibit and its surroundings as the grouping reads them. Tiles are indices of the map,
/// row by row; the vectors hold one entry per map tile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub width: usize,
    pub height: usize,
    /// The tiles of the exhibit in its list order.
    pub order: Vec<usize>,
    pub inside: Vec<bool>,
    /// A tile of the exhibit with a habitat fence on one of its four sides.
    pub fenced: Vec<bool>,
    pub path: Vec<bool>,
    pub in_zoo: Vec<bool>,
}

impl Plan {
    /// Reads exhibit `id` of a world.
    pub fn of(world: &World, id: u32) -> Plan {
        let tiles = world.zoo.tiles.len();
        let mut plan = Plan {
            width: world.zoo.width as usize,
            height: world.zoo.height as usize,
            order: world
                .exhibits()
                .get(id)
                .map_or_else(Vec::new, |e| e.order.clone()),
            inside: vec![false; tiles],
            fenced: vec![false; tiles],
            path: (0..tiles).map(|t| world.path(t).is_some()).collect(),
            in_zoo: (0..tiles).map(|t| world.in_zoo(t)).collect(),
        };
        for &t in &plan.order {
            plan.inside[t] = true;
            plan.fenced[t] = [0, 2, 4, 6].into_iter().any(|d| world.closed(t, d));
        }
        plan
    }

    /// The tile `d` steps away in direction `k`, if it is on the map.
    fn step(&self, t: usize, k: usize, d: i32) -> Option<usize> {
        let x = (t % self.width) as i32 + STEP[k].0 * d;
        let y = (t / self.width) as i32 + STEP[k].1 * d;
        let on = (0..self.width as i32).contains(&x) && (0..self.height as i32).contains(&y);
        on.then(|| y as usize * self.width + x as usize)
    }
}

/// A group of path tiles, and what it sees.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Area {
    /// Its tiles in the order they joined.
    pub tiles: Vec<usize>,
    /// For each tile, the directions (bit `k`) in which it looks at the exhibit within 3 tiles;
    /// a tile with none belongs to the list but to no direction.
    pub looks: Vec<u8>,
    /// The exhibit tiles it sees (10.4), set by [`Area::observe`].
    pub seen: Vec<usize>,
}

/// Builds the viewing areas of an exhibit (10.2 and 10.3): the path tiles within 2 tiles of a
/// fenced tile, each joined to the area of a neighbour with room, else made a new area.
pub fn areas(plan: &Plan) -> Vec<Area> {
    let mut set: Vec<usize> = Vec::new();
    let mut taken = vec![false; plan.inside.len()];
    for &e in plan.order.iter().filter(|&&e| plan.fenced[e]) {
        let (ex, ey) = ((e % plan.width) as i32, (e / plan.width) as i32);
        for x in ex - 2..=ex + 2 {
            for y in ey - 2..=ey + 2 {
                let on =
                    (0..plan.width as i32).contains(&x) && (0..plan.height as i32).contains(&y);
                let t = (y.max(0) as usize) * plan.width + x.max(0) as usize;
                if on && !plan.inside[t] && !taken[t] {
                    taken[t] = true;
                    set.push(t);
                }
            }
        }
    }
    let mut areas: Vec<Area> = Vec::new();
    // The area held by each direction of a tile.
    let mut slots: HashMap<usize, [Option<usize>; 8]> = HashMap::new();
    for t in set.into_iter().filter(|&t| plan.path[t] && plan.in_zoo[t]) {
        let joined = [0, 2, 4, 6].into_iter().find_map(|k| {
            let n = plan.step(t, k, 1).filter(|&n| plan.path[n])?;
            let a = slots.get(&n)?.iter().flatten().next().copied()?;
            // Rows for a neighbour along y, columns along x.
            let coord = |t: usize| {
                if k % 4 == 0 {
                    t / plan.width
                } else {
                    t % plan.width
                }
            };
            let tiles = areas[a].tiles.iter().map(|&t| coord(t));
            let (low, high) = tiles.fold((coord(n), coord(n)), |(l, h), c| (l.min(c), h.max(c)));
            // The rows or columns counted inclusive are `high - low + 1`.
            (((high - low) as i32) < SPAN).then_some(a)
        });
        let a = joined.unwrap_or_else(|| {
            areas.push(Area::default());
            areas.len() - 1
        });
        let mut looks = 0;
        let held = slots.entry(t).or_default();
        for (k, slot) in held.iter_mut().enumerate() {
            let first = (1..=LOOK)
                .map_while(|d| plan.step(t, k, d))
                .find(|&n| plan.inside[n]);
            if first.is_some() {
                *slot = Some(a);
                looks |= 1 << k;
            }
        }
        areas[a].tiles.push(t);
        areas[a].looks.push(looks);
    }
    areas
}

/// What the line of sight needs of one entity's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Body {
    /// `cHeight`, in elevation steps (an assumption: the unit is not specified).
    pub height: i32,
    /// `cBlocksLOS`, default 1.
    pub blocks: bool,
    /// `cSeeThrough`.
    pub see_through: bool,
}

fn kind<'a>(catalogue: &'a Catalogue, e: &Entity) -> Option<&'a EntityType> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    catalogue.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
}

/// The `cDecayedLife` of the types of a catalogue, for [`wear`].
pub fn decayed(catalogue: &Catalogue) -> impl Fn(&Entity) -> Option<i32> + '_ {
    move |e| number(kind(catalogue, e)?, "cDecayedLife").map(|v| v as i32)
}

/// The bodies of the types of a catalogue; a type that is not found has the defaults.
pub fn bodies(catalogue: &Catalogue) -> impl Fn(&Entity) -> Body + '_ {
    move |e| {
        let int = |key: &str| kind(catalogue, e).and_then(|ty| number(ty, key));
        Body {
            height: int("cHeight").map_or(0, |v| v as i32),
            blocks: int("cBlocksLOS").is_none_or(|v| v != 0.0),
            see_through: int("cSeeThrough").is_some_and(|v| v != 0.0),
        }
    }
}

/// The map as the line of sight reads it: heights in half elevation steps (a doubled `z`), so a
/// level of 2 is one step. A tile has the elevation of its four quarters' corners, the top of
/// the scenery that blocks the sight in each quarter and of the fence on each of its sides 0, 2,
/// 4, 6 (`i32::MIN` for none).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ground {
    pub width: usize,
    pub height: usize,
    /// The elevation of the reference corner of each tile.
    pub base: Vec<i32>,
    pub corner: Vec<[i32; 4]>,
    pub scenery: Vec<[i32; 4]>,
    pub fence: Vec<[i32; 4]>,
}

impl Ground {
    /// Flat ground at elevation 0 with nothing on it.
    pub fn new(width: usize, height: usize) -> Ground {
        let n = width * height;
        Ground {
            width,
            height,
            base: vec![0; n],
            corner: vec![[0; 4]; n],
            scenery: vec![[i32::MIN; 4]; n],
            fence: vec![[i32::MIN; 4]; n],
        }
    }

    /// Reads the ground of a world; `body` tells what blocks of each entity.
    pub fn of(world: &World, body: impl Fn(&Entity) -> Body) -> Ground {
        let mut ground = Ground::new(world.zoo.width as usize, world.zoo.height as usize);
        let murky = |t: usize| {
            let tank = world.tank_at(t).and_then(|id| world.tank(id));
            tank.is_some_and(|tank| tank.purity <= MURKY)
        };
        for (t, tile) in world.zoo.tiles.iter().enumerate() {
            ground.base[t] = tile.elevation;
            for (q, corner) in [7, 1, 5, 3].into_iter().enumerate() {
                let height = corner_elevation(tile, corner);
                ground.corner[t][q] = 2 * height;
                if let Some(e) = world.quadrant(t, q) {
                    let b = body(&world.zoo.entities[e]);
                    if b.blocks {
                        ground.scenery[t][q] = 2 * (height + b.height);
                    }
                }
            }
            for d in [0, 2, 4, 6] {
                let Some(e) = world.edge(t, d) else { continue };
                let b = body(&world.zoo.entities[e]);
                let wall = world.shape(e).is_some_and(|s| s.tank.wall);
                let across = world.neighbour(t, d);
                let clear = b.see_through && !(wall && (murky(t) || across.is_some_and(murky)));
                if !clear {
                    ground.fence[t][d / 2] = 2 * (tile.elevation + b.height);
                }
            }
        }
        ground
    }

    /// Whether a fence stands between tile `from` and its neighbour `to`, higher than level `z`.
    fn fenced(&self, from: usize, to: usize, z: i32) -> bool {
        let (a, b) = (from as i32, to as i32);
        let d = match b - a {
            -1 => 3,
            1 => 1,
            n if n < 0 => 0,
            _ => 2,
        };
        self.fence[from][d].max(self.fence[to][(d + 2) % 4]) > z
    }

    /// Whether tile `from` sees tile `to` (10.4): a line walked in half tile steps from one tile
    /// to the other, one elevation step above the base of each. The ground and the scenery of the
    /// two end tiles do not block it; a fence on the way does at each change of tile.
    pub fn sees(&self, from: usize, to: usize) -> bool {
        let at = |t: usize| {
            (
                (2 * (t % self.width)) as i32,
                (2 * (t / self.width)) as i32,
                2 * self.base[t] + 2,
            )
        };
        let (a, b) = (at(from), at(to));
        let (dx, dy, dz) = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
        let n = dx.abs().max(dy.abs()).max(dz.abs());
        let along = |s: i32, d: i32, i: i32| s + (2 * d * i + n).div_euclid(2 * n);
        let mut last = from;
        for i in 1..=n {
            let (x, y, z) = (along(a.0, dx, i), along(a.1, dy, i), along(a.2, dz, i));
            let t = (y >> 1) as usize * self.width + (x >> 1) as usize;
            if t != last {
                // A step across a corner crosses the x edge, then the y edge.
                let (tx, ty) = (t % self.width, t / self.width);
                let (lx, ly) = (last % self.width, last / self.width);
                let mid = ly * self.width + tx;
                let cuts = if tx != lx && ty != ly {
                    self.fenced(last, mid, z) || self.fenced(mid, t, z)
                } else {
                    self.fenced(last, t, z)
                };
                if cuts {
                    return false;
                }
                last = t;
            }
            let q = ((y & 1) * 2 + (x & 1)) as usize;
            if t != from && t != to && self.corner[t][q].max(self.scenery[t][q]) > z {
                return false;
            }
        }
        true
    }
}

impl Area {
    /// Rebuilds the observable area (10.4): for each tile in order, each remaining exhibit tile
    /// within 10 tiles and in line of sight joins it.
    pub fn observe(&mut self, plan: &Plan, ground: &Ground) {
        let mut rest = plan.order.clone();
        self.seen.clear();
        for &t in &self.tiles {
            let (x, y) = ((t % plan.width) as i32, (t / plan.width) as i32);
            rest.retain(|&e| {
                let (dx, dy) = ((e % plan.width) as i32 - x, (e / plan.width) as i32 - y);
                let seen = dx * dx + dy * dy <= RANGE && ground.sees(t, e);
                if seen {
                    self.seen.push(e);
                }
                !seen
            });
        }
    }

    /// The pressure of `guests` standing on the area on an animal on tile `animal` (10.6): a
    /// tile of the area is drawn, the guests thin out by 10 % for each tile of distance, and the
    /// total, y and x parts are returned, to be added to the animal's own. The direction from
    /// the animal to the tile is taken from the signs of the offsets (an animal on the tile
    /// counts as direction 0).
    pub fn pressure(
        &self,
        width: usize,
        guests: u32,
        animal: usize,
        rand: &mut dyn FnMut() -> u32,
    ) -> [i32; 3] {
        if self.tiles.is_empty() || width == 0 {
            return [0; 3];
        }
        let t = self.tiles[rand() as usize % self.tiles.len()];
        let (dx, dy) = (
            (t % width) as i32 - (animal % width) as i32,
            (t / width) as i32 - (animal / width) as i32,
        );
        let mut p = f64::from(guests);
        for _ in 0..f64::from(dx * dx + dy * dy).sqrt().round() as i32 {
            p = (0.9 * p).round();
        }
        if guests > 0 && p == 0.0 {
            p = 1.0;
        }
        let p = p as i32;
        let dir = STEP
            .iter()
            .position(|&s| s == (dx.signum(), dy.signum()))
            .unwrap_or(0);
        let sign = |up: bool| if up { p } else { -p };
        [
            p,
            sign([0, 1, 7].contains(&dir)),
            sign([1, 2, 3].contains(&dir)),
        ]
    }
}

/// How worn the habitat fences on the edge of an exhibit are (rule 12): the order is the order
/// of the states.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wear {
    #[default]
    Sound,
    /// A fence has decayed.
    Decayed,
    /// A fence is broken.
    Broken,
}

/// The state of every exhibit of a world. A habitat fence or tank wall counts when its life is
/// at most `decayed(entity)`, the `cDecayedLife` of its type (`None` for a type without a
/// number); it is broken when its life is below 1. The state goes to the exhibit that owns the
/// fence's tile and to the one across, when the slot of the opposite side there is empty; the
/// grounds are left out.
pub fn wear(world: &World, decayed: impl Fn(&Entity) -> Option<i32>) -> HashMap<u32, Wear> {
    let mut state: HashMap<u32, Wear> = world
        .exhibits()
        .list()
        .iter()
        .map(|e| (e.id, Wear::Sound))
        .collect();
    for (e, entity) in world.zoo.entities.iter().enumerate() {
        let life = entity
            .common
            .as_ref()
            .and_then(|c| c.fence)
            .map(|f| i64::from(f.life));
        let fence = world.shape(e).is_some_and(|s| s.fence);
        let (Some(life), true, Some(limit)) = (life, fence, decayed(entity)) else {
            continue;
        };
        let Some((t, d)) = world.slot(e).filter(|_| life <= i64::from(limit)) else {
            continue;
        };
        let hurt = if life < 1 {
            Wear::Broken
        } else {
            Wear::Decayed
        };
        let across = world
            .neighbour(t, d)
            .filter(|&n| world.edge(n, (d + 4) % 8).is_none());
        for tile in std::iter::once(t).chain(across) {
            let slot = world
                .exhibits()
                .region(tile)
                .and_then(|id| state.get_mut(&id));
            if let Some(slot) = slot {
                *slot = (*slot).max(hurt);
            }
        }
    }
    state
}
