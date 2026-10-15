//! Shaping and painting the ground (terrain editing 3 to 8): the brush, the terrain types, the
//! rules that say which tiles accept a type, the session that snapshots every tile it touches and
//! the cost computed from the snapshot, accept and cancel. Shaping steps are in [`shape`].
//!
//! Nothing here knows the interface or the clock: [`Tool`] works on a [`World`] and the cash.

use crate::config::{Config, to_float, to_int};
use crate::entity::{Catalogue, EntityType};
use crate::money::{CONSTRUCTION, Money};
use crate::render::{Mark, corner_elevation};
use crate::timing::ini_value;
use crate::vfs::Vfs;
use crate::world::World;
use crate::zoo::{Entity, Tile};
use crate::zoom::Zoom;
use shape::Request;

pub mod shape;

/// The default price of one step of elevation (`[Map] elevationCost`).
const ELEVATION_COST: f32 = 8.0;
/// The most a price may be: a larger one is read as a typing mistake.
const MAX_PRICE: f32 = 1e9;
/// The brush sides.
pub const MIN_SIZE: usize = 1;
pub const MAX_SIZE: usize = 5;
/// The terrain types the list leaves out: waterfall, trampled ground and gunnite.
const UNLISTED: [u8; 3] = [12, 16, 17];
/// The type that guests make: shown in the paint tab (6.4).
const TRAMPLED: u8 = 16;

/// The colours of the highlights (map drawing 7).
const SELECTION: [u8; 3] = [0x51, 0xC0, 0x70];
const INVALID: [u8; 3] = [0xEC, 0, 0];
const TRAMPLED_COLOR: [u8; 3] = [0xFB, 0xC0, 0x0A];

/// What a modification of the ground is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Modify,
    Paint,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Modify => "modify",
            Mode::Paint => "paint",
        }
    }
}

/// One section of `terrain/tilete*.cfg` (map drawing 4.1, terrain editing 10).
#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub kind: u8,
    /// The price of giving a tile this type.
    pub cost: f32,
    /// `water` above 0: fresh or salt water.
    pub water: bool,
    /// The animation of the icon, and the string number of the type's name and help.
    pub icon: String,
    pub help: i32,
    /// Whether the paint list shows it.
    pub listed: bool,
}

/// The terrain types of the install, by type number. A type that two sections define keeps the
/// later one; a number above 255 is left out.
pub fn types(vfs: &Vfs) -> Vec<Type> {
    let mut members: Vec<String> = vfs
        .list(None)
        .into_iter()
        .filter(|m| {
            let m = m.to_ascii_lowercase();
            m.starts_with("terrain/tilete") && m.ends_with(".cfg")
        })
        .collect();
    members.sort();
    let mut found: Vec<Type> = Vec::new();
    for member in members {
        let Some(config) = vfs.read(&member).ok().and_then(|b| Config::parse(&b).ok()) else {
            continue;
        };
        for (_, section) in config.sections() {
            let Some(kind) = section
                .get("type")
                .and_then(to_int)
                .and_then(|k| u8::try_from(k).ok())
            else {
                continue;
            };
            let text = |key: &str| {
                section
                    .get(key)
                    .map_or(String::new(), |v| String::from_utf8_lossy(v).into_owned())
            };
            let new = Type {
                kind,
                cost: section.get("cost").map_or(0.0, price),
                water: section.get("water").and_then(to_int).unwrap_or(0) > 0,
                icon: text("icon"),
                help: section.get("helpid").and_then(to_int).unwrap_or(0),
                listed: !UNLISTED.contains(&kind),
            };
            match found.iter_mut().find(|t| t.kind == kind) {
                Some(old) => *old = new,
                None => found.push(new),
            }
        }
    }
    found.sort_by_key(|t| t.kind);
    found
}

/// A price as a number: 0 for one that is negative, not finite or above [`MAX_PRICE`].
fn price(value: &[u8]) -> f32 {
    let v = to_float(value);
    if (0.0..=MAX_PRICE).contains(&v) {
        v
    } else {
        0.0
    }
}

/// The price of one step of elevation: `[Map] elevationCost` of the ini file, 8 when it is
/// missing, negative or too large (terrain editing 8.1).
pub fn elevation_cost(ini: &[u8]) -> f32 {
    Config::parse(ini)
        .ok()
        .and_then(|c| {
            let value = to_float(ini_value(&c, "Map", "elevationCost")?);
            (0.0..=MAX_PRICE).contains(&value).then_some(value)
        })
        .unwrap_or(ELEVATION_COST)
}

