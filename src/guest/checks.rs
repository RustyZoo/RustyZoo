//! The status tick of a guest: the update of the values (guests 5.3) and the checks (6.1 to 6.9,
//! 6.12) with their countdowns.
//!
//! [`run`] is the checks on the guest's own state and the generator; [`checks`] is the hook that
//! reads the world for it: after the need checks it runs their seek parts, which look for a
//! building that satisfies the need ([`Cx::seek`]), then the trash check and the seek for a trash
//! can ([`Cx::seek_trash`]), the gift check and the gift's seek, and runs the viewing check
//! ([`super::view`]) and the surroundings check ([`super::mega`]) when their countdowns come up.

use super::*;
use crate::behaviour::Data;
use crate::goal::{Cx, LEAVE};
use crate::sim::Rng;
use crate::unit::{Ctx, Kind, Part, Unit};

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

/// The thoughts of leaving unhappy, by band of happiness (guests 6.3).
const LEAVING: i32 = 10087;

/// The needs of the seek parts, in the order of the checks: bathroom, hunger, thirst, energy and
/// the wish for a gift.
pub const SEEKS: [&str; 5] = ["bathroom", "food", "drink", "energy", "gift"];
/// The index of the energy check in [`SEEKS`].
pub const ENERGY_SEEK: usize = 3;

/// What the checks of a tick ask of the caller.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Out {
    /// The seek parts that are due, by [`SEEKS`].
    pub seek: [bool; SEEKS.len()],
    /// The viewing check is due.
    pub view: bool,
    /// The trash check's seek part is due: the guest holds litter and sets no other target.
    pub trash: bool,
    /// The surroundings check is due.
    pub surround: bool,
    /// The thoughts the checks gave, in order (the unhappy leave check).
    pub thoughts: Vec<i32>,
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
    let Some(k) = (unit.kind == Kind::Guest)
        .then(|| keys(world, ctx.cat, at))
        .flatten()
    else {
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
    let mut out = needs_part(g, k, wandered, skip, rng);
    trash(g, k, (wandered, skip), &mut out);
    rest(g, k, (wandered, skip), rng, &mut out);
    out
}

/// The checks up to the need checks (6.3 to 6.7), whose seek parts are due in the result.
fn needs_part(g: &mut Guest, k: &Keys, wandered: bool, skip: bool, rng: &mut Rng) -> Out {
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
            out.thoughts.push(LEAVING + b as i32);
        }
        // 6.4 to 6.7.
        for (i, (v, n, c)) in NEED_CHECKS.into_iter().enumerate() {
            let fired = !wandered && tick(&mut g.countdowns[c], k.periods[c]);
            let [increment, threshold, angry] = k.needs[n];
            let top = if v == ENERGY { k.max_energy } else { 100 };
            // The need being served does not grow (6.4 to 6.7; the meal at a bench serves hunger).
            let served = g.need == Some(SEEKS[i]) || (v == HUNGER && g.need == Some("eat"));
            if fired && !served {
                g.pending[v] = g.pending[v].saturating_add(increment);
                if g.values[v] >= top {
                    g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(angry);
                }
            }
            let held = match v {
                HUNGER => g.items[0].is_some(),
                THIRST => g.items[1].is_some(),
                _ => false,
            };
            out.seek[i] = (fired || wandered)
                && !held
                && (v == ENERGY || !g.stand)
                && g.values[v] >= threshold;
        }
    }
    out
}

/// The trash check (6.8): every guest pays for it, unless it is on its way to a trash can; one
/// that holds litter and has no target is to look for a can.
fn trash(g: &mut Guest, k: &Keys, (wandered, skip): (bool, bool), out: &mut Out) {
    if skip {
        return;
    }
    let fired = !wandered && tick(&mut g.countdowns[TRASH], k.periods[TRASH]);
    if fired && g.need != Some("trash") {
        g.pending[HAPPINESS] = g.pending[HAPPINESS].saturating_add(k.trash);
    }
    out.trash = (fired || wandered) && g.items[4].is_some() && g.building == 0 && !g.stand;
}

/// The gift check (6.9), the viewing and surroundings countdowns, and the unused one.
fn rest(g: &mut Guest, k: &Keys, (wandered, skip): (bool, bool), rng: &mut Rng, out: &mut Out) {
    if !skip {
        // 6.9: the draw of the wish for a gift.
        if wandered || tick(&mut g.countdowns[GIFT], k.periods[GIFT]) {
            g.gift = match g.values[HAPPINESS] {
                71..=100 => rng.chance(k.souvenir[1]),
                31..=70 => rng.chance(k.souvenir[0]),
                _ => false,
            };
            out.seek[4] = g.gift && g.items[3].is_none() && !g.stand;
        }
    }
    // Step 4 of 6.1 clears the wandered flag; then the viewing and surroundings checks.
    out.view = tick(&mut g.countdowns[VIEWING], k.periods[VIEWING]);
    out.surround = tick(&mut g.countdowns[SURROUNDINGS], k.periods[SURROUNDINGS]);
    tick(&mut g.countdowns[LIKE], k.periods[LIKE]);
}

/// Runs `seek` on the context with the guest `g` standing in its unit, and takes it back.
fn with(cx: &mut Cx, g: Guest, seek: impl FnOnce(&mut Cx)) -> Guest {
    cx.unit.part = Part::Guest(g);
    seek(cx);
    match &mut cx.unit.part {
        Part::Guest(g) => std::mem::take(g),
        _ => Guest::default(),
    }
}

/// The seek parts of `range` of [`SEEKS`] that are due, for the guest `g` that stands in `unit`.
fn seeks(cx: &mut Cx, g: Guest, range: std::ops::Range<usize>, out: &Out) -> Guest {
    with(cx, g, |cx| {
        (range.filter(|&i| out.seek[i])).for_each(|i| cx.seek(SEEKS[i]));
    })
}

/// The checks of a status tick (guests 6.1).
pub fn checks(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let Some(k) = (unit.kind == Kind::Guest)
        .then(|| keys(world, ctx.cat, at))
        .flatten()
    else {
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
    let wandered = unit.goal.wandered;
    let mut out = needs_part(&mut g, &k, wandered, skip, ctx.rng);
    // The seek parts look at the guest as it stands, in the order of the checks.
    let mut cx = Cx {
        unit: &mut *unit,
        world: &mut *world,
        ctx: &mut *ctx,
        at,
        data,
    };
    for text in std::mem::take(&mut out.thoughts) {
        cx.think(text, None);
    }
    g = seeks(&mut cx, g, 0..ENERGY_SEEK + 1, &out);
    trash(&mut g, &k, (wandered, skip), &mut out);
    g = with(&mut cx, g, |cx| {
        if out.trash {
            cx.seek_trash();
        }
    });
    rest(&mut g, &k, (wandered, skip), cx.ctx.rng, &mut out);
    g = seeks(&mut cx, g, ENERGY_SEEK + 1..SEEKS.len(), &out);
    cx.unit.goal.wandered = false;
    if out.view {
        view::check(&mut g, &k, cx.world, cx.ctx.cat, cx.ctx.rng, tile);
    }
    // 6.11: the megatile under the guest.
    let width = cx.world.zoo.width as usize;
    let cell = tile.and_then(|t| cx.world.units.mega.at(width, t)).copied();
    if let Some(cell) = cell.filter(|_| out.surround) {
        for text in mega::check(&mut g, &k, &cell) {
            cx.think(text, None);
        }
    }
    cx.unit.part = Part::Guest(g);
}

#[cfg(test)]
mod tests;
