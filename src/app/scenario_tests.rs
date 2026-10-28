//! The end of a scenario and its objectives panel, on the made-up interface of the start tests.

use super::start_tests::*;
use super::*;
use crate::lists::Scenario as Listed;
use crate::menu::Start;
use crate::money::Money;
use crate::sprite::tests::Install;
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
        assert_eq!(app.interface().unwrap().text(45206).unwrap(), b"text-16100");
        app.pass(now + 66, &[]);
        assert!(app.camera().is_some());
        // The answer stops the game and shows the scenario screen of the result.
        app.interface().unwrap().answer(7);
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
        assert_eq!(app.interface().unwrap().text(45206).unwrap(), b"text-16101");
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
        assert_eq!(ui.text(4142).unwrap(), b"text-9001");
        assert_eq!(ui.text(4143).unwrap(), b"text-story");
        let rows = match &ui.tree.elements[ui.tree.find(4145).unwrap()].kind {
            crate::ui::layout::Kind::ListBox(l) => l.rows.rows.clone(),
            _ => panic!("no list"),
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, b"text-9101 1");
        // Pressed again with the panel open, the lines come again (the game's state is the same).
        app.pass(now + 132, &[Command::Char(b'O')]);
        assert_eq!(said(app, 2, now + 132), ["menu scenario list 1"]);
    });
}

/// Whether the button of the end dialog is shown.
fn button(app: &mut App, id: i32) -> bool {
    shown(app, id)
}

#[test]
fn the_end_dialog_has_yes_and_no_and_only_no_stops_the_game() {
    with_menu("end-yes-no", "", |app, _| {
        let now = play(app, "s/win.scn");
        let (now, _) = until_asked(app, now);
        assert!(button(app, 45210) && button(app, 45211) && !button(app, 45212));
        // Yes: the dialog closes and the game goes on.
        app.interface().unwrap().answer(6);
        app.pass(now + 66, &[]);
        assert!(app.camera().is_some() && !shown(app, DIALOG) && !app.quit());
        assert!(
            said(app, 1, now + 66)
                .iter()
                .all(|l| !l.starts_with("menu game"))
        );
        // The end was told once: no second dialog.
        for p in 2..40 {
            app.pass(now + 66 * p, &[]);
            assert!(!app.interface().unwrap().modal_open());
        }
    });
    // No stops it and shows the scenario screen of the result.
    with_menu("end-no", "", |app, _| {
        let now = play(app, "s/win.scn");
        let (now, _) = until_asked(app, now);
        app.interface().unwrap().answer(7);
        app.pass(now + 66, &[]);
        assert!(app.camera().is_none());
        assert!(
            said(app, 1, now + 66)
                .iter()
                .any(|l| l.starts_with("menu scenario select"))
        );
    });
}

#[test]
fn yes_after_a_tutorial_starts_the_next_tutorial() {
    with_menu("end-next", "", |app, _| {
        let now = play(app, "scenario/scn01a/scn01a.scn");
        let (now, _) = until_asked(app, now);
        app.interface().unwrap().answer(6);
        app.pass(now + 66, &[]);
        let lines = said(app, 1, now + 66);
        assert!(
            lines.contains(&"menu game start scenario saved.zoo".to_string()),
            "{lines:?}"
        );
        assert!(app.camera().is_some() && !shown(app, DIALOG));
        let next = app.sim.scenario().map(|s| s.file().to_string());
        assert_eq!(next.as_deref(), Some("scenario/scn01/scn01.scn"));
    });
    // A tutorial without a next one (scn01 with no expansion goes to scn02, which is missing here
    // from the files) and a plain scenario start nothing.
    with_menu("end-no-next", "", |app, _| {
        let now = play(app, "s/win.scn");
        let (now, _) = until_asked(app, now);
        app.interface().unwrap().answer(6);
        app.pass(now + 66, &[]);
        assert!(app.sim.scenario().is_some_and(|s| s.file() == "s/win.scn"));
    });
}

#[test]
fn no_after_a_tutorial_shows_the_main_menu() {
    with_menu("end-tutorial-no", "", |app, _| {
        let now = play(app, "scenario/scn01a/scn01a.scn");
        let (now, _) = until_asked(app, now);
        app.interface().unwrap().answer(7);
        app.pass(now + 66, &[]);
        let lines = said(app, 1, now + 66);
        assert_eq!(lines[0], "menu game stop");
        // The scenario screen is not shown, though the tutorial is in the list.
        assert!(
            lines.iter().all(|l| !l.starts_with("menu scenario")),
            "{lines:?}"
        );
        assert!(app.camera().is_none() && shown(app, 26));
    });
}

/// Plays a start with these availability triggers over a lion (5) and a zebra (6), both
/// available; the flags at the end, the message line and what the pass said.
fn availability(name: &str, triggers: &[&str], body: impl FnOnce(&mut App, [bool; 2], &Install)) {
    let list: String = (0..triggers.len())
        .map(|i| format!("triggers=t{i}\n"))
        .collect();
    let sections: String = triggers
        .iter()
        .enumerate()
        .map(|(i, t)| format!("[t{i}]\n{t}\n"))
        .collect();
    let text = format!(
        "[duration]\nnummonths=-1\n[start]\n{list}{sections}\
         [8]\nlayout=45000\nshortText1=153\nmodal=1\n"
    );
    let files = [("s/av.scn", text.into_bytes())];
    let cat = Some(super::avail_tests::catalogue());
    with_game(name, "", &files, cat, |app, install| {
        play(app, "s/av.scn");
        let cat = app.renderer.catalogue();
        let build = &app.game.as_ref().unwrap().build;
        let flags = [0, 1].map(|i| build.is_available(cat, i));
        body(app, flags, install);
    });
}

