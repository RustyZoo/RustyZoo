//! The hunt of animals (animals 20) on a 20 by 20 map of plain ground: the ladder rungs, the chase
//! goal, the catch, the preattack job and the animal's half of the avoid goal. `allo` hunts
//! `velocira` (both preattack species), the `orca` hunts the manta ray, a man is prey of `allo`.

use super::*;
use crate::animal::Animal;
use crate::fixtures::unit_ty;
use crate::guest::Guest;
use crate::unit::testing::world_with;

const ALLO: u32 = 1;
const VELO: u32 = 2;
const GUEST: u32 = 3;
const NAMES: [(&str, i32); 5] = [
    ("allo", 5094),
    ("velocira", 5093),
    ("orca", 5400),
    ("manta", 5418),
    ("man", 9503),
];

fn cat() -> Catalogue {
    let mut cat = Catalogue::default();
    let keys = [
        "cPrey=5093\ncPrey=9503\ncPreattack=1\ncResetPreyPosition=1\ncEatsEggs=1",
        "cPreattack=1",
        "cPrey=5418",
        "",
    ];
    for ((name, id), keys) in NAMES.iter().zip(keys) {
        cat.push(unit_ty("animals", name, "m", *id, keys));
    }
    cat.push(unit_ty("guests", "man", "", 9503, ""));
    cat.chase_time_out = Some(10);
    cat
}

/// The animals `(id, type, tile)` on the ground, adults, and a man at (19, 19).
fn zoo(beasts: &[(u32, &str, (i32, i32))]) -> World {
    let mut w = world_with((20, 20), |zoo| {
        for &(id, kind, tile) in beasts {
            let mut a = entity("animals", kind, id, centre(tile), 0);
            a.subtype = "m".into();
            zoo.entities.push(a);
        }
        zoo.entities
            .push(entity("guests", "man", GUEST, centre((19, 19)), 0));
    });
    for &(id, ..) in beasts {
        w.units.get_mut(id).unwrap().part = Part::Animal(Animal::default());
    }
    w.units.get_mut(GUEST).unwrap().part = Part::Guest(Guest::default());
    w.units.ai_ticks = 100;
    w.units.class.defense_radius = 3;
    w
}

fn animal(w: &mut World, id: u32) -> &mut Animal {
    match &mut w.units.get_mut(id).unwrap().part {
        Part::Animal(a) => a,
        _ => unreachable!(),
    }
}

/// The animal is loose on the grounds, where it counts as a threat.
fn loose(w: &mut World, id: u32) {
    animal(w, id).escaped = true;
    w.units.escapes.push(id);
}

fn sets() -> Sets {
    let hold: &[&str] = &["fKeepControl()", "fKeepControl()"];
    let walk: &[&str] = &["fWalk(0,0)"];
    Sets::new(&[
        ("bWalk", walk),
        ("bRunFromPredator", walk),
        ("bChasePrey", walk),
        ("bPreattack", hold),
        ("bDefense", hold),
        ("bCaughtPrey", hold),
        ("bCaughtGuest", hold),
        ("bCaughtEgg", hold),
        ("bEatenEgg", hold),
        ("bCaughtBy", hold),
    ])
}

/// Runs `f` on the goal context of unit `id`, whose state is put back; `removed` asked to go.
fn on<R>(w: &mut World, id: u32, removed: &[u32], f: impl FnOnce(&mut Cx) -> R) -> R {
    let (cat, data, mut rng) = (cat(), sets(), Rng::default());
    let mut u = w.units.get(id).unwrap().clone();
    let at = w.find(id).unwrap();
    let (events, mut removals) = (Events::default(), removed.to_vec());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(id).unwrap() = u;
    r
}

/// One whole goal update of unit `id`; the generator after it.
fn step(w: &mut World, id: u32) -> Rng {
    let (cat, data, mut rng) = (cat(), sets(), Rng::default());
    let mut u = w.units.get(id).unwrap().clone();
    let at = w.find(id).unwrap();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    update(&mut u, w, &mut ctx, at, &data);
    *w.units.get_mut(id).unwrap() = u;
    rng
}

fn unit_of(w: &World, id: u32) -> &Unit {
    w.units.get(id).unwrap()
}

fn record(w: &World, id: u32) -> (u8, u8, u32) {
    let r = &unit_of(w, id).goal.record;
    (r.task, r.mode, r.target.id)
}

