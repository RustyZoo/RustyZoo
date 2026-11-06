//! The idle animation of the fixed objects and the fences (sprite drawing 3.6 and 6.2): scenery,
//! buildings and the like play their `idle` animation forward in a loop, one clock each, from the
//! step after the entity appears. A loop that ends drops the time left over, so it takes a little
//! more than `count x delay`. A building plays `used` once when a guest comes into it and
//! `idledead` while it is dead; the base loop then starts again at frame 0.
//!
//! The clocks are not saved: a loaded zoo restarts them at frame 0. The picture reads the frames
//! without staging the map again (`Staged::reframe`).

use std::collections::HashMap;

use super::Rng;
use crate::behaviour::{Data, Timing};
use crate::building::Live;
use crate::render::order::Kind;
use crate::world::World;
use crate::zoo::Entity;

/// The animation an entity plays and the milliseconds it has been shown, with its timing (a
/// count of 0: no animation to play).
#[derive(Debug, Clone, Copy)]
struct Clock {
    frame: u32,
    t: u32,
    name: &'static str,
    timing: Timing,
    /// The animation is a sequence: it plays once, then the base takes over from frame 0.
    once: bool,
    /// The base animation, which loops.
    base: (&'static str, Timing),
    /// A building has a guest inside; it is in its dead state (guest buildings 10.1, 11.1.1).
    inside: bool,
    dead: bool,
}

impl Clock {
    fn new(timing: Timing, t: u32) -> Clock {
        Clock {
            frame: 0,
            t,
            name: "idle",
            timing,
            once: false,
            base: ("idle", timing),
            inside: false,
            dead: false,
        }
    }

    /// One advance by `dt` ms (sprite drawing 6.2).
    fn advance(&mut self, dt: u32) {
        let Timing { count, delay } = self.timing;
        // A still base picture never advances.
        if count <= 1 && !self.once {
            return;
        }
        self.t = self.t.saturating_add(dt);
        if delay == 0 {
            return;
        }
        while self.t >= delay {
            self.t -= delay;
            self.frame += 1;
            if self.frame >= count {
                self.restart();
                return;
            }
        }
    }

    /// The base animation starts again at frame 0; the time left over is dropped.
    fn restart(&mut self) {
        (self.name, self.timing) = self.base;
        (self.frame, self.t, self.once) = (0, 0, false);
    }

    /// A building's state of this step: the dead state changes the base (`idledead`, back to
    /// `idle`); a guest coming in from an empty building starts `used` (guest buildings 10.1).
    fn track(&mut self, e: &Entity, live: &Live, data: &dyn Data) {
        let dead = live.dead;
        if dead != self.dead {
            self.dead = dead;
            let name = if dead { "idledead" } else { "idle" };
            if let Some(timing) = data.timing(e, name) {
                self.base = (name, timing);
                self.restart();
            }
        }
        let inside = live.inside();
        if inside
            && !self.inside
            && !dead
            && let Some(timing) = data.timing(e, "used")
        {
            (self.name, self.timing, self.once) = ("used", timing, true);
            (self.frame, self.t) = (0, 0);
        }
        self.inside = inside;
    }
}

/// The clocks of the fixed objects and the fences, by entity id.
#[derive(Debug, Default)]
pub(super) struct Idles(HashMap<u32, Clock>);

impl Idles {
    /// A new game starts no clock.
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    /// The frame of each animated entity, for the picture.
    pub(super) fn frames(&self) -> HashMap<u32, u32> {
        let animated = self.0.iter().filter(|(_, c)| c.timing.count > 1);
        animated.map(|(&id, c)| (id, c.frame)).collect()
    }

    /// The animation of each entity that does not show `idle` (a building in use or dead).
    pub(super) fn looks(&self) -> HashMap<u32, &'static str> {
        let other = self.0.iter().filter(|(_, c)| c.name != "idle");
        other.map(|(&id, c)| (id, c.name)).collect()
    }

    /// The start of a step: the entities that are new get a clock at frame 0, which advances by
    /// this step; one made later in the step (a ripple) advances from the next. A fence that is
    /// not a gate starts at a random time of 0 to 1999 ms (sprite drawing 3.6). The clocks of
    /// entities that are gone are dropped.
    pub(super) fn register(&mut self, world: &World, data: &dyn Data, rng: &mut Rng) {
        let mut animated = 0;
        for e in &world.zoo.entities {
            let Some(c) = &e.common else { continue };
            let kind = Kind::of_class(&e.class);
            if !matches!(kind, Kind::Fixed | Kind::Edge) {
                continue;
            }
            animated += 1;
            // A trash can shows its own pictures.
            let live = (e.class == "building" && e.subtype != "trshcan")
                .then(|| world.units.buildings.get(&c.id))
                .flatten();
            let clock = self.0.entry(c.id).or_insert_with(|| {
                let timing = (data.timing(e, "idle")).unwrap_or(Timing { count: 0, delay: 0 });
                let random = kind == Kind::Edge && !e.subtype.starts_with('g');
                let mut clock = Clock::new(timing, if random { rng.pick(2000) } else { 0 });
                // Guests inside a loaded building did not just come in.
                clock.inside = live.is_some_and(Live::inside);
                clock
            });
            if let Some(live) = live {
                clock.track(e, live, data);
            }
        }
        if self.0.len() > animated {
            let live: std::collections::HashSet<u32> = (world.zoo.entities.iter())
                .filter_map(|e| e.common.as_ref().map(|c| c.id))
                .collect();
            self.0.retain(|id, _| live.contains(id));
        }
    }

