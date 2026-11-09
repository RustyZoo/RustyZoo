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

mod object;
pub mod order;
mod pick;
mod placed;
mod shape;
mod stage;
pub(crate) use object::{fence_name, rest_name};
pub(crate) use shape::shape_of;
#[cfg(test)]
use stage::{add_ripples, bubbles};
mod staged;
mod tank;
#[cfg(test)]
pub(crate) mod tests;
mod tiles;

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
use placed::{Boxed, Flat, Pic, Placed};
pub use staged::Staged;

/// The most pixels of the picture; a 150 by 150 map with its relief needs about 55 million.
pub(crate) const MAX_PIXELS: u64 = 100_000_000;

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
pub(crate) struct Mark {
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
pub(crate) struct Extra {
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
    /// The entity (id) that carries the selection marker (sprite drawing 9.2).
    pub selected: Option<u32>,
    /// The form of the selected animal, which decides the footprint the marker is chosen by.
    pub form: Form,
    /// The animals' reactions to a change of their exhibit: the unit (id), whether it likes the
    /// change and the milliseconds since; each is a face above its unit (animals 10).
    pub faces: Vec<(u32, bool, u32)>,
    /// The tank filters that are switched off or show bubbles (tank staff 7.3, 7.5); a filter that
    /// is not here shows its health and no bubbles.
    pub filters: Vec<FilterLook>,
    /// The frame each animated fixed object shows (sprite drawing 6.2). It is not compared: a new
    /// frame is applied to the staged map by [`Staged::reframe`], without staging it again.
    pub idle: Frames,
    /// The animation of the buildings that show `used` or `idledead` (entity id); the others show
    /// `idle`.
    pub looks: HashMap<u32, &'static str>,
    /// The tank walls (ids, ascending) that connect a tank to a land exhibit (tank fauna 12.1).
    pub connectors: Vec<u32>,
}

/// The key of the clock of the bubbles of the filter `id` in [`Extra::idle`]: the filter's own
/// clock is under its id.
pub(crate) fn bubble_key(id: u32) -> u32 {
    id | 1 << 31
}

/// The key of the clock of the ripples against the tank wall `id` in [`Extra::idle`].
pub(crate) fn ripple_key(id: u32) -> u32 {
    id | 1 << 30
}

/// The frames of the idle animations by entity id; always equal to another, see [`Extra::idle`].
#[derive(Debug, Default, Clone)]
pub(crate) struct Frames(pub HashMap<u32, u32>);

impl PartialEq for Frames {
    fn eq(&self, _: &Frames) -> bool {
        true
    }
}

impl Eq for Frames {}

/// What a tank filter shows besides its health.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FilterLook {
    /// The id of the filter.
    pub id: u32,
    /// It is switched off: it faces no water.
    pub off: bool,
    /// The bubbles of its last cycle are shown on the tank side of its wall; their frame is that of
    /// the clock [`bubble_key`] names in [`Extra::idle`].
    pub bubbles: bool,
}

/// What an animal is, as far as its footprint goes: its plain one, its egg's or its crate's.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    #[default]
    Plain,
    Egg,
    Crate,
}

/// The animation and frame a unit of the zoo holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Shown {
    /// The id of the entity.
    pub id: u32,
    pub animation: String,
    /// The frame to draw.
    pub frame: u32,
}

/// An object that is not on the map: a pending object. It is painted like the temporary
/// entities of the tile under it (map drawing 9.5), tinted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ghost {
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
    /// The animation a pending egg or crate shows (unit placing 9.1); the idle one when none.
    pub anim: Option<&'static str>,
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

