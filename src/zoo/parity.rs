//! Re-save parity of the shipped files (issues 1037, 1439): each map and saved game of version 105
//! or 106 is opened as Continue does and written back before the first step, and the file is
//! compared section by section with the file the original would write (save games 15): the source
//! with the changes the original makes too, [`expected`]. A file may differ by [`BUDGET`] at most;
//! it is empty, so every file is within [`DEFAULT`].

use super::*;
use crate::goal::USE;
use crate::{animal::Class, exhibit::TankConfig, place, sim::drive::Run, terrain, world::Decay};
use std::collections::BTreeMap;

/// Per file, the most bytes that differ and the most the size may change by, for a file that the
/// original's changes cannot explain.
const BUDGET: &[(&str, usize, usize)] = &[];

/// What a file without a pin may differ by: the version word, the clocks a Continue start still
/// touches (the three status timers) and the like.
const DEFAULT: (usize, usize) = (64, 0);

/// Files the writer refuses today (none); one that writes is measured against [`DEFAULT`] unless pinned.
const REFUSED: &[&str] = &[];

/// What the original changes in a file that the expected file applies, per file (the rest hold 0):
/// the records of a type the game does not have (15.2), the events posted at load (15.4) and how many
/// times the longevity research lengthens the animals (15.5).
#[derive(Debug, Default, PartialEq, Eq)]
struct Changes {
    dropped: usize,
    events: usize,
    lengthened: u32,
}

impl Changes {
    const NONE: Changes = Changes {
        dropped: 0,
        events: 0,
        lengthened: 0,
    };
}

const CHANGES: &[(&str, Changes)] = &[
    (
        "maps/lg_resrt.zoo",
        Changes {
            events: 4,
            ..Changes::NONE
        },
    ),
    (
        "maps/lg_sandy.zoo",
        Changes {
            dropped: 1,
            ..Changes::NONE
        },
    ),
    (
        "maps/lg_sub.zoo",
        Changes {
            dropped: 26,
            ..Changes::NONE
        },
    ),
    (
        "maps/med_kids.zoo",
        Changes {
            lengthened: 1,
            ..Changes::NONE
        },
    ),
    (
        "maps/scn28.zoo",
        Changes {
            events: 2,
            ..Changes::NONE
        },
    ),
    (
        "maps/scn28b.zoo",
        Changes {
            events: 1,
            ..Changes::NONE
        },
    ),
    (
        "maps/scn31.zoo",
        Changes {
            dropped: 2,
            events: 8,
            ..Changes::NONE
        },
    ),
    (
        "maps/scn33.zoo",
        Changes {
            dropped: 1,
            ..Changes::NONE
        },
    ),
    (
        "maps/scn36.zoo",
        Changes {
            events: 8,
            ..Changes::NONE
        },
    ),
    (
        "maps/scn37.zoo",
        Changes {
            dropped: 4,
            ..Changes::NONE
        },
    ),
    (
        "saved games/test.zoo",
        Changes {
            events: 2,
            lengthened: 2,
            ..Changes::NONE
        },
    ),
];

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

/// What the longevity research (aquatic species, way 2) adds to a life, in per cent.
const LONGEVITY: i32 = 25;

