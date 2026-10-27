//! The end of a scenario (scenarios 5.4, 9) and the objectives panel (4.5).

use super::*;
use crate::scenario;
use crate::ui::widgets::Row;

/// The Scenario button and the panel it opens: the name, the story and the list of goals.
pub(super) const BUTTON: i32 = 4107;
const NAME: i32 = 4142;
const STORY: i32 = 4143;
const LIST: i32 = 4145;
/// The end dialog (the large one), its picture element and the picture it shows.
const DIALOG: i32 = 45200;
const DIALOG_PICTURE: i32 = 45204;
const GENERIC: &str = "ui/event/generic/generic";

impl App<'_> {
    /// The scenario has ended (5.4): the result and the unlocks are stored, the game's layouts
    /// are hidden and the end dialog asks for an OK; without an interface, or a dialog to show,
    /// the game stops at once.
    pub(super) fn scenario_over(&mut self) {
        let Some(end) = self.sim.scenario_ended() else {
            return;
        };
        let result = if end.won { "won" } else { "lost" };
        self.menu_events.push(format!("scenario ended {result}"));
        let (Some(ui), Some(menu)) = (&mut self.ui, &mut self.menu) else {
            return;
        };
        self.menu_events.extend(menu.scenario_ended(&end));
        for id in GAME_LAYOUTS {
            if let Some(i) = ui.tree.find(id) {
                ui.tree.hide(i);
            }
        }
        let text = scenario::end_text(&end.file, end.won, menu.expansions());
        let picture = ui.tree.picture(self.renderer.vfs(), GENERIC);
        ui.set_picture(DIALOG_PICTURE, picture);
        let message = to_cp1252(&ui.string(text));
        self.ending = ui.ask(DIALOG, &message, Buttons::OK);
        self.menu_events.push(format!("scenario dialog {text}"));
        if !self.ending {
            self.end_answered();
        }
    }

    /// The end dialog was answered: the game stops and the scenario screen shows the story of the
    /// result (O1).
    pub(super) fn end_answered(&mut self) {
        self.ending = false;
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
        let words = Words::new(|_| String::new());
        let rows = scenario.lines().into_iter().map(|l| {
            let args = l.values.map(|v| Arg::Int(i64::from(v)));
            let text = format::expand(&words, &ui.template(l.text), &args);
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
