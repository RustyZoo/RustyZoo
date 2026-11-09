//! The recovery team panel in the game (staff 18): what opens it (the helicopter selected on the
//! map), what closes it, and its name field and fire button. The panel itself is
//! [`crate::panel::recovery`].

use super::*;
use crate::panel::unitlist as lists;

impl App<'_> {
    /// Selects the helicopter with entity id `id` and shows its panel, filled at once.
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

    /// Notices what the player did to the panel and the selected helicopter since the last look:
    /// a closed panel or a helicopter that left the world clears the selection (`panel team
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

    /// The fire button: the team is removed as the bulldozer removes it, after the question with
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
