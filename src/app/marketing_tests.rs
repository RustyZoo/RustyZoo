//! The set-up of the marketing at a start and a load, its panel buttons and its file word, on the
//! made-up window of the money tests.

use super::money_tests::{map, with_members};
use super::*;
use crate::fixtures::Install;
use crate::menu::Start;
use crate::money::StartCash;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::Tree;
use crate::vfs::Vfs;

const SIZE: (u32, u32) = (64, 48);

struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            22100 => "$".into(),
            23100 => "%s none".into(),
            23101 => "%s min".into(),
            23103 => "%s max".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The button of the zoo status window (1014, key Z) and the window with the marketing elements.
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/bar.lyt\n[c]\ntype=UILayout\nlayout=ui/status.lyt\n";
    let bar = "[LayoutInfo]\nid=2\nlayer=1\nx=0\ny=0\ndx=40\ndy=10\n\
        [z]\ntype=UIButton\nid=1014\nx=0\ny=0\ndx=5\ndy=5\nstate=2048\naction=3\ntarget=14\n";
    let status = "[LayoutInfo]\nid=14\nlayer=5\nx=0\ny=0\ndx=64\ndy=48\nstate=1\n\
        [less]\ntype=UIButton\nid=4052\nx=0\ny=20\ndx=5\ndy=5\nlayer=1\n\
        [more]\ntype=UIButton\nid=4053\nx=10\ny=20\ndx=5\ndy=5\nlayer=1\n\
        [text]\ntype=UIText\nid=4054\nx=20\ny=20\ndx=30\ndy=5\nlayer=1\n";
    let files = Install::new(
        &format!("app-marketing-ui-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/bar.lyt", bar.as_bytes().to_vec()),
            ("ui/status.lyt", status.as_bytes().to_vec()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&files.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    assert!(ui.show(2));
    ui
}

const LEVELS: &str = "[marketing]\nfunding=none\nfunding=min\nfunding=max\n\
    [none]\nname=23100\n[min]\nname=23101\ncost=200\nbenefit=5\n\
    [max]\nname=23103\ncost=1000\nbenefit=30\n";

/// Scenarios: one with the marketing files, one that names a file that is not there, one without.
fn files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("s/mk.scn", b"[start]\nmarketing=mk.cfg\n".to_vec()),
        ("s/lost.scn", b"[start]\nmarketing=ghost.cfg\n".to_vec()),
        ("s/no.scn", b"[start]\nresearch=none.cfg\n".to_vec()),
        ("mk.cfg", b"[marketing]\nmarketing=mklevels.cfg\n".to_vec()),
        ("mklevels.cfg", LEVELS.as_bytes().to_vec()),
    ]
}

fn with_game(name: &str, body: impl FnOnce(&mut App)) {
    with_members(name, &files(), |app| {
        app.set_interface(interface(name));
        body(app);
    });
}

fn start(scenario: &str) -> Start {
    Start {
        path: "saved.zoo".into(),
        label: "scenario".into(),
        forget_last: false,
        freeform: false,
        cash: StartCash::default(),
        reset: false,
        reset_fences: false,
        scenario: Some(scenario.into()),
    }
}

/// A scenario start over a bare map.
fn begin(app: &mut App, scenario: &str) {
    app.open_game(map(), Money::default()).unwrap();
    app.begin(&start(scenario));
}

/// The level of the game's marketing, or none without a configuration.
fn level(app: &App) -> Option<usize> {
    app.sim.marketing().config().map(|c| c.level)
}

/// A saved game of `scenario` (none for no scenario block) that stores `word`.
fn saved(scenario: Option<&str>, word: Option<u32>) -> Zoo {
    let mut zoo = map();
    if let Some(file) = scenario {
        zoo.set_scenario(file, &[]);
    }
    if let Some(word) = word {
        zoo.set_marketing(word);
    }
    zoo
}

fn load(app: &mut App, zoo: Zoo) {
    app.open_zoo(zoo, Money::default(), true).unwrap();
}

/// The text of the funding element and whether the buttons are disabled.
fn panel(app: &mut App) -> (String, bool, bool) {
    let ui = app.interface().unwrap();
    let off =
        |id| ui.tree.elements[ui.tree.find(id).unwrap()].state & crate::ui::layout::DISABLED != 0;
    let text = String::from_utf8(ui.text(4054).unwrap()).unwrap();
    (text, off(4052), off(4053))
}

#[test]
fn a_scenario_start_makes_the_configuration_at_level_0_and_the_next_start_replaces_it() {
    with_game("start", |app| {
        begin(app, "s/mk.scn");
        let levels = app.sim.marketing().config().unwrap().levels.len();
        assert_eq!((level(app), levels), (Some(0), 3));
        app.sim.marketing_mut().fund(true);
        assert_eq!(level(app), Some(1));
        // Every start begins at level 0 again.
        begin(app, "s/mk.scn");
        assert_eq!(level(app), Some(0));
        // A key that names a missing file, or no key at all: no configuration.
        app.sim.marketing_mut().fund(true);
        begin(app, "s/lost.scn");
        assert_eq!(level(app), None);
        begin(app, "s/mk.scn");
        begin(app, "s/no.scn");
        assert_eq!(level(app), None);
    });
}

