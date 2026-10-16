//! The information panels in the game: what opens the exhibit panel (the gate of an exhibit under
//! the pointer, the exhibit list button) and the animal panel (an animal under the pointer, a row
//! of the exhibit panel's animal list), what their lists, fields and buttons do, and their
//! refresh on the interface clock (information panels 4, 5, 6, 10).

use super::*;
use crate::panel::animal;
use crate::panel::exhibit::{BUTTON, LAYOUT, NAME, PLAQUE};
use crate::text::from_cp1252;
use crate::unit::Part;

impl App<'_> {
    /// A release on the map with no tool: the topmost entity under the pointer is selected
    /// (information panels 4.1). An animal opens the animal panel, the gate of an exhibit the
    /// exhibit panel on that exhibit; anything else, or nothing, clears the selection. Other
    /// entities open no panel yet.
    pub(super) fn select(&mut self) {
        let (Some(game), Some(ui)) = (&self.game, &self.ui) else {
            return;
        };
        let world = &game.world;
        let point = game.camera.view_point(ui.pointer());
        let hit = (game.staged)
            .pick(point, 0, |id| world.find(id).is_some())
            .and_then(|id| world.find(id));
        let id_of = |at: usize| world.zoo.entities[at].common.as_ref().map(|c| c.id);
        let animal = hit
            .filter(|&at| world.zoo.entities[at].class == "animals")
            .and_then(id_of)
            .filter(|&id| {
                (world.units.get(id))
                    .is_some_and(|u| matches!(&u.part, Part::Animal(a) if !a.dying))
            });
        let gate = hit.filter(|&at| world.is_door(at)).and_then(|at| {
            let id = id_of(at)?;
            (world.exhibits().list().iter())
                .find(|e| e.gate == Some(id))
                .map(|e| e.id)
        });
        match (animal, gate) {
            (Some(id), _) => self.open_animal(id),
            (None, Some(gate)) => {
                self.close_animal();
                self.open_exhibit(gate);
            }
            (None, None) => self.close_animal(),
        }
    }

    /// Shows the exhibit panel, with the exhibit list button down, on exhibit `id`.
    fn open_exhibit(&mut self, id: u32) {
        let Some(ui) = &mut self.ui else { return };
        if !ui.shown(LAYOUT) {
            ui.press_button(BUTTON);
        }
        // Without the main panel (or a button that was down already) the layout is shown directly.
        if !ui.shown(LAYOUT) {
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

    /// A row of the exhibit list was selected (10.1): that exhibit is current, the view centres
    /// on its gate tile and the gate is selected, which deselects an animal and hides its panel;
    /// no row makes none current.
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
            self.close_animal();
        }
    }

    /// A row of the exhibit panel's animal list was selected (10.5): that animal is selected,
    /// its panel opens and the view centres on it.
    pub(super) fn panel_animal(&mut self, row: i32) {
        let Some(game) = &mut self.game else { return };
        let id = usize::try_from(row)
            .ok()
            .and_then(|r| game.panel.animal_of(r));
        let tile = id.and_then(|id| game.world.find(id)).and_then(|at| {
            let c = game.world.zoo.entities[at].common.as_ref()?;
            Some((c.x.div_euclid(64), c.y.div_euclid(64)))
        });
        if let Some((id, (x, y))) = id.zip(tile) {
            self.open_animal(id);
            let Some(game) = &mut self.game else { return };
            if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) {
                game.camera.centre_on(&game.world.zoo, x, y);
            }
        }
    }

    /// Selects the animal with entity id `id` and shows its panel, filled at once (4.1, 4.3). The
    /// exhibit panel, when shown, follows it (4.5); a recommendations plaque that is open follows
    /// it too.
    fn open_animal(&mut self, id: u32) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        game.animal.select(id);
        ui.show(animal::LAYOUT);
        if ui.shown(LAYOUT) {
            game.panel.current =
                (game.world.find(id)).and_then(|at| exhibit::animals_exhibit(&game.world, at));
            game.panel.clock.force = true;
        }
        if ui.is_down(animal::RECOMMENDATIONS) {
            game.panel.show_plaque(id);
        }
    }

    /// Hides the animal panel: the next look clears the selection.
    fn close_animal(&mut self) {
        if let Some(ui) = &mut self.ui {
            ui.set_hidden(animal::LAYOUT, true);
        }
    }

    /// Notices what the player did to the animal panel since the last look: a closed panel or an
    /// animal that left the world clears the selection (4.1, 4.4); the recommendations button
    /// shows or hides the plaque, and the plaque's own close releases the button; the unit
    /// panels sit above the exhibit panel while it is shown (4.2).
    fn animal_sync(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let gone = game.animal.selected.is_some_and(|id| {
            game.world.find(id).is_none()
                || !(game.world.units.get(id))
                    .is_some_and(|u| matches!(&u.part, Part::Animal(a) if !a.dying))
        });
        if gone || !ui.shown(animal::LAYOUT) {
            if game.animal.selected.is_some() {
                self.panel_lines.push("animal none".into());
            }
            game.animal.deselect();
            ui.set_hidden(animal::LAYOUT, true);
        }
        ui.set_alt(animal::LAYOUT, ui.shown(LAYOUT));
        let selected = game.animal.selected;
        let (down, shown) = (ui.is_down(animal::RECOMMENDATIONS), ui.shown(PLAQUE));
        match (selected, down, shown) {
            (Some(id), true, false) if !game.animal.plaque_shown => {
                game.panel.show_plaque(id);
                ui.show(PLAQUE);
            }
            (None, ..) | (_, true, false) => {
                ui.set_button(animal::RECOMMENDATIONS, false, true);
                ui.set_hidden(PLAQUE, true);
            }
            (_, false, true) => {
                ui.set_hidden(PLAQUE, true);
            }
            _ => {}
        }
        game.animal.plaque_shown = ui.shown(PLAQUE);
        game.panel.animal = game.animal.selected;
    }

    /// Whether the view follows the selected animal: the panel's follow button is down.
    pub(super) fn following(&self) -> bool {
        let selected = self
            .game
            .as_ref()
            .is_some_and(|g| g.animal.selected.is_some());
        selected
            && self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.is_down(animal::FOLLOW))
    }

    /// At the interface pass of a following view, the animal is at the centre (camera 10.3).
    pub(super) fn follow(&mut self) {
        let following = self.following();
        let Some(game) = self.game.as_mut().filter(|_| following) else {
            return;
        };
        let at = game.animal.selected.and_then(|id| game.world.find(id));
        if let Some(c) = at.and_then(|at| game.world.zoo.entities[at].common.as_ref()) {
            game.camera.centre_on_point((c.x, c.y, c.z));
        }
    }

    /// The exhibit button of the animal panel (4.5): the exhibit panel closes when shown, else
    /// opens on the animal's exhibit; an animal on the grounds does nothing.
    pub(super) fn animal_exhibit(&mut self) {
        let (Some(game), Some(ui)) = (&self.game, &mut self.ui) else {
            return;
        };
        if ui.shown(LAYOUT) {
            if !ui.press_button(BUTTON) {
                ui.set_hidden(LAYOUT, true);
            }
            return;
        }
        let exhibit = game.animal.selected.and_then(|id| game.world.find(id));
        if let Some(x) = exhibit.and_then(|at| exhibit::animals_exhibit(&game.world, at)) {
            self.open_exhibit(x);
        }
    }

    /// The player ended editing the animal's name field: a text with a visible character is its
    /// name (exhibits and money 3.6); the field shows the name again either way.
    pub(super) fn animal_rename(&mut self) {
        let (Some(game), Some(ui)) = (&mut self.game, &self.ui) else {
            return;
        };
        let at = game.animal.selected.and_then(|id| game.world.find(id));
        let bytes = ui.text(animal::NAME).unwrap_or_default();
        if let Some(c) = at.and_then(|at| game.world.zoo.entities[at].common.as_mut())
            && exhibits::visible(&bytes)
        {
            c.name = bytes.iter().map(|&b| from_cp1252(b)).collect();
        }
        game.animal.reselect();
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
        self.animal_sync();
        let ticking = self.fired.interface.is_some() && !self.paused() && !self.modal();
        let (Some(game), Some(ui)) = (&mut self.game, &mut self.ui) else {
            return;
        };
        let (vfs, cat) = (self.renderer.vfs(), self.renderer.catalogue());
        if ui.shown(LAYOUT) && game.panel.clock.due(now, ticking, exhibit::PERIOD) {
            let lines = game.panel.refresh(ui, &game.world, cat, vfs);
            self.panel_lines.extend(lines);
        }
        if ui.shown(animal::LAYOUT) && game.animal.clock.due(now, ticking, animal::PERIOD) {
            let when = self.sim.calendar().state().0;
            self.panel_lines
                .extend(game.animal.refresh(ui, &game.world, cat, vfs, when));
        }
        if game.panel.plaque.is_some() && game.panel.plaque_clock.due(now, ticking, exhibit::PERIOD)
        {
            let lines = game.panel.refresh_plaque(ui, &game.world, cat, vfs);
            self.panel_lines.extend(lines);
        }
    }
}
