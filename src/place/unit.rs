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
use crate::behaviour::{Animation, Runner};
use crate::build::{cost, number, plain};
use crate::camera::zoom_view;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::GROUNDS;
use crate::money::{ANIMAL_PURCHASE, Money};
use crate::place::Class;
use crate::render::order::{Kind, ground_height, project};
use crate::staff;
use crate::world::{Shape, World};
use crate::zoo::Entity;

/// Animals can only be placed in exhibits.
pub const IN_AN_EXHIBIT: i32 = 10108;
/// An underwater animal needs a tank with water in it.
pub const NEEDS_WATER: i32 = 10554;
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
/// The tank point looks this many steps toward the viewer (unit placing 5.1: 48, plus a setting of
/// the map view that is 0).
const TANK_STEPS: i32 = 48;
/// The crate search goes this far, in tiles of Manhattan distance (unit placing 9.3).
const CRATE_RANGE: i32 = 50;

/// Whether the type is placed as an animal by the Adopt panel.
pub fn is_animal(ty: &EntityType) -> bool {
    ty.class == "animals"
}

/// Whether the type is placed as a unit by the Adopt or the Hire panel.
pub fn is_unit(ty: &EntityType) -> bool {
    is_animal(ty) || staff::is_staff(ty)
}

/// What the type costs to place now: `cPurchaseCost`, halved in October for an animal of Dinosaur
/// Digs (`cExpansionID` 1; unit placing 5.3).
pub fn price(ty: &EntityType, october: bool) -> f32 {
    let half = october && is_animal(ty) && plain(ty, "cExpansionID") == Some(1.0);
    cost(ty) * if half { 0.5 } else { 1.0 }
}

/// The sell price of an animal (unit placing 10.2), computed in double with the single-precision
/// constants of the original and rounded to `f32`. At minimum health it is halved again.
pub fn sell_price(cost: f32, happiness: f32, health: f32, max_hits: f32, minimum: bool) -> f32 {
    let (rate, step) = (f64::from(0.8_f32), f64::from(0.005_f32));
    let kept = (f64::from(happiness) + 100.0) * step + f64::from(health) / f64::from(max_hits);
    let price = f64::from(cost) * rate * kept * 0.5;
    (if minimum { price * 0.5 } else { price }) as f32
}

/// The sell price of an animal of the type that is as new: its initial happiness and full health
/// (the status variables come with the animal's own state).
pub fn new_sell_price(ty: &EntityType) -> f32 {
    let max_hits = number(ty, "cMaxHits").filter(|&m| m > 0.0).unwrap_or(500.0);
    let happiness = number(ty, "cInitialHappiness").unwrap_or(60.0);
    sell_price(cost(ty), happiness, max_hits, max_hits, false)
}

/// Where the pointer is: the ground point under it, its own screen row and the zoom, which the
/// tank point reads (5.1).
#[derive(Debug, Clone, Copy)]
pub struct Aim {
    pub point: (i64, i64),
    /// The row of the pointer in the view's pixels at the current zoom.
    pub row: i64,
    pub zoom: i32,
}

/// The pending animal of the type `ty` for the pointer `aim`, the view turned by `view`; `None`
/// when the type is unknown or the point is off the map. `water` tells for each terrain type
/// whether it is water.
pub fn pending(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    ty: usize,
    view: u8,
    aim: Aim,
    october: bool,
) -> Option<Pending> {
    let price = price(cat.types.get(ty)?, october);
    pending_of(world, cat, water, ty, facing(0, view), view, aim, price)
}

/// The pending unit of the one the player carries (8.3): its own facing turned with the view, so
/// that it keeps its screen facing, and no cost.
pub fn carried(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    carry: &Carry,
    view: u8,
    aim: Aim,
) -> Option<Pending> {
    let turn = 2 * (i32::from(carry.view) - i32::from(view));
    let dir = (i64::from(carry.facing) + i64::from(turn)).rem_euclid(8) as u32;
    pending_of(world, cat, water, carry.ty, dir, view, aim, 0.0)
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
    check(&mut p, world, t, water, lifted.is_some());
    Some(p)
}

/// The height of the water surface over the floor of a tank at `level`: 16 per level.
fn water_height(level: u32) -> i32 {
    i32::try_from(level).unwrap_or(i32::MAX).saturating_mul(16)
}

