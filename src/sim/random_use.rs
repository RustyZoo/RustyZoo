//! The random use of the buildings, wired to the simulation (guest buildings 10.1): the time
//! drawn when a building is made or placed, and the `used` animation queued when it is up.

use super::Sim;
use crate::behaviour::Data;
use crate::entity::Catalogue;
use crate::place::bulldozer::type_of;
use crate::world::World;

impl Sim {
    /// A building of random use plays `used` when its time is up (guest buildings 10.1).
    pub(super) fn random_use(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        data: &dyn Data,
        at: usize,
    ) {
        let now = self.ai_ticks;
        if let Some(times) = crate::building::random_use::due(world, cat, at, now, &mut self.rng) {
            // Each queued sequence carries the use sound and starts it when it begins.
            let e = &world.zoo.entities[at];
            let id = e.common.as_ref().map_or(0, |c| c.id);
            let heard = type_of(cat, e).and_then(|t| crate::building::random_use::sound(t, id));
            self.idles.queue_used(world, at, times, heard, data);
        }
    }

    /// The random use time of a pending building of type `ty`, drawn when it is made (guest
    /// buildings 10.1 point 2.5): none for a type that is not of random use.
    pub(crate) fn use_time(&mut self, ty: &crate::entity::EntityType) -> Option<u32> {
        crate::building::random_use::time(ty, self.ai_ticks, &mut self.rng)
    }

    /// The building `id` just placed keeps the time `kept` of its pending object; without one
    /// (a tool not chosen in the list) it draws now (guest buildings 10.1 point 2.5).
    pub(crate) fn keep_use(
        &mut self,
        world: &mut World,
        cat: &Catalogue,
        id: u32,
        kept: Option<u32>,
    ) {
        let Some(at) = world.find(id) else { return };
        match (kept, world.zoo.entities[at].common.as_mut()) {
            (Some(time), Some(c)) => c.stamp = Some(time),
            _ => crate::building::random_use::draw(world, cat, at, self.ai_ticks, &mut self.rng),
        }
    }

