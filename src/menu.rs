//! The main menu (layout 26): what is set when it appears and what its buttons do that the
//! layout does not already say (main menu 4). The screens it opens (scenarios, freeform maps,
//! credits) are layouts shown by the buttons' own actions.
//!
//! A game started from the menu comes with a later issue; here Exit ends the program.

use std::path::{Component, Path, PathBuf};

use crate::config::{Config, to_int};
use crate::text::format::{Arg, Strings, expand};
use crate::ui::interface::Interface;
use crate::ui::layout::DISABLED;
use crate::userdir;

/// The layout of the main menu and the elements the code touches.
const LAYOUT: i32 = 26;
const BACKGROUND: i32 = 7100;
const CONTINUE: i32 = 7113;
const ZOO_ITEMS: i32 = 7114;
const EXIT: i32 = 7116;
const VERSION: i32 = 7119;
/// The string of the version label and the tooltips of Continue.
const VERSION_TEXT: i32 = 1526;
const CONTINUE_TIP: i32 = 7123;
const NO_CONTINUE_TIP: i32 = 7127;
/// What the tooltip adds to a string number when `[UI] helpType` is not 0.
const LONG_HELP: i32 = 30000;

/// Templates of the game's strings take their values from the call, never from a string number.
struct NoStrings;

impl Strings for NoStrings {
    fn string(&self, _: u32) -> &str {
        ""
    }
}

pub struct Menu {
    ini: Config,
    /// The install folder, which a relative `lastfile` is relative to.
    root: PathBuf,
    /// The folder of the settings that are written back; none: they are not.
    user: Option<PathBuf>,
    /// The product version of the executable, for the label at the bottom of the picture.
    version: Option<String>,
    /// The expansion flags (bit 1 and bit 2, the `id` of an `xpac*.cfg`).
    expansions: i32,
}

impl Menu {
    pub fn new(
        ini: Config,
        root: &Path,
        user: Option<PathBuf>,
        version: Option<String>,
        expansions: i32,
    ) -> Menu {
        Menu {
            ini,
            root: root.to_path_buf(),
            user,
            version,
            expansions,
        }
    }

    fn setting(&self, section: &str, key: &str) -> Option<&[u8]> {
        self.ini.section(section)?.get(key)
    }

    /// Writes `key` of `[user]` back, to the memory of the menu and to the settings of the user.
    fn write_back(&mut self, key: &str, value: &str) {
        self.ini.set("user", key, value);
        if let Some(dir) = &self.user
            && let Err(e) = userdir::save(dir, "user", key, value)
        {
            eprintln!("rustyzoo: warning: {e}");
        }
    }

    /// Shows the main menu: the background of the expansion flags, Continue enabled or not with
    /// its tooltip, Zoo Items disabled, the version label. Returns the lines for the log.
    pub fn show(&mut self, ui: &mut Interface) -> Vec<String> {
        let mut log = Vec::new();
        ui.show(LAYOUT);
        // The layout lists one picture per combination of the flags; the first is drawn.
        if let Some(i) = ui.tree.find(BACKGROUND) {
            let look = &mut ui.tree.elements[i].look;
            if let Some(&picture) = look.animation.get(self.expansions as usize & 3) {
                look.animation = vec![picture];
            }
        }
        self.check_continue(ui, &mut log);
        // Zoo Items needs the original's download service.
        enable(ui, ZOO_ITEMS, false);
        if let Some(version) = &self.version
            && let Some(i) = ui.tree.find(VERSION)
        {
            ui.tree.elements[i].look.text_id = vec![VERSION_TEXT];
            let text = expand(
                &NoStrings,
                &ui.template(VERSION_TEXT),
                &[Arg::Text(version)],
            );
            ui.set_string(VERSION_TEXT, text);
            log.push(format!("version {version}"));
        }
        ui.touch();
        log
    }

    /// Continue is enabled when `[user] lastfile` names a file: the tooltip names it. Otherwise
    /// the button is grey, `lastfile` is emptied and the tooltip says the game was not saved.
    fn check_continue(&mut self, ui: &mut Interface, log: &mut Vec<String>) {
        let last = self
            .setting("user", "lastfile")
            .map(|v| String::from_utf8_lossy(v).trim().to_string())
            .unwrap_or_default();
        let exists = !last.is_empty() && is_file(&self.root, &last.replace('\\', "/"));
        let long = self
            .setting("UI", "helpType")
            .and_then(to_int)
            .is_some_and(|n| n != 0);
        let offset = if long { LONG_HELP } else { 0 };
        let tip = if exists {
            CONTINUE_TIP
        } else {
            NO_CONTINUE_TIP
        };
        if let Some(i) = ui.tree.find(CONTINUE) {
            ui.tree.elements[i].help_id = vec![tip];
        }
        enable(ui, CONTINUE, exists);
        if exists {
            let name = last.rsplit('\\').next().unwrap_or_default();
            let text = expand(
                &NoStrings,
                &ui.template(CONTINUE_TIP + offset),
                &[Arg::Text(name)],
            );
            ui.set_string(CONTINUE_TIP + offset, text);
            log.push(format!("continue enabled {name}"));
        } else {
            if !last.is_empty() {
                self.write_back("lastfile", "");
            }
            log.push("continue disabled".to_string());
        }
    }

    /// The button `id` was selected: whether the program ends.
    pub fn selected(&mut self, _ui: &mut Interface, id: i32) -> (Vec<String>, bool) {
        match id {
            EXIT => (vec!["exit".to_string()], true),
            _ => (Vec::new(), false),
        }
    }
}

/// Whether `path` (relative to `root`, or absolute) names a file, each part matched ignoring
/// letter case as every file of the game is.
fn is_file(root: &Path, path: &str) -> bool {
    let mut found = PathBuf::new();
    for part in Path::new(path).components() {
        if found.as_os_str().is_empty()
            && !matches!(part, Component::RootDir | Component::Prefix(_))
        {
            found.push(root);
        }
        let Component::Normal(name) = part else {
            found.push(part);
            continue;
        };
        let exact = found.join(name);
        found = if exact.exists() {
            exact
        } else {
            let Ok(dir) = std::fs::read_dir(&found) else {
                return false;
            };
            match dir
                .flatten()
                .find(|e| e.file_name().eq_ignore_ascii_case(name))
            {
                Some(entry) => entry.path(),
                None => return false,
            }
        };
    }
    found.is_file()
}

/// Enables or disables the button `id`, which draws grey and takes no click while disabled.
fn enable(ui: &mut Interface, id: i32, on: bool) {
    if let Some(i) = ui.tree.find(id) {
        match on {
            true => ui.tree.elements[i].state &= !DISABLED,
            false => ui.tree.elements[i].state |= DISABLED,
        }
    }
}

#[cfg(test)]
mod tests;
