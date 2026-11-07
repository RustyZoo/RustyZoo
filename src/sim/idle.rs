//! The looped animations of the fixed objects, the fences and the flights (sprite drawing 3.6 and
//! 6.2, ambients 5.1): scenery, buildings, flying creatures and the like play the animation they
//! rest on forward in a loop, one clock each, from the step after the entity appears; the bubbles
//! of a tank filter have a clock of their own (tank staff 7.7). A loop that ends drops the time left over, so it takes a little more than
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
use crate::behaviour::{Data, Heard, Timing};
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
    /// The use sound the queued `used` sequences carry (guest buildings 10.1 point 2.4), and the one
    /// of the sequence that just became the playing one, for the building to start.
    sound: Option<Heard>,
    begun: Option<Heard>,
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
            sound: None,
            begun: None,
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
            // The sequence carries the sound: it starts with it, once.
            self.begun = self.sound.clone();
            if self.queued.is_none() {
                self.sound = None;
            }
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
                self.sound = None;
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
    /// The start times drawn for fence pieces outside the entity loop, taken by the next
    /// [`Idles::register`].
    pub(super) starts: BTreeMap<u32, u32>,
    /// The `used` queued by the check of this step (entity id, timing, times): it joins the queue
    /// after the clocks advance, so it cannot start before the next step.
    later: Vec<(u32, Timing, u32, Option<Heard>)>,
    /// The use sounds of the sequences that began in this step (entity id).
    heard: Vec<(u32, Heard)>,
}

impl Idles {
    /// A new game starts no clock.
    pub(crate) fn clear(&mut self) {
        self.fixed.clear();
        self.bubbles.clear();
        self.starts.clear();
        self.later.clear();
        self.heard.clear();
    }

    /// The use sounds the `used` sequences that began since the last call start at their building.
    pub(crate) fn take_heard(&mut self) -> Vec<(u32, Heard)> {
        std::mem::take(&mut self.heard)
    }

    /// The start time of the fence piece `id` was drawn outside the entity loop (a heal or a
    /// break, sprite drawing 3.6 items 4 and 5): its animation starts there at the next register,
    /// whether or not its picture changed.
    pub(crate) fn start_next(&mut self, id: u32, start: u32) {
        self.starts.insert(id, start);
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
    /// the check. Each sequence carries the use sound `heard`, started when it begins. A building
    /// with no clock or no `used` plays nothing.
    pub(crate) fn queue_used(
        &mut self,
        world: &World,
        at: usize,
        times: u32,
        heard: Option<Heard>,
        data: &dyn Data,
    ) {
        let e = &world.zoo.entities[at];
        let id = e.common.as_ref().map_or(0, |c| c.id);
        let timing = data
            .timing(e, "used")
            .filter(|_| self.fixed.contains_key(&id));
        self.later.extend(timing.map(|t| (id, t, times, heard)));
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
            if !matches!(kind, Kind::Fixed | Kind::Edge | Kind::Flight) {
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
                if let Some(start) = self.starts.remove(&c.id) {
                    self.fixed
                        .insert(c.id, fence_clock(world, cat, data, e, start));
                } else if self.fixed.get(&c.id).is_none_or(|k| k.state != state) {
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
        self.starts.clear();
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
        for (id, c) in &mut self.fixed {
            self.heard.extend(c.begun.take().map(|h| (*id, h)));
        }
        for (id, t, times, heard) in self.later.drain(..) {
            if let Some(c) = self.fixed.get_mut(&id) {
                if times > 0 {
                    c.sound = heard.or(c.sound.take());
                }
                // A play that comes while another is queued adds to it.
                let waiting = c.queued.filter(|q| q.0 == "used").map_or(0, |q| q.2);
                let times = times.saturating_add(waiting);
                c.queued = (times > 0).then_some(("used", t, times));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
