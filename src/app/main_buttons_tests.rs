//! The zoom, rotate, pause and play buttons of the game screen and their keys (camera 8.4,
//! ui layout 7.11, input 4.4 and 4.5), on the made-up main panel of the start tests: the buttons
//! 1007 (zoom in), 1023 (out), 1008 (rotate counter-clockwise), 1009 (clockwise), 1071 (pause) and
//! 1072 (play) are 4 pixels wide in a row from x 40, at y 40.

use super::start_tests::*;
use super::*;
use crate::ui::layout::DISABLED;

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
