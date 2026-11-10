//! Placing animals and staff (unit placing 5 to 7, 9.3 and 10.2): the pending unit and the checks
//! of its tile, the purchase, the name from a counter per type name, the crate's tile and the
//! sell price.
//!
//! The pending unit is a [`Pending`] like an object's, snapped as a 1 x 1 object and facing a
//! fixed screen direction. [`pending`] runs the checks of 6.1 in order, the first failure keeping
//! its message; a staff member's (6.3) are those of an animal without the ones that are about an
//! exhibit, the water message, the slope and the size; a guest's (6.4) are its own. The tank point
//! (5.1) lifts a guest or an underwater species over a tank under the pointer to the water, where
//! the tank tile is judged by 6.5 and the first row that applies gives the message.
//!
//! Picking a unit up and putting it down is [`pick_up`], [`put_down`] and [`carried`], the pending
//! unit of the one the player holds (8.2 to 8.6): the same checks, the cost 0.

use super::{
    NO_FUNDS, NOT_IN_THE_ZOO, Outcome, Pending, Verdict, entity, facing, fail, fenced, path_shape,
    snap, uneven,
};
use crate::behaviour::{Animation, Data, Runner};
use crate::build::{cost, number, plain};
use crate::camera::zoom_view;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::GROUNDS;
use crate::money::{ANIMAL_PURCHASE, Money};
use crate::place::Class;
use crate::render::Form;
use crate::render::order::{Kind, ground_height, project};
use crate::staff;
use crate::unit::{Part, Unit, movement, swim};
use crate::world::{Shape, World};
use crate::zoo::Entity;

/// Animals can only be placed in exhibits.
pub(crate) const IN_AN_EXHIBIT: i32 = 10108;
/// An underwater animal needs a tank with water in it.
pub(crate) const NEEDS_WATER: i32 = 10554;
/// The animal needs a deeper tank than this one.
const DEEPER: i32 = 10560;
/// Only show types stand in a show tank.
const NOT_FOR_SHOWS: i32 = 10561;
/// Nothing stands at the ladder and platform of a tank.
const LADDER: i32 = 10562;
/// An animal that does not swim in tanks.
const NOT_IN_TANKS: i32 = 10553;
/// A staff member that does not swim in tanks.
const NOT_FOR_STAFF: i32 = 10563;
/// An animal that cannot swim stands on land.
const NOT_ON_WATER_UNIT: i32 = 10569;
/// The tank point looks `48 + maximumTankHeight` steps toward the viewer (unit placing 5.1).
const TANK_STEPS: i32 = 48;
/// The message of an egg on water or in a tank.
const EGG_ON_LAND: i32 = 10570;
/// The crate search goes this far, in tiles of Manhattan distance (unit placing 9.3).
const CRATE_RANGE: i32 = 50;

/// Whether the type is placed as an animal by the Adopt panel.
pub(crate) fn is_animal(ty: &EntityType) -> bool {
    ty.class == "animals"
}

/// Whether the type is placed as a unit by the Adopt or the Hire panel.
pub(crate) fn is_unit(ty: &EntityType) -> bool {
    is_animal(ty) || staff::is_staff(ty)
}

/// What the type costs to place now: `cPurchaseCost`, halved in October for an animal of Dinosaur
/// Digs (`cExpansionID` 1; unit placing 5.3).
pub(crate) fn price(ty: &EntityType, october: bool) -> f32 {
    let half = october && is_animal(ty) && plain(ty, "cExpansionID") == Some(1.0);
    cost(ty) * if half { 0.5 } else { 1.0 }
}

/// The sell price of an animal (unit placing 10.2): every operation in `f32`, in the order of
/// the formula. At minimum health it is halved again.
pub(crate) fn sell_price(
    cost: f32,
    happiness: f32,
    health: f32,
    max_hits: f32,
    minimum: bool,
) -> f32 {
    let kept = (happiness + 100.0) * 0.005 + health / max_hits;
    let price = cost * 0.8 * kept * 0.5;
    if minimum { price * 0.5 } else { price }
}

/// The sell price of an animal of the type that is as new: its initial happiness and full health,
/// for an animal whose live state is not at hand.
pub(crate) fn new_sell_price(ty: &EntityType) -> f32 {
    let max_hits = max_hits(ty);
    let happiness = number(ty, "cInitialHappiness").unwrap_or(60.0);
    sell_price(cost(ty), happiness, max_hits, max_hits, false)
}

/// `cMaxHits`, 500 when the type has none or a value below 1.
fn max_hits(ty: &EntityType) -> f32 {
    number(ty, "cMaxHits").filter(|&m| m > 0.0).unwrap_or(500.0)
}

/// The sell price of this animal from its own happiness, health and minimum-health flag.
pub(crate) fn animal_sell_price(ty: &EntityType, a: &crate::animal::Animal) -> f32 {
    sell_price(
        cost(ty),
        a.happiness as f32,
        a.health as f32,
        max_hits(ty),
        a.at_minimum_health,
    )
}

