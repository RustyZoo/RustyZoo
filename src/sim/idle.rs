//! The looped animations of the fixed objects and the fences (sprite drawing 3.6 and 6.2):
//! scenery, buildings and the like play the animation they rest on forward in a loop, one clock
//! each, from the step after the entity appears; the bubbles of a tank filter have a clock of their
//! own (tank staff 7.7). A loop that ends drops the time left over, so it takes a little more than
//! `count x delay`. When a guest comes into an empty building its base loop becomes `used` once the
//! pass of `idle` in progress ends, and `idle` never comes back; a dead building shows `idledead`
//! at once, and alive again `idle`, both from frame 0; a building of random use queues `used` once
//! behind the base when its time is up (`building::random_use`). A fence that is not a gate starts at a
//! random time, again when its picture changes or it wears, and plays the row its life and the slope
//! choose; a gate does not loop: its player runs only while it opens or closes.
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
use crate::render::fence_name;
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
                // The switch clears the player, a queued `used` included.
                (self.base, self.next, self.queued) = ((name, timing), None, None);
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

/// The animation a fence piece that is not a gate shows, as the state of its clock: it changes
/// with its life and the slope under it (sprite drawing 3.6).
fn fence_state(world: &World, cat: &Catalogue, e: &Entity) -> Option<(String, i32)> {
    let c = e.common.as_ref()?;
    let ty = type_of(cat, e)?;
    let tile = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64));
    let tile = tile.map(|t| &world.zoo.tiles[t]);
    let life = c.fence.map(|f| f.life);
    Some((fence_name(ty, life, tile, c.rotation % 8), 0))
}

/// The clock of a fence piece whose animation starts at `start` ms (sprite drawing 3.6).
fn fence_clock(world: &World, cat: &Catalogue, data: &dyn Data, e: &Entity, start: u32) -> Clock {
    let state = fence_state(world, cat, e);
    let timing =
        (state.as_ref().and_then(|s| data.timing(e, &s.0))).or_else(|| data.timing(e, "idle"));
    let mut clock = Clock::new(timing, state);
    clock.t = start;
    clock
}

/// The clocks of the fixed objects and the fences, by entity id, and of the sets of bubbles, by the
/// id of their filter with the count of sets that filter had made when the clock started.
#[derive(Debug, Default)]
pub(crate) struct Idles {
    fixed: BTreeMap<u32, Clock>,
    bubbles: BTreeMap<u32, (u32, Clock)>,
    /// The `used` queued by the check of this step (entity id, timing, times): it joins the queue
    /// after the clocks advance, so it cannot start before the next step.
    later: Vec<(u32, Timing, u32)>,
}

