//! The viewing areas of the exhibits follow the map (habitat rating 10.1): built when an exhibit
//! is made or its fenced tiles change, and updated for each path tile placed or removed.

use super::*;
use crate::exhibit::viewing::Plan;

impl World {
    /// Builds the areas again of the exhibits whose tile list or fenced tiles are not the ones
    /// their areas were built from (a new exhibit, a fence placed or removed, tanks merged or
    /// split); the slots of the exhibits that are gone are cleared.
    pub(super) fn resize_areas(&mut self) {
        let ids: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        self.exhibits
            .views
            .slots
            .values_mut()
            .flatten()
            .filter(|s| s.is_some_and(|(x, _)| !ids.contains(&x)))
            .for_each(|s| *s = None);
        for id in ids {
            let Some(order) = self.exhibits.get(id).map(|e| e.order.clone()) else {
                continue;
            };
            let fenced = (order.iter().copied())
                .filter(|&t| [0, 2, 4, 6].into_iter().any(|d| self.closed(t, d)))
                .collect();
            let key = (order, fenced);
            if self.exhibits.get(id).is_some_and(|e| e.built == key) {
                continue;
            }
            let plan = Plan::of(self, id);
            self.exhibits.rebuild(&plan, id);
            if let Some(e) = self.exhibits.get_mut(id) {
                e.built = key;
            }
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
