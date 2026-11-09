//! The random use of a building type (guest buildings 10.1 point 2): `cRandomUse` 1 queues the
//! `used` animation when the AI clock passes a time kept as the entity base word (the one the
//! file saves), `cMinUsePeriod` plus a draw below `cMaxUsePeriod - cMinUsePeriod` ahead of it.
//! A random use also starts the type's use sound at the building (guest buildings 10.2).

use crate::behaviour::Heard;
use crate::config::to_int;
use crate::entity::type_of;
use crate::entity::{Catalogue, EntityType};
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
    let n = |key: &str, default: i32| crate::build::number(ty, key).map_or(default, |v| v as i32);
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

/// The time of a building of type `ty` made at the AI clock `now`: none, and no draw, for a type
/// that is not of random use.
pub(crate) fn time(ty: &EntityType, now: u32, rng: &mut Rng) -> Option<u32> {
    read(ty).map(|u| next(&u, now, rng))
}

/// The time of the building at `at` is drawn from the AI clock `now` (it was made or its
/// statistics were reset). Nothing for another type.
pub(crate) fn draw(world: &mut World, cat: &Catalogue, at: usize, now: u32, rng: &mut Rng) {
    let e = &world.zoo.entities[at];
    let Some(time) = type_of(cat, e).and_then(|t| time(t, now, rng)) else {
        return;
    };
    if let Some(c) = world.zoo.entities[at].common.as_mut() {
        c.stamp = Some(time);
    }
}

/// A building of a loaded zoo is first made like a new one (one draw), then takes the stored time
/// (from version 35; a stored 0 is the AI clock `now` held before the file's own was read, which
/// the creation draws from too).
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

