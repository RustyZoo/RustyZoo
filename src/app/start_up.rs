//! The end of the start-up, once the main menu is shown (main menu 3.5): the first-run tutorial,
//! the saved game of the command line, and the warning about user-made types.

use super::*;

/// The question box of the warning (`ui/confirm2.lyt`), its message string and its icon.
const WARNING: i32 = 45500;
const WARNING_TEXT: i32 = 162;
const INFO_PICTURE: &str = "ui/sharedui/info/info";
/// The check box that turns the warning off and its label, hidden in the layout.
const WARNING_BOX: i32 = 45514;
const WARNING_LABEL: i32 = 45515;

/// What came of the command line (main menu 3.5 point 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line {
    /// No path, or an empty one: nothing was tried.
    Idle,
    /// The game opened and the menu is gone.
    Started,
    /// The file did not open or did not load.
    Failed,
}

impl App<'_> {
    /// The end of the start-up: in order, the first that applies of the tutorial that is due, the
    /// saved game `path` of the command line (a failed tutorial falls through to it), and the main
    /// menu, over which a command line that did not fail may show the warning about user-made
    /// types.
    pub fn start_up(&mut self, path: Option<&str>) {
        if self.first_run() {
            return;
        }
        if self.command_line(path) == Line::Idle {
            self.warn_user_types();
        }
    }

    /// Opens the saved game `path` as Continue does, but `lastfile` stays: a path that is empty
    /// tries nothing, one that does not open fails silently, one that does not load shows the
    /// loader's message over the menu (main menu 3.5 points 4.3 to 4.7, 5.2).
    fn command_line(&mut self, path: Option<&str>) -> Line {
        let Some(path) = path.filter(|p| !p.is_empty()) else {
            return Line::Idle;
        };
        let start = Start {
            path: path.to_string(),
            label: "load".to_string(),
            forget_last: false,
            freeform: false,
            cash: Default::default(),
            reset: false,
            reset_fences: false,
            scenario: None,
        };
        match self.open_file(&start) {
            true => Line::Started,
            false => Line::Failed,
        }
    }

    /// Shows the warning about user-made types over the main menu, once, unless the player turned
    /// it off: OK only, the check box under the message is unticked (main menu 3.5 point 6).
    fn warn_user_types(&mut self) {
        if !self.menu.as_mut().is_some_and(Menu::warning_due) {
            return;
        }
        let Some(ui) = &mut self.ui else { return };
        let text = to_cp1252(&ui.string(WARNING_TEXT));
        if ui.ask(WARNING, &text, Buttons::OK) {
            let picture = ui.tree.picture(self.renderer.vfs(), INFO_PICTURE);
            ui.set_question_icon(WARNING, picture);
            ui.set_down(WARNING_BOX, false);
            for id in [WARNING_BOX, WARNING_LABEL] {
                ui.set_hidden(id, false);
            }
            self.warned = true;
        }
    }

    /// The check box of the warning was ticked or unticked since the last look: the setting is
    /// written at once, 0 when ticked and 1 when not (point 6.5).
    pub(super) fn warning_ticked(&mut self) {
        let Some(ui) = self.ui.as_mut().filter(|_| self.warned) else {
            return;
        };
        let moved =
            ui.take_selected_of(&[WARNING_BOX]).len() + ui.take_deselected_of(&[WARNING_BOX]).len();
        let ticked = ui.is_down(WARNING_BOX);
        if let (true, Some(menu)) = (moved > 0, &mut self.menu) {
            menu.write_back(
                "user",
                "showUserEntityWarning",
                if ticked { "0" } else { "1" },
            );
        }
    }

    /// The warning was answered: the check box and its label hide again (point 6.6).
    pub(super) fn warning_closed(&mut self) {
        self.warned = false;
        if let Some(ui) = &mut self.ui {
            ui.set_down(WARNING_BOX, false);
            for id in [WARNING_BOX, WARNING_LABEL] {
                ui.set_hidden(id, true);
            }
        }
    }
}
