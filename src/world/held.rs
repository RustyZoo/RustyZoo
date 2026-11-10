//! The units held off the map or off a list (carried, hidden, unlisted): how they are set, and
//! how they keep it across the rebuild of the scene.

use super::World;
use crate::render::order::Held;

/// How the entity list moved since the scene was built: the owner index still counts the old one.
#[derive(Clone, Copy)]
pub(super) enum Shift {
    /// The entity at this position left the list.
    Removed(usize),
    /// An entity was put in the list at this position.
    Inserted(usize),
}

impl Shift {
    /// The position now of the entity that stood at `old`; none if it is the one removed.
    fn now(self, old: usize) -> Option<usize> {
        match self {
            Shift::Removed(r) if old == r => None,
            Shift::Removed(r) => Some(old - usize::from(old > r)),
            Shift::Inserted(i) => Some(old + usize::from(old >= i)),
        }
    }
}

impl Shift {
    /// The place of a slot between two entities (the one a held building leaves) after the move:
    /// it stays after the entities before it.
    pub(super) fn slot(self, at: usize) -> usize {
        match self {
            Shift::Removed(r) => at - usize::from(r < at),
            Shift::Inserted(i) => at + usize::from(i < at),
        }
    }
}

impl World {
    /// The ids of the units held off the map or off a list, with how. They keep how they are held
    /// across the rebuild: a carried unit is on no list and not painted (simulation 6.2), a
    /// hidden user is not painted but stays on its list (guest buildings 6.4 point 6), a unit
    /// off its list stays off it (6.7.1).
    pub(super) fn held_units(&self, shift: Option<Shift>) -> Vec<(u32, Held)> {
        (self.scene.held())
            .filter_map(|(o, held)| {
                let at = shift.map_or(Some(self.owner[o]), |s| s.now(self.owner[o]))?;
                let id = self.zoo.entities.get(at)?.common.as_ref()?.id;
                Some((id, held))
            })
            .collect()
    }

    /// Holds the units again in the rebuilt scene; one that is gone is let go.
    pub(super) fn hold_again(&mut self, held: Vec<(u32, Held)>) {
        for (id, held) in held {
            let Some(at) = self.find(id) else {
                continue;
            };
            match held {
                Held::Unlisted => self.unlist_unit(at),
                Held::Hidden => self.hide_unit(at),
                Held::Lifted => self.lift_unit(at),
            }
        }
    }

    /// Takes the unit at position `at` of the entity list off the map, as the pick-up does (unit
    /// placing 8.2): it stays in the list, stands on no tile and is not drawn, until
    /// [`World::place_unit`] puts it down.
    pub(crate) fn lift_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.lift_unit(o);
            self.dirty = true;
        }
    }

    /// Stops painting the unit at position `at` of the entity list; it stays on its tile's unit
    /// list (a user hidden in a building, guest buildings 6.7.1).
    pub(crate) fn hide_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.hide_unit(o);
            self.dirty = true;
        }
    }

    /// Takes the unit at position `at` of the entity list off its tile's unit list only (guest
    /// buildings 6.7.1): it keeps its place, is drawn as before and joins a list again at its
    /// first change of tile.
    pub(crate) fn unlist_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.unlist_unit(o);
            self.dirty = true;
        }
    }

    /// Whether the entity at position `at` is painted.
    #[cfg(test)]
    pub(crate) fn is_drawn(&self, at: usize) -> bool {
        self.fits(at)
    }

    /// Paints the unit at position `at` where it stands, on no tile list.
    pub(crate) fn paint_unit(&mut self, at: usize) {
        if let Ok(o) = self.owner.binary_search(&at) {
            self.scene.paint_unit(o);
            self.dirty = true;
        }
    }
}
