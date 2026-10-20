//! Placing and removing objects, paths and fences: so far the pending object, the type the player
//! chose as it follows the mouse (placing 4 and 5), the placing of scenery (placing 6.1) and the
//! laying of paths and fences by dragging (placing 5.3, 6.2 and 6.3).
//!
//! [`pending`] snaps the world point under the mouse to where the type would stand, lifts it to
//! the ground, turns it, and checks the place: the verdict decides the tint, the cost text and
//! the message a click shows. The checks are the ones every kind of object shares (the tile is in
//! the zoo, the quadrants of the footprint are free except for paths, a path or a fence replaces
//! a piece), those of scenery (land or water, flat ground at one height, no fence inside a big
//! footprint), those of paths (flat or one raised side, dirt painted first on water) and those of
//! fences (the zoo wall stays, nothing crosses the side, the smart fences test, not the entrance).
//! [`place`] adds a valid pending object to the world and takes its price. A drag is a list of
//! pending pieces, [`path_run`] and [`fence_run`] say where and [`afford`] which of them the cash
//! covers. The gate tool is in [`gate`].

pub mod bulldozer;
pub mod gate;
pub mod unit;

use crate::build::{cost, number};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::money::{CONSTRUCTION, Money, Symbol};
use crate::render::order::{Kind, ground_height};
use crate::render::{Extra, Ghost};
use crate::terrain::{self, Habits};
use crate::text::format::{money, whole_units};
use crate::timing::ini_whole;
use crate::ui::layout::color;
use crate::vfs::Vfs;
use crate::world::{Shape, World};
use crate::zoo::{Common, Entity, Fence};

/// The message of a place outside the zoo wall (or off the map).
pub const NOT_IN_THE_ZOO: i32 = 10311;
/// The message of an object that cannot be placed because another one stands in the way.
pub const IN_THE_WAY: i32 = 10555;
/// The message of an object that is not a land object on a land tile.
pub const NOT_ON_WATER: i32 = 10564;
/// The message of a purchase the cash does not cover.
pub const NO_FUNDS: i32 = 10002;
/// The message of a tank filter that is not next to a finished tank without a filter.
pub const NO_TANK: i32 = 10517;

/// The `cLife` of a fence type that has none.
pub(crate) const LIFE: f32 = 10.0;
/// The `cStrength` of a fence type that has none.
pub(crate) const STRENGTH: f32 = 500.0;

/// The biggest placement footprint side taken, in half tiles (the paint order drops a bigger one).
const MAX_FOOTPRINT: i32 = 64;

/// The tints of the pending object (sprite drawing 9.1), keys of the map view element of
/// `ui/gamescrn.lyt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tints {
    pub valid: [u8; 3],
    pub overwrite: [u8; 3],
    pub invalid: [u8; 3],
    /// The tint of a gate that is valid but where no keeper is sure to pass (placing 6.4).
    pub highlight: [u8; 3],
    /// The tint of what the bulldozer would remove.
    pub delete: [u8; 3],
}

impl Default for Tints {
    /// The shipped values.
    fn default() -> Tints {
        Tints {
            valid: [0, 225, 0],
            overwrite: [225, 112, 0],
            invalid: [225, 0, 0],
            highlight: [225, 225, 0],
            delete: [225, 0, 0],
        }
    }
}

impl Tints {
    /// The colours the install's `ui/gamescrn.lyt` gives, the shipped ones for a missing key.
    pub fn load(vfs: &Vfs) -> Tints {
        let config = vfs
            .read("ui/gamescrn.lyt")
            .ok()
            .and_then(|bytes| Config::parse(&bytes).ok())
            .unwrap_or_default();
        let view = config.section("MapView");
        let key =
            |name: &str, default: [u8; 3]| view.and_then(|s| color(s, name)).unwrap_or(default);
        let shipped = Tints::default();
        Tints {
            valid: key("entityvalidcolor", shipped.valid),
            overwrite: key("entityoverwritecolor", shipped.overwrite),
            invalid: key("entityinvalidcolor", shipped.invalid),
            highlight: key("entityhighlightcolor", shipped.highlight),
            delete: key("entitydeletecolor", shipped.delete),
        }
    }
}

/// Which terrain types are water, by type number.
pub fn water(types: &[terrain::Type]) -> Vec<bool> {
    let mut wet = vec![
        false;
        types
            .iter()
            .map(|t| usize::from(t.kind) + 1)
            .max()
            .unwrap_or(0)
    ];
    for t in types {
        wet[usize::from(t.kind)] = t.water;
    }
    wet
}

/// How a type is placed: by the way its class takes its place on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Holds the quadrants of its footprint: scenery, buildings and the rest.
    Fixed,
    /// The path piece of a tile.
    Path,
    /// A side of a tile: fences and tank walls.
    Fence,
}

