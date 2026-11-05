//! The goals of the units (goals 3 to 9, 12): the record every unit carries, the goal objects
//! built from it, the order of one goal update and the idle behaviour, the wander.
//!
//! A unit holds one [`Record`]: a task (what it does), a job (the step of a staff task) and their
//! targets. The goal object of the task and the sub-goal object of the job are built lazily at the
//! next goal update, through [`Goal::build`] and [`Sub::build`]; setting a task only marks the
//! object stale. [`update`] is one unit's goal update: its kind's ladder, the common part (one
//! draw), the goal object, then the sub-goal object. The per-kind goals and ladders of later
//! issues are variants of [`Goal`] and rungs of the ladder.
//!
//! Going somewhere is [`Cx::go_to`]: a path through the finder, the destination, the walk set.
//! The log family `unit`: `unit goal <id> <task> <mode> <target id or -> <tile x,y or ->` when
//! the task changes and `unit job <id> <job>` when the job does, with the original's names.

use crate::behaviour::{Data, Env};
use crate::path::Dest;
use crate::unit::{Ctx, Kind, Part, Unit, walk};
use crate::world::World;
use crate::zoo::Entity;

mod animal;
mod avoid;
mod chase;
mod dropped;
mod drown;
mod entry;
mod guest;
mod object;
mod prey;
mod reach;
mod recover;
mod show;
mod staff;
mod stops;
#[cfg(test)]
mod tests;
mod trick;

pub(crate) use animal::{DRINK, KEEPER_FOOD, SURFACE};
pub(crate) use dropped::{dropped, picked_up, reset_unit, stir_units};
pub(crate) use entry::{crowd, meal, send, use_call};
use guest::Ride;
pub(crate) use guest::{IN_BUILDING, LEAVE, rides_loaded};
pub(crate) use object::{Goal, Sub};
pub(crate) use staff::release_group;
pub(crate) use stops::{lose_task, resume_use, stop_all, stop_use};
pub(crate) use trick::{TRICK_FOOD, start_trick};

pub(crate) const NONE: u8 = 0;
/// The wander.
pub(crate) const RANDOM: u8 = 1;
/// A guest picks what to see next.
pub(crate) const SITE: u8 = 100;
pub(crate) const HABITAT: u8 = 101;
/// The duties of a keeper (staff 7.5).
pub(crate) const DUTIES: u8 = 110;
/// A keeper darts and crates an escaped animal (staff 7.11).
pub(crate) const CAPTURE: u8 = 111;
pub(crate) const GAWK: u8 = 112;
/// A guest walks to a scenery object and gawks at it (goals 11.12).
pub(crate) const SCENERY: u8 = 128;
pub(crate) const DO_SCENERY: u8 = 129;
/// The maintenance duties of a worker (staff 8).
pub(crate) const MAINT: u8 = 120;
/// A keeper enters a tank by the ladders of its gate and leaves it (tank staff 4).
pub(crate) const ENTER_TANK: u8 = 126;
pub(crate) const EXIT_TANK: u8 = 127;
/// A tour guide's leg to a viewing area and its talk there (tour guide 5 to 7); the first is also
/// a guest's walk after a guide.
pub(crate) const LEG: u8 = 118;
pub(crate) const TALK: u8 = 119;
/// A guest is sick on the spot (goals 11.10).
pub(crate) const SICK: u8 = 121;
/// The tasks the costs of the path finder look at (pathfinding 6.5, 6.6): entering a building,
/// chasing, avoiding a predator, returning to a show building, recovering from a fall into water
/// and the animal's trips to and from a show tank.
pub(crate) const ENTER_BUILDING: u8 = 104;
/// The other tasks of the building chain and of eating (goals 11.3, 11.4).
pub(crate) const GO_BUILDING: u8 = 102;
pub(crate) const ENTRANCE: u8 = 103;
pub(crate) const EXIT_BUILDING: u8 = 105;
pub(crate) const USE: u8 = 106;
pub(crate) const EAT: u8 = 107;
pub(crate) const EAT_STANDING: u8 = 108;
pub(crate) const EATING_STANDING: u8 = 109;
pub(crate) const CHASE: u8 = 116;
pub(crate) const AVOID: u8 = 117;
pub(crate) const RETURN_TO_BUILDING: u8 = 130;
pub(crate) const RECOVER: u8 = 131;
pub(crate) const DROWN: u8 = 132;
pub(crate) const SHOWTRICK: &str = "showtrick";
pub(crate) const GO_TO_SHOW: u8 = 133;
pub(crate) const RETURN_FROM_SHOW: u8 = 134;
/// The tasks of the building chain, 0x66 to 0x6a.
pub(crate) const BUILDING: std::ops::RangeInclusive<u8> = 102..=106;
/// The names of the tasks 100 to 134, by the number less 100.
#[rustfmt::skip]
const TASKS: [&str; 35] = [
    "site", "habitat", "building", "gotoentrance", "enterbuilding", "leavebuilding",
    "dobuilding", "eat", "eatstanding", "eatingstanding", "habitatduties", "capturing", "gawk",
    "leave", "keeperfood", "otherfood", "chase", "avoid", "viewingarea", "guideduties",
    "maintduties", "sick", "drinkwater", "targetbuilding", "surfacebreath", "trickfood",
    "entertank", "exittank", "scenery", "doscenery", "returntobuilding", "recoverfromwater",
    "drowninwater", "gotoshow", "returnfromshow",
];
/// The names of the jobs 1 to 29, by the number less 1.
#[rustfmt::skip]
const JOBS: [&str; 29] = [
    "PutFood", "PuttingFood", "HealAnimal", "HealingAnimal", "CleanDirt", "CleaningDirt",
    "TrashPile", "SweepTrash", "TrashCan", "EmptyTrash", "ZooDoo", "BagDoo", "ChaseAnimal",
    "TranquilizeAnimal", "RetrieveAnimal", "GoToFence", "FixFence", "Preattack", "ConductShow",
    "ClimbUpOutsideLadder", "ClimbDownOutsideLadder", "GoToOutsideLadderBase",
    "GoToInsideLadderBase", "CleanTank", "CleaningTank", "GoToFilter", "FixFilter",
    "ClimbUpInsideLadder", "ClimbDownInsideLadder",
];

