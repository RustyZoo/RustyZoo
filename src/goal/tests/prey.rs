//! The rest of the chase (animals 21): the bored check's view and its lonely chase end, the path of
//! the chase to an egg and ahead of a prey, and the meal. Same map and species as the hunt tests.

use super::chase::{ALLO, GUEST, VELO, animal, cat, loose, on, record, sets, unit_of, zoo};
use super::*;
use crate::animal::BORED;
use crate::unit::Ctx;

const MANTA: u32 = VELO;
const KEEPER: u32 = 9;

fn tile((x, y): (i32, i32)) -> usize {
    (y * 20 + x) as usize
}

/// The first draw of the generator seeded with `seed`.
fn first(seed: u32) -> u32 {
    let mut rng = Rng::default();
    rng.seed(seed);
    rng.rand()
}

/// A seed whose first draw has `pick(draw)` true.
fn seed_with(pick: impl Fn(u32) -> bool) -> u32 {
    (0..).find(|&s| pick(first(s))).unwrap()
}

/// How many numbers `rng` drew since `before`.
fn drew(before: &Rng, rng: &Rng) -> usize {
    let (mut check, mut n) = (before.clone(), 0);
    while check != *rng && n < 8 {
        _ = check.rand();
        n += 1;
    }
    n
}

/// Runs `f` on unit `id` with the generator seeded; it returns the answer and the draws made.
fn seeded<R>(w: &mut World, id: u32, seed: u32, f: impl FnOnce(&mut Cx) -> R) -> (R, usize) {
    on(w, id, &[], |cx| {
        cx.ctx.rng.seed(seed);
        let before = cx.ctx.rng.clone();
        let r = f(cx);
        (r, drew(&before, cx.ctx.rng))
    })
}

/// One status tick of unit `id` whose only due check is the bored check; the draws it made.
fn bored(w: &mut World, id: u32) -> usize {
    let (cat, data, mut rng) = (cat(), sets(), Rng::default());
    let mut u = w.units.get(id).unwrap().clone();
    let at = w.find(id).unwrap();
    if let Part::Animal(a) = &mut u.part {
        a.counters.fill(1000);
        a.counters[BORED] = 0;
        a.zap = 1000;
    }
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::animal::checks(&mut u, w, &mut ctx, at, &data);
    *w.units.get_mut(id).unwrap() = u;
    drew(&Rng::default(), &rng)
}

#[test]
fn the_bored_check_frightens_an_animal_that_sees_a_threat_without_a_draw() {
    // 21.1 point 9 (a): a velocira 3 tiles from a loose allo.
    let mut w = zoo(&[(ALLO, "allo", (13, 10)), (VELO, "velocira", (10, 10))]);
    loose(&mut w, ALLO);
    assert_eq!(bored(&mut w, VELO), 0);
    let v = unit_of(&w, VELO);
    assert!(v.frightened && v.goal.record.task == NONE, "no task yet");
    // With the flag up the searches do not run: nothing changes.
    assert_eq!(bored(&mut w, VELO), 0);
    // Nothing in sight: the flag stays down.
    let mut w = zoo(&[(ALLO, "allo", (13, 10)), (VELO, "velocira", (10, 10))]);
    bored(&mut w, VELO);
    assert!(!unit_of(&w, VELO).frightened);
}

#[test]
fn a_lonely_friendly_chase_ends_with_a_reset_that_leaves_the_partner_alone() {
    // 21.1 point 9 (c): an orca alone in task 0x74 whose target is an orca of another tank.
    let mut w = zoo(&[(ALLO, "orca", (5, 5)), (VELO, "orca", (15, 15))]);
    let u = w.units.get_mut(ALLO).unwrap();
    (u.goal.record.task, u.goal.record.target.id, u.held) = (CHASE, VELO, true);
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    animal(&mut w, ALLO).playing = true;
    animal(&mut w, ALLO).prey = VELO;
    assert_eq!(bored(&mut w, ALLO), 0, "the firing makes no draw");
    let a = animal(&mut w, ALLO).clone();
    assert_eq!((a.playing, a.prey, record(&w, ALLO).0), (false, 0, NONE));
    assert_eq!(unit_of(&w, VELO).predator, ALLO);
    assert!(
        unit_of(&w, ALLO).held,
        "held and visibility are not touched"
    );
}

