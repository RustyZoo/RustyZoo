//! Running the sets: the animation a unit holds (the player of sprite drawing 6) and the runner
//! that executes a set call by call (behaviours 4 to 6).
//!
//! A unit holds an [`Animation`] and a [`Runner`]. Each step the unit's turn advances the
//! animation by the step, and later runs the set ([`Runner::step`]): the current call executes,
//! a call that is done lets the next one start in the same step, a nested set runs in place of
//! its parent's current call, and a failed call ends the whole chain. What a set needs of the
//! data comes through [`Data`].
//!
//! Until the move and animal issues land, the calls that belong to them (moves, the faces,
//! the effects on other entities, `fDie`, ...) are done at once with no effect; a routine call
//! is done when the play state is ready and the unit's goals (which run every step) do the rest;
//! a nested terrain or tank set is the land one.

use std::collections::VecDeque;

use super::{Call, Number, Op, Play, Routine, Row};
use crate::sim::Rng;
use crate::zoo::Entity;

/// The longest nesting of sets (behaviours O8): the original has no limit.
const DEPTH: usize = 16;
/// The most entries one play call queues: `fPlayTime` repeats its animation to cover the time.
const ENTRIES: i64 = 10_000;

/// The frames of an animation and the time each lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub count: u32,
    pub delay: u32,
}

/// What the runner reads of the data, for the unit of an entity.
pub trait Data {
    /// The set `name` the unit starts: its calls, if it has any (behaviours 3.1).
    fn set(&self, unit: &Entity, name: &str) -> Option<&[Call]>;
    /// The rows of the unit's ambient table.
    fn ambient(&self, unit: &Entity) -> &[Row];
    /// The frames and delay of one of the unit's animations; `None` if it has none of that name.
    fn timing(&self, unit: &Entity, animation: &str) -> Option<Timing>;
    /// The value of a number of a call (behaviours 5.8).
    fn number(&self, unit: &Entity, number: &Number) -> i32;
    /// The sound the unit's `[Sounds]` section names with this key.
    fn sound(&self, unit: &Entity, key: &str) -> Option<Heard>;
}

/// No data: no set, no animation.
pub struct NoData;

impl Data for NoData {
    fn set(&self, _: &Entity, _: &str) -> Option<&[Call]> {
        None
    }
    fn ambient(&self, _: &Entity) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &Entity, _: &str) -> Option<Timing> {
        None
    }
    fn number(&self, _: &Entity, number: &Number) -> i32 {
        match number {
            Number::Int(n) => *n,
            Number::Characteristic(_) => 0,
        }
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<Heard> {
        None
    }
}

/// A sound of a `[Sounds]` key: the member of the file (behaviours 5.2), the base term `B` (the
/// second value of the key, sound 4.4) and the name of its voice: each key of each type has one
/// (sound 9.1).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Heard {
    pub file: String,
    pub base: i32,
    pub voice: String,
}

/// What happened that the log and the app are told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// A top set was started.
    Set(String),
    /// The animation shown changed.
    Anim(String),
    /// An entry with a sound began: the key in the unit's `[Sounds]`.
    Sound(String),
}

/// One animation of a sequence, played forward or reversed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    name: String,
    reverse: bool,
    sound: Option<String>,
}

impl Entry {
    fn new(name: &str, reverse: bool) -> Entry {
        Entry {
            name: name.to_string(),
            reverse,
            sound: None,
        }
    }
}

/// The animation a unit holds: the sequence being played, the sequences queued, the base
/// animation that loops when nothing else plays, and the frame (sprite drawing 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Animation {
    /// The animation shown.
    pub name: String,
    /// The frame counted up from 0; [`Animation::shown`] is the one drawn.
    pub frame: u32,
    /// Milliseconds the frame has been shown.
    pub elapsed: u32,
    reverse: bool,
    /// The frames of the animation shown; 0 if it has none.
    count: u32,
    base: String,
    /// The sequence being played; empty while the base animation shows.
    seq: Vec<Entry>,
    /// The unit's stop-at-end flag: whatever sequence is current holds its last frame when it
    /// ends, for good (behaviours 5.2).
    stop: bool,
    stopped: bool,
    entry: usize,
    queue: VecDeque<Vec<Entry>>,
    /// The key of the sound of the current entry, started at the next advance.
    sound: Option<String>,
    /// What happened since the notes were last taken.
    pub notes: Vec<Note>,
}

