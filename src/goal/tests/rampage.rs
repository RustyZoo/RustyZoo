//! A rampaging animal chooses a building to charge (animals 11.11, shows 30.3 point 4) on a
//! synthetic map: a lion 1 at tile (0, 0), a bench 8 at (3, 4) (25 squared tiles away) and a stand 7
//! at (6, 8) (100).

use super::*;
use crate::animal::Animal;
use crate::behaviour::Flow;
use crate::fixtures::ty;
use crate::unit::testing::world_with;

/// A lion whose rampage lasts.
const LION: &str = "[Characteristics/Integers]\ncRampageTimeMax=100000\n";
/// A bench and a hot dog stand: both serve the need `building`, and they take any animal.
const OPEN: &str = "[Characteristics/Integers]\ncCapacity=2\n[Satisfies]\nbuilding\nfood\n\
    [slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\n\
    exitpos=48\ncapacity=2\n";
/// A grandstand of the shipped data: its only slot lists the four guest ids.
const GUESTS: &str = "[Characteristics/Integers]\ncCapacity=2\n[Satisfies]\nbuilding\ngrandstand\n\
    [slots]\nname=slot\n[slot]\nid=9503\nid=9504\nid=9505\nid=9506\nslotpos=0\nslotpos=0\n\
    entrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=2\n";

fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(ty("animals", "lion", "m", LION));
    for (name, text) in [("bench", OPEN), ("stand", OPEN), ("gstand", GUESTS)] {
        cat.push(ty("building", name, "", text));
    }
    cat
}

/// The lion at tile (0, 0), rampaging when `rampaging`, and the buildings `(type, id, tile)`.
fn zoo(rampaging: bool, buildings: &[(&str, u32, (i32, i32))]) -> World {
    let mut w = world_with((10, 10), |zoo| {
        let mut lion = entity("animals", "lion", 1, centre((0, 0)), 0);
        lion.subtype = "m".into();
        zoo.entities.push(lion);
        for &(kind, id, at) in buildings {
            let b = entity("building", kind, id, centre(at), 4);
            zoo.entities.push(b);
        }
    });
    let u = w.units.get_mut(1).unwrap();
    u.started = true;
    if let Part::Animal(a) = &mut u.part {
        (a.counters, a.life, a.baby) = ([1000; 16], 5000, 5000);
        (a.rampaging, a.happiness) = (rampaging, -500);
    }
    w
}

fn animal(w: &World) -> &Animal {
    match &unit(w).part {
        Part::Animal(a) => a,
        _ => unreachable!(),
    }
}

/// The call `fTargetBuilding` once on the lion; its answer.
fn call(w: &mut World, rng: &mut Rng) -> Option<Flow> {
    let (cat, data) = (types(), Sets::walking());
    let mut u = unit(w).clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let flow = crate::animal::rampage::target(&mut u, w, &mut ctx, 0, &data);
    *w.units.get_mut(1).unwrap() = u;
    flow
}

/// The generator after `n` draws of the lion's search, one of `n` ties each.
fn drawn(ties: u32) -> Rng {
    let mut rng = Rng::default();
    rng.pick(ties);
    rng
}

const BOTH: [(&str, u32, (i32, i32)); 2] = [("bench", 8, (3, 4)), ("stand", 7, (6, 8))];

#[test]
fn a_rampaging_animal_takes_the_nearest_building_with_one_draw_and_heads_for_it() {
    let mut w = zoo(true, &BOTH);
    let mut rng = Rng::default();
    // The call goes on as the move: the flag is up, the bench is the target.
    assert_eq!(call(&mut w, &mut rng), None);
    assert_eq!(rng, drawn(1));
    let (a, u) = (animal(&w), unit(&w));
    assert_eq!((a.building, a.need, a.rampage_target), (8, 1, true));
    let r = &u.goal.record;
    assert_eq!((r.task, r.mode, r.done), (123, 1, false));
    assert_eq!(r.target.tile, Some(tile(3, 4)));
    assert!(!u.path.is_empty() && u.destination.is_some());
    // The flag is up: the next run makes no draw, no search.
    let mut rng = Rng::default();
    assert_eq!(call(&mut w, &mut rng), None);
    assert_eq!(rng, Rng::default());
}

