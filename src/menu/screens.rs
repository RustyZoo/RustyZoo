//! The scenario screen (layout 32) and the freeform screen (layout 39): their lists, the picture
//! and text of the selected entry, the starting cash and the Play buttons (main menu 5 and 6).
//! The screens themselves are shown by the buttons of the main menu; this fills them in.

use super::{Menu, Request, Start, enable};
use crate::config::{Config, to_int};
use crate::lists::{Cash, Status};
use crate::money::{SYMBOL, StartCash, Symbol};
use crate::text::to_cp1252;
use crate::ui::interface::{Action, Interface};
use crate::ui::widgets::Row;
use crate::vfs::Vfs;

/// The buttons of the main menu that open the screens.
const SCENARIOS: i32 = 7111;
const FREEFORM: i32 = 7110;

/// The scenario screen: list, picture, story, Play, Back.
const SCENARIO_LIST: i32 = 50002;
const SCENARIO_PICTURE: i32 = 50001;
const SCENARIO_STORY: i32 = 50004;
const SCENARIO_PLAY: i32 = 50008;

/// The freeform screen: list, picture, description, cash field and its spinners, difficulty, Play.
const MAP_LIST: i32 = 11504;
const MAP_PICTURE: i32 = 11501;
const MAP_DESCRIPTION: i32 = 11507;
const CASH_FIELD: i32 = 11510;
const CASH_UP: i32 = 11511;
const CASH_DOWN: i32 = 11512;
const DIFFICULTY: i32 = 11531;
const MAP_PLAY: i32 = 11513;
const BACK: [i32; 2] = [50009, 11514];

/// The icons in front of a scenario: won, playable, locked.
const ICONS: [&str; 3] = [
    "ui/scenario/iconc/iconc",
    "ui/scenario/iconp/iconp",
    "ui/scenario/iconf/iconf",
];
/// The line of an empty freeform list (a constant of the program).
const NO_MAPS: &str = "No maps found.";

/// The text of a key of a section of a scenario file, if the file reads.
fn desc(vfs: &Vfs, file: &str, section: &str) -> impl Fn(&str) -> Option<String> {
    let config = vfs
        .read(file)
        .ok()
        .and_then(|bytes| Config::parse(&bytes).ok())
        .unwrap_or_default();
    let section = section.to_string();
    move |key| {
        let value = config.section(&section)?.get(key)?;
        Some(value.iter().map(|&b| crate::text::from_cp1252(b)).collect())
    }
}

/// Whether a start clears the accounts: `reset` above 0, else `resetdate` above 0 (both
/// integers; 0 or less does nothing; exhibits and money 5.4).
fn resets(start: &impl Fn(&str) -> Option<String>) -> bool {
    above_zero(start, "reset") || above_zero(start, "resetdate")
}

/// Whether the integer `key` of `[start]` is above 0 (missing, unreadable, 0 or less is not).
fn above_zero(start: &impl Fn(&str) -> Option<String>, key: &str) -> bool {
    start(key).and_then(|v| to_int(v.as_bytes())) > Some(0)
}