/// What an object needs of the ground (the keys `cSwims` and `cOnlySwims`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Habit {
    pub swims: bool,
    pub only_swims: bool,
}

/// A tile of the map by its column and row.
pub type Cell = (u32, u32);

/// The tile under a window point of the world.
pub type Under<'a> = &'a dyn Fn(&World, (i32, i32)) -> Option<Cell>;

/// How the entities say what they need of the ground.
pub type Habits<'a> = &'a dyn Fn(&Entity) -> Habit;

/// What an entity needs of the ground, from its type; an entity of an unknown type does not swim.
pub fn habit(catalogue: &Catalogue, entity: &Entity) -> Habit {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    catalogue
        .find(
            &entity.class,
            &bytes(&entity.type_name),
            &bytes(&entity.subtype),
        )
        .map_or_else(Habit::default, type_habit)
}

/// What a type needs of the ground: `cSwims` and `cOnlySwims` of the plain characteristics (a
/// flag is on for the value 1 only). A type that swims and is neither a land nor an underwater
/// type (`cLand` is on when missing) lives in water alone. A type without characteristics does
/// not swim.
pub fn type_habit(ty: &EntityType) -> Habit {
    let Some(section) = ty
        .config
        .as_deref()
        .and_then(|c| c.section("Characteristics/Integers"))
    else {
        return Habit::default();
    };
    let flag = |key: &str| section.get(key).map(|v| to_int(v) == Some(1));
    let swims = flag("cSwims").unwrap_or(false);
    let land = flag("cLand").unwrap_or(true);
    let underwater = flag("cUnderwater").unwrap_or(false);
    Habit {
        swims,
        only_swims: flag("cOnlySwims").unwrap_or(false) || (swims && !underwater && !land),
    }
}

/// The top left tile of a brush of `size` tiles a side centred on `centre`, moved back inside a
/// map of `width` by `height` tiles when it sticks out (terrain editing 4.2). An even brush has
/// its centre tile at the upper left of its middle.
pub fn brush_origin(size: usize, centre: (i32, i32), width: u32, height: u32) -> (i32, i32) {
    let n = size.clamp(MIN_SIZE, MAX_SIZE) as i32;
    let lo = (n - 1) / 2;
    let fit = |c: i32, side: u32| {
        let room = i32::try_from(side)
            .unwrap_or(i32::MAX)
            .saturating_sub(n)
            .max(0);
        c.saturating_sub(lo).clamp(0, room)
    };
    (fit(centre.0, width), fit(centre.1, height))
}

/// The first state of each tile a session touched.
#[derive(Debug, Default)]
pub struct Session {
    touched: Vec<(usize, Tile)>,
}

impl Session {
    /// Records `tile` as tile `t` was, unless the session has touched it.
    pub fn touch(&mut self, t: usize, tile: Tile) {
        if !self.touched.iter().any(|&(n, _)| n == t) {
            self.touched.push((t, tile));
        }
    }

    /// The cost of the session, computed again from the snapshot (terrain editing 8.2): for each
    /// touched tile the largest change of a corner times the step price, and the price of the
    /// type it has now when that differs from its first.
    pub fn cost(&self, world: &World, elevation: f32, types: &[Type]) -> f32 {
        self.touched
            .iter()
            .filter_map(|&(t, old)| Some((old, world.zoo.tiles.get(t)?)))
            .map(|(old, now)| {
                let steps = [7, 1, 5, 3]
                    .into_iter()
                    .map(|c| (corner_elevation(now, c) - corner_elevation(&old, c)).abs())
                    .max()
                    .unwrap_or(0);
                let own = types
                    .iter()
                    .find(|t| t.kind == now.terrain)
                    .map_or(0.0, |t| t.cost);
                steps as f32 * elevation + if now.terrain == old.terrain { 0.0 } else { own }
            })
            .sum()
    }

    /// Puts every touched tile back and forgets them (terrain editing 8.5): elevations and shape
    /// always, the type only when the tile accepts it as it is now, before its shape is put back.
    pub fn restore(&mut self, world: &mut World, habits: Habits, types: &[Type]) {
        for (t, old) in std::mem::take(&mut self.touched) {
            let width = world.zoo.width as usize;
            let terrain = if accepts(world, habits, types, t, old.terrain) {
                old.terrain
            } else {
                world.zoo.tiles[t].terrain
            };
            world.set_tile(
                (t % width) as i32,
                (t / width) as i32,
                old.elevation,
                old.shape,
                terrain,
            );
        }
    }

