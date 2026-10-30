//! Running the sets: the animation a unit holds (the player of sprite drawing 6) and the runner
//! that executes a set call by call (behaviours 4 to 6).
//!
//! A unit holds an [`Animation`] and a [`Runner`]. Each step the unit's turn advances the
//! animation by the step, and later runs the set ([`Runner::step`]): the current call executes,
//! a call that is done lets the next one start in the same step, a nested set runs in place of
//! its parent's current call, and a failed call ends the whole chain. What a set needs of the
//! data comes through [`Data`].
//!
//! A move call is the unit's business (movement 8.2): the runner stops at it and hands it out
//! ([`Runner::take_move`]), and the answer ([`Runner::answer`]) lets the set go on in the same
//! step ([`Runner::resume`]). Until the animal issues land, the other calls that belong to them
//! (the effects on other entities, ...) are done at once with no effect; a
//! routine call is done when the play state is ready and the unit's goals (which run every step)
//! do the rest; a nested terrain or tank set is the land one.

use std::collections::VecDeque;
use std::sync::Arc;

use super::{Call, Gait, Number, Op, Place, Play, Routine, Row, Target, parse_call, split_call};
use crate::sim::Rng;
use crate::zoo::{Behaviour as Saved, Entity};

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
    /// The rows of the unit's ambient table number `table` of [`super::TABLES`].
    fn ambient(&self, unit: &Entity, table: usize) -> &[Row];
    /// The frames and delay of one of the unit's animations; `None` if it has none of that name.
    fn timing(&self, unit: &Entity, animation: &str) -> Option<Timing>;
    /// The value of a number of a call (behaviours 5.8).
    fn number(&self, unit: &Entity, number: &Number) -> i32;
    /// The sound the unit's `[Sounds]` section names with this key.
    fn sound(&self, unit: &Entity, key: &str) -> Option<Heard>;
    /// A whole number of `[Characteristics/Integers]` of the unit class file `ztunit.ai`, shared by
    /// all units (pathfinding 6.5); `None` when the file or the key is missing.
    fn unit_class(&self, _key: &str) -> Option<i32> {
        None
    }
}

/// No data: no set, no animation.
pub struct NoData;