#[test]
fn two_buildings_at_the_same_distance_are_drawn_in_list_order() {
    let mut w = zoo(true, &[("bench", 8, (3, 4)), ("stand", 7, (4, 3))]);
    let mut rng = Rng::default();
    assert_eq!(call(&mut w, &mut rng), None);
    assert_eq!(rng, drawn(2));
    let n = Rng::default().pick(2) as usize;
    assert_eq!(animal(&w).building, [8, 7][n]);
}

#[test]
fn a_building_that_admits_guests_only_is_no_candidate_and_no_draw_is_made() {
    let mut w = zoo(true, &[("gstand", 9, (2, 2))]);
    let mut rng = Rng::default();
    assert_eq!(call(&mut w, &mut rng), Some(Flow::Done));
    assert_eq!(rng, Rng::default());
    let (a, u) = (animal(&w), unit(&w));
    assert_eq!((a.building, a.rampage_target, u.goal.record.task), (0, false, 0));
}

#[test]
fn an_animal_that_does_not_rampage_draws_nothing_and_its_flag_goes_down() {
    let mut w = zoo(false, &BOTH);
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.rampage_target = true;
    }
    let mut rng = Rng::default();
    assert_eq!(call(&mut w, &mut rng), Some(Flow::Done));
    assert_eq!(rng, Rng::default());
    assert!(!animal(&w).rampage_target);
    assert_eq!(unit(&w).goal.record.task, 0);
}

#[test]
fn no_path_means_no_target_but_the_draw_is_spent() {
    // The lion's tile is closed on all four sides (a step on each edge).
    let mut w = zoo(true, &BOTH);
    w.zoo.tiles[tile(0, 0)].edges = 0xAA;
    let mut rng = Rng::default();
    assert_eq!(call(&mut w, &mut rng), Some(Flow::Done));
    assert_eq!(rng, drawn(1));
    assert!(!animal(&w).rampage_target && unit(&w).goal.record.task == 0);
}

#[test]
fn the_rampaging_lion_runs_to_the_building_and_the_goal_ends_there() {
    let sets: &[(&str, &[&str])] = &[
        ("bEscapedRampage", &["fTargetBuilding()", "fPlay(idle)"]),
        ("bWalk", &["fWalk(0,0)"]),
    ];
    let data = Sets::new(sets);
    let (mut w, mut s) = (zoo(true, &BOTH), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.escaped = true;
    }
    // The rampage set starts as the ambient choice does.
    let mut u = unit(&w).clone();
    crate::animal::escape::ambient_sets(&mut u, &w, &data, &mut Rng::default(), 0);
    *w.units.get_mut(1).unwrap() = u;
    assert_eq!(unit(&w).runner.top(), Some("bEscapedRampage"));
    let mut seen: Vec<(u8, bool)> = Vec::new();
    let mut speed = 0;
    for _ in 0..400 {
        s.step_with(&data, &types(), &mut w, 67, |_, _, _| {});
        let r = &unit(&w).goal.record;
        seen.push((r.task, animal(&w).rampage_target));
        speed = speed.max(unit(&w).speed);
        if seen.contains(&(123, true)) && r.task == 0 {
            break;
        }
    }
    // It took the target, ran, and the goal ended with the flag down in the bench's tile.
    seen.dedup();
    assert!(seen.contains(&(123, true)), "{seen:?}");
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), (3, 4), "{seen:?}");
    assert_eq!(seen.last(), Some(&(0, false)));
    // At the fast rate of its type (76 in these tests).
    assert_eq!(speed, 76);
}
