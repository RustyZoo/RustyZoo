//! The research tree of a game: branches, categories and programs, read from the research lists
//! of the scenario at its start (research 3, 4, 7.4) and counted by the zoo status (research 9.1).
//! The step (research 5) runs the funded branches; the section of a saved game is read and written
//! here (research 7). The characteristic effects (6.4, 6.7) are in [`effects`].

use crate::building::upgrade::Upgrade;
use crate::config::{Config, Section, to_float, to_int};
use crate::entity::Catalogue;
use crate::money::{Money, RESEARCH};
use crate::vfs::Vfs;

mod effects;
#[cfg(test)]
mod tests;
mod view;

pub use effects::{Life, Overlay};

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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    pub name: i32,
    pub cost: f32,
    pub progress: f32,
    pub target: i32,
    pub effect: i32,
    /// `effectval1` to `effectval3`.
    pub vals: [i32; 3],
    /// The rank: lower first.
    pub order: i32,
    /// `entityIcon`: the picture of a building upgrade's button.
    pub icon: String,
    /// `icon` and `helpid`: the picture of the panel and the string of its tooltip.
    pub picture: String,
    pub help: i32,
}

/// A program the step completed: the caller applies its effect (5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Done {
    pub name: i32,
    pub target: i32,
    pub effect: i32,
    pub vals: [i32; 3],
}

/// The float 2.3148148e-05 the step multiplies by instead of dividing by 43 200 (5.3).
const PER_MINUTE: f32 = 2.314_814_8e-5;

impl Program {
    /// Whether the program is complete: its progress has reached its cost.
    pub fn done(&self) -> bool {
        self.cost <= self.progress
    }

