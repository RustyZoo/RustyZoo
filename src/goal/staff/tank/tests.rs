//! The place of the food in a tank on the synthetic tank of the swimmers' tests: 4 by 3 tiles
//! from (2, 2), the walls on its border.

use super::*;
use crate::unit::testing::tank_world;

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

#[test]
fn food_is_put_where_the_drawn_animal_can_swim_not_against_the_wall() {
    let (w, keeper) = tank();
    let x = w.tank_at(tile((3, 3))).unwrap();
    // The keeper alone would take any of the 11 free tiles, the wall tiles too.
    assert_eq!(food_places(&w, x, &keeper, &[]).len(), 11);
    // A ray off the walls is closed out of the tiles near them (pathfinding 6.6 item 4): it
    // could never swim to a pile there.
    let ray = food_mover(&w, w.find(1)).unwrap();
    assert_eq!(food_places(&w, x, &ray, &[]), [tile((4, 3))]);
    // A lost tile is not taken, and an animal that is gone leaves the choice to the keeper.
    assert!(food_places(&w, x, &ray, &[tile((4, 3))]).is_empty());
    assert!(food_mover(&w, None).is_none());
    assert!(food_mover(&w, Some(usize::MAX)).is_none());
}
