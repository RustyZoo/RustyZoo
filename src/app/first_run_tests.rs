//! The tutorial that starts by itself at the end of the start-up (main menu 3.5).

use super::start_tests::with_game_in;

const FIRST: &str = "scenario/scn01a/scn01a.scn";
const DINO: &str = "scenario/scn19/scn19.scn";
const AQUA: &str = "scenario/scn27/scn27.scn";

fn scn(savegame: &str) -> Vec<u8> {
    format!("[start]\nsavegame={savegame}\n").into_bytes()
}

/// Runs the first run of a menu with `ini` and `expansions` over the tutorials `files`. Gives
/// whether a game started, the settings written and the messages told.
fn first_run(
    name: &str,
    ini: &str,
    expansions: i32,
    files: &[(&str, Vec<u8>)],
) -> (bool, String, Vec<String>) {
    let mut out = (false, String::new(), Vec::new());
    with_game_in(name, ini, expansions, files, None, |app, install| {
        app.first_run();
        out.0 = app.camera().is_some();
        out.1 = std::fs::read_to_string(install.0.join("user/zoo.ini")).unwrap_or_default();
        out.2 = (app.menu_events.iter())
            .filter(|l| l.starts_with("message "))
            .cloned()
            .collect();
    });
    out
}

/// The flags set in `settings`, sorted.
fn written(settings: &str) -> Vec<&str> {
    let mut keys: Vec<&str> = settings
        .lines()
        .filter(|l| l.starts_with("started"))
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn the_first_tutorial_starts_and_sets_the_three_flags_whatever_the_expansions() {
    for (i, expansions) in [0, 3].into_iter().enumerate() {
        let (started, settings, said) = first_run(
            &format!("fr-first{i}"),
            "",
            expansions,
            &[(FIRST, scn("saved.zoo"))],
        );
        assert!(started && said.is_empty(), "{said:?}");
        assert_eq!(
            written(&settings),
            [
                "startedAquaTutorial=1",
                "startedDinoTutorial=1",
                "startedFirstTutorial=1"
            ]
        );
    }
}

#[test]
fn a_flag_that_is_not_zero_means_started_and_anything_else_means_not() {
    let files = [(FIRST, scn("saved.zoo"))];
    for (i, (value, due)) in [
        ("-1", false),
        ("1", false),
        ("7", false),
        ("0", true),
        ("", true),
        ("x", true),
    ]
    .into_iter()
    .enumerate()
    {
        let ini = format!("[UI]\nstartedFirstTutorial={value}\n");
        let (started, settings, _) = first_run(&format!("fr-flag{i}"), &ini, 0, &files);
        assert_eq!(
            (started, written(&settings).len() == 3),
            (due, due),
            "{value:?}"
        );
    }
    assert!(first_run("fr-missing", "[UI]\n", 0, &files).0);
}

#[test]
fn the_expansion_tutorials_follow_one_by_one_and_need_their_expansion() {
    let files = [(DINO, scn("saved.zoo")), (AQUA, scn("saved.zoo"))];
    let done = "[UI]\nstartedFirstTutorial=1\n";
    // Dinosaur Digs flagged: its tutorial, then both flags.
    let (started, settings, _) = first_run("fr-dino", done, 1, &files);
    assert!(started);
    assert_eq!(
        written(&settings),
        ["startedAquaTutorial=1", "startedDinoTutorial=1"]
    );
    // Only Marine Mania flagged: the Dinosaur Digs flag is 0 but its expansion is not there.
    let (started, settings, _) = first_run("fr-aqua", done, 2, &files);
    assert!(started);
    assert_eq!(written(&settings), ["startedAquaTutorial=1"]);
    // Both flagged and the Dinosaur Digs tutorial done: the Marine Mania one.
    let ini = format!("{done}startedDinoTutorial=1\n");
    let (started, settings, _) = first_run("fr-aqua2", &ini, 3, &files);
    assert!(started);
    assert_eq!(written(&settings), ["startedAquaTutorial=1"]);
    // No expansion, or both done: nothing starts.
    assert!(!first_run("fr-none", done, 0, &files).0);
    let all = "[UI]\nstartedFirstTutorial=1\nstartedDinoTutorial=1\nstartedAquaTutorial=1\n";
    assert!(!first_run("fr-all", all, 3, &files).0);
}

#[test]
fn a_start_that_fails_writes_nothing_and_no_later_tutorial_is_tried() {
    // The first tutorial has no map to start, the others would start.
    let later = [(DINO, scn("saved.zoo")), (AQUA, scn("saved.zoo"))];
    let (started, settings, said) = first_run("fr-chain", "", 3, &later);
    assert!(!started && written(&settings).is_empty() && said.is_empty());
    // The first one done, the Dinosaur Digs one fails: not the Marine Mania one.
    let ini = "[UI]\nstartedFirstTutorial=1\n";
    let files = [(DINO, scn("ghost.zoo")), (AQUA, scn("saved.zoo"))];
    let (started, settings, said) = first_run("fr-chain2", ini, 3, &files);
    assert!(!started && written(&settings).is_empty() && said.is_empty());
}

#[test]
fn only_a_map_that_opens_but_does_not_load_tells_why() {
    for (i, savegame) in ["", "ghost.zoo", "scenario"].into_iter().enumerate() {
        let files = [(FIRST, scn(savegame))];
        let (started, settings, said) = first_run(&format!("fr-silent{i}"), "", 0, &files);
        assert!(
            !started && written(&settings).is_empty() && said.is_empty(),
            "{savegame:?} {said:?}"
        );
    }
    let (started, settings, said) = first_run("fr-bad", "", 0, &[(FIRST, scn("bad.zoo"))]);
    assert!(!started && written(&settings).is_empty());
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with("message ") && said[0].contains("bad.zoo"),
        "{said:?}"
    );
}

#[test]
fn a_write_that_fails_is_ignored_and_the_game_runs() {
    with_game_in(
        "fr-write",
        "",
        0,
        &[(FIRST, scn("saved.zoo"))],
        None,
        |app, install| {
            // The settings folder is a file: nothing can be written there.
            std::fs::write(install.0.join("user"), b"").unwrap();
            app.first_run();
            assert!(app.camera().is_some());
        },
    );
}
