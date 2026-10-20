//! Drawing a whole map the way the original draws its map view, at the three zoom levels.
//!
//! The ground of a tile is one picture chosen by a key (terrain type, shape, variation and the
//! types of the eight neighbours) and read from the caches the game ships (`ztats`, `ztatb`);
//! when the cache has none, [`crate::ground::Renderer`] makes it. Cliffs are drawn by the tile in
//! front of them from `ztst/fringe`, or, when the data lacks that copy, from the root bitmaps
//! ([`crate::bitmap`]). Objects are drawn with the base animation of their type; a building the
//! player can recolour is drawn with the colour variants its record stores. A tank exhibit is
//! painted in layers (`tank.rs`): the water, its side faces and the glass are blended at one
//! half over what is behind them, and the contents are drawn above or below the water.
//! The order comes from [`order::paint_order`]; this module turns each item of it into pictures
//! placed in view space, then draws them all on a canvas that holds them.

pub mod order;
mod pick;
mod tank;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::rc::Rc;

use crate::Framebuffer;
use crate::anim::{AnimSet, Animation, View};
use crate::bitmap;
use crate::camera::zoom_view;
use crate::config::{Config, Section, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::ground::{self, Key, Picture};
use crate::sprite::{Blend, Frame, Palette, Place, Sprite, screen};
use crate::timing::ini_whole;
use crate::vfs::Vfs;
use crate::world::{Shape, TankKind};
use crate::zoo::{Tile, Zoo};
use crate::zoom::{Zoom, origin_offset, piece_anchor};
use order::{Item, Kind, Object, Scene, TankSpec, map_direction, paint_order, project};

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

/// What to draw of a map besides all of it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    /// Draw only the window of view space with this left, top, width and height, black where
    /// the map has nothing, instead of the whole map: the view-space point of the top left
    /// pixel is the scroll position of the original's view.
    pub window: Option<(i32, i32, u32, u32)>,
    /// Leave out animals, guests, staff and ambient animals.
    pub no_units: bool,
    /// The zoom value of the view (-2, 0 or 2; map drawing 2.4): the picture and the window are
    /// in the view space of that zoom.
    pub zoom: i32,
    /// Draw the grid lines of every tile (the grid switched on).
    pub grid: bool,
}

/// The north grid, east grid and highlight sprites of a shape.
type Sprites = (Option<String>, Option<String>, Option<String>);

/// A tile drawn with the highlight sprite of this colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub tile: usize,
    pub color: [u8; 3],
}

/// What a picture of a map contains.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Summary {
    pub width: usize,
    pub height: usize,
    /// Tiles drawn with a picture of the cache, tiles drawn with a picture rendered at run time,
    /// and tiles left undrawn because that failed (a terrain texture is missing).
    pub tiles_cached: usize,
    pub tiles_rendered: usize,
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
    /// One line; the size is left out in a total over several maps.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.width > 0 {
            write!(f, "{}x{} ", self.width, self.height)?;
        }
        write!(
            f,
            "tiles: {} from the cache, {} rendered, {} undrawn; cliff pieces: {} drawn, {} missing; \
             objects: {} drawn, {} with a missing type or animation, {} not placed",
            self.tiles_cached,
            self.tiles_rendered,
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
        self.tiles_cached += other.tiles_cached;
        self.tiles_rendered += other.tiles_rendered;
        self.ground_missing += other.ground_missing;
        self.cliffs_drawn += other.cliffs_drawn;
        self.cliffs_missing += other.cliffs_missing;
        self.objects_drawn += other.objects_drawn;
        self.objects_missing += other.objects_missing;
        self.objects_skipped += other.objects_skipped;
    }
}

/// The transient items of the map view, given to the staging (placing 4.5).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Extra {
    /// The pending objects: one, or a piece for each tile of a drag.
    pub ghosts: Vec<Ghost>,
    /// Entities of the zoo (positions in `zoo.entities`) drawn tinted, with their tint.
    pub tinted: Vec<(usize, [u8; 3])>,
    /// The highlights of the terrain tool.
    pub marks: Vec<Mark>,
    /// The animation and frame the units hold; a unit that is not here shows its idle animation.
    pub shown: Vec<Shown>,
    /// Entities (ids) that are in the zoo but not drawn: the unit the player carries.
    pub hidden: Vec<u32>,
    /// The unit (id) that carries the selection marker (sprite drawing 9.2).
    pub selected: Option<u32>,
}

/// The animation and frame a unit of the zoo holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The id of the entity.
    pub id: u32,
    pub animation: String,
    /// The frame to draw.
    pub frame: u32,
}

/// An object that is not on the map: a pending object. It is painted like the temporary
/// entities of the tile under it (map drawing 9.5), tinted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ghost {
    /// The type, a position in the catalogue.
    pub ty: usize,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Direction 0 to 7.
    pub facing: u32,
    pub tint: [u8; 3],
    /// The wall height of a pending gate over a tank wall, drawn as a stack; 0 for any other
    /// object.
    pub wall: u32,
}

/// A picture of the cache: a frame and its palette.
type Flat = Rc<(Frame, Palette)>;

enum Pic {
    Flat(Flat),
    /// A picture drawn with one colour in place of its palette: the highlight of a tile.
    Mark(Flat, Palette),
    /// A ground picture rendered at run time; its pixels have an alpha.
    Rendered(Rc<Picture>),
    /// A frame of an animation seen from a view, with the palette to draw it with in place of
    /// its own.
    Anim(Rc<Animation>, View, Option<Palette>, usize),
    /// An animation drawn blended with what is behind it.
    Blended(Rc<Animation>, View, Blend),
}

/// A piece of a tile: where its tile's corner is in view space at normal zoom, and the offset of
/// the piece's anchor from the tile's image origin there (map drawing 8).
#[derive(Clone, Copy)]
struct Piece {
    corner: (i32, i32),
    offset: (i32, i32),
}

/// The frames of a view drawn for frame `frame`: its background frame, then the frame itself, and
/// whether they are mirrored.
fn shown_frames<'a>(
    anim: &'a Animation,
    view: &View,
    frame: usize,
) -> Option<Vec<(&'a Frame, bool)>> {
    let sprite = &anim.sprites.get(view.file)?.0;
    let frames = sprite.extra.iter().chain(sprite.frames.get(frame));
    Some(frames.map(|frame| (frame, view.mirror)).collect())
}

