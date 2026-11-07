//! Re-save parity of the shipped files (issue 1037): each map and saved game of version 105 or 106
//! is loaded as `sim` does and written back before the first step, and the file is compared with
//! the original section by section. The differences are pinned in [`BUDGET`]; a file may differ
//! less, never more, so lower a pin when a section is fixed.

use super::*;
use crate::{animal::Class, exhibit::TankConfig, place, sim::drive::Run, terrain, world::Decay};
use std::collections::BTreeMap;

/// Per file: the most bytes that differ (section by section, a section that changed its size
/// counting the bytes added or lost) and the most the size may change by, measured on the full
/// game and on the French install (the larger of the two).
const BUDGET: &[(&str, usize, usize)] = &[
    ("maps/lg_resrt.zoo", 115, 80),
    ("maps/lg_sandy.zoo", 8689, 108),
    ("maps/lg_sub.zoo", 61463, 2808),
    ("maps/med_kids.zoo", 758, 56),
    ("maps/scn28.zoo", 397, 40),
    ("maps/scn31.zoo", 5463, 69),
    ("maps/scn33.zoo", 717, 113),
    ("maps/scn36.zoo", 831, 166),
    ("maps/scn37.zoo", 2310, 452),
    ("maps/scn42.zoo", 1602, 1517),
    ("saved games/test.zoo", 353, 78),
];

/// What a file without a pin may differ by: the version word, counters and clocks.
const DEFAULT: (usize, usize) = (64, 0);

/// Files the writer refuses today; one that writes is measured against [`DEFAULT`] unless pinned.
const REFUSED: &[&str] = &["maps/scn28b.zoo"];

/// The sections of a file as (name, bytes); the name of an entity is `entity/CLASS`, and what
/// follows the last section is `tail`. Equal names are paired in order across two files.
fn sections(bytes: &[u8]) -> Vec<(String, &[u8])> {
    let mut at = 0;
    let mut out = Vec::new();
    for (name, end) in Zoo::sections(bytes).unwrap() {
        out.push((name, &bytes[at..end]));
        at = end;
    }
    out.push(("tail".into(), &bytes[at..]));
    out
}

/// Per section name, over the sections paired in order: the bytes that differ and the size change.
fn compare(old: &[u8], new: &[u8]) -> BTreeMap<String, (usize, isize)> {
    let (old, mut new) = (sections(old), sections(new));
    let mut out: BTreeMap<String, (usize, isize)> = BTreeMap::new();
    let mut take = |name: &str| {
        let at = new.iter().position(|(n, _)| n == name);
        at.map_or(&[][..], |i| new.remove(i).1)
    };
    for (name, a) in &old {
        let b = take(name);
        let same = a.iter().zip(b).filter(|(x, y)| x == y).count();
        let e = out.entry(name.clone()).or_default();
        e.0 += a.len().max(b.len()) - same;
        e.1 += b.len() as isize - a.len() as isize;
    }
    for (name, b) in new {
        let e = out.entry(name).or_default();
        (e.0, e.1) = (e.0 + b.len(), e.1 + b.len() as isize);
    }
    out
}

/// The file `member` of the install saved back by a run started on it, or why it is refused.
fn resaved(vfs: &crate::vfs::Vfs, cat: Catalogue, member: &str) -> Result<Vec<u8>, String> {
    resaved_after(vfs, cat, member, |_| {})
}

/// [`resaved`] after `before` has changed the started run (the parity measure steps no run).
fn resaved_after(
    vfs: &crate::vfs::Vfs,
    cat: Catalogue,
    member: &str,
    before: impl FnOnce(&mut Run),
) -> Result<Vec<u8>, String> {
    let (zoo, money) = Zoo::parse_money(&vfs.read(member).unwrap())
        .and_then(|(zoo, money)| Ok((zoo.checked(&cat)?, money)))
        .unwrap();
    let renderer = crate::render::MapRenderer::new(vfs, &cat);
    let language = crate::strings::Language::of(vfs.root());
    let mut world = crate::world::World::with_language(zoo, |e| renderer.shape(e), language);
    world.set_water(place::water(&terrain::types(vfs)));
    world.set_tank_config(TankConfig::load(vfs));
    world.units.show = crate::show::Params::load(vfs);
    world.set_decay(Decay::load(vfs));
    let economy = vfs.read("economy.cfg").unwrap_or_default();
    let types = (cat, Class::load(vfs));
    let mut run = Run::start((vfs.ini(), &economy), types, world, money, 1, false);
    before(&mut run);
    run.save()
}