/// The row an action tells of, or none (-1 is none or a disabled row).
fn row(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

impl Menu {
    /// The buttons of the screens; the lines for the log.
    pub(super) fn pressed(&mut self, vfs: &Vfs, ui: &mut Interface, id: i32) -> Vec<String> {
        let mut log = Vec::new();
        match id {
            SCENARIOS => self.open_scenarios(vfs, ui, &mut log),
            FREEFORM => self.open_freeform(vfs, ui, &mut log),
            SCENARIO_PLAY => self.play_scenario(vfs, ui, &mut log),
            MAP_PLAY => self.play_freeform(vfs, ui, &mut log),
            CASH_UP | CASH_DOWN => {
                match id {
                    CASH_UP => self.cash.up(),
                    _ => self.cash.down(),
                }
                self.show_cash(ui, true, &mut log);
            }
            id if BACK.contains(&id) => log.push("back".to_string()),
            _ => {}
        }
        log
    }

    /// What the lists and the cash field tell: the selection moved, a double click, an edit that
    /// ended. The lines for the log.
    pub fn action(&mut self, vfs: &Vfs, ui: &mut Interface, action: Action) -> Vec<String> {
        let mut log = Vec::new();
        match action {
            Action::Row(SCENARIO_LIST, value) => {
                self.describe_scenario(vfs, ui, row(value), &mut log)
            }
            Action::Row(MAP_LIST, value) => self.describe_freeform(vfs, ui, row(value), &mut log),
            Action::Double(SCENARIO_LIST, _) => self.play_scenario(vfs, ui, &mut log),
            Action::Double(MAP_LIST, _) => self.play_freeform(vfs, ui, &mut log),
            Action::Edited(CASH_FIELD) => {
                let typed = ui.text(CASH_FIELD).unwrap_or_default();
                self.cash.typed(&typed, &ui.string(SYMBOL));
                self.show_cash(ui, true, &mut log);
            }
            _ => {}
        }
        log
    }

    // ---- The scenario screen (5)

    /// The screen is shown: the list is filled again, with the icon of each scenario, and the
    /// selection that was there, or the first entry, is selected and described.
    fn open_scenarios(&mut self, vfs: &Vfs, ui: &mut Interface, log: &mut Vec<String>) {
        let mut rows = Vec::new();
        log.push(format!("scenarios {}", self.scenarios.len()));
        for s in &self.scenarios {
            let status = s.status(&self.ini);
            let icon = ICONS[status as usize];
            rows.push(Row {
                text: to_cp1252(&ui.string(s.name)),
                icon: ui.tree.picture(vfs, icon),
                ..Row::default()
            });
            log.push(format!(
                "scenario {} {} {}",
                s.section,
                status.name(),
                s.name
            ));
        }
        ui.set_rows(SCENARIO_LIST, rows);
        let first = self.scenario.filter(|&i| i < self.scenarios.len());
        let first = first.or((!self.scenarios.is_empty()).then_some(0));
        ui.select_row(SCENARIO_LIST, first);
        self.describe_scenario(vfs, ui, first, log);
    }

    /// Selecting a scenario: Play is off until the entry is known to be playable, and the picture
    /// and story are those of its `[desc]` section (the locked ones if it is locked); a missing
    /// picture or story hides its element.
    fn describe_scenario(
        &mut self,
        vfs: &Vfs,
        ui: &mut Interface,
        selected: Option<usize>,
        log: &mut Vec<String>,
    ) {
        enable(ui, SCENARIO_PLAY, false);
        self.scenario = selected.filter(|&i| i < self.scenarios.len());
        let Some(s) = self.scenario.map(|i| &self.scenarios[i]) else {
            ui.set_picture(SCENARIO_PICTURE, None);
            ui.set_text(SCENARIO_STORY, b"");
            ui.set_hidden(SCENARIO_STORY, true);
            log.push("scenario none".to_string());
            return;
        };
        let status = s.status(&self.ini);
        let playable = status != Status::Locked;
        let get = desc(vfs, &s.file, "desc");
        let (picture, story) = match playable {
            true => ("picture", "story"),
            false => ("lockedpicture", "lockedstory"),
        };
        let picture = get(picture);
        let shown = picture
            .as_deref()
            .and_then(|name| ui.tree.picture(vfs, name));
        ui.set_picture(SCENARIO_PICTURE, shown);
        let story = get(story).map(|name| ui.text_entry(&to_cp1252(&name)));
        self.set_story(ui, SCENARIO_STORY, story);
        enable(ui, SCENARIO_PLAY, playable);
        log.push(format!(
            "scenario select {} {} picture {} story {} play {}",
            s.section,
            status.name(),
            shown.map_or("none", |_| picture.as_deref().unwrap_or("none")),
            story_state(ui, SCENARIO_STORY),
            if playable { "enabled" } else { "disabled" },
        ));
    }

    /// Puts a story or a description into its text, or hides the text if there is none.
    fn set_story(&self, ui: &mut Interface, id: i32, text: Option<Vec<u8>>) {
        let text = text.filter(|t| !t.is_empty());
        ui.set_text(id, text.as_deref().unwrap_or_default());
        ui.set_hidden(id, text.is_none());
    }

    /// Play, or a double click on an entry, if Play is enabled: the map named by `[start]
    /// savegame` of the scenario file is opened (main menu 8.3).
    fn play_scenario(&mut self, vfs: &Vfs, ui: &mut Interface, log: &mut Vec<String>) {
        let Some(s) = self.scenario.map(|i| &self.scenarios[i]) else {
            return;
        };
        if !enabled(ui, SCENARIO_PLAY) {
            return;
        }
        log.push(format!("play scenario {} {}", s.section, s.file));
        let label = format!("scenario {}", s.section);
        let file = s.file.clone();
        self.open_from(vfs, &file, label, None);
    }

    /// Asks the game to open the map `[start] savegame` of the scenario file `file` names. A
    /// freeform start (`freeform` is the starting cash of its screen) sets the cash to that
    /// amount; a scenario applies its `setcash` then its `addcash` (main menu 8.3).
    fn open_from(&mut self, vfs: &Vfs, file: &str, label: String, freeform: Option<i64>) {
        let start = desc(vfs, file, "start");
        let number = |key: &str| {
            start(key)
                .and_then(|v| to_int(v.as_bytes()))
                .map(|n| n as f32)
        };
        self.request = Some(match start("savegame") {
            Some(path) => Request::Open(Start {
                path,
                label,
                forget_last: true,
                freeform: freeform.is_some(),
                cash: match freeform {
                    Some(amount) => StartCash {
                        set: Some(amount as f32),
                        add: None,
                    },
                    None => StartCash {
                        set: number("setcash"),
                        add: number("addcash"),
                    },
                },
                reset: resets(&start),
                reset_fences: above_zero(&start, "resetfences"),
            }),
            None => Request::Message(format!("{file}: no map to start")),
        });
    }

    // ---- The freeform screen (6)

    /// The screen is shown: the list of maps (or its one line), the selection that was there or
    /// the first map, and the starting cash from the settings.
    fn open_freeform(&mut self, vfs: &Vfs, ui: &mut Interface, log: &mut Vec<String>) {
        let rows: Vec<Row> = match self.freeforms.is_empty() {
            true => vec![Row {
                text: NO_MAPS.as_bytes().to_vec(),
                ..Row::default()
            }],
            false => self
                .freeforms
                .iter()
                .map(|m| Row {
                    text: to_cp1252(&ui.string(m.name)),
                    ..Row::default()
                })
                .collect(),
        };
        log.push(format!("freeform {}", self.freeforms.len()));
        for m in &self.freeforms {
            log.push(format!("map {} {}", m.file, m.name));
        }
        ui.set_rows(MAP_LIST, rows);
        let first = self.freeform.filter(|&i| i < self.freeforms.len());
        let first = first.or((!self.freeforms.is_empty()).then_some(0));
        ui.select_row(MAP_LIST, first.or(Some(0)));
        self.describe_freeform(vfs, ui, first, log);
        self.cash = Cash::from_settings(&self.ini);
        self.show_cash(ui, false, log);
    }

    /// Selecting a map: its picture and description, hidden when it has none.
    fn describe_freeform(
        &mut self,
        vfs: &Vfs,
        ui: &mut Interface,
        selected: Option<usize>,
        log: &mut Vec<String>,
    ) {
        self.freeform = selected.filter(|&i| i < self.freeforms.len());
        let Some(m) = self.freeform.map(|i| &self.freeforms[i]) else {
            ui.set_picture(MAP_PICTURE, None);
            self.set_story(ui, MAP_DESCRIPTION, None);
            enable(ui, MAP_PLAY, false);
            log.push("map none".to_string());
            return;
        };
        let shown = (!m.icon.is_empty())
            .then(|| ui.tree.picture(vfs, &m.icon))
            .flatten();
        ui.set_picture(MAP_PICTURE, shown);
        let text = (!m.description.is_empty()).then(|| ui.text_entry(&to_cp1252(&m.description)));
        self.set_story(ui, MAP_DESCRIPTION, text);
        enable(ui, MAP_PLAY, true);
        log.push(format!(
            "map select {} picture {} description {}",
            m.file,
            if shown.is_some() { &m.icon } else { "none" },
            story_state(ui, MAP_DESCRIPTION),
        ));
    }

    /// Shows the amount, the difficulty it makes and the spinners (each off at its limit); a
    /// change is written to `[UI] MSStartingCash`.
    fn show_cash(&mut self, ui: &mut Interface, changed: bool, log: &mut Vec<String>) {
        let cash = self.cash;
        let symbol = Symbol::new(ui.string(SYMBOL));
        ui.set_text(CASH_FIELD, &to_cp1252(&symbol.text(cash.amount)));
        enable(ui, CASH_UP, cash.can_up());
        enable(ui, CASH_DOWN, cash.can_down());
        let difficulty = cash.difficulty();
        ui.set_text(DIFFICULTY, &to_cp1252(&ui.string(difficulty.string())));
        if changed {
            self.write_back("UI", "MSStartingCash", &cash.amount.to_string());
        }
        log.push(format!(
            "cash {} {} up {} down {}",
            cash.amount,
            difficulty.name(),
            if cash.can_up() { "on" } else { "off" },
            if cash.can_down() { "on" } else { "off" },
        ));
    }

    /// Play, or a double click on a map, if Play is enabled: the map named by `[start] savegame`
    /// of its description file is opened, and the starting cash goes with it.
    fn play_freeform(&mut self, vfs: &Vfs, ui: &mut Interface, log: &mut Vec<String>) {
        let Some(m) = self.freeform.map(|i| &self.freeforms[i]) else {
            return;
        };
        if !enabled(ui, MAP_PLAY) {
            return;
        }
        log.push(format!(
            "play freeform {} cash {}",
            m.file, self.cash.amount
        ));
        let label = format!("freeform cash {}", self.cash.amount);
        let file = m.file.clone();
        self.open_from(vfs, &file, label, Some(self.cash.amount));
    }
}

/// Whether the button `id` takes clicks.
fn enabled(ui: &Interface, id: i32) -> bool {
    ui.tree
        .find(id)
        .is_some_and(|i| ui.tree.elements[i].state & crate::ui::layout::DISABLED == 0)
}

/// `shown` or `hidden`, for the log.
fn story_state(ui: &Interface, id: i32) -> &'static str {
    match ui.tree.find(id) {
        Some(i) if ui.tree.elements[i].state & crate::ui::layout::HIDDEN == 0 => "shown",
        _ => "hidden",
    }
}

#[cfg(test)]
mod tests;
