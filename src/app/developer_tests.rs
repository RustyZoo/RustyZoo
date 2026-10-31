//! The hidden developer button that Ctrl+D shows and the research button of its panel.

use super::tests::NoTexts;
use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::menu::Start;
use crate::money::{DONATIONS, Money, NET};
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
    [c]\ntype=UILayout\nlayout=ui/panel.lyt\nstate=1\n";
const PANEL: &str = "[LayoutInfo]\nid=34\nlayer=7\nx=20\ny=20\ndx=200\ndy=20\nstate=1\n\
    [a]\ntype=UIButton\nid=5013\nx=0\ny=0\ndx=8\ndy=8\ncolorasicon=1\n\
    [e]\ntype=UIEditableText\nid=5012\nx=0\ny=10\ndx=180\ndy=8\n";

/// The lion and the zebra start available and are the targets of two programs; the gnu starts
/// unavailable and is no program's target.
fn catalogue() -> Catalogue {
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
fn with_game(name: &str, research: bool, dev_key: bool, body: impl FnOnce(&mut App, &Catalogue)) {
    let install = cache_install_with(name, frame_file(), &files(research));
    let vfs = Vfs::open(&install.0).unwrap();
    let cat = catalogue();
    let mut app = App::empty(
        MapRenderer::new(&vfs, &cat),
        Timers::new(66, 16, 200),
        SIZE,
        Settings::default(),
    )
    .unwrap();
    app.open_game(tile_with(&[]), Money::default()).unwrap();
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

fn available(app: &App, cat: &Catalogue) -> [bool; 3] {
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

/// Clicks the cash field of the panel, then sends `keys` (the text replaces the selection) and
/// Enter, which ends the edit.
fn edit_cash(app: &mut App, now: u64, keys: &[Command]) {
    click(app, now, (22, 32));
    let mut all = keys.to_vec();
    all.push(Command::Key(Key::Enter, false, false));
    app.pass(now + 10, &all);
}

/// The cash, and the donations of the accounts (the net is the same: nothing else is booked).
fn cash_and_donations(app: &App) -> (f32, f32) {
    let m = app.money().unwrap();
    assert_eq!(m.total[NET], m.total[DONATIONS]);
    (m.cash, m.total[DONATIONS])
}

#[test]
fn the_panel_cash_field_sets_the_cash_and_books_the_difference_as_a_donation() {
    with_game("dev-cash", false, true, |app, _| {
        app.set_money(Money {
            cash: 1000.0,
            ..Money::default()
        });
        app.pass(20, &[Command::DevMode]);
        click(app, 40, (2, 2));
        assert!(shown(app, 34));
        let typed = |text: &[u8]| text.iter().map(|&b| Command::Char(b)).collect::<Vec<_>>();
        let backspace = [Command::Key(Key::Backspace, false, false)];
        // Escape takes the edit back to the text the field had (never applied): no change.
        click(app, 50, (22, 32));
        app.pass(
            55,
            &[Command::Char(b'9'), Command::Key(Key::Escape, false, false)],
        );
        assert_eq!(cash_and_donations(app), (1000.0, 0.0));
        // Less cash: a negative donation.
        edit_cash(app, 60, &typed(b"400"));
        assert_eq!(cash_and_donations(app), (400.0, -600.0));
        // Zero and a negative amount are amounts.
        edit_cash(app, 80, &typed(b"0"));
        assert_eq!(cash_and_donations(app), (0.0, -1000.0));
        edit_cash(app, 100, &typed(b"-50"));
        assert_eq!(cash_and_donations(app), (-50.0, -1050.0));
        // A decimal number keeps its fraction.
        edit_cash(app, 120, &typed(b"12.5"));
        assert_eq!(cash_and_donations(app), (12.5, -987.5));
        // Text that is no number reads as 0, as the price fields do; an empty field too.
        edit_cash(app, 140, &typed(b"abc"));
        assert_eq!(cash_and_donations(app), (0.0, -1000.0));
        edit_cash(app, 160, &backspace);
        assert_eq!(cash_and_donations(app), (0.0, -1000.0));
        // A huge amount stays finite and is booked as a donation.
        edit_cash(app, 180, &typed(b"1000000000000"));
        let big = 1_000_000_000_000_f64 as f32;
        assert_eq!(cash_and_donations(app), (big, -1000.0 + big));
    });
}