impl Idles {
    /// A new game starts no clock.
    pub(crate) fn clear(&mut self) {
        self.fixed.clear();
        self.bubbles.clear();
        self.later.clear();
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
    /// `explode`; the base is not changed (guest buildings 10.1 point 2.4). It joins the queue
    /// when the clocks advance at the end of the step: the original advances the animation before
    /// the check. A building with no clock or no `used` plays nothing.
    pub(crate) fn queue_used(&mut self, world: &World, at: usize, times: u32, data: &dyn Data) {
        let e = &world.zoo.entities[at];
        let id = e.common.as_ref().map_or(0, |c| c.id);
        if let (true, Some(t)) = (self.fixed.contains_key(&id), data.timing(e, "used")) {
            self.later.push((id, t, times));
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
            // A fence that is not a gate draws its start time, a still one too (the original draws
            // before it looks at the frame count), and again when its picture changes.
            if kind == Kind::Edge && !e.subtype.starts_with('g') {
                let state = fence_state(world, cat, e);
                if self.fixed.get(&c.id).is_none_or(|k| k.state != state) {
                    self.fixed
                        .insert(c.id, fence_clock(world, cat, data, e, rng.pick(2000)));
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
            // A gate does not loop.
            if kind == Kind::Edge {
                timing = None;
            }
            let mut clock = Clock::new(timing, state);
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

    /// A fence piece's wear step ran at `at` in the entity loop (placing 7.4): its picture is
    /// chosen again, which restarts its animation at a random time of 0 to 1999 ms, `draws` times
    /// (a break draws three times: the break's picture, its validation's and the wear step's).
    pub(crate) fn rewind_fence(
        &mut self,
        world: &World,
        cat: &Catalogue,
        data: &dyn Data,
        rng: &mut Rng,
        (at, draws): (usize, u32),
    ) {
        let e = &world.zoo.entities[at];
        if let Some(c) = &e.common {
            let start = (0..draws).map(|_| rng.pick(2000)).last().unwrap_or(0);
            self.fixed
                .insert(c.id, fence_clock(world, cat, data, e, start));
        }
    }

    /// A worker mended the fence piece `id` and its start time `start` was drawn in the step
    /// (sprite drawing 3.6 item 5): its animation starts there, so the next register draws nothing.
    pub(crate) fn start_fence(
        &mut self,
        world: &World,
        cat: &Catalogue,
        data: &dyn Data,
        (id, start): (u32, u32),
    ) {
        if let Some(e) = world.find(id).map(|at| &world.zoo.entities[at]) {
            self.fixed
                .insert(id, fence_clock(world, cat, data, e, start));
        }
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
        for (id, t, times) in self.later.drain(..) {
            if let Some(c) = self.fixed.get_mut(&id) {
                // A play that comes while another is queued adds to it.
                let waiting = c.queued.filter(|q| q.0 == "used").map_or(0, |q| q.2);
                let times = times.saturating_add(waiting);
                c.queued = (times > 0).then_some(("used", t, times));
            }
        }
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

    /// A catalogue with the fence type `iron`/`f` whose `cDecayedLife` is 2.
    fn iron() -> Catalogue {
        let mut cat = Catalogue::default();
        cat.push(crate::entity::EntityType {
            class: "fences",
            type_name: b"iron".to_vec(),
            subtype: b"f".to_vec(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(b"[f/Characteristics/Integers]\ncDecayedLife=2\n")
                    .unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        cat
    }

    /// A world with one `iron` fence (id 1) of the given life, on flat ground.
    fn worn(life: i32) -> World {
        world_with((4, 4), |zoo| {
            let mut piece = entity("fences", "iron", 1, (32, 0), 0);
            piece.subtype = "f".into();
            piece.common.as_mut().unwrap().fence.as_mut().unwrap().life = life;
            zoo.entities.push(piece);
        })
    }

    /// `idle` of 20 frames of 100 ms, `det` of 10 frames of 50 ms, `broke` of 4 frames of 25 ms.
    fn rows() -> Fixed {
        let t = |count, delay| Timing { count, delay };
        Fixed(
            Some(t(20, 100)),
            vec![("det", t(10, 50)), ("broke", t(4, 25))],
        )
    }

    /// A worn fence plays the animation its life chooses, not `idle`'s timing; when the picture
    /// changes the clock starts again at a new random time, one draw (sprite drawing 3.6).
    #[test]
    fn a_worn_fence_follows_its_own_animation() {
        let (cat, data) = (iron(), rows());
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        let mut expect = Rng::default();
        let start = |idles: &mut Idles, life: i32, rng: &mut Rng| {
            idles.register(&worn(life), &cat, &data, rng);
            idles.fixed[&1].t
        };
        for (life, count) in [(5, 20), (2, 10), (0, 4)] {
            let t = start(&mut idles, life, &mut rng);
            assert_eq!(t, expect.pick(2000), "life {life}");
            assert_eq!(idles.fixed[&1].timing.count, count, "life {life}");
            // The same picture again: no new draw.
            start(&mut idles, life, &mut rng);
            assert_eq!(rng.state(), expect.state());
        }
        // A gate keeps `idle` whatever its life.
        let mut gate = worn(0);
        gate.zoo.entities[0].subtype = "g".into();
        let mut none = Idles::default();
        none.register(&gate, &cat, &data, &mut rng);
        assert!(none.frames().is_empty());
    }

    /// A wear step rewinds the clock in place: one draw, three for a break, the last one is the
    /// start time.
    #[test]
    fn the_wear_step_of_a_fence_draws_again() {
        let (cat, data) = (iron(), rows());
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        idles.register(&worn(5), &cat, &data, &mut rng);
        for (life, draws, count) in [(2, 1, 10), (0, 3, 4)] {
            let before = rng.clone();
            idles.rewind_fence(&worn(life), &cat, &data, &mut rng, (0, draws));
            let mut expect = before;
            let starts: Vec<u32> = (0..draws).map(|_| expect.pick(2000)).collect();
            assert_eq!(rng.state(), expect.state(), "life {life}");
            assert_eq!(idles.fixed[&1].t, starts[draws as usize - 1]);
            assert_eq!(idles.fixed[&1].timing.count, count);
            // The register of the next step finds the picture as it is: no draw.
            idles.register(&worn(life), &cat, &data, &mut rng);
            assert_eq!(rng.state(), expect.state());
        }
    }

    /// A mended piece starts at the time its worker drew; the next register draws nothing.
    #[test]
    fn a_mended_fence_starts_where_it_drew() {
        let (cat, data) = (iron(), rows());
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        idles.register(&worn(0), &cat, &data, &mut rng);
        let before = rng.state();
        idles.start_fence(&worn(5), &cat, &data, (1, 1234));
        assert_eq!(
            (idles.fixed[&1].t, idles.fixed[&1].timing.count),
            (1234, 20)
        );
        idles.register(&worn(5), &cat, &data, &mut rng);
        assert_eq!(rng.state(), before);
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
        idles.queue_used(&w, 0, 1, &Building);
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
            idles.queue_used(&before, 0, 2, &Building);
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
