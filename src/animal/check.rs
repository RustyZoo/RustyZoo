//! The conditional checks of a status tick that read the animal and its environment only:
//! captivity, social, habitat, bored, shelter, other and zap.

use super::class::{Class, Keys};
use super::*;

/// The captivity tick (animals 4.5): the change, and below the threshold two draws that may
/// trample the ground under the animal.
pub(super) fn captivity(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &mut Env,
    rng: &mut Rng,
    out: &mut Out,
    read: Reader,
) {
    if !fire(&mut a.counters[CAPTIVITY], class.periods[CAPTIVITY]) {
        return;
    }
    a.change(k.captivity.saturating_neg());
    let low = class.ui_threshold.saturating_mul(2).saturating_sub(201);
    if k.dirt_chance > 0
        && a.happiness <= low
        && rng.chance(class.dirt_chance_chance)
        && rng.chance(k.dirt_chance)
    {
        out.trample = env.grass;
        if out.trample {
            read(Read::Trample, a, &out.meal, env, rng);
        }
    }
}

/// The social check (animals 5.1): the bits at every tick, the changes when the counter fires.
pub(super) fn social(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &mut Env,
    rng: &mut Rng,
    m: &Meal,
    read: Reader,
) {
    read(Read::Exhibit, a, m, env, rng);
    [FEW, MANY, CRAMPED, CROWDED]
        .into_iter()
        .for_each(|b| a.unmark(b));
    let adults = f64::from(env.census.adults);
    let size = if env.exhibit { env.size } else { 0 };
    let (mut few, mut many) = (false, false);
    if env.species < k.number_min {
        few = true;
        a.mark(FEW, k.number_min_change < -5);
    } else {
        // Quotient and product each rounded to 24 bits (animals 5.1).
        let f = match size {
            0 => 1.0,
            n => single(n as f64 / f64::from(k.habitat_size)).max(1.0),
        };
        if adults > single(f * f64::from(k.number_max)) {
            many = true;
            a.mark(MANY, k.number_max_change < -5);
        }
    }
    let density = match env.census.all_adults {
        0 => k.density,
        n => match (size as f32 / n as f32).round_ties_even() as i32 {
            0 => k.density,
            d => d,
        },
    };
    let cramped = density < k.density;
    if cramped {
        a.mark(CRAMPED, k.all_crowded_change < -5);
    }
    let crowded = !a.egg && !a.in_show && a.pressure[0] > k.crowd;
    if crowded {
        a.mark(CROWDED, k.crowd_change < -5);
    }
    // The counter runs down at every tick but restarts only outside a show (the original's flaw,
    // issue 317: an animal whose counter ends during a show never checks again).
    let old = a.counters[SOCIAL];
    a.counters[SOCIAL] = old.wrapping_sub(1);
    if old != 0 || a.egg || a.in_show {
        return;
    }
    a.counters[SOCIAL] = class.periods[SOCIAL].wrapping_sub(1);
    if few {
        a.change(k.number_min_change);
    } else if many {
        a.change(k.number_max_change);
    }
    if cramped {
        a.change(k.all_crowded_change);
    }
    if crowded {
        a.change(k.crowd_change);
        let [_, px, py] = a.pressure;
        a.crowd_direction = match (px.signum(), py.signum()) {
            (0, 1) => 4,
            (0, -1) => 0,
            (1, 0) => 6,
            (-1, 0) => 2,
            (1, 1) => 5,
            (1, -1) => 7,
            (-1, 1) => 3,
            (-1, -1) => 1,
            _ => a.crowd_direction,
        };
    }
}

/// The habitat check (animals 5.4): the bit at every tick, the change when the counter fires.
pub(super) fn habitat(a: &mut Animal, k: &Keys, class: &Class, env: &Env, messages: &mut Vec<i32>) {
    let other = env.other.clamp(-500, 150) / 10;
    a.unmark(INCOMPATIBLE);
    if other < 0 {
        a.mark(INCOMPATIBLE, other < -9);
    }
    if !fire(&mut a.counters[HABITAT], class.periods[HABITAT]) || a.egg || a.in_show {
        return;
    }
    let r = env.rating;
    if r >= k.habitat_preference {
        a.change(k.happy_habitat);
    } else if i64::from(r) * 100 < i64::from(k.pct_habitat) * i64::from(k.habitat_preference) {
        a.change(k.very_angry_habitat);
        if env.exhibit {
            messages.push(DISLIKES_EXHIBIT);
        }
    } else {
        a.change(k.angry_habitat);
    }
    a.change(other);
}

/// The bored check (animals 5.5): a fed, rested animal with company may get thirsty. The
/// predator and playmate searches and the chase are M7's and find nothing.
pub(super) fn bored(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
    out: &mut Out,
) {
    if a.egg || a.in_show || !fire(&mut a.counters[BORED], class.periods[BORED]) {
        return;
    }
    a.unmark(STIMULATION);
    // A tired animal tries to sleep.
    if a.energy >= k.energy_threshold {
        return life::sleep(a, k, class, env, out);
    }
    if a.at_minimum_health || a.occupied(env) || a.rampaging {
        return;
    }
    if a.hunger >= k.hunger_threshold {
        return;
    }
    let alone = env.census.species <= 1;
    let (drink, play) = (k.drink, if alone { 0 } else { k.chase });
    let total = drink.wrapping_add(play);
    if total > 0 && rng.rand() as i32 % total < drink {
        a.thirsty |= !env.wet && !a.escaped;
    }
}

/// The shelter check (animals 6): the draw that decides whether the animal looks for a shelter.
/// An animal that aims at a building that still serves the need keeps it; else the nearest
/// building that serves it, one drawn among the ties, becomes its target. The goals walk it there.
pub(super) fn shelter(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
    out: &mut Out,
) {
    if !(fire(&mut a.counters[SHELTER], class.periods[SHELTER]) && k.building_use > 0)
        || !rng.chance(k.building_use)
        || env.shelter.held
    {
        return;
    }
    let n = env.shelter.nearest.len() as u32;
    if n > 0 {
        (a.building, a.need) = (
            env.shelter.nearest[rng.pick(n) as usize],
            crate::guest::REST_NEED,
        );
        out.shelter = Some(a.building);
    }
}

/// The check of other animals (animals 5.2): bit 3 and the change while another animal of the
/// exhibit is very unhappy.
pub(super) fn other(a: &mut Animal, k: &Keys, class: &Class, env: &Env) {
    if !fire(&mut a.counters[OTHER], class.periods[OTHER]) {
        return;
    }
    a.unmark(ANGRY);
    if env.exhibit && env.census.unhappy > 0 && k.other_angry < 0 {
        a.change(k.other_angry);
        a.mark(ANGRY, k.other_angry < -9);
    }
}

/// The zap check (animals 11): nothing runs for an animal in a show; its counter fires when it
/// was below 1, and the draw of a fence test is made when nothing occupies the animal and its
/// chance is not 0; a success asks for the test itself ([`fence::test`]).
pub(super) fn zap(
    a: &mut Animal,
    k: &Keys,
    class: &Class,
    env: &Env,
    rng: &mut Rng,
    out: &mut Out,
) {
    if a.in_show {
        return;
    }
    let old = a.zap;
    a.zap = old.wrapping_sub(1);
    if old < 1 {
        a.zap = class.zap.wrapping_sub(1);
        let current = a.fence_chance.unwrap_or(k.test_fence);
        out.fence = !a.occupied(env) && current != 0 && rng.chance(k.test_fence);
    }
}