/// The tank point (unit placing 5.1): from the ground point under the pointer, step toward the
/// viewer by 32 world units on each axis, 48 times (the start is step 0), and keep the last point
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
    for k in 0..TANK_STEPS {
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

/// The footprint of the type in quarters, along the axes of the map for the facing.
fn footprint(t: &EntityType, dir: u32) -> (i32, i32) {
    let own = (t.footprint[0].clamp(0, 64), t.footprint[1].clamp(0, 64));
    match dir % 4 {
        0 => own,
        _ => (own.1, own.0),
    }
}

/// The units on the tiles `tiles`: their position in the list, position and the half sides of
/// their blocking rectangle (placing 5.2).
fn units_on(world: &World, tiles: &[usize]) -> Vec<(usize, i32, i32, (i32, i32))> {
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
        let (fx, fy) = shape.footprint;
        let rect = match c.rotation % 4 {
            0 => (fx, fy),
            _ if c.rotation % 2 != 0 => (fx.max(fy), fx.max(fy)),
            _ => (fy, fx),
        };
        found.push((i, c.x, c.y, (16 * rect.0, 16 * rect.1)));
    }
    found
}

/// Whether the object holding a quarter lets a unit through: only `walkable` counts, whatever
/// the unit (unit placing 6.1).
fn lets_through(world: &World, object: usize) -> bool {
    world.shape(object).is_some_and(|s: Shape| s.walkable.0)
}

