//! The checks of the animals that live in tanks (tank fauna 10, 11; animals-needs 7.2): the
//! breath, the murky water and the water check that moves an animal between land, water and tank.

use super::{Animal, BREATH, Class, Env, Keys, MURKY, WATER, fire};
use crate::behaviour::Data;
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::sim::Rng;
use crate::unit::{Ctx, Part, Unit};
use crate::world::World;

/// 10.1: every `cBreathCheck` status ticks an animal under water (tank mode 4) gains breath. The
/// counter restarts only when it runs out under water: out of it the check never runs again (the
/// original's bug, 10.3, kept).
pub(crate) fn breath(a: &mut Animal, k: &Keys, class: &Class, env: &Env) {
    let was = a.counters[BREATH];
    a.counters[BREATH] = was.wrapping_sub(1);
    if was != 0 || env.tank_mode != 4 {
        return;
    }
    a.counters[BREATH] = class.periods[BREATH].wrapping_sub(1);
    a.breath = a.breath.saturating_add(k.breath_increment).clamp(0, 100);
}

/// 10.2: whether the animal has to go to the surface: it is under water (tank mode 4) and has the
/// breath of its threshold (`cBreathThreshold`, 100 without the key; a threshold of 0 or less is
/// always reached).
pub(crate) fn needs_air(breath: i32, k: &Keys, tank_mode: u8) -> bool {
    tank_mode == 4 && breath >= k.breath_threshold
}

/// 11: every `cMurkyWaterCheck` status ticks an animal in a tank loses happiness and health by the
/// band of the purity, happiness when the water level is outside its depths (not in a show tank)
/// and both when the water is of the wrong kind.
pub(crate) fn murky(a: &mut Animal, k: &Keys, class: &Class, env: &Env) {
    if !fire(&mut a.counters[MURKY], class.periods[MURKY]) {
        return;
    }
    let Some(tank) = env.tank else {
        return;
    };
    let purity = i64::from(tank.purity);
    let bands = [
        (
            k.extremely_murky,
            k.extremely_murky_change,
            k.extremely_murky_health,
        ),
        (k.very_murky, k.very_murky_change, k.very_murky_health),
        (k.murky, k.murky_change, k.murky_health),
    ];
    if let Some(&(_, happiness, health)) = bands.iter().find(|b| purity < i64::from(b.0)) {
        a.change(happiness);
        a.health_change = a.health_change.saturating_add(health);
    }
    let level = i64::from(tank.level);
    let outside = level < i64::from(k.depth_min) || level > i64::from(k.depth_max);
    if !tank.show && !a.in_show && k.depth_min != k.depth_max && outside {
        a.change(k.depth_change);
    }
    if tank.salt != k.salt {
        a.change(k.salinity_change);
        a.health_change = a.health_change.saturating_add(k.salinity_health);
    }
}

/// `rand() % 100 < percent`: one draw.
fn roll(rng: &mut Rng, percent: i32) -> bool {
    (rng.pick(100) as i32) < percent
}

/// The water check (animals-needs 7.2): by its terrain mode an animal may go to the tank mode, the
/// water mode or the land mode, with the draws and the counts of the tiles of its exhibit and its
/// amphibious neighbours. It answers the mode asked for and, when the animal is to be sent to a
/// tile of that kind (tank fauna 5), the draw that picks the tile (the caller [`switch`]es).
pub(crate) fn water(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
) -> Option<(u8, Option<usize>)> {
    if a.egg || a.in_show || !fire(&mut a.counters[WATER], class.periods[WATER]) {
        return None;
    }
    // A sick or escaped animal takes no part in the first two rules, nor in the draw of the land
    // rule.
    let sick = a.at_minimum_health || a.escaped;
    let [land, water, tank] = env.kinds;
    let over = |count: usize, needed: i32| count as i64 > i64::from(needed);
    let tank_rule = |rng: &mut Rng| {
        !sick && k.underwater && roll(rng, k.enter_tank) && over(tank, k.underwater_needed)
    };
    let water_rule =
        |rng: &mut Rng| !sick && k.swims && roll(rng, k.enter_water) && over(water, k.water_needed);
    // A sick or escaped animal skips the draw of the land rule.
    let land_rule = |rng: &mut Rng| (sick || roll(rng, k.enter_land)) && over(land, k.land_needed);
    let mode = match a.terrain {
        0 if tank_rule(rng) => 2,
        0 if water_rule(rng) => 1,
        1 if land_rule(rng) => 0,
        1 if tank_rule(rng) => 2,
        2 if !k.only_underwater && land_rule(rng) => 0,
        2 if !k.only_underwater && water_rule(rng) => 1,
        _ => return None,
    };
    // The animal in an exhibit that does not stand on a tile of the new kind is sent to one.
    let kinds = env.kinds[usize::from(mode)];
    let sent = env.exhibit && env.stands != mode && kinds > 0;
    Some((mode, sent.then(|| rng.pick(kinds as u32) as usize)))
}

