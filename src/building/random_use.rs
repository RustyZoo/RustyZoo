//! The random use of a building type (guest buildings 10.1 point 2): `cRandomUse` 1 queues the
//! `used` animation when the AI clock passes a time kept as the entity base word (the one the
//! file saves), `cMinUsePeriod` plus a draw below `cMaxUsePeriod - cMinUsePeriod` ahead of it.
//! The use sound is not modelled yet.

use crate::entity::{Catalogue, EntityType};
use crate::place::bulldozer::type_of;
use crate::sim::Rng;
use crate::world::World;

/// The first file version that stores the next time.
const STORED_FROM: u32 = 35;

/// The periods of a type with `cRandomUse` 1 and the times `used` is queued (`cUseAnimCount`,
/// default 1; a negative count wraps to about four billion, as in the original).
struct Use {
    min: i32,
    max: i32,
    count: u32,
}

fn read(ty: &EntityType) -> Option<Use> {
    let n = |key: &str, default: i32| {
        crate::build::number(ty, key).map_or(default, |v| v as i32)
    };
    (ty.class == "building" && n("cRandomUse", 0) == 1).then(|| Use {
        min: n("cMinUsePeriod", 0),
        max: n("cMaxUsePeriod", 0),
        count: n("cUseAnimCount", 1) as u32,
    })
}

/// `now + min + rand() % (max - min)`, one draw when the difference is above 0, else none.
fn next(u: &Use, now: u32, rng: &mut Rng) -> u32 {
    let span = u.max.wrapping_sub(u.min);
    let r = if span > 0 { rng.pick(span as u32) } else { 0 };
    now.wrapping_add(u.min as u32).wrapping_add(r)
}

/// The time of the building at `at` is drawn from the AI clock `now` (it was made or its
/// statistics were reset). Nothing for another type.
pub(crate) fn draw(world: &mut World, cat: &Catalogue, at: usize, now: u32, rng: &mut Rng) {
    let e = &world.zoo.entities[at];
    let Some(u) = type_of(cat, e).and_then(read) else {
        return;
    };
    let time = next(&u, now, rng);
    if let Some(c) = world.zoo.entities[at].common.as_mut() {
        c.stamp = Some(time);
    }
}

/// A building of a loaded zoo is first made like a new one (one draw), then takes the stored time
/// (from version 35; a stored 0 is the AI clock `now`).
pub(crate) fn loaded(world: &mut World, cat: &Catalogue, at: usize, now: u32, rng: &mut Rng) {
    let e = &world.zoo.entities[at];
    if type_of(cat, e).and_then(read).is_none() {
        return;
    }
    let stored = e.common.as_ref().and_then(|c| c.stamp);
    draw(world, cat, at, now, rng);
    if world.zoo.version >= STORED_FROM
        && let Some(c) = world.zoo.entities[at].common.as_mut()
    {
        c.stamp = Some(stored.unwrap_or(now));
    }
}

/// The check of the building's update: when the AI clock `now` is past its time (strict, both
/// unsigned), the times `used` is to be queued, and a new time is drawn.
pub(crate) fn due(
    world: &mut World,
    cat: &Catalogue,
    at: usize,
    now: u32,
    rng: &mut Rng,
) -> Option<u32> {
    let e = &world.zoo.entities[at];
    let u = type_of(cat, e).and_then(read)?;
    let c = world.zoo.entities[at].common.as_mut()?;
    if now <= c.stamp.unwrap_or(now) {
        return None;
    }
    c.stamp = Some(next(&u, now, rng));
    Some(u.count)
}
