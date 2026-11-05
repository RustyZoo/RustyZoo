//! The hunt of an animal (animals 20): the ladder rungs that start the avoid (0x75) and the
//! chase (0x74), the chase goal, the catch it ends in and the preattack job (job 18) a hunter and
//! its prey fight with. The avoid goal of an animal is [`super::avoid`].

use super::dropped::with_unit;
use super::{AVOID, CHASE, Cx, IN_BUILDING};
use crate::animal::Animal;
use crate::guest::view::tile_of;
use crate::place::bulldozer::type_of;
use crate::show::in_show_now;
use crate::unit::predator::find_prey;
use crate::unit::{Kind, Part, movement};

/// The job of the preattack and of the defence.
pub(super) const PREATTACK: u8 = 18;
/// What a hunter is told when it catches a guest (animals 20.3 point 3).
const THOUGHT: i32 = 10246;

impl Cx<'_, '_> {
    /// The animal's own fields; none for any other unit.
    fn hunter(&mut self) -> Option<&mut Animal> {
        match &mut self.unit.part {
            Part::Animal(a) => Some(a),
            _ => None,
        }
    }

    /// Runs `f` on the animal's fields.
    fn mark(&mut self, f: impl FnOnce(&mut Animal)) {
        if let Some(a) = self.hunter() {
            f(a);
        }
    }

    /// 9.3: the animal is doing something (the rampage sub-state is not kept).
    pub(crate) fn occupied(&self) -> bool {
        let Part::Animal(a) = &self.unit.part else {
            return false;
        };
        let task = self.unit.goal.record.task;
        let set = matches!(
            self.unit.runner.top(),
            Some("bSick" | "bSleep" | "bEat" | "bTestFence")
        );
        let tasks = matches!(
            task,
            102..=106 | 114 | 115 | 122 | 124 | 128 | 129 | 133 | 134
        );
        self.unit.caught
            || set
            || tasks
            // In a show, as it is now (shows 32.3 point 3).
            || in_show_now(self.world, self.ctx.cat, self.unit, self.at)
            || a.thirsty
            || a.wants_food
            || a.caught_prey
            || a.dying
            || a.drinking
    }

    /// The target unit of the record: its id and place in the entity list, none
    /// when it names no unit or a removed one.
    fn target_unit(&self) -> Option<(u32, usize)> {
        let id = self.unit.goal.record.target.id;
        let at = self
            .world
            .find(id)
            .filter(|_| !self.ctx.removals.contains(&id))?;
        self.world.units.get(id).map(|_| (id, at))
    }

    /// The species key `key` of the entity at `at` is exactly 1.
    fn flag(&self, at: usize, key: &str) -> bool {
        type_of(self.ctx.cat, &self.world.zoo.entities[at])
            .and_then(|t| crate::build::number(t, key))
            .is_some_and(|v| v as i32 == 1)
    }

    /// The unit `other` is of the hunter's own type id.
    pub(super) fn same_type(&self, other: u32) -> bool {
        let ty = |at: usize| type_of(self.ctx.cat, &self.world.zoo.entities[at]).map(|t| t.name_id);
        let other = self.world.find(other).and_then(ty);
        other.is_some() && other == ty(self.at)
    }

    /// Both animals fight with a preattack (20.4 point 1).
    fn preattack_with(&self, other: u32) -> bool {
        let at = self.world.find(other);
        let animal = matches!(self.world.units.get(other).map(|u| &u.part),
            Some(Part::Animal(a)) if !a.egg);
        animal && self.flag(self.at, "cPreattack") && at.is_some_and(|t| self.flag(t, "cPreattack"))
    }