    pub fn clear(&mut self) {
        self.touched.clear();
    }
}

/// What leaving the tool did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// There was nothing to pay or the session was paid.
    Kept,
    /// The cash did not cover the session: it was cancelled.
    Short,
}

/// The terrain tool: the brush, the paint type, the session.
pub struct Tool {
    types: Vec<Type>,
    elevation: f32,
    mode: Option<Mode>,
    /// The brush side and the paint type, kept between openings of the panel.
    size: usize,
    kind: u8,
    /// The tile under the pointer the brush was placed around, and the brush's top left tile.
    centre: Option<(i32, i32)>,
    brush: Option<(i32, i32)>,
    session: Session,
    /// The shaping mode: whether the ground is kept joined, and whether the brush is brought to a
    /// height instead of being dragged.
    contiguous: bool,
    level: bool,
    pressed: bool,
    /// The window point of the last pointer message of the stroke.
    last: Option<(i32, i32)>,
    /// Shaping: the window point the vertical drag is measured from, and the elevation the
    /// level modes bring the brush to.
    reference: (i32, i32),
    target: i32,
    /// What painting the brush where it is would cost; 0 right after a paint.
    brush_cost: f32,
    /// The tile whose help the paint tab last asked to show.
    helped: Option<Cell>,
}

impl Tool {
    /// The tool of a game: the types of the install and the price of a step.
    pub fn new(types: Vec<Type>, elevation: f32) -> Tool {
        Tool {
            types,
            elevation,
            mode: None,
            size: 2,
            kind: 0,
            centre: None,
            brush: None,
            session: Session::default(),
            contiguous: true,
            level: false,
            pressed: false,
            last: None,
            reference: (0, 0),
            target: 0,
            brush_cost: 0.0,
            helped: None,
        }
    }

    /// The mode while a terrain tab is the map view's tool.
    pub fn mode(&self) -> Option<Mode> {
        self.mode
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn kind(&self) -> u8 {
        self.kind
    }

    /// The shaping mode: whether the ground is kept joined, and whether the brush is brought to a
    /// height instead of being dragged.
    pub fn shaping(&self) -> (bool, bool) {
        (self.contiguous, self.level)
    }

    pub fn set_shaping(&mut self, contiguous: bool, level: bool) {
        (self.contiguous, self.level) = (contiguous, level);
    }

    /// The types the paint list shows, in order.
    pub fn listed(&self) -> Vec<&Type> {
        self.types.iter().filter(|t| t.listed).collect()
    }

    /// Every type of the install, by number.
    pub fn types(&self) -> &[Type] {
        &self.types
    }

    /// The type with this number.
    pub fn find(&self, kind: u8) -> Option<&Type> {
        self.types.iter().find(|t| t.kind == kind)
    }

    /// The tool becomes the map view's tool: the session starts empty (terrain editing 3.1).
    pub fn enter(&mut self, mode: Mode) {
        self.mode = Some(mode);
        self.session.clear();
        self.centre = None;
        self.brush = None;
        self.pressed = false;
        self.brush_cost = 0.0;
    }

    /// The tool is left: the session is accepted when the cash covers it, else cancelled
    /// (terrain editing 8.6).
    pub fn leave(
        &mut self,
        world: &mut World,
        habits: Habits,
        money: &mut Money,
        lines: &mut Vec<String>,
    ) -> Left {
        let cost = self.cost(world);
        let left = if money.can_pay(cost) {
            self.accept(world, habits, money, lines);
            Left::Kept
        } else {
            self.cancel(world, habits, lines);
            Left::Short
        };
        self.mode = None;
        self.centre = None;
        self.brush = None;
        self.pressed = false;
        left
    }

    /// The cost of the session.
    pub fn cost(&self, world: &World) -> f32 {
        self.session.cost(world, self.elevation, &self.types)
    }

    /// Pays the session and starts a new one; the cost of the brush is worked out again. A
    /// session that costs nothing is kept as it is (terrain editing 8.4).
    pub fn accept(
        &mut self,
        world: &World,
        habits: Habits,
        money: &mut Money,
        lines: &mut Vec<String>,
    ) -> bool {
        let cost = self.cost(world);
        if cost <= 0.0 || !money.charge(CONSTRUCTION, cost) {
            return false;
        }
        lines.push(format!("accept {}", whole(cost)));
        self.session.clear();
        self.refresh(world, habits);
        true
    }

    /// Takes the session back.
    pub fn cancel(&mut self, world: &mut World, habits: Habits, lines: &mut Vec<String>) {
        if self.session.touched.is_empty() {
            return;
        }
        lines.push(format!("cancel {}", whole(self.cost(world))));
        self.session.restore(world, habits, &self.types);
        self.refresh(world, habits);
    }

    /// Sets the brush side; refused outside 1 to 5.
    pub fn set_size(&mut self, size: usize) -> bool {
        if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
            return false;
        }
        self.size = size;
        true
    }

