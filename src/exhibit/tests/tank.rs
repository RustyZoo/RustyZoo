//! Tank exhibits on synthetic maps: which exhibits are tanks, which are show tanks, the tank
//! list the picture reads, and the default names.

use super::*;
use crate::exhibit::Tank;

type Piece = (&'static str, (i32, i32, u32));

/// A wall piece of a class of `shape`: `tank`, `show`, or an ordinary `fences` piece.
fn wall(class: &str, (x, y, d): (i32, i32, u32)) -> Entity {
    let mut e = entity(class, x, y, d);
    e.subtype = "f".into();
    e
}

/// The map with these pieces already on it, as if loaded.
fn built(pieces: &[Piece]) -> World {
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.iter().map(|&(class, p)| wall(class, p)));
    unique(&mut z);
    World::new(z, shape)
}

/// The pieces of `class` round the square of tiles `x0..=x1` by `y0..=y1`.
fn ring(class: &'static str, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Piece> {
    square(x0, y0, x1, y1)
        .into_iter()
        .map(|p| (class, p))
        .collect()
}

fn tank(world: &World, x: usize, y: usize) -> Option<Tank> {
    let t = y * SIZE as usize + x;
    world.tank_at(t).and_then(|id| world.tank(id))
}

#[test]
fn an_exhibit_closed_by_tank_walls_only_is_a_tank() {
    let world = built(&ring("tank", 2, 2, 3, 3));
    assert_eq!(sizes(&world), [4]);
    assert_eq!(tank(&world, 2, 3).map(|t| t.show), Some(false));
    assert_eq!(world.tank_at(0), None, "the grounds");
    // One piece of another fence makes it a land exhibit.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces[0].0 = "fences";
    let world = built(&pieces);
    assert_eq!(sizes(&world), [4]);
    assert_eq!(tank(&world, 2, 2), None);
}

#[test]
fn the_wall_of_either_tile_of_a_line_counts() {
    // The east line of the tank holds a tank wall on the slot of the tile outside only.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces.retain(|&(_, p)| p != (3, 2, 2) && p != (3, 3, 2));
    pieces.extend([("tank", (4, 2, 6)), ("tank", (4, 3, 6))]);
    assert!(tank(&built(&pieces), 2, 2).is_some());
    // A fence of another type inside and a tank wall outside: land.
    let mut pieces = ring("tank", 2, 2, 3, 3);
    pieces[2].0 = "fences";
    pieces.push(("tank", (2, 1, 4)));
    assert_eq!(tank(&built(&pieces), 2, 2), None);
}

#[test]
fn a_tank_is_a_show_tank_when_every_line_passes_and_one_has_a_show_wall() {
    assert_eq!(
        tank(&built(&ring("show", 2, 2, 3, 3)), 2, 2).map(|t| t.show),
        Some(true)
    );
    // One normal wall among show walls: not a show tank.
    let mut pieces = ring("show", 2, 2, 3, 3);
    pieces[0].0 = "tank";
    assert_eq!(tank(&built(&pieces), 2, 2).map(|t| t.show), Some(false));
    // The same with a second wall on the line, from the tile outside: the line passes.
    pieces.push(("show", (2, 1, 4)));
    assert_eq!(tank(&built(&pieces), 2, 2).map(|t| t.show), Some(true));
}

#[test]
fn a_show_wall_across_stands_for_the_tank_whose_wall_it_is_on_a_tile_of_another_tank() {
    // A normal tank (1..2) and a show tank (3..4) side by side, each with the wall of its own
    // tile on the shared line: each has the kind of its walls.
    let mut pieces = ring("tank", 1, 2, 2, 3);
    pieces.extend(ring("show", 3, 2, 4, 3));
    pieces.retain(|&(_, p)| !(p.2 == 2 && p.0 == 2) && !(p.2 == 6 && p.0 == 3));
    pieces.extend([("tank", (2, 2, 2)), ("tank", (2, 3, 2))]);
    pieces.extend([("show", (3, 2, 6)), ("show", (3, 3, 6))]);
    let world = built(&pieces);
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(false));
    assert_eq!(tank(&world, 4, 2).map(|t| t.show), Some(true));
    // Only the walls of the show tank on the shared line: the normal tank stands on a line
    // whose only wall is a show wall, but its other lines have normal ones: not a show tank.
    let mut only = pieces.clone();
    only.retain(|&(_, p)| !(p.0 == 2 && p.2 == 2));
    let world = built(&only);
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(false));
    assert_eq!(tank(&world, 4, 2).map(|t| t.show), Some(true));
}