/// For the tests of the other modules: gives every unit a task that has no goal, so that the
/// goals (the wander) leave its path, its set and the generator alone.
#[cfg(test)]
pub(crate) fn quiet(world: &mut World) {
    let ids: Vec<u32> = world.units.iter().map(|u| u.id).collect();
    for id in ids {
        if let Some(u) = world.units.get_mut(id) {
            u.goal.record.task = 115;
        }
    }
}

/// The name the original gives a task.
pub fn task_name(task: u8) -> &'static str {
    match task {
        NONE => "none",
        RANDOM => "random",
        2 => "special",
        t => TASKS
            .get(usize::from(t).wrapping_sub(100))
            .unwrap_or(&"unknown"),
    }
}

/// The name the original gives a job.
pub(crate) fn job_name(job: u8) -> &'static str {
    match job {
        0 => "none",
        j => JOBS.get(usize::from(j) - 1).unwrap_or(&"unknown"),
    }
}

/// An entity and a place a task or a job aims at (3.1). The tile and the position follow each
/// other: a tile gives its corner, a position its tile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Target {
    pub id: u32,
    pub tile: Option<usize>,
    /// In movement units; z in 16ths of a height step.
    pub pos: Option<(i32, i32, i32)>,
}

impl Target {
    pub(crate) fn clear(&mut self) {
        *self = Target::default();
    }

    pub(crate) fn is_set(&self) -> bool {
        self.id != 0 || self.tile.is_some()
    }

    pub(crate) fn set_tile(&mut self, world: &World, tile: Option<usize>) {
        self.tile = tile;
        let width = world.zoo.width as usize;
        self.pos = tile.and_then(|t| {
            let z = world.zoo.tiles.get(t)?.elevation * 16;
            Some(((t % width) as i32 * 64, (t / width) as i32 * 64, z))
        });
    }

    pub(crate) fn set_pos(&mut self, world: &World, pos: Option<(i32, i32, i32)>) {
        self.pos = pos;
        self.tile = pos.and_then(|(x, y, _)| world.tile_index(x.div_euclid(64), y.div_euclid(64)));
    }
}

