//! The keeper's capture of an escaped animal on a 20 by 20 map: the zookeeper at (10, 12), a zebra
//! at (13, 10) loose on the grounds, and the species that need the helicopter and the scientist.

use super::*;
use crate::animal::Animal;
use crate::entity::EntityType;
use crate::fixtures::unit_ty;
use crate::unit::Staff;

const KEEPER: u32 = 2;
const BEAST: u32 = 50;
const OTHER: u32 = 51;
const CAUGHT: u32 = 777;

fn ty(class: &'static str, name: &str, id: i32, text: &str) -> EntityType {
    unit_ty(class, name, "m", id, text)
}

/// The zookeeper's weapon range is `range`, a zebra has 80 energy and a heron needs the helicopter.
fn types(range: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(ty("keeper", "keeper", 9500, range));
    cat.push(ty("keeper", "scient", 9551, ""));
    cat.push(ty("animals", "zebra", 5100, "cMaxEnergy=80"));
    cat.push(ty("animals", "heron", 5200, "cHeliRecovery=1"));
    cat.push(ty("animals", "ibis", 5300, "cHeliRecovery=2"));
    cat.push(ty("animals", "kiwi", 5400, "cHeliRecovery=-1"));
    cat.push(ty("animals", "trex", 7001, "cPrey=9500"));
    cat
}

/// The keeper and the animals `(id, type, tile)`, all loose; every countdown far from firing.
fn zoo(animals: &[(u32, &str, (i32, i32))]) -> World {
    let mut w = world((20, 20), |_, e| {
        let mut keeper = entity("keeper", "keeper", KEEPER, centre((10, 12)), 0);
        keeper.subtype = "m".into();
        e.push(keeper);
        for &(id, kind, tile) in animals {
            let mut a = entity("animals", kind, id, centre(tile), 0);
            a.subtype = "m".into();
            e.push(a);
        }
    });
    w.units.get_mut(KEEPER).unwrap().part = Part::Staff(Staff {
        chase: 1000,
        ..Staff::default()
    });
    for &(id, ..) in animals {
        w.units.get_mut(id).unwrap().part = Part::Animal(Animal {
            escaped: true,
            ..Animal::default()
        });
        w.units.escapes.push(id);
    }
    w
}

fn sets() -> Sets {
    Sets::new(&[
        ("bChaseAnimal", &["fWalk(0,0)"]),
        ("bRetrieveAnimal", &["fWalk(0,0)"]),
        ("bFireWeapon", &["fKeepControl()"]),
        ("bWalk", &["fWalk(0,0)"]),
    ])
}