impl Class {
    pub fn of(ty: &EntityType) -> Class {
        match Kind::of_class(ty.class) {
            Kind::Path => Class::Path,
            Kind::Edge | Kind::Wall => Class::Fence,
            _ => Class::Fixed,
        }
    }
}

/// What the place under the mouse is worth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Valid,
    /// Valid, and it replaces a piece (a path on a path, a fence on a held slot).
    Overwrite,
    /// A gate that is valid but shown in the highlight colour (placing 6.4).
    Highlight,
    Invalid,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Valid => "valid",
            Verdict::Overwrite => "overwrite",
            Verdict::Highlight => "highlight",
            Verdict::Invalid => "invalid",
        }
    }
}

/// The pending object: the chosen type at its snapped place.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// The type, a position in the catalogue.
    pub ty: usize,
    /// World position, snapped, and the ground height there.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Direction 0 to 7: the side of a fence, the facing of an object, 0 for a path.
    pub facing: u32,
    /// The tile under the snapped point and its quadrant there (`qy * 2 + qx`).
    pub tile: (i32, i32),
    pub quadrant: usize,
    pub verdict: Verdict,
    /// The message the first failed check stores, shown when the player clicks.
    pub message: Option<i32>,
    /// What the pending object costs, 0 for the replacement of a piece of the same type; for a
    /// path on water it includes the painting of the tile.
    pub price: f32,
    /// The terrain type a path on water paints the tile with before it is laid.
    pub paint: Option<u8>,
    /// The objects in the way (positions in `zoo.entities`), tinted red until the next move.
    pub blocking: Vec<usize>,
    /// The wall height a pending gate over a tank wall is drawn with (placing 6.4); 0 for any
    /// other object.
    pub wall: u32,
}

impl Pending {
    /// What the cost text shows: minus the cost when the place is valid, 0 otherwise (placing 4.5).
    pub fn amount(&self) -> i64 {
        match self.verdict {
            Verdict::Invalid => 0,
            _ => whole_units(f64::from(self.price)).saturating_neg(),
        }
    }

    /// The transient items the staging draws for it: the object itself, tinted by the verdict,
    /// and the objects in its way tinted red.
    pub fn extra(&self, tints: &Tints) -> Extra {
        extra_of(std::slice::from_ref(self), tints)
    }
}

/// The transient items of a list of pending pieces (a drag is one piece per tile).
pub fn extra_of(pieces: &[Pending], tints: &Tints) -> Extra {
    let mut extra = Extra::default();
    for p in pieces {
        let tint = match p.verdict {
            Verdict::Valid => tints.valid,
            Verdict::Overwrite => tints.overwrite,
            Verdict::Highlight => tints.highlight,
            Verdict::Invalid => tints.invalid,
        };
        extra.ghosts.push(Ghost {
            ty: p.ty,
            x: p.x,
            y: p.y,
            z: p.z,
            facing: p.facing,
            tint,
            wall: p.wall,
        });
        for &e in &p.blocking {
            if !extra.tinted.iter().any(|t| t.0 == e) {
                extra.tinted.push((e, tints.invalid));
            }
        }
    }
    extra
}

/// The cost text of a drag: minus the sum of the pieces that will be bought (placing 4.5).
pub fn run_amount(pieces: &[Pending]) -> i64 {
    let sum: f32 = pieces
        .iter()
        .filter(|p| p.verdict != Verdict::Invalid)
        .map(|p| p.price)
        .sum();
    whole_units(f64::from(sum)).saturating_neg()
}

/// Walks the pieces of a drag in order, adding up the cost of the valid ones: a valid piece that
/// would bring the sum above the cash is not valid (it will not be bought) and stores no message
/// (placing 6.2 items 3 and 4).
pub fn afford(pieces: &mut [Pending], money: &Money) {
    let mut sum = 0.0;
    for p in pieces.iter_mut().filter(|p| p.verdict != Verdict::Invalid) {
        if money.can_pay(sum + p.price) {
            sum += p.price;
        } else {
            p.verdict = Verdict::Invalid;
        }
    }
}

/// Which way the first leg of a path drag runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    X,
    Y,
}

/// The tiles of the L a path drag lays from the start tile to the tile under the mouse, the start
/// first (placing 6.2). `leg` is the leg mode of the drag: it is cleared while the mouse is on the
/// start tile and chosen by the first other tile the mouse enters, x when the move is longer along
/// x, else y.
pub fn path_run(start: (i32, i32), cell: (i32, i32), leg: &mut Option<Leg>) -> Vec<(i32, i32)> {
    let (dx, dy) = (cell.0 - start.0, cell.1 - start.1);
    if (dx, dy) == (0, 0) {
        *leg = None;
        return vec![start];
    }
    let first = *leg.get_or_insert(if dx.abs() > dy.abs() { Leg::X } else { Leg::Y });
    let mut run = vec![start];
    let mut at = start;
    let mut walk = |by: (i32, i32), n: i32| {
        for _ in 0..n {
            at = (at.0 + by.0, at.1 + by.1);
            run.push(at);
        }
    };
    let (sx, sy) = (dx.signum(), dy.signum());
    match first {
        Leg::X => {
            walk((sx, 0), dx.abs());
            walk((0, sy), dy.abs());
        }
        Leg::Y => {
            walk((0, sy), dy.abs());
            walk((sx, 0), dx.abs());
        }
    }
    run
}

