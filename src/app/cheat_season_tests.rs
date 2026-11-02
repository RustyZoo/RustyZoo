//! The date hooks' availability (cheats 6) on a game with the four seasonal items.

use super::developer_tests::with_world;
use super::*;
use crate::cheat::{HOLIDAY_TREE, JACK_O_LANTERN, SNOWMAN, YELLOW_BRICK};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::timing::Date;
use crate::zoo::Clocks;

const IDS: [i32; 4] = [HOLIDAY_TREE, JACK_O_LANTERN, SNOWMAN, YELLOW_BRICK];

/// The four items, all available at the start.
fn items() -> Catalogue {
    let mut cat = Catalogue::default();
    for id in IDS {
        cat.push(EntityType {
            class: "objects",
            type_name: format!("item{id}").into_bytes(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                Config::parse(b"[Global]\nAvailableAtStartup=1\n").unwrap(),
            )),
            name_id: id,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

fn on(app: &App, cat: &Catalogue) -> [bool; 4] {
    let build = &app.game.as_ref().unwrap().build;
    [0, 1, 2, 3].map(|i| build.is_available(cat, i))
}

fn today(app: &mut App, month: u32, day: u32) {
    let date = Date {
        year: 2002,
        month,
        day,
        weekday: 0,
        hour: 12,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    let clocks = Clocks {
        calendar: date.game_ms(),
        ..Clocks::default()
    };
    app.sim.start_game(0, clocks);
}

fn run(body: impl FnOnce(&mut App, &Catalogue)) {
    with_world("cheat-season", (false, false), (items(), &[]), body);
}

#[test]
fn a_game_opens_with_the_four_items_unavailable() {
    run(|app, cat| assert_eq!(on(app, cat), [false; 4]));
}

#[test]
fn a_guest_created_on_christmas_makes_the_snowman_and_the_tree_available_until_another_day() {
    run(|app, cat| {
        today(app, 12, 25);
        app.guest_created();
        // Order of the ids: tree, jack-o'-lantern, snowman, path.
        assert_eq!(on(app, cat), [true, false, true, false]);
        today(app, 12, 26);
        app.guest_created();
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn a_guest_created_on_the_last_day_of_october_makes_the_jack_o_lantern_available() {
    run(|app, cat| {
        today(app, 10, 31);
        app.guest_created();
        assert_eq!(on(app, cat), [false, true, false, false]);
        today(app, 11, 1);
        app.guest_created();
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn the_yellow_brick_path_follows_the_trio_flag_once_and_a_start_does_not_bring_it_back() {
    run(|app, cat| {
        today(app, 3, 3);
        app.guest_created();
        assert_eq!(on(app, cat), [false; 4]);
        app.game
            .as_mut()
            .unwrap()
            .world
            .units
            .cheat
            .lions_tigers_bears = true;
        app.guest_created();
        assert_eq!(on(app, cat), [false, false, false, true]);
        // A new game takes it away; the hook has spent its one time.
        app.open_game(crate::render::tests::tile_with(&[]), Money::default())
            .unwrap();
        assert_eq!(on(app, cat), [false; 4]);
        app.game
            .as_mut()
            .unwrap()
            .world
            .units
            .cheat
            .lions_tigers_bears = true;
        app.guest_created();
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn the_hooks_do_nothing_without_a_game() {
    run(|app, _| {
        app.game = None;
        app.guest_created();
        app.game_started();
    });
}
