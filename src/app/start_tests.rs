//! Starting a game from the menu and leaving it again, on a made-up interface and a made-up map.

use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::menu::Menu;
use crate::render::tests::{cache_install, frame_file};
use crate::sprite::tests::Install;
use crate::ui::draw::Strings as UiStrings;
use crate::ui::layout::{HIDDEN, Tree};
use crate::vfs::Vfs;
use crate::zoo::Zoo;

const SIZE: (u32, u32) = (64, 48);

struct Texts;

impl UiStrings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            151 => "Exit?",
            153 => "Leave?",
            318 => "Cannot open",
            _ => "",
        }
        .into()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The main menu (layout 26) with Exit (7116, at the top left) and Continue (7113, at 20, 0), a
/// question (45000, its message 45006 and the buttons Yes, No, OK and Cancel at 0, 10, 20 and 30
/// of its top) and, hidden, the options panel (5) with Main Menu (1503) and Exit Game (1504).
fn interface(install: &Install) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/menu.lyt\n\
        [c]\ntype=UILayout\nlayout=ui/q.lyt\n\
        [d]\ntype=UILayout\nlayout=ui/options.lyt\n";
    let button = "type=UIButton\ndx=10\ndy=10\ncolorasicon=1\n";
    let menu = format!(
        "[LayoutInfo]\nid=26\nlayer=2\nx=0\ny=0\ndx=whole\ndy=whole\nstate=1\n\
         [x]\nid=7116\nx=0\ny=0\n{button}[y]\nid=7113\nx=20\ny=0\n{button}"
    );
    let q = format!(
        "[LayoutInfo]\nid=45000\nlayer=7\nx=0\ny=0\ndx=40\ndy=10\nstate=1\n\
         [m]\ntype=UIText\nid=45006\nx=0\ny=0\ndx=40\ndy=10\n\
         [a]\nid=45010\nx=0\ny=0\ntarget=6\naction=5\n{button}\
         [b]\nid=45011\nx=10\ny=0\ntarget=7\naction=5\n{button}\
         [c]\nid=45012\nx=20\ny=0\ntarget=1\naction=5\n{button}\
         [d]\nid=45013\nx=30\ny=0\ntarget=2\naction=5\n{button}"
    );
    let options = format!(
        "[LayoutInfo]\nid=5\nlayer=3\nx=40\ny=20\ndx=20\ndy=20\nstate=1\n\
         [a]\nid=1503\nx=0\ny=0\n{button}[b]\nid=1504\nx=10\ny=10\n{button}"
    );
    let made = Install::new(
        &format!(
            "start-ui-{}",
            install.0.file_name().unwrap().to_string_lossy()
        ),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/menu.lyt", menu.into_bytes()),
            ("ui/q.lyt", q.into_bytes()),
            ("ui/options.lyt", options.into_bytes()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&made.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    Interface::new(tree, Box::new(Texts))
}

/// The bytes of a map of 2 by 2 tiles.
fn map() -> Vec<u8> {
    crate::zoo::tests::file(106, 2, 2, &[], &[])
}

/// Runs `body` on the main menu over a folder that has `saved.zoo` (a good map), `bad.zoo` (not
/// a map), and the settings `ini`.
fn with_menu(name: &str, ini: &str, body: impl FnOnce(&mut App, &Install)) {
    let files = Install::new(&format!("start-{name}"), &[]);
    std::fs::write(files.0.join("saved.zoo"), map()).unwrap();
    std::fs::write(files.0.join("bad.zoo"), b"not a map").unwrap();
    let ui = interface(&files);
    let ini = Config::parse(ini.as_bytes()).unwrap();
    let menu = Menu::new(ini.clone(), &files.0, Some(files.0.join("user")), None, 0);
    let game = cache_install(&format!("start-game-{name}"), frame_file());
    let vfs = Vfs::open(&game.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut app = App::menu(
        MapRenderer::new(&vfs, &catalogue),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
        ui,
        menu,
        Music::from_config(&ini),
    )
    .unwrap();
    body(&mut app, &files);
}

fn click(x: i32, y: i32) -> [Command; 3] {
    [
        Command::Mouse(Mouse::Move(x, y)),
        Command::Mouse(Mouse::Down),
        Command::Mouse(Mouse::Up),
    ]
}

/// The log lines of a pass that the menu and the game wrote, without the lead.
fn said(app: &App, pass: u64, now: u64) -> Vec<String> {
    let lead = format!("pass {pass} t={now} ");
    app.log_lines(pass, now)
        .iter()
        .filter_map(|l| l.strip_prefix(&lead))
        .filter(|l| l.starts_with("menu "))
        .map(String::from)
        .collect()
}

fn shown(app: &mut App, id: i32) -> bool {
    let ui = app.interface().unwrap();
    ui.tree
        .find(id)
        .is_some_and(|i| ui.tree.elements[i].state & HIDDEN == 0)
}

const CONTINUE: [Command; 3] = [
    Command::Mouse(Mouse::Move(25, 5)),
    Command::Mouse(Mouse::Down),
    Command::Mouse(Mouse::Up),
];

#[test]
fn continue_opens_the_file_hides_the_menu_fades_the_music_and_runs_with_the_capped_step() {
    with_menu("continue", "[user]\nlastfile=Saved.ZOO\n", |app, _| {
        app.pass(0, &[]);
        assert!(app.camera().is_none());
        assert!(app.pass(66, &CONTINUE).is_some());
        let lines = said(app, 1, 66);
        assert_eq!(
            lines,
            [
                "menu continue Saved.ZOO",
                "menu game start continue Saved.ZOO"
            ]
        );
        assert!(app.camera().is_some() && !app.paused() && !app.quit());
        assert!(!shown(app, 26));
        // The first step is the cap, whatever the clock says; then the ordinary ones.
        let log = app.log_lines(1, 66);
        assert!(log.iter().any(|l| l.contains(" step 200 ")), "{log:?}");
        app.pass(133, &[]);
        assert!(
            app.log_lines(2, 133)
                .iter()
                .any(|l| l.contains(" step 67 "))
        );
        // The music waits five updates, then fades.
        for pass in 3..9 {
            app.pass(pass * 66, &[]);
        }
        assert!(app.music().is_some_and(|(m, _)| !m.playing()));
    });
}

/// Starts the game of `Continue` and runs a few passes; returns the clock.
fn started(app: &mut App) -> u64 {
    app.pass(0, &[]);
    app.pass(66, &CONTINUE);
    assert!(app.camera().is_some());
    66
}

fn key(key: Key) -> [Command; 1] {
    [Command::Key(key, false, false)]
}

#[test]
fn escape_asks_and_no_stops_the_game_for_the_menu_with_continue_checked_and_the_music_again() {
    with_menu("escape", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        // The music has begun to fade: stopping the game starts it again from the beginning.
        for _ in 0..9 {
            now += 66;
            app.pass(now, &[]);
        }
        assert!(app.music().is_some_and(|(m, _)| !m.playing()));
        now += 66;
        app.pass(now, &key(Key::Escape));
        assert_eq!(said(app, 11, now), ["menu game ask leave"]);
        assert!(app.paused() && app.camera().is_some());
        assert!(app.interface().unwrap().modal_open());
        // Cancel, and Yes (which cannot save yet), leave the game as it is.
        now += 66;
        app.pass(now, &key(Key::Escape));
        assert!(app.camera().is_some() && !app.paused());
        now += 66;
        app.pass(now, &key(Key::Escape));
        now += 66;
        app.pass(now, &key(Key::Y));
        assert_eq!(said(app, 14, now), ["menu game save unavailable"]);
        assert!(app.camera().is_some() && !app.paused());
        // No stops it: the menu is back, Continue is checked again, the music restarts.
        now += 66;
        app.pass(now, &key(Key::Escape));
        now += 66;
        app.pass(now, &key(Key::N));
        assert_eq!(
            said(app, 16, now),
            [
                "menu game stop",
                "menu continue saved.zoo",
                "menu music restart"
            ]
            .map(|l| l.replace("continue saved.zoo", "continue enabled saved.zoo"))
        );
        assert!(app.camera().is_none() && app.calendar().is_none() && !app.quit());
        assert!(shown(app, 26) && !app.paused());
        assert!(app.music().is_some_and(|(m, _)| m.playing()) && app.music_restarted());
        app.pass(now + 66, &[]);
        assert!(!app.music_restarted(), "once");
        // And a game can be started again from the menu.
        app.pass(now + 132, &CONTINUE);
        assert!(app.camera().is_some() && !shown(app, 26));
    });
}

#[test]
fn closing_the_window_ends_the_menu_at_once_and_asks_in_a_game() {
    with_menu("close", "[user]\nlastfile=saved.zoo\n", |app, _| {
        app.pass(0, &[Command::Close]);
        assert!(app.quit(), "the menu ends at once");
    });
    with_menu("close-game", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        now += 66;
        app.pass(now, &[Command::Close]);
        assert_eq!(said(app, 2, now), ["menu game ask exit"]);
        assert!(!app.quit() && app.paused());
        // A second close answers the question with Cancel and asks again.
        now += 66;
        app.pass(now, &[Command::Close]);
        assert_eq!(said(app, 3, now), ["menu game ask exit"]);
        assert!(!app.quit() && app.interface().unwrap().modal_open());
        // Cancel stays, No ends, and a quit command ends at once.
        now += 66;
        app.pass(now, &key(Key::Escape));
        assert!(!app.quit() && !app.interface().unwrap().modal_open());
        now += 66;
        app.pass(now, &[Command::Close]);
        now += 66;
        app.pass(now, &key(Key::N));
        assert!(app.quit());
        assert!(said(app, 6, now).contains(&"menu game exit".to_string()));
    });
    with_menu("quit", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let now = started(app);
        app.pass(now + 66, &[Command::Quit]);
        assert!(app.quit(), "a script's quit is not a question");
    });
}

