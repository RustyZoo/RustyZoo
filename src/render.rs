//! Drawing a whole map the way the original draws its map view, at normal zoom.
//!
//! The ground of a tile is one picture chosen by a key (terrain type, shape, variation and the
//! types of the eight neighbours) and read from the caches the game ships (`ztats`, `ztatb`).
//! A tile with no cached picture is left undrawn and counted. Cliffs are drawn by the tile in
//! front of them from `ztst/fringe`. Objects are drawn with the base animation of their type.
//! The order comes from [`order::paint_order`]; this module turns each item of it into pictures
//! placed in view space, then draws them all on a canvas that holds them.

pub mod order;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::Framebuffer;
use crate::anim::{AnimSet, Animation, View};
use crate::config::{Config, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::sprite::{Frame, Palette};
use crate::vfs::Vfs;
use crate::zoo::Zoo;
use order::{Item, Kind, Object, Scene, map_direction, paint_order, project};

/// The most pixels of the picture; a 150 by 150 map with its relief needs about 55 million.
pub const MAX_PIXELS: u64 = 100_000_000;

/// Why a map could not be drawn.
#[derive(Debug)]
pub struct RenderError(String);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for RenderError {}

/// What a picture of a map contains.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub width: usize,
    pub height: usize,
    /// Tiles whose ground picture was drawn, and tiles whose picture is not in the cache.
    pub tiles_drawn: usize,
    pub ground_missing: usize,
    /// Cliff pieces drawn, and wanted but not found.
    pub cliffs_drawn: usize,
    pub cliffs_missing: usize,
    /// Objects drawn, objects whose type or animation is missing, and objects that are not on
    /// the map or whose record has no position.
    pub objects_drawn: usize,
    pub objects_missing: usize,
    pub objects_skipped: usize,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}x{} tiles: {} drawn, {} ground pictures missing; cliff pieces: {} drawn, {} missing; \
             objects: {} drawn, {} with a missing type or animation, {} not placed",
            self.width,
            self.height,
            self.tiles_drawn,
            self.ground_missing,
            self.cliffs_drawn,
            self.cliffs_missing,
            self.objects_drawn,
            self.objects_missing,
            self.objects_skipped,
        )
    }
}

impl Summary {
    /// Adds the counts of another map; the size becomes 0.
    pub fn add(&mut self, other: &Summary) {
        self.width = 0;
        self.height = 0;
        self.tiles_drawn += other.tiles_drawn;
        self.ground_missing += other.ground_missing;
        self.cliffs_drawn += other.cliffs_drawn;
        self.cliffs_missing += other.cliffs_missing;
        self.objects_drawn += other.objects_drawn;
        self.objects_missing += other.objects_missing;
        self.objects_skipped += other.objects_skipped;
    }
}

/// A picture of the cache: a frame and its palette.
type Flat = Rc<(Frame, Palette)>;

enum Pic {
    Flat(Flat),
    Anim(Rc<Animation>, View),
}

/// A picture placed in view space: `(x, y)` is its anchor.
struct Placed {
    x: i32,
    y: i32,
    pic: Pic,
}

impl Placed {
    /// The box `[left, top, right, bottom)` of the pixels the picture can cover.
    fn bounds(&self) -> Option<[i64; 4]> {
        let frames: Vec<(&Frame, bool)> = match &self.pic {
            Pic::Flat(flat) => vec![(&flat.0, false)],
            Pic::Anim(anim, view) => {
                let sprite = &anim.sprites.get(view.file)?.0;
                sprite
                    .extra
                    .iter()
                    .chain(sprite.frames.first())
                    .map(|frame| (frame, view.mirror))
                    .collect()
            }
        };
        frames
            .into_iter()
            .map(|(frame, mirror)| {
                let (w, h) = (i64::from(frame.width), i64::from(frame.height));
                let (xo, yo) = (i64::from(frame.x_offset), i64::from(frame.y_offset));
                let left = match mirror {
                    false => i64::from(self.x) - xo,
                    true => i64::from(self.x) + xo - (w - 1),
                };
                let top = i64::from(self.y) - yo;
                [left, top, left + w, top + h]
            })
            .reduce(|a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })
    }

    fn draw(&self, target: &mut Framebuffer, dx: i32, dy: i32) {
        let (x, y) = (self.x - dx, self.y - dy);
        match &self.pic {
            Pic::Flat(flat) => {
                let (frame, palette) = (&flat.0, &flat.1);
                frame.draw(
                    palette,
                    target,
                    x - i32::from(frame.x_offset),
                    y - i32::from(frame.y_offset),
                );
            }
            Pic::Anim(anim, view) => anim.draw(*view, 0, target, x, y),
        }
    }
}