/// The middle of each side of a tile, by map direction 0, 2, 4, 6 (placing 4.2).
pub const MIDDLE: [(i32, i32); 4] = [(32, 0), (63, 32), (32, 63), (0, 32)];

/// The pieces `(tile, side)` of the L a fence drag lays, the start piece first (placing 6.2): the
/// first leg runs along the side `first` chosen at the press on the tile `start`, the second
/// along the side the mouse's position `mouse` (a world point on the map) selects, with the shift
/// and the corner piece of the table of 6.2. A point far off the map is brought near it, so the
/// run stays finite.
pub fn fence_run(start: (i32, i32), first: u32, mouse: (i64, i64)) -> Vec<((i32, i32), u32)> {
    let first = first / 2 % 4 * 2;
    let near = |v: i64| v.clamp(-(1 << 20), 1 << 20);
    let (mx, my) = (near(mouse.0), near(mouse.1));
    let (lx, ly) = (mx.rem_euclid(64), my.rem_euclid(64));
    let cell = (mx.div_euclid(64), my.div_euclid(64));
    let (px, py) = (
        i64::from(start.0) * 64 + i64::from(MIDDLE[first as usize / 2].0),
        i64::from(start.1) * 64 + i64::from(MIDDLE[first as usize / 2].1),
    );
    // The side of the second leg, and the tile the mouse is taken to be on (the table of 6.2).
    let (a, b) = match first {
        0 => (mx < px, my < py),
        4 => (mx > px, my > py),
        6 => (my < py, mx < px),
        _ => (my > py, mx > px),
    };
    let (second, moved) = match (first, a, b) {
        (0, true, true) => (2, (-1, 0)),
        (0, true, false) => (6, (0, 0)),
        (0, false, true) => (6, (1, 0)),
        (0, false, false) => (2, (0, 0)),
        (4, true, true) => (6, (1, 0)),
        (4, true, false) => (2, (0, 0)),
        (4, false, true) => (2, (-1, 0)),
        (4, false, false) => (6, (0, 0)),
        (6, true, true) => (4, (0, -1)),
        (6, true, false) => (0, (0, 0)),
        (6, false, true) => (0, (0, 1)),
        (6, false, false) => (4, (0, 0)),
        (_, true, true) => (0, (0, 1)),
        (_, true, false) => (4, (0, 0)),
        (_, false, true) => (4, (0, -1)),
        (_, false, false) => (0, (0, 0)),
    };
    let (dx, dy) = (
        cell.0 + moved.0 - i64::from(start.0),
        cell.1 + moved.1 - i64::from(start.1),
    );
    let (sx, sy) = (dx.signum(), dy.signum());
    let mut run = vec![(start, first)];
    let mut at = (i64::from(start.0), i64::from(start.1));
    // `n` pieces of `side`, each one tile on from the last.
    fn walk(
        run: &mut Vec<((i32, i32), u32)>,
        at: &mut (i64, i64),
        by: (i64, i64),
        n: i64,
        side: u32,
    ) {
        for _ in 0..n {
            *at = (at.0 + by.0, at.1 + by.1);
            run.push(((at.0 as i32, at.1 as i32), side));
        }
    }
    if first.is_multiple_of(4) {
        let shift = (second == 6 && dx > 0) || (second == 2 && dx < 0);
        let corner = match first {
            0 => sy > 0 || (dy == 0 && ly > 32),
            _ => sy < 0 || (dy == 0 && ly < 32),
        };
        walk(
            &mut run,
            &mut at,
            (sx, 0),
            dx.abs() - i64::from(shift),
            first,
        );
        if shift {
            at.0 += sx;
        }
        if corner {
            run.push(((at.0 as i32, at.1 as i32), second));
        }
        walk(&mut run, &mut at, (0, sy), dy.abs(), second);
    } else {
        let shift = (second == 4 && dy < 0) || (second == 0 && dy > 0);
        let corner = match first {
            2 => sx < 0 || (dx == 0 && lx < 32),
            _ => sx > 0 || (dx == 0 && lx > 32),
        };
        walk(
            &mut run,
            &mut at,
            (0, sy),
            dy.abs() - i64::from(shift),
            first,
        );
        if shift {
            at.1 += sy;
        }
        if corner {
            run.push(((at.0 as i32, at.1 as i32), second));
        }
        walk(&mut run, &mut at, (sx, 0), dx.abs(), second);
    }
    run
}