/// The goal record of a unit (3.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Record {
    pub task: u8,
    /// Stored and logged, never read (goals O2).
    pub mode: u8,
    pub target: Target,
    pub job: u8,
    pub job_target: Target,
    /// The AI tick the task was set at, and its duration in AI ticks (8.3).
    pub start: u32,
    pub duration: u32,
    pub done: bool,
    pub job_done: bool,
}

impl Record {
    /// 8.3: strictly more ticks than the duration have passed since the task was set.
    pub(crate) fn duration_over(&self, now: u32) -> bool {
        now.wrapping_sub(self.start) > self.duration
    }
}

/// The active goal a unit writes (save games 11.2, S7): its task when the task has a goal object for
/// the kind of unit, -1 for none.
pub(crate) fn active_goal(task: u8, kind: Kind) -> i32 {
    Goal::build(task, kind).map_or(-1, |_| i32::from(task))
}

/// The goal state of a unit: the record, the objects built from it and the flags of the base
/// goal.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct Goals {
    pub record: Record,
    object: Option<Goal>,
    stale: bool,
    sub: Option<Sub>,
    sub_stale: bool,
    /// The type numbers of the goal and job objects, the ones a save writes (save games 11.2):
    /// the task and job they were built for, which a stale object keeps until it is replaced.
    held: (Option<u8>, Option<u8>),
    /// Raised by the guest's ladder to let an idle unit wander again (7.1).
    pub wants_to_decide: bool,
    /// A guest made a random walk and stands until a draw or its status tick lets it walk again
    /// (7.3); the status tick lowers it (`guest::checks`).
    pub wandered: bool,
    /// The tries a guest has left to climb out of the water (tank fauna 13); not saved.
    tries: Option<u8>,
    /// The state of the ride on the water (11.13); not saved.
    ride: Ride,
}

/// The state hash of a run reads this text: `held` is not in it (it follows from the objects), so
/// the hashes stay as they were.
impl std::fmt::Debug for Goals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Goals")
            .field("record", &self.record)
            .field("object", &self.object)
            .field("stale", &self.stale)
            .field("sub", &self.sub)
            .field("sub_stale", &self.sub_stale)
            .field("wants_to_decide", &self.wants_to_decide)
            .field("wandered", &self.wandered)
            .finish()
    }
}

impl Goals {
    /// A load (behaviours 4.7): from version 72 the object whose number the file holds as active is
    /// built, and marked neither stale nor to be initialised, when it is the record's task (or job).
    /// Any other case leaves the object to be built, init included, at the first update.
    pub(crate) fn built(&mut self, kind: Kind, active: i32, active_sub: i32) {
        let r = &self.record;
        if active == i32::from(r.task) {
            (self.object, self.stale) = (Goal::build(r.task, kind), false);
            self.held.0 = self.object.map(|_| r.task);
        }
        if active_sub == i32::from(r.job) {
            (self.sub, self.sub_stale) = (Sub::build(r.job, kind), false);
            self.held.1 = self.sub.map(|_| r.job);
        }
    }

    /// The numbers the unit's goal and job objects hold, -1 for no object (save games 11.2, behaviours
    /// 4.7): a stale object still has its old number.
    pub(crate) fn held(&self) -> (i32, i32) {
        let n = |h: Option<u8>| h.map_or(-1, i32::from);
        (n(self.held.0), n(self.held.1))
    }

    /// Whether the goal object and the job object exist.
    #[cfg(test)]
    pub(crate) fn built_pair(&self) -> (bool, bool) {
        (self.object.is_some(), self.sub.is_some())
    }
}

/// What a goal update works on: the unit taken out of the table for its turn, the world, the
/// managers' context, the unit's place in the entity list and the data of the sets.
pub(crate) struct Cx<'a, 'c> {
    pub unit: &'a mut Unit,
    pub world: &'a mut World,
    pub ctx: &'a mut Ctx<'c>,
    pub at: usize,
    pub data: &'a dyn Data,
}

/// One unit's goal update (6.2), after its behaviour.
pub fn update(unit: &mut Unit, world: &mut World, ctx: &mut Ctx, at: usize, data: &dyn Data) {
    let mut cx = Cx {
        unit,
        world,
        ctx,
        at,
        data,
    };
    if !cx.ladder() {
        return;
    }
    // Movement 4.7: a refused step makes the goal give up; the flag is cleared.
    if std::mem::take(&mut cx.unit.aborted) {
        cx.abort();
    }
    cx.common();
    cx.run_goal();
    cx.run_sub();
}

