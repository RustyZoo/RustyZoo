//! The fence test start (animals 11.10).

use super::life::Life;
use super::*;
use crate::animal::fence;
use crate::unit::Ctx;
use crate::unit::testing::{centre, entity as thing, world_with};

const TEST: &str = "bTestFence";

fn data() -> Life {
    Life::new().with(TEST, "fPlay(idle)")
}

/// A 6 by 6 map with the animal 500 on tile (2,2) and the `extra` entities; the animal's unit.
fn map(extra: Vec<Entity>) -> (World, Unit) {
    let world = world_with((6, 6), |zoo| {
        zoo.entities
            .push(thing("animals", "zebra", 500, centre((2, 2)), 0));
        zoo.entities.extend(extra);
    });
    let mut unit = world.units.get(500).unwrap().clone();
    unit.part = Part::Animal(Animal {
        sleeping: true,
        ..Animal::default()
    });
    (world, unit)
}

/// A fence piece on side `d` of tile `t`; `wire` is electrified, `life` its life.
fn piece(kind: &str, t: (i32, i32), d: u32, life: i32) -> Entity {
    let mut e = thing("fences", kind, 20 + d + 8 * t.0 as u32, centre(t), d);
    if let Some(f) = e.common.as_mut().and_then(|c| c.fence.as_mut()) {
        f.life = life;
    }
    e
}

fn keys_with(set: impl FnOnce(&mut Keys)) -> Keys {
    let mut k = keys();
    k.test_fence = 50;
    set(&mut k);
    k
}

/// Runs the start for the animal; its facing and whether anything started.
fn start(world: &mut World, unit: &mut Unit, k: &Keys, data: &Life) -> (u32, bool) {
    let at = world.find(500).unwrap();
    let (cat, events, mut removals) = (Catalogue::default(), crate::sim::Events::default(), vec![]);
    let mut rng = Rng::default();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    fence::test(unit, world, &mut ctx, at, data, k);
    let facing = world.zoo.entities[at].common.as_ref().unwrap().rotation;
    (
        facing,
        unit.runner.running() && unit.runner.top() == Some(TEST),
    )
}

fn animal_of(unit: &Unit) -> &Animal {
    let Part::Animal(a) = &unit.part else {
        panic!()
    };
    a
}

#[test]
fn a_working_electrified_fence_on_a_side_starts_the_test_facing_it() {
    for d in [0, 2, 4, 6] {
        let (mut w, mut unit) = map(vec![piece("wire", (2, 2), d, 5)]);
        unit.moving = true;
        unit.path = vec![5, 6];
        let k = keys_with(|k| (k.fence_drop, k.fence_min, k.zap_change) = (5, 5, -25));
        assert_eq!(start(&mut w, &mut unit, &k, &data()), (d, true), "side {d}");
        let a = animal_of(&unit);
        assert!(!a.sleeping && !unit.moving && unit.path.is_empty());
        assert_eq!((a.fence_chance, a.happiness_change), (Some(45), -25));
    }
}

#[test]
fn the_fence_across_the_line_counts_as_the_neighbours_piece_on_the_opposite_side() {
    let (mut w, mut unit) = map(vec![piece("wire", (3, 2), 6, 5)]);
    assert_eq!(start(&mut w, &mut unit, &keys(), &data()), (2, true));
}

#[test]
fn a_fence_that_is_not_working_electrified_takes_no_side_and_changes_nothing() {
    // A plain fence, a broken wire, a decayed one (life at or below the decayed life).
    for (kind, life) in [("f", 5), ("wire", 0), ("wire", 2)] {
        let (mut w, mut unit) = map(vec![piece(kind, (2, 2), 2, life)]);
        assert_eq!(start(&mut w, &mut unit, &keys(), &data()), (0, false));
        let a = animal_of(&unit);
        assert!(a.sleeping, "{kind} {life}: it kept sleeping");
        assert_eq!((a.fence_chance, a.happiness_change), (None, 0));
    }
    // A plain piece in the slot decides alone: the wire across is not looked at.
    let plain_first = vec![piece("f", (2, 2), 2, 5), piece("wire", (3, 2), 6, 5)];
    let (mut w, mut unit) = map(plain_first);
    assert!(!start(&mut w, &mut unit, &keys(), &data()).1);
}

#[test]
fn an_object_in_a_quarter_along_the_side_refuses_it_but_another_quarter_does_not() {
    // Side 2 looks at the quarters 2 and 3 (the east half): a tree at the south-east quarter.
    let tree = |x, y| thing("objects", "tree", 30, (2 * 64 + x, 2 * 64 + y), 0);
    let wire = piece("wire", (2, 2), 2, 5);
    let (mut w, mut unit) = map(vec![wire.clone(), tree(48, 48)]);
    assert!(!start(&mut w, &mut unit, &keys(), &data()).1);
    let (mut w, mut unit) = map(vec![wire, tree(16, 16)]);
    assert_eq!(start(&mut w, &mut unit, &keys(), &data()), (2, true));
}

