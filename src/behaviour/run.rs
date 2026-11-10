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

mod animation;
mod data;

pub(crate) use animation::{Animation, Entry, Note};

use std::sync::Arc;

pub(crate) use data::NoData;
pub use data::{Data, Heard, Timing};

use super::{Call, Gait, Op, Place, Play, Routine, Target, parse_call, split_call};
use crate::sim::Rng;
use crate::zoo::{Behaviour as Saved, Entity};

/// The longest nesting of sets (behaviours O8): the original has no limit.
const DEPTH: usize = 16;
/// The most entries one play call queues: `fPlayTime` repeats its animation to cover the time.
const ENTRIES: i64 = 10_000;

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

/// The two names the idle animation of a unit is chosen from (behaviours 5.10): `forced` is the
/// one an egg, a crate or the water gives, which has no fallback; else the type's animation
/// `named` is taken, and the plain `idle` when the type has none of that name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rest {
    pub forced: Option<&'static str>,
    pub named: &'static str,
}

impl Rest {
    /// The animation chosen: the forced one when `has` finds it, else the named one, else the
    /// plain `idle`; none when the type has none of them (behaviours 5.10, 5.10.1 point 5).
    pub(crate) fn pick(self, has: impl Fn(&str) -> bool) -> Option<&'static str> {
        match self.forced {
            Some(name) => has(name).then_some(name),
            None => [self.named, "idle"].into_iter().find(|n| has(n)),
        }
    }
}

impl Default for Rest {
    fn default() -> Self {
        Rest {
            forced: None,
            named: "idle",
        }
    }
}

/// What a unit needs to run its set.
pub(crate) struct Env<'a> {
    pub data: &'a dyn Data,
    pub unit: &'a Entity,
    pub rng: &'a mut Rng,
}

/// The set a unit runs: the chain from the top set down to the innermost nested one, the set it
/// will start when the top one ends, and the play state the play calls share.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Runner {
    chain: Vec<Level>,
    /// The pending call: the name and its own interruptible flag (behaviours 4.2).
    pending: Option<(String, bool)>,
    interruptible: bool,
    /// No play call is in progress; `playing`: one is.
    ready: bool,
    playing: bool,
    /// A start was accepted since the unit last looked (movement 8.3).
    started: bool,
    /// Taken from a saved game and not yet stepped: the player holds no sequence of the unit's
    /// own, so its first play call is shown at once, not behind the base loop (behaviours 4.7).
    fresh: bool,
    /// Where the unit is for its ambient table: set by the unit before each run.
    pub place: Place,
    /// The unit's tank mode and whether its tile is a water tile, which alone choose the set of
    /// a nesting call (tank fauna 9.5): set by the unit before each run.
    pub tank_mode: u8,
    pub wet: bool,
    /// What the idle animation of the unit depends on (behaviours 5.10): set by the unit before
    /// each run; not in the state hash.
    pub rest: Rest,
    /// The guide's task is the talk and not done: its routine does not make idle the base
    /// (behaviours 5.5). Set by the unit before each run.
    pub talking: bool,
    /// The move call the last execution stopped at, for the unit to run.
    moving: Option<(Gait, Target, bool)>,
    /// The move call is an `fTargetBuilding` (animals 11.11 point 4): the animal first chooses its
    /// building.
    rampage: bool,
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
    /// An `fEndTrick` call ran, for the unit to report its trick done (shows 23.3).
    end_trick: bool,
    /// An `fCaughtPrey` and an `fKillPrey` call ran, for the hunter's meal (animals 21.5).
    meal: [bool; 2],
}

/// The state hash of a run reads this text: `started` is not in it (the walk it feeds is hashed
/// through the moving flag), so the hashes stay as they were.
impl std::fmt::Debug for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("chain", &self.chain)
            .field("pending", &self.pending)
            .field("interruptible", &self.interruptible)
            .field("ready", &self.ready)
            .field("playing", &self.playing)
            .field("fresh", &self.fresh)
            .field("place", &self.place)
            .field("tank_mode", &self.tank_mode)
            .field("wet", &self.wet)
            .field("talking", &self.talking)
            .field("moving", &self.moving)
            .field("face", &self.face)
            .field("target", &self.target)
            .field("halt", &self.halt)
            .field("die", &self.die)
            .field("hatch", &self.hatch)
            .field("enter", &self.enter)
            .field("exit", &self.exit)
            .finish()
    }
}