impl Cx<'_, '_> {
    pub(crate) fn record(&mut self) -> &mut Record {
        &mut self.unit.goal.record
    }

    pub(crate) fn now(&self) -> u32 {
        self.world.units.ai_ticks
    }

    /// The tile the unit stands on.
    fn tile(&self) -> Option<usize> {
        let c = self.world.zoo.entities.get(self.at)?.common.as_ref()?;
        self.world
            .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
    }

    /// 3.2: sets the task. The object is marked stale, the job cleared.
    pub(crate) fn set_task(&mut self, task: u8, mode: u8, id: u32, tile: Option<usize>) {
        let (now, unit, guide) = (self.now(), self.unit.id, self.unit.kind == Kind::Guide);
        let world = &mut *self.world;
        let goal = &mut self.unit.goal;
        let r = &mut goal.record;
        let changed = r.task != task;
        (r.task, r.mode, r.done, r.start, r.duration) = (task, mode, false, now, 0);
        r.target.clear();
        r.target.id = id;
        r.target.set_tile(world, tile);
        goal.stale = true;
        if changed {
            if let (true, Some(job)) = (guide, staff::guide_job(task)) {
                world.units.staff_note(format!("job {unit} {job}"));
            }
            let width = world.zoo.width as usize;
            let place = tile.map_or("-".into(), |t| format!("{},{}", t % width, t / width));
            let target = if id == 0 { "-".into() } else { id.to_string() };
            let line = format!("goal {unit} {} {mode} {target} {place}", task_name(task));
            world.units.note(line);
        }
        self.set_job(0, 0, None);
    }

    /// 3.2: sets the job and its target; the task is untouched.
    pub(crate) fn set_job(&mut self, job: u8, id: u32, tile: Option<usize>) {
        let world = &mut *self.world;
        let goal = &mut self.unit.goal;
        let r = &mut goal.record;
        let changed = r.job != job;
        (r.job, r.job_done) = (job, false);
        r.job_target.clear();
        r.job_target.id = id;
        r.job_target.set_tile(world, tile);
        goal.sub_stale = true;
        if changed {
            let line = format!("job {} {}", self.unit.id, job_name(job));
            match self.unit.kind {
                Kind::Keeper | Kind::Worker | Kind::Guide => world.units.staff_note(line),
                _ => world.units.note(line),
            }
        }
    }

    /// The guest's non-path cost goes back to its type's, over a saved or emergency one
    /// (pathfinding 6.5).
    pub(in crate::goal) fn cost_back(&mut self) {
        (self.unit.mover.emergency, self.unit.mover.word) = (false, None);
    }

    /// 12.1: clears the record, empties the path, lowers the flags.
    pub(crate) fn reset(&mut self) {
        match self.unit.kind {
            Kind::Keeper => self.keeper_reset(),
            Kind::Guide => self.guide_reset(),
            Kind::Worker => self.worker_reset(),
            _ => self.unfollow(),
        }
        self.set_task(NONE, 0, 0, None);
        self.unit.path.clear();
        self.unit.goal.wants_to_decide = false;
        // A guest's building target goes with it, and so does the eat-standing flag (guests 8).
        self.clear_target();
        self.cost_back();
        match &mut self.unit.part {
            Part::Guest(g) => g.stand = false,
            // An animal's reset lowers its need flags too and ends its escape walk, and it lets go of
            // its kept predator and its kept prey (12.1, animals 9.2, 11.3).
            Part::Animal(a) => {
                a.reset_needs();
                (self.unit.frightened, self.unit.predator) = (false, 0);
                self.unit.clear_own();
            }
            _ => {}
        }
    }

    /// 12.3: empties the path and invalidates the targets; the task stays. An animal's escape walk
    /// ends (animals 11.3).
    pub(crate) fn abort(&mut self) {
        if let Part::Animal(a) = &mut self.unit.part {
            a.walk = None;
        }
        self.unit.path.clear();
        match self.unit.goal.object {
            Some(Goal::Ride) => self.ride_invalidate(),
            _ => self.record().target.clear(),
        }
        self.record().job_target.clear();
    }

