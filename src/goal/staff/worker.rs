//! The goals of a maintenance worker (staff 8): the duties (task 0x78) choose a task of the pool
//! and its job, and the jobs go to the trash can and empty it (9, 10), go to the litter and
//! sweep it (7, 8), go to dung or food and bag it (11, 12), go to a worn fence and mend it (16,
//! 17), and go to a worn tank filter and service it (26, 27, tank staff 8). The worker's target
//! tile is the task's, kept in [`Staff::target`] (staff 8.3 step 3).

use super::super::{Cx, MAINT, NONE, RANDOM};
use crate::build::number;
use crate::path::{self, MAX};
use crate::staff::pool::{self, Task};
use crate::staff::{chores, duty_kinds, service};
use crate::unit::movement;

/// The jobs after the first of a task.
const SWEEP: u8 = 8;
const EMPTY: u8 = 10;
const BAG: u8 = 12;
const FIX: u8 = 17;
const FIX_FILTER: u8 = 27;
/// The milliseconds between two clearings of the unreachable lists (staff 4.3).
const CLEARING: i32 = 10_000;

impl Cx<'_, '_> {
    /// 8.1: the worker with no task, or whose task is done, takes the maintenance duties. Before
    /// anything the countdown of his unreachable lists runs (staff 4.3).
    pub(in crate::goal) fn worker_ladder(&mut self, task: u8, done: bool) {
        let every = (self.who())
            .and_then(|w| number(w, "cClearInvalidListInterval"))
            .map_or(CLEARING, |n| n as i32);
        let step = i32::try_from(self.ctx.step).unwrap_or(i32::MAX);
        if let Some(s) = self.staff() {
            s.clear = s.clear.saturating_sub(step);
            if s.clear <= 0 {
                (s.clear, s.lost_tiles) = (every, Vec::new());
            }
        }
        if task == NONE || done {
            self.set_task(MAINT, 4, 0, None);
        }
    }

    /// Whether the worker stands within one tile of tile `t` (8 neighbours and itself).
    fn near(&self, t: usize) -> bool {
        let (w, here) = (self.world.zoo.width as i32, self.tile());
        let at = |t: usize| (t as i32 % w, t as i32 / w);
        here.is_some_and(|h| (at(h).0 - at(t).0).abs() <= 1 && (at(h).1 - at(t).1).abs() <= 1)
    }

    /// The task the worker holds in the pool.
    fn held(&mut self) -> Option<Task> {
        let id = self.staff()?.task;
        self.world.units.pool.get(id).cloned()
    }

    /// The task and the job are over: the task leaves the pool, the target and the job go.
    fn work_finish(&mut self) {
        if let Some(s) = self.staff() {
            let task = std::mem::take(&mut s.task);
            s.target = None;
            self.world.units.pool.finish(task);
        }
        self.set_job(0, 0, None);
    }

    /// The job failed (staff 8.4): the target and the job go, the task stays in the pool for anyone.
    fn work_fail(&mut self) {
        let id = self.unit.id;
        self.world.units.pool.unlink(id);
        if let Some(s) = self.staff() {
            (s.task, s.target) = (0, None);
        }
        self.set_job(0, 0, None);
    }

    /// A tile the worker could not reach goes on his list.
    fn list(&mut self, tile: usize) {
        if let Some(s) = self.staff() {
            s.lost_tiles.push(tile);
        }
    }

    /// Whether the worker's type has `key` above 0.
    fn raised(&self, key: &str) -> bool {
        self.who().and_then(|w| number(w, key)).unwrap_or(0.0) > 0.0
    }

    /// The worker's reset (goals 12.1) gives his task back.
    pub(in crate::goal) fn worker_reset(&mut self) {
        let id = self.unit.id;
        self.world.units.pool.unlink(id);
        if let Some(s) = self.staff() {
            (s.task, s.target) = (0, None);
        }
    }

    // ---- 0x78, the choice of a task