    /// The target is in the preattack range (20.4 point 2): 32-bit squared tile distance, no height.
    pub(super) fn in_range(&self, other: u32) -> bool {
        if !self.preattack_with(other) {
            return false;
        }
        let xy = |at: usize| {
            let w = self.world.zoo.width as i32;
            tile_of(self.world, at).map(|t| (t as i32 % w, t as i32 / w))
        };
        let there = self.world.find(other).and_then(xy);
        let d = match (xy(self.at), there) {
            (Some(a), Some(b)) => {
                let (dx, dy) = (a.0 - b.0, a.1 - b.1);
                dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))
            }
            _ => 0x7fff_ffff,
        };
        let r = self.world.units.class.defense_radius;
        d <= r.wrapping_mul(r)
    }

    /// The unit turns towards the unit `other`.
    fn face_unit(&mut self, other: u32) {
        let xy = |at: usize| {
            let w = self.world.zoo.width as i32;
            tile_of(self.world, at).map(|t| (t as i32 % w, t as i32 / w))
        };
        if let Some(to) = self.world.find(other).and_then(xy) {
            crate::animal::food::turn(self.unit, self.world, self.at, to);
        }
    }

    /// Stops everything (staff 7.1: reset, idle animation): the goal reset when `reset`, the set
    /// and the pending set dropped, the movement stopped.
    fn halt(&mut self, reset: bool, idle: bool) {
        if reset {
            self.reset();
        }
        self.unit.runner = Default::default();
        movement::stop(self.unit);
        if idle {
            movement::idle(self.unit, &*self.world, self.data, self.at);
        }
    }

    /// The avoid task, mode 3, on the threat `p`, which becomes the kept predator; the kept prey and
    /// the playing flag go and the chase stamp is the AI clock (20.1 points 1.1 and 1.2).
    fn start_avoid(&mut self, p: u32) {
        let now = self.now();
        self.set_task(AVOID, 3, p, None);
        self.unit.predator = p;
        self.mark(|a| (a.prey, a.playing, a.stamp) = (0, false, now));
    }

    /// 10.1 point 2 (animals 20.1): the avoid, caught and chase rungs. `Some(false)` ends the goal
    /// update (a start, with no common part), `Some(true)` goes to the common part, `None` goes
    /// on down the ladder. `task` is the one read before the ladder.
    pub(super) fn hunt_rungs(&mut self, task: u8) -> Option<bool> {
        let chain = IN_BUILDING.contains(&self.unit.goal.record.task);
        let Part::Animal(a) = &self.unit.part else {
            return None;
        };
        let (escaped, playing) = (a.escaped, a.playing);
        if self.unit.frightened && task != AVOID && !chain {
            let p = self.predator(0);
            if p != 0 {
                self.abort_show();
                self.start_avoid(p);
                return Some(false);
            }
            if !escaped && !playing && !self.occupied() {
                let m = self.predator(1);
                if m != 0 {
                    self.start_avoid(m);
                    return Some(false);
                }
                (self.unit.predator, self.unit.frightened) = (0, false);
            }
        }
        if self.unit.caught && task != AVOID {
            if self.unit.predator == 0 {
                let p = self.predator(0);
                if p != 0 {
                    self.unit.predator = p;
                    self.hunter()?.prey = 0;
                }
            }
            let (now, p) = (self.now(), self.unit.predator);
            self.set_task(AVOID, 3, p, None);
            let a = self.hunter()?;
            (a.playing, a.stamp) = (false, now);
            return Some(false);
        }
        let Part::Animal(a) = &self.unit.part else {
            return None;
        };
        let (escaped, kept) = (a.escaped, a.prey);
        if !(escaped || a.playing) {
            return None;
        }
        if task == CHASE {
            return Some(true);
        }
        if chain {
            return None;
        }
        let q = find_prey(self.world, self.ctx.cat, self.unit, self.at, 0);
        if q != 0 && q != kept && self.predator(0) == 0 {
            self.abort_show();
            self.set_task(CHASE, 3, q, None);
            return Some(false);
        }
        if !escaped && !self.unit.frightened && !self.occupied() {
            let f = find_prey(self.world, self.ctx.cat, self.unit, self.at, 1);
            if f != 0 && kept == 0 {
                self.set_task(CHASE, 3, f, None);
                return Some(false);
            }
        }
        None
    }

    /// 11.7 (animals 20.5): what an animal does with the units its escape check sees. `task` is
    /// the one read before the check's rampage step.
    pub(crate) fn escape_dispatch(&mut self, task: u8) {
        if self.predator(0) != 0 {
            if !self.unit.frightened {
                self.abort_show();
                self.unit.frightened = true;
            }
            return;
        }
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        let calm = !a.playing && !a.escaped && !self.occupied() && !a.rampaging;
        if self.predator(1) != 0 {
            self.unit.frightened |= calm;
            return;
        }
        (self.unit.predator, self.unit.frightened) = (0, false);
        if task == AVOID {
            self.reset();
        }
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        let free = !(self.unit.caught || a.tranq || a.playing || task == CHASE || task == AVOID);
        if free && find_prey(self.world, self.ctx.cat, self.unit, self.at, 0) != 0 {
            self.abort_show();
            self.mark(|a| a.playing = true);
        }
    }

    /// 20.2 point 1, init: the flags go down and the target is the kept prey.
    pub(super) fn hunt_init(&mut self) -> bool {
        let (id, now) = (self.unit.goal.record.target.id, self.now());
        self.mark(|a| (a.pre, a.prey, a.stamp) = ([false; 2], id, now));
        true
    }

    /// 20.2 point 4: the chase is over when the two units meet.
    fn hunt_done(&self, other: (u32, usize)) -> bool {
        let Part::Animal(a) = &self.unit.part else {
            return false;
        };
        if !self.same_type(other.0) {
            let grid = |at: usize| {
                let c = self.world.zoo.entities[at].common.as_ref();
                c.map(|c| (c.x.div_euclid(64), c.y.div_euclid(64), c.z.div_euclid(16)))
            };
            return grid(self.at).is_some() && grid(self.at) == grid(other.1);
        }
        let out = self.ctx.cat.chase_time_out.unwrap_or(5);
        let late = self.now().wrapping_sub(a.stamp) as i32 >= out;
        late || self.rectangles_meet(other.1)
    }

    /// The blocking rectangles of the unit and the unit at `other` overlap (placing 5.2).
    fn rectangles_meet(&self, other: usize) -> bool {
        let tiles: Vec<usize> = [self.at, other]
            .iter()
            .filter_map(|&at| tile_of(self.world, at))
            .collect();
        let found = crate::place::unit::units_on(self.world, &tiles);
        let rect = |at| found.iter().find(|r| r.0 == at);
        let (Some(a), Some(b)) = (rect(self.at), rect(other)) else {
            return false;
        };
        let far = |p: i32, q: i32, u: i32, v: i32| i64::from((p - q).abs()) < i64::from(u + v);
        far(a.1, b.1, a.3.0, b.3.0) && far(a.2, b.2, a.3.1, b.3.1)
    }

    /// 20.2 point 2, check done.
    pub(super) fn hunt_check_done(&mut self) {
        let target = self.target_unit();
        let job = self.unit.goal.record.job;
        let Some(a) = self.hunter() else {
            return;
        };
        if a.pre[0] && job == 0 {
            a.pre[1] = true;
        }
        let Some((id, at)) = target else {
            self.unit.goal.record.done = true;
            return;
        };
        let friendly = self.same_type(id);
        let pre = self.hunter().map_or([true; 2], |a| a.pre);
        if !(pre[0] && pre[1]) && (friendly || !self.preattack_with(id)) {
            self.mark(|a| a.pre = [true; 2]);
        }
        let pre = self.hunter().map_or([true; 2], |a| a.pre);
        if !pre[0] {
            if self.in_range(id) {
                self.mark(|a| a.pre[0] = true);
                self.set_job(PREATTACK, 0, None);
                let world = &*self.world;
                self.unit.goal.record.target.set_tile(world, None);
                return;
            }
        } else if pre[1] && self.hunt_done((id, at)) {
            self.unit.goal.record.done = true;
            return;
        }
        if self.unit.path.len() < 2 {
            let world = &*self.world;
            self.unit.goal.record.target.set_tile(world, None);
        }
    }

    /// 20.2 point 3, decide: the path to the target (the target's tile, no partial path: O15) and
    /// `bChasePrey`; no path or no set ends the chase, forced.
    pub(super) fn hunt_decide(&mut self) {
        let pre = self.hunter().map_or([false; 2], |a| a.pre);
        if (pre[0] && !pre[1]) || self.unit.goal.record.target.tile.is_some() {
            return;
        }
        let there = self
            .target_unit()
            .and_then(|(_, at)| tile_of(self.world, at));
        let walking = there.is_some_and(|t| self.path_to(t, false))
            && self.walk_set("bChasePrey", false, true);
        if !walking {
            self.stop_chase(true);
        }
    }

    /// 20.2 point 5, complete: the catch, then, once its set is over, the end of the chase.
    pub(super) fn hunt_complete(&mut self) {
        match self.hunter().map(|a| a.caught_prey) {
            Some(false) => self.catch(),
            Some(true) if self.done_caught() => self.stop_chase(false),
            _ => {}
        }
    }

    /// 20.2 point 7: no catch set of the hunter runs, and a caught guest has finished its own.
    fn done_caught(&self) -> bool {
        let set = self.unit.runner.top().unwrap_or_default();
        if ["bCaughtGuest", "bCaughtPrey", "bCaughtEgg"].contains(&set) {
            return false;
        }
        let id = self.unit.goal.record.target.id;
        let guest = self.world.units.get(id).filter(|u| u.kind == Kind::Guest);
        let set = guest.and_then(|g| g.runner.top()).unwrap_or_default();
        !(set == "bDefense" || set.contains("bCaughtBy"))
    }

    /// 20.2 point 6, the end of the chase: the kept prey lets go (of the hunter's own type, or `force`), the
    /// flags go with the goal, the hunter stands free and visible.
    pub(super) fn stop_chase(&mut self, force: bool) {
        let kept = self.hunter().map_or(0, |a| a.prey);
        if kept != 0 && self.world.units.get(kept).is_some() && (force || self.same_type(kept)) {
            if let Some(k) = self.world.units.get_mut(kept) {
                k.predator = 0;
            }
            self.mark(|a| a.prey = 0);
        }
        self.mark(|a| (a.caught_prey, a.playing) = (false, false));
        self.halt(true, false);
        (self.unit.held, self.unit.hidden) = (false, false);
    }

    /// 20.3, the catch: the hunter reached its prey.
    fn catch(&mut self) {
        let Some((id, _)) = self.target_unit() else {
            return self.stop_chase(false);
        };
        let (kind, held) = self
            .world
            .units
            .get(id)
            .map_or((Kind::Animal, true), |u| (u.kind, u.held));
        let seized =
            matches!(self.world.units.get(id).map(|u| &u.part), Some(Part::Guest(g)) if g.seized);
        if self.same_type(id) || held || seized {
            return self.stop_chase(false);
        }
        self.halt(false, false);
        self.face_unit(id);
        let egg =
            matches!(self.world.units.get(id).map(|u| &u.part), Some(Part::Animal(a)) if a.egg);
        match kind {
            Kind::Guest => self.catch_guest(id),
            _ if egg => {
                with_unit(self.world, self.ctx, self.data, id, |cx| {
                    cx.start_set("bEatenEgg", true);
                    if let Part::Animal(a) = &mut cx.unit.part {
                        a.dying = true;
                    }
                });
                self.start_set("bCaughtEgg", true);
                self.mark(|a| a.caught_prey = true);
            }
            _ => {
                with_unit(self.world, self.ctx, self.data, id, |cx| {
                    cx.halt(false, true);
                    cx.unit.held = true;
                });
                let (unit, world, cat) = (&mut *self.unit, &mut *self.world, self.ctx.cat);
                crate::unit::walk::random_walk(unit, world, cat, self.ctx.rng, self.at);
                let dest = self.unit.destination;
                let world = &*self.world;
                self.unit.goal.record.target.set_pos(world, dest);
                self.start_set("bCaughtPrey", true);
                self.mark(|a| a.caught_prey = true);
            }
        }
    }

    /// 20.3 point 3: the catch of a guest.
    fn catch_guest(&mut self, guest: u32) {
        let me = self.unit.id;
        let taken = self
            .world
            .units
            .get(guest)
            .is_some_and(|g| g.predator != me && g.caught);
        if taken {
            self.reset();
            (self.unit.held, self.unit.hidden) = (false, false);
            self.mark(|a| a.caught_prey = false);
            return;
        }
        let me_at = self.at;
        with_unit(self.world, self.ctx, self.data, guest, |cx| {
            cx.halt(false, false);
            (cx.unit.held, cx.unit.predator) = (true, me);
            cx.was_caught(me);
        });
        self.mark(|a| a.prey = 0);
        self.start_set("bCaughtGuest", true);
        if self.flag(me_at, "cResetPreyPosition") {
            let here = self.world.zoo.entities[me_at].common.as_ref();
            let spot = here.map(|c| ((c.x, c.y, c.z), c.rotation));
            let at = self.world.find(guest);
            if let (Some((point, rotation)), Some(at)) = (spot, at) {
                self.world.place_unit(at, point, rotation);
            }
        }
        with_unit(self.world, self.ctx, self.data, guest, |cx| {
            if let Part::Guest(g) = &mut cx.unit.part {
                g.seized = true;
            }
            cx.think(THOUGHT, Some(me));
        });
        self.mark(|a| a.caught_prey = true);
        self.unit.held = true;
    }

    /// 20.3 point 6, the caught reaction: the unit is caught by the hunter `p`. It runs from an animal
    /// (the avoid task on it), faces it and plays `bCaughtBy` (or the lion's).
    fn was_caught(&mut self, p: u32) {
        let hunter = self.world.units.get(p).filter(|u| u.kind == Kind::Animal);
        let task = self.unit.goal.record.task;
        if hunter.is_some() && task != AVOID && !IN_BUILDING.contains(&task) {
            self.reset();
            self.set_task(AVOID, 3, p, None);
            self.unit.predator = p;
        }
        self.face_unit(p);
        let female = self
            .world
            .find(p)
            .is_some_and(|at| self.world.zoo.entities[at].subtype == "f");
        let lion = if female {
            "bCaughtBylionf"
        } else {
            "bCaughtBylionm"
        };
        if self.start_set("bCaughtBy", true) || self.start_set(lion, true) {
            self.unit.caught = true;
        }
    }

    /// 20.4 point 3, init of the preattack job: the hunter (playing) or its prey (afraid) faces the
    /// other and starts `bPreattack` or `bDefense`, held. False when it cannot.
    pub(super) fn preattack_init(&mut self) -> bool {
        let Part::Animal(a) = &self.unit.part else {
            return false;
        };
        let (playing, afraid) = (a.playing, self.unit.frightened);
        let (set, other) = match (playing, afraid) {
            (true, _) => ("bPreattack", true),
            (false, true) => ("bDefense", false),
            _ => return false,
        };
        let target = self.unit.goal.record.target.id;
        self.face_unit(target);
        self.unit.held = true;
        movement::stop(self.unit);
        if !self.start_set(set, true) {
            self.unit.held = false;
            return false;
        }
        let kept = self.hunter().map_or(0, |a| a.prey);
        if let Some(k) = self
            .world
            .units
            .get_mut(kept)
            .filter(|k| other && k.kind == Kind::Animal)
        {
            k.held = true;
        }
        true
    }

    /// 20.4 point 3, check done of the preattack job.
    pub(super) fn preattack_check_done(&mut self) {
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        let name = if a.playing {
            "bPreattack"
        } else if self.unit.frightened {
            "bDefense"
        } else {
            return;
        };
        self.unit.goal.record.job_done = self.unit.runner.top() != Some(name);
    }

    /// 20.4 point 3, complete: the holds go and the job is cleared.
    pub(super) fn preattack_complete(&mut self) {
        self.unit.held = false;
        let kept = self.hunter().map_or(0, |a| a.prey);
        if let Some(k) = self
            .world
            .units
            .get_mut(kept)
            .filter(|k| matches!(k.part, Part::Animal(_)))
        {
            k.held = false;
        }
        self.set_job(0, 0, None);
    }

    /// 20.4 point 4, the animal's half of the avoid decide: it defends once per goal against a
    /// threat that fights (the flags are the avoid goal's own).
    pub(super) fn defence(&mut self) {
        let target = self.target_unit();
        let pre = self.hunter().map_or([true; 2], |a| a.pre);
        if !(pre[0] && pre[1]) {
            let fight =
                target.is_some_and(|(id, _)| !self.same_type(id) && self.preattack_with(id));
            if !fight {
                self.mark(|a| a.pre = [true; 2]);
            }
        }
        let pre = self.hunter().map_or([true; 2], |a| a.pre);
        if let Some((id, _)) = target
            && !pre[0]
            && !pre[1]
            && self.in_range(id)
        {
            self.mark(|a| a.pre = [true, false]);
            self.set_job(PREATTACK, 0, None);
            let world = &*self.world;
            self.unit.goal.record.target.set_tile(world, None);
            self.unit.clear_own();
            return;
        }
        if pre[0] && self.unit.goal.record.job == 0 {
            self.mark(|a| a.pre[1] = true);
        }
    }
}
