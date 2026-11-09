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
    /// The goal has a trigger.
    acts: bool,
    /// Its trigger shows a popup: a trigger 2 with a popup of its own (`truleb` 0) or a press that
    /// names one.
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
            acts: int("trulea") != 0,
            popup: int("trulea") == 2
                && match int("truleb") {
                    0 => int("targa") > 0,
                    1 | 2 => int("targb") > 0,
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
    /// player presses the element of the first goal asking for a press that has not fired.
    fn step(&mut self) {
        let mut commands = Vec::new();
        let ui = self.app.ui.as_mut().unwrap();
        if ui.modal_open() {
            if !self.ended {
                ui.answer(1);
            }
        } else if let Some(g) = self
            .goals
            .iter()
            .find(|g| g.leaf && g.rule[0] == 10 && g.rule[1] != 0 && !self.fired.contains(&g.name))
        {
            // The play button shows once the pause button was pressed.
            if g.rule[2] == 1072 {
                commands.push(Command::Press(1071));
            }
            commands.push(Command::Press(g.rule[2]));
        }
        self.tick(STEP, &commands);
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
        // The first tutorial goes on for 60 scenario passes (90 s of the game), the others stop
        // at the first goal that wants something built.
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

/// What a test does to the prepared save of a tutorial: where the player takes over, and what the
/// goals from there on count that no step of the test builds.
struct Plan {
    n: &'static str,
    /// The first goal the player plays; those before it are done in the save.
    from: &'static str,
    /// Goals from there on that are done in the save too: conditions on exhibits and ratings that
    /// no step of the test reaches. Only for files whose goals do not count the goals done.
    skip: &'static [&'static str],
    /// Objects put in the zoo: the name id of the type and how many.
    put: &'static [(i32, usize)],
    /// Staff hired: the class, the type and how many.
    hire: &'static [(&'static str, &'static str, usize)],
    /// Tiles closed with fences, each an exhibit.
    fences: usize,
    /// Animals crated: the name id of the type.
    crated: &'static [i32],
    /// Tank wall heights set: the position of the tank among the tanks, the height.
    heights: &'static [(usize, u32)],
}

const NONE: Plan = Plan {
    n: "",
    from: "",
    skip: &[],
    put: &[],
    hire: &[],
    fences: 0,
    crated: &[],
    heights: &[],
};

const PLANS: [Plan; 7] = [
    Plan {
        n: "scn01a",
        from: "click_buy2",
        put: &[(6050, 1)],
        ..NONE
    },
    Plan {
        n: "scn01",
        from: "click_buy5",
        put: &[(6009, 3)],
        ..NONE
    },
    Plan {
        n: "scn02",
        from: "PLACE_KAPOK_TREE",
        skip: &[
            "LION_SUITABLE",
            "show_new_buttons",
            "unlock_new_items",
            "CLEOPARD_SUITABLE",
        ],
        put: &[(7065, 4)],
        ..NONE
    },
    Plan {
        n: "scn19",
        from: "SHOW_STAFF_2",
        put: &[(8031, 1)],
        hire: &[("keeper", "scient", 2)],
        crated: &[5094],
        ..NONE
    },
    Plan {
        n: "scn27",
        from: "show_building",
        put: &[(8060, 1)],
        ..NONE
    },
    Plan {
        n: "scn28",
        from: "research_text",
        skip: &["RESEARCH"],
        fences: 1,
        ..NONE
    },
    Plan {
        n: "scn28b",
        from: "close_exhibit_info",
        hire: &[("keeper", "keeper", 2)],
        heights: &[(0, 8), (1, 4)],
        ..NONE
    },
];

/// Places an object of the catalogue type `at` on `point` (world units) as a click on a valid
/// place does; a building makes what it gives birth to. Gives the id.
fn place_at(app: &mut App, at: usize, point: (i32, i32)) -> u32 {
    use crate::place::{Outcome, Verdict};
    let cat = app.renderer.catalogue();
    let ty = &cat.types[at];
    let renderer = &app.renderer;
    let elapsed = app.sim.calendar().elapsed_ms();
    let game = app.game.as_mut().unwrap();
    let (point, none) = ((i64::from(point.0), i64::from(point.1)), |_| None);
    let moved = crate::place::moved(&game.world, cat, &game.water, &none, false, at, 0, point);
    let mut p = moved.unwrap();
    (p.verdict, p.price) = (Verdict::Valid, 0.0);
    let colors = renderer.default_colors(ty);
    let shape = |e: &_| renderer.shape(e);
    let (world, money, water) = (&mut game.world, &mut game.money, &game.water);
    let Outcome::Placed(id) = crate::place::place(world, money, ty, &p, water, colors, shape)
    else {
        panic!("{} not placed at {point:?}", ty.name_id);
    };
    crate::building::built(world, ty, id, elapsed);
    if ty.class == "building" {
        let built = crate::timing::file_time(elapsed);
        let rng = &mut app.sim.rng;
        crate::building::birth::births(world, cat, id, rng, built, shape);
    }
    id
}

/// Puts `n` objects of the type with the name id `name_id` on tiles near the middle of the map.
fn put(app: &mut App, name_id: i32, n: usize) {
    let at = (app.renderer.catalogue().types.iter())
        .position(|t| t.name_id == name_id)
        .unwrap();
    let game = app.game.as_ref().unwrap();
    let (w, h) = (game.world.zoo.width as i32, game.world.zoo.height as i32);
    for k in 0..n as i32 {
        place_at(app, at, ((w / 2 + 3 * k) * 64 + 32, (h / 2) * 64 + 32));
    }
}

/// Closes `n` tiles near the middle of the map with fences, each its own exhibit.
fn fence(app: &mut App, n: usize) {
    let cat = app.renderer.catalogue();
    // The chain link fence.
    let at = (cat.types.iter())
        .position(|t| t.name_id == 9301 && t.subtype == b"f")
        .unwrap();
    let game = app.game.as_ref().unwrap();
    let (w, h) = (game.world.zoo.width as i32, game.world.zoo.height as i32);
    for k in 0..n as i32 {
        let (x, y) = ((w / 2 + 3 * k) * 64, (h / 2 + 5) * 64);
        for (dx, dy) in [(32, 6), (58, 32), (32, 58), (6, 32)] {
            let id = place_at(app, at, (x + dx, y + dy));
            let game = app.game.as_ref().unwrap();
            let e = &game.world.zoo.entities[game.world.find(id).unwrap()];
            println!(
                "fence {id} rotation {}",
                e.common.as_ref().unwrap().rotation
            );
        }
    }
}

/// What the plan adds to the zoo of the game just loaded.
fn arrange(app: &mut App, plan: &Plan) {
    for &(id, count) in plan.put {
        put(app, id, count);
    }
    fence(app, plan.fences);
    let cat = app.renderer.catalogue();
    let game = app.game.as_mut().unwrap();
    for &(class, name, count) in plan.hire {
        for k in 0..count as i32 {
            let at = (game.world.zoo.width as i32 / 2 + 2 * k, 3);
            let rng = &mut app.sim.rng;
            let hired =
                crate::sim::drive::add_unit(&mut game.world, cat, rng, class, Some(name), at, None);
            assert!(hired, "{class} {name}");
        }
    }
    for &id in plan.crated {
        let renderer = &app.renderer;
        let made = crate::scenario::boxed(&mut game.world, cat, id, b'm', |e| renderer.shape(e));
        assert!(made.is_some(), "{id}");
    }
    for &(at, height) in plan.heights {
        let tanks: Vec<u32> = (game.world.exhibits().list().iter())
            .filter(|x| x.tank.is_some())
            .map(|x| x.id)
            .collect();
        game.world.set_height(tanks[at], height);
    }
}

/// A save of the tutorial made at test time: its map as the start leaves it, with the goals
/// before the plan's first one, and its `skip` ones, done.
fn prepared(rig: &Rig, plan: &Plan) -> String {
    let file = path(plan.n);
    let layout = goals(&read(rig, &file).unwrap());
    let mut app = rig.app(&format!("prepare-{}", plan.n));
    start(&mut app, &file);
    let mut words = app.sim.scenario().unwrap().states();
    assert_eq!(words.len(), 1 + layout.len(), "{}", plan.n);
    let first = layout.iter().position(|g| g.name == plan.from).unwrap();
    for (i, g) in layout.iter().enumerate() {
        if g.leaf && (i < first || plan.skip.contains(&g.name.as_str())) {
            words[1 + i] = 1;
        }
    }
    let save = rig.user.0.join(format!("{}.zoo", plan.n));
    let save = save.to_str().unwrap();
    app.save_game(save);
    let bytes = std::fs::read(save).unwrap();
    let (mut zoo, money) = crate::zoo::Zoo::parse_money_in(&bytes, &rig.cat).unwrap();
    zoo.set_scenario(&file, &words);
    std::fs::write(save, zoo.write(&money).unwrap()).unwrap();
    save.to_string()
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn each_prepared_tutorial_ends_after_its_last_popup_and_yes_starts_the_next_one() {
    let rig = rig("tutorial-end");
    let full = installed_expansions(&rig.vfs) == 3;
    let next = [
        ("scn01a", "scn01"),
        ("scn01", "scn02"),
        ("scn02", "scn19"),
        ("scn19", "scn27"),
        ("scn27", "scn28"),
        ("scn28", "scn28b"),
    ];
    for plan in &PLANS {
        let n = plan.n;
        let file = path(n);
        let Some(cfg) = read(&rig, &file) else {
            eprintln!("skipped {n}: the install lacks it");
            continue;
        };
        let layout = goals(&cfg);
        let first = layout.iter().position(|g| g.name == plan.from).unwrap();
        // What the player plays, the triggers that run in the game, in the order of the file.
        let played: Vec<&Goal> = (layout[first..].iter())
            .filter(|g| g.leaf && g.acts && !plan.skip.contains(&g.name.as_str()))
            .collect();
        let save = prepared(&rig, plan);
        let mut app = rig.app(&format!("end-{n}"));
        assert!(app.load_saved(&save), "{n}");
        arrange(&mut app, plan);
        let mut play = Play::new(&mut app, layout.clone());
        assert!(play.until(500, |p| p.ended), "{n}: {:?}", play.fired);
        // The triggers of the goals the player plays came in the order of the file.
        let names: Vec<&str> = played.iter().map(|g| g.name.as_str()).collect();
        let ran: Vec<&str> = play.fired.iter().map(String::as_str).collect();
        assert!(ran.ends_with(&names), "{n}: {ran:?} {names:?}");
        // Each popup came once and was answered; none was cancelled.
        let popups = played.iter().filter(|g| g.popup).count();
        assert_eq!(play.lines("ui ask 45100").len(), popups, "{n}");
        assert_eq!(play.lines("ui answer 1").len(), popups, "{n}");
        assert!(play.lines("ui answer 2").is_empty(), "{n}");
        // The end: won, then the dialog with its text, after the last answer.
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
        assert!(won < dialog && last_answer.unwrap() < dialog, "{n}");
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
                let line = play
                    .log
                    .iter()
                    .skip(dialog)
                    .find(|l| l.1.starts_with(&started));
                assert!(
                    line.is_some(),
                    "{n}: {again} {:?}",
                    play.log.iter().skip(dialog).collect::<Vec<_>>()
                );
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
}
