//! The goals of animals: the ladder's need rungs, the keeper food goal, the drink goal and the
//! surface goal (goals 10.1, 11.8, 11.9; tank fauna 10.2).

use super::{BUILDING, Cx, GO_BUILDING, NONE, RANDOM};
use crate::animal::{NEED, food, tank};
use crate::behaviour::Target;
use crate::path;
use crate::unit::{Part, walk};

/// Task 0x72, keeper food, 0x7a, drink water, and 0x7c, go to the surface for air.
pub const KEEPER_FOOD: u8 = 114;
pub const DRINK: u8 = 122;
pub const SURFACE: u8 = 124;

impl Cx<'_, '_> {
    /// 10.1 point 4: a hungry animal that has found food sets the keeper food task, unless it
    /// has it, is in the building chain or is drinking. Points 6 and 7 keep the building chain and
    /// send the animal to the shelter it aims at; they end the ladder, so the thirst of point 8 and
    /// the breath of point 9 wait. Point 8 sets the drink task for a thirsty animal and point 9
    /// comes last, whatever those do.
    pub(super) fn animal_ladder(&mut self, task: u8) {
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        let (drinking, hungry, thirsty, breath) = (a.drinking, a.wants_food, a.thirsty, a.breath);
        let free = !(drinking || BUILDING.contains(&task));
        let food = free && hungry && task != KEEPER_FOOD;
        if food {
            self.set_task(KEEPER_FOOD, 1, 0, None);
        }
        if self.shelter_rung() {
            return;
        }
        if free && !food && thirsty && task != DRINK {
            self.set_task(DRINK, 1, 0, None);
        }
        // Point 9: an animal under water that has the breath of its threshold goes to the surface.
        if self.unit.tank == 4 && self.record().task != SURFACE {
            let k = crate::animal::keys(self.world, self.ctx.cat, self.at);
            if tank::needs_air(breath, &k, self.unit.tank) {
                self.set_task(SURFACE, 0, 0, None);
            }
        }
    }

    /// 10.1 points 6 and 7: an animal busy in the building chain keeps its task. One that aims at a
    /// building that serves `animalrest` and whose task is not done goes there when it is in no
    /// building task, or is in the walk to the building with an empty path (guest buildings 6.2;
    /// the whole goal is reset when it cannot start out). Whether either held.
    fn shelter_rung(&mut self) -> bool {
        let (task, done) = (self.record().task, self.record().done);
        let chain = BUILDING.contains(&task);
        let stalled = task == GO_BUILDING && self.unit.path.is_empty();
        if chain && !done && !stalled {
            return true;
        }
        let Part::Animal(a) = &self.unit.part else {
            return false;
        };
        let target = a.building;
        let serves = self.building(target).is_some_and(|b| b.1.satisfies(NEED));
        if done || !serves {
            return false;
        }
        if !self.set_out(target) {
            self.reset();
        }
        true
    }

    /// Tank fauna 10.2 start: the animal has to stand on a tank tile.
    pub(super) fn surface_init(&self) -> bool {
        self.tile().is_some_and(|t| self.world.tank_at(t).is_some())
    }

    /// Tank fauna 10.2 decide: with no target, the animal aims at the surface (its 3D target), draws
    /// a destination of its terrain mode and walks there; the goal is cleared when any step fails.
    pub(super) fn surface_decide(&mut self) {
        if self.record().target.tile.is_some() || !self.surface_init() {
            return;
        }
        self.unit.target = Target::Surface;
        self.unit.path.clear();
        let found = walk::surface_walk(self.unit, self.world, self.ctx.cat, self.ctx.rng, self.at);
        if let Some(to) = self.unit.sent.take() {
            return self.send_to(to);
        }
        if !(found && self.start_walk(false, false, false)) {
            self.set_task(NONE, 0, 0, None);
        }
    }

    /// Tank fauna 10.2 complete: at the surface the animal breathes (`bBreathe`, which empties its
    /// breath when it starts) and its target is cleared.
    pub(super) fn surface_complete(&mut self) {
        if self.start_set("bBreathe", false)
            && let Part::Animal(a) = &mut self.unit.part
        {
            a.breath = 0;
        }
        self.reset();
    }

    /// Tank fauna 5: sends the animal to tile `to`, which has the terrain mode it just took: it
    /// stops what it does and walks there at the base level of the tile.
    pub fn send_to(&mut self, to: usize) {
        crate::unit::movement::stop(self.unit);
        self.reset();
        self.set_task(RANDOM, 1, 0, None);
        let level = self.world.zoo.tiles[to].elevation;
        if self.path_to_level(to, false, Some(level)) {
            self.start_walk(false, false, true);
        }
    }

