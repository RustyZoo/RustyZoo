//! Saving and loading in play (save games 3 to 5, main menu 7.1 and 9): the Save Game button and
//! Ctrl+S, the Yes of the three questions, Load Game and Ctrl+L, and Load Saved Game of the menu.
//!
//! The system's file dialogs are replaced by two screens of the game's own: the name dialog for
//! the file to save (its folder is that of `[user] lastfile`, else the saved games folder of the
//! user) and the list of the `.zoo` files of that folder, which is the freeform screen with the
//! saved games in its list. The game stands still while either is open.

use std::path::{Path, PathBuf};

use super::exhibits::{NAME_DIALOG, NAME_FIELD};
use super::{Action, App, Asked, Buttons, OPTIONS_PANEL, SAVE_BUTTON};
use crate::menu::{self, FREEFORM_LAYOUT, SAVED_LIST};
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

/// The save dialog being open: the folder of the file and what follows a successful save (the
/// question it answers; none for the button).
pub(super) struct Saving {
    then: Option<Asked>,
    dir: PathBuf,
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

    /// The folder of `[user] lastfile` and its file name, or the saved games folder and no name.
    fn dialog_start(&self) -> (PathBuf, String) {
        let last = self.menu.as_ref().and_then(menu::Menu::last_path);
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

    /// The Save Game button or Ctrl+S: the save dialog, if saving is allowed.
    pub(super) fn save_pressed(&mut self) {
        if self.can_save() {
            self.begin_save(None);
        }
    }

    /// Opens the save dialog, with the name of the last file in its field. `then` is the question
    /// the save is the Yes of: it goes on after a successful save.
    pub(super) fn begin_save(&mut self, then: Option<Asked>) {
        let (dir, name) = self.dialog_start();
        let Some(ui) = &mut self.ui else { return };
        let title = to_cp1252(&ui.string(SAVE_TITLE));
        if !ui.ask(NAME_DIALOG, &title, Buttons::OK_CANCEL) {
            return eprintln!("rustyzoo: warning: this interface has no save dialog");
        }
        ui.set_char_limit(NAME_FIELD, NAME_BUFFER);
        ui.set_text(NAME_FIELD, &to_cp1252(&name));
        ui.focus(NAME_FIELD);
        self.saving = Some(Saving { then, dir });
        self.menu_events.push("game save dialog".to_string());
    }

    /// The save dialog was answered: OK writes the file named in the field, anything else ends
    /// the command without saving (save games 4).
    pub(super) fn save_answered(&mut self, code: i32, saving: Saving) {
        let Some(ui) = &self.ui else { return };
        let mut name = ui.text(NAME_FIELD).unwrap_or_default();
        if code != 1 || name.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        if !name.ends_with(EXTENSION.as_bytes()) {
            if name.len() > LONGEST {
                return self.say(&ui.string(NAME_TOO_LONG));
            }
            name.extend_from_slice(EXTENSION.as_bytes());
        }
        let name: String = name.iter().map(|&b| from_cp1252(b)).collect();
        let path = saving.dir.join(name);
        let written = std::fs::create_dir_all(&saving.dir)
            .map_err(|e| e.to_string())
            .and_then(|()| self.write_zoo(&path));
        if let Err(why) = written {
            eprintln!("rustyzoo: warning: save {}: {why}", path.display());
            let text = self.ui.as_ref().map(|ui| ui.string(SAVE_FAILED));
            return self.say(&text.unwrap_or(why));
        }
        let path = std::path::absolute(&path).unwrap_or(path);
        let path = path.to_string_lossy().into_owned();
        self.menu_events.push(format!("game saved {path}"));
        if let Some(menu) = &mut self.menu {
            menu.set_last_file(&path);
        }
        self.saved_then(saving.then);
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
            (Some(_), _) => self.ask(Asked::Load),
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
        if let Some(menu) = &mut self.menu {
            menu.set_last_file(&path);
        }
        self.close_list();
        if let Some(ui) = &mut self.ui
            && let Some(i) = ui.tree.find(OPTIONS_PANEL)
        {
            ui.tree.hide(i);
        }
    }
}