/// Whether a tile of this shape byte takes a path: flat, or with one side raised (placing 5.3).
pub(crate) fn path_shape(shape: u8) -> bool {
    matches!(shape, 0x00 | 0x05 | 0x14 | 0x41 | 0x50)
}

/// What painting a tile with dirt would be, for the path check on water: the terrain type and the
/// price, or `None` where the terrain tool would refuse (placing 5.3; dirt is type 3, or type 0
/// when fewer than 4 types are loaded).
pub fn dirt<'a>(
    world: &'a World,
    types: &'a [terrain::Type],
    habits: Habits<'a>,
) -> impl Fn(usize) -> Option<(u8, f32)> + 'a {
    let kind = if types.len() < 4 { 0 } else { 3 };
    move |t| {
        let price = types.iter().find(|ty| ty.kind == kind)?.cost;
        terrain::accepts(world, habits, types, t, kind).then_some((kind, price))
    }
}

/// A switch of the `[UI]` section of the ini file: on when missing or not a number, off for 0.
fn ui_switch(ini: &[u8], key: &str) -> bool {
    Config::parse(ini)
        .ok()
        .and_then(|c| ini_whole(&c, "UI", key))
        .is_none_or(|n| n != 0)
}

/// `[UI] smartFences` of the ini file, the test of fences against large objects (placing 6.3
/// rule 5).
pub fn smart_fences(ini: &[u8]) -> bool {
    ui_switch(ini, "smartFences")
}

/// `[UI] confirmSell` of the ini file, the question before a type that needs one is removed
/// (placing 7.1).
pub fn confirm_sell(ini: &[u8]) -> bool {
    ui_switch(ini, "confirmSell")
}

/// The text of the cost at the pointer: an amount in whole units without grouping, `($25)` for
/// minus 25 in US English (placing 4.5). `symbol` is string 22100.
pub fn cost_text(symbol: String, amount: i64) -> String {
    money(&Symbol::new(symbol), amount, false)
}

/// The map direction a pending object faces for the step `step` of its panel when the view is
/// turned by `view` (placing 4.4): it keeps its facing on screen, `2 * step - 4`, so a turn of the
/// view turns it on the map.
pub fn facing(step: usize, view: u8) -> u32 {
    (2 * (step as i32 % 4 - i32::from(view % 4)) - 4).rem_euclid(8) as u32
}

/// The side of a tile (0, 2, 4 or 6) that holds the point (`lx`, `ly`) of the tile, 0 to 63: the
/// two diagonals cut the tile in four triangles (placing 4.2.1).
pub fn side(lx: i32, ly: i32) -> u32 {
    if lx < ly {
        if lx + ly <= 64 { 6 } else { 4 }
    } else if lx + ly <= 64 {
        0
    } else {
        2
    }
}

/// The world position the pending object takes for the world point (`px`, `py`): `fp` is its
/// placement footprint after the turn, `facing` the side of a fence (placing 4.2).
pub fn snap(class: Class, (px, py): (i32, i32), fp: (i32, i32), facing: u32) -> (i32, i32) {
    let (tx, ty) = (px.div_euclid(64), py.div_euclid(64));
    let (lx, ly) = (px.rem_euclid(64), py.rem_euclid(64));
    let (x, y) = match class {
        Class::Path => (32, 32),
        Class::Fence => MIDDLE[(facing / 2 % 4) as usize],
        Class::Fixed => {
            let axis = |local: i32, f: i32| match f {
                1 => 32 * (local / 32) + 16,
                f if f / 2 % 2 != 0 => 32,
                _ => 0,
            };
            (axis(lx, fp.0), axis(ly, fp.1))
        }
    };
    (
        tx.wrapping_mul(64).wrapping_add(x),
        ty.wrapping_mul(64).wrapping_add(y),
    )
}

/// `cPlacementFootprintX/Y` in half tiles, the footprint when the type has none.
fn placement_footprint(ty: &EntityType) -> (i32, i32) {
    let read = |key: &str, default: i32| {
        number(ty, key)
            .filter(|n| n.abs() < 1e6)
            .map_or(default, |n| n as i32)
    };
    (
        read("cPlacementFootprintX", ty.footprint[0]),
        read("cPlacementFootprintY", ty.footprint[1]),
    )
}

/// Whether the type only lives in water: `cOnlySwims` 1, or `cSwims` 1 with neither `cUnderwater`
/// nor `cLand` 1 (placing 5.2).
fn only_swims(ty: &EntityType) -> bool {
    let flag = |key: &str, default: bool| number(ty, key).map_or(default, |n| n == 1.0);
    flag("cOnlySwims", false)
        || (flag("cSwims", false) && !flag("cLand", true) && !flag("cUnderwater", false))
}

/// Whether the entity is of the type.
fn is_of(entity: &crate::zoo::Entity, ty: &EntityType) -> bool {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    entity.class == ty.class
        && bytes(&entity.type_name) == ty.type_name
        && bytes(&entity.subtype) == ty.subtype
}

