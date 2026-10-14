//! The main menu (layout 26): what is set when it appears and what its buttons do that the
//! layout does not already say (main menu 4). The screens it opens (scenarios, freeform maps,
//! credits) are layouts shown by the buttons' own actions.
//!
//! The credits are a layout of pages that replace one another on the interface clock. A game
//! started from the menu comes with a later issue; here Exit ends the program.

use std::path::{Component, Path, PathBuf};

use crate::config::{Config, to_int};
use crate::text::format::{Arg, Strings, expand};
use crate::ui::interface::Interface;
use crate::ui::layout::{DISABLED, HIDDEN};
use crate::userdir;
use crate::vfs::Vfs;

/// The layout of the main menu and the elements the code touches.
const LAYOUT: i32 = 26;
const BACKGROUND: i32 = 7100;
const CONTINUE: i32 = 7113;
const ZOO_ITEMS: i32 = 7114;
const EXIT: i32 = 7116;
const VERSION: i32 = 7119;
const CREDITS: i32 = 7115;
/// The layout of the credits.
const CREDITS_LAYOUT: i32 = 40;
/// Seconds a credits page stays when no registry file says.
const CREDITS_DELAY: i32 = 8;
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

/// The layouts of the credits pages in order, and the seconds each stays: from the registry files
/// `ui/credits/credit*.cfg`, taken in descending order of their names, those whose
/// `[credits] version` is above 0; every section but `[credits]` is a page whose `layout` is the
/// id of its layout (in the order the configuration reader lists sections), and the last file
/// that has a `delay` sets it.
pub fn credit_pages(vfs: &Vfs) -> (Vec<i32>, u32) {
    let mut files = vfs.list(Some("ui/credits/credit*.cfg"));
    files.sort_by(|a, b| b.cmp(a));
    let (mut pages, mut delay) = (Vec::new(), CREDITS_DELAY);
    for file in files {
        let Some(config) = vfs.read(&file).ok().and_then(|b| Config::parse(&b).ok()) else {
            continue;
        };
        let head = config.section("credits");
        let number = |key: &str| head.and_then(|h| h.get(key)).and_then(to_int);
        if number("version").is_none_or(|v| v <= 0) {
            continue;
        }
        delay = number("delay").unwrap_or(delay);
        for (name, section) in config.sections() {
            if !name.eq_ignore_ascii_case(b"credits")
                && let Some(layout) = section.get("layout").and_then(to_int)
            {
                pages.push(layout);
            }
        }
    }
    (pages, u32::try_from(delay).unwrap_or(0))
}

/// The credits being shown: the page and the interface time it has been shown.
struct Showing {
    page: usize,
    elapsed: u64,
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
    /// The credits pages (layout ids) and the seconds each stays.
    pages: Vec<i32>,
    delay: u32,
    credits: Option<Showing>,
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
            pages: Vec::new(),
            delay: CREDITS_DELAY as u32,
            credits: None,
        }
    }

    /// The credits pages, see [`credit_pages`].
    pub fn set_credits(&mut self, pages: Vec<i32>, delay: u32) {
        (self.pages, self.delay) = (pages, delay);
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
        self.credits = None;
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

    /// The button `id` was selected: the lines for the log and whether the program ends.
    pub fn selected(&mut self, ui: &mut Interface, id: i32) -> (Vec<String>, bool) {
        match id {
            EXIT => (vec!["exit".to_string()], true),
            CREDITS => (self.show_credits(ui), false),
            _ => (Vec::new(), false),
        }
    }

    /// Shows page `page` of the credits and none of the others: the credits layout shows all its
    /// pages, one over the other. A page with no layout (an expansion that is not there) shows
    /// nothing and keeps its time.
    fn show_page(&mut self, ui: &mut Interface, page: usize) -> String {
        for &id in &self.pages {
            if let Some(i) = ui.tree.find(id) {
                ui.tree.elements[i].state |= HIDDEN;
            }
        }
        let id = self.pages[page];
        let shown = ui.show(id);
        self.credits = Some(Showing { page, elapsed: 0 });
        let empty = if shown { "" } else { " (no layout)" };
        format!("credits page {} layout {id}{empty}", page + 1)
    }

    fn show_credits(&mut self, ui: &mut Interface) -> Vec<String> {
        match self.pages.is_empty() {
            true => Vec::new(),
            false => vec![self.show_page(ui, 0)],
        }
    }

    /// An interface update `period` milliseconds after the last one: after each `delay` seconds
    /// the next credits page replaces the one shown, and after the last one the credits hide
    /// themselves. Credits that were closed (Exit hides the layout) are over.
    pub fn update(&mut self, ui: &mut Interface, period: u32) -> Vec<String> {
        let Some(shown) = &mut self.credits else {
            return Vec::new();
        };
        let open = ui
            .tree
            .find(CREDITS_LAYOUT)
            .is_some_and(|i| ui.tree.elements[i].state & HIDDEN == 0);
        if !open {
            self.credits = None;
            return vec!["credits closed".to_string()];
        }
        shown.elapsed += u64::from(period);
        if shown.elapsed < u64::from(self.delay) * 1000 {
            return Vec::new();
        }
        let next = shown.page + 1;
        if next < self.pages.len() {
            return vec![self.show_page(ui, next)];
        }
        self.credits = None;
        if let Some(i) = ui.tree.find(CREDITS_LAYOUT) {
            ui.tree.hide(i);
        }
        ui.touch();
        vec!["credits end".to_string()]
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
