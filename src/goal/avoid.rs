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

    /// Whether predator `p` is an animal on a rampage (shows 30.3 point 3).
    pub(crate) fn rampaging(&self, p: u32) -> bool {
        let part = self.world.units.get(p).map(|u| &u.part);
        matches!(part, Some(Part::Animal(a)) if a.rampaging)
    }

    /// Guest ladder point 3: a frightened guest that is not avoiding looks again, unless it is in
    /// a building task whose building is not a grandstand. With none in sight the flag goes down.
    /// A guest bound for a grandstand (its building target, walking or inside) never avoids: a
    /// rampaging threat empties the grandstand and the ladder ends (shows 30.3 point 3). Else it
    /// resets its goal and takes the avoid task, mode 3, on the threat, which it keeps, pays the
    /// emergency cost off the paths and forgets leaving. Whether the ladder ends here.
    pub(super) fn fright_ladder(&mut self, task: u8) -> bool {
        if !self.unit.frightened || task == AVOID {
            return false;
        }
        let building = match &self.unit.part {
            Part::Guest(g) => g.building,
            _ => 0,
        };
        let stand = crate::show::is_grandstand(self.world, self.ctx.cat, building);
        if super::IN_BUILDING.contains(&task) && !stand {
            return false;
        }
        let p = self.predator(0);
        if p == 0 {
            self.unit.frightened = false;
            return true;
        }
        if stand {
            if self.rampaging(p) {
                crate::show::empty_grandstands_for(self.world, self.ctx.cat, self.unit, building);
            }
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

    /// Guest ladder point 3, last paragraph: a caught guest that is not avoiding, whatever its fright
    /// (also one in a building task), keeps the predator it has or takes the one in sight, resets its
    /// goal and takes the avoid task, mode 3, on it (0 when none) with the emergency cost. Whether
    /// it did.
    pub(super) fn caught_ladder(&mut self, task: u8) -> bool {
        if !self.unit.caught || task == AVOID {
            return false;
        }
        if self.unit.predator == 0 {
            self.unit.predator = self.predator(0);
        }
        self.reset();
        self.set_task(AVOID, 3, self.unit.predator, None);
        self.unit.mover.emergency = true;
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

    /// 11.11 init: an animal's defence flags start down (animals 20.4 point 4).
    pub(super) fn avoid_init(&mut self) -> bool {
        if let Part::Animal(a) = &mut self.unit.part {
            a.pre = [false; 2];
        }
        true
    }

    /// 11.11 decide: with a threat and no target tile, the unit runs away along the flee path. A
    /// guest that is caught plays its set instead; an animal then defends itself (animals 20.6).
    pub(super) fn avoid_decide(&mut self) {
        if self.unit.caught && self.unit.kind == Kind::Guest {
            return;
        }
        let r = &self.unit.goal.record;
        if r.target.id != 0 && r.target.tile.is_none() && self.flee() {
            self.walk_set("bRunFromPredator", false, true);
        }
        if self.unit.kind == Kind::Animal {
            self.defence();
        }
    }

    /// `avoid_done(T)` (11.11): in a friendly chase the avoid lasts `cChaseTimeOut` AI ticks from
    /// the chase stamp; otherwise it lasts while a threat or a chasing playmate is in sight.
    fn avoid_done(&mut self, other: Option<u32>) -> bool {
        let friendly = self.unit.kind == Kind::Animal && other.is_some_and(|t| self.same_type(t));
        if let (true, Part::Animal(a)) = (friendly, &self.unit.part) {
            let out = self.ctx.cat.chase_time_out.unwrap_or(5);
            return self.now().wrapping_sub(a.stamp) as i32 >= out;
        }
        self.predator(0) == 0 && self.predator(1) == 0
    }

    /// 11.11 check done: when the threat is gone or no predator is seen any more, and the unit is
    /// neither caught nor defending itself, the avoid ends (the goal resets, the flag and the kept
    /// predator go, a hunter that chased it for fun lets go of it, a guest that is not coming back
    /// leaves the zoo). A caught unit waits for its set to end. Otherwise, at the end of a leg, the
    /// exit rule and a new leg.
    pub(super) fn avoid_check_done(&mut self) {
        let id = self.record().target.id;
        let gone = self.ctx.removals.contains(&id) || self.world.find(id).is_none();
        let set = self.unit.runner.top().unwrap_or_default();
        let held = set.contains("bCaughtBy") || set.contains("bDefense");
        let over = match self.unit.caught {
            true => !held,
            false => (gone || self.avoid_done((!gone).then_some(id))) && !held,
        };
        if over {
            let friend = !gone && self.unit.kind == Kind::Animal && self.same_type(id);
            let me = self.unit.id;
            self.reset();
            if self.unit.caught {
                self.unit.caught = false;
            } else if friend
                && let Some(Part::Animal(t)) = self.world.units.get_mut(id).map(|t| &mut t.part)
                && t.prey == me
            {
                t.prey = 0;
            }
            (self.unit.frightened, self.unit.predator) = (false, 0);
            if let Part::Guest(g) = &mut self.unit.part
                && g.returning.is_none()
            {
                g.leave = true;
            }
            return;
        }
        if !self.unit.caught && self.unit.path.len() < 2 {
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
        self.stop_and_exit();
    }

    /// Stops everything (goal reset, sets and pending set dropped, movement stopped), calms the
    /// unit, thinks 10568 and takes the exit task, mode 0 (goals 10.2 point 1, 11.11).
    pub(super) fn stop_and_exit(&mut self) {
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
        // The ring is `cPredatorRadius + 2` in 32 bits; beyond the map's side it holds no tile, and
        // a negative one is empty. The first ring is looked at before the `R < 1` test.
        let side = width.max(i64::from(self.world.zoo.height)) as i32;
        let mut r = predator::radius(self.ctx.cat).wrapping_add(2).min(side);
        loop {
            let mut cand = VecDeque::new();
            let r64 = i64::from(r);
            for ox in if r < 0 { 0..0 } else { -r64..r64 + 1 } {
                let column: Vec<i64> = match ox.abs() == r64 {
                    true => (-r64..=r64).collect(),
                    false => vec![-r64, r64],
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
            r = r.saturating_sub(1);
            if r < 1 {
                break;
            }
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
