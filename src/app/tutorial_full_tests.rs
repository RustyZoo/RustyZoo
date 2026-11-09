//! The seven shipped tutorials played in the real interface of the full game (scenarios 10.8):
//! the first passes with their locks, and each one from its first popup to the end dialog and the
//! next tutorial by a script that presses, drags, places, adopts and hires as the goals ask
//! (`bot`), with no goal preset. Ignored without `ZT_DATA_DIR`.

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

mod bot;
mod freeform;

/// The seed of every game of the test.
const SEED: u32 = 1;

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
        app.set_seed(SEED);
        app.pass(0, &[]);
        app
    }
}

/// A leaf or a group of a scenario file, in the order the state words are saved (5.3).
#[derive(Debug, Clone)]
struct Goal {
    name: String,
    leaf: bool,
    /// `rulea`, `ruleb`, `arga`, `argb`, `value`.
    rule: [i32; 5],
    /// The goal has a trigger.
    acts: bool,
    /// Its trigger shows a popup: a trigger 2 with a popup of its own (`truleb` 0) or a press that
    /// names one, a trigger 4 that makes a type available and names one.
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
            rule: [
                int("rulea"),
                int("ruleb"),
                int("arga"),
                int("argb"),
                int("value"),
            ],
            acts: int("trulea") != 0,
            popup: match (int("trulea"), int("truleb")) {
                (2, 0) => int("targa") > 0,
                (2, 1 | 2) | (4, 4) => int("targb") > 0,
                _ => false,
            },
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

/// A game being played: the lines of the scenario, the questions and the menu, with the pass they
/// came in.
struct Play<'a, 'b> {
    app: &'a mut App<'b>,
    goals: Vec<Goal>,
    log: Vec<(u64, String)>,
    /// The goals whose trigger ran, in order.
    fired: Vec<String>,
    pass: u64,
    now: u64,
    ended: bool,
    /// The exhibits whose ground the player painted.
    painted: Vec<u32>,
    /// The last plant or rock put in an exhibit, with the rating before it, and those that
    /// lowered a rating.
    last: Option<(i32, usize)>,
    bad: Vec<usize>,
    /// Commands that go in the pass of the next click on the map.
    after: Vec<Command>,
}

impl<'a, 'b> Play<'a, 'b> {
    fn new(app: &'a mut App<'b>, goals: Vec<Goal>) -> Self {
        Play {
            app,
            goals,
            log: Vec::new(),
            fired: Vec::new(),
            pass: 0,
            now: 0,
            ended: false,
            painted: Vec::new(),
            last: None,
            bad: Vec::new(),
            after: Vec::new(),
        }
    }

    /// One pass `ms` later with these commands; its lines are kept.
    fn tick(&mut self, ms: u64, commands: &[Command]) {
        (self.pass, self.now) = (self.pass + 1, self.now + ms);
        self.app.pass(self.now, commands);
        let lead = format!("pass {} t={} ", self.pass, self.now);
        for line in self.app.log_lines(self.pass, self.now) {
            let Some(rest) = line.strip_prefix(&lead) else {
                continue;
            };
            if let Some(t) = rest.strip_prefix("sim scenario trigger ") {
                self.fired.push(t.split(' ').next().unwrap().to_string());
            }
            if rest.starts_with("menu scenario dialog") {
                self.ended = true;
            }
            let keep = ["sim scenario ", "ui ask", "ui answer", "ui press", "menu "];
            if keep.iter().any(|k| rest.starts_with(k)) {
                self.log.push((self.pass, rest.to_string()));
            }
        }
    }

