//! The regions on synthetic maps.

use super::*;
use crate::render::order::Kind;
use crate::world::Shape;
use crate::zoo::{Common, Entity, Tile, View, Zoo};

const SIZE: i32 = 8;

fn entity(class: &str, tx: i32, ty: i32, rotation: u32) -> Entity {
    Entity {
        class: class.to_string(),
        type_name: String::new(),
        subtype: String::new(),
        body_size: None,
        common: Some(Common {
            x: tx * 64 + 32,
            y: ty * 64 + 32,
            z: 0,
            rotation,
            id: 0,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
            movement_mode: 0,
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let fence = e.class == "fences";
    match e.class.as_str() {
        "fences" | "plain" => Some(Shape {
            kind: Kind::Edge,
            footprint: (1, 1),
            fence,
        }),
        "gate" => Some(Shape {
            kind: Kind::Fixed,
            footprint: (2, 2),
            fence: false,
        }),
        _ => None,
    }
}

/// An 8 by 8 map whose entrance is a gate on tile (7, 7) facing east, off the map: no exterior.
fn zoo(fences: &[(i32, i32, u32)]) -> Zoo {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let mut entities = vec![entity("gate", SIZE - 1, SIZE - 1, 2)];
    entities.extend(fences.iter().map(|&(x, y, d)| entity("fences", x, y, d)));
    Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: SIZE as u32,
        height: SIZE as u32,
        entrance: (SIZE as u32 - 1, SIZE as u32 - 1),
        habitat_count: 0,
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; (SIZE * SIZE) as usize],
        entities,
        view: View::default(),
    }
}

/// The pieces round the square of tiles `x0..=x1` by `y0..=y1`, each on the edge of its own tile.
fn square(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32, u32)> {
    let mut pieces = Vec::new();
    for x in x0..=x1 {
        pieces.push((x, y0, 0));
        pieces.push((x, y1, 4));
    }
    for y in y0..=y1 {
        pieces.push((x0, y, 6));
        pieces.push((x1, y, 2));
    }
    pieces
}

fn sizes(world: &World) -> Vec<usize> {
    world.exhibits().list().iter().map(|e| e.tiles).collect()
}

fn region(world: &World, x: usize, y: usize) -> u32 {
    world.exhibits().region(y * SIZE as usize + x).unwrap()
}

#[test]
fn no_fence_makes_no_exhibit() {
    let world = World::new(zoo(&[]), shape);
    assert!(world.exhibits().list().is_empty());
    assert_eq!(region(&world, 0, 0), GROUNDS);
    assert_eq!(world.exhibits().region(64), None, "off the map");
}

#[test]
fn a_closed_square_is_an_exhibit() {
    let world = World::new(zoo(&square(2, 3, 4, 4)), shape);
    assert_eq!(sizes(&world), [6]);
    assert_eq!(world.exhibits().list()[0].first, 3 * 8 + 2);
    assert_eq!(region(&world, 3, 4), 1);
    assert_eq!(region(&world, 5, 4), GROUNDS);
    assert_eq!(region(&world, 7, 7), GROUNDS, "the entrance");
    // One tile is an exhibit too.
    assert_eq!(sizes(&World::new(zoo(&square(0, 0, 0, 0)), shape)), [1]);
}

#[test]
fn a_square_with_a_gap_is_not() {
    let mut pieces = square(2, 3, 4, 4);
    pieces.retain(|&p| p != (3, 3, 0));
    assert!(sizes(&World::new(zoo(&pieces), shape)).is_empty());
    // A fence that does not close exhibits is no fence.
    let mut world = zoo(&[]);
    world.entities.extend(
        square(2, 3, 4, 4)
            .iter()
            .map(|&(x, y, d)| entity("plain", x, y, d)),
    );
    assert!(sizes(&World::new(world, shape)).is_empty());
}

#[test]
fn a_side_is_closed_by_the_fence_of_either_tile() {
    // The same line as the east side of tile (2, 3) and the west side of tile (3, 3).
    let mut pieces = square(1, 1, 2, 2);
    pieces.retain(|&p| p != (2, 1, 2) && p != (2, 2, 2));
    pieces.extend([(3, 1, 6), (3, 2, 6)]);
    assert_eq!(sizes(&World::new(zoo(&pieces), shape)), [4]);
}

#[test]
fn nested_squares_are_two_exhibits() {
    let mut pieces = square(1, 1, 6, 6);
    pieces.extend(square(3, 3, 4, 4));
    let world = World::new(zoo(&pieces), shape);
    assert_eq!(
        sizes(&world),
        [32, 4],
        "the ring has no tile of the inner square"
    );
    assert_eq!(region(&world, 1, 1), 1);
    assert_eq!(region(&world, 3, 3), 2);
    assert_eq!(region(&world, 0, 0), GROUNDS);
}

#[test]
fn the_map_edge_closes_a_region() {
    // A wall across the map: the west part leans on three sides of the edge.
    let pieces: Vec<_> = (0..SIZE).map(|y| (2, y, 2)).collect();
    let world = World::new(zoo(&pieces), shape);
    assert_eq!(sizes(&world), [24]);
    assert_eq!(region(&world, 0, 0), 1);
    assert_eq!(region(&world, 3, 0), GROUNDS);
}

#[test]
fn the_tiles_beyond_the_wall_are_the_exterior() {
    // The entrance gate stands on tile (4, 4) facing west, in a gap of the wall along the east
    // side of column 4: the exterior is the west part and the gate tile joins it to the grounds.
    let mut z = zoo(&(0..SIZE)
        .filter(|&y| y != 4)
        .map(|y| (4, y, 2))
        .collect::<Vec<_>>());
    z.entities[0] = entity("gate", 4, 4, 6);
    z.entrance = (4, 4);
    let world = World::new(z, shape);
    assert!(world.exhibits().list().is_empty());
    assert_eq!(region(&world, 0, 0), EXTERIOR);
    assert_eq!(region(&world, 3, 7), EXTERIOR);
    assert_eq!(region(&world, 4, 4), GROUNDS);
    assert_eq!(region(&world, 5, 0), GROUNDS);
}

#[test]
fn a_fence_placed_or_removed_builds_the_regions_again() {
    let mut pieces = square(2, 3, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = World::new(zoo(&pieces), shape);
    assert!(sizes(&world).is_empty());
    let id = world
        .add(
            entity("fences", last.0, last.1, last.2),
            shape(&entity("fences", 0, 0, 0)),
        )
        .unwrap();
    assert_eq!(sizes(&world), [6]);
    world.begin();
    assert!(world.remove(id));
    assert!(sizes(&world).is_empty());
    assert!(world.undo());
    assert_eq!(sizes(&world), [6], "taking back the removal");
}
