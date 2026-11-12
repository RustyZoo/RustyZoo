//! Re-save parity of the shipped files (issues 1037, 1439): each map and saved game of version 105
//! or 106 is opened as Continue does and written back before the first step, and the file is
//! compared section by section with the file the original would write (save games 15): the source
//! with the changes the original makes too, built from its bytes by [`expected`] (not by the
//! engine's writer). Nothing may differ but what [`allowed`] names.

use super::*;
use crate::{animal::Class, exhibit::TankConfig, place, sim::drive::Run, terrain, world::Decay};
use expected::{Changes, Inputs, build, ranges};
use std::ops::Range;

mod expected;

/// What the spec says the original changes in a file, per file (the rest hold none): the records
/// of a type the game does not have (15.2), the events posted at load by kind (15.4), the times the
/// longevity research lengthens the animals (15.5) and the size of the scenario block the original
/// writes (15.10).
struct Pin {
    dropped: usize,
    events: &'static [(u16, usize)],
    lengthened: u32,
    scenario: Option<usize>,
}

const NONE: Pin = Pin {
    dropped: 0,
    events: &[],
    lengthened: 0,
    scenario: None,
};

const PINS: &[(&str, Pin)] = &[
    (
        "maps/lg_resrt.zoo",
        Pin {
            events: &[(10033, 4)],
            ..NONE
        },
    ),
    ("maps/lg_sandy.zoo", Pin { dropped: 1, ..NONE }),
    (
        "maps/lg_sub.zoo",
        Pin {
            dropped: 26,
            ..NONE
        },
    ),
    (
        "maps/med_kids.zoo",
        Pin {
            lengthened: 1,
            ..NONE
        },
    ),
    (
        "maps/scn28.zoo",
        Pin {
            events: &[(10032, 2)],
            ..NONE
        },
    ),
    (
        "maps/scn28b.zoo",
        Pin {
            events: &[(10030, 1)],
            scenario: Some(885),
            ..NONE
        },
    ),
    (
        "maps/scn31.zoo",
        Pin {
            dropped: 2,
            events: &[(10030, 1), (10034, 7)],
            ..NONE
        },
    ),
    (
        "maps/scn33.zoo",
        Pin {
            dropped: 1,
            scenario: Some(885),
            ..NONE
        },
    ),
    (
        "maps/scn36.zoo",
        Pin {
            events: &[(10030, 2), (10033, 1), (10036, 5)],
            scenario: Some(889),
            ..NONE
        },
    ),
    ("maps/scn37.zoo", Pin { dropped: 4, ..NONE }),
    (
        "saved games/test.zoo",
        Pin {
            events: &[(10030, 1), (10032, 1)],
            lengthened: 2,
            ..NONE
        },
    ),
];

/// The size of the research section the original writes after a load, in every file (15.10).
const RESEARCH: usize = 1694;

/// A section of one file against the same section of the other: the offsets of the bytes that
/// differ, and the change of size.
struct Diff {
    name: String,
    at: Vec<usize>,
    size: isize,
}

/// The sections of both files side by side, with the ranges of `masked` (in `expect`) zeroed in
/// both. `Err` when the two files do not have the same sections.
fn diff(expect: &[u8], engine: &[u8], masked: &[Range<usize>]) -> Result<Vec<Diff>, String> {
    let (a, b) = (ranges(expect)?, ranges(engine)?);
    let names = |s: &[(String, Range<usize>)]| s.iter().map(|s| s.0.clone()).collect::<Vec<_>>();
    if names(&a) != names(&b) {
        let sizes = |s: &[(String, Range<usize>)]| {
            let sizes: Vec<_> = s.iter().map(|s| (s.0.clone(), s.1.len())).collect();
            format!(
                "{} sections {:?}",
                sizes.len(),
                sizes.iter().find(|n| n.0.starts_with("entity"))
            )
        };
        return Err(format!(
            "the sections differ: {} against {}",
            sizes(&a),
            sizes(&b)
        ));
    }
    let mut out = Vec::new();
    for ((name, ra), (_, rb)) in a.iter().zip(&b) {
        let (mut x, mut y) = (expect[ra.clone()].to_vec(), engine[rb.clone()].to_vec());
        for m in masked
            .iter()
            .filter(|m| m.start >= ra.start && m.end <= ra.end)
        {
            let rel = m.start - ra.start..m.end - ra.start;
            x[rel.clone()].fill(0);
            if let Some(y) = y.get_mut(rel) {
                y.fill(0);
            }
        }
        let at: Vec<usize> = (0..x.len().min(y.len()))
            .filter(|&i| x[i] != y[i])
            .collect();
        let size = y.len() as isize - x.len() as isize;
        out.push(Diff {
            name: name.clone(),
            at,
            size,
        });
    }
    Ok(out)
}

