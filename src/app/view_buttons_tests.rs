//! The snapshot button and the three hide/show buttons of the main bar (map drawing 15), on the
//! made-up map and catalogue of the bulldozer tests.

use super::bulldozer_tests::{DELETE, ROOT, Session, catalogue, ty, with_game_of};
use super::view_buttons::TEST_FOLDER;
use super::*;
use crate::entity::Catalogue;
use crate::fixtures::TempDir;
use crate::render::Hide;
use crate::ui::layout::SELECTED;

const FOLIAGE: i32 = 1066;
const BUILDINGS: i32 = 1067;
const GUESTS: i32 = 1068;
const SNAPSHOT: i32 = 1099;
const ON_SCREEN: i32 = 9100;
/// The tree, bench, shop and cart of the bulldozer tests, then a palm (foliage) and a hut (a
/// building of the list `structures`).
const PALM: usize = 20;
const HUT: usize = 21;
const SHOP: usize = 2;
const TREE: usize = 7;
const MAN: usize = 14;

fn types() -> Catalogue {
    let mut cat = catalogue();
    let palm = "[Characteristics/Integers]\ncFoliage=1\n";
    cat.push(ty("objects", b"palm", b"", palm, [1, 1, 1]));
    cat.push(ty(
        "building",
        b"hut",
        b"",
        "[Member]\nstructures\n",
        [2, 2, 1],
    ));
    cat
}

/// The main bar's buttons at the bottom of the window: 1066 to 1068 toggles, 1099 plain; and a
/// text the screen shows and the map alone does not.
fn root() -> String {
    let button = |id: i32, x: i32, state: &str| {
        format!(
            "[b{id}]\ntype=UIButton\nid={id}\nx={x}\ny=200\ndx=8\ndy=8\nhelpid={id}\n{state}\
             transparent=0\n"
        )
    };
    format!(
        "{ROOT}{}{}{}{}[t]\ntype=UIText\nid={ON_SCREEN}\nx=0\ny=100\ndx=120\ndy=40\n\
         forecolor=255\nforecolor=255\nforecolor=255\n",
        button(FOLIAGE, 100, "state=2048\n"),
        button(BUILDINGS, 110, "state=2048\n"),
        button(GUESTS, 120, "state=2048\n"),
        button(SNAPSHOT, 130, ""),
    )
}

fn play(name: &str, ini: &str, body: impl FnOnce(&mut Session)) {
    with_game_of(name, ini, (types(), &root(), &[]), |s| {
        let ui = s.app.interface().unwrap();
        for id in [FOLIAGE, BUILDINGS, GUESTS, SNAPSHOT, ON_SCREEN] {
            ui.set_hidden(id, false);
        }
        // What the start of a game does (the tree here is the test's own).
        s.app.view_sync();
        body(s)
    });
}

fn click_on(id: i32) -> [Command; 3] {
    let x = match id {
        FOLIAGE => 100,
        BUILDINGS => 110,
        GUESTS => 120,
        _ => 130,
    };
    [
        Command::Mouse(Mouse::Move(x + 3, 203)),
        Command::Mouse(Mouse::Down),
        Command::Mouse(Mouse::Up),
    ]
}

fn drawn(s: &Session) -> usize {
    s.app.game.as_ref().unwrap().staged.summary.objects_drawn
}

/// Whether the button is down, and the string its tooltip shows.
fn state(s: &mut Session, id: i32) -> (bool, i32) {
    let ui = s.app.interface().unwrap();
    let e = &ui.tree.elements[ui.tree.find(id).unwrap()];
    (e.state & SELECTED != 0, e.help_id[0])
}

/// A palm, a tree, a hut, a shop and a man on tiles of their own.
fn stock(s: &mut Session) {
    for (i, ty) in [PALM, TREE, HUT, SHOP, MAN].into_iter().enumerate() {
        s.add(ty, (18 + 3 * i as i32) * 64 + 32, 20 * 64 + 32, 0);
    }
    s.pass(&[]);
}

#[test]
fn the_foliage_button_hides_and_shows_every_foliage_object_and_swaps_its_tooltip() {
    play("view-foliage", "", |s| {
        stock(s);
        assert_eq!(drawn(s), 5);
        assert_eq!(state(s, FOLIAGE), (true, FOLIAGE));
        s.pass(&click_on(FOLIAGE));
        assert_eq!(drawn(s), 4, "the palm alone is gone");
        assert_eq!(state(s, FOLIAGE), (false, 1065), "up, \"show foliage\"");
        assert_eq!(state(s, BUILDINGS), (true, BUILDINGS));
        s.pass(&click_on(FOLIAGE));
        assert_eq!(drawn(s), 5);
        assert_eq!(state(s, FOLIAGE), (true, FOLIAGE));
        // Foliage added while hidden is hidden too; the world keeps all of it.
        s.pass(&click_on(FOLIAGE));
        s.add(PALM, 30 * 64 + 32, 22 * 64 + 32, 0);
        s.pass(&[]);
        assert_eq!((drawn(s), s.count()), (4, 6));
    });
}

