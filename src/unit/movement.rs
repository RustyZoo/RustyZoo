//! The move of a unit by the step (movement 3 to 5, 7.2, 8.2): the distance of the step, the
//! walk along the tiles of the path with its corners, the entry of a new tile, the end of a walk
//! and the height above the ground.
//!
//! Kept as the original has it: the remainder of the distance is lost at every step (engine issue
//! 301) and a long diagonal step of a unit that may not go diagonally does not move it (issue 302).
//! The move call of a behaviour set (8.2) starts a walk with the rate, the animation and the
//! animation speed of its speed band, and ends it. Entering a tile opens the gate ahead of a unit
//! that is not an animal and closes the gate behind it (6.2), and has a swimmer that changes
//! between land and water pick its move animation again. The gate plays its animation and sound
//! ([`super::gate`]).
//!
//! A unit in a tank (move kind 3) moves by [`swim`]'s 3D move instead, and the tank position check
//! runs after every move step.
//!
//! Not here yet: the escape of an animal (6.3), the repeat time and the reverse of a move call,
//! the index block of a unit larger than a tile (the map index holds the unit on one tile).

use super::{Kind, Part, Unit, swim};
use crate::behaviour::{Data, Flow, Gait, Number, Target};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::path;
use crate::render::order::ground_height;
use crate::sim::Rng;
use crate::world::World;

#[cfg(test)]
mod tests;

/// The eight directions as steps of a tile (movement 2).
pub(super) const DIR: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// The rate of a guest (7.2): its type's rate less 5 plus its individual `offset` (0 to 10), then
/// 10 % up for an energy of 75 or more and 10 % down for 19 or less, truncated. With `variable`
/// off (`[AI] VariableUnitSpeeds` is 0) the rate as it is.
pub fn guest_speed(rate: u32, offset: u32, energy: i32, variable: bool) -> u32 {
    if !variable {
        return rate;
    }
    let s = f64::from(rate) - 5.0 + f64::from(offset);
    let s = match energy {
        75.. => (s * 1.1) as i32,
        ..=19 => (s * 0.9) as i32,
        _ => s as i32,
    };
    s.max(0) as u32
}

/// The three speed bands of a move call (7.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Band {
    #[default]
    Slow,
    Medium,
    Fast,
}

impl Band {
    /// The band of a call; a call with its own animation falls back to the slow one (7.3).
    fn of(gait: &Gait) -> Band {
        match gait {
            Gait::Medium => Band::Medium,
            Gait::Fast => Band::Fast,
            _ => Band::Slow,
        }
    }

    /// The move animation of the band and the keys of its rate and of its animation speed.
    fn names(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Band::Slow => ("walk", "cSlowRate", "cSlowAnimSpeed"),
            Band::Medium => ("trot", "cMediumRate", "cMediumAnimSpeed"),
            Band::Fast => ("run", "cFastRate", "cFastAnimSpeed"),
        }
    }
}

/// Stops the walk: speed 0, no path, not moving, no call walking.
pub fn stop(unit: &mut Unit) {
    (unit.speed, unit.moving, unit.walking) = (0, false, false);
    (unit.segment, unit.vertical) = (None, 0);
    unit.path.clear();
    unit.animation.cut();
}

/// The tile of a point, if it is on the map.
fn tile_of(world: &World, (x, y): (i32, i32)) -> Option<usize> {
    world.tile_index(x.div_euclid(64), y.div_euclid(64))
}

/// The direction from tile `a` to tile `b` by the signs of the differences, -1 for the same tile.
pub(super) fn direction(world: &World, a: usize, b: usize) -> i32 {
    let width = world.zoo.width as usize;
    let d = |a: usize, b: usize| (b as i32 - a as i32).signum();
    let step = (d(a % width, b % width), d(a / width, b / width));
    DIR.iter().position(|&s| s == step).map_or(-1, |i| i as i32)
}

/// The margin of the corners: 20 movement units, 5 for a unit whose footprint covers more than
/// four cells (2).
fn margin(world: &World, at: usize) -> i32 {
    match world
        .shape(at)
        .is_some_and(|s| s.footprint.0 * s.footprint.1 > 4)
    {
        true => 5,
        false => 20,
    }
}

