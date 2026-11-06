//! The idle animation of the fixed objects (sprite drawing 3.6 and 6.2): scenery, buildings and the
//! like play their `idle` animation forward in a loop, one clock each, from the step after the
//! entity appears. A loop that ends drops the time left over, so it takes a little more than
//! `count x delay`.
//!
//! The clocks are not saved: a loaded zoo restarts them at frame 0. The picture reads the frames
//! without staging the map again (`Staged::reframe`).

use std::collections::HashMap;

use crate::behaviour::{Data, Timing};
use crate::render::order::Kind;
use crate::world::World;

/// The frame an entity shows and the milliseconds it has been shown, with the animation's timing
/// (a count of 0: no animation to play).
#[derive(Debug, Clone, Copy)]
struct Clock {
    frame: u32,
    t: u32,
    timing: Timing,
}

impl Clock {
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

/// The clocks of the fixed objects, by entity id.
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

    /// One step: the entities that are new get a clock at frame 0, the others advance by `step`.
    pub(super) fn step(&mut self, world: &World, data: &dyn Data, step: u32) {
        let mut fixed = 0;
        for e in &world.zoo.entities {
            let Some(c) = &e.common else { continue };
            if Kind::of_class(&e.class) != Kind::Fixed {
                continue;
            }
            fixed += 1;
            match self.0.get_mut(&c.id) {
                Some(clock) => clock.advance(step),
                None => {
                    let timing = data.timing(e, "idle").unwrap_or(Timing { count: 0, delay: 0 });
                    self.0.insert(c.id, Clock { frame: 0, t: 0, timing });
                }
            }
        }
        // Clocks of entities that are gone.
        if self.0.len() > fixed {
            let live: std::collections::HashSet<u32> = (world.zoo.entities.iter())
                .filter_map(|e| e.common.as_ref().map(|c| c.id))
                .collect();
            self.0.retain(|id, _| live.contains(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::testing::{centre, entity, world_with};

    fn clock(count: u32, delay: u32) -> Clock {
        Clock {
            frame: 0,
            t: 0,
            timing: Timing { count, delay },
        }
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

    /// Each fixed object has its own clock from the step after it appears; units, fences and
    /// objects with no idle animation have none; a removed entity's clock goes.
    #[test]
    fn each_fixed_object_runs_its_own_clock() {
        let mut world = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("scenery", "tree", 1, centre((0, 0)), 0));
            zoo.entities
                .push(entity("animals", "lion", 2, centre((1, 1)), 0));
            zoo.entities.push(entity("fences", "f", 3, (32, 0), 0));
        });
        let data = Fixed(Some(Timing {
            count: 4,
            delay: 100,
        }));
        let mut idles = Idles::default();
        idles.step(&world, &data, 70);
        assert_eq!(idles.frames().into_iter().collect::<Vec<_>>(), [(1, 0)]);
        // The tree appeared at the first step: 70 ms, then 140 ms of 100 per frame.
        idles.step(&world, &data, 70);
        assert_eq!(idles.frames()[&1], 0);
        idles.step(&world, &data, 70);
        assert_eq!(idles.frames()[&1], 1);
        // A second tree starts later and is behind.
        world
            .zoo
            .entities
            .push(entity("scenery", "bush", 4, centre((2, 2)), 0));
        for _ in 0..3 {
            idles.step(&world, &data, 70);
        }
        assert_eq!((idles.frames()[&1], idles.frames()[&4]), (3, 1));
        world.zoo.entities.remove(0);
        idles.step(&world, &data, 70);
        assert!(!idles.frames().contains_key(&1));
        // Types with no `idle` animation show frame 0 and are not listed.
        let mut none = Idles::default();
        none.step(&world, &Fixed(None), 70);
        none.step(&world, &Fixed(None), 70);
        assert!(none.frames().is_empty());
    }
}
