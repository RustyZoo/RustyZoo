//! A keeper's capture of an escaped animal (staff 7.1 point 2, 7.11, animals 11.6): the search that
//! claims the nearest animal he may capture, the capture task and its jobs 13 (close in), 14 (the
//! dart) and 15 (the crate). The helicopter and its hook are not here.

use super::super::{CAPTURE, Cx};
use crate::animal::{self, Animal, boxed};
use crate::build::number;
use crate::unit::{Part, movement, predator};

/// The jobs of the capture.
pub(super) const CHASE: u8 = 13;
pub(super) const TRANQ: u8 = 14;
pub(super) const RETRIEVE: u8 = 15;
/// The type name ids that capture: the zookeeper, the helicopter and the scientist.
const ZOOKEEPER: i32 = 9500;
const HELICOPTER: i32 = 9550;
const SCIENTIST: i32 = 9551;
/// The distance to a unit with no tile.
const NOWHERE: i32 = 0x7fff_ffff;

impl Cx<'_, '_> {
    /// The animal `id`, when it is one and exists.
    fn animal(&self, id: u32) -> Option<&Animal> {
        self.world.find(id)?;
        match &self.world.units.get(id)?.part {
            Part::Animal(a) => Some(a),
            _ => None,
        }
    }

    /// The capture test (animals 11.6): the animal is loose on a dry tile and this member's type
    /// fits: the zookeeper for a species without helicopter recovery, else the helicopter while it
    /// is awake and the scientist once it is darted.
    fn can_capture(&self, id: u32) -> bool {
        let (Some(a), Some(at)) = (self.animal(id), self.world.find(id)) else {
            return false;
        };
        let wet = self
            .tile_of_entity(at)
            .is_some_and(|t| self.world.is_water(t));
        if !a.escaped || wet {
            return false;
        }
        let heli = animal::keys(self.world, self.ctx.cat, at).heli_recovery;
        let fits = match (heli, a.tranq) {
            (false, _) => ZOOKEEPER,
            (true, false) => HELICOPTER,
            (true, true) => SCIENTIST,
        };
        self.who().is_some_and(|w| w.name_id == fits)
    }

