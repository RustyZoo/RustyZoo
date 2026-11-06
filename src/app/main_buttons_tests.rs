//! The zoom, rotate, pause and play buttons of the game screen and their keys (camera 8.4,
//! ui layout 7.11, input 4.4 and 4.5), on the made-up main panel of the start tests: the buttons
//! 1007 (zoom in), 1023 (out), 1008 (rotate counter-clockwise), 1009 (clockwise), 1071 (pause) and
//! 1072 (play) are 4 pixels wide in a row from x 40, at y 40.

use super::start_tests::*;
use super::*;
use crate::ui::layout::DISABLED;
use crate::zoo::{Common, Zoo};

const ZOOM_IN: i32 = 1007;
const ZOOM_OUT: i32 = 1023;
const PAUSE: i32 = 1071;
const PLAY: i32 = 1072;

/// A click on the button `id` of the row.
fn press(id: i32) -> [Command; 3] {
    let at = [ZOOM_IN, ZOOM_OUT, 1008, 1009, PAUSE, PLAY];
    click(
        42 + 4 * at.iter().position(|&b| b == id).unwrap() as i32,
        42,
    )
}

fn disabled(app: &mut App, id: i32) -> bool {
    let ui = app.interface().unwrap();
    ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED != 0
}

fn game<'a>(app: &'a mut App<'_>) -> &'a mut Game {
    app.game.as_mut().unwrap()
}

/// What a pass told of the interface: its press lines.
fn pressed(app: &mut App, now: u64, commands: &[Command]) -> Vec<String> {
    app.pass(now, commands);
    let lead = format!("pass {} t={now} ui press ", now / 66);
    app.log_lines(now / 66, now)
        .iter()
        .filter_map(|l| l.strip_prefix(&lead))
        .map(String::from)
        .collect()
}

#[test]
fn the_zoom_buttons_zoom_by_2_and_grey_at_the_limits() {
    with_menu("zoom-buttons", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        let mut go = |app: &mut App, id| {
            now += 66;
            pressed(app, now, &press(id))
        };
        // The map of the test is stored zoomed out (-2): the game opened with zoom out greyed.
        assert_eq!(game(app).camera.zoom, -2);
        assert!(!disabled(app, ZOOM_IN) && disabled(app, ZOOM_OUT));
        assert!(go(app, ZOOM_OUT).is_empty());
        assert_eq!(game(app).camera.zoom, -2);
        assert_eq!(go(app, ZOOM_IN), ["1007"]);
        assert_eq!(game(app).camera.zoom, 0);
        assert!(!disabled(app, ZOOM_IN) && !disabled(app, ZOOM_OUT));
        assert_eq!(go(app, ZOOM_IN), ["1007"]);
        assert_eq!(game(app).camera.zoom, 2);
        assert!(disabled(app, ZOOM_IN) && !disabled(app, ZOOM_OUT));
        // A greyed button takes no click: no press, no change.
        assert!(go(app, ZOOM_IN).is_empty());
        assert_eq!(game(app).camera.zoom, 2);
        assert_eq!(go(app, ZOOM_OUT), ["1023"]);
        assert_eq!(go(app, ZOOM_OUT), ["1023"]);
        assert_eq!(game(app).camera.zoom, -2);
        assert!(!disabled(app, ZOOM_IN) && disabled(app, ZOOM_OUT));
    });
}

#[test]
fn the_rotate_buttons_turn_a_quarter_and_are_never_greyed() {
    with_menu(
        "rotate-buttons",
        "[user]\nlastfile=saved.zoo\n",
        |app, _| {
            let mut now = started(app);
            let mut go = |app: &mut App, id| {
                now += 66;
                pressed(app, now, &press(id));
                game(app).camera.rotation
            };
            // The map is stored turned 3 quarters.
            assert_eq!([go(app, 1009), go(app, 1009), go(app, 1009)], [0, 1, 2]);
            assert_eq!([go(app, 1009), go(app, 1008), go(app, 1008)], [3, 2, 1]);
            assert!(!disabled(app, 1008) && !disabled(app, 1009));
        },
    );
}

