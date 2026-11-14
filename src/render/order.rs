//! The order in which the original paints the tiles of a map and the objects on them.
//!
//! A [`Scene`] is the map reduced to what decides the order: the tiles, and for every object
//! where it stands (a tile list, a quadrant of a tile, a tile edge, or the path of a tile).
//! [`paint_order_with`] returns the sequence of things to paint, back to front. The whole map is
//! taken as visible, so the visibility tests of the original are always true. The tiles of a tank
//! exhibit are painted in layers (map drawing 9.7) instead of by the walk of the other groups;
//! the extra items they need are [`Item::Pane`], [`Item::Face`], [`Item::Surface`],
//! [`Item::Quarter`], [`Item::Under`], [`Item::Shadow`] and [`Item::Platform`].

use std::collections::HashMap;

use crate::zoo::Tile;

mod scene;
mod tank;

/// The tallest elevation a tile may have; the shipped maps stay below 13. Larger values (a
/// damaged file) are cut so that the picture stays bounded.
pub(crate) const MAX_ELEVATION: i32 = 64;

/// The biggest footprint side accepted, in half tiles; a bigger object is dropped.
const MAX_FOOTPRINT: i32 = 64;

/// How an object takes its place on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Stands anywhere on a tile and holds no quadrant: animals, guests, staff.
    Unit,
    /// The path piece of a tile.
    Path,
    /// Stands on one edge of a tile, the one the facing points to: fences.
    Edge,
    /// A tank wall: an [`Kind::Edge`] that a tank exhibit may make active.
    Wall,
    /// Holds the quadrants of its footprint: buildings, scenery and the rest.
    Fixed,
    /// Holds no quadrant and is in no tile list: the pending piece of a path, attached each frame
    /// to the tile under it and painted after that tile's objects.
    Overlay,
    /// An ambient creature: on no tile, painted after the whole map in list order (ambients 5.2),
    /// with a shadow that goes with the tile under its shadow point (ambients 5.3).
    Flight,
    /// A water sparkle of a tank: in the tile's transient list, painted after its water (map
    /// drawing 9.7.10).
    Sparkle,
}

impl Kind {
    /// The way the objects of a class take their place.
    pub(crate) fn of_class(class: &str) -> Kind {
        match class {
            "paths" => Kind::Path,
            "fences" => Kind::Edge,
            "tankwall" => Kind::Wall,
            "ambient" => Kind::Flight,
            "animals" | "guests" | "keeper" | "maint" | "tour" | "helicopter" => Kind::Unit,
            _ => Kind::Fixed,
        }
    }
}

/// One object of the map.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Object {
    pub kind: Kind,
    /// World position; a tile is 64 units wide.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Direction 0 to 7.
    pub facing: u32,
    /// Footprint in half tiles along the object's own axes.
    pub footprint: (i32, i32),
    /// The type asks to be painted after the other objects of its tile.
    pub late: bool,
    /// `cHeight` of the type, in elevation steps.
    pub height: i32,
}

/// A tank exhibit as the painter needs it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TankSpec {
    /// The first tile of the exhibit and its second (gate) tile; (-1, -1) for none.
    pub tile: (i32, i32),
    pub gate: (i32, i32),
    /// The tank height and the water level, in elevation steps above the floor.
    pub height: i32,
    pub level: i32,
}

/// The glass pane of a wall seen from a side of the tank (the `lgrey`, `bgrey`, `rgrey` and
/// `fgrey` pictures).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glass {
    UpperLeft,
    UpperRight,
    LowerRight,
    LowerLeft,
}

/// One thing to paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    /// The ground of a tile (index `y * width + x`) with its cliffs and grid lines.
    Terrain(usize),
    /// The path piece, an object of kind [`Kind::Path`].
    Path(usize),
    /// An object of kind [`Kind::Edge`].
    Edge(usize),
    /// Any other object.
    Object(usize),
    /// The glass pane `k` steps above the floor of a tank wall, drawn blended.
    Pane { wall: usize, k: i32, glass: Glass },
    /// The water face `k` steps above the floor of a tank wall (`right`: on the lower-right
    /// wall, else on the lower-left one), drawn blended. `noise` selects the scum picture.
    Face {
        wall: usize,
        k: i32,
        right: bool,
        noise: u8,
    },
    /// The water surface of a tile at world height `z`, drawn blended. `noise` selects the scum.
    Surface { tile: usize, z: i32, noise: u8 },
    /// One of the four quarters of the water surface (0 top, 1 right, 2 left, 3 bottom).
    Quarter { tile: usize, z: i32, quad: u8 },
    /// The underwater picture of an object, if its type has one.
    Under(usize),
    /// The ground shadow of an object, if its type draws one.
    Shadow(usize),
    /// The platform of the gate on a tile.
    Platform(usize),
}