#[test]
fn the_probe_must_lie_on_the_map_and_in_the_animals_region() {
    // A wire on every side of the tile (3,2) makes it a pen of its own.
    let pen: Vec<Entity> = [0, 2, 4, 6].map(|d| piece("wire", (3, 2), d, 5)).into();
    let one = keys_with(|k| k.fence_offset = 1);
    // From the grounds the probe tile one step east is in the pen: refused.
    let (mut w, mut unit) = map(pen.clone());
    assert!(!start(&mut w, &mut unit, &one, &data()).1);
    // The animal inside the pen finds its own wire (offset 0), the first side being 0.
    let (mut w, mut unit) = map(pen);
    let at = w.find(500).unwrap();
    w.place_unit(at, (3 * 64 + 32, 2 * 64 + 32, 0), 0);
    assert_eq!(start(&mut w, &mut unit, &keys(), &data()), (0, true));
    // A probe off the map (negative, or past the edge, or beyond any int) takes no side.
    for offset in [-9, 1, 1000, i32::MAX, i32::MIN] {
        let corner: Vec<Entity> = [0, 2, 4, 6].map(|d| piece("wire", (0, 0), d, 5)).into();
        let (mut w, mut unit) = map(corner);
        let at = w.find(500).unwrap();
        w.place_unit(at, (32, 32, 0), 0);
        let k = keys_with(|k| k.fence_offset = offset);
        assert!(!start(&mut w, &mut unit, &k, &data()).1, "offset {offset}");
    }
}

#[test]
fn the_chance_drops_to_zero_below_the_minimum_and_the_zap_check_stops_drawing() {
    let k = keys_with(|k| (k.fence_drop, k.fence_min) = (5, 5));
    let (mut w, mut unit) = map(vec![piece("wire", (2, 2), 0, 5)]);
    let mut seen = Vec::new();
    for _ in 0..11 {
        start(&mut w, &mut unit, &k, &data());
        seen.push(animal_of(&unit).fence_chance.unwrap());
    }
    assert_eq!(seen[..10], [45, 40, 35, 30, 25, 20, 15, 10, 5, 0]);
    // A result below the minimum becomes 0 even when it is not: 6 - 4 = 2 is under 5.
    let k = keys_with(|k| (k.fence_drop, k.fence_min) = (4, 5));
    if let Part::Animal(a) = &mut unit.part {
        a.fence_chance = Some(6);
    }
    start(&mut w, &mut unit, &k, &data());
    assert_eq!(animal_of(&unit).fence_chance, Some(0));
    let k = keys_with(|k| (k.fence_drop, k.fence_min) = (5, 5));
    // Chance 0: the draw is not made, as for a species with no chance.
    let mut a = Animal { zap: 0, ..animal() };
    let (env, mut rng) = (in_exhibit(), Rng::default());
    a.fence_chance = Some(0);
    assert!(!crate::animal::run(&mut a, &k, &Class::default(), &env, &mut rng).fence);
    let (mut none, mut other) = (Animal { zap: 0, ..animal() }, Rng::default());
    let off = keys_with(|k| k.test_fence = 0);
    crate::animal::run(&mut none, &off, &Class::default(), &env, &mut other);
    assert!(rng == other);
}

#[test]
fn a_set_the_species_lacks_still_stops_the_animal_but_lowers_nothing() {
    let (mut w, mut unit) = map(vec![piece("wire", (2, 2), 4, 5)]);
    let k = keys_with(|k| (k.fence_drop, k.zap_change) = (5, -25));
    assert_eq!(start(&mut w, &mut unit, &k, &Life::new()).0, 4);
    let a = animal_of(&unit);
    assert!(!a.sleeping);
    assert_eq!((a.fence_chance, a.happiness_change), (None, 0));
}

#[test]
fn the_zap_draw_asks_for_the_test_only_when_it_succeeds_on_a_current_chance() {
    let (class, env) = (Class::default(), in_exhibit());
    let zap = |chance: i32, current: Option<i32>| {
        let mut a = Animal {
            zap: 0,
            fence_chance: current,
            ..animal()
        };
        let k = keys_with(|k| k.test_fence = chance);
        crate::animal::run(&mut a, &k, &class, &env, &mut Rng::default()).fence
    };
    assert!(zap(100, None));
    assert!(!zap(0, None), "no chance, no draw");
    assert!(!zap(100, Some(0)), "the current chance is 0");
    assert!(zap(100, Some(-5)), "a negative current chance draws");
    // Every draw below 0 fails.
    assert!(!zap(-1, None));
}

