//! The marine keeper in a tank (tank staff 4 to 6): entering (task 0x7e) and leaving (0x7f) by
//! the ladders of the tank's gate, and the jobs of both: walking to the foot of the outside ladder
//! (22), climbing it (20), climbing down the inside ladder into the water (29), swimming to the
//! inside ladder (23), climbing it (28), climbing down the outside one (21) and cleaning the
//! water (24, 25). Food in the water is in [`super`]'s jobs 1 and 2. A tank inside a tank (tank
//! staff 5.4) is not served: the keeper goes to the outer tank only.

use super::Cx;
use crate::behaviour::Target;
use crate::build::number;
use crate::goal::{DUTIES, EXIT_TANK};
use crate::path::{self, MAX};
use crate::staff::ladder::{Ladder, Point};
use crate::unit::{movement, swim};
use crate::world::World;

/// The jobs of the tank (goals 5.2).
pub(super) const CLIMB_UP_OUTSIDE: u8 = 20;
pub(super) const CLIMB_DOWN_OUTSIDE: u8 = 21;
const TO_OUTSIDE_LADDER: u8 = 22;
const TO_INSIDE_LADDER: u8 = 23;
pub(super) const CLEAN_TANK: u8 = 24;
const CLEANING_TANK: u8 = 25;
const CLIMB_UP_INSIDE: u8 = 28;
pub(super) const CLIMB_DOWN_INSIDE: u8 = 29;

/// Whether the job is one of the tank's.
pub(super) fn is_tank_job(job: u8) -> bool {
    matches!(job, 20..=25 | 28 | 29)
}

impl Cx<'_, '_> {
    /// The tank of the keeper's own target tile.
    fn target_tank(&self) -> Option<u32> {
        self.own().and_then(|t| self.world.tank_at(t))
    }

    /// The tank the keeper stands in.
    fn here_tank(&self) -> Option<u32> {
        self.tile().and_then(|t| self.world.tank_at(t))
    }

    /// The water level of the tank `x` in elevation steps, 0 for none.
    fn water(&self, x: Option<u32>) -> i32 {
        x.and_then(|x| self.world.tank(x))
            .map_or(0, |t| t.steps().1)
    }

    // ---- 0x7e and 0x7f

    /// 4.1 init: the exhibit of the keeper's target tile must be a tank; then job 22.
    pub(in crate::goal) fn enter_init(&mut self) -> bool {
        let ok = self.target_tank().is_some();
        if ok {
            self.set_job(TO_OUTSIDE_LADDER, 0, None);
        }
        ok
    }

    /// 4.2 init: the tank under him, its inside gate tile his target tile; then job 23.
    pub(in crate::goal) fn exit_init(&mut self) -> bool {
        let inside = self.here_tank().and_then(|x| Ladder::of(self.world, x));
        let Some(inside) = inside.map(|l| l.inside) else {
            return false;
        };
        self.keeper_target(Some(inside));
        self.set_job(TO_INSIDE_LADDER, 0, None);
        true
    }

    /// Done when there is no job; the complete gives him the duties where he stands.
    pub(in crate::goal) fn ladder_task_check_done(&mut self) {
        self.record().done = self.record().job == 0;
    }

    pub(in crate::goal) fn ladder_task_complete(&mut self) {
        self.set_task(DUTIES, 0, 0, None);
    }

    // ---- the jobs

    /// Moves the keeper to the point, facing `d`, into the air layer of a ladder.
    fn climb_to(&mut self, p: Point, d: u32) {
        movement::stop(self.unit);
        self.world.place_unit(self.at, p, d);
        if let Some(c) = self.world.zoo.entities[self.at].common.as_mut() {
            c.movement_mode = 1;
        }
    }

    /// The climb: the destination point and the set that plays it. A type without the set climbs
    /// at once (the job is done).
    fn climb(&mut self, to: Point, set: &str) {
        self.unit.destination = Some(to);
        let world = &*self.world;
        self.unit.goal.record.job_target.set_pos(world, Some(to));
        if !self.start_set(set, false) {
            self.record().job_done = true;
        }
    }