/// The checks of 6.1 for an animal that is not crated, of 6.3 for a staff member or of 6.4 for a
/// guest, in order. `tanked`: the tank point found a tank, so the unit is in tank mode.
fn check(p: &mut Pending, world: &World, t: &EntityType, water: &[bool], tanked: bool) {
    let staff = staff::is_staff(t);
    let guest = t.class == "guests";
    let Some(tile) = world
        .tile_index(p.tile.0, p.tile.1)
        .filter(|&tile| world.in_zoo(tile))
    else {
        return fail(p, Some(NOT_IN_THE_ZOO));
    };
    let flag = |key: &str| number(t, key) == Some(1.0);
    // The messages about water read `cOnlyUnderwater`, whatever the class (unit placing 6.1).
    let (swims, only) = (flag("cSwims"), flag("cOnlyUnderwater"));
    let (fx, fy) = footprint(t, p.facing);
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
    let wet = water
        .get(usize::from(world.zoo.tiles[tile].terrain))
        .copied()
        .unwrap_or(false);
    if wet && !swims && !guest {
        let message = if only { NEEDS_WATER } else { NOT_ON_WATER_UNIT };
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
    // It stands in an exhibit.
    let region = world.exhibits().region(tile);
    if !staff && !guest && region.is_none_or(|r| r == GROUNDS) {
        fail(p, Some(if only { NEEDS_WATER } else { IN_AN_EXHIBIT }));
    }
    let tank = world.tank_at(tile);
    if let Some(id) = tank {
        // The tank check runs on a place that is still valid (6.1 step 3).
        let message = tank_message(world, id, tile, t).filter(|_| p.verdict == Verdict::Valid);
        if let Some(message) = message {
            fail(p, Some(message));
        }
    } else if only {
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

/// The message of the tank check of 6.5 for a unit on `tile` of tank `id`: the first row of the
/// table that applies, in the table's order.
fn tank_message(world: &World, id: u32, tile: usize, t: &EntityType) -> Option<i32> {
    let tank = world.tank(id)?;
    let under = number(t, "cUnderwater") == Some(1.0);
    let depth = number(t, "cDepth").unwrap_or(0.0);
    // Only a keeper stands at the ladder.
    if gate_inside(world, id) == Some(tile) && t.class != "keeper" {
        return Some(LADDER);
    }
    // The filled tank and the depth are an animal's.
    if is_animal(t) && under && (!tank.filled || tank.level < 1) {
        return Some(NEEDS_WATER);
    }
    if is_animal(t) && depth > tank.level as f32 {
        return Some(DEEPER);
    }
    if !under {
        return Some(if staff::is_staff(t) {
            NOT_FOR_STAFF
        } else {
            NOT_IN_TANKS
        });
    }
    (tank.show && plain(t, "cShow") != Some(1.0)).then_some(NOT_FOR_SHOWS)
}

/// The default name of a new unit of the type, `text` being the type's name: with
/// `cUseNumbersInName` (the default) a space and the next number of the counter of the type name
/// (`giraffe`, not the text), started at 1 (unit placing 7.2).
pub fn name(world: &mut World, ty: &EntityType, text: &str) -> String {
    if plain(ty, "cUseNumbersInName").is_some_and(|n| n != 1.0) {
        return text.into();
    }
    let key = String::from_utf8_lossy(&ty.type_name).into_owned();
    let counter = world.zoo.names.entry(key).or_insert(0);
    *counter = counter.saturating_add(1);
    format!("{text} {counter}")
}

/// The category a unit's purchase is booked in: animal purchase, or the wages of the staff's class.
pub fn category(ty: &EntityType) -> usize {
    staff::wages(ty).unwrap_or(ANIMAL_PURCHASE)
}

/// Releases the button over a pending unit (unit placing 7.1): a valid place is paid with the
/// cash, booked in its [`category`], and gets the unit named `text` (the type's name, numbered
/// by [`name`]); a place that is not valid shows its stored message, a cash that does not cover
/// the cost the message of insufficient funds. The undo list records the placement of an animal;
/// hiring a staff member empties it.
pub fn place(
    world: &mut World,
    money: &mut Money,
    ty: &EntityType,
    text: &str,
    p: &Pending,
    shape: impl Fn(&Entity) -> Option<Shape>,
) -> Outcome {
    if p.verdict == Verdict::Invalid {
        return Outcome::Refused(p.message);
    }
    if !money.can_pay(p.price) {
        return Outcome::Refused(Some(NO_FUNDS));
    }
    let mut unit = entity(ty, p, Vec::new());
    if let Some(common) = unit.common.as_mut() {
        common.name = name(world, ty, text);
    }
    let shape = shape(&unit);
    match world.add(unit, shape) {
        Some(id) => {
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

/// The tile for the crate of an animal that was put out of a tank, searched from `start` (unit
/// placing 9.3): the tiles of the zoo grounds that are not water and hold no unit and no path, in
/// rings of Manhattan distance 3 to 50, each scored 0 to 3 (a passable quarter, flat, all four
/// quarters empty); the first with 3 wins, else the best. `None` when no tile qualifies.
pub fn crate_spot(world: &World, start: usize) -> Option<usize> {
    let width = world.zoo.width as usize;
    let (sx, sy) = ((start % width) as i32, (start / width) as i32);
    let mut best: Option<(u32, usize)> = None;
    for r in 3..=CRATE_RANGE {
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
pub struct Carry {
    pub id: u32,
    /// The type, a position in the catalogue.
    pub ty: usize,
    pub from: (i32, i32, i32),
    pub facing: u32,
    pub view: u8,
}

/// Picks the unit `id` up (unit placing 8.2): it is taken off the map, stops everything (its move,
/// its behaviour set and its animation, the goals being the goal issues') and is marked carried,
/// so its update is suspended; the exhibit it left is marked for a new rating. `None` when `id` is
/// no unit, is carried already, or its type is not moveable (`cMoveable` 0).
pub fn pick_up(world: &mut World, cat: &Catalogue, id: u32, view: u8) -> Option<Carry> {
    let at = world.find(id)?;
    let e = &world.zoo.entities[at];
    let c = e.common.as_ref()?;
    let ty = cat.position(&e.class, e.type_name.as_bytes(), e.subtype.as_bytes())?;
    if number(&cat.types[ty], "cMoveable") == Some(0.0) || world.units.get(id)?.carried {
        return None;
    }
    let carry = Carry {
        id,
        ty,
        from: (c.x, c.y, c.z),
        facing: c.rotation,
        view,
    };
    let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
    world.lift_unit(at);
    if let Some(t) = tile {
        world.mark_rating(t);
    }
    world.zoo.entities[at].common.as_mut()?.movement_mode = 0;
    // A keeper told the exhibit he is no longer coming: he closes the gate he remembers.
    crate::staff::leaves(world, id);
    let unit = world.units.get_mut(id)?;
    unit.tank = 0;
    unit.carried = true;
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
pub fn put_down(world: &mut World, id: u32, point: (i32, i32, i32), facing: u32) -> bool {
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
    if let Some(unit) = world.units.get_mut(id) {
        unit.carried = false;
        // A guest put down has lost its wish to leave (guests 8).
        if let crate::unit::Part::Guest(g) = &mut unit.part {
            g.leave = false;
        }
    }
    crate::unit::swim::put(world, id);
    world.mark_rating(tile);
    true
}

#[cfg(test)]
mod tests;