#[test]
fn a_young_animal_growing_up_takes_the_species_chance_again() {
    let (mut w, mut unit) = map(vec![]);
    let at = w.find(500).unwrap();
    w.zoo.entities[at].subtype = "y".into();
    if let Part::Animal(a) = &mut unit.part {
        a.fence_chance = Some(0);
    }
    let adult = |sub: &str| EntityType {
        class: "animals",
        type_name: b"zebra".to_vec(),
        subtype: sub.into(),
        file: None,
        config: None,
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let mut cat = Catalogue::default();
    cat.push(adult("m"));
    let (events, mut removals, mut rng) = (crate::sim::Events::default(), vec![], Rng::default());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::animal::breed::grow_up(&mut unit, &mut w, &mut ctx, at, &data());
    assert_eq!(w.zoo.entities[at].subtype, "m");
    assert_eq!(animal_of(&unit).fence_chance, None);
}

#[test]
fn a_status_tick_whose_zap_draw_succeeds_starts_the_test_and_one_that_fails_does_not() {
    let wire = piece("wire", (2, 2), 2, 5);
    let tick = |chance: &str| {
        let (mut w, mut unit) = map(vec![wire.clone()]);
        let at = w.find(500).unwrap();
        w.zoo.entities[at].subtype = "f".into();
        unit.part = Part::Animal(Animal { zap: 0, ..animal() });
        let cat = zebras_with(
            &format!("cTestFenceChance={chance}\ncZapHappinessChange=-7"),
            "",
        );
        let (events, mut removals, mut rng) =
            (crate::sim::Events::default(), vec![], Rng::default());
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &cat,
        };
        crate::animal::checks(&mut unit, &mut w, &mut ctx, at, &data());
        let facing = w.zoo.entities[at].common.as_ref().unwrap().rotation;
        (
            unit.runner.top() == Some(TEST),
            facing,
            animal_of(&unit).happiness_change,
        )
    };
    assert_eq!(tick("100"), (true, 2, -7));
    assert!(!tick("0").0);
    assert!(!tick("-5").0, "a negative chance draws and fails");
}

#[test]
fn the_exterior_counts_as_the_grounds_for_the_fence_test_probe() {
    // A zoo wall down the east side of column 1 and the entrance at (2, 0) facing west: columns 0
    // and 1 are the exterior. The animal stands on (1, 3); a wire on the east side of (2, 3) is
    // found by the probe one tile east, which lies on the grounds.
    let mut extra: Vec<Entity> = (0..6)
        .map(|y: i32| thing("zoowall", "", 300 + y as u32, centre((1, y)), 2))
        .collect();
    extra.push(thing("building", "", 320, centre((2, 0)), 6));
    extra.push(piece("wire", (2, 3), 2, 5));
    let world = world_with((6, 6), |zoo| {
        zoo.entrance = (2, 0);
        zoo.entities
            .push(thing("animals", "zebra", 500, centre((1, 3)), 0));
        zoo.entities.extend(extra);
    });
    let exterior = world.tile_index(1, 3).unwrap();
    assert_eq!(
        world.exhibits().region(exterior),
        Some(crate::exhibit::EXTERIOR)
    );
    let mut unit = world.units.get(500).unwrap().clone();
    unit.part = Part::Animal(Animal::default());
    let mut w = world;
    let k = keys_with(|k| k.fence_offset = 1);
    assert_eq!(start(&mut w, &mut unit, &k, &data()), (2, true));
}

/// The animal 1 of an amphibious world at (6, 2) with the tick's `out`, the bored step `between`; `task` is the goal task from before the check.
fn after_tick(out: &Out, task: u8, live: u8, between: impl FnOnce(&mut Unit)) -> (Unit, World) {
    use crate::unit::testing::amphibious_world_wired;
    let mut w = amphibious_world_wired(2, &[(1, "polar", (6, 2))]);
    crate::unit::testing::still_water(&mut w);
    let u = w.units.get_mut(1).unwrap();
    (u.typed, u.started, u.mover.swims) = (true, true, true);
    let at = w.find(1).unwrap();
    let mut unit = w.units.get(1).unwrap().clone();
    unit.part = Part::Animal(Animal::default());
    unit.goal.record.task = live;
    let (cat, events, mut removals) = (Catalogue::default(), crate::sim::Events::default(), vec![]);
    let mut rng = Rng::default();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let data = crate::unit::testing::Walker::new("fWalk(0,0)");
    let bored = |unit: &mut Unit, _: &mut World, _: &mut Ctx| between(unit);
    let args = (out, &keys(), task);
    crate::animal::escape::after(&mut unit, &mut w, &mut ctx, at, &data, args, bored);
    (unit, w)
}

