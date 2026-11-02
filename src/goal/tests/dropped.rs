//! A unit put down by hand (unit placing 8.5): the rule of its kind, and the recovery from water
//! a guest starts on a pond (tank fauna 13), on a 10 by 10 map with a pen from (1, 1) to (3, 3)
//! and a strip of water from (4, 7) to (6, 7).

use super::*;
use crate::guest::Guest;
use crate::unit::Staff;
use crate::unit::testing::{Walker, tank_world_with, world_with};

const GUEST: u32 = 1;
const KEEPER: u32 = 2;
const OTHER: u32 = 3;
const THIRD: u32 = 4;

fn pond() -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (8, 5);
        for x in 4..=6 {
            zoo.tiles[7 * 10 + x].terrain = 1;
        }
        for i in 1..=3 {
            for (tile, d) in [((i, 1), 0), ((i, 3), 4), ((1, i), 6), ((3, i), 2)] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(tile), d));
            }
        }
        zoo.entities
            .push(entity("guests", "man", GUEST, centre((5, 7)), 0));
        for (id, tile) in [(KEEPER, (2, 2)), (OTHER, (2, 2)), (THIRD, (2, 2))] {
            let mut keeper = entity("keeper", "keeper", id, centre(tile), 0);
            keeper.subtype = "m".into();
            zoo.entities.push(keeper);
        }
    });
    w.set_water(vec![false, true]);
    w.units.get_mut(GUEST).unwrap().part = Part::Guest(Guest {
        countdowns: [1000; 11],
        saw_animal: 1000,
        ..Guest::default()
    });
    w
}

/// Puts unit `id` on `tile`, then runs the drop rule.
fn drop_at(w: &mut World, id: u32, tile: (i32, i32)) {
    let at = w.find(id).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre(tile);
    drop_rule(w, id);
}

fn drop_rule(w: &mut World, id: u32) {
    super::super::dropped(w, &Catalogue::default(), &mut Rng::default(), &NoData, id);
}

fn task(w: &World, id: u32) -> u8 {
    w.units.get(id).unwrap().goal.record.task
}

fn wandered(w: &World) -> bool {
    w.units.get(GUEST).unwrap().goal.wandered
}

#[test]
fn a_guest_dropped_on_water_outside_a_tank_recovers_and_thinks_10568() {
    let mut w = pond();
    drop_at(&mut w, GUEST, (5, 7));
    assert_eq!(task(&w, GUEST), RECOVER);
    assert_eq!(w.units.thoughts.of(GUEST)[0].text, 10568);
    assert!(wandered(&w));
}

#[test]
fn a_guest_dropped_on_dry_land_only_chooses_a_new_goal() {
    let mut w = pond();
    drop_at(&mut w, GUEST, (5, 5));
    assert_eq!(task(&w, GUEST), NONE);
    assert!(w.units.thoughts.of(GUEST).is_empty());
    assert!(wandered(&w));
}

#[test]
fn a_guest_dropped_in_a_tank_does_not_recover_from_water() {
    let guest = entity("guests", "man", GUEST, centre((3, 3)), 0);
    let mut w = tank_world_with(5, 5, &[], vec![guest]);
    w.set_water(vec![false, true]);
    w.zoo.tiles[3 * 12 + 3].terrain = 1;
    assert!(w.is_water(3 * 12 + 3) && w.tank_at(3 * 12 + 3).is_some());
    drop_rule(&mut w, GUEST);
    assert_eq!(task(&w, GUEST), NONE);
    assert!(w.units.thoughts.of(GUEST).is_empty());
}

#[test]
fn a_unit_picked_up_stops_its_goal() {
    let mut w = pond();
    w.units.get_mut(GUEST).unwrap().goal.record.task = SITE;
    super::super::picked_up(
        &mut w,
        &Catalogue::default(),
        &mut Rng::default(),
        &NoData,
        GUEST,
    );
    assert_eq!(task(&w, GUEST), NONE);
}

#[test]
fn a_keeper_dropped_in_an_exhibit_starts_its_duties_unless_two_keepers_work_there() {
    let mut w = pond();
    // On the grounds: nothing.
    drop_at(&mut w, KEEPER, (6, 3));
    assert_eq!(task(&w, KEEPER), NONE);
    // In the pen: the duties.
    drop_at(&mut w, KEEPER, (2, 2));
    assert_eq!(task(&w, KEEPER), DUTIES);
    // One other keeper working there: still the duties; two: none.
    w.units.get_mut(KEEPER).unwrap().goal.record.task = NONE;
    w.units.get_mut(OTHER).unwrap().goal.record.task = DUTIES;
    drop_rule(&mut w, KEEPER);
    assert_eq!(task(&w, KEEPER), DUTIES);
    w.units.get_mut(THIRD).unwrap().goal.record.task = DUTIES;
    w.units.get_mut(KEEPER).unwrap().goal.record.task = NONE;
    drop_rule(&mut w, KEEPER);
    assert_eq!(task(&w, KEEPER), NONE);
}

#[test]
fn a_staff_member_dropped_forgets_the_targets_it_could_not_reach() {
    let mut w = pond();
    for id in [KEEPER, OTHER] {
        let Part::Staff(s) = &mut w.units.get_mut(id).unwrap().part else {
            panic!("not staff");
        };
        *s = Staff {
            lost_tiles: vec![5, 6],
            lost_animals: vec![7],
            assigned: vec![9],
            ..Staff::default()
        };
    }
    // A keeper on the grounds and one in the pen.
    drop_at(&mut w, KEEPER, (6, 3));
    drop_at(&mut w, OTHER, (2, 2));
    for id in [KEEPER, OTHER] {
        let Part::Staff(s) = &w.units.get(id).unwrap().part else {
            panic!("not staff");
        };
        assert_eq!((&s.lost_tiles, &s.lost_animals), (&vec![], &vec![]), "{id}");
        assert_eq!(s.assigned, [9]);
    }
}

#[test]
fn a_guest_in_the_water_walks_out_to_dry_land_and_goes_on() {
    let mut w = pond();
    drop_at(&mut w, GUEST, (5, 7));
    let (mut s, data) = (sim(), Walker::new("fWalk(0,0)"));
    let log = run_with(&mut w, &mut s, &data, 200);
    let dry = |w: &World| {
        let c = w.zoo.entities[w.find(GUEST).unwrap()]
            .common
            .clone()
            .unwrap();
        !w.is_water(
            w.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
                .unwrap(),
        )
    };
    assert!(dry(&w), "{log:?}");
    assert!(
        log.iter().any(|l| l == "goal 1 recoverfromwater 0 - -"),
        "{log:?}"
    );
    assert!(task(&w, GUEST) != RECOVER && task(&w, GUEST) != DROWN);
}

#[test]
fn a_guest_that_cannot_get_out_uses_its_tries_and_takes_the_drowning_task() {
    // Water as far as the map goes: nothing dry to reach.
    let mut w = pond();
    for t in &mut w.zoo.tiles {
        t.terrain = 1;
    }
    drop_at(&mut w, GUEST, (5, 7));
    let (mut s, data) = (sim(), Walker::new("fWalk(0,0)"));
    let log = run_with(&mut w, &mut s, &data, 2000);
    assert_eq!(task(&w, GUEST), DROWN, "{log:?}");
}
