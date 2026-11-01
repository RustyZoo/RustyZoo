//! Fright and the avoid task 0x75 of guests and staff (goals 10.2 point 3, 11.11, staff 7.1 point
//! 1, tour guide 5). The chase check of a guest or a staff member ([`crate::guest::checks`],
//! [`crate::staff::checks`]) raises its frightened flag when it sees a predator
//! ([`crate::unit::predator`]); its ladder then gives it task 0x75 with the threat as target and
//! keeps the threat as its predator. The goal runs from it in legs, each to the first reachable
//! tile of a ring round the unit, the tiles in the direction away from the threat first, and ends
//! when no predator is seen any more: a guest then leaves the zoo, one that fled into an exhibit
//! leaves it at the end of a leg. Not here: the caught flag of a chase and the animal's own avoid.

use super::{AVOID, Cx, EXIT_TANK};
use crate::exhibit::{EXTERIOR, GROUNDS};
use crate::unit::{Kind, Part, movement, predator, walk};
use std::collections::VecDeque;

impl Cx<'_, '_> {
    /// `find_predator(unit, mode)` (animals 11.8).
    pub(crate) fn predator(&self, mode: u8) -> u32 {
        let (cat, removed) = (self.ctx.cat, &*self.ctx.removals);
        predator::find(self.world, cat, removed, self.unit, self.at, mode)
    }

    /// Guest ladder point 3: a frightened guest that is not avoiding and not in a building task
    /// looks again; with none in sight the flag goes down, else it resets its goal and takes the
    /// avoid task, mode 3, on the threat, which it keeps, pays the emergency cost off the paths and
    /// forgets leaving. Whether the ladder ends here. (No building is a grandstand before the
    /// shows.)
    pub(super) fn fright_ladder(&mut self, task: u8) -> bool {
        if !self.unit.frightened || task == AVOID || super::IN_BUILDING.contains(&task) {
            return false;
        }
        let p = self.predator(0);
        if p == 0 {
            self.unit.frightened = false;
            return true;
        }
        self.reset();
        self.set_task(AVOID, 3, p, None);
        (self.unit.mover.emergency, self.unit.predator) = (true, p);
        if let Part::Guest(g) = &mut self.unit.part {
            g.leave = false;
        }
        true
    }

    /// Staff 7.1 point 1: a frightened member who is not avoiding looks again and, seeing one,
    /// takes the avoid task, mode 3, on it with no goal reset and keeps it; a guide first lets its
    /// group go. The flag goes down either way. Whether the ladder ends here.
    pub(super) fn staff_fright(&mut self, task: u8) -> bool {
        if !self.unit.frightened || task == AVOID {
            return false;
        }
        let p = self.predator(0);
        if p != 0 {
            if self.unit.kind == Kind::Guide {
                self.release();
            }
            self.set_task(AVOID, 3, p, None);
            self.unit.predator = p;
        }
        self.unit.frightened = false;
        true
    }

    /// 11.11 decide: with a threat and no target tile, the unit runs away along the flee path.
    pub(super) fn avoid_decide(&mut self) {
        let r = &self.unit.goal.record;
        if r.target.id != 0 && r.target.tile.is_none() && self.flee() {
            self.walk_set("bRunFromPredator", false, true);
        }
    }

    /// 11.11 check done: when the threat is gone or no predator is seen any more, and the unit is
    /// neither caught nor defending itself, the avoid ends (the goal resets, the flag and the kept
    /// predator go, a guest that is not coming back leaves the zoo). Otherwise, at the end of a
    /// leg, the exit rule and a new leg.
    pub(super) fn avoid_check_done(&mut self) {
        let id = self.record().target.id;
        let gone = self.ctx.removals.contains(&id) || self.world.find(id).is_none();
        let set = self.unit.runner.top().unwrap_or_default();
        let held = set.contains("bCaughtBy") || set.contains("bDefense");
        if (gone || self.predator(0) == 0) && !held {
            self.reset();
            (self.unit.frightened, self.unit.predator) = (false, 0);
            if let Part::Guest(g) = &mut self.unit.part
                && g.returning.is_none()
            {
                g.leave = true;
            }
            return;
        }
        if self.unit.path.len() < 2 {
            if self.record().target.tile.is_some() {
                self.avoid_exit();
            }
            let world = &*self.world;
            self.unit.goal.record.target.set_tile(world, None);
        }
    }

    /// The exit rule (11.11, also the complete action): a guest standing in an exhibit or a tank
    /// stops everything and takes the exit task, mode 0, with thought 10568.
    pub(super) fn avoid_exit(&mut self) {
        let region = self.tile().and_then(|t| self.world.exhibits().region(t));
        if self.unit.kind != Kind::Guest || !region.is_some_and(|r| r != GROUNDS && r != EXTERIOR) {
            return;
        }
        self.reset();
        self.unit.runner = Default::default();
        movement::stop(self.unit);
        self.think(10568, None);
        self.set_task(EXIT_TANK, 0, 0, None);
        self.unit.frightened = false;
    }

    /// The flee path (`getPathAwayFromTarget`): on the border of the square of radius
    /// `cPredatorRadius + 2` round the unit, the tiles its strict tile test accepts, those within an
    /// eighth of a turn of "away from the threat" first (in reverse order), then the others; the
    /// first a whole path reaches wins. The ring shrinks down to 1; then the unit walks at random.
    fn flee(&mut self) -> bool {
        let id = self.record().target.id;
        let from = self
            .world
            .find(id)
            .and_then(|e| crate::guest::view::tile_of(self.world, e));
        let (Some(from), Some(here)) = (from, self.tile()) else {
            return false;
        };
        let away = movement::direction(self.world, from, here);
        let width = i64::from(self.world.zoo.width);
        let (x, y) = (here as i64 % width, here as i64 / width);
        // Beyond the map's side a ring holds no tile.
        let side = width.max(i64::from(self.world.zoo.height));
        let mut r = (predator::radius(self.ctx.cat) + 2).min(side);
        while r >= 1 {
            let mut cand = VecDeque::new();
            for ox in -r..=r {
                let column: Vec<i64> = match ox.abs() == r {
                    true => (-r..=r).collect(),
                    false => vec![-r, r],
                };
                for oy in column {
                    let p = self.world.tile_index((x + ox) as i32, (y + oy) as i32);
                    let Some(p) = p.filter(|&p| walk::fits(self.unit, self.world, here, p, false))
                    else {
                        continue;
                    };
                    match (away - movement::direction(self.world, here, p)).abs() {
                        0 | 1 | 7 => cand.push_front(p),
                        _ => cand.push_back(p),
                    }
                }
            }
            if cand.into_iter().any(|p| self.path_to(p, false)) {
                return true;
            }
            r -= 1;
        }
        self.unit.path.clear();
        let (unit, world) = (&mut *self.unit, &mut *self.world);
        walk::random_walk(unit, world, self.ctx.cat, self.ctx.rng, self.at)
    }

    /// 11.6 init: an afraid guest thinks 10247 as it starts to leave.
    pub(super) fn leave_init(&mut self) -> bool {
        if self.unit.afraid {
            self.think(10247, None);
        }
        true
    }
}
