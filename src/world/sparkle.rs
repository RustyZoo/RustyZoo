//! The state of the water sparkles of the tanks (map drawing 9.7.10): per tank a timer and the
//! sparkles it made, each an entity id and an age. The attempt that makes one is [`crate::sim`]'s.

use super::World;

/// The age in ms above which a sparkle goes, and the numerator of the timer reset.
const LIFE: u32 = 4000;
const PERIOD: u32 = 40_000;

/// One tank's sparkles: the timer `S` (0 for a new tank, not saved) and the list in the order
/// made.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Sparkles {
    pub timer: i32,
    pub list: Vec<(u32, u32)>,
}

impl World {
    /// The tanks and, for the hash, their sparkles.
    pub(crate) fn sparkles(&self) -> impl Iterator<Item = (u32, &Sparkles)> {
        self.sparkles.iter().map(|(&id, s)| (id, s))
    }

    /// The sparkles flagged for removal and not yet gone: they are not drawn.
    pub(crate) fn doomed(&self) -> &[u32] {
        &self.doomed
    }

    /// The sparkles flagged for removal, for the next removal pass.
    pub(crate) fn take_doomed(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.doomed)
    }

    /// Every sparkle of tank `id` is flagged for removal at once and the list emptied; the timer
    /// stays (9.7.10 point 6).
    pub(crate) fn drop_sparkles(&mut self, id: u32) {
        if let Some(s) = self.sparkles.get_mut(&id) {
            self.doomed.extend(s.list.drain(..).map(|(e, _)| e));
        }
    }

    /// The same for every tank: a save.
    pub(crate) fn drop_all_sparkles(&mut self) {
        let ids: Vec<u32> = self.sparkles.keys().copied().collect();
        ids.into_iter().for_each(|id| self.drop_sparkles(id));
    }

    /// The ids of the tanks that are, in list order, for one pass of the sparkles; the sparkles of
    /// a tank that is gone are flagged for removal first (it was deleted).
    pub(crate) fn sparkle_tanks(&mut self) -> Vec<u32> {
        let tanks = self.tank_ids();
        let gone: Vec<u32> = (self.sparkles.keys().copied())
            .filter(|id| !tanks.contains(id))
            .collect();
        for id in gone {
            self.drop_sparkles(id);
            self.sparkles.remove(&id);
        }
        tanks
    }

    /// Point 2 and 3: every sparkle of tank `id` ages by `dt`, those above 4000 ms go; then the
    /// timer loses `dt` and, when it is below 0, restarts at `40000 / n` (`n` the tiles of the
    /// tank) and tells that an attempt is due.
    pub(crate) fn sparkle_tick(&mut self, id: u32, dt: u32) -> bool {
        let n = self.exhibits.get(id).map_or(0, |e| e.order.len() as u32);
        let s = self.sparkles.entry(id).or_default();
        let mut gone = Vec::new();
        s.list.retain_mut(|(entity, age)| {
            *age = age.saturating_add(dt);
            let keep = *age <= LIFE;
            if !keep {
                gone.push(*entity);
            }
            keep
        });
        self.doomed.extend(gone);
        s.timer = s.timer.wrapping_sub(dt as i32);
        let due = s.timer < 0;
        if due {
            s.timer = PERIOD.checked_div(n).map_or(0, |v| v as i32);
        }
        due
    }

    /// Tank `id` made sparkle `entity`, with age 0.
    pub(crate) fn push_sparkle(&mut self, id: u32, entity: u32) {
        self.sparkles.entry(id).or_default().list.push((entity, 0));
    }
}
