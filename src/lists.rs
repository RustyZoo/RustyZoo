//! The lists behind the scenario and freeform screens (main menu 5 and 6): the scenarios of the
//! `scenar*.cfg` files with their results and locks, the freeform maps of the `free*.cfg` files,
//! and the starting cash of the freeform screen. Pure data: the screens are in [`crate::menu`].

use std::collections::BTreeMap;

use crate::config::{Config, to_int};
use crate::text::from_cp1252;
use crate::timing::ini_whole;
use crate::vfs::Vfs;

/// The longest name of a scenario list file.
const MAX_FILE_NAME: usize = 12;
/// The result of a scenario that was never finished.
const NEVER: i32 = 3;

/// One scenario of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// The name of its section, which decides its place in the list.
    pub section: String,
    /// The number of the string of its name.
    pub name: i32,
    /// The scenario file.
    pub file: String,
    /// The key of `[scenario]` of the ini file that holds its result.
    pub result_key: String,
    /// The result keys that must all be won before this one can be played.
    pub locks: Vec<String>,
}

/// What the icon in front of a scenario says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Won,
    Playable,
    Locked,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Won => "won",
            Status::Playable => "playable",
            Status::Locked => "locked",
        }
    }
}

/// The result stored for a result key: 3 never finished, 2 lost, 1 won, 0 unlocked by a cheat.
pub fn result(ini: &Config, key: &str) -> i32 {
    ini_whole(ini, "scenario", key).unwrap_or(NEVER)
}

impl Scenario {
    /// Won below 2; else playable when every lock is won; else locked.
    pub fn status(&self, ini: &Config) -> Status {
        if result(ini, &self.result_key) < 2 {
            Status::Won
        } else if self.locks.iter().all(|key| result(ini, key) < 2) {
            Status::Playable
        } else {
            Status::Locked
        }
    }
}

fn text(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| from_cp1252(b)).collect()
}

/// The scenarios: every section of the resources `scenar*.cfg` whose whole name has at most 12
/// characters, by ascending byte order of the section names (a name met again replaces the
/// earlier one), those marked `expansion=` only when all those flags are in `expansions`.
pub fn scenario_list(vfs: &Vfs, expansions: i32) -> Vec<Scenario> {
    let mut found: BTreeMap<Vec<u8>, Scenario> = BTreeMap::new();
    for file in vfs.list(Some("scenar*.cfg")) {
        if file.len() > MAX_FILE_NAME {
            continue;
        }
        let Some(config) = vfs.read(&file).ok().and_then(|b| Config::parse(&b).ok()) else {
            continue;
        };
        for (name, section) in config.sections() {
            let wanted = section.get("expansion").and_then(to_int).unwrap_or(0);
            if wanted & !expansions != 0 {
                continue;
            }
            let get = |key: &str| section.get(key).map(text).unwrap_or_default();
            let result_key = match get("unlocks") {
                key if key.is_empty() => text(name),
                key => key,
            };
            found.insert(
                name.to_vec(),
                Scenario {
                    section: text(name),
                    name: section.get("name").and_then(to_int).unwrap_or(0),
                    file: get("scenario"),
                    result_key,
                    locks: section.get_all("locks").iter().map(|k| text(k)).collect(),
                },
            );
        }
    }
    found.into_values().collect()
}

/// One map of the freeform list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Freeform {
    /// The map description file.
    pub file: String,
    /// The number of the string of its name.
    pub name: i32,
    /// The name of the text of its description.
    pub description: String,
    /// The animation shown as its picture.
    pub icon: String,
}