    /// 11.9 decide: with no target, the nearest water tile of the exhibit under the animal that is
    /// not salt (the first in the exhibit's tile order on a tie); the animal then goes next to it
    /// (`next_to`, never straight onto it) with `bFindOtherFood`. With no way the goal resets. The
    /// thirst goes down either way.
    pub(super) fn drink_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let water = {
            let (world, here) = (&*self.world, self.tile());
            let distance = squared_distance(world, here);
            // The salt water of a tank is terrain 10.
            here.and_then(|t| food::exhibit_of(world, t)).and_then(|x| {
                let tiles = world.tiles_of(x).into_iter();
                let fresh =
                    tiles.filter(|&t| world.is_water(t) && world.zoo.tiles[t].terrain != 10);
                fresh.min_by_key(|&t| distance(t))
            })
        };
        let walking = water.is_some_and(|w| self.next_to(w, false))
            && self.walk_set("bFindOtherFood", false, false);
        if let Part::Animal(a) = &mut self.unit.part {
            a.thirsty = false;
        }
        if !walking {
            self.reset();
        }
    }

    /// Animals needs 7.1, the "go next to a tile" helper: one draw `rand() % 100` on every call.
    /// With `direct` set, a draw below 50 and the unit not in a tank, the path goes to `to`
    /// itself. Otherwise to the first of its four straight neighbours (nearest to the unit first,
    /// the later direction first on a tie) that the unit may step onto from `to` and that has a
    /// path. A successful request leaves the unit's destination on the tile it was asked for, which
    /// the walk then takes as the goal's target. The `cKeepMoving` extension is not ported (M7).
    fn next_to(&mut self, to: usize, direct: bool) -> bool {
        let draw = self.ctx.rng.rand() % 100;
        let mover = crate::unit::movement::mover(self.unit, self.world, self.at, to);
        if direct && draw < 50 && !mover.deep {
            return self.path_to(to, false);
        }
        let near: Vec<usize> = {
            let world = &*self.world;
            let distance = squared_distance(world, self.tile());
            let mut near: Vec<(usize, usize)> = [6, 4, 2, 0]
                .into_iter()
                .filter_map(|d| Some((world.neighbour(to, d)?, d)))
                .collect();
            near.sort_by_key(|&(t, _)| distance(t));
            let settings = &world.units.finder.settings;
            // A unit in a tank steps between tank tiles at its own level (tank fauna 7.1).
            let level = mover.levels.1;
            let through = self.unit.through_building;
            let valid = |&(t, d): &(usize, usize)| match mover.deep {
                true => path::allowed_deep(
                    world,
                    &mover,
                    settings,
                    ((to, level), (t, level), d),
                    through,
                ),
                false => path::allowed(world, &mover, settings, (to, t, d), through),
            };
            near.iter().filter(|c| valid(c)).map(|&(t, _)| t).collect()
        };
        near.into_iter().any(|t| self.path_to(t, false))
    }

    /// 11.9 complete: at the water an animal that is not drinking starts `bEat` and another after
    /// it; once the set has ended the goal resets.
    pub(super) fn drink_complete(&mut self) {
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        let drinking = a.drinking;
        let wet = self.tile().is_some_and(|t| self.world.is_water(t));
        let eating = self.unit.runner.top() == Some("bEat") && self.unit.runner.running();
        if drinking && eating {
            return;
        }
        if !drinking && !wet && self.start_set("bEat", false) {
            self.unit.runner.request("bEat");
        } else {
            self.reset();
        }
        if let Part::Animal(a) = &mut self.unit.part {
            a.drinking = !drinking && !wet && self.unit.goal.record.task == DRINK;
        }
    }

    /// 11.8 decide: with no target, a piece of the species' food in the exhibit under the animal
    /// (not its home exhibit), chosen by up to three draws (the nearest, else the piece with the
    /// least left, else a random one); then the walk to it with `bFindKeeperFood`. With no piece
    /// or no way there the task is cleared (the walk is `next_to`, half of the time straight to the
    /// piece, else onto a neighbour of it, which then is the goal's target). The flag that started it goes down. With no exhibit
    /// under the animal nothing happens at all.
    pub(super) fn keeper_food_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let here = self.tile();
        let (world, cat) = (&*self.world, self.ctx.cat);
        // With no exhibit under the animal the decide does nothing: no draw, no flag, no clear.
        let Some(x) = here.and_then(|t| food::exhibit_of(world, t)) else {
            return;
        };
        if let Part::Animal(a) = &mut self.unit.part {
            a.wants_food = false;
        }
        let k = crate::animal::keys(world, cat, self.at);
        let regions = food::near(world, x, &k);
        let groups: Vec<_> = (regions.iter())
            .map(|&r| food::pieces(world, cat, &[r], k.food_kind))
            .collect();
        let width = world.zoo.width as usize;
        let at = |t: usize| ((t % width) as i64, (t / width) as i64);
        let (hx, hy) = here.map_or((0, 0), at);
        let distance = |tile: usize| (at(tile).0 - hx).pow(2) + (at(tile).1 - hy).pow(2);
        let chosen = food::choose(&groups, distance, self.ctx.rng);
        let walking = chosen.is_some_and(|p| self.next_to(p.tile, true))
            && self.walk_set("bFindKeeperFood", false, false);
        match chosen {
            Some(p) if walking => {
                if let Part::Animal(a) = &mut self.unit.part {
                    a.food = p.id;
                }
            }
            _ => self.set_task(NONE, 0, 0, None),
        }
    }

    /// 11.8 complete: at the food, an animal that is not eating faces it and takes its first bite
    /// if some is left there; one whose food is gone resets its goal.
    pub(super) fn keeper_food_complete(&mut self) {
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        if a.eating {
            return;
        }
        let m = food::meal(self.unit, self.world, self.data, self.at);
        if m.left > 0 {
            food::face(self.unit, self.world, self.at);
            food::feed(self);
        } else {
            self.reset();
        }
    }
}

/// The squared distance in tiles and height from tile `here` (the origin when none).
fn squared_distance(
    world: &crate::world::World,
    here: Option<usize>,
) -> impl Fn(usize) -> i64 + '_ {
    let width = world.zoo.width as usize;
    let at = move |t: usize| {
        (
            (t % width) as i64,
            (t / width) as i64,
            world.zoo.tiles[t].elevation as i64,
        )
    };
    let (hx, hy, hz) = here.map_or((0, 0, 0), at);
    move |t| {
        let (x, y, z) = at(t);
        (x - hx).pow(2) + (y - hy).pow(2) + (z - hz).pow(2)
    }
}
