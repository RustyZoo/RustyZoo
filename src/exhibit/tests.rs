//! The regions on synthetic maps.

use super::*;
use crate::render::order::Kind;
use crate::world::{Event, Shape};
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
    // A piece with a subtype is an ordinary fence (`f`) or its gate (`g`); the others are walls.
    let wall = fence && e.subtype.is_empty();
    match e.class.as_str() {
        "fences" | "plain" => Some(Shape {
            kind: Kind::Edge,
            footprint: (1, 1),
            fence,
            wall,
            gate: false,
            gated: fence && !wall,
        }),
        "gate" => Some(Shape {
            kind: Kind::Fixed,
            footprint: (2, 2),
            fence: false,
            wall: false,
            gate: true,
            gated: false,
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
        habitat_tiles: Vec::new(),
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
fn stored_first_tiles_are_matched_one_to_one_with_the_exhibits() {
    let mut pieces = square(1, 1, 6, 6);
    pieces.extend(square(3, 3, 4, 4));
    let world = World::new(zoo(&pieces), shape);
    let ex = world.exhibits();
    assert_eq!(ex.distinct(&world, &[(1, 1), (3, 3)]), 2);
    assert_eq!(
        ex.distinct(&world, &[(1, 1), (2, 2)]),
        1,
        "the same exhibit twice"
    );
    assert_eq!(
        ex.distinct(&world, &[(0, 0), (-1, 0), (9, 9)]),
        0,
        "grounds, off the map"
    );
}

#[test]
fn an_entrance_without_a_building_has_no_exterior() {
    let mut z = zoo(&square(2, 2, 4, 4));
    z.entities.remove(0);
    let world = World::new(z, shape);
    assert_eq!(region(&world, 7, 7), GROUNDS);
    assert_eq!(sizes(&world), [9]);
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

// ---- Exhibits as fences are placed and removed

/// An ordinary fence piece that can have a gate.
fn piece(x: i32, y: i32, d: u32) -> Entity {
    let mut e = entity("fences", x, y, d);
    e.subtype = "f".into();
    e
}

/// The map of `zoo` with these pieces already on it, as if loaded.
fn loaded(pieces: &[(i32, i32, u32)]) -> World {
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.iter().map(|&(x, y, d)| piece(x, y, d)));
    unique(&mut z);
    World::new(z, shape)
}

/// Gives the entities of a zoo different ids, below its counter.
fn unique(z: &mut Zoo) {
    for (n, e) in z.entities.iter_mut().enumerate() {
        e.common.as_mut().unwrap().id = n as u32 + 1;
    }
}

fn put(world: &mut World, (x, y, d): (i32, i32, u32)) -> u32 {
    world.add(piece(x, y, d), shape(&piece(0, 0, 0))).unwrap()
}

/// The id of the piece on side `d` of tile (`x`, `y`).
fn at(world: &World, x: i32, y: i32, d: usize) -> u32 {
    let t = world.tile_index(x, y).unwrap();
    let e = world.edge(t, d).unwrap();
    world.zoo.entities[e].common.as_ref().unwrap().id
}

fn names(world: &World) -> Vec<(u32, String, usize)> {
    let list = world.exhibits().list();
    list.iter()
        .map(|e| (e.id, e.name.clone(), e.tiles))
        .collect()
}

/// The tile and side of the gate of exhibit `id`, as `(x, y, side)`.
fn gate(world: &World, id: u32) -> Option<(usize, usize, usize)> {
    let (t, d) = world.gate_slot(id)?;
    Some((t % SIZE as usize, t / SIZE as usize, d))
}

fn is_gate(world: &World, id: u32) -> bool {
    let at = world.find(id).unwrap();
    world.zoo.entities[at].subtype == "g"
}

#[test]
fn closing_a_square_makes_an_exhibit_with_a_gate_on_the_last_piece() {
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    assert!(world.take_events().is_empty());
    let id = put(&mut world, last);
    assert_eq!(
        world.take_events(),
        [Event::New { id: 1, ask: true }, Event::Gate(1)]
    );
    assert_eq!(names(&world), [(1, "Exhibit 1".to_string(), 9)]);
    assert_eq!(gate(&world, 1), Some((4, 4, 2)));
    assert!(is_gate(&world, id));
    // The counter goes on.
    for p in square(0, 6, 0, 6) {
        put(&mut world, p);
    }
    assert_eq!(names(&world)[1], (2, "Exhibit 2".to_string(), 1));
}

#[test]
fn a_piece_that_cuts_nothing_changes_nothing() {
    let mut world = loaded(&square(2, 2, 4, 4));
    world.take_events();
    put(&mut world, (3, 3, 2));
    put(&mut world, (6, 6, 0));
    assert!(world.take_events().is_empty());
    assert_eq!(world.exhibits().list().len(), 1);
}

#[test]
fn removing_a_piece_deletes_an_exhibit_open_to_the_grounds() {
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    let door = put(&mut world, last);
    world.take_events();
    // Another piece: the exhibit goes, and its gate is a fence again.
    let other = at(&world, 2, 2, 0);
    assert!(world.remove(other));
    assert_eq!(world.take_events(), [Event::Removed(1)]);
    assert!(world.exhibits().list().is_empty());
    assert!(!is_gate(&world, door));
    assert_eq!(region(&world, 3, 3), GROUNDS);
    // Closing it again is a new exhibit with the next number.
    put(&mut world, (2, 2, 0));
    assert_eq!(names(&world), [(2, "Exhibit 2".to_string(), 9)]);
}

#[test]
fn a_cut_exhibit_keeps_the_side_of_its_gate() {
    let mut pieces = square(1, 1, 6, 2);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    put(&mut world, last);
    world.take_events();
    assert_eq!(gate(&world, 1), Some((6, 2, 2)));
    put(&mut world, (3, 1, 2));
    assert!(world.take_events().is_empty());
    put(&mut world, (3, 2, 2));
    assert_eq!(
        world.take_events(),
        [
            Event::Split(1),
            Event::New { id: 2, ask: true },
            Event::Gate(2)
        ]
    );
    assert_eq!(
        names(&world),
        [(1, "Exhibit 1".into(), 6), (2, "Exhibit 2".into(), 6)]
    );
    assert_eq!(region(&world, 5, 1), 1);
    assert_eq!(region(&world, 1, 1), 2);
    assert_eq!(gate(&world, 1), Some((6, 2, 2)), "unchanged");
    // The new part opens to the grounds: the first good pair of the walk, on its south side.
    assert_eq!(gate(&world, 2), Some((3, 2, 4)));
}

#[test]
fn a_cut_exhibit_without_a_gate_keeps_the_side_of_the_piece() {
    let mut world = loaded(&square(1, 1, 6, 2));
    put(&mut world, (3, 1, 2));
    world.take_events();
    put(&mut world, (3, 2, 2));
    assert_eq!(
        world.take_events(),
        [
            Event::Split(1),
            Event::New { id: 2, ask: true },
            Event::Gate(2)
        ]
    );
    assert_eq!(region(&world, 1, 1), 1, "the side of the tile of the piece");
    assert_eq!(region(&world, 5, 1), 2);
    assert_eq!(names(&world)[0].1, "", "a loaded exhibit keeps its name");
    assert_eq!(gate(&world, 2), Some((4, 1, 0)));
}

/// Two exhibits side by side, x 1 to 2 and x 3 to `east`, with one piece for each line between
/// them (the east side of x 2): the pieces of both exhibits are loaded.
fn pair(east: i32) -> World {
    let mut pieces = square(1, 1, 2, 2);
    let mut y = square(3, 1, east, 2);
    y.retain(|&(x, _, d)| !(x == 3 && d == 6));
    pieces.extend(y);
    loaded(&pieces)
}

#[test]
fn two_merged_exhibits_leave_the_larger_and_on_equal_sizes_the_far_side() {
    let mut world = pair(4);
    assert_eq!(sizes(&world), [4, 4]);
    let gone = at(&world, 2, 2, 2);
    world.remove(gone);
    assert_eq!(world.take_events(), [Event::Merged { kept: 2, gone: 1 }]);
    assert_eq!(names(&world), [(2, String::new(), 8)]);
    // The first side is larger.
    let mut world = loaded(&{
        let mut p = square(1, 1, 3, 2);
        let mut y = square(4, 1, 5, 2);
        y.retain(|&(x, _, d)| !(x == 4 && d == 6));
        p.extend(y);
        p
    });
    let gone = at(&world, 3, 2, 2);
    world.remove(gone);
    assert_eq!(world.take_events(), [Event::Merged { kept: 1, gone: 2 }]);
}

#[test]
fn the_exhibit_whose_gate_was_removed_is_the_one_that_goes() {
    let mut world = pair(4);
    let line = at(&world, 2, 2, 2);
    world.set_gate(2, line);
    let kept = at(&world, 1, 1, 0);
    world.set_gate(1, kept);
    world.remove(line);
    assert_eq!(world.take_events(), [Event::Merged { kept: 1, gone: 2 }]);
    assert!(is_gate(&world, kept), "the survivor keeps its gate");
}

#[test]
fn an_exhibit_that_opens_into_the_other_goes() {
    // Y is the larger, but its gate leads into X: X stays, and Y's gate is a fence again.
    let mut world = pair(5);
    let lead = at(&world, 2, 1, 2);
    world.set_gate(2, lead);
    let line = at(&world, 2, 2, 2);
    world.remove(line);
    assert_eq!(world.take_events(), [Event::Merged { kept: 1, gone: 2 }]);
    assert!(!is_gate(&world, lead));
}

#[test]
fn a_gate_removed_from_a_line_that_stays_closed_moves_to_the_other_piece() {
    let mut pieces = square(1, 1, 2, 2);
    pieces.push((3, 1, 6));
    let mut world = loaded(&pieces);
    let door = at(&world, 2, 1, 2);
    world.set_gate(1, door);
    world.remove(door);
    assert_eq!(world.take_events(), [Event::Gate(1)]);
    assert_eq!(gate(&world, 1), Some((3, 1, 6)));
    assert_eq!(names(&world).len(), 1);
}

#[test]
fn a_replaced_piece_keeps_the_exhibit_and_its_gate() {
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    let door = put(&mut world, last);
    world.take_events();
    let plain = at(&world, 2, 2, 0);
    let new = world
        .replace(plain, piece(2, 2, 0), shape(&piece(0, 0, 0)))
        .unwrap();
    let new_gate = world
        .replace(door, piece(4, 4, 2), shape(&piece(0, 0, 0)))
        .unwrap();
    assert!(world.take_events().is_empty());
    assert_eq!(names(&world), [(1, "Exhibit 1".into(), 9)]);
    assert!(!is_gate(&world, new));
    assert!(is_gate(&world, new_gate));
    assert_eq!(world.exhibits().get(1).unwrap().gate, Some(new_gate));
}

#[test]
fn a_gate_nobody_can_walk_to_tells_the_exhibit_is_inaccessible() {
    // An exhibit inside another one that has no gate: no keeper can reach it.
    let mut world = loaded(&square(0, 0, 5, 5));
    for p in square(2, 2, 3, 3) {
        put(&mut world, p);
    }
    let events = world.take_events();
    assert!(
        matches!(
            events[..],
            [
                Event::Split(1),
                Event::New { id: 2, .. },
                Event::Gate(2),
                Event::Blocked {
                    id: 2,
                    message: 11000
                }
            ]
        ),
        "{events:?}"
    );
    assert!(gate(&world, 2).is_some());
}

#[test]
fn a_gate_blocked_by_objects_tells_the_entrance_is_blocked() {
    let mut z = zoo(&[]);
    z.entities
        .extend(square(0, 0, 5, 5).iter().map(|&(x, y, d)| piece(x, y, d)));
    z.entities.push(entity("gate", 2, 2, 0));
    unique(&mut z);
    let mut world = World::new(z, shape);
    world.take_events();
    for p in square(2, 2, 2, 2) {
        put(&mut world, p);
    }
    let events = world.take_events();
    assert!(
        matches!(
            events[..],
            [
                Event::Split(1),
                _,
                Event::Gate(2),
                Event::Blocked { message: 11001, .. }
            ]
        ),
        "{events:?}"
    );
}

#[test]
fn the_walk_follows_the_map_edge_round_its_corners() {
    // A wall across the map leaves the west part against three edges; the tile beside its last
    // piece slopes, so the gate goes on the first good pair after the walk turned two corners.
    let mut pieces: Vec<_> = (0..SIZE - 1).map(|y| (2, y, 2)).collect();
    let last = (2, SIZE - 1, 2);
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.drain(..).map(|(x, y, d)| piece(x, y, d)));
    z.tiles[(SIZE * (SIZE - 1) + 3) as usize].shape = 0x55;
    unique(&mut z);
    let mut world = World::new(z, shape);
    world.take_events();
    put(&mut world, last);
    assert_eq!(
        world.take_events()[..2],
        [Event::New { id: 1, ask: true }, Event::Gate(1)]
    );
    assert_eq!(gate(&world, 1), Some((2, 0, 2)));
}

#[test]
fn an_undo_makes_no_gate_asks_no_name_and_keeps_the_counter() {
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    world.begin();
    let door = put(&mut world, last);
    world.take_events();
    let other = at(&world, 2, 2, 0);
    world.begin();
    world.remove(other);
    world.take_events();
    assert!(world.undo());
    assert_eq!(world.take_events(), [Event::New { id: 2, ask: false }]);
    let e = world.exhibits().get(2).unwrap();
    assert_eq!((e.gate, e.tiles), (None, 9), "no gate: it is restored");
    assert!(!is_gate(&world, door));
    put(&mut world, (6, 6, 0));
    for p in square(0, 6, 0, 6) {
        put(&mut world, p);
    }
    assert_eq!(world.exhibits().get(3).unwrap().name, "Exhibit 2");
}

#[test]
fn a_gate_is_not_put_where_the_keeper_would_have_to_swim() {
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.iter().map(|&(x, y, d)| piece(x, y, d)));
    unique(&mut z);
    // The lake lies against the east side of the last piece: terrain type 1 is water.
    z.tiles[4 * SIZE as usize + 5].terrain = 1;
    let mut world = World::new(z, shape);
    world.set_water(vec![false, true]);
    world.take_events();
    put(&mut world, last);
    let (x, y, d) = gate(&world, 1).unwrap();
    assert_ne!((x, y, d), (4, 4, 2), "not on the lake");
    assert_eq!(
        world.take_events().len(),
        2,
        "a good pair was found: no message"
    );
}
