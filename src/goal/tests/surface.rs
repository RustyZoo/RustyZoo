//! The surface goal of an air breather in a tank (tank fauna 10.2) on the synthetic tank of 4 by 3
//! tiles from (2, 2): the floor at level 0, the walls 5 high and the water full at level 5, so the
//! surface is at height 80.

use super::*;
use crate::unit::testing::{Walker, tank_world};

/// A tank type's sets: the walker's, and `bBreathe`, which never ends.
struct Gills(Walker, Vec<Call>);

impl Data for Gills {
    fn set(&self, e: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        match name {
            "bBreathe" => Some(&self.1),
            _ => self.0.set(e, name),
        }
    }
    fn ambient(&self, e: &crate::zoo::Entity, place: usize) -> &[Row] {
        self.0.ambient(e, place)
    }
    fn timing(&self, e: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        self.0.timing(e, name)
    }
    fn number(&self, e: &crate::zoo::Entity, number: &Number) -> i32 {
        self.0.number(e, number)
    }
    fn sound(&self, e: &crate::zoo::Entity, key: &str) -> Option<Heard> {
        self.0.sound(e, key)
    }
}

fn gills() -> Gills {
    let keep = parse_call(b"fKeepControl()").unwrap().0;
    Gills(Walker::new("fSwim(level,0)"), vec![keep])
}

/// A fish at this height (16 to a level) with this breath.
fn fish(breath: i32, height: i32) -> World {
    let mut w = tank_world(5, 5, &[(1, (3, 3), height)]);
    let u = w.units.get_mut(1).unwrap();
    (u.typed, u.started, u.mover.underwater, u.mover.depth) = (true, true, true, 1);
    u.goal.record.task = RANDOM;
    if let Part::Animal(a) = &mut u.part {
        (a.terrain, a.breath) = (2, breath);
        // The counters of the checks stay far from firing.
        a.counters = [1000; 16];
    }
    w
}

fn breath(w: &World) -> i32 {
    let Part::Animal(a) = &w.units.get(1).unwrap().part else {
        panic!("not an animal");
    };
    a.breath
}

#[test]
fn an_animal_under_water_with_the_breath_of_its_threshold_takes_the_surface_task() {
    // The default threshold is 100.
    for (breath, height, takes) in [
        (100, 16, true),
        (99, 16, false),
        (100, 80, false),
        (0, 16, false),
    ] {
        let (mut w, mut s) = (fish(breath, height), sim());
        let log = run_with(&mut w, &mut s, &gills(), 1);
        let took = log.iter().any(|l| l.contains("goal 1 surfacebreath 0 "));
        assert_eq!(took, takes, "breath {breath} at height {height}");
    }
}

#[test]
fn it_swims_to_the_surface_breathes_there_and_goes_back_to_its_walks() {
    let (mut w, mut s) = (fish(100, 16), sim());
    let data = gills();
    let mut log = Vec::new();
    // The set starts as the animal arrives at the surface: the level 5, the height 80, mode 3.
    for _ in 0..400 {
        log.extend(run_with(&mut w, &mut s, &data, 1));
        if log.iter().any(|l| l.contains("set 1 bBreathe")) {
            break;
        }
    }
    let line = |what: &str| log.iter().position(|l| l.contains(what));
    assert!(
        line("goal 1 surfacebreath") < line("set 1 bBreathe"),
        "{log:?}"
    );
    let u = w.units.get(1).unwrap();
    let z = w.zoo.entities[w.find(1).unwrap()]
        .common
        .as_ref()
        .unwrap()
        .z;
    assert_eq!(
        (z, u.tank, u.target),
        (80, 3, crate::behaviour::Target::Surface)
    );
    // The breath is 0 and the goal is over; the animal does not ask for air again.
    assert_eq!(breath(&w), 0);
    log.extend(run_with(&mut w, &mut s, &data, 2));
    assert_ne!(w.units.get(1).unwrap().goal.record.task, SURFACE);
    assert_eq!(
        log.iter()
            .filter(|l| l.contains("goal 1 surfacebreath"))
            .count(),
        1
    );
}

#[test]
fn the_goal_is_refused_out_of_a_tank_and_waits_for_its_target() {
    // Out of a tank the start fails: the record is cleared.
    let mut w = world((10, 10), |_, e| {
        e.push(entity("animals", "", 1, centre((8, 8)), 0))
    });
    assert!(!with_cx(&mut w, &mut Rng::default(), |cx| cx.surface_init()));
    // In a tank it starts, and the decide with a target already set does nothing.
    let mut w = fish(100, 16);
    let mut rng = Rng::default();
    assert!(with_cx(&mut w, &mut rng, |cx| cx.surface_init()));
    with_data(&mut w, &mut rng, &gills(), |cx| {
        cx.set_task(SURFACE, 0, 0, Some(tile(3, 3)));
        cx.surface_decide();
        assert!(cx.unit.path.is_empty() && cx.unit.target != crate::behaviour::Target::Surface);
    });
}