    /// The end of the entity pass: every clock advances by `step`.
    pub(super) fn advance(&mut self, step: u32) {
        self.0.values_mut().for_each(|c| c.advance(step));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::testing::{centre, entity, world_with};

    fn clock(count: u32, delay: u32) -> Clock {
        Clock::new(Timing { count, delay }, 0)
    }

    /// A loop drops the time left over at its end: 21 frames of 125 ms at 67 ms steps take
    /// 2680 ms, not 2625.
    #[test]
    fn a_loop_drops_the_time_left_over_when_it_wraps() {
        let mut c = clock(21, 125);
        let mut passes = Vec::new();
        for _ in 0..2 {
            let mut steps = 0;
            loop {
                c.advance(67);
                steps += 1;
                if c.frame == 0 && steps > 1 {
                    break;
                }
            }
            passes.push(steps * 67);
            assert_eq!(c.t, 0);
        }
        // The second pass starts from nothing again: same length.
        assert_eq!(passes, [2680, 2680]);
    }

    #[test]
    fn a_still_picture_or_a_delay_of_zero_never_advances() {
        for (count, delay) in [(1, 125), (0, 125), (5, 0)] {
            let mut c = clock(count, delay);
            c.advance(10_000);
            assert_eq!(c.frame, 0, "{count} {delay}");
        }
        // A huge step still ends on a frame of the loop.
        let mut c = clock(3, 1);
        c.advance(u32::MAX);
        assert!(c.frame < 3);
    }

    struct Fixed(Option<Timing>);

    /// A type with `idle` of 4 frames of 100 ms, `used` of 2 frames of 100 ms and `idledead` of
    /// 3 frames of 100 ms.
    struct Building;

    impl Data for Fixed {
        fn set(&self, _: &crate::zoo::Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
            None
        }
        fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[crate::behaviour::Row] {
            &[]
        }
        fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
            self.0.filter(|_| name == "idle")
        }
        fn number(&self, _: &crate::zoo::Entity, _: &crate::behaviour::Number) -> i32 {
            0
        }
        fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Heard> {
            None
        }
    }

