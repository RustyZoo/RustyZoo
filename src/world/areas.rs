//! The viewing areas of the exhibits follow the map (habitat rating 10.1): resized when an exhibit
//! is made, split, merged or morphed, and updated for each path tile placed or removed.

use super::*;
use crate::exhibit::viewing::Plan;

impl World {
    /// Resizes the areas of the exhibits `ids` (10.1): the ones the original resizes, an exhibit
    /// made, split, merged, morphed, or a tank merged or split; the slots of the exhibits that
    /// are gone are cleared.
    pub(super) fn resize_areas(&mut self, ids: &[u32]) {
        let alive: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        self.exhibits
            .views
            .slots
            .values_mut()
            .flatten()
            .filter(|s| s.is_some_and(|(x, _)| !alive.contains(&x)))
            .for_each(|s| *s = None);
        for &id in ids {
            let plan = Plan::of(self, id);
            self.exhibits.resize(&plan, id);
        }
    }

    /// The path tiles are now the ones of `before`, but for the ones placed or removed by the
    /// last change: the areas follow.
    pub(super) fn follow_paths(&mut self, before: &[bool]) {
        let width = self.zoo.width as usize;
        for (t, &was) in before.iter().enumerate() {
            let now = self.path(t).is_some();
            if now && !was {
                let near = self.exhibits.near(t, width);
                let plans = near.into_iter().map(|id| (id, Plan::of(self, id)));
                let plans = plans.collect();
                self.exhibits.path_placed(t, plans);
            } else if was && !now {
                self.exhibits.path_removed(t);
            }
        }
    }

    /// Which tiles hold a path.
    pub(super) fn path_tiles(&self) -> Vec<bool> {
        (0..self.zoo.tiles.len())
            .map(|t| self.path(t).is_some())
            .collect()
    }
}
