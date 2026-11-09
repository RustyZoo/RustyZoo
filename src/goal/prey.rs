//! What a hunter does about its prey besides the chase goal (animals 21): the bored check's view of
//! the units round it, the path that leads the prey and the meal the behaviour sets end in.

use crate::animal::Sight;
use crate::goal::{CHASE, Cx};
use crate::unit::predator::find_prey;
use crate::unit::{Kind, Part};

/// The message of a staff member eaten by an animal.
const EATEN: i32 = 11002;

impl Cx<'_, '_> {
    /// What the bored check reads of the units round the animal (animals 21.1). With afraid up
    /// neither search runs.
    pub(crate) fn sight(&self) -> Sight {
        let (afraid, task) = (self.unit.frightened, self.unit.goal.record.task);
        let friendly = task == CHASE
            && self.target_unit().is_some_and(|(id, _)| {
                matches!(
                    self.world.units.get(id).map(|u| &u.part),
                    Some(Part::Animal(_))
                ) && self.same_type(id)
            });
        Sight {
            afraid,
            task,
            threat: !afraid && self.predator(0) != 0,
            chaser: !afraid && self.predator(1) != 0,
            prey: find_prey(self.world, self.ctx.cat, self.unit, self.at, 1) != 0,
            friendly,
        }
    }

    /// 21.2 point 3, the lead path: to a node up to `cChaseLookAhead` steps ahead on the target's
    /// own path, never one farther from the hunter than the target; one draw while the target
    /// has a path. No partial path.
    pub(super) fn lead_path(&mut self) -> bool {
        let Some((id, at)) = self.target_unit() else {
            return false;
        };
        let w = self.world.zoo.width as i32;
        let grid = |at: usize| {
            let c = self.world.zoo.entities[at].common.as_ref()?;
            Some((c.x.div_euclid(64), c.y.div_euclid(64), c.z.div_euclid(16)))
        };
        let (Some(h), Some(p)) = (grid(self.at), grid(at)) else {
            return false;
        };
        let theirs = self
            .world
            .units
            .get(id)
            .map(|u| (u.path.clone(), u.levels.clone()));
        let (path, levels) = theirs.unwrap_or_default();
        let to = |(x, y, _): (i32, i32, i32)| self.world.tile_index(x, y);
        let n = path.len() as i32;
        if n == 0 {
            return to(p).is_some_and(|t| self.path_to(t, false));
        }
        let near = |a: (i32, i32, i32), b: (i32, i32, i32)| {
            let sq = |d: i32| d.wrapping_mul(d);
            sq(a.0 - b.0)
                .wrapping_add(sq(a.1 - b.1))
                .wrapping_add(sq(a.2 - b.2))
        };
        let mut l = self.ctx.cat.chase_look_ahead.unwrap_or(1);
        if l.wrapping_add(1) > n {
            l = n - 1;
        }
        let d2 = near(p, h);
        if l.wrapping_mul(l) > d2 {
            l = ((f64::from(d2).sqrt() - 1.0) as i32).max(0);
        }
        let r = self
            .ctx
            .rng
            .rand()
            .checked_rem((l as u32).wrapping_add(1))
            .unwrap_or(0) as usize;
        let node = path
            .get(r)
            .copied()
            .filter(|&t| t < self.world.zoo.tiles.len());
        let ahead = node.map(|t| {
            let z = levels
                .get(r)
                .copied()
                .unwrap_or(self.world.zoo.tiles[t].elevation);
            ((t as i32 % w), (t as i32 / w), z)
        });
        let goal = match (node, ahead) {
            (Some(t), Some(n)) if near(n, h) <= d2 => Some(t),
            _ => to(p),
        };
        goal.is_some_and(|t| self.path_to(t, false))
    }

    /// `fCaughtPrey` (21.5 point 1) of an animal: the target unit, else the kept prey, is consumed
    /// and let go, and the hunter's own hunt ends at once.
    pub(crate) fn caught_prey(&mut self) {
        let Some(kept) = self.hunter().map(|a| a.prey) else {
            return;
        };
        let target = self.target_unit().map(|(id, _)| id);
        let eaten = target.or_else(|| {
            Some(kept).filter(|&k| {
                self.world.find(k).is_some()
                    && self.world.units.get(k).is_some_and(|u| !u.suspended())
            })
        });
        let Some(id) = eaten else {
            return self.mark(|a| a.prey = 0);
        };
        let kind = self.world.units.get(id).map(|u| u.kind);
        if kind != Some(Kind::Guest) || self.flag(self.at, "cIsManEater") {
            if matches!(
                kind,
                Some(Kind::Keeper | Kind::Worker | Kind::Guide | Kind::Heli)
            ) {
                let names = vec![self.name_of(id), self.name_of(self.unit.id)];
                self.world.units.post_named(EATEN, self.unit.id, names);
            }
            self.remove_unit(id);
        }
        if let Some(t) = self.world.units.get_mut(id) {
            t.predator = 0;
        }
        self.unit.hidden = false;
        self.unit.held = false;
        self.reset();
    }

    /// `fKillPrey` (21.5 point 4) of an animal: the target unit is hidden and removed.
    pub(crate) fn kill_prey(&mut self) {
        if let (Some((id, _)), Some(_)) = (self.target_unit(), self.hunter()) {
            self.remove_unit(id);
        }
    }

    /// The unit `id` is made invisible and set removed.
    fn remove_unit(&mut self, id: u32) {
        if let Some(at) = self.world.find(id) {
            self.world.hide_unit(at);
        }
        self.ctx.remove(self.world, id);
    }
}
