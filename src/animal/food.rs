//! Hunger, food and eating (animals-needs 4 and 5).
//!
//! The hunger check and `fEat` run among the checks of an animal's status tick ([`hunger`],
//! [`eat`]) on a [`Meal`]: what the eating reads of the food and of the animal's sets, and what it
//! asks of them (the bites, the set to start or queue, the goal to reset). [`apply`] does what the
//! meal asks once the checks are done; the keeper food goal calls [`feed`] for the first bite.
//!
//! A food entity of the class `food` keeps its units left in the `food` field of its common part
//! (zoo format 6.6) and shows `full`, `half` or `empty` by thirds of the type's `cMaxFoodUnits`.
//!
//! The log line `animal eat <animal> <food> <left> <state>` follows each bite.

use super::{Animal, Env, Keys, NO_FOOD, Out, find, fire};
use crate::behaviour::{Data, Env as Sets};
use crate::build::number;
use crate::entity::Catalogue;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::goal::Cx;
use crate::sim::Rng;
use crate::unit::{Ctx, Part, Unit};
use crate::world::World;
use crate::zoo::Entity;

/// The animations of a food entity by fullness (5.1).
pub const STATES: [&str; 3] = ["empty", "half", "full"];

/// The fullness of a pile: 0 below a third, 1 below two thirds, else 2 (5.1).
pub fn fullness(left: i32, max: i32) -> usize {
    let part = f64::from(left) / f64::from(max.max(1));
    match part {
        p if p < 0.3333 => 0,
        p if p < 0.6667 => 1,
        _ => 2,
    }
}

/// The full amount of a food type (`cMaxFoodUnits`, 0 when absent).
pub fn max_units(cat: &Catalogue, e: &Entity) -> i32 {
    find(cat, e)
        .and_then(|t| number(t, "cMaxFoodUnits"))
        .unwrap_or(0.0) as i32
}

/// The kind of food a type holds or an animal eats (`cKeeperFoodType`).
fn kind(cat: &Catalogue, e: &Entity) -> i32 {
    find(cat, e)
        .and_then(|t| number(t, "cKeeperFoodType"))
        .unwrap_or(0.0) as i32
}

/// What eating reads of the world and asks of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meal {
    /// The units left in the food it aims at, 0 for none; it falls with each bite.
    pub left: i32,
    /// It stands on a water tile; the current set is `bEat`; the type has a set `bEat`.
    pub wet: bool,
    pub in_set: bool,
    pub has_set: bool,
    /// Asked of the world: the units taken, `bEat` started, `bEat` queued after the current set,
    /// and the goal reset with the queued set forgotten (5.4).
    pub bites: i32,
    pub start: bool,
    pub queue: bool,
    pub stop: bool,
}

impl Animal {
    /// `stopEating` (5.4): the need flags of the reset goal go down too.
    pub fn stop_eating(&mut self, found: bool, meal: &mut Meal) {
        (self.no_food, self.eating) = (found, false);
        (self.wants_food, self.thirsty) = (false, false);
        (meal.stop, meal.queue) = (true, false);
    }
}

/// `fEat` (5.3): a bite of the food it aims at. Its units fall, the pending hunger is set to the
/// bite's worth (an assignment: the increment of this tick is lost, 5.5).
pub fn eat(a: &mut Animal, k: &Keys, m: &mut Meal) {
    let bite = m.left.min(k.bite).max(0);
    if bite == 0 || m.wet {
        return a.stop_eating(false, m);
    }
    if !m.in_set {
        if !m.has_set {
            return;
        }
        (m.start, m.in_set) = (true, true);
    }
    (a.no_food, a.eating) = (false, true);
    m.left -= bite;
    m.bites += bite;
    a.hunger_change = k.food_value.saturating_mul(bite).saturating_neg();
    m.queue = m.left > 0;
}