    /// The ladders and the platform of the tank, `None` for no tank or no gate.
    fn gate(&self, tank: Option<u32>) -> Option<Ladder> {
        Ladder::of(self.world, tank?)
    }

    /// 4 init of a job of the tank; false clears the job.
    pub(super) fn tank_init(&mut self, job: u8) -> bool {
        let here = self.here_tank();
        let tank = match job {
            CLIMB_UP_OUTSIDE | TO_OUTSIDE_LADDER => self.target_tank(),
            _ => here,
        };
        if tank.is_none() {
            return false;
        }
        let Some(l) = self.gate(tank) else {
            return job == TO_OUTSIDE_LADDER;
        };
        match job {
            CLIMB_UP_OUTSIDE => self.climb_to(l.out_foot, l.toward_inside),
            CLIMB_DOWN_OUTSIDE => self.climb_to(l.out_top, l.toward_inside),
            CLIMB_UP_INSIDE => self.climb_to(l.in_foot, l.toward_outside),
            CLIMB_DOWN_INSIDE => self.climb_to(l.in_top, l.toward_outside),
            _ => {}
        }
        true
    }

    /// 4 check done.
    pub(super) fn tank_check_done(&mut self, job: u8) {
        let here = self.here_tank();
        let target = self.target_tank();
        let tank = match job {
            CLIMB_UP_OUTSIDE | CLIMB_DOWN_OUTSIDE => target,
            _ => here,
        };
        let gone = match job {
            TO_OUTSIDE_LADDER => false,
            CLEAN_TANK | CLEANING_TANK => self.water(tank) < 1,
            _ => tank.is_none(),
        };
        if gone {
            return self.set_job(0, 0, None);
        }
        let arrived = self.reached(true);
        match job {
            TO_OUTSIDE_LADDER if arrived => {
                let gate = self.gate(self.target_tank());
                match gate.is_some_and(|l| self.tile() == Some(l.outside)) {
                    true => self.record().job_done = true,
                    false => self.record().job_target.clear(),
                }
            }
            TO_INSIDE_LADDER if arrived => {
                let gate = self.gate(here);
                match gate.is_some_and(|l| self.tile() == Some(l.inside)) {
                    true => self.record().job_done = true,
                    false => self.record().job_target.clear(),
                }
            }
            CLEANING_TANK if arrived => {
                let pct = self
                    .who()
                    .and_then(|w| number(w, "cCleanTankPct"))
                    .unwrap_or(10.0);
                if let Some(x) = here {
                    let purity = self.world.tank(x).map_or(0, |t| i64::from(t.purity));
                    self.world.set_purity(x, purity + pct as i64);
                }
                self.record().job_done = true;
            }
            CLEAN_TANK | CLIMB_UP_OUTSIDE | CLIMB_DOWN_OUTSIDE | CLIMB_UP_INSIDE
            | CLIMB_DOWN_INSIDE
                if arrived =>
            {
                self.record().job_done = true;
            }
            _ => {}
        }
    }

    /// 4 and 6 decide, with no job target yet.
    pub(super) fn tank_decide(&mut self, job: u8) {
        let here = self.here_tank();
        match job {
            TO_OUTSIDE_LADDER => self.walk_to_outside_ladder(),
            CLIMB_UP_OUTSIDE => {
                if let Some(l) = self.gate(self.target_tank()) {
                    self.climb(l.out_top, "bClimbUp");
                }
            }
            CLIMB_DOWN_OUTSIDE => {
                if let Some(l) = self.gate(self.target_tank()) {
                    self.climb(l.out_foot, "bClimbDown");
                }
            }
            CLIMB_UP_INSIDE => {
                if let Some(l) = self.gate(here) {
                    self.climb(l.in_top, "bClimbUp");
                }
            }
            CLIMB_DOWN_INSIDE => {
                if let Some(l) = self.gate(here) {
                    self.climb(l.in_surface, "bClimbDown");
                }
            }
            TO_INSIDE_LADDER => self.swim_to_inside_ladder(here),
            CLEAN_TANK => self.clean_decide(here, false),
            _ => self.clean_decide(here, true),
        }
    }

