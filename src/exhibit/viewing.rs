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
/// A tank is murky for the see-through walls when its purity is at most this (a fixed value of
/// the program).
const MURKY: u32 = 1;

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
    /// `cHeight`, in elevation steps.
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

/// The map as the line of sight reads it (10.4). Elevations are in steps; the line itself is in
/// half steps. A tile has its base elevation, for each of its four quadrants the top of the
/// scenery that blocks the sight (the elevation of the corner the original reads plus the
/// scenery's `cHeight`) and for each side 0, 2, 4, 6 the top of the fence that blocks (its
/// tile's elevation plus its height); `i32::MIN` for none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ground {
    pub width: usize,
    pub height: usize,
    pub base: Vec<i32>,
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
            scenery: vec![[i32::MIN; 4]; n],
            fence: vec![[i32::MIN; 4]; n],
        }
    }

    /// Reads the ground of a world; `body` tells what blocks of each entity.
    pub fn of(world: &World, body: impl Fn(&Entity) -> Body) -> Ground {
        let mut ground = Ground::new(world.zoo.width as usize, world.zoo.height as usize);
        for (t, tile) in world.zoo.tiles.iter().enumerate() {
            ground.base[t] = tile.elevation;
            // The corner read for each quadrant, the last two the diagonally opposite ones.
            for (q, corner) in [7, 5, 1, 3].into_iter().enumerate() {
                if let Some(e) = world.quadrant(t, q) {
                    let b = body(&world.zoo.entities[e]);
                    if b.blocks {
                        ground.scenery[t][q] = corner_elevation(tile, corner) + b.height;
                    }
                }
            }
            let tank = world.tank_at(t).and_then(|id| world.tank(id));
            for d in [0, 2, 4, 6] {
                let Some(e) = world.edge(t, d) else { continue };
                let b = body(&world.zoo.entities[e]);
                let wall = world.shape(e).is_some_and(|s| s.tank.wall);
                let murky = tank.is_some_and(|tank| tank.purity <= MURKY);
                if b.see_through && !(wall && murky) {
                    continue;
                }
                let own = tank.map_or(0, |tank| tank.height as i32);
                let h = if wall && own != 0 { own } else { b.height };
                ground.fence[t][d / 2] = tile.elevation + h;
            }
        }
        ground
    }

    /// Whether a fence stands on the edge between the adjacent tiles `a` and `b` (the slot of
    /// either) with its top above the eye `eye`, in steps.
    fn closed(&self, a: usize, b: usize, eye: i32) -> bool {
        let side = if a / self.width == b / self.width {
            if b > a { 1 } else { 3 }
        } else if b > a {
            2
        } else {
            0
        };
        self.fence[a][side].max(self.fence[b][(side + 2) % 4]) > eye
    }

    /// Whether the line passes between the tiles `prev` and `t` of one step (10.4): in one
    /// column or row it is blocked by a fence on the edge, across a corner only when both routes
    /// round the corner are closed.
    fn cut(&self, t: usize, prev: usize, eye: i32) -> bool {
        let w = self.width;
        if t % w == prev % w || t / w == prev / w {
            return self.closed(prev, t, eye);
        }
        let a = (prev / w) * w + t % w;
        let b = (t / w) * w + prev % w;
        let route = |m: usize| self.closed(prev, m, eye) || self.closed(m, t, eye);
        route(a) && route(b)
    }

    /// Whether tile `from` sees tile `to`: the integer walk of rule 10.4 from an eye one step
    /// above the base of `from` to one above the base of `to`, every point tested against the
    /// ground (the base elevation of its tile), the scenery of its quadrant and, at each change
    /// of tile, the fences between the tiles (against the eye of `from`).
    pub fn sees(&self, from: usize, to: usize) -> bool {
        let w = self.width;
        let at = |t: usize| {
            [
                (2 * (t % w)) as i32,
                (2 * (t / w)) as i32,
                2 * self.base[t] + 2,
            ]
        };
        let (a, b) = (at(from), at(to));
        let s: [i32; 3] = std::array::from_fn(|i| (b[i] - a[i]).signum());
        let n: [i32; 3] = std::array::from_fn(|i| 2 * (b[i] - a[i]).abs());
        let major = if n[0] >= n[1].max(n[2]) {
            0
        } else if n[1] >= n[0].max(n[2]) {
            1
        } else {
            2
        };
        let mut err: [i32; 3] = std::array::from_fn(|i| n[i] - (n[major] >> 1));
        let (mut p, mut prev) = (a, None);
        let eye = self.base[from] + 1;
        loop {
            // With z major the tile is taken half a tile on along a growing axis.
            let shift = |i: usize| i32::from(major == 2 && a[i] < b[i]);
            let t = (((p[1] + shift(1)) >> 1) as usize) * w + ((p[0] + shift(0)) >> 1) as usize;
            let q = ((p[1] & 1) * 2 + (p[0] & 1)) as usize;
            if p[2] < 2 * self.base[t]
                || p[2] < self.scenery[t][q]
                || prev.is_some_and(|prev| prev != t && self.cut(t, prev, eye))
            {
                return false;
            }
            if p[major] == b[major] {
                return true;
            }
            for i in (0..3).filter(|&i| i != major) {
                if err[i] >= 0 {
                    p[i] += s[i];
                    err[i] -= n[major];
                }
            }
            p[major] += s[major];
            for i in (0..3).filter(|&i| i != major) {
                err[i] += n[i];
            }
            prev = Some(t);
        }
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
    /// the animal to the tile is taken from the signs of the offsets (an animal on the tile has
    /// none: both parts lose).
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
            p = (0.9 * p).round_ties_even();
        }
        if guests > 0 && p == 0.0 {
            p = 1.0;
        }
        let p = p as i32;
        let dir = STEP
            .iter()
            .position(|&s| s == (dx.signum(), dy.signum()))
            .unwrap_or(8);
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
/// number); it is broken when its life is below 1, unless it is a tank wall or a zoo wall. The state goes to the exhibit that owns the
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
        // Tank walls and zoo walls never break, whatever their life.
        let sturdy = world.shape(e).is_some_and(|s| s.tank.wall || s.wall);
        let hurt = if life < 1 && !sturdy {
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
