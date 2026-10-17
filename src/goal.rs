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
//! The chase task (0x74) stays off in M6 (goals O11): the original's ladder starts it now and
//! then for a second or two; the ladder here never does.
//!
//! The log family `unit`: `unit goal <id> <task> <mode> <target id or -> <tile x,y or ->` when
//! the task changes and `unit job <id> <job>` when the job does, with the original's names.

use crate::behaviour::{Data, Env};
use crate::path::Dest;
use crate::unit::{Ctx, Kind, Part, Unit, walk};
use crate::world::World;
use crate::zoo::Entity;

mod animal;
mod guest;
#[cfg(test)]
mod tests;

pub use animal::{DRINK, KEEPER_FOOD};
pub use guest::LEAVE;

pub const NONE: u8 = 0;
/// The wander.
pub const RANDOM: u8 = 1;
/// A guest picks what to see next.
pub const SITE: u8 = 100;
pub const HABITAT: u8 = 101;
pub const GAWK: u8 = 112;
/// The tasks the costs of the path finder look at (pathfinding 6.5, 6.6): entering a building,
/// chasing, avoiding a predator, returning to a show building, recovering from a fall into water
/// and the animal's trips to and from a show tank.
pub const ENTER_BUILDING: u8 = 104;
pub const CHASE: u8 = 116;
pub const AVOID: u8 = 117;
pub const RETURN_TO_BUILDING: u8 = 130;
pub const RECOVER: u8 = 131;
pub const GO_TO_SHOW: u8 = 133;
pub const RETURN_FROM_SHOW: u8 = 134;
/// The tasks of the building chain, 0x66 to 0x6a.
const BUILDING: std::ops::RangeInclusive<u8> = 102..=106;
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
            u.goal.record.task = 134;
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
pub fn job_name(job: u8) -> &'static str {
    match job {
        0 => "none",
        j => JOBS.get(usize::from(j) - 1).unwrap_or(&"unknown"),
    }
}

/// An entity and a place a task or a job aims at (3.1). The tile and the position follow each
/// other: a tile gives its corner, a position its tile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    pub id: u32,
    pub tile: Option<usize>,
    /// In movement units; z in 16ths of a height step.
    pub pos: Option<(i32, i32, i32)>,
}

impl Target {
    pub fn clear(&mut self) {
        *self = Target::default();
    }

    pub fn is_set(&self) -> bool {
        self.id != 0 || self.tile.is_some()
    }

    pub fn set_tile(&mut self, world: &World, tile: Option<usize>) {
        self.tile = tile;
        let width = world.zoo.width as usize;
        self.pos = tile.and_then(|t| {
            let z = world.zoo.tiles.get(t)?.elevation * 16;
            Some(((t % width) as i32 * 64, (t / width) as i32 * 64, z))
        });
    }

    pub fn set_pos(&mut self, world: &World, pos: Option<(i32, i32, i32)>) {
        self.pos = pos;
        self.tile = pos.and_then(|(x, y, _)| world.tile_index(x.div_euclid(64), y.div_euclid(64)));
    }
}

/// The goal record of a unit (3.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
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
    pub fn duration_over(&self, now: u32) -> bool {
        now.wrapping_sub(self.start) > self.duration
    }
}

/// The goal object of a task. Each variant is a small state machine over the unit and its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    /// Tasks 0 and 1: nothing and the wander (7).
    Base,
    /// 0x65 of a guest: walk to the viewing tile of an exhibit (11.2).
    Habitat,
    /// 0x72 of an animal: walk to a piece of keeper food and eat it (11.8).
    KeeperFood,
    /// 0x71 of a guest: walk to the zoo entrance and go (11.6).
    Leave,
    /// 0x7a of an animal: walk to the water and drink (11.9).
    DrinkWater,
    /// 100 of a guest: choose an exhibit to see (11.1).
    Site,
    /// 0x70 of a guest: view the animals from the viewing area (11.5).
    Gawk,
}

impl Goal {
    /// The factory (4.2): the base goal for every number outside the table of tasks, the goal of
    /// the task for the ones built so far, nothing for the others (they wait for their issue).
    fn build(task: u8, kind: Kind) -> Option<Goal> {
        match (task, kind) {
            (..=99, _) | (135.., _) => Some(Goal::Base),
            (HABITAT, Kind::Guest) => Some(Goal::Habitat),
            (KEEPER_FOOD, Kind::Animal) => Some(Goal::KeeperFood),
            (LEAVE, Kind::Guest) => Some(Goal::Leave),
            (DRINK, Kind::Animal) => Some(Goal::DrinkWater),
            (SITE, Kind::Guest) => Some(Goal::Site),
            (GAWK, Kind::Guest) => Some(Goal::Gawk),
            _ => None,
        }
    }

