//! The looped animations of the fixed objects (sprite drawing 3.6 and 6.2): scenery, buildings and
//! the like play the animation they rest on forward in a loop, one clock each, from the step after
//! the entity appears; the bubbles of a tank filter have a clock of their own (tank staff 7.7). A
//! loop that ends drops the time left over, so it takes a little more than `count x delay`.
//!
//! The animation is the type's first key in byte order (`idle` for nearly all), or for a filter, a
//! food object, a trash can and a dung pile the one their state chooses: a change of state starts
//! it again at frame 0. The clocks are not saved: a loaded zoo restarts them at frame 0. The picture
//! reads the frames without staging the map again (`Staged::reframe`).

use std::collections::{HashMap, HashSet};

use crate::behaviour::{Data, Timing};
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
}

impl Clock {
    fn new(timing: Option<Timing>, state: Option<(String, i32)>) -> Clock {
        let timing = timing.unwrap_or(Timing { count: 0, delay: 0 });
        Clock {
            frame: 0,
            t: 0,
            timing,
            state,
        }
    }

    /// One advance of the loop by `dt` ms (sprite drawing 6.2, the base sequence alone).
    fn advance(&mut self, dt: u32) {
        let Timing { count, delay } = self.timing;
        // A still picture never advances.
        if count <= 1 {
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
                // The base sequence starts again and the time left is dropped.
                (self.frame, self.t) = (0, 0);
                return;
            }
        }
    }
}

/// The clocks of the fixed objects, by entity id, and of the sets of bubbles, by the id of their
/// filter with the count of sets that filter had made when the clock started.
#[derive(Debug, Default)]
pub(crate) struct Idles {
    fixed: HashMap<u32, Clock>,
    bubbles: HashMap<u32, (u32, Clock)>,
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

    /// The start of a step: the entities that are new get a clock at frame 0, which advances by
    /// this step; one made later in the step (a ripple, a set of bubbles) advances from the next. A
    /// state that changed starts its animation again. The clocks of entities that are gone, and of
    /// bubbles that are gone, are dropped.
    pub(crate) fn register(&mut self, world: &World, cat: &Catalogue, data: &dyn Data) {
        let looks = world.filter_looks();
        let mut fixed = 0;
        for e in &world.zoo.entities {
            let Some(c) = &e.common else { continue };
            if Kind::of_class(&e.class) != Kind::Fixed {
                continue;
            }
            fixed += 1;
            let known = self.fixed.get(&c.id);
            if known.is_some_and(|k| k.state.is_none()) {
                continue;
            }
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
            let timing = (state.as_ref().and_then(|s| named(&s.0)))
                .or_else(|| named(base.as_deref().unwrap_or("idle")));
            self.fixed.insert(c.id, Clock::new(timing, state));
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

    /// An entity present when a step starts advances by that step; one made during the step
    /// (a ripple) first advances in the next. Units, fences and objects with no idle animation
    /// have no clock; a removed entity's clock goes.
    #[test]
    fn an_entity_first_seen_at_a_step_advances_by_it() {
        let mut world = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("scenery", "tree", 1, centre((0, 0)), 0));
            zoo.entities
                .push(entity("animals", "lion", 2, centre((1, 1)), 0));
            zoo.entities.push(entity("fences", "f", 3, (32, 0), 0));
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
            idles.register(world, &Catalogue::default(), &data);
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
        none.register(&world, &Catalogue::default(), &Fixed(None, vec![]));
        none.advance(70);
        assert!(none.frames().is_empty());
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
            idles.register(world, &cat, &data);
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
            idles.register(world, &cat, &data);
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
}
