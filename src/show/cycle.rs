//! The show cycle of one show info (shows 5): the announce, the start, the items, the stop and
//! the abort. It posts events (shows 6.1) and asks messages; its only draws are the toy
//! validations of 23.6 and the tricks it asks the animals to start.

mod validate;

use super::{KEEPER, event, performer};
use crate::behaviour::{Data, NoData};
use crate::entity::Catalogue;
use crate::place::bulldozer::type_of;
use crate::sim::{Event, Events, KEEPER_CHANNEL, Rng, SHOW_CHANNEL, UNIT_CHANNEL, WORLD_CHANNEL};
use crate::world::World;
use crate::zoo::{Item, ItemState, ShowInfo};

/// The messages of a show, with the tank's name in them: about to start, started, ended.
const SOON: i32 = 11490;
const STARTED: i32 = 11491;
const ENDED: i32 = 11492;

/// One show info taken out of the world for its turn; the world's list holds none for it until
/// the turn ends, so another show is found there and this one is `info`.
struct Cycle<'a> {
    world: &'a mut World,
    cat: &'a Catalogue,
    events: &'a mut Events,
    /// The generator and the sets' data, for the tricks the animals are asked to start.
    rng: &'a mut Rng,
    data: &'a dyn Data,
    /// The AI clock.
    now: u32,
    info: ShowInfo,
    /// The show tank.
    exhibit: Option<u32>,
}

/// The turn of the show info of habitat record `n` (shows 5.1): `upkeep` is the sum of the upkeep
/// of the grandstands that face its tank.
pub(super) fn update(
    world: &mut World,
    cat: &Catalogue,
    (events, rng, data): (&mut Events, &mut Rng, &dyn Data),
    now: u32,
    n: usize,
    upkeep: f32,
) {
    with(world, cat, (events, rng, data), now, n, |c| {
        c.listen();
        c.update();
        c.info.events.clear();
        let [this, _, total] = &mut c.info.upkeep;
        let sum = f32::from_bits(*total) - f32::from_bits(*this);
        (*this, *total) = (upkeep.to_bits(), (sum + upkeep).to_bits());
    });
}

/// `abort()` of the show whose show id is `id`, if it has an info (shows 5.6).
pub(super) fn abort(world: &mut World, cat: &Catalogue, events: &mut Events, id: u16) {
    let now = world.units.ai_ticks;
    if let Some(n) = super::habitat_of(world, id) {
        // An abort starts no trick: it draws nothing and needs no sets.
        let (rng, data) = (&mut Rng::default(), &NoData);
        with(world, cat, (events, rng, data), now, n, |c| c.abort());
    }
}

fn with(
    world: &mut World,
    cat: &Catalogue,
    (events, rng, data): (&mut Events, &mut Rng, &dyn Data),
    now: u32,
    n: usize,
    f: impl FnOnce(&mut Cycle),
) {
    let Some(info) = world.zoo.shows.infos.get_mut(n).and_then(Option::take) else {
        return;
    };
    let exhibit = super::tank_of(world, n);
    let mut cycle = Cycle {
        world,
        cat,
        events,
        rng,
        data,
        now,
        info,
        exhibit,
    };
    f(&mut cycle);
    let info = cycle.info;
    world.zoo.shows.infos[n] = Some(info);
}

/// The animal moves to its next item: the index goes up and the five flags of the item go down.
fn next_item(st: &mut ItemState) {
    st.item = st.item.wrapping_add(1);
    (st.finished, st.aborted, st.cue_sent) = (false, false, false);
    (st.ready, st.started) = (false, false);
}