/// The use sound of a building type, `[UseSound]` `name` with its `attenuation` as the base term;
/// none for a type without a name. Its voice is the building's, so one that still plays goes on.
pub(crate) fn sound(ty: &EntityType, id: u32) -> Option<Heard> {
    let section = ty.config.as_deref()?.section("UseSound")?;
    let name = section.get("name").filter(|v| !v.is_empty())?;
    Some(Heard {
        file: ty.sound_member(name),
        base: section.get("attenuation").and_then(to_int).unwrap_or(0),
        voice: format!("building/{id}/UseSound"),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use crate::sim::idle::tests::Fixed;
    use crate::unit::testing::{centre, entity, world_with};

    fn cat(keys: &str) -> Catalogue {
        let mut cat = Catalogue::default();
        let text = format!("[Characteristics/Integers]\n{keys}\n");
        cat.push(fixtures::ty("building", "geyser", "", &text));
        cat
    }

    fn world(stamp: Option<u32>, version: u32) -> World {
        let mut w = world_with((4, 4), |zoo| {
            let mut e = entity("building", "geyser", 1, centre((0, 0)), 0);
            e.common.as_mut().unwrap().stamp = stamp;
            zoo.entities.push(e);
            zoo.version = version;
        });
        w.zoo.version = version;
        w
    }

    fn stamp(w: &World) -> Option<u32> {
        w.zoo.entities[0].common.as_ref().unwrap().stamp
    }

    fn rng() -> Rng {
        let mut r = Rng::default();
        r.seed(7);
        r
    }

    /// The time is `now + min + rand() % (max - min)` with one draw when the difference is above
    /// 0; else the minimum alone and no draw. Another flag than 1, or a missing one, does nothing.
    #[test]
    fn the_time_is_drawn_from_the_periods_and_only_for_a_flag_of_1() {
        let mut first = rng();
        let r = first.pick(40);
        for (keys, want, draws) in [
            (
                "cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=60",
                Some(100 + 20 + r),
                1,
            ),
            (
                "cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=20",
                Some(120),
                0,
            ),
            (
                "cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=5",
                Some(120),
                0,
            ),
            ("cRandomUse=1", Some(100), 0),
            (
                "cRandomUse=1\ncMinUsePeriod=-30",
                Some(70 + rng().pick(30)),
                1,
            ),
            (
                "cRandomUse=1\ncMaxUsePeriod=99999999999",
                Some(100 + rng().pick(i32::MAX as u32)),
                1,
            ),
            ("cRandomUse=0\ncMaxUsePeriod=60", None, 0),
            ("cRandomUse=2\ncMaxUsePeriod=60", None, 0),
            ("cRandomUse=-1\ncMaxUsePeriod=60", None, 0),
            ("cMaxUsePeriod=60", None, 0),
        ] {
            let (mut w, mut g) = (world(None, 70), rng());
            draw(&mut w, &cat(keys), 0, 100, &mut g);
            assert_eq!(stamp(&w), want, "{keys}");
            let mut spent = rng();
            (0..draws).for_each(|_| _ = spent.rand());
            assert_eq!(g.state(), spent.state(), "{keys}");
        }
    }

    /// A building fires when the AI clock is strictly past its time, queues `used`
    /// `cUseAnimCount` times (default 1), and draws a new time from the same clock.
    #[test]
    fn it_fires_past_its_time_and_draws_again() {
        let keys = "cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=60";
        let c = cat(keys);
        let mut w = world(Some(50), 70);
        let mut g = rng();
        assert_eq!(due(&mut w, &c, 0, 49, &mut g), None);
        assert_eq!(due(&mut w, &c, 0, 50, &mut g), None, "equal does not fire");
        assert_eq!(stamp(&w), Some(50));
        assert_eq!(g.state(), rng().state(), "nothing drawn");
        assert_eq!(due(&mut w, &c, 0, 51, &mut g), Some(1));
        assert_eq!(stamp(&w), Some(51 + 20 + rng().pick(40)));
        // Counts: 3, 0, a negative one wraps to about four billion.
        for (count, want) in [("3", 3), ("0", 0), ("-1", u32::MAX)] {
            let c = cat(&format!("{keys}\ncUseAnimCount={count}"));
            let mut w = world(Some(0), 70);
            assert_eq!(due(&mut w, &c, 0, 1, &mut rng()), Some(want), "{count}");
        }
        // Other types never fire.
        let none = cat("cRandomUse=0");
        assert_eq!(due(&mut world(Some(0), 70), &none, 0, 1, &mut g), None);
    }

    /// A loaded building draws once like a new one, then keeps the stored time (a stored 0 is the
    /// clock), from version 35; before, the drawn time stays.
    #[test]
    fn a_loaded_building_draws_once_then_takes_the_stored_time() {
        let c = cat("cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=60");
        let drawn = 100 + 20 + rng().pick(40);
        for (stored, version, want) in [
            (Some(5), 35, 5),
            (None, 35, 100),
            (Some(5), 34, drawn),
            (None, 34, drawn),
        ] {
            let (mut w, mut g) = (world(stored, version), rng());
            loaded(&mut w, &c, 0, 100, &mut g);
            assert_eq!(stamp(&w), Some(want), "{stored:?} {version}");
            let mut spent = rng();
            spent.rand();
            assert_eq!(g.state(), spent.state());
        }
        // A type without random use draws nothing and keeps the word.
        let (mut w, mut g) = (world(None, 70), rng());
        loaded(&mut w, &cat(""), 0, 100, &mut g);
        assert_eq!((stamp(&w), g.state()), (None, rng().state()));
    }

    /// In the step loop the building plays `used` once when its time is up, then `idle` returns.
    #[test]
    fn the_step_plays_used_once_when_the_time_is_up() {
        use crate::behaviour::Timing;
        use crate::sim::Sim;
        use crate::zoo::Clocks;
        let c = cat("cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=60");
        let mut w = world(Some(0), 70);
        let data = Fixed(
            Some(Timing {
                count: 2,
                delay: 100,
            }),
            vec![(
                "used",
                Timing {
                    count: 2,
                    delay: 100,
                },
            )],
        );
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(16, Clocks::default());
        let mut seen = Vec::new();
        for _ in 0..40 {
            sim.step_with(&data, &c, &mut w, 100, |_, _, _| {});
            seen.push(sim.idle_looks().get(&1).copied());
        }
        // The clock passes 0 after 1000 ms; the time drawn is at least 20 ticks later.
        assert!(seen[..9].iter().all(Option::is_none));
        let plays = seen.iter().filter(|s| **s == Some("used")).count();
        assert!(plays >= 1, "{seen:?}");
        assert!(seen.last().unwrap().is_none(), "{seen:?}");
        assert!(stamp(&w).unwrap() >= 20, "a new time was drawn");
    }

    /// The sim gives a placed building its pending object's time (or draws one when it has none),
    /// draws at a reset start, and at load keeps the stored time.
    #[test]
    fn the_sim_keeps_at_placing_draws_at_a_reset_and_keeps_the_stored_time_at_load() {
        use crate::sim::Sim;
        use crate::zoo::Clocks;
        let c = cat("cRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=60");
        let at = |ticks| {
            let mut sim = Sim::new(b"");
            sim.set_seed(1);
            sim.start_game(
                16,
                Clocks {
                    ai_ticks: ticks,
                    ..Clocks::default()
                },
            );
            sim
        };
        let drawn = |sim: &Sim, ticks: u32| ticks + 20 + sim.rng.clone().pick(40);
        let mut sim = at(500);
        let (mut w, want) = (world(None, 70), drawn(&sim, 500));
        sim.keep_use(&mut w, &c, 1, None);
        assert_eq!(stamp(&w), Some(want));
        let state = sim.rng.clone();
        sim.keep_use(&mut w, &c, 1, Some(7));
        assert_eq!((stamp(&w), &sim.rng), (Some(7), &state), "no draw");
        sim.keep_use(&mut w, &c, 99, None);
        let (mut w, want) = (world(Some(3), 70), drawn(&at(900), 900));
        at(900).draw_use_all(&mut w, &c);
        assert_eq!(stamp(&w), Some(want));
        let (mut w, mut sim) = (world(Some(3), 70), at(900));
        sim.load(&mut w, &c);
        assert_eq!(stamp(&w), Some(3));
        assert_ne!(sim.rng, at(900).rng, "one draw");
    }
}