/// Where the pointer is: the ground point under it, its own screen row and the zoom, which the
/// tank point reads (5.1).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Aim {
    pub point: (i64, i64),
    /// The row of the pointer in the view's pixels at the current zoom.
    pub row: i64,
    pub zoom: i32,
}

/// The pending animal of the type `ty` for the pointer `aim`, the view turned by `view`; `None`
/// when the type is unknown or the point is off the map. `water` tells for each terrain type
/// whether it is water.
pub(crate) fn pending(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    ty: usize,
    view: u8,
    aim: Aim,
    october: bool,
) -> Option<Pending> {
    let t = cat.types.get(ty)?;
    let form = if is_egg(t) { Form::Egg } else { Form::Plain };
    let dir = facing(0, view);
    pending_of(
        world,
        cat,
        water,
        ty,
        dir,
        view,
        aim,
        price(t, october),
        form,
    )
}

/// The type the pending unit of the tool `tool` is made from (unit placing 5.4), given the type
/// `made` its creation used: the same species, as a young for an egg. An animal with none made
/// has no pending unit (an egg layer with no young type makes nothing, 3.3 item 8).
pub(crate) fn made_type(cat: &Catalogue, made: Option<usize>, tool: usize) -> Option<usize> {
    let t = cat.types.get(tool)?;
    let same = |m: &&EntityType| {
        (m.class, &m.type_name) == (t.class, &t.type_name) && (m.subtype == t.subtype || is_egg(m))
    };
    match made.filter(|&m| cat.types.get(m).is_some_and(|m| same(&m))) {
        None if is_animal(t) => None,
        made => made.or(Some(tool)),
    }
}

/// Whether the pending animal of this type is an egg (unit placing 3.3 item 8): the young type of
/// a species that lays eggs, which is the only one made for it.
pub(crate) fn is_egg(ty: &EntityType) -> bool {
    is_animal(ty) && ty.subtype == b"y" && number(ty, "cLaysEggs") == Some(1.0)
}

/// The pending unit of the one the player carries (8.3): its own facing turned with the view, so
/// that it keeps its screen facing, and no cost. A crated animal is judged as a crate (6.2).
pub(crate) fn carried(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    carry: &Carry,
    view: u8,
    aim: Aim,
) -> Option<Pending> {
    pending_of(
        world,
        cat,
        water,
        carry.ty,
        turned(carry, view),
        view,
        aim,
        0.0,
        match world.units.get(carry.id).map(|u| &u.part) {
            Some(crate::unit::Part::Animal(a)) if a.egg => Form::Egg,
            Some(crate::unit::Part::Animal(a)) if a.crated => Form::Crate,
            _ => Form::Plain,
        },
    )
}

/// The map direction the carried thing faces when the view is turned by `view`: it keeps its
/// screen facing, `facing + 2 * (view at pick-up - view now)` (unit placing 8.3).
pub(crate) fn turned(carry: &Carry, view: u8) -> u32 {
    let turn = 2 * (i32::from(carry.view) - i32::from(view));
    (i64::from(carry.facing) + i64::from(turn)).rem_euclid(8) as u32
}

#[allow(clippy::too_many_arguments)]
fn pending_of(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    ty: usize,
    dir: u32,
    view: u8,
    aim: Aim,
    price: f32,
    form: Form,
) -> Option<Pending> {
    let t = cat.types.get(ty)?;
    let ground = (
        i32::try_from(aim.point.0).ok()?,
        i32::try_from(aim.point.1).ok()?,
    );
    // A guest or an underwater species is put over the tank under the pointer, if any (5.1).
    let swimmer = t.class == "guests" || number(t, "cUnderwater") == Some(1.0);
    let lifted = swimmer
        .then(|| tank_point(world, view, ground, aim.row, aim.zoom))
        .flatten();
    let point = lifted.unwrap_or(ground);
    let (tile_x, tile_y) = (point.0.div_euclid(64), point.1.div_euclid(64));
    let tile = world.tile_index(tile_x, tile_y)?;
    let (x, y) = snap(Class::Fixed, point, (1, 1), dir);
    let (lx, ly) = (x - 64 * tile_x, y - 64 * tile_y);
    let level = lifted.and_then(|_| world.tank_at(tile).and_then(|id| world.tank(id)));
    let mut p = Pending {
        ty,
        x,
        y,
        z: ground_height(&world.zoo.tiles[tile], lx, ly)
            .saturating_add(level.map_or(0, |l| water_height(l.level))),
        facing: dir,
        tile: (tile_x, tile_y),
        quadrant: (ly / 32 * 2 + lx / 32).clamp(0, 3) as usize,
        verdict: Verdict::Valid,
        message: None,
        price,
        paint: None,
        blocking: Vec::new(),
        wall: 0,
    };
    check(&mut p, world, t, water, lifted.is_some(), form);
    Some(p)
}