    /// The building upgrade the program gives (6.3): its name, picture, upkeep and the changes.
    fn upgrade(&self) -> Upgrade {
        let [upkeep, adult, child] = self.vals;
        Upgrade {
            name: self.name,
            icon: self.icon.clone(),
            upkeep,
            adult,
            child,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Category {
    pub name: i32,
    /// `helpid`: the string of the tooltip of its row.
    pub help: i32,
    pub expansion: i32,
    /// The tick of the panel; it starts set.
    pub tick: bool,
    pub programs: Vec<Program>,
}

impl Category {
    /// Whether the entry of its expansion is disabled (3.5): its programs are never chosen nor
    /// worked on.
    fn skipped(&self, off: &[bool; 4]) -> bool {
        let entry = usize::try_from(self.expansion)
            .ok()
            .and_then(|e| e.checked_add(1));
        entry.is_some_and(|e| off.get(e) == Some(&true))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Branch {
    pub name: i32,
    /// `noprogicon`: the picture shown while it has no current program.
    pub no_icon: String,
    pub funding: Vec<Level>,
    /// The index of the funding level; it starts at 0.
    pub level: usize,
    pub categories: Vec<Category>,
    /// The category and program the branch works on; not saved.
    pub current: Option<(usize, usize)>,
}

/// The branches in load order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tree {
    pub branches: Vec<Branch>,
    /// The expansion entries that are disabled (3.5): all, base game, first expansion, second expansion.
    pub off: [bool; 4],
    /// The value a start sets the purchase panels' selector to (3.5); none leaves it as it was.
    pub selection: Option<i32>,
    /// The manager's accumulator, in milliseconds (5.1).
    pub(crate) acc: u32,
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

/// The name of a picture a key gives, empty when there is none.
fn picture(s: Option<&Section>, key: &str) -> String {
    let value = s.and_then(|s| s.get(key));
    value.map_or(String::new(), |v| {
        crate::vfs::name_from_bytes(v.trim_ascii())
    })
}

fn read(vfs: &Vfs, path: &[u8]) -> Option<Config> {
    Config::parse(&vfs.read(&String::from_utf8_lossy(path)).ok()?).ok()
}

impl Tree {
    /// The tree of the scenario `file` (3.1): the files its `[start] research` values name.
    /// `None` when the scenario cannot be read; a scenario with no value has an empty tree.
    /// Each branch file read gives its branch a current program, by a draw from `rand` (5.5).
    /// A saved game's scenario gets the same expansion entries as a start (3.5, 7.2).
    pub fn of_scenario(vfs: &Vfs, file: &str, rand: &mut dyn FnMut() -> u32) -> Option<Tree> {
        let main = Config::parse(&vfs.read(file).ok()?).ok()?;
        let values = main
            .section("start")
            .map_or(&[][..], |s| s.get_all("research"));
        let mut tree = Tree {
            off: expansions_off(main.section("desc")),
            selection: expansion_selection(main.section("desc")),
            ..Tree::default()
        };
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
                    tree.add_branch(vfs, path, &mut left, rand);
                }
            }
        }
        Some(tree)
    }

    /// Reads a branch file (3.2, 3.3): a branch whose name id is already in the tree is reused,
    /// with the new funding levels and the new categories after its own.
    fn add_branch(
        &mut self,
        vfs: &Vfs,
        path: &[u8],
        left: &mut usize,
        rand: &mut dyn FnMut() -> u32,
    ) {
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
                no_icon: picture(section, "noprogicon"),
                ..Branch::default()
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
        branch.choose(&self.off, rand);
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

    /// The building upgrades of the programs complete, as `(building name id, upgrade)` in tree
    /// order (research 6.3, 7.2): the lists of a game that starts or loads.
    pub fn upgrades(&self) -> Vec<(i32, Upgrade)> {
        let done = self.programs().filter(|p| p.effect == 1 && p.done());
        done.map(|p| (p.target, p.upgrade())).collect()
    }

    /// The upgrade of the program named `name` and the building it is for, when it is an effect 1
    /// program: what a completion adds.
    pub fn upgrade_of(&self, name: i32) -> Option<(i32, Upgrade)> {
        let p = self.programs().find(|p| p.name == name && p.effect == 1)?;
        Some((p.target, p.upgrade()))
    }

    /// Resets or completes every program as the tree is built (3.3) or loaded (7.2), in tree order:
    /// the targets of the effect 0 programs, with whether each is available (the program is complete), and
    /// the effects 2 to 7 of the complete ones are laid on `over`, which starts empty (6.9, R4). The
    /// changes of the life counter come back for the animals the map already holds (7.4).
    pub fn apply(&mut self, cat: &Catalogue, over: &mut Overlay) -> (Vec<(i32, bool)>, Vec<Life>) {
        *over = Overlay::default();
        let (mut flags, mut life) = (Vec::new(), Vec::new());
        for n in 0..self.programs().count() {
            let Some(p) = self.programs().nth(n) else {
                break;
            };
            let done = Done {
                name: p.name,
                target: p.target,
                effect: p.effect,
                vals: p.vals,
            };
            if p.effect == 0 {
                flags.push((p.target, p.done()));
            } else if p.done() {
                life.extend(self.complete(cat, over, &done));
            }
        }
        (flags, life)
    }

    /// Applies the characteristic effect of a completed program (6.4, 6.7): a change of the types
    /// (2, 4, 5), or a discount on the other programs and a training mark (7). The changes of the
    /// life counter are returned for the live animals.
    pub fn complete(&mut self, cat: &Catalogue, over: &mut Overlay, d: &Done) -> Vec<Life> {
        let [percent, staff, mark] = d.vals;
        match d.effect {
            2 | 4 | 5 => return over.apply(cat, d.effect, d.target, d.vals),
            7 => {
                let kept = 100i32.wrapping_sub(percent) as f32;
                let all = self.branches.iter_mut().flat_map(|b| &mut b.categories);
                for p in all
                    .flat_map(|c| &mut c.programs)
                    .filter(|p| p.effect == d.target)
                {
                    p.cost = (f64::from(kept) * f64::from(p.cost) * f64::from(0.01_f32)) as f32;
                }
                over.train(cat, staff, mark);
            }
            _ => {}
        }
        Vec::new()
    }
}

/// Reads the bytes of the research records (7.1): `(kind, name id, value)` until the end record or
/// the end of the bytes; the value is the raw word, a tick's byte included.
fn records(bytes: &[u8]) -> Vec<(i32, u32, u32)> {
    let word = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let mut out = Vec::new();
    let mut at = 4;
    while let Some(kind) = word(at).map(|k| k as i32).filter(|&k| k >= 0) {
        let Some(name) = word(at + 4) else { break };
        let (value, size) = match kind {
            1 => (bytes.get(at + 8).map(|&b| u32::from(b)), 9),
            0 | 2 => (word(at + 8), 12),
            _ => break,
        };
        let Some(value) = value else { break };
        out.push((kind, name, value));
        at += size;
    }
    out
}

impl Tree {
    /// The research section of a saved game is read (7.2): everything is reset (funding level 0,
    /// ticks set, no progress), the records set what they name (the first match by name id;
    /// an unknown id is ignored, a funding level out of range is 0), the effects of the programs
    /// complete are the caller's (`apply`), and each branch chooses its current program.
    /// `bytes` is empty for a file with no section (before version 40).
    pub fn load(&mut self, bytes: &[u8], rand: &mut dyn FnMut() -> u32) {
        for b in &mut self.branches {
            b.level = 0;
            for c in &mut b.categories {
                c.tick = true;
                c.programs.iter_mut().for_each(|p| p.progress = 0.0);
            }
        }
        for (kind, name, value) in records(bytes) {
            let name = name as i32;
            match kind {
                0 => {
                    if let Some(b) = self.branches.iter_mut().find(|b| b.name == name) {
                        b.level = Some(value as usize)
                            .filter(|&l| l < b.funding.len())
                            .unwrap_or(0);
                    }
                }
                1 => {
                    let mut all = self.branches.iter_mut().flat_map(|b| &mut b.categories);
                    if let Some(c) = all.find(|c| c.name == name) {
                        c.tick = value != 0;
                    }
                }
                _ => {
                    let categories = self.branches.iter_mut().flat_map(|b| &mut b.categories);
                    let mut all = categories.flat_map(|c| &mut c.programs);
                    if let Some(p) = all.find(|p| p.name == name) {
                        p.progress = f32::from_bits(value);
                    }
                }
            }
        }
        for b in &mut self.branches {
            b.choose(&self.off, rand);
        }
    }

    /// The research section as the writer makes it from the live tree (7.1, 7.5): the leading word,
    /// then a record for each branch, its categories and their programs, then the end record.
    pub fn save(&self) -> Vec<u8> {
        let mut o = 0u32.to_le_bytes().to_vec();
        let word = |o: &mut Vec<u8>, kind: i32, name: i32| {
            o.extend(kind.to_le_bytes());
            o.extend(name.to_le_bytes());
        };
        for b in &self.branches {
            word(&mut o, 0, b.name);
            o.extend((b.level as u32).to_le_bytes());
            for c in &b.categories {
                word(&mut o, 1, c.name);
                o.push(c.tick.into());
                for p in &c.programs {
                    word(&mut o, 2, p.name);
                    o.extend(p.progress.to_le_bytes());
                }
            }
        }
        o.extend((-1i32).to_le_bytes());
        o
    }
}

/// The disabled expansion entries a scenario starts with (3.5).
fn expansions_off(desc: Option<&Section>) -> [bool; 4] {
    let name = int(desc, "name", 0);
    if (16000..=16015).contains(&name) || name == 16022 {
        [false, false, true, true]
    } else if int(desc, "expansionLocked", 0) != 0 {
        let own = int(desc, "expansion", 0);
        [false, false, own != 2, own != 3]
    } else {
        [false; 4]
    }
}

/// The value the selector is set to at a start (3.5): the base game for the base scenarios, the
/// scenario's `defaultExpansion` (else its `expansion`) for a locked one, else nothing.
fn expansion_selection(desc: Option<&Section>) -> Option<i32> {
    let name = int(desc, "name", 0);
    if (16000..=16015).contains(&name) || name == 16022 {
        Some(1)
    } else if int(desc, "expansionLocked", 0) != 0 {
        Some(int(desc, "defaultExpansion", int(desc, "expansion", 0)))
    } else {
        None
    }
}

impl Tree {
    /// One step of `ms` milliseconds of the research manager (5.1): every 3 s of simulated time
    /// each branch pays and works (5.3). The programs completed, in order, are returned.
    pub fn step(&mut self, ms: u32, money: &mut Money, rand: &mut dyn FnMut() -> u32) -> Vec<Done> {
        self.acc = self.acc.wrapping_add(ms);
        let minutes = self.acc.wrapping_mul(7200) / 60000;
        let mut done = Vec::new();
        if minutes >= 360 {
            self.acc = 0;
            for b in &mut self.branches {
                b.update(minutes, &self.off, money, rand, &mut done);
            }
        }
        done
    }
}

impl Branch {
    /// The program the branch works on, if its category is ticked and not skipped.
    fn working(&self, off: &[bool; 4]) -> Option<(usize, usize)> {
        let (c, p) = self.current?;
        let category = self.categories.get(c)?;
        let on = category.tick && !category.skipped(off);
        (on && p < category.programs.len()).then_some((c, p))
    }

    /// One branch update with `m` game minutes (5.3). Every operation is a `f32` one, rounded as
    /// the original's unit rounds it.
    fn update(
        &mut self,
        m: u32,
        off: &[bool; 4],
        money: &mut Money,
        rand: &mut dyn FnMut() -> u32,
        done: &mut Vec<Done>,
    ) {
        let Some((c, p)) = self.working(off) else {
            self.choose(off, rand);
            return;
        };
        if let Some(level) = self.funding.get(self.level) {
            let by = |x: f32| (f64::from(m) * f64::from(x)) as f32 * PER_MINUTE;
            let (pay, gain) = (by(level.cost), by(level.work));
            if pay <= money.cash {
                if pay != 0.0 {
                    money.spend(RESEARCH, pay);
                }
                self.categories[c].programs[p].progress += gain;
            }
        }
        let program = &self.categories[c].programs[p];
        if program.done() {
            done.push(Done {
                name: program.name,
                target: program.target,
                effect: program.effect,
                vals: program.vals,
            });
            self.choose(off, rand);
        }
    }

    /// Chooses the current program (5.5): a program that was started first, else one of the
    /// lowest rank, by one draw.
    pub fn choose(&mut self, off: &[bool; 4], rand: &mut dyn FnMut() -> u32) {
        let eligible =
            (self.categories.iter().enumerate()).filter(|(_, c)| c.tick && !c.skipped(off));
        let open: Vec<(usize, usize)> = eligible
            .flat_map(|(ci, c)| {
                let programs = c.programs.iter().enumerate();
                programs
                    .filter(|(_, p)| p.progress < p.cost && p.cost > 0.0)
                    .map(move |(pi, _)| (ci, pi))
            })
            .collect();
        let program = |&(c, p): &(usize, usize)| &self.categories[c].programs[p];
        let mut current = open.iter().find(|&a| program(a).progress > 0.0).copied();
        if current.is_none()
            && let Some(lowest) = open.iter().map(program).map(|p| p.order).min()
        {
            let first: Vec<_> = open.iter().filter(|a| program(a).order <= lowest).collect();
            current = first.get(rand() as usize % first.len()).map(|a| **a);
        }
        self.current = current;
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
                vals: [1, 2, 3].map(|n| int(p, &format!("effectval{n}"), 0)),
                order: int(p, "order", 0),
                icon: picture(p, "entityIcon"),
                picture: picture(p, "icon"),
                help: int(p, "helpid", 0),
            });
        }
        Some(Category {
            name: int(section, "name", 0),
            help: int(section, "helpid", 0),
            expansion: int(section, "expansion", 0),
            tick: true,
            programs,
        })
    }
}
