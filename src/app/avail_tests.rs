//! The start of a scenario: the triggers set the availability, then the ini rewards come last.

use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::menu::Start;
use crate::render::tests::{cache_install_with, frame_file, tile_with};
use crate::vfs::Vfs;

const SIZE: (u32, u32) = (64, 48);

pub(super) fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for (name, id) in [("lion", 5), ("zebra", 6)] {
        cat.push(EntityType {
            class: "animals",
            type_name: name.as_bytes().to_vec(),
            subtype: b"m".to_vec(),
            file: None,
            config: None,
            name_id: id,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// Starts the scenario `text` with the reward of the name id 5 and tells the two flags.
fn flags(name: &str, text: &str) -> [bool; 2] {
    flags_of(name, text, None, &[])
}

/// [`flags`] for a saved game of the scenario with these goal states instead of a start. The
/// types `won` are unlocked by a win of this run first (its menu settings hold them, not the ini).
fn flags_of(name: &str, text: &str, saved: Option<&[u32]>, won: &[i32]) -> [bool; 2] {
    let files = [("s/a.scn", text.as_bytes().to_vec())];
    let install = cache_install_with(name, frame_file(), &files);
    let mut vfs = Vfs::open(&install.0).unwrap();
    let ini = Config::parse(b"[user]\nunlockCount=1\nunlockEntity0=5\n").unwrap();
    vfs.overlay_ini(&ini).unwrap();
    let cat = catalogue();
    let mut app = App::empty(
        MapRenderer::new(&vfs, &cat),
        Timers::new(66, 16, 200),
        SIZE,
        Settings::default(),
    )
    .unwrap();
    if let Some(states) = saved {
        if !won.is_empty() {
            let mut menu = Menu::new(ini, &install.0, None, None, 0);
            let end = crate::scenario::Ended {
                file: "s/a.scn".into(),
                won: true,
                unlocks: won.to_vec(),
            };
            menu.scenario_ended(&end);
            app.menu = Some(menu);
        }
        let bytes = crate::zoo::tests::naming(
            crate::zoo::tests::file(106, 2, 2, &[], &[]),
            Some("s/a.scn"),
        );
        let mut zoo = crate::zoo::Zoo::parse(&bytes).unwrap();
        zoo.set_scenario("s/a.scn", states);
        app.open_game_as(zoo, Money::default(), true).unwrap();
        let build = &app.game.as_ref().unwrap().build;
        return [0, 1].map(|i| build.is_available(&cat, i));
    }
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
    let build = &app.game.as_ref().unwrap().build;
    [0, 1].map(|i| build.is_available(&cat, i))
}

#[test]
fn a_hide_all_start_trigger_does_not_undo_an_ini_reward() {
    let hide = |truleb: i32| format!("[start]\ntriggers=t\n[t]\ntrulea=4\ntruleb={truleb}\n");
    let one = |id: i32| format!("{}targa={id}\n", hide(1));
    // Both types start available; the reward is the lion's.
    assert_eq!(
        flags("avail-none", "[duration]\nnummonths=-1\n"),
        [true, true]
    );
    assert_eq!(flags("avail-all", &hide(9)), [true, false]);
    assert_eq!(flags("avail-animals", &hide(7)), [true, false]);
    assert_eq!(
        flags("avail-one", &hide(1)),
        [true, true],
        "name id 0 matches neither"
    );
    assert_eq!(flags("avail-6", &one(6)), [true, false]);
    assert_eq!(flags("avail-5", &one(5)), [true, true], "the reward wins");
}

#[test]
fn a_reward_won_in_the_run_still_applies_after_a_load() {
    // The resume triggers hide every type: the install ini leaves only the lion, and the zebra
    // is the reward of a win of this run, which the menu settings hold.
    let resume = "[resume]\ntriggers=r\n[r]\ntrulea=4\ntruleb=9\n";
    let text = format!("[duration]\nnummonths=-1\n{resume}");
    let won = flags_of("load-won", &text, Some(&[0, 0]), &[6]);
    assert_eq!(won, [true, true]);
}

#[test]
fn a_saved_game_runs_the_resume_triggers_and_the_triggers_of_its_goal_states_the_reward_last() {
    let goal = "[goals]\ngoal=g\n[g]\nrulea=99\narga=1\nvalue=1\ntrulea=4\ntruleb=1\ntarga=6\n";
    let text = |extra: &str| format!("[duration]\nnummonths=-1\n{extra}{goal}");
    let load =
        |name: &str, extra: &str, states: &[u32]| flags_of(name, &text(extra), Some(states), &[]);
    // The goal's trigger hides the zebra only when its state is stored.
    assert_eq!(load("load-0", "", &[0, 0]), [true, true]);
    assert_eq!(load("load-1", "", &[0, 1]), [true, false]);
    assert_eq!(load("load-missing", "", &[]), [true, true]);
    // The resume list hides every type; the reward is the lion's and wins.
    let resume = "[resume]\ntriggers=r\n[r]\ntrulea=4\ntruleb=9\n";
    assert_eq!(load("load-resume", resume, &[0, 0]), [true, false]);
}
