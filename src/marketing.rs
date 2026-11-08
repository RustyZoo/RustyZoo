//! The marketing of a game (marketing): the funding levels the scenario names, the level the
//! player picked, the monthly charge every 3 s of simulated time and the benefit the new guest
//! check adds to the rating. The panel is [`crate::panel::marketing`]'s.

use crate::config::{Config, Section, to_float, to_int};
use crate::money::{MARKETING, Money};
use crate::vfs::Vfs;

#[cfg(test)]
mod tests;

/// The float 2.3148148e-05 the update multiplies by instead of dividing by 43 200 (4.2).
const PER_MINUTE: f32 = 2.314_814_8e-5;

/// One funding level: its text, the money per month and the rating it adds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Level {
    pub name: i32,
    pub cost: f32,
    pub benefit: i32,
}

/// The levels in file order and the index of the current one.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Funding {
    pub levels: Vec<Level>,
    pub level: usize,
}

/// The marketing manager: the clock of its charge and the configuration, if the set-up made one.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Marketing {
    acc: u32,
    config: Option<Funding>,
}

impl Marketing {
    /// What the set-up of the scenario `file` makes (3.2): no configuration when `[start]
    /// marketing` is missing or names a file that cannot be read, else level 0 over the levels the
    /// file it names lists, none at all when that file cannot be read.
    pub(crate) fn of_scenario(vfs: &Vfs, file: &str) -> Marketing {
        let outer = read(vfs, file).and_then(|c| {
            let name = c.section("start")?.get("marketing")?.to_vec();
            read(vfs, &String::from_utf8_lossy(&name))
        });
        let Some(outer) = outer else {
            return Marketing::default();
        };
        let inner = outer.section("marketing").and_then(|s| s.get("marketing"));
        let levels = inner
            .and_then(|name| read(vfs, &String::from_utf8_lossy(name)))
            .map_or_else(Vec::new, |c| levels(&c));
        Marketing {
            acc: 0,
            config: Some(Funding { levels, level: 0 }),
        }
    }

    /// The configuration, if there is one.
    pub(crate) fn config(&self) -> Option<&Funding> {
        self.config.as_ref()
    }

    /// The current level; none without a configuration or with an empty list.
    fn current(&self) -> Option<&Level> {
        let c = self.config.as_ref()?;
        c.levels.get(c.level)
    }

    /// The rating the current level adds to the new guest check (5): 0 without a level.
    pub(crate) fn benefit(&self) -> i32 {
        self.current().map_or(0, |l| l.benefit)
    }

    /// One step of `ms` milliseconds (4.1): every 3 s of simulated time the current level is paid.
    pub(crate) fn step(&mut self, ms: u32, money: &mut Money) {
        self.acc = self.acc.wrapping_add(ms);
        let minutes = self.acc.wrapping_mul(7200) / 60000;
        if minutes < 360 {
            return;
        }
        self.acc = 0;
        // An empty list pays nothing (3.4).
        let Some(level) = self.current() else { return };
        // One single-precision operation after the other (main loop 5.4).
        let pay = minutes as f32 * level.cost * PER_MINUTE;
        // A share the cash cannot cover is skipped and never owed (4.2).
        if !(money.cash < pay) {
            money.spend(MARKETING, pay);
        }
    }

    /// The "less" and "more" buttons (6.3): one level down, held at 0, or up, held at the last.
    pub(crate) fn fund(&mut self, up: bool) {
        let Some(c) = &mut self.config else { return };
        c.level = match (up, c.levels.len()) {
            (_, 0) => 0,
            (true, n) => (c.level + 1).min(n - 1),
            (false, _) => c.level.saturating_sub(1),
        };
    }

    /// The word of the file (7.1): the level, or 0 without a configuration.
    pub(crate) fn saved(&self) -> u32 {
        self.config.as_ref().map_or(0, |c| c.level as u32)
    }

    /// The word a file stores (7.2): the level when it is below the list's count, else 0; dropped
    /// without a configuration.
    pub(crate) fn load(&mut self, level: u32) {
        if let Some(c) = &mut self.config {
            c.level = usize::try_from(level).ok().filter(|&l| l < c.levels.len()).unwrap_or(0);
        }
    }
}

fn read(vfs: &Vfs, name: &str) -> Option<Config> {
    Config::parse(&vfs.read(name).ok()?).ok()
}

/// The levels the `funding` values of the file name, in order; a missing key or section leaves 0.
fn levels(file: &Config) -> Vec<Level> {
    let funding = file.section("marketing").map_or(&[][..], |s| s.get_all("funding"));
    funding
        .iter()
        .map(|name| {
            let s: Option<&Section> = file.section(name);
            let get = |key: &str| s.and_then(|s| s.get(key));
            Level {
                name: get("name").and_then(to_int).unwrap_or(0),
                cost: get("cost").map_or(0.0, to_float),
                benefit: get("benefit").and_then(to_int).unwrap_or(0),
            }
        })
        .collect()
}