#[test]
fn the_chase_to_an_egg_draws_once_and_goes_to_the_egg_or_next_to_it() {
    // 21.2 point 1 and 5 (d).
    let egg = tile((14, 10));
    let go = |seed| {
        let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
        animal(&mut w, VELO).egg = true;
        let u = w.units.get_mut(ALLO).unwrap();
        (u.goal.record.task, u.goal.record.target.id) = (CHASE, VELO);
        let (_, draws) = seeded(&mut w, ALLO, seed, |cx| cx.hunt_decide());
        let h = unit_of(&w, ALLO);
        (
            h.runner.top() == Some("bChasePrey"),
            draws,
            h.goal.record.target.tile,
        )
    };
    let (walking, draws, to) = go(seed_with(|d| d % 100 < 50));
    assert!(walking && draws == 1 && to == Some(egg));
    let (walking, draws, to) = go(seed_with(|d| d % 100 >= 50));
    assert!(walking && draws == 1 && to.is_some_and(|t| t != egg));
}

#[test]
fn the_chase_aims_ahead_on_the_prey_path_and_draws_once_while_it_has_one() {
    // 21.2 point 3 and 5 (a) to (c): an orca at (10, 10), a manta whose path leads towards it.
    let run = |manta: (i32, i32), path: &[(i32, i32)], seed: u32| {
        let mut w = zoo(&[(ALLO, "orca", (10, 10)), (MANTA, "manta", manta)]);
        w.units.get_mut(MANTA).unwrap().path = path.iter().map(|&t| tile(t)).collect();
        let u = w.units.get_mut(ALLO).unwrap();
        (u.goal.record.task, u.goal.record.target.id) = (CHASE, MANTA);
        let (_, draws) = seeded(&mut w, ALLO, seed, |cx| cx.hunt_decide());
        (unit_of(&w, ALLO).goal.record.target.tile, draws)
    };
    // d2 = 16 and 4 nodes: L = 3, one draw % 4, the node r (all nearer than the manta).
    let path = [(13, 10), (12, 10), (11, 10), (10, 11)];
    for r in 0..4 {
        let seed = seed_with(|d| d % 4 == r);
        let (to, draws) = run((14, 10), &path, seed);
        assert_eq!((to, draws), (Some(tile(path[r as usize])), 1), "node {r}");
    }
    // A node farther from the orca than the manta is not taken: the manta's own tile is.
    let far = [(14, 14); 4];
    let (to, draws) = run((14, 10), &far, seed_with(|d| d % 4 == 2));
    assert_eq!((to, draws), (Some(tile((14, 10))), 1));
    // A manta that stands still: no draw, its own tile.
    assert_eq!(run((14, 10), &[], 0), (Some(tile((14, 10))), 0));
    // d2 = 4: L = 1, a draw % 2; d2 = 1: L = 0, and still one draw.
    let near = [(11, 10), (10, 10)];
    let (to, draws) = run((12, 10), &near, seed_with(|d| d % 2 == 1));
    assert_eq!((to, draws), (Some(tile((10, 10))), 1));
    let (to, draws) = run((11, 10), &near, 5);
    assert_eq!((to, draws), (Some(tile((11, 10))), 1));
}

#[test]
fn the_meal_removes_what_the_hunter_eats_and_ends_the_hunt() {
    // 21.5 point 6: an allo holding a velocira at its fCaughtPrey.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (10, 10))]);
    let u = w.units.get_mut(ALLO).unwrap();
    (
        u.goal.record.task,
        u.goal.record.target.id,
        u.held,
        u.hidden,
    ) = (CHASE, VELO, true, true);
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    let a = animal(&mut w, ALLO);
    (a.caught_prey, a.playing, a.prey) = (true, true, VELO);
    let removed = on(&mut w, ALLO, &[], |cx| {
        cx.caught_prey();
        cx.ctx.removals.clone()
    });
    assert_eq!(removed, [VELO]);
    assert!(w.units.take_messages().is_empty());
    let h = unit_of(&w, ALLO);
    assert_eq!((h.goal.record.task, h.held, h.hidden), (NONE, false, false));
    let a = animal(&mut w, ALLO).clone();
    assert_eq!((a.caught_prey, a.playing, a.prey), (false, false, 0));
    // With no target the kept prey is eaten; with none at all only the link is cut.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (10, 10))]);
    animal(&mut w, ALLO).prey = VELO;
    let removed = on(&mut w, ALLO, &[], |cx| {
        cx.caught_prey();
        cx.ctx.removals.clone()
    });
    assert_eq!(removed, [VELO]);
    let mut w = zoo(&[(ALLO, "allo", (10, 10))]);
    animal(&mut w, ALLO).prey = 99;
    let removed = on(&mut w, ALLO, &[], |cx| {
        cx.caught_prey();
        cx.ctx.removals.clone()
    });
    assert!(removed.is_empty() && animal(&mut w, ALLO).prey == 0);
}