impl Default for Animation {
    fn default() -> Self {
        Animation {
            name: String::new(),
            frame: 0,
            elapsed: 0,
            reverse: false,
            count: 0,
            base: "idle".to_string(),
            seq: Vec::new(),
            stop: false,
            stopped: false,
            entry: 0,
            queue: VecDeque::new(),
            sound: None,
            notes: Vec::new(),
        }
    }
}

impl Animation {
    /// The frame of the animation to draw: the counted one, or counted from the end when the
    /// entry is reversed (sprite drawing 6.3).
    pub fn shown(&self) -> u32 {
        match self.reverse {
            true => (self.count.saturating_sub(1)).saturating_sub(self.frame),
            false => self.frame,
        }
    }

    /// Whether the base animation shows, nothing else playing.
    pub fn on_base(&self) -> bool {
        self.seq.is_empty()
    }

    /// The animation of the current entry.
    fn current(&self) -> &str {
        self.seq.get(self.entry).map_or(&self.base, |e| &e.name)
    }

    /// Shows the current entry from its first frame: a new name is noted, its sound waits for the
    /// next advance.
    fn enter(&mut self, timing: &dyn Fn(&str) -> Option<Timing>) {
        let entry = self.seq.get(self.entry);
        let (name, reverse) =
            entry.map_or((self.base.clone(), false), |e| (e.name.clone(), e.reverse));
        self.sound = entry.and_then(|e| e.sound.clone());
        self.count = timing(&name).map_or(0, |t| t.count);
        self.reverse = reverse;
        if self.name != name {
            self.notes.push(Note::Anim(name.clone()));
            self.name = name;
        }
    }

    /// The sequence ends: the next queued one starts, else the base animation (sprite drawing
    /// 6.2).
    fn finish(&mut self, timing: &dyn Fn(&str) -> Option<Timing>) {
        self.seq = self.queue.pop_front().unwrap_or_default();
        (self.entry, self.frame, self.elapsed) = (0, 0, 0);
        self.enter(timing);
    }

    /// Advances by `dt` milliseconds (sprite drawing 6.2). A frame lasts the delay of its file;
    /// the time left when a sequence ends is dropped.
    pub fn advance(&mut self, dt: u32, timing: &dyn Fn(&str) -> Option<Timing>) {
        if self.stopped {
            return;
        }
        let Some(mut now) = timing(&self.name).filter(|_| self.name == self.current()) else {
            self.finish(timing);
            return;
        };
        if self.on_base() && now.count == 1 && self.queue.is_empty() {
            return;
        }
        if dt != 0 {
            self.notes.extend(self.sound.take().map(Note::Sound));
        }
        self.elapsed = self.elapsed.saturating_add(dt);
        let entries = self.seq.len().max(1);
        while now.delay > 0 && self.elapsed >= now.delay {
            self.elapsed -= now.delay;
            self.frame += 1;
            if self.frame < now.count {
                continue;
            }
            if self.stop {
                (self.stopped, self.elapsed, self.frame) = (true, 0, now.count.saturating_sub(1));
                return;
            }
            // The next entry starts, and whole animations the time covers are skipped.
            loop {
                self.entry += 1;
                if self.entry >= entries {
                    self.finish(timing);
                    return;
                }
                self.frame = 0;
                self.enter(timing);
                let Some(next) = timing(&self.name) else {
                    self.finish(timing);
                    return;
                };
                now = next;
                let whole = now.count.saturating_mul(now.delay);
                if self.elapsed <= whole {
                    break;
                }
                self.elapsed -= whole;
            }
        }
    }

    /// Drops the queued sequences and queues `seq` behind the one playing, the base loop
    /// included: the player takes it when that ends (behaviours 5.2).
    fn queue(&mut self, seq: Vec<Entry>) {
        self.queue.clear();
        self.queue.push_back(seq);
        self.base = "idle".to_string();
    }

    /// Clears the player and shows `name` from its first frame as the base (`fSetBase`).
    fn set_base(&mut self, name: &str, timing: &dyn Fn(&str) -> Option<Timing>) {
        self.reset();
        self.base = name.to_string();
        self.finish(timing);
    }

