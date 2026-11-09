//! The shipped scenario files, read from the game (`ZT_DATA_DIR`, ignored without it): the freeform
//! maps with their extra files and the seven tutorials, walked without a zoo (scenarios 10.8, 11.7).

use super::*;
use std::collections::HashMap;
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

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_shipped_tutorials_fire_each_trigger_once_and_start_with_the_locks() {
    let vfs = data().unwrap();
    for n in ["scn01a", "scn01", "scn02", "scn19", "scn27", "scn28", "scn28b"] {
        let file = format!("scenario/{n}/{n}.scn");
        if !vfs.contains(&file) {
            eprintln!("skipped {n}: the install lacks it");
            continue;
        }
        let (log, _) = walk(&vfs, &file);
        let triggers: Vec<&String> = log
            .iter()
            .filter(|l| l.starts_with("scenario trigger"))
            .collect();
        // The start triggers come first: every type unavailable, then the locks.
        assert!(triggers[0].contains(" start 4 9 "), "{n}: {:?}", triggers[0]);
        assert!(triggers.len() > 3, "{n}: the steps do not advance");
        // A trigger runs once (the tutorials hold no goal that fires again without a zoo).
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for t in &triggers {
            *seen.entry(t.as_str()).or_default() += 1;
        }
        assert!(seen.values().all(|&c| c == 1), "{n}: {seen:?}");
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
    let (donations, awards) = (listed("freeform/donation.scn"), listed("freeform/awards.scn"));
    assert_eq!((donations.len(), awards.len()), (19, 45));
    assert_eq!(names[1..20], donations[..]);
    assert_eq!(names[20..65], awards[..]);
}