    /// 4.1 job 22: the way to the tile outside the gate; with none a random walk and the job
    /// stays.
    fn walk_to_outside_ladder(&mut self) {
        let outside = self.gate(self.target_tank()).map(|l| l.outside);
        if outside.is_some_and(|o| self.go_job(o)) {
            return;
        }
        self.wander(true);
    }

    /// 4.2 job 23: the way to the tile inside the gate at the water surface, none when he stands
    /// on it.
    fn swim_to_inside_ladder(&mut self, tank: Option<u32>) {
        let Some(l) = self.gate(tank) else {
            return self.set_job(0, 0, None);
        };
        if self.tile() == Some(l.inside) {
            let here = self.world.zoo.entities[self.at]
                .common
                .as_ref()
                .map(|c| (c.x, c.y, c.z));
            let world = &*self.world;
            self.unit.goal.record.job_target.set_pos(world, here);
            return;
        }
        let surface = self.world.zoo.tiles[l.inside].elevation + self.water(tank);
        if !(self.path_to_level(l.inside, false, Some(surface))
            && self.start_walk(false, true, false))
        {
            self.set_job(0, 0, None);
        }
    }

    /// 6 jobs 24 and 25: a tile of the tank and a depth, and the way there; job 25 sweeps with
    /// the cleaning set of his depth.
    fn clean_decide(&mut self, tank: Option<u32>, sweeping: bool) {
        let (Some(x), Some(here)) = (tank, self.tile()) else {
            return self.set_job(0, 0, None);
        };
        let tiles = self.world.tiles_of(x);
        let mover = movement::mover(self.unit, self.world, self.at, here);
        let behind = self.facing_behind();
        let ahead = |t: usize| {
            let d = movement::direction(self.world, here, t);
            d < 0 || ![0, 1, 7].contains(&(d + 8 - behind).rem_euclid(8))
        };
        let open: Vec<usize> = match sweeping {
            true => Vec::new(),
            false => (tiles.iter().copied())
                .filter(|&t| path::terrain(self.world, &mover, t, None) < MAX && ahead(t))
                .collect(),
        };
        let from = if open.is_empty() { &tiles } else { &open };
        if from.is_empty() {
            return self.set_job(0, 0, None);
        }
        let tile = from[self.ctx.rng.pick(from.len() as u32) as usize];
        self.unit.target = if sweeping {
            Target::Level
        } else {
            Target::None
        };
        let level = self.level_in(tile);
        let walking = self.path_to_level(tile, false, Some(level));
        let set = match (sweeping, self.unit.tank) {
            (false, _) => "bWalk",
            (true, 3) => "bCleanTankSurf",
            (true, 4) => "bCleanTankSub",
            _ => return self.set_job(0, 0, None),
        };
        if !(walking && self.walk_set(set, true, false)) {
            self.set_job(0, 0, None);
        }
    }

    /// The direction behind the keeper.
    fn facing_behind(&self) -> i32 {
        let facing = self.world.zoo.entities[self.at]
            .common
            .as_ref()
            .map_or(0, |c| c.rotation);
        (facing as i32 + 4) % 8
    }

    /// The level a destination in `tile` gets for the keeper (tank fauna 6.2).
    pub(super) fn level_in(&mut self, tile: usize) -> i32 {
        let z = self.world.zoo.entities[self.at]
            .common
            .as_ref()
            .map_or(0, |c| c.z);
        swim::level_of(self.unit, self.world, self.ctx.rng, tile, z.div_euclid(16))
    }

