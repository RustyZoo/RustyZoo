//! The end of a scenario (scenarios 5.4, 9) and the objectives panel (4.5).

use super::*;
use crate::scenario::{self, built_line};
use crate::ui::widgets::Row;

/// The Scenario button and the panel it opens: the name, the story and the list of goals.
pub(super) const BUTTON: i32 = 4107;
const NAME: i32 = 4142;
const STORY: i32 = 4143;
const LIST: i32 = 4145;
/// The end dialog (the large one), its picture element and the picture it shows.
const DIALOG: i32 = 45200;
const DIALOG_PICTURE: i32 = 45204;
/// The answers of the end dialog.
const YES: i32 = 6;
const NO: i32 = 7;
const GENERIC: &str = "ui/event/generic/generic";
/// The layouts the end hides (5.4.1), in the order of the general hide-all routine: the buy and hire
/// panels, the game options, the research and developer panels, the unit lists and the exhibit list.
/// Then the selection is cleared, and the zoo status, the scenario panel and the show panel are hidden.
const HIDDEN: [i32; 11] = [
    crate::build::CONSTRUCTION,
    crate::build::ADOPT,
    crate::build::OBJECTS,
    crate::build::HIRE,
    crate::panel::research::LAYOUT,
    OPTIONS_PANEL,
    34,
    16,
    12,
    31,
    crate::panel::exhibit::LAYOUT,
];
const HIDDEN_LAST: [i32; 3] = [14, 803, 41];
/// The toolbar buttons that open the hidden layouts: each is released with its layout.
const BUTTONS: [i32; 3] = [
    crate::status::STATUS_BUTTON,
    crate::panel::research::BUTTON,
    crate::panel::exhibit::BUTTON,
];

impl App<'_> {
    /// The scenario has ended (5.4): the result and the unlocks are stored, the panels of the
    /// hide-all routine are hidden and the selection cleared (5.4.1), and the end dialog asks Yes or No; without an interface, or a dialog to show,
    /// the game stops at once.
    pub(super) fn scenario_over(&mut self) {
        let Some(end) = self.sim.scenario_ended() else {
            return;
        };
        // The clean-up before the dialog gives the elements back and allows selling again (8.4).
        self.scenario_clean();
        if let Some(game) = &mut self.game {
            game.animal.no_sale = false;
            game.animal.clock.force = true;
        }
        let result = if end.won { "won" } else { "lost" };
        self.menu_events.push(format!("scenario ended {result}"));
        let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) else {
            return;
        };
        let (lines, newly) = menu.scenario_ended(&end);
        self.menu_events.extend(lines);
        hide_layouts(ui, &HIDDEN);
        if let Some(game) = &mut self.game {
            game.selected = None;
        }
        hide_layouts(ui, &HIDDEN_LAST);
        for id in BUTTONS {
            ui.set_button(id, false, true);
        }
        let text = scenario::end_text(&end.file, end.won, menu.expansions());
        let picture = ui.tree.picture(self.renderer.vfs(), GENERIC);
        ui.set_picture(DIALOG_PICTURE, picture);
        let message = to_cp1252(&ui.string(text));
        self.ending = ui.ask(DIALOG, &message, Buttons::YES_NO);
        self.menu_events.push(format!("scenario dialog {text}"));
        self.end_rewards(&newly);
        if !self.ending {
            self.end_answered(NO);
        }
    }

    /// The end dialog was answered (9.1). No stops the game: the main menu shows after a tutorial,
    /// the scenario screen with the story of the result after any other scenario. Yes after a
    /// tutorial that has a next one starts it; otherwise (and for any other answer) the dialog just
    /// closes. Without a
    /// dialog to answer (no interface) the answer is No.
    pub(super) fn end_answered(&mut self, code: i32) {
        self.ending = false;
        if code == YES {
            let (Some(menu), Some(file)) = (&mut self.menu, self.sim.scenario().map(|s| s.file()))
            else {
                return;
            };
            if let Some(next) = scenario::next_tutorial(file, menu.expansions()) {
                menu.start_file(self.renderer.vfs(), &next);
                self.requested();
            }
            return;
        }
        if code != NO {
            return;
        }
        self.stop_game();
        if let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) {
            let lines = menu.show_result(self.renderer.vfs(), ui);
            self.menu_events.extend(lines);
        }
    }

    /// The Scenario button was pressed: the panel gets the scenario's name and story and one line
    /// per goal that shows one, with the icon of its state.
    pub(super) fn objectives(&mut self) {
        let (Some(ui), Some(scenario)) = (&mut self.ui, self.sim.scenario()) else {
            return;
        };
        let vfs = self.renderer.vfs();
        let (name, story) = scenario.title();
        ui.set_text(NAME, &to_cp1252(&ui.string(name)));
        let story = ui.text_entry(story);
        ui.set_text(STORY, &story);
        let icons = scenario.icons().clone().map(|n| ui.tree.picture(vfs, &n));
        let rows = scenario.lines().into_iter().map(|l| {
            let text = built_line(ui, l);
            let icon = icons[match l.state {
                0 => 0,
                2 => 2,
                _ => 1,
            }];
            Row {
                text: to_cp1252(&text),
                icon,
                ..Row::default()
            }
        });
        let rows: Vec<Row> = rows.collect();
        self.menu_events
            .push(format!("scenario list {}", rows.len()));
        ui.set_rows(LIST, rows);
    }
}

/// Hides the layouts that the interface has (a layout the scenario's interface lacks is skipped).
fn hide_layouts(ui: &mut Interface, layouts: &[i32]) {
    for &id in layouts {
        if let Some(i) = ui.tree.find(id) {
            ui.tree.hide(i);
        }
    }
}
