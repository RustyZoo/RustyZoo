//! The water of a tank (exhibits and money 3.10): the rise and fall by one elevation step per
//! simulation step, the pause, the wall height, a split and a merge.

use super::tank::{CONFIG, put_wall, ring, tank, tank_shape, tanks};
use super::*;

/// A new exhibit tank (2..3, 2..3), made with its last wall.
fn new_tank(kind: &'static str, things: &[(&str, i32, i32)]) -> World {
    let mut pieces = ring(kind, 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, things, &[]);
    world.begin();
    world.take_events();
    put_wall(&mut world, last.0, last.1);
    world
}

fn water(world: &World) -> (u32, u32, bool) {
    let t = tank(world, 2, 2).unwrap();
    (t.height, t.level, t.filled)
}

#[test]
fn a_new_tank_starts_empty_and_filled_and_rises_one_step_per_simulation_step() {
    let mut world = new_tank("tank", &[]);
    assert_eq!(water(&world), (5, 0, true));
    let mut levels = Vec::new();
    for _ in 0..6 {
        world.step_water();
        levels.push(water(&world).1);
    }
    // Full at the height less one (waterOffset -1), 4 steps for an exhibit tank.
    assert_eq!(levels, [1, 2, 3, 4, 4, 4]);
    let mut world = new_tank("show", &[]);
    let steps = (0..20).take_while(|_| world.step_water()).count();
    assert_eq!((steps, water(&world).1), (8, 8));
    // Nothing moves, and nothing is told, once it is full.
    world.take_events();
    assert!(!world.step_water());
    assert!(world.take_events().is_empty());
}

#[test]
fn the_level_is_told_to_the_picture() {
    let mut world = new_tank("tank", &[]);
    world.step_water();
    world.step_water();
    assert_eq!(world.zoo.tanks[0].level, 2);
    assert!(world.take_dirty());
}

#[test]
fn a_tank_made_while_paused_is_full_at_once_and_drained_at_once() {
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    world.set_paused(true);
    world.begin();
    put_wall(&mut world, last.0, last.1);
    assert_eq!(water(&world), (5, 4, true));
    world.drain(1);
    assert_eq!(water(&world), (5, 0, false));
    world.fill(1);
    assert_eq!(water(&world), (5, 4, true));
}

#[test]
fn a_drain_empties_one_step_per_simulation_step_and_a_fill_fills_again() {
    let mut world = new_tank("tank", &[]);
    (0..4).for_each(|_| _ = world.step_water());
    world.drain(1);
    let levels: Vec<u32> = (0..5)
        .map(|_| {
            world.step_water();
            water(&world).1
        })
        .collect();
    assert_eq!(levels, [3, 2, 1, 0, 0]);
    world.fill(1);
    world.step_water();
    assert_eq!(water(&world), (5, 1, true));
}

#[test]
fn an_animal_in_the_tank_keeps_it_from_draining() {
    let mut world = new_tank("tank", &[("animals", 3 * 64 + 30, 3 * 64 + 30)]);
    world.drain(1);
    assert!(water(&world).2, "an animal stays");
}

#[test]
fn a_guest_left_in_a_new_tank_leaves_it_empty() {
    let mut world = new_tank("tank", &[("guests", 2 * 64 + 5, 3 * 64 + 5)]);
    assert!(world.take_events().contains(&Event::Objects(1)));
    world.accept_tank(1);
    assert_eq!(water(&world), (5, 0, false), "the fill was refused twice");
    for _ in 0..3 {
        world.step_water();
    }
    assert_eq!(water(&world).1, 0);
}

#[test]
fn a_tank_asked_about_is_filled_after_the_yes() {
    // A bench is sold on the Yes: nothing is left that refuses water.
    let mut world = new_tank("tank", &[("bench", 2 * 64 + 16, 2 * 64 + 16)]);
    assert!(world.take_events().contains(&Event::Objects(1)));
    assert!(!water(&world).2, "the bench refuses the water for now");
    world.remove(
        world
            .zoo
            .entities
            .iter()
            .find(|e| e.class == "bench")
            .and_then(|e| e.common.as_ref())
            .map(|c| c.id)
            .unwrap(),
    );
    world.accept_tank(1);
    assert!(water(&world).2);
}

