//! Sleep, health, sickness, old age and death (animals-needs 6, 9 and 10).
//!
//! The checks are plain functions of the animal, an [`Env`] and the generator, like the others of
//! the module: what they ask of the world (a set to start, the animal removed, its exhibit told of
//! a death) goes into the [`Out`] of the tick.

use super::{
    Animal, CAUGHT_ILL, Class, Env, ILL, Keys, Out, SICK, SICK_NEAR, TAKEN_AWAY, VERY_SICK, fire,
};
use crate::behaviour::{Data, Env as Run};
use crate::sim::Rng;
use crate::unit::{Ctx, Kind, Part, Unit};
use crate::world::World;

/// The task of a keeper at work in an exhibit (0x6e).
const DUTIES: u8 = 110;

/// What the checks read of the animal's sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sets {
    /// The type has the set.
    pub has_sleep: bool,
    pub has_sick: bool,
    pub has_die: bool,
    /// `bSleep` is the current or the queued set; `bSick` the current one; `bDie` the current one
    /// and it runs.
    pub sleep_now: bool,
    pub sick_now: bool,
    pub dying_now: bool,
}

impl Animal {
    /// Falling ill sets the health to 0, which is what sick means (9).
    fn fall_ill(&mut self) {
        (self.health, self.at_minimum_health) = (0, true);
    }
}

/// The sleep check (6): tiredness rises, a tired animal starts `bSleep`, a rested one wakes. It
/// counts its counter down at every call and acts when it fires, or at every call while asleep.
pub fn sleep(a: &mut Animal, k: &Keys, class: &Class, env: &Env, out: &mut Out) {
    let fired = fire(&mut a.counters[super::ENERGY], class.periods[super::ENERGY]);
    if !(fired || a.sleeping) {
        return;
    }
    let sets = &env.sets;
    let mut asleep = sets.sleep_now || out.sets.contains(&"bSleep");
    // An animal in the water or a tank cannot start to sleep: it only tires.
    if !a.sleeping {
        let start = !env.wet && a.energy >= k.energy_threshold && !asleep && sets.has_sleep;
        if start {
            (a.sleeping, asleep) = (true, true);
            out.sets.push("bSleep");
            out.log.push("sleep");
        } else {
            a.energy_change = a.energy_change.saturating_add(k.energy_increment);
        }
    }
    if a.sleeping && !asleep {
        (a.energy, a.sleeping, a.slept) = (0, false, env.now);
        out.log.push("wake");
    }
}

/// The health check (9): the starving count, the unwell animal, the sick company and the three
/// ways to fall ill.
pub fn health(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng, out: &mut Out) {
    if !fire(&mut a.counters[super::HEALTH], class.periods[super::HEALTH]) {
        return;
    }
    a.unmark(SICK);
    a.unmark(SICK_NEAR);
    if a.health == 0 {
        a.fall_ill();
        a.starving = if a.hunger >= 100 { a.starving + 1 } else { 0 };
    } else {
        a.starving = 0;
    }
    let mut removed = false;
    if a.starving < k.sick_time {
        if a.at_minimum_health {
            out.messages.push(VERY_SICK);
        }
    } else if !env.keeper_working {
        removed = true;
    }
    // Low health: the set, and the unhappiness (without a set to start: not at all).
    let low = i64::from(a.health) * 100 <= i64::from(k.pct_hits) * i64::from(k.max_hits);
    let mut unwell = low;
    if low && !env.wet && !a.egg {
        match (env.sets.sick_now, env.sets.has_sick) {
            (false, true) => out.sets.push("bSick"),
            (false, false) => unwell = false,
            _ => {}
        }
    }
    if unwell {
        a.change(k.sick_change);
        a.mark(SICK, false);
    }
    if !a.egg {
        if env.sick_near > 0 {
            a.change(k.other_sick_change);
            a.mark(SICK_NEAR, false);
            if !a.at_minimum_health && rng.chance(k.sick_chance) && rng.chance(4) {
                a.fall_ill();
                out.messages.push(CAUGHT_ILL);
                out.log.push("infected");
            }
        } else if !a.at_minimum_health && rng.chance(k.sick_random) && rng.pick(100) == 0 {
            a.fall_ill();
            out.messages.push(ILL);
            out.log.push("ill");
        }
    }
    if removed {
        out.messages.push(TAKEN_AWAY);
        out.log.push("taken");
        (out.remove, out.mourn) = (true, false);
    }
}

