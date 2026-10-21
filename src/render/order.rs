//! The order in which the original paints the tiles of a map and the objects on them.
//!
//! A [`Scene`] is the map reduced to what decides the order: the tiles, and for every object
//! where it stands (a tile list, a quadrant of a tile, a tile edge, or the path of a tile).
//! [`paint_order`] returns the sequence of things to paint, back to front. The whole map is
//! taken as visible, so the visibility tests of the original are always true. The tiles of a tank
//! exhibit are painted in layers (map drawing 9.7) instead of by the walk of the other groups;
//! the extra items they need are [`Item::Pane`], [`Item::Face`], [`Item::Surface`],
//! [`Item::Quarter`], [`Item::Under`], [`Item::Shadow`] and [`Item::Platform`].

use crate::zoo::Tile;

mod tank;

/// The tallest elevation a tile may have; the shipped maps stay below 13. Larger values (a
/// damaged file) are cut so that the picture stays bounded.
pub const MAX_ELEVATION: i32 = 64;

/// The biggest footprint side accepted, in half tiles; a bigger object is dropped.
const MAX_FOOTPRINT: i32 = 64;

/// How an object takes its place on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
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
    /// Holds no quadrant and is in no tile list: ambient animals, attached each frame to the
    /// tile under them and painted after that tile's objects.
    Overlay,
}

impl Kind {
    /// The way the objects of a class take their place.
    pub fn of_class(class: &str) -> Kind {
        match class {
            "paths" => Kind::Path,
            "fences" => Kind::Edge,
            "tankwall" => Kind::Wall,
            "ambient" => Kind::Overlay,
            "animals" | "guests" | "staff" | "keeper" | "maint" | "tour" | "helicopter" => {
                Kind::Unit
            }
            _ => Kind::Fixed,
        }
    }
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
    /// The type asks to be painted after the other objects of its tile.
    pub late: bool,
    /// `cHeight` of the type, in elevation steps.
    pub height: i32,
}

/// A tank exhibit as the painter needs it.
#[derive(Debug, Clone, Copy)]
pub struct TankSpec {
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
pub enum Glass {
    UpperLeft,
    UpperRight,
    LowerRight,
    LowerLeft,
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
    /// Per tile, the units standing on it.
    list: Vec<Vec<usize>>,
    /// Per tile, the overlays (ambient animals) attached to it.
    overlay: Vec<Vec<usize>>,
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
    /// The tank exhibits, and per tile the exhibit it belongs to.
    pub tanks: Vec<TankSpec>,
    tank: Vec<Option<usize>>,
    /// The pseudo-random bytes of the water (9.7.7): per tile of the sorted array, and for the
    /// faces.
    scum: Vec<u8>,
    face_scum: Vec<u8>,
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
            overlay: vec![Vec::new(); n],
            transient: vec![Vec::new(); n],
            covers: vec![0; objects.len()],
            path: vec![None; n],
            big: vec![None; n],
            placed: vec![false; objects.len()],
            tanks: Vec::new(),
            tank: vec![None; n],
            scum: Vec::new(),
            face_scum: Vec::new(),
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

    pub fn index(&self, tx: i32, ty: i32) -> Option<usize> {
        (tx >= 0 && ty >= 0 && (tx as usize) < self.width && (ty as usize) < self.height)
            .then(|| ty as usize * self.width + tx as usize)
    }

    /// Whether the object stands on the map (its position is over a tile).
    pub fn is_placed(&self, o: usize) -> bool {
        self.placed[o]
    }

    /// Adds an object to the map as the original's loader does. One that finds no place is
    /// dropped: it is not placed and never painted.
    fn place(&mut self, o: usize) {
        let obj = self.objects[o];
        let Some(t) = self.tile_at(obj.x, obj.y) else {
            return;
        };
        self.placed[o] = match obj.kind {
            Kind::Path if self.path[t].is_none() => {
                self.path[t] = Some(o);
                true
            }
            Kind::Edge | Kind::Wall
                if obj.facing.is_multiple_of(2)
                    && self.edge[t][obj.facing as usize / 2].is_none() =>
            {
                self.edge[t][obj.facing as usize / 2] = Some(o);
                true
            }
            Kind::Fixed => self.place_fixed(o),
            Kind::Unit => {
                self.list[t].push(o);
                true
            }
            Kind::Overlay => {
                self.overlay[t].push(o);
                true
            }
            Kind::Path | Kind::Edge | Kind::Wall => false,
        };
    }