#[test]
fn lowering_the_walls_cuts_the_water_at_once_and_raising_them_lets_it_rise() {
    let mut world = new_tank("tank", &[]);
    (0..4).for_each(|_| _ = world.step_water());
    world.set_height(1, 3);
    assert_eq!(water(&world), (3, 2, true), "cut to the full level at once");
    world.set_height(1, 6);
    world.step_water();
    assert_eq!(water(&world), (6, 3, true), "one step up");
    // Never below 2 and never above the highest wall of tanks.cfg.
    world.set_height(1, 0);
    assert_eq!(water(&world).0, 2);
    world.set_height(1, 4_000_000_000);
    assert_eq!(water(&world).0, CONFIG.limit);
}

#[test]
fn a_drained_tank_does_not_move_when_its_walls_rise() {
    let mut world = new_tank("tank", &[]);
    world.drain(1);
    world.set_height(1, 8);
    world.step_water();
    assert_eq!(water(&world), (8, 0, false));
}

#[test]
fn a_tank_cut_in_two_gives_the_part_the_level_of_the_tank_it_comes_from() {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    (0..2).for_each(|_| _ = world.step_water());
    put_wall(&mut world, "tank", (3, 2, 2));
    let (a, b) = (tank(&world, 2, 2).unwrap(), tank(&world, 5, 2).unwrap());
    assert_eq!((a.level, a.filled), (2, true));
    assert_eq!(
        (b.level, b.filled),
        (2, true),
        "the new part has the level too"
    );
}

#[test]
fn two_tanks_that_merge_get_the_mean_height_and_the_flag_of_the_larger_volume() {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    let line = put_wall(&mut world, "tank", (3, 2, 2));
    assert_eq!(world.exhibits().list().len(), 2);
    // Tank 1 (tiles 2 and 3) stands at 9 and is drained, tank 2 stays at 5 and is filled.
    let mine = world.tank_at(2 * SIZE as usize + 2).unwrap();
    world.set_height(mine, 9);
    world.drain(mine);
    (0..4).for_each(|_| _ = world.step_water());
    assert_eq!(water(&world), (9, 0, false));
    let other = tank(&world, 5, 2).unwrap();
    assert_eq!((other.height, other.level, other.filled), (5, 4, true));
    // Both walls of the line go: the tanks become one.
    let copy = world
        .edge(2 * SIZE as usize + 4, 6)
        .and_then(|e| world.zoo.entities[e].common.as_ref())
        .map(|c| c.id)
        .unwrap();
    world.begin();
    world.remove(line);
    world.remove(copy);
    assert_eq!(world.exhibits().list().len(), 1);
    // The mean height is (9 * 2 + 5 * 2) / 4 = 7; the drained tank has the larger volume.
    let merged = tank(&world, 2, 2).unwrap();
    assert_eq!((merged.height, merged.filled), (7, false));
}

#[test]
fn the_pending_gate_over_a_tank_wall_takes_the_height_of_the_tank() {
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let height = |w: &World, x, y, d| w.wall_height(w.edge(t(x, y), d).unwrap());
    let world = new_tank("tank", &[]);
    assert_eq!(height(&world, 2, 2, 0), 5, "an exhibit tank");
    assert_eq!(height(&world, 3, 3, 4), 5);
    assert_eq!(height(&new_tank("show", &[]), 2, 2, 0), 9, "a show tank");
    // The height of the tank, not the water: it does not move with the level.
    let mut world = new_tank("tank", &[]);
    world.step_water();
    assert_eq!(height(&world, 2, 2, 0), 5);
    world.set_height(1, 7);
    assert_eq!(height(&world, 2, 2, 0), 7);
    // A wall that no tank records, and a piece that is no tank wall, have none.
    let loaded = tanks(&ring("tank", 2, 2, 3, 3), &[], &[]);
    assert_eq!(height(&loaded, 2, 2, 0), 0);
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces[0].0 = "fences";
    let land = tanks(&pieces, &[], &[]);
    assert_eq!(land.wall_height(land.edge(t(2, 2), 0).unwrap()), 0);
    // Not a piece of the list: nothing.
    assert_eq!(world.wall_height(usize::MAX), 0);
}

