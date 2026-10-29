//! Saving and loading in play (save games 3 to 5, main menu 7.1 and 9): the Save Game button and
//! Ctrl+S, the Yes of the three questions, Load Game and Ctrl+L, and the load button of the main menu.
//!
//! The system's file dialogs are replaced by two screens of the game's own: the name dialog for
//! the file to save (its folder is that of `[user] lastfile`, else the saved games folder of the
//! user) and the list of the `.zoo` files of that folder, which is the freeform screen with the
//! saved games in its list. The game stands still while either is open.

use std::path::{Path, PathBuf};

use super::exhibits::{NAME_DIALOG, NAME_FIELD};
use super::{Action, App, Asked, Buttons, OPTIONS_PANEL, QUESTION, SAVE_BUTTON};
use crate::build::Build;
use crate::menu::{self, FREEFORM_LAYOUT, SAVED_LIST};
use crate::panel::exhibit;
use crate::text::{from_cp1252, to_cp1252};
use crate::ui::layout::DISABLED;
use crate::userdir;

/// The titles of the load and save dialogs, and the messages of a failed write and of a name that
/// is too long.
const LOAD_TITLE: i32 = 309;
const SAVE_TITLE: i32 = 310;
const SAVE_FAILED: i32 = 11645;
const NAME_TOO_LONG: i32 = 11646;
/// The name buffer of the dialog, and the longest name that gets `.zoo` appended.
const NAME_BUFFER: i32 = 260;
const LONGEST: usize = 256;
const EXTENSION: &str = ".zoo";
/// The question asked before a file is replaced (the original asks with the system's dialog).
const REPLACE_TEXT: &str = "The file already exists. Replace it?";

/// The save dialog being open: the folder of the file and what follows a successful save (the
/// question it answers; none for the button). `replace` is the name as typed, while the question
/// about replacing a file is open.
pub(super) struct Saving {
    then: Option<Asked>,
    dir: PathBuf,
    replace: Option<String>,
}

/// The file name a typed name is saved as: `.zoo` added unless it ends in it (case-sensitive).
fn zoo_name(typed: &str) -> String {
    if typed.ends_with(EXTENSION) {
        typed.to_string()
    } else {
        format!("{typed}{EXTENSION}")
    }
}

/// The list of saved games being shown: the folder, the file names and the row selected.
pub(super) struct Listing {
    dir: PathBuf,
    files: Vec<String>,
    selected: Option<usize>,
}

