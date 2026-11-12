//! The habitat section of a save: one record per exhibit of the list, in its order (zoo format
//! 4.1.1), from the live exhibit.

use super::tank::{put_wall, ring, tank_shape, tanks, wall};
use super::*;

/// The zoo a save of `world` writes, read back from the written file.
fn written(world: &World) -> Zoo {
    let saved = world.saved(|_| true);
    let (mut file, money) =
        Zoo::parse_money(&crate::zoo::tests::file(106, 8, 8, &[], &[])).unwrap();
    (file.habitat_tiles, file.habitat_gates) = (saved.habitat_tiles, saved.habitat_gates);
    (file.habitat_info, file.tanks, file.shows) = (saved.habitat_info, saved.tanks, saved.shows);
    Zoo::parse(&file.write(&money).unwrap()).unwrap()
}

/// A tank (2..5 by 2) built in play, named.
fn built() -> World {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    put_wall(&mut world, last.0, last.1);
    world
}

#[test]
fn a_tank_built_in_play_is_saved_with_a_record() {
    let mut world = built();
    let id = world.tank_at(2 * SIZE as usize + 2).unwrap();
    world.rename(id, "Dolphins".into());
    let back = written(&world);
    assert_eq!(back.habitat_tiles, [(2, 2)]);
    assert_eq!(back.habitat_info[0].name, "Dolphins");
    assert_eq!(back.habitat_info[0].created, 0);
    let live = world.tank(id).unwrap();
    assert_eq!(back.tanks.len(), 1);
    let t = back.tanks[0];
    assert_eq!(
        (t.height, t.level, t.filled),
        (live.height, live.level, live.filled)
    );
    assert_eq!(
        (t.salt, t.target, t.purity),
        (live.salt, live.target, live.purity)
    );
}

#[test]
fn both_halves_of_a_split_tank_are_saved_with_a_record() {
    let mut world = built();
    let piece = wall("tank", (3, 2, 2));
    world.add(piece.clone(), tank_shape(&piece)).unwrap();
    for new in world.waiting() {
        world.give_gate(new);
    }
    assert_eq!(world.exhibits().list().len(), 2);
    let back = written(&world);
    assert_eq!(back.habitat_tiles.len(), 2);
    assert_eq!(back.tanks.len(), 2);
    let live: Vec<_> = world
        .exhibits()
        .list()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    let names: Vec<_> = back.habitat_info.iter().map(|i| i.name.clone()).collect();
    assert_eq!(names, live, "in the order of the list");
    assert!(names.iter().all(|n| !n.is_empty()));
}

#[test]
fn an_exhibit_that_is_gone_has_no_record() {
    let mut world = saved_world([0; 9]);
    assert_eq!(world.zoo.habitat_tiles.len(), 2, "the file stored two");
    assert_eq!(
        written(&world).habitat_tiles.len(),
        1,
        "the record of the grounds is dropped"
    );
    let gone = at(&world, 2, 2, 0);
    assert!(world.remove(gone));
    assert!(world.exhibits().list().is_empty());
    let back = written(&world);
    assert!(back.habitat_tiles.is_empty());
    assert!(back.habitat_info.is_empty());
}

#[test]
fn a_reload_keeps_the_names_and_the_first_tile_of_the_exhibits() {
    let mut world = built();
    let id = world.tank_at(2 * SIZE as usize + 2).unwrap();
    world.rename(id, "Dolphins".into());
    let mut saved = world.saved(|_| true);
    saved.locale = 1033;
    let reloaded = World::new(saved, tank_shape);
    let id = reloaded.tank_at(2 * SIZE as usize + 2).unwrap();
    assert_eq!(reloaded.exhibits().get(id).unwrap().name, "Dolphins");
}

/// The first tile column of each exhibit of a save and a reload of `world`, in list order.
fn firsts(world: &World) -> Vec<usize> {
    let mut saved = world.saved(|_| true);
    saved.locale = 1033;
    let list = World::new(saved, tank_shape);
    let min = |e: &Exhibit| e.order.iter().map(|t| t % SIZE as usize).min().unwrap();
    list.exhibits().list().iter().map(min).collect()
}

#[test]
fn a_reload_lists_the_half_on_the_cut_piece_first_whatever_the_record_order() {
    for (piece, want) in [((3, 2, 2), vec![2, 4]), ((4, 2, 6), vec![4, 2])] {
        let mut pieces = ring("tank", 2, 2, 5, 2);
        let last = pieces.pop().unwrap();
        let mut world = tanks(&pieces, &[], &[]);
        world.begin();
        put_wall(&mut world, last.0, last.1);
        put_wall(&mut world, "tank", piece);
        assert_eq!(firsts(&world), want, "cut {piece:?}");
    }
}