/// The map as the painter sees it.
pub(crate) struct Scene {
    pub width: usize,
    pub height: usize,
    pub rotation: u8,
    pub tiles: Vec<Tile>,
    pub objects: Vec<Object>,
    /// Per tile, the objects of the edge slots, by map direction `0, 2, 4, 6`.
    edge: Vec<[Option<usize>; 4]>,
    /// Per tile, the objects of the four quadrants, index `qy * 2 + qx` (the quadrant at local
    /// `(32 * qx, 32 * qy)`).
    quad: Vec<[Option<usize>; 4]>,
    /// Per tile, the units standing on it.
    list: Vec<Vec<usize>>,
    /// Per tile, the overlays (the pending path piece) attached to it.
    overlay: Vec<Vec<usize>>,
    /// The flights, in list order.
    flights: Vec<usize>,
    /// Per tile, the transient entities of a tank (the ripples), painted after its water.
    transient: Vec<Vec<usize>>,
    /// Per object, the number of quadrants it holds.
    covers: Vec<u32>,
    /// Per tile, its path piece.
    path: Vec<Option<usize>>,
    /// Per tile, the object that holds the quadrant at local (32.., 32..) and is bigger than
    /// one tile.
    big: Vec<Option<usize>>,
    /// Per object, whether it stands on the map at all.
    placed: Vec<bool>,
    /// Per building, the units it draws itself right after its own picture (map drawing 9.5.1).
    pad: HashMap<usize, Vec<usize>>,
    /// The tank exhibits, and per tile the exhibit it belongs to.
    pub tanks: Vec<TankSpec>,
    tank: Vec<Option<usize>>,
    /// The pseudo-random bytes of the water (9.7.7): per tile of the sorted array, and for the
    /// faces.
    scum: Vec<u8>,
    face_scum: Vec<u8>,
}

/// How a unit object is held off the map, which a rebuild of the index keeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Held {
    /// Painted, but on no tile list (see [`Scene::unlist_unit`]).
    Unlisted,
    /// On its tile's list, not painted (see [`Scene::hide_unit`]).
    Hidden,
    /// On no tile list and not painted (see [`Scene::lift_unit`]).
    Lifted,
}

/// The tile step of each map direction, `0..8`.
pub(crate) const STEP: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// The projection of a world point to view space at normal zoom (map drawing 2.2).
pub(crate) fn project(rotation: u8, wx: i32, wy: i32, wz: i32) -> (i32, i32) {
    let (a, b) = [(0, 1), (1, 0), (0, -1), (-1, 0)][usize::from(rotation % 4)];
    let (wx, wy, wz) = (i64::from(wx), i64::from(wy), i64::from(wz));
    let u = wx * a + wy * b;
    let v = wx * b - wy * a;
    // Rust's integer division truncates toward zero, like the original's.
    let px = (v + u) * 32 / 64;
    let py = (u - v) * 16 / 64 - wz;
    (
        px.clamp(-(1 << 30), 1 << 30) as i32,
        py.clamp(-(1 << 30), 1 << 30) as i32,
    )
}

/// The point a flight's shadow falls on: 300 units from it on both axes, straight down the screen
/// at every rotation (ambients 5.3).
pub(crate) fn shadow_point(rotation: u8, x: i32, y: i32) -> (i32, i32) {
    let (dx, dy) = [(-300, 300), (300, 300), (300, -300), (-300, -300)][usize::from(rotation % 4)];
    (x.saturating_add(dx), y.saturating_add(dy))
}

/// The `(row, pos)` pair of a tile for sorting (map drawing 9.2).
pub(crate) fn sort_key(rotation: u8, tx: i32, ty: i32) -> (i32, i32) {
    match rotation % 4 {
        0 => (-tx, ty),
        1 => (ty, tx),
        2 => (tx, -ty),
        _ => (-ty, -tx),
    }
}