/// Whether the byte `at` of section `name` of the expected file `section` may differ (save games
/// 15.1 point 3): the show id counter, the last two bytes of `shows`; and in `finances` the load's
/// own rating check: the first status timer, the rating, and the current month, the current year
/// and the total of the zoo value, the rating, the animal happiness, the guest happiness and the
/// guest count.
fn allowed(name: &str, section: &[u8], at: usize) -> bool {
    match name {
        "shows" => at + 2 >= section.len(),
        "finances" => {
            let word = |o: usize| u32_at(section, o) as usize;
            let (rows, columns) = (word(45), word(49));
            let (month, year) = (word(37).clamp(1, 12) - 1, word(41).min(columns - 1));
            let cell = |table: usize, n: usize| 53 + 4 * (table + n);
            (4..8).contains(&at)
                || (17..21).contains(&at)
                || [15, 16, 17, 18, 25].iter().any(|&r| {
                    let cells = [
                        cell(0, r * 12 + month),
                        cell(rows * 12, r * columns + year),
                        cell(rows * (12 + columns), r),
                    ];
                    cells.iter().any(|&c| (c..c + 4).contains(&at))
                })
        }
        _ => false,
    }
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The file `member` of the install opened as Continue does and saved back at once, or why it is
/// refused.
fn resaved(vfs: &crate::vfs::Vfs, cat: Catalogue, member: &str) -> Result<Vec<u8>, String> {
    crate::app::continued_save(vfs, &cat, &vfs.read(member).unwrap())
}

/// The file `member` saved back by a run started on it (a new game's start) after `before` has
/// changed the run: the parity measure steps no run, a test of a step does.
fn resaved_after(
    vfs: &crate::vfs::Vfs,
    cat: Catalogue,
    member: &str,
    before: impl FnOnce(&mut Run),
) -> Result<Vec<u8>, String> {
    let (zoo, money) = Zoo::parse_money_in(&vfs.read(member).unwrap(), &cat).unwrap();
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

/// A file of version 105 or 106 saved back: the report of what differs from the expected file
/// outside what [`allowed`] names, and whether anything does; `Err` when the writer refuses the
/// file or it does not have the changes of [`PINS`]; `None` for another version.
fn measure(vfs: &crate::vfs::Vfs, member: &str) -> Option<Result<(String, bool), String>> {
    let old = vfs.read(member).unwrap();
    let version = Zoo::parse(&old).map_or(0, |z| z.version);
    if !(105..=106).contains(&version) {
        return None;
    }
    Some(check(vfs, member, &old, version))
}

fn check(
    vfs: &crate::vfs::Vfs,
    member: &str,
    old: &[u8],
    version: u32,
) -> Result<(String, bool), String> {
    let cat = Catalogue::load(vfs).unwrap();
    let language = crate::strings::Language::of(vfs.root()).number;
    let new = resaved(vfs, Catalogue::load(vfs).unwrap(), member)
        .map_err(|why| format!("{member}: v{version} refused: {why}"))?;
    let pin = PINS.iter().find(|p| p.0 == member).map_or(&NONE, |p| &p.1);
    let inputs = Inputs {
        vfs,
        cat: &cat,
        source: old,
        engine: &new,
        language,
        changes: Changes {
            lengthened: pin.lengthened,
        },
    };
    let built =
        build(&inputs).map_err(|why| format!("{member}: v{version} no expected file: {why}"))?;
    let events: Vec<(u16, usize)> = built.events.iter().map(|(k, n)| (*k, *n)).collect();
    if (built.dropped, &events[..]) != (pin.dropped, pin.events) {
        return Err(format!(
            "{member}: {} records dropped and the events {events:?}, the spec says {} and {:?}",
            built.dropped, pin.dropped, pin.events
        ));
    }
    let by = diff(&built.bytes, &new, &built.masked).map_err(|why| format!("{member}: {why}"))?;
    let expect = ranges(&built.bytes)?;
    let mut report = format!("{member}: v{version}");
    let mut bad = false;
    for (d, (_, range)) in by.iter().zip(&expect) {
        let section = &built.bytes[range.clone()];
        let loose: Vec<usize> =
            d.at.iter()
                .copied()
                .filter(|&at| !allowed(&d.name, section, at))
                .collect();
        if !loose.is_empty() || d.size != 0 {
            bad = true;
            report += &format!(
                "\n    {}: {} bytes differ, first at {:?}, size {:+}",
                d.name,
                loose.len(),
                &loose[..loose.len().min(8)],
                d.size
            );
        }
    }
    // The sizes of the two sections the original rewrites are pinned (15.10).
    let engine = ranges(&new)?;
    let size_of = |name: &str| engine.iter().find(|s| s.0 == name).map(|s| s.1.len());
    // The scenario sizes were observed on the English install.
    let scenario = pin.scenario.filter(|_| language == 1033);
    for (name, want) in [("research", Some(RESEARCH)), ("scenario", scenario)] {
        if want.is_some() && size_of(name) != want {
            bad = true;
            report += &format!(
                "\n    {name}: {:?} bytes, the original writes {want:?}",
                size_of(name)
            );
        }
    }
    Ok((report, bad))
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
    // The files of the English and French installs are measured; the others differ in what they
    // store (their language, for one), and a base-only install has few of version 105 and 106:
    // they are measured and printed.
    let measured = crate::fixtures::is_measured(&dir);
    let (mut failed, mut checked) = (Vec::new(), 0);
    // Four files at a time: a run loads the whole catalogue.
    for group in members.chunks(4) {
        let done: Vec<_> = std::thread::scope(|s| {
            let jobs: Vec<_> = group.iter().map(|m| s.spawn(|| measure(&vfs, m))).collect();
            jobs.into_iter().map(|j| j.join().unwrap()).collect()
        });
        for done in done.into_iter().flatten() {
            match done {
                Err(why) => {
                    println!("{why}");
                    failed.push(why);
                }
                Ok((report, bad)) => {
                    println!("{report}");
                    checked += 1;
                    if bad {
                        failed.push(report);
                    }
                }
            }
        }
    }
    assert!(checked >= 10 || !measured, "{checked} files measured");
    assert!(
        failed.is_empty() || !measured,
        "differs:\n{}",
        failed.join("\n")
    );
}

#[test]
fn what_differs_is_found_by_section_and_the_masked_bytes_are_left_out() {
    let file = super::tests::lean(106);
    assert!(
        diff(&file, &file, &[])
            .unwrap()
            .iter()
            .all(|d| d.at.is_empty() && d.size == 0)
    );
    let names: Vec<_> = ranges(&file).unwrap().into_iter().map(|s| s.0).collect();
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
    let by = diff(&file, &other, &[]).unwrap();
    let find = |name: &str| by.iter().find(|d| d.name == name).unwrap();
    assert_eq!(
        (find("tiles").at.clone(), find("tiles").size),
        (
            vec![
                file[..at].len()
                    - 1
                    - ranges(&file)
                        .unwrap()
                        .iter()
                        .find(|s| s.0 == "tiles")
                        .unwrap()
                        .1
                        .start
            ],
            0
        )
    );
    assert_eq!(find("tail").size, 2);
    // The byte is left out when it is masked.
    let masked = diff(&file, &other, std::slice::from_ref(&(at - 1..at))).unwrap();
    assert!(masked.iter().all(|d| d.at.is_empty()));
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

/// The records of a file as the game would list them again: the first tile, the gate, the figures
/// and the water of each, and whether it holds a show (the names only in the language of the install).
fn records(zoo: &Zoo, named: bool) -> Vec<String> {
    let mut tanks = zoo.tanks.iter().peekable();
    let mut out = Vec::new();
    for (n, ((tile, gate), info)) in (zoo.habitat_tiles.iter().zip(&zoo.habitat_gates))
        .zip(&zoo.habitat_info)
        .enumerate()
    {
        let tank = tanks
            .next_if(|t| t.tile == *tile)
            .map(|t| (t.height, t.level, t.salt, t.purity));
        let name = if named { info.name.as_str() } else { "" };
        let (stats, dates) = (info.stats, (info.created, info.viewing, info.serviced));
        let show = zoo.shows.info(n).is_some();
        out.push(format!(
            "{tile:?} {gate:?} {name} {stats:?} {dates:?} {tank:?} {show}"
        ));
    }
    out
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn every_shipped_file_that_loads_is_saved_back_with_its_habitat_records() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let language = crate::strings::Language::of(vfs.root()).number;
    let members: Vec<_> = vfs
        .list(None)
        .into_iter()
        .filter(|m| m.ends_with(".zoo"))
        .collect();
    let (mut failed, mut checked) = (Vec::new(), 0);
    for group in members.chunks(4) {
        let done: Vec<_> = std::thread::scope(|s| {
            let jobs: Vec<_> = group
                .iter()
                .map(|m| {
                    s.spawn(|| {
                        let old = vfs.read(m).unwrap();
                        let cat = Catalogue::load(&vfs).unwrap();
                        let new = resaved(&vfs, Catalogue::load(&vfs).unwrap(), m).ok()?;
                        let (a, b) = (
                            Zoo::parse_in(&old, &cat).ok()?,
                            Zoo::parse_in(&new, &cat).ok()?,
                        );
                        let named = a.locale == language;
                        let same = records(&a, named) == records(&b, named);
                        Some((m.clone(), same))
                    })
                })
                .collect();
            jobs.into_iter().map(|j| j.join().unwrap()).collect()
        });
        for (member, same) in done.into_iter().flatten() {
            checked += 1;
            if !same {
                failed.push(member);
            }
        }
    }
    println!("{checked} files that load");
    assert!(failed.is_empty(), "the records differ: {failed:?}");
}