/// The terrain mode a tile suits (tank fauna 5): 2 a tank tile, 1 a water tile that is not a
/// tank's, else 0.
pub(crate) fn kind(world: &World, t: usize) -> u8 {
    match () {
        _ if world.tank_at(t).is_some() => 2,
        _ if world.is_water(t) => 1,
        _ => 0,
    }
}

/// The tiles of region `x` that the picks of tank fauna 5 draw from: an exhibit's, or for the zoo
/// grounds (the exterior included, animals 11.5 points 2 and 3) the fill of the grounds from the
/// entrance tile (medium: the list of its last fill, exact while no exhibit was made), then any
/// grounds tile that fill does not reach, in map order.
pub(crate) fn tiles(world: &World, x: u32) -> Vec<usize> {
    if x != GROUNDS {
        return world.tiles_of(x);
    }
    let region = world.exhibits();
    let grounds = |t: usize| matches!(region.region(t), Some(GROUNDS | EXTERIOR));
    let (ex, ey) = world.zoo.entrance;
    let start = (i32::try_from(ex).ok().zip(i32::try_from(ey).ok()))
        .and_then(|(x, y)| world.tile_index(x, y));
    let mut list: Vec<usize> = start.map_or(Vec::new(), |s| crate::exhibit::fill(world, s));
    list.retain(|&t| grounds(t));
    let mut seen = vec![false; world.zoo.tiles.len()];
    list.iter().for_each(|&t| seen[t] = true);
    list.extend((0..seen.len()).filter(|&t| !seen[t] && grounds(t)));
    list
}

/// The terrain mode change (tank fauna 5) after the water check: the animal takes the terrain
/// mode (a species that lives only under water, always the tank mode); when it changes, the animal
/// walks to the tile `pick` of the tiles of its exhibit and its amphibious neighbours that have
/// the new kind.
pub(crate) fn switch(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    (mode, pick): (u8, Option<usize>),
) {
    let mode = if unit.mover.only_underwater { 2 } else { mode };
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    if std::mem::replace(&mut a.terrain, mode) == mode {
        return;
    }
    world.units.tell(format!("terrain {} {mode}", unit.id));
    send(unit, world, ctx, at, data, (mode, pick));
}

/// The animal walks to the tile `pick` of the tiles of its region and its amphibious neighbours
/// that have the kind `mode` (tank fauna 5 point 3).
pub(crate) fn send(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
    (mode, pick): (u8, Option<usize>),
) {
    let tile = (world.zoo.entities.get(at).and_then(|e| e.common.as_ref()))
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let x = tile.and_then(|t| world.exhibits().region(t));
    // The exterior is part of the grounds' list (animals 11.5 point 3).
    let x = x.map(|x| if x == EXTERIOR { GROUNDS } else { x });
    let Some((x, pick)) = x.zip(pick) else {
        return;
    };
    let neighbours = (world.exhibits().get(x)).map_or(Vec::new(), |e| e.amphibious.clone());
    let tiles = (tiles(world, x).into_iter())
        .chain(neighbours.into_iter().flat_map(|n| world.tiles_of(n)))
        .filter(|&t| kind(world, t) == mode);
    if let Some(to) = tiles.into_iter().nth(pick) {
        crate::goal::send(unit, world, ctx, at, data, to);
    }
}