/// The pending object of the type `ty` (a position in the catalogue) with the rotation step
/// `step` of its panel, for the world point `point` under the mouse, the view turned by `view`.
/// `None` when the type is unknown or the point is off the map. `water` tells for each terrain
/// type whether it is water, `dirt` what painting a tile with dirt would be (see [`dirt`]) and
/// `smart` whether the smart fences test is on (see [`smart_fences`]).
#[allow(clippy::too_many_arguments)]
pub fn pending(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    dirt: &dyn Fn(usize) -> Option<(u8, f32)>,
    smart: bool,
    ty: usize,
    step: usize,
    view: u8,
    point: (i64, i64),
) -> Option<Pending> {
    let t = cat.types.get(ty)?;
    let class = Class::of(t);
    let point = (i32::try_from(point.0).ok()?, i32::try_from(point.1).ok()?);
    let (tile_x, tile_y) = (point.0.div_euclid(64), point.1.div_euclid(64));
    let tile = world.tile_index(tile_x, tile_y)?;
    let dir = match class {
        Class::Fixed => facing(step, view),
        Class::Path => 0,
        Class::Fence => side(point.0.rem_euclid(64), point.1.rem_euclid(64)),
    };
    let own = placement_footprint(t);
    // The footprint is along the object's own axes; it turns with the facing.
    let fp = match class == Class::Fixed && dir % 4 != 0 {
        true => (own.1, own.0),
        false => own,
    };
    let (x, y) = snap(class, point, fp, dir);
    let (lx, ly) = (x - 64 * tile_x, y - 64 * tile_y);
    let mut pending = Pending {
        ty,
        x,
        y,
        z: ground_height(&world.zoo.tiles[tile], lx, ly),
        facing: dir,
        tile: (tile_x, tile_y),
        quadrant: (ly / 32 * 2 + lx / 32).clamp(0, 3) as usize,
        verdict: Verdict::Valid,
        message: None,
        price: cost(t),
        paint: None,
        blocking: Vec::new(),
        wall: 0,
    };
    check(&mut pending, world, cat, water, dirt, smart, class, fp);
    Some(pending)
}

