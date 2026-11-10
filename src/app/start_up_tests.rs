//! The saved game of the command line and the warning about user-made types, at the end of the
//! start-up (main menu 3.5 points 4 to 6).

use super::start_tests::{click, key, shown, with_game_in};
use super::*;
use crate::fixtures::Install;
use crate::ui::layout::{HIDDEN, NEVER_SHOWN};

const FIRST: &str = "scenario/scn01a/scn01a.scn";
const DONE: &str = "[UI]\nstartedFirstTutorial=1\nstartedDinoTutorial=1\nstartedAquaTutorial=1\n";
const BOX: (i32, i32) = (15, 25);
const OK: (i32, i32) = (35, 15);

fn scn(savegame: &str) -> Vec<u8> {
    format!("[start]\nsavegame={savegame}\n").into_bytes()
}

/// Runs the end of the start-up over a menu with `ini`, `types` saying whether the catalogue held
/// a user-made type, then `body`.
fn started_up(
    name: &str,
    ini: &str,
    types: bool,
    files: &[(&str, Vec<u8>)],
    path: Option<&str>,
    body: impl FnOnce(&mut App, &Install),
) {
    with_game_in(name, ini, 0, files, None, |app, install| {
        app.menu.as_mut().unwrap().set_user_types(types);
        app.start_up(path);
        body(app, install);
    });
}

fn settings(install: &Install) -> String {
    std::fs::read_to_string(install.0.join("user/zoo.ini")).unwrap_or_default()
}

fn messages<'a>(app: &'a App) -> Vec<&'a String> {
    (app.menu_events.iter())
        .filter(|l| l.starts_with("message "))
        .collect()
}

/// Whether the element and its layouts are drawn and hit.
fn visible(app: &mut App, id: i32) -> bool {
    app.interface().unwrap().shown(id)
}

/// Whether the element is back to hidden and never shown by its layout, as the file has it.
fn put_away(app: &mut App, id: i32) -> bool {
    let ui = app.interface().unwrap();
    let i = ui.tree.find(id).unwrap();
    ui.tree.elements[i].state & (HIDDEN | NEVER_SHOWN) == HIDDEN | NEVER_SHOWN
}

fn asking(app: &mut App) -> bool {
    app.interface().unwrap().modal_open() && shown(app, 45500)
}

#[test]
fn the_command_line_opens_the_game_and_the_menu_never_shows() {
    let ini = format!("{DONE}[user]\nlastfile=bad.zoo\n");
    started_up(
        "cl-open",
        &ini,
        true,
        &[],
        Some("saved.zoo"),
        |app, install| {
            assert!(app.camera().is_some() && !shown(app, 26));
            assert!(
                app.menu_events
                    .contains(&"game start load saved.zoo".into())
            );
            assert!(messages(app).is_empty() && !asking(app));
            // Continue keeps naming the last file: the path is not written.
            assert!(!settings(install).contains("lastfile"));
        },
    );
}

#[test]
fn no_path_an_empty_one_or_a_file_that_does_not_open_leaves_the_menu_and_says_nothing() {
    for (i, path) in [None, Some(""), Some("ghost.zoo"), Some("sub/ghost.zoo")]
        .into_iter()
        .enumerate()
    {
        started_up(&format!("cl-none{i}"), DONE, false, &[], path, |app, _| {
            assert!(app.camera().is_none() && shown(app, 26), "{path:?}");
            assert!(messages(app).is_empty() && !asking(app), "{path:?}");
        });
    }
}

#[test]
fn a_file_that_does_not_load_shows_the_loader_message_over_the_menu() {
    started_up("cl-bad", DONE, false, &[], Some("bad.zoo"), |app, _| {
        assert!(app.camera().is_none() && shown(app, 26));
        let said = messages(app);
        assert!(said.len() == 1 && said[0].contains("bad.zoo"), "{said:?}");
        assert!(app.interface().unwrap().modal_open());
    });
}