#[test]
fn an_afraid_animal_that_sees_a_threat_starts_the_avoid_and_its_goal_update_ends() {
    // 20.1 points 1.1 and 5: task 0x75 mode 3 on the allo, kept predator, stamp, no draw.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    loose(&mut w, ALLO);
    w.units.get_mut(VELO).unwrap().frightened = true;
    animal(&mut w, VELO).playing = true;
    animal(&mut w, VELO).prey = ALLO + 10;
    let rng = step(&mut w, VELO);
    assert_eq!(record(&w, VELO), (AVOID, 3, ALLO));
    let a = animal(&mut w, VELO).clone();
    assert_eq!(unit_of(&w, VELO).predator, ALLO);
    assert_eq!((a.prey, a.playing, a.stamp), (0, false, 100));
    assert_eq!(
        rng,
        Rng::default(),
        "a start ends the update before the draw"
    );
    // No threat: the flag and the kept predator go and the ladder goes on.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    w.units.get_mut(VELO).unwrap().frightened = true;
    w.units.get_mut(VELO).unwrap().predator = 77;
    step(&mut w, VELO);
    assert_eq!(unit_of(&w, VELO).predator, 0);
    assert!(!unit_of(&w, VELO).frightened);
    assert_ne!(record(&w, VELO).0, AVOID);
}

#[test]
fn a_playmate_that_chases_it_starts_the_avoid_unless_it_is_escaped() {
    // 20.1 points 1.2 and 1.3: the orca B hunts the orca A (B's kept prey is A).
    let two = |escaped| {
        let mut w = zoo(&[(ALLO, "orca", (10, 10)), (VELO, "orca", (13, 10))]);
        animal(&mut w, VELO).prey = ALLO;
        loose(&mut w, VELO);
        w.units.get_mut(ALLO).unwrap().frightened = true;
        animal(&mut w, ALLO).escaped = escaped;
        step(&mut w, ALLO);
        w
    };
    let w = two(false);
    assert_eq!(record(&w, ALLO), (AVOID, 3, VELO));
    assert_eq!(unit_of(&w, ALLO).predator, VELO);
    let w = two(true);
    assert!(unit_of(&w, ALLO).frightened, "afraid stays up");
    assert_ne!(record(&w, ALLO).0, AVOID);
}

#[test]
fn the_caught_rung_starts_the_avoid_with_the_kept_predator_and_no_abort() {
    // 20.1 point 2.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    loose(&mut w, ALLO);
    w.units.get_mut(VELO).unwrap().caught = true;
    animal(&mut w, VELO).playing = true;
    step(&mut w, VELO);
    assert_eq!(record(&w, VELO), (AVOID, 3, ALLO));
    assert!(!animal(&mut w, VELO).playing);
    // With a kept predator that is no threat any more the target is still the kept one.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    w.units.get_mut(VELO).unwrap().caught = true;
    w.units.get_mut(VELO).unwrap().predator = 55;
    step(&mut w, VELO);
    assert_eq!(record(&w, VELO), (AVOID, 3, 55));
}

#[test]
fn an_escaped_animal_hunts_the_prey_in_reach_and_the_update_ends() {
    // 20.1 point 3.3 and 7: the allo, escaped, a velocira 4 tiles away.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    loose(&mut w, ALLO);
    let rng = step(&mut w, ALLO);
    assert_eq!(record(&w, ALLO), (CHASE, 3, VELO));
    assert_eq!(rng, Rng::default());
    // The next update builds the goal: kept prey and stamp.
    step(&mut w, ALLO);
    let a = animal(&mut w, ALLO).clone();
    assert_eq!((a.prey, a.stamp), (VELO, 100));
    // A threat in sight, or the same prey already kept, starts nothing.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    loose(&mut w, ALLO);
    animal(&mut w, ALLO).prey = VELO;
    animal(&mut w, ALLO).stamp = 5;
    let hunt = on(&mut w, ALLO, &[], |cx| cx.hunt_rungs(RANDOM));
    assert_eq!(hunt, None);
}