    /// A dying, darted, crated or egg animal runs no goal update (10.1 point 1).
    fn halted(&self) -> bool {
        matches!(&self.unit.part, Part::Animal(a) if a.dying || a.tranq || a.crated || a.egg)
    }

    /// 7.3: whether an idle unit wanders: it is not in the building chain and not chased (M7).
    /// A guest also needs its wandered flag down.
    fn keeps_moving(&self) -> bool {
        let free = self.unit.kind != Kind::Guest || !self.unit.goal.wandered;
        free && !BUILDING.contains(&self.unit.goal.record.task)
    }

    /// The unit's movement mode: 1 flying, 3 swimming in a tank.
    fn mode(&self) -> u32 {
        let e = self.world.zoo.entities.get(self.at);
        e.and_then(|e| e.common.as_ref())
            .map_or(0, |c| c.movement_mode)
    }

    /// 6.2 before the object: one draw, and one time in 15 a unit outside the zoo is removed and
    /// a unit on the footprint of a building is sent out of it.
    fn common(&mut self) {
        if self.ctx.rng.pick(15) != 0 {
            return;
        }
        match self.tile() {
            Some(t) if self.world.in_zoo(t) => self.send_out(t),
            _ => self.ctx.remove(self.world, self.unit.id),
        }
    }

    /// 10: the checks of the unit's kind that may replace the task. At most one fires; those
    /// of the per-kind issues come in front of these. False ends the goal update there.
    fn ladder(&mut self) -> bool {
        let r = &self.unit.goal.record;
        let (task, done) = (r.task, r.done);
        match self.unit.kind {
            // 10.1 point 1: a halted animal's record is cleared at every update (the start is
            // stamped again, the object marked stale) and nothing else runs, not even the draw.
            Kind::Animal if self.halted() => {
                self.set_task(NONE, 0, 0, None);
                return false;
            }
            Kind::Animal => return self.animal_ladder(task),
            Kind::Guest => self.guest_ladder(task, done),
            Kind::Keeper => self.keeper_ladder(task, done),
            Kind::Worker => self.worker_ladder(task, done),
            Kind::Guide => self.guide_ladder(task, done),
            Kind::Heli => return self.heli_ladder(task),
        }
        true
    }

    /// 10.2 point 8: the task goes on. Point 10: the guest stands while its wandered flag is up,
    /// else it picks again: the sites to see, else the seen count goes and it may decide to leave.
    /// Point 1 (a guest in an exhibit or a tank leaves it), point 2 (a guest that is to leave
    /// takes the leave task) and point 3 (a frightened guest avoids its predator) come first.
    fn guest_ladder(&mut self, task: u8, done: bool) {
        self.spread();
        if self.return_ladder()
            || self.exit_ladder()
            || self.leave_ladder()
            || self.fright_ladder(task)
            || self.sick_ladder()
            || self.stand_ladder(task)
        {
            return;
        }
        // A guest after a guide keeps its done task until it is released (tour guide 10.3).
        if task != NONE && task != SITE && (!done || task == LEG) {
            return;
        }
        // Point 9: a guest that holds food looks for a place to eat it.
        let food = matches!(&self.unit.part, Part::Guest(g) if g.items[0].is_some());
        if food && task != ENTRANCE && !(104..=106).contains(&task) {
            self.reset();
            self.set_task(EAT, 4, 0, None);
            return;
        }
        if self.unit.goal.wandered {
            if self.ctx.rng.pick(15) == 0 {
                self.unit.goal.wandered = false;
                self.unit.goal.wants_to_decide = true;
            }
        } else if task != SITE {
            self.reset();
            if self.next_site() {
                return;
            }
            if self.keeps_moving() {
                self.set_task(RANDOM, 1, 0, None);
            }
        }
    }

    /// 4.2: removes a stale object, builds a missing one, updates it.
    fn run_goal(&mut self) {
        if self.unit.goal.stale {
            self.unit.goal.stale = false;
            if let Some(old) = self.unit.goal.object.take() {
                old.abort(self);
            }
        }
        if self.unit.goal.object.is_none() {
            let task = self.record().task;
            self.unit.goal.object = Goal::build(task, self.unit.kind);
            self.unit.goal.held.0 = self.unit.goal.object.map(|_| task);
            if let Some(goal) = self.unit.goal.object
                && !goal.init(self)
            {
                self.unit.goal.object = None;
                self.unit.goal.held.0 = None;
                self.set_task(NONE, 0, 0, None);
            }
        }
        if let Some(goal) = self.unit.goal.object {
            goal.update(self);
        }
    }