/// A picture placed in view space: `(x, y)` is its anchor at normal zoom.
struct Placed {
    x: i32,
    y: i32,
    /// Set for the pieces of a tile, which have their own anchors at the other zooms.
    piece: Option<Piece>,
    /// Set for a grid line that only a switched-on grid draws; it never counts in the bounds.
    grid: bool,
    pic: Pic,
}

/// Clamps to the range of the view-space coordinates the drawing uses.
fn clamp32(n: i64) -> i32 {
    n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl Placed {
    /// The box `[left, top, right, bottom)` of the pixels the picture can cover.
    fn bounds(&self) -> Option<[i64; 4]> {
        if let Pic::Rendered(picture) = &self.pic {
            let left = i64::from(self.x) - i64::from(picture.x_offset);
            let top = i64::from(self.y) - i64::from(picture.y_offset);
            let (w, h) = (
                i64::from(picture.image.width),
                i64::from(picture.image.height),
            );
            return Some([left, top, left + w, top + h]);
        }
        let frames: Vec<(&Frame, bool)> = match &self.pic {
            Pic::Flat(flat) | Pic::Mark(flat, _) => vec![(&flat.0, false)],
            Pic::Rendered(_) => return None,
            Pic::Anim(anim, view, _, frame) => shown_frames(anim, view, *frame)?,
            Pic::Blended(anim, view, _) => shown_frames(anim, view, 0)?,
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

    /// The anchor at the zoom value `zoom` (level `level`), in the view space of that zoom: the
    /// camera's conversion of the view position, and for a tile piece the origin offset and the
    /// anchor of map drawing 4.4 and 8.
    fn anchor(&self, rotation: u8, zoom: i32, level: Zoom) -> (i64, i64) {
        match self.piece {
            Some(Piece { corner, offset }) if level != Zoom::Normal => {
                // `piece_anchor` adds its origin unchanged, so the corner is added in i64 after.
                let (cx, cy) = zoom_view(zoom, corner);
                let (x, y) = piece_anchor(origin_offset(rotation, level), offset, level);
                (cx + i64::from(x), cy + i64::from(y))
            }
            _ => zoom_view(zoom, (self.x, self.y)),
        }
    }

    /// The box `b` of the picture at normal zoom, moved to the zoomed `anchor`: scaled about the
    /// anchor by the blitter, with two pixels to spare for its rounding.
    fn zoomed_box(&self, b: &[i64; 4], anchor: (i64, i64), level: Zoom) -> [i64; 4] {
        let rel = [
            b[0] - i64::from(self.x),
            b[1] - i64::from(self.y),
            b[2] - i64::from(self.x),
            b[3] - i64::from(self.y),
        ];
        let rel = match level {
            Zoom::Normal => {
                return [
                    rel[0] + anchor.0,
                    rel[1] + anchor.1,
                    rel[2] + anchor.0,
                    rel[3] + anchor.1,
                ];
            }
            Zoom::In => rel.map(|n| n * 2),
            Zoom::Out => rel.map(|n| n.div_euclid(2)),
        };
        [
            rel[0] + anchor.0 - 2,
            rel[1] + anchor.1 - 2,
            rel[2] + anchor.0 + 2,
            rel[3] + anchor.1 + 2,
        ]
    }

    /// Draws the picture with its anchor at `anchor` of the view whose top left pixel is
    /// (`dx`, `dy`), at the zoom `level`.
    fn draw(
        &self,
        target: &mut Framebuffer,
        (dx, dy): (i32, i32),
        anchor: (i64, i64),
        level: Zoom,
    ) {
        let at = (
            clamp32(anchor.0 - i64::from(dx)),
            clamp32(anchor.1 - i64::from(dy)),
        );
        match &self.pic {
            Pic::Flat(flat) => flat.0.draw_zoomed(&flat.1, target, at, false, None, level),
            Pic::Mark(flat, palette) => flat.0.draw_zoomed(palette, target, at, false, None, level),
            Pic::Rendered(picture) => blend(&picture.image, target, at, picture, level),
            Pic::Anim(anim, view, palette, frame) => {
                anim.draw_zoomed(*view, *frame, target, at, palette.as_ref(), None, level)
            }
            Pic::Blended(anim, view, blend) => {
                anim.draw_zoomed(*view, 0, target, at, None, Some(*blend), level)
            }
        }
    }
}

/// A map ready to be drawn from one rotation: its pictures placed in view space at normal zoom,
/// in paint order, each with the box it can cover. Any window of the view, at any zoom, is drawn
/// from it.
pub struct Staged {
    rotation: u8,
    placed: Vec<(Placed, [i64; 4])>,
    /// For each picture of `placed`, the entity of the zoo it was placed for (picking).
    hits: Vec<Option<pick::Hit>>,
    /// The box `[left, top, right, bottom)` of every tile.
    tile_bounds: [i64; 4],
    /// The box of every tile and every picture.
    bounds: [i64; 4],
    pub summary: Summary,
}

impl Staged {
    /// The size of the picture of the whole map at normal zoom, or an error if it is empty or
    /// too big.
    pub fn size(&self) -> Result<(u32, u32), RenderError> {
        let (w, h) = check_size(&self.bounds)?;
        Ok((w as u32, h as u32))
    }

    /// The view-space point of the top left pixel of the picture of the whole map at normal zoom.
    pub fn origin(&self) -> (i32, i32) {
        (self.bounds[0] as i32, self.bounds[1] as i32)
    }

    /// The box `[left, top, right, bottom)` of the whole map in the view space of the zoom value
    /// `zoom`.
    pub fn bounds_at(&self, zoom: i32) -> [i64; 4] {
        let level = Zoom::from_value(zoom);
        if level == Zoom::Normal {
            return self.bounds;
        }
        // The tiles scale by the zoom factor, with two pixels to spare.
        let scale = |n: i64| match zoom {
            ..=-2 => n.div_euclid(-i64::from(zoom)),
            _ => n.saturating_mul(i64::from(zoom)),
        };
        let [l, t, r, b] = self.tile_bounds.map(scale);
        let tiles = [l - 2, t - 2, r + 2, b + 2];
        self.placed
            .iter()
            .filter(|(p, _)| !p.grid)
            .fold(tiles, |all, (placed, b)| {
                let anchor = placed.anchor(self.rotation, zoom, level);
                union(all, placed.zoomed_box(b, anchor, level))
            })
    }

    /// Draws the window of the size of `target` whose top left pixel is the point (`view_x`,
    /// `view_y`) of the view space at the zoom value `zoom` (-2, 0 or 2; map drawing 2.4): black,
    /// then every picture whose box meets the window, in paint order, the grid lines of the tiles
    /// only when `grid` is set (those a cliff forces are always drawn). The anchors are the camera's
    /// view positions at the zoom, the tile pieces' those of map drawing 8, and the blitter has
    /// the variant of the zoom level.
    pub fn draw(&self, target: &mut Framebuffer, view_x: i32, view_y: i32, zoom: i32, grid: bool) {
        target.fill([0, 0, 0, 255]);
        let level = Zoom::from_value(zoom);
        let (left, top) = (i64::from(view_x), i64::from(view_y));
        let right = left + i64::from(target.width);
        let bottom = top + i64::from(target.height);
        for (placed, b) in self.placed.iter().filter(|(p, _)| grid || !p.grid) {
            let anchor = placed.anchor(self.rotation, zoom, level);
            let b = placed.zoomed_box(b, anchor, level);
            if b[0] < right && b[2] > left && b[1] < bottom && b[3] > top {
                placed.draw(target, (view_x, view_y), anchor, level);
            }
        }
    }
}

/// Draws an RGBA image whose anchor is at `at` at a zoom, source over, clipped. The pixels land
/// as those of a frame with the same offsets do.
fn blend(
    image: &Framebuffer,
    target: &mut Framebuffer,
    (x, y): (i32, i32),
    picture: &Picture,
    zoom: Zoom,
) {
    let place = Place {
        x: i64::from(x),
        y: i64::from(y),
        mirror: false,
        zoom,
        x_offset: i64::from(picture.x_offset),
        y_offset: i64::from(picture.y_offset),
        height: i64::from(image.height),
    };
    let width = image.width as usize;
    for (row, line) in image.pixels.chunks_exact(width.max(1) * 4).enumerate() {
        let (first, count) = place.row(row as i64);
        for ty in first..first + count {
            if !(0..i64::from(target.height)).contains(&ty) {
                continue;
            }
            for (column, pixel) in line.as_chunks::<4>().0.iter().enumerate() {
                if pixel[3] == 0 {
                    continue;
                }
                let (first, copies) = place.column(column as i64);
                for tx in first..first + copies {
                    if !(0..i64::from(target.width)).contains(&tx) {
                        continue;
                    }
                    let at = (ty as usize * target.width as usize + tx as usize) * 4;
                    let back = &mut target.pixels[at..at + 4];
                    let alpha = u32::from(pixel[3]);
                    let mixed = [0, 1, 2].map(|c| {
                        ((u32::from(pixel[c]) * alpha + u32::from(back[c]) * (255 - alpha)) / 255)
                            as u8
                    });
                    let [r, g, b] = screen(mixed);
                    back.copy_from_slice(&[r, g, b, 255]);
                }
            }
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
pub(crate) fn corner_elevation(tile: &crate::zoo::Tile, corner: usize) -> i32 {
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

/// Draws maps. It keeps what it loaded between maps: the catalogue, the animations and the
/// pictures of the cache.
pub struct MapRenderer<'a> {
    vfs: &'a Vfs,
    catalogue: &'a Catalogue,
    blend: Vec<bool>,
    /// The cliff pieces 1 to 15, as the names of their root bitmaps.
    fringe: HashMap<u32, String>,
    /// For each shape (decimal), the north and east grid sprites and the highlight.
    grid: HashMap<u32, Sprites>,
    flats: HashMap<String, Option<Flat>>,
    /// The renderer of ground pictures the cache lacks; `None` if the data cannot make any.
    ground: Option<ground::Renderer>,
    rendered: HashMap<String, Option<Rc<Picture>>>,
    anims: HashMap<String, Option<Rc<Animation>>>,
    /// The palette files read for recolouring, by normalised member.
    palettes: HashMap<String, Option<Palette>>,
    /// `building.ai`, which holds the colour groups the buildings share.
    groups: Rc<Config>,
    /// The purities below which tank water is murky and very murky (`tanks.cfg`).
    murky: (i32, i32),
    /// `[advanced] useTankShadows` of the ini file: units cast shadows on the floor of a tank.
    tank_shadows: bool,
}

impl<'a> MapRenderer<'a> {
    /// The files of the game this renders from.
    pub fn vfs(&self) -> &'a Vfs {
        self.vfs
    }

    /// The entity types this renders with.
    pub fn catalogue(&self) -> &'a Catalogue {
        self.catalogue
    }

    /// How an entity takes its place on the map, if its type is known.
    pub fn shape(&self, entity: &crate::zoo::Entity) -> Option<Shape> {
        let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        let ty = self.catalogue.find(
            &entity.class,
            &bytes(&entity.type_name),
            &bytes(&entity.subtype),
        )?;
        Some(Shape {
            kind: Kind::of_class(&entity.class),
            footprint: (ty.footprint[0], ty.footprint[1]),
            fence: habitat_fence(ty),
            wall: member(ty, "zoowall"),
            gate: member(ty, "zoogate"),
            gated: matches!(entity.class.as_str(), "fences" | "tankwall")
                && self
                    .catalogue
                    .find(&entity.class, &bytes(&entity.type_name), b"g")
                    .is_some(),
            tank: tank_kind(ty),
            walkable: (
                crate::build::number(ty, "walkable") == Some(1.0),
                crate::build::number(ty, "walkableByTall") == Some(1.0),
            ),
        })
    }

    /// The colour choices a new building of the type starts with, one byte per colour group (sprite
    /// drawing 8.1): the position of the `defaultpal` of each group among its variants. Empty for a
    /// type that is not recoloured.
    pub fn default_colors(&self, ty: &EntityType) -> Vec<u8> {
        let Some(config) = ty.config.as_deref() else {
            return Vec::new();
        };
        let flag = config
            .section("Characteristics/Integers")
            .and_then(|s| s.get("cIsColorReplaced"))
            .and_then(to_int);
        let Some(replaced) = config
            .section("colorrep")
            .filter(|_| ty.class == "building" && flag.is_some_and(|n| n != 0))
        else {
            return Vec::new();
        };
        let defaults = replaced.get_all("defaultpal");
        let named = replaced.get_all("replace").iter().enumerate();
        named
            .map(|(g, name)| default_choice(self.groups.section(name), defaults.get(g)))
            .collect()
    }

    pub fn new(vfs: &'a Vfs, catalogue: &'a Catalogue) -> MapRenderer<'a> {
        let read = |member: &str| {
            vfs.read(member)
                .ok()
                .and_then(|bytes| Config::parse(&bytes).ok())
                .unwrap_or_default()
        };
        let groups = Rc::new(read("building.ai"));
        let mut fringe = HashMap::new();
        for (name, section) in read("fringe.cfg").sections() {
            if let (Some(piece), Some(image)) = (
                to_int(name).and_then(|n| u32::try_from(n).ok()),
                section.get("image"),
            ) {
                fringe.insert(piece, bitmap::member_name(image));
            }
        }
        let mut grid = HashMap::new();
        for (name, section) in read("tile.cfg").sections() {
            if let Some(shape) = to_int(name).and_then(|n| u32::try_from(n).ok()) {
                let sprite = |key: &str| section.get(key).map(bitmap::member_name);
                grid.insert(
                    shape,
                    (sprite("northgrid"), sprite("eastgrid"), sprite("highlight")),
                );
            }
        }
        // The original reads the very murky limit and then overwrites it with the extremely murky
        // one, which it does read from the file (map drawing 9.7.1).
        let tanks = read("tanks.cfg");
        let limit = |key: &str| {
            tanks
                .section("tank")
                .and_then(|s| s.get(key))
                .and_then(to_int)
        };
        let murky = limit("murkyWaterPurity").unwrap_or(60);
        let very_murky = limit("extremelyMurkyWaterPurity")
            .or_else(|| limit("veryMurkyWaterPurity"))
            .unwrap_or(30);
        let tank_shadows = Config::parse(vfs.ini())
            .ok()
            .and_then(|c| ini_whole(&c, "advanced", "useTankShadows"))
            .is_none_or(|v| v == 1);
        MapRenderer {
            vfs,
            catalogue,
            blend: terrain_blend(vfs),
            fringe,
            grid,
            flats: HashMap::new(),
            ground: ground::Renderer::load(vfs).ok(),
            rendered: HashMap::new(),
            anims: HashMap::new(),
            palettes: HashMap::new(),
            groups,
            murky: (murky, very_murky),
            tank_shadows,
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

    /// The picture of a root bitmap: its shipped copy, or the bitmap converted in memory.
    fn piece(&mut self, bitmap: &str) -> Option<Flat> {
        let key = format!("{bitmap}|bitmap");
        if let Some(found) = self.flats.get(&key) {
            return found.clone();
        }
        let found = bitmap::load(self.vfs, bitmap).map(Rc::new);
        self.flats.insert(key, found.clone());
        found
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

    /// The palette file `member`, read once.
    fn palette(&mut self, member: &[u8]) -> Option<&Palette> {
        let member = crate::vfs::normalise(&String::from_utf8_lossy(member));
        self.palettes
            .entry(member)
            .or_insert_with_key(|member| Palette::parse(&self.vfs.read(member).ok()?).ok())
            .as_ref()
    }

    /// The palette to draw a recoloured building with, if its type is one (sprite drawing 8): the
    /// palette of the sprite file with the fixed colours of the type (only if the file names the
    /// type's `fullpal`), then, for each colour group, the entries of the variant the building stores (the default one for a group it
    /// stores nothing for). `None` draws the plain palette.
    fn recoloured(
        &mut self,
        ty: &EntityType,
        sprite: &(Sprite, Palette),
        stored: &[u8],
    ) -> Option<Palette> {
        let config = ty.config.clone()?;
        let flag = config
            .section("Characteristics/Integers")?
            .get("cIsColorReplaced");
        if ty.class != "building" || flag.and_then(to_int) != Some(1) {
            return None;
        }
        let replaced = config.section("colorrep")?;
        let fixed = config.section(replaced.get("color")?)?;
        let mut start = usize::try_from(to_int(fixed.get("ncolors")?)?).ok()?;
        let mut palette = sprite.1.clone();
        let same = same_member;
        // The fixed colours belong to the palette `fullpal` names, not to any other.
        if let (Some(full), Some(colors)) = (fixed.get("fullpal"), fixed.get("colorpal"))
            && same(sprite.0.palette.as_bytes(), full)
            && let Some(colors) = self.palette(colors)
        {
            palette.overwrite(0, colors, 0, start);
        }
        let groups = Rc::clone(&self.groups);
        let defaults = replaced.get_all("defaultpal");
        for (g, name) in replaced.get_all("replace").iter().enumerate() {
            let group = groups.section(name)?;
            let variants = group.get_all("pal");
            let chosen = match stored.get(g) {
                Some(&choice) => usize::from(choice),
                None => usize::from(default_choice(Some(group), defaults.get(g))),
            };
            if let Some(colors) = variants.get(chosen).and_then(|v| self.palette(v)) {
                palette.overwrite(start, colors, 1, usize::MAX);
            }
            start = start.saturating_add(usize::try_from(to_int(group.get("ncolors")?)?).ok()?);
        }
        Some(palette)
    }

    /// Draws the map seen from `rotation` (0 to 3), whole, on a black background.
    pub fn render(
        &mut self,
        zoo: &Zoo,
        rotation: u8,
    ) -> Result<(Framebuffer, Summary), RenderError> {
        self.render_with(zoo, rotation, &Options::default())
    }

    /// As [`MapRenderer::render`], as `options` say: [`MapRenderer::stage`] and one
    /// [`Staged::draw`] over the whole map or the window.
    pub fn render_with(
        &mut self,
        zoo: &Zoo,
        rotation: u8,
        options: &Options,
    ) -> Result<(Framebuffer, Summary), RenderError> {
        let staged = self.stage(zoo, rotation, options.no_units)?;
        let (x, y, w, h) = match options.window {
            Some(window) => window,
            None => {
                let b = staged.bounds_at(options.zoom);
                let (w, h) = check_size(&b)?;
                (b[0] as i32, b[1] as i32, w as u32, h as u32)
            }
        };
        check_size(&[0, 0, i64::from(w), i64::from(h)])?;
        let mut canvas = Framebuffer::new(w, h);
        staged.draw(&mut canvas, x, y, options.zoom, options.grid);
        Ok((canvas, staged.summary))
    }

    /// Does all the work of drawing the map seen from `rotation` (0 to 3) that does not depend
    /// on the window: the objects, the scene, the paint order and the pictures placed in view
    /// space with their bounds. The pictures stay cached in the renderer for the next call.
    pub fn stage(
        &mut self,
        zoo: &Zoo,
        rotation: u8,
        no_units: bool,
    ) -> Result<Staged, RenderError> {
        self.stage_with(zoo, rotation, no_units, &Extra::default())
    }

    /// As [`MapRenderer::stage`], with the transient items of the map view: they are in the paint
    /// order but not in the zoo. The marks of `extra` give a tile (an index into the tiles) the
    /// highlight sprite in their colour, after its ground and grid lines (map drawing 7); a tile
    /// that has a mark takes the grid lines off the two tiles behind it.
    pub fn stage_with(
        &mut self,
        zoo: &Zoo,
        rotation: u8,
        no_units: bool,
        extra: &Extra,
    ) -> Result<Staged, RenderError> {
        let marks: HashMap<usize, [u8; 3]> =
            extra.marks.iter().map(|m| (m.tile, m.color)).collect();
        let shown: HashMap<u32, &Shown> = extra.shown.iter().map(|s| (s.id, s)).collect();
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

        let tiles: Vec<Tile> = zoo
            .tiles
            .iter()
            .map(|t| Tile {
                elevation: t
                    .elevation
                    .clamp(-order::MAX_ELEVATION, order::MAX_ELEVATION),
                ..*t
            })
            .collect();
        let mut tile_bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
        for (t, tile) in tiles.iter().enumerate() {
            let (ox, oy) = origin(tile, (t % width) as i32, (t / width) as i32, rotation);
            let (x, y) = (i64::from(ox), i64::from(oy));
            tile_bounds = union(tile_bounds, [x, y + 16, x + 64, y + 49]);
        }
        // Refuse a huge map before the work: staging costs memory in proportion to its tiles.
        check_size(&tile_bounds)?;

        // The objects the original keeps: those whose type it knows.
        let mut objects = Vec::new();
        let mut metas: Vec<Meta> = Vec::new();
        // The entity each object stands for, by index in `objects`.
        let mut owners: Vec<Option<pick::Hit>> = Vec::new();
        for (position, entity) in zoo.entities.iter().enumerate() {
            let Some(c) = &entity.common else {
                summary.objects_skipped += 1;
                continue;
            };
            let kind = Kind::of_class(&entity.class);
            if no_units && matches!(kind, Kind::Unit | Kind::Overlay) {
                continue;
            }
            if extra.hidden.contains(&c.id) {
                continue;
            }
            let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
            let Some(ty) = self.catalogue.find(
                &entity.class,
                &bytes(&entity.type_name),
                &bytes(&entity.subtype),
            ) else {
                summary.objects_missing += 1;
                continue;
            };
            let [fx, fy, _] = ty.footprint;
            objects.push(Object {
                kind,
                x: c.x,
                y: c.y,
                z: c.z.clamp(-4096, 4096),
                facing: c.rotation % 8,
                footprint: (fx, fy),
                late: draws_late(ty),
                height: ty.height,
            });
            owners.push(Some(pick::Hit {
                id: c.id,
                threshold: pick::threshold(ty),
            }));
            metas.push(Meta {
                ty,
                colors: &c.colors,
                life: c.fence.map(|f| f.life as i32),
                mode: c.movement_mode,
                portal: c.fence.is_some_and(|f| f.portal),
                food: c.food,
                tint: extra
                    .tinted
                    .iter()
                    .find(|(e, _)| *e == position)
                    .map(|(_, tint)| *tint),
                wall: 0,
                shown: shown
                    .get(&c.id)
                    .map(|s| (s.animation.as_str(), s.frame as usize)),
            });
        }
        for ghost in &extra.ghosts {
            let Some(ty) = self.catalogue.types.get(ghost.ty) else {
                continue;
            };
            objects.push(Object {
                kind: Kind::Overlay,
                x: ghost.x,
                y: ghost.y,
                z: ghost.z.clamp(-4096, 4096),
                facing: ghost.facing % 8,
                footprint: (ty.footprint[0], ty.footprint[1]),
                late: draws_late(ty),
                height: ty.height,
            });
            owners.push(None);
            metas.push(Meta {
                ty,
                colors: &[],
                life: None,
                mode: 0,
                portal: false,
                food: 0,
                tint: Some(ghost.tint),
                wall: ghost.wall.min(order::MAX_ELEVATION as u32) as i32,
                shown: None,
            });
        }
        let mut scene = Scene::new(width, height, rotation, &tiles, objects);
        let cut = |n: u32| i32::try_from(n.min(order::MAX_ELEVATION as u32)).unwrap_or(0);
        scene.set_tanks(
            &zoo.tanks
                .iter()
                .map(|t| TankSpec {
                    tile: t.tile,
                    gate: t.gate,
                    height: cut(t.height),
                    level: cut(t.level),
                })
                .collect::<Vec<_>>(),
            &metas
                .iter()
                .map(|m| habitat_fence(m.ty))
                .collect::<Vec<_>>(),
        );
        let looks: Vec<_> = zoo
            .tanks
            .iter()
            .map(|t| self.look(t.salt, t.purity))
            .collect();
        // The game puts a ripple on the clean water against every active wall of a tank.
        let first_ripple = metas.len();
        let ripple = self.catalogue.find("objects", b"other", b"tankripl");
        for ty in ripple.iter().flat_map(|&ty| {
            let n = add_ripples(&mut scene, &looks, tiles.len());
            std::iter::repeat_n(ty, n)
        }) {
            metas.push(Meta {
                ty,
                colors: &[],
                life: None,
                mode: 0,
                portal: false,
                food: 0,
                tint: None,
                wall: 0,
                shown: None,
            });
        }

        let mut placed: Vec<Placed> = Vec::new();
        let mut hits: Vec<Option<pick::Hit>> = Vec::new();
        for item in paint_order(&scene) {
            let before = placed.len();
            // The pictures of a path, an edge object or any other object are the entity's own.
            let owner = match item {
                Item::Path(o) | Item::Edge(o) | Item::Object(o) => owners.get(o).copied().flatten(),
                _ => None,
            };
            match item {
                Item::Terrain(t) => self.terrain(&scene, &marks, t, &mut summary, &mut placed),
                Item::Object(o) if o >= first_ripple => {
                    self.object(&scene, o, &metas[o], &mut Summary::default(), &mut placed)
                }
                Item::Object(o) if metas[o].wall > 0 && owners[o].is_none() => {
                    self.pending_wall(&scene, &metas, o, &mut summary, &mut placed)
                }
                Item::Path(o) | Item::Object(o) => {
                    self.object(&scene, o, &metas[o], &mut summary, &mut placed)
                }
                Item::Edge(o) if scene.wall_tank(o).is_none() => {
                    self.object(&scene, o, &metas[o], &mut summary, &mut placed)
                }
                item => self.tank_item(&scene, &metas, &looks, item, &mut summary, &mut placed),
            }
            hits.resize(placed.len(), owner);
            if let (Item::Object(o), Some(hit)) = (item, owner)
                && Some(hit.id) == extra.selected
                && placed.len() > before
                && scene.objects[o].kind == Kind::Unit
                && self.marker(metas[o].ty, &mut placed)
            {
                hits.push(None);
            }
        }
        summary.objects_skipped += (0..metas.len()).filter(|&o| !scene.is_placed(o)).count();

        // The box of every tile and every picture.
        let (placed, hits): (Vec<_>, Vec<_>) = placed
            .into_iter()
            .zip(hits)
            .filter_map(|(p, hit)| p.bounds().map(|b| ((p, b), hit)))
            .unzip();
        let bounds = placed
            .iter()
            .filter(|(p, _)| !p.grid)
            .fold(tile_bounds, |all, (_, b)| union(all, *b));
        Ok(Staged {
            rotation,
            placed,
            hits,
            tile_bounds,
            bounds,
            summary,
        })
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

    /// The ground picture of a tile: the cache's, or else one rendered at run time.
    fn ground(&mut self, scene: &Scene, t: usize) -> Option<Pic> {
        let r = scene.rotation;
        let tile = scene.tiles[t];
        let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let kind = self.kind(scene, t);
        let (a, b) = [(0, 1), (1, 0), (0, -1), (-1, 0)][usize::from(r)];
        let umod3 = |k: i32| (k as u32) % 3;
        let key = Key {
            terrain: kind,
            shape: tile.shape.rotate_right(2 * u32::from(r)),
            variation: (umod3(tx * b - ty * a) * 3 + umod3(tx * a + ty * b)) as u8,
            neighbours: std::array::from_fn(|k| self.seen(scene, t, map_direction(r, k))),
        };
        let member = key.member();
        let stem = member.strip_suffix(".lle").unwrap_or(&member);
        let palette = key.palette();
        let palette = palette.strip_suffix(".pal").unwrap_or(&palette);
        if let Some(flat) = self.flat(stem, palette) {
            return Some(Pic::Flat(flat));
        }
        if !self.rendered.contains_key(&member) {
            let picture = self
                .ground
                .as_ref()
                .and_then(|g| g.render(&key).ok())
                .map(Rc::new);
            self.rendered.insert(member.clone(), picture);
        }
        self.rendered[&member].clone().map(Pic::Rendered)
    }

    /// Places the pictures of one tile: cliffs, ground and grid lines.
    fn terrain(
        &mut self,
        scene: &Scene,
        marks: &HashMap<usize, [u8; 3]>,
        t: usize,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let r = scene.rotation;
        let tile = scene.tiles[t];
        let (tx, ty) = ((t % scene.width) as i32, (t / scene.width) as i32);
        let corner = corner_view(&tile, tx, ty, r);
        let (ox, oy) = origin(&tile, tx, ty, r);
        let piece_at = |offset: (i32, i32)| Piece { corner, offset };
        let shape = tile.shape.rotate_right(2 * u32::from(r));
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
                            piece: Some(piece_at((x, y))),
                            grid: false,
                            pic: Pic::Flat(flat),
                        });
                    }
                    None => summary.cliffs_missing += 1,
                }
            }
        }
        match self.ground(scene, t) {
            Some(pic) => {
                match pic {
                    Pic::Rendered(_) => summary.tiles_rendered += 1,
                    _ => summary.tiles_cached += 1,
                }
                out.push(Placed {
                    x: ox + 32,
                    y: oy + 32,
                    piece: Some(piece_at((32, 32))),
                    grid: false,
                    pic,
                });
            }
            None => summary.ground_missing += 1,
        }
        // The grid lines a cliff forces are always drawn; the others only with the grid on.
        let (north, east, highlight) = self
            .grid
            .get(&u32::from(shape))
            .cloned()
            .unwrap_or_default();
        for (e, (forced, name)) in [(forced[0], north), (forced[1], east)]
            .into_iter()
            .enumerate()
        {
            let behind = scene
                .neighbour(t, map_direction(r, e * 2))
                .is_some_and(|n| marks.contains_key(&n));
            if let Some(name) = name.filter(|_| !behind)
                && let Some(flat) = self.piece(&name)
            {
                out.push(Placed {
                    x: ox + 32,
                    y: oy + 32,
                    piece: Some(piece_at((32, 32))),
                    grid: !forced,
                    pic: Pic::Flat(flat),
                });
            }
        }
        if let Some(color) = marks.get(&t)
            && let Some(name) = highlight
            && let Some(flat) = self.piece(&name)
        {
            let palette = flat.1.solid(*color);
            out.push(Placed {
                x: ox + 32,
                y: oy + 32,
                piece: Some(piece_at((32, 32))),
                grid: false,
                pic: Pic::Mark(flat, palette),
            });
        }
    }

    /// The `.ani` member an object shows at rest.
    fn member(&self, scene: &Scene, o: usize, meta: &Meta) -> Option<String> {
        let (obj, ty) = (&scene.objects[o], meta.ty);
        let wanted = match ty.class {
            "fences" | "tankwall" => {
                let gate = ty.subtype.first() == Some(&b'g');
                // A gate shows the intact row whatever its life; the others by life.
                let decayed = ty
                    .config
                    .as_deref()
                    .and_then(|c| {
                        c.section([&ty.subtype[..], b"/Characteristics/Integers"].concat())
                    })
                    .and_then(|s| s.get("cDecayedLife"))
                    .and_then(to_int)
                    .unwrap_or(0);
                let row = match meta.life {
                    Some(life) if !gate && life < 1 => "broke",
                    Some(life) if !gate && life <= decayed => "det",
                    _ => "idle",
                };
                // The corners of the edge the fence stands on, in this order.
                let slope = scene.tile_at(obj.x, obj.y).map(|t| {
                    let tile = &scene.tiles[t];
                    let [c1, c2] = match obj.facing / 2 {
                        0 => [7, 1],
                        1 => [1, 3],
                        2 => [3, 5],
                        _ => [5, 7],
                    };
                    corner_elevation(tile, c2).cmp(&corner_elevation(tile, c1))
                });
                match slope {
                    Some(std::cmp::Ordering::Greater) => format!("{row}30p"),
                    Some(std::cmp::Ordering::Less) => format!("{row}30n"),
                    _ => row.to_string(),
                }
            }
            // An animal inside a tank swims: below the water it shows `subidle`, at the surface
            // `surfidle` (sprite drawing 3.6).
            "animals" if meta.mode == 3 => {
                let t = scene.tile_at(obj.x, obj.y);
                match t.and_then(|t| scene.surface(t)) {
                    Some(surface) if obj.z >= surface => "surfidle".to_string(),
                    Some(_) => "subidle".to_string(),
                    None => "idle".to_string(),
                }
            }
            "animals" | "guests" | "keeper" | "maint" | "tour" => {
                meta.shown.map_or("idle", |(name, _)| name).to_string()
            }
            // A dung pile shows `idle`, `small` or `giant` by its size (animals-needs 8.2).
            "objects" if ty.type_name == b"other" && ty.subtype.ends_with(b"poo") => {
                ["idle", "small", "giant"][meta.food.clamp(0, 2) as usize].into()
            }
            // A trash can shows `empty` up to 12 pieces and `half` from 13; `full` is never shown
            // (guest buildings 8). Its counter holds the pieces.
            "building" if ty.subtype == b"trshcan" => {
                ["empty", "half"][usize::from(meta.food >= 13)].into()
            }
            // Food shows `empty`, `half` or `full` by its units left (animals-needs 5.1).
            "food" => {
                let max = crate::build::number(ty, "cMaxFoodUnits").unwrap_or(0.0) as i32;
                crate::animal::food::STATES[crate::animal::food::fullness(meta.food, max)].into()
            }
            _ => String::new(),
        };
        // Without the swimming pictures an animal in a tank shows its rest picture.
        let tank = ty.class == "animals" && meta.mode == 3;
        let mut animations = ty.animations();
        let at = [wanted.as_str(), "idle"]
            .into_iter()
            .take(if tank || meta.shown.is_some() { 2 } else { 1 })
            .filter(|w| !w.is_empty())
            .find_map(|w| animations.iter().position(|a| a.name == w));
        let found = at.map(|i| animations.swap_remove(i));
        found.or_else(|| ty.base_animation()).and_then(|a| a.member)
    }

    /// Places the selection marker after the picture of the unit that was placed last (sprite
    /// drawing 9.2): `selsmall` for a footprint of at most 1 x 1 half tiles, `selmed` for at most
    /// 4 in each direction, else `sellarge`. Its anchor is the unit's, up and left by a quarter of
    /// the x offset of the unit's frame, and by its y offset, 5 and half the height of the marker's
    /// own frame (the type's height plays no part). The marker has one frame, so it
    /// needs no clock. Returns whether it was placed.
    fn marker(&mut self, ty: &EntityType, out: &mut Vec<Placed>) -> bool {
        let Some(Placed {
            x,
            y,
            pic: Pic::Anim(anim, view, _, at),
            ..
        }) = out.last()
        else {
            return false;
        };
        let Some(frame) = shown_frames(anim, view, *at).and_then(|f| f.last().map(|f| f.0)) else {
            return false;
        };
        let (x, y, xo, yo) = (*x, *y, i32::from(frame.x_offset), i32::from(frame.y_offset));
        let [fx, fy, _] = ty.footprint;
        let name = match fx.max(fy) {
            ..=1 => "selsmall",
            2..=4 => "selmed",
            _ => "sellarge",
        };
        let Some(anim) = self.animation(&format!("ui/select/{name}/{name}.ani")) else {
            return false;
        };
        let view = View {
            file: 0,
            mirror: false,
        };
        // Up by the unit frame's y offset, 5 and half the marker's own frame height.
        let half = anim.sprites[0]
            .0
            .frames
            .first()
            .map_or(0, |f| i32::from(f.height) / 2);
        out.push(Placed {
            x: x - xo / 4,
            y: y - (yo + 5 + half),
            piece: None,
            grid: false,
            pic: Pic::Anim(anim, view, None, 0),
        });
        true
    }

    /// Places the picture of one object.
    fn object(
        &mut self,
        scene: &Scene,
        o: usize,
        meta: &Meta,
        summary: &mut Summary,
        out: &mut Vec<Placed>,
    ) {
        let obj = scene.objects[o];
        if !scene.is_placed(o) {
            return;
        }
        let ty = meta.ty;
        let anim = self
            .member(scene, o, meta)
            .and_then(|member| self.animation(&member));
        let Some(anim) = anim else {
            summary.objects_missing += 1;
            return;
        };
        // The pending piece of a path is an overlay that shows what a placed one would.
        let view = match obj.kind {
            Kind::Path | Kind::Overlay if ty.class == "paths" => View {
                file: path_view(scene, o, ty).min(anim.sprites.len().saturating_sub(1)),
                mirror: false,
            },
            _ => anim.view_of(obj.facing as u8, scene.rotation),
        };
        let (x, y) = project(scene.rotation, obj.x, obj.y, obj.z);
        summary.objects_drawn += 1;
        // A unit's frame, as far as the picture of its view has them.
        let last = anim
            .sprites
            .get(view.file)
            .map_or(0, |s| s.0.frames.len().saturating_sub(1));
        let frame = meta.shown.map_or(0, |(_, frame)| frame.min(last));
        let palette = anim.sprites.get(view.file).and_then(|sprite| {
            let own = self.recoloured(ty, sprite, meta.colors);
            match meta.tint {
                Some(tint) => Some(own.unwrap_or_else(|| sprite.1.clone()).tinted(tint)),
                None => own,
            }
        });
        out.push(Placed {
            x,
            y,
            piece: None,
            grid: false,
            pic: Pic::Anim(anim, view, palette, frame),
        });
    }
}

/// Whether two palette file names are the same member.
fn same_member(a: &[u8], b: &[u8]) -> bool {
    crate::vfs::normalise(&String::from_utf8_lossy(a))
        == crate::vfs::normalise(&String::from_utf8_lossy(b))
}

/// The position of `default` among the `pal` variants of a colour group, 0 when it is not there
/// (sprite drawing 8.1).
fn default_choice(group: Option<&Section>, default: Option<&Vec<u8>>) -> u8 {
    group
        .zip(default)
        .and_then(|(group, d)| group.get_all("pal").iter().position(|v| same_member(v, d)))
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(0)
}

/// The type and the condition of an object.
struct Meta<'a> {
    ty: &'a EntityType,
    /// The colour choices the record stores.
    colors: &'a [u8],
    /// The life of a fence or tank wall, if the record has one.
    life: Option<i32>,
    /// The movement mode of a unit: 3 inside a tank.
    mode: u32,
    /// A tank wall that is a portal.
    portal: bool,
    /// The units left of a food entity.
    food: i32,
    /// The colour the object is drawn tinted with (sprite drawing 9.1).
    tint: Option<[u8; 3]>,
    /// The wall height of a pending tank gate, 0 for anything else.
    wall: i32,
    /// The animation and frame a unit holds.
    shown: Option<(&'a str, usize)>,
}

/// Whether the `[Member]` list of the type holds this name.
pub(crate) fn member(ty: &EntityType, name: &str) -> bool {
    ty.config
        .as_deref()
        .and_then(|c| c.section("Member"))
        .is_some_and(|m| m.has(name))
}

/// What the type says about tanks: a tank wall (a `[TankFence]` section) and its show flag, and
/// what a tank allows inside it (exhibits and money 3.9).
fn tank_kind(ty: &EntityType) -> TankKind {
    let flag = |key: &str| crate::build::number(ty, key) == Some(1.0);
    let wall = ty
        .config
        .as_deref()
        .is_some_and(|c| c.section("TankFence").is_some());
    let show = flag("cIsShowFence");
    TankKind {
        wall,
        show,
        underwater: wall || flag("cUnderwater"),
        surface: flag("cSurface"),
        shows: show || flag("cShow"),
        transient: flag("cIsTransient"),
        life: crate::build::number(ty, "cLife").unwrap_or(crate::place::LIFE) as u32,
        strength: crate::build::number(ty, "cStrength").unwrap_or(crate::place::STRENGTH) as u32,
        depth: crate::build::number(ty, "cDepth").unwrap_or(0.0) as i32,
        height: crate::build::number(ty, "cHeight").unwrap_or(0.0) as i32,
    }
}

/// Whether the type is a habitat fence: a fence or tank wall whose definition has a
/// `[HabitatFence]` or `[TankFence]` section. Such a fence ends the area of an exhibit.
fn habitat_fence(ty: &EntityType) -> bool {
    matches!(ty.class, "fences" | "tankwall")
        && ty.config.as_deref().is_some_and(|c| {
            c.section("HabitatFence").is_some() || c.section("TankFence").is_some()
        })
}

/// Whether the type asks to be painted after the other objects of its tile (`cDrawsLate`, read
/// from the plain characteristics whatever the subtype, true only for 1).
fn draws_late(ty: &EntityType) -> bool {
    ty.config
        .as_deref()
        .and_then(|c| c.section("Characteristics/Integers"))
        .and_then(|s| s.get("cDrawsLate"))
        .and_then(to_int)
        == Some(1)
}

/// The smallest box `[left, top, right, bottom)` that holds both.
fn union(a: [i64; 4], b: [i64; 4]) -> [i64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The size of the picture that holds `bounds`, or an error if it is empty or too big.
pub(crate) fn check_size(bounds: &[i64; 4]) -> Result<(i64, i64), RenderError> {
    let (w, h) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
    if w <= 0 || h <= 0 || w as u64 * h as u64 > MAX_PIXELS {
        return Err(RenderError(format!(
            "the picture would be {w}x{h} pixels, more than {MAX_PIXELS}"
        )));
    }
    Ok((w, h))
}

/// The view position at normal zoom of the corner of the tile (`tx`, `ty`) that its image origin
/// is measured from: the tile's corner 7 in plan, at the height of the screen-left corner
/// (map drawing 4.4).
fn corner_view(tile: &Tile, tx: i32, ty: i32, r: u8) -> (i32, i32) {
    let height = 16 * corner_elevation(tile, map_direction(r, 7));
    project(r, 64 * tx, 64 * ty, height)
}

/// The image origin of the tile (`tx`, `ty`) in view space at normal zoom (map drawing 4.4).
fn origin(tile: &Tile, tx: i32, ty: i32, r: u8) -> (i32, i32) {
    let (px, py) = corner_view(tile, tx, ty, r);
    let (dx, dy) = origin_offset(r, Zoom::Normal);
    (px + dx, py + dy)
}

/// The key of the picture of a path piece (placing 6.5): the tile's shape byte turned to the view
/// when the tile is sloped, else 100 plus a bit for each of the four sides, in view order, whose
/// neighbour holds a path.
fn path_key(shape: u8, rotation: u8, near: [bool; 4]) -> i32 {
    match i32::from(shape.rotate_right(2 * u32::from(rotation))) {
        0 => 100 + near.iter().fold(0, |key, &n| 2 * key + i32::from(n)),
        100 => 255,
        s => s,
    }
}

/// The index of the picture of a path piece (entity types 9.3, sprite drawing 3.6).
fn path_view(scene: &Scene, o: usize, ty: &EntityType) -> usize {
    let obj = &scene.objects[o];
    let Some(t) = scene.tile_at(obj.x, obj.y) else {
        return 0;
    };
    let near = [0, 1, 2, 3].map(|e| {
        let d = map_direction(scene.rotation, e * 2);
        scene
            .neighbour(t, d)
            .and_then(|n| scene.path_of(n))
            .is_some()
    });
    let code = path_key(scene.tiles[t].shape, scene.rotation, near);
    let codes: Vec<Option<i32>> = ty
        .config
        .as_deref()
        .and_then(|c| c.section("Shapes"))
        .map(|s| s.get_all("shape").iter().map(|v| to_int(v)).collect())
        .unwrap_or_default();
    shape_index(&codes, code)
}

/// The position of the picture with this key among the `shape` keys of a path type; the isolated
/// piece (100) for a key it lacks, the first picture for a type without it (placing 6.5).
fn shape_index(codes: &[Option<i32>], key: i32) -> usize {
    let find = |key: i32| codes.iter().position(|&c| c == Some(key));
    find(key).or_else(|| find(100)).unwrap_or(0)
}

/// Adds a ripple, a transient object at the surface, against every active wall of every tank that
/// holds clean water (map drawing 9.7.8); the number added. They are made at each staging, so
/// they come and go with the level and the purity.
fn add_ripples(scene: &mut Scene, looks: &[tank::Look], tiles: usize) -> usize {
    let mut added = 0;
    for (k, (tank, look)) in scene.tanks.clone().iter().zip(looks).enumerate() {
        if tank.level == 0 || !look.is_clean() {
            continue;
        }
        let inside: Vec<usize> = (0..tiles)
            .filter(|&t| scene.tank_of(t) == Some(k))
            .collect();
        for t in inside {
            for d in [0, 2, 4, 6] {
                let wall = scene
                    .edge_object(t, d)
                    .filter(|&w| scene.wall_tank(w) == Some(k))
                    .map(|w| scene.objects[w]);
                let Some(wall) = wall else { continue };
                let z = scene.surface(t).unwrap_or(0);
                let ripple = Object {
                    kind: Kind::Unit,
                    z,
                    footprint: (1, 1),
                    late: false,
                    height: 0,
                    ..wall
                };
                scene.add_transient(t, ripple);
                added += 1;
            }
        }
    }
    added
}