/// The local position of a corner (numbered as in the specifications), in tiles.
fn corner_xy(corner: usize) -> (i32, i32) {
    match corner {
        7 => (0, 0),
        1 => (1, 0),
        3 => (1, 1),
        _ => (0, 1),
    }
}

fn corner_at(x: i32, y: i32) -> usize {
    match (x, y) {
        (0, 0) => 7,
        (1, 0) => 1,
        (1, 1) => 3,
        _ => 5,
    }
}

/// The elevation of the corner of a tile, in steps.
fn corner_elevation(tile: &crate::zoo::Tile, corner: usize) -> i32 {
    let field = |c: usize| {
        let shift = match c {
            7 => 0,
            5 => 2,
            3 => 4,
            _ => 6,
        };
        i32::from(tile.shape >> shift & 3)
    };
    tile.elevation + field(corner) - field(7)
}

/// The image origin of a tile relative to the projection of its corner at the height of the
/// screen-left corner (map drawing 4.4).
const ORIGIN_OFFSET: [(i32, i32); 4] = [(0, -32), (-32, -16), (-64, -32), (-32, -48)];

/// The terrain types: whether each takes part in blending.
fn terrain_blend(vfs: &Vfs) -> Vec<bool> {
    let mut members: Vec<String> = vfs
        .list(None)
        .into_iter()
        .filter(|m| {
            let m = m.to_ascii_lowercase();
            m.starts_with("terrain/tilete") && m.ends_with(".cfg")
        })
        .collect();
    members.sort();
    let mut blend = Vec::new();
    for member in members {
        let Ok(bytes) = vfs.read(&member) else {
            continue;
        };
        let Ok(config) = Config::parse(&bytes) else {
            continue;
        };
        for (_, section) in config.sections() {
            let kind = section.get("type").and_then(to_int).unwrap_or(0);
            let on = section.get("blend").and_then(to_int).is_none_or(|b| b > 0);
            if let Ok(kind) = usize::try_from(kind)
                && kind < 256
            {
                if blend.len() <= kind {
                    blend.resize(kind + 1, true);
                }
                blend[kind] = on;
            }
        }
    }
    blend
}

/// The pre-converted copy of a bitmap named in a configuration file: `ztst/<name>` with the
/// extension taken off.
fn converted(image: &[u8]) -> String {
    let name = String::from_utf8_lossy(image);
    let name = name.trim().replace('\\', "/");
    let stem = name
        .rsplit_once('.')
        .map_or(name.as_str(), |(stem, _)| stem);
    format!("ztst/{stem}")
}

/// Draws maps. It keeps what it loaded between maps: the catalogue, the animations and the
/// pictures of the cache.
pub struct Renderer<'a> {
    vfs: &'a Vfs,
    catalogue: &'a Catalogue,
    blend: Vec<bool>,
    /// The cliff pieces 1 to 15, as `ztst` names.
    fringe: HashMap<u32, String>,
    /// For each shape (decimal), the north and east grid sprites.
    grid: HashMap<u32, (Option<String>, Option<String>)>,
    flats: HashMap<String, Option<Flat>>,
    anims: HashMap<String, Option<Rc<Animation>>>,
}