#[test]
fn the_ctrl_arrows_press_the_buttons_and_a_greyed_one_ignores_its_key() {
    with_menu("zoom-keys", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        let mut go = |app: &mut App, c: Command| {
            now += 66;
            pressed(app, now, &[c])
        };
        // Zoom out is greyed at -2: its key does nothing.
        assert!(go(app, Command::Zoom(false)).is_empty());
        // The other keys press their buttons: the press line shows, and the effect follows.
        assert_eq!(go(app, Command::Zoom(true)), ["1007"]);
        assert_eq!(go(app, Command::Zoom(true)), ["1007"]);
        assert_eq!(game(app).camera.zoom, 2);
        assert!(disabled(app, ZOOM_IN));
        assert!(go(app, Command::Zoom(true)).is_empty());
        assert_eq!(game(app).camera.zoom, 2);
        assert_eq!(go(app, Command::Rotate(true)), ["1009"]);
        assert_eq!(go(app, Command::Rotate(false)), ["1008"]);
        assert_eq!(game(app).camera.rotation, 3);
        assert_eq!(go(app, Command::Zoom(false)), ["1023"]);
        assert_eq!(game(app).camera.zoom, 0);
        assert!(!disabled(app, ZOOM_IN));
    });
}

#[test]
fn the_pause_and_play_buttons_swap_and_the_keys_do_the_same() {
    with_menu("pause-buttons", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        let mut go = |app: &mut App, commands: &[Command]| {
            now += 66;
            let told = pressed(app, now, commands);
            let ui = app.interface().unwrap();
            let visible = (ui.shown(PAUSE), ui.shown(PLAY));
            (told, app.paused(), visible)
        };
        // A game starts running with the pause button shown.
        assert_eq!(go(app, &[]), (vec![], false, (true, false)));
        assert_eq!(
            go(app, &press(PAUSE)),
            (vec!["1071".into()], true, (false, true))
        );
        assert_eq!(
            go(app, &press(PLAY)),
            (vec!["1072".into()], false, (true, false))
        );
        // Space presses the visible one.
        assert_eq!(
            go(app, &[Command::Space]),
            (vec!["1071".into()], true, (false, true))
        );
        assert_eq!(
            go(app, &[Command::Space]),
            (vec!["1072".into()], false, (true, false))
        );
        // The Pause key does the pause itself: no press.
        assert_eq!(go(app, &[Command::Pause]), (vec![], true, (false, true)));
        assert_eq!(go(app, &[Command::Pause]), (vec![], false, (true, false)));
    });
}

#[test]
fn the_paused_message_shows_while_paused_and_goes_with_the_unpause() {
    with_menu("pause-message", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let mut now = started(app);
        // The line after the commands and one more pass (a press acts at the end of its pass).
        let mut go = |app: &mut App, commands: &[Command]| {
            for commands in [commands, &[]] {
                now += 66;
                app.pass(now, commands);
            }
            let ui = app.interface().unwrap();
            ui.text(7001).filter(|_| ui.shown(7001))
        };
        assert_eq!(go(app, &[]), None);
        let said = Some(b"text-400".to_vec());
        assert_eq!(go(app, &press(PAUSE)), said);
        // The string is in the history while it is on the line, and leaves it with the line.
        assert_eq!(app.interface().unwrap().history(), [b"text-400".to_vec()]);
        // It stays for as long as the pause does.
        for _ in 0..200 {
            assert_eq!(go(app, &[]), said);
        }
        // The play button, the Pause key and Space do the same.
        assert_eq!(go(app, &press(PLAY)), None);
        assert!(app.interface().unwrap().history().is_empty());
        assert_eq!(go(app, &[Command::Pause]), said);
        assert_eq!(go(app, &[Command::Pause]), None);
        assert_eq!(go(app, &[Command::Space]), said);
        assert_eq!(go(app, &[Command::Space]), None);
    });
}

#[test]
fn a_bare_game_paused_has_no_message_line() {
    with_menu("pause-bare", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let now = started(app);
        app.bare();
        app.pass(now + 66, &[Command::Pause]);
        app.pass(now + 132, &[]);
        let paused = app.paused();
        let ui = app.interface().unwrap();
        assert!(paused && ui.text(7001).filter(|_| ui.shown(7001)).is_none());
    });
}

#[test]
fn a_new_game_starts_running_with_the_pause_button_shown() {
    with_menu("pause-new", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let now = started(app);
        app.pass(now + 66, &[Command::Pause]);
        assert!(app.paused() && app.interface().unwrap().shown(PLAY));
        app.open_game(Zoo::parse(&map()).unwrap(), Money::default())
            .unwrap();
        assert!(!app.paused());
        let ui = app.interface().unwrap();
        assert!(ui.shown(PAUSE) && !ui.shown(PLAY));
    });
}