/// The height of the water surface over the floor of a tank at `level`: 16 per level.
fn water_height(level: u32) -> i32 {
    i32::try_from(level).unwrap_or(i32::MAX).saturating_mul(16)
}

/// The tank point (unit placing 5.1): from the ground point under the pointer, step toward the
/// viewer by 32 world units on each axis, `48 + maximumTankHeight` times (the start is step 0), and keep the last point
/// over a tank whose floor, projected at the tile's elevation, lies below the pointer's row by
/// exactly the water level in steps: `(floor row - pointer row) / step`, truncated toward zero.
/// So the pointer is at the water surface or less than one step above it. A step is 16 rows at
/// normal zoom, `16 * zoom` above it and `16 / -zoom` below.
fn tank_point(
    world: &World,
    view: u8,
    ground: (i32, i32),
    row: i64,
    zoom: i32,
) -> Option<(i32, i32)> {
    let (a, b) = [(0, 1), (1, 0), (0, -1), (-1, 0)][usize::from(view % 4)];
    let step = match zoom {
        ..=-2 => (16 / i64::from(zoom).abs()).max(1),
        2.. => 16 * i64::from(zoom),
        _ => 16,
    };
    let mut found = None;
    // The setting is cut to 255 when it is read; the bound keeps a loose number from looping.
    for k in 0..TANK_STEPS + world.tank_config().limit.min(255) as i32 {
        let x = ground.0.saturating_add(32 * k * (a - b));
        let y = ground.1.saturating_add(32 * k * (a + b));
        let Some(t) = world.tile_index(x.div_euclid(64), y.div_euclid(64)) else {
            continue;
        };
        let Some(tank) = world.tank_at(t).and_then(|id| world.tank(id)) else {
            continue;
        };
        let floor = world.zoo.tiles[t].elevation.saturating_mul(16);
        let floor_row = zoom_view(zoom, project(view, x, y, floor)).1;
        if (floor_row - row) / step == i64::from(tank.level) {
            found = Some((x, y));
        }
    }
    found
}

/// The footprint of the type in quarters, or of its egg or crate, along the axes of the map for
/// the facing.
fn footprint(t: &EntityType, dir: u32, form: Form) -> (i32, i32) {
    let [x, y] = match form {
        Form::Egg => t.form_footprint(true),
        Form::Crate => t.form_footprint(false),
        Form::Plain => [t.footprint[0], t.footprint[1]],
    };
    let own = (x.clamp(0, 64), y.clamp(0, 64));
    match dir % 4 {
        0 => own,
        _ => (own.1, own.0),
    }
}

/// The units on the tiles `tiles`: their position in the list, position and the half sides of
/// their blocking rectangle (placing 5.2).
pub(crate) fn units_on(world: &World, tiles: &[usize]) -> Vec<(usize, i32, i32, (i32, i32))> {
    let mut found = Vec::new();
    for (i, e) in world.zoo.entities.iter().enumerate() {
        let (Some(c), Some(shape)) = (e.common.as_ref(), world.shape(i)) else {
            continue;
        };
        let on = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
        if shape.kind != Kind::Unit || !on.is_some_and(|on| tiles.contains(&on)) {
            continue;
        }
        // The unit the player carries is off the map (8.2).
        if world.is_carried(c.id) {
            continue;
        }
        let (fx, fy) = world.unit_footprint(i).unwrap_or(shape.footprint);
        let rect = match c.rotation % 4 {
            0 => (fx, fy),
            _ if c.rotation % 2 != 0 => (fx.max(fy), fx.max(fy)),
            _ => (fy, fx),
        };
        found.push((i, c.x, c.y, (16 * rect.0, 16 * rect.1)));
    }
    found
}

/// Placing 5.2 item 4: whether the unit at `i` lets an object of type `t` through: the
/// helicopter, a tall unit when the type has `walkableByTall` 1, a rampaging animal when the type
/// has `cRubbleable` 1 (the default), and an animal whose species has `cRampageChance` above 0
/// when the object is rubble.
pub(super) fn lets_through_object(world: &World, i: usize, t: &EntityType) -> bool {
    let e = &world.zoo.entities[i];
    let unit = (e.common.as_ref()).and_then(|c| world.units.get(c.id));
    let rampaging =
        unit.is_some_and(|u| matches!(&u.part, crate::unit::Part::Animal(a) if a.rampaging));
    e.class == "helicopter"
        || unit.is_some_and(|u| u.mover.tall) && number(t, "walkableByTall") == Some(1.0)
        || rampaging && number(t, "cRubbleable").is_none_or(|n| n == 1.0)
        || t.class == "rubble" && unit.is_some_and(|u| u.mover.rampager)
}

/// Whether the object holding a quarter lets a unit through: only `walkable` counts, whatever
/// the unit (unit placing 6.1).
fn lets_through(world: &World, object: usize) -> bool {
    world.shape(object).is_some_and(|s: Shape| s.walkable.0)
}

