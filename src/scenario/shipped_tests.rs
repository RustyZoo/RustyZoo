//! The shipped scenario files, read from the game (`ZT_DATA_DIR`, ignored without it): the freeform
//! maps with their extra files and the seven tutorials, walked without a zoo (scenarios 10.8, 11.7).

use super::*;
use std::path::Path;

fn data() -> Option<Vfs> {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    Some(Vfs::open(Path::new(&dir)).unwrap())
}

/// The elements the file's goals ask about with a press: rule 10 with a `ruleb` other than 0, and
/// rule 11.
fn pressed(cfg: &Config) -> Vec<i32> {
    let names = cfg.section("goals").map_or(&[][..], |s| s.get_all("goal"));
    let mut ids = Vec::new();
    for name in names {
        let s = cfg.section(name);
        let (rule, by) = (int(s, "rulea"), int(s, "ruleb"));
        if rule == 11 || (rule == 10 && by != 0) {
            ids.push(int(s, "arga"));
        }
    }
    ids
}

/// Plays the tutorial `file` with a player who presses every element a goal asks about, in every
/// pass, and answers every popup; no zoo, so a goal on the zoo waits for ever. Returns the triggers
/// logged, and whether the scenario ended.
fn walk(vfs: &Vfs, file: &str) -> (Vec<String>, bool) {
    let cfg = Config::parse(&vfs.read(file).unwrap()).unwrap();
    let ids = pressed(&cfg);
    let mut s = Scenario::prepare(vfs, file).unwrap();
    let mut log = Vec::new();
    s.start(&mut log);
    let env = Env::new(1, 0, None);
    for _ in 0..200 {
        // Every string reads the same, and the row picked has it: rule 11's text matches.
        s.gui().sync(&|_| true, &|_| b"x".to_vec());
        for &id in &ids {
            s.gui().pressed(id, Some((0, b"x".to_vec())));
        }
        s.update(PERIOD, &env, &mut log);
        while s.next_act().is_some() {}
        if s.run() != Run::On {
            break;
        }
    }
    (log, s.run() != Run::On)
}