/// The file the original writes when `source` is opened with Continue and saved before the first
/// step (save games 15), and what it changed. `engine` is the engine's save of the file: the
/// bytes the original draws at random or takes from the running game, which a comparison leaves
/// out (the data word of a posted event, the places of the guests seated again, the research
/// section and the scenario block of the running game, 15.10), are taken from it.
fn expected(
    source: &[u8],
    cat: &Catalogue,
    engine: &[u8],
    language: u32,
    changes: &Changes,
) -> Result<(Vec<u8>, Changes), String> {
    let (mut zoo, money) = Zoo::parse_money_in(source, cat).map_err(|e| e.to_string())?;
    let (theirs, _) = Zoo::parse_money_in(engine, cat).map_err(|e| e.to_string())?;
    let mut made = Changes {
        lengthened: changes.lengthened,
        ..Changes::default()
    };
    // 15.2: the records of the type `jeep1`, which the update archive's scenery file lacks, are
    // gone (the parse drops them; they are counted from the file as it is).
    let all = Zoo::parse(source).map_err(|e| e.to_string())?;
    let jeep =
        |e: &Entity| (&*e.class, &*e.type_name, &*e.subtype) == ("objects", "other", "jeep1");
    made.dropped = all.entities.iter().filter(|e| jeep(e)).count();
    // 15.4 point 1: the pieces at or below their `cDecayedLife` post 10033 (10034 when electrified)
    // in the next buffer, in entity list order.
    let mut events = Vec::new();
    for e in &zoo.entities {
        let Some((c, f)) = e.common.as_ref().and_then(|c| Some((c, c.fence?))) else {
            continue;
        };
        let ty = crate::place::bulldozer::type_of(cat, e);
        let key = |name| {
            ty.and_then(|t| crate::build::number(t, name))
                .unwrap_or(0.0)
        };
        let kind = if f.life < 1 {
            // A piece that can break is broken again.
            ty.is_some_and(crate::world::deteriorates).then_some(10034)
        } else if i64::from(f.life) <= key("cDecayedLife") as i64 {
            Some(if key("cIsElectrified") != 0.0 {
                10034
            } else {
                10033
            })
        } else {
            None
        };
        if let Some(kind) = kind {
            events.push(crate::world::piece_event(c.id, kind, 0));
        }
    }
    // The engine's file holds those posted at load behind the stored ones. The pieces' events
    // must be the rule's, in its order; the data word is an address in the original (15.4 point 2),
    // and the dung (10032) and filter (10035, 10036) events of the other after-load steps are
    // taken from the engine's file, their number pinned.
    let queue = &mut zoo.clocks.queue;
    let first = queue.next_is_first;
    let next = if first {
        &mut queue.first
    } else {
        &mut queue.second
    };
    let posted = &theirs.clocks.queue;
    let posted = if first { &posted.first } else { &posted.second };
    let posted = posted.get(next.len()..).unwrap_or_default().to_vec();
    let pieces = |events: &[Event]| -> Vec<(u32, u16)> {
        let pieces = events.iter().filter(|e| matches!(e.kind, 10033 | 10034));
        pieces.map(|e| (e.sender, e.kind)).collect()
    };
    if pieces(&posted) != pieces(&events) {
        return Err(format!(
            "pieces: {:?}, the rule gives {:?}",
            pieces(&posted),
            pieces(&events)
        ));
    }
    made.events = posted.len();
    next.extend(posted);
    // 15.5: the life is cut to the type's `cTimeDeath - 1`, then lengthened by every completion.
    for e in &mut zoo.entities {
        let ty = crate::place::bulldozer::type_of(cat, e);
        let Some(animal) = e.common.as_mut().and_then(|c| c.animal.as_mut()) else {
            continue;
        };
        let (Some(life), Some(death)) = (
            animal.life,
            ty.and_then(|t| crate::build::number(t, "cTimeDeath")),
        ) else {
            continue;
        };
        let mut life = life.min(death as i32 - 1);
        let aquatic = ty.and_then(|t| crate::build::plain(t, "cExpansionID")) == Some(2.0);
        for _ in 0..changes.lengthened * u32::from(aquatic) {
            if life > 0 {
                life = (i64::from(life) * i64::from(LONGEVITY) / 100) as i32 + life;
            }
        }
        animal.life = Some(life);
    }
    // 15.6: the guests using a building (a grandstand seats them again at random places) are
    // left at the places of the engine's file.
    let places: BTreeMap<u32, (i32, i32, i32)> = theirs
        .entities
        .iter()
        .filter_map(|e| e.common.as_ref())
        .map(|c| (c.id, (c.x, c.y, c.z)))
        .collect();
    for c in zoo.entities.iter_mut().filter_map(|e| e.common.as_mut()) {
        let using = c
            .unit
            .as_ref()
            .is_some_and(|u| u.target.goal == i32::from(USE));
        if let (true, Some(&at)) = (c.guest.is_some() && using, places.get(&c.id)) {
            (c.x, c.y, c.z) = at;
        }
    }
    // A file of another language than the install's has every entity and habitat named again in the
    // install's (localisation 10): the names are the engine's.
    if zoo.locale != language {
        let names: BTreeMap<u32, &str> = (theirs.entities.iter())
            .filter_map(|e| e.common.as_ref())
            .map(|c| (c.id, &*c.name))
            .collect();
        for c in zoo.entities.iter_mut().filter_map(|e| e.common.as_mut()) {
            if let Some(name) = names.get(&c.id) {
                c.name = name.to_string();
            }
        }
    }
    if zoo.locale != language && zoo.habitat_info.len() == theirs.habitat_info.len() {
        for (a, b) in zoo.habitat_info.iter_mut().zip(&theirs.habitat_info) {
            a.name = b.name.clone();
        }
    }
    // 15.7: the load adds two to the next show id per show tank (each has a show info).
    let tanks = zoo.shows.infos.iter().flatten().count();
    zoo.shows.next_show = zoo.shows.next_show.wrapping_add(2 * tanks as u16);
    // 15.10: the original rewrites the research section and the scenario block.
    zoo.kept.research = theirs.kept.research.clone();
    zoo.kept.scenario = theirs.kept.scenario.clone();
    Ok((zoo.write(&money)?, made))
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

/// A file of version 105 or 106 saved back: the report of its sections, the bytes that differ
/// from the expected file and the change of size; `Err` when the writer refuses it or the file
/// does not have the changes of [`CHANGES`]; `None` for another version.
fn measure(vfs: &crate::vfs::Vfs, member: &str) -> Option<Result<(String, usize, isize), String>> {
    let old = vfs.read(member).unwrap();
    let version = Zoo::parse(&old).map_or(0, |z| z.version);
    if !(105..=106).contains(&version) {
        return None;
    }
    let cat = Catalogue::load(vfs).unwrap();
    let language = crate::strings::Language::of(vfs.root()).number;
    let new = match resaved(vfs, Catalogue::load(vfs).unwrap(), member) {
        Ok(new) => new,
        Err(why) => return Some(Err(format!("{member}: v{version} refused: {why}"))),
    };
    let pinned = CHANGES.iter().find(|c| c.0 == member).map(|c| &c.1);
    let (want, _) = (pinned.unwrap_or(&Changes::NONE), ());
    let (expect, made) = match expected(&old, &cat, &new, language, want) {
        Ok(done) => done,
        Err(why) => return Some(Err(format!("{member}: v{version} no expected file: {why}"))),
    };
    if &made != want {
        return Some(Err(format!(
            "{member}: changes {made:?}, the spec says {want:?}"
        )));
    }
    let by = compare(&expect, &new);
    let bytes = by.values().map(|v| v.0).sum::<usize>();
    let size = new.len() as isize - expect.len() as isize;
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