impl Cycle<'_> {
    fn id(&self) -> u16 {
        self.info.id
    }

    /// Posts an event of the show info with the delay of every show post, 1.
    fn post(&mut self, kind: u16, target: u32, channel: u8, data: u32, extra: u16) {
        self.events.post(Event {
            sender: self.id().into(),
            target,
            data,
            extra,
            kind,
            delay: 1,
            sender_channel: SHOW_CHANNEL,
            channel,
        });
    }

    /// Asks message `message` with the tank's name in it.
    fn tell(&mut self, message: i32) {
        let Some(x) = self.exhibit.and_then(|x| self.world.exhibits().get(x)) else {
            return;
        };
        let name = x.name.clone();
        self.world.units.post_named(message, 0, vec![name]);
    }

    fn tank_dry(&self) -> bool {
        let tank = (self.exhibit).and_then(|x| self.world.exhibits().get(x)?.tank);
        tank.is_some_and(|t| t.level == 0)
    }

    fn units(&self, t: u32) -> usize {
        self.info.units(t).len()
    }

    fn has_keeper(&self) -> bool {
        self.units(KEEPER) > 0
    }

    /// 9552 when `species` is an animal type, else 0 (shows 3.1).
    fn keeper_type(&self, species: u32) -> u32 {
        let animal = (self.cat.types.iter())
            .any(|t| t.class == "animals" && u32::try_from(t.name_id) == Ok(species));
        if animal { KEEPER } else { 0 }
    }

    /// The first item with trick id `id` in the show's current script (shows 23.2).
    fn first_item(&self, id: u16) -> Option<Item> {
        let script = self.info.show.script;
        let scripts = self.world.zoo.shows.scripts.iter();
        (scripts.filter(|s| s.id == script).flat_map(|s| &s.items))
            .find(|i| i.id == id)
            .cloned()
    }

    /// The number of items of script `id`, 0 for a script that is missing.
    fn items(&self, id: u16) -> usize {
        (self.world.zoo.shows.scripts.iter())
            .find(|s| s.id == id)
            .map_or(0, |s| s.items.len())
    }

    /// The unit `u` is in the world's unit table.
    fn is_unit(&self, u: u32) -> bool {
        self.world.units.get(u).is_some()
    }

    /// The type id of entity `u`.
    fn type_id(&self, u: u32) -> Option<u32> {
        let e = &self.world.zoo.entities[self.world.find(u)?];
        u32::try_from(type_of(self.cat, e)?.name_id).ok()
    }

    /// Reads show `id`: this one or another.
    fn show<T>(&self, id: u16, f: impl FnOnce(&ShowInfo) -> T) -> Option<T> {
        if id == self.id() {
            return Some(f(&self.info));
        }
        let other = (self.world.zoo.shows.infos.iter().flatten()).find(|i| i.id == id);
        other.map(f)
    }

    /// Whether `u` is in a show (shows 18.2).
    fn in_show(&self, u: u32) -> bool {
        let Some(unit) = self.world.units.get(u) else {
            return false;
        };
        let held = |i: &ShowInfo| i.show.items.iter().any(|st| st.animal == u);
        super::in_show(self.world, self.cat, unit, |show| {
            self.show(show, held).unwrap_or(false)
        })
    }

    // ---- Reading the show info's own events (shows 5.7)

    fn listen(&mut self) {
        let id = u32::from(self.id());
        let heard: Vec<u16> = (self.events.listen(id, SHOW_CHANNEL))
            .map(|e| e.kind)
            .collect();
        for kind in heard {
            match kind {
                event::STOP => self.stop(),
                event::ABORT => self.abort(),
                _ => {}
            }
        }
    }

    // ---- The show's update (shows 5.2)

    /// The show's update; the code is not used.
    fn update(&mut self) -> i32 {
        let s = self.info.scheduled_script();
        if s == 0 {
            return self.end(6);
        }
        let show = &self.info.show;
        if show.running && show.script == s {
            if show.stopped {
                self.stop();
                return 0;
            }
            return match self.run() {
                0 => 0,
                r => self.end(r),
            };
        }
        if self.tank_dry() {
            return self.end(10);
        }
        let species = self.info.scheduled().unwrap_or(0);
        if self.info.show.ready && self.units(species) < 1 {
            self.abort();
            self.info.show.ready = false;
            return self.end(1);
        }
        if self.info.frequency != -1 && i64::from(self.info.next) <= i64::from(self.now) {
            if !self.has_keeper() {
                self.recalc(false);
                return self.end(9);
            }
            if self.items(s) == 0 {
                self.recalc(true);
                return self.end(6);
            }
            return match self.start() {
                0 => 0,
                r => {
                    self.stop();
                    self.end(r)
                }
            };
        }
        if self.items(s) == 0 {
            return self.end(6);
        }
        if self.soon() {
            if self.units(species) < 1 {
                self.recalc(true);
                return self.end(1);
            }
            let told = self.info.show.ready;
            self.info.show.ready = true;
            self.post(event::ANNOUNCE, 0, WORLD_CHANNEL, 0, 1);
            self.info.show.species = species;
            self.info.show.keeper = self.keeper_type(species);
            // The announce is posted at every step of a qualifying AI tick; the message line shows
            // the same message once.
            if !told && self.has_keeper() {
                self.tell(SOON);
            }
        }
        0
    }

    /// The show is within the announce window: at most a minute of AI ticks before the start, at
    /// a multiple of 5 (shows 5.2 `about_to_start`).
    fn soon(&self) -> bool {
        let freq = self.info.frequency;
        if freq == -1 {
            return false;
        }
        let window = i64::from(if freq <= 59 { freq } else { 60 });
        let left = i64::from(self.info.next) - i64::from(self.now);
        left > 0 && left <= window && left % 5 == 0
    }

    /// Aborts a running show and gives the code back.
    fn end(&mut self, code: i32) -> i32 {
        if self.info.show.running {
            self.abort();
        }
        code
    }

    /// The next show time, and with `advance` the next species of the schedule.
    fn recalc(&mut self, advance: bool) {
        let len = self.info.schedule.len() as u32;
        if advance && len > 0 {
            self.info.index = (self.info.index.wrapping_add(1)) % len;
        }
        self.info.next = match self.info.frequency {
            -1 => -1,
            f => (self.now as i32).wrapping_add(f),
        };
    }

    // ---- Start (shows 5.3)

    fn start(&mut self) -> i32 {
        let s = self.info.scheduled_script();
        let sp = (self.world.zoo.shows.scripts.iter())
            .find(|x| x.id == s)
            .map_or(0, |x| x.kind);
        if sp == 0 {
            return 4;
        }
        if self.units(sp) < 1 {
            return 1;
        }
        if self.tank_dry() {
            return 10;
        }
        self.info.show.species = sp;
        self.info.show.keeper = self.keeper_type(sp);
        self.set_script(s);
        if self.validate() != 0 {
            return 2;
        }
        self.info.show.items.clear();
        let id = self.id();
        for u in self.info.units(sp).to_vec() {
            if !self.is_unit(u) {
                if let Some(r) = self.info.records.iter_mut().find(|r| r.key == sp) {
                    r.units.retain(|&x| x != u);
                }
                continue;
            }
            let shown = self.world.units.get(u).map_or(0, |x| x.show);
            let idle = self.show(shown, |i| !i.is_started());
            if idle == Some(true) || !self.in_show(u) {
                self.info.show.items.push(ItemState {
                    script: self.info.show.script,
                    animal: u,
                    item: -1,
                    finished: false,
                    aborted: false,
                    cue_sent: false,
                    ready: false,
                    started: false,
                    done: false,
                });
                if let Some(x) = self.world.units.get_mut(u) {
                    x.show = id;
                }
            }
        }
        if self.info.units(sp).is_empty() {
            return 1;
        }
        let now = self.now;
        let show = &mut self.info.show;
        (show.ready, show.running, show.stopped, show.start) = (false, true, false, now);
        self.post(event::STARTED, 0, WORLD_CHANNEL, 0, 0);
        self.tell(STARTED);
        0
    }

    /// Makes script `s` the current one (shows 5.3 `set_script`).
    fn set_script(&mut self, s: u16) {
        if self.info.show.script == s {
            return;
        }
        if self.info.show.script != 0 {
            self.reinit();
        }
        let kind = (self.world.zoo.shows.scripts.iter())
            .find(|x| x.id == s)
            .map(|x| x.kind);
        if let Some(species) = kind {
            self.info.show.script = s;
            self.info.show.species = species;
            self.info.show.keeper = self.keeper_type(species);
        }
        self.recalc(false);
    }

    /// The units of the species are all units of its type, and the script keeps the items they can
    /// do (shows 5.3 `validate`).
    fn validate(&mut self) -> i32 {
        let sp = self.info.show.species;
        let list = self.info.units(sp).to_vec();
        if list.is_empty() {
            return 1;
        }
        if list
            .iter()
            .any(|&u| !self.is_unit(u) || self.type_id(u) != Some(sp))
        {
            return 4;
        }
        let script = self.info.show.script;
        // An item that fails goes at once and the next one is tested at the same index.
        let mut i = 0;
        while i < self.items(script) {
            if self.validate_item(i as i16) == 0 {
                i += 1;
            } else if let Some(x) =
                (self.world.zoo.shows.scripts.iter_mut()).find(|x| x.id == script)
            {
                x.items.remove(i);
            }
        }
        if self.items(script) == 0 { 6 } else { 0 }
    }

    // ---- Running the items (shows 5.4)

    fn run(&mut self) -> i32 {
        if self.tank_dry() {
            return 10;
        }
        let species = self.info.show.species;
        if self.units(species) < 1 {
            return 1;
        }
        if self.units(self.info.show.keeper) < 1 {
            return 9;
        }
        let count = self.items(self.info.show.script);
        if count == 0 {
            return 6;
        }
        let list = self.info.units(species).to_vec();
        let (mut all_done, mut all_ready) = (true, true);
        for &u in &list {
            if self.world.find(u).is_none() {
                return 4;
            }
            let Some(st) = self.info.show.items.iter().find(|s| s.animal == u) else {
                continue;
            };
            if !st.done {
                all_done = false;
                if st.item == -1 && (!all_ready || !st.ready) {
                    all_ready = false;
                }
            }
        }
        if all_done {
            self.stop();
            return 0;
        }
        let id = self.id();
        let ours = |c: &Cycle, u: u32| c.world.units.get(u).is_some_and(|x| x.show == id);
        if !list.iter().any(|&u| ours(self, u)) {
            return 4;
        }
        for &u in &list {
            let Some(at) = self.info.show.items.iter().position(|s| s.animal == u) else {
                continue;
            };
            let mut st = self.info.show.items[at].clone();
            self.turn(u, &mut st, count, all_ready);
            self.info.show.items[at] = st;
        }
        // The code of the loop is 0 at its end: an item that fails is aborted on the spot.
        0
    }

    /// One animal's turn in a run step.
    fn turn(&mut self, u: u32, st: &mut ItemState, count: usize, all_ready: bool) {
        if st.done {
            return;
        }
        if st.item != -1 && (st.finished || st.aborted) {
            if !st.aborted {
                self.trick_event(st);
            }
            if i64::from(st.item) + 1 >= count as i64 {
                st.done = true;
                return;
            }
            self.next_item_posts(u);
            next_item(st);
            // The flags were just cleared: the cue is always posted.
            if !st.aborted {
                self.post(event::NEXT_ITEM, 0, KEEPER_CHANNEL, 0, st.item as u16);
            }
        }
        if !self.is_unit(u) {
            return;
        }
        if st.item == -1 && all_ready {
            next_item(st);
            self.next_item_posts(u);
            if !st.aborted {
                self.post(event::NEXT_ITEM, 0, KEEPER_CHANNEL, 0, st.item as u16);
            }
        }
        performer::read_events(self.world, self.cat, self.events, u, st);
        let r = self.run_current_item(u, st, count);
        if r != 0 {
            self.abort_item(u, st);
        }
    }

    /// Tells everyone and the animal that its next item begins.
    fn next_item_posts(&mut self, u: u32) {
        self.post(event::NEXT_ITEM, 0, WORLD_CHANNEL, 0, 0);
        self.post(event::NEXT_ITEM, u, UNIT_CHANNEL, 0, 0);
    }

    /// The cues of the current item of `u` and, once it is ready, the start of its trick (shows
    /// 5.4 `run_current_item`); 0 or the code that aborts the item.
    fn run_current_item(&mut self, u: u32, st: &mut ItemState, count: usize) -> i32 {
        let ready;
        if st.finished {
            ready = true;
        } else if st.cue_sent {
            ready = st.ready;
        } else {
            ready = st.ready;
            let r = self.validate_item(st.item);
            if r != 0 {
                self.abort_item(u, st);
                return r;
            }
            if !ready {
                if st.item == -1 {
                    self.post(event::CALL, u, UNIT_CHANNEL, 0, 0);
                } else {
                    self.post(event::PREPARE, u, UNIT_CHANNEL, 0, 0);
                    self.post(event::CUE, 0, WORLD_CHANNEL, 0, 0);
                    self.post(event::CUE, u, UNIT_CHANNEL, 0, 0);
                    self.post(event::CUE, self.id().into(), SHOW_CHANNEL, 0, 0);
                }
                st.cue_sent = true;
                return 0;
            }
        }
        if !ready || st.finished || st.started {
            return 0;
        }
        if !self.is_unit(u) || usize::from(st.item as u16) >= count {
            return -1;
        }
        // The item is the first with the trick id of the script's item (shows 23.2).
        let script = self.info.show.script;
        let item = (self.world.zoo.shows.scripts.iter()).find(|s| s.id == script);
        let item = item.and_then(|s| s.items.get(usize::from(st.item as u16)));
        let item = item.and_then(|i| self.first_item(i.id));
        let Some(item) = item else {
            return 3;
        };
        let (world, cat, show) = (&mut *self.world, self.cat, self.info.id);
        let r = crate::goal::start_trick(world, cat, self.rng, self.data, u, (&item, show));
        if r == 0 {
            st.started = true;
        }
        r
    }

    /// The item of `u` ends aborted (shows 5.4 `abort_item`). A refused trick (8) asks a trick
    /// event with no state, which does nothing.
    fn abort_item(&mut self, u: u32, st: &mut ItemState) {
        (st.finished, st.aborted) = (true, true);
        self.post(event::ITEM_ABORTED, 0, WORLD_CHANNEL, 0, 0);
        self.post(event::ITEM_ABORTED, u, UNIT_CHANNEL, 0, 0);
    }

    /// A finished item that was not aborted is scored and told as a reaction (shows 8.1).
    fn trick_event(&mut self, st: &ItemState) {
        let script = self.info.show.script;
        let item = (self.world.zoo.shows.scripts.iter())
            .find(|s| s.id == script)
            .and_then(|s| s.items.get(usize::try_from(st.item).ok()?));
        let Some((kind, satisfaction, v)) = item.map(|i| (i.marker, i.satisfaction, i.current))
        else {
            return;
        };
        if kind == 3 {
            return;
        }
        let show = &mut self.info.show;
        show.possible = show.possible.wrapping_add(satisfaction as u32);
        show.done += 1;
        show.achieved = show.achieved.wrapping_add(v as u32);
        let [bad, good, great] = self.world.units.show.trick;
        let (kind, extra) = if v <= bad {
            (event::BAD, bad.wrapping_sub(v))
        } else if v >= great {
            (event::GREAT, v.wrapping_sub(great))
        } else if v >= good {
            (event::GOOD, v.wrapping_sub(good))
        } else {
            (event::NEUTRAL, 0)
        };
        self.post(kind, 0, WORLD_CHANNEL, v as u32, extra as u16);
        self.post(event::TRICK_DONE, 0, KEEPER_CHANNEL, 0, st.item as u16);
    }

    // ---- Stop and abort (shows 5.6)

    /// The show ends: the end and its figures are told, the show is reset and the next species is
    /// scheduled.
    fn stop(&mut self) {
        if self.info.show.running {
            self.tell(ENDED);
        }
        let (percent, bonus) = self.figures();
        self.post(event::ENDED, 0, WORLD_CHANNEL, percent, bonus);
        self.reinit();
        self.recalc(true);
        self.apply_pending();
    }

    fn abort(&mut self) {
        self.post(event::ABORTED, 0, WORLD_CHANNEL, 0, 0);
        self.stop();
    }

    /// The show goes back to its idle state; its animals are released.
    fn reinit(&mut self) {
        let id = self.id();
        for u in self.info.units(self.info.show.species).to_vec() {
            if self.world.units.get(u).is_some_and(|x| x.show == id) {
                performer::release(self.world, self.cat, u);
            }
        }
        let show = &mut self.info.show;
        show.species = 0;
        show.items.clear();
        (show.ready, show.running, show.stopped) = (false, false, true);
        (show.state_word, show.start, show.done) = (0, 0, 0);
        (show.possible, show.achieved) = (0, 0);
    }

    /// The satisfaction percent and the rest bonus the end sends (shows 8.2).
    fn figures(&self) -> (u32, u16) {
        let show = &self.info.show;
        let n = self.items(show.script) as i64;
        let k = i64::from(show.done);
        let [min, max] = self.world.units.show.ideal.map(i64::from);
        let penalty = match (n > 0 && k <= n, k > max, k < min) {
            (true, true, _) => k - max,
            (true, _, true) => min - k,
            _ => 0,
        };
        let f = if show.possible == 0 || show.achieved == 0 {
            0.0
        } else if show.achieved < show.possible {
            f64::from(show.achieved) / f64::from(show.possible as i32)
        } else {
            1.0
        };
        let p = i64::from((f * 100.0) as i32);
        let percent = if penalty >= 1 {
            (p - penalty).max(0)
        } else {
            p
        };
        let bonus = if penalty < 100 { 100 - penalty } else { 100 };
        (percent as u32, bonus as u16)
    }

    /// The scripts the editor chose take the place of the old ones when a show stops.
    fn apply_pending(&mut self) {
        for at in 0..self.info.records.len() {
            let record = &self.info.records[at];
            let (key, old, new) = (record.key, record.script, record.pending);
            if new == u16::MAX {
                continue;
            }
            self.info.records[at].pending = u16::MAX;
            if new == old {
                continue;
            }
            self.world.zoo.shows.scripts.retain(|s| s.id != old);
            self.info.records[at].script = new;
            let filled = self.items(new) > 0;
            let schedule = &mut self.info.schedule;
            if filled {
                if !schedule.contains(&key) {
                    schedule.push(key);
                }
            } else if schedule.contains(&key) {
                schedule.retain(|&t| t != key);
                if self.info.index as usize >= self.info.schedule.len() {
                    self.info.index = 0;
                }
            }
        }
    }
}
