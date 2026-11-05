//! The place of the food in a tank on the synthetic tank of the swimmers' tests: 4 by 3 tiles
//! from (2, 2), the walls on its border.

use super::*;
use crate::entity::Catalogue;
use crate::goal::EXIT_TANK;
use crate::sim::{Events, Rng};
use crate::unit::testing::{Walker, tank_world};
use crate::unit::{Ctx, Kind, Part, Staff};

/// A ray that keeps off the walls (`cAvoidEdges` 1) at tile (3, 3), and a keeper's mover that does
/// not. The inside of the tank is the two tiles (3, 3) and (4, 3).
fn tank() -> (World, path::Mover) {
    let mut w = tank_world(5, 4, &[(1, (3, 3), 32)]);
    let u = w.units.get_mut(1).unwrap();
    (u.typed, u.mover.underwater, u.mover.avoid_edges) = (true, true, 1);
    let mut keeper = u.mover.clone();
    keeper.avoid_edges = 0;
    (w, keeper)
}

fn tile((x, y): (usize, usize)) -> usize {
    y * 12 + x
}

/// The keeper's place in the tank of `w` as a staff unit with the lost tiles `lost`: unit 1's
/// body, a marine keeper's when `marine`, else one that swims in no water.
fn keeper_place(w: &mut World, marine: bool, lost: Vec<usize>) -> Option<(usize, i32)> {
    let x = w.tank_at(tile((3, 3))).unwrap();
    let at = w.find(1).unwrap();
    let mut u = w.units.get(1).unwrap().clone();
    (u.typed, u.mover.underwater) = (marine, marine);
    u.part = Part::Staff(Staff {
        lost_tiles: lost,
        ..Staff::default()
    });
    let (events, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let cat = Catalogue::default();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let data = Walker::new("fSwim(level,0)");
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data: &data,
    };
    cx.tank_food_place(x)
}

#[test]
fn food_is_put_where_the_drawn_animal_can_swim_not_against_the_wall() {
    let (w, keeper) = tank();
    let x = w.tank_at(tile((3, 3))).unwrap();
    // The keeper alone would take any of the 11 free tiles, the wall tiles too.
    assert_eq!(food_places(&w, x, Some(&keeper)).len(), 11);
    // A ray off the walls is closed out of the tiles near them (pathfinding 6.6 item 4): it
    // could never swim to a pile there.
    let ray = food_mover(&w, w.find(1)).unwrap();
    assert_eq!(food_places(&w, x, Some(&ray)), [tile((4, 3))]);
    // An animal that is gone leaves no mover.
    assert!(food_mover(&w, None).is_none());
    assert!(food_mover(&w, Some(usize::MAX)).is_none());
}

#[test]
fn a_tile_the_keeper_lost_once_is_a_candidate_again() {
    // The tank keeps no unreachable list (5.3): the one open tile is taken though it was lost.
    let (mut w, _) = tank();
    assert_eq!(
        keeper_place(&mut w, true, vec![tile((4, 3))]).map(|p| p.0),
        Some(tile((4, 3)))
    );
}

#[test]
fn with_no_animal_drawn_the_keeper_has_no_cost_test() {
    // The animal lives outside the tank, so none is drawn: the keeper's own cost, which closes
    // the water to him, is not asked and a tile is still found.
    let mut w = tank_world(5, 4, &[(1, (0, 0), 0)]);
    assert!(keeper_place(&mut w, false, Vec::new()).is_some());
}

#[test]
fn the_drawn_animal_comes_from_the_stored_list_of_a_recomputed_tank() {
    // The dirty tank is recomputed first, so its list holds the ray and the only tile it can
    // swim to is taken.
    let (mut w, _) = tank();
    let x = w.tank_at(tile((3, 3))).unwrap();
    assert!(w.exhibits().get(x).unwrap().rating.dirty);
    assert_eq!(
        keeper_place(&mut w, true, Vec::new()).map(|p| p.0),
        Some(tile((4, 3)))
    );
    assert!(!w.exhibits().get(x).unwrap().rating.dirty);
    assert_eq!(w.exhibits().get(x).unwrap().rating.animals, [1]);
    // A list that does not hold the ray draws no animal: no cost test, any of the 11 tiles.
    w.exhibits_mut().get_mut(x).unwrap().rating.animals.clear();
    assert_ne!(
        keeper_place(&mut w, true, Vec::new()).map(|p| p.0),
        Some(tile((4, 3)))
    );
}

/// Runs `f` on the goal context of unit 7 of `show_tank`, made an animal, whose state is put back.
fn as_animal<R>(w: &mut World, f: impl FnOnce(&mut Cx) -> R) -> R {
    let cat = Catalogue::default();
    let (events, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let mut u = w.units.get(7).unwrap().clone();
    (u.kind, u.part) = (Kind::Animal, Part::Animal(Default::default()));
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let at = w.find(7).unwrap();
    let data = Walker::new("fWalk(0,0)");
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(7).unwrap() = u;
    r
}

#[test]
fn the_animal_s_tank_exit_writes_its_own_target_tile_and_the_goal_reset_clears_it() {
    // Shows 32.2 point 4: its exhibit's inside gate tile, the centre as the position.
    let mut w = crate::unit::testing::show_tank();
    let inside = Ladder::of(&w, 1).unwrap().inside;
    let goal = crate::goal::object::Goal::build(EXIT_TANK, Kind::Animal).unwrap();
    assert!(as_animal(&mut w, |cx| goal.init(cx)));
    let u = w.units.get(7).unwrap();
    assert_eq!(u.own_tile, Some(inside));
    assert_eq!(
        u.own_pos[..2],
        [
            (inside % 12) as i32 * 64 + 32,
            (inside / 12) as i32 * 64 + 32
        ]
    );
    assert_eq!(u.goal.record.job, 0, "no ladder job for an animal yet");
    as_animal(&mut w, |cx| {
        cx.ladder_task_check_done();
        cx.ladder_task_complete();
    });
    let u = w.units.get(7).unwrap();
    assert_eq!((u.own_tile, u.own_pos), (None, [-1; 3]));
}

#[test]
fn the_animal_s_tank_exit_off_the_tank_writes_nothing() {
    let mut w = crate::unit::testing::show_tank();
    let at = w.find(7).unwrap();
    w.place_unit(at, (9 * 64 + 32, 6 * 64 + 32, 0), 0);
    assert!(!as_animal(&mut w, |cx| cx.exit_init()));
    assert_eq!(w.units.get(7).unwrap().own_tile, None);
}
