//! The research tree of a game: branches, categories and programs, read from the research lists
//! of the scenario at its start (research 3, 4, 7.4) and counted by the zoo status (research 9.1).
//! Funding, the step and the other effects than availability come later.

use crate::avail::Availability;
use crate::config::{Config, Section, to_float, to_int};
use crate::entity::Catalogue;
use crate::vfs::Vfs;

#[cfg(test)]
mod tests;

/// The most programs a tree takes: more than any game has, and a bound on a mod whose files name
/// each other many times.
const MAX_PROGRAMS: usize = 100_000;

/// One funding level of a branch: its text, the money and the work per month.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub name: i32,
    pub cost: f32,
    pub work: f32,
}

/// One research program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub name: i32,
    pub cost: f32,
    pub progress: f32,
    pub target: i32,
    pub effect: i32,
}

impl Program {
    /// Whether the program is complete: its progress has reached its cost.
    pub fn done(&self) -> bool {
        self.cost <= self.progress
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub name: i32,
    pub expansion: i32,
    /// The tick of the panel; it starts set.
    pub tick: bool,
    pub programs: Vec<Program>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub name: i32,
    pub funding: Vec<Level>,
    /// The index of the funding level; it starts at 0.
    pub level: usize,
    pub categories: Vec<Category>,
}

/// The branches in load order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree {
    pub branches: Vec<Branch>,
}

/// An integer key of a section, `default` when the key or the section is missing.
fn int(s: Option<&Section>, key: &str, default: i32) -> i32 {
    s.and_then(|s| s.get(key))
        .and_then(to_int)
        .unwrap_or(default)
}

fn float(s: Option<&Section>, key: &str) -> f32 {
    s.and_then(|s| s.get(key)).map_or(0.0, to_float)
}

fn read(vfs: &Vfs, path: &[u8]) -> Option<Config> {
    Config::parse(&vfs.read(&String::from_utf8_lossy(path)).ok()?).ok()
}

impl Tree {
    /// The tree of the scenario `file` (3.1): the files its `[start] research` values name.
    /// `None` when the scenario cannot be read; a scenario with no value has an empty tree.
    pub fn of_scenario(vfs: &Vfs, file: &str) -> Option<Tree> {
        let main = Config::parse(&vfs.read(file).ok()?).ok()?;
        let values = main
            .section("start")
            .map_or(&[][..], |s| s.get_all("research"));
        let mut tree = Tree::default();
        let mut left = MAX_PROGRAMS;
        for value in values {
            // `NAME.EXT` lists the files that start with the first 6 characters of NAME and end
            // with `.EXT`; a value with no dot, or ending in one, lists nothing.
            let value = String::from_utf8_lossy(value);
            let Some(dot) = value.rfind('.').filter(|&d| d + 1 < value.len()) else {
                continue;
            };
            let prefix: String = value[..dot].chars().take(6).collect();
            for list in vfs.listed(&prefix, &value[dot..]) {
                let Some(list) = read(vfs, list.as_bytes()) else {
                    continue;
                };
                for path in list
                    .section("branches")
                    .map_or(&[][..], |s| s.get_all("branch"))
                {
                    tree.add_branch(vfs, path, &mut left);
                }
            }
        }
        Some(tree)
    }

    /// Reads a branch file (3.2, 3.3): a branch whose name id is already in the tree is reused,
    /// with the new funding levels and the new categories after its own.
    fn add_branch(&mut self, vfs: &Vfs, path: &[u8], left: &mut usize) {
        let Some(cfg) = read(vfs, path) else {
            return;
        };
        let section = cfg.section("branch");
        let Some(name) = section.and_then(|s| s.get("name")).and_then(to_int) else {
            return;
        };
        let funding = section.map_or(&[][..], |s| s.get_all("funding"));
        let funding = funding.iter().map(|f| {
            let level = cfg.section(f);
            Level {
                name: int(level, "name", 0),
                cost: float(level, "cost"),
                work: float(level, "work"),
            }
        });
        let at = self.branches.iter().position(|b| b.name == name);
        let at = at.unwrap_or_else(|| {
            self.branches.push(Branch {
                name,
                funding: Vec::new(),
                level: 0,
                categories: Vec::new(),
            });
            self.branches.len() - 1
        });
        let branch = &mut self.branches[at];
        branch.funding = funding.collect();
        branch.level = 0;
        for path in section.map_or(&[][..], |s| s.get_all("category")) {
            if let Some(category) = Category::read(vfs, path, left) {
                branch.categories.push(category);
            }
        }
    }

    /// Every program in tree order.
    fn programs(&self) -> impl Iterator<Item = &Program> {
        let categories = self.branches.iter().flat_map(|b| &b.categories);
        categories.flat_map(|c| &c.programs)
    }

    /// The percentage of programs complete, 100 when there is none (zoo status 5.5).
    pub fn share(&self) -> i32 {
        let all = self.programs().count() as i64;
        let done = self.programs().filter(|p| p.done()).count() as i64;
        (done * 100).checked_div(all).unwrap_or(100) as i32
    }

    /// Resets or completes every program as the tree is built (3.3): the target of an effect 0
    /// is unavailable, or available when the program is complete, in tree order.
    pub fn apply(&self, cat: &Catalogue, avail: &mut Availability) {
        for p in self.programs().filter(|p| p.effect == 0) {
            avail.set(cat, p.target, p.done());
        }
    }
}

impl Category {
    fn read(vfs: &Vfs, path: &[u8], left: &mut usize) -> Option<Category> {
        let cfg = read(vfs, path)?;
        let section = cfg.section("category");
        let mut programs = Vec::new();
        for path in section.map_or(&[][..], |s| s.get_all("program")) {
            if *left == 0 {
                break;
            }
            let Some(cfg) = read(vfs, path) else {
                continue;
            };
            *left -= 1;
            let p = cfg.section("research");
            programs.push(Program {
                name: int(p, "name", 0),
                cost: float(p, "cost"),
                progress: 0.0,
                target: int(p, "target", -1),
                effect: int(p, "effect", -1),
            });
        }
        Some(Category {
            name: int(section, "name", 0),
            expansion: int(section, "expansion", 0),
            tick: true,
            programs,
        })
    }
}