    /// Chooses the paint type; refused for a type that is not in the list.
    pub fn set_kind(&mut self, kind: u8) -> bool {
        match self.types.iter().any(|t| t.kind == kind && t.listed) {
            true => {
                self.kind = kind;
                true
            }
            false => false,
        }
    }

    /// The tiles of the brush, in rows.
    pub fn tiles(&self, world: &World) -> Vec<usize> {
        let Some((x0, y0)) = self.brush else {
            return Vec::new();
        };
        let n = self.size as i32;
        (y0..y0 + n)
            .flat_map(|y| (x0..x0 + n).map(move |x| (x, y)))
            .filter_map(|(x, y)| world.tile_index(x, y))
            .collect()
    }

    /// Whether tile `t` accepts the paint type (terrain editing 6.2).
    pub fn accepts(&self, world: &World, habits: Habits, t: usize) -> bool {
        accepts(world, habits, &self.types, t, self.kind)
    }

    /// Places the brush around the tile under the pointer and works out what painting it would
    /// cost; whether it moved. It does not move when the tile is off the map or is the brush's
    /// own centre already.
    pub fn place(&mut self, world: &World, habits: Habits, tile: Option<(u32, u32)>) -> bool {
        let Some(centre) =
            tile.and_then(|(x, y)| Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?)))
        else {
            return false;
        };
        let lo = (self.size as i32 - 1) / 2;
        if self.brush.is_some_and(|(x, y)| (x + lo, y + lo) == centre) {
            return false;
        }
        self.set_centre(world, habits, centre);
        true
    }

    fn set_centre(&mut self, world: &World, habits: Habits, centre: (i32, i32)) {
        self.centre = Some(centre);
        self.brush = Some(brush_origin(
            self.size,
            centre,
            world.zoo.width,
            world.zoo.height,
        ));
        self.brush_cost = self.price_of_brush(world, habits);
    }

    /// Places the brush again after its side or the paint type changed.
    pub fn refresh(&mut self, world: &World, habits: Habits) {
        if let Some(centre) = self.centre {
            self.set_centre(world, habits, centre);
        }
    }

    /// What painting the brush where it is costs: the price of the type for each tile that
    /// accepts it.
    pub fn brush_cost(&self) -> f32 {
        self.brush_cost
    }

    /// The amount written next to the pointer in the paint tab: minus what painting the brush
    /// where it is costs, 0 when nothing would be painted (terrain editing 7.3).
    pub fn pointer_amount(&self) -> Option<i64> {
        (self.mode == Some(Mode::Paint)).then(|| whole(self.brush_cost).saturating_neg())
    }

    /// The help string to show for the tile under the pointer: in the paint tab, button up, when
    /// the tile is not the one asked about last (which is never forgotten), that of its terrain
    /// type (terrain editing 7.3). `None` for no change, a tile off the map (it changes nothing) or
    /// a type without help.
    pub fn help_at(&mut self, world: &World, tile: Option<Cell>) -> Option<i32> {
        let tile = tile.filter(|_| self.mode == Some(Mode::Paint) && !self.pressed)?;
        if self.helped == Some(tile) {
            return None;
        }
        self.helped = Some(tile);
        let (x, y) = tile;
        let help = self.find(world.zoo.tile(x, y)?.terrain)?.help;
        (help > 0).then_some(help)
    }

    fn price_of_brush(&self, world: &World, habits: Habits) -> f32 {
        if self.mode != Some(Mode::Paint) {
            return 0.0;
        }
        let own = self.find(self.kind).map_or(0.0, |t| t.cost);
        let n = self
            .tiles(world)
            .into_iter()
            .filter(|&t| self.accepts(world, habits, t))
            .count();
        n as f32 * own
    }

    /// The left button went down on the map at the window point `at`, on tile `tile` (terrain
    /// editing 5.6 and 6.3): the stroke begins; painting, the brush paints at once; in a level
    /// mode it takes the height of the tile and makes a step at once.
    pub fn press(
        &mut self,
        world: &mut World,
        habits: Habits,
        at: (i32, i32),
        tile: Option<(u32, u32)>,
        lines: &mut Vec<String>,
    ) {
        world.begin();
        self.pressed = true;
        self.last = Some(at);
        self.reference = at;
        self.brush = None;
        self.place(world, habits, tile);
        if self.brush.is_none() {
            return;
        }
        if self.mode == Some(Mode::Paint) {
            self.paint(world, habits, lines);
        } else if self.level {
            let under = tile.and_then(|(x, y)| world.zoo.tile(x, y));
            self.target = under.map_or(0, |t| t.elevation);
            self.shape(world, &[Request::Level(self.target)], lines);
        }
    }

    /// The pointer moved to the window point `at`, over the map; `tiles` gives the tile under a
    /// window point, `zoom` the view's. Button up, the brush follows; held, painting paints along
    /// the way: when the pointer moved more than 16 pixels it is cut into 10 points, the new
    /// position first; shaping, the brush follows by the rules of its mode and the vertical drag
    /// makes steps.
    pub fn moved(
        &mut self,
        world: &mut World,
        habits: Habits,
        at: (i32, i32),
        tiles: Under,
        zoom: Zoom,
        lines: &mut Vec<String>,
    ) {
        if !self.pressed {
            self.place(world, habits, tiles(world, at));
            return;
        }
        let last = self.last.replace(at).unwrap_or(at);
        if self.mode == Some(Mode::Modify) {
            return self.drag(world, habits, at, tiles, zoom, lines);
        }
        let far = (at.0 - last.0).abs() > 16 || (at.1 - last.1).abs() > 16;
        let points: Vec<(i32, i32)> = match far {
            true => (0..10)
                .map(|k| {
                    let along =
                        |a: i32, b: i32| a + ((i64::from(b) - i64::from(a)) * k / 10) as i32;
                    (along(at.0, last.0), along(at.1, last.1))
                })
                .collect(),
            false => vec![at],
        };
        for point in points {
            if self.place(world, habits, tiles(world, point)) {
                self.paint(world, habits, lines);
            }
        }
    }

    /// One update of a shaping stroke (terrain editing 5.6): the brush follows the pointer when
    /// the mode says so, then the level modes make a step and the drag modes as many steps as
    /// the pointer travelled since the reference point, up for raising and down for lowering.
    fn drag(
        &mut self,
        world: &mut World,
        habits: Habits,
        at: (i32, i32),
        tiles: Under,
        zoom: Zoom,
        lines: &mut Vec<String>,
    ) {
        let gap = |a: i32, b: i32| i64::from(a) - i64::from(b);
        let (dx, dy) = (
            gap(at.0, self.reference.0).abs(),
            gap(at.1, self.reference.1).abs(),
        );
        let follows = match (self.level, self.contiguous) {
            (true, _) => dx > 15 || dy > 7,
            (false, true) => dx > 15,
            (false, false) => false,
        };
        if follows {
            self.reference.0 = at.0;
            if !self.level && dy > 32 {
                self.reference.1 = at.1;
            }
            self.place(world, habits, tiles(world, at));
        }
        if self.level {
            return self.shape(world, &[Request::Level(self.target)], lines);
        }
        let lo = (self.size as i32 - 1) / 2;
        let Some((x0, y0)) = self.brush else { return };
        let Some(e) = world
            .tile_index(x0 + lo, y0 + lo)
            .map(|t| i64::from(world.zoo.tiles[t].elevation))
        else {
            return;
        };
        let unit: i64 = match zoom {
            Zoom::Out => 8,
            Zoom::Normal => 16,
            Zoom::In => 32,
        };
        let limit = i64::from(shape::LIMIT);
        let up = gap(self.reference.1, at.1);
        let (d, request, room) = match up {
            up if up >= unit && e < limit => (up, Request::Raise, limit - e),
            up if -up >= unit && e > -limit => (-up, Request::Lower, e + limit),
            _ => return,
        };
        let steps = (d / unit + i64::from(d % unit > unit / 2))
            .min(room)
            .min(2 * limit);
        self.reference.1 = at.1;
        self.shape(world, &vec![request; steps as usize], lines);
    }

    /// Makes the steps one after the other over the brush; the number of tiles that changed and
    /// the cost of the session are told.
    fn shape(&mut self, world: &mut World, steps: &[Request], lines: &mut Vec<String>) {
        let before = self.cost(world);
        let brush = self.tiles(world);
        let width = world.zoo.width as usize;
        let mut changed = std::collections::BTreeSet::new();
        for &request in steps {
            for (t, elevation, shape) in
                shape::run(world, &self.types, &brush, self.contiguous, request)
            {
                let tile = world.zoo.tiles[t];
                self.session.touch(t, tile);
                let (x, y) = ((t % width) as i32, (t / width) as i32);
                world.set_tile(x, y, elevation, shape, tile.terrain);
                changed.insert(t);
            }
        }
        if !changed.is_empty() {
            lines.push(format!("shape {}", changed.len()));
        }
        let cost = self.cost(world);
        if cost != before {
            lines.push(format!("cost {}", whole(cost)));
        }
    }

    /// The button went up.
    pub fn release(&mut self) {
        self.pressed = false;
        self.last = None;
    }

    /// Gives the paint type to every tile of the brush that accepts it; the cost of the session
    /// is then told. Whether a tile changed.
    fn paint(&mut self, world: &mut World, habits: Habits, lines: &mut Vec<String>) -> bool {
        let before = self.cost(world);
        let width = world.zoo.width as usize;
        let mut painted = 0;
        for t in self.tiles(world) {
            if !self.accepts(world, habits, t) {
                continue;
            }
            let old = world.zoo.tiles[t];
            self.session.touch(t, old);
            world.set_tile(
                (t % width) as i32,
                (t / width) as i32,
                old.elevation,
                old.shape,
                self.kind,
            );
            painted += 1;
        }
        if painted > 0 {
            lines.push(format!("paint {painted}"));
            self.brush_cost = 0.0;
        }
        let cost = self.cost(world);
        if cost != before {
            lines.push(format!("cost {}", whole(cost)));
        }
        painted > 0
    }

    /// The highlights: the tiles of the brush, green for one the tool may change and red for one
    /// outside the zoo or, painting, one that does not accept the type unless it has it (terrain
    /// editing 7.2); painting, the trampled tiles of the map in their colour.
    pub fn marks(&self, world: &World, habits: Habits) -> Vec<Mark> {
        let Some(mode) = self.mode else {
            return Vec::new();
        };
        let brush = self.tiles(world);
        let mut marks: Vec<Mark> = match mode {
            Mode::Paint => world
                .zoo
                .tiles
                .iter()
                .enumerate()
                .filter(|(t, tile)| tile.terrain == TRAMPLED && !brush.contains(t))
                .map(|(tile, _)| Mark {
                    tile,
                    color: TRAMPLED_COLOR,
                })
                .collect(),
            Mode::Modify => Vec::new(),
        };
        marks.extend(brush.iter().map(|&t| {
            let valid = world.in_zoo(t)
                && (mode == Mode::Modify
                    || self.accepts(world, habits, t)
                    || world.zoo.tiles[t].terrain == self.kind);
            Mark {
                tile: t,
                color: if valid { SELECTION } else { INVALID },
            }
        }));
        marks
    }
}