    /// 4 and 6 complete.
    pub(super) fn tank_complete(&mut self, job: u8) {
        let here = self.here_tank();
        let task = self.record().task;
        match job {
            TO_OUTSIDE_LADDER => self.set_job(CLIMB_UP_OUTSIDE, 0, None),
            CLIMB_UP_OUTSIDE => {
                if let Some(l) = self.gate(self.target_tank()) {
                    movement::stop(self.unit);
                    self.world.place_unit(self.at, l.platform, l.toward_inside);
                }
                self.set_job(0, 0, None);
            }
            TO_INSIDE_LADDER if self.unit.tank != 2 => self.set_job(CLIMB_UP_INSIDE, 0, None),
            TO_INSIDE_LADDER if task == EXIT_TANK => self.set_job(CLIMB_DOWN_OUTSIDE, 0, None),
            CLIMB_UP_INSIDE if task == EXIT_TANK && here.is_some() => {
                self.set_job(CLIMB_DOWN_OUTSIDE, 0, None);
            }
            CLEANING_TANK => {
                let threshold = self.who().and_then(|w| number(w, "cCleanTankThreshold"));
                let purity = here
                    .and_then(|x| self.world.tank(x))
                    .map_or(0, |t| t.purity);
                match i64::from(purity) < threshold.map_or(100, |v| v as i64) {
                    true => self.set_job(CLEAN_TANK, 0, None),
                    false => self.set_job(0, 0, None),
                }
            }
            CLEAN_TANK => self.set_job(CLEANING_TANK, 0, None),
            _ => self.set_job(0, 0, None),
        }
    }

    /// 5.3: the place for a pile of food in the water and the way there. The job ends when no
    /// tile of the tank is free. Returns the tile and the level of the pile.
    pub(super) fn tank_food_place(&mut self, x: u32, here: usize) -> Option<(usize, i32)> {
        use crate::panel::exhibit::animals_in;
        let animals = animals_in(self.world, x);
        let n = animals.len() as u32;
        let drawn = (n > 0).then(|| animals[self.ctx.rng.pick(n) as usize]);
        let (lost, _) = self.lost();
        // The cost of a tile is the drawn animal's; with none, his own.
        let mover = food_mover(self.world, drawn)
            .unwrap_or_else(|| movement::mover(self.unit, self.world, self.at, here));
        let places = food_places(self.world, x, &mover, &lost);
        let n = places.len() as u32;
        if n == 0 {
            return None;
        }
        let tile = places[self.ctx.rng.pick(n) as usize];
        let top = self.water(Some(x));
        let depth = self
            .food_type()
            .and_then(|(_, ty)| number(ty, "cDepth"))
            .unwrap_or(0.0) as i32;
        let d = match depth < top {
            true => depth + self.ctx.rng.pick((top - depth) as u32) as i32,
            false => 0,
        };
        Some((tile, self.world.zoo.tiles[tile].elevation + d))
    }
}

/// The path finder's view of the animal at entity `at`: whose terrain costs choose the place of
/// the food (5.3).
fn food_mover(world: &World, at: Option<usize>) -> Option<path::Mover> {
    let at = at?;
    let c = world.zoo.entities.get(at)?.common.as_ref()?;
    let here = world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))?;
    Some(movement::mover(world.units.get(c.id)?, world, at, here))
}

/// 5.3: the tiles of tank `x` where a pile can go: not listed as lost, with the four quarter
/// holders and the tile free of units, and open (cost below 20) to the unit of `mover`.
fn food_places(world: &World, x: u32, mover: &path::Mover, lost: &[usize]) -> Vec<usize> {
    let free = |t: &usize| {
        !lost.contains(t)
            && (0..4).all(|q| world.quadrant(*t, q).is_none())
            && !world.unit_on(*t)
            && path::terrain(world, mover, *t, None) < MAX
    };
    world.tiles_of(x).into_iter().filter(free).collect()
}

#[cfg(test)]
mod tests;