/// The checks of 6.1 for an animal (6.2: a crate may stand on the zoo grounds), of 6.3 for a staff
/// member or of 6.4 for a guest, in order. `tanked`: the tank point found a tank, so the unit is in
/// tank mode.
fn check(p: &mut Pending, world: &World, t: &EntityType, water: &[bool], tanked: bool, form: Form) {
    let crated = form == Form::Crate;
    let staff = staff::is_staff(t);
    let guest = t.class == "guests";
    let Some(tile) = world
        .tile_index(p.tile.0, p.tile.1)
        .filter(|&tile| world.in_zoo(tile))
    else {
        return fail(p, Some(NOT_IN_THE_ZOO));
    };
    let flag = |key: &str| number(t, key) == Some(1.0);
    // The messages about water read `cOnlyUnderwater`, whatever the class (unit placing 6.1). An
    // egg does not swim.
    let (swims, only) = (flag("cSwims") && form != Form::Egg, flag("cOnlyUnderwater"));
    let (fx, fy) = footprint(t, p.facing, form);
    // A large animal runs the footprint check of placing 5.2 (items 1, 3 to 6): its quarters.
    let large = fx > 2 || fy > 2;
    let (x0, y0) = (p.x - 16 * (fx - 1), p.y - 16 * (fy - 1));
    let mut quads = Vec::new();
    for (i, j) in (0..fx).flat_map(|i| (0..fy).map(move |j| (i, j))) {
        let (qx, qy) = ((x0 + 32 * i).div_euclid(32), (y0 + 32 * j).div_euclid(32));
        match world.tile_index(qx.div_euclid(2), qy.div_euclid(2)) {
            Some(at) if world.in_zoo(at) => {
                quads.push((at, (qy.rem_euclid(2) * 2 + qx.rem_euclid(2)) as usize));
            }
            _ => fail(p, None),
        }
    }
    let mut tiles = vec![tile];
    if large {
        tiles = quads.iter().map(|&(at, _)| at).collect();
        tiles.dedup();
    }
    let units = units_on(world, &tiles);
    // A passable tile, and no other unit's rectangle over the animal's own.
    let passable = (0..4).any(|q| {
        world
            .quadrant(tile, q)
            .is_none_or(|o| lets_through(world, o))
    });
    let (hx, hy) = (16 * fx, 16 * fy);
    let overlaps = |&(_, x, y, (ox, oy)): &(usize, i32, i32, (i32, i32))| {
        i64::from((p.x - x).abs()) < i64::from(hx) + i64::from(ox)
            && i64::from((p.y - y).abs()) < i64::from(hy) + i64::from(oy)
    };
    // A guest wants all four quarters empty, even of walkable objects (6.4).
    let empty = (0..4).all(|q| world.quadrant(tile, q).is_none());
    if !(passable || tanked) || (guest && !empty) || units.iter().any(overlaps) {
        fail(p, None);
    }
    // Flat, or raised along one side only: always for a guest, for an animal unless it is small.
    if (guest || !staff && fx * fy >= 4) && !path_shape(world.zoo.tiles[tile].shape) {
        fail(p, None);
    }
    let wet_at = |at: usize| {
        let terrain = usize::from(world.zoo.tiles[at].terrain);
        water.get(terrain).copied().unwrap_or(false)
    };
    let wet = wet_at(tile);
    if wet && !swims && !guest {
        let message = match (only, form) {
            (true, _) => NEEDS_WATER,
            (_, Form::Egg) => EGG_ON_LAND,
            _ => NOT_ON_WATER_UNIT,
        };
        fail(p, (!staff).then_some(message));
    }
    // A large animal holds no quarter, stands flat at one height and crosses no fence or cliff.
    let held: Vec<usize> = match large {
        true => quads
            .iter()
            .filter_map(|&(at, q)| world.quadrant(at, q))
            .collect(),
        false => Vec::new(),
    };
    if large && (!held.is_empty() || uneven(world, &tiles) || fenced(world, x0, y0, (fx, fy))) {
        fail(p, None);
    }
    // A large crate's quarters take the ground test of placing 5.2 item 2.
    let ground = |&(at, _): &(usize, usize)| match super::only_swims(t) {
        true => !wet_at(at),
        false => !swims && wet_at(at),
    };
    if large && crated && quads.iter().any(ground) {
        fail(p, None);
    }
    // It stands in an exhibit.
    let region = world.exhibits().region(tile);
    if !staff && !guest && !crated && region.is_none_or(|r| r == GROUNDS) {
        fail(p, Some(if only { NEEDS_WATER } else { IN_AN_EXHIBIT }));
    }
    let tank = world.tank_at(tile);
    if let Some(id) = tank {
        // The tank check runs on a place that is still valid (6.1 step 3).
        let message = match p.verdict {
            Verdict::Valid => tank_message(world, id, tile, t, (p.x, p.y, hx, hy), form),
            _ => None,
        };
        if let Some(message) = message {
            fail(p, message);
        }
    } else if only && !crated {
        fail(p, Some(NEEDS_WATER));
    }
    // What is in the way: the objects holding the tile's quarters and every unit on it.
    if p.verdict == Verdict::Invalid {
        let around = (0..4).filter_map(|q| world.quadrant(tile, q));
        for i in around.chain(held).chain(units.iter().map(|u| u.0)) {
            fail(p, Some(super::IN_THE_WAY));
            if !p.blocking.contains(&i) {
                p.blocking.push(i);
            }
        }
    }
    // Without the tank point a tank tile is never valid.
    if tank.is_some() && !tanked {
        fail(p, None);
    }
}