    /// One pass of a step: a popup that is open gets its answer (OK), the end dialog none, else the
    /// player does what the first goal that is not done asks.
    fn step(&mut self) {
        if self.app.ui.as_ref().unwrap().modal_open() {
            // Enter answers Yes where the question has it, else OK.
            let enter = [Command::Key(Key::Enter, false, false)];
            self.tick(STEP, if self.ended { &[] } else { &enter });
            return;
        }
        if let Some(goal) = self.next_goal() {
            self.act(&goal);
        }
        self.tick(STEP, &[]);
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

    fn disabled(&mut self, id: i32) -> bool {
        let ui = self.app.ui.as_mut().unwrap();
        ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED != 0
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

/// The ids the scenario `did` ("disable", "enable") to, in the lines so far.
fn ids_of(play: &Play, did: &str) -> Vec<i32> {
    let lines = play.lines("sim scenario element");
    let ids = lines.into_iter().filter_map(|l| l.strip_suffix(did));
    let id = |l: &str| l.trim_end().rsplit(' ').next()?.parse().ok();
    ids.filter_map(id).collect()
}

/// The ids the start disabled.
fn blocked(play: &Play) -> Vec<i32> {
    ids_of(play, "disable")
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_shipped_tutorials_start_locked_and_advance_with_presses_and_answers() {
    let rig = rig("tutorial-start");
    for n in SERIES {
        let file = path(n);
        let Some(cfg) = read(&rig, &file) else {
            eprintln!("skipped {n}: the install lacks it");
            continue;
        };
        let layout = goals(&cfg);
        let mut app = rig.app(n);
        start(&mut app, &file);
        let mut play = Play::new(&mut app, layout.clone());
        // The first popups are answered. The start left every id of its block disabled, the
        // triggers since enabled some; Ctrl+S saves nothing and the Scenario button does nothing
        // (10.8 point 5).
        play.until(12, |p| {
            p.pass >= 3 && !p.app.ui.as_ref().unwrap().modal_open()
        });
        if n == "scn01a" {
            // Both popups were answered: the hint shows, and a click on it shows the last popup
            // again, then the hint again (10.8 points 1 and 6).
            let ui = play.app.ui.as_mut().unwrap();
            assert!(ui.shown(45400) && !ui.modal_open());
            let (x, y, w, h) = ui.tree.rect(ui.tree.find(45406).unwrap());
            play.tick(100, &super::start_tests::click(x + w / 2, y + h / 2));
            let ui = play.app.ui.as_mut().unwrap();
            assert!(ui.modal_open() && !ui.shown(45400));
            play.step();
            assert!(play.app.ui.as_ref().unwrap().shown(45400));
        }
        let off = blocked(&play);
        let on = ids_of(&play, "enable");
        assert!(off.len() > 20, "{n}: {off:?}");
        let held: Vec<i32> = off.iter().copied().filter(|id| !on.contains(id)).collect();
        let open: Vec<i32> = held
            .iter()
            .copied()
            .filter(|&id| !play.disabled(id))
            .collect();
        assert!(open.is_empty(), "{n}: {open:?}");
        let (a, b) = (play.disabled(1502), play.disabled(4107));
        assert!(a && b, "{n}: {a} {b}");
        play.tick(100, &[]);
        let (dialogs, presses) = (
            play.lines("menu game save").len(),
            play.lines("ui press").len(),
        );
        let keys = [Command::Key(Key::S, true, false), Command::Press(4107)];
        play.tick(100, &keys);
        assert_eq!(play.lines("menu game save").len(), dialogs, "{n}");
        assert_eq!(play.lines("ui press").len(), presses, "{n}");
        // The play goes on for 20 scenario passes (30 s of the game), the first tutorial for 60.
        play.until(if n == "scn01a" { 60 } else { 20 }, |_| false);
        let asks = play.lines("ui ask 45100").len();
        let answers = play.lines("ui answer 1").len();
        assert!(asks >= 2 && answers + 1 >= asks, "{n}: {asks} {answers}");
        assert!(play.lines("ui answer 2").is_empty(), "{n}");
        // Each trigger of a goal runs once, in the order of the file as far as the player gets.
        let mut names: Vec<&str> = play.fired.iter().map(String::as_str).collect();
        names.retain(|f| layout.iter().any(|g| g.name == *f));
        let at: Vec<usize> = (names.iter())
            .map(|f| layout.iter().position(|g| g.name == *f).unwrap())
            .collect();
        assert!(at.windows(2).all(|w| w[0] < w[1]), "{n}: {names:?}");
        assert!(names.len() > 3, "{n}: {names:?}");
        // The zoom, rotate and pause buttons are seen by the goals that ask for them.
        if n == "scn01a" {
            for goal in ["ZOOM_IN", "ROTATE_CCW", "PLAY"] {
                assert!(names.contains(&goal), "{goal}: {names:?}");
            }
        }
        println!("{n}: {} triggers {}", names.len(), asks);
    }
}

/// The tutorial `n` played by script from its first popup to the end dialog, and Yes (or No after
/// the last) answered.
fn played(n: &str) {
    let rig = rig(&format!("tutorial-end-{n}"));
    let full = installed_expansions(&rig.vfs) == 3;
    let next = [
        ("scn01a", "scn01"),
        ("scn01", "scn02"),
        ("scn02", "scn19"),
        ("scn19", "scn27"),
        ("scn27", "scn28"),
        ("scn28", "scn28b"),
    ];
    let file = path(n);
    let Some(cfg) = read(&rig, &file) else {
        eprintln!("skipped {n}: the install lacks it");
        return;
    };
    let layout = goals(&cfg);
    // The triggers that run in the game, in the order of the file, and their popups.
    let played: Vec<&Goal> = layout.iter().filter(|g| g.leaf && g.acts).collect();
    let mut app = rig.app(n);
    start(&mut app, &file);
    let mut play = Play::new(&mut app, layout.clone());
    assert!(play.until(600, |p| p.ended), "{n}: {:?}", play.next_goal());
    // The triggers came in the order of the file (a goal that is not sticky runs again).
    let mut ran: Vec<&str> = Vec::new();
    for f in play.fired.iter().map(String::as_str) {
        if layout.iter().any(|g| g.name == f) && !ran.contains(&f) {
            ran.push(f);
        }
    }
    let names: Vec<&str> = played.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(ran, names, "{n}");
    // Each popup came once and none was cancelled.
    let popups = played.iter().filter(|g| g.popup).count();
    assert_eq!(play.lines("ui ask 45100").len(), popups, "{n}");
    assert!(play.lines("ui answer 2").is_empty(), "{n}");
    // The end: won, then the dialog with its text, after the last popup was answered.
    let at = |what: &str| play.log.iter().position(|l| l.1.starts_with(what));
    let last_answer = play.log.iter().rposition(|l| l.1 == "ui answer 1");
    let (won, dialog) = (
        at("sim scenario end won").unwrap(),
        at("menu scenario dialog"),
    );
    let text = if n == "scn28b" { 16102 } else { 16104 };
    let dialog = dialog.unwrap();
    assert_eq!(
        play.log[dialog].1,
        format!("menu scenario dialog {text}"),
        "{n}"
    );
    // (The lines of one pass come by family: the end is told in the pass of the dialog.)
    assert!(play.log[won].0 <= play.log[dialog].0, "{n}");
    assert!(last_answer.unwrap() < dialog, "{n}");
    // The clean-up gave the elements back and hid the hint (10.8 points 5 and 6).
    play.tick(100, &[]);
    let ids = blocked(&play);
    assert!(ids.len() > 20, "{n}");
    let locked = ids.iter().filter(|&&id| play.disabled(id)).count();
    assert_eq!(locked, 0, "{n}");
    let ui = play.app.ui.as_ref().unwrap();
    assert!(!ui.shown(45400) && ui.modal_open(), "{n}");
    // Yes starts the next tutorial; No after the last stops the game and shows the main menu
    // (10.8 point 3).
    let again = next.iter().find(|x| x.0 == n).map(|x| path(x.1));
    match (full, again) {
        (true, Some(again)) => {
            play.app.ui.as_mut().unwrap().answer(6);
            play.until(3, |p| !p.lines("sim scenario start").is_empty());
            let started = format!("sim scenario start {again} ");
            let line = (play.log.iter().skip(dialog)).find(|l| l.1.starts_with(&started));
            assert!(line.is_some(), "{n}: {again}");
        }
        (true, None) => {
            play.app.ui.as_mut().unwrap().answer(7);
            play.tick(100, &[]);
            assert!(play.app.camera().is_none(), "{n}");
            assert!(play.app.ui.as_ref().unwrap().shown(26), "{n}");
        }
        _ => {}
    }
}

macro_rules! tutorials {
    ($($test:ident $n:literal)*) => {$(
        #[test]
        #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
        fn $test() {
            played($n);
        }
    )*};
}

tutorials! {
    the_first_tutorial_is_played_by_script_to_its_end "scn01a"
    the_second_tutorial_is_played_by_script_to_its_end "scn01"
    the_third_tutorial_is_played_by_script_to_its_end "scn02"
    the_dinosaur_tutorial_is_played_by_script_to_its_end "scn19"
    the_first_marine_tutorial_is_played_by_script_to_its_end "scn27"
    the_second_marine_tutorial_is_played_by_script_to_its_end "scn28"
    the_third_marine_tutorial_is_played_by_script_to_its_end "scn28b"
}
