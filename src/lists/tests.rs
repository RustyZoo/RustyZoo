use super::*;
use crate::sprite::tests::Install;

fn vfs(name: &str, members: &[(&str, &str)]) -> (Install, Vfs) {
    let members: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|(n, c)| (*n, c.as_bytes().to_vec()))
        .collect();
    let install = Install::new(name, &members);
    let vfs = Vfs::open(&install.0).unwrap();
    (install, vfs)
}

const FIRST: &str = "[bb]\nname=11\nscenario=s/bb.scn\nunlocks=bb\nlocks=aa\n\
    [aa]\nname=10\nscenario=s/aa.scn\nunlocks=aa\n\
    [dd]\nname=13\nscenario=s/dd.scn\nexpansion=1\n";
const SECOND: &str = "[cc]\nname=12\nscenario=s/cc.scn\nlocks=aa\nlocks=bb\n\
    [aa]\nname=99\nscenario=s/new.scn\nunlocks=aa\n\
    [ee]\nname=15\nexpansion=3\n";

#[test]
fn scenarios_are_listed_by_section_whatever_the_file_and_a_repeated_section_replaces() {
    let (_i, vfs) = vfs(
        "lists-order",
        &[
            ("scenar01.cfg", FIRST),
            ("scenarx.cfg", SECOND),
            // Too long a name, and not a list: never read.
            ("scenario/scn01/economy.cfg", "[zz]\nname=1\n"),
        ],
    );
    let names = |flags| {
        scenario_list(&vfs, flags)
            .iter()
            .map(|s| s.section.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(0), ["aa", "bb", "cc"]);
    assert_eq!(names(1), ["aa", "bb", "cc", "dd"]);
    assert_eq!(names(3), ["aa", "bb", "cc", "dd", "ee"]);
    assert_eq!(names(2), ["aa", "bb", "cc"], "a flag that is not there");
    let list = scenario_list(&vfs, 0);
    assert_eq!((list[0].name, list[0].file.as_str()), (99, "s/new.scn"));
    assert_eq!(list[1].locks, ["aa"]);
    assert_eq!(list[2].locks, ["aa", "bb"]);
    // The result key is the section when no `unlocks` says otherwise.
    assert_eq!(list[2].result_key, "cc");
}

#[test]
fn the_files_that_are_not_there_or_not_readable_give_an_empty_list() {
    let (_i, vfs) = vfs("lists-none", &[("other.cfg", "[a]\n")]);
    assert!(scenario_list(&vfs, 3).is_empty());
    assert!(freeform_list(&vfs).is_empty());
}

fn ini(text: &str) -> Config {
    Config::parse(text.as_bytes()).unwrap()
}

#[test]
fn a_scenario_is_won_playable_or_locked_by_the_results() {
    let (_i, vfs) = vfs(
        "lists-status",
        &[("scenar01.cfg", FIRST), ("scenarx.cfg", SECOND)],
    );
    let list = scenario_list(&vfs, 0);
    let states = |text: &str| {
        let ini = ini(text);
        list.iter()
            .map(|s| s.status(&ini).name())
            .collect::<Vec<_>>()
    };
    // Nothing finished: only the scenarios without locks can be played.
    assert_eq!(states(""), ["playable", "locked", "locked"]);
    // A win unlocks, 2 is a loss, 0 a cheat; a result of 3 or garbage is never finished.
    assert_eq!(states("[scenario]\naa=1\n"), ["won", "playable", "locked"]);
    assert_eq!(
        states("[Scenario]\nAA=0\nbb=1\n"),
        ["won", "won", "playable"]
    );
    assert_eq!(
        states("[scenario]\naa=2\n"),
        ["playable", "locked", "locked"]
    );
    assert_eq!(
        states("[scenario]\naa=x\nbb=-5\n"),
        ["playable", "won", "locked"]
    );
    assert_eq!(result(&ini(""), "zz"), 3);
}

#[test]
fn freeform_maps_come_from_the_files_in_name_order_and_unreadable_ones_are_left_out() {
    let (_i, vfs) = vfs(
        "lists-free",
        &[
            (
                "freeform.cfg",
                "[freeform]\nfreeform=f/a.scn\nfreeform=f/gone.scn\nfreeform=f/b.scn\n",
            ),
            ("freefo01.cfg", "[freeform]\nfreeform=f/c.scn\n"),
            ("freecvs.cfg", "[freeform]\nfreeform=f/c.scn\n"),
            (
                "f/a.scn",
                "[freeform]\nname=1\ndescription=DESC_A\nicon=f/a/a\n",
            ),
            ("f/b.scn", "[freeform]\nname=2\n"),
            ("f/c.scn", "[start]\nsavegame=maps/c.zoo\n"),
        ],
    );
    let list = freeform_list(&vfs);
    let files: Vec<&str> = list.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(files, ["f/c.scn", "f/a.scn", "f/b.scn"]);
    assert_eq!(
        (
            list[1].name,
            list[1].description.as_str(),
            list[1].icon.as_str()
        ),
        (1, "DESC_A", "f/a/a")
    );
    assert_eq!((list[0].name, list[2].icon.as_str()), (0, ""));
}

#[test]
fn the_cash_starts_inside_its_bounds_and_stops_at_them() {
    let cash = |text: &str| Cash::from_settings(&ini(text));
    let full = cash(
        "[UI]\nMSStartingCash=500000\nMSMinCash=10000\nMSMaxCash=500000\nMSCashIncrement=5000\n",
    );
    assert_eq!(
        (full.amount, full.can_up(), full.can_down()),
        (500_000, false, true)
    );
    // Defaults.
    let default = cash("");
    assert_eq!(
        (default.amount, default.min, default.max, default.step),
        (50_000, 5000, 500_000, 5000)
    );
    // The start is limited too, and the keys ignore case.
    assert_eq!(cash("[ui]\nmsstartingcash=1\n").amount, 5000);
    assert_eq!(cash("[UI]\nMSStartingCash=99999999999\n").amount, 500_000);
    assert_eq!(cash("[UI]\nMSStartingCash=-4\n").amount, 5000);
    let mut c = cash("[UI]\nMSStartingCash=10000\nMSMinCash=10000\n");
    assert!(!c.can_down());
    c.down();
    assert_eq!(c.amount, 10_000);
    c.up();
    assert_eq!(c.amount, 15_000);
    for _ in 0..200 {
        c.up();
    }
    assert_eq!((c.amount, c.can_up()), (500_000, false));
    // A step that is 0 or negative never leaves the bounds; one that is huge goes to the bound.
    for step in ["0", "-7000", "2147483647"] {
        let mut c = cash(&format!("[UI]\nMSCashIncrement={step}\n"));
        c.up();
        c.down();
        c.down();
        assert!((c.min..=c.max).contains(&c.amount), "{step}");
    }
    // Bounds that cross: the upper one wins and nothing panics.
    let mut c = cash("[UI]\nMSMinCash=900\nMSMaxCash=100\n");
    c.up();
    c.down();
    assert_eq!(c.amount, 100);
}

#[test]
fn the_difficulty_follows_the_amount() {
    let at = |amount| {
        let mut c = Cash::from_settings(&ini("[UI]\nMSMinCash=0\n"));
        c.set(amount);
        c.difficulty()
    };
    assert_eq!(at(0), Difficulty::Hard);
    assert_eq!(at(24_999), Difficulty::Hard);
    assert_eq!(at(25_000), Difficulty::Middle);
    assert_eq!(at(100_000), Difficulty::Middle);
    assert_eq!(at(100_001), Difficulty::Easy);
    assert_eq!(Difficulty::Hard.string(), 11538);
    assert_eq!(Difficulty::Easy.string(), 11536);
}

#[test]
fn a_typed_amount_loses_the_symbols_and_is_limited() {
    let typed = |text: &[u8]| {
        let mut c = Cash::from_settings(&ini("[UI]\nMSStartingCash=75000\nMSMinCash=10000\n"));
        c.typed(text, "$");
        c.amount
    };
    assert_eq!(typed(b"$195,000"), 195_000);
    assert_eq!(typed(b"195000"), 195_000);
    assert_eq!(typed(b"1"), 10_000);
    assert_eq!(typed(b""), 10_000);
    assert_eq!(typed(b"-300"), 10_000);
    assert_eq!(typed(b"99999999"), 500_000);
    // Too many digits wrap around, as the original's conversion does; nothing panics.
    assert!((10_000..=500_000).contains(&typed(b"99999999999999999999")));
    assert_eq!(typed(b"1.5e9"), 10_000);
    let mut c = Cash::from_settings(&ini(""));
    c.typed(b"CHF 20000", "CHF ");
    assert_eq!(c.amount, 20_000);
}