/// The tile inside the gate of tank `id`, where its ladder and platform are.
pub(crate) fn gate_inside(world: &World, id: u32) -> Option<usize> {
    let (g, side) = world.gate_slot(id)?;
    match world.tank_at(g) == Some(id) {
        true => Some(g),
        false => world.neighbour(g, side),
    }
}

/// The tank check of 6.5 for a unit on `tile` of tank `id`, whose blocking rectangle is centred at
/// `(x, y)` with the half sides `(hx, hy)`: the first row of the table that applies refuses, with
/// its message, if it has one (rows 7 and 8 have none).
fn tank_message(
    world: &World,
    id: u32,
    tile: usize,
    t: &EntityType,
    (x, y, hx, hy): (i32, i32, i32, i32),
    form: Form,
) -> Option<Option<i32>> {
    let tank = world.tank(id)?;
    let flag = |key: &str| number(t, key) == Some(1.0);
    let (under, animal) = (flag("cUnderwater"), is_animal(t));
    // Only a keeper stands at the ladder: on its tile, or with the tile's centre under the unit.
    let width = world.zoo.width as i32;
    let gate = gate_inside(world, id);
    let centre = |g: usize| ((g as i32 % width) * 64 + 32, (g as i32 / width) * 64 + 32);
    let covers = |g: usize| {
        let (cx, cy) = centre(g);
        (x - cx).abs() <= hx && (y - cy).abs() <= hy
    };
    if t.class != "keeper" && gate.is_some_and(|g| g == tile || covers(g)) {
        return Some(Some(LADDER));
    }
    if animal && form == Form::Egg {
        return Some(Some(EGG_ON_LAND));
    }
    let filled = tank.filled && tank.level >= 1;
    if animal && under && form != Form::Crate && !filled {
        return Some(Some(NEEDS_WATER));
    }
    // The depth is raised to the height of each object in a quarter that does not let it through.
    let tall = flag("cTall");
    let held = (0..4).filter_map(|q| world.quadrant(tile, q));
    let blocks = held.filter_map(|o| world.shape(o)).filter(|s| match tall {
        true => !s.walkable.1,
        false => !s.walkable.0,
    });
    let depth = blocks.fold(number(t, "cDepth").unwrap_or(0.0), |d, s| {
        d.max(s.tank.height as f32)
    });
    if animal && depth > tank.level as f32 {
        return Some(Some(DEEPER));
    }
    if !under {
        return Some(Some(if staff::is_staff(t) {
            NOT_FOR_STAFF
        } else {
            NOT_IN_TANKS
        }));
    }
    if tank.show && plain(t, "cShow") != Some(1.0) {
        return Some(Some(NOT_FOR_SHOWS));
    }
    // A swimmer that keeps off the walls, and a floating object in the fourth quarter: no message.
    let edges = number(t, "cAvoidEdges").map_or(0, |r| r.max(0.0) as u32);
    let floating = world
        .quadrant(tile, 3)
        .and_then(|o| world.shape(o))
        .is_some_and(|s| s.tank.surface);
    (floating || edges >= 1 && crate::path::near_edge(world, tile, edges)).then_some(None)
}

/// The name of a new entity of the type, `text` being the type's name: [`numbered`], kept only
/// when it holds a visible character (ambients 3.3), else empty. Every name made at an add goes
/// through here.
pub(crate) fn name(world: &mut World, ty: &EntityType, text: &str) -> String {
    let name = numbered(world, ty, text);
    let language = world.language_number();
    match crate::text::visible(language, &crate::strings::encode_text(language, &name)) {
        true => name,
        false => String::new(),
    }
}

