//! The aborts an animal causes through its show id (shows 30.2 points 4.5 to 4.7 and 8): a world
//! with the plain tank `T` holding an orca (1, a performer of the idle show 1001), a manta ray (2)
//! and a shark (3) that eats orcas, all adults.

use super::show::staged_with;
use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::show::event;
use crate::unit::Part;
use std::sync::Arc;

const ORCA: i32 = 5401;
const MANTA: i32 = 5418;
const SHARK: i32 = 5500;

/// An adult of its own type with the prey list `prey`; the orca performs.
fn adult(name: &str, id: i32, prey: &str) -> EntityType {
    let text = format!("[Characteristics/Integers]\n{prey}\n[tricks]\ntrick=a\n");
    EntityType {
        class: "animals",
        type_name: name.as_bytes().to_vec(),
        subtype: b"m".to_vec(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn cat() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(adult("orca", ORCA, "cPrey=5418\ncPrey=5417"));
    cat.push(adult("manta", MANTA, ""));
    cat.push(adult("shark", SHARK, &format!("cPrey={ORCA}")));
    cat
}

/// The world with the animals at `at`, the clock at 500, the orca in show 1001.
fn tank(at: [(i32, i32); 3]) -> World {
    let animals = [(1, "orca", at[0]), (2, "manta", at[1]), (3, "shark", at[2])];
    let mut w = staged_with(&animals, true);
    for e in &mut w.zoo.entities {
        if e.class == "animals" {
            e.subtype = "m".into();
        }
    }
    w.units.ai_ticks = 500;
    w
}

/// The kinds of the events posted outside the show manager's step, in order.
fn posted(w: &World) -> Vec<u16> {
    w.units.posts.iter().map(|e| e.kind).collect()
}

fn next(w: &World) -> i32 {
    w.zoo.shows.infos[0].as_ref().unwrap().next
}

/// The escape check of animal 1 fires, with `in_show` as the show manager made it.
fn escape_check(w: &mut World, in_show: bool) -> Unit {
    let mut unit = w.units.get(1).unwrap().clone();
    if let Part::Animal(a) = &mut unit.part {
        // Alive, with every counter far from firing but the escape check's.
        a.counters.fill(1000);
        (a.in_show, a.life, a.counters[crate::animal::ESCAPE]) = (in_show, 5000, 0);
    }
    let (events, mut removals, mut rng) = (Events::default(), vec![], Rng::default());
    let cat = cat();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let at = w.find(1).unwrap();
    crate::animal::checks(&mut unit, w, &mut ctx, at, &NoData);
    unit
}

#[test]
fn a_threat_seen_by_the_escape_check_aborts_the_show_of_an_animal_that_is_not_in_it() {
    let mut w = tank([(3, 3), (4, 3), (3, 2)]);
    let unit = escape_check(&mut w, false);
    assert!(unit.afraid);
    assert_eq!(posted(&w), [event::ABORTED, event::ENDED]);
    assert_eq!(next(&w), 590);
    // Already afraid: no second abort. A threat gone: the flag goes down.
    w.units.posts.clear();
    w.units.get_mut(1).unwrap().afraid = true;
    assert!(escape_check(&mut w, false).afraid);
    assert!(posted(&w).is_empty());
    if let Part::Animal(a) = &mut w.units.get_mut(3).unwrap().part {
        a.crated = true;
    }
    assert!(!escape_check(&mut w, false).afraid);
}

#[test]
fn an_animal_in_a_show_runs_no_escape_check_so_it_neither_flees_nor_aborts() {
    let mut w = tank([(3, 3), (4, 3), (3, 2)]);
    let unit = escape_check(&mut w, true);
    assert!(!unit.afraid && posted(&w).is_empty());
    assert_eq!(next(&w), 0);
}

#[test]
fn an_animal_dropped_into_a_tank_aborts_the_show_of_the_unit_it_fears_there() {
    // The orca is the manta ray's predator: its show is aborted, the manta's own is not.
    let mut w = tank([(3, 3), (4, 3), (8, 6)]);
    w.units.get_mut(2).unwrap().show = 1001;
    super::super::dropped(&mut w, &cat(), &mut Rng::default(), &NoData, 2);
    assert_eq!(posted(&w), [event::ABORTED, event::ENDED]);
    assert_eq!(next(&w), 590);
}

#[test]
fn an_animal_dropped_into_a_tank_aborts_the_show_of_its_prey_when_it_fears_nothing() {
    // The orca fears the shark, taken out of the tank; the manta ray it hunts has the show id.
    let mut w = tank([(3, 3), (4, 3), (8, 6)]);
    w.units.get_mut(1).unwrap().show = 0;
    w.units.get_mut(2).unwrap().show = 1001;
    super::super::dropped(&mut w, &cat(), &mut Rng::default(), &NoData, 1);
    assert_eq!(posted(&w), [event::ABORTED, event::ENDED]);
}

#[test]
fn an_animal_dropped_with_no_threat_and_no_prey_or_out_of_a_tank_aborts_nothing() {
    // The shark and the orca are on the plain ground; the manta ray is dropped into the tank.
    let mut w = tank([(8, 6), (4, 3), (9, 6)]);
    super::super::dropped(&mut w, &cat(), &mut Rng::default(), &NoData, 2);
    // The orca with a show id, dropped where its prey is not: its own show stays.
    super::super::dropped(&mut w, &cat(), &mut Rng::default(), &NoData, 1);
    // Out of a tank nothing is looked for, whoever is near.
    let mut w = tank([(8, 6), (9, 6), (8, 6)]);
    super::super::dropped(&mut w, &cat(), &mut Rng::default(), &NoData, 2);
    assert!(posted(&w).is_empty());
}

#[test]
fn an_animals_goal_reset_lowers_afraid() {
    let (mut w, mut rng) = (map("animals"), Rng::default());
    w.units.get_mut(1).unwrap().afraid = true;
    with_cx(&mut w, &mut rng, |cx| cx.reset());
    assert!(!w.units.get(1).unwrap().afraid);
}