    /// 8.3: with no job, the nearest task he may do; with none, the wander.
    pub(in crate::goal) fn maint_decide(&mut self) {
        if self.record().job != 0 {
            return;
        }
        let Some(best) = self.best_task() else {
            self.set_task(RANDOM, 0, 0, None);
            // The worker's duty kinds are drawn and read by nothing (O12).
            for _ in 0..2 {
                self.ctx.rng.pick(3);
            }
            return;
        };
        let id = self.unit.id;
        if best.worker != 0 && best.worker != id {
            self.lose_task(best.worker);
        }
        let previous = self.staff().map_or(0, |s| s.task);
        let pool = &mut self.world.units.pool;
        pool.finish(previous);
        if let Some(t) = pool.tasks.iter_mut().find(|t| t.id == best.id) {
            t.worker = id;
        }
        if let Some(s) = self.staff() {
            (s.task, s.target) = (best.id, Some(best.tile));
        }
        self.set_job(best.job, 0, None);
    }

    /// 8.3 step 2: the pool in order; a task he may do (its kind allowed and switched on, its tile
    /// not listed), nearer than the best so far, and not held by a worker who is nearer.
    fn best_task(&mut self) -> Option<Task> {
        let (id, here) = (self.unit.id, self.tile()?);
        let (allowed, lost) = (duty_kinds(self.ctx.cat), self.lost().0);
        let off = self.staff()?.off;
        let mut best: Option<(&Task, i64)> = None;
        for t in &self.world.units.pool.tasks {
            let kind = match t.job {
                pool::TRASH_CAN => 0,
                pool::TRASH_PILE | pool::ZOO_DOO => 1,
                pool::GO_TO_FENCE => 2,
                pool::GO_TO_FILTER => 3,
                _ => continue,
            };
            if kind >= allowed || off[kind] || lost.contains(&t.tile) {
                continue;
            }
            if best.is_some_and(|(b, _)| b.urgent && !t.urgent) {
                break;
            }
            let d = service::distance(self.world, here, t.tile);
            if best.is_some_and(|(_, near)| d >= near) {
                continue;
            }
            let holder = self.world.units.get(t.worker).filter(|_| t.worker != id);
            let other = holder.and_then(|u| self.world.find(u.id));
            let other = other.and_then(|e| self.tile_of_entity(e));
            if other.is_some_and(|o| service::distance(self.world, o, t.tile) <= d) {
                continue;
            }
            best = Some((t, d));
        }
        best.map(|(t, _)| t.clone())
    }

    /// Worker `id` loses the task another one takes: his link and his goal go.
    fn lose_task(&mut self, id: u32) {
        if let Some(other) = self.world.units.get_mut(id) {
            crate::goal::lose_task(other);
        }
    }

    // ---- the jobs

