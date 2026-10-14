//! The order in which the original paints the tiles of a map and the objects on them.
//!
//! A [`Scene`] is the map reduced to what decides the order: the tiles, and for every object
//! where it stands (a tile list, a quadrant of a tile, a tile edge, or the path of a tile).
//! [`paint_order`] returns the sequence of things to paint, back to front. The whole map is
//! taken as visible, so the visibility tests of the original are always true.

use crate::zoo::Tile;

/// The tallest elevation a tile may have; the shipped maps stay below 13. Larger values (a
/// damaged file) are cut so that the picture stays bounded.
pub const MAX_ELEVATION: i32 = 64;

/// The biggest footprint side taken into account, in half tiles.
const MAX_FOOTPRINT: i32 = 64;

/// How an object takes its place on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Stands anywhere on a tile and holds no quadrant: animals, guests, staff.
    Unit,
    /// The path piece of a tile.
    Path,
    /// Stands on one edge of a tile, the one the facing points to: fences, tank walls.
    Edge,
    /// Holds the quadrants of its footprint: buildings, scenery and the rest.
    Fixed,
}

/// One object of the map.
#[derive(Debug, Clone, Copy)]
pub struct Object {
    pub kind: Kind,
    /// World position; a tile is 64 units wide.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Direction 0 to 7.
    pub facing: u32,
    /// Footprint in half tiles along the object's own axes.
    pub footprint: (i32, i32),
}

/// One thing to paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// The ground of a tile (index `y * width + x`) with its cliffs and grid lines.
    Terrain(usize),
    /// The path piece, an object of kind [`Kind::Path`].
    Path(usize),
    /// An object of kind [`Kind::Edge`].
    Edge(usize),
    /// Any other object.
    Object(usize),
}

/// The map as the painter sees it.
pub struct Scene {
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
    /// Per tile, the objects that hold no quadrant.
    list: Vec<Vec<usize>>,
    /// Per object, the number of quadrants it holds.
    covers: Vec<u32>,
    /// Per tile, its path piece.
    path: Vec<Option<usize>>,
    /// Per tile, the object that holds the quadrant at local (32.., 32..) and is bigger than
    /// one tile.
    big: Vec<Option<usize>>,
    /// Per object, whether it stands on the map at all.
    placed: Vec<bool>,
}

/// The tile step of each map direction, `0..8`.
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

