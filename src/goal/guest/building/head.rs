//! The walk start of a guest on a building target (guest buildings 6.2): [`Cx::head_for`] and the
//! building step it ends in.

use super::CROWDED;
use crate::goal::Cx;

impl Cx<'_, '_> {
    /// Guest buildings 6.2: the guest takes building `b` for `need` and sets out for the entrance
    /// of the nearest open slot: task 0x66, or 0x67 for a type with a direct entrance. Whether it
    /// did. If not (guest buildings 6.2 point 2, goals 12.2.1 point 2): a guest thinks 10243 when
    /// the building is at its capacity and the goal record has a target tile; a returning guest
    /// recovers from the water and the start counts as done; any other guest clears its goal,
    /// which raises its wandered flag.
    pub(in crate::goal) fn head_for(&mut self, b: u32, need: &'static str) -> bool {
        self.head_for_at(b, need, false)
    }

    /// [`Cx::head_for`]; `shortcut` for the searches that pass 1 to the walk start (the energy
    /// seek and the first trash search), whose failure is never the water recovery.
    pub(in crate::goal) fn head_for_at(
        &mut self,
        b: u32,
        need: &'static str,
        shortcut: bool,
    ) -> bool {
        let mut prior = None;
        if let Some(g) = self.guest() {
            prior = g.need;
            (g.need, g.building) = (Some(need), b);
        }
        self.building_step(b, shortcut, prior)
    }

    /// The building step of guest buildings 6.2 on the guest's building target, whose need being
    /// served (`prior` is the one before, for the shortcut) is kept as it is.
    pub(in crate::goal) fn building_step(
        &mut self,
        b: u32,
        shortcut: bool,
        prior: Option<&'static str>,
    ) -> bool {
        let tiled = self.record().target.tile.is_some();
        if self.set_out(b) {
            return true;
        }
        let returning = self.guest().is_some_and(|g| g.returning.is_some());
        if self.guest().is_some() {
            if tiled && self.at_capacity(b) {
                self.think(CROWDED, Some(b));
            }
            if returning && !shortcut {
                self.recover_from_water(b);
                return true;
            }
            // Goals 12.2.1: the energy seek and the first trash search end in the shortcut.
            if !(shortcut && self.sit_or_trash(prior)) {
                self.clear_goal();
            }
        }
        self.clear_target();
        false
    }
}