/// The answer to a move call (movement 8.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flow {
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
            started: false,
            fresh: false,
            place: Place::Land,
            tank_mode: 0,
            wet: false,
            rest: Rest::default(),
            talking: false,
            moving: None,
            rampage: false,
            face: false,
            target: false,
            halt: false,
            die: false,
            hatch: false,
            end_trick: false,
            meal: [false; 2],
            enter: false,
            exit: false,
        }
    }
}

impl Runner {
    /// Whether the top set has a call in progress.
    pub(crate) fn running(&self) -> bool {
        self.chain.first().is_some_and(|l| l.running)
    }

    /// The name of the top set, if the unit has started one.
    pub(crate) fn top(&self) -> Option<&str> {
        self.chain.first().map(|l| l.name.as_str())
    }

    /// Whether a set start was accepted since the last time, once.
    pub(crate) fn take_started(&mut self) -> bool {
        std::mem::take(&mut self.started)
    }

    /// The move call the set reached, once.
    pub(crate) fn take_move(&mut self) -> Option<(Gait, Target, bool)> {
        self.moving.take()
    }

    /// Whether a face-the-food call ran since the last time, once.
    pub(crate) fn take_face(&mut self) -> bool {
        std::mem::take(&mut self.face)
    }

    /// Whether a face-the-target call ran since the last time, once.
    pub(crate) fn take_target(&mut self) -> bool {
        std::mem::take(&mut self.target)
    }

    /// Whether a halt call ran since the last time, once.
    pub(crate) fn take_halt(&mut self) -> bool {
        std::mem::take(&mut self.halt)
    }

    /// The set asked to follow the current one, if any.
    pub(crate) fn queued(&self) -> Option<&str> {
        self.pending.as_ref().map(|p| p.0.as_str())
    }

    /// The interruptible flag of the pending call; up when there is none.
    pub(crate) fn queued_flag(&self) -> bool {
        self.pending.as_ref().is_none_or(|p| p.1)
    }

    /// Forgets the set asked to follow the current one (animals-needs 5.4).
    pub(crate) fn forget(&mut self) {
        self.pending = None;
    }