/// The projection of a world point to view space at normal zoom (map drawing 2.2).
pub fn project(rotation: u8, wx: i32, wy: i32, wz: i32) -> (i32, i32) {
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

/// The `(row, pos)` pair of a tile for sorting (map drawing 9.2).
pub fn sort_key(rotation: u8, tx: i32, ty: i32) -> (i32, i32) {
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

impl Scene {
    /// Places the objects on the tiles. `tiles` has `width * height` entries; the elevations
    /// are cut to [`MAX_ELEVATION`].
    pub fn new(
        width: usize,
        height: usize,
        rotation: u8,
        tiles: &[Tile],
        objects: Vec<Object>,
    ) -> Scene {
        assert_eq!(tiles.len(), width * height);
        let n = tiles.len();
        let mut scene = Scene {
            width,
            height,
            rotation: rotation % 4,
            tiles: tiles
                .iter()
                .map(|t| Tile {
                    elevation: t.elevation.clamp(-MAX_ELEVATION, MAX_ELEVATION),
                    ..*t
                })
                .collect(),
            edge: vec![[None; 4]; n],
            quad: vec![[None; 4]; n],
            list: vec![Vec::new(); n],
            covers: vec![0; objects.len()],
            path: vec![None; n],
            big: vec![None; n],
            placed: vec![false; objects.len()],
            objects,
        };
        for o in 0..scene.objects.len() {
            scene.place(o);
        }
        scene
    }

    /// The tile index under a world point, if it is on the map.
    pub fn tile_at(&self, x: i32, y: i32) -> Option<usize> {
        let (tx, ty) = (x.div_euclid(64), y.div_euclid(64));
        self.index(tx, ty)
    }

    fn index(&self, tx: i32, ty: i32) -> Option<usize> {
        (tx >= 0 && ty >= 0 && (tx as usize) < self.width && (ty as usize) < self.height)
            .then(|| ty as usize * self.width + tx as usize)
    }

    /// Whether the object stands on the map (its position is over a tile).
    pub fn is_placed(&self, o: usize) -> bool {
        self.placed[o]
    }

    fn place(&mut self, o: usize) {
        let obj = self.objects[o];
        let Some(t) = self.tile_at(obj.x, obj.y) else {
            return;
        };
        self.placed[o] = true;
        match obj.kind {
            Kind::Path if self.path[t].is_none() => self.path[t] = Some(o),
            Kind::Edge
                if obj.facing.is_multiple_of(2)
                    && self.edge[t][obj.facing as usize / 2].is_none() =>
            {
                self.edge[t][obj.facing as usize / 2] = Some(o)
            }
            Kind::Fixed => self.place_fixed(o, t),
            _ => self.list[t].push(o),
        }
    }

    /// Enters an object in the quadrants of its footprint that are free.
    fn place_fixed(&mut self, o: usize, t: usize) {
        let obj = self.objects[o];
        let (fx, fy) = obj.footprint;
        let (fx, fy) = (fx.clamp(1, MAX_FOOTPRINT), fy.clamp(1, MAX_FOOTPRINT));
        // The footprint is given along the object's own axes; a quarter turn swaps them.
        let (fx, fy) = if obj.facing % 4 == 2 {
            (fy, fx)
        } else {
            (fx, fy)
        };
        let cells = |c: i32, f: i32| {
            let from = (i64::from(c) - 16 * i64::from(f)).div_euclid(32);
            let to = (i64::from(c) + 16 * i64::from(f) - 1).div_euclid(32);
            from..=to
        };
        let large = fx > 2 || fy > 2;
        for cy in cells(obj.y, fy) {
            for cx in cells(obj.x, fx) {
                let Some(tile) = self.index((cx >> 1) as i32, (cy >> 1) as i32) else {
                    continue;
                };
                let q = ((cy & 1) * 2 + (cx & 1)) as usize;
                if self.quad[tile][q].is_none() {
                    self.quad[tile][q] = Some(o);
                    self.covers[o] += 1;
                    if q == 3 && large {
                        self.big[tile] = Some(o);
                    }
                }
            }
        }
        if self.covers[o] == 0 {
            self.list[t].push(o);
        }
    }

    /// The state of the edge of tile `t` in map direction `d` (even): 0 and 1 mean the ground
    /// is joined, 2 and 3 a step. The file stores the four states in the order of the
    /// directions 6, 4, 2, 0 from the low bits up.
    pub fn edge_state(&self, t: usize, d: usize) -> u8 {
        (self.tiles[t].edges >> (2 * (3 - d / 2))) & 3
    }

    /// The object on the edge of tile `t` in map direction `d`.
    pub fn edge_object(&self, t: usize, d: usize) -> Option<usize> {
        self.edge[t][d / 2]
    }

    /// The path piece of a tile.
    pub fn path_of(&self, t: usize) -> Option<usize> {
        self.path[t]
    }

    /// The tile across the edge or corner in map direction `d`.
    pub fn neighbour(&self, t: usize, d: usize) -> Option<usize> {
        let (dx, dy) = STEP[d % 8];
        self.index((t % self.width) as i32 + dx, (t / self.width) as i32 + dy)
    }
}

/// The map direction of a screen direction (0 upper left edge, 1 top corner, 2 upper right edge,
/// ...) at a rotation.
pub fn map_direction(rotation: u8, screen: usize) -> usize {
    (screen + 8 - 2 * usize::from(rotation % 4)) % 8
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    New,
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
    order: Vec<usize>,
    at: Vec<usize>,
    keys: Vec<(i32, i32)>,
    state: Vec<State>,
    counter: Vec<u32>,
    painted: Vec<bool>,
    quads: [usize; 4],
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
        self.state[i] = State::Objects;
        self.paint_edge(i, UPPER_LEFT);
        self.paint_edge(i, UPPER_RIGHT);
        let t = self.order[i];
        let scene = self.scene;
        let rotation = scene.rotation;
        // (forced z, view y, view x, object)
        let mut list: Vec<(i32, i32, i32, usize)> = Vec::new();
        let push = |list: &mut Vec<_>, forced: i32, o: usize| {
            let obj = &scene.objects[o];
            let (px, py) = project(rotation, obj.x, obj.y, obj.z);
            list.push((forced, py, px, o));
        };
        for &o in &scene.list[t] {
            push(&mut list, 0, o);
        }
        let quads = self.quads.map(|q| scene.quad[t][q]);
        for (k, quad) in quads.iter().enumerate() {
            let Some(o) = *quad else { continue };
            self.counter[o] += 1;
            if self.counter[o] != scene.covers[o] {
                continue;
            }
            let forced = match k {
                0 => -1,
                1 | 2 if quads[0] == Some(o) => -1,
                3 if quads[1] == Some(o) || quads[2] == Some(o) => 1,
                _ => 0,
            };
            push(&mut list, forced, o);
        }
        list.sort();
        self.out
            .extend(list.into_iter().map(|(.., o)| Item::Object(o)));
    }
}

/// The things to paint, back to front, for a whole map.
pub fn paint_order(scene: &Scene) -> Vec<Item> {
    // The recursion follows chains of tiles; give it room on big maps.
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(s, || paint_order_here(scene))
            .and_then(|handle| handle.join().map_err(|_| std::io::Error::other("panic")))
            .unwrap_or_else(|_| paint_order_here(scene))
    })
}

fn paint_order_here(scene: &Scene) -> Vec<Item> {
    let n = scene.tiles.len();
    let rotation = scene.rotation;
    let mut order: Vec<usize> = (0..n).collect();
    let key = |t: usize| sort_key(rotation, (t % scene.width) as i32, (t / scene.width) as i32);
    order.sort_by_key(|&t| key(t));
    let mut at = vec![0; n];
    for (i, &t) in order.iter().enumerate() {
        at[t] = i;
    }
    let keys = order.iter().map(|&t| key(t)).collect();
    let mut painter = Painter {
        scene,
        order,
        at,
        keys,
        state: vec![State::New; n],
        counter: vec![0; scene.objects.len()],
        painted: vec![false; scene.objects.len()],
        quads: quadrant_order(rotation),
        out: Vec::with_capacity(n + scene.objects.len()),
        calls: 0,
    };
    painter.paint_map();
    painter.out
}