#[test]
fn a_failed_tutorial_falls_through_to_the_command_line_and_a_started_one_skips_it() {
    // The tutorial's map does not open: nothing is written and the saved game opens.
    let ghost = [(FIRST, scn("ghost.zoo"))];
    started_up(
        "cl-fall",
        "",
        false,
        &ghost,
        Some("saved.zoo"),
        |app, install| {
            assert!(app.camera().is_some() && !shown(app, 26));
            assert!(!settings(install).contains("started"));
        },
    );
    // The tutorial starts: the command line is not tried, so its bad file says nothing.
    let good = [(FIRST, scn("saved.zoo"))];
    started_up(
        "cl-skip",
        "",
        true,
        &good,
        Some("bad.zoo"),
        |app, install| {
            assert!(app.camera().is_some() && messages(app).is_empty() && !asking(app));
            assert!(settings(install).contains("startedFirstTutorial=1"));
            assert!(!app.menu_events.iter().any(|l| l.contains("load bad.zoo")));
        },
    );
}

#[test]
fn the_warning_shows_once_with_ok_alone_and_an_unticked_box() {
    started_up("warn-shape", DONE, true, &[], None, |app, install| {
        assert!(asking(app) && shown(app, 26));
        assert!(visible(app, 45514) && visible(app, 45515));
        assert!(shown(app, 45512));
        assert!(!shown(app, 45510), "Yes is not there");
        assert!(!app.interface().unwrap().is_down(45514));
        // OK writes nothing, closes the box and hides the check box again.
        app.pass(66, &click(OK.0, OK.1));
        assert!(!asking(app) && put_away(app, 45514) && put_away(app, 45515));
        assert!(!settings(install).contains("showUserEntityWarning"));
        // The flag was read once: nothing asks again.
        app.start_up(None);
        assert!(!asking(app));
    });
}

#[test]
fn ticking_writes_0_at_once_and_unticking_1_and_ok_keeps_what_is_there() {
    started_up("warn-tick", DONE, true, &[], None, |app, install| {
        let value = |app: &mut App, now, at: (i32, i32)| {
            app.pass(now, &click(at.0, at.1));
            settings(install)
        };
        assert!(value(app, 66, BOX).contains("showUserEntityWarning=0"));
        assert!(asking(app), "the box stays open");
        assert!(value(app, 132, BOX).contains("showUserEntityWarning=1"));
        assert!(value(app, 198, BOX).contains("showUserEntityWarning=0"));
        // Enter answers OK: the key stays as it was.
        app.pass(264, &key(Key::Enter));
        assert!(!asking(app) && put_away(app, 45514));
        assert!(settings(install).contains("showUserEntityWarning=0"));
    });
}

#[test]
fn the_setting_decides_and_only_a_menu_that_shows_untouched_asks() {
    let mut on = vec![DONE.to_string()];
    for value in ["-1", "7", "", "x", "2147483647"] {
        on.push(format!("{DONE}[user]\nshowUserEntityWarning={value}\n"));
    }
    for (i, ini) in on.iter().enumerate() {
        started_up(&format!("warn-on{i}"), ini, true, &[], None, |app, _| {
            assert!(asking(app), "{ini:?}");
        });
    }
    let off = format!("{DONE}[user]\nshowUserEntityWarning=0\n");
    started_up("warn-off", &off, true, &[], None, |app, _| {
        assert!(!asking(app))
    });
    started_up("warn-none", DONE, false, &[], None, |app, _| {
        assert!(!asking(app));
    });
    // A command line that failed or opened a game, and a tutorial that started, skip it.
    for (i, path) in ["bad.zoo", "ghost.zoo", "saved.zoo"]
        .into_iter()
        .enumerate()
    {
        started_up(
            &format!("warn-line{i}"),
            DONE,
            true,
            &[],
            Some(path),
            |app, _| {
                assert!(!asking(app), "{path}");
            },
        );
    }
    let good = [(FIRST, scn("saved.zoo"))];
    started_up("warn-tut", "", true, &good, None, |app, _| {
        assert!(app.camera().is_some() && !asking(app));
    });
}
