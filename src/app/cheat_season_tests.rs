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
    // A recolourable building, for the Halloween colours.
    cat.push(EntityType {
        class: "building",
        type_name: b"tent".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            Config::parse(b"[Characteristics/Integers]\ncIsColorReplaced=1\n").unwrap(),
        )),
        name_id: 1,
        footprint: [1, 1, 1],
        height: 0,
    });
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
fn with_scenario(name: &str, text: &str, ini: &str, body: impl FnOnce(&mut App, &Catalogue)) {
    let files = [("s/a.scn", text.as_bytes().to_vec())];
    let install = cache_install_with(name, frame_file(), &files);
    let mut vfs = crate::vfs::Vfs::open(&install.0).unwrap();
    vfs.overlay_ini(&Config::parse(ini.as_bytes()).unwrap())
        .unwrap();
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
    with_scenario("cheat-season-start", &text, "", |app, cat| {
        restart(app);
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn a_resume_trigger_that_makes_an_item_available_is_undone_by_the_start_hook() {
    let text = format!("[resume]\ntriggers=t\n[t]\n{MAKE}");
    with_scenario("cheat-season-resume", &text, "", |app, cat| {
        app.open_zoo(zoo(Some("s/a.scn")), Money::default(), true)
            .unwrap();
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn an_ini_reward_that_makes_an_item_available_is_kept_after_the_start_hook() {
    let ini = "[user]\nunlockCount=1\nunlockEntity0=7078\n";
    with_scenario(
        "cheat-season-reward",
        "[duration]\nnummonths=-1\n",
        ini,
        |app, cat| {
            restart(app);
            assert_eq!(on(app, cat), [false, true, false, false]);
        },
    );
}

fn real_halloween() -> (u32, u32) {
    (10, 31)
}

fn real_other_day() -> (u32, u32) {
    (10, 30)
}

fn real_31_december() -> (u32, u32) {
    (12, 31)
}

/// The colours of the entities of the open game, in list order.
fn colours(app: &App) -> Vec<Vec<u8>> {
    let zoo = &app.game.as_ref().unwrap().world.zoo;
    zoo.entities
        .iter()
        .map(|e| e.common.as_ref().unwrap().colors.clone())
        .collect()
}

/// The open game's world is a man, a woman, a tent and a keeper, colours all 1.
fn dress(app: &mut App) {
    use crate::unit::testing::{centre, entity, world_with};
    let mut world = world_with((10, 10), |zoo| {
        for (id, class, kind) in [
            (1, "guests", "man"),
            (2, "guests", "woman"),
            (3, "building", "tent"),
            (4, "keeper", "keeper"),
        ] {
            let mut e = entity(class, kind, id, centre((id as i32, 1)), 0);
            e.common.as_mut().unwrap().colors = vec![1; 4];
            zoo.entities.push(e);
        }
    });
    if let Some(crate::unit::Part::Guest(g)) = world.units.get_mut(2).map(|u| &mut u.part) {
        g.female = true;
    }
    app.game.as_mut().unwrap().world = world;
}

#[test]
fn a_start_on_the_real_31_october_raises_the_flag_and_launches_the_permanent_witch_once() {
    run(|app, _| {
        app.sim.real_day = real_halloween;
        assert!(!app.cheats.season.halloween);
        app.game_started();
        assert!(app.cheats.season.halloween);
        app.game_started();
        let log = app.sim.take_lines();
        let witches = log.iter().filter(|l| l.as_str() == "fly 5084 30").count();
        assert_eq!(witches, 1, "{log:?}");
        // Another real day does not lower the flag.
        app.sim.real_day = real_other_day;
        app.game_started();
        assert!(app.cheats.season.halloween);
    });
}

#[test]
fn every_start_on_the_real_31_october_gives_the_jack_o_lantern_back_and_recolours() {
    run(|app, cat| {
        app.sim.real_day = real_halloween;
        for _ in 0..2 {
            dress(app);
            app.game_started();
            assert_eq!(on(app, cat), [false, true, false, false]);
            let want = [vec![10, 2, 1, 1], vec![10, 11, 1, 1], vec![7, 5, 1, 1]];
            // The keeper (staff) keeps its colours.
            let want = [want.to_vec(), vec![vec![1; 4]]].concat();
            assert_eq!(
                colours(app)[..],
                want,
                "guests (10, 2 or 11), buildings (7, 5)"
            );
        }
    });
}

#[test]
fn a_start_on_another_real_day_recolours_nothing_and_leaves_the_four_items_unavailable() {
    run(|app, cat| {
        app.sim.real_day = real_other_day;
        dress(app);
        app.game_started();
        assert_eq!(on(app, cat), [false; 4]);
        assert!(colours(app).iter().all(|c| c[..] == [1; 4]));
        assert!(!app.cheats.season.halloween);
        assert!(!app.sim.take_lines().iter().any(|l| l.starts_with("fly")));
    });
}

#[test]
fn after_a_real_halloween_start_the_game_date_no_longer_offers_the_jack_o_lantern() {
    run(|app, cat| {
        app.sim.real_day = real_halloween;
        app.game_started();
        // A later start on another real day takes 7078 away and nothing gives it back (6.5.4).
        app.sim.real_day = real_other_day;
        app.game_started();
        today(app, 10, 31);
        app.guest_created();
        assert_eq!(on(app, cat), [false; 4]);
    });
}

#[test]
fn a_guest_created_on_the_game_date_launches_the_flying_objects() {
    run(|app, _| {
        let log = |app: &mut App, month, day| {
            today(app, month, day);
            app.guest_created();
            app.sim
                .take_lines()
                .into_iter()
                .filter(|l| l.starts_with("fly"))
                .collect::<Vec<_>>()
        };
        app.sim.take_lines();
        assert_eq!(log(app, 12, 25), ["fly 5083"]);
        assert_eq!(
            log(app, 12, 25),
            Vec::<String>::new(),
            "the sleigh once while its flag is up"
        );
        assert_eq!(log(app, 12, 26), Vec::<String>::new());
        assert_eq!(
            log(app, 12, 25),
            ["fly 5083"],
            "another day lowered the flag"
        );
        assert_eq!(log(app, 9, 15), ["fly 5082"]);
        assert_eq!(log(app, 9, 15), Vec::<String>::new());
        // The witch comes at every creation, with or without the permanent flag.
        assert_eq!(log(app, 10, 31), ["fly 5084"]);
        app.cheats.season.halloween = true;
        assert_eq!(log(app, 10, 31), ["fly 5084"]);
    });
}

#[test]
fn a_start_and_a_file_load_stop_the_loops_so_the_sleigh_comes_again() {
    run(|app, _| {
        today(app, 12, 25);
        app.guest_created();
        app.game_started();
        app.sim.take_lines();
        today(app, 12, 25);
        app.guest_created();
        assert!(app.sim.take_lines().contains(&"fly 5083".to_string()));
        app.open_game(tile_with(&[]), Money::default()).unwrap();
        today(app, 12, 25);
        app.guest_created();
        assert!(app.sim.take_lines().contains(&"fly 5083".to_string()));
    });
}

#[test]
fn a_real_31_of_another_month_is_no_halloween_at_the_start_and_at_the_arrival() {
    run(|app, cat| {
        app.sim.real_day = real_31_december;
        dress(app);
        app.game_started();
        assert_eq!(on(app, cat), [false; 4]);
        assert!(colours(app).iter().all(|c| c[..] == [1; 4]));
        assert!(!app.cheats.season.halloween);
        assert!(!app.sim.take_lines().iter().any(|l| l.starts_with("fly")));
    });
    assert_ne!(
        crate::guest::tests::arrival_shirt(real_31_december),
        10,
        "an uncoloured guest at the arrival"
    );
}

#[test]
fn a_start_on_the_real_31_october_leaves_the_staff_colours() {
    run(|app, _| {
        app.sim.real_day = real_halloween;
        dress(app);
        app.game_started();
        let staff = &app.game.as_ref().unwrap().world.zoo.entities;
        let keeper = staff.iter().find(|e| e.class == "keeper").unwrap();
        assert_eq!(keeper.common.as_ref().unwrap().colors, [1; 4]);
        assert_eq!(colours(app)[0][0], 10, "the guests were recoloured");
    });
}

/// The three ambient types of the hooks, with the integers `ini` for each.
fn flights(ini: &str) -> Catalogue {
    let mut cat = items();
    for (id, name) in [(5082, "biplane"), (5083, "sleigh"), (5084, "witch")] {
        cat.push(EntityType {
            class: "ambient",
            type_name: name.into(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                Config::parse(format!("[Characteristics/Integers]\n{ini}").as_bytes()).unwrap(),
            )),
            name_id: id,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// A game after a scenario start, whose types include the three ambient ones.
fn fly(ini: &str, body: impl FnOnce(&mut App)) {
    let cat = (flights(ini), &[][..]);
    with_world("cheat-flights", (false, false), cat, |app, _| {
        restart(app);
        body(app);
    });
}

/// The ambient creatures of the game: type name, speed.
fn creatures(app: &App) -> Vec<(String, u32)> {
    let zoo = &app.game.as_ref().unwrap().world.zoo;
    let speed = |e: &crate::zoo::Entity| e.common.as_ref().and_then(|c| c.flight).map(|f| f.speed);
    zoo.entities
        .iter()
        .filter(|e| e.class == "ambient")
        .map(|e| (e.type_name.clone(), speed(e).unwrap()))
        .collect()
}

#[test]
fn the_date_hooks_make_a_flight_of_the_type_of_the_day() {
    fly("cSpeed=100\n", |app| {
        for (month, day, name) in [(12, 25, "sleigh"), (9, 15, "biplane"), (10, 31, "witch")] {
            today(app, month, day);
            app.guest_created();
            assert_eq!(creatures(app).pop(), Some((name.to_string(), 100)));
        }
        assert_eq!(creatures(app).len(), 3);
        // The same day again launches nothing more for the vehicles.
        today(app, 10, 31);
        app.guest_created();
        today(app, 9, 15);
        app.guest_created();
        today(app, 9, 15);
        app.guest_created();
        assert_eq!(creatures(app).len(), 5, "a second witch, no second biplane");
    });
}

#[test]
fn the_permanent_witch_makes_no_flight() {
    fly("cSpeed=100\n", |app| {
        app.sim.real_day = real_halloween;
        app.game_started();
        assert!(creatures(app).is_empty());
    });
}

#[test]
fn a_type_that_is_missing_makes_no_flight_and_no_loop_and_a_line_says_so() {
    run(|app, _| {
        app.sim.take_lines();
        today(app, 12, 25);
        app.guest_created();
        let log = app.sim.take_lines();
        assert!(log.contains(&"fly 5083".to_string()), "{log:?}");
        assert!(log.contains(&"flight 5083 missing".to_string()), "{log:?}");
        assert!(creatures(app).is_empty());
        assert!(app.loop_sounds_placed().is_empty());
        // The permanent witch is no launch of a type, so it has nothing to miss.
        app.sim.real_day = real_halloween;
        app.game_started();
        assert!(!app.sim.take_lines().iter().any(|l| l.contains("missing")));
    });
}

#[test]
fn the_sleigh_and_the_biplane_hold_a_loop_each_and_the_witch_none() {
    fly("cSpeed=100\n", |app| {
        let loops = |app: &App| {
            let mut sounds: Vec<_> = (app.loop_sounds_placed().into_iter())
                .map(|s| (s.file, s.base))
                .collect();
            sounds.sort();
            sounds
        };
        assert!(loops(app).is_empty());
        today(app, 10, 31);
        app.guest_created();
        assert!(loops(app).is_empty(), "the witch has no loop");
        today(app, 12, 25);
        app.guest_created();
        assert_eq!(loops(app), [("sounds/sleigh.wav".to_string(), 0)]);
        today(app, 9, 15);
        app.guest_created();
        assert_eq!(
            loops(app),
            [
                ("sounds/biplane.wav".to_string(), 1000),
                ("sounds/sleigh.wav".to_string(), 0)
            ]
        );
        // A new game drops the holds: the flights of the old one are gone with it.
        app.open_game(tile_with(&[]), Money::default()).unwrap();
        assert!(loops(app).is_empty());
    });
}

#[test]
fn a_flight_leaves_the_map_and_takes_its_loop_with_it() {
    // 1,000 units a second: the one-tile map is crossed in a few steps.
    fly("cSpeed=1000\n", |app| {
        today(app, 12, 25);
        app.guest_created();
        let at = |app: &App| {
            let c = app.game.as_ref().unwrap().world.zoo.entities[0]
                .common
                .clone()
                .unwrap();
            (c.x, c.y)
        };
        let start = at(app);
        assert_eq!(app.loop_sounds_placed().len(), 1);
        app.pass(200, &[]);
        // The first move takes it off the map; it stays, with its loop, until the next removal pass.
        assert_ne!(at(app), start, "it flew");
        assert_eq!(creatures(app).len(), 1);
        for now in [400, 600] {
            app.pass(now, &[]);
        }
        assert!(creatures(app).is_empty(), "and left the map");
        assert!(app.loop_sounds_placed().is_empty());
    });
}