#[test]
fn a_bare_map_or_a_load_with_no_scenario_block_keeps_the_game_before_and_takes_the_file_level() {
    with_game("keep", |app| {
        begin(app, "s/mk.scn");
        app.sim.marketing_mut().fund(true);
        // A bare map: the configuration stays; the file stores 0, which is the level it takes.
        app.open_game(saved(None, Some(2)), Money::default())
            .unwrap();
        assert_eq!(level(app), Some(2));
        // A saved game with no scenario block: the same.
        load(app, saved(None, Some(1)));
        assert_eq!(level(app), Some(1));
        // A level out of the list reads as 0.
        load(app, saved(None, Some(3)));
        assert_eq!(level(app), Some(0));
        load(app, saved(None, Some(u32::MAX)));
        assert_eq!(level(app), Some(0));
        // A file with no word (before version 60) leaves the level as it is.
        app.sim.marketing_mut().fund(true);
        load(app, saved(None, None));
        assert_eq!(level(app), Some(1));
        // Without a configuration the word is dropped.
        begin(app, "s/no.scn");
        load(app, saved(None, Some(2)));
        assert_eq!(level(app), None);
    });
}

#[test]
fn a_load_of_a_scenario_makes_the_set_up_again_and_then_takes_the_file_level() {
    with_game("resume", |app| {
        begin(app, "s/no.scn");
        load(app, saved(Some("s/mk.scn"), Some(2)));
        assert_eq!(level(app), Some(2));
        // A word out of the list is 0, and a file with no word leaves the set-up's level 0.
        load(app, saved(Some("s/mk.scn"), Some(7)));
        assert_eq!(level(app), Some(0));
        app.sim.marketing_mut().fund(true);
        load(app, saved(Some("s/mk.scn"), None));
        assert_eq!(level(app), Some(0));
        // A scenario with no marketing drops the configuration, whatever the file stores.
        load(app, saved(Some("s/no.scn"), Some(2)));
        assert_eq!(level(app), None);
    });
}

#[test]
fn opening_a_game_does_not_touch_the_configuration() {
    with_game("life", |app| {
        begin(app, "s/mk.scn");
        app.sim.marketing_mut().fund(true);
        // Opening a game does not touch it: only a set-up does.
        app.open_game(map(), Money::default()).unwrap();
        assert_eq!(level(app), Some(1));
    });
}

#[test]
fn the_buttons_move_the_level_and_the_panel_follows_at_once_even_when_paused() {
    with_game("buttons", |app| {
        begin(app, "s/mk.scn");
        // The window opens with the text of level 0 and the "less" button disabled.
        app.pass(20, &[Command::Char(b'z')]);
        assert!(app.interface().unwrap().shown(14));
        assert_eq!(panel(app), ("$0 none".into(), true, false));
        app.pause_to(true);
        let ui = |app: &mut App, id| app.interface().unwrap().press_button(id);
        assert!(ui(app, 4053));
        app.pass(40, &[]);
        assert_eq!(level(app), Some(1));
        assert_eq!(panel(app), ("$200 min".into(), false, false));
        assert!(ui(app, 4053));
        app.pass(60, &[]);
        assert_eq!(panel(app), ("$1000 max".into(), false, true));
        // At the last level "more" cannot be pressed; "less" goes back down.
        assert!(!ui(app, 4053));
        assert!(ui(app, 4052));
        app.pass(80, &[]);
        assert_eq!((level(app), panel(app).0), (Some(1), "$200 min".into()));
        // The log tells the panel.
        let said: Vec<_> = app
            .log_lines(3, 80)
            .into_iter()
            .filter(|l| l.contains("funding"))
            .collect();
        assert_eq!(said.len(), 1, "{said:?}");
        // Without a configuration the buttons do nothing and the text is left alone.
        begin(app, "s/no.scn");
        app.interface().unwrap().set_text(4054, b"as it was");
        assert!(ui(app, 4053));
        app.pass(100, &[]);
        assert_eq!((level(app), panel(app).0), (None, "as it was".into()));
    });
}

#[test]
fn a_save_writes_the_level_or_0_without_a_configuration() {
    with_game("save", |app| {
        let dir = Install::new("app-marketing-save", &[]);
        let path = dir.0.join("m.zoo");
        let word = |app: &mut App| {
            app.save_game(path.to_str().unwrap());
            Zoo::parse(&std::fs::read(&path).unwrap())
                .unwrap()
                .marketing()
        };
        let good = crate::zoo::tests::file(106, 2, 2, &[], &[]);
        app.open_game(Zoo::parse(&good).unwrap(), Money::default())
            .unwrap();
        app.begin(&start("s/mk.scn"));
        app.sim.marketing_mut().fund(true);
        app.sim.marketing_mut().fund(true);
        assert_eq!(word(app), Some(2));
        app.begin(&start("s/no.scn"));
        assert_eq!(word(app), Some(0));
    });
}