#[test]
fn the_water_switch_comes_before_the_fence_test_in_one_tick() {
    // A land animal with a wire on its north side; the water check switches it to the tank mode
    // in the same tick as the zap draw succeeds: the test, started last, stops the goal the
    // switch's send set (animals 4.2: water check before zap check).
    let out = Out {
        terrain: Some((2, Some(0))),
        fence: true,
        ..Out::default()
    };
    let (unit, _) = after_tick(&out, 0, crate::goal::RANDOM, |_| {});
    assert_eq!(animal_of(&unit).terrain, 2);
    assert_eq!(unit.runner.top(), Some(TEST));
    assert_ne!(
        unit.goal.record.task,
        crate::goal::RANDOM,
        "the test's stop came after the switch's send"
    );
}

#[test]
fn the_predator_dispatch_comes_before_the_water_switch_in_one_tick() {
    // No threat in sight, the animal read the avoid task before the check: the dispatch resets
    // its goal (animals 11.7), and the switch's send, which comes after it, keeps its walk.
    let out = Out {
        terrain: Some((2, Some(0))),
        fired: true,
        ..Out::default()
    };
    let (unit, _) = after_tick(&out, crate::goal::AVOID, crate::goal::AVOID, |_| {});
    assert_eq!(animal_of(&unit).terrain, 2);
    assert_eq!(unit.goal.record.task, crate::goal::RANDOM);
}

#[test]
fn the_dispatch_reads_the_task_from_before_the_check() {
    // Animals 20.5 point 5: the avoid reset tests the snapshot, not the live task.
    let out = Out {
        fired: true,
        ..Out::default()
    };
    let other = crate::goal::CHASE;
    let (unit, _) = after_tick(&out, crate::goal::AVOID, other, |_| {});
    assert_eq!(unit.goal.record.task, 0, "the snapshot was the avoid");
    let (unit, _) = after_tick(&out, other, crate::goal::AVOID, |_| {});
    assert_eq!(
        unit.goal.record.task,
        crate::goal::AVOID,
        "the live avoid is not read"
    );
}

#[test]
fn the_dispatch_comes_after_the_send_in_one_tick() {
    // The send of an animal on water starts a walk; the dispatch (snapshot: avoid, no threat)
    // then resets the goal, so the walk is gone (animals 4.2: the send is part of the check).
    let sent = Out {
        sent: Some(0),
        ..Out::default()
    };
    let (unit, _) = after_tick(&sent, crate::goal::AVOID, crate::goal::RANDOM, |_| {});
    assert_eq!(unit.goal.record.task, crate::goal::RANDOM, "the send walks");
    let both = Out {
        fired: true,
        ..sent
    };
    let (unit, _) = after_tick(&both, crate::goal::AVOID, crate::goal::RANDOM, |_| {});
    assert_eq!(unit.goal.record.task, 0, "the reset came after the send");
}

#[test]
fn the_dispatch_does_not_see_the_bored_steps_afraid() {
    // Animals 4.2: the escape check and its dispatch run before the bored check. With nothing
    // in sight the dispatch lowers afraid, and the bored step's raise comes after it.
    let out = Out {
        fired: true,
        afraid: true,
        ..Out::default()
    };
    let (unit, _) = after_tick(&out, 0, crate::goal::RANDOM, |u| u.frightened |= out.afraid);
    assert!(unit.frightened);
    let (unit, _) = after_tick(&out, 0, crate::goal::RANDOM, |_| {});
    assert!(
        !unit.frightened,
        "the dispatch lowers afraid when nothing is in sight"
    );
}

#[test]
fn the_bored_step_comes_before_the_water_switch_and_the_fence_test() {
    // Animals 4.2: the bored check comes before the water check and the zap check, so it sees
    // the land mode and no fence test started.
    let out = Out {
        terrain: Some((2, Some(0))),
        fence: true,
        ..Out::default()
    };
    let mut seen = None;
    let (unit, _) = after_tick(&out, 0, crate::goal::RANDOM, |u| {
        seen = Some((animal_of(u).terrain, u.runner.top() == Some(TEST)));
    });
    assert_eq!(seen, Some((0, false)));
    assert_eq!(
        animal_of(&unit).terrain,
        2,
        "the switch and the test still ran"
    );
    assert_eq!(unit.runner.top(), Some(TEST));
}