/// Each tutorial with the triggers the walk fires, the popups among them in order, and the first goal
/// that never fires: the walk has no zoo, so it stops at the first goal that asks for something built
/// (the end needs a prepared save, 10.8 point 2).
const TUTORIALS: [(&str, usize, &[&str], &str); 7] = [
    (
        "scn01a",
        21,
        &[
            "INTRO",
            "options_dialog",
            "DELETE",
            "GAME_OPTIONS",
            "ZOOM_IN",
            "ROTATE_CCW",
            "PLAY",
            "UNDO",
        ],
        "click_buy2",
    ),
    ("scn01", 6, &["INTRO", "exit"], "click_construction"),
    (
        "scn02",
        8,
        &["INTRO", "exit", "TERRAIN_DIALOG"],
        "click_construction",
    ),
    (
        "scn19",
        36,
        &[
            "INTRO",
            "STORY",
            "FILTER_DIALOG",
            "DELETE_FENCE",
            "CLICK_CLOSE_STATUS",
            "CLICK_CLOSE_RESEARCH",
            "CLICK_GUEST_TOGGLE",
            "CLICK_FOLIAGE_TOGGLE",
            "CLICK_BUILDING_TOGGLE",
        ],
        "SORT_DINO",
    ),
    (
        "scn27",
        6,
        &["INTRO", "FILTER_DIALOG"],
        "AQUA_TUTORIAL_CHAIN",
    ),
    ("scn28", 6, &["INTRO", "SHOW_DESC"], "AQUA_TUTORIAL2_CHAIN"),
    (
        "scn28b",
        7,
        &["INTRO", "COMBINED_DESC", "BUILD_WALRUS"],
        "AQUA_TUTORIAL3_CHAIN",
    ),
];

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_shipped_tutorials_fire_each_trigger_once_and_start_with_the_locks() {
    let vfs = data().unwrap();
    for (n, count, popups, stall) in TUTORIALS {
        let file = format!("scenario/{n}/{n}.scn");
        if !vfs.contains(&file) {
            eprintln!("skipped {n}: the install lacks it");
            continue;
        }
        let (log, ended) = walk(&vfs, &file);
        let triggers: Vec<&str> = log
            .iter()
            .filter_map(|l| l.strip_prefix("scenario trigger "))
            .collect();
        // The start triggers come first: every type unavailable, then the locks.
        assert!(
            triggers[0].contains(" start 4 9 "),
            "{n}: {:?}",
            triggers[0]
        );
        // The steps advance as far as the presses and the answers take them.
        assert_eq!(triggers.len(), count, "{n}: {triggers:?}");
        // A trigger runs once (the tutorials hold no goal that fires again without a zoo).
        let mut sorted = triggers.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), count, "{n}: {triggers:?}");
        // The popups come in this order: a popup is a trigger 2 with a `truleb` of 0.
        let shown: Vec<&str> = triggers
            .iter()
            .map(|t| t.split(' ').collect::<Vec<_>>())
            .filter(|f| f[2] == "2" && f[3] == "0")
            .map(|f| f[0])
            .collect();
        assert_eq!(shown, popups, "{n}");
        // The walk has no zoo: it ends at the first goal that asks for one, the scenario goes on.
        let cfg = Config::parse(&vfs.read(&file).unwrap()).unwrap();
        let goals = cfg.section("goals").map_or(&[][..], |s| s.get_all("goal"));
        let first = goals
            .iter()
            .map(|g| String::from_utf8_lossy(g))
            .find(|g| !triggers.iter().any(|t| t.starts_with(&format!("{g} "))));
        assert_eq!(first.as_deref(), Some(stall), "{n}");
        assert!(!ended, "{n}");
    }
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn a_full_game_freeform_map_lists_its_extra_goals_in_the_order_of_the_spec() {
    let vfs = data().unwrap();
    if !crate::fixtures::has_expansions(vfs.root().as_os_str()) {
        eprintln!("skipped: the install has no expansions");
        return;
    }
    let maps = vfs.listed("freeform/ff", ".scn");
    let Some(map) = maps.first() else {
        panic!("no freeform map");
    };
    let s = Scenario::prepare(&vfs, map).unwrap();
    // The timer, the 19 donation goals, the 45 award goals, the 5 of `unlock.scn`, then the 37 of the
    // numbered unlock files.
    assert_eq!(s.goals.len(), 107, "{map}");
    let names: Vec<&str> = s.goals.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names[0], "timer");
    // The goals of each extra file follow, file after file, in the order the file lists them.
    let listed = |file: &str| -> Vec<String> {
        let cfg = Config::parse(&vfs.read(file).unwrap()).unwrap();
        let goals = cfg.section("goals").map_or(&[][..], |s| s.get_all("goal"));
        goals
            .iter()
            .map(|g| String::from_utf8_lossy(g).into_owned())
            .collect()
    };
    let (donations, awards) = (
        listed("freeform/donation.scn"),
        listed("freeform/awards.scn"),
    );
    assert_eq!((donations.len(), awards.len()), (19, 45));
    assert_eq!(names[1..20], donations[..]);
    assert_eq!(names[20..65], awards[..]);
    // The 5 of `unlock.scn`, then the numbered unlock files in ascending name order.
    let mut numbered: Vec<String> = vfs
        .listed("freeform/unlock", ".scn")
        .into_iter()
        .filter(|f| f != "freeform/unlock.scn")
        .collect();
    numbered.sort();
    let unlocks: Vec<String> = ["freeform/unlock.scn".to_string()]
        .iter()
        .chain(&numbered)
        .flat_map(|f| listed(f))
        .collect();
    assert_eq!(unlocks.len(), 42, "{numbered:?}");
    assert_eq!(names[65..], unlocks[..]);
    // Several files name their goals alike: the first pass shows the files' order, by the block each
    // `DEFAULT_ITEMS` makes available.
    let files: Vec<String> = ["freeform/unlock.scn".to_string()]
        .into_iter()
        .chain(numbered)
        .collect();
    let blocks: Vec<Avail> = (files.iter())
        .filter_map(|f| {
            let cfg = Config::parse(&vfs.read(f).unwrap()).unwrap();
            let s = cfg.section("DEFAULT_ITEMS");
            let v = ["trulea", "truleb", "targa", "targb"].map(|k| int(s, k));
            s.and(avail(&cfg, v))
        })
        .collect();
    let mut s = Scenario::prepare(&vfs, map).unwrap();
    s.update(PERIOD, &Env::new(1, 0, None), &mut Vec::new());
    let first: Vec<Avail> = (s.gui().take_avail().into_iter())
        .filter(|a| matches!(a.which, Which::Names(_)))
        .collect();
    assert!(first.len() > 10, "{first:?}");
    assert_eq!(first, blocks);
}
