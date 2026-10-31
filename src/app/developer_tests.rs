//! The hidden developer button that Ctrl+D shows and the research button of its panel.

use super::tests::NoTexts;
use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::menu::Start;
use crate::render::tests::{cache_install_with, frame_file, tile_with};
use crate::ui::layout::Tree;
use crate::vfs::Vfs;
use std::sync::Arc;

const SIZE: (u32, u32) = (64, 48);

const ROOT: &str = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
    [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
    forecolor=255\nforecolor=218\nforecolor=90\n\
    [b]\ntype=UIButton\nid=1098\nx=0\ny=0\ndx=8\ndy=8\ncolorasicon=1\nlayer=5\n\
    action=3\ntarget=34\nstate=16\nstate=2048\n\
    [c]\ntype=UILayout\nlayout=ui/panel.lyt\nstate=1\n\
    [d]\ntype=UIText\nid=3402\nx=40\ny=30\ndx=8\ndy=8\n";
const PANEL: &str = "[LayoutInfo]\nid=34\nlayer=7\nx=20\ny=20\ndx=20\ndy=20\nstate=1\n\
    [a]\ntype=UIButton\nid=5013\nx=0\ny=0\ndx=8\ndy=8\ncolorasicon=1\n";

/// The lion and the zebra start available and are the targets of two programs; the gnu starts
/// unavailable and is no program's target.
pub(super) fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for (name, id, start) in [("lion", 5, 1), ("zebra", 6, 1), ("gnu", 7, 0)] {
        let ini = format!("[Global]\nAvailableAtStartup={start}\n");
        cat.push(EntityType {
            class: "animals",
            type_name: name.as_bytes().to_vec(),
            subtype: b"m".to_vec(),
            file: None,
            config: Some(Arc::new(Config::parse(ini.as_bytes()).unwrap())),
            name_id: id,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

fn files(research: bool) -> Vec<(&'static str, Vec<u8>)> {
    let start = if research {
        "[start]\nresearch=r.cfg\n"
    } else {
        "[duration]\nnummonths=-1\n"
    };
    vec![
        ("s/a.scn", start.as_bytes().to_vec()),
        ("ui/root.lyt", ROOT.as_bytes().to_vec()),
        ("ui/panel.lyt", PANEL.as_bytes().to_vec()),
        ("r.cfg", b"[branches]\nbranch=b.cfg\n".to_vec()),
        (
            "b.cfg",
            b"[branch]\nname=1\ncategory=c.cfg\nfunding=none\n[none]\nname=2\n".to_vec(),
        ),
        (
            "c.cfg",
            b"[category]\nname=3\nprogram=p1.cfg\nprogram=p2.cfg\n".to_vec(),
        ),
        (
            "p1.cfg",
            b"[research]\nname=11\ncost=90\ntarget=5\neffect=0\n".to_vec(),
        ),
        (
            "p2.cfg",
            b"[research]\nname=12\ncost=10\ntarget=6\neffect=0\n".to_vec(),
        ),
    ]
}

/// A game of the scenario (with a research tree or not) and the interface above, the developer
/// ini key set or not.
pub(super) fn with_game(
    name: &str,
    research: bool,
    dev_key: bool,
    body: impl FnOnce(&mut App, &Catalogue),
) {
    with_world(name, (research, dev_key), (catalogue(), &[]), body);
}

/// As [`with_game`] with the types `cat` and the entities `(class, type, subtype)` in the map,
/// their ids counting from 1.
pub(super) fn with_world(
    name: &str,
    (research, dev_key): (bool, bool),
    (cat, entities): (Catalogue, &[(&str, &str, &str)]),
    body: impl FnOnce(&mut App, &Catalogue),
) {
    let install = cache_install_with(name, frame_file(), &files(research));
    let vfs = Vfs::open(&install.0).unwrap();
    let mut app = App::empty(
        MapRenderer::new(&vfs, &cat),
        Timers::new(66, 16, 200),
        SIZE,
        Settings::default(),
    )
    .unwrap();
    let mut zoo = tile_with(entities);
    for (i, e) in zoo.entities.iter_mut().enumerate() {
        e.common.as_mut().unwrap().id = i as u32 + 1;
    }
    app.open_game(zoo, Money::default()).unwrap();
    app.begin(&Start {
        path: "scenario.zoo".into(),
        label: "scenario".into(),
        forget_last: true,
        freeform: false,
        cash: Default::default(),
        reset: false,
        reset_fences: false,
        scenario: Some("s/a.scn".into()),
    });
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    app.set_interface(Interface::new(tree, Box::new(NoTexts)));
    app.timers.speed = crate::timing::Speed::new(dev_key);
    app.pass(0, &[]);
    body(&mut app, &cat);
}

fn click(app: &mut App, now: u64, at: (i32, i32)) {
    let (x, y) = at;
    app.pass(
        now,
        &[
            Command::Mouse(Mouse::Move(x, y)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ],
    );
}

fn shown(app: &mut App, id: i32) -> bool {
    app.interface().unwrap().shown(id)
}

pub(super) fn available(app: &App, cat: &Catalogue) -> [bool; 3] {
    let build = &app.game.as_ref().unwrap().build;
    [0, 1, 2].map(|i| build.is_available(cat, i))
}

#[test]
fn ctrl_d_shows_and_hides_the_developer_button_with_the_mode() {
    with_game("dev-button", false, true, |app, _| {
        assert!(!shown(app, research::DEV_BUTTON));
        app.pass(20, &[Command::DevMode]);
        assert!(shown(app, research::DEV_BUTTON));
        app.pass(40, &[Command::DevMode]);
        assert!(!shown(app, research::DEV_BUTTON));
        app.pass(60, &[Command::DevMode]);
        assert!(shown(app, research::DEV_BUTTON));
    });
}

#[test]
fn without_the_ini_key_ctrl_d_leaves_the_button_hidden() {
    with_game("dev-button-off", false, false, |app, _| {
        app.pass(20, &[Command::DevMode]);
        assert!(!shown(app, research::DEV_BUTTON));
        click(app, 40, (2, 2));
        assert!(!shown(app, 34));
    });
}

#[test]
fn the_panel_research_button_completes_the_programs_and_makes_every_type_available() {
    with_game("dev-research", true, true, |app, cat| {
        // The programs' targets start unavailable; the gnu too.
        assert_eq!(available(app, cat), [false, false, false]);
        app.pass(20, &[Command::DevMode]);
        click(app, 40, (2, 2));
        assert!(shown(app, 34));
        click(app, 60, (22, 22));
        assert_eq!(available(app, cat), [true, true, true]);
        let tree = app.sim.research().unwrap();
        let programs = &tree.branches[0].categories[0].programs;
        // Complete for this run only: no progress is set.
        assert!(programs.iter().all(|p| p.progress == 0.0));
    });
}

#[test]
fn the_panel_research_button_without_a_tree_still_makes_every_type_available() {
    with_game("dev-research-none", false, true, |app, cat| {
        assert_eq!(available(app, cat), [true, true, false]);
        app.developer_research();
        assert_eq!(available(app, cat), [true, true, true]);
        app.developer_research();
        assert_eq!(available(app, cat), [true, true, true]);
    });
}