#[test]
fn the_zoom_check_follows_the_zoom_of_the_game_and_full_screen_at_800() {
    with_menu("zoom-check", "[user]\nlastfile=saved.zoo\n", |app, _| {
        let now = started(app);
        let check = |app: &mut App, zoom: i32| {
            game(app).camera.zoom = zoom;
            app.zoom_check();
            (disabled(app, ZOOM_OUT), disabled(app, ZOOM_IN))
        };
        // Below -1 only zoom in is offered; from -1 to 1 both; from 2 only zoom out.
        assert_eq!(check(app, -4), (true, false));
        assert_eq!(check(app, -2), (true, false));
        assert_eq!(check(app, -1), (false, false));
        assert_eq!(check(app, 1), (false, false));
        assert_eq!(check(app, 2), (false, true));
        assert_eq!(check(app, 7), (false, true));
        assert_eq!(check(app, i32::MAX), (false, true));
        assert_eq!(check(app, i32::MIN), (true, false));
        // Full screen in a view of 800 pixels: zoom in is not offered at 0 either, and a zoom of
        // 2 or more goes to 0.
        game(app).camera.size = (800, 600);
        game(app).camera.zoom = 2;
        app.pass(now + 66, &[Command::Fullscreen(true)]);
        assert_eq!(game(app).camera.zoom, 0);
        assert!(disabled(app, ZOOM_IN) && !disabled(app, ZOOM_OUT));
        app.pass(now + 132, &[Command::Fullscreen(false)]);
        assert!(!disabled(app, ZOOM_IN));
    });
}

/// A rotation and a zoom of a 75 by 75 map, with an entity of `class` at a tile away from the view
/// centre, no pass in between: when `selected` the entity is the selected one (`Game::selected`),
/// else it is only on the panel's stale selection, as right after a pick-up. The view keeps the
/// entity's tile when selected (and when it left the world, the view centre), else the centre.
fn reference(name: &str, class: &str, selected: bool) {
    with_menu(name, "[user]\nlastfile=saved.zoo\n", |app, _| {
        started(app);
        let zoo = &mut game(app).world.zoo;
        (zoo.width, zoo.height) = (75, 75);
        let tile = crate::zoo::Tile {
            elevation: 0,
            shape: 0,
            terrain: 0,
            edges: 0,
        };
        zoo.tiles = vec![tile; 75 * 75];
        let zoo = game(app).world.zoo.clone();
        let view = crate::zoo::View {
            zoom: 0,
            rotation: 0,
            scroll: (1216, 16),
            brush: 1,
        };
        let before = Camera::open(&view, (1024, 768), game(app).camera.settings);
        let by = |c: &Camera, at, rotate: bool| {
            let mut c = c.clone();
            if rotate {
                c.rotate(&zoo, at, true);
            } else {
                c.zoom(&zoo, at, false);
            }
            c.scroll
        };
        let at = Some((64 * 37 + 5, 64 * 37 + 9));
        for rotate in [true, false] {
            assert_ne!(by(&before, at, rotate), by(&before, None, rotate));
        }
        let g = game(app);
        let common = Common {
            id: 77,
            x: 64 * 37 + 5,
            y: 64 * 37 + 9,
            ..Default::default()
        };
        g.world.zoo.entities.push(Entity {
            class: class.into(),
            type_name: "x".into(),
            subtype: String::new(),
            body_size: None,
            common: Some(common),
        });
        g.guest.selected = (!selected).then_some(77);
        g.selected = selected.then_some(77);
        let want = if selected { at } else { None };
        for rotate in [true, false] {
            game(app).camera = before.clone();
            if rotate {
                app.rotate(true);
            } else {
                app.zoom_pressed(false);
            }
            assert_eq!(game(app).camera.scroll, by(&before, want, rotate));
        }
        // A selected entity that left the world counts for nothing.
        game(app).world.zoo.entities.pop();
        game(app).camera = before.clone();
        app.zoom_pressed(false);
        assert_eq!(game(app).camera.scroll, by(&before, None, false));
    });
}

#[test]
fn a_zoom_or_a_rotation_keeps_the_selected_unit_and_without_one_the_view_centre() {
    reference("zoom-selected", "guests", true);
}

#[test]
fn a_selected_building_is_the_reference_of_a_zoom_or_a_rotation() {
    reference("zoom-building", "building", true);
}

#[test]
fn a_carried_unit_is_not_the_reference_of_a_zoom_or_a_rotation() {
    reference("zoom-carried", "guests", false);
}