/// The old age part of the reproduction check (10): the life counter runs down by the check's
/// period, and once it is spent each check rolls the species' chance of dying. The animal then
/// starts `bDie` (its `fDie` removes it) or, with no such set, is removed at once.
pub fn aging(a: &mut Animal, k: &Keys, class: &Class, env: &Env, rng: &mut Rng, out: &mut Out) {
    let period = class.periods[super::REPRODUCTION];
    if !fire(&mut a.counters[super::REPRODUCTION], period) {
        return;
    }
    a.life = a.life.saturating_sub(period);
    if a.life < 1 && !a.dying && rng.chance(k.death_chance) {
        a.dying = true;
        out.log.push("dying");
        if env.sets.has_die && !env.wet {
            out.sets.push("bDie");
        } else {
            (out.remove, out.mourn) = (true, true);
        }
    }
}

/// What the checks read of the sets of the animal at position `at`. A set that has ended still
/// is the top one until another starts, so the current set is the top one that runs.
pub fn sets(unit: &Unit, world: &World, data: &dyn Data, at: usize) -> Sets {
    let (entity, runner) = (&world.zoo.entities[at], &unit.runner);
    let has = |name| data.set(entity, name).is_some();
    let now = |name| runner.top() == Some(name) && runner.running();
    Sets {
        has_sleep: has("bSleep"),
        has_sick: has("bSick"),
        has_die: has("bDie"),
        sleep_now: now("bSleep") || runner.queued() == Some("bSleep"),
        sick_now: now("bSick"),
        dying_now: now("bDie"),
    }
}

/// Whether a keeper is at work (task 0x6e) on a tile of exhibit `x` (habitat rating 5.1).
pub fn keeper_at_work(world: &World, x: u32) -> bool {
    let at_work = world.units.iter().filter(|u| u.kind == Kind::Keeper);
    at_work.filter(|u| u.goal.record.task == DUTIES).any(|u| {
        let c = world
            .find(u.id)
            .and_then(|e| world.zoo.entities[e].common.as_ref());
        let tile = c.and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
        tile.and_then(|t| world.exhibits().region(t)) == Some(x)
    })
}

/// The ambient choice of an idle, tired animal runs the sleep check instead (behaviours 6.4 point
/// 1): called where a set would be chosen, when none runs and none is queued.
pub fn ambient_sleep(
    unit: &mut Unit,
    world: &mut World,
    ctx: &mut Ctx,
    at: usize,
    data: &dyn Data,
) {
    let idle = !unit.runner.running() && unit.runner.queued().is_none();
    let Part::Animal(a) = &unit.part else {
        return;
    };
    if !idle || a.egg || a.dying || a.crated {
        return;
    }
    let k = super::keys(world, ctx.cat, at);
    let sets = sets(unit, world, data, at);
    let tile = (world.zoo.entities[at].common.as_ref())
        .and_then(|c| world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)));
    let wet = tile.is_some_and(|t| world.is_water(t) || world.tank_at(t).is_some());
    let Part::Animal(a) = &mut unit.part else {
        return;
    };
    if a.energy < k.energy_threshold {
        return;
    }
    let class = world.units.class.clone();
    let (mut out, now) = (Out::default(), world.units.now);
    let env = Env {
        wet,
        now,
        sets,
        ..Env::default()
    };
    sleep(a, &k, &class, &env, &mut out);
    let id = unit.id;
    for set in &out.sets {
        let env = Run {
            data,
            unit: &world.zoo.entities[at],
            rng: &mut *ctx.rng,
        };
        unit.runner
            .start(&mut unit.animation, &env, set, true, false, true);
    }
    out.log
        .iter()
        .for_each(|w| world.units.tell(format!("{w} {id}")));
}
