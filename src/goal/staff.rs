//! The goals of a keeper (staff 7): going to the exhibit that needs him (task 0x65), his duties
//! there (0x6e) and the jobs of the duties, feeding, healing and raking (1 to 6). The exhibit
//! counters, the claimed and unreachable places and the gate he opened follow his own target tile
//! ([`Cx::keeper_target`]), which is not the goal record's target tile (staff 7.4). A marine
//! keeper's tanks are in [`tank`].

use super::{Cx, DUTIES, ENTER_TANK, EXIT_TANK, HABITAT, NONE, RANDOM};
use crate::animal::{self, dirt, food};
use crate::build::{cost, number};
use crate::entity::EntityType;
use crate::path::{self, MAX};
use crate::place::bulldozer::type_of;
use crate::staff::{assign, service};
use crate::unit::{Part, Staff, movement};
use tank::{CLEAN_TANK, CLIMB_DOWN_INSIDE, is_tank_job};

mod guide;
mod tank;
mod worker;

/// The jobs of the duties.
const PUT_FOOD: u8 = 1;
const PUTTING_FOOD: u8 = 2;
const HEAL: u8 = 3;
const HEALING: u8 = 4;
const CLEAN: u8 = 5;
const CLEANING: u8 = 6;
/// The messages 11004 (trouble getting to an exhibit) and 11001 (blocked entrance), staff 10.
const TROUBLE: i32 = 11004;
const BLOCKED: i32 = 11001;
/// The food types by `cKeeperFoodType` (staff 7.6).
const FOODS: [&str; 14] = [
    "herbchow", "carnchow", "fruichow", "bambchow", "graschow", "fishchow", "inschow", "dinocarn",
    "dinogras", "krill", "fish", "chum", "seagrass", "urchin",
];
/// A food pile goes within this squared distance of the keeper, and 20 failed places give up an
/// animal (7.6).
const REACH: i64 = 10;
const GIVE_UP: usize = 20;