/// The checks every object shares (placing 5.1, 5.2, 5.3): the first failure keeps its message.
#[allow(clippy::too_many_arguments)]
fn check(
    p: &mut Pending,
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    dirt: &dyn Fn(usize) -> Option<(u8, f32)>,
    smart: bool,
    class: Class,
    fp: (i32, i32),
) {
    let t = &cat.types[p.ty];
    let (tx, ty) = p.tile;
    let Some(tile) = world.tile_index(tx, ty) else {
        return fail(p, Some(NOT_IN_THE_ZOO));
    };
    if !world.in_zoo(tile) {
        return fail(p, Some(NOT_IN_THE_ZOO));
    }
    // A tank filter goes next to a tank that no filter serves, facing it (placing 5.1).
    if t.class == "filter" {
        match world.filter_facing(tile, p.facing) {
            Some(d) => p.facing = d,
            None => return fail(p, Some(NO_TANK)),
        }
    }
    let same = |old: usize| {
        let same = world.zoo.entities.get(old).is_some_and(|e| is_of(e, t));
        (Verdict::Overwrite, if same { 0.0 } else { cost(t) })
    };
    if class == Class::Fence {
        return check_fence(p, world, cat, water, smart, tile);
    }
    if class == Class::Path
        && let Some(old) = world.path(tile)
    {
        (p.verdict, p.price) = same(old);
    }
    if class == Class::Path {
        // The only checks of a path (placing 5.3): flat or one side raised, and on water only
        // where dirt could be painted first. No footprint, nothing in the way: it goes under
        // objects.
        if !path_shape(world.zoo.tiles[tile].shape) {
            fail(p, None);
        }
        let wet = water
            .get(usize::from(world.zoo.tiles[tile].terrain))
            .copied()
            .unwrap_or(false);
        if wet {
            match dirt(tile) {
                Some((kind, paint)) => {
                    p.paint = Some(kind);
                    p.price += paint;
                }
                None => fail(p, None),
            }
        }
        return;
    }
    // The quadrants of the footprint (map drawing 9.3.1): all on the map, in the zoo and free.
    if !(1..=MAX_FOOTPRINT).contains(&fp.0) || !(1..=MAX_FOOTPRINT).contains(&fp.1) {
        return fail(p, None);
    }
    let wet = |at: usize| {
        water
            .get(usize::from(world.zoo.tiles[at].terrain))
            .copied()
            .unwrap_or(false)
    };
    let (x0, y0) = (p.x - 16 * (fp.0 - 1), p.y - 16 * (fp.1 - 1));
    // The quadrants on the map and in the zoo; the others fail without a message (only the
    // object's own tile has one, above).
    let mut quads = Vec::new();
    for j in 0..fp.1 {
        for i in 0..fp.0 {
            let qx = (x0 + 32 * i).div_euclid(32);
            let qy = (y0 + 32 * j).div_euclid(32);
            match world.tile_index(qx.div_euclid(2), qy.div_euclid(2)) {
                Some(at) if world.in_zoo(at) => {
                    quads.push((at, (qy.rem_euclid(2) * 2 + qx.rem_euclid(2)) as usize));
                }
                _ => fail(p, None),
            }
        }
    }
    // Land or water (placing 5.4, 5.2 item 2). A type that is not a land type stands on
    // water only, which says so (10564) before anything else of the footprint; a type that
    // only swims fails on land, one that does not swim in water, both without a message.
    let flag = |key: &str, default: bool| number(t, key).map_or(default, |n| n == 1.0);
    let swims = flag("cSwims", false);
    for &(at, _) in &quads {
        // The tiles of a tank count as water for the types that live in it (placing 5.4).
        let tank = world.tank_at(at).is_some();
        if !flag("cLand", true) && !wet(at) && !tank {
            fail(p, Some(NOT_ON_WATER));
        }
        if (only_swims(t) && !wet(at) && !tank) || (!only_swims(t) && !swims && wet(at)) {
            fail(p, None);
        }
    }
    for &(at, quadrant) in &quads {
        if let Some(held) = world.quadrant(at, quadrant) {
            fail(p, Some(IN_THE_WAY));
            if !p.blocking.contains(&held) {
                p.blocking.push(held);
            }
        }
    }
    let tiles: Vec<usize> = quads.iter().map(|&(at, _)| at).collect();
    // Units on a tile of the footprint whose rectangle meets the object's are in the way (item 4);
    // none is, for a walkable type. A unit's rectangle is its footprint centred on it, both axes
    // the larger one at an odd rotation, the axes swapped at the other quarter turn. The helicopter
    // lets the object through (it moves in the air); recommended: nobody else does.
    let units: Vec<(usize, i32, i32, (i32, i32))> = world
        .zoo
        .entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let (c, shape) = (e.common.as_ref()?, world.shape(i)?);
            let on = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
            let (fx, fy) = shape.footprint;
            let rect = match c.rotation % 4 {
                0 => (fx, fy),
                _ if c.rotation % 2 != 0 => (fx.max(fy), fx.max(fy)),
                _ => (fy, fx),
            };
            let unit = shape.kind == Kind::Unit && tiles.contains(&on);
            unit.then_some((i, c.x, c.y, rect))
        })
        .collect();
    let meets = |centre: i32, own: i32, other: i32, reach: i32| {
        i64::from((centre - other).abs()) < i64::from(own) + i64::from(reach)
    };
    for &(i, x, y, (fx, fy)) in &units {
        if !flag("walkable", false)
            && world.zoo.entities[i].class != "helicopter"
            && meets(p.x, 16 * fp.0, x, 16 * fx)
            && meets(p.y, 16 * fp.1, y, 16 * fy)
        {
            fail(p, Some(IN_THE_WAY));
            p.blocking.push(i);
        }
    }
    // More than a quarter tile needs flat ground at one height (item 5), more than a tile no
    // fence and no cliff on the sides inside it (item 6).
    if ((fp.0 > 1 || fp.1 > 1) && uneven(world, &tiles))
        || ((fp.0 > 2 || fp.1 > 2) && fenced(world, x0, y0, fp))
    {
        fail(p, None);
    }
    // A place refused for any reason collects every unit on its tiles, whether its rectangle meets
    // the object's or not (10555 unless a message was stored before).
    if p.verdict == Verdict::Invalid {
        for &(i, ..) in &units {
            if !p.blocking.contains(&i) {
                fail(p, Some(IN_THE_WAY));
                p.blocking.push(i);
            }
        }
    }
}

/// Whether a tile of `tiles` is not flat or not at the height of the first (placing 5.2 item 5).
fn uneven(world: &World, tiles: &[usize]) -> bool {
    let tile = |t: usize| world.zoo.tiles[t];
    tiles.first().is_some_and(|&first| {
        tiles
            .iter()
            .any(|&t| tile(t).shape != 0 || tile(t).elevation != tile(first).elevation)
    })
}

/// Whether a tile side inside the footprint of `fp` quarters whose first quarter is at
/// `(x0, y0)` holds a fence or is a cliff (placing 5.2 item 6).
fn fenced(world: &World, x0: i32, y0: i32, fp: (i32, i32)) -> bool {
    let (tx0, ty0) = (x0.div_euclid(64), y0.div_euclid(64));
    let tx1 = (x0 + 32 * (fp.0 - 1)).div_euclid(64);
    let ty1 = (y0 + 32 * (fp.1 - 1)).div_euclid(64);
    (ty0..=ty1).any(|ty| {
        (tx0..=tx1).any(|tx| {
            [(2, 1, 0), (4, 0, 1)].into_iter().any(|(d, dx, dy)| {
                let inside = tx + dx <= tx1 && ty + dy <= ty1;
                let both = world
                    .tile_index(tx, ty)
                    .zip(world.tile_index(tx + dx, ty + dy));
                matches!((inside, both), (true, Some((a, b)))
                    if world.edge(a, d).is_some()
                        || world.edge(b, (d + 4) % 8).is_some()
                        || world.cliff(a, d)
                        || world.cliff(b, (d + 4) % 8))
            })
        })
    })
}

