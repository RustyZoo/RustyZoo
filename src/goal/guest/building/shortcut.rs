//! The sit and trash shortcut of a guest whose building walk start failed (goals 12.2.1): the
//! energy seek and the first trash search end here when the need being served is still the same
//! and the building in the target's place still serves it, which happens only when the building
//! it was heading for was removed on the way (the walkers are not touched, goals 12.2.1 point 6).

use super::litter::FULL;
use super::{IN_BUILDING, wants};
use crate::goal::Cx;

/// The needs a building must satisfy to be a need building (guests 7.3).
const NEEDS: [&str; 10] = [
    "food",
    "drink",
    "bathroom",
    "gift",
    "trash",
    "energy",
    "eat",
    "grandstand",
    "fun",
    "showtrick",
];

impl Cx<'_, '_> {
    /// Guests 7.3: the guest's building target is a building that satisfies a need, looked up now.
    pub(in crate::goal) fn heads_for_need(&mut self) -> bool {
        let id = self.guest().map_or(0, |g| g.building);
        (self.building(id)).is_some_and(|(_, k)| NEEDS.iter().any(|n| k.satisfies(n)))
    }

    /// Goals 12.2.1 points 3 and 4: the clear goal with 1, for a guest whose building walk start
    /// failed while the need it served before was `prior`. Whether it set the guest out again,
    /// in which case the clear goal returns before the wandered flag; its caller resets the goal
    /// at once all the same (point 5).
    pub(in crate::goal) fn sit_or_trash(&mut self, prior: Option<&'static str>) -> bool {
        let id = self.guest().map_or(0, |g| g.building);
        let holds = |cx: &Self, need| (cx.building(id)).is_some_and(|(_, k)| k.satisfies(need));
        if IN_BUILDING.contains(&self.record().task) {
            return false;
        }
        match prior {
            Some("energy") if holds(self, "energy") => self.sit(),
            Some("trash") if holds(self, "trash") => self.bin(),
            _ => false,
        }
    }

    /// The sit branch: a bench drawn among all within reach, taken by the update with 0.
    fn sit(&mut self) -> bool {
        self.reset();
        let Some(radius) = self.radius() else {
            return false;
        };
        let Some(b) = self.in_reach("energy", radius) else {
            self.give_up();
            self.think(wants("energy").1, None);
            return false;
        };
        if let Some(g) = self.guest() {
            g.building = b;
        }
        let started = self.head_for(b, "energy");
        if !started {
            self.think(wants("energy").1, None);
        }
        started
    }

    /// The trash branch: a can drawn among all within reach, taken by the update with 0; one that
    /// cannot be taken drops the litter.
    fn bin(&mut self) -> bool {
        self.reset();
        let Some(radius) = self.radius() else {
            return false;
        };
        let failed = self.guest().is_some_and(|g| g.littered);
        let started = match self.in_reach("trash", radius) {
            // The flag is up: a can with room is only noted, with no target set.
            Some(b) if failed && self.fill_of(b).is_some_and(|f| f <= FULL) => return true,
            Some(b) if !failed => {
                if let Some(g) = self.guest() {
                    g.building = b;
                }
                self.head_for(b, "trash")
            }
            _ => false,
        };
        if !started {
            self.drop_litter();
            self.give_up();
        }
        started
    }

    /// The guest's `cSearchRadius`.
    fn radius(&self) -> Option<i64> {
        let k = crate::guest::keys(self.world, self.ctx.cat, self.at)?;
        Some(i64::from(k.search_radius))
    }

    /// The reset of a clear goal, with the wandered flag raised.
    fn give_up(&mut self) {
        self.reset();
        self.unit.goal.wandered = true;
    }
}
