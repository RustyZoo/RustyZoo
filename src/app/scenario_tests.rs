//! The end of a scenario and its objectives panel, on the made-up interface of the start tests.

use super::start_tests::*;
use super::*;
use crate::lists::Scenario as Listed;
use crate::menu::Start;
use crate::money::Money;
use crate::sprite::tests::Install;
use crate::ui::layout::{DISABLED, SELECTED};
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
        // The game stands still behind the dialog; the main bar stays (5.4.1).
        assert!(app.interface().unwrap().modal_open() && shown(app, DIALOG));
        assert!(shown(app, MAIN_PANEL));
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
fn a_hint_of_minus_1_shows_the_built_text_of_the_goal_in_progress() {
    // The goal is in progress (10 is not reached by 5): its template takes the value and the name.
    let scn = b"[desc]\nname=9001\nstory=SCN_STORY\n[start]\ntriggers=t\n[t]\ntrulea=2\n\
        truleb=0\ntarga=9\n[goals]\ngoal=g\n[g]\nrulea=99\narga=5\nvalue=10\ntext=9102\n\
        [9]\nlayout=45300\npersistentTextLayout=45400\npersistentShortText=-1\n";
    with_game(
        "hint-text",
        "",
        &[("s/hint.scn", scn.to_vec())],
        None,
        |app, _| {
            play(app, "s/hint.scn");
            assert!(shown(app, 45400));
            let ui = app.interface().unwrap();
            assert_eq!(ui.text(45406), Some(b"hint 10 lion".to_vec()));
        },
    );
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
    let mut cat = super::avail_tests::catalogue();
    cat.awards = crate::award::Definitions::of(&[(
        3,
        crate::award::Definition {
            name: 29001,
            tooltip: 0,
            icon: String::new(),
        },
    )]);
    let cat = Some(cat);
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

#[test]
fn truleb_10_for_an_id_already_listed_changes_no_flag_and_a_new_one_does() {
    // 6 is listed in the settings already: its unlock changes no flag, and the listed rewards
    // apply at the start, so it is available even after its hiding. 5 is not listed: its unlock
    // makes it available.
    let text = "[duration]\nnummonths=-1\n[start]\ntriggers=t0\ntriggers=t1\ntriggers=t2\n\
        triggers=t3\n[t0]\ntrulea=4\ntruleb=1\ntarga=6\n[t1]\ntrulea=4\ntruleb=1\ntarga=5\n\
        [t2]\ntrulea=4\ntruleb=10\ntarga=6\n[t3]\ntrulea=4\ntruleb=10\ntarga=5\n";
    let files = [("s/av.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game(
        "avail-listed",
        "[user]\nunlockCount=1\nunlockEntity0=6\n",
        &files,
        cat,
        |app, _| {
            play(app, "s/av.scn");
            let cat = app.renderer.catalogue();
            let build = &app.game.as_ref().unwrap().build;
            assert_eq!([0, 1].map(|i| build.is_available(cat, i)), [true, true]);
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

#[test]
fn a_win_does_not_show_a_hidden_reward_again_when_it_unlocks_a_listed_id() {
    let text = "[desc]\nname=9001\n[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb=9\n\
        [completion]\nunlock=6\n[goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntext=9101\n";
    let files = [("s/rw.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game("end-relisted", "", &files, cat, |app, _| {
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
fn a_trigger_unlock_of_a_listed_id_shows_no_hidden_type_during_play() {
    // The goal's trigger unlocks the zebra (6) when the goal is won, after the start rewards.
    let text = "[desc]\nname=9001\n[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb=0\n\
        [goals]\ngoal=g\n[g]\nrulea=99\narga=1\ntype=0\nvalue=1\ntrulea=4\ntruleb=10\ntarga=6\n";
    let files = [("s/tu.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game("trigger-relisted", "", &files, cat, |app, _| {
        app.pass(0, &[]);
        app.menu.as_mut().unwrap().unlock(&[6]);
        let now = play(app, "s/tu.scn");
        let hide = crate::scenario::Avail {
            which: crate::scenario::Which::Name(6),
            on: false,
            tell: None,
        };
        let cat = app.renderer.catalogue();
        app.game.as_mut().unwrap().build.change(cat, &[hide]);
        assert_eq!(flags(app), [true, false]);
        until_asked(app, now);
        assert_eq!(flags(app), [true, false]);
    });
}

#[test]
fn an_untyped_id_a_start_trigger_unlocked_is_listed_again_by_a_win() {
    // The start trigger unlocks 99, which has no type: the lookup after the triggers does not mark
    // it, so the win of 99 lists it again (entity types 10.4).
    let text = "[desc]\nname=9001\n[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb=10\ntarga=99\n\
        [completion]\nunlock=99\n[goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntext=9101\n";
    let files = [("s/un.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game("untyped-relisted", "", &files, cat, |app, install| {
        let now = play(app, "s/un.scn");
        until_asked(app, now);
        let ini = std::fs::read_to_string(install.0.join("user/zoo.ini")).unwrap();
        assert!(
            ini.contains("unlockCount=2") && ini.contains("unlockEntity1=99"),
            "{ini}"
        );
    });
}

/// The lines the passes since the start told, with the start's own.
fn told(app: &App) -> String {
    app.log_lines(1, 66).join("\n")
}

#[test]
fn trigger_1_shows_its_string_built_with_the_value_in_the_message_line() {
    availability(
        "deed-say",
        &["trulea=1\ntruleb=1\ntarga=9101\ntargb=42"],
        |app, _, _| {
            assert_eq!(
                app.interface().unwrap().text(7001).unwrap(),
                b"text-9101 42"
            );
            assert!(told(app).contains("scenario message 9101 kind 1"));
        },
    );
    // A string of 0 or less shows nothing.
    for text in [0, -4] {
        let body = format!("trulea=1\ntarga={text}\ntargb=42");
        availability("deed-say-none", &[&body], |app, _, _| {
            assert_eq!(app.interface().unwrap().text(7001).unwrap(), b"");
            assert!(!told(app).contains("scenario message"));
        });
    }
}

#[test]
fn trigger_6_gives_the_award_once_and_names_it_in_the_message_line() {
    let defined = |name: &str, triggers: &[&str], held: &[i32], said: &str| {
        availability(name, triggers, |app, _, _| {
            let game = app.game.as_ref().unwrap();
            // The map of the tests holds award 0 already.
            let mut got = game.world.zoo.awards.clone();
            got.retain(|&a| a != 0 || held.contains(&0));
            assert_eq!(got, held, "{name}");
            let line = app.interface().unwrap().text(7001).unwrap();
            assert_eq!(String::from_utf8_lossy(&line), said, "{name}");
        });
    };
    // The award 3 has a definition (see `catalogue`); the string names it.
    defined(
        "award-3",
        &["trulea=6\ntarga=3\ntargb=9401"],
        &[3],
        "won Gold",
    );
    // A duplicate is ignored; the larger id comes first.
    defined(
        "award-twice",
        &[
            "trulea=6\ntarga=3",
            "trulea=6\ntarga=3",
            "trulea=6\ntarga=8",
        ],
        &[8, 3],
        "",
    );
    // No definition, no text, or a text of 0 or less: the award is given, nothing shows.
    defined(
        "award-undefined",
        &["trulea=6\ntarga=4\ntargb=9401"],
        &[4],
        "",
    );
    for text in ["", "targb=0", "targb=-2"] {
        let body = format!("trulea=6\ntarga=3\n{text}");
        defined("award-silent", &[&body], &[3], "");
    }
    // A `truleb` other than 0 gives nothing but the message still shows.
    defined(
        "award-truleb",
        &["trulea=6\ntruleb=1\ntarga=3\ntargb=9401"],
        &[],
        "won Gold",
    );
    // A missing or negative id and a huge one are held as read.
    defined("award-missing", &["trulea=6"], &[0], "");
    availability("award-log", &["trulea=6\ntarga=-5"], |app, _, _| {
        assert!(told(app).contains("scenario award -5"));
    });
    defined("award-negative", &["trulea=6\ntarga=-5"], &[-5], "");
    defined(
        "award-huge",
        &["trulea=6\ntarga=2147483647"],
        &[i32::MAX],
        "",
    );
}

#[test]
fn trigger_3_donates_adds_or_sets_the_cash_and_tells_the_amount() {
    let cash = |name: &str, how: i32, amount: i32, text: i32| {
        let trigger = format!("trulea=3\ntruleb={how}\ntarga={amount}\ntargb={text}");
        let base = Money::default();
        availability(name, &[&trigger], |app, _, _| {
            let money = &app.game.as_ref().unwrap().money;
            let want = match how {
                0 | 1 => base.cash + amount as f32,
                _ => amount as f32,
            };
            assert_eq!(money.cash, want, "{name}");
            let donated = money.total[crate::money::DONATIONS];
            assert_eq!(donated, if how == 0 { amount as f32 } else { 0.0 });
            let said = app.interface().unwrap().text(7001).unwrap();
            let said = String::from_utf8_lossy(&said).into_owned();
            assert_eq!(said.starts_with("gift"), text > 0, "{said}");
            assert!(told(app).contains(&format!("scenario cash {how} {amount}")));
        });
    };
    cash("deed-donate", 0, 500, 9301);
    cash("deed-add", 1, 250, 0);
    cash("deed-set", 2, 1000, 9301);
    cash("deed-set-negative", 2, -40, -1);
}

#[test]
fn trigger_9_forbids_or_allows_the_sell_button_and_triggers_5_and_8_report_what_they_did() {
    let no_sale = |name, body: &str| {
        availability(name, &[body], |app, _, _| {
            assert!(app.game.as_ref().unwrap().animal.no_sale, "{name}");
            assert!(told(app).contains("scenario sale off"));
        });
    };
    no_sale("deed-sale-0", "trulea=9\ntarga=0");
    no_sale("deed-sale-negative", "trulea=9\ntarga=-3");
    no_sale("deed-sale-missing", "trulea=9");
    availability("deed-sale-1", &["trulea=9\ntarga=1"], |app, _, _| {
        assert!(!app.game.as_ref().unwrap().animal.no_sale);
        assert!(told(app).contains("scenario sale on"));
    });
    // The map has no animals and no tile three tiles from the entrance: no crate, so no popup.
    availability("deed-5", &["trulea=5\ntarga=5\ntargb=8"], |app, _, _| {
        assert!(told(app).contains("scenario boxed 5 none"), "{}", told(app));
        assert!(!app.interface().unwrap().modal_open());
    });
    // A removal that finds nothing still shows its popup.
    availability(
        "deed-8",
        &["trulea=8\ntruleb=1\ntarga=5\ntargb=8"],
        |app, _, _| {
            assert!(told(app).contains("scenario remove 1 5 count 0"));
            assert!(app.interface().unwrap().modal_open());
        },
    );
}

#[test]
fn trigger_3_message_writes_the_amount_with_cents_and_grouping() {
    let body = "trulea=3\ntruleb=1\ntarga=1500\ntargb=9301";
    availability("deed-cents", &[body], |app, _, _| {
        assert_eq!(
            app.interface().unwrap().text(7001).unwrap(),
            b"gift $1,500.00"
        );
    });
}

#[test]
fn trigger_3_with_an_unknown_mode_changes_no_cash_but_still_tells() {
    for how in [3, 7, -1, i32::MAX] {
        let body = format!("trulea=3\ntruleb={how}\ntarga=1500\ntargb=9301");
        availability("deed-cash-other", &[&body], |app, _, _| {
            assert_eq!(app.game.as_ref().unwrap().money.cash, Money::default().cash);
            let said = app.interface().unwrap().text(7001).unwrap();
            assert!(said.starts_with(b"gift $1,500"), "{how}");
        });
    }
    // Missing mode is 0, a donation: unchanged. No message when targb is missing.
    availability(
        "deed-cash-nomsg",
        &["trulea=3\ntruleb=7\ntarga=1500"],
        |app, _, _| {
            assert_eq!(app.interface().unwrap().text(7001).unwrap(), b"");
        },
    );
}

#[test]
fn trigger_8_with_an_unknown_kind_removes_nothing_but_still_pops_up() {
    for by in [4, 7, -1, i32::MAX] {
        let body = format!("trulea=8\ntruleb={by}\ntarga=5\ntargb=8");
        availability("deed-remove-other", &[&body], |app, _, _| {
            assert!(app.interface().unwrap().modal_open(), "{by}");
            assert!(told(app).contains(&format!("scenario remove {by} 5 count 0")));
        });
    }
    // Without a popup number nothing shows.
    availability(
        "deed-remove-nopopup",
        &["trulea=8\ntruleb=7\ntarga=5"],
        |app, _, _| {
            assert!(!app.interface().unwrap().modal_open());
        },
    );
}

#[test]
fn a_scenario_end_allows_selling_again() {
    let text = "[desc]\nname=9001\n[start]\ntriggers=t\n[t]\ntrulea=9\ntarga=0\n\
                [goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntext=9101\n";
    let files = [("s/sell.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game("sell-end", "", &files, cat, |app, _| {
        let now = play(app, "s/sell.scn");
        assert!(app.game.as_ref().unwrap().animal.no_sale);
        until_asked(app, now);
        assert!(!app.game.as_ref().unwrap().animal.no_sale);
    });
}

#[test]
fn a_scenario_end_hides_the_panels_and_the_selection_and_yes_shows_none() {
    let text = "[desc]\nname=9001\n[goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntext=9101\n";
    let files = [("s/hide.scn", text.as_bytes().to_vec())];
    let cat = Some(super::avail_tests::catalogue());
    with_game("end-hide", "", &files, cat, |app, _| {
        let now = play(app, "s/hide.scn");
        let id = add_lion(app);
        app.game.as_mut().unwrap().selected = Some(id);
        app.interface().unwrap().show(15);
        app.interface().unwrap().set_button(1019, true, true);
        let now = until_asked(app, now).0;
        let game = app.game.as_ref().unwrap();
        assert!(game.selected.is_none() && game.animal.selected.is_none());
        assert!(!shown(app, 15) && !shown(app, animal::LAYOUT) && shown(app, MAIN_PANEL));
        let ui = app.interface().unwrap();
        assert_eq!(
            ui.tree.elements[ui.tree.find(1019).unwrap()].state & SELECTED,
            0
        );
        // Yes (no next tutorial) closes the dialog and shows none of them again.
        ui.answer(6);
        let now = now + 66;
        app.pass(now, &[]);
        for step in 1..30 {
            app.pass(now + step * 66, &[]);
        }
        let game = app.game.as_ref().unwrap();
        assert!(game.selected.is_none() && game.animal.selected.is_none());
        assert!(!shown(app, 15) && !shown(app, animal::LAYOUT) && !shown(app, DIALOG));
    });
}

/// Adds a lion (name id 5) to the game and returns its id.
fn add_lion(app: &mut App) -> u32 {
    let lion = Entity {
        class: "animals".into(),
        type_name: "lion".into(),
        subtype: "m".into(),
        body_size: None,
        common: Some(crate::zoo::Common {
            x: 5 * 64 + 32,
            y: 5 * 64 + 32,
            id: 0,
            ..Default::default()
        }),
    };
    app.game.as_mut().unwrap().world.add(lion, None).unwrap()
}

#[test]
fn trigger_8_cancels_the_carry_and_the_selection_of_the_entity_it_removes() {
    // A goal that sees the button 2002 shown removes the entities of name id 5 (a lion) or 6.
    for (target, goes) in [(5, true), (6, false)] {
        let text = format!(
            "[duration]\nnummonths=-1\n[desc]\nname=9001\n[goals]\ngoal=g\n[g]\nrulea=10\nruleb=0\n\
             arga=2002\nvalue=1\ntrulea=8\ntruleb=0\ntarga={target}\n"
        );
        let files = [("s/rm.scn", text.into_bytes())];
        let cat = Some(super::avail_tests::catalogue());
        with_game("remove-carry", "", &files, cat, |app, _| {
            app.pass(0, &[]);
            app.interface().unwrap().set_hidden(2002, true);
            let mut now = play(app, "s/rm.scn");
            let id = add_lion(app);
            app.carry_up(id);
            let game = app.game.as_mut().unwrap();
            assert_eq!(game.carry.as_ref().map(|c| c.id), Some(id));
            game.selected = Some(id);
            app.interface().unwrap().set_hidden(2002, false);
            for _ in 0..70 {
                now += 66;
                app.pass(now, &[]);
            }
            let game = app.game.as_ref().unwrap();
            assert_eq!(game.world.find(id).is_none(), goes, "{target}");
            assert_eq!(game.carry.is_none(), goes, "{target}");
            assert_eq!(game.selected.is_none(), goes, "{target}");
        });
    }
}

#[test]
fn a_scenario_end_enables_what_a_trigger_disabled_and_keeps_what_it_hid() {
    // The start disables the button 2001 and hides 7001; a win or a loss enables the button and
    // leaves 7001 hidden (8.4).
    for (name, kind) in [("win", ""), ("lose", "type=7\n")] {
        let text = format!(
            "[desc]\nname=9001\n[start]\ntriggers=t\ntriggers=u\n[t]\ntrulea=2\ntruleb=3\ntarga=2001\n\
             [u]\ntrulea=2\ntruleb=7\ntarga=7001\n[goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\n{kind}"
        );
        let file = format!("s/{name}-clean.scn");
        let files = [(file.as_str(), text.into_bytes())];
        let cat = Some(super::avail_tests::catalogue());
        with_game(&format!("end-clean-{name}"), "", &files, cat, |app, _| {
            app.pass(0, &[]);
            app.interface().unwrap().set_hidden(7001, false);
            let now = play(app, &file);
            let off = |app: &mut App| {
                let ui = app.interface().unwrap();
                ui.tree.elements[ui.tree.find(2001).unwrap()].state & DISABLED != 0
            };
            assert!(off(app), "{name}");
            assert!(!shown(app, 7001), "{name}");
            until_asked(app, now);
            assert!(!off(app), "{name} disabled");
            assert!(!shown(app, 7001), "{name} stays hidden");
            // The mark is gone: the next show of the root layout brings the element back.
            app.interface().unwrap().show(1);
            assert!(shown(app, 7001), "{name} shown again");
        });
    }
}
