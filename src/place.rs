//! Placing and removing objects, paths and fences: so far the pending object, the type the player
//! chose as it follows the mouse (placing 4 and 5).
//!
//! [`pending`] snaps the world point under the mouse to where the type would stand, lifts it to
//! the ground, turns it, and checks the place: the verdict decides the tint, the cost text and
//! the message a click shows. The checks here are the ones every kind of object shares (the tile
//! is in the zoo, the quadrants of the footprint are free, a path or a fence replaces a piece);
//! the rules of one kind (a slope, water, a cliff inside a footprint, the fence rules) come with
//! the placing of that kind.

use crate::build::{cost, number};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::money::Symbol;
use crate::render::order::{Kind, ground_height};
use crate::render::{Extra, Ghost};
use crate::text::format::{money, whole_units};
use crate::ui::layout::color;
use crate::vfs::Vfs;
use crate::world::World;

/// The message of a place outside the zoo wall (or off the map).
pub const NOT_IN_THE_ZOO: i32 = 10311;
/// The message of an object that cannot be placed because another one stands in the way.
pub const IN_THE_WAY: i32 = 10555;
/// The message of a tank filter that is not next to a finished tank without a filter.
pub const NO_TANK: i32 = 10517;

/// The biggest placement footprint side taken, in half tiles (the paint order drops a bigger one).
const MAX_FOOTPRINT: i32 = 64;

/// The tints of the pending object (sprite drawing 9.1), keys of the map view element of
/// `ui/gamescrn.lyt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tints {
    pub valid: [u8; 3],
    pub overwrite: [u8; 3],
    pub invalid: [u8; 3],
}

impl Default for Tints {
    /// The shipped values.
    fn default() -> Tints {
        Tints {
            valid: [0, 225, 0],
            overwrite: [225, 112, 0],
            invalid: [225, 0, 0],
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
        }
    }
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
    Invalid,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Valid => "valid",
            Verdict::Overwrite => "overwrite",
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
    /// What the pending object costs, 0 for the replacement of a piece of the same type.
    pub price: f32,
    /// The objects in the way (positions in `zoo.entities`), tinted red until the next move.
    pub blocking: Vec<usize>,
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
        let tint = match self.verdict {
            Verdict::Valid => tints.valid,
            Verdict::Overwrite => tints.overwrite,
            Verdict::Invalid => tints.invalid,
        };
        Extra {
            ghost: Some(Ghost {
                ty: self.ty,
                x: self.x,
                y: self.y,
                z: self.z,
                facing: self.facing,
                tint,
            }),
            tinted: self.blocking.iter().map(|&e| (e, tints.invalid)).collect(),
            marks: Vec::new(),
        }
    }
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
        Class::Fence => [(32, 0), (63, 32), (32, 63), (0, 32)][(facing / 2 % 4) as usize],
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

/// Whether the entity is of the type.
fn is_of(entity: &crate::zoo::Entity, ty: &EntityType) -> bool {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    entity.class == ty.class
        && bytes(&entity.type_name) == ty.type_name
        && bytes(&entity.subtype) == ty.subtype
}

/// The pending object of the type `ty` (a position in the catalogue) with the rotation step
/// `step` of its panel, for the world point `point` under the mouse, the view turned by `view`.
/// `None` when the type is unknown or the point is off the map.
pub fn pending(
    world: &World,
    cat: &Catalogue,
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
        blocking: Vec::new(),
    };
    check(&mut pending, world, cat, class, fp);
    Some(pending)
}

/// The checks every object shares (placing 5.1, 5.2, 5.3): the first failure keeps its message.
fn check(p: &mut Pending, world: &World, cat: &Catalogue, class: Class, fp: (i32, i32)) {
    let t = &cat.types[p.ty];
    let (tx, ty) = p.tile;
    let Some(tile) = world.tile_index(tx, ty) else {
        return fail(p, Some(NOT_IN_THE_ZOO));
    };
    if !world.in_zoo(tile) {
        return fail(p, Some(NOT_IN_THE_ZOO));
    }
    // A tank filter needs a finished tank next to it (placing 5.1); no tank exists yet, so none is valid.
    if t.class == "filter" {
        return fail(p, Some(NO_TANK));
    }
    let same = |old: usize| {
        let same = world.zoo.entities.get(old).is_some_and(|e| is_of(e, t));
        (Verdict::Overwrite, if same { 0.0 } else { cost(t) })
    };
    if class == Class::Fence {
        if let Some(old) = world.edge(tile, p.facing as usize) {
            (p.verdict, p.price) = same(old);
        }
        return;
    }
    if class == Class::Path
        && let Some(old) = world.path(tile)
    {
        (p.verdict, p.price) = same(old);
    }
    // The quadrants of the footprint (map drawing 9.3.1): all on the map, in the zoo and free.
    if !(1..=MAX_FOOTPRINT).contains(&fp.0) || !(1..=MAX_FOOTPRINT).contains(&fp.1) {
        return fail(p, None);
    }
    for j in 0..fp.1 {
        for i in 0..fp.0 {
            let qx = (p.x - 16 * (fp.0 - 1) + 32 * i).div_euclid(32);
            let qy = (p.y - 16 * (fp.1 - 1) + 32 * j).div_euclid(32);
            let Some(at) = world.tile_index(qx.div_euclid(2), qy.div_euclid(2)) else {
                fail(p, Some(NOT_IN_THE_ZOO));
                continue;
            };
            if !world.in_zoo(at) {
                fail(p, Some(NOT_IN_THE_ZOO));
            } else if let Some(held) =
                world.quadrant(at, (qy.rem_euclid(2) * 2 + qx.rem_euclid(2)) as usize)
            {
                fail(p, Some(IN_THE_WAY));
                if !p.blocking.contains(&held) {
                    p.blocking.push(held);
                }
            }
        }
    }
}

/// The place is not valid; the first message stays.
fn fail(p: &mut Pending, message: Option<i32>) {
    p.verdict = Verdict::Invalid;
    p.message = p.message.or(message);
}

#[cfg(test)]
mod tests;