#[test]
fn the_options_panel_buttons_ask_the_same_questions() {
    with_menu("options", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        // The Main Menu button (1503, at 40, 20 of the window) and Exit Game (1504, at 50, 30).
        app.interface().unwrap().show(5);
        now += 66;
        app.pass(now, &click(42, 22));
        assert_eq!(said(app, 2, now), ["menu game ask leave"]);
        now += 66;
        app.pass(now, &key(Key::N));
        assert!(
            app.camera().is_none() && !shown(app, 5),
            "the panel is hidden"
        );
        now += 66;
        app.pass(now, &CONTINUE);
        app.interface().unwrap().show(5);
        now += 66;
        app.pass(now, &click(52, 32));
        assert_eq!(said(app, 5, now), ["menu game ask exit"]);
        now += 66;
        app.pass(now, &key(Key::N));
        assert!(app.quit());
    });
}

#[test]
fn escape_closes_an_open_panel_before_it_asks_and_does_nothing_in_the_menu() {
    with_menu("panel", "[user]\nlastfile=saved.zoo\n", |app, _| {
        app.pass(0, &key(Key::Escape));
        assert!(
            !app.interface().unwrap().modal_open(),
            "no game, no question"
        );
        let mut now = started(app);
        app.interface().unwrap().show(5);
        now += 66;
        app.pass(now, &key(Key::Escape));
        assert!(!shown(app, 5) && !app.interface().unwrap().modal_open());
        now += 66;
        app.pass(now, &key(Key::Escape));
        assert!(app.interface().unwrap().modal_open());
    });
}