/// 4.3: the direction a unit that travelled in `travel` takes towards `wanted` at the sub-tile
/// point `(px, py)`: a right-angle turn slides diagonally until the unit is `m` units inside the
/// tile it should follow.
fn turning(travel: i32, wanted: i32, (px, py): (i32, i32), m: i32) -> i32 {
    if wanted % 2 != 0 {
        return wanted;
    }
    let side = |c: i32, low: i32, mid: i32, high: i32| match c {
        c if c > 64 - m => high,
        c if c >= m => mid,
        _ => low,
    };
    match (travel, wanted) {
        (0 | 4, 2) => side(py, 3, 2, 1),
        (0 | 4, 6) => side(py, 5, 6, 7),
        (2 | 6, 0) => side(px, 1, 0, 7),
        (2 | 6, 4) => side(px, 3, 4, 5),
        _ => wanted,
    }
}

/// 4.4: the displacement of a step of `distance` in direction `dir`. The shortening of a long
/// diagonal step of a unit that may not go diagonally multiplies the direction by a factor the
/// original truncates to 0: such a step does not move (issue 302).
fn displacement(dir: i32, distance: i32, diagonals: bool, m: i32) -> (i32, i32) {
    let Some(&(ux, uy)) = usize::try_from(dir).ok().and_then(|d| DIR.get(d)) else {
        return (0, 0);
    };
    if dir % 2 == 1 && !diagonals && distance >= (32 - m) * 2 {
        return (0, 0);
    }
    (ux * distance, uy * distance)
}

/// The move of one step of `ms` milliseconds for the unit at position `at` of the entity list
/// (3), then the check of its height (5).
pub fn step(unit: &mut Unit, world: &mut World, ms: u32, at: usize) {
    let before = here(world, at);
    // A hidden user walking out of its building is moved but stays off the map (guest buildings
    // 6.6); one that stays inside is not touched.
    let off_map = unit.hidden && unit.moving && unit.speed > 0;
    if unit.hidden && !off_map {
        return;
    }
    if unit.moving && unit.speed > 0 {
        // The remainder of the division is lost (issue 301).
        let distance = (unit.speed.wrapping_mul(ms) / 1000).max(1);
        let in_tank = world.zoo.entities[at]
            .common
            .as_ref()
            .is_some_and(|c| c.movement_mode == 3);
        let done = match in_tank {
            true => swim::progress(unit, world, distance, at),
            false => progress(unit, world, distance, at),
        };
        if done {
            unit.through_building = false;
        }
    }
    swim::check(unit, world, at);
    // The depth is told after the check has put a swimmer on the surface.
    let now = here(world, at);
    if now != before && now.is_some_and(|t| world.tank_at(t).is_some()) {
        let z = point(world, at).map_or(0, |p| p.2);
        world.units.lines.push(format!("depth {} {z}", unit.id));
    }
    snap(world, at);
    if off_map {
        world.lift_unit(at);
    }
}

/// The end of a walk (8.2 step 3): the path is empty, or the unit stands in its destination tile.
fn ended(unit: &Unit, world: &World, at: usize) -> bool {
    let target = unit
        .destination
        .and_then(|(x, y, _)| tile_of(world, (x, y)));
    // In a tank the unit has arrived at the destination's level too (goals 8.2).
    let cell = |(x, y, z): (i32, i32, i32)| (x.div_euclid(64), y.div_euclid(64), z.div_euclid(16));
    let swimming = world.zoo.entities[at]
        .common
        .as_ref()
        .is_some_and(|c| c.movement_mode == 3);
    let arrived = match (swimming, point(world, at), unit.destination) {
        (true, Some(p), Some(d)) => cell(p) == cell(d),
        _ => here(world, at).is_some() && here(world, at) == target,
    };
    unit.path.is_empty() || arrived
}

/// The tile of the unit at position `at` of the entity list.
fn here(world: &World, at: usize) -> Option<usize> {
    point(world, at).and_then(|(x, y, _)| tile_of(world, (x, y)))
}

