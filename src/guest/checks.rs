//! The status tick of a guest: the update of the values (guests 5.3) and the checks (6.1 to 6.9,
//! 6.12) with their countdowns.
//!
//! [`run`] is the checks on the guest's own state and the generator; [`checks`] is the hook that
//! reads the world for it, resets the goal when a tired guest finds no bench, and runs the
//! viewing check ([`super::view`]) when its countdown comes up. The seek parts of the need checks
//! look for a building that satisfies the need: until the buildings exist they find none, which
//! costs no draw, so only the energy check, which resets the goal then, does anything.

use super::*;
use crate::behaviour::Data;
use crate::goal::{Cx, LEAVE};
use crate::sim::Rng;
use crate::unit::{Ctx, Part, Unit};

/// The countdowns that are not needs, in the order of [`PERIODS`].
const LEAVE_CHECK: usize = 3;
const GIFT: usize = 4;
const CHASE: usize = 6;
const TRASH: usize = 7;
const LIKE: usize = 8;
const VIEWING: usize = 9;
const SURROUNDINGS: usize = 10;
/// The need checks in their order: bathroom, hunger, thirst, energy. Each is the value it checks,
/// its row of [`Keys::needs`] and its countdown.
const NEED_CHECKS: [(usize, usize, usize); 4] = [
    (BATHROOM, 3, 2),
    (HUNGER, 0, 0),
    (THIRST, 1, 1),
    (ENERGY, 2, 5),
];

/// What the checks of a tick ask of the caller.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Out {
    /// The tired guest found no bench: its goal is reset.
    pub reset: bool,
    /// The viewing check is due.
    pub view: bool,
}

/// The countdown rule (6.1): it goes down by one; the check fires when it was 0 and restarts at
/// `period - 1`.
fn tick(c: &mut i32, period: i32) -> bool {
    let fired = *c == 0;
    *c = if fired {
        period.wrapping_sub(1)
    } else {
        c.wrapping_sub(1)
    };
    fired
}

/// 5.3: the pending changes are added, the values kept between their limits, the changes cleared.
pub fn update(g: &mut Guest, max_energy: i32) {
    let limits = [(-100, 100), (0, 100), (0, 100), (0, max_energy), (0, 100)];
    for ((v, p), (low, high)) in g.values.iter_mut().zip(&mut g.pending).zip(limits) {
        *v = v.saturating_add(*p).min(high).max(low);
        *p = 0;
    }
}

/// The status values at a status tick.
pub fn needs(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize) {
    let Some(k) = keys(world, ctx.cat, at) else {
        return;
    };
    let Part::Guest(g) = &mut unit.part else {
        return;
    };
    update(g, k.max_energy);
    let v = g.values;
    let line = format!(
        "status {} {} {} {} {} {}",
        unit.id, v[0], v[1], v[2], v[4], v[3]
    );
    world.units.guest_note(line);
}

/// The checks in the order of 6.1 after the countdowns of reset (not kept: `[AI]
/// GuestResetInterval` is 0 in the shipped game) and saw-animal. `wandered` is the ladder's flag;
/// `skip` is set while the guest is on water, in a tank or leaving, when checks 6.3 to 6.9 do
/// nothing.
pub fn run(g: &mut Guest, k: &Keys, wandered: bool, skip: bool, rng: &mut Rng) -> Out {
    let mut out = Out::default();
    g.saw_animal = match g.saw_animal {
        0 => k.saw_animal_reset.wrapping_sub(1),
        n => n.wrapping_sub(1),
    };
    tick(&mut g.countdowns[CHASE], k.periods[CHASE]);
    if !skip {
        // 6.3: while the wandered flag is up the countdown is not asked and the draws are made.
        let due = match wandered {
            true => {
                g.countdowns[LEAVE_CHECK] = k.periods[LEAVE_CHECK].wrapping_sub(1);
                true
            }
            false => tick(&mut g.countdowns[LEAVE_CHECK], k.periods[LEAVE_CHECK]),
        };
        let band = match g.values[HAPPINESS] {
            -40..=-20 => Some(0),
            -80..=-41 => Some(1),
            -100..=-81 => Some(2),
            _ => None,
        };
        if let Some(b) = band.filter(|_| due)
            && rng.chance(k.leave[b])
        {
            g.leave = true;
        }
        // 6.4 to 6.7.
        for (v, n, c) in NEED_CHECKS {
            let fired = !wandered && tick(&mut g.countdowns[c], k.periods[c]);
            let [increment, threshold, angry] = k.needs[n];
            let top = if v == ENERGY { k.max_energy } else { 100 };
            if fired {
                g.pending[v] = g.pending[v].saturating_add(increment);
                if g.values[v] >= top {
                    g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(angry);
                }
            }
            out.reset |= (fired || wandered) && v == ENERGY && g.values[v] >= threshold;
        }
        // 6.8.
        if !wandered && tick(&mut g.countdowns[TRASH], k.periods[TRASH]) {
            g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(k.trash);
        }
        // 6.9: the draw of the wish for a gift, which no gift shop answers yet.
        if wandered || tick(&mut g.countdowns[GIFT], k.periods[GIFT]) {
            match g.values[HAPPINESS] {
                71..=100 => _ = rng.chance(k.souvenir[1]),
                31..=70 => _ = rng.chance(k.souvenir[0]),
                _ => {}
            }
        }
    }
    // Step 4 of 6.1 clears the wandered flag; then the viewing and surroundings checks.
    out.view = tick(&mut g.countdowns[VIEWING], k.periods[VIEWING]);
    tick(&mut g.countdowns[SURROUNDINGS], k.periods[SURROUNDINGS]);
    tick(&mut g.countdowns[LIKE], k.periods[LIKE]);
    out
}

/// The checks of a status tick (guests 6.1).
pub fn checks(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let Some(k) = keys(world, ctx.cat, at) else {
        return;
    };
    let Part::Guest(g) = &mut unit.part else {
        return;
    };
    let mut g = std::mem::take(g);
    let tile = view::tile_of(world, at);
    let wet = tile.is_some_and(|t| {
        let x = world
            .exhibits()
            .region(t)
            .and_then(|r| world.exhibits().get(r));
        world.is_water(t) || x.is_some_and(|x| x.tank.is_some())
    });
    let skip = wet || unit.goal.record.task == LEAVE;
    let out = run(&mut g, &k, unit.goal.wandered, skip, ctx.rng);
    unit.goal.wandered = false;
    if out.reset {
        let mut cx = Cx {
            unit: &mut *unit,
            world: &mut *world,
            ctx: &mut *ctx,
            at,
            data,
        };
        cx.reset();
    }
    if out.view {
        view::check(&mut g, &k, world, ctx.cat, ctx.rng, tile);
    }
    unit.part = Part::Guest(g);
}

#[cfg(test)]
mod tests;