/// Runs `f` on the goal context of unit `id` with the types of `cat`.
fn with<R>(w: &mut World, cat: &Catalogue, id: u32, f: impl FnOnce(&mut Cx) -> R) -> R {
    let mut u = w.units.get(id).unwrap().clone();
    let at = w.find(id).unwrap();
    let (mut rng, events, mut removals) = (Rng::default(), Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    let data = sets();
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

/// One goal update of the keeper without the common part's draw: the ladder, the goal, the job.
fn tick(w: &mut World, cat: &Catalogue) {
    with(w, cat, KEEPER, |cx| {
        let r = cx.record().clone();
        cx.keeper_ladder(r.task, r.done);
        cx.run_goal();
        cx.run_sub();
    });
}

fn animal(w: &mut World, id: u32) -> &mut Animal {
    match &mut w.units.get_mut(id).unwrap().part {
        Part::Animal(a) => a,
        _ => unreachable!(),
    }
}

fn captured(w: &World) -> u32 {
    match &w.units.get(KEEPER).unwrap().part {
        Part::Staff(s) => s.captured,
        _ => unreachable!(),
    }
}

fn record(w: &World) -> Record {
    w.units.get(KEEPER).unwrap().goal.record.clone()
}

/// Moves the keeper to tile `to`.
fn put(w: &mut World, to: (i32, i32)) {
    let at = w.find(KEEPER).unwrap();
    let (x, y) = centre(to);
    w.place_unit(at, (x, y, 0), 0);
}

#[test]
fn a_keeper_who_finds_a_loose_animal_stops_everything_and_takes_the_capture_task() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    with(&mut w, &cat, KEEPER, |cx| {
        cx.set_task(HABITAT, 4, 0, None);
        assert!(cx.start_set("bWalk", true) && cx.unit.runner.running());
        cx.keeper_ladder(HABITAT, false);
        // The walk set is dropped; the goal is not built yet.
        assert!(!cx.unit.runner.running());
        let r = cx.record();
        assert_eq!((r.task, r.mode, r.target.id, r.job), (CAPTURE, 3, BEAST, 0));
    });
    assert_eq!((captured(&w), animal(&mut w, BEAST).claim), (BEAST, KEEPER));
    // The goal's init gives the chase.
    with(&mut w, &cat, KEEPER, |cx| cx.run_goal());
    assert_eq!(record(&w).job, 13);
    // Nothing loose: the round of the habitats goes on.
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    animal(&mut w, BEAST).escaped = false;
    tick(&mut w, &cat);
    assert_ne!(record(&w).task, CAPTURE);
    assert_eq!(captured(&w), 0);
}

#[test]
fn the_nearest_animal_by_the_manhattan_distance_is_taken_and_a_live_claim_is_skipped() {
    let cat = types("");
    let animals = [(BEAST, "zebra", (13, 10)), (OTHER, "zebra", (11, 12))];
    let found = |claim: u32, other: u32| {
        let mut w = zoo(&animals);
        (animal(&mut w, BEAST).claim, animal(&mut w, OTHER).claim) = (claim, other);
        tick(&mut w, &cat);
        captured(&w)
    };
    assert_eq!(found(0, 0), OTHER, "1 against 5");
    // A claim of a unit that exists (the keeper's own included) keeps the animal for him.
    assert_eq!(found(0, KEEPER), BEAST);
    assert_eq!(found(0, BEAST), BEAST, "the animal itself is a live entity");
    // One of a unit that is gone does not.
    assert_eq!(found(CAUGHT, CAUGHT), OTHER);
    // Of equal distances the first of the escaped list wins.
    let mut w = zoo(&[(BEAST, "zebra", (12, 12)), (OTHER, "zebra", (8, 12))]);
    tick(&mut w, &cat);
    assert_eq!(captured(&w), BEAST);
    let mut w = zoo(&[(BEAST, "zebra", (12, 12)), (OTHER, "zebra", (8, 12))]);
    w.units.escapes.reverse();
    tick(&mut w, &cat);
    assert_eq!(captured(&w), OTHER);
    // The capture id is cleared by a search that finds nothing.
    let mut w = zoo(&animals);
    tick(&mut w, &cat);
    for id in [BEAST, OTHER] {
        animal(&mut w, id).escaped = false;
    }
    with(&mut w, &cat, KEEPER, |cx| {
        cx.set_task(NONE, 0, 0, None);
        cx.keeper_ladder(NONE, false);
    });
    assert_eq!(captured(&w), 0);
}

#[test]
fn the_zookeeper_takes_what_needs_no_helicopter_and_the_scientist_what_a_helicopter_darted() {
    let cat = types("");
    // (type of the animal, darted, type of the keeper) -> captures
    let cases = [
        ("zebra", false, "keeper", true),
        ("zebra", true, "keeper", true),
        ("zebra", false, "scient", false),
        ("heron", false, "keeper", false),
        ("heron", true, "keeper", false),
        ("heron", false, "scient", false),
        ("heron", true, "scient", true),
    ];
    for (kind, darted, member, takes) in cases {
        let mut w = zoo(&[(BEAST, kind, (13, 10))]);
        animal(&mut w, BEAST).tranq = darted;
        let at = w.find(KEEPER).unwrap();
        w.zoo.entities[at].type_name = member.into();
        tick(&mut w, &cat);
        assert_eq!(
            record(&w).task == CAPTURE,
            takes,
            "{kind} {darted} {member}"
        );
        // A scientist takes a darted animal at its crate.
        if takes && member == "scient" {
            assert_eq!(record(&w).job, 15);
        }
    }
}

#[test]
fn an_animal_on_water_is_left_alone() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    w.set_water(vec![false, true]);
    w.zoo.tiles[10 * 20 + 13].terrain = 1;
    tick(&mut w, &cat);
    assert_ne!(record(&w).task, CAPTURE);
    assert_eq!(captured(&w), 0);
}