/// Shows the move animation of the unit's band, or `own` (the name and the percentage a call
/// gives), and sets the animation speed, the one of the water on water (7.3). False when the type
/// has no animation of that name.
fn animate(
    unit: &mut Unit,
    world: &World,
    data: &dyn Data,
    at: usize,
    own: Option<(&str, i32)>,
) -> bool {
    let (walk, _, speed) = unit.band.names();
    let entity = &world.zoo.entities[at];
    let water = here(world, at).is_some_and(|t| world.is_water(t));
    // A swimmer on water shows `swim` in every band; a call's own name is replaced for an animal
    // only (7.3).
    let egg = matches!(&unit.part, Part::Animal(a) if a.egg);
    let swims = unit.mover.swims && !egg && water;
    let forced = swim::move_name(unit).or(swims.then_some("swim"));
    let name = match (own, forced) {
        (Some((name, _)), f) if f.is_none() || unit.kind != Kind::Animal => name,
        (_, Some(name)) => name,
        _ => walk,
    };
    if data.timing(entity, name).is_none() {
        return false;
    }
    let key = format!(
        "{speed}{}",
        if water || unit.tank > 2 { "Water" } else { "" }
    );
    // The data tells a type without the key from one that says 0 by neither: the default is 100.
    let table = match data.number(entity, &Number::Characteristic(key)) {
        0 => 100,
        p => p,
    };
    // Kept in 16 bits.
    unit.anim_speed = own.map_or(table, |o| o.1) as u16;
    unit.animation.base_to(name);
    true
}

/// Picks the move animation again after a swimmer changed between land and water (6.2), or the
/// idle one when the unit stands (tank fauna 4).
pub fn reanimate(unit: &mut Unit, world: &World, data: &dyn Data, at: usize) {
    if !std::mem::take(&mut unit.flip) {
        return;
    }
    if unit.walking {
        animate(unit, world, data, at, None);
        unit.animation.cut();
    } else if !unit.moving {
        idle(unit, world, data, at);
    }
}

/// The unit shows its idle animation again (8.2 step 1; the normal end of a walk keeps the move
/// animation, step 3).
fn idle(unit: &mut Unit, world: &World, data: &dyn Data, at: usize) {
    let entity = &world.zoo.entities[at];
    let name = swim::idle_name(unit);
    if data.timing(entity, name).is_some() {
        unit.animation.base_to(name);
    }
    unit.animation.cut();
}

/// The move call `gait` of the unit's set, run once per step (8.2).
pub fn call(
    unit: &mut Unit,
    world: &mut World,
    data: &dyn Data,
    rng: &mut Rng,
    at: usize,
    (gait, target): &(Gait, Target),
) -> Flow {
    if unit.walking {
        // 2: the unit moves from the next step's move on, once the player shows the move
        // animation. 3: the call ends with the walk, an aborted one included (4.7).
        if !unit.moving && unit.animation.shows_base() {
            unit.moving = true;
        }
        if !unit.aborted && !ended(unit, world, at) {
            return Flow::Running;
        }
        stop(unit);
        return Flow::Done;
    }
    // The 3D target is recorded while the unit's goal is the plain wander: `0` and the name it
    // has leave it alone, any other first argument clears the goal's destination and is recorded
    // (behaviours 5.4, tank fauna 6.2).
    if unit.goal.record.task == crate::goal::RANDOM
        && !matches!(*target, Target::Keep)
        && *target != unit.target
    {
        unit.goal.record.target.clear();
        unit.target = *target;
    }
    // 1: a new move. A unit with a destination to go to waits for the path its goal finds; one
    // with none, or standing in it, walks at random.
    if unit.path.is_empty() {
        let wants = (unit.destination)
            .and_then(|(x, y, _)| tile_of(world, (x, y)))
            .is_some_and(|d| Some(d) != here(world, at));
        if wants {
            return Flow::Running;
        }
        unit.aborted = false;
        if !super::walk::random_walk(unit, world, rng, at) {
            return Flow::Failed;
        }
    }
    if unit.path.len() < 2 {
        stop(unit);
        idle(unit, world, data, at);
        return Flow::Done;
    }
    unit.band = Band::of(gait);
    let entity = &world.zoo.entities[at];
    let (rate, own) = match gait {
        Gait::Own {
            anim,
            speed,
            percent,
            ..
        } => (
            data.number(entity, speed),
            Some((anim.as_str(), data.number(entity, percent))),
        ),
        _ => {
            let key = Number::Characteristic(unit.band.names().1.into());
            (data.number(entity, &key), None)
        }
    };
    if !animate(unit, world, data, at, own) {
        stop(unit);
        return Flow::Failed;
    }
    // A guest's own pace (7.2) needs its offset and energy, which the guest issues bring.
    unit.speed = rate.max(0) as u32;
    unit.walking = true;
    Flow::Running
}

