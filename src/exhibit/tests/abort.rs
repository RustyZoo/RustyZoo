//! The abort event posted by every drain (shows 30.4 point 5): a zoo load, a merge, a split and
//! an undo, as the drain tool does.

use super::tank::{put_wall, ring, tank, tanks};
use super::water::loaded;
use super::*;
use crate::show::event::ABORT;

/// The abort events posted so far, by show id.
fn aborts(world: &World) -> Vec<u32> {
    let posts = &world.units.posts;
    (posts.iter().filter(|e| e.kind == ABORT).map(|e| e.target)).collect()
}

#[test]
fn loading_a_drained_tank_posts_the_abort_and_a_filled_one_does_not() {
    assert_eq!(aborts(&loaded(5, 0, &[])), [0]);
    assert!(aborts(&loaded(5, 2, &[])).is_empty());
}

#[test]
fn a_part_cut_off_a_drained_tank_posts_the_abort() {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    world.drain(1);
    let before = aborts(&world).len();
    put_wall(&mut world, "tank", (3, 2, 2));
    // The new part is drained like the tank it comes from.
    assert_eq!(aborts(&world).len(), before + 1);
}

#[test]
fn a_merge_that_ends_drained_posts_the_abort() {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    let line = put_wall(&mut world, "tank", (3, 2, 2));
    let mine = world.tank_at(2 * SIZE as usize + 2).unwrap();
    world.set_height(mine, 9);
    world.drain(mine);
    let copy = world
        .edge(2 * SIZE as usize + 4, 6)
        .and_then(|e| world.zoo.entities[e].common.as_ref())
        .map(|c| c.id)
        .unwrap();
    world.begin();
    world.remove(line);
    let before = aborts(&world).len();
    world.remove(copy);
    assert!(!tank(&world, 2, 2).unwrap().filled);
    assert_eq!(aborts(&world).len(), before + 1);
}

#[test]
fn an_undo_that_puts_a_drained_tank_back_posts_the_abort() {
    let mut world = loaded(9, 0, &[]);
    world.zoo.tanks.clear();
    let side = world.edge(2 * SIZE as usize + 2, 0).unwrap();
    let side = world.zoo.entities[side].common.as_ref().unwrap().id;
    world.begin();
    world.remove(side);
    world.units.posts.clear();
    assert!(!world.undo().is_empty());
    assert!(!tank(&world, 2, 2).unwrap().filled);
    assert_eq!(aborts(&world), [0]);
}