/// The quadrants in screen order top, right, left, bottom, as indexes `qy * 2 + qx`.
fn quadrant_order(rotation: u8) -> [usize; 4] {
    let at = |q: usize| {
        let (qx, qy) = ((q & 1) as i32, (q >> 1) as i32);
        project(rotation, 32 * qx + 16, 32 * qy + 16, 0)
    };
    let mut quads = [0usize, 1, 2, 3];
    quads.sort_by_key(|&q| at(q).1);
    let [top, a, b, bottom] = quads;
    let (left, right) = if at(a).0 <= at(b).0 { (a, b) } else { (b, a) };
    [top, right, left, bottom]
}
/// The height in world units of the ground at the local point (`lx`, `ly`), 0 to 64, of a tile
/// (map drawing 3.4); each term is cut towards zero on its own.
pub(crate) fn ground_height(tile: &Tile, lx: i32, ly: i32) -> i32 {
    let (a, b) = (lx / 4, ly / 4);
    let rise = match tile.shape {
        0x50 => a,
        0x14 => b,
        0x05 => -a,
        0x41 => -b,
        0x40 if lx > ly => a - b,
        0x10 if lx + ly > 64 => a + b - 16,
        0x04 if ly > lx => b - a,
        0x01 if lx + ly < 64 => -a - b,
        0x01 => -16,
        0x54 if lx + ly < 64 => a + b,
        0x54 => 16,
        0x51 if ly > lx => a - b,
        0x45 if lx + ly > 64 => 16 - a - b,
        0x15 if lx > ly => b - a,
        0x44 if ly > lx => b - a,
        0x44 => a - b,
        0x11 if lx + ly < 64 => -a - b,
        0x11 => a + b - 32,
        0x64 => a + b,
        0x19 => b - a,
        0x46 => -a - b,
        0x91 => a - b,
        _ => 0,
    };
    16 * tile.elevation + rise
}

/// The map direction of a screen direction (0 upper left edge, 1 top corner, 2 upper right edge,
/// ...) at a rotation.
pub(crate) fn map_direction(rotation: u8, screen: usize) -> usize {
    (screen + 8 - 2 * usize::from(rotation % 4)) % 8
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    /// A tile of a tank exhibit, gathered and not yet painted (9.7.2).
    Collected,
    Terrain,
    Objects,
    Done,
}

/// The screen edges of a tile, as used by the paint order.
const UPPER_LEFT: usize = 0;
const UPPER_RIGHT: usize = 1;
const LOWER_RIGHT: usize = 2;
const LOWER_LEFT: usize = 3;

struct Painter<'a> {
    scene: &'a Scene,
    /// Map tile of each array position, and the position of each map tile.
    order: &'a [usize],
    at: &'a [usize],
    keys: &'a [(i32, i32)],
    state: Vec<State>,
    counter: Vec<u32>,
    painted: Vec<bool>,
    quads: [usize; 4],
    /// The flights whose shadow falls on a map tile, by tile.
    shadows: HashMap<usize, Vec<usize>>,
    out: Vec<Item>,
    calls: usize,
}