    /// Drops what is playing and queued; the base animation shows from the next advance.
    fn reset(&mut self) {
        self.seq = Vec::new();
        (self.entry, self.frame, self.elapsed) = (0, 0, 0);
        self.queue.clear();
        self.name.clear();
    }
}

/// One set of the chain being run.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Level {
    name: String,
    /// The set has calls.
    loaded: bool,
    running: bool,
    /// The index of the current call, and whether it has finished.
    call: usize,
    done: bool,
}

/// What a unit needs to run its set.
pub struct Env<'a> {
    pub data: &'a dyn Data,
    pub unit: &'a Entity,
    pub rng: &'a mut Rng,
}

/// The set a unit runs: the chain from the top set down to the innermost nested one, the set it
/// will start when the top one ends, and the play state the play calls share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runner {
    chain: Vec<Level>,
    pending: Option<String>,
    interruptible: bool,
    /// No play call is in progress; `playing`: one is.
    ready: bool,
    playing: bool,
}

impl Default for Runner {
    fn default() -> Self {
        Runner {
            chain: Vec::new(),
            pending: None,
            interruptible: true,
            ready: true,
            playing: false,
        }
    }
}

impl Runner {
    /// Whether the top set has a call in progress.
    pub fn running(&self) -> bool {
        self.chain.first().is_some_and(|l| l.running)
    }

    /// The name of the top set, if the unit has started one.
    pub fn top(&self) -> Option<&str> {
        self.chain.first().map(|l| l.name.as_str())
    }

    /// Asks for a set to start when the running one ends (behaviours 4.2).
    pub fn request(&mut self, name: &str) {
        self.pending = Some(name.to_string());
    }

    /// Starts the set `name` as the top set. A running set that is not interruptible refuses
    /// unless `force`. Unless `keep`, the animation drops what it plays and has queued
    /// (behaviours 4.2). Returns whether it started.
    pub fn start(
        &mut self,
        anim: &mut Animation,
        env: &Env,
        name: &str,
        interruptible: bool,
        force: bool,
        keep: bool,
    ) -> bool {
        if self.running() && !self.interruptible && !force {
            return false;
        }
        (self.ready, self.playing, self.interruptible) = (true, false, interruptible);
        self.chain.clear();
        self.nest(env, name);
        if !keep {
            anim.reset();
        }
        anim.notes.push(Note::Set(name.to_string()));
        true
    }

    fn nest(&mut self, env: &Env, name: &str) {
        self.chain.push(Level {
            name: name.to_string(),
            loaded: env.data.set(env.unit, name).is_some(),
            running: true,
            call: 0,
            done: false,
        });
    }

    /// The unit's behaviour hook (behaviours 4.6): a pending set starts, else an ambient one is
    /// chosen by `key`; then the top set executes.
    pub fn step(&mut self, anim: &mut Animation, env: &mut Env, key: i32) {
        if !self.running() {
            match self.pending.take() {
                Some(name) => {
                    self.start(anim, env, &name, true, false, true);
                }
                None => self.choose(anim, env, key),
            }
        }
        if self.running() {
            self.execute(0, anim, env);
        }
    }

    /// The ambient choice of the animals' checks: only when no set runs and none is pending.
    pub fn ambient(&mut self, anim: &mut Animation, env: &mut Env, key: i32) {
        if !self.running() && self.pending.is_none() {
            self.choose(anim, env, key);
        }
    }

    /// The first row of the ambient table that holds `key` starts (behaviours 6.1).
    fn choose(&mut self, anim: &mut Animation, env: &Env, key: i32) {
        let rows = env.data.ambient(env.unit);
        if let Some(row) = rows.iter().find(|r| r.lo <= key && key <= r.hi) {
            self.start(anim, env, &row.set, true, false, true);
        }
    }

    /// Executes the set at depth `k` (behaviours 4.3); returns whether it failed. A call that
    /// is done lets the next start in the same step.
    fn execute(&mut self, k: usize, anim: &mut Animation, env: &mut Env) -> bool {
        loop {
            let level = &mut self.chain[k];
            let calls = env.data.set(env.unit, &level.name).unwrap_or_default();
            if !level.loaded {
                level.running = false;
                return true;
            }
            if level.done {
                (level.call, level.done) = (level.call + 1, false);
            }
            let Some(call) = calls.get(level.call) else {
                level.running = false;
                return false;
            };
            let failed = if self.chain.len() > k + 1 {
                let failed = self.execute(k + 1, anim, env);
                if !self.chain[k + 1].running {
                    self.chain.truncate(k + 1);
                    self.chain[k].done = true;
                }
                failed
            } else {
                !self.run(k, call, anim, env)
            };
            let level = &mut self.chain[k];
            if failed {
                level.running = false;
                return true;
            }
            if !level.done {
                return false;
            }
        }
    }

