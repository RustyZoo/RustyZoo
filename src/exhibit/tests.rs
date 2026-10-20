//! The regions on synthetic maps.

use super::*;
use crate::render::order::Kind;
use crate::world::{Event, Shape, TankKind};
use crate::zoo::{Common, Entity, HabitatInfo, Tile, View, Zoo};

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
            ..Default::default()
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let tank = matches!(e.class.as_str(), "tank" | "show");
    let fence = e.class == "fences" || tank;
    // A piece with a subtype is an ordinary fence (`f`) or its gate (`g`); the others are walls.
    let wall = e.class == "fences" && e.subtype.is_empty();
    match e.class.as_str() {
        "fences" | "plain" | "tank" | "show" => Some(Shape {
            kind: Kind::Edge,
            footprint: (1, 1),
            fence,
            wall,
            gate: false,
            gated: fence && !wall,
            walkable: (false, false),
            tank: TankKind {
                wall: tank,
                show: e.class == "show",
                underwater: tank,
                shows: e.class == "show",
                life: 10,
                strength: 500,
                ..TankKind::NONE
            },
        }),
        "gate" => Some(Shape {
            kind: Kind::Fixed,
            footprint: (2, 2),
            fence: false,
            wall: false,
            gate: true,
            gated: false,
            walkable: (true, true),
            tank: TankKind::NONE,
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
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; (SIZE * SIZE) as usize],
        entities,
        view: View::default(),
        clocks: Default::default(),
        names: Default::default(),
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
    assert!(!world.undo().is_empty());
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

/// Places a piece; a new exhibit is named at once (default name), which gives it its gate.
fn put(world: &mut World, (x, y, d): (i32, i32, u32)) -> u32 {
    let id = place(world, (x, y, d));
    for new in world.waiting() {
        world.give_gate(new);
    }
    id
}

/// Places a piece; a new exhibit waits for its name and has no gate yet.
fn place(world: &mut World, (x, y, d): (i32, i32, u32)) -> u32 {
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
        .swap(plain, piece(2, 2, 0), shape(&piece(0, 0, 0)))
        .unwrap();
    let new_gate = world
        .swap(door, piece(4, 4, 2), shape(&piece(0, 0, 0)))
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
    assert!(!world.undo().is_empty());
    // The exhibit is made again quietly; the recorded name and gate are given back after it.
    assert_eq!(
        world.take_events(),
        [Event::New { id: 2, ask: false }, Event::Gate(2)]
    );
    let e = world.exhibits().get(2).unwrap();
    assert_eq!((e.gate, e.tiles), (Some(door), 9), "its gate is restored");
    assert_eq!(e.name, "Exhibit 1");
    assert!(is_gate(&world, door));
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

#[test]
fn a_gate_removed_from_the_grounds_slot_of_a_closed_line_moves_to_the_other_piece() {
    let mut pieces = square(1, 1, 2, 2);
    pieces.push((3, 1, 6));
    let mut world = loaded(&pieces);
    let door = at(&world, 3, 1, 6);
    world.set_gate(1, door);
    world.remove(door);
    assert_eq!(world.take_events(), [Event::Gate(1)]);
    assert_eq!(gate(&world, 1), Some((2, 1, 2)));
}

#[test]
fn two_pieces_on_a_wet_line_are_code_1_so_the_start_pair_is_blocked_not_inaccessible() {
    let mut pieces = square(3, 3, 3, 3);
    let last = pieces.pop().unwrap();
    let mut z = zoo(&[]);
    z.entities
        .extend(pieces.iter().map(|&(x, y, d)| piece(x, y, d)));
    z.entities.push(entity("plain", 4, 3, 6));
    unique(&mut z);
    z.tiles[3 * SIZE as usize + 3].terrain = 1;
    let mut world = World::new(z, shape);
    world.set_water(vec![false, true]);
    world.take_events();
    put(&mut world, last);
    let events = world.take_events();
    assert!(
        events.contains(&Event::Blocked {
            id: 1,
            message: 11001
        }),
        "{events:?}"
    );
}

#[test]
fn a_new_exhibit_has_no_gate_until_it_is_named() {
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    let id = place(&mut world, last);
    assert_eq!(world.take_events(), [Event::New { id: 1, ask: true }]);
    assert_eq!((world.waiting(), gate(&world, 1)), (vec![1], None));
    assert!(!is_gate(&world, id));
    world.give_gate(1);
    assert_eq!(world.take_events(), [Event::Gate(1)]);
    assert_eq!(
        (world.waiting(), gate(&world, 1)),
        (vec![], Some((4, 4, 2)))
    );
}

/// The exhibit the piece on side `d` of tile (`x`, `y`) could be the gate of.
fn gate_for(world: &World, x: i32, y: i32, d: usize) -> Option<u32> {
    let t = world.tile_index(x, y).unwrap();
    world.gate_for(world.edge(t, d).unwrap())
}

#[test]
fn a_gate_goes_on_a_piece_between_an_exhibit_and_the_grounds_with_the_slot_across_empty() {
    // A square with its gate on the last piece (4, 4, side 2) and two pieces of no exhibit.
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    pieces.extend([(3, 3, 2), (6, 6, 0), (0, 0, 0)]);
    let mut world = loaded(&pieces);
    put(&mut world, last);
    world.take_events();
    assert_eq!(gate(&world, 1), Some((4, 4, 2)));
    // 1: a piece on the outer side of the line is valid, the gate itself is not (3), a piece with
    // another one on the slot across is not, and neither is one on the edge of the map.
    assert_eq!(gate_for(&world, 2, 2, 0), Some(1));
    assert_eq!(gate_for(&world, 4, 4, 2), None, "already the gate");
    assert_eq!(gate_for(&world, 0, 0, 0), None, "off the map across");
    put(&mut world, (2, 1, 4));
    assert_eq!(gate_for(&world, 2, 2, 0), None, "the slot across is held");
    assert_eq!(gate_for(&world, 2, 1, 4), None, "from the other side too");
    // 2: both tiles in the exhibit, or both on the grounds.
    assert_eq!(gate_for(&world, 3, 3, 2), None, "inside the exhibit");
    assert_eq!(gate_for(&world, 6, 6, 0), None, "no exhibit");
    // 2: between two exhibits, on a line only one piece holds.
    let both = loaded(&[
        (0, 1, 0),
        (1, 1, 0),
        (0, 1, 4),
        (1, 1, 4),
        (0, 1, 6),
        (1, 1, 2),
        (2, 1, 0),
        (2, 1, 4),
        (2, 1, 2),
    ]);
    assert_eq!(both.exhibits().list().len(), 2);
    assert_eq!(gate_for(&both, 1, 1, 2), None, "between two exhibits");
    // A zoo wall piece never takes a gate.
    let mut z = zoo(&[]);
    z.entities.push(entity("fences", 1, 6, 0));
    unique(&mut z);
    assert_eq!(gate_for(&World::new(z, shape), 1, 6, 0), None, "a wall");
}

#[test]
fn moving_the_gate_turns_the_old_one_back_clears_the_undo_list_and_says_so() {
    let mut world = loaded(&square(2, 2, 4, 4));
    world.take_events();
    let (old, new) = (at(&world, 2, 2, 0), at(&world, 3, 2, 0));
    world.set_gate(1, old);
    assert!(is_gate(&world, old));
    world.begin();
    place(&mut world, (6, 6, 0));
    assert!(world.move_gate(1, new, 7, 300));
    assert_eq!(gate(&world, 1), Some((3, 2, 0)));
    assert!(is_gate(&world, new) && !is_gate(&world, old));
    assert_eq!(world.take_events(), [Event::Gate(1)]);
    let at_new = world.find(new).unwrap();
    let fence = world.zoo.entities[at_new].common.as_ref().unwrap().fence;
    assert_eq!(fence, None, "a piece with no wear data keeps none");
    assert!(world.undo().is_empty(), "the undo list was cleared");
    // Unknown exhibit or piece: nothing changes.
    assert!(!world.move_gate(9, new, 7, 300));
    assert!(!world.move_gate(1, 999, 7, 300));
    assert_eq!(gate(&world, 1), Some((3, 2, 0)));
}

fn passes(world: &World, x: i32, y: i32, d: usize) -> bool {
    let t = world.tile_index(x, y).unwrap();
    world.keeper_passes(world.edge(t, d).unwrap())
}

#[test]
fn a_gate_where_the_keeper_cannot_pass_is_still_valid_but_not_clear() {
    let mut z = zoo(&[]);
    z.entities
        .extend(square(2, 2, 4, 4).iter().map(|&(x, y, d)| piece(x, y, d)));
    unique(&mut z);
    z.tiles[SIZE as usize + 2].terrain = 1;
    z.tiles[2 * SIZE as usize + 5].shape = 0x0f;
    let mut world = World::new(z, shape);
    world.set_water(vec![false, true]);
    // Plain grass on both sides: clear.
    assert_eq!(gate_for(&world, 3, 2, 0), Some(1));
    assert!(passes(&world, 3, 2, 0));
    // Water on the tile across (code 4), and a tile of no path shape (cost 16): not clear.
    assert_eq!(gate_for(&world, 2, 2, 0), Some(1));
    assert!(!passes(&world, 2, 2, 0));
    assert_eq!(gate_for(&world, 4, 2, 2), Some(1));
    assert!(!passes(&world, 4, 2, 2));
}

fn stored(name: &str, stats: [u32; 9], created: u64) -> HabitatInfo {
    HabitatInfo {
        name: name.into(),
        stats,
        created,
        serviced: 0,
    }
}

#[test]
fn a_loaded_exhibit_keeps_its_stored_name_statistics_date_and_service_time() {
    let load = |locale: u32, created: u64| {
        let mut z = zoo(&[]);
        z.locale = locale;
        z.entities
            .extend(square(2, 2, 4, 4).iter().map(|&(x, y, d)| piece(x, y, d)));
        z.habitat_tiles = vec![(3, 3)];
        z.habitat_gates = vec![((-1, -1), -1)];
        // Donations then upkeep, each this month, last month, total.
        let mut stats = [0; 9];
        for (k, v) in [12.5f32, 0.0, 12.5, 185.49, 1.0, 186.49].iter().enumerate() {
            stats[k] = v.to_bits();
        }
        z.habitat_info = vec![HabitatInfo {
            serviced: 4321,
            ..stored("Lions", stats, created)
        }];
        unique(&mut z);
        World::new(z, shape)
    };
    // 17 January 2001, and the "never" default 1 January 2000.
    let day = 864_000_000_000u64;
    let world = load(1033, (134_774 + 11_323 + 16) * day);
    let e = world.exhibits().get(1).unwrap();
    assert_eq!(e.name, "Lions");
    assert_eq!(e.rating.serviced, 4321, "the time last serviced");
    assert_eq!(e.stats.donations, [0.0, 12.5, 12.5]);
    assert_eq!(e.stats.upkeep, [1.0, 185.49, 186.49]);
    let d = e.stats.created.unwrap();
    assert_eq!((d.year, d.month, d.day, d.weekday), (2001, 1, 17, 2));
    let d = load(1033, 125_911_584_000_000_000)
        .exhibits()
        .get(1)
        .unwrap()
        .stats
        .created;
    assert_eq!(d.map(|d| (d.year, d.month, d.day)), Some((2000, 1, 1)));
    // A file of another language keeps the default name, the rest loads the same.
    let world = load(1031, 0);
    let e = world.exhibits().get(1).unwrap();
    assert_eq!(e.name, "");
    assert_eq!(e.stats.donations, [0.0, 12.5, 12.5]);
    assert_eq!(e.stats.created.map(|d| d.year), Some(1601));
}

#[test]
fn the_gate_of_a_loaded_exhibit_is_the_piece_its_record_names() {
    let load = |tile: (i32, i32), side: i32| {
        let mut z = zoo(&[]);
        z.entities
            .extend(square(2, 2, 4, 4).iter().map(|&(x, y, d)| piece(x, y, d)));
        z.habitat_tiles = vec![(3, 3)];
        z.habitat_gates = vec![(tile, side)];
        z.habitat_info = vec![stored("Lions", [0; 9], 0)];
        unique(&mut z);
        World::new(z, shape)
    };
    let world = load((3, 2), 0);
    assert_eq!(gate(&world, 1), Some((3, 2, 0)));
    // Moving the gate sends the stored one back to a fence.
    let mut world = world;
    let (old, new) = (at(&world, 3, 2, 0), at(&world, 2, 2, 0));
    assert!(world.move_gate(1, new, 10, 500));
    assert_eq!(world.exhibits().get(1).unwrap().gate, Some(new));
    assert_ne!(old, new);
    // A record with no gate (x negative) gets the gate walk from the line the file closes last.
    assert_eq!(gate(&load((-1, -1), -1), 1), Some((4, 4, 2)));
    // A side that is not one, a tile off the map and an empty slot leave no gate.
    for (tile, side) in [((3, 2), 3), ((3, 2), 8), ((3, 2), -2)] {
        assert_eq!(load(tile, side).exhibits().get(1).unwrap().gate, None);
    }
    for (tile, side) in [((80, 2), 0), ((3, 3), 0), ((i32::MAX, i32::MIN), 2)] {
        assert_eq!(load(tile, side).exhibits().get(1).unwrap().gate, None);
    }
}

mod filter;
mod rating;
mod tank;
mod viewing;
mod water;

/// The name and the gate piece of the exhibit on tile (`x`, `y`).
fn exhibit_at(world: &World, x: usize, y: usize) -> (String, Option<u32>) {
    let e = world.exhibits().get(region(world, x, y)).unwrap();
    (e.name.clone(), e.gate)
}

#[test]
fn undoing_a_merge_gives_both_exhibits_their_names_and_gates_back() {
    let mut world = pair(4);
    let line = at(&world, 2, 2, 2);
    let (west, east) = (at(&world, 1, 1, 0), at(&world, 4, 1, 0));
    world.rename(1, "Lions".into());
    world.rename(2, "Tigers".into());
    world.set_gate(1, west);
    world.set_gate(2, east);
    world.begin();
    world.remove(line);
    assert_eq!(world.exhibits().list().len(), 1);
    assert!(
        !is_gate(&world, west),
        "the exhibit that goes loses its gate"
    );
    assert!(!world.undo().is_empty());
    assert_eq!(world.exhibits().list().len(), 2);
    assert_eq!(exhibit_at(&world, 1, 1), ("Lions".into(), Some(west)));
    assert_eq!(exhibit_at(&world, 4, 1), ("Tigers".into(), Some(east)));
    assert!(is_gate(&world, west) && is_gate(&world, east));
    assert!(!is_gate(&world, line), "the piece put back is a fence");
}

#[test]
fn undoing_a_piece_that_cut_an_exhibit_gives_it_its_name_and_gate_back() {
    let mut pieces = square(1, 1, 6, 2);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    put(&mut world, last);
    world.rename(1, "Big".into());
    put(&mut world, (3, 1, 2));
    let gate_before = gate(&world, 1);
    assert_eq!(gate_before, Some((6, 2, 2)));
    world.begin();
    put(&mut world, (3, 2, 2));
    assert_eq!(world.exhibits().list().len(), 2);
    assert!(!world.undo().is_empty());
    let (name, gate_id) = exhibit_at(&world, 5, 1);
    assert_eq!(name, "Big");
    assert_eq!(world.exhibits().list().len(), 1);
    assert_eq!(gate_id, Some(at(&world, 6, 2, 2)));
    assert_eq!(region(&world, 1, 1), region(&world, 5, 1));
    // The exhibit that was cut off is gone with its gate: its piece is a fence again.
    assert!(!is_gate(&world, at(&world, 3, 2, 4)));
}

// ---- The order of the tiles and of the boundary (exhibits and money 3.1)

/// The pieces that close the tiles `tiles`: one on each side whose neighbour is not in the set.
fn outline(tiles: &[(i32, i32)]) -> Vec<(i32, i32, u32)> {
    let mut pieces = Vec::new();
    for &(x, y) in tiles {
        for (d, (dx, dy)) in [(0, (0, -1)), (2, (1, 0)), (4, (0, 1)), (6, (-1, 0))] {
            if !tiles.contains(&(x + dx, y + dy)) {
                pieces.push((x, y, d));
            }
        }
    }
    pieces
}

#[test]
fn an_l_shaped_exhibit_lists_its_tiles_and_boundary_in_the_original_order() {
    // Tiles (2,2), (2,3), (2,4) and (3,4), the file closing its line on (2,2) last: filled from
    // there, put in that order, listed in reverse; the boundary is read from the list, sides
    // 0, 2, 4, 6.
    let world = loaded(&outline(&[(3, 4), (2, 4), (2, 3), (2, 2)]));
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let list = world.exhibits().get(1).unwrap();
    assert_eq!(list.order, [t(3, 4), t(2, 4), t(2, 3), t(2, 2)]);
    let side = |a: (usize, usize), b: (usize, usize)| (t(a.0, a.1), Some(t(b.0, b.1)));
    assert_eq!(
        world.exhibits().boundary(&world, 1),
        [
            side((3, 4), (3, 3)),
            side((3, 4), (4, 4)),
            side((3, 4), (3, 5)),
            side((2, 4), (2, 5)),
            side((2, 4), (1, 4)),
            side((2, 3), (3, 3)),
            side((2, 3), (1, 3)),
            side((2, 2), (2, 1)),
            side((2, 2), (3, 2)),
            side((2, 2), (1, 2)),
        ]
    );
    assert_eq!(world.outside(1), Some(t(3, 3)));
}

#[test]
fn a_new_exhibit_is_filled_from_the_tile_beside_the_piece() {
    // The square (2,2)-(3,3) closed by its last piece on tile (3,3): filled from there, not from
    // its first tile, so its first boundary pair is on the top row.
    let mut pieces = square(2, 2, 3, 3);
    let last = pieces.iter().position(|&p| p == (3, 3, 2)).unwrap();
    pieces.remove(last);
    let mut world = loaded(&pieces);
    place(&mut world, (3, 3, 2));
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let order = &world.exhibits().get(1).unwrap().order;
    assert_eq!(order, &[t(2, 2), t(3, 2), t(2, 3), t(3, 3)]);
    assert_eq!(world.outside(1), Some(t(2, 1)));
}

#[test]
fn a_loaded_exhibit_is_filled_from_the_line_the_file_closes_last() {
    // The same L with its last piece on (3,4): the fill starts there and the list is the other way.
    let world = loaded(&outline(&[(2, 2), (2, 3), (2, 4), (3, 4)]));
    let t = |x: usize, y: usize| y * SIZE as usize + x;
    let list = &world.exhibits().get(1).unwrap().order;
    assert_eq!(list, &[t(2, 2), t(2, 3), t(2, 4), t(3, 4)]);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn a_loaded_exhibit_lists_its_stored_tile_first() {
    use crate::entity::Catalogue;
    use crate::render::MapRenderer;
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let renderer = MapRenderer::new(&vfs, &catalogue);
    let (mut stored, mut first) = (0, 0);
    for map in vfs.list(None) {
        if !map.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        let Ok(zoo) = Zoo::parse(&vfs.read(&map).unwrap()).and_then(|z| z.checked(&catalogue))
        else {
            continue;
        };
        let tiles = zoo.habitat_tiles.clone();
        let world = World::new(zoo, |e| renderer.shape(e));
        for (x, y) in tiles {
            let t = world.tile_index(x, y).unwrap();
            let e = world
                .exhibits()
                .get(world.exhibits().region(t).unwrap())
                .unwrap();
            stored += 1;
            first += usize::from(e.order.first() == Some(&t));
        }
    }
    assert!(stored > 0);
    assert_eq!(first, stored);
}

#[test]
fn the_gate_choice_searches_from_the_entrance_with_the_node_cap() {
    // The pair at the last piece is reached from the entrance (7, 7) by a search of the finder:
    // with eight directions it needs 13 nodes, with four directions 16 (pathfinding 8).
    let blocked = |diagonal, node_max| {
        let mut pieces = square(2, 2, 4, 4);
        let last = pieces.pop().unwrap();
        let mut world = loaded(&pieces);
        let finder = crate::path::Settings {
            diagonal,
            node_max,
            ..Default::default()
        };
        world.set_finder(finder);
        put(&mut world, last);
        world.take_events().contains(&Event::Blocked {
            id: 1,
            message: 11000,
        })
    };
    assert!(!blocked(true, 500) && !blocked(false, 500));
    assert!(!blocked(true, 13) && blocked(true, 12));
    assert!(!blocked(false, 16) && blocked(false, 15));
}

#[test]
fn the_gate_choice_searches_with_the_switch_the_last_request_left() {
    // After a guest's request the finder searches in four directions: 16 nodes, not 13.
    let mut pieces = square(2, 2, 4, 4);
    let last = pieces.pop().unwrap();
    let mut world = loaded(&pieces);
    world.set_finder(crate::path::Settings {
        node_max: 13,
        ..Default::default()
    });
    let mut finder = std::mem::take(&mut world.units.finder);
    let guest = crate::path::Mover::new(crate::path::Kind::Guest, Some(0));
    finder.request(&world, &guest, 1, false);
    world.units.finder = finder;
    put(&mut world, last);
    assert!(world.take_events().contains(&Event::Blocked {
        id: 1,
        message: 11000,
    }));
}