impl Painter<'_> {
    /// The array position of the neighbour across a screen edge. The original never counts
    /// array position 0 as the upper neighbour of a tile.
    fn nb(&self, i: usize, edge: usize) -> Option<usize> {
        let d = map_direction(self.scene.rotation, edge * 2);
        let n = self.scene.neighbour(self.order[i], d).map(|t| self.at[t])?;
        (edge >= LOWER_RIGHT || n > 0).then_some(n)
    }

    fn pos(&self, i: usize) -> i32 {
        self.keys[i].1
    }

    fn row(&self, i: usize) -> i32 {
        self.keys[i].0
    }

    /// The edge object of the tile at array position `i` on a screen edge.
    fn edge_object(&self, i: usize, edge: usize) -> Option<usize> {
        let d = map_direction(self.scene.rotation, edge * 2);
        self.scene.edge_object(self.order[i], d)
    }

    fn big(&self, i: usize) -> Option<usize> {
        self.scene.big[self.order[i]]
    }

    /// Whether tile `a` and its neighbour `b` across the screen edge `e` of `a` are one piece of
    /// ground, for a group whose big object is `big`.
    fn joined(&self, a: usize, b: usize, e: usize, big: Option<usize>) -> bool {
        let d = map_direction(self.scene.rotation, e * 2);
        self.scene.edge_state(self.order[a], d) < 2
            && self.edge_object(a, e).is_none()
            && self.edge_object(b, (e + 2) % 4).is_none()
            && self.big(b) == big
    }

    fn paint_edge(&mut self, i: usize, edge: usize) {
        if let Some(o) = self.edge_object(i, edge)
            && !self.painted[o]
        {
            self.painted[o] = true;
            self.out.push(Item::Edge(o));
        }
    }

    fn paint_terrain(&mut self, i: usize) {
        self.out.push(Item::Terrain(self.order[i]));
        self.state[i] = State::Terrain;
        if let Some(p) = self.scene.path[self.order[i]] {
            self.out.push(Item::Path(p));
        }
    }

    fn paint_map(&mut self) {
        for i in 0..self.order.len() {
            if self.state[i] == State::New {
                self.paint_group(i, i32::MAX);
            }
        }
        // Not reached with a consistent order; keeps every tile painted whatever the data.
        for i in 0..self.order.len() {
            if self.state[i] != State::Done {
                if self.state[i] == State::New {
                    self.paint_terrain(i);
                }
                self.paint_tile_objects(i);
                self.state[i] = State::Done;
            }
        }
    }

    /// Finds the back end of the group that holds `i` and paints the group from there.
    fn paint_from_back(&mut self, mut i: usize) {
        let big = self.big(i);
        loop {
            if let Some(u) = self.nb(i, UPPER_RIGHT)
                && self.state[u] == State::New
                && self.joined(i, u, UPPER_RIGHT, big)
            {
                i = u;
                continue;
            }
            if let Some(p) = self.nb(i, UPPER_LEFT)
                && self.state[p] == State::New
                && self.joined(i, p, UPPER_LEFT, big)
            {
                i = p;
                continue;
            }
            break;
        }
        self.paint_group(i, i32::MAX);
    }

    fn paint_group(&mut self, start: usize, mut limit: i32) {
        // A guard against runaway recursion on data the order was not built for.
        self.calls += 1;
        if self.calls > 40 * self.order.len() + 1000 {
            return;
        }
        if self.scene.tank[self.order[start]].is_some() {
            return self.paint_tank(start, limit);
        }
        for edge in [UPPER_LEFT, UPPER_RIGHT] {
            if let Some(n) = self.nb(start, edge)
                && self.state[n] != State::Done
            {
                self.paint_from_back(n);
                if self.state[n] != State::Done {
                    return;
                }
            }
        }
        if self.state[start] != State::New {
            return;
        }
        let big = self.big(start);
        let mut runs: Vec<usize> = Vec::new();
        let mut i = start;
        'rows: loop {
            let first = i;
            self.paint_terrain(i);
            self.paint_edge(i, UPPER_LEFT);
            runs.push(i);
            let mut prev = i;
            let mut j = self.nb(i, LOWER_RIGHT);
            while let Some(jj) = j {
                if self.pos(jj) > limit
                    || self.state[jj] != State::New
                    || !self.joined(jj, prev, UPPER_LEFT, big)
                    || self.big(jj) != big
                {
                    break;
                }
                if let Some(u) = self.nb(jj, UPPER_RIGHT)
                    && self.state[u] == State::New
                {
                    break;
                }
                self.paint_terrain(jj);
                runs.push(jj);
                prev = jj;
                j = self.nb(jj, LOWER_RIGHT);
            }
            limit = limit.min(self.pos(prev));

            // Try to go on with the next row.
            let Some(mut n) = self.nb(first, LOWER_LEFT) else {
                break;
            };
            if !self.joined(first, n, LOWER_LEFT, big) {
                break;
            }
            loop {
                if self.state[n] != State::New {
                    break 'rows;
                }
                if let Some(u) = self.nb(n, UPPER_RIGHT)
                    && self.state[u] == State::New
                {
                    self.paint_from_back(u);
                    if self.state[u] == State::New {
                        break 'rows;
                    }
                }
                let Some(p) = self.nb(n, UPPER_LEFT) else {
                    break;
                };
                if self.state[p] == State::Done {
                    break;
                }
                if self.joined(n, p, UPPER_LEFT, big) {
                    n = p;
                    continue;
                }
                self.paint_from_back(p);
                if self.state[p] == State::New {
                    break 'rows;
                }
                break;
            }
            i = n;
        }

        // The objects, one row of the group at a time.
        let mut deferred = Vec::new();
        let mut from = 0;
        while from < runs.len() {
            let mut to = from + 1;
            while to < runs.len() && self.row(runs[to]) == self.row(runs[from]) {
                to += 1;
            }
            let seg = &runs[from..to];
            for &t in seg {
                self.paint_edge(t, UPPER_RIGHT);
            }
            for (k, &t) in seg.iter().enumerate() {
                self.paint_tile_objects(t);
                if k + 1 == seg.len() {
                    deferred.push(t);
                } else {
                    self.paint_edge(t, LOWER_RIGHT);
                }
            }
            for &t in seg {
                self.paint_edge(t, LOWER_LEFT);
                self.state[t] = State::Done;
            }
            from = to;
        }
        for t in deferred {
            self.paint_edge(t, LOWER_RIGHT);
        }
    }

    /// The objects of one tile (map drawing 9.5).
    fn paint_tile_objects(&mut self, i: usize) {
        if matches!(self.state[i], State::Objects | State::Done) {
            return;
        }
        self.state[i] = State::Objects;
        self.paint_edge(i, UPPER_LEFT);
        self.paint_edge(i, UPPER_RIGHT);
        let list = self.tile_entries(i, &|_| true);
        for e in list {
            self.out.push(Item::Object(e.4));
            let users = self.scene.pad.get(&e.4).into_iter().flatten();
            self.out.extend(users.map(|&u| Item::Object(u)));
        }
        self.overlays(i, true, true);
    }
}