#[test]
fn the_keeper_closes_in_on_a_path_that_may_be_partial_and_fires_within_his_weapon_range() {
    // The animal is 13 squared tiles away.
    let ranges = [
        ("cWeaponRange=3", false),
        ("cWeaponRange=4", true),
        ("cWeaponRange=-4", true),
        ("cWeaponRange=0", false),
        ("cWeaponRange=2147483647", false),
    ];
    for (key, in_range) in ranges {
        let cat = types(key);
        let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
        tick(&mut w, &cat);
        let u = w.units.get(KEEPER).unwrap();
        let r = &u.goal.record;
        if in_range {
            // He stops, faces the animal and fires; the dart follows.
            assert_eq!((r.job, u.runner.top()), (14, Some("bFireWeapon")), "{key}");
            assert!(u.path.is_empty() && !u.moving, "{key}");
            continue;
        }
        assert_eq!((r.job, u.runner.top()), (13, Some("bChaseAnimal")), "{key}");
        assert!(!u.path.is_empty() && r.job_target.tile.is_some(), "{key}");
        assert_eq!(r.job_target.pos, u.destination, "{key}");
    }
    // The leg ends where he stands on the tile he aimed at, and the animal is elsewhere.
    let cat = types("cWeaponRange=0");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    tick(&mut w, &cat);
    let aim = record(&w).job_target.tile.unwrap();
    let at = w.find(BEAST).unwrap();
    w.place_unit(at, (centre((16, 10)).0, centre((16, 10)).1, 0), 0);
    put(&mut w, ((aim % 20) as i32, (aim / 20) as i32));
    tick(&mut w, &cat);
    assert_eq!((record(&w).job, record(&w).job_target.tile), (13, None));
}

#[test]
fn only_a_heli_recovery_of_one_calls_for_the_helicopter() {
    let cat = types("");
    for (kind, takes) in [
        ("heron", false),
        ("ibis", true),
        ("kiwi", true),
        ("zebra", true),
    ] {
        let mut w = zoo(&[(BEAST, kind, (13, 10))]);
        tick(&mut w, &cat);
        assert_eq!(record(&w).task == CAPTURE, takes, "{kind}");
    }
}

#[test]
fn a_staff_type_without_a_weapon_range_darts_within_five_tiles() {
    let cat = types("");
    // 13 squared tiles away: inside the range of 5 (25).
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    tick(&mut w, &cat);
    assert_eq!(record(&w).job, 14);
    // 6 tiles away (36): he closes in.
    let mut w = zoo(&[(BEAST, "zebra", (16, 12))]);
    tick(&mut w, &cat);
    assert_eq!(record(&w).job, 13);
}

#[test]
fn the_first_qualifying_animal_is_taken_when_no_distance_is_known() {
    let cat = types("");
    let lost = |w: &mut World, id: u32| {
        let at = w.find(id).unwrap();
        w.zoo.entities[at].common.as_mut().unwrap().x = -100_000;
    };
    let found = |w: &mut World| {
        with(w, &cat, KEEPER, |cx| cx.keeper_ladder(NONE, false));
        captured(w)
    };
    let two = [(BEAST, "zebra", (13, 10)), (OTHER, "zebra", (11, 12))];
    // A keeper with no tile takes the first, and so does a lone animal with no tile.
    let mut w = zoo(&two);
    lost(&mut w, KEEPER);
    assert_eq!(found(&mut w), BEAST);
    let mut w = zoo(&two[..1]);
    lost(&mut w, BEAST);
    assert_eq!(found(&mut w), BEAST);
    // One with a tile replaces a first with none, but a first with a tile keeps its place against
    // a later one with none.
    let mut w = zoo(&two);
    lost(&mut w, BEAST);
    assert_eq!(found(&mut w), OTHER);
    let mut w = zoo(&two);
    lost(&mut w, OTHER);
    assert_eq!(found(&mut w), BEAST);
}

#[test]
fn the_dart_puts_the_animal_to_sleep_and_a_crate_follows_for_a_zookeeper() {
    let cat = types("cWeaponRange=4");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    let u = w.units.get_mut(BEAST).unwrap();
    (u.wait, u.moving, u.goal.record.task) = (i32::MAX, true, 1);
    animal(&mut w, BEAST).drinking = true;
    // The first update closes in, fires and starts the job of the dart, which goes on while the
    // set plays.
    tick(&mut w, &cat);
    tick(&mut w, &cat);
    assert_eq!((record(&w).job, animal(&mut w, BEAST).tranq), (14, false));
    w.units.get_mut(KEEPER).unwrap().runner = Default::default();
    tick(&mut w, &cat);
    let u = w.units.get(BEAST).unwrap();
    assert_eq!((u.wait, u.moving, u.goal.record.task), (0, false, 0));
    let a = animal(&mut w, BEAST);
    assert_eq!(
        (a.tranq, a.energy, a.counters[crate::animal::ENERGY]),
        (true, 80, 0)
    );
    assert!(!a.drinking && a.claim == KEEPER);
    assert_eq!(record(&w).job, 15);
}

