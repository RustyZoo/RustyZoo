//! The guest panel in the game (information panels 4, 8): what opens it (a guest selected on the
//! map, [`App::units_sync`]), its buttons and name field, and the look at what the player did to
//! it. The panel itself is [`crate::panel::guest`].

use super::*;
use crate::panel::guest;

impl App<'_> {
    /// Selects the guest with entity id `id` and shows its panel, filled at once (4.3).
    pub(super) fn open_guest(&mut self, id: u32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        game.selected = Some(id);
        game.guest.select(id);
        ui.show(guest::LAYOUT);
    }

    /// Hides the guest panel: the next look clears the selection.
    pub(super) fn close_guest(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.set_hidden(guest::LAYOUT, true);
        }
    }

    /// Notices what the player did to the guest panel and the selected guest since the last look:
    /// a closed panel or a guest that left the world clears the selection (`panel guest none`),
    /// and the panel sits above the exhibit panel while it is shown (4.2).
    pub(super) fn guest_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let gone = (game.guest.selected).is_some_and(|id| game.world.find(id).is_none());
        if gone || !ui.shown(guest::LAYOUT) {
            if game.guest.selected.is_some() {
                self.panel_lines.push("guest none".into());
            }
            game.guest.deselect();
            ui.set_hidden(guest::LAYOUT, true);
        }
        ui.set_alt(guest::LAYOUT, ui.shown(exhibit::LAYOUT));
    }

    /// The pick-up button: the selected guest is picked up as a second click does.
    pub(super) fn guest_pick_up(&mut self) {
        if let Some(id) = self.game.as_ref().and_then(|g| g.guest.selected) {
            self.carry_up(id);
        }
    }

    /// The player ended editing the name field: a text with a visible character is the name.
    pub(super) fn guest_rename(&mut self) {
        self.entity_hook(Subject::Guest, guest::NAME);
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let at = game.guest.selected.and_then(|id| game.world.find(id));
        let bytes = ui.text(guest::NAME).unwrap_or_default();
        if let Some(c) = at.and_then(|at| game.world.zoo.entities[at].common.as_mut())
            && exhibits::visible(&bytes)
        {
            c.name = bytes.iter().map(|&b| crate::text::from_cp1252(b)).collect();
        }
        game.guest.reselect();
    }
}