impl<'c> Cx<'_, 'c> {
    /// The keeper's type.
    fn who(&self) -> Option<&'c EntityType> {
        type_of(self.ctx.cat, &self.world.zoo.entities[self.at])
    }

    /// The exhibit under the keeper; none on the zoo grounds.
    fn exhibit(&self) -> Option<u32> {
        food::exhibit_of(self.world, self.tile()?)
    }

    fn staff(&mut self) -> Option<&mut Staff> {
        match &mut self.unit.part {
            Part::Staff(s) => Some(s),
            _ => None,
        }
    }

    fn lost(&self) -> (Vec<usize>, Vec<u32>) {
        match &self.unit.part {
            Part::Staff(s) => (s.lost_tiles.clone(), s.lost_animals.clone()),
            _ => Default::default(),
        }
    }

    fn name_of(&self, id: u32) -> String {
        let named = self
            .world
            .find(id)
            .and_then(|e| self.world.zoo.entities[e].common.as_ref());
        named.map_or_else(String::new, |c| c.name.clone())
    }

    fn exhibit_name(&self, x: u32) -> String {
        self.world
            .exhibits()
            .get(x)
            .map_or_else(String::new, |e| e.name.clone())
    }

    /// The message 11004 with the keeper's name and the name of what he cannot reach (staff 10).
    fn trouble(&mut self, other: String) {
        let names = vec![self.name_of(self.unit.id), other];
        self.world.units.post_named(TROUBLE, self.unit.id, names);
    }

    /// The keeper's own target tile (staff 7.4).
    fn own(&self) -> Option<usize> {
        match &self.unit.part {
            Part::Staff(s) => s.target,
            _ => None,
        }
    }

    /// The keeper no longer comes to the exhibit of his own target tile: its count of keepers on
    /// their way goes down (not below 0) and he closes the gate he remembers (movement 6.2).
    fn target_left(&mut self) {
        let exhibit = self.own().and_then(|t| food::exhibit_of(self.world, t));
        if let Some(x) = exhibit.and_then(|x| self.world.exhibits_mut().get_mut(x)) {
            x.rating.coming = x.rating.coming.saturating_sub(1);
        }
        self.world.units.release_gate(self.unit);
    }

    /// The keeper's own setter of his target tile (staff 7.4): another tile than the one he has
    /// first gives up the old one.
    pub(super) fn keeper_target(&mut self, tile: Option<usize>) {
        if self.own() != tile {
            self.target_left();
            if let Some(s) = self.staff() {
                s.target = tile;
            }
        }
    }

    /// A held animal is let go (7.7), when its keeper's goal ends.
    pub(super) fn keeper_reset(&mut self) {
        if matches!(self.record().job, HEAL | HEALING) {
            let id = self.record().job_target.id;
            self.hold(id, false);
        }
        self.keeper_target(None);
    }

    /// The animal `id` stops where it stands and waits for the keeper, or goes on (7.7).
    fn hold(&mut self, id: u32, on: bool) {
        if let Some(a) = self.world.units.get_mut(id) {
            if on {
                (a.destination, a.wait) = (None, i32::MAX);
                movement::stop(a);
            } else {
                a.wait = 0;
            }
        }
    }

    /// 7.1 points 3 and 4: with nothing to do on the grounds he goes to an exhibit; done in the
    /// exhibit of his target tile he starts his duties unless two keepers work there.
    pub(super) fn keeper_ladder(&mut self, task: u8, done: bool) {
        let here = self.exhibit();
        if task == NONE || (done && here.is_none()) {
            // Standing in a tank he leaves it first (point 3.1).
            if here.is_some_and(|x| self.world.tank(x).is_some()) {
                return self.set_task(EXIT_TANK, 0, 0, None);
            }
            self.set_task(HABITAT, 4, 0, None);
            return self.keeper_target(None);
        }
        let target = self.own().and_then(|t| food::exhibit_of(self.world, t));
        let alone = here.is_some_and(|x| service::present(self.world, x) < 2);
        if done && alone && here == target {
            self.set_task(DUTIES, 0, 0, None);
        }
    }

    // ---- 0x65, to the exhibit

    /// 7.4 check done: the exhibit must still want him; on arrival he must be in it.
    pub(super) fn exhibit_check_done(&mut self) {
        let Some(x) = self.own().and_then(|t| food::exhibit_of(self.world, t)) else {
            return;
        };
        let (cat, who) = (self.ctx.cat, self.who());
        if who.is_some_and(|w| assign::block_service(self.world, cat, x, w, (false, true)) != 0) {
            return self.reset();
        }
        let there = (self.unit.destination)
            .and_then(|(px, py, _)| self.world.tile_index(px.div_euclid(64), py.div_euclid(64)));
        if there.is_none() || there != self.tile() {
            return;
        }
        // A tank is served from the tile outside its gate (staff 7.4, check done).
        let gate = service::gate_tiles(self.world, x).map(|g| g.1);
        let at_gate = self.world.tank(x).is_some() && gate == self.tile();
        if self.exhibit() == Some(x) || at_gate {
            self.record().done = true;
        } else {
            self.reset();
            self.set_task(HABITAT, 4, 0, None);
        }
    }

    /// 7.4 decide, with no target tile yet: choose an exhibit and walk to a tile beside its gate.
    pub(super) fn exhibit_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let (cat, here) = (self.ctx.cat, self.exhibit());
        let assigned = match &self.unit.part {
            Part::Staff(s) => s.assigned.clone(),
            _ => Vec::new(),
        };
        let lost = self.lost().0;
        let chosen =
            (self.who()).and_then(|w| service::choose(self.world, cat, w, &assigned, here, &lost));
        let Some(x) = chosen else {
            return self.set_task(RANDOM, 1, 0, None);
        };
        let gate = service::gate_tiles(self.world, x);
        let mover = movement::mover(self.unit, self.world, self.at, self.tile().unwrap_or(0));
        // A tank is walked to at the tile outside its gate, a land exhibit at one beside its inside
        // tile.
        let tank = self.world.tank(x).is_some();
        let walk = gate.map(|(inside, outside)| match tank {
            true => outside,
            false => self.gate_side(x, inside, &mover),
        });
        // A path, else one more in the relaxed mode that may stop short (pathfinding 6.2): a
        // path that ends anywhere but in the exhibit is a short one.
        self.unit.path.clear();
        let mut ok = walk.is_some_and(|to| self.path_to(to, false));
        let mut short = false;
        if let (false, Some(to)) = (ok, walk) {
            self.unit.mover.relaxed = true;
            ok = self.path_to(to, true);
            self.unit.mover.relaxed = false;
            let end = self.unit.path.last();
            short = !tank && end.and_then(|&t| food::exhibit_of(self.world, t)) != Some(x);
        }
        let started = ok && self.start_walk(false, false, false);
        if !started {
            self.unit.path.clear();
            return self.exhibit_failed(x, gate, false);
        }
        let tiles = self.world.tiles_of(x);
        let tile = tiles[self.ctx.rng.pick(tiles.len() as u32) as usize];
        self.keeper_target(Some(tile));
        self.world
            .units
            .staff_note(format!("go {} {x}", self.unit.id));
        match short {
            true => self.exhibit_failed(x, gate, true),
            false => {
                if let Some(e) = self.world.exhibits_mut().get_mut(x) {
                    e.rating.coming += 1;
                }
            }
        }
    }

    /// One of the eight tiles round the gate's inside tile that is in the exhibit and open to the
    /// keeper, drawn at random, else that tile itself.
    fn gate_side(&mut self, x: u32, inside: usize, mover: &path::Mover) -> usize {
        let w = self.world.zoo.width as usize;
        let (gx, gy) = ((inside % w) as i32, (inside / w) as i32);
        let mut around = Vec::new();
        for (ox, oy) in (-1..=1).flat_map(|ox| (-1..=1).map(move |oy| (ox, oy))) {
            let t = self.world.tile_index(gx + ox, gy + oy);
            let open = |t: &usize| path::terrain(self.world, mover, *t, None) < MAX;
            around.extend(t.filter(|_| ox != 0 || oy != 0).filter(open));
        }
        around.retain(|&t| self.world.exhibits().region(t) == Some(x));
        match around.is_empty() {
            true => inside,
            false => around[self.ctx.rng.pick(around.len() as u32) as usize],
        }
    }

    /// 7.4 failure: a blocked entrance is told (11001) and so is a way not found (11004, with no
    /// walk started); a short walk to an open entrance goes on without a word. Otherwise the
    /// exhibit hears he is not coming and he wanders.
    fn exhibit_failed(&mut self, x: u32, gate: Option<(usize, usize)>, walking: bool) {
        let mover = movement::mover(self.unit, self.world, self.at, self.tile().unwrap_or(0));
        let tank = self.world.tank(x).is_some();
        let blocked = gate.is_none_or(|g| service::entrance_blocked(self.world, &mover, g, tank));
        let name = self.exhibit_name(x);
        match (blocked, walking) {
            (true, _) => {
                let id = self.unit.id;
                self.world.units.post_named(BLOCKED, id, vec![name]);
            }
            (_, false) => self.trouble(name),
            _ => return,
        }
        self.target_left();
        self.set_task(RANDOM, 1, 0, None);
    }

    /// 7.4 complete: the duties, unless two keepers work there.
    pub(super) fn exhibit_complete(&mut self) {
        let tank = self.own().and_then(|t| food::exhibit_of(self.world, t));
        if tank.is_some_and(|x| self.world.tank(x).is_some()) {
            return self.set_task(ENTER_TANK, 0, 0, None);
        }
        match self.exhibit() {
            Some(x) if service::present(self.world, x) >= 2 => self.reset(),
            _ => self.set_task(DUTIES, 0, 0, None),
        }
    }

    // ---- 0x6e, the duties

    /// 7.5 init: his lists are emptied, the exhibit loses one coming keeper and his animals are
    /// told he visited.
    pub(super) fn duties_init(&mut self) {
        if let Some(s) = self.staff() {
            (s.lost_tiles, s.lost_animals) = (Vec::new(), Vec::new());
        }
        let Some(x) = self.exhibit() else {
            return;
        };
        if let Some(e) = self.world.exhibits_mut().get_mut(x) {
            e.rating.coming = e.rating.coming.saturating_sub(1);
        }
        let mine = self
            .who()
            .map(|w| service::animals(self.world, self.ctx.cat, x, w));
        for a in mine.unwrap_or_default() {
            if let Some(Part::Animal(a)) = self.world.units.get_mut(a.id).map(|u| &mut u.part) {
                a.keeper_visited = true;
            }
        }
    }

    /// 7.5 check done: done on the grounds.
    pub(super) fn duties_check_done(&mut self) {
        if self.exhibit().is_none() {
            self.record().done = true;
        }
    }

    /// 7.5 decide: the next job, in the order food, heal, rake; none left ends the duties.
    pub(super) fn duties_decide(&mut self) {
        let (Some(x), Some(who)) = (self.exhibit(), self.who()) else {
            return;
        };
        let cat = self.ctx.cat;
        if assign::block_service(self.world, cat, x, who, (false, true)) != 0 {
            self.record().done = true;
        }
        if self.record().done || self.record().job != 0 {
            return;
        }
        let (tiles, animals) = self.lost();
        let mine: Vec<_> = service::animals(self.world, cat, x, who);
        let claimed = service::claimed(self.world);
        let bare: Vec<_> = (mine.iter())
            .filter(|a| !animals.contains(&a.id) && !a.egg && a.bare)
            .collect();
        let eater = bare.iter().find(|a| a.hungry).copied().or_else(|| {
            let n = bare.len() as u32;
            (n > 0).then(|| bare[self.ctx.rng.pick(n) as usize])
        });
        let sick = (mine.iter()).any(|a| {
            let free = !claimed.contains(&a.tile) && !self.world.is_water(a.tile);
            a.sick && !animals.contains(&a.id) && free
        });
        let rake = service::dung(self.world, cat, x, who).into_iter();
        let rake = rake
            .filter(|t| !claimed.contains(t) && !tiles.contains(t))
            .count();
        let tank = self.world.tank(x);
        let murky = i64::from(self.world.tank_config().murky);
        let job = match eater {
            Some(a) if service::food_needed(self.world, cat, x, who, a.food) > 0 => {
                if let Some(s) = self.staff() {
                    s.food_kind = a.food;
                }
                Some((PUT_FOOD, a.id))
            }
            _ if sick => Some((HEAL, 0)),
            _ if tank.is_none() && rake > 0 => Some((CLEAN, 0)),
            _ if tank.is_some_and(|t| t.level > 0 && i64::from(t.purity) < murky) => {
                Some((CLEAN_TANK, 0))
            }
            _ => None,
        };
        match job {
            // On the platform any job first climbs down into the water (tank staff 5.2).
            Some(_) if self.unit.tank == 2 => self.set_job(CLIMB_DOWN_INSIDE, 0, None),
            Some((job, id)) => self.set_job(job, id, None),
            None => self.record().done = true,
        }
    }

    /// 7.5 complete: the exhibit is serviced, the lists emptied and the goal ends.
    pub(super) fn duties_complete(&mut self) {
        let now = u64::from(self.now());
        if let Some(x) = self.exhibit() {
            self.world
                .units
                .staff_note(format!("done {} {x}", self.unit.id));
            if let Some(e) = self.world.exhibits_mut().get_mut(x) {
                e.rating.serviced = now;
            }
        }
        if let Some(s) = self.staff() {
            (s.lost_tiles, s.lost_animals) = (Vec::new(), Vec::new());
        }
        self.reset();
    }

    // ---- the jobs

    /// The set a job plays.
    fn job_set(&mut self, job: u8) -> &'static str {
        let kind = self.staff().map_or(0, |s| s.food_kind);
        let id = self.record().job_target.id;
        let target = self.world.units.get(id);
        let egg = matches!(target.map(|u| &u.part), Some(Part::Animal(a)) if a.egg);
        let tank = self.exhibit().is_some_and(|x| self.world.tank(x).is_some());
        match job {
            PUTTING_FOOD if tank => "bFeedTank",
            PUTTING_FOOD if kind == 1 => "bFeedCarn",
            PUTTING_FOOD => "bFeedHerb",
            HEALING if egg => "bHealEgg",
            HEALING => "bHeal",
            _ => "bClean",
        }
    }

    /// The init of a job: the animal or the dung must be there; he faces it and starts the set.
    /// False clears the job.
    pub(super) fn job_init(&mut self, job: u8) -> bool {
        if is_tank_job(job) {
            return self.tank_init(job);
        }
        if !matches!(job, PUTTING_FOOD | HEALING | CLEANING) {
            return true;
        }
        let r = self.record().job_target.clone();
        let there = match job {
            PUTTING_FOOD => r.tile,
            HEALING => (self.world.find(r.id)).and_then(|e| self.tile_of_entity(e)),
            _ => r.tile.filter(|&t| self.dung_at(t)),
        };
        let Some(tile) = there else {
            self.keeper_target(None);
            return false;
        };
        self.face(tile);
        let set = self.job_set(job);
        if !self.start_set(set, false) {
            self.record().job_done = true;
        }
        true
    }

    fn tile_of_entity(&self, e: usize) -> Option<usize> {
        let c = self.world.zoo.entities[e].common.as_ref()?;
        self.world
            .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
    }

    /// Whether the centre slot of tile `t` holds the keeper's dung.
    fn dung_at(&self, t: usize) -> bool {
        let who = self.who();
        who.is_some_and(|w| service::is_dung(self.world, self.ctx.cat, w, t))
    }

    /// He turns towards tile `t` (the direction of the eight nearest to it).
    fn face(&mut self, t: usize) {
        let (Some(here), w) = (self.tile(), self.world.zoo.width as i32) else {
            return;
        };
        let (dx, dy) = (
            (t as i32 % w) - (here as i32 % w),
            (t as i32 / w) - (here as i32 / w),
        );
        let Some(c) = self.world.zoo.entities[self.at].common.as_ref() else {
            return;
        };
        if let Some(facing) = food::nearest_direction(dx, dy) {
            self.world.place_unit(self.at, (c.x, c.y, c.z), facing);
        }
    }

    /// A job is over: no job, no target tile.
    fn job_end(&mut self) {
        self.keeper_target(None);
        self.set_job(0, 0, None);
    }

    /// The set of the job has ended, or was never there.
    fn set_ended(&mut self, job: u8) -> bool {
        let set = self.job_set(job);
        !(self.unit.runner.top() == Some(set) && self.unit.runner.running())
    }

    /// 4.3 check done of a job: the keeper stands on the job's tile, or the set has ended.
    pub(super) fn job_check_done(&mut self, job: u8) {
        if is_tank_job(job) {
            return self.tank_check_done(job);
        }
        let id = self.record().job_target.id;
        let done = match job {
            PUT_FOOD | HEAL | CLEAN => self.reached(true),
            HEALING if self.world.find(id).is_none() => return self.job_end(),
            _ => self.set_ended(job),
        };
        self.record().job_done |= done;
    }

    pub(super) fn job_decide(&mut self, job: u8) {
        if self.record().job_target.tile.is_none() {
            if is_tank_job(job) {
                return self.tank_decide(job);
            }
            match job {
                PUT_FOOD => self.food_decide(),
                HEAL => self.heal_decide(),
                CLEAN => self.rake_decide(),
                _ => {}
            }
        }
    }

    pub(super) fn job_complete(&mut self, job: u8) {
        if is_tank_job(job) {
            return self.tank_complete(job);
        }
        let r = self.record().job_target.clone();
        match job {
            PUT_FOOD => {
                let more = self.food_short();
                match more {
                    true => {
                        self.set_job(PUTTING_FOOD, r.id, r.tile);
                        // The pile in the water goes at the depth the keeper swam to.
                        if self.exhibit().is_some_and(|x| self.world.tank(x).is_some()) {
                            self.record().job_target.pos = r.pos;
                        }
                    }
                    false => self.job_end(),
                }
            }
            PUTTING_FOOD => self.food_put(r.tile),
            HEAL => self.set_job(HEALING, r.id, r.tile),
            HEALING => self.healed(r.id),
            CLEAN => self.set_job(CLEANING, 0, r.tile),
            _ => self.raked(r.tile),
        }
    }

    /// Walks to tile `to` for the job: a path and the walk set; the job target follows.
    fn go_job(&mut self, to: usize) -> bool {
        self.go_job_at(to, None)
    }

    /// The same to the level `level` of the tile, in a tank.
    fn go_job_at(&mut self, to: usize, level: Option<i32>) -> bool {
        self.unit.path.clear();
        self.path_to_level(to, false, level) && self.start_walk(false, true, false)
    }

    /// Whether the animals of the exhibit still need food of the keeper's food kind.
    fn food_short(&mut self) -> bool {
        let kind = self.staff().map_or(0, |s| s.food_kind);
        let (x, who) = (self.exhibit(), self.who());
        x.zip(who)
            .is_some_and(|(x, w)| service::food_needed(self.world, self.ctx.cat, x, w, kind) > 0)
    }

    fn food_type(&mut self) -> Option<(&'static str, &'c EntityType)> {
        let name = FOODS.get(usize::try_from(self.staff()?.food_kind).ok()?)?;
        Some((name, self.ctx.cat.find("food", b"food", name.as_bytes())?))
    }

    /// 7.6 job 1: the place for the pile, within reach, free and open; the way there.
    fn food_decide(&mut self) {
        let id = self.record().job_target.id;
        let (x, who, here) = (self.exhibit(), self.who(), self.tile());
        let ok = self.world.find(id).is_some() && self.food_type().is_some();
        let (Some(x), Some(_), Some(here), true) = (x, who, here, ok) else {
            return self.job_end();
        };
        let (tiles, _) = self.lost();
        // A tank has its place in the water (tank staff 5.3).
        let (mut level, tank) = (None, self.world.tank(x).is_some());
        let tank_place = match tank {
            true => self.tank_food_place(x, here),
            false => None,
        };
        if tank && tank_place.is_none() {
            return self.job_end();
        }
        let claimed = service::claimed(self.world);
        let inside = service::gate_tiles(self.world, x).map(|g| g.0);
        let mover = movement::mover(self.unit, self.world, self.at, here);
        let free = |t: &usize| {
            let world = &*self.world;
            !tiles.contains(t)
                && (0..4).all(|q| world.quadrant(*t, q).is_none())
                && !world.unit_on(*t)
                && path::terrain(world, &mover, *t, None) < MAX
                && Some(*t) != inside
                && service::distance(world, here, *t) < REACH
                && !claimed.contains(t)
        };
        let places: Vec<usize> = self.world.tiles_of(x).into_iter().filter(free).collect();
        let place = match (tank_place, places.len()) {
            (Some((tile, l)), _) => {
                level = Some(l);
                tile
            }
            (None, 0) => here,
            (None, n) => places[self.ctx.rng.pick(n as u32) as usize],
        };
        if self.go_job_at(place, level) {
            return self.keeper_target(Some(place));
        }
        let give_up = self.staff().is_some_and(|s| {
            s.lost_tiles.push(place);
            s.lost_tiles.len() >= GIVE_UP
        });
        if give_up {
            if let Some(s) = self.staff() {
                (s.lost_tiles, s.lost_animals) =
                    (Vec::new(), [s.lost_animals.clone(), vec![id]].concat());
            }
            let name = self.name_of(id);
            self.trouble(name);
        }
        self.job_end();
    }

    /// 7.6 job 2 complete: a pile of food of the species' kind on the job's tile, paid as animal
    /// food, when food is still needed.
    fn food_put(&mut self, tile: Option<usize>) {
        let x = self.exhibit();
        if let (Some(tile), Some(x), Some((name, ty)), true) =
            (tile, x, self.food_type(), self.food_short())
        {
            let per = self
                .who()
                .and_then(|w| number(w, "cFoodPerTile"))
                .unwrap_or(100.0);
            let facing = self.ctx.rng.rand() & 7;
            let w = self.world.zoo.width as usize;
            let at = ((tile % w) as i32 * 64 + 32, (tile / w) as i32 * 64 + 32);
            let in_water = self.world.tank(x).is_some();
            let z = self
                .record()
                .job_target
                .pos
                .map(|p| p.2)
                .filter(|_| in_water);
            let names = ("food", "food", name);
            if dirt::put_z(self.world, ty, names, (at.0, at.1, z), facing, per as i32) {
                self.world.mark_rating(tile);
                self.spend(x, crate::money::ANIMAL_FOOD, cost(ty));
            }
        }
        self.job_end();
    }

    /// 7.7 job 3: the nearest sick animal of his kind, the way to it, and the animal held.
    fn heal_decide(&mut self) {
        let (x, who, here) = (self.exhibit(), self.who(), self.tile());
        let (Some(x), Some(who), Some(here)) = (x, who, here) else {
            return self.job_end();
        };
        let (_, lost) = self.lost();
        let claimed = service::claimed(self.world);
        let mine = service::animals(self.world, self.ctx.cat, x, who);
        let sick = mine
            .iter()
            .filter(|a| a.sick && !lost.contains(&a.id) && !claimed.contains(&a.tile));
        let Some(a) = sick.min_by_key(|a| service::distance(self.world, here, a.tile)) else {
            return self.job_end();
        };
        if !self.go_job(a.tile) {
            if let Some(s) = self.staff() {
                s.lost_animals.push(a.id);
            }
            let name = self.name_of(a.id);
            self.trouble(name);
            return self.job_end();
        }
        self.record().job_target.id = a.id;
        self.keeper_target(Some(a.tile));
        self.hold(a.id, true);
    }

    /// 7.7 job 4 complete: a fifth of the purchase cost is paid as animal health and the animal
    /// is healed.
    fn healed(&mut self, id: u32) {
        let (cat, x) = (self.ctx.cat, self.exhibit());
        if let Some(at) = self.world.find(id) {
            let k = animal::keys(self.world, cat, at);
            let price = type_of(cat, &self.world.zoo.entities[at]).map_or(0.0, |t| 0.2 * cost(t));
            if let Some(Part::Animal(a)) = self.world.units.get_mut(id).map(|u| &mut u.part) {
                a.heal(&k);
            }
            self.world.units.tell(format!("heal {id}"));
            if let Some(x) = x {
                self.spend(x, crate::money::ANIMAL_HEALTH, price);
                if let Some(e) = self.world.exhibits_mut().get_mut(x) {
                    e.rating.revise = true;
                }
            }
        }
        self.hold(id, false);
        self.job_end();
    }

    /// 7.8 job 5: the nearest pile of his dung, the way to it.
    fn rake_decide(&mut self) {
        let (x, who, here) = (self.exhibit(), self.who(), self.tile());
        let (Some(x), Some(who), Some(here)) = (x, who, here) else {
            return self.job_end();
        };
        let (lost, _) = self.lost();
        let claimed = service::claimed(self.world);
        let piles = service::dung(self.world, self.ctx.cat, x, who).into_iter();
        let piles = piles.filter(|t| !lost.contains(t) && !claimed.contains(t));
        let Some(pile) = piles.min_by_key(|&t| service::distance(self.world, here, t)) else {
            return self.job_end();
        };
        if self.go_job(pile) {
            return self.keeper_target(Some(pile));
        }
        if let Some(s) = self.staff() {
            s.lost_tiles.push(pile);
        }
        let name = self.exhibit_name(x);
        self.trouble(name);
        self.job_end();
    }

    /// 7.8 job 6 complete: the dung goes, the exhibit is rated again.
    fn raked(&mut self, tile: Option<usize>) {
        if let Some(t) = tile.filter(|&t| self.dung_at(t)) {
            let pile = self.world.quadrant(t, 3);
            let id = pile.and_then(|e| self.world.zoo.entities[e].common.as_ref().map(|c| c.id));
            if let Some(id) = id {
                self.ctx.remove(self.world, id);
            }
            self.world.mark_rating(t);
        }
        self.job_end();
    }

    /// Books what a keeper spent in `category` for the exhibit `x`, and rates it again.
    fn spend(&mut self, x: u32, category: usize, price: f32) {
        let price = if price.is_finite() {
            price.max(0.0)
        } else {
            0.0
        };
        self.world.units.spent.push((category, price));
        if let Some(e) = self.world.exhibits_mut().get_mut(x) {
            e.stats.upkeep[1] += price;
            e.stats.upkeep[2] += price;
            e.rating.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests;