#[test]
fn a_playing_animal_starts_the_friendly_chase_only_when_free() {
    // 20.1 point 3.4: playing, not escaped, an orca of its own type in reach, no kept prey.
    let mut w = zoo(&[(ALLO, "orca", (10, 10)), (VELO, "orca", (13, 10))]);
    animal(&mut w, ALLO).playing = true;
    on(&mut w, ALLO, &[], |cx| {
        assert_eq!(cx.hunt_rungs(RANDOM), Some(false))
    });
    assert_eq!(record(&w, ALLO), (CHASE, 3, VELO));
    let mut w = zoo(&[(ALLO, "orca", (10, 10)), (VELO, "orca", (13, 10))]);
    animal(&mut w, ALLO).playing = true;
    animal(&mut w, ALLO).prey = 99;
    animal(&mut w, ALLO).wants_food = true;
    assert_eq!(on(&mut w, ALLO, &[], |cx| cx.hunt_rungs(RANDOM)), None);
}

#[test]
fn the_chase_goal_keeps_its_prey_and_a_friendly_chase_ends_on_contact_or_timeout() {
    // 20.2 points 1, 4, 5 and 6, test (b).
    let chase = |apart: (i32, i32), now: u32| {
        let mut w = zoo(&[(ALLO, "orca", (10, 10)), (VELO, "orca", apart)]);
        animal(&mut w, ALLO).playing = true;
        w.units.get_mut(ALLO).unwrap().goal.record.task = CHASE;
        w.units.get_mut(ALLO).unwrap().goal.record.target.id = VELO;
        on(&mut w, ALLO, &[], |cx| {
            cx.hunt_init();
            cx.unit.goal.object = Some(Goal::Chase);
        });
        animal(&mut w, VELO).prey = 0;
        w.units.get_mut(VELO).unwrap().predator = ALLO;
        w.units.ai_ticks = now;
        w
    };
    let done = |w: &mut World| {
        on(w, ALLO, &[], |cx| {
            cx.hunt_check_done();
            cx.record().done
        })
    };
    let mut w = chase((16, 10), 100);
    assert_eq!(animal(&mut w, ALLO).prey, VELO);
    assert!(!done(&mut w));
    assert!(done(&mut chase((16, 10), 110)), "ten ticks after the init");
    assert!(done(&mut chase((10, 10), 101)), "the rectangles meet");
    // Complete: both links 0, task 0, playing 0.
    let mut w = chase((16, 10), 110);
    on(&mut w, ALLO, &[], |cx| {
        cx.record().done = true;
        cx.hunt_complete();
    });
    assert_eq!(unit_of(&w, VELO).predator, 0);
    let a = animal(&mut w, ALLO).clone();
    assert_eq!((a.prey, a.playing, record(&w, ALLO).0), (0, false, NONE));
}

#[test]
fn a_hunter_on_the_prey_catches_it_and_the_prey_stands_held() {
    // 20.3 points 5 and 8: the velocira is held and idle, the allo plays bCaughtPrey.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (10, 10))]);
    let set = |w: &mut World| {
        w.units.get_mut(ALLO).unwrap().goal.record.task = CHASE;
        w.units.get_mut(ALLO).unwrap().goal.record.target.id = VELO;
        on(w, ALLO, &[], |cx| cx.hunt_complete());
    };
    set(&mut w);
    let (h, p) = (unit_of(&w, ALLO), unit_of(&w, VELO));
    assert_eq!(h.runner.top(), Some("bCaughtPrey"));
    assert!(p.held && p.runner.top().is_none());
    assert_eq!(
        record(&w, ALLO).0,
        CHASE,
        "the task stays until the set ends"
    );
    assert!(animal(&mut w, ALLO).caught_prey);
    // While the set runs nothing ends; once it is over the chase stops.
    on(&mut w, ALLO, &[], |cx| cx.hunt_complete());
    assert!(animal(&mut w, ALLO).caught_prey);
    on(&mut w, ALLO, &[], |cx| {
        cx.unit.runner = Default::default();
        cx.hunt_complete();
    });
    assert_eq!(
        (record(&w, ALLO).0, animal(&mut w, ALLO).playing),
        (NONE, false)
    );
    // A held prey is not caught again: the chase just stops.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (10, 10))]);
    w.units.get_mut(VELO).unwrap().held = true;
    set(&mut w);
    assert_eq!(record(&w, ALLO).0, NONE);
}