#[test]
fn the_building_and_guest_buttons_hide_their_kind_and_leave_the_rest_drawn() {
    play("view-others", "", |s| {
        stock(s);
        s.pass(&click_on(BUILDINGS));
        assert_eq!(drawn(s), 4, "the hut alone, not the shop");
        assert_eq!(state(s, BUILDINGS), (false, 1069));
        // The guest stays drawn while the buildings are hidden.
        s.pass(&click_on(GUESTS));
        assert_eq!(drawn(s), 3);
        assert_eq!(state(s, GUESTS), (false, 1070));
        assert_eq!(state(s, FOLIAGE), (true, FOLIAGE));
        s.pass(&click_on(BUILDINGS));
        assert_eq!(drawn(s), 4);
        assert_eq!(state(s, BUILDINGS), (true, BUILDINGS));
        s.pass(&click_on(GUESTS));
        assert_eq!(drawn(s), 5);
        assert_eq!(state(s, GUESTS), (true, GUESTS));
    });
}

#[test]
fn the_keys_press_the_buttons_and_ctrl_b_is_another_key_in_the_developer_mode() {
    play("view-keys", "", |s| {
        stock(s);
        let ctrl = |key| [Command::Key(key, true, false)];
        s.pass(&ctrl(Key::F));
        assert_eq!((drawn(s), state(s, FOLIAGE).0), (4, false));
        s.pass(&ctrl(Key::B));
        assert_eq!((drawn(s), state(s, BUILDINGS).0), (3, false));
        s.pass(&ctrl(Key::V));
        assert_eq!((drawn(s), state(s, GUESTS).0), (2, false));
        s.pass(&ctrl(Key::F));
        s.pass(&ctrl(Key::B));
        s.pass(&ctrl(Key::V));
        assert_eq!(drawn(s), 5);
        // A letter without Ctrl does nothing.
        s.pass(&[Command::Key(Key::F, false, false)]);
        assert_eq!(drawn(s), 5);
        s.pass(&ctrl(Key::B));
        assert_eq!(drawn(s), 4);
        s.pass(&ctrl(Key::B));
        s.app.timers.speed = crate::timing::Speed::new(true);
        s.pass(&[Command::DevMode]);
        s.pass(&ctrl(Key::B));
        assert_eq!(
            (drawn(s), state(s, BUILDINGS).0),
            (5, true),
            "no press in the developer mode"
        );
        s.pass(&ctrl(Key::F));
        assert_eq!(drawn(s), 4, "Ctrl+F still presses");
    });
}