fn point(world: &World, at: usize) -> Option<(i32, i32, i32)> {
    let c = world.zoo.entities.get(at)?.common.as_ref()?;
    Some((c.x, c.y, c.z))
}

/// A walking unit on the ground stands on it (5): its height is the ground's at its point. Not
/// done in a tank, where the unit swims.
fn snap(world: &mut World, at: usize) {
    let Some(c) = world.zoo.entities.get(at).and_then(|e| e.common.as_ref()) else {
        return;
    };
    let (x, y, rotation) = (c.x, c.y, c.rotation);
    let Some(t) = tile_of(world, (x, y)) else {
        return;
    };
    if c.movement_mode == 3 || world.tank_at(t).is_some() {
        return;
    }
    let ground = ground_height(&world.zoo.tiles[t], x.rem_euclid(64), y.rem_euclid(64));
    world.place_unit(at, (x, y, ground), rotation);
}

/// Whether the tile step from `here` to `next` is still allowed (4.7).
fn allowed(unit: &Unit, world: &World, at: usize, here: usize, next: usize) -> bool {
    let Ok(d) = usize::try_from(direction(world, here, next)) else {
        return false;
    };
    if world.neighbour(here, d) != Some(next) {
        return false;
    }
    let mover = mover(unit, world, at, here);
    path::allowed(
        world,
        &mover,
        &world.units.finder.settings,
        (here, next, d),
        unit.through_building,
    )
}

/// Tells the finder whether the unit at position `at` of the entity list is in a tank and at which
/// level the depth search starts (and ends, until a destination says otherwise); the start node is
/// entered with the unit's facing and its `vertical` direction (pathfinding 3.6).
pub fn aim(m: &mut path::Mover, world: &World, at: usize, vertical: i32) {
    let c = world.zoo.entities.get(at).and_then(|e| e.common.as_ref());
    m.deep = c.is_some_and(|c| c.movement_mode == 3);
    m.start = (c.map_or(0, |c| c.rotation as usize), vertical);
    let level = c.map_or(0, |c| c.z.div_euclid(16));
    m.levels = (level, level);
}

/// What the costs of the path finder read of the unit's type (the characteristics of 7.1 and of
/// the pathfinding specification).
pub fn read_type(unit: &mut Unit, data: &dyn Data, entity: &crate::zoo::Entity) {
    let flag = |key: &str| data.number(entity, &Number::Characteristic(key.into())) == 1;
    let m = &mut unit.mover;
    (m.swims, m.underwater, m.only_underwater) =
        (flag("cSwims"), flag("cUnderwater"), flag("cOnlyUnderwater"));
    (m.tall, m.climbs) = (flag("cTall"), flag("cClimbsCliffs"));
    m.surface = flag("cSurface");
    m.depth = data.number(entity, &Number::Characteristic("cDepth".into()));
    m.avoid_edges = data
        .number(entity, &Number::Characteristic("cAvoidEdges".into()))
        .max(0) as u32;
    // The guest's costs off the paths are the unit class file's; a missing key keeps the fallback.
    if unit.kind == Kind::Guest {
        let cost = |key, fallback: u32| data.unit_class(key).map_or(fallback, |v| v.max(0) as u32);
        m.non_path_cost = cost("cNonPathCost", m.non_path_cost);
        m.emergency_cost = cost("cNonPathCostEmergency", m.emergency_cost);
    }
}