/// The default name of a new entity of the type, `text` being the type's name: with
/// `cUseNumbersInName` (on by default for a unit, a helicopter, a building and a tank filter, off
/// for anything else) a space and the next number of the counter of its key, started at 1 (unit
/// placing 7.2). The key is the type name for a unit (`giraffe`, not the text) and the subtype for
/// a building or a filter (`vendfood`, `f`). Scenery, fences, paths, tank walls, rubble, ambient
/// animals and food the bare text whatever the flag says. A guest is numbered by the counter
/// `guests`, shared by its four types. A numbered class with an empty text keeps an empty name
/// and takes no number (localisation 10).
fn numbered(world: &mut World, ty: &EntityType, text: &str) -> String {
    let bare = matches!(
        ty.class,
        "objects" | "fences" | "paths" | "tankwall" | "rubble" | "ambient" | "food"
    );
    let subtyped = matches!(ty.class, "building" | "filter");
    let counted = is_unit(ty) || subtyped || matches!(ty.class, "helicopter" | "guests");
    let numbered = plain(ty, "cUseNumbersInName").unwrap_or(f32::from(u8::from(counted)));
    if bare || numbered != 1.0 {
        return text.into();
    }
    if text.is_empty() {
        return String::new();
    }
    let key = match ty.class {
        "guests" => b"guests",
        _ if subtyped => ty.subtype.as_slice(),
        _ => ty.type_name.as_slice(),
    };
    let key = String::from_utf8_lossy(key).into_owned();
    let counter = world.zoo.names.entry(key).or_insert(0);
    *counter = counter.saturating_add(1);
    format!("{text} {counter}")
}

/// The name of a new entity of the type from the type's name text (`world.units.names`): empty
/// when the interface gave none (a guest's text is the guests' string, not the type's).
pub(crate) fn named(world: &mut World, ty: &EntityType) -> String {
    let text = match ty.class {
        "guests" => Some(world.units.guest_name.clone()),
        _ => world.units.names.get(&ty.name_id).cloned(),
    };
    match text {
        Some(text) => name(world, ty, &text),
        None => String::new(),
    }
}

/// The category a unit's purchase is booked in: animal purchase, or the wages of the staff's class.
pub(crate) fn category(ty: &EntityType) -> usize {
    staff::wages(ty).unwrap_or(ANIMAL_PURCHASE)
}

/// The state a pending unit was made with (unit placing 5.4): the type it was made from, the id it
/// took and its creation state. The placed unit keeps all three.
#[derive(Debug, Clone)]
pub(crate) struct Made {
    pub ty: usize,
    pub id: u32,
    pub part: Part,
}

/// Releases the button over a pending unit (unit placing 7.1): a valid place is paid with the
/// cash, booked in its [`category`], and gets the unit named `text` (the type's name, numbered
/// by [`name`]); a place that is not valid shows its stored message, a cash that does not cover
/// the cost the message of insufficient funds. The undo list records the placement of an animal;
/// hiring a staff member empties it. The unit takes the id and the state of the pending unit,
/// `made`, if it has one; at the add it takes the tank mode and the terrain mode of where it
/// stands, and a staff member marks its exhibit for a new rating (7.1 items 1 and 4).
pub(crate) fn place(
    world: &mut World,
    money: &mut Money,
    ty: &EntityType,
    text: &str,
    p: &Pending,
    shape: impl Fn(&Entity) -> Option<Shape>,
    (made, data): (Option<Made>, &dyn Data),
) -> Outcome {
    if p.verdict == Verdict::Invalid {
        return Outcome::Refused(p.message);
    }
    if !money.can_pay(p.price) {
        return Outcome::Refused(Some(NO_FUNDS));
    }
    let mut unit = entity(ty, p, Vec::new(), &world.research, world.units.now);
    if let Some(common) = unit.common.as_mut() {
        common.name = name(world, ty, text);
        if let Some(Part::Staff(s)) = made.as_ref().map(|m| &m.part) {
            common.colors.clone_from(&s.colours);
        }
    }
    let shape = shape(&unit);
    let tile = world.tile_index(p.tile.0, p.tile.1);
    match world.add_made(unit, shape, true, made.as_ref().map(|m| m.id)) {
        Some(id) => {
            arrive(world, id, made.map(|m| m.part), data);
            if let Some(t) = tile.filter(|_| !is_animal(ty)) {
                world.mark_rating(t);
            }
            world.priced(p.price);
            if staff::is_staff(ty) {
                world.clear_undo();
            }
            money.charge(category(ty), p.price);
            Outcome::Placed(id)
        }
        None => Outcome::Refused(None),
    }
}

/// The unit `id` just added takes the state it was made with, if it was, and what the add gives
/// it (unit placing 7.1 item 1): the tank position check and the terrain mode, and the picture of
/// an egg or a crate.
fn arrive(world: &mut World, id: u32, part: Option<Part>, data: &dyn Data) {
    let (Some(at), Some(slot)) = (world.find(id), world.units.get_mut(id)) else {
        return;
    };
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    if let Some(part) = part {
        unit.part = part;
        unit.started = true;
    }
    movement::read_type(&mut unit, data, &world.zoo.entities[at]);
    unit.typed = true;
    swim::added(&mut unit, world, data, at);
    swim::add_to_map(&mut unit, world, at);
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
}