#[test]
fn a_darted_animal_is_crated_where_it_stands_and_the_claim_and_the_task_go() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (11, 12))]);
    animal(&mut w, BEAST).tranq = true;
    tick(&mut w, &cat);
    assert_eq!(record(&w).job, 15);
    // He is beside it: the crate is done at once and the job stays done.
    let a = animal(&mut w, BEAST).clone();
    assert!(a.crated && !a.escaped && !a.tranq && a.claim == 0);
    assert_eq!(record(&w).task, CAPTURE);
    assert!(record(&w).job_done);
    // The next update finds him unable to capture it any more: the goal resets.
    tick(&mut w, &cat);
    assert_eq!((record(&w).task, captured(&w)), (NONE, 0));
}

#[test]
fn the_keepers_crate_snaps_with_the_crate_footprint_and_lets_go_of_prey_and_predator() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (11, 12)), (OTHER, "zebra", (3, 3))]);
    let at = w.find(BEAST).unwrap();
    let mut shape = w.shape(at).unwrap();
    shape.forms = Some([(1, 2), (4, 2)]);
    w.retype(at, "m", shape);
    // Darted off the quarter centres, facing 2, holding the meal OTHER; a predator is on it.
    let (x, y) = (11 * 64 + 41, 12 * 64 + 9);
    w.place_unit(at, (x, y, 0), 2);
    let a = animal(&mut w, BEAST);
    (a.tranq, a.caught_prey, a.prey) = (true, true, OTHER);
    w.units.get_mut(BEAST).unwrap().predator = KEEPER;
    w.units.get_mut(OTHER).unwrap().predator = BEAST;
    tick(&mut w, &cat);
    let a = animal(&mut w, BEAST).clone();
    assert!(a.crated && !a.escaped && !a.tranq && a.claim == 0);
    assert!(!a.caught_prey && a.prey == 0);
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    // The crate is 4 by 2 half tiles turned to 2 by 4: the snap is (32, 0) in the tile.
    assert_eq!((c.x, c.y, c.rotation), (11 * 64 + 32, 12 * 64, 2));
    assert_eq!(w.units.get(BEAST).unwrap().predator, 0);
    assert_eq!(w.units.get(OTHER).unwrap().predator, 0);
}

#[test]
fn a_far_darted_animal_is_walked_to_and_one_that_woke_is_chased_again() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    animal(&mut w, BEAST).tranq = true;
    tick(&mut w, &cat);
    tick(&mut w, &cat);
    let u = w.units.get(KEEPER).unwrap();
    assert_eq!(
        (u.goal.record.job, u.runner.top()),
        (15, Some("bRetrieveAnimal"))
    );
    assert!(!u.goal.record.job_done);
    animal(&mut w, BEAST).tranq = false;
    tick(&mut w, &cat);
    assert_eq!(record(&w).job, 13);
}

#[test]
fn a_replaced_capture_task_gives_the_animal_back() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    tick(&mut w, &cat);
    assert_eq!(animal(&mut w, BEAST).claim, KEEPER);
    with(&mut w, &cat, KEEPER, |cx| {
        cx.set_task(RANDOM, 1, 0, None);
        cx.run_goal();
    });
    assert_eq!((captured(&w), animal(&mut w, BEAST).claim), (0, 0));
    // A target that is no animal refuses the task at its init.
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    with(&mut w, &cat, KEEPER, |cx| {
        cx.set_task(CAPTURE, 3, KEEPER, None);
        cx.run_goal();
        assert_eq!(cx.record().task, NONE);
    });
}

#[test]
fn a_keeper_picked_up_frees_the_animal_he_was_after() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    tick(&mut w, &cat);
    crate::staff::leaves(&mut w, KEEPER);
    assert_eq!((captured(&w), animal(&mut w, BEAST).claim), (0, 0));
}

