//! The end of a scenario and its objectives panel, on the made-up interface of the start tests.

use super::start_tests::*;
use super::*;
use crate::lists::Scenario as Listed;
use crate::menu::Start;
use crate::money::Money;
use crate::zoo::Zoo;

const MAIN_PANEL: i32 = 2;
const DIALOG: i32 = 45200;

/// Plays the scenario `file` (listed as `aa`) for a pass and returns the clock.
fn play(app: &mut App, file: &str) -> u64 {
    app.pass(0, &[]);
    let listed = Listed {
        section: "aa".into(),
        name: 9001,
        file: file.into(),
        result_key: "aa".into(),
        locks: Vec::new(),
    };
    app.menu
        .as_mut()
        .unwrap()
        .set_lists(vec![listed], Vec::new());
    app.open_game(Zoo::parse(&map()).unwrap(), Money::default())
        .unwrap();
    app.begin(&Start {
        path: "saved.zoo".into(),
        label: "scenario".into(),
        forget_last: true,
        freeform: false,
        cash: Default::default(),
        reset: false,
        reset_fences: false,
        scenario: Some(file.into()),
    });
    // The first pass tells the start.
    app.pass(66, &[]);
    66
}

/// Passes until a question is open (at most 100) and what the menu said meanwhile.
fn until_asked(app: &mut App, mut now: u64) -> (u64, Vec<String>) {
    let mut said_all = Vec::new();
    for pass in 1..100 {
        now += 66;
        app.pass(now, &[]);
        said_all.extend(said(app, pass, now));
        if app.interface().unwrap().modal_open() {
            break;
        }
    }
    (now, said_all)
}

#[test]
fn a_won_scenario_stores_its_result_asks_and_leaves_for_the_scenario_screen() {
    with_menu("end-won", "", |app, files| {
        let now = play(app, "s/win.scn");
        let (now, told) = until_asked(app, now);
        assert_eq!(
            told,
            [
                "menu scenario ended won",
                "menu scenario result aa 1",
                "menu scenario unlock 7",
                "menu scenario dialog 16100",
            ]
        );
        let ini = std::fs::read_to_string(files.0.join("user/zoo.ini")).unwrap();
        assert!(
            ini.contains("aa=1") && ini.contains("unlockEntity0=7"),
            "{ini}"
        );
        // The game stands still behind the dialog, and the game's layouts are hidden.
        assert!(app.interface().unwrap().modal_open() && shown(app, DIALOG));
        assert!(!shown(app, MAIN_PANEL));
        assert_eq!(app.interface().unwrap().text(45206).unwrap(), b"Done");
        app.pass(now + 66, &[]);
        assert!(app.camera().is_some());
        // The answer stops the game and shows the scenario screen of the result.
        app.interface().unwrap().answer(1);
        app.pass(now + 132, &[]);
        let lines = said(app, 99, now + 132);
        assert_eq!(lines[0], "menu game stop");
        assert!(
            lines.contains(&"menu scenario aa won 9001".to_string()),
            "{lines:?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("menu scenario select aa won "))
        );
        assert!(app.camera().is_none() && shown(app, 26) && !shown(app, DIALOG));
    });
}

#[test]
fn the_end_dialog_of_a_lost_scenario_and_the_window_closing_during_it() {
    with_menu("end-lost", "", |app, files| {
        let now = play(app, "s/lose.scn");
        let (now, told) = until_asked(app, now);
        assert_eq!(
            told,
            [
                "menu scenario ended lost",
                "menu scenario result aa 2",
                "menu scenario dialog 16101",
            ]
        );
        let ini = std::fs::read_to_string(files.0.join("user/zoo.ini")).unwrap();
        assert!(ini.contains("aa=2") && !ini.contains("unlock"), "{ini}");
        assert_eq!(app.interface().unwrap().text(45206).unwrap(), b"Failed");
        // Closing the window ends the run at once: the result is stored, nothing is left to save.
        app.pass(now + 66, &[Command::Close]);
        assert!(app.quit());
    });
}

#[test]
fn a_game_that_is_not_a_scenario_never_asks_and_a_running_one_waits() {
    with_menu("end-none", "", |app, _| {
        app.pass(0, &[]);
        app.open_game(Zoo::parse(&map()).unwrap(), Money::default())
            .unwrap();
        for p in 1..60 {
            app.pass(66 * p, &[]);
            assert!(!app.interface().unwrap().modal_open());
        }
    });
}

#[test]
fn the_o_key_and_the_button_fill_the_objectives_panel() {
    with_menu("objectives", "", |app, _| {
        let now = play(app, "s/win.scn");
        app.pass(now + 66, &[Command::Char(b'o')]);
        assert_eq!(said(app, 1, now + 66), ["menu scenario list 1"]);
        assert!(shown(app, 803));
        let ui = app.interface().unwrap();
        assert_eq!(ui.text(4142).unwrap(), b"Name");
        assert_eq!(ui.text(4143).unwrap(), b"Story");
        let rows = match &ui.tree.elements[ui.tree.find(4145).unwrap()].kind {
            crate::ui::layout::Kind::ListBox(l) => l.rows.rows.clone(),
            _ => panic!("no list"),
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, b"Keep 1 zoos");
        // Pressed again with the panel open, the lines come again (the game's state is the same).
        app.pass(now + 132, &[Command::Char(b'O')]);
        assert_eq!(said(app, 2, now + 132), ["menu scenario list 1"]);
    });
}