    /// A start with `reset` draws the random use time of every building again, in list order,
    /// from the restored clock (guest buildings 10.1 point 2.5).
    pub(crate) fn draw_use_all(&mut self, world: &mut World, cat: &Catalogue) {
        for at in 0..world.zoo.entities.len() {
            crate::building::random_use::draw(world, cat, at, self.ai_ticks, &mut self.rng);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::idle::Idles;
    use super::super::idle::tests::{Building, Fixed};
    use super::Sim;
    use crate::behaviour::{Data, Timing};
    use crate::entity::Catalogue;
    use crate::sim::rng::Rng;
    use crate::unit::testing::{centre, entity, world_with};
    use crate::world::World;

    /// `used` queued behind a building's loop plays `times` times, one pass each, then the base
    /// comes back: `idle` for a building never entered, `used` after an entry; a type with no
    /// `used` plays nothing.
    #[test]
    fn a_queued_use_plays_its_passes_and_the_base_returns() {
        let world = |users: u32| {
            let mut w = world_with((4, 4), |zoo| {
                zoo.entities
                    .push(entity("building", "hut", 1, centre((0, 0)), 0));
            });
            w.units.buildings.insert(
                1,
                crate::building::Live {
                    extra: users,
                    ..Default::default()
                },
            );
            w
        };
        let run = |users: u32, times: u32, data: &dyn Data| {
            let (mut rng, mut idles) = (Rng::default(), Idles::default());
            idles.register(&world(0), &Catalogue::default(), data, &mut rng);
            let w = world(users);
            idles.queue_used(&w, 0, times, None, data);
            let mut seen = Vec::new();
            for _ in 0..12 {
                // The pass of `idle` (4 frames) runs to its end first.
                idles.register(&w, &Catalogue::default(), data, &mut rng);
                idles.advance(100);
                seen.push(idles.looks().get(&1).copied());
            }
            seen
        };
        let used = Some("used");
        // The pass of idle ends after 400 ms, then 2 passes of 200 ms, then idle again.
        assert_eq!(
            run(0, 2, &Building),
            [
                None, None, None, used, used, used, used, None, None, None, None, None
            ]
        );
        assert_eq!(run(0, 0, &Building), [None; 12], "a count of 0");
        // After an entry the base is `used`: it loops, with the queue ahead.
        assert!(run(1, 1, &Building)[11] == used);
        // No `used` animation: nothing.
        let none = Fixed(
            Some(Timing {
                count: 4,
                delay: 100,
            }),
            vec![],
        );
        assert_eq!(run(0, 3, &none), [None; 12]);
    }

    fn hut(dead: bool) -> World {
        let mut w = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("building", "hut", 1, centre((0, 0)), 0));
        });
        let live = crate::building::Live {
            dead,
            ..Default::default()
        };
        w.units.buildings.insert(1, live);
        w
    }

    /// A `used` queued by the check of a step joins the queue after the clocks advance: when the
    /// pass in progress ends in that very step, the base returns and `used` waits for the next
    /// pass (the original advances the animation before the check).
    #[test]
    fn a_use_queued_by_the_check_waits_for_the_next_step() {
        let (w, mut rng, mut idles) = (hut(false), Rng::default(), Idles::default());
        idles.register(&w, &Catalogue::default(), &Building, &mut rng);
        // Three steps into the 4 frames of `idle`; the fourth ends the pass.
        (0..3).for_each(|_| idles.advance(100));
        idles.queue_used(&w, 0, 1, None, &Building);
        idles.advance(100);
        assert_eq!(
            idles.looks().get(&1),
            None,
            "no `used` in the step of the check"
        );
        // The next pass of `idle` runs 400 ms; then `used` plays its 200 ms.
        (0..3).for_each(|_| idles.advance(100));
        assert_eq!(idles.looks().get(&1), None);
        idles.advance(100);
        assert_eq!(idles.looks().get(&1), Some(&"used"));
    }

    /// The dead switch clears the player, a queued `used` included, both ways: `idledead` starts
    /// at once and the `used` never plays; alive again, `idle` at once.
    #[test]
    fn the_dead_switch_drops_a_queued_use() {
        for dead in [true, false] {
            let (mut rng, mut idles) = (Rng::default(), Idles::default());
            let (before, after) = (hut(!dead), hut(dead));
            idles.register(&before, &Catalogue::default(), &Building, &mut rng);
            idles.queue_used(&before, 0, 2, None, &Building);
            idles.advance(10);
            idles.register(&after, &Catalogue::default(), &Building, &mut rng);
            let want = dead.then_some(&"idledead");
            assert_eq!(idles.looks().get(&1), want, "dead {dead}");
            // Over many passes the queued `used` never shows.
            for _ in 0..30 {
                idles.advance(100);
                assert_ne!(idles.looks().get(&1), Some(&"used"));
            }
        }
    }

    /// A random use that queues `used` also carries the type's `[UseSound]` to the building, with
    /// its attenuation as the base; no sound without a name, without `used` or before the time,
    /// and the sound draws nothing.
    #[test]
    fn a_random_use_starts_the_use_sound() {
        let ini = |sound: &str| {
            format!(
                "[Characteristics/Integers]\ncRandomUse=1\ncMinUsePeriod=20\ncMaxUsePeriod=60\n{sound}"
            )
        };
        let run = |sound: &str, now: u32, data: &dyn Data| {
            let mut cat = Catalogue::default();
            cat.push(crate::fixtures::ty("building", "hut", "", &ini(sound)));
            let mut sim = Sim::new(b"");
            let mut w = hut(false);
            w.zoo.entities[0].common.as_mut().unwrap().stamp = Some(10);
            sim.ai_ticks = now;
            sim.idles.register(&w, &cat, data, &mut sim.rng);
            let mut spent = sim.rng.clone();
            sim.random_use(&mut w, &cat, data, 0);
            // The redraw of the time is the one draw, and only when it fired.
            if now > 10 {
                spent.rand();
            }
            assert_eq!(sim.rng.state(), spent.state(), "{sound}");
            // The sound starts with the `used` sequence, not at the check.
            assert!(sim.idles.take_heard().is_empty(), "{sound}");
            let mut heard = Vec::new();
            for _ in 0..12 {
                sim.idles.advance(100);
                heard.extend(sim.idles.take_heard());
            }
            heard
        };
        let sound = |base: i32| {
            let heard = crate::behaviour::Heard {
                file: "scenery/building/hut.wav".into(),
                base,
                voice: "building/1/UseSound".into(),
            };
            vec![(1, heard)]
        };
        let named = "[UseSound]\nloops=1\nname=scenery/building/hut.wav\n";
        assert_eq!(
            run(&format!("{named}attenuation=1300"), 11, &Building),
            sound(1300)
        );
        assert_eq!(
            run(&format!("{named}attenuation=-5"), 11, &Building),
            sound(-5)
        );
        assert_eq!(
            run(&format!("{named}attenuation=0x7FFFFFFF"), 11, &Building),
            sound(i32::MAX)
        );
        assert_eq!(
            run(&format!("{named}attenuation=zz"), 11, &Building),
            sound(0)
        );
        assert_eq!(run(named, 11, &Building), sound(0), "no attenuation");
        // Equal to the time does not fire; no name, an empty name or no section: no sound.
        let at = format!("{named}attenuation=1");
        assert_eq!(run(&at, 10, &Building), vec![]);
        for none in [
            "",
            "[UseSound]\nattenuation=5",
            "[UseSound]\nname=\nattenuation=5",
        ] {
            assert_eq!(run(none, 11, &Building), vec![], "{none}");
        }
        // A type with no `used` animation does nothing.
        let no_used = Fixed(
            Some(Timing {
                count: 4,
                delay: 100,
            }),
            vec![],
        );
        assert_eq!(run(&at, 11, &no_used), vec![]);
    }
}
