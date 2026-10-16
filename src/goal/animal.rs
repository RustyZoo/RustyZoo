//! The goals of animals: the ladder's need rungs, the keeper food goal and the drink goal (goals
//! 10.1, 11.8, 11.9).

use super::{BUILDING, Cx, NONE};
use crate::animal::food;
use crate::unit::Part;

/// Task 0x72, keeper food, and 0x7a, drink water.
pub const KEEPER_FOOD: u8 = 114;
pub const DRINK: u8 = 122;

impl Cx<'_, '_> {
    /// 10.1 point 4: a hungry animal that has found food sets the keeper food task, unless it
    /// has it, is in the building chain or is drinking; point 8 does the same for a thirsty one
    /// and the drink task.
    pub(super) fn animal_ladder(&mut self, task: u8) {
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        if a.drinking || BUILDING.contains(&task) {
            return;
        }
        if a.wants_food && task != KEEPER_FOOD {
            self.set_task(KEEPER_FOOD, 1, 0, None);
        } else if a.thirsty && task != DRINK {
            self.set_task(DRINK, 1, 0, None);
        }
    }

    /// 11.9 decide: with no target, the nearest water tile of the exhibit under the animal that is
    /// not salt (the first in the exhibit's tile order on a tie); the animal then goes to one of
    /// the four straight neighbours of it, nearest to the animal first and the later direction
    /// first on a tie, each valid when the step from the water to it is allowed for the animal
    /// (pathfinding 4.2); the first with a path wins, with `bFindOtherFood`. With none the goal
    /// resets. The thirst goes down either way.
    pub(super) fn drink_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        let (world, here) = (&*self.world, self.tile());
        let width = world.zoo.width as usize;
        let at = |t: usize| {
            (
                (t % width) as i64,
                (t / width) as i64,
                world.zoo.tiles[t].elevation as i64,
            )
        };
        let (hx, hy, hz) = here.map_or((0, 0, 0), at);
        let distance = |t: usize| {
            let (x, y, z) = at(t);
            (x - hx).pow(2) + (y - hy).pow(2) + (z - hz).pow(2)
        };
        // The salt water of a tank is terrain 10.
        let water = here.and_then(|t| food::exhibit_of(world, t)).and_then(|x| {
            let tiles = world.tiles_of(x).into_iter();
            let fresh = tiles.filter(|&t| world.is_water(t) && world.zoo.tiles[t].terrain != 10);
            fresh.min_by_key(|&t| distance(t))
        });
        let mut walking = false;
        if let Some(w) = water {
            let mut shore: Vec<(usize, usize)> = [6, 4, 2, 0]
                .into_iter()
                .filter_map(|d| Some((world.neighbour(w, d)?, d)))
                .collect();
            shore.sort_by_key(|&(t, _)| distance(t));
            let mover = crate::unit::movement::mover(self.unit, self.world, self.at, w);
            let settings = &self.world.units.finder.settings;
            let valid = |&(t, d): &(usize, usize)| {
                crate::path::allowed(
                    self.world,
                    &mover,
                    settings,
                    (w, t, d),
                    self.unit.through_building,
                )
            };
            let shore: Vec<usize> = shore.iter().filter(|c| valid(c)).map(|&(t, _)| t).collect();
            walking = shore.into_iter().any(|t| self.path_to(t, false))
                && self.walk_set("bFindOtherFood", false, false);
        }
        if let Part::Animal(a) = &mut self.unit.part {
            a.thirsty = false;
        }
        if !walking {
            self.reset();
        }
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
    /// (the home exhibit is not kept yet), chosen by up to three draws (the nearest, else the
    /// piece with the least left, else a random one); then the walk to it with `bFindKeeperFood`.
    /// With no piece or no way there the task is cleared. The flag that started it goes down. With
    /// no exhibit under the animal nothing happens at all.
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
        let walking = chosen.is_some_and(|p| self.path_to(p.tile, false))
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