#[test]
fn a_hunter_eats_an_egg_and_catches_a_guest() {
    // 20.3 points 4 and 3.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (10, 10))]);
    animal(&mut w, VELO).egg = true;
    w.units.get_mut(ALLO).unwrap().goal.record.target.id = VELO;
    on(&mut w, ALLO, &[], |cx| cx.hunt_complete());
    assert_eq!(unit_of(&w, VELO).runner.top(), Some("bEatenEgg"));
    assert!(animal(&mut w, VELO).dying);
    assert_eq!(unit_of(&w, ALLO).runner.top(), Some("bCaughtEgg"));
    assert!(animal(&mut w, ALLO).caught_prey);
    let mut w = zoo(&[(ALLO, "allo", (10, 10))]);
    w.units.get_mut(ALLO).unwrap().goal.record.target.id = GUEST;
    animal(&mut w, ALLO).prey = GUEST;
    on(&mut w, ALLO, &[], |cx| cx.hunt_complete());
    let g = unit_of(&w, GUEST);
    assert!(g.held && g.caught);
    assert_eq!((g.predator, record(&w, GUEST)), (ALLO, (AVOID, 3, ALLO)));
    assert_eq!(g.runner.top(), Some("bCaughtBy"));
    assert_eq!(w.units.thoughts.of(GUEST)[0].text, 10246);
    let a = animal(&mut w, ALLO).clone();
    assert!(a.caught_prey && unit_of(&w, ALLO).held && a.prey == 0);
    assert_eq!(unit_of(&w, ALLO).runner.top(), Some("bCaughtGuest"));
    // Taken to the hunter's place, and no prey for another hunter any more.
    let at = w.find(GUEST).unwrap();
    assert_eq!(w.zoo.entities[at].common.as_ref().unwrap().x, 10 * 64 + 32);
    assert!(matches!(&w.units.get(GUEST).unwrap().part, Part::Guest(g) if g.seized));
}

#[test]
fn the_hunter_in_range_starts_the_preattack_and_the_prey_its_defence() {
    // 20.4 point 6: an allo hunting a velocira within 3 tiles; the velocira fleeing it.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    animal(&mut w, ALLO).playing = true;
    w.units.get_mut(ALLO).unwrap().goal.record.task = CHASE;
    w.units.get_mut(ALLO).unwrap().goal.record.target.id = VELO;
    step(&mut w, ALLO);
    let h = unit_of(&w, ALLO);
    assert_eq!(
        (h.goal.record.job, h.runner.top()),
        (18, Some("bPreattack"))
    );
    assert!(h.held && unit_of(&w, VELO).held);
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    loose(&mut w, ALLO);
    w.units.get_mut(VELO).unwrap().frightened = true;
    w.units.get_mut(VELO).unwrap().goal.record.task = AVOID;
    w.units.get_mut(VELO).unwrap().goal.record.target.id = ALLO;
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    step(&mut w, VELO);
    let p = unit_of(&w, VELO);
    assert_eq!((p.goal.record.job, p.runner.top()), (18, Some("bDefense")));
    // The orca has no cPreattack: its hunt has no job.
    let mut w = zoo(&[(ALLO, "orca", (10, 10)), (VELO, "manta", (12, 10))]);
    animal(&mut w, ALLO).playing = true;
    w.units.get_mut(ALLO).unwrap().goal.record.task = CHASE;
    w.units.get_mut(ALLO).unwrap().goal.record.target.id = VELO;
    step(&mut w, ALLO);
    assert_eq!(unit_of(&w, ALLO).goal.record.job, 0);
}

#[test]
fn a_range_of_the_preattack_is_read_in_tiles_and_wraps() {
    // 20.4 point 2: distance 3 is in range, 4 is not; radius 0, negative, huge.
    let range = |radius: i32, at: (i32, i32)| {
        let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", at)]);
        w.units.class.defense_radius = radius;
        on(&mut w, ALLO, &[], |cx| cx.in_range(VELO))
    };
    assert!(range(3, (13, 10)));
    assert!(!range(3, (14, 10)));
    assert!(range(0, (10, 10)));
    assert!(!range(0, (11, 10)));
    assert!(range(-3, (13, 10)));
    assert!(range(i32::MAX, (10, 10)), "the square wraps to 1");
    assert!(!range(i32::MAX, (12, 10)));
}