/// A tank (2..3, 2..3) of a loaded map with this height and level, and things in it.
fn loaded(height: u32, level: u32, things: &[(&str, i32, i32)]) -> World {
    let mut z = zoo(&[]);
    z.entities.extend(
        ring("tank", 2, 2, 3, 3)
            .into_iter()
            .map(|(c, p)| entity_wall(c, p)),
    );
    z.entities.extend(things.iter().map(|&(c, x, y)| {
        let mut e = entity(c, 0, 0, 0);
        let at = e.common.as_mut().unwrap();
        (at.x, at.y) = (x, y);
        e
    }));
    unique(&mut z);
    z.tanks = vec![crate::zoo::Tank {
        tile: (2, 2),
        gate: (-1, -1),
        height,
        level,
        salt: true,
        purity: 90,
    }];
    let mut world = World::new(z, tank_shape);
    world.set_tank_config(CONFIG);
    world.take_events();
    world
}

fn entity_wall(class: &str, p: (i32, i32, u32)) -> Entity {
    let mut e = entity(class, p.0, p.1, p.2);
    e.subtype = "f".into();
    e
}

#[test]
fn a_loaded_tank_goes_through_the_wall_and_level_rules_and_moves_on_by_its_flag() {
    // Saved while filling: it goes on rising to the full level.
    let mut world = loaded(5, 2, &[]);
    assert_eq!(water(&world), (5, 2, true));
    world.step_water();
    world.step_water();
    world.step_water();
    assert_eq!(water(&world), (5, 4, true));
    // Level above the full level: cut. Walls below 2 are raised to 2. Empty: drained.
    assert_eq!(water(&loaded(3, 9, &[])), (3, 2, true));
    assert_eq!(water(&loaded(0, 0, &[])), (2, 0, false));
    assert_eq!(
        water(&loaded(4_000_000_000, 1, &[])),
        (CONFIG.limit, 1, true)
    );
    let mut empty = loaded(5, 0, &[]);
    empty.step_water();
    assert_eq!(water(&empty), (5, 0, false), "it stays empty");
    // Opening the map tells nothing.
    assert!(loaded(5, 2, &[]).take_events().is_empty());
}

#[test]
fn a_part_cut_off_a_drained_tank_is_drained_unless_an_animal_stands_in_it() {
    let cut = |animal: bool| {
        let mut pieces = ring("tank", 2, 2, 5, 2);
        let last = pieces.pop().unwrap();
        let mut world = tanks(&pieces, &[], &[]);
        put_wall(&mut world, last.0, last.1);
        world.drain(1);
        (0..4).for_each(|_| _ = world.step_water());
        if animal {
            for x in [2, 5] {
                let fish = entity("animals", x, 2, 0);
                world.add(fish.clone(), tank_shape(&fish)).unwrap();
            }
        }
        put_wall(&mut world, "tank", (3, 2, 2));
        // The new part is the one that is not drained like the tank it comes from.
        let parts = [2, 5].map(|x| tank(&world, x, 2).unwrap());
        (
            parts.iter().map(|p| p.level).sum::<u32>(),
            parts.iter().filter(|p| p.filled).count(),
        )
    };
    assert_eq!(cut(false), (0, 0));
    // The drain is refused with an animal in the new part: it stays as the creation made it.
    assert_eq!(cut(true), (0, 1));
}

#[test]
fn a_merge_that_ends_drained_removes_what_the_water_would_refuse_without_a_question() {
    let mut pieces = ring("tank", 2, 2, 5, 2);
    let last = pieces.pop().unwrap();
    // A bench in the tank (2, 2), put there after the tank was made.
    let mut world = tanks(&pieces, &[], &[]);
    put_wall(&mut world, last.0, last.1);
    let line = put_wall(&mut world, "tank", (3, 2, 2));
    let mine = world.tank_at(2 * SIZE as usize + 2).unwrap();
    world.set_height(mine, 9);
    world.drain(mine);
    let bench = entity("bench", 2, 2, 0);
    let id = world.add(bench.clone(), tank_shape(&bench)).unwrap();
    let copy = world
        .edge(2 * SIZE as usize + 4, 6)
        .and_then(|e| world.zoo.entities[e].common.as_ref())
        .map(|c| c.id)
        .unwrap();
    world.begin();
    world.remove(line);
    world.remove(copy);
    assert!(
        !tank(&world, 2, 2).unwrap().filled,
        "the larger volume is drained"
    );
    assert!(world.find(id).is_none(), "the bench is gone, nobody asked");
}
