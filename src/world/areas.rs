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

    /// The areas of a loaded zoo (10.1): the entities are added in file order, so the areas are
    /// what the placements and resizes of that sequence leave, not one build of the final map. The
    /// paths and fence pieces are replayed on a scratch world, and its areas go to the exhibits
    /// that hold the same tiles.
    pub(super) fn replay_areas(&mut self) {
        let placed = |s: &Shape| s.fence || matches!(s.kind, Kind::Path);
        if !self.shapes.iter().flatten().any(placed) {
            return;
        }
        let mut zoo = self.zoo.clone();
        zoo.entities.clear();
        zoo.habitat_tiles.clear();
        zoo.habitat_gates.clear();
        zoo.habitat_info.clear();
        zoo.tanks.clear();
        let mut scratch = World::with_language(zoo, |_| None, self.language.clone());
        scratch.in_zoo.clone_from(&self.in_zoo);
        for (e, shape) in self.zoo.entities.iter().zip(&self.shapes) {
            if let Some(s) = shape.filter(placed) {
                scratch.add(e.clone(), Some(s));
                scratch.events.clear();
            }
        }
        let mut found: BTreeMap<u32, u32> = BTreeMap::new();
        for e in scratch.exhibits.list() {
            let real = e.order.first().and_then(|&t| self.exhibits.region(t));
            if let Some(real) = real.filter(|r| !found.values().any(|v| v == r)) {
                found.insert(e.id, real);
            }
        }
        let mut views = std::mem::take(&mut scratch.exhibits.views);
        views.slots.values_mut().flatten().for_each(|s| {
            *s = s.and_then(|(x, a)| found.get(&x).map(|&r| (r, a)));
        });
        self.exhibits.views = views;
        let ids: Vec<u32> = self.exhibits.list().iter().map(|e| e.id).collect();
        let mut missing = Vec::new();
        for id in ids {
            let from = found.iter().find(|f| *f.1 == id).map(|f| *f.0);
            match from.and_then(|f| scratch.exhibits.get(f)) {
                Some(f) => {
                    let areas = f.areas.clone();
                    if let Some(e) = self.exhibits.get_mut(id) {
                        e.areas = areas;
                        e.rating.dirty = true;
                    }
                }
                None => missing.push(id),
            }
        }
        self.resize_areas(&missing);
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