#[test]
fn a_file_that_will_not_load_is_a_message_and_the_menu_stays() {
    with_menu("bad", "[user]\nlastfile=bad.zoo\n", |app, _| {
        app.pass(0, &[]);
        app.pass(66, &CONTINUE);
        assert_eq!(
            said(app, 1, 66),
            [
                "menu continue bad.zoo",
                "menu message bad.zoo: not a saved game"
            ]
        );
        assert!(app.camera().is_none() && shown(app, 26) && !app.quit());
        assert!(
            app.interface().unwrap().modal_open(),
            "the message is a box"
        );
        // OK closes it, and a good file can still be started afterwards.
        app.pass(132, &key(Key::Enter));
        assert!(!app.interface().unwrap().modal_open());
    });
    with_menu("gone", "[user]\nlastfile=saved.zoo\n", |app, files| {
        app.pass(0, &[]);
        std::fs::remove_file(files.0.join("saved.zoo")).unwrap();
        app.pass(66, &CONTINUE);
        assert_eq!(
            said(app, 1, 66),
            ["menu continue saved.zoo", "menu message Cannot open"]
        );
        assert!(app.camera().is_none() && shown(app, 26));
    });
}

#[test]
fn a_run_without_a_menu_leaves_by_ending() {
    with_menu("nomenu", "", |app, _| {
        // `run <map>`: a game and its interface, no menu behind it.
        let game = Zoo::parse(&map()).unwrap();
        app.menu = None;
        app.open_game(game).unwrap();
        app.pass(0, &[]);
        app.pass(66, &key(Key::Escape));
        assert!(app.interface().unwrap().modal_open());
        app.pass(132, &key(Key::N));
        assert!(app.quit(), "nowhere to go back to");
    });
}

#[test]
fn a_game_without_an_interface_has_no_question_to_ask() {
    let game = cache_install("start-bare", frame_file());
    let vfs = Vfs::open(&game.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        Zoo::parse(&map()).unwrap(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.pass(0, &key(Key::Escape));
    assert!(!app.quit() && app.camera().is_some(), "Escape asks nothing");
    app.pass(66, &[Command::Close]);
    assert!(app.quit(), "closing ends it");
}
