//! The seven shipped tutorials played in the real interface of the full game (scenarios 10.8):
//! from the start, as far as presses and answers take them, and from a save prepared at test time
//! from the shipped map and the shipped scenario file, to the end dialog and the next tutorial.
//! Ignored without `ZT_DATA_DIR`.

use super::*;
use crate::config::{Config, to_int};
use crate::entity::Catalogue;
use crate::fixtures::TempDir;
use crate::menu::Menu;
use crate::strings::StringTables;
use crate::text::Fonts;
use crate::ui::interface::Interface;
use crate::ui::layout::{DISABLED, Tree, installed_expansions};
use crate::vfs::Vfs;
use std::path::Path;

/// One pass of the test is one pass of the scenario: a step of 1500 ms.
const STEP: u64 = 1500;

/// An install and the files every game of the test shares.
struct Rig {
    vfs: Vfs,
    cat: Catalogue,
    user: TempDir,
}

/// The install of `ZT_DATA_DIR`, with a user folder of its own.
fn rig(name: &str) -> Rig {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(Path::new(&dir)).unwrap();
    let cat = Catalogue::load(&vfs).unwrap();
    Rig {
        vfs,
        cat,
        user: TempDir::new(name),
    }
}

/// The interface of the install, as a run builds it.
fn interface(vfs: &Vfs, size: (u32, u32)) -> Interface {
    let tables = StringTables::open(vfs.root()).unwrap();
    let mut tree = Tree::load(vfs, "ui/gamescrn.lyt", installed_expansions(vfs));
    let mut fonts = Fonts::default();
    tree.resolve(size.0 as i32, size.1 as i32, |font| {
        let (face, points) = font.map_or(("Arial", "8"), |(face, points)| {
            let string = |n: i32| u32::try_from(n).map_or("", |n| tables.string(n));
            (string(face), string(points))
        });
        fonts.get(face, points).line_height()
    });
    assert_eq!(tree.failure, None);
    Interface::new(tree, Box::new(tables))
}

impl Rig {
    /// A program on its main menu: the scenario list of the install, no drawing after the first
    /// frame, and a step of 1500 ms at most.
    fn app(&self, which: &str) -> App<'_> {
        let user = self.user.0.join(which);
        let size = (1024, 768);
        let ini = Config::parse(self.vfs.ini()).unwrap();
        let expansions = installed_expansions(&self.vfs);
        let mut menu = Menu::new(ini, self.vfs.root(), Some(user.clone()), None, expansions);
        let lists = (
            crate::lists::scenario_list(&self.vfs, expansions),
            crate::lists::freeform_list(&self.vfs),
        );
        menu.set_lists(lists.0, lists.1);
        let mut app = App::menu(
            MapRenderer::new(&self.vfs, &self.cat),
            Settings::default(),
            Timers::new(66, 1_000_000, STEP as u32),
            size,
            interface(&self.vfs, size),
            menu,
            None,
        )
        .unwrap();
        app.set_user_folder(Some(user));
        app.pass(0, &[]);
        app
    }
}

/// A leaf or a group of a scenario file, in the order the state words are saved (5.3).
#[derive(Debug, Clone)]
struct Goal {
    name: String,
    leaf: bool,
    rule: [i32; 3],
    /// A trigger 2 with `truleb` 0: the goal shows a popup.
    popup: bool,
}

/// The goals of a scenario file as the manager saves them: depth first, a group before its
/// children. The timer's word comes before them all.
fn goals(cfg: &Config) -> Vec<Goal> {
    fn walk(cfg: &Config, name: &[u8], out: &mut Vec<Goal>) {
        let s = cfg.section(name);
        let int = |key: &str| s.and_then(|s| s.get(key)).and_then(to_int).unwrap_or(0);
        let kids = s.map_or(&[][..], |s| s.get_all("child"));
        let leaf = int("rtype") == 0;
        out.push(Goal {
            name: String::from_utf8_lossy(name).into_owned(),
            leaf,
            rule: [int("rulea"), int("ruleb"), int("arga")],
            popup: int("trulea") == 2 && int("truleb") == 0,
        });
        if !leaf {
            kids.iter().for_each(|k| walk(cfg, k, out));
        }
    }
    let mut out = Vec::new();
    for g in cfg.section("goals").map_or(&[][..], |s| s.get_all("goal")) {
        walk(cfg, g, &mut out);
    }
    out
}