/// The hunger check (4): the rise, and what a hungry animal does about food.
pub fn hunger(a: &mut Animal, k: &Keys, class: &super::Class, env: &Env, out: &mut Out) {
    if !fire(&mut a.counters[super::HUNGER], class.periods[super::HUNGER]) {
        return;
    }
    a.hunger_change = a.hunger_change.saturating_add(k.hunger_increment);
    // The test sees the hunger before this check's rise. Sleeping is a later issue's flag.
    if a.eating {
        eat(a, k, &mut out.meal);
    } else if a.hunger >= k.hunger_threshold && !(a.wants_food || env.food_task || a.rampaging) {
        match env.food_total >= 1 {
            true if !a.in_show => (a.wants_food, a.thirsty) = (true, false),
            true => {}
            false => no_food(a, k, out),
        }
    }
}

/// 4.3: no food for a hungry animal.
fn no_food(a: &mut Animal, k: &Keys, out: &mut Out) {
    if !a.no_food {
        (a.no_food, a.eating) = (true, false);
    } else if !a.in_show {
        out.messages.push(NO_FOOD);
        if a.hunger >= 100 {
            a.change(k.no_food_change);
            a.health_change = a.health_change.saturating_sub(k.hungry_health);
        }
    }
    a.wants_food = false;
}

/// The exhibit a tile belongs to, if it is not the grounds or outside the zoo.
pub fn exhibit_of(world: &World, tile: usize) -> Option<u32> {
    let region = world.exhibits().region(tile)?;
    Some(region).filter(|&r| r != GROUNDS && r != EXTERIOR)
}

/// The exhibits whose food an animal in `x` eats: its own, and the amphibious neighbours for an
/// amphibious species (4.1).
pub fn near(world: &World, x: u32, k: &Keys) -> Vec<u32> {
    let neighbours = world.exhibits().get(x).map(|e| e.amphibious.clone());
    let neighbours = neighbours.filter(|_| k.amphibious).unwrap_or_default();
    std::iter::once(x).chain(neighbours).collect()
}

/// A piece of food: its id, its place in the entity list, the tile it was found on and its units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub id: u32,
    pub at: usize,
    pub tile: usize,
    pub left: i32,
}

/// The pieces of food of `kind` in the exhibits `regions`, in the tile order of the exhibit and
/// the slot order of the tile (habitat rating 5.1).
pub fn pieces(world: &World, cat: &Catalogue, regions: &[u32], wanted: i32) -> Vec<Piece> {
    let mut found: Vec<Piece> = Vec::new();
    let tiles = regions.iter().filter_map(|&x| world.exhibits().get(x));
    for &tile in tiles.flat_map(|e| &e.order) {
        for e in (0..4).filter_map(|q| world.quadrant(tile, q)) {
            let entity = &world.zoo.entities[e];
            if entity.class != "food" || found.iter().any(|p| p.at == e) {
                continue;
            }
            if let Some(c) = entity
                .common
                .as_ref()
                .filter(|_| kind(cat, entity) == wanted)
            {
                found.push(Piece {
                    id: c.id,
                    at: e,
                    tile,
                    left: c.food,
                });
            }
        }
    }
    found
}

/// 11.8: the piece the keeper food goal goes to: with a chance of 1 in 4 (`rand & 3 == 0`) a
/// random one, else with a chance of 1 in 4 the one with the least left, else the nearest by
/// `distance` of its tile. The first of equals wins. Nothing is drawn when there is no piece.
pub fn choose(pieces: &[Piece], distance: impl Fn(usize) -> i64, rng: &mut Rng) -> Option<Piece> {
    if pieces.is_empty() {
        return None;
    }
    if rng.rand() & 3 == 0 {
        return pieces.get(rng.pick(pieces.len() as u32) as usize).copied();
    }
    if rng.rand() & 3 == 0 {
        return pieces.iter().min_by_key(|p| p.left).copied();
    }
    pieces.iter().min_by_key(|p| distance(p.tile)).copied()
}