    fn task(self) -> u8 {
        match self {
            Goal::Base => RANDOM,
            Goal::Habitat => HABITAT,
            Goal::KeeperFood => KEEPER_FOOD,
            Goal::Leave => LEAVE,
            Goal::DrinkWater => DRINK,
            Goal::Site => SITE,
            Goal::Gawk => GAWK,
        }
    }

    /// Called once when the object is built; false clears the record.
    fn init(self, cx: &mut Cx) -> bool {
        self != Goal::Gawk || cx.gawk_init()
    }

    /// Called when a stale object is removed.
    fn abort(self, _: &mut Cx) {}

    /// 4.3: one update of the object.
    fn update(self, cx: &mut Cx) {
        if self == Goal::Base {
            return cx.base();
        }
        if !cx.record().done {
            self.check_done(cx);
        }
        if cx.record().task != self.task() {
            cx.unit.goal.stale = true;
        } else if !cx.record().done {
            self.decide(cx);
        } else {
            self.complete(cx);
        }
    }

    fn check_done(self, cx: &mut Cx) {
        match self {
            Goal::Leave => return cx.leave_check_done(),
            Goal::Gawk => return cx.gawk_check_done(),
            _ => {}
        }
        if self != Goal::Base && cx.reached(false) {
            cx.record().done = true;
        }
    }

    fn decide(self, cx: &mut Cx) {
        match self {
            Goal::Habitat if cx.record().target.tile.is_none() => cx.set_task(NONE, 0, 0, None),
            Goal::KeeperFood => cx.keeper_food_decide(),
            Goal::Leave => cx.leave_decide(),
            Goal::DrinkWater => cx.drink_decide(),
            Goal::Site => cx.site_decide(),
            Goal::Gawk => cx.gawk_decide(),
            _ => {}
        }
    }

    fn complete(self, cx: &mut Cx) {
        match self {
            Goal::Habitat => cx.set_task(GAWK, 0, 0, None),
            Goal::KeeperFood => cx.keeper_food_complete(),
            Goal::Leave => cx.leave_complete(),
            Goal::DrinkWater => cx.drink_complete(),
            Goal::Gawk => cx.gawk_complete(),
            Goal::Base | Goal::Site => {}
        }
    }
}

/// The sub-goal object of a job. Only the base one so far (9.2): every job gets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sub {
    Base,
}

impl Sub {
    fn build(_: u8) -> Option<Sub> {
        Some(Sub::Base)
    }

    /// 9.2.
    fn update(self, cx: &mut Cx) {
        if cx.record().job == 0 || cx.record().job_done {
            return;
        }
        if cx.reached(true) {
            cx.record().job_target.clear();
            cx.record().job_done = true;
        }
        if !cx.record().job_done && !cx.record().job_target.is_set() {
            cx.wander(true);
        }
    }
}

/// The goal state of a unit: the record, the objects built from it and the flags of the base
/// goal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Goals {
    pub record: Record,
    object: Option<Goal>,
    stale: bool,
    sub: Option<Sub>,
    sub_stale: bool,
    /// Raised by the guest's ladder to let an idle unit wander again (7.1).
    pub wants_to_decide: bool,
    /// A guest made a random walk and stands until a draw or its status tick lets it walk again
    /// (7.3); the status tick lowers it (`guest::checks`).
    pub wandered: bool,
}

/// What a goal update works on: the unit taken out of the table for its turn, the world, the
/// managers' context, the unit's place in the entity list and the data of the sets.
pub struct Cx<'a, 'c> {
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
    pub fn record(&mut self) -> &mut Record {
        &mut self.unit.goal.record
    }

    fn now(&self) -> u32 {
        self.world.units.ai_ticks
    }

    /// The tile the unit stands on.
    fn tile(&self) -> Option<usize> {
        let c = self.world.zoo.entities.get(self.at)?.common.as_ref()?;
        self.world
            .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
    }

