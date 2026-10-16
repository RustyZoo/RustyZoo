//! The goals of animals: the ladder's need rungs and the keeper food goal (goals 10.1, 11.8).
//!
//! Drinking is a later issue's.

use super::{BUILDING, Cx, NONE};
use crate::animal::food;
use crate::unit::Part;

/// Task 0x72, keeper food.
pub const KEEPER_FOOD: u8 = 114;

impl Cx<'_, '_> {
    /// 10.1 point 4: a hungry animal that has found food sets the keeper food task, unless it
    /// has it or is in the building chain. (The drinking flag that also blocks it is a later
    /// issue's.)
    pub(super) fn animal_ladder(&mut self, task: u8) {
        let Part::Animal(a) = &self.unit.part else {
            return;
        };
        if a.wants_food && task != KEEPER_FOOD && !BUILDING.contains(&task) {
            self.set_task(KEEPER_FOOD, 1, 0, None);
        }
    }

    /// 11.8 decide: with no target, a piece of the species' food in the exhibit under the animal
    /// (the home exhibit is not kept yet), chosen by two draws of `rand & 3` (a random piece, the
    /// piece with the least left, else the nearest); then the walk to it with `bFindKeeperFood`.
    /// With no piece or no way there the task is cleared. The flag that started it goes down.
    pub(super) fn keeper_food_decide(&mut self) {
        if self.record().target.tile.is_some() {
            return;
        }
        if let Part::Animal(a) = &mut self.unit.part {
            a.wants_food = false;
        }
        let here = self.tile();
        let (world, cat) = (&*self.world, self.ctx.cat);
        let k = crate::animal::keys(world, cat, self.at);
        let x = here.and_then(|t| food::exhibit_of(world, t));
        let regions = x.map(|x| food::near(world, x, &k)).unwrap_or_default();
        let pieces = food::pieces(world, cat, &regions, k.food_kind);
        let width = world.zoo.width as usize;
        let at = |t: usize| ((t % width) as i64, (t / width) as i64);
        let (hx, hy) = here.map_or((0, 0), at);
        let distance = |tile: usize| (at(tile).0 - hx).pow(2) + (at(tile).1 - hy).pow(2);
        let chosen = food::choose(&pieces, distance, self.ctx.rng);
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