/// The unit `id` shows its idle picture: the egg's or the crate's for an egg or a crate (unit
/// placing 9.1).
pub(crate) fn show_idle(world: &mut World, id: u32, data: &dyn Data) {
    let (Some(at), Some(slot)) = (world.find(id), world.units.get_mut(id)) else {
        return;
    };
    if !matches!(&slot.part, Part::Animal(a) if a.egg || a.crated) {
        return;
    }
    let mut unit = std::mem::replace(slot, Unit::new(id, slot.kind));
    movement::idle(&mut unit, world, data, at);
    if let Some(slot) = world.units.get_mut(id) {
        *slot = unit;
    }
}

/// The animal `id`, refused by a new tank, is put out of it (unit placing 9.1, 9.3), starting the
/// search at tile `start`: crated where it stands, an egg never (one whose countdown is spent is
/// put in its egg state again, the countdown kept), then moved to the middle of the tile
/// [`crate_spot`] finds, where it takes the tank mode and the terrain mode. `false` when no tile
/// qualifies: it stays where it is, for the caller to sell.
pub(crate) fn refuse(
    world: &mut World,
    cat: &Catalogue,
    id: u32,
    start: usize,
    data: &dyn Data,
) -> bool {
    let Some(at) = world.find(id) else {
        return false;
    };
    let (keys, sickly) = (
        crate::animal::keys(world, cat, at),
        crate::animal::breed::sickly(cat),
    );
    let egg = match world.units.get_mut(id).map(|u| &mut u.part) {
        Some(Part::Animal(a)) if a.egg => {
            if a.hatch < 1 {
                a.refresh_egg(&keys, sickly);
            }
            true
        }
        Some(Part::Animal(a)) => {
            crate::animal::boxed::crate_up(a);
            (a.playing, a.caught_prey, a.prey) = (false, false, 0);
            false
        }
        _ => return false,
    };
    if !egg {
        crate::animal::boxed::snap(world, at);
        if let Some(unit) = world.units.get_mut(id) {
            crate::goal::stop_all(unit);
            unit.animation = Animation::default();
            unit.predator = 0;
        }
        swim::put(world, id, data);
    }
    let Some(spot) = crate_spot(world, start) else {
        return false;
    };
    let width = world.zoo.width as usize;
    let (x, y) = (
        (spot % width) as i32 * 64 + 32,
        (spot / width) as i32 * 64 + 32,
    );
    let z = ground_height(&world.zoo.tiles[spot], 32, 32);
    world.show_moved(at, false);
    world.lift_unit(at);
    let from = (world.zoo.entities[at].common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let rotation = world.zoo.entities[at]
        .common
        .as_ref()
        .map_or(0, |c| c.rotation);
    world.place_unit(at, (x, y, z), rotation);
    world.show_moved(at, true);
    swim::put(world, id, data);
    from.into_iter()
        .chain([spot])
        .for_each(|t| world.mark_rating(t));
    true
}

/// The tile for the crate of an animal that was put out of a tank, searched from `start` (unit
/// placing 9.3): the tiles of the zoo grounds that are not water and hold no unit and no path, in
/// rings of Manhattan distance 3 to 50, each scored 0 to 3 (a passable quarter, flat, all four
/// quarters empty); the first with 3 wins, else the best. `None` when no tile qualifies.
pub(crate) fn crate_spot(world: &World, start: usize) -> Option<usize> {
    crate_spot_in(world, start, CRATE_RANGE)
}

/// [`crate_spot`] with the rings going up to Manhattan distance `range`.
pub(crate) fn crate_spot_in(world: &World, start: usize, range: i32) -> Option<usize> {
    let width = world.zoo.width as usize;
    let (sx, sy) = ((start % width) as i32, (start / width) as i32);
    let mut best: Option<(u32, usize)> = None;
    for r in 3..=range {
        for dx in -r..=r {
            let k = r - dx.abs();
            for dy in [-k, k] {
                let Some(t) = world.tile_index(sx + dx, sy + dy) else {
                    continue;
                };
                if !world.in_zoo(t)
                    || world.exhibits().region(t) != Some(GROUNDS)
                    || world.is_water(t)
                    || world.path(t).is_some()
                    || world.unit_on(t)
                {
                    continue;
                }
                let score = u32::from(
                    (0..4).any(|q| world.quadrant(t, q).is_none_or(|o| lets_through(world, o))),
                ) + u32::from(world.zoo.tiles[t].shape == 0)
                    + u32::from((0..4).all(|q| world.quadrant(t, q).is_none()));
                if score == 3 {
                    return Some(t);
                }
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, t));
                }
            }
        }
    }
    best.map(|(_, t)| t)
}

/// A unit the player holds (unit placing 8.2): which one, its type, and where it stood, which way
/// it faced and how the view was turned, for the facing it keeps and for cancelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Carry {
    pub id: u32,
    /// The type, a position in the catalogue.
    pub ty: usize,
    pub from: (i32, i32, i32),
    pub facing: u32,
    pub view: u8,
}