#[test]
fn a_line_with_only_a_wall_across_passes_when_that_stands_on_a_tank_tile() {
    // The show tank (1..2) has show walls on three sides and on its east line only the plain
    // wall of the tank beside it (3..4), a tank tile: the line passes, so it is a show tank. If
    // the neighbour is a land exhibit instead, the line fails.
    let mut pieces = ring("show", 1, 2, 2, 3);
    pieces.retain(|&(_, p)| !(p.0 == 2 && p.2 == 2));
    let mut beside = ring("tank", 3, 2, 4, 3);
    let mut world = {
        let mut all = pieces.clone();
        all.append(&mut beside.clone());
        built(&all)
    };
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(true));
    assert_eq!(tank(&world, 4, 2).map(|t| t.show), Some(false));
    beside[0].0 = "fences";
    pieces.append(&mut beside);
    world = built(&pieces);
    assert_eq!(tank(&world, 1, 2).map(|t| t.show), Some(false));
    assert_eq!(tank(&world, 4, 2), None);
}

#[test]
fn the_tank_records_of_a_file_give_the_tanks_their_water() {
    let mut z = zoo(&[]);
    z.entities.extend(
        ring("show", 2, 2, 3, 3)
            .into_iter()
            .map(|(c, p)| wall(c, p)),
    );
    unique(&mut z);
    z.tanks = vec![crate::zoo::Tank {
        tile: (2, 2),
        gate: (3, 3),
        height: 9,
        level: 6,
        salt: false,
        purity: 80,
    }];
    let mut world = World::new(z, shape);
    let t = tank(&world, 3, 3).unwrap();
    assert_eq!(
        (t.show, t.height, t.level, t.salt, t.purity),
        (true, 9, 6, false, 80)
    );
    // The list the picture reads is the same, and follows the tanks that change.
    assert_eq!(world.zoo.tanks.len(), 1);
    assert_eq!(
        (world.zoo.tanks[0].tile, world.zoo.tanks[0].gate),
        ((2, 2), (3, 3))
    );
    assert!(!world.take_dirty(), "opening a map changes nothing");
    let side = at(&world, 2, 2, 0);
    world.remove(side);
    assert!(world.zoo.tanks.is_empty(), "a land exhibit is no tank");
    assert!(world.take_dirty());
}

#[test]
fn a_tank_closed_by_the_player_has_the_name_of_a_tank_or_a_show_tank() {
    let mut pieces = ring("tank", 2, 2, 3, 3);
    let last = pieces.pop().unwrap();
    let mut world = built(&pieces);
    world.set_exhibit_words(["Exhibit".into(), "Tank".into(), "Show Tank".into()]);
    world.add(wall(last.0, last.1), shape(&wall(last.0, last.1)));
    for id in world.waiting() {
        world.give_gate(id);
    }
    assert_eq!(names(&world), [(1, "Tank 1".to_string(), 4)]);
    assert!(tank(&world, 2, 2).is_some());
    assert_eq!(world.zoo.tanks.len(), 1);
    // A show tank next to it takes the counter on.
    for (c, p) in ring("show", 5, 5, 6, 6) {
        world.add(wall(c, p), shape(&wall(c, p)));
        for id in world.waiting() {
            world.give_gate(id);
        }
    }
    assert_eq!(names(&world)[1], (2, "Show Tank 2".to_string(), 4));
    assert_eq!(tank(&world, 5, 5).map(|t| t.show), Some(true));
    // A land exhibit is still "Exhibit".
    for (c, p) in ring("fences", 0, 5, 0, 6) {
        world.add(wall(c, p), shape(&wall(c, p)));
        for id in world.waiting() {
            world.give_gate(id);
        }
    }
    assert_eq!(names(&world)[2].1, "Exhibit 3");
}

#[test]
fn the_tiles_of_a_tank_are_carried_by_any_tile_of_it() {
    let world = built(&ring("tank", 2, 2, 3, 3));
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let mut tiles = world.carried(t(3, 2));
    tiles.sort_unstable();
    assert_eq!(tiles, [t(2, 2), t(3, 2), t(2, 3), t(3, 3)]);
    assert!(world.carried(t(0, 0)).is_empty());
}