impl<'a> Renderer<'a> {
    pub fn new(vfs: &'a Vfs, catalogue: &'a Catalogue) -> Renderer<'a> {
        let read = |member: &str| {
            vfs.read(member)
                .ok()
                .and_then(|bytes| Config::parse(&bytes).ok())
                .unwrap_or_default()
        };
        let mut fringe = HashMap::new();
        for (name, section) in read("fringe.cfg").sections() {
            if let (Some(piece), Some(image)) = (
                to_int(name).and_then(|n| u32::try_from(n).ok()),
                section.get("image"),
            ) {
                fringe.insert(piece, converted(image));
            }
        }
        let mut grid = HashMap::new();
        for (name, section) in read("tile.cfg").sections() {
            if let Some(shape) = to_int(name).and_then(|n| u32::try_from(n).ok()) {
                let sprite = |key: &str| section.get(key).map(converted);
                grid.insert(shape, (sprite("northgrid"), sprite("eastgrid")));
            }
        }
        Renderer {
            vfs,
            catalogue,
            blend: terrain_blend(vfs),
            fringe,
            grid,
            flats: HashMap::new(),
            anims: HashMap::new(),
        }
    }

    /// The picture of a cache file `<stem>.lle` with its palette `<palette>`, if both exist.
    fn flat(&mut self, stem: &str, palette: &str) -> Option<Flat> {
        let key = format!("{stem}|{palette}");
        if let Some(found) = self.flats.get(&key) {
            return found.clone();
        }
        let load = || -> Option<Flat> {
            let frame = Frame::parse(&self.vfs.read(&format!("{stem}.lle")).ok()?).ok()?;
            let palette = Palette::parse(&self.vfs.read(&format!("{palette}.pal")).ok()?).ok()?;
            Some(Rc::new((frame, palette)))
        };
        let found = load();
        self.flats.insert(key, found.clone());
        found
    }

    /// A picture of `ztst`, which has the palette next to it.
    fn piece(&mut self, stem: &str) -> Option<Flat> {
        self.flat(stem, stem)
    }

    fn animation(&mut self, member: &str) -> Option<Rc<Animation>> {
        if let Some(found) = self.anims.get(member) {
            return found.clone();
        }
        let found = AnimSet::load(self.vfs, member)
            .ok()
            .and_then(|set| Animation::open(self.vfs, set).ok())
            .map(Rc::new);
        self.anims.insert(member.to_string(), found.clone());
        found
    }

    /// Draws the map seen from `rotation` (0 to 3), whole, on a black background.
    pub fn render(
        &mut self,
        zoo: &Zoo,
        rotation: u8,
    ) -> Result<(Framebuffer, Summary), RenderError> {
        // Pictures of the previous map: the blended ones are numerous and not shared.
        self.flats.clear();
        let rotation = rotation % 4;
        let (width, height) = (zoo.width as usize, zoo.height as usize);
        let mut summary = Summary {
            width,
            height,
            ..Summary::default()
        };
        if width == 0 || height == 0 {
            return Err(RenderError("the map has no tile".into()));
        }

        // The objects with a position, their types and their kinds.
        let mut objects = Vec::new();
        let mut types: Vec<Option<&EntityType>> = Vec::new();
        for entity in &zoo.entities {
            let Some(c) = &entity.common else {
                summary.objects_skipped += 1;
                continue;
            };
            let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
            let ty = self.catalogue.find(
                &entity.class,
                &bytes(&entity.type_name),
                &bytes(&entity.subtype),
            );
            let kind = match entity.class.as_str() {
                "paths" => Kind::Path,
                "fences" | "tankwall" => Kind::Edge,
                "animals" | "guests" | "staff" | "keeper" | "maint" | "tour" | "helicopter" => {
                    Kind::Unit
                }
                _ => Kind::Fixed,
            };
            let [fx, fy, _] = ty.map_or([1, 1, 1], |t| t.footprint);
            objects.push(Object {
                kind,
                x: c.x,
                y: c.y,
                z: c.z.clamp(-4096, 4096),
                facing: c.rotation % 8,
                footprint: (fx, fy),
            });
            types.push(ty);
        }
        let scene = Scene::new(width, height, rotation, &zoo.tiles, objects);

        let mut placed: Vec<Placed> = Vec::new();
        for item in paint_order(&scene) {
            match item {
                Item::Terrain(t) => self.terrain(&scene, t, &mut summary, &mut placed),
                Item::Path(o) | Item::Edge(o) | Item::Object(o) => {
                    self.object(&scene, o, types[o], &mut summary, &mut placed)
                }
            }
        }
        summary.objects_skipped += (0..types.len()).filter(|&o| !scene.is_placed(o)).count();

        // The canvas holds every picture and every tile.
        let mut bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
        let mut grow = |b: [i64; 4]| {
            bounds = [
                bounds[0].min(b[0]),
                bounds[1].min(b[1]),
                bounds[2].max(b[2]),
                bounds[3].max(b[3]),
            ]
        };
        for t in 0..scene.tiles.len() {
            let (ox, oy) = origin(&scene, t);
            let (x, y) = (i64::from(ox), i64::from(oy));
            grow([x, y + 16, x + 64, y + 49]);
        }
        placed.iter().filter_map(Placed::bounds).for_each(&mut grow);
        let (w, h) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
        if w <= 0 || h <= 0 || w as u64 * h as u64 > MAX_PIXELS {
            return Err(RenderError(format!(
                "the picture would be {w}x{h} pixels, more than {MAX_PIXELS}"
            )));
        }
        let mut canvas = Framebuffer::new(w as u32, h as u32);
        canvas
            .pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
        for p in &placed {
            p.draw(&mut canvas, bounds[0] as i32, bounds[1] as i32);
        }
        Ok((canvas, summary))
    }

    /// The type of the terrain of a tile, as the drawing code uses it.
    fn kind(&self, scene: &Scene, t: usize) -> u8 {
        let kind = scene.tiles[t].terrain;
        match usize::from(kind) < self.blend.len().max(1) {
            true => kind,
            false => 0,
        }
    }

    fn blends(&self, kind: u8) -> bool {
        self.blend.get(usize::from(kind)).copied().unwrap_or(true)
    }

    /// Whether the way from tile `t` across the edge in map direction `d` is open for blending.
    fn open(&self, scene: &Scene, t: usize, d: usize) -> Option<usize> {
        let n = scene.neighbour(t, d)?;
        (scene.edge_state(t, d) < 2
            && scene.edge_object(t, d).is_none()
            && scene.edge_object(n, (d + 4) % 8).is_none())
        .then_some(n)
    }

    /// The terrain type seen in map direction `d` from tile `t`, or the tile's own.
    fn seen(&self, scene: &Scene, t: usize, d: usize) -> u8 {
        let own = self.kind(scene, t);
        if !self.blends(own) {
            return own;
        }
        let neighbour = if d.is_multiple_of(2) {
            self.open(scene, t, d)
        } else {
            let (a, b) = ((d + 7) % 8, (d + 1) % 8);
            let target = scene.neighbour(t, d);
            // The way through the edge neighbour in direction `first`, then across `second`.
            let through = |first: usize, second: usize| {
                let e = self.open(scene, t, first)?;
                let n = scene.neighbour(e, second)?;
                (self.blends(self.kind(scene, e))
                    && scene.edge_state(e, second) < 2
                    && scene.edge_object(e, second).is_none()
                    && scene.edge_object(n, (second + 4) % 8).is_none())
                .then_some(n)
            };
            target.filter(|_| through(a, b).is_some() || through(b, a).is_some())
        };
        match neighbour {
            Some(n) if self.blends(self.kind(scene, n)) => self.kind(scene, n),
            _ => own,
        }
    }

    /// The ground picture of a tile, from the cache.
    fn ground(&mut self, scene: &Scene, t: usize) -> Option<Flat> {
        let r = scene.rotation;
        let tile = scene.tiles[t];
        let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let kind = self.kind(scene, t);
        let shape = tile.shape.rotate_right(2 * u32::from(r));
        let (a, b) = [(0, 1), (1, 0), (0, -1), (-1, 0)][usize::from(r)];
        let umod3 = |k: i32| (k as u32) % 3;
        let variation = umod3(tx * b - ty * a) * 3 + umod3(tx * a + ty * b);
        let n: Vec<u8> = (0..8)
            .map(|k| self.seen(scene, t, map_direction(r, k)))
            .collect();
        let blended = n.iter().any(|&k| k != kind);
        let stem = match blended {
            false => format!("ztats/{kind:02x}{shape:02x}{variation:02x}"),
            true => {
                let hex = |s: &[u8]| s.iter().map(|k| format!("{k:02x}")).collect::<String>();
                let dir = format!("ztatb/{kind:02x}/{}/{}", hex(&n[..4]), hex(&n[4..]));
                format!("{dir}/{shape:02x}{variation:02x}")
            }
        };
        let palette = match blended {
            false => format!("ztats/{kind:02x}"),
            true => stem.clone(),
        };
        self.flat(&stem, &palette)
    }

    /// Places the pictures of one tile: cliffs, ground and forced grid lines.
    fn terrain(&mut self, scene: &Scene, t: usize, summary: &mut Summary, out: &mut Vec<Placed>) {
        let r = scene.rotation;
        let (ox, oy) = origin(scene, t);
        let tile = scene.tiles[t];
        let shape = tile.shape.rotate_right(2 * u32::from(r));
        let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let (left, top) = (i32::from(shape & 3), i32::from(shape >> 6));
        let mut forced = [false; 2];
        for (e, forced) in forced.iter_mut().enumerate() {
            let d = map_direction(r, e * 2);
            let Some(n) = scene.neighbour(t, d) else {
                continue;
            };
            if scene.edge_state(t, d) < 2 {
                continue;
            }
            // The corners of the edge: left and top, or top and right.
            let corner = |s: usize| map_direction(r, s);
            let (ca, cb) = match e {
                0 => (corner(7), corner(1)),
                _ => (corner(1), corner(3)),
            };
            let nt = scene.tiles[n];
            let (nx, ny) = ((n % scene.width) as i32, (n / scene.width) as i32);
            let elevation = |c: usize| {
                let (cx, cy) = corner_xy(c);
                let own = corner_elevation(&tile, c);
                let (lx, ly) = (tx + cx - nx, ty + cy - ny);
                (own, corner_elevation(&nt, corner_at(lx, ly)))
            };
            let ((own_a, nb_a), (own_b, nb_b)) = (elevation(ca), elevation(cb));
            let gap = nb_a.min(nb_b) - own_a.max(own_b);
            let slope = own_a - own_b;
            let nslope = nb_a - nb_b;
            *forced = gap < 0 || slope < 0;
            if gap < 0 {
                continue;
            }
            let (x, mut y) = match e {
                0 => (16, 32),
                _ => (48, 32 + 16 * (left - top)),
            };
            // The pieces of the upper-left edge are numbered 11 to 15, those of the other 1 to 5.
            let base = if e == 0 { 10 } else { 0 };
            let mut pieces = Vec::new();
            if slope != 0 {
                pieces.push((base + if slope > 0 { 5 } else { 4 }, y));
                if slope < 0 {
                    y -= 16;
                }
            }
            for _ in 0..gap.min(2 * order::MAX_ELEVATION + 4) {
                pieces.push((base + 3, y));
                y -= 16;
            }
            if nslope != 0 {
                pieces.push((base + if nslope < 0 { 2 } else { 1 }, y));
            }
            for (piece, y) in pieces {
                match self
                    .fringe
                    .get(&piece)
                    .cloned()
                    .and_then(|s| self.piece(&s))
                {
                    Some(flat) => {
                        summary.cliffs_drawn += 1;
                        out.push(Placed {
                            x: ox + x,
                            y: oy + y,
                            pic: Pic::Flat(flat),
                        });
                    }
                    None => summary.cliffs_missing += 1,
                }
            }
        }
        match self.ground(scene, t) {
            Some(flat) => {
                summary.tiles_drawn += 1;
                out.push(Placed {
                    x: ox + 32,
                    y: oy + 32,
                    pic: Pic::Flat(flat),
                });
            }
            None => summary.ground_missing += 1,
        }
        // Only the grid lines a cliff forces appear on a plain picture.
        let (north, east) = self
            .grid
            .get(&u32::from(shape))
            .cloned()
            .unwrap_or_default();
        for (on, name) in [(forced[0], north), (forced[1], east)] {
            if let (true, Some(name)) = (on, name)
                && let Some(flat) = self.piece(&name)
            {
                out.push(Placed {
                    x: ox + 32,
                    y: oy + 32,
                    pic: Pic::Flat(flat),
                });
            }
        }
    }

    /// The `.ani` member an object shows at rest.
    fn member(&self, scene: &Scene, o: usize, ty: &EntityType) -> Option<String> {
        let obj = &scene.objects[o];
        let wanted = match ty.class {
            "fences" | "tankwall" => {
                let gate = ty.subtype.first() == Some(&b'g');
                let slope = scene.tile_at(obj.x, obj.y).filter(|_| !gate).map(|t| {
                    let tile = &scene.tiles[t];
                    // The corners of the edge the fence stands on, in this order.
                    let [c1, c2] = match obj.facing / 2 {
                        0 => [7, 1],
                        1 => [1, 3],
                        2 => [3, 5],
                        _ => [5, 7],
                    };
                    corner_elevation(tile, c2).cmp(&corner_elevation(tile, c1))
                });
                match slope {
                    Some(std::cmp::Ordering::Greater) => "idle30p",
                    Some(std::cmp::Ordering::Less) => "idle30n",
                    _ => "idle",
                }
            }
            "animals" | "guests" | "keeper" | "maint" | "tour" => "idle",
            _ => "",
        };
        let found = ty
            .animations()
            .into_iter()
            .find(|a| a.name == wanted && !wanted.is_empty());
        found.or_else(|| ty.base_animation()).and_then(|a| a.member)
    }

    /// Places the picture of one object.
    fn object(
        &mut self,
        scene: &Scene,
        o: usize,
        ty: Option<&EntityType>,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let obj = scene.objects[o];
        if !scene.is_placed(o) {
            return;
        }
        let anim = ty
            .and_then(|ty| self.member(scene, o, ty))
            .and_then(|member| self.animation(&member));
        let (Some(anim), Some(ty)) = (anim, ty) else {
            summary.objects_missing += 1;
            return;
        };
        let view = match obj.kind {
            Kind::Path => View {
                file: path_view(scene, o, ty).min(anim.sprites.len().saturating_sub(1)),
                mirror: false,
            },
            _ => anim.view_of(obj.facing as u8, scene.rotation),
        };
        let (x, y) = project(scene.rotation, obj.x, obj.y, obj.z);
        summary.objects_drawn += 1;
        out.push(Placed {
            x,
            y,
            pic: Pic::Anim(anim, view),
        });
    }
}

