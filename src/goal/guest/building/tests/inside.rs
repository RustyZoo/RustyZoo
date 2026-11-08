//! The inside tile of a building (goals 11.3.2).

use super::*;
use crate::goal::guest::building::inside::walk;

fn held(x0: i32, y0: i32, x1: i32, y1: i32) -> impl Fn(i32, i32) -> bool {
    move |x, y| (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
}

#[test]
fn the_worked_example_of_a_two_by_two_building_from_its_entrance_tiles() {
    let two = held(10, 10, 11, 11);
    for (from, want) in [
        ((10, 9), (10, 10)),
        ((11, 9), (11, 10)),
        ((9, 10), (10, 10)),
        ((9, 11), (10, 11)),
        ((10, 12), (10, 11)),
        ((11, 12), (11, 11)),
        ((12, 11), (11, 11)),
        // The only diagonal: the first direction misses and the cursor moves on.
        ((12, 10), (11, 11)),
        ((12, 12), (11, 11)),
    ] {
        assert_eq!(walk(from, (11, 11), &two), want, "from {from:?}");
    }
}

#[test]
fn a_one_by_one_building_is_found_on_the_first_step_from_each_side() {
    let one = held(10, 10, 10, 10);
    for from in [(10, 9), (9, 10), (10, 11), (11, 10)] {
        assert_eq!(walk(from, (10, 10), &one), (10, 10), "from {from:?}");
    }
}

#[test]
fn a_miss_gives_the_tile_past_the_last_run_and_the_same_tile_itself() {
    let none = |_, _| false;
    // dx = 0, dy = 3: n = 3, the run tests (5, 1), (5, 2), (5, 3), then ends on (5, 4).
    assert_eq!(walk((5, 0), (5, 3), none), (5, 4));
    assert_eq!(walk((5, 5), (5, 5), none), (5, 5));
    // A run off the map holds nothing: the result is off the map too.
    assert_eq!(walk((0, 1), (0, -5), none), (0, -6));
}

#[test]
fn the_enter_decide_walks_to_the_inside_tile_found_from_the_unit_not_to_the_slot() {
    let cat = types();
    // The booth (5, 5) has its slot on its own tile; the unit at (7, 2) walks south three tiles, none
    // of which holds the booth, and the target is the tile past the run.
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((7, 2)), 0));
        zoo.entities
            .push(entity("building", "booth", 7, centre((5, 5)), 4));
    });
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 7;
        cx.set_task(ENTER_BUILDING, 0, 7, None);
        cx.enter_decide();
        assert_eq!(cx.record().task, ENTER_BUILDING);
        assert_eq!(cx.record().target.tile, w_tile((7, 6)));
    });
    // From (5, 2) the run meets the booth's quarter on its third tile.
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((5, 2)), 0));
        zoo.entities
            .push(entity("building", "booth", 7, centre((5, 5)), 4));
    });
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 7;
        cx.set_task(ENTER_BUILDING, 0, 7, None);
        cx.enter_decide();
        assert_eq!(cx.record().target.tile, w_tile((5, 5)));
    });
    // A unit off the map has no position: the goal ends.
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities.push(entity("guests", "man", 1, (-100, 32), 0));
        zoo.entities
            .push(entity("building", "booth", 7, centre((5, 5)), 4));
    });
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 7;
        cx.set_task(ENTER_BUILDING, 0, 7, None);
        cx.enter_decide();
        assert_eq!(cx.record().task, 0);
    });
}