/// The fence check (placing 6.3): the zoo wall stays, nothing crosses the side, the smart fences
/// test and the entrance quadrant refuse a piece without a message, a fence that only swims is
/// refused on dry land with 10564; the slot of a piece of the same type above its decayed life takes a
/// free replacement, any other the full price.
fn check_fence(
    p: &mut Pending,
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    smart: bool,
    tile: usize,
) {
    let t = &cat.types[p.ty];
    let d = p.facing as usize;
    if world.wall(tile, d) || world.gate(tile) {
        return fail(p, None);
    }
    // In a tank only a tank wall stands, a show wall in a show tank and the other in the others
    // (placing 6.3 rule 3).
    if let Some(tank) = world.tank_at(tile).and_then(|id| world.tank(id)) {
        let wall = t
            .config
            .as_deref()
            .is_some_and(|c| c.section("TankFence").is_some());
        if !wall || (number(t, "cIsShowFence") == Some(1.0)) != tank.show {
            return fail(p, None);
        }
    }
    // No object holds a quadrant of this tile and one of the tile across the side.
    let held = |t: usize| (0..4).filter_map(move |q| world.quadrant(t, q));
    if let Some(across) = world.neighbour(tile, d)
        && held(tile).any(|h| held(across).any(|o| o == h))
    {
        return fail(p, None);
    }
    if smart && smarter(p, world, cat) {
        return fail(p, None);
    }
    if let Some(old) = world.edge(tile, d).and_then(|o| world.zoo.entities.get(o)) {
        let life = old
            .common
            .as_ref()
            .and_then(|c| c.fence)
            .map_or_else(|| number(t, "cLife").unwrap_or(LIFE), |f| f.life as f32);
        let free = is_of(old, t) && life > number(t, "cDecayedLife").unwrap_or(0.0);
        (p.verdict, p.price) = (Verdict::Overwrite, if free { 0.0 } else { cost(t) });
    }
    // A fence that only swims stands on water or in a tank alone: 10564.
    let wet = world.tank_at(tile).is_some()
        || water
            .get(usize::from(world.zoo.tiles[tile].terrain))
            .copied()
            .unwrap_or(false);
    if number(t, "cLand") == Some(0.0) && only_swims(t) && !wet {
        fail(p, Some(NOT_ON_WATER));
    }
}

/// Whether the smart fences test refuses the piece (placing 6.3 rule 5): within four tiles of its
/// tile, a unit with a body over two half tiles that covers it, or the object holding the last
/// quarter of a tile whose placement footprint differs from its footprint and whose rectangle
/// holds the piece.
fn smarter(p: &Pending, world: &World, cat: &Catalogue) -> bool {
    let (tx, ty) = p.tile;
    let near = |x: i32, y: i32| (x - tx).abs() <= 4 && (y - ty).abs() <= 4;
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    let of = |e: &Entity| cat.find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype));
    let counted =
        |x: i32, y: i32| near(x, y) && world.tile_index(x, y).is_some_and(|u| world.in_zoo(u));
    let units = world.zoo.entities.iter().any(|e| {
        let Some(c) = e
            .common
            .as_ref()
            .filter(|_| Kind::of_class(&e.class) == Kind::Unit)
        else {
            return false;
        };
        let (ux, uy) = (c.x.div_euclid(64), c.y.div_euclid(64));
        let [fx, fy, _] = of(e).map_or([0; 3], |t| t.footprint);
        counted(ux, uy)
            && (fx > 2 || fy > 2)
            && fx / 2 >= 2 * (ux - tx).abs()
            && fy / 2 >= 2 * (uy - ty).abs()
    });
    units
        || (tx - 4..=tx + 4).any(|ux| {
            (ty - 4..=ty + 4).any(|uy| {
                let Some(u) = world.tile_index(ux, uy).filter(|_| counted(ux, uy)) else {
                    return false;
                };
                let Some(e) = world.quadrant(u, 3).and_then(|o| world.zoo.entities.get(o)) else {
                    return false;
                };
                let (Some(c), Some(t)) = (e.common.as_ref(), of(e)) else {
                    return false;
                };
                let own = (t.footprint[0], t.footprint[1]);
                let mut at = placement_footprint(t);
                if c.rotation % 4 != 0 {
                    at = (at.1, at.0);
                }
                (c.x.div_euclid(64), c.y.div_euclid(64)) == (ux, uy)
                    && placement_footprint(t) != own
                    && i64::from((p.x - c.x).abs()) < 16 * i64::from(at.0)
                    && i64::from((p.y - c.y).abs()) < 16 * i64::from(at.1)
            })
        })
}