/// What a test sees of a game: the lines of the scenario, the questions and the menu, with the
/// pass they came in.
struct Play<'a, 'b> {
    app: &'a mut App<'b>,
    goals: Vec<Goal>,
    log: Vec<(u64, String)>,
    fired: Vec<String>,
    pass: u64,
    ended: bool,
}

impl<'a, 'b> Play<'a, 'b> {
    fn new(app: &'a mut App<'b>, goals: Vec<Goal>) -> Self {
        Play {
            app,
            goals,
            log: Vec::new(),
            fired: Vec::new(),
            pass: 0,
            ended: false,
        }
    }

    /// One pass: a question that is open gets its answer (OK to a popup, none to the end
    /// dialog), else the player presses the element of the first goal asking for a press that has
    /// not fired.
    fn step(&mut self) {
        self.pass += 1;
        let mut commands = Vec::new();
        let ui = self.app.ui.as_mut().unwrap();
        if ui.modal_open() {
            if !self.ended {
                ui.answer(1);
            }
        } else if let Some(g) = self.goals.iter().find(|g| {
            g.leaf && g.rule[0] == 10 && g.rule[1] != 0 && !self.fired.contains(&g.name)
        }) {
            // The play button shows once the pause button was pressed.
            if g.rule[2] == 1072 {
                commands.push(Command::Press(1071));
            }
            commands.push(Command::Press(g.rule[2]));
        }
        let now = self.pass * STEP;
        self.app.pass(now, &commands);
        for line in self.app.log_lines(self.pass, now) {
            let Some(rest) = line.strip_prefix(&format!("pass {} t={now} ", self.pass)) else {
                continue;
            };
            if let Some(t) = rest.strip_prefix("sim scenario trigger ") {
                self.fired.push(t.split(' ').next().unwrap().to_string());
            }
            if rest.starts_with("menu scenario dialog") {
                self.ended = true;
            }
            let keep = ["sim scenario ", "ui ask", "ui answer", "menu "];
            if keep.iter().any(|k| rest.starts_with(k)) {
                self.log.push((self.pass, rest.to_string()));
            }
        }
    }

    /// Passes until `stop` holds, at most `limit`. Gives whether it did.
    fn until(&mut self, limit: u64, stop: impl Fn(&Self) -> bool) -> bool {
        for _ in 0..limit {
            self.step();
            if stop(self) {
                return true;
            }
        }
        false
    }

    /// The lines that start with `what`, in order.
    fn lines(&self, what: &str) -> Vec<&str> {
        let starts = |l: &&(u64, String)| l.1.starts_with(what);
        (self.log.iter().filter(starts))
            .map(|l| l.1.as_str())
            .collect()
    }
}

/// Starts the tutorial `file` from the main menu, as the next-tutorial answer does.
fn start(app: &mut App, file: &str) {
    let menu = app.menu.as_mut().unwrap();
    menu.start_file(app.renderer.vfs(), file);
    app.requested();
}

const SERIES: [&str; 7] = [
    "scn01a", "scn01", "scn02", "scn19", "scn27", "scn28", "scn28b",
];

fn path(n: &str) -> String {
    format!("scenario/{n}/{n}.scn")
}

fn read(rig: &Rig, file: &str) -> Option<Config> {
    Config::parse(&rig.vfs.read(file).ok()?).ok()
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn explore() {
    let rig = rig("tutorial-prefix");
    for n in SERIES {
        let file = path(n);
        let Some(cfg) = read(&rig, &file) else {
            eprintln!("skipped {n}: the install lacks it");
            continue;
        };
        let mut app = rig.app(n);
        start(&mut app, &file);
        let mut play = Play::new(&mut app, goals(&cfg));
        // 60 scenario passes, 90 s of the game.
        play.until(60, |_| false);
        println!(
            "{n}: fired {} popups {}",
            play.fired.len(),
            play.lines("ui ask").len()
        );
        let ui = play.app.ui.as_mut().unwrap();
        let off = [1502, 4107].map(|id| ui.tree.find(id).map(|i| ui.tree.elements[i].state & DISABLED != 0));
        println!("disabled {off:?}");
    }
}
