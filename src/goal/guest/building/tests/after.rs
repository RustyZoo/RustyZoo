//! The rest of the return buildings (guest buildings 12.2 points 16 and 22, goals 11.13 points 6
//! and 9): the invalidated target of a ride, a ride built by a load and the 0x69 complete that finds
//! no building.

use super::ride::{inside, lake, leave, ride, riding, set};
use super::*;
use crate::goal::{Goal, RECOVER, RETURN_TO_BUILDING};

#[test]
fn an_aborted_ride_leaves_the_members_of_a_tracking_building_and_recovers() {
    // Goals 11.13 point 6: the target is still a building of a tracker type.
    let (cat, mut w) = lake("dolshack");
    inside(&cat, &mut w, 0);
    leave(&cat, &mut w);
    assert_eq!(w.units.buildings[&9].members, [1]);
    let r = &mut w.units.get_mut(1).unwrap().goal.record;
    (r.task, r.target.id) = (RETURN_TO_BUILDING, 9);
    ride(&cat, &mut w, |cx| {
        cx.unit.goal.object = Some(Goal::Ride);
        cx.abort();
    });
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.task, r.mode, r.target.id), (RECOVER, 0, 0));
    assert!(w.units.buildings[&9].members.is_empty());
    // A target that is no building any more, or a building that tracks nobody, loses nobody.
    for (key, target) in [("dolshack", 77), ("boatshak", 9)] {
        let (cat, mut w) = lake(key);
        inside(&cat, &mut w, 0);
        w.units.buildings.get_mut(&9).unwrap().members.push(1);
        let r = &mut w.units.get_mut(1).unwrap().goal.record;
        (r.task, r.target.id) = (RETURN_TO_BUILDING, target);
        ride(&cat, &mut w, |cx| {
            cx.unit.goal.object = Some(Goal::Ride);
            cx.abort();
        });
        assert_eq!(w.units.buildings[&9].members, [1], "{key} {target}");
        assert_eq!(w.units.get(1).unwrap().goal.record.task, RECOVER);
    }
    // Any other goal only loses its target.
    let (cat, mut w) = riding("dolshack", 8060);
    ride(&cat, &mut w, |cx| cx.abort());
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.task, r.target.id), (RETURN_TO_BUILDING, 0));
}

/// A game of the lake saved with guest 1 in the ride of the building type `key`, loaded at AI
/// tick 123 as a file of version `version`.
fn loaded(key: &str, id: i32, version: u32) -> World {
    let (cat, mut w) = riding(key, id);
    w.zoo.version = version;
    let mut block = crate::zoo::UnitBlock::default();
    block.target.goal = i32::from(RETURN_TO_BUILDING);
    block.target.active = i32::from(RETURN_TO_BUILDING);
    block.target.active_sub = -1;
    block.target.words[0] = 9;
    let at = w.find(1).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().unit = Some(Box::new(block));
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(
        100,
        crate::zoo::Clocks {
            ai_ticks: 123,
            ..Default::default()
        },
    );
    sim.load(&mut w, &cat);
    w
}

#[test]
fn a_loaded_ride_has_no_legs_and_the_flags_of_the_type_of_the_return_name_id() {
    // Goals 11.13 point 9: built, never initialised; the flags are not lost.
    for (key, id, idler, viewer) in [
        ("chroom", 8061, true, false),
        ("boatshak", 8059, false, true),
        ("dolshack", 8060, false, false),
        ("chroom", 5, false, false),
    ] {
        let w = loaded(key, id, 94);
        let ride = &w.units.get(1).unwrap().goal.ride;
        let want = Ride {
            until: 123,
            idler,
            viewer,
            ..Ride::default()
        };
        assert_eq!(ride, &want, "{key} {id}");
    }
    // A file older than version 72 builds the object at its first update, init included.
    let w = loaded("chroom", 8061, 71);
    assert_eq!(w.units.get(1).unwrap().goal.ride, Ride::default());
}

#[test]
fn the_exit_complete_that_finds_no_building_gives_the_ride_target_0() {
    // Open point 12: the ride's init then fails and clears the record.
    let (cat, mut w) = lake("boatshak");
    inside(&cat, &mut w, 0);
    set(&mut w, |g| (g.building, g.returning) = (77, Some(8059)));
    ride(&cat, &mut w, |cx| cx.exit_done());
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.task, r.target.id), (RETURN_TO_BUILDING, 0));
    assert!(!ride(&cat, &mut w, |cx| cx.ride_init()));
}