    impl Data for Building {
        fn set(&self, _: &crate::zoo::Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
            None
        }
        fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[crate::behaviour::Row] {
            &[]
        }
        fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
            let count = match name {
                "idle" => 4,
                "used" => 2,
                "idledead" => 3,
                _ => return None,
            };
            Some(Timing { count, delay: 100 })
        }
        fn number(&self, _: &crate::zoo::Entity, _: &crate::behaviour::Number) -> i32 {
            0
        }
        fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Heard> {
            None
        }
    }

    /// An entity present when a step starts advances by that step; one made during the step
    /// (a ripple) first advances in the next. Units and objects with no idle animation have no
    /// clock; a removed entity's clock goes.
    #[test]
    fn an_entity_first_seen_at_a_step_advances_by_it() {
        let mut world = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("scenery", "tree", 1, centre((0, 0)), 0));
            zoo.entities
                .push(entity("animals", "lion", 2, centre((1, 1)), 0));
        });
        let data = Fixed(Some(Timing {
            count: 4,
            delay: 100,
        }));
        let mut idles = Idles::default();
        let step = |idles: &mut Idles, world: &World, ms: u32| {
            idles.register(world, &data, &mut Rng::default());
            idles.advance(ms);
        };
        // The first step of a loaded map counts: 200 ms of 100 per frame.
        step(&mut idles, &world, 200);
        assert_eq!(idles.frames().into_iter().collect::<Vec<_>>(), [(1, 2)]);
        // A second tree made in the middle of a step does not advance in it.
        world
            .zoo
            .entities
            .push(entity("scenery", "bush", 4, centre((2, 2)), 0));
        idles.advance(70);
        assert_eq!(idles.frames().get(&4), None);
        step(&mut idles, &world, 70);
        assert_eq!((idles.frames()[&1], idles.frames()[&4]), (3, 0));
        step(&mut idles, &world, 70);
        assert_eq!(idles.frames()[&4], 1);
        world.zoo.entities.remove(0);
        step(&mut idles, &world, 70);
        assert!(!idles.frames().contains_key(&1));
        // Types with no `idle` animation show frame 0 and are not listed.
        let mut none = Idles::default();
        none.register(&world, &Fixed(None), &mut Rng::default());
        none.advance(70);
        assert!(none.frames().is_empty());
    }

    /// A fence that is not a gate starts at a random time of 0 to 1999 ms, one draw each; a gate
    /// starts at 0 and draws nothing.
    #[test]
    fn a_fence_starts_at_a_random_time_and_a_gate_at_zero() {
        let world = world_with((4, 4), |zoo| {
            for id in 1..=40 {
                zoo.entities.push(entity("fences", "f", id, (32, 0), 0));
            }
            zoo.entities.push(entity("fences", "g", 41, (32, 0), 0));
            zoo.entities.last_mut().unwrap().subtype = "gate".into();
        });
        // 20 frames of 100 ms: a loop of 2000 ms shows every start.
        let data = Fixed(Some(Timing {
            count: 20,
            delay: 100,
        }));
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        idles.register(&world, &data, &mut rng);
        let mut expect = Rng::default();
        let starts: Vec<u32> = (0..40).map(|_| expect.pick(2000)).collect();
        assert_eq!(rng.state(), expect.state(), "40 draws, none for the gate");
        for (id, t) in (1..=40).zip(&starts) {
            assert_eq!(idles.0[&id].t, *t);
            assert!(*t < 2000);
        }
        assert_eq!(idles.0[&41].t, 0);
        // The frames are not all the same: neighbouring fences are not in step.
        idles.advance(67);
        let frames = idles.frames();
        assert!(frames.values().any(|&f| f != frames[&41]));
        // A fence already known draws nothing more.
        idles.register(&world, &data, &mut rng);
        assert_eq!(rng.state(), expect.state());
    }

    /// A guest coming into an empty building starts `used`, which plays once; the base loop then
    /// restarts at frame 0. A dead building shows `idledead`, looping, until it is alive again.
    #[test]
    fn a_building_plays_used_once_and_idledead_while_dead() {
        let world = |f: &dyn Fn(&mut crate::building::Live)| {
            let mut w = world_with((4, 4), |zoo| {
                zoo.entities
                    .push(entity("building", "hut", 1, centre((0, 0)), 0));
            });
            let mut live = crate::building::Live::default();
            f(&mut live);
            w.units.buildings.insert(1, live);
            w
        };
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        let mut step = |w: &World, ms: u32| {
            idles.register(w, &Building, &mut rng);
            idles.advance(ms);
            (
                idles.looks().get(&1).copied(),
                idles.frames().get(&1).copied(),
            )
        };
        let empty = world(&|_| {});
        assert_eq!(step(&empty, 150), (None, Some(1)));
        // A guest comes in: `used` from frame 0, advanced by the step it comes in.
        let inside = world(&|l| l.users.push((9, 0, 0)));
        assert_eq!(step(&inside, 100), (Some("used"), Some(1)));
        // (A building first seen with a guest inside is not a guest coming in.)
        let mut seen = Idles::default();
        seen.register(&inside, &Building, &mut Rng::default());
        assert!(seen.looks().is_empty());
        // It stays inside; the sequence ends and the base restarts at 0, not where it was.
        assert_eq!(step(&inside, 100), (None, Some(0)));
        assert_eq!(step(&inside, 100), (None, Some(1)), "no new use");
        // Out, then in again: `used` again.
        assert_eq!(step(&empty, 100).0, None);
        assert_eq!(step(&inside, 50), (Some("used"), Some(0)));
        // Dead: `idledead` from frame 0, looping over its 3 frames.
        let dead = world(&|l| l.dead = true);
        assert_eq!(step(&dead, 100), (Some("idledead"), Some(1)));
        assert_eq!(step(&dead, 200), (Some("idledead"), Some(0)), "wrapped");
        // Alive again: `idle` from frame 0.
        assert_eq!(step(&empty, 100), (None, Some(1)));
        // A type with no `used` or `idledead` keeps its loop.
        let mut none = Idles::default();
        none.register(
            &inside,
            &Fixed(Some(Timing {
                count: 4,
                delay: 100,
            })),
            &mut rng,
        );
        none.advance(250);
        assert_eq!((none.looks().len(), none.frames()[&1]), (0, 2));
        // A trash can shows its own pictures, whoever is inside.
        let mut can = world(&|l| l.users.push((9, 0, 0)));
        can.zoo.entities[0].subtype = "trshcan".into();
        let mut idles = Idles::default();
        idles.register(&can, &Building, &mut rng);
        idles.register(&can, &Building, &mut rng);
        assert!(idles.looks().is_empty());
    }
}