    /// The tiles under an object bigger than one tile in x or in y (a footprint above 2 half
    /// tiles after the turn), in index order; none for another object (terrain editing 5.5). They
    /// are the tiles that contain the points `(x - 16 fx + 64 a, y - 16 fy + 64 b)` for `0 <= a <
    /// ceil(fx / 2)` and `0 <= b < ceil(fy / 2)`, a point's tile being its coordinates divided by
    /// 64 and cut towards zero; points off the map are left out. Not the quadrants of the object.
    pub fn large_tiles(&self, o: usize) -> Vec<usize> {
        let obj = &self.objects[o];
        let (fx, fy) = match obj.facing.is_multiple_of(4) {
            true => obj.footprint,
            false => (obj.footprint.1, obj.footprint.0),
        };
        if fx <= 2 && fy <= 2 {
            return Vec::new();
        }
        let at = |v: i32, f: i32, k: i32| {
            i32::try_from((i64::from(v) - 16 * i64::from(f) + 64 * i64::from(k)) / 64).ok()
        };
        let mut tiles: Vec<usize> = (0..(fy + 1) / 2)
            .flat_map(|b| (0..(fx + 1) / 2).map(move |a| (a, b)))
            .filter_map(|(a, b)| self.index(at(obj.x, fx, a)?, at(obj.y, fy, b)?))
            .collect();
        tiles.sort_unstable();
        tiles.dedup();
        tiles
    }

    /// The units standing on tile `t`.
    pub fn units_on(&self, t: usize) -> &[usize] {
        &self.list[t]
    }

    /// Puts the unit object `o` at a new point and facing; its tile list follows.
    pub fn move_unit(&mut self, o: usize, (x, y, z): (i32, i32, i32), facing: u32) {
        let (from, to) = (
            self.tile_at(self.objects[o].x, self.objects[o].y),
            self.tile_at(x, y),
        );
        let obj = &mut self.objects[o];
        (obj.x, obj.y, obj.z, obj.facing) = (x, y, z, facing);
        if obj.kind == Kind::Unit && (from != to || !self.placed[o]) {
            if let Some(f) = from {
                self.list[f].retain(|&u| u != o);
            }
            self.placed[o] = to.is_some();
            if let Some(t) = to {
                self.list[t].push(o);
            }
        }
    }

    /// Takes unit object `o` off the map: it is on no tile list and is not painted, until
    /// [`Scene::move_unit`] puts it down.
    pub fn lift_unit(&mut self, o: usize) {
        if let Some(t) = self.tile_at(self.objects[o].x, self.objects[o].y) {
            self.list[t].retain(|&u| u != o);
        }
        self.placed[o] = false;
    }