/// The entity a pending object of the type `ty` becomes when it is placed; `colors` are the colour
/// choices a new building starts with (see `MapRenderer::default_colors`).
pub fn entity(ty: &EntityType, p: &Pending, colors: Vec<u8>) -> Entity {
    let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
    Entity {
        class: ty.class.into(),
        type_name: text(&ty.type_name),
        subtype: text(&ty.subtype),
        body_size: None,
        common: Some(Common {
            x: p.x,
            y: p.y,
            z: p.z,
            rotation: p.facing,
            id: 0,
            name: String::new(),
            colors,
            fence: (Class::of(ty) == Class::Fence).then(|| Fence {
                strength: number(ty, "cStrength").unwrap_or(STRENGTH) as u32,
                life: number(ty, "cLife").unwrap_or(LIFE) as i32,
                timers: [0; 2],
                portal: false,
            }),
            food: number(ty, "cMaxFoodUnits").unwrap_or(0.0) as i32,
            filter: (ty.class == "filter").then(|| crate::world::placed_filter(ty)),
            ..Default::default()
        }),
    }
}

/// What a release over a place came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The object was placed, with this id.
    Placed(u32),
    /// Nothing was placed; the message to show, if the place or the cash gave one.
    Refused(Option<i32>),
}

/// Releases the button over the pending object (placing 6.1): a valid place is paid with the cash
/// and gets the object, booked as construction; one the cash does not cover shows the message of
/// insufficient funds, an invalid one its stored message, or else, when the type only lives in
/// water and its own tile is not water, the water message (the second look of 5.1). Scenery,
/// buildings, paths and fences are placed here: a path on water paints its tile first, one on a
/// path and a fence on a held slot take the old piece's place (placing 5.3, 6.3).
/// `shape` tells how the new entity takes its place, `water` which terrain types are water and
/// `colors` the colour choices a new building starts with.
pub fn place(
    world: &mut World,
    money: &mut Money,
    ty: &EntityType,
    p: &Pending,
    water: &[bool],
    colors: Vec<u8>,
    shape: impl Fn(&Entity) -> Option<Shape>,
) -> Outcome {
    if p.verdict == Verdict::Invalid {
        let wet = world.tile_index(p.tile.0, p.tile.1).is_some_and(|t| {
            world.tank_at(t).is_some()
                || water
                    .get(usize::from(world.zoo.tiles[t].terrain))
                    .copied()
                    .unwrap_or(false)
        });
        let second = (only_swims(ty) && !wet).then_some(NOT_ON_WATER);
        return Outcome::Refused(p.message.or(second));
    }
    let class = Class::of(ty);
    if !money.can_pay(p.price) {
        return Outcome::Refused(Some(NO_FUNDS));
    }
    let entity = entity(ty, p, colors);
    let shape = shape(&entity);
    let tile = world.tile_index(p.tile.0, p.tile.1);
    // The painting of a path on water is its own action, with no price: only the path's own price comes
    // back on undo (placing 5.3, 8).
    let mut paint = 0.0;
    if class == Class::Path
        && let (Some(kind), Some(t)) = (p.paint, tile)
    {
        let at = world.zoo.tiles[t];
        world.set_tile(p.tile.0, p.tile.1, at.elevation, at.shape, kind);
        paint = (p.price - cost(ty)).max(0.0);
    }
    // A path or a fence takes the place of the piece on its tile or its slot: the old piece stays
    // in its place of the entity list and takes the new type (placing 5.3, 6.3).
    let old = match class {
        Class::Path => tile.and_then(|t| world.path(t)),
        Class::Fence => tile.and_then(|t| world.edge(t, p.facing as usize)),
        Class::Fixed => None,
    };
    // A path on a path of the same type records nothing, so there is no price to tell.
    let quiet = class == Class::Path && old.is_some_and(|o| world.same_type(o, &entity));
    let old = old.and_then(|o| world.zoo.entities[o].common.as_ref().map(|c| c.id));
    let added = match (old, class) {
        (Some(id), Class::Fence | Class::Path) => world.swap(id, entity, shape),
        (_, _) => world.add(entity, shape),
    };
    match added {
        Some(id) => {
            if !quiet {
                world.priced(p.price - paint);
            }
            money.charge(CONSTRUCTION, p.price);
            Outcome::Placed(id)
        }
        None => Outcome::Refused(None),
    }
}

/// The place is not valid; the first message stays.
fn fail(p: &mut Pending, message: Option<i32>) {
    p.verdict = Verdict::Invalid;
    p.message = p.message.or(message);
}

#[cfg(test)]
mod tests;