    /// 3.2: sets the task. The object is marked stale, the job cleared.
    pub fn set_task(&mut self, task: u8, mode: u8, id: u32, tile: Option<usize>) {
        let (now, unit) = (self.now(), self.unit.id);
        // A keeper whose target tile changes gives up the exhibit it was going to.
        if self.unit.goal.record.target.tile != tile {
            self.world.units.release_gate(self.unit);
        }
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
            let width = world.zoo.width as usize;
            let place = tile.map_or("-".into(), |t| format!("{},{}", t % width, t / width));
            let target = if id == 0 { "-".into() } else { id.to_string() };
            let line = format!("goal {unit} {} {mode} {target} {place}", task_name(task));
            world.units.note(line);
        }
        self.set_job(0, 0, None);
    }

    /// 3.2: sets the job and its target; the task is untouched.
    pub fn set_job(&mut self, job: u8, id: u32, tile: Option<usize>) {
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
            world
                .units
                .note(format!("job {} {}", self.unit.id, job_name(job)));
        }
    }

    /// 12.1: clears the record, empties the path, lowers the flags.
    pub fn reset(&mut self) {
        self.set_task(NONE, 0, 0, None);
        self.unit.path.clear();
        self.unit.goal.wants_to_decide = false;
    }

    /// 12.3: empties the path and invalidates the targets; the task stays.
    pub fn abort(&mut self) {
        self.unit.path.clear();
        self.record().target.clear();
        self.record().job_target.clear();
    }

    /// A dying, crated or egg animal runs no goal update (10.1 point 1).
    fn halted(&self) -> bool {
        matches!(&self.unit.part, Part::Animal(a) if a.dying || a.crated || a.egg)
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

    /// 8.2: whether the unit reached the target of the task, or of the job.
    fn reached(&self, job: bool) -> bool {
        let r = &self.unit.goal.record;
        let target = if job { &r.job_target } else { &r.target };
        let Some(c) = self
            .world
            .zoo
            .entities
            .get(self.at)
            .and_then(|e| e.common.as_ref())
        else {
            return false;
        };
        let grid =
            |(x, y, z): (i32, i32, i32)| (x.div_euclid(64), y.div_euclid(64), z.div_euclid(16));
        match (c.movement_mode, target.pos) {
            (1, pos) => pos == Some((c.x, c.y, c.z)),
            (3, pos) => pos.map(grid) == Some(grid((c.x, c.y, c.z))),
            _ => target.tile.is_some() && target.tile == self.tile(),
        }
    }

    /// 6.2 before the object: one draw, and one time in 15 a unit outside the zoo is removed.
    /// (The unit on the footprint of a building is sent out by the building issue.)
    fn common(&mut self) {
        if self.ctx.rng.pick(15) == 0 && !self.tile().is_some_and(|t| self.world.in_zoo(t)) {
            self.ctx.remove(self.world, self.unit.id);
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
            Kind::Animal => self.animal_ladder(task),
            Kind::Guest => self.guest_ladder(task, done),
            _ => {}
        }
        true
    }

    /// 10.2 point 8: the task goes on. Point 10: the guest stands while its wandered flag is up,
    /// else it picks again: the sites to see, else the seen count goes and it may decide to leave.
    /// Point 1 (a guest in an exhibit or a tank leaves it) and point 2 (a guest that is to leave
    /// takes the leave task) come first.
    fn guest_ladder(&mut self, task: u8, done: bool) {
        if self.exit_ladder() || self.leave_ladder() {
            return;
        }
        if task != NONE && task != SITE && !done {
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
            if let Some(goal) = self.unit.goal.object
                && !goal.init(self)
            {
                self.unit.goal.object = None;
                self.set_task(NONE, 0, 0, None);
            }
        }
        if let Some(goal) = self.unit.goal.object {
            goal.update(self);
        }
    }

    fn run_sub(&mut self) {
        if std::mem::take(&mut self.unit.goal.sub_stale) {
            self.unit.goal.sub = None;
        }
        if self.unit.goal.sub.is_none() {
            let job = self.record().job;
            self.unit.goal.sub = Sub::build(job);
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
    /// fewer than two calls left. With more the task stays and the decide draws a new place.
    fn check_done(&mut self) {
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
        if !walk::random_walk(self.unit, self.world, self.ctx.rng, self.at) {
            return;
        }
        if !job && self.unit.kind == Kind::Animal {
            if let Some(dest) = self.unit.destination {
                let world = &*self.world;
                self.unit.goal.record.target.set_pos(world, Some(dest));
            }
        } else {
            self.start_walk(false, job, true);
        }
    }

    /// Asks the finder for a path to tile `to`; on success the unit holds the path and its
    /// destination (the tile's centre at its height, movement 8.1).
    pub fn path_to(&mut self, to: usize, partial: bool) -> bool {
        let Some(here) = self.tile() else {
            return false;
        };
        let mover = crate::unit::movement::mover(self.unit, self.world, self.at, here);
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
    pub fn start_walk(&mut self, run: bool, job: bool, force: bool) -> bool {
        let set = match (self.mode(), run) {
            (1, _) => "bFly",
            (_, true) => "bRun",
            _ => "bWalk",
        };
        self.walk_set(set, job, force)
    }

    /// The same with the walk set the goal names (`bFindKeeperFood`).
    pub fn walk_set(&mut self, set: &str, job: bool, force: bool) -> bool {
        let started =
            !self.unit.suspended() && self.unit.destination.is_some() && self.start_set(set, force);
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
    pub fn start_set(&mut self, name: &str, force: bool) -> bool {
        let entity: &Entity = &self.world.zoo.entities[self.at];
        let env = Env {
            data: self.data,
            unit: entity,
            rng: &mut *self.ctx.rng,
        };
        let (anim, runner) = (&mut self.unit.animation, &mut self.unit.runner);
        runner.start(anim, &env, name, true, force, true)
    }

    /// 8.1: a path to `to`, then the walk.
    pub fn go_to(&mut self, to: usize, run: bool, job: bool, force: bool) -> bool {
        self.path_to(to, false) && self.start_walk(run, job, force)
    }
}