    /// Takes the quadrants of the footprint, all or none: it wants the `fx * fy` quadrants
    /// that contain the points `(x - 16 (fx - 1) + 32 i, y - 16 (fy - 1) + 32 j)`. Returns
    /// whether it got them.
    fn place_fixed(&mut self, o: usize) -> bool {
        let obj = self.objects[o];
        let (fx, fy) = obj.footprint;
        // The footprint is given along the object's own axes; it turns with the facing.
        let (fx, fy) = if !obj.facing.is_multiple_of(4) {
            (fy, fx)
        } else {
            (fx, fy)
        };
        if !(1..=MAX_FOOTPRINT).contains(&fx) || !(1..=MAX_FOOTPRINT).contains(&fy) {
            return false;
        }
        let mut wanted = Vec::with_capacity((fx * fy) as usize);
        for j in 0..fy {
            for i in 0..fx {
                let px = i64::from(obj.x) - 16 * i64::from(fx - 1) + 32 * i64::from(i);
                let py = i64::from(obj.y) - 16 * i64::from(fy - 1) + 32 * i64::from(j);
                let (cx, cy) = (px.div_euclid(32), py.div_euclid(32));
                let tile = i32::try_from(cx >> 1)
                    .ok()
                    .zip(i32::try_from(cy >> 1).ok())
                    .and_then(|(tx, ty)| self.index(tx, ty));
                match tile {
                    Some(tile) if self.quad[tile][((cy & 1) * 2 + (cx & 1)) as usize].is_none() => {
                        wanted.push((tile, ((cy & 1) * 2 + (cx & 1)) as usize))
                    }
                    _ => return false,
                }
            }
        }
        let large = fx > 2 || fy > 2;
        for &(tile, q) in &wanted {
            self.quad[tile][q] = Some(o);
            if q == 3 && large {
                self.big[tile] = Some(o);
            }
        }
        self.covers[o] = (fx * fy) as u32;
        true
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

    /// The number of quadrants the object holds.
    pub fn quadrants(&self, o: usize) -> u32 {
        self.covers[o]
    }

    /// The path piece of a tile.
    pub fn path_of(&self, t: usize) -> Option<usize> {
        self.path[t]
    }

    /// The object that holds quadrant `q` (`qy * 2 + qx`) of tile `t`.
    pub fn quadrant_object(&self, t: usize, q: usize) -> Option<usize> {
        self.quad[t][q]
    }

    /// Adds a transient entity on tile `t` of a tank (map drawing 9.7.4); returns its index.
    pub fn add_transient(&mut self, t: usize, obj: Object) -> usize {
        self.objects.push(obj);
        self.covers.push(0);
        self.placed.push(true);
        self.transient[t].push(self.objects.len() - 1);
        self.objects.len() - 1
    }

    /// Declares the tank exhibits. `fences[o]` tells whether object `o` is a habitat fence (a
    /// fence or tank wall whose type has a `[HabitatFence]` or `[TankFence]` section). The area
    /// of a tank is what the original fills from the first tile of its record: every tile
    /// reachable through edges where neither slot holds a habitat fence, whatever the heights. A
    /// tile that is already in another exhibit is taken over.
    pub fn set_tanks(&mut self, tanks: &[TankSpec], fences: &[bool]) {
        self.tanks = tanks.to_vec();
        self.tank = vec![None; self.tiles.len()];
        let fence = |t: usize, d: usize| {
            self.edge[t][d / 2].is_some_and(|o| fences.get(o).copied().unwrap_or(false))
        };
        for (k, spec) in tanks.iter().enumerate() {
            let Some(first) = self.index(spec.tile.0, spec.tile.1) else {
                continue;
            };
            let mut seen = vec![false; self.tiles.len()];
            seen[first] = true;
            let mut todo = vec![first];
            while let Some(t) = todo.pop() {
                self.tank[t] = Some(k);
                for d in [0, 2, 4, 6] {
                    let Some(n) = self.neighbour(t, d) else {
                        continue;
                    };
                    if !seen[n] && !fence(t, d) && !fence(n, (d + 4) % 8) {
                        seen[n] = true;
                        todo.push(n);
                    }
                }
            }
        }
        // The pseudo-random bytes of the water: one per tile, then one per face level.
        let mut x = 50u32;
        let mut next = move || {
            x = x.wrapping_mul(214013).wrapping_add(2531011);
            ((x >> 16) & 0x7fff) as u8 % 255
        };
        self.scum = (0..self.tiles.len()).map(|_| next()).collect();
        self.face_scum = (0..255).map(|_| next()).collect();
    }

    /// The tank exhibit (an index in [`Scene::tanks`]) that tile `t` belongs to.
    pub fn tank_of(&self, t: usize) -> Option<usize> {
        self.tank[t]
    }

    /// The tank an active wall belongs to: a tank wall of a tank taller than 0, standing on a
    /// tile of the tank or on the edge of one.
    pub fn wall_tank(&self, o: usize) -> Option<usize> {
        let obj = &self.objects[o];
        if obj.kind != Kind::Wall || !self.placed[o] {
            return None;
        }
        let t = self.tile_at(obj.x, obj.y)?;
        let across = self.neighbour(t, obj.facing as usize % 8);
        self.tank_of(t)
            .or_else(|| across.and_then(|n| self.tank_of(n)))
            .filter(|&k| self.tanks[k].height > 0)
    }

    /// The height of the water surface over tile `t`: the ground at its centre plus the level.
    pub fn surface(&self, t: usize) -> Option<i32> {
        let tank = &self.tanks[self.tank_of(t)?];
        Some(ground_height(&self.tiles[t], 32, 32) + 16 * tank.level)
    }

    /// The tile across the edge or corner in map direction `d`.
    pub fn neighbour(&self, t: usize, d: usize) -> Option<usize> {
        let (dx, dy) = STEP[d % 8];
        self.index((t % self.width) as i32 + dx, (t / self.width) as i32 + dy)
    }
}

/// The height in world units of the ground at the local point (`lx`, `ly`), 0 to 64, of a tile
/// (map drawing 3.4); each term is cut towards zero on its own.
pub fn ground_height(tile: &Tile, lx: i32, ly: i32) -> i32 {
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
pub fn map_direction(rotation: u8, screen: usize) -> usize {
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
        self.out.extend(list.into_iter().map(|e| Item::Object(e.4)));
        self.overlays(i);
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
    let key = |t: usize| sort_key(rotation, (t % scene.width) as i32, (t / scene.width) as i32);
    // By key, then by tile: the order of a stable sort by key, without a key per comparison.
    let mut sorted: Vec<((i32, i32), usize)> = (0..n).map(|t| (key(t), t)).collect();
    sorted.sort_unstable();
    let (keys, order): (Vec<_>, Vec<_>) = sorted.into_iter().unzip();
    let mut at = vec![0; n];
    for (i, &t) in order.iter().enumerate() {
        at[t] = i;
    }
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