/// What eating reads of the animal at position `at`: the food it aims at and its place.
pub fn meal(unit: &Unit, world: &World, data: &dyn Data, at: usize) -> Meal {
    let entity = &world.zoo.entities[at];
    let Part::Animal(a) = &unit.part else {
        return Meal::default();
    };
    let food = world.find(a.food).map(|i| &world.zoo.entities[i]);
    let tile = entity
        .common
        .as_ref()
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    Meal {
        left: (food.filter(|f| f.class == "food"))
            .map_or(0, |f| f.common.as_ref().map_or(0, |c| c.food)),
        wet: tile.is_some_and(|t| world.is_water(t)),
        in_set: unit.runner.top() == Some("bEat"),
        has_set: data.set(entity, "bEat").is_some(),
        ..Meal::default()
    }
}

/// Does what a meal asked: `bEat` started or queued, the goal reset, the food's units taken.
pub fn apply(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    m: Meal,
) {
    if m.start {
        let env = Sets {
            data,
            unit: &world.zoo.entities[at],
            rng: &mut *ctx.rng,
        };
        unit.runner
            .start(&mut unit.animation, &env, "bEat", true, false, true);
    }
    if m.queue {
        unit.runner.request("bEat");
    }
    if m.stop {
        unit.runner.forget();
        let mut cx = Cx {
            unit,
            world,
            ctx,
            at,
            data,
        };
        cx.reset();
        cx.unit.goal.wants_to_decide = true;
    }
    let Part::Animal(a) = &unit.part else {
        return;
    };
    let Some(i) = world.find(a.food).filter(|_| m.bites > 0) else {
        return;
    };
    let (id, max) = (a.food, max_units(ctx.cat, &world.zoo.entities[i]));
    // At 0 the food goes at the next removal pass.
    world.set_food(i, m.left);
    if m.left <= 0 {
        ctx.remove(world, id);
    }
    let line = format!(
        "eat {} {id} {} {}",
        unit.id,
        m.left,
        STATES[fullness(m.left, max)]
    );
    world.units.tell(line);
}

/// `fEat` outside the checks, from the goal that reached the food.
pub fn feed(cx: &mut Cx) {
    let k = super::keys(cx.world, cx.ctx.cat, cx.at);
    let mut m = meal(cx.unit, cx.world, cx.data, cx.at);
    if let Part::Animal(a) = &mut cx.unit.part {
        eat(a, &k, &mut m);
    }
    apply(cx.unit, cx.world, cx.ctx, cx.at, cx.data, m);
}

/// The direction of the eight that is nearest to the offset `(dx, dy)` in tiles, as the
/// original's arc tangent gives it, by the signs and the slope (22.5 degrees is a slope of
/// 0.41421356). Both 0 gives none.
pub(super) fn nearest_direction(dx: i32, dy: i32) -> Option<u32> {
    let (ax, ay) = (i64::from(dx).abs(), i64::from(dy).abs());
    let side = |long: i64, short: i64| short * 100_000_000 < long * 41_421_356;
    let step = match () {
        _ if side(ax, ay) => (dx.signum(), 0),
        _ if side(ay, ax) => (0, dy.signum()),
        _ => (dx.signum(), dy.signum()),
    };
    let all = [
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ];
    all.iter().position(|&d| d == step).map(|d| d as u32)
}

/// `fFaceTowardFood`: the animal turns to the tile of the food it aims at; nothing without one.
pub fn face(unit: &Unit, world: &mut World, at: usize) {
    let Part::Animal(a) = &unit.part else {
        return;
    };
    let tile = |e: &Entity| {
        e.common
            .as_ref()
            .map(|c| (c.x.div_euclid(64), c.y.div_euclid(64)))
    };
    let food = world
        .find(a.food)
        .and_then(|i| tile(&world.zoo.entities[i]));
    let me = &world.zoo.entities[at];
    let (Some(food), Some(c), Some(here)) = (food, me.common.as_ref(), tile(me)) else {
        return;
    };
    let Some(facing) = nearest_direction(food.0 - here.0, food.1 - here.1) else {
        return;
    };
    let (point, old) = ((c.x, c.y, c.z), c.rotation);
    world.place_unit(at, point, facing);
    if facing != old {
        world.units.note(format!("face {} {facing}", unit.id));
    }
}