impl App<'_> {
    /// Whether the Save Game button lets the player save: it does unless it is there and disabled
    /// (the tutorials disable it).
    pub(super) fn can_save(&self) -> bool {
        let Some(ui) = &self.ui else { return true };
        let button = ui.tree.find(SAVE_BUTTON);
        button.is_none_or(|i| ui.tree.elements[i].state & DISABLED == 0)
    }

    /// `save FILE` of a script: the game as it is, with the camera of the moment. A failure is a
    /// warning and the run goes on.
    pub(super) fn save_game(&mut self, file: &str) {
        match self.write_zoo(Path::new(file)) {
            Ok(()) => self.menu_events.push(format!("game saved {file}")),
            Err(why) => eprintln!("rustyzoo: warning: save {file}: {why}"),
        }
    }

    /// Writes the game to `path` through a file beside it that is renamed when it is whole, so a
    /// failed save keeps the file that was there.
    fn write_zoo(&self, path: &Path) -> Result<(), String> {
        let game = self.game.as_ref().ok_or("no game to save")?;
        let cat = self.renderer.catalogue();
        let mut zoo = (game.world).saved(crate::zoo::knows(cat));
        (zoo.clocks, zoo.view) = (self.sim.clocks(), game.camera.view());
        game.world.finish_save(&mut zoo, cat);
        zoo.expansion = game.build.stored();
        if let Some(section) = self.sim.research_section() {
            zoo.set_research(section);
        }
        if let Some(scenario) = self.sim.scenario() {
            zoo.set_scenario(scenario.file(), &scenario.states());
        }
        let bytes = zoo.write(&game.money)?;
        let mut aside = path.as_os_str().to_owned();
        aside.push(".tmp");
        let aside = PathBuf::from(aside);
        let done = std::fs::write(&aside, bytes).and_then(|()| std::fs::rename(&aside, path));
        if done.is_err() {
            let _ = std::fs::remove_file(&aside);
        }
        done.map_err(|e| e.to_string())
    }

    /// Makes `path` the last file: `[user] lastfile` of the menu, or without the menu the file of
    /// this game, written to the user folder when there is one.
    fn set_last(&mut self, path: &str) {
        match &mut self.menu {
            Some(menu) => menu.set_last_file(path),
            None => {
                self.last_file = Some(PathBuf::from(path));
                if let Some(dir) = &self.user
                    && let Err(why) = userdir::save(dir, "user", "lastfile", path)
                {
                    eprintln!("rustyzoo: warning: {why}");
                }
            }
        }
    }

    /// The folder of `[user] lastfile` and its file name, or the saved games folder and no name.
    fn dialog_start(&self) -> (PathBuf, String) {
        let last = match &self.menu {
            Some(menu) => menu.last_path(),
            None => self.last_file.clone(),
        };
        let last = last.filter(|p| p.parent().is_some_and(Path::is_dir));
        match last {
            Some(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                let dir = p.parent().map(Path::to_path_buf);
                (dir.unwrap_or_default(), name.unwrap_or_default())
            }
            None => (
                userdir::saved_games().unwrap_or_else(|| PathBuf::from("Saved Games")),
                String::new(),
            ),
        }
    }

    /// The Save Game button: the save dialog, if saving is allowed. The button closes nothing.
    pub(super) fn save_pressed(&mut self) {
        if self.can_save() {
            self.begin_save(None);
        }
    }

    /// Ctrl+S: the open panels close first, then the save dialog as the button opens it.
    pub(super) fn save_key(&mut self) {
        if self.can_save() {
            self.close_panels();
        }
        self.save_pressed();
    }

    /// Closes the open panels and clears the selection, before a save dialog or a question
    /// (save games 3.1): the options panel, the tool panels, and the exhibit, animal, staff, guest
    /// and building panels. Their next look clears their selections.
    fn close_panels(&mut self) {
        if let Some(game) = &mut self.game {
            game.selected = None;
        }
        self.close_building();
        self.close_guest();
        self.close_staff();
        self.close_animal();
        if let Some(ui) = &mut self.ui {
            Build::hide_panels(ui);
            for id in [OPTIONS_PANEL, exhibit::LAYOUT] {
                ui.set_hidden(id, true);
            }
            ui.touch();
        }
    }

    /// Opens the save dialog, with the name of the last file in its field. `then` is the question
    /// the save is the Yes of: it goes on after a successful save.
    pub(super) fn begin_save(&mut self, then: Option<Asked>) {
        let (dir, name) = self.dialog_start();
        self.name_dialog(dir, &name, then);
    }

    /// Opens the save dialog with `name` in its field, for the save of `dir` that `then` follows.
    fn name_dialog(&mut self, dir: PathBuf, name: &str, then: Option<Asked>) {
        let Some(ui) = &mut self.ui else { return };
        let title = to_cp1252(&ui.string(SAVE_TITLE));
        if !ui.ask(NAME_DIALOG, &title, Buttons::OK_CANCEL) {
            return eprintln!("rustyzoo: warning: this interface has no save dialog");
        }
        ui.set_char_limit(NAME_FIELD, NAME_BUFFER);
        ui.set_wide_text(NAME_FIELD, true);
        ui.set_text(NAME_FIELD, &to_cp1252(name));
        ui.focus(NAME_FIELD);
        self.saving = Some(Saving {
            then,
            dir,
            replace: None,
        });
        self.menu_events.push("game save dialog".to_string());
    }

    /// The save dialog was answered: OK writes the file named in the field, anything else ends
    /// the command without saving (save games 4). A name whose file exists is first confirmed
    /// (save games 4 point 2): Yes writes it, No opens the dialog again with the name as typed.
    /// The name is the one typed; `.zoo` is added to a name with no extension for the question,
    /// and to any name not ending in it for the file.
    pub(super) fn save_answered(&mut self, code: i32, saving: Saving) {
        if let Some(typed) = saving.replace {
            if code == 6 {
                let path = saving.dir.join(zoo_name(&typed));
                return self.write_named(&saving.dir, &path, saving.then);
            }
            return self.name_dialog(saving.dir, &typed, saving.then);
        }
        let Some(ui) = &self.ui else { return };
        let name = ui.text(NAME_FIELD).unwrap_or_default();
        if code != 1 || name.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        let typed: String = name.iter().map(|&b| from_cp1252(b)).collect();
        if !typed.ends_with(EXTENSION) && name.len() > LONGEST {
            return self.say(&ui.string(NAME_TOO_LONG));
        }
        let asked = if Path::new(&typed).extension().is_some() {
            typed.clone()
        } else {
            zoo_name(&typed)
        };
        if saving.dir.join(asked).is_file() {
            let Some(ui) = &mut self.ui else { return };
            if !ui.ask(QUESTION, &to_cp1252(REPLACE_TEXT), Buttons::YES_NO) {
                return eprintln!("rustyzoo: warning: this interface has no replace question");
            }
            self.saving = Some(Saving {
                replace: Some(typed),
                ..saving
            });
            return;
        }
        let path = saving.dir.join(zoo_name(&typed));
        self.write_named(&saving.dir, &path, saving.then);
    }

    /// Writes the game to `path` in `dir` (made if missing), then makes it the last file and goes
    /// on with the question it answers. A failed write shows its message.
    fn write_named(&mut self, dir: &Path, path: &Path, then: Option<Asked>) {
        let written = std::fs::create_dir_all(dir)
            .map_err(|e| e.to_string())
            .and_then(|()| self.write_zoo(path));
        if let Err(why) = written {
            eprintln!("rustyzoo: warning: save {}: {why}", path.display());
            let text = self.ui.as_ref().map(|ui| ui.string(SAVE_FAILED));
            return self.say(&text.unwrap_or(why));
        }
        let path = std::path::absolute(path).unwrap_or(path.to_path_buf());
        let path = path.to_string_lossy().into_owned();
        self.menu_events.push(format!("game saved {path}"));
        self.set_last(&path);
        self.saved_then(then);
    }

    /// A save is over, or there was no dialog to save with: the question goes on, or the options
    /// panel closes.
    fn saved_then(&mut self, then: Option<Asked>) {
        match (then, &mut self.ui) {
            (Some(kind), _) => self.proceed(kind),
            (None, Some(ui)) => {
                if let Some(i) = ui.tree.find(OPTIONS_PANEL) {
                    ui.tree.hide(i);
                    ui.touch();
                }
            }
            (None, None) => {}
        }
    }

    /// Ctrl+L: in a game the question of Load Game, in the main menu the list of saved games.
    pub(super) fn load_pressed(&mut self) {
        match (&self.game, &self.ui) {
            (Some(_), _) => {
                if self.can_save() {
                    self.close_panels();
                }
                self.ask(Asked::Load);
            }
            (None, Some(ui)) if ui.shown(menu::LAYOUT) => self.open_list(),
            _ => {}
        }
    }

    // ---- The list of saved games

    /// Whether the list of saved games is on the screen.
    pub(super) fn listing_open(&self) -> bool {
        let shown = self.ui.as_ref().is_some_and(|ui| ui.shown(FREEFORM_LAYOUT));
        shown && self.listing.is_some()
    }

    /// Shows the `.zoo` files of the folder of the last file, or of the saved games folder, by
    /// name.
    pub(super) fn open_list(&mut self) {
        let (dir, _) = self.dialog_start();
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.to_ascii_lowercase().ends_with(EXTENSION))
            .collect();
        files.sort_by_key(|n| n.to_lowercase());
        let Some(ui) = &mut self.ui else { return };
        let title = to_cp1252(&ui.string(LOAD_TITLE));
        let names = files.iter().map(|n| to_cp1252(n)).collect();
        if !menu::show_saved(ui, names, &title) {
            return eprintln!("rustyzoo: warning: this interface has no list of saved games");
        }
        self.menu_events
            .push(format!("game load list {}", files.len()));
        self.listing = Some(Listing {
            dir,
            files,
            selected: None,
        });
    }

    /// The list is closed without loading.
    pub(super) fn close_list(&mut self) {
        self.listing = None;
        if let Some(ui) = &mut self.ui
            && let Some(i) = ui.tree.find(FREEFORM_LAYOUT)
        {
            ui.tree.hide(i);
            ui.touch();
        }
    }

    /// A row was selected in the list, or double clicked.
    pub(super) fn listed(&mut self, action: Action) {
        let (Some(list), Some(ui)) = (&mut self.listing, &mut self.ui) else {
            return;
        };
        let (Action::Row(SAVED_LIST, row) | Action::Double(SAVED_LIST, row)) = action else {
            return;
        };
        list.selected = usize::try_from(row).ok().filter(|&r| r < list.files.len());
        menu::enable_load(ui, list.selected.is_some());
        if matches!(action, Action::Double(..)) {
            self.load_listed();
        }
    }

    /// Loads the file selected in the list. When it opens, it is the last file and the list and
    /// the options panel close; when it does not, the loader's message is shown.
    pub(super) fn load_listed(&mut self) {
        let Some(list) = &self.listing else { return };
        let Some(name) = list.selected.map(|i| &list.files[i]) else {
            return;
        };
        let path = list.dir.join(name);
        let path = std::path::absolute(&path).unwrap_or(path);
        let path = path.to_string_lossy().into_owned();
        if !self.load_saved(&path) {
            return;
        }
        self.set_last(&path);
        self.close_list();
        if let Some(ui) = &mut self.ui
            && let Some(i) = ui.tree.find(OPTIONS_PANEL)
        {
            ui.tree.hide(i);
        }
    }
}