#[test]
fn a_guest_is_eaten_only_by_a_man_eater_and_a_keeper_eaten_is_announced() {
    // 21.5 point 3: the allo eats no guest, the orca of this map does.
    let eat = |kind: &str, target: u32| {
        let mut w = zoo(&[(ALLO, kind, (10, 10))]);
        let mut keeper = entity("keeper", "keeper", KEEPER, centre((10, 10)), 0);
        keeper.subtype = "m".into();
        w.zoo.entities.push(keeper);
        w.units.sync(&w.zoo.entities.clone());
        w.units.get_mut(ALLO).unwrap().goal.record.target.id = target;
        w.units.get_mut(target).unwrap().predator = ALLO;
        let removed = on(&mut w, ALLO, &[], |cx| {
            cx.caught_prey();
            cx.ctx.removals.clone()
        });
        (
            removed,
            w.units.take_messages().len(),
            unit_of(&w, target).predator,
        )
    };
    assert_eq!(eat("allo", GUEST), (vec![], 0, 0));
    assert_eq!(eat("orca", GUEST), (vec![GUEST], 0, 0));
    assert_eq!(eat("allo", KEEPER), (vec![KEEPER], 1, 0));
}

#[test]
fn a_kill_removes_the_target_and_none_does_nothing() {
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (10, 10))]);
    w.units.get_mut(ALLO).unwrap().goal.record.target.id = VELO;
    let removed = on(&mut w, ALLO, &[], |cx| {
        cx.kill_prey();
        cx.ctx.removals.clone()
    });
    assert_eq!(removed, [VELO]);
    let removed = on(&mut w, VELO, &[], |cx| {
        cx.kill_prey();
        cx.ctx.removals.clone()
    });
    assert!(removed.is_empty());
}

#[test]
fn the_look_ahead_is_read_missing_zero_negative_and_huge_without_a_panic() {
    // Missing: 1; 0: the node 0, no look ahead; a huge one is cut to the path; a negative one
    // draws and aims at the prey's own tile.
    let run = |look: Option<i32>| {
        let mut w = zoo(&[(ALLO, "orca", (10, 10)), (MANTA, "manta", (14, 10))]);
        let path = [(13, 10), (12, 10), (11, 10), (10, 11)];
        w.units.get_mut(MANTA).unwrap().path = path.iter().map(|&t| tile(t)).collect();
        let u = w.units.get_mut(ALLO).unwrap();
        (u.goal.record.task, u.goal.record.target.id) = (CHASE, MANTA);
        let (mut cat, data, mut rng) = (cat(), sets(), Rng::default());
        cat.chase_look_ahead = look;
        let at = w.find(ALLO).unwrap();
        let mut unit = w.units.get(ALLO).unwrap().clone();
        let (events, mut removals) = (Events::default(), Vec::new());
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &cat,
        };
        let mut cx = Cx {
            unit: &mut unit,
            world: &mut w,
            ctx: &mut ctx,
            at,
            data: &data,
        };
        cx.hunt_decide();
        let to = cx.unit.goal.record.target.tile;
        (to.is_some_and(|t| t != 0), drew(&Rng::default(), &rng))
    };
    for look in [
        None,
        Some(0),
        Some(-1),
        Some(-5),
        Some(i32::MIN),
        Some(i32::MAX),
    ] {
        assert_eq!(run(look), (true, 1), "{look:?}");
    }
}

#[test]
fn a_rampage_target_flag_occupies_the_animal_in_the_goal_ladder() {
    // 21.4 point 3 and 4: the same "any of" flags as the animal's own.
    let mut w = zoo(&[(ALLO, "allo", (10, 10))]);
    assert!(!on(&mut w, ALLO, &[], |cx| cx.occupied()));
    animal(&mut w, ALLO).rampage_target = true;
    assert!(on(&mut w, ALLO, &[], |cx| cx.occupied()));
}