/// The freeform maps: the `freeform` keys of `[freeform]` in every resource whose name starts with
/// `free` and ends with `.cfg` (none with `cvs` in it), the files in ascending order of their
/// names, the keys in file order; a description file that cannot be read is left out.
pub fn freeform_list(vfs: &Vfs) -> Vec<Freeform> {
    let mut files: Vec<String> = vfs
        .list(Some("free*.cfg"))
        .into_iter()
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            lower.starts_with("free") && lower.ends_with(".cfg") && !lower.contains("cvs")
        })
        .collect();
    files.sort_by_key(|f| f.to_ascii_lowercase());
    let mut maps = Vec::new();
    for file in files {
        let Some(config) = vfs.read(&file).ok().and_then(|b| Config::parse(&b).ok()) else {
            continue;
        };
        for path in config
            .section("freeform")
            .map(|s| s.get_all("freeform"))
            .unwrap_or_default()
        {
            let path = text(path);
            let Some(map) = vfs.read(&path).ok().and_then(|b| Config::parse(&b).ok()) else {
                continue;
            };
            let head = map.section("freeform");
            let get = |key: &str| head.and_then(|s| s.get(key)).map(text).unwrap_or_default();
            maps.push(Freeform {
                name: head
                    .and_then(|s| s.get("name"))
                    .and_then(to_int)
                    .unwrap_or(0),
                description: get("description"),
                icon: get("icon"),
                file: path,
            });
        }
    }
    maps
}

/// How hard a game with this much starting cash is, as the number of the string that says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Hard,
    Middle,
    Easy,
}

impl Difficulty {
    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Hard => "hard",
            Difficulty::Middle => "middle",
            Difficulty::Easy => "easy",
        }
    }

    pub fn string(self) -> i32 {
        match self {
            Difficulty::Hard => 11538,
            Difficulty::Middle => 11537,
            Difficulty::Easy => 11536,
        }
    }
}

/// The starting cash of the freeform screen with its limits and step (`[UI] MSStartingCash`,
/// `MSMinCash`, `MSMaxCash`, `MSCashIncrement`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cash {
    pub amount: i64,
    pub min: i64,
    pub max: i64,
    pub step: i64,
}

impl Cash {
    /// The cash the settings say, limited to the two bounds (the upper one wins if they cross).
    pub fn from_settings(ini: &Config) -> Cash {
        let get = |key: &str, default: i64| ini_whole(ini, "UI", key).map_or(default, i64::from);
        let mut cash = Cash {
            amount: 0,
            min: get("MSMinCash", 5000),
            max: get("MSMaxCash", 500_000),
            step: get("MSCashIncrement", 5000),
        };
        cash.set(get("MSStartingCash", 50_000));
        cash
    }

    /// Sets the amount, kept between the bounds.
    pub fn set(&mut self, amount: i64) {
        self.amount = amount.max(self.min).min(self.max);
    }

    pub fn can_up(&self) -> bool {
        self.amount < self.max
    }

    pub fn can_down(&self) -> bool {
        self.amount > self.min
    }

    /// One step more or less, stopping at the limit.
    pub fn up(&mut self) {
        self.set(self.amount.saturating_add(self.step));
    }

    pub fn down(&mut self) {
        self.set(self.amount.saturating_sub(self.step));
    }

    /// Reads what was typed: the currency symbol, the group separator of the user's locale and
    /// the spaces are taken out, then the number is limited to the bounds. Anything that is no number reads as 0 (the lower bound).
    pub fn typed(&mut self, typed: &[u8], symbol: &str) {
        let mut text = text(typed);
        if !symbol.is_empty() {
            text = text.replace(symbol, "");
        }
        text = text.replace(crate::text::format::money_group(), "");
        text.retain(|c| !c.is_whitespace());
        if let Some(n) = to_int(text.as_bytes()) {
            self.set(i64::from(n));
        }
    }

    /// Below 25,000 hard, above 100,000 easy.
    pub fn difficulty(&self) -> Difficulty {
        match self.amount {
            ..25_000 => Difficulty::Hard,
            100_001.. => Difficulty::Easy,
            _ => Difficulty::Middle,
        }
    }
}

#[cfg(test)]
mod tests;
