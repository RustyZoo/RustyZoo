//! The show panel in the game (information panels 19.3; shows 33): what opens it (a show tank
//! set on the exhibit panel, a grandstand that faces one), what hides it, the drop-down lists
//! that close when they lose the keyboard focus, and the buttons, rows and field of the window.
//! The window itself is [`crate::panel::show`].

use super::*;
use crate::entity::Catalogue;
use crate::panel::show::{self, FINANCES_TAB, LAYOUT, SCRIPT_TAB};
use crate::show as shows;

/// The panels that showing the window hides, and that hide it when they are shown (19.3 points 3
/// and 7): the building, buying, hiring, research, options and developer menus.
const PANELS: [i32; 7] = [8, 3, 4, 10, 15, 5, 34];
/// The zoo status panels, which showing the window hides.
const STATUS: [i32; 2] = [14, 803];
/// The panels that hide the window when they are shown: the exhibit, animal, guest and staff
/// panels, and [`PANELS`].
const UNITS: [i32; 4] = [exhibit::LAYOUT, animal::LAYOUT, guest::LAYOUT, staff::LAYOUT];
/// The two drop-downs: the layout and the list that holds the keyboard focus while it is open.
const DROP_DOWNS: [(i32, i32); 2] = [
    (show::SPECIES_LAYOUT, show::SPECIES_LIST),
    (show::FREQUENCY_LAYOUT, show::FREQUENCY_LIST),
];

/// Opens the window on show tank `x` and tab `tab` (19.3): the exhibit is set, the tab selected,
/// and the layout shown when it is hidden, which hides the other panels and the drop-downs.
fn open(game: &mut Game, ui: &mut Interface, cat: &Catalogue, x: u32, tab: i32) {
    game.show.stand = None;
    game.show.set_exhibit(ui, &mut game.world, cat, Some(x));
    if !ui.is_down(tab) {
        let other = SCRIPT_TAB + FINANCES_TAB - tab;
        if ui.shown(LAYOUT) {
            ui.press_button(tab);
        } else {
            ui.set_button(other, false, true);
            ui.set_button(tab, true, true);
        }
    }
    if !ui.shown(LAYOUT) {
        ui.show(LAYOUT);
        for id in PANELS.into_iter().chain(STATUS) {
            ui.set_hidden(id, true);
        }
    }
    for (layout, _) in DROP_DOWNS {
        ui.set_hidden(layout, true);
    }
    game.show.seen = PANELS
        .into_iter()
        .chain(UNITS)
        .map(|id| ui.shown(id))
        .collect();
    game.building.clock.force = true;
}

impl App<'_> {
    /// The exhibit panel's exhibit was set (19.3 point 1): a show tank opens the window on Show
    /// Script; another exhibit, if it is a change, hides it.
    pub(super) fn show_exhibit_set(&mut self) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        let x = game.panel.current;
        if let Some(x) = x.filter(|&x| shows::info_of(&game.world, x).is_some()) {
            open(game, ui, cat, x, SCRIPT_TAB);
        } else if x != game.show.exhibit {
            ui.set_hidden(LAYOUT, true);
            game.show.set_exhibit(ui, &mut game.world, cat, None);
        }
    }

    /// Grandstand `stand` was selected (19.3 point 2): when it faces a show tank, the window
    /// opens on finances, until the grandstand is deselected.
    pub(super) fn show_stand(&mut self, stand: u32) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        let x = shows::faced_exhibit(&game.world, cat, stand);
        if let Some(x) = x.filter(|_| shows::is_grandstand(&game.world, cat, stand)) {
            open(game, ui, cat, x, FINANCES_TAB);
            game.show.stand = Some(stand);
        }
    }

    /// The window as the player left it since the last look, and its refresh (19.3, 19.4): a
    /// panel shown after it, a deselected grandstand, a show info that is gone or a drop-down
    /// that lost the focus hides what they hide; a clock tick or a change of the species list
    /// refreshes it.
    pub(super) fn show_pass(&mut self, now: u64, ticking: bool) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        let shown: Vec<bool> = PANELS
            .into_iter()
            .chain(UNITS)
            .map(|id| ui.shown(id))
            .collect();
        let opened = (shown.iter().zip(&game.show.seen)).any(|(now, was)| *now && !*was);
        if opened {
            ui.set_hidden(LAYOUT, true);
        }
        game.show.seen = shown;
        if game
            .show
            .stand
            .is_some_and(|s| game.building.selected != Some(s))
        {
            game.show.stand = None;
            ui.set_hidden(LAYOUT, true);
        }
        let gone = game
            .show
            .exhibit
            .is_some_and(|x| shows::info_of(&game.world, x).is_none());
        if gone {
            game.show.set_exhibit(ui, &mut game.world, cat, None);
        }
        for (layout, list) in DROP_DOWNS {
            if ui.shown(layout) && !ui.has_focus(list) {
                ui.set_hidden(layout, true);
            }
        }
        if ui.shown(LAYOUT) != game.show.shown {
            game.show.shown = !game.show.shown;
            let word = if game.show.shown { "open" } else { "closed" };
            self.lines.panel.push(format!("show window {word}"));
        }
        if !ui.shown(LAYOUT) {
            return;
        }
        let rows = game
            .show
            .exhibit
            .map(|x| show::species_of(ui, &game.world, x));
        game.show.clock.force |= rows.is_some_and(|r| r != game.show.rows);
        if game.show.clock.due(now, ticking, show::PERIOD) {
            let lines = game.show.refresh(ui, &mut game.world, cat);
            self.lines.panel.extend(lines);
            game.building.clock.force = true;
        }
    }

    /// A button of the window ran its action (19.6, 19.5, 19.7): the admission buttons, and the two
    /// drop-down buttons, which give their list the keyboard focus.
    pub(super) fn show_pressed(&mut self, id: i32) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        match id {
            show::UP | show::DOWN => {
                game.show.press(ui, &mut game.world, cat, id == show::UP);
                game.building.clock.force = true;
            }
            show::SPECIES_BUTTON => _ = ui.focus(show::SPECIES_LIST),
            show::FREQUENCY_BUTTON => _ = ui.focus(show::FREQUENCY_LIST),
            _ => {}
        }
    }

    /// A row of a drop-down was chosen.
    pub(super) fn show_row(&mut self, list: i32, row: i32) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        match list {
            show::SPECIES_LIST => game.show.choose_species(ui, &mut game.world, cat, row),
            _ => game.show.choose_frequency(ui, &mut game.world, cat, row),
        }
        game.building.clock.force = true;
        game.show.clock.force = true;
    }

    /// The player ended editing the admission field (19.6).
    pub(super) fn show_typed(&mut self) {
        playing!(self, game, ui);
        let cat = self.renderer.catalogue();
        game.show.type_admission(ui, &mut game.world, cat);
        game.building.clock.force = true;
    }
}
