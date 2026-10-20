//! The links between a tank and the land exhibit beside it on a synthetic map: the tank from
//! (2, 2) is 2 high, so the wall tops are at level 2; the land exhibit from (6, 2) shares its right
//! wall.

use super::*;
use crate::unit::testing::amphibious_world;

const WIDE: usize = 12;

/// The number of the tank and of the land exhibit.
fn pair(w: &World) -> (u32, u32) {
    let region = |t: usize| w.exhibits.region(t).unwrap();
    (region(2 * WIDE + 2), region(2 * WIDE + 6))
}

/// The links of the pair, as each lists them.
fn links(w: &World) -> (Vec<u32>, Vec<u32>) {
    let (tank, land) = pair(w);
    let of = |x: u32| w.exhibits.get(x).unwrap().amphibious.clone();
    (of(tank), of(land))
}

#[test]
fn a_tank_and_the_land_level_with_its_walls_are_neighbours_and_the_wall_between_connects() {
    let w = amphibious_world(2, &[]);
    let (tank, land) = pair(&w);
    assert_ne!(tank, land);
    assert_eq!(links(&w), (vec![land], vec![tank]));
    // The tank's wall on the line, and no other piece, is a connector.
    let wall = |y: i32| w.edge(2 * WIDE + 5 + y as usize * WIDE, 2).unwrap();
    assert!((0..3).all(|y| w.connector(wall(y))));
    let others = w.edge(2 * WIDE + 2, 0).unwrap();
    assert!(!w.connector(others));
    assert_eq!(w.connectors.len(), 3);
}

#[test]
fn land_that_is_not_level_with_the_wall_tops_does_not_link() {
    for land in [0, 1, 3] {
        let w = amphibious_world(land, &[]);
        assert_eq!(links(&w), (vec![], vec![]), "land at {land}");
        assert!(w.connectors.is_empty());
    }
}

#[test]
fn a_second_exhibit_fence_on_every_line_shuts_the_link() {
    let mut w = amphibious_world(2, &[]);
    let fence = w.shapes[w.find(1000 + 6 * 10 + 2).unwrap()];
    let piece = |y: i32| {
        crate::unit::testing::entity("fences", "iron", 9000 + y as u32, (6 * 64, y * 64 + 32), 6)
    };
    // A fence of the land exhibit beside the tank wall on one line leaves the other two lines.
    let first = w.add(piece(2), fence).unwrap();
    assert_eq!(links(&w).0.len(), 1);
    let rest: Vec<u32> = [3, 4]
        .iter()
        .map(|&y| w.add(piece(y), fence).unwrap())
        .collect();
    assert_eq!(links(&w), (vec![], vec![]));
    assert!(w.connectors.is_empty());
    // Taking them away gives the link back.
    assert!(w.remove(first) && rest.iter().all(|&id| w.remove(id)));
    assert_eq!(links(&w).0.len(), 1);
}

#[test]
fn the_links_follow_the_wall_height_and_the_ground() {
    let mut w = amphibious_world(2, &[]);
    let (tank, _) = pair(&w);
    w.set_height(tank, 3);
    assert_eq!(links(&w), (vec![], vec![]), "taller walls");
    w.set_height(tank, 2);
    assert_eq!(links(&w).0.len(), 1, "back to level");
    // A land tile of the line lowered breaks that line only; all of them, the link.
    let set = |w: &mut World, y: i32, level: i32| {
        let tile = w.zoo.tiles[y as usize * WIDE + 6];
        w.set_tile(6, y, level, tile.shape, tile.terrain);
    };
    set(&mut w, 2, 1);
    assert_eq!(links(&w).0.len(), 1, "two lines are left");
    set(&mut w, 3, 1);
    set(&mut w, 4, 1);
    assert!(
        links(&w).0.is_empty(),
        "the land is lower than the wall tops"
    );
    (2..5).for_each(|y| set(&mut w, y, 2));
    assert_eq!(links(&w).0.len(), 1);
}

#[test]
fn a_show_tank_does_not_link() {
    let mut w = amphibious_world(2, &[]);
    let (tank, _) = pair(&w);
    w.exhibits
        .get_mut(tank)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .show = true;
    w.relink();
    assert_eq!(links(&w), (vec![], vec![]));
}

/// The step cost from the land tile (6, y) to the tank tile (5, y) for an animal that lives in
/// tanks and, unless `only`, on land too.
fn across(w: &World, only: bool) -> u32 {
    let from = 2 * WIDE + 6;
    let mut mover = crate::path::Mover::new(crate::path::Kind::Animal, Some(from));
    (mover.underwater, mover.only_underwater) = (true, only);
    crate::path::step(w, &mover, from, from - 1, 6, None)
}

#[test]
fn an_amphibious_animal_crosses_a_connector_and_one_that_lives_only_in_water_does_not() {
    let w = amphibious_world(2, &[]);
    assert!(across(&w, false) < crate::path::MAX);
    assert_eq!(across(&w, true), crate::path::MAX);
    // Without the mark on the wall it holds the animal back.
    let mut w = amphibious_world(2, &[]);
    w.connectors.clear();
    assert_eq!(across(&w, false), crate::path::MAX);
}