/// A price in whole units, as the log tells it.
fn whole(amount: f32) -> i64 {
    crate::text::format::whole_units(f64::from(amount))
}

/// Whether tile `t` accepts the terrain type `kind` (terrain editing 6.2): it is inside the zoo,
/// it does not have the type, and a change between land and water has its conditions: no path and
/// no animal, guest or staff member on the tile; water into land: no object in the four quarter
/// slots or on the four sides lives in water alone; land into water: the tile is flat and every
/// object in the quarter slots swims (a fence on a side never stops water).
pub fn accepts(world: &World, habits: Habits, types: &[Type], t: usize, kind: u8) -> bool {
    let Some(tile) = world.zoo.tiles.get(t) else {
        return false;
    };
    if !world.in_zoo(t) || tile.terrain == kind {
        return false;
    }
    let water = |kind: u8| types.iter().any(|ty| ty.kind == kind && ty.water);
    let habit = |e: usize| habits(&world.zoo.entities[e]);
    let slots = || (0..4).filter_map(|q| world.quadrant(t, q)).map(habit);
    let sides = || {
        [0, 2, 4, 6]
            .into_iter()
            .filter_map(|d| world.edge(t, d))
            .map(habit)
    };
    let busy = || world.path(t).is_some() || world.unit_on(t);
    match (water(tile.terrain), water(kind)) {
        (true, false) => !slots().chain(sides()).any(|h| h.only_swims) && !busy(),
        (false, true) => tile.shape == 0 && slots().all(|h| h.swims) && !busy(),
        _ => true,
    }
}

#[cfg(test)]
mod tests;