#[test]
fn the_avoid_of_an_animal_ends_when_the_threat_is_gone_or_after_a_friendly_chase() {
    // 20.6 point 6: a velocira avoiding an allo that was removed.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    w.units.get_mut(VELO).unwrap().frightened = true;
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    w.units.get_mut(VELO).unwrap().goal.record.task = AVOID;
    w.units.get_mut(VELO).unwrap().goal.record.target.id = ALLO;
    on(&mut w, VELO, &[ALLO], |cx| cx.avoid_check_done());
    assert_eq!(record(&w, VELO), (NONE, 0, 0));
    let u = unit_of(&w, VELO);
    assert_eq!((u.frightened, u.predator), (false, 0));
    // Two orcas: the chased one stops ten ticks after the stamp with the chaser in reach, and the
    // chaser lets go of it.
    let mut w = zoo(&[(ALLO, "orca", (10, 10)), (VELO, "orca", (11, 10))]);
    animal(&mut w, ALLO).prey = VELO;
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    w.units.get_mut(VELO).unwrap().frightened = true;
    w.units.get_mut(VELO).unwrap().goal.record.task = AVOID;
    w.units.get_mut(VELO).unwrap().goal.record.target.id = ALLO;
    animal(&mut w, VELO).stamp = 91;
    on(&mut w, VELO, &[], |cx| cx.avoid_check_done());
    assert_eq!(record(&w, VELO).0, AVOID, "nine ticks");
    animal(&mut w, VELO).stamp = 90;
    on(&mut w, VELO, &[], |cx| cx.avoid_check_done());
    assert_eq!(record(&w, VELO).0, NONE);
    assert_eq!(animal(&mut w, ALLO).prey, 0);
    // A caught unit waits for its set to end.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    let v = w.units.get_mut(VELO).unwrap();
    (v.caught, v.goal.record.task, v.goal.record.target.id) = (true, AVOID, ALLO);
    on(&mut w, VELO, &[], |cx| {
        cx.start_set("bCaughtBy", true);
        cx.avoid_check_done();
    });
    assert!(unit_of(&w, VELO).caught);
    w.units.get_mut(VELO).unwrap().runner = Default::default();
    on(&mut w, VELO, &[], |cx| cx.avoid_check_done());
    assert!(!unit_of(&w, VELO).caught);
    assert_eq!(record(&w, VELO).0, NONE);
}

#[test]
fn a_goal_reset_lets_go_of_the_hunt() {
    // 12.1: playing, caught prey, the kept prey and the kept predator go with the reset.
    let mut w = zoo(&[(ALLO, "allo", (10, 10))]);
    let a = animal(&mut w, ALLO);
    (a.playing, a.caught_prey, a.prey) = (true, true, 7);
    w.units.get_mut(ALLO).unwrap().predator = 8;
    on(&mut w, ALLO, &[], |cx| cx.reset());
    let a = animal(&mut w, ALLO).clone();
    assert_eq!((a.playing, a.caught_prey, a.prey), (false, false, 0));
    assert_eq!(unit_of(&w, ALLO).predator, 0);
}

#[test]
fn an_afraid_or_playing_animal_answers_no_call_to_a_show() {
    // Shows 21.2 point 1: the ladder ends before the call is read.
    for (afraid, playing) in [(true, false), (false, true)] {
        let mut w = zoo(&[(ALLO, "orca", (10, 10))]);
        animal(&mut w, ALLO).playing = playing;
        animal(&mut w, ALLO).escaped = afraid;
        let u = w.units.get_mut(ALLO).unwrap();
        (u.frightened, u.called) = (afraid, true);
        assert!(on(&mut w, ALLO, &[], |cx| cx.animal_ladder(RANDOM)));
        assert_eq!(record(&w, ALLO).0, NONE, "{afraid} {playing}");
    }
    let mut w = zoo(&[(ALLO, "orca", (10, 10))]);
    w.units.get_mut(ALLO).unwrap().called = true;
    on(&mut w, ALLO, &[], |cx| cx.animal_ladder(RANDOM));
    assert_eq!(record(&w, ALLO).0, crate::goal::GO_TO_SHOW);
}

#[test]
fn a_chase_with_no_path_stops_forced_and_the_prey_lets_go() {
    // 20.2 points 3 and 6: the target is gone, so there is no path; the kept prey is of another
    // type and lets go all the same.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    animal(&mut w, ALLO).prey = VELO;
    animal(&mut w, ALLO).playing = true;
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    let u = w.units.get_mut(ALLO).unwrap();
    (u.goal.record.task, u.goal.record.target.id, u.held) = (CHASE, 99, true);
    on(&mut w, ALLO, &[], |cx| cx.hunt_decide());
    assert_eq!(unit_of(&w, VELO).predator, 0);
    let a = animal(&mut w, ALLO).clone();
    assert_eq!((a.prey, a.playing), (0, false));
    assert_eq!((record(&w, ALLO).0, unit_of(&w, ALLO).held), (NONE, false));
    // The end of a chase that is not forced leaves the predator of a prey of another type.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    animal(&mut w, ALLO).prey = VELO;
    w.units.get_mut(VELO).unwrap().predator = ALLO;
    on(&mut w, ALLO, &[], |cx| cx.stop_chase(false));
    assert_eq!(unit_of(&w, VELO).predator, ALLO);
}

