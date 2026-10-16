//! Fence wear in the step driver (placing 7.4): the interval, the steps, the hash.

use super::drive::Run;
use crate::animal::Class;
use crate::entity::{Catalogue, EntityType};
use crate::money::Money;
use crate::unit::testing::{entity, world_with};
use crate::world::{Decay, World};

/// A fence type `iron` with the given characteristics under its plain section.
fn types(keys: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    for subtype in [&b"f"[..], b"g"] {
        cat.push(EntityType {
            class: "fences",
            type_name: b"iron".to_vec(),
            subtype: subtype.to_vec(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(
                    format!("[Characteristics/Integers]\n{keys}\n").as_bytes(),
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

fn run_state(run: &Run) -> (u32, u32, u32) {
    let f = fence(run);
    (f.life, f.strength, f.timers[1])
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
