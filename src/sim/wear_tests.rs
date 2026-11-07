//! Fence wear in the step driver (placing 7.4): the interval, the steps, the hash.

use super::drive::Run;
use crate::animal::Class;
use crate::entity::{Catalogue, EntityType};
use crate::fixtures::TempDir;
use crate::money::Money;
use crate::unit::testing::{entity, world_with};
use crate::world::{Decay, World};

/// A fence type `iron` that can deteriorate, with the given characteristics.
fn types(keys: &str) -> Catalogue {
    types_with("[HabitatFence]\n", keys)
}

/// The same with more `sections` (text of other blocks) before the characteristics.
fn types_with(sections: &str, keys: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    for subtype in [&b"f"[..], b"g"] {
        cat.push(EntityType {
            class: "fences",
            type_name: b"iron".to_vec(),
            subtype: subtype.to_vec(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(
                    format!("{sections}[Characteristics/Integers]\n{keys}\n").as_bytes(),
                )
                .unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

const WEARING: &str = "cStrength=300\ncLife=3\ncDecayedLife=2\ncDecayDelta=100";

/// A world with one `iron` fence piece (id 7) of the given subtype, drawing from 1000 to 3000 ms.
fn world(subtype: &str) -> World {
    let mut world = world_with((4, 4), |zoo| {
        let mut piece = entity("fences", "iron", 7, (96, 64), 0);
        piece.subtype = subtype.into();
        zoo.entities.push(piece);
    });
    world.set_decay(Decay {
        min: 1000,
        max: 3000,
    });
    world
}

fn run(cat: Catalogue, world: World, seed: u32) -> Run {
    Run::new(
        (b"", b""),
        (cat, Class::default()),
        world,
        Money::default(),
        seed,
    )
}

fn fence(run: &Run) -> crate::zoo::Fence {
    run.world.zoo.entities[0]
        .common
        .as_ref()
        .unwrap()
        .fence
        .unwrap()
}

#[test]
fn the_interval_is_the_lower_bound_plus_the_rounded_share_of_the_range() {
    let d = |min, max| Decay { min, max };
    let at = |decay: Decay, draw| decay_interval(decay, draw);
    assert_eq!(at(d(1000, 3000), 0), 1000);
    assert_eq!(at(d(1000, 3000), 0x7fff), 3000);
    assert_eq!(at(d(1000, 3000), 41), 1003);
    assert_eq!(at(d(480_000, 1_200_000), 18467), 480_000 + 405_781);
    // Single precision all along: 881 and 3093 differ by 1 from the `/ 32767` form.
    assert_eq!(at(d(480_000, 1_200_000), 881), 480_000 + 19358);
    assert_eq!(at(d(480_000, 1_200_000), 3093), 480_000 + 67964);
    // Equal bounds, upside-down bounds, zero and huge ones, a draw out of range.
    assert_eq!(at(d(500, 500), 9000), 500);
    assert_eq!(at(d(3000, 1000), 0x7fff), 1000);
    assert_eq!(at(d(0, 0), 5), 0);
    assert_eq!(at(d(u32::MAX, u32::MAX), 0x7fff), u32::MAX);
    assert_eq!(at(d(0, u32::MAX), u32::MAX), u32::MAX);
}

/// The interval a piece draws, through [`World::heal`].
fn decay_interval(decay: Decay, draw: u32) -> u32 {
    let cat = types(WEARING);
    let mut w = world("f");
    w.set_decay(decay);
    w.heal(&cat, 7, &mut || draw).unwrap()
}

#[test]
fn a_piece_goes_healthy_then_decayed_then_broken_at_the_drawn_times() {
    let mut run = run(types(WEARING), world("f"), 1);
    let healthy = fence(&run);
    assert_eq!((healthy.strength, healthy.life), (300, 3));
    let interval = healthy.timers[0];
    assert!((1000..=3000).contains(&interval) && healthy.timers[1] == interval);
    // The first draw of the seed is 41 of 32767: 1003 ms.
    assert_eq!(interval, 1003);
    let mut log = Vec::new();
    let mut at = |ms| {
        let lines = run.step(ms);
        let (decayed, broken): (Vec<_>, Vec<_>) = lines
            .iter()
            .filter(|l| l.starts_with("sim fence"))
            .partition(|l| l.ends_with("decayed"));
        (decayed.len(), broken.len(), run_state(&run))
    };
    // 1000 ms: nothing yet; 3 ms more: the countdown reaches 0, life 2 is the decayed life.
    log.push(at(1000));
    log.push(at(3));
    assert_eq!(log[0].2, (3, 300, 3));
    assert_eq!((log[1].0, log[1].2), (1, (2, 200, 1003)));
    // Strength never goes below 1 and the piece breaks at life 0 with strength 0.
    log.push(at(1003));
    assert_eq!(log[2].2, (1, 100, 1003));
    log.push(at(1003));
    assert_eq!((log[3].1, log[3].2), (1, (0, 0, 1003)));
    // A broken piece does not wear any further.
    log.push(at(5000));
    assert_eq!((log[4].0 + log[4].1, log[4].2.0), (0, 0));
}

/// The wear step of a fence draws its new start time in the entity loop (placing 7.4): one draw
/// for a step that wears, three for the step that breaks (the break sets the picture, its
/// validation sets it again and the step sets it a last time; sprite drawing 3.6 item 2), none for
/// a step that does not.
#[test]
fn a_break_by_wear_draws_three_start_times() {
    let mut run = run(types(WEARING), world("f"), 1);
    run.step(1000);
    // Draws a step makes: the state of the generator is compared with picks from a copy.
    let mut draws = |ms| {
        let before = run.sim.rng.clone();
        run.step(ms);
        (0..8)
            .find(|&n| {
                let mut copy = before.clone();
                (0..n).for_each(|_| _ = copy.pick(2000));
                copy == run.sim.rng
            })
            .unwrap()
    };
    // Life 3 to 2, 2 to 1, then the break, 1 to 0.
    assert_eq!([draws(2), draws(1), draws(1003)], [0, 1, 1]);
    assert_eq!(draws(1003), 3);
    // The task for the broken piece makes its own draws; the wear makes none more.
    draws(5000);
    assert_eq!(draws(5000), 0);
}

#[test]
fn a_piece_that_breaks_plays_its_break_sound_once_at_the_piece() {
    let sounds = |keys: &str| {
        let mut run = run(types(&format!("{WEARING}\n{keys}")), world("f"), 1);
        let mut heard = Vec::new();
        for ms in [1003, 1003, 1003, 5000] {
            let lines = run.step(ms);
            heard.extend(lines.into_iter().filter(|l| l.starts_with("unit sound")));
        }
        heard
    };
    let key = "cBreakSound=sounds/brokfenc.wav\ncBreakSoundAtten=1100";
    assert_eq!(sounds(key), ["unit sound 7 sounds/brokfenc.wav"]);
    assert!(sounds("cBreakSoundAtten=1100").is_empty());
}

#[test]
fn a_loose_animal_found_in_a_step_dates_the_escape_at_that_moment() {
    let mut run = run(types(WEARING), world("f"), 1);
    run.step(100);
    assert_eq!(run.sim.status.escape, crate::zoo::NEVER);
    run.world.units.loose = true;
    run.step(100);
    let now = run.world.units.now;
    assert_eq!(run.sim.status.escape, crate::timing::file_time(now));
    assert!(!run.world.units.loose);
}

fn run_state(run: &Run) -> (i32, u32, u32) {
    let f = fence(run);
    (f.life, f.strength, f.timers[1])
}

/// The run of a version 55 file whose piece is stored worn: strength 9, life 1, timers 5 and 6.
fn stored(version: u32) -> Run {
    let mut world = world("f");
    world.zoo.version = version;
    let piece = world.zoo.entities[0].common.as_mut().unwrap();
    piece.fence = Some(crate::zoo::Fence {
        strength: 9,
        life: 1,
        timers: [5, 6],
        portal: false,
        open: false,
    });
    run(types(WEARING), world, 1)
}

#[test]
fn a_piece_of_a_file_from_version_55_keeps_its_condition_until_a_map_asks_for_a_reset() {
    let mut kept = stored(55);
    assert_eq!((fence(&kept).strength, fence(&kept).life), (9, 1));
    assert_eq!(fence(&kept).timers, [5, 6]);
    // A start whose map file has `resetfences` heals it and draws its interval.
    let cat = types(WEARING);
    let mut money = Money::default();
    kept.sim.new_game(&mut kept.world, &cat, &mut money, true);
    let healed = fence(&kept);
    assert_eq!((healed.strength, healed.life), (300, 3));
    assert_eq!(healed.timers[0], healed.timers[1]);
    // A file of version 54 is healed at load, a file of 55 is not.
    assert_eq!(fence(&stored(54)).life, 3);
    assert_eq!(fence(&stored(54)).timers, [1003; 2]);
}

#[test]
fn the_repair_cheat_mends_the_broken_and_decayed_pieces_and_leaves_the_others() {
    // Life 3 is healthy, 2 is the decayed life, 0 is broken; the piece of an old type that does
    // not wear is not touched.
    for (life, mended) in [(3, false), (2, true), (1, true), (0, true), (-4, true)] {
        let mut run = stored(55);
        let piece = run.world.zoo.entities[0].common.as_mut().unwrap();
        piece.fence = Some(crate::zoo::Fence {
            strength: 9,
            life,
            timers: [5, 6],
            portal: false,
            open: false,
        });
        run.sim.repair_fences(&mut run.world, &run.cat);
        let f = fence(&run);
        match mended {
            true => assert_eq!((f.strength, f.life, f.timers[0]), (300, 3, f.timers[1])),
            false => assert_eq!((f.strength, f.life, f.timers), (9, life, [5, 6])),
        }
    }
    // A piece that never wears stays as it is, and so does a world with no piece.
    let mut run = run_with("f", "cLife=3");
    let before = fence(&run);
    run.sim.repair_fences(&mut run.world, &run.cat);
    assert_eq!(fence(&run), before);
    run.world.zoo.entities.clear();
    run.sim.repair_fences(&mut run.world, &run.cat);
}

#[test]
fn the_start_of_a_game_logs_the_healthy_pieces_that_wear() {
    // The wear steps of the same run are logged by the step.
    let mut run = run(types(WEARING), world("f"), 1);
    let lines = run.step(1003);
    assert!(
        lines.contains(&"sim fence 7 healthy 1003".to_string()),
        "{lines:?}"
    );
    assert!(lines.contains(&"sim fence 7 life 2 strength 200 decayed".to_string()));
    // Neither a gate piece, nor a type of strength 500, nor one that cannot break wears.
    for (subtype, keys) in [
        ("g", WEARING),
        ("f", "cStrength=500\ncLife=3"),
        ("f", "cLife=3"),
        ("f", &format!("{WEARING}\ncIndestructible=1")),
    ] {
        let mut run = run_with(subtype, keys);
        assert!(
            run.step(100_000).iter().all(|l| !l.contains("fence")),
            "{keys}"
        );
        assert_eq!(fence(&run).life, 3);
    }
}

fn run_with(subtype: &str, keys: &str) -> Run {
    run(types(keys), world(subtype), 1)
}

#[test]
fn the_hash_follows_every_value_of_the_wear() {
    let base = || run(types(WEARING), world("f"), 1);
    let hash = base().hash();
    assert_eq!(base().hash(), hash, "the same run, the same state");
    let changed = |edit: &dyn Fn(&mut crate::zoo::Fence)| {
        let mut r = base();
        let c = r.world.zoo.entities[0].common.as_mut().unwrap();
        edit(c.fence.as_mut().unwrap());
        r.hash()
    };
    let all = [
        changed(&|f| f.strength -= 1),
        changed(&|f| f.life -= 1),
        changed(&|f| f.timers[0] += 1),
        changed(&|f| f.timers[1] -= 1),
    ];
    for (n, h) in all.iter().enumerate() {
        assert_ne!(*h, hash, "value {n}");
        assert!(all.iter().filter(|x| *x == h).count() == 1, "value {n}");
    }
    // Wearing moves the hash on, and the same seed and steps give the same hash again.
    let worn = |ms| {
        let mut r = base();
        r.step(ms);
        r.hash()
    };
    assert_ne!(worn(1003), worn(500));
    assert_eq!(worn(1003), worn(1003));
}

#[test]
fn a_huge_step_wears_one_life_and_no_step_at_all_wears_nothing() {
    let mut r = run(types(WEARING), world("f"), 1);
    r.step(u32::MAX);
    assert_eq!(fence(&r).life, 2);
    let mut r = run(types(WEARING), world("f"), 1);
    r.step(0);
    assert_eq!(fence(&r).life, 3);
}

#[test]
fn the_bounds_are_read_from_the_class_file_with_the_defaults_for_what_is_missing() {
    let load = |name: &str, text: Option<&str>| {
        let dir = TempDir::new(name);
        dir.write("zoo.ini", b"");
        if let Some(text) = text {
            dir.write("fences/fence.ai", text.as_bytes());
        }
        let vfs = crate::vfs::Vfs::open(&dir.0).unwrap();
        Decay::load(&vfs)
    };
    let default = Decay::default();
    assert_eq!(load("none", None), default);
    assert_eq!(
        load(
            "keys",
            Some("[Decay]\r\nminDecayTime = 5\r\nmaxDecayTime=90\r\n")
        ),
        Decay { min: 5, max: 90 }
    );
    // A missing key keeps its default; a negative one reads as 0.
    assert_eq!(
        load("half", Some("[Decay]\nmaxDecayTime = 70\n")),
        Decay {
            min: default.min,
            max: 70
        }
    );
    assert_eq!(
        load(
            "neg",
            Some("[Decay]\nminDecayTime = -4\nmaxDecayTime = x\n")
        ),
        Decay { min: 0, max: 0 }
    );
}

/// The events on the task channel that the last step posted, as `(type, sender, data)`.
fn posted(run: &Run) -> Vec<(u16, u32, u32)> {
    run.sim
        .events
        .listen(1, 0x4d)
        .map(|e| {
            assert_eq!(
                (e.target, e.extra, e.delay, e.sender_channel),
                (0, 0, 1, 0x45)
            );
            (e.kind, e.sender, e.data)
        })
        .collect()
}

#[test]
fn a_worn_piece_posts_its_events_with_the_worker_tile() {
    // The piece stands on tile (1, 1) of a 4 by 4 map, which is the tile 5.
    let mut run = run(types(WEARING), world("f"), 1);
    // Nothing is posted before the countdown ends; at the decayed life (2) the piece says so.
    run.step(1000);
    assert!(posted(&run).is_empty());
    run.step(3);
    assert_eq!(posted(&run), [(10033, 7, 5)]);
    // The next life is 1: nothing. The break (life 0) posts 10034, and a broken piece is quiet.
    run.step(1003);
    assert!(posted(&run).is_empty());
    run.step(1003);
    assert_eq!(posted(&run), [(10034, 7, 5)]);
    run.step(100_000);
    assert!(posted(&run).is_empty());
    // An electrified type is "broken" at its decayed life already.
    let keys = format!("{WEARING}\ncIsElectrified=1");
    let mut run = run_with("f", &keys);
    run.step(1003);
    assert_eq!(posted(&run), [(10034, 7, 5)]);
    // With no decayed life, the break posts 10034 and then 10033; a type of life 1 breaks at once.
    let mut run = run_with("f", "cStrength=300\ncLife=1\ncDecayDelta=100");
    run.step(1003);
    assert_eq!(posted(&run), [(10034, 7, 5), (10033, 7, 5)]);
}

/// The kinds of the events a loaded world with the piece at `life` posts, and their data.
fn loaded(keys: &str, life: Option<i32>) -> Vec<(u16, u32)> {
    loaded_in("[HabitatFence]\n", keys, life)
}

/// The same for a type with these other `sections`.
fn loaded_in(sections: &str, keys: &str, life: Option<i32>) -> Vec<(u16, u32)> {
    let mut w = world("f");
    let c = w.zoo.entities[0].common.as_mut().unwrap();
    c.fence = life.map(|life| crate::zoo::Fence {
        strength: 300,
        life,
        timers: [5, 5],
        portal: false,
        open: false,
    });
    w.loaded_events(&types_with(sections, keys))
        .iter()
        .map(|e| {
            let sent = (e.sender, e.target, e.channel, e.sender_channel, e.delay);
            assert_eq!(sent, (7, 0, 0x4d, 0x45, 1));
            (e.kind, e.data)
        })
        .collect()
}

#[test]
fn a_loaded_piece_posts_broken_or_decayed_with_its_worker_tile() {
    // Broken (life 0), decayed (at the decayed life), and sound pieces; the tile is the piece's own.
    assert_eq!(loaded(WEARING, Some(0)), [(10034, 5)]);
    assert_eq!(loaded(WEARING, Some(2)), [(10033, 5)]);
    assert_eq!(loaded(WEARING, Some(1)), [(10033, 5)]);
    assert_eq!(loaded(WEARING, Some(3)), []);
    assert_eq!(loaded(WEARING, Some(i32::MAX)), []);
    // The original tests "life below 1" on a signed number: a stored life above `i32::MAX` is a
    // broken piece.
    assert_eq!(loaded(WEARING, Some(-1)), [(10034, 5)]);
    assert_eq!(loaded(WEARING, Some(i32::MIN)), [(10034, 5)]);
    // An electrified piece posts the urgent kind, once, and a piece with no wear data none.
    let live = format!("{WEARING}\ncIsElectrified=1");
    assert_eq!(loaded(&live, Some(2)), [(10034, 5)]);
    assert_eq!(loaded(&live, Some(0)), [(10034, 5)]);
    assert_eq!(loaded(WEARING, None), []);
    // The default decayed life is 0: only a broken piece posts.
    assert_eq!(loaded("cStrength=300", Some(1)), []);
    assert_eq!(loaded("cStrength=300", Some(0)), [(10034, 5)]);
}

#[test]
fn the_events_of_a_loaded_zoo_wait_for_the_next_step_after_the_stored_ones() {
    let cat = types_with("[HabitatFence]\n", WEARING);
    let mut run = run(types(WEARING), world("f"), 1);
    run.world.zoo.entities[0]
        .common
        .as_mut()
        .unwrap()
        .fence
        .as_mut()
        .unwrap()
        .life = 0;
    let before = run.sim.events.buffers()[1].len();
    run.sim.fences_loaded(&mut run.world, &cat);
    let next = run.sim.events.buffers()[1];
    assert_eq!(next.len(), before + 1);
    assert_eq!(next.last().map(|e| e.kind), Some(10034));
}

#[test]
fn a_loaded_broken_piece_posts_only_when_its_type_can_deteriorate() {
    let fence = "[HabitatFence]\n";
    let sturdy = format!("{WEARING}\ncIndestructible=1");
    assert_eq!(loaded_in(fence, WEARING, Some(0)), [(10034, 5)]);
    assert_eq!(loaded_in(fence, &sturdy, Some(0)), []);
    assert_eq!(loaded_in("", WEARING, Some(0)), []);
    // A tank wall or a zoo wall never breaks either.
    assert_eq!(
        loaded_in("[HabitatFence]\n[TankFence]\n", WEARING, Some(0)),
        []
    );
    assert_eq!(
        loaded_in("[HabitatFence]\n[Member]\nzoowall\n", WEARING, Some(0)),
        []
    );
    assert_eq!(
        loaded_in("[HabitatFence]\n[Member]\nzoogate\n", WEARING, Some(0)),
        []
    );
    // The decayed branch has no such test.
    assert_eq!(loaded_in(fence, &sturdy, Some(2)), [(10033, 5)]);
    assert_eq!(loaded_in("", WEARING, Some(2)), [(10033, 5)]);
}

#[test]
fn a_piece_whose_stored_life_is_above_i32_max_is_broken_and_does_not_wear() {
    let mut run = run(types(WEARING), world("f"), 1);
    let c = run.world.zoo.entities[0].common.as_mut().unwrap();
    c.fence.as_mut().unwrap().life = u32::MAX as i32;
    let before = fence(&run);
    assert!(run.step(10_000).iter().all(|l| !l.starts_with("fence")));
    assert_eq!(fence(&run), before);
}

#[test]
fn a_loaded_zoo_posts_its_can_fence_and_filter_events_in_entity_list_order() {
    let cat = || {
        let mut cat = types_with("[HabitatFence]\n", WEARING);
        cat.push(EntityType {
            class: "building",
            type_name: b"can".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(b"[Satisfies]\nbuilding\ntrash\n").unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        cat.push(EntityType {
            class: "filter",
            type_name: b"filter".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(b"[Characteristics/Integers]\ncDecayedHealth=5\n")
                    .unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        cat
    };
    let mut world = world("f");
    // A broken filter between the piece and the can.
    let mut filter = entity("filter", "filter", 8, (96, 128), 0);
    filter.common.as_mut().unwrap().filter = Some(crate::zoo::Filter {
        health: 0,
        timers: None,
        ..Default::default()
    });
    world.zoo.entities.push(filter);
    let mut can = entity("building", "can", 9, (96, 160), 0);
    can.common.as_mut().unwrap().building = Some(crate::zoo::BuildingRecord {
        guests: [0; 3],
        income: [0; 3],
        upkeep: None,
        upgrade: 0,
        price: 0,
        users: Vec::new(),
        users_now: 0,
        members: Vec::new(),
        fill: 13,
        built: 0,
    });
    world.zoo.entities.push(can);
    let mut run = run(cat(), world, 1);
    let piece = run.world.zoo.entities[0].common.as_mut().unwrap();
    piece.fence.as_mut().unwrap().life = 0;
    assert_eq!(run.world.units.buildings[&9].fill, 13);
    let before = run.sim.events.buffers()[1].len();
    run.sim.fences_loaded(&mut run.world, &cat());
    let next = run.sim.events.buffers()[1];
    let kinds: Vec<_> = next[before..].iter().map(|e| (e.sender, e.kind)).collect();
    assert_eq!(kinds, [(7, 10034), (8, 10036), (9, 10030)]);
    // A can before the piece posts first.
    run.world.zoo.entities.reverse();
    let before = run.sim.events.buffers()[1].len();
    run.sim.fences_loaded(&mut run.world, &cat());
    let next = run.sim.events.buffers()[1];
    let kinds: Vec<_> = next[before..].iter().map(|e| (e.sender, e.kind)).collect();
    assert_eq!(kinds, [(9, 10030), (8, 10036), (7, 10034)]);
}

#[test]
fn a_run_starts_with_the_events_of_the_loaded_zoo() {
    let mut world = world("f");
    world.zoo.version = 55;
    world.zoo.entities[0]
        .common
        .as_mut()
        .unwrap()
        .fence
        .as_mut()
        .unwrap()
        .life = 0;
    let cat = types_with("[HabitatFence]\n", WEARING);
    let run = run(cat, world, 1);
    assert!(run.sim.events.buffers()[1].iter().any(|e| e.kind == 10034));
}

/// The break cheat on a world with `subtype` pieces of these keys; the world, the lines and the
/// event kinds.
fn broken(subtype: &str, keys: &str, life: i32) -> (World, Vec<String>, Vec<u16>) {
    let mut w = world(subtype);
    w.zoo.entities[0].common.as_mut().unwrap().fence = Some(crate::zoo::Fence {
        strength: 50,
        life,
        timers: [5, 6],
        portal: false,
        open: false,
    });
    let mut lines = Vec::new();
    let events = w.break_fences(&types(keys), &mut lines);
    (w, lines, events.0.iter().map(|e| e.kind).collect())
}

#[test]
fn the_break_cheat_breaks_every_piece_that_wears_and_tells_the_workers() {
    // From any life: healthy, decayed, the last one, and a huge or negative one that is not broken.
    for life in [3, 2, 1, i32::MAX] {
        let (w, lines, kinds) = broken("f", WEARING, life);
        let f = w.zoo.entities[0].common.as_ref().unwrap().fence.unwrap();
        assert_eq!((f.life, f.strength), (0, 0), "{life}");
        assert!(kinds.contains(&10034), "{life}");
        assert!(lines.iter().any(|l| l.ends_with("broken")), "{life}");
    }
    // A piece already broken (life 0 or less) stays as it is and posts nothing.
    for life in [0, -1, i32::MIN] {
        let (w, lines, kinds) = broken("f", WEARING, life);
        let f = w.zoo.entities[0].common.as_ref().unwrap().fence.unwrap();
        assert_eq!((f.life, f.strength), (life, 50), "{life}");
        assert!(lines.is_empty() && kinds.is_empty(), "{life}");
    }
}

/// The break cheat on a piece of a type with these `sections` and `keys` and this `life`; whether
/// it broke, with the event kinds.
fn breaks(sections: &str, keys: &str, subtype: &str, life: i32) -> (bool, Vec<u16>) {
    let mut w = world(subtype);
    w.zoo.entities[0].common.as_mut().unwrap().fence = Some(crate::zoo::Fence {
        strength: 50,
        life,
        timers: [5, 6],
        portal: false,
        open: false,
    });
    let events = w
        .break_fences(&types_with(sections, keys), &mut Vec::new())
        .0;
    let f = w.zoo.entities[0].common.as_ref().unwrap().fence.unwrap();
    (f.life == 0, events.iter().map(|e| e.kind).collect())
}

#[test]
fn the_break_cheat_takes_what_can_deteriorate_with_the_break_routines_own_test() {
    let fence = "[HabitatFence]\n";
    let tank = "[HabitatFence]\n[TankFence]\n";
    let wall = "[HabitatFence]\n[Member]\nzoogate\n";
    let sturdy = format!("{WEARING}\ncIndestructible=1");
    // Never broken: no habitat block, a tank fence, a zoo gate, an indestructible type, a life of 500.
    for (sections, keys, life) in [
        ("", WEARING, 3),
        (tank, WEARING, 3),
        (wall, WEARING, 3),
        (fence, sturdy.as_str(), 3),
        (fence, WEARING, 500),
    ] {
        assert_eq!(
            breaks(sections, keys, "f", life),
            (false, vec![]),
            "{sections}{life}"
        );
    }
    // Broken, with one event: a gate of an exhibit fence, a strength of 500, no `cStrength` at all.
    for (keys, subtype) in [
        (WEARING, "g"),
        ("cStrength=500\ncLife=3", "f"),
        ("cLife=3", "f"),
    ] {
        assert_eq!(
            breaks(fence, keys, subtype, 3),
            (true, vec![10034]),
            "{keys}"
        );
    }
    // A piece outside the zoo breaks too.
    let mut w = world_with((4, 4), |zoo| {
        let mut piece = entity("fences", "iron", 7, (64 * 100, 64), 0);
        piece.subtype = "f".into();
        zoo.entities.push(piece);
    });
    w.zoo.entities[0].common.as_mut().unwrap().fence = Some(crate::zoo::Fence {
        strength: 50,
        life: 3,
        timers: [5, 6],
        portal: false,
        open: false,
    });
    let cat = types(WEARING);
    assert!(!w.wears(&cat, 0));
    assert_eq!(w.break_fences(&cat, &mut Vec::new()).0.len(), 1);
    // No piece at all.
    let mut w = world_with((4, 4), |_| {});
    assert!(
        w.break_fences(&types(WEARING), &mut Vec::new())
            .0
            .is_empty()
    );
}

/// A run of the pieces of `subtypes` (ids 7, 8 ...), healthy, and the generator as it stands.
fn drawn(subtypes: &[&str]) -> Run {
    let mut w = world_with((4, 4), |zoo| {
        for (i, (id, subtype)) in (7..).zip(subtypes).enumerate() {
            let mut piece = entity("fences", "iron", id, (96 + 64 * i as i32, 64), 0);
            piece.subtype = (*subtype).into();
            zoo.entities.push(piece);
        }
    });
    w.set_decay(Decay {
        min: 1000,
        max: 3000,
    });
    let mut run = run(types(WEARING), w, 1);
    // The heal of the load, taken at the first register.
    run.sim.idles.starts.clear();
    run
}

/// The generator `n` draws after `rng`.
fn after(rng: &super::Rng, n: u32) -> super::Rng {
    let mut next = rng.clone();
    (0..n).for_each(|_| _ = next.pick(2000));
    next
}

#[test]
fn every_heal_draws_the_start_of_a_piece_right_after_its_interval() {
    let cat = types(WEARING);
    // `resetfences` (a piece, a gate that draws its interval only, a piece), the repair cheat and
    // the heal of an old file: two draws a piece that is not a gate, one for a gate.
    let mut run = drawn(&["f", "g", "f"]);
    let before = run.sim.rng.clone();
    run.sim
        .new_game(&mut run.world, &cat, &mut Money::default(), true);
    assert_eq!(run.sim.rng.state(), after(&before, 5).state());
    // The start of a piece follows its own interval: the second draw of its pair.
    let (first, last) = (after(&before, 1), after(&before, 4));
    assert_eq!(run.sim.idles.starts[&7], first.clone().pick(2000));
    assert_eq!(run.sim.idles.starts[&9], last.clone().pick(2000));
    assert!(!run.sim.idles.starts.contains_key(&8));
    // The repair cheat heals the pieces that are broken, not the gate.
    let mut run = drawn(&["f", "f"]);
    run.world.zoo.entities[1]
        .common
        .as_mut()
        .unwrap()
        .fence
        .as_mut()
        .unwrap()
        .life = 0;
    let before = run.sim.rng.clone();
    run.sim.repair_fences(&mut run.world, &cat);
    assert_eq!(run.sim.rng.state(), after(&before, 2).state());
    assert_eq!(run.sim.idles.starts.keys().collect::<Vec<_>>(), [&8]);
    // A file older than version 55 heals its pieces at load.
    let mut run = drawn(&["f"]);
    run.world.zoo.version = 54;
    let before = run.sim.rng.clone();
    run.sim.heal(&mut run.world, &cat, 0);
    assert_eq!(run.sim.rng.state(), after(&before, 2).state());
}

#[test]
fn the_break_cheat_draws_two_starts_for_each_piece_that_breaks_at_its_place() {
    let cat = types(WEARING);
    let mut run = drawn(&["f", "g", "f"]);
    run.world.zoo.entities[2]
        .common
        .as_mut()
        .unwrap()
        .fence
        .as_mut()
        .unwrap()
        .life = 0;
    let before = run.sim.rng.clone();
    run.sim.break_fences(&mut run.world, &cat);
    // Piece 7 breaks (two draws); the gate breaks too and draws nothing; the broken piece 9 stays.
    assert_eq!(run.sim.rng.state(), after(&before, 2).state());
    assert_eq!(run.sim.idles.starts[&7], after(&before, 1).pick(2000));
    assert_eq!(run.sim.idles.starts.len(), 1);
    // Nothing left to break: no draw.
    let after_first = run.sim.rng.clone();
    run.sim.break_fences(&mut run.world, &cat);
    assert_eq!(run.sim.rng.state(), after_first.state());
}

#[test]
fn a_loaded_piece_with_life_below_1_draws_two_starts_in_list_order() {
    let cat = types(WEARING);
    // Pieces 7 (broken), 8 (healthy), 9 (a broken gate: no start), 10 (broken).
    let mut run = drawn(&["f", "f", "g", "f"]);
    for at in [0, 2, 3] {
        let life = [-1, 3, 0, i32::MIN][at];
        let c = run.world.zoo.entities[at].common.as_mut().unwrap();
        c.fence.as_mut().unwrap().life = life;
    }
    let before = run.sim.rng.clone();
    run.sim.fences_loaded(&mut run.world, &cat);
    assert_eq!(run.sim.rng.state(), after(&before, 4).state());
    assert_eq!(run.sim.idles.starts[&7], after(&before, 1).pick(2000));
    assert_eq!(run.sim.idles.starts[&10], after(&before, 3).pick(2000));
    assert_eq!(run.sim.idles.starts.len(), 2);
    // A type that cannot deteriorate, and a life that is not below 1, draw nothing.
    let none = types_with("", WEARING);
    let before = run.sim.rng.clone();
    run.sim.fences_loaded(&mut run.world, &none);
    assert_eq!(run.sim.rng.state(), before.state());
    let c = run.world.zoo.entities[0].common.as_mut().unwrap();
    c.fence.as_mut().unwrap().life = i32::MAX;
    let c = run.world.zoo.entities[3].common.as_mut().unwrap();
    c.fence.as_mut().unwrap().life = 1;
    run.sim.fences_loaded(&mut run.world, &cat);
    assert_eq!(run.sim.rng.state(), before.state());
}

#[test]
fn the_repair_cheat_applies_no_wear_test() {
    // A broken gate and a broken piece of strength 500 are made healthy; a gate draws its interval only.
    let cat = types("cStrength=500\ncLife=3\ncDecayedLife=2");
    let mut run = drawn(&["g", "f"]);
    for at in 0..2 {
        let c = run.world.zoo.entities[at].common.as_mut().unwrap();
        c.fence.as_mut().unwrap().life = 0;
    }
    let before = run.sim.rng.clone();
    run.sim.repair_fences(&mut run.world, &cat);
    for at in 0..2 {
        let f = run.world.zoo.entities[at]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap();
        assert_eq!((f.strength, f.life), (500, 3), "{at}");
    }
    assert_eq!(run.sim.rng.state(), after(&before, 3).state());
}

const SOUND: &str = "cBreakSound=sounds/brokfenc.wav\ncBreakSoundAtten=1100";

/// The run of a loaded piece stored with `life` and the strength 9, of a type with these `sections`.
fn loaded_run(sections: &str, life: i32) -> Run {
    let mut world = world("f");
    world.zoo.version = 55;
    let piece = world.zoo.entities[0].common.as_mut().unwrap();
    piece.fence = Some(crate::zoo::Fence {
        strength: 9,
        life,
        timers: [5, 6],
        portal: false,
        open: false,
    });
    run(
        types_with(sections, &format!("{WEARING}\n{SOUND}")),
        world,
        1,
    )
}

fn sounds(run: &mut Run) -> Vec<String> {
    // Short enough for the countdown of 6 ms not to end.
    let lines = run.step(1);
    lines
        .into_iter()
        .filter(|l| l.starts_with("unit sound"))
        .collect()
}

#[test]
fn a_loaded_piece_with_life_below_1_is_normalised_and_plays_its_break_sound() {
    let key = "[HabitatFence]\n";
    for life in [-1, i32::MIN, 0] {
        let mut run = loaded_run(key, life);
        assert_eq!((fence(&run).life, fence(&run).strength), (0, 0), "{life}");
        assert_eq!(
            sounds(&mut run),
            ["unit sound 7 sounds/brokfenc.wav"],
            "{life}"
        );
        assert!(sounds(&mut run).is_empty());
    }
    // A life of 1 and a huge life are left alone and silent.
    for life in [1, i32::MAX] {
        let mut run = loaded_run(key, life);
        assert_eq!((fence(&run).life, fence(&run).strength), (life, 9));
        assert!(sounds(&mut run).is_empty());
    }
    // A type that cannot deteriorate keeps the stored life and plays nothing.
    for sections in ["", "[HabitatFence]\n[TankFence]\n"] {
        let mut run = loaded_run(sections, -1);
        assert_eq!((fence(&run).life, fence(&run).strength), (-1, 9));
        assert!(sounds(&mut run).is_empty());
    }
}

#[test]
fn a_piece_of_a_type_that_is_no_exhibit_fence_does_not_break_at_the_wear_step() {
    // Life 3 goes to 0 and then on: no break, no sound, no 10034, the strength stays at 1.
    let keys = format!("{WEARING}\n{SOUND}");
    let mut run = run(types_with("", &keys), world("f"), 1);
    let mut kinds = Vec::new();
    let mut heard = Vec::new();
    for _ in 0..5 {
        let lines = run.step(1003);
        heard.extend(lines.into_iter().filter(|l| l.starts_with("unit sound")));
        kinds.extend(posted(&run).into_iter().map(|e| e.0));
    }
    assert_eq!((fence(&run).life, fence(&run).strength), (-2, 1));
    assert_eq!(kinds, [10033]);
    assert!(heard.is_empty());
    // The same type with the block breaks at life 0.
    let mut run = run_with("f", &keys);
    (0..3).for_each(|_| _ = run.step(1003));
    assert_eq!((fence(&run).life, fence(&run).strength), (0, 0));
}