/// A file of version 105 or 106 saved back: the report of its sections, the bytes that differ and
/// the change of size; `Err` when the writer refuses it; `None` for another version.
fn measure(vfs: &crate::vfs::Vfs, member: &str) -> Option<Result<(String, usize, isize), String>> {
    let old = vfs.read(member).unwrap();
    let version = Zoo::parse(&old).map_or(0, |z| z.version);
    if !(105..=106).contains(&version) {
        return None;
    }
    let new = match resaved(vfs, Catalogue::load(vfs).unwrap(), member) {
        Ok(new) => new,
        Err(why) => return Some(Err(format!("{member}: v{version} refused: {why}"))),
    };
    let by = compare(&old, &new);
    let bytes = by.values().map(|v| v.0).sum::<usize>();
    let size = new.len() as isize - old.len() as isize;
    let mut report = format!("{member}: v{version} bytes {bytes} size {size:+}");
    let mut worst: Vec<_> = by.iter().filter(|(_, v)| v.0 > 0).collect();
    worst.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
    for (name, (bytes, size)) in worst {
        report += &format!("\n    {name}: bytes {bytes} size {size:+}");
    }
    Some(Ok((report, bytes, size)))
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn every_shipped_file_is_saved_back_within_its_budget() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let members: Vec<_> = vfs
        .list(None)
        .into_iter()
        .filter(|m| m.ends_with(".zoo"))
        .collect();
    // The budgets were measured on the English and French installs: the files of the others
    // differ in what they store (their language, for one), and a base-only install has few
    // of version 105 and 106. They are measured and printed, not held to the budgets.
    let measured = crate::fixtures::is_measured(&dir);
    let (mut failed, mut checked) = (Vec::new(), 0);
    // Four files at a time: a run loads the whole catalogue.
    for group in members.chunks(4) {
        let done: Vec<_> = std::thread::scope(|s| {
            let jobs: Vec<_> = group.iter().map(|m| s.spawn(|| measure(&vfs, m))).collect();
            jobs.into_iter().map(|j| j.join().unwrap()).collect()
        });
        for (member, done) in group.iter().zip(done) {
            match done {
                None => {}
                Some(Err(why)) => {
                    println!("{why}");
                    if measured && !REFUSED.contains(&member.as_str()) {
                        failed.push(why);
                    }
                }
                Some(Ok((report, bytes, size))) => {
                    println!("{report}");
                    checked += 1;
                    let pin = BUDGET.iter().find(|b| b.0 == member.as_str());
                    let (max_bytes, max_size) = pin.map_or(DEFAULT, |b| (b.1, b.2));
                    if measured && (bytes > max_bytes || size.unsigned_abs() > max_size) {
                        failed.push(format!(
                            "{report}\n  budget: bytes {max_bytes} size +-{max_size}"
                        ));
                    }
                }
            }
        }
    }
    assert!(checked >= 10 || !measured, "{checked} files measured");
    assert!(failed.is_empty(), "over budget:\n{}", failed.join("\n"));
}

#[test]
fn what_differs_is_counted_by_section() {
    let file = super::tests::lean(106);
    assert!(compare(&file, &file).values().all(|v| *v == (0, 0)));
    let names: Vec<_> = sections(&file).into_iter().map(|s| s.0).collect();
    assert!(names.contains(&"tiles".into()) && names.last().unwrap() == "tail");
    let at = Zoo::sections(&file)
        .unwrap()
        .iter()
        .find(|s| s.0 == "tiles")
        .unwrap()
        .1;
    // One byte of the last tile changed, and two bytes added after the last section.
    let mut other = file.clone();
    other[at - 1] ^= 1;
    other.extend([0, 0]);
    let by = compare(&file, &other);
    assert_eq!((by["tiles"], by["tail"]), ((1, 0), (2, 2)));
    assert_eq!(by.values().map(|v| v.0).sum::<usize>(), 3);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn a_stepped_scn28_save_holds_the_record_that_the_specialists_need_test_made() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    if !crate::fixtures::has_member("a_stepped_scn28_save", &vfs, "maps/scn28.zoo") {
        return;
    }
    let old = Zoo::parse(&vfs.read("maps/scn28.zoo").unwrap()).unwrap();
    let key = 9552;
    let mut now = 0;
    let saved = resaved_after(
        &vfs,
        Catalogue::load(&vfs).unwrap(),
        "maps/scn28.zoo",
        |run| {
            // A show whose keeper type is not set yet (a new tank, or after a stop), no record of him.
            let info = run
                .world
                .zoo
                .shows
                .infos
                .iter_mut()
                .flatten()
                .next()
                .unwrap();
            info.records.retain(|r| r.key != key);
            info.show.keeper = 0;
            // He is called in the first steps: the step that makes the record dates it.
            for _ in 0..5 {
                run.step(100);
                let info = run.world.zoo.shows.infos.iter().flatten().next().unwrap();
                if info.record(key).is_some() {
                    break;
                }
            }
            now = run.world.units.now;
        },
    )
    .unwrap();
    let new = Zoo::parse(&saved).unwrap();
    let (old, new) = (
        old.shows.infos.iter().flatten().next().unwrap(),
        new.shows.infos.iter().flatten().next().unwrap(),
    );
    let keys: Vec<_> = new.records.iter().map(|r| r.key).collect();
    assert!(keys.is_sorted(), "{keys:?}");
    let made = new.record(key).expect("the record of the specialist type");
    assert_eq!(
        made,
        &TypeRecord::new(key, u16::MAX, crate::timing::file_time(now))
    );
    assert_eq!(made.date, crate::timing::file_time(now));
    // The other records are the file's.
    let rest = |i: &ShowInfo| {
        i.records
            .iter()
            .filter(|r| r.key != key)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(rest(new), rest(old));
}