/// Sets the fields of the path finder's `Mover` that depend on a unit's task (pathfinding 6.5,
/// 6.6): a guest on its way out (0x71), avoiding a predator (0x75), recovering from a fall into
/// water (0x83) or entering a building (0x68, both recovering) or travelling in water (0x83 and
/// 0x82), and an animal chasing (0x74, `target` is
/// the unit chased) or on a show trip (0x85, 0x86). The other kinds ignore the tasks.
pub fn task(m: &mut path::Mover, task: u8, target: u32) {
    use crate::goal::{
        AVOID, CHASE, ENTER_BUILDING, GO_TO_SHOW, LEAVE, RECOVER, RETURN_FROM_SHOW,
        RETURN_TO_BUILDING,
    };
    let (guest, animal) = (m.kind == path::Kind::Guest, m.kind == path::Kind::Animal);
    m.leaving = guest && task == LEAVE;
    m.avoiding = guest && task == AVOID;
    m.recovering = guest && [ENTER_BUILDING, RECOVER].contains(&task);
    m.in_water = guest && [RETURN_TO_BUILDING, RECOVER].contains(&task);
    m.show = animal && [GO_TO_SHOW, RETURN_FROM_SHOW].contains(&task);
    m.chasing = (animal && task == CHASE && target != 0).then_some(target);
}

/// The unit as the costs of the path finder see it, standing on tile `here`: its place, its
/// destination tile now (before a request sets a new one), the building flag, its task and, for an
/// animal, its terrain mode. The values of its type come from [`read_type`]; the relaxed and test
/// flags of a keeper and an animal are the caller's, on `unit.mover`.
pub(crate) fn mover(unit: &Unit, world: &World, at: usize, here: usize) -> path::Mover {
    let mut m = unit.mover.clone();
    (m.at, m.id) = (Some(here), Some(unit.id));
    aim(&mut m, world, at, unit.vertical);
    m.footprint = world
        .shape(at)
        .map_or(1, |s| (s.footprint.0 * s.footprint.1).clamp(1, 4) as u32);
    m.dest = unit
        .destination
        .and_then(|(x, y, _)| tile_of(world, (x, y)));
    m.building |= unit.through_building;
    m.target_building = match &unit.part {
        Part::Guest(g) if g.building != 0 && world.find(g.building).is_some() => Some(g.building),
        _ => None,
    };
    let record = &unit.goal.record;
    task(&mut m, record.task, record.target.id);
    m.water_mode = matches!(&unit.part, Part::Animal(a) if a.terrain == 1);
    m
}

/// Whether the step of a unit in a tank from level `za` of tile `a` to level `zb` of tile `b` in
/// direction `d` is still allowed (tank fauna 8.2).
pub(super) fn allowed_deep(
    unit: &Unit,
    world: &World,
    at: usize,
    ((a, za), (b, zb)): ((usize, i32), (usize, i32)),
    d: usize,
) -> bool {
    let mover = mover(unit, world, at, a);
    path::allowed_deep(
        world,
        &mover,
        &world.units.finder.settings,
        ((a, za), (b, zb), d),
        unit.through_building,
    )
}

