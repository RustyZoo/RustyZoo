//! The date hooks' availability (cheats 6) on a game with the four seasonal items.

use super::developer_tests::with_world;
use super::*;
use crate::cheat::{HOLIDAY_TREE, JACK_O_LANTERN, SNOWMAN, YELLOW_BRICK};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::render::tests::{cache_install_with, frame_file, tile_with};
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

/// A scenario start of `s/a.scn`, which runs the start hook at the end of its set-up.
fn restart(app: &mut App) {
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
}

/// A game after a scenario start, which runs the start hook.
fn run(body: impl FnOnce(&mut App, &Catalogue)) {
    with_world(
        "cheat-season",
        (false, false),
        (items(), &[]),
        |app, cat| {
            restart(app);
            body(app, cat);
        },
    );
}

/// A map with the scenario block naming `scenario`, or an empty block.
fn zoo(scenario: Option<&str>) -> Zoo {
    Zoo::parse(&crate::zoo::tests::naming(
        crate::zoo::tests::file(106, 2, 2, &[], &[]),
        scenario,
    ))
    .unwrap()
}

/// The items made available again after a start.
fn all_on(app: &mut App) {
    app.make_available(&IDS.map(|id| (id, true)));
}

#[test]
fn a_scenario_or_freeform_start_makes_the_four_items_unavailable() {
    run(|app, cat| {
        assert_eq!(on(app, cat), [false; 4]);
        all_on(app);
        restart(app);
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn a_bare_map_does_not_run_the_start_hook() {
    run(|app, cat| {
        all_on(app);
        app.open_game(tile_with(&[]), Money::default()).unwrap();
        assert_eq!(on(app, cat), [true; 4]);
    });
}

#[test]
fn a_saved_game_with_a_scenario_block_runs_the_start_hook() {
    run(|app, cat| {
        all_on(app);
        app.open_zoo(zoo(Some("s/a.scn")), Money::default(), true)
            .unwrap();
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn a_saved_game_with_an_empty_scenario_block_does_not_run_the_start_hook() {
    run(|app, cat| {
        all_on(app);
        app.open_zoo(zoo(None), Money::default(), true).unwrap();
        assert_eq!(on(app, cat), [true; 4]);
    });
}

#[test]
fn a_saved_game_whose_scenario_block_fails_is_refused_and_runs_no_start_hook() {
    run(|app, cat| {
        all_on(app);
        let refused = app.open_zoo(zoo(Some("s/missing.scn")), Money::default(), true);
        assert!(refused.is_err());
        assert_eq!(on(app, cat), [true; 4]);
        // The same block opened straight (no refusal) still runs no hook.
        app.open_game_as(zoo(Some("s/missing.scn")), Money::default(), true)
            .unwrap();
        assert_eq!(on(app, cat), [true; 4]);
    });
}

#[test]
fn the_permanent_halloween_flag_keeps_the_jack_o_lantern_over_a_guest_hook() {
    run(|app, cat| {
        app.cheats.season.halloween = true;
        app.make_available(&[(JACK_O_LANTERN, true)]);
        today(app, 11, 1);
        app.guest_created();
        assert_eq!(on(app, cat), [false, true, false, false]);
        app.make_available(&[(JACK_O_LANTERN, false)]);
        today(app, 10, 31);
        app.guest_created();
        assert_eq!(on(app, cat), [false; 4]);
        // The flag down: the day flag was left alone, so the day makes it available.
        app.cheats.season.halloween = false;
        app.guest_created();
        assert_eq!(on(app, cat), [false, true, false, false]);
    });
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
        restart(app);
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

/// An app with the four items and the scenario `s/a.scn` of this text.
fn with_scenario(name: &str, text: &str, body: impl FnOnce(&mut App, &Catalogue)) {
    let files = [("s/a.scn", text.as_bytes().to_vec())];
    let install = cache_install_with(name, frame_file(), &files);
    let vfs = crate::vfs::Vfs::open(&install.0).unwrap();
    let cat = items();
    let mut app = App::empty(
        MapRenderer::new(&vfs, &cat),
        Timers::new(66, 16, 200),
        (64, 48),
        Settings::default(),
    )
    .unwrap();
    body(&mut app, &cat);
}

/// A trigger that makes the jack-o'-lantern's id available.
const MAKE: &str = "trulea=4\ntruleb=0\ntarga=7078\n";

#[test]
fn a_start_trigger_that_makes_an_item_available_is_undone_by_the_start_hook() {
    let text = format!("[start]\ntriggers=t\n[t]\n{MAKE}");
    with_scenario("cheat-season-start", &text, |app, cat| {
        restart(app);
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn a_resume_trigger_that_makes_an_item_available_is_undone_by_the_start_hook() {
    let text = format!("[resume]\ntriggers=t\n[t]\n{MAKE}");
    with_scenario("cheat-season-resume", &text, |app, cat| {
        app.open_zoo(zoo(Some("s/a.scn")), Money::default(), true)
            .unwrap();
        assert_eq!(on(app, cat), [false; 4]);
    });
}