    /// Runs the current call of the set at depth `k` once; returns whether it succeeded. It
    /// raises the level's `done` when the call has finished (behaviours 5).
    fn run(&mut self, k: usize, call: &Call, anim: &mut Animation, env: &mut Env) -> bool {
        let timing = |name: &str| env.data.timing(env.unit, name);
        let found = |name: &String| timing(name).is_some();
        let entries = match &call.op {
            Op::Play(how, names) => {
                let names = names.iter().filter(|n| found(n));
                let mut list = Vec::new();
                for name in names {
                    list.push(Entry::new(name, *how == Play::Reverse));
                    if *how == Play::PingPong {
                        list.push(Entry::new(name, true));
                    }
                }
                anim.stop |= *how == Play::StopAtEnd;
                list
            }
            Op::PlayWithSound { anim, sound } => {
                let entry = Entry {
                    sound: Some(sound.clone()),
                    ..Entry::new(anim, false)
                };
                vec![entry].into_iter().filter(|e| found(&e.name)).collect()
            }
            Op::PlayTime { anim, time } => {
                let millis = i64::from(env.data.number(env.unit, time)) * 1000;
                let times = match timing(anim) {
                    None => 0,
                    Some(t) if t.count.saturating_mul(t.delay) == 0 => 2,
                    Some(t) => {
                        let length = i64::from(t.count) * i64::from(t.delay);
                        (1 + (millis - 1).max(0) / length).min(ENTRIES)
                    }
                };
                vec![Entry::new(anim, false); times as usize]
            }
            Op::SetBase(name) => {
                if self.ready {
                    if !found(name) {
                        return false;
                    }
                    anim.set_base(name, &timing);
                    self.chain[k].done = true;
                }
                return true;
            }
            Op::PlaySet(name) => return self.nest_call(env, name),
            Op::PlaySetProb(rows) => {
                // One draw, and another after every row that is passed (behaviours 5.3).
                let mut draw = env.rng.pick(100) as i32 + 1;
                let mut left = 100i64;
                for (name, weight) in rows {
                    if draw <= *weight {
                        return self.nest_call(env, name);
                    }
                    left -= i64::from(*weight);
                    if 0 < left && left <= 100 {
                        draw = env.rng.pick(left as u32) as i32 + 1;
                    }
                }
                return false;
            }
            Op::PlaySetTerrain { land: name, .. } | Op::PlaySetTank { land: name, .. } => {
                return self.nest_call(env, name);
            }
            Op::Routine(routine) => {
                // The goals run every step anyway (goals 6.3): the call is done when the play
                // state is ready. A call for another class of unit does nothing; the keeper's
                // fails (behaviours 5.5).
                let own = match routine {
                    Routine::Keeper => "keeper",
                    Routine::Guest => "guests",
                    Routine::Guide => "tour",
                    Routine::Maint => "maint",
                };
                if env.unit.class != own {
                    return *routine != Routine::Keeper;
                }
                self.chain[k].done = self.ready;
                return true;
            }
            Op::KeepControl | Op::Spray | Op::FaceDirection | Op::TargetBuilding => {
                return true;
            }
            Op::Fail => return false,
            // The rest has its effect in its own issue and is done at once.
            _ => {
                self.chain[k].done = true;
                return true;
            }
        };
        // A play call: wait for the sequence in progress, else queue the entries. It is done as
        // soon as the player has taken them.
        if !self.ready {
            if self.playing && (anim.queue.is_empty() || anim.name.is_empty()) {
                (self.ready, self.playing) = (true, false);
                self.chain[k].done = true;
            }
            return true;
        }
        if entries.is_empty() {
            return false;
        }
        anim.queue(entries);
        (self.playing, self.ready) = (true, false);
        true
    }

    fn nest_call(&mut self, env: &Env, name: &str) -> bool {
        if self.chain.len() >= DEPTH {
            return false;
        }
        self.nest(env, name);
        true
    }
}

#[cfg(test)]
mod tests;
