//! The keep-moving extension of the next-to helper (animals-needs 7.1 step 4) on its own: which
//! tasks get the pile and the tile past it appended, on the ground and for a swimmer in a tank.

use super::chase::sets;
use super::*;
use crate::fixtures::unit_ty;
use crate::unit::testing::{centre, tank_world, world_with};

const ID: u32 = 1;

fn tile(w: &World, (x, y): (usize, usize)) -> usize {
    y * w.zoo.width as usize + x
}

/// The path `next_to` leaves for unit `ID` set to `task`, aiming at `to`, with the species keys
/// `keys`; nothing when it finds no way.
fn walk(w: &mut World, keys: &str, task: u8, to: (usize, usize)) -> Option<Vec<usize>> {
    let mut cat = Catalogue::default();
    cat.push(unit_ty("animals", "fish", "", 1, keys));
    cat.push(unit_ty("animals", "zebra", "m", 2, keys));
    let (data, mut rng) = (sets(), Rng::default());
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut u = w.units.get(ID).unwrap().clone();
    let at = w.find(ID).unwrap();
    let to = tile(w, to);
    u.goal.record.task = task;
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
    cx.next_to(to, false, false).then(|| u.path.clone())
}

/// A zebra at (10, 10) on a plain 20 by 20 ground.
fn ground() -> World {
    let mut w = world_with((20, 20), |zoo| {
        let mut a = entity("animals", "zebra", ID, centre((10, 10)), 0);
        a.subtype = "m".into();
        zoo.entities.push(a);
    });
    w.units.get_mut(ID).unwrap().part = Part::Animal(crate::animal::Animal::default());
    w
}

/// A fish at (2, 3) in the synthetic tank (tiles x 2 to 5, y 2 to 4).
fn tank() -> World {
    let mut w = tank_world(5, 4, &[(ID, (2, 3), 32)]);
    crate::unit::testing::still_water(&mut w);
    let u = w.units.get_mut(ID).unwrap();
    (u.typed, u.started, u.mover.underwater, u.mover.depth) = (true, true, true, 1);
    u.mover.swims = true;
    let at = w.find(ID).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().movement_mode = 3;
    w
}

#[test]
fn only_the_random_walk_and_the_keeper_food_task_get_the_pile_and_the_tile_past_it() {
    let end =
        |task, keys| walk(&mut ground(), keys, task, (14, 10)).map(|p| p[p.len() - 3..].to_vec());
    let w = ground();
    let t = |x| tile(&w, (x, 10));
    for task in [RANDOM, KEEPER_FOOD] {
        assert_eq!(
            end(task, "cKeepMoving=1"),
            Some(vec![t(13), t(14), t(15)]),
            "{task}"
        );
        for keys in [
            "cKeepMoving=0",
            "cKeepMoving=2",
            "cKeepMoving=-1",
            "",
            "cKeepMoving=2147483647",
        ] {
            let p = walk(&mut ground(), keys, task, (14, 10)).unwrap();
            assert_eq!(p.last(), Some(&t(13)), "{task} {keys}");
        }
    }
    // The drink, the chase and no task end on the neighbour.
    for task in [DRINK, CHASE, NONE] {
        let p = walk(&mut ground(), "cKeepMoving=1", task, (14, 10)).unwrap();
        assert_eq!(p.last(), Some(&t(13)), "{task}");
    }
}

#[test]
fn a_keep_moving_swimmer_in_a_tank_goes_over_the_food_and_one_tile_past_it() {
    let w = tank();
    let t = |x| tile(&w, (x, 3));
    let path = |task, keys, to| walk(&mut tank(), keys, task, to);
    let end = |p: Option<Vec<usize>>, n: usize| p.map(|p| p[p.len() - n..].to_vec());
    // The food at (4, 3): the neighbour (3, 3), the food, and (5, 3) inside the tank.
    let p = path(KEEPER_FOOD, "cKeepMoving=1", (4, 3));
    assert_eq!(end(p, 3), Some(vec![t(3), t(4), t(5)]));
    let p = path(RANDOM, "cKeepMoving=1", (4, 3));
    assert_eq!(end(p, 3), Some(vec![t(3), t(4), t(5)]));
    // The drink task and a species that does not keep moving stop on the neighbour.
    for (task, keys) in [(DRINK, "cKeepMoving=1"), (KEEPER_FOOD, "cKeepMoving=0")] {
        assert_eq!(
            end(path(task, keys, (4, 3)), 1),
            Some(vec![t(3)]),
            "{task} {keys}"
        );
    }
    // The food at the wall (5, 3): the tile past is not water, so one point is appended.
    let p = path(KEEPER_FOOD, "cKeepMoving=1", (5, 3));
    assert_eq!(end(p, 2), Some(vec![t(4), t(5)]));
}

#[test]
fn a_step_past_the_food_that_is_not_valid_appends_the_food_only() {
    // On the ground the map edge is the end: food at (19, 10) has no tile past it.
    let w = ground();
    let t = |x| tile(&w, (x, 10));
    let p = walk(&mut ground(), "cKeepMoving=1", KEEPER_FOOD, (19, 10)).unwrap();
    assert_eq!(p[p.len() - 2..], [t(18), t(19)]);
}