/// 4.2: one step of the walk along the path by `distance`; true when the move is done.
fn progress(unit: &mut Unit, world: &mut World, mut distance: u32, at: usize) -> bool {
    if distance > 64 {
        distance = 16;
    }
    if unit.path.is_empty() {
        return true;
    }
    let Some((x, y, z)) = point(world, at) else {
        return true;
    };
    let here = tile_of(world, (x, y));
    let dest = unit
        .destination
        .and_then(|(x, y, _)| tile_of(world, (x, y)));
    let (Some(here), Some(dest)) = (here, dest) else {
        unit.path.clear();
        return true;
    };
    let first = unit.path[0];
    if first >= world.zoo.tiles.len() {
        unit.path.clear();
        return true;
    }
    let arrived = here == dest;
    // A guest goes diagonally only in a tank (4.3).
    let diagonals = world.units.finder.settings.diagonal
        && (unit.kind != Kind::Guest || world.tank_at(here).is_some());
    let wanted = if arrived {
        unit.travel
    } else if diagonals && here != first {
        direction(world, here, first)
    } else if unit.path.len() == 1 {
        -1
    } else {
        let next = unit.path[1];
        if !(next < world.zoo.tiles.len() && allowed(unit, world, at, here, next)) {
            if !std::mem::replace(&mut unit.aborted, true) {
                world.units.lines.push(format!("abort {}", unit.id));
            }
            return true;
        }
        direction(world, here, next)
    };
    let m = margin(world, at);
    let dir = turning(unit.travel, wanted, (x.rem_euclid(64), y.rem_euclid(64)), m);
    let (dx, dy) = displacement(dir, distance as i32, diagonals, m);
    let (nx, ny) = (x + dx, y + dy);
    let entered = tile_of(world, (nx, ny));
    // The unit does not leave its last tile, unless it may go diagonally and the tile it enters is
    // the one it aims at: the way back from a corner cut (4.2, 13.7).
    let back = diagonals && !arrived && here != first && entered == Some(first);
    if entered != Some(here) && ((unit.path.len() == 1 && !back) || entered.is_none()) {
        unit.path.clear();
        return true;
    }
    let old = world.zoo.entities[at]
        .common
        .as_ref()
        .map_or(0, |c| c.rotation);
    let facing = u32::try_from(dir).unwrap_or(old);
    world.place_unit(at, (nx, ny, z), facing);
    if facing != old {
        world.units.lines.push(format!("face {} {facing}", unit.id));
    }
    if let Some(new) = entered.filter(|&n| n != here) {
        enter(unit, world, here, new);
    }
    if arrived {
        unit.path.clear();
    }
    arrived
}

/// 4.5 and 6.2: the unit crossed from tile `old` into tile `new`: its travel direction, the old
/// tile off the path, the exhibit it entered, the swimmer's change of animation and the gates.
pub(super) fn enter(unit: &mut Unit, world: &mut World, old: usize, new: usize) {
    unit.travel = direction(world, old, new);
    if unit.path.first() == Some(&old) {
        unit.pop_node();
    }
    let width = world.zoo.width as usize;
    world
        .units
        .lines
        .push(format!("tile {} {},{}", unit.id, new % width, new / width));
    let (before, after) = (world.exhibits().region(old), world.exhibits().region(new));
    if let Some(r) = after.filter(|&r| r != GROUNDS && r != EXTERIOR && after != before) {
        world.units.lines.push(format!("enter {} {r}", unit.id));
    }
    unit.flip |= unit.mover.swims && world.is_water(old) != world.is_water(new);
    // The gate on the way (towards the path's second entry) is opened (not by an animal), the
    // one just crossed is closed.
    let ahead = (unit.path.get(1)).and_then(|&n| gate(world, new, direction(world, new, n)));
    if let Some(g) = ahead.filter(|_| unit.kind != Kind::Animal) {
        world.units.lines.push(format!("gate {} open {g}", unit.id));
        world.units.open_gate(g);
        unit.gate = Some(g);
    }
    if let Some(g) = gate(world, new, (unit.travel + 4) % 8) {
        world
            .units
            .lines
            .push(format!("gate {} close {g}", unit.id));
        world.units.close_gate(g);
        if unit.gate == Some(g) {
            unit.gate = None;
        }
    }
}

/// The id of the gate on the edge of tile `t` in direction `d`, on either side of the edge. A
/// diagonal `d` looks at the edge slot `d / 2` of the tile, then at slot `(d + 4) % 8 / 2` of the
/// neighbour in that direction (movement 6.2).
fn gate(world: &World, t: usize, d: i32) -> Option<u32> {
    let d = usize::try_from(d).ok().filter(|&d| d < 8)?;
    let across = world
        .neighbour(t, d)
        .and_then(|n| world.edge(n, (d + 4) % 8));
    let piece = world.edge(t, d).or(across)?;
    let id = world.zoo.entities[piece].common.as_ref()?.id;
    world.is_door(piece).then_some(id)
}
