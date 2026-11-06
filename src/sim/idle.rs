//! The looped animations of the fixed objects and the fences (sprite drawing 3.6 and 6.2):
//! scenery, buildings and the like play the animation they rest on forward in a loop, one clock
//! each, from the step after the entity appears; the bubbles of a tank filter have a clock of their
//! own (tank staff 7.7). A loop that ends drops the time left over, so it takes a little more than
//! `count x delay`. When a guest comes into an empty building its base loop becomes `used` once the
//! pass of `idle` in progress ends, and `idle` never comes back; a dead building shows `idledead`
//! at once, and alive again `idle`, both from frame 0; a building of random use queues `used` once
//! behind the base when its time is up (`building::random_use`). A fence that is not a gate starts at a
//! random time; a gate does not loop: its player runs only while it opens or closes.
//!
//! The animation is the type's first key in byte order (`idle` for nearly all), or for a filter, a
//! food object, a trash can and a dung pile the one their state chooses: a change of state starts
//! it again at frame 0. The clocks are not saved: a loaded zoo restarts them at frame 0. The picture
//! reads the frames without staging the map again (`Staged::reframe`).

use std::collections::{BTreeMap, HashMap, HashSet};

use super::Rng;
use crate::behaviour::{Data, Timing};
use crate::building::Live;
use crate::entity::Catalogue;
use crate::place::bulldozer::type_of;
use crate::render::order::Kind;
use crate::render::{bubble_key, rest_name};
use crate::world::World;
use crate::zoo::Entity;