    /// The set a job plays.
    fn work_set(job: u8) -> &'static str {
        match job {
            SWEEP => "bSweep",
            EMPTY => "bEmpty",
            _ => "bFix",
        }
    }

    /// The init of a job: the sweep, the emptying and the mending start their set where the worker
    /// stands, at what is there to do; false clears the job.
    pub(in crate::goal) fn work_init(&mut self, job: u8) -> bool {
        if !matches!(job, SWEEP | EMPTY | BAG | FIX | FIX_FILTER) {
            return true;
        }
        let Some(tile) = self.own() else {
            self.work_fail();
            return false;
        };
        // The bag starts where he stands, at what lies in the centre slot; an empty slot starts
        // nothing (staff 8.6).
        if job == BAG {
            movement::stop(self.unit);
            self.unit.destination = None;
            let set = match chores::centre(self.world, tile) {
                Some((_, true, _)) => "bBagDino",
                Some(_) => "bBag",
                None => {
                    self.record().job_done = true;
                    return true;
                }
            };
            if !self.start_set(set, false) {
                self.record().job_done = true;
            }
            return true;
        }
        // The service starts where he stands and sets no facing (tank staff 8).
        if job == FIX_FILTER {
            movement::stop(self.unit);
            self.unit.destination = None;
            if !self.start_set(Self::work_set(job), false) {
                self.record().job_done = true;
            }
            return true;
        }
        let cat = self.ctx.cat;
        let ready = match job {
            EMPTY => chores::trash_in(self.world, cat, tile) > 0,
            FIX => {
                let found = chores::decayed_near(self.world, cat, tile);
                self.record().job_target.id = found.map_or(0, |f| f.0);
                found.is_some()
            }
            _ => true,
        };
        if !ready {
            self.work_finish();
            return false;
        }
        movement::stop(self.unit);
        self.unit.destination = None;
        self.face(tile);
        if !self.start_set(Self::work_set(job), false) {
            self.record().job_done = true;
        }
        true
    }

    /// Check done of a job: arrived, or the set has ended.
    pub(in crate::goal) fn work_check_done(&mut self, job: u8) {
        let Some(tile) = self.own() else {
            return;
        };
        let cat = self.ctx.cat;
        match job {
            // Nothing is tested while he walks: only once he stands on the tile his walk targets.
            pool::TRASH_CAN if self.reached(true) => {
                if self.near(tile) && chores::trash_in(self.world, cat, tile) > 0 {
                    self.record().job_done = true;
                } else {
                    self.work_finish();
                }
            }
            // No dung left anywhere ends the walk, even to food an emptied exhibit left (staff 8.6).
            pool::ZOO_DOO if !chores::poo_left(self.world) => self.work_finish(),
            pool::TRASH_PILE | pool::ZOO_DOO | pool::GO_TO_FENCE if self.tile() == Some(tile) => {
                self.record().job_done = true;
            }
            pool::TRASH_PILE if !chores::litter_left(self.world) => self.work_finish(),
            pool::GO_TO_FILTER if self.reached(true) => self.record().job_done = true,
            BAG => {
                let top = self.unit.runner.top();
                let bagging = matches!(top, Some("bBag" | "bBagDino"));
                self.record().job_done |= !(bagging && self.unit.runner.running());
            }
            SWEEP | EMPTY | FIX | FIX_FILTER => {
                let set = Self::work_set(job);
                let running = self.unit.runner.top() == Some(set) && self.unit.runner.running();
                self.record().job_done |= !running;
            }
            _ => {}
        }
    }

    /// Decide of a job with no target yet: the way to the tile of the task.
    pub(in crate::goal) fn work_decide(&mut self, job: u8) {
        if self.record().job_target.tile.is_some() {
            return;
        }
        let Some(tile) = self.own() else {
            return self.work_fail();
        };
        let urgent = self.held().is_some_and(|t| t.urgent);
        let cat = self.ctx.cat;
        match job {
            pool::TRASH_CAN => return self.can_decide(tile),
            pool::GO_TO_FILTER => return self.filter_decide(tile, urgent),
            // An exhibit with animals again is the keepers' work: the task is void.
            pool::ZOO_DOO if self.keepers_own(tile) => return self.work_finish(),
            pool::GO_TO_FENCE => match chores::decayed_near(self.world, cat, tile) {
                Some((piece, _)) => self.record().job_target.id = piece,
                // A piece mended since the event leaves nothing to do: the task is void.
                None => return self.work_finish(),
            },
            _ => {}
        }
        if !self.go_work(tile, urgent) {
            self.list(tile);
            self.work_fail();
        }
    }

    /// Whether tile `t` lies in an exhibit that holds animals.
    fn keepers_own(&self, t: usize) -> bool {
        let x = self.world.exhibits().region(t);
        let x = x.filter(|&x| x != crate::exhibit::GROUNDS && x != crate::exhibit::EXTERIOR);
        x.is_some_and(|x| !crate::panel::exhibit::animals_in(self.world, x).is_empty())
    }

    /// 8.4 job 9: the way to the can, else to one of the four tiles beside it, drawn.
    fn can_decide(&mut self, tile: usize) {
        if self.go_work(tile, false) {
            return;
        }
        self.list(tile);
        let sides: Vec<usize> = (0..8)
            .step_by(2)
            .filter_map(|d| self.world.neighbour(tile, d))
            .collect();
        let n = sides.len() as u32;
        let side = (n > 0).then(|| sides[self.ctx.rng.pick(n) as usize]);
        if side.is_some_and(|s| self.go_work(s, false)) {
            return;
        }
        if let Some(s) = side {
            self.list(s);
        }
        self.work_fail();
    }

    /// 8 job 26: the way to the first side of the filter that is dry land and open to him; a
    /// filter with none goes on his list.
    fn filter_decide(&mut self, tile: usize, urgent: bool) {
        let Some(here) = self.tile().filter(|_| !self.lost().0.contains(&tile)) else {
            return self.work_fail();
        };
        let mover = movement::mover(self.unit, self.world, self.at, here);
        let sides: Vec<usize> = (0..8)
            .step_by(2)
            .filter_map(|d| self.world.neighbour(tile, d))
            .collect();
        for side in sides {
            let world = &*self.world;
            let dry = world.tank_at(side).is_none() && !world.is_water(side);
            if dry && path::terrain(world, &mover, side, None) < MAX && self.go_work(side, urgent) {
                return;
            }
        }
        self.list(tile);
        self.work_fail();
    }

    /// A path to tile `to` and the walk, running when `run`.
    fn go_work(&mut self, to: usize, run: bool) -> bool {
        self.unit.path.clear();
        self.path_to(to, false) && self.start_walk(run, true, false)
    }

    /// Complete of a job: the next job of the task, or the work itself and the end of the task.
    pub(in crate::goal) fn work_complete(&mut self, job: u8) {
        let Some(tile) = self.own() else {
            return self.work_fail();
        };
        let id = self.record().job_target.id;
        match job {
            pool::TRASH_PILE => self.set_job(SWEEP, 0, Some(tile)),
            pool::TRASH_CAN => self.set_job(EMPTY, 0, Some(tile)),
            pool::ZOO_DOO => self.set_job(BAG, 0, Some(tile)),
            pool::GO_TO_FENCE => self.set_job(FIX, id, Some(tile)),
            pool::GO_TO_FILTER => self.set_job(FIX_FILTER, 0, Some(tile)),
            FIX_FILTER => {
                self.service(tile);
                self.work_finish();
            }
            SWEEP => {
                self.sweep(tile);
                self.work_finish();
            }
            EMPTY => {
                chores::empty(self.world, self.ctx.cat, tile);
                self.work_finish();
            }
            BAG => {
                self.bag(tile);
                self.work_finish();
            }
            _ => {
                self.mend(id);
                self.work_finish();
            }
        }
    }

    /// 8.6 job 12, and the keeper's raking (7.8): what lies in the centre slot goes, the exhibit is
    /// rated again and the compost pays unless it is food.
    pub(in crate::goal) fn bag(&mut self, tile: usize) {
        let held = chores::centre(self.world, tile);
        let Some((id, dino, food)) = held.filter(|h| !self.ctx.removals.contains(&h.0)) else {
            return;
        };
        self.ctx.remove(self.world, id);
        self.world.mark_rating(tile);
        if !food {
            chores::recycle(self.world, self.ctx.cat, tile, dino);
        }
    }

    /// 8 job 27: the filter in the centre slot of the tile is serviced.
    fn service(&mut self, tile: usize) {
        let filter = self.world.quadrant(tile, 3);
        let id = filter.and_then(|e| self.world.zoo.entities[e].common.as_ref().map(|c| c.id));
        if let Some(id) = id {
            self.world.service_filter(self.ctx.cat, id);
        }
    }

    /// 8.5 job 8: every piece of litter on the tile and its sides, or all its neighbours, goes.
    fn sweep(&mut self, tile: usize) {
        let wide = self.raised("cCleanTrashRadius");
        for t in chores::swept(self.world, tile, wide) {
            for id in chores::litter_in(self.world, t) {
                self.ctx.remove(self.world, id);
            }
        }
    }

    /// 8.7 job 17: the piece and the one across become healthy, and with `cFixFenceModifier` the two
    /// lines beside; each is told.
    fn mend(&mut self, piece: u32) {
        let (cat, wide) = (self.ctx.cat, self.raised("cFixFenceModifier"));
        let Some(at) = self.world.find(piece) else {
            return;
        };
        for id in chores::mended(self.world, at, wide) {
            let rng = &mut *self.ctx.rng;
            if self.world.heal(cat, id, &mut || rng.rand()).is_none() {
                continue;
            }
            let life = (self.world.find(id))
                .and_then(|e| self.world.zoo.entities[e].common.as_ref()?.fence)
                .map_or(0, |f| f.life);
            self.world.units.world_note(format!("fence {id} {life}"));
        }
    }
}

#[cfg(test)]
mod tests;