    /// The squared tile distance to the unit `id`, `NOWHERE` when either has no tile.
    fn distance(&self, id: u32) -> i32 {
        let there = self.world.find(id).and_then(|e| self.tile_of_entity(e));
        let width = self.world.zoo.width as i32;
        match (self.tile(), there) {
            (Some(a), Some(b)) => {
                let (dx, dy) = (
                    (a as i32 % width) - (b as i32 % width),
                    (a as i32 / width) - (b as i32 / width),
                );
                dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))
            }
            _ => NOWHERE,
        }
    }

    /// `foundEscapedAnimal` (animals 11.6): his capture id goes, then the escaped animals with no
    /// live claim he may capture are walked in order and the one nearest by the Manhattan
    /// distance wins: the first qualifying animal is taken whatever its distance, a later one
    /// replaces it only when strictly nearer. It is written as his capture id and he is the
    /// animal's claim. Whether one was found.
    fn found_escaped(&mut self) -> bool {
        if let Some(s) = self.staff() {
            s.captured = 0;
        }
        let here = self.tile();
        let width = self.world.zoo.width as i32;
        let (mut best, mut found) = (NOWHERE, 0);
        for e in predator::escaped(self.world) {
            let Some(id) = self.world.zoo.entities[e].common.as_ref().map(|c| c.id) else {
                continue;
            };
            let claim = match self.world.units.get(id).map(|u| &u.part) {
                Some(Part::Animal(a)) => a.claim,
                _ => continue,
            };
            if claim != 0 && self.world.find(claim).is_some() || !self.can_capture(id) {
                continue;
            }
            let d = match (here, self.tile_of_entity(e)) {
                (Some(a), Some(b)) => {
                    let (a, b) = (a as i32, b as i32);
                    ((a % width) - (b % width)).abs() + ((a / width) - (b / width)).abs()
                }
                _ => NOWHERE,
            };
            if found == 0 || d < best {
                (best, found) = (d, id);
            }
        }
        if found == 0 {
            return false;
        }
        let me = self.unit.id;
        if let Some(s) = self.staff() {
            s.captured = found;
        }
        if let Some(Part::Animal(a)) = self.world.units.get_mut(found).map(|u| &mut u.part) {
            a.claim = me;
        }
        true
    }

    /// Staff 7.1 point 2: a keeper who is not capturing and finds an animal to capture stops
    /// everything and takes the capture task, mode 3, on it. Whether the ladder ends here.
    pub(super) fn capture_ladder(&mut self, task: u8) -> bool {
        if task == CAPTURE || !self.found_escaped() {
            return false;
        }
        self.reset();
        self.unit.runner = Default::default();
        movement::stop(self.unit);
        let animal = self.staff().map_or(0, |s| s.captured);
        self.set_task(CAPTURE, 3, animal, None);
        true
    }

    /// `doneCapture`: his capture id and the claim of that animal go.
    pub(in crate::goal) fn done_capture(&mut self) {
        let animal = self.staff().map_or(0, |s| std::mem::take(&mut s.captured));
        if let Some(Part::Animal(a)) = self.world.units.get_mut(animal).map(|u| &mut u.part) {
            a.claim = 0;
        }
    }

    /// 7.11 init: the target must be an animal; job 15 when it is darted, else job 13.
    pub(in crate::goal) fn capture_init(&mut self) -> bool {
        let id = self.record().target.id;
        let Some(a) = self.animal(id) else {
            return false;
        };
        let job = if a.tranq { RETRIEVE } else { CHASE };
        self.set_job(job, id, None);
        true
    }

    /// 7.11 check done: the target is no longer an animal or he may not capture it.
    pub(in crate::goal) fn capture_check_done(&mut self) {
        let id = self.record().target.id;
        if !self.can_capture(id) {
            self.record().done = true;
        }
    }

    /// 7.11 complete: the goal resets, then the capture is over.
    pub(in crate::goal) fn capture_complete(&mut self) {
        self.reset();
        self.done_capture();
    }

    /// The job's target tile is given up once he stands on it.
    fn arrived(&mut self) {
        if self.reached(true) {
            let world = &*self.world;
            self.unit.goal.record.job_target.set_tile(world, None);
        }
    }

    /// Job 13 check done: the animal is within the weapon range (a squared distance), else the
    /// tile of the leg is given up on arrival.
    pub(super) fn chase_check_done(&mut self) {
        let range = (self.who().and_then(|w| number(w, "cWeaponRange"))).map_or(5, |n| n as i32);
        let target = self.record().target.id;
        if self.distance(target) <= range.wrapping_mul(range) {
            self.record().job_done = true;
        } else {
            self.arrived();
        }
    }

    /// Jobs 13 and 15 decide: a path to the tile of the animal, partial if need be, and the set
    /// that follows it; the leg ends where the animal was.
    pub(super) fn chase_decide(&mut self, set: &str, interruptible: bool) {
        let id = self.record().job_target.id;
        let Some(at) = self.world.find(id) else {
            return self.set_job(0, 0, None);
        };
        let Some(to) = self.tile_of_entity(at) else {
            return;
        };
        if self.path_to(to, true) && self.start_with(set, interruptible, false) {
            let (world, dest) = (&*self.world, self.unit.destination);
            self.unit.goal.record.job_target.set_pos(world, dest);
        }
    }

    /// Job 13 complete: he stops, faces the animal and fires; job 14 follows.
    pub(super) fn chase_complete(&mut self) {
        let id = self.record().target.id;
        self.unit.runner = Default::default();
        movement::stop(self.unit);
        if let Some(t) = self.world.find(id).and_then(|e| self.tile_of_entity(e)) {
            self.face(t);
        }
        const FIRE: &str = "bFireWeapon";
        if self.unit.runner.top() != Some(FIRE) {
            self.start_with(FIRE, false, true);
        }
        self.set_job(TRANQ, id, None);
    }

    /// Job 14 check done: the set of the shot is over, or never was.
    pub(super) fn dart_check_done(&mut self) {
        let firing = self.unit.runner.top() == Some("bFireWeapon") && self.unit.runner.running();
        self.record().job_done |= !firing;
    }

    /// Job 14 complete: the animal is darted. If he may still capture it, the crate follows.
    pub(super) fn dart_complete(&mut self) {
        let id = self.record().job_target.id;
        let Some(at) = self.animal(id).and(self.world.find(id)) else {
            return self.set_job(0, 0, None);
        };
        let keys = animal::keys(self.world, self.ctx.cat, at);
        if let Some(unit) = self.world.units.get_mut(id) {
            boxed::tranquilise(unit, &keys);
        }
        match self.can_capture(id) {
            true => self.set_job(RETRIEVE, id, None),
            false => self.set_job(0, 0, None),
        }
    }

    /// Job 15 check done: an animal that woke is chased again; next to it he is done.
    pub(super) fn retrieve_check_done(&mut self) {
        let id = self.record().job_target.id;
        let Some(a) = self.animal(id) else {
            return self.set_job(0, 0, None);
        };
        if !a.tranq {
            return self.set_job(CHASE, id, None);
        }
        let target = self.record().target.id;
        if self.distance(target) <= 1 {
            self.record().job_done = true;
        } else {
            self.arrived();
        }
    }

    /// Job 15 complete: the animal is crated where it stands.
    pub(super) fn retrieve_complete(&mut self) {
        let id = self.record().job_target.id;
        match self.animal(id) {
            Some(_) => boxed::crate_in_place(self.world, id),
            None => self.set_job(0, 0, None),
        }
    }
}
