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

/// The show neighbours (exhibits and money 3.11) of `show_world`: the plain tank `T` from (2, 2),
/// the show tank `S` from (6, 2) and the land exhibit `L` from (0, 2).
mod show {
    use super::*;
    use crate::unit::testing::show_world;
    use crate::world::TankValue;

    fn ids(w: &World) -> (u32, u32, u32) {
        let region = |x: usize| w.exhibits.region(2 * WIDE + x).unwrap();
        (region(2), region(6), region(0))
    }

    /// The show neighbours of `T`, `S` and `L`.
    fn shows(w: &World) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
        let (t, s, l) = ids(w);
        let of = |x: u32| w.exhibits.get(x).unwrap().show_near.clone();
        (of(t), of(s), of(l))
    }

    fn tank_mut(w: &mut World, id: u32) -> &mut crate::exhibit::Tank {
        w.exhibits.get_mut(id).unwrap().tank.as_mut().unwrap()
    }

    #[test]
    fn a_filled_show_tank_and_a_filled_plain_tank_level_with_it_are_show_neighbours() {
        let w = show_world(&[]);
        let (t, s, l) = ids(&w);
        assert_eq!(shows(&w), (vec![s], vec![t], vec![]));
        // The land exhibit is the plain tank's amphibious neighbour, not a show neighbour.
        assert_eq!(w.exhibits.get(t).unwrap().amphibious, [l]);
    }

    #[test]
    fn each_condition_of_the_link_breaks_it() {
        let linked = |w: &World| !shows(w).0.is_empty();
        let (t, s, _) = ids(&show_world(&[]));
        assert!(linked(&show_world(&[])), "the world as built");
        // Two show tanks, two plain tanks.
        for show in [(true, true), (false, false)] {
            let mut w = show_world(&[]);
            tank_mut(&mut w, t).show = show.0;
            tank_mut(&mut w, s).show = show.1;
            w.relink();
            assert!(!linked(&w), "{show:?}");
        }
        // A tank that is not filled, on either side.
        for id in [t, s] {
            let mut w = show_world(&[]);
            w.drain(id);
            assert!(!linked(&w), "drained {id}");
        }
        // Walls lower than 4 with their tops level: both at 3.
        let mut w = show_world(&[]);
        tank_mut(&mut w, t).height = 3;
        tank_mut(&mut w, s).height = 3;
        w.relink();
        assert!(!linked(&w), "3 high");
        // Wall tops at different levels.
        let mut w = show_world(&[]);
        w.set_height(s, 5);
        assert!(!linked(&w), "5 against 4");
    }

    #[test]
    fn the_wall_tops_are_the_height_plus_the_base_level() {
        let mut w = show_world(&[]);
        let (t, s, _) = ids(&w);
        // A higher base level of S with the same walls breaks the link, a taller T gives it back.
        let tile = w.zoo.tiles[2 * WIDE + 6];
        w.set_tile(6, 2, tile.elevation + 1, tile.shape, tile.terrain);
        assert!(shows(&w).0.is_empty(), "S one step higher");
        w.set_height(t, 5);
        assert_eq!(shows(&w).0, [s], "tops at 5 on both");
    }

    #[test]
    fn the_link_follows_a_new_wall_height() {
        let mut w = show_world(&[]);
        let (t, s, _) = ids(&w);
        w.set_height(t, 5);
        assert_eq!(shows(&w), (vec![], vec![], vec![]));
        w.set_height(s, 5);
        assert_eq!(shows(&w), (vec![s], vec![t], vec![]));
        w.drain(t);
        assert_eq!(shows(&w), (vec![], vec![], vec![]), "drained");
        w.edit_tank(t, |k, _| k.filled = true);
        assert_eq!(shows(&w), (vec![s], vec![t], vec![]), "filled again");
    }

    #[test]
    fn an_undone_filled_state_rebuilds_the_show_links() {
        let mut w = show_world(&[]);
        let (t, s, _) = ids(&w);
        w.drain(t);
        assert!(shows(&w).0.is_empty(), "drained");
        // The undo of a removed wall puts the filled flag back.
        w.restore_tank(2 * WIDE + 2, TankValue::Filled(true));
        assert_eq!(shows(&w), (vec![s], vec![t], vec![]), "filled again");
        w.restore_tank(2 * WIDE + 2, TankValue::Filled(false));
        assert_eq!(shows(&w), (vec![], vec![], vec![]), "drained again");
    }
}
