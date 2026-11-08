//! The wall-information updates of a tank, which set its ripples up again (map drawing 9.7.8
//! point 3.2 to 3.4): which edits owe one, for which tank, and in which order with the other
//! draws.

use super::tank::{put_wall, ring, tank, tank_shape, tanks, wall};
use super::*;
use crate::world::Draw;

/// A tank (2..5 by 2) with water and its gate chosen, all draws taken.
fn watered() -> World {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    put_wall(&mut world, last.0, last.1);
    (0..2).for_each(|_| _ = world.step_water());
    while world.next_draw(false).is_some() {}
    world
}

fn tank_id(world: &World, x: usize) -> u32 {
    world.tank_at(2 * SIZE as usize + x).unwrap()
}

#[test]
fn a_split_updates_x_then_the_new_tank_then_ends_after_the_gate_with_x_and_y() {
    let mut world = watered();
    let x = tank_id(&world, 2);
    let wall = wall("tank", (3, 2, 2));
    let shape = tank_shape(&wall);
    world.add(wall, shape).unwrap();
    // X keeps the side of its gate; the new tank is on the other side.
    let y = tank_id(&world, 2);
    assert_ne!(x, y);
    // X, then Y: the level it takes from X, its own update; the last two wait for the gate.
    use Draw::{Ripples, Walls};
    assert_eq!(world.draws(), [Walls(x), Ripples(y), Walls(y)]);
    for new in world.waiting() {
        world.give_gate(new);
    }
    let draws = world.draws();
    assert_eq!(draws.len(), 6, "{draws:?}");
    assert!(matches!(draws[3], Walls(t) if t == y || t == x), "the gate");
    assert_eq!(draws[4..], [Walls(x), Walls(y)]);
}

#[test]
fn a_merge_with_a_gate_updates_the_tank_that_stays_once() {
    let mut world = watered();
    let line = put_wall(&mut world, "tank", (3, 2, 2));
    let copy = world
        .edge(2 * SIZE as usize + 4, 6)
        .and_then(|e| world.zoo.entities[e].common.as_ref())
        .map(|c| c.id)
        .unwrap();
    while world.next_draw(false).is_some() {}
    world.remove(line);
    assert!(world.draws().is_empty(), "the line stays closed");
    world.remove(copy);
    let kept = tank_id(&world, 2);
    let updates = world.draws().iter().filter(|d| **d == Draw::Walls(kept));
    assert_eq!(updates.count(), 1, "{:?}", world.draws());
}

#[test]
fn a_gate_moved_onto_a_tank_wall_draws_the_old_start_then_updates_the_tank() {
    let mut world = watered();
    let id = tank_id(&world, 2);
    let old = world.exhibits().get(id).unwrap().gate.unwrap();
    let new = world.edge(2 * SIZE as usize + 2, 0).unwrap();
    let new = world.zoo.entities[new].common.as_ref().unwrap().id;
    assert_ne!(old, new);
    while world.next_draw(false).is_some() {}
    assert!(world.move_gate(id, new, 7, 300));
    assert_eq!(world.draws(), [Draw::Start(old), Draw::Walls(id)]);
    // A gate on a piece of a land exhibit's tile updates no tank.
    assert!(tank(&world, 2, 2).is_some());
}

#[test]
fn a_tank_wall_edit_that_closes_nothing_or_removes_a_wall_updates_nothing() {
    let mut world = watered();
    let id = tank_id(&world, 2);
    // The height tool draws no ripple.
    world.set_height(id, 3);
    assert!(
        world.draws().iter().all(|d| !matches!(d, Draw::Walls(_))),
        "{:?}",
        world.draws()
    );
    while world.next_draw(false).is_some() {}
    // A piece that closes nothing outside the tank.
    put_wall(&mut world, "tank", (6, 6, 0));
    assert!(world.draws().is_empty());
}

#[test]
fn an_active_wall_is_a_tank_wall_with_a_wall_height() {
    let mut world = watered();
    let id = tank_id(&world, 2);
    let wall = world.edge(2 * SIZE as usize + 2, 0).unwrap();
    let wall = world.zoo.entities[wall].common.as_ref().unwrap().id;
    assert_eq!(world.active_wall_tank(wall), Some(id));
    assert_eq!(world.active_wall_tank(u32::MAX), None, "missing");
    let far = put_wall(&mut world, "tank", (6, 6, 0));
    assert_eq!(world.active_wall_tank(far), None, "not a wall of a tank");
}

#[test]
fn a_piece_of_the_same_type_bought_on_an_active_wall_updates_its_tank_then_draws_a_start() {
    let mut world = watered();
    let id = tank_id(&world, 2);
    let wall = world.edge(2 * SIZE as usize + 2, 0).unwrap();
    let wall = world.zoo.entities[wall].common.as_ref().unwrap().id;
    assert!(world.retyped_wall(wall));
    assert_eq!(world.draws(), [Draw::Walls(id), Draw::Start(wall)]);
    while world.next_draw(false).is_some() {}
    // Nothing for a piece that is not an active tank wall, or no piece.
    let far = put_wall(&mut world, "tank", (6, 6, 0));
    assert!(!world.retyped_wall(far) && !world.retyped_wall(u32::MAX));
    assert!(world.draws().is_empty());
}

#[test]
fn a_new_tank_filled_at_once_while_paused_sets_no_ripples_up() {
    for (paused, full) in [(true, true), (false, false)] {
        let mut pieces = ring("tank", 2, 2, 3, 3);
        let last = pieces.pop().unwrap();
        let mut world = tanks(&pieces, &[], &[]);
        world.set_paused(paused);
        world.begin();
        put_wall(&mut world, last.0, last.1);
        let id = tank_id(&world, 2);
        let level = world.tank(id).unwrap().level;
        assert_eq!(level > 0, full, "paused {paused}");
        assert!(!world.draws().contains(&Draw::Ripples(id)), "{paused}");
        assert_eq!(world.is_bare(id), full, "paused {paused}");
    }
}