/// The tiles of a map sorted by paint key, which depend only on the map size and rotation: a
/// staging keeps them for the next one.
#[derive(Debug, PartialEq)]
pub(crate) struct TileOrder {
    width: usize,
    tiles: usize,
    rotation: u8,
    keys: Vec<(i32, i32)>,
    /// Map tile of each array position, and the position of each map tile.
    order: Vec<usize>,
    at: Vec<usize>,
}

impl TileOrder {
    pub(crate) fn new(scene: &Scene) -> TileOrder {
        let (width, n, rotation) = (scene.width, scene.tiles.len(), scene.rotation);
        let key = |t: usize| sort_key(rotation, (t % width) as i32, (t / width) as i32);
        // By key, then by tile: the order of a stable sort by key, without a key per comparison.
        let mut sorted: Vec<((i32, i32), usize)> = (0..n).map(|t| (key(t), t)).collect();
        sorted.sort_unstable();
        let (keys, order): (Vec<_>, Vec<_>) = sorted.into_iter().unzip();
        let mut at = vec![0; n];
        for (i, &t) in order.iter().enumerate() {
            at[t] = i;
        }
        TileOrder {
            width,
            tiles: n,
            rotation,
            keys,
            order,
            at,
        }
    }

    /// The order kept in `cache`, made again first when the map size or rotation changed.
    pub(crate) fn cached<'a>(cache: &'a mut Option<TileOrder>, scene: &Scene) -> &'a TileOrder {
        let same = |o: &TileOrder| {
            (o.width, o.tiles, o.rotation) == (scene.width, scene.tiles.len(), scene.rotation)
        };
        if !cache.as_ref().is_some_and(same) {
            *cache = None;
        }
        cache.get_or_insert_with(|| TileOrder::new(scene))
    }
}

/// [`paint_order_with`] and a tile order made for `scene`.
#[cfg(test)]
pub(crate) fn paint_order(scene: &Scene) -> Vec<Item> {
    paint_order_with(scene, &TileOrder::new(scene))
}

/// The things to paint, back to front, for a whole map whose tile order is `tiles`.
pub(crate) fn paint_order_with(scene: &Scene, tiles: &TileOrder) -> Vec<Item> {
    // The recursion follows chains of tiles; give it room on big maps.
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(s, || paint_order_here(scene, tiles))
            .and_then(|handle| handle.join().map_err(|_| std::io::Error::other("panic")))
            .unwrap_or_else(|_| paint_order_here(scene, tiles))
    })
}

fn paint_order_here(scene: &Scene, tiles: &TileOrder) -> Vec<Item> {
    let n = scene.tiles.len();
    let rotation = scene.rotation;
    let mut painter = Painter {
        scene,
        order: &tiles.order,
        at: &tiles.at,
        keys: &tiles.keys,
        state: vec![State::New; n],
        counter: vec![0; scene.objects.len()],
        painted: vec![false; scene.objects.len()],
        quads: quadrant_order(rotation),
        shadows: HashMap::new(),
        out: Vec::with_capacity(n + scene.objects.len()),
        calls: 0,
    };
    // The shadows are attached newest first, each at the head, so a tile lists the oldest first.
    for &o in scene.flights.iter().rev() {
        let (x, y) = shadow_point(rotation, scene.objects[o].x, scene.objects[o].y);
        if let Some(t) = scene.tile_at(x, y) {
            painter.shadows.entry(t).or_default().push(o);
        }
    }
    painter.paint_map();
    // The flights come after the whole map, the oldest first.
    painter
        .out
        .extend(scene.flights.iter().map(|&o| Item::Object(o)));
    painter.out
}