/// The image origin of a tile in view space (map drawing 4.4).
fn origin(scene: &Scene, t: usize) -> (i32, i32) {
    let r = scene.rotation;
    let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
    let height = 16 * corner_elevation(&scene.tiles[t], map_direction(r, 7));
    let (px, py) = project(r, 64 * tx, 64 * ty, height);
    let (dx, dy) = ORIGIN_OFFSET[usize::from(r)];
    (px + dx, py + dy)
}

/// The index of the picture of a path piece (entity types 9.3, sprite drawing 3.6).
fn path_view(scene: &Scene, o: usize, ty: &EntityType) -> usize {
    let obj = &scene.objects[o];
    let Some(t) = scene.tile_at(obj.x, obj.y) else {
        return 0;
    };
    let shape = i32::from(
        scene.tiles[t]
            .shape
            .rotate_right(2 * u32::from(scene.rotation)),
    );
    let code = match shape {
        0 => {
            let near = |e: usize| {
                let d = map_direction(scene.rotation, e * 2);
                i32::from(
                    scene
                        .neighbour(t, d)
                        .and_then(|n| scene.path_of(n))
                        .is_some(),
                )
            };
            100 + 8 * near(0) + 4 * near(1) + 2 * near(2) + near(3)
        }
        100 => 255,
        s => s,
    };
    let codes: Vec<Option<i32>> = ty
        .config
        .as_deref()
        .and_then(|c| c.section("Shapes"))
        .map(|s| s.get_all("shape").iter().map(|v| to_int(v)).collect())
        .unwrap_or_default();
    let find = |code: i32| codes.iter().position(|&c| c == Some(code));
    find(code).or_else(|| find(100)).unwrap_or(0)
}
