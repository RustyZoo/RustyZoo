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

/// The id of the piece on side `d` of tile (`x`, `y`).
fn piece_at(world: &World, x: usize, y: usize, d: usize) -> u32 {
    let at = world.edge(y * SIZE as usize + x, d).unwrap();
    world.zoo.entities[at].common.as_ref().unwrap().id
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
    // X's walls are brought to its side first (the copy of the cut piece, healthy), then X, then
    // Y: the level it takes from X, its own update; the last two wait for the gate.
    use Draw::{Heal, Ripples, Walls};
    let copy = piece_at(&world, 4, 2, 6);
    assert_eq!(world.draws(), [Heal(copy), Walls(x), Ripples(y), Walls(y)]);
    for new in world.waiting() {
        world.give_gate(new);
    }
    let draws = world.draws();
    assert_eq!(draws.len(), 7, "{draws:?}");
    assert!(matches!(draws[4], Walls(t) if t == y || t == x), "the gate");
    assert_eq!(draws[5..], [Walls(x), Walls(y)]);
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

/// A save sets every clean tank's ripples up again (tank staff 9), a tank built filled while
/// paused that no gate update reached too.
#[test]
fn a_save_takes_the_bare_mark_off_a_tank_built_filled_while_paused() {
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.set_paused(true);
    world.begin();
    put_wall(&mut world, last.0, last.1);
    let id = tank_id(&world, 2);
    assert!(world.is_bare(id), "built filled while paused");
    let mut sim = crate::sim::Sim::new(b"");
    sim.save_started(&mut world);
    assert!(!world.is_bare(id), "still bare after the save");
}

/// The piece that cuts a tank is on the side of the new tank: the new tank takes a copy, after the
/// start times of the cut tank (map drawing 9.7.8 point 3.4).
#[test]
fn a_split_whose_piece_stands_on_the_cut_tank_copies_it_for_the_new_tank_after_the_cut_one() {
    let mut world = watered();
    let x = tank_id(&world, 2);
    let wall = wall("tank", (4, 2, 6));
    let shape = tank_shape(&wall);
    world.add(wall, shape).unwrap();
    // X keeps the side of its gate, the east.
    let (y, copy) = (tank_id(&world, 2), piece_at(&world, 3, 2, 2));
    assert_ne!(x, y);
    use Draw::{Heal, Ripples, Walls};
    let draws = world.draws();
    assert_eq!(draws[..2], [Walls(x), Heal(copy)], "{draws:?}");
    assert_eq!(draws[2..], [Ripples(y), Walls(y)], "{draws:?}");
}

/// The walls a new tank brings inwards are validated again: each moved piece draws a start time,
/// in the order of the tank's boundary, and nothing else is drawn (no water).
#[test]
fn the_walls_a_new_tank_brings_inwards_draw_a_start_time_each() {
    let pieces = [
        ("tank", (2, 1, 4)),
        ("tank", (3, 1, 4)),
        ("tank", (2, 3, 0)),
        ("tank", (3, 3, 0)),
        ("tank", (1, 2, 2)),
    ];
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    let last = wall("tank", (4, 2, 6));
    let shape = tank_shape(&last);
    world.add(last, shape).unwrap();
    let inside = [(2, 2), (3, 2)].map(|(x, y)| y * SIZE as usize + x);
    let mut moved = Vec::new();
    for t in inside {
        for d in [0, 2, 4, 6] {
            let across = world.neighbour(t, d).unwrap();
            let piece = world.edge(t, d).filter(|_| !inside.contains(&across));
            moved.extend(piece.map(|p| world.zoo.entities[p].common.as_ref().unwrap().id));
        }
    }
    let mut starts: Vec<u32> = (world.draws().iter())
        .map(|d| match d {
            Draw::Start(id) => *id,
            other => panic!("{other:?}"),
        })
        .collect();
    starts.sort_unstable();
    moved.sort_unstable();
    assert_eq!(starts.len(), 6);
    assert_eq!(starts, moved);
}

/// A merge deletes the walls that stood between the two tanks after the kept tank's update, not
/// before: the update still counts them (map drawing 9.7.8 points 3.2.5 and 3.5).
#[test]
fn a_merge_deletes_the_walls_between_the_two_tanks_after_the_update() {
    let mut pieces = ring("tank", 2, 2, 5, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    put_wall(&mut world, last.0, last.1);
    (0..2).for_each(|_| _ = world.step_water());
    put_wall(&mut world, "tank", (3, 2, 2));
    put_wall(&mut world, "tank", (3, 3, 2));
    assert_eq!(world.exhibits().list().len(), 2);
    let width = SIZE as usize;
    // Each tank has its own piece on both lines; the row 2 pair opens first.
    let (line, copy) = (piece_at(&world, 3, 2, 2), piece_at(&world, 4, 2, 6));
    let between = [piece_at(&world, 3, 3, 2), piece_at(&world, 4, 3, 6)];
    while world.next_draw(false).is_some() {}
    world.remove(line);
    assert!(world.draws().is_empty(), "the line stays closed");
    world.remove(copy);
    let kept = tank_id(&world, 2);
    assert_eq!(world.exhibits().list().len(), 1);
    let draws = world.draws();
    let at = draws.iter().position(|d| *d == Draw::Walls(kept));
    let at = at.unwrap_or_else(|| panic!("{draws:?}"));
    let mut gone: Vec<u32> = (draws[at + 1..].iter())
        .map(|d| match d {
            Draw::Delete(id) => *id,
            other => panic!("{other:?}"),
        })
        .collect();
    gone.sort_unstable();
    let mut want = between.to_vec();
    want.sort_unstable();
    assert_eq!(gone, want);
    // Still there when the update runs; the tile edges are free once the deletes run.
    assert!(world.edge(3 * width + 3, 2).is_some());
    while let Some(draw) = world.next_draw(false) {
        if let Draw::Delete(id) = draw {
            assert!(world.remove_one(id, false));
        }
    }
    assert!(world.edge(3 * width + 3, 2).is_none());
    assert!(world.edge(3 * width + 4, 6).is_none());
    assert_eq!(world.exhibits().list().len(), 1);
}

/// An undo that merges two tanks deletes nothing (not while undoing).
#[test]
fn an_undone_removal_that_merges_two_tanks_deletes_no_wall() {
    let mut pieces = ring("tank", 2, 2, 5, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.begin();
    put_wall(&mut world, last.0, last.1);
    put_wall(&mut world, "tank", (3, 2, 2));
    world.begin();
    put_wall(&mut world, "tank", (3, 3, 2));
    assert_eq!(world.exhibits().list().len(), 2);
    while world.next_draw(false).is_some() {}
    // Undoing the cut joins the tanks again.
    assert!(!world.undo().is_empty());
    assert_eq!(world.exhibits().list().len(), 1);
    assert!(
        world.draws().iter().all(|d| !matches!(d, Draw::Delete(_))),
        "{:?}",
        world.draws()
    );
}