    fn run_sub(&mut self) {
        if std::mem::take(&mut self.unit.goal.sub_stale) {
            self.sub_dropped();
        }
        if self.unit.goal.sub.is_none() {
            let job = self.record().job;
            self.unit.goal.sub = Sub::build(job, self.unit.kind);
            self.unit.goal.held.1 = self.unit.goal.sub.map(|_| job);
            let refused = match self.unit.goal.sub {
                Some(Sub::Keeper(job)) => !self.job_init(job),
                Some(Sub::Worker(job)) => !self.work_init(job),
                Some(Sub::Preattack) => !self.preattack_init(),
                _ => false,
            };
            if refused {
                self.unit.goal.sub = None;
                self.unit.goal.held.1 = None;
                self.set_job(0, 0, None);
            }
        }
        if let Some(sub) = self.unit.goal.sub {
            sub.update(self);
        }
    }

    /// 7.1: the update of the base goal.
    fn base(&mut self) {
        let removed = self.ctx.removals.contains(&self.unit.id);
        let r = &self.unit.goal.record;
        let (task, done) = (r.task, r.done);
        if task != NONE && !done && !removed {
            self.check_done();
            let r = &self.unit.goal.record;
            let has_target = r.target.is_set() || r.job_target.is_set();
            if !r.done
                && (!has_target || (r.target.id != 0 && r.target.tile.is_none()))
                && r.job == 0
            {
                self.decide();
                if self.record().target.tile.is_none() && self.keeps_moving() {
                    self.set_task(RANDOM, 1, 0, None);
                    self.unit.goal.wants_to_decide = false;
                }
            }
        } else if self.unit.goal.wants_to_decide {
            if (task == NONE || done) && self.keeps_moving() {
                self.set_task(RANDOM, 1, 0, None);
            }
            self.unit.goal.wants_to_decide = false;
        }
        let r = &self.unit.goal.record;
        if r.task == RANDOM {
            if r.done {
                self.reset();
                self.unit.goal.wandered = self.unit.kind == Kind::Guest;
            }
        } else if r.task == NONE && self.keeps_moving() {
            self.set_task(RANDOM, 1, 0, None);
            self.unit.goal.wants_to_decide = false;
        }
    }

    /// The calls the unit's top set has left (behaviours 4.1).
    fn calls_left(&self) -> usize {
        let entity = &self.world.zoo.entities[self.at];
        self.unit.runner.calls_left(self.data, entity)
    }

    /// 7.2: the unit reached its target, which is cleared; a wander is done when the top set has
    /// fewer than two calls left. With more the task stays and the decide draws a new place. An
    /// animal's escape walk decides nothing here; it ends on its last tile (animals 11.3).
    fn check_done(&mut self) {
        let here = self.tile();
        if self.record().task == RANDOM
            && crate::animal::escape::walk_done(self.unit, self.world, here)
        {
            return;
        }
        if self.reached(false) {
            self.record().target.clear();
            if self.calls_left() < 2 && self.record().task == RANDOM {
                self.record().done = true;
                self.unit.goal.wants_to_decide = false;
            }
        }
    }

    /// 7.2: the wander's decide.
    fn decide(&mut self) {
        if self.record().task == RANDOM {
            self.wander(false);
        }
    }

    /// 7.2: unless it is walking, the unit empties its path, asks for a random destination
    /// and walks there, forced. The target is the job's for the base sub-goal. An animal's
    /// wander starts no set: its ambient sets walk it to the destination, which becomes its
    /// target.
    fn wander(&mut self, job: bool) {
        let r = &self.unit.goal.record;
        if (if job { &r.job_target } else { &r.target }).tile.is_some()
            && !self.unit.path.is_empty()
        {
            return;
        }
        self.unit.path.clear();
        let walked = walk::random_walk(self.unit, self.world, self.ctx.cat, self.ctx.rng, self.at);
        if let Some(to) = self.unit.sent.take() {
            return self.send_to(to);
        }
        if !walked {
            return;
        }
        if !job && self.unit.kind == Kind::Animal {
            if let Some(dest) = self.unit.destination {
                let world = &*self.world;
                self.unit.goal.record.target.set_pos(world, Some(dest));
            }
            // On an escape walk it runs (goals 7.2, animals 11.3); with neither set the walk ends.
            let escaping = matches!(&self.unit.part, Part::Animal(a) if a.walk.is_some());
            let run = |cx: &mut Self, set| {
                let can = !cx.unit.suspended() && cx.unit.destination.is_some();
                can && cx.start_with(set, false, true)
            };
            if escaping
                && !run(self, "bRun")
                && !run(self, "bRunFromPredator")
                && let Part::Animal(a) = &mut self.unit.part
            {
                a.walk = None;
            }
        } else {
            self.start_walk(false, job, true);
        }
    }