    /// The unit's answer to the move call: it is done, or fails the set.
    pub(crate) fn answer(&mut self, flow: Flow) {
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
    pub(crate) fn resume(&mut self, anim: &mut Animation, env: &mut Env) {
        if self.running() {
            self.execute(0, anim, env);
        }
    }

    /// Writes the pending call as it is, name and flag (a saved game, a restored use; behaviours
    /// 4.7). An empty name is none.
    pub(crate) fn keep(&mut self, name: &str, interruptible: bool) {
        self.pending = (!name.is_empty()).then(|| (name.to_string(), interruptible));
    }

    /// Asks for a set to start when the running one ends (behaviours 4.2): the pending call is
    /// replaced only if it is interruptible (an empty one is) or `force`. Whether it was written.
    pub(crate) fn queue(&mut self, name: &str, interruptible: bool, force: bool) -> bool {
        if !force && !self.queued_flag() {
            return false;
        }
        self.pending = Some((name.to_string(), interruptible));
        true
    }

    /// Whether [`Runner::queue_if_new`] would write: the name is not the top set's and the
    /// pending call may be replaced.
    pub(crate) fn can_queue(&self, name: &str, force: bool) -> bool {
        self.top() != Some(name) && (force || self.queued_flag())
    }

    /// `queue`, unless the top set already has this name (behaviours 4.2).
    pub(crate) fn queue_if_new(&mut self, name: &str, interruptible: bool, force: bool) -> bool {
        self.can_queue(name, force) && self.queue(name, interruptible, force)
    }

    /// The calls the top set has left, the current one included: none when no set runs, and a
    /// nested set is not counted (behaviours 4.1).
    pub(crate) fn calls_left(&self, data: &dyn Data, unit: &Entity) -> usize {
        let Some(top) = self.chain.first().filter(|l| l.running && l.loaded) else {
            return 0;
        };
        let calls = match &top.own {
            Some(own) => own.len(),
            None => data.set(unit, &top.name).map_or(0, <[Call]>::len),
        };
        calls.saturating_sub(top.call)
    }

    /// The name of the call the top running set is on (behaviours 5.10.2 point 2).
    pub(crate) fn call_name(&self, data: &dyn Data, unit: &Entity) -> Option<String> {
        let top = self.chain.first().filter(|l| l.running && l.loaded)?;
        match &top.own {
            Some(own) => own.get(top.call).map(|c| c.name.to_string()),
            None => (data.set(unit, &top.name))
                .and_then(|s| s.get(top.call))
                .map(|c| c.name.to_string()),
        }
    }

    /// Starts the set `name` as the top set. A running set that is not interruptible refuses
    /// unless `force`. Unless `keep`, the animation drops what it plays and has queued
    /// (behaviours 4.2). Returns whether it started and the type has calls for it.
    pub(crate) fn start(
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
        self.started = true;
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
    pub(crate) fn restore(&mut self, saved: &Saved) -> bool {
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
    pub(crate) fn saved<'a>(
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
                // A set saved before its first run holds an empty parsed call and current-call text:
                // the first run parses them, so until then the save writes them as read (save games 15.8).
                let unrun = self.fresh
                    && (base.levels.get(i)).is_some_and(|b| b.name == l.name && b.call.is_empty());
                if !unrun {
                    out.text = text.to_string();
                    out.call = current.map_or(String::new(), |c| c.name.to_string());
                    out.args = if current.is_some() { args } else { Vec::new() };
                }
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
    pub(crate) fn step(&mut self, anim: &mut Animation, env: &mut Env, key: Option<i32>) {
        if !self.running() {
            match self.pending.take() {
                Some((name, interruptible)) => {
                    self.start(anim, env, &name, interruptible, false, true);
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
    pub(crate) fn ambient(&mut self, anim: &mut Animation, env: &mut Env, key: i32) {
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
            Op::PlaySetProb(rows) => return self.nest_prob(env, rows),
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
            Op::Routine(routine) => return self.routine(k, *routine, anim, env),
            Op::DustBall => return self.dust_ball(k, anim, env),
            Op::KeepControl | Op::Spray | Op::FaceDirection => {
                return true;
            }
            // The call is the unit's, like a move: the target choice, then a fast move (animals
            // 11.11 point 4).
            Op::TargetBuilding => {
                (self.moving, self.rampage) = (Some(takes::charge()), true);
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
            Op::EndTrick => {
                self.end_trick = true;
                self.chain[k].done = true;
                return true;
            }
            Op::CaughtPrey | Op::KillPrey => {
                self.meal[usize::from(matches!(call.op, Op::KillPrey))] = true;
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
            Op::Move {
                gait,
                target,
                reverse,
                ..
            } => {
                self.moving = Some((gait.clone(), *target, *reverse));
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
        // The base of the call is the unit's idle animation; without one the call fails.
        let Some(idle) = self.idle(env) else {
            return false;
        };
        let base = anim.shows_base();
        anim.queue(entries, idle);
        if self.fresh && base {
            anim.finish(&timing);
        }
        (self.playing, self.ready) = (true, false);
        true
    }

    /// The idle animation of the unit (behaviours 5.10): the forced one when the type has it, else
    /// the named one, else the plain `idle`; none when the type has none of them.
    fn idle(&self, env: &Env) -> Option<&'static str> {
        self.rest
            .pick(|name| env.data.timing(env.unit, name).is_some())
    }

    /// A set picked at random among the rows (behaviours 5.3): one draw, and another after every
    /// row that is passed.
    fn nest_prob(&mut self, env: &mut Env, rows: &[(String, i32)]) -> bool {
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
        false
    }

    /// A routine call (behaviours 5.5): the goals run every step anyway (goals 6.3), so the call
    /// is done when the play state is ready. A call for another class of unit does nothing; the
    /// keeper's fails.
    fn routine(&mut self, k: usize, routine: Routine, anim: &mut Animation, env: &Env) -> bool {
        let own = match routine {
            Routine::Keeper => "keeper",
            Routine::Guest => "guests",
            Routine::Guide => "tour",
            Routine::Maint => "maint",
        };
        if env.unit.class != own {
            return routine != Routine::Keeper;
        }
        // It makes idle the base at every run (goals 6.3), like the move call makes its
        // animation: a finished walk keeps its move animation until the cut shows it.
        let idle = env.data.timing(env.unit, "idle").is_some();
        if !(routine == Routine::Guide && self.talking) && idle {
            anim.base_to("idle");
        }
        self.chain[k].done = self.ready;
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

mod cloud;
mod takes;
#[cfg(test)]
mod tests;