/// Picks the unit `id` up (unit placing 8.2): it is taken off the map, stops everything (its move,
/// its behaviour set and its animation, the goals being the goal issues') and is marked carried,
/// so its update is suspended; the exhibit it left is marked for a new rating. An escaped animal
/// is crated (9.1). `None` when `id` is
/// no unit, is carried already, or its type is not moveable (`cMoveable` 0).
pub(crate) fn pick_up(world: &mut World, cat: &Catalogue, id: u32, view: u8) -> Option<Carry> {
    let at = world.find(id)?;
    let e = &world.zoo.entities[at];
    e.common.as_ref()?;
    let ty = cat.position(&e.class, e.type_name.as_bytes(), e.subtype.as_bytes())?;
    if number(&cat.types[ty], "cMoveable") == Some(0.0) || world.units.get(id)?.carried {
        return None;
    }
    // A guest in the middle of a building visit leaves the building first (guest buildings 11.2).
    crate::building::leave::pick_up(world, cat, id);
    let e = &world.zoo.entities[at];
    let c = e.common.as_ref()?;
    let carry = Carry {
        id,
        ty,
        from: (c.x, c.y, c.z),
        facing: c.rotation,
        view,
    };
    let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
    world.show_moved(at, false);
    world.lift_unit(at);
    if let Some(t) = tile {
        world.mark_rating(t);
    }
    let c = world.zoo.entities[at].common.as_mut()?;
    c.movement_mode = 0;
    // The state clear writes "no slot" in the unit block of every kind (goals 11.3.1 point 2); a
    // guest's and an animal's live slot is written over it at save, a staff member's is this byte.
    if let Some(u) = c.unit.as_mut() {
        u.slot = u8::MAX;
    }
    // A keeper told the exhibit he is no longer coming: he closes the gate he remembers.
    crate::staff::leaves(world, id);
    let unit = world.units.get_mut(id)?;
    unit.tank = 0;
    unit.carried = true;
    // The pick-up of the animal itself lowers the hold of an aborted heal (staff 7.7).
    unit.held = false;
    match &mut unit.part {
        // An escaped animal is crated (8.2 point 4); any other one's escape walk ends.
        crate::unit::Part::Animal(a) => {
            // The state clear of a pick-up writes "no need" (shows 32.1 point 1) and "no slot"
            // (goals 11.3.1 point 2).
            (a.need, a.slot) = (crate::guest::NO_NEED, crate::guest::NO_SLOT);
            match a.escaped {
                true => crate::animal::boxed::crate_up(a),
                false => a.walk = None,
            }
        }
        // A guest forgets the building it meant to return to (unit placing 8.2 point 5) and its
        // slot (goals 11.3.1 point 2).
        crate::unit::Part::Guest(g) => {
            g.returning = None;
            g.slot = crate::guest::NO_SLOT;
        }
        _ => {}
    }
    if unit.kind == crate::unit::Kind::Animal {
        // The state clear writes the own target tile and its position directly (shows 32.2 point 4).
        unit.clear_own();
    }
    unit.moving = false;
    unit.destination = None;
    unit.path.clear();
    unit.travel = -1;
    (unit.through_building, unit.aborted) = (false, false);
    unit.runner = Runner::default();
    unit.animation = Animation::default();
    unit.events.clear();
    Some(carry)
}

/// Puts the carried unit `id` down at `point` facing `facing` (unit placing 8.4 and 8.6; the
/// caller has judged the place): it is added to the map again, takes the tank mode and the terrain
/// mode of its tile (tank fauna 4, 5), is no longer carried, and the exhibit of the tile is marked for a new rating. Its home is the
/// exhibit it stands in; the goals then start anew at its next update. `false` when the unit is
/// gone or not carried.
pub(crate) fn put_down(
    world: &mut World,
    id: u32,
    point: (i32, i32, i32),
    facing: u32,
    data: &dyn Data,
) -> bool {
    let (Some(at), true) = (
        world.find(id),
        world.units.get(id).is_some_and(|u| u.carried),
    ) else {
        return false;
    };
    let Some(tile) = world.tile_index(point.0.div_euclid(64), point.1.div_euclid(64)) else {
        return false;
    };
    world.place_unit(at, point, facing);
    world.show_moved(at, true);
    if let Some(unit) = world.units.get_mut(id) {
        unit.carried = false;
        // A guest put down has lost its wish to leave (guests 8) and its return building
        // (unit placing 8.5, pathfinding 6.5).
        if let crate::unit::Part::Guest(g) = &mut unit.part {
            g.leave = false;
            g.returning = None;
            g.return_set = None;
        }
    }
    // The member lists of the return buildings let it go (guest buildings 12.2 point 3).
    crate::building::ret::forget(world, id);
    crate::unit::swim::put(world, id, data);
    world.mark_rating(tile);
    true
}

#[cfg(test)]
mod tests;