#[test]
fn a_hunter_gives_up_a_guest_already_seized() {
    // 20.3 points 1 and 3: nothing is done to the guest, no set starts, nothing is thought.
    let give_up = |seized: bool, caught: bool| {
        let mut w = zoo(&[(ALLO, "allo", (10, 10))]);
        let g = w.units.get_mut(GUEST).unwrap();
        (g.caught, g.predator) = (caught, if caught { 77 } else { 0 });
        if let Part::Guest(g) = &mut g.part {
            g.seized = seized;
        }
        let h = w.units.get_mut(ALLO).unwrap();
        (h.goal.record.task, h.goal.record.target.id) = (CHASE, GUEST);
        animal(&mut w, ALLO).prey = GUEST;
        on(&mut w, ALLO, &[], |cx| cx.hunt_complete());
        let (h, g) = (unit_of(&w, ALLO), unit_of(&w, GUEST));
        assert_eq!(record(&w, ALLO).0, NONE, "{seized} {caught}");
        assert!(!h.held && h.runner.top().is_none() && !g.held && g.runner.top().is_none());
        assert!(w.units.thoughts.of(GUEST).is_empty());
        assert_eq!(g.predator, if caught { 77 } else { 0 });
        assert!(!animal(&mut w, ALLO).caught_prey);
    };
    give_up(true, false);
    give_up(false, true);
}

#[test]
fn an_animal_defends_once_per_avoid_goal() {
    // 20.4 point 4: the velocira avoiding an allo within 3 tiles defends, and never again.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (12, 10))]);
    let u = w.units.get_mut(VELO).unwrap();
    (u.goal.record.task, u.goal.record.target.id) = (AVOID, ALLO);
    let across = w.tile_index(3, 3);
    let mut u = unit_of(&w, VELO).clone();
    u.aim(&w, across);
    *w.units.get_mut(VELO).unwrap() = u;
    on(&mut w, VELO, &[], |cx| cx.defence());
    assert_eq!(unit_of(&w, VELO).goal.record.job, 18);
    assert_eq!(animal(&mut w, VELO).pre, [true, false]);
    // The defence empties the own target tile too (shows 32.2 point 4).
    assert_eq!(unit_of(&w, VELO).own_tile, None);
    assert_eq!(unit_of(&w, VELO).own_pos, [-1; 3]);
    // The job is over: the goal marks the defence over and does not start another.
    on(&mut w, VELO, &[], |cx| {
        cx.set_job(0, 0, None);
        cx.defence();
    });
    assert_eq!(unit_of(&w, VELO).goal.record.job, 0);
    assert_eq!(animal(&mut w, VELO).pre, [true, true]);
    on(&mut w, VELO, &[], |cx| cx.defence());
    assert_eq!(unit_of(&w, VELO).goal.record.job, 0);
}

#[test]
fn the_chase_walks_on_when_the_preattack_job_is_gone() {
    // 20.2 point 2.1, 20.4 point 5: with the job running nothing is decided; once it is gone the
    // preattack is over and the chase starts its walk.
    let mut w = zoo(&[(ALLO, "allo", (10, 10)), (VELO, "velocira", (14, 10))]);
    let u = w.units.get_mut(ALLO).unwrap();
    (u.goal.record.task, u.goal.record.target.id) = (CHASE, VELO);
    animal(&mut w, ALLO).pre = [true, false];
    let walk = |w: &mut World, job: u8| {
        w.units.get_mut(ALLO).unwrap().goal.record.job = job;
        on(w, ALLO, &[], |cx| {
            cx.hunt_check_done();
            cx.hunt_decide();
        });
        (
            animal(w, ALLO).pre,
            unit_of(w, ALLO).runner.top().map(String::from),
        )
    };
    assert_eq!(walk(&mut w, 18), ([true, false], None));
    assert_eq!(
        walk(&mut w, 0),
        ([true, true], Some("bChasePrey".to_string()))
    );
}