/// The animation of a fence piece of the row `row` (`idle`, `det`, `broke`) facing `facing` on
/// `tile`: the name of the row with `30p` when the second corner of the edge it stands on is
/// higher than the first, `30n` when it is lower (sprite drawing 3.6). The gates use it too.
pub(crate) fn slope_name(row: &str, tile: &crate::zoo::Tile, facing: u32) -> String {
    // The corners of the edge, in this order.
    let [first, second] = match facing / 2 {
        0 => [7, 1],
        1 => [1, 3],
        2 => [3, 5],
        _ => [5, 7],
    };
    match corner_elevation(tile, second).cmp(&corner_elevation(tile, first)) {
        std::cmp::Ordering::Greater => format!("{row}30p"),
        std::cmp::Ordering::Less => format!("{row}30n"),
        std::cmp::Ordering::Equal => row.to_string(),
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
    /// The pictures of the tiles of the last staging, for the next one.
    grounds: Option<Rc<Grounds>>,
}

/// The pictures of every tile of a staging and what they were made from: the next staging
/// takes them again while the ground, the edge objects and the marks are the same (a staging
/// in which only units moved).
struct Grounds {
    width: usize,
    rotation: u8,
    tiles: Vec<Tile>,
    /// Per tile, whether each edge slot holds an object.
    edges: Vec<[bool; 4]>,
    marks: HashMap<usize, [u8; 3]>,
    /// Per tile, its pictures with their boxes, and its counts.
    placed: Vec<(Vec<Boxed>, Summary)>,
}

/// An item of the paint order of a staging: the pictures of a tile, or one other picture.
#[derive(Clone, Copy)]
enum Entry {
    Tile(usize),
    Own(usize),
}

impl<'a> MapRenderer<'a> {
    /// The files of the game this renders from.
    pub(crate) fn vfs(&self) -> &'a Vfs {
        self.vfs
    }

    /// The entity types this renders with.
    pub(crate) fn catalogue(&self) -> &'a Catalogue {
        self.catalogue
    }

    /// How an entity takes its place on the map, if its type is known.
    pub fn shape(&self, entity: &crate::zoo::Entity) -> Option<Shape> {
        shape_of(self.catalogue, entity)
    }

    /// The colour choices a new building of the type starts with, one byte per colour group (sprite
    /// drawing 8.1): the position of the `defaultpal` of each group among its variants. Empty for a
    /// type that is not recoloured.
    pub(crate) fn default_colors(&self, ty: &EntityType) -> Vec<u8> {
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
            grounds: None,
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
        let member = crate::vfs::normalise(&crate::vfs::name_from_bytes(member));
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
    #[cfg(test)]
    pub(crate) fn render(
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
}

/// Whether two palette file names are the same member.
fn same_member(a: &[u8], b: &[u8]) -> bool {
    crate::vfs::normalise(&crate::vfs::name_from_bytes(a))
        == crate::vfs::normalise(&crate::vfs::name_from_bytes(b))
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
    /// The life of a fence or tank wall, or the health of a tank filter, if the record has one.
    life: Option<i32>,
    /// The movement mode of a unit: 3 inside a tank.
    mode: u32,
    /// A tank wall that is a portal.
    portal: bool,
    /// A tank wall that is the connector of a combined exhibit (tank fauna 12.4).
    connector: bool,
    /// The units left of a food entity.
    food: i32,
    /// A tank filter that is switched off.
    off: bool,
    /// The object is the bubbles of the tank filter whose clock this is: the key of the clock.
    clock: Option<u32>,
    /// The colour the object is drawn tinted with (sprite drawing 9.1).
    tint: Option<[u8; 3]>,
    /// The wall height of a pending tank gate, 0 for anything else.
    wall: i32,
    /// The animation and frame a unit holds.
    shown: Option<(&'a str, usize)>,
    /// The frame of the idle animation of a fixed object, if it has a clock.
    idle: Option<u32>,
    /// The animation of a building in use or dead.
    look: Option<&'static str>,
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
pub(crate) fn tank_kind(ty: &EntityType) -> TankKind {
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
        tall: flag("cTall"),
        life: crate::build::number(ty, "cLife").unwrap_or(crate::place::LIFE) as i32,
        strength: crate::build::number(ty, "cStrength").unwrap_or(crate::place::STRENGTH) as u32,
        depth: crate::build::number(ty, "cDepth").unwrap_or(0.0) as i32,
        height: crate::build::number(ty, "cHeight").unwrap_or(0.0) as i32,
        jumpable: crate::build::number(ty, "cIsJumpable").is_some_and(|n| n != 0.0),
        climbable: crate::build::number(ty, "cIsClimbable").is_some_and(|n| n != 0.0),
        electrified: crate::build::number(ty, "cIsElectrified").is_some_and(|n| n != 0.0),
        decayed: crate::build::number(ty, "cDecayedLife").unwrap_or(0.0) as i32,
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
