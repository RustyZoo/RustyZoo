//! The exhibit panel in the game: what opens it (the gate of an exhibit under the pointer, the
//! Exhibit List button), what its list and fields do, and its refresh on the interface clock
//! (information panels 4, 5, 10).

use super::*;
use crate::panel::exhibit::{BUTTON, LAYOUT, NAME, PLAQUE};
use crate::text::from_cp1252;

impl App<'_> {
    /// A release on the map with no tool: the topmost entity under the pointer is selected, and
    /// the gate of an exhibit opens the exhibit panel on that exhibit (4.1). Any other entity
    /// opens no panel yet.
    pub(super) fn select(&mut self) {
        let (Some(game), Some(ui)) = (&self.game, &self.ui) else {
            return;
        };
        let world = &game.world;
        let point = game.camera.view_point(ui.pointer());
        let hit = (game.staged)
            .pick(point, 0, |id| world.find(id).is_some())
            .and_then(|id| world.find(id));
        let gate = hit.filter(|&at| world.is_door(at)).and_then(|at| {
            let id = world.zoo.entities[at].common.as_ref()?.id;
            (world.exhibits().list().iter())
                .find(|e| e.gate == Some(id))
                .map(|e| e.id)
        });
        if let Some(id) = gate {
            self.open_exhibit(id);
        }
    }

    /// Shows the exhibit panel, with the Exhibit List button down, on exhibit `id`.
    fn open_exhibit(&mut self, id: u32) {
        let Some(ui) = &mut self.ui else { return };
        if !ui.shown(LAYOUT) && !ui.press_button(BUTTON) {
            ui.show(LAYOUT);
        }
        self.panel_sync();
        let Some(game) = &mut self.game else { return };
        game.panel.current = Some(id);
        game.panel.clock.force = true;
    }

    /// Notices that the layout was shown or hidden since the last look (4.2): the button follows
    /// it, a shown panel starts with no exhibit and a hidden one forgets it.
    fn panel_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let shown = ui.shown(LAYOUT);
        if shown != game.panel.shown {
            ui.set_button(BUTTON, shown, true);
            match shown {
                true => game.panel.opened(),
                false => game.panel.closed(),
            }
        }
        if !ui.shown(PLAQUE) {
            game.panel.plaque = None;
        }
    }

    /// A row of the exhibit list was selected (10.1): that exhibit is current and the view
    /// centres on its gate tile; no row makes none current.
    pub(super) fn panel_row(&mut self, row: i32) {
        let Some(game) = &mut self.game else { return };
        let id = usize::try_from(row)
            .ok()
            .and_then(|r| game.panel.exhibit_of(r));
        game.panel.current = id;
        game.panel.clock.force = true;
        let slot = id.and_then(|id| game.world.gate_slot(id));
        if let Some((tile, _)) = slot {
            let width = game.world.zoo.width as usize;
            let (x, y) = ((tile % width) as u32, (tile / width) as u32);
            game.camera.centre_on(&game.world.zoo, x, y);
            if let Some(ui) = &mut self.ui {
                ui.touch();
            }
        }
    }

    /// A row of the animal list was selected: the recommendations plaque shows that animal (the
    /// original opens the animal panel, which does not exist yet, and its button shows the plaque).
    pub(super) fn panel_animal(&mut self, row: i32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let id = usize::try_from(row)
            .ok()
            .and_then(|r| game.panel.animal_of(r));
        if let Some(id) = id {
            game.panel.show_plaque(id);
            ui.show(PLAQUE);
        }
    }

    /// The player ended editing the name field (10.2): a text with a visible character is the
    /// exhibit's name; the field and the list show the name again either way.
    pub(super) fn panel_rename(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let Some(id) = game.panel.current else { return };
        let bytes = ui.text(NAME).unwrap_or_default();
        if exhibits::visible(&bytes) {
            let text: String = bytes.iter().map(|&b| from_cp1252(b)).collect();
            self.exhibit_lines.push(format!("name {id} {text:?}"));
            game.world.rename(id, text);
        }
        game.panel.clock.force = true;
    }

    /// Refreshes the panel and the plaque when due (5.1): at the interface update, not while the
    /// game is paused, and at once when something forced it. Writes the `panel` lines.
    pub(super) fn panel_pass(&mut self, now: u64) {
        self.panel_sync();
        let ticking = self.fired.interface.is_some() && !self.paused() && !self.modal();
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        if ui.shown(LAYOUT) && game.panel.clock.due(now, ticking) {
            let lines = game.panel.refresh(ui, &game.world, cat, vfs);
            self.panel_lines.extend(lines);
        }
        if game.panel.plaque.is_some() && game.panel.plaque_clock.due(now, ticking) {
            let lines = game.panel.refresh_plaque(ui, &game.world, cat, vfs);
            self.panel_lines.extend(lines);
        }
    }
}