/// The frame an entity shows and the milliseconds it has been shown, with the animation's timing
/// (a count of 0: no animation to play), and the state that chose the animation with the amount
/// it was chosen at, if a state does.
#[derive(Debug, Clone)]
struct Clock {
    frame: u32,
    t: u32,
    timing: Timing,
    state: Option<(String, i32)>,
    /// The animation a building shows and its base animation, which loops.
    name: &'static str,
    base: (&'static str, Timing),
    /// The base the pass in progress hands over to when it ends (`used`).
    next: Option<(&'static str, Timing)>,
    /// A building's users now and its dead state (guest buildings 10.1, 11.1.1).
    users: u32,
    dead: bool,
    /// A one-shot queued behind the base loop (`explode` of a rubble piece): it starts when the
    /// sequence playing ends, then the base loop takes over for good.
    queued: Option<(&'static str, Timing, u32)>,
}

impl Clock {
    fn new(timing: Option<Timing>, state: Option<(String, i32)>) -> Clock {
        let timing = timing.unwrap_or(Timing { count: 0, delay: 0 });
        Clock {
            frame: 0,
            t: 0,
            timing,
            state,
            name: "idle",
            base: ("idle", timing),
            next: None,
            users: 0,
            dead: false,
            queued: None,
        }
    }

    /// One advance of the loop by `dt` ms (sprite drawing 6.2, the base sequence alone).
    fn advance(&mut self, dt: u32) {
        let Timing { count, delay } = self.timing;
        // A still base picture with nothing queued never advances.
        if count <= 1 && self.queued.is_none() && self.name == self.base.0 {
            return;
        }
        // A sequence with no frame ends at once.
        if count == 0 {
            self.restart();
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

    /// The sequence playing ends: the queued one starts, else the base animation again, the one a
    /// guest asked for if there is one, at frame 0; the time left over is dropped.
    fn restart(&mut self) {
        if let Some((name, timing, times)) = self.queued.take() {
            (self.name, self.timing) = (name, timing);
            self.queued = (times > 1).then_some((name, timing, times - 1));
        } else {
            self.base = self.next.take().unwrap_or(self.base);
            (self.name, self.timing) = self.base;
        }
        (self.frame, self.t) = (0, 0);
    }

    /// A building's state of this step: the dead state sets the base at once (`idledead`, back to
    /// `idle`); a guest coming into an empty building makes `used` the base when the pass in
    /// progress ends (guest buildings 10.1).
    fn track(&mut self, e: &Entity, live: &Live, data: &dyn Data) {
        let dead = live.dead;
        if dead != self.dead {
            self.dead = dead;
            let name = if dead { "idledead" } else { "idle" };
            if let Some(timing) = data.timing(e, name) {
                (self.base, self.next) = ((name, timing), None);
                self.restart();
            }
        }
        let users = live.users_now();
        if self.users == 0
            && users > 0
            && !dead
            && let Some(timing) = data.timing(e, "used")
        {
            self.next = Some(("used", timing));
        }
        self.users = users;
    }
}

/// The clocks of the fixed objects and the fences, by entity id, and of the sets of bubbles, by the
/// id of their filter with the count of sets that filter had made when the clock started.
#[derive(Debug, Default)]
pub(crate) struct Idles {
    fixed: BTreeMap<u32, Clock>,
    bubbles: BTreeMap<u32, (u32, Clock)>,
}

impl Idles {
    /// A new game starts no clock.
    pub(crate) fn clear(&mut self) {
        self.fixed.clear();
        self.bubbles.clear();
    }

    /// The frame of each animated clock, for the picture: an entity under its id, the bubbles of a
    /// filter under [`bubble_key`].
    pub(crate) fn frames(&self) -> HashMap<u32, u32> {
        let animated = self.fixed.iter().filter(|(_, c)| c.timing.count > 1);
        let bubbles = (self.bubbles.iter()).map(|(&id, (_, c))| (bubble_key(id), c));
        let bubbles = bubbles.filter(|(_, c)| c.timing.count > 1);
        let all = animated.map(|(&id, c)| (id, c)).chain(bubbles);
        all.map(|(key, c)| (key, c.frame)).collect()
    }

    /// The animation of each entity that does not show `idle` (a building in use or dead, a rubble
    /// piece exploding).
    pub(super) fn looks(&self) -> HashMap<u32, &'static str> {
        let other = self.fixed.iter().filter(|(_, c)| c.name != "idle");
        other.map(|(&id, c)| (id, c.name)).collect()
    }

    /// Queues `explode` once behind the `idle` base of each piece `ids` that turn-to-rubble made
    /// (sprite drawing 3.6): it starts when the sequence playing ends. A piece with no clock or
    /// no such animation plays nothing.
    pub(crate) fn queue_explode(&mut self, world: &World, ids: &[u32], data: &dyn Data) {
        for id in ids {
            let timing =
                (world.find(*id)).and_then(|at| data.timing(&world.zoo.entities[at], "explode"));
            if let (Some(c), Some(t)) = (self.fixed.get_mut(id), timing) {
                c.queued = Some(("explode", t, 1));
            }
        }
    }

    /// Queues `used` `times` times behind the base loop of the building at `at`, as the rubble's
    /// `explode`; the base is not changed (guest buildings 10.1 point 2.4). A building with no
    /// clock or no `used` plays nothing.
    pub(crate) fn queue_used(&mut self, world: &World, at: usize, times: u32, data: &dyn Data) {
        let e = &world.zoo.entities[at];
        let id = e.common.as_ref().map_or(0, |c| c.id);
        if let (Some(c), Some(t)) = (self.fixed.get_mut(&id), data.timing(e, "used")) {
            // A play that comes while another is queued adds to it.
            let waiting = c.queued.filter(|q| q.0 == "used").map_or(0, |q| q.2);
            let times = times.saturating_add(waiting);
            c.queued = (times > 0).then_some(("used", t, times));
        }
    }

    /// The start of a step: the entities that are new get a clock at frame 0, which advances by
    /// this step; one made later in the step (a ripple, a set of bubbles) advances from the next. A
    /// state that changed starts its animation again. A fence that is not a gate starts at a random
    /// time of 0 to 1999 ms (sprite drawing 3.6), a still one too. The clocks of entities that are
    /// gone, and of bubbles that are gone, are dropped.
    pub(crate) fn register(
        &mut self,
        world: &World,
        cat: &Catalogue,
        data: &dyn Data,
        rng: &mut Rng,
    ) {
        let looks = world.filter_looks();
        let mut fixed = 0;
        for e in &world.zoo.entities {
            let Some(c) = &e.common else { continue };
            let kind = Kind::of_class(&e.class);
            if !matches!(kind, Kind::Fixed | Kind::Edge) {
                continue;
            }
            fixed += 1;
            // A trash can shows its own pictures.
            let live = (e.class == "building" && e.subtype != "trshcan")
                .then(|| world.units.buildings.get(&c.id))
                .flatten();
            if let Some(k) = self.fixed.get_mut(&c.id)
                && k.state.is_none()
            {
                if let Some(live) = live {
                    k.track(e, live, data);
                }
                continue;
            }
            let known = self.fixed.get(&c.id);
            let ty = type_of(cat, e);
            let life = (c.filter).map(|f| i32::try_from(f.health).unwrap_or(i32::MAX));
            let off = looks.iter().any(|l| l.id == c.id && l.off);
            // A change of the amount restarts the animation, even in the same state (3.6).
            let state = ty
                .and_then(|t| rest_name(t, life, off, c.food))
                .map(|n| (n, c.food));
            if known.is_some_and(|k| k.state == state) {
                continue;
            }
            // The animation the state names, else the first key of the type.
            let base = ty.and_then(|t| t.base_animation()).map(|a| a.name);
            let named = |name: &str| data.timing(e, name);
            let mut timing = (state.as_ref().and_then(|s| named(&s.0)))
                .or_else(|| named(base.as_deref().unwrap_or("idle")));
            let mut start = 0;
            if kind == Kind::Edge {
                // A gate does not loop; any other fence draws its start time, a still one too
                // (the original draws before it looks at the frame count).
                match e.subtype.starts_with('g') {
                    true => timing = None,
                    false => start = rng.pick(2000),
                }
            }
            let mut clock = Clock::new(timing, state);
            clock.t = start;
            if let Some(live) = live {
                // Guests inside a loaded building did not just come in.
                clock.users = live.users_now();
                clock.track(e, live, data);
            }
            self.fixed.insert(c.id, clock);
        }
        if self.fixed.len() > fixed {
            let live: HashSet<u32> = (world.zoo.entities.iter())
                .filter_map(|e| e.common.as_ref().map(|c| c.id))
                .collect();
            self.fixed.retain(|id, _| live.contains(id));
        }
        self.register_bubbles(world, data);
    }

    /// The bubbles of the filters: a set that is new starts at frame 0 with the timing of the
    /// type's `idle` animation, and goes when its filter shows none.
    fn register_bubbles(&mut self, world: &World, data: &dyn Data) {
        let shown: Vec<(u32, u32)> = world.filter_bubbles().collect();
        self.bubbles
            .retain(|id, _| shown.iter().any(|s| s.0 == *id));
        if shown.is_empty() {
            return;
        }
        let ty = |name: &str| Entity {
            class: "objects".into(),
            type_name: "other".into(),
            subtype: name.into(),
            body_size: None,
            common: None,
        };
        let timing = data.timing(&ty("tankbub1"), "idle");
        for (id, made) in shown {
            if self.bubbles.get(&id).is_none_or(|b| b.0 != made) {
                self.bubbles.insert(id, (made, Clock::new(timing, None)));
            }
        }
    }

    /// The end of the entity pass: every clock advances by `step`.
    pub(crate) fn advance(&mut self, step: u32) {
        self.fixed.values_mut().for_each(|c| c.advance(step));
        (self.bubbles.values_mut()).for_each(|b| b.1.advance(step));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::unit::testing::{centre, entity, world_with};

    fn clock(count: u32, delay: u32) -> Clock {
        Clock::new(Some(Timing { count, delay }), None)
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

    /// The timing of `idle`, and of the animations `named`.
    pub(crate) struct Fixed(pub Option<Timing>, pub Vec<(&'static str, Timing)>);

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
            let named = self.1.iter().find(|n| n.0 == name).map(|n| n.1);
            named.or(self.0.filter(|_| name == "idle"))
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
        let data = Fixed(
            Some(Timing {
                count: 4,
                delay: 100,
            }),
            vec![],
        );
        let mut idles = Idles::default();
        let step = |idles: &mut Idles, world: &World, ms: u32| {
            idles.register(world, &Catalogue::default(), &data, &mut Rng::default());
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
        none.register(
            &world,
            &Catalogue::default(),
            &Fixed(None, vec![]),
            &mut Rng::default(),
        );
        none.advance(70);
        assert!(none.frames().is_empty());
    }

    /// A world of `n` fences of ids 1 to `n` and, when `gate`, a gate of id 100.
    fn fences(n: u32, gate: bool) -> World {
        world_with((4, 4), |zoo| {
            for id in 1..=n {
                zoo.entities.push(entity("fences", "f", id, (32, 0), 0));
            }
            if gate {
                zoo.entities.push(entity("fences", "g", 100, (32, 0), 0));
                zoo.entities.last_mut().unwrap().subtype = "gate".into();
            }
        })
    }

    /// A fence that is not a gate starts at a random time of 0 to 1999 ms, one draw each.
    #[test]
    fn a_fence_starts_at_a_random_time() {
        let world = fences(40, true);
        // 20 frames of 100 ms: a loop of 2000 ms shows every start.
        let data = Fixed(
            Some(Timing {
                count: 20,
                delay: 100,
            }),
            vec![],
        );
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        idles.register(&world, &Catalogue::default(), &data, &mut rng);
        let mut expect = Rng::default();
        let starts: Vec<u32> = (0..40).map(|_| expect.pick(2000)).collect();
        assert_eq!(rng.state(), expect.state(), "40 draws, none for the gate");
        for (id, t) in (1..=40).zip(&starts) {
            assert_eq!(idles.fixed[&id].t, *t);
            assert!(*t < 2000);
        }
        // Neighbouring fences are not in step.
        idles.advance(67);
        let frames = idles.frames();
        assert!(frames.values().any(|&f| f != frames[&1]));
        // A fence already known draws nothing more.
        idles.register(&world, &Catalogue::default(), &data, &mut rng);
        assert_eq!(rng.state(), expect.state());
    }

    /// A fence whose idle is a still picture draws its start time all the same: one draw a piece,
    /// none for a gate, and with no animation at all.
    #[test]
    fn a_still_fence_draws_its_start_time_too() {
        let world = fences(3, true);
        let mut expect = Rng::default();
        (0..3).for_each(|_| _ = expect.pick(2000));
        for data in [
            Fixed(
                Some(Timing {
                    count: 1,
                    delay: 100,
                }),
                vec![],
            ),
            Fixed(None, vec![]),
        ] {
            let mut rng = Rng::default();
            Idles::default().register(&world, &Catalogue::default(), &data, &mut rng);
            assert_eq!(rng.state(), expect.state());
        }
    }

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
            idles.queue_used(&w, 0, times, data);
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

    /// A gate does not loop by itself: with an animated idle it stays on frame 0 after any number
    /// of steps (its player runs only while it opens or closes).
    #[test]
    fn a_gate_stays_on_frame_zero() {
        let world = fences(1, true);
        let data = Fixed(
            Some(Timing {
                count: 20,
                delay: 100,
            }),
            vec![],
        );
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        for _ in 0..50 {
            idles.register(&world, &Catalogue::default(), &data, &mut rng);
            idles.advance(67);
        }
        assert!(!idles.frames().contains_key(&100));
        assert!(idles.frames().contains_key(&1), "a fence loops");
    }

    /// A guest coming into an empty building lets the pass of `idle` end, then `used` loops from
    /// frame 0 and `idle` does not come back; a second guest changes nothing. A dead building
    /// shows `idledead` at once, and alive again `idle`, from frame 0.
    #[test]
    fn a_building_keeps_used_as_its_base_loop() {
        let world = |users: u32, dead: bool| {
            let mut w = world_with((4, 4), |zoo| {
                zoo.entities
                    .push(entity("building", "hut", 1, centre((0, 0)), 0));
            });
            let live = crate::building::Live {
                extra: users,
                dead,
                ..Default::default()
            };
            w.units.buildings.insert(1, live);
            w
        };
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        let mut step = |w: &World, ms: u32| {
            idles.register(w, &Catalogue::default(), &Building, &mut rng);
            idles.advance(ms);
            (
                idles.looks().get(&1).copied(),
                idles.frames().get(&1).copied(),
            )
        };
        let (empty, one, two) = (world(0, false), world(1, false), world(2, false));
        assert_eq!(step(&empty, 150), (None, Some(1)));
        // A guest comes in: the pass of `idle` (4 frames of 100 ms) goes on to its end.
        assert_eq!(step(&one, 100), (None, Some(2)));
        assert_eq!(
            step(&two, 100),
            (None, Some(3)),
            "a second guest: no change"
        );
        // The pass ends: `used` (2 frames) from frame 0.
        assert_eq!(step(&two, 100), (Some("used"), Some(0)));
        assert_eq!(step(&one, 100), (Some("used"), Some(1)));
        // It loops, and it does not go back to `idle` when the guest leaves.
        assert_eq!(step(&empty, 100), (Some("used"), Some(0)));
        assert_eq!(step(&empty, 100), (Some("used"), Some(1)));
        // Another guest in an empty building changes nothing more.
        assert_eq!(step(&one, 100), (Some("used"), Some(0)));
        // Dead: `idledead` at once from frame 0, looping over its 3 frames.
        let dead = world(0, true);
        assert_eq!(step(&dead, 0), (Some("idledead"), Some(0)));
        assert_eq!(step(&dead, 100), (Some("idledead"), Some(1)));
        assert_eq!(step(&dead, 200), (Some("idledead"), Some(0)), "wrapped");
        // Alive again: `idle` at once from frame 0, and `used` is gone.
        assert_eq!(step(&empty, 100), (None, Some(1)));
        // A building first seen with guests inside is not a guest coming in.
        let mut seen = Idles::default();
        seen.register(&one, &Catalogue::default(), &Building, &mut Rng::default());
        seen.advance(1000);
        assert!(seen.looks().is_empty());
        // A type with no `used` keeps its loop, and a trash can its own pictures.
        let mut none = Idles::default();
        let fixed = Fixed(
            Some(Timing {
                count: 4,
                delay: 100,
            }),
            vec![],
        );
        none.register(&empty, &Catalogue::default(), &fixed, &mut rng);
        none.register(&one, &Catalogue::default(), &fixed, &mut rng);
        none.advance(250);
        assert_eq!((none.looks().len(), none.frames()[&1]), (0, 2));
        let mut can = world(1, false);
        can.zoo.entities[0].subtype = "trshcan".into();
        let mut idles = Idles::default();
        idles.register(&world(0, false), &Catalogue::default(), &Building, &mut rng);
        idles.register(&can, &Catalogue::default(), &Building, &mut rng);
        idles.advance(1000);
        assert!(idles.looks().is_empty());
    }

    /// A food object plays the animation its fill chooses (sprite drawing 3.6; the type's first
    /// key, `empty`, is not it); a change of fill starts the other animation at frame 0; a state
    /// whose animation the type lacks falls back to `idle`.
    #[test]
    fn a_food_object_loops_the_animation_its_state_chooses_and_restarts_when_it_changes() {
        let mut cat = Catalogue::default();
        let ini = "[Characteristics/Integers]\ncMaxFoodUnits=12\n";
        cat.push(crate::fixtures::ty("food", "food", "chum", ini));
        let mut world = world_with((4, 4), |zoo| {
            let mut e = entity("food", "food", 1, centre((0, 0)), 0);
            e.subtype = "chum".into();
            e.common.as_mut().unwrap().food = 12;
            zoo.entities.push(e);
        });
        let slow = Timing {
            count: 4,
            delay: 100,
        };
        let fast = Timing {
            count: 4,
            delay: 50,
        };
        let data = Fixed(None, vec![("full", slow), ("half", fast)]);
        let mut idles = Idles::default();
        let step = |idles: &mut Idles, world: &World, ms: u32| {
            idles.register(world, &cat, &data, &mut Rng::default());
            idles.advance(ms);
        };
        step(&mut idles, &world, 200);
        assert_eq!(idles.frames()[&1], 2, "full");
        step(&mut idles, &world, 70);
        assert_eq!(
            idles.frames()[&1],
            2,
            "70 ms are not a frame: still the same state"
        );
        // Half full: its own animation, from frame 0, with its own delay.
        world.zoo.entities[0].common.as_mut().unwrap().food = 6;
        step(&mut idles, &world, 70);
        assert_eq!(idles.frames()[&1], 1, "half");
        // Empty has no animation here and no `idle`: nothing to play.
        world.zoo.entities[0].common.as_mut().unwrap().food = 0;
        step(&mut idles, &world, 70);
        assert!(!idles.frames().contains_key(&1), "empty");
        // Nothing left of a type that has no fill (huge, negative): the state clamps.
        for food in [i32::MAX, i32::MIN, -1] {
            world.zoo.entities[0].common.as_mut().unwrap().food = food;
            step(&mut idles, &world, u32::MAX);
            assert!(idles.frames().values().all(|&f| f < 4), "{food}");
        }
    }

    /// A food object restarts its loop at frame 0 at every change of its amount, a bite that leaves
    /// it `full` included (sprite drawing 3.6).
    #[test]
    fn a_food_object_restarts_its_loop_at_every_change_of_its_amount() {
        let mut cat = Catalogue::default();
        let ini = "[Characteristics/Integers]\ncMaxFoodUnits=12\n";
        cat.push(crate::fixtures::ty("food", "food", "chum", ini));
        let mut world = world_with((4, 4), |zoo| {
            let mut e = entity("food", "food", 1, centre((0, 0)), 0);
            e.subtype = "chum".into();
            e.common.as_mut().unwrap().food = 12;
            zoo.entities.push(e);
        });
        let full = Timing {
            count: 4,
            delay: 100,
        };
        let data = Fixed(None, vec![("full", full)]);
        let mut idles = Idles::default();
        let mut step = |world: &World, ms: u32| {
            idles.register(world, &cat, &data, &mut Rng::default());
            idles.advance(ms);
            idles.frames()[&1]
        };
        assert_eq!(step(&world, 200), 2);
        // Still `full` after the bite: frame 0 again, then 70 ms.
        world.zoo.entities[0].common.as_mut().unwrap().food = 11;
        assert_eq!(step(&world, 70), 0);
        // The same amount goes on.
        assert_eq!(step(&world, 70), 1);
    }

    /// The pieces whose `explode` is playing, in id order.
    fn exploding(idles: &Idles) -> Vec<u32> {
        let mut ids: Vec<u32> = (idles.looks().into_iter())
            .filter(|(_, n)| *n == "explode")
            .map(|(id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// A rubble piece made by turn-to-rubble plays `explode` once, from the end of its `idle`
    /// loop (one delay), then `idle` again for good; a piece not queued, or whose type has no
    /// `explode`, never does (sprite drawing 3.6).
    #[test]
    fn a_rubble_piece_plays_explode_once_after_its_idle_loop() {
        let world = world_with((4, 4), |zoo| {
            for id in 1..=3 {
                zoo.entities
                    .push(entity("rubble", "other", id, centre((0, 0)), 0));
            }
        });
        let idle = Timing {
            count: 1,
            delay: 125,
        };
        let explode = Timing {
            count: 3,
            delay: 100,
        };
        let data = Fixed(Some(idle), vec![("explode", explode)]);
        let mut idles = Idles::default();
        idles.register(&world, &Catalogue::default(), &data, &mut Rng::default());
        // Piece 3 is not queued; an unknown id is ignored.
        idles.queue_explode(&world, &[1, 2, 99], &data);
        let step = |idles: &mut Idles, ms: u32| {
            idles.advance(ms);
            (exploding(idles), idles.frames())
        };
        let (on, _) = step(&mut idles, 124);
        assert!(on.is_empty(), "the idle delay has not ended");
        let (on, frames) = step(&mut idles, 1);
        assert_eq!(on, [1, 2]);
        assert_eq!((frames[&1], frames[&2]), (0, 0));
        assert!(!frames.contains_key(&3));
        let (_, frames) = step(&mut idles, 250);
        assert_eq!(frames[&1], 2);
        // The last frame ends: idle again, still, and no second explosion.
        let (on, frames) = step(&mut idles, 50);
        assert!(on.is_empty() && frames.is_empty());
        let (on, frames) = step(&mut idles, u32::MAX);
        assert!(on.is_empty() && frames.is_empty());
    }

    /// A type with no `explode`, a delay of 0 or a huge step never end a run or play twice.
    #[test]
    fn a_missing_explode_or_odd_timings_play_nothing_or_stay_on_a_frame() {
        let world = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("rubble", "other", 1, centre((0, 0)), 0));
        });
        let idle = Timing {
            count: 1,
            delay: 125,
        };
        let none = Fixed(Some(idle), vec![]);
        let mut idles = Idles::default();
        idles.register(&world, &Catalogue::default(), &none, &mut Rng::default());
        idles.queue_explode(&world, &[1], &none);
        idles.advance(1000);
        assert!(exploding(&idles).is_empty());
        for explode in [
            Timing { count: 4, delay: 0 },
            Timing {
                count: 0,
                delay: 83,
            },
            Timing { count: 1, delay: 1 },
        ] {
            let data = Fixed(Some(idle), vec![("explode", explode)]);
            let mut idles = Idles::default();
            idles.register(&world, &Catalogue::default(), &data, &mut Rng::default());
            idles.queue_explode(&world, &[1], &data);
            idles.advance(125);
            idles.advance(u32::MAX);
            let frames = idles.frames();
            assert!(frames.values().all(|&f| f < 4));
            // A delay of 0 never ends; the others are back on `idle`.
            assert_eq!(exploding(&idles).len(), usize::from(explode.delay == 0));
        }
    }
}