/// The objects an area drag from tile 17 to tile 28 of the row would remove, with the buttons
/// `pressed` clicked first.
fn dragged(name: &str, pressed: &[i32]) -> usize {
    let mut objects = 0;
    play(name, "", |s| {
        stock(s);
        for &id in pressed {
            s.pass(&click_on(id));
        }
        s.pass(&[DELETE]);
        s.pass(&[
            s.at(17, 20, 32, 32),
            Command::Mouse(Mouse::Down),
            s.at(18, 20, 32, 32),
        ]);
        let told = s.pass(&[s.at(28, 20, 32, 32)]);
        let line = told
            .iter()
            .find(|l| l.starts_with("tool bulldoze area"))
            .unwrap();
        objects = line
            .split(" objects ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
    });
    objects
}

#[test]
fn the_bulldozer_skips_hidden_foliage_and_hidden_buildings_and_cannot_pick_them() {
    // The palm, the tree, the hut and the shop; a guest is no object of the area.
    assert_eq!(dragged("view-dozer-all", &[]), 4);
    assert_eq!(dragged("view-dozer-foliage", &[FOLIAGE]), 3);
    assert_eq!(dragged("view-dozer-buildings", &[BUILDINGS]), 3);
    assert_eq!(dragged("view-dozer-both", &[FOLIAGE, BUILDINGS]), 2);
    assert_eq!(
        dragged("view-dozer-guests", &[GUESTS]),
        4,
        "guests hide nothing of it"
    );
    play("view-dozer-click", "", |s| {
        stock(s);
        s.pass(&[DELETE]);
        let over = s.pass(&[s.at(18, 20, 32, 32)]);
        assert!(over.iter().any(|l| l.contains("objects/palm")), "{over:?}");
        s.pass(&click_on(FOLIAGE));
        s.pass(&[s.at(10, 10, 32, 32)]);
        let over = s.pass(&[s.at(18, 20, 32, 32)]);
        assert!(over.iter().all(|l| !l.contains("objects/palm")), "{over:?}");
        // The tree beside it is picked as before.
        let over = s.pass(&[s.at(21, 20, 32, 32)]);
        assert!(over.iter().any(|l| l.contains("objects/tree")), "{over:?}");
    });
}

fn snap_text(s: &mut Session) -> String {
    for _ in 0..8 {
        s.pass(&[]);
    }
    let text = s.app.interface().unwrap().text(7001).unwrap();
    text.iter().map(|&b| char::from(b)).collect()
}

fn read(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn the_snapshot_button_and_ctrl_p_write_the_next_numbered_picture_and_say_so() {
    let dir = TempDir::new("view-snap");
    let shots = dir.0.join("shots");
    TEST_FOLDER.with(|f| *f.borrow_mut() = Some(shots.clone()));
    // A quality that is out of range, negative or missing still writes a JPEG; useJPEG 0 a BMP.
    for (i, ini) in [
        "",
        "[UI]\nJPEGQuality=0\n",
        "[UI]\nJPEGQuality=-7\n",
        "[UI]\nJPEGQuality=99999\n",
        "[UI]\nuseJPEG=7\nJPEGQuality=x\n",
    ]
    .into_iter()
    .enumerate()
    {
        play(&format!("view-snap-jpeg-{i}"), ini, |s| {
            stock(s);
            s.pass(&click_on(SNAPSHOT));
            let name = format!("Zoo{:03}.jpg", i + 1);
            let jpeg = read(&shots.join(&name));
            assert_eq!(&jpeg[..2], &[0xff, 0xd8], "{ini:?}");
            assert_eq!(
                snap_text(s),
                format!("saved in {}", shots.join(&name).display())
            );
        });
    }
    play("view-snap-bmp", "[UI]\nuseJPEG=0\n", |s| {
        stock(s);
        s.pass(&click_on(SNAPSHOT));
        let plain = read(&shots.join("Zoo001.bmp"));
        assert_eq!(&plain[..2], b"BM");
        // Ctrl+P is the same press; the numbers go on.
        s.pass(&[Command::Key(Key::P, true, false)]);
        assert_eq!(
            read(&shots.join("Zoo002.bmp")),
            plain,
            "the map alone, again"
        );
        // Shift held: the screen as it is, with what the interface draws.
        s.app.interface().unwrap().set_text(ON_SCREEN, b"HELLO");
        s.pass(&[Command::Shift(true)]);
        s.pass(&click_on(SNAPSHOT));
        assert_ne!(read(&shots.join("Zoo003.bmp")), plain);
        s.pass(&[Command::Shift(false)]);
        s.pass(&click_on(SNAPSHOT));
        assert_eq!(read(&shots.join("Zoo004.bmp")), plain, "Shift released");
        s.pass(&[Command::Key(Key::P, true, true)]);
        assert_ne!(read(&shots.join("Zoo005.bmp")), plain, "Ctrl+Shift+P");
        // A disabled button takes no press.
        s.app.interface().unwrap().disable(SNAPSHOT, true);
        s.pass(&click_on(SNAPSHOT));
        s.pass(&[Command::Key(Key::P, true, false)]);
        assert!(!shots.join("Zoo006.bmp").exists());
    });
}

#[test]
fn a_snapshot_that_cannot_be_written_says_so_and_the_game_goes_on() {
    let dir = TempDir::new("view-snap-fail");
    std::fs::write(dir.0.join("blocker"), b"x").unwrap();
    TEST_FOLDER.with(|f| *f.borrow_mut() = Some(dir.0.join("blocker").join("shots")));
    play("view-snap-fail", "", |s| {
        s.pass(&click_on(SNAPSHOT));
        assert_eq!(snap_text(s), "not saved");
        s.pass(&[]);
    });
}

#[test]
fn a_game_that_opens_starts_with_everything_shown_and_the_buttons_selected() {
    use super::start_tests::{CONTINUE, click, key, started, with_menu};
    with_menu("view-reopen", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        let mut go = |app: &mut App, commands: &[Command]| {
            now += 66;
            app.pass(now, commands);
        };
        let state = |app: &mut App, id| {
            let ui = app.interface().unwrap();
            let e = &ui.tree.elements[ui.tree.find(id).unwrap()];
            (e.state & SELECTED != 0, e.help_id[0])
        };
        go(app, &click(42, 46));
        go(app, &click(50, 46));
        assert_eq!(state(app, FOLIAGE), (false, 1065));
        assert_eq!(state(app, GUESTS), (false, 1070));
        assert_eq!(state(app, BUILDINGS), (true, BUILDINGS));
        // Leave the game for the menu, and open it again.
        go(app, &key(Key::Escape));
        go(app, &key(Key::N));
        assert!(app.camera().is_none());
        go(app, &CONTINUE);
        assert!(app.camera().is_some());
        assert_eq!(app.game.as_ref().unwrap().view.hide, Hide::default());
        for id in [FOLIAGE, BUILDINGS, GUESTS] {
            assert_eq!(state(app, id), (true, id));
        }
    });
}