#[test]
fn a_start_trigger_that_changed_a_flag_tells_its_message_or_popup() {
    // Hide all, then the lion available with a message: the lion's name fills the string.
    availability(
        "avail-msg",
        &[
            "trulea=4\ntruleb=9",
            "trulea=4\ntruleb=2\ntarga=5\ntargb=9201",
        ],
        |app, flags, _| {
            assert_eq!(flags, [true, false]);
            assert_eq!(app.interface().unwrap().text(7001).unwrap(), b"told lion");
            assert!(!app.interface().unwrap().modal_open());
        },
    );
    // The zebra hidden with a popup: the dialog opens with its text.
    availability(
        "avail-pop",
        &["trulea=4\ntruleb=5\ntarga=6\ntargb=8"],
        |app, flags, _| {
            assert_eq!(flags, [true, false]);
            let ui = app.interface().unwrap();
            assert!(ui.modal_open());
            assert_eq!(ui.text(45006).unwrap(), b"text-153");
            assert_eq!(ui.text(7001).unwrap(), b"");
        },
    );
    // Nothing changed (the lion is available already; no zebra has the id 99): nothing told.
    availability(
        "avail-none",
        &[
            "trulea=4\ntruleb=2\ntarga=5\ntargb=9201",
            "trulea=4\ntruleb=5\ntarga=99\ntargb=8",
        ],
        |app, flags, _| {
            assert_eq!(flags, [true, true]);
            let ui = app.interface().unwrap();
            assert!(!ui.modal_open());
            assert_eq!(ui.text(7001).unwrap(), b"");
        },
    );
}

#[test]
fn truleb_10_makes_the_type_available_for_good_and_stores_it_with_the_rewards() {
    availability(
        "avail-unlock",
        &[
            "trulea=4\ntruleb=9",
            "trulea=4\ntruleb=10\ntarga=6\ntargb=9201",
        ],
        |app, flags, install| {
            assert_eq!(flags, [false, true]);
            let ini = std::fs::read_to_string(install.0.join("user/zoo.ini")).unwrap();
            assert!(
                ini.contains("unlockEntity0=6") && ini.contains("unlockCount=1"),
                "{ini}"
            );
            assert_eq!(app.interface().unwrap().text(7001).unwrap(), b"");
        },
    );
}

/// A scenario that hides every type at its start, then wins (or loses) at once with these unlocks;
/// `body` runs when the end dialog is open.
fn rewarded(name: &str, unlock: &str, kind: &str, body: impl FnOnce(&mut App)) {
    let text = format!(
        "[desc]\nname=9001\n[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb=9\n\
         [completion]\n{unlock}\n[goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntext=9101\n{kind}\n"
    );
    let files = [("s/rw.scn", text.into_bytes())];
    let cat = Some(super::avail_tests::catalogue());
    with_game(name, "", &files, cat, |app, _| {
        let now = play(app, "s/rw.scn");
        until_asked(app, now);
        body(app);
    });
}

fn flags(app: &App) -> [bool; 2] {
    let cat = app.renderer.catalogue();
    let build = &app.game.as_ref().unwrap().build;
    [0, 1].map(|i| build.is_available(cat, i))
}

#[test]
fn a_win_makes_its_unlocked_types_available_at_once() {
    // The zebra is unlocked; the ids 0, -3, and 99 (no such type) change nothing.
    rewarded(
        "end-reward",
        "unlock=0\nunlock=-3\nunlock=99\nunlock=6",
        "",
        |app| assert_eq!(flags(app), [false, true]),
    );
    // A loss unlocks nothing, and a win with no unlock leaves everything hidden.
    rewarded("end-lost", "unlock=6", "type=7", |app| {
        assert_eq!(flags(app), [false, false]);
    });
    rewarded("end-none", "", "", |app| {
        assert_eq!(flags(app), [false, false]);
    });
}

#[test]
fn the_rewards_unlocked_in_this_run_apply_when_the_next_scenario_starts() {
    let text = "[duration]\nnummonths=-1\n[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb=9\n";
    let files = [("s/hide.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game("next-start", "", &files, cat, |app, _| {
        // The settings of the install hold no reward: the menu's unlock is all there is.
        app.pass(0, &[]);
        app.menu.as_mut().unwrap().unlock(&[6]);
        play(app, "s/hide.scn");
        assert_eq!(flags(app), [false, true]);
    });
}

/// A type of an earlier reward that the game hid stays hidden at the end, win or loss.
fn hidden_reward_stays_hidden_at_the_end(name: &str, kind: &str) {
    let text = format!(
        "[desc]\nname=9001\n[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb=9\n\
         [goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntext=9101\n{kind}\n"
    );
    let files = [("s/rw.scn", text.into_bytes())];
    let cat = Some(super::avail_tests::catalogue());
    with_game(name, "", &files, cat, |app, _| {
        app.pass(0, &[]);
        app.menu.as_mut().unwrap().unlock(&[6]);
        let now = play(app, "s/rw.scn");
        assert_eq!(flags(app), [false, true]);
        let hide = crate::scenario::Avail {
            which: crate::scenario::Which::Name(6),
            on: false,
            tell: None,
        };
        let cat = app.renderer.catalogue();
        app.game.as_mut().unwrap().build.change(cat, &[hide]);
        assert_eq!(flags(app), [false, false]);
        until_asked(app, now);
        assert_eq!(flags(app), [false, false]);
    });
}

#[test]
fn a_win_leaves_a_hidden_earlier_reward_hidden() {
    hidden_reward_stays_hidden_at_the_end("end-hidden-win", "");
}

#[test]
fn a_loss_leaves_a_hidden_earlier_reward_hidden() {
    hidden_reward_stays_hidden_at_the_end("end-hidden-loss", "type=7");
}
