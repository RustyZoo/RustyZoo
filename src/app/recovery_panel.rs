//! The recovery team panel in the game (staff 18): what opens it (the helicopter base selected, by a click on it or on its
//! helicopter), what closes it, and its name field and fire button. The panel itself is
//! [`crate::panel::recovery`].

use super::*;
use crate::entity::Catalogue;
use crate::panel::unitlist as lists;
use crate::world::World;

/// Whether entity `id` is the helicopter base (building name id 8031).
fn is_base(world: &World, cat: &Catalogue, id: u32) -> bool {
    let at = world.find(id).and_then(|at| world.zoo.entities.get(at));
    at.and_then(|e| crate::entity::type_of(cat, e))
        .is_some_and(crate::build::drt_base)
}

/// The base a click on entity `id` selects: the base of a helicopter (unit placing 8.1 point 1),
/// else `id` itself.
pub(super) fn selected_for(world: &World, cat: &Catalogue, id: u32) -> u32 {
    match world.units.get(id).map(|u| (u.kind, &u.part)) {
        Some((crate::unit::Kind::Heli, crate::unit::Part::Staff(s)))
            if is_base(world, cat, s.heli.base) =>
        {
            s.heli.base
        }
        _ => id,
    }
}

impl App<'_> {
    /// Whether entity `id` is the helicopter base (building name id 8031), whose panel this is.
    pub(super) fn is_base(&self, id: u32) -> bool {
        let game = self.game.as_ref();
        game.is_some_and(|g| is_base(&g.world, self.renderer.catalogue(), id))
    }

    /// Selects the base with entity id `id` and shows its panel, filled at once.
    pub(super) fn open_recovery(&mut self, id: u32) {
        playing!(self, game, ui);
        game.selected = Some(id);
        game.recovery.select(id);
        ui.show(recovery::LAYOUT);
    }

    /// Hides the panel: the next look clears the selection.
    pub(super) fn close_recovery(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.set_hidden(recovery::LAYOUT, true);
        }
    }

    /// Notices what the player did to the panel and the selected base since the last look:
    /// a closed panel or a base that left the world clears the selection (`panel team
    /// none`), and the panel sits above the exhibit panel while it is shown.
    pub(super) fn recovery_sync(&mut self) {
        playing!(self, game, ui);
        let gone = (game.recovery.selected).is_some_and(|id| game.world.find(id).is_none());
        if gone || !ui.shown(recovery::LAYOUT) {
            if game.recovery.selected.is_some() {
                self.lines.panel.push("team none".into());
            }
            game.recovery.deselect();
            ui.set_hidden(recovery::LAYOUT, true);
        }
        ui.set_alt(
            recovery::LAYOUT,
            ui.shown(exhibit::LAYOUT) || lists::any_shown(ui),
        );
    }

    /// The fire button: the base is removed as the bulldozer removes it, after the question with
    /// its monthly cost when `[UI] confirmSell` asks for one (staff 18 point 5).
    pub(super) fn recovery_fire(&mut self) {
        if let Some(id) = self.game.as_ref().and_then(|g| g.recovery.selected) {
            self.remove_one(id);
        }
    }

    /// The player ended editing the name field: a text with a visible character is the name.
    pub(super) fn recovery_rename(&mut self) {
        let selected = self.game.as_ref().and_then(|g| g.recovery.selected);
        self.entity_hook(Subject::Keeper, recovery::NAME, selected);
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let at = game.recovery.selected.and_then(|id| game.world.find(id));
        let bytes = ui.text(recovery::NAME).unwrap_or_default();
        if let Some(at) = at {
            game.world.rename_entity(at, &bytes);
        }
        game.recovery.reselect();
    }
}