#[test]
fn a_darted_animal_is_no_threat_to_staff() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "trex", (13, 10))]);
    for darted in [false, true] {
        animal(&mut w, BEAST).tranq = darted;
        let seen = with(&mut w, &cat, KEEPER, |cx| cx.predator(0));
        assert_eq!(seen, if darted { 0 } else { BEAST });
    }
}

#[test]
fn a_darted_animal_runs_no_goal_update() {
    let cat = types("");
    let mut w = zoo(&[(BEAST, "zebra", (13, 10))]);
    for darted in [false, true] {
        animal(&mut w, BEAST).tranq = darted;
        let goals = with(&mut w, &cat, BEAST, |cx| {
            cx.set_task(RANDOM, 1, 0, None);
            let more = cx.ladder();
            (more, cx.record().task)
        });
        assert_eq!(
            goals,
            if darted {
                (false, NONE)
            } else {
                (true, RANDOM)
            }
        );
    }
}

#[test]
fn a_zookeeper_darts_and_crates_a_loose_animal_in_a_run() {
    let cat = types("cWeaponRange=3");
    let mut w = zoo(&[(BEAST, "zebra", (14, 10))]);
    // The animal keeps still and its checks never fire.
    animal(&mut w, BEAST).counters = [1_000_000; 16];
    let data = Sets::new(&[
        ("bChaseAnimal", &["fWalk(0,0)"]),
        ("bRetrieveAnimal", &["fWalk(0,0)"]),
        ("bFireWeapon", &["fPlay(a)"]),
        ("bWalk", &["fWalk(0,0)"]),
        ("bSleep", &["fKeepControl()"]),
    ]);
    let mut s = sim();
    let mut log = Vec::new();
    for _ in 0..600 {
        s.step_with(&data, &cat, &mut w, 67, |_, _, _| {});
        log.extend(w.units.take_lines());
        log.extend(w.units.take_staff_lines());
        if animal(&mut w, BEAST).crated {
            break;
        }
    }
    let a = animal(&mut w, BEAST).clone();
    assert!(a.crated && !a.escaped && a.claim == 0, "{log:?}");
    assert!(
        log.iter()
            .any(|l| l == &format!("goal {KEEPER} capturing 3 {BEAST} -"))
    );
    for job in ["ChaseAnimal", "TranquilizeAnimal", "RetrieveAnimal"] {
        assert!(
            log.iter().any(|l| l == &format!("job {KEEPER} {job}")),
            "{job}"
        );
    }
}

#[test]
fn the_dart_stops_the_chase_of_a_hunter_unless_it_holds_caught_prey() {
    // Animals 21.3: a hunter without caught prey stops its hunt, forced; one with the prey in its
    // jaws is only put to sleep and its meal goes on.
    let cat = types("cWeaponRange=4");
    let dart = |caught: bool| {
        let mut w = zoo(&[(BEAST, "zebra", (13, 10)), (OTHER, "kiwi", (14, 10))]);
        let u = w.units.get_mut(BEAST).unwrap();
        (u.goal.record.task, u.held, u.hidden) = (0x74, true, true);
        let a = animal(&mut w, BEAST);
        (a.prey, a.playing, a.caught_prey) = (OTHER, true, caught);
        w.units.get_mut(OTHER).unwrap().predator = BEAST;
        w.units.get_mut(KEEPER).unwrap().goal.record.job_target.id = BEAST;
        with(&mut w, &cat, KEEPER, |cx| cx.dart_complete());
        w
    };
    let mut w = dart(false);
    let hunter = w.units.get(BEAST).unwrap();
    assert_eq!(
        (hunter.goal.record.task, hunter.held, hunter.hidden),
        (0, false, false)
    );
    assert_eq!(w.units.get(OTHER).unwrap().predator, 0);
    let a = animal(&mut w, BEAST);
    assert_eq!((a.tranq, a.prey, a.playing, a.energy), (true, 0, false, 80));
    let mut w = dart(true);
    let hunter = w.units.get(BEAST).unwrap();
    assert_eq!((hunter.goal.record.task, hunter.held), (0x74, true));
    assert_eq!(w.units.get(OTHER).unwrap().predator, BEAST);
    let a = animal(&mut w, BEAST);
    assert_eq!(
        (a.tranq, a.prey, a.playing, a.energy),
        (true, OTHER, true, 80)
    );
}