    /// Asks the finder for a path to tile `to`; on success the unit holds the path and its
    /// destination (the tile's centre at its height, movement 8.1).
    pub(crate) fn path_to(&mut self, to: usize, partial: bool) -> bool {
        self.path_to_level(to, partial, None)
    }

    /// The same for a unit in a tank, to the level `level` of the destination when it is given
    /// (else the unit's own).
    pub(crate) fn path_to_level(&mut self, to: usize, partial: bool, level: Option<i32>) -> bool {
        let Some(here) = self.tile() else {
            return false;
        };
        let mut mover = crate::unit::movement::mover(self.unit, self.world, self.at, here);
        if let Some(l) = level {
            mover.levels.1 = l;
        }
        let mut finder = std::mem::take(&mut self.world.units.finder);
        let reply = finder.request(self.world, &mover, to, partial);
        self.world.units.finder = finder;
        if let Some(path) = reply.path {
            (self.unit.path, self.unit.levels) = (path, reply.levels.clone());
        }
        let width = self.world.zoo.width as usize;
        let centre = |t: usize| {
            let z = (reply.levels.last().map(|l| l * 16))
                .or_else(|| self.world.zoo.tiles.get(t).map(|t| t.elevation * 16))
                .unwrap_or(0);
            Some((
                (t % width) as i32 * 64 + 32,
                (t / width) as i32 * 64 + 32,
                z,
            ))
        };
        match reply.dest {
            Dest::Set(t) => self.unit.destination = centre(t),
            Dest::Clear => self.unit.destination = None,
            Dest::Keep => {}
        }
        reply.ok
    }

    /// 8.1 `goToDest`: starts the walk set towards the destination the unit holds, `force` as
    /// the goal says. It fails when the unit may not move, has no destination or the set does
    /// not start (refused, or the type has no calls for it); then nothing changes. On success
    /// the target becomes the destination.
    pub(crate) fn start_walk(&mut self, run: bool, job: bool, force: bool) -> bool {
        let set = match (self.mode(), run) {
            (1, _) => "bFly",
            (_, true) => "bRun",
            _ => "bWalk",
        };
        self.walk_set(set, job, force)
    }

    /// The same with the walk set the goal names (`bFindKeeperFood`).
    pub(crate) fn walk_set(&mut self, set: &str, job: bool, force: bool) -> bool {
        let started = !self.unit.suspended()
            && !self.unit.held
            && self.unit.destination.is_some()
            && self.start_set(set, force);
        if !started {
            return false;
        }
        let (world, dest) = (&*self.world, self.unit.destination);
        let r = &mut self.unit.goal.record;
        (if job {
            &mut r.job_target
        } else {
            &mut r.target
        })
        .set_pos(world, dest);
        true
    }

    /// The plain start of a set (behaviours 4.2): a running `name` starts again. Interruptible;
    /// the sequence playing goes on, the move call of the set puts the walk animation behind it.
    pub(crate) fn start_set(&mut self, name: &str, force: bool) -> bool {
        self.start_with(name, true, force)
    }

    /// The same, `interruptible` as the caller says.
    pub(crate) fn start_with(&mut self, name: &str, interruptible: bool, force: bool) -> bool {
        let entity: &Entity = &self.world.zoo.entities[self.at];
        let env = Env {
            data: self.data,
            unit: entity,
            rng: &mut *self.ctx.rng,
        };
        let (anim, runner) = (&mut self.unit.animation, &mut self.unit.runner);
        runner.start(anim, &env, name, interruptible, force, true)
    }
}
