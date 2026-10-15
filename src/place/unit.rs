//! Placing animals and staff (unit placing 5 to 7, 9.3 and 10.2): the pending unit and the checks
//! of its tile, the purchase, the name from a counter per type name, the crate's tile and the
//! sell price.
//!
//! The pending unit is a [`Pending`] like an object's, snapped as a 1 x 1 object and facing a
//! fixed screen direction. [`pending`] runs the checks of 6.1 in order, the first failure keeping
//! its message; a staff member's (6.3) are those of an animal without the ones that are about an
//! exhibit, the water message, the slope and the size. The tank point (5.1) is not done: a tank
//! tile is never valid, and says why.

use super::{
    NO_FUNDS, NOT_IN_THE_ZOO, Outcome, Pending, Verdict, entity, facing, fail, fenced, path_shape,
    snap, uneven,
};
use crate::build::{cost, number, plain};
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::GROUNDS;
use crate::money::{ANIMAL_PURCHASE, Money};
use crate::place::Class;
use crate::render::order::{Kind, ground_height};
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

/// The pending animal of the type `ty` for the world point `point` under the mouse, the view
/// turned by `view`; `None` when the type is unknown or the point is off the map. `water` tells
/// for each terrain type whether it is water.
pub fn pending(
    world: &World,
    cat: &Catalogue,
    water: &[bool],
    ty: usize,
    view: u8,
    point: (i64, i64),
    october: bool,
) -> Option<Pending> {
    let t = cat.types.get(ty)?;
    let point = (i32::try_from(point.0).ok()?, i32::try_from(point.1).ok()?);
    let (tile_x, tile_y) = (point.0.div_euclid(64), point.1.div_euclid(64));
    let tile = world.tile_index(tile_x, tile_y)?;
    let dir = facing(0, view);
    let (x, y) = snap(Class::Fixed, point, (1, 1), dir);
    let (lx, ly) = (x - 64 * tile_x, y - 64 * tile_y);
    let mut p = Pending {
        ty,
        x,
        y,
        z: ground_height(&world.zoo.tiles[tile], lx, ly),
        facing: dir,
        tile: (tile_x, tile_y),
        quadrant: (ly / 32 * 2 + lx / 32).clamp(0, 3) as usize,
        verdict: Verdict::Valid,
        message: None,
        price: price(t, october),
        paint: None,
        blocking: Vec::new(),
        wall: 0,
    };
    check(&mut p, world, t, water);
    Some(p)
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

/// The checks of 6.1 for an animal that is not crated, or of 6.3 for a staff member, in order.
fn check(p: &mut Pending, world: &World, t: &EntityType, water: &[bool]) {
    let staff = staff::is_staff(t);
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
    if !passable || units.iter().any(overlaps) {
        fail(p, None);
    }
    // Flat, or raised along one side only, unless the animal is small.
    if !staff && fx * fy >= 4 && !path_shape(world.zoo.tiles[tile].shape) {
        fail(p, None);
    }
    let wet = water
        .get(usize::from(world.zoo.tiles[tile].terrain))
        .copied()
        .unwrap_or(false);
    if wet && !swims {
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
    if !staff && region.is_none_or(|r| r == GROUNDS) {
        fail(p, Some(if only { NEEDS_WATER } else { IN_AN_EXHIBIT }));
    }
    let tank = world.tank_at(tile);
    if let Some(id) = tank {
        if let Some(message) = tank_message(world, id, tile, t) {
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
    if tank.is_some() {
        fail(p, None);
    }
}

/// The message of the tank check of 6.5 for an animal on `tile` of tank `id`: the last row that
/// applies.
fn tank_message(world: &World, id: u32, tile: usize, t: &EntityType) -> Option<i32> {
    let tank = world.tank(id)?;
    let flag = |key: &str| number(t, key) == Some(1.0);
    let (under, staff) = (flag("cUnderwater"), staff::is_staff(t));
    let inside = world
        .gate_slot(id)
        .and_then(|(g, side)| match world.tank_at(g) == Some(id) {
            true => Some(g),
            false => world.neighbour(g, side),
        });
    let depth = number(t, "cDepth").unwrap_or(0.0);
    let mut message = None;
    // Only a keeper stands at the ladder; the filled tank and the depth are an animal's.
    if inside == Some(tile) && t.class != "keeper" {
        message = Some(LADDER);
    }
    if under && !staff && (!tank.filled || tank.level < 1) {
        message = Some(NEEDS_WATER);
    }
    if depth > tank.level as f32 {
        message = Some(DEEPER);
    }
    if !under {
        message = Some(if staff { NOT_FOR_STAFF } else { NOT_IN_TANKS });
    }
    if tank.show && plain(t, "cShow") != Some(1.0) {
        message = Some(NOT_FOR_SHOWS);
    }
    message
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

#[cfg(test)]
mod tests;
