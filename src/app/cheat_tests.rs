//! The name hook and the research category cheats, driven with invented texts.

use super::developer_tests::{available, with_game};
use super::*;
use crate::cheat::{Cheat, Subject};

/// The category of the fixture's tree has the name id 3; its programs make the lion and the zebra
/// available.
const CATEGORY: i32 = 3;
const NONE: [bool; 3] = [false, false, false];
const BOTH: [bool; 3] = [true, true, false];

fn arm(app: &mut App, subject: Subject, text: &str, name: i32) {
    app.cheats
        .push(subject, text, Cheat::ResearchCategory(name));
}

fn progress(app: &App) -> Vec<f32> {
    let tree = app.sim.research().unwrap();
    let programs = &tree.branches[0].categories[0].programs;
    programs.iter().map(|p| p.progress).collect()
}

#[test]
fn a_guest_name_completes_the_category_without_progress_and_can_be_repeated() {
    with_game("cheat-guest", true, false, |app, cat| {
        arm(app, Subject::Guest, "Zed Zed", CATEGORY);
        assert_eq!(available(app, cat), NONE);
        app.interface().unwrap().set_text(3402, b"Zed Zed");
        app.guest_rename();
        assert_eq!(available(app, cat), BOTH);
        assert_eq!(progress(app), [0.0, 0.0]);
        // The same text again runs it again; nothing is undone by another text.
        app.interface().unwrap().set_text(3402, b"other");
        app.guest_rename();
        app.interface().unwrap().set_text(3402, b"Zed Zed");
        app.guest_rename();
        assert_eq!(available(app, cat), BOTH);
    });
}

#[test]
fn only_the_exact_text_of_the_right_class_runs_a_cheat() {
    with_game("cheat-exact", true, false, |app, cat| {
        arm(app, Subject::Guest, "Zed Zed", CATEGORY);
        for text in ["zed zed", "Zed Zed ", " Zed Zed", "Zed", ""] {
            app.interface().unwrap().set_text(3402, text.as_bytes());
            app.guest_rename();
        }
        app.name_hook(Subject::Animal, "Zed Zed");
        app.name_hook(Subject::Staff, "Zed Zed");
        app.name_hook(Subject::Exhibit, "Zed Zed");
        assert_eq!(available(app, cat), NONE);
        app.name_hook(Subject::Guest, "Zed Zed");
        assert_eq!(available(app, cat), BOTH);
    });
}

#[test]
fn a_category_that_does_not_exist_or_a_game_without_a_tree_does_nothing() {
    for (research, name) in [(true, 0), (true, -1), (true, i32::MAX), (false, CATEGORY)] {
        with_game("cheat-none", research, false, |app, cat| {
            let before = available(app, cat);
            arm(app, Subject::Guest, "Zed Zed", name);
            app.name_hook(Subject::Guest, "Zed Zed");
            assert_eq!(available(app, cat), before, "{research} {name}");
        });
    }
}

#[test]
fn without_a_game_or_a_field_the_hook_does_nothing() {
    with_game("cheat-nogame", true, false, |app, cat| {
        arm(app, Subject::Guest, "Zed Zed", CATEGORY);
        // No text in the field: no hook.
        app.guest_rename();
        assert_eq!(available(app, cat), NONE);
        app.game = None;
        app.name_hook(Subject::Guest, "Zed Zed");
        app.entity_hook(Subject::Guest, 3402);
    });
}