impl Data for NoData {
    fn set(&self, _: &Entity, _: &str) -> Option<&[Call]> {
        None
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[Row] {
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
    /// The cut (sprite drawing 6.2): at the next advance it ends the sequence playing, or the loop
    /// of another animation than the base, and is cleared; it stays while the base shows.
    cut: bool,
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
            cut: false,
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
    pub fn shows_base(&self) -> bool {
        self.seq.is_empty() && self.name == self.base
    }

    /// Makes `name` the base, without a cut and without clearing the player: what plays goes on
    /// to its end, the loop of the old base included (movement 8.2 step 1).
    pub fn base_to(&mut self, name: &str) {
        self.base = name.to_string();
    }

    /// Sets the cut (sprite drawing 6.2): a forced set start, the stop of a walk, the resets of
    /// the move and idle animations.
    pub fn cut(&mut self) {
        self.cut = true;
    }

    /// The animation of the current entry; with no sequence, the loop that was installed.
    fn current(&self) -> &str {
        self.seq.get(self.entry).map_or(&self.name, |e| &e.name)
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
        if self.cut && (!self.queue.is_empty() || !self.shows_base()) {
            self.cut = false;
            self.finish(timing);
        }
        let Some(mut now) = timing(&self.name).filter(|_| self.name == self.current()) else {
            self.finish(timing);
            return;
        };
        if self.shows_base() && now.count == 1 && self.queue.is_empty() {
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
    pub fn set_base(&mut self, name: &str, timing: &dyn Fn(&str) -> Option<Timing>) {
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
    /// Started by a forced start and not yet run: the first run cuts the animation.
    forced: bool,
    /// The calls of a set loaded from a saved game, the current one first, in place of the type's
    /// set of that name (behaviours 4.7); `call` counts in this list.
    own: Option<Arc<[Call]>>,
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
    /// Taken from a saved game and not yet stepped: the player holds no sequence of the unit's
    /// own, so its first play call is shown at once, not behind the base loop (behaviours 4.7).
    fresh: bool,
    /// Where the unit is for its ambient table: set by the unit before each run.
    pub place: Place,
    /// The unit's tank mode and whether its tile is a water tile, which alone choose the set of
    /// a nesting call (tank fauna 9.5): set by the unit before each run.
    pub tank_mode: u8,
    pub wet: bool,
    /// The guide's task is the talk and not done: its routine does not make idle the base
    /// (behaviours 5.5). Set by the unit before each run.
    pub talking: bool,
    /// The move call the last execution stopped at, for the unit to run.
    moving: Option<(Gait, Target)>,
    /// A `fFaceTowardFood` call ran, for the unit to turn.
    face: bool,
    /// A `fFaceTowardTarget` call ran, for a guest or staff member to turn to its own target.
    target: bool,
    /// A `fHalt` call ran, for the unit to stop moving.
    halt: bool,
    /// An `fDie` call ran, for the animal to be removed.
    die: bool,
    /// An `fHatch` call ran, for the egg to hatch.
    hatch: bool,
    /// An `fEnterBuilding` and an `fExitBuilding` call ran, for the building to take or release
    /// the unit (guest buildings 6.4.1).
    enter: bool,
    exit: bool,
}

/// The answer to a move call (movement 8.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// The call goes on in the next step.
    Running,
    Done,
    Failed,
}

impl Default for Runner {
    fn default() -> Self {
        Runner {
            chain: Vec::new(),
            pending: None,
            interruptible: true,
            ready: true,
            playing: false,
            fresh: false,
            place: Place::Land,
            tank_mode: 0,
            wet: false,
            talking: false,
            moving: None,
            face: false,
            target: false,
            halt: false,
            die: false,
            hatch: false,
            enter: false,
            exit: false,
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

    /// The move call the set reached, once.
    pub fn take_move(&mut self) -> Option<(Gait, Target)> {
        self.moving.take()
    }

    /// Whether a face-the-food call ran since the last time, once.
    pub fn take_face(&mut self) -> bool {
        std::mem::take(&mut self.face)
    }

    /// Whether a face-the-target call ran since the last time, once.
    pub fn take_target(&mut self) -> bool {
        std::mem::take(&mut self.target)
    }

    /// Whether a halt call ran since the last time, once.
    pub fn take_halt(&mut self) -> bool {
        std::mem::take(&mut self.halt)
    }

    /// Whether an `fDie` call ran since the last time, once.
    pub fn take_die(&mut self) -> bool {
        std::mem::take(&mut self.die)
    }

    /// Whether an `fHatch` call ran since the last time, once.
    pub fn take_hatch(&mut self) -> bool {
        std::mem::take(&mut self.hatch)
    }

    /// Whether an `fEnterBuilding` call ran since the last time, once.
    pub fn take_enter(&mut self) -> bool {
        std::mem::take(&mut self.enter)
    }

    /// Whether an `fExitBuilding` call ran since the last time, once.
    pub fn take_exit(&mut self) -> bool {
        std::mem::take(&mut self.exit)
    }

    /// The set asked to follow the current one, if any.
    pub fn queued(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    /// Forgets the set asked to follow the current one (animals-needs 5.4).
    pub fn forget(&mut self) {
        self.pending = None;
    }

    /// The unit's answer to the move call: it is done, or fails the set.
    pub fn answer(&mut self, flow: Flow) {
        let Some(level) = self.chain.last_mut() else {
            return;
        };
        match flow {
            Flow::Running => {}
            Flow::Done => level.done = true,
            Flow::Failed => level.loaded = false,
        }
    }

    /// Goes on with the set after an answer, without choosing a new one.
    pub fn resume(&mut self, anim: &mut Animation, env: &mut Env) {
        if self.running() {
            self.execute(0, anim, env);
        }
    }

    /// Asks for a set to start when the running one ends (behaviours 4.2).
    pub fn request(&mut self, name: &str) {
        self.pending = Some(name.to_string());
    }

    /// The calls the top set has left, the current one included: none when no set runs, and a
    /// nested set is not counted (behaviours 4.1).
    pub fn calls_left(&self, data: &dyn Data, unit: &Entity) -> usize {
        let Some(top) = self.chain.first().filter(|l| l.running && l.loaded) else {
            return 0;
        };
        let calls = match &top.own {
            Some(own) => own.len(),
            None => data.set(unit, &top.name).map_or(0, <[Call]>::len),
        };
        calls.saturating_sub(top.call)
    }

    /// Starts the set `name` as the top set. A running set that is not interruptible refuses
    /// unless `force`. Unless `keep`, the animation drops what it plays and has queued
    /// (behaviours 4.2). Returns whether it started and the type has calls for it.
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
        if force {
            anim.cut();
        }
        anim.notes.push(Note::Set(name.to_string()));
        self.chain[0].loaded
    }

    fn nest(&mut self, env: &Env, name: &str) {
        self.chain.push(Level {
            name: name.to_string(),
            loaded: env.data.set(env.unit, name).is_some(),
            running: true,
            call: 0,
            done: false,
            forced: false,
            own: None,
        });
    }

    /// Takes the set a saved game holds as the unit's own (behaviours 4.7): each level of the
    /// chain as read, its remaining calls kept as texts, the name not looked up again. The play
    /// state and the animation stay as for a new unit, so the current call runs again from its
    /// start. A set that was not running counts as ended, and the unit stays as it is. Returns
    /// whether a set was taken.
    pub fn restore(&mut self, saved: &Saved) -> bool {
        let top = saved.levels.first();
        if top.is_none_or(|l| l.name.is_empty() || l.tail[2] == 0) {
            return false;
        }
        let call = |text: &String| {
            parse_call(text.as_bytes()).map_or(
                Call {
                    name: "",
                    op: Op::Fail,
                    text: text.clone(),
                },
                |(call, _)| call,
            )
        };
        self.interruptible = top.is_some_and(|l| l.flag != 0);
        self.chain = (saved.levels.iter().take(DEPTH))
            .map(|l| Level {
                name: l.name.clone(),
                loaded: l.tail[0] != 0,
                running: l.tail[2] != 0,
                call: 0,
                done: l.tail[1] != 0,
                forced: l.tail[3] != 0,
                own: Some(l.list_b.iter().map(call).collect()),
            })
            .collect();
        (self.ready, self.playing, self.fresh) = (true, false, true);
        true
    }

    /// The set the unit runs now, in the form of a saved game (behaviours 4.1, 4.7; the inverse of
    /// [`Runner::restore`]): each level of the chain with its remaining calls as texts, the
    /// current one first, taken from the set `sets` finds by name unless the level holds its own.
    /// `base` is the set the unit's record was read with, whose fields nothing here changes are
    /// kept. `None` when the unit has run no set since it was made or loaded.
    pub fn saved<'a>(
        &self,
        base: &Saved,
        sets: impl Fn(&str) -> Option<&'a [Call]>,
    ) -> Option<Saved> {
        let last = self.chain.len().checked_sub(1)?;
        let levels = (self.chain.iter().enumerate())
            .map(|(i, l)| {
                let mut out = base.levels.get(i).cloned().unwrap_or_default();
                let calls = l
                    .own
                    .as_deref()
                    .or_else(|| sets(&l.name))
                    .unwrap_or_default();
                let rest = calls.get(l.call..).unwrap_or_default();
                let current = rest.first();
                let text = current.map_or("", |c| c.text.as_str());
                let (_, args) = split_call(text.as_bytes());
                out.name = l.name.clone();
                if i == 0 {
                    out.flag = u8::from(self.interruptible);
                }
                out.text = text.to_string();
                out.call = current.map_or(String::new(), |c| c.name.to_string());
                out.args = if current.is_some() { args } else { Vec::new() };
                out.flags[0] = u8::from(l.done);
                out.child = u8::from(i < last);
                out.list_b = rest.iter().map(|c| c.text.clone()).collect();
                out.tail = [l.loaded, l.done, l.running, l.forced].map(u8::from);
                out
            })
            .collect();
        Some(Saved { levels })
    }

    /// The unit's behaviour hook (behaviours 4.6): a pending set starts, else an ambient one is
    /// chosen by `key` (none: no choice this time); then the top set executes.
    pub fn step(&mut self, anim: &mut Animation, env: &mut Env, key: Option<i32>) {
        if !self.running() {
            match self.pending.take() {
                Some(name) => {
                    self.start(anim, env, &name, true, false, true);
                }
                None => key.map_or((), |key| self.choose(anim, env, key)),
            }
        }
        if self.running() {
            self.execute(0, anim, env);
        }
        self.fresh = false;
    }

    /// The ambient choice of the animals' checks: only when no set runs and none is pending.
    pub fn ambient(&mut self, anim: &mut Animation, env: &mut Env, key: i32) {
        if !self.running() && self.pending.is_none() {
            self.choose(anim, env, key);
        }
    }

    /// The first row of the ambient table that holds `key` starts (behaviours 6.1).
    fn choose(&mut self, anim: &mut Animation, env: &Env, key: i32) {
        let rows = env.data.ambient(env.unit, self.place as usize);
        if let Some(row) = rows.iter().find(|r| r.lo <= key && key <= r.hi) {
            self.start(anim, env, &row.set, true, false, true);
        }
    }

    /// Executes the set at depth `k` (behaviours 4.3); returns whether it failed. A call that
    /// is done lets the next start in the same step.
    fn execute(&mut self, k: usize, anim: &mut Animation, env: &mut Env) -> bool {
        loop {
            let level = &mut self.chain[k];
            let own = level.own.clone();
            let calls = own
                .as_deref()
                .or_else(|| env.data.set(env.unit, &level.name))
                .unwrap_or_default();
            if !level.loaded {
                level.running = false;
                return true;
            }
            if std::mem::take(&mut level.forced) {
                anim.cut();
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
            Op::PlaySetTerrain { land, water, tank } => {
                let name = match (self.tank_mode, tank) {
                    (4, Some((_, under))) => under,
                    (3, Some((surface, _))) => surface,
                    _ if self.wet => water,
                    _ => land,
                };
                return self.nest_call(env, name);
            }
            Op::PlaySetTank { land, tank } => {
                let name = if self.tank_mode == 4 { tank } else { land };
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
                // It makes idle the base at every run (goals 6.3), like the move call makes its
                // animation: a finished walk keeps its move animation until the cut shows it.
                if !(*routine == Routine::Guide && self.talking) && found(&"idle".to_string()) {
                    anim.base_to("idle");
                }
                self.chain[k].done = self.ready;
                return true;
            }
            Op::KeepControl | Op::Spray | Op::FaceDirection | Op::TargetBuilding => {
                return true;
            }
            Op::FaceFood => {
                self.face = true;
                self.chain[k].done = true;
                return true;
            }
            Op::FaceTarget => {
                self.target = true;
                self.chain[k].done = true;
                return true;
            }
            Op::Halt => {
                self.halt = true;
                self.chain[k].done = true;
                return true;
            }
            Op::Die => {
                self.die = true;
                self.chain[k].done = true;
                return true;
            }
            Op::Hatch => {
                self.hatch = true;
                self.chain[k].done = true;
                return true;
            }
            Op::EnterBuilding => {
                self.enter = true;
                self.chain[k].done = true;
                return true;
            }
            Op::ExitBuilding => {
                self.exit = true;
                self.chain[k].done = true;
                return true;
            }
            Op::Move { gait, target, .. } => {
                self.moving = Some((gait.clone(), *target));
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
        let base = anim.shows_base();
        anim.queue(entries);
        if self.fresh && base {
            anim.finish(&timing);
        }
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
