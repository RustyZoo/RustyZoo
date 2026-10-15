//! The world on synthetic maps.

use super::*;
use crate::zoo::{Common, View};

fn zoo(width: u32, height: u32, entrance: (u32, u32)) -> Zoo {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width,
        height,
        entrance,
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; (width * height) as usize],
        entities: Vec::new(),
        view: View::default(),
        clocks: Default::default(),
    }
}

fn entity(class: &str, x: i32, y: i32, rotation: u32) -> Entity {
    Entity {
        class: class.to_string(),
        type_name: String::new(),
        subtype: String::new(),
        body_size: None,
        common: Some(Common {
            x,
            y,
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

const FENCE: Shape = Shape {
    kind: Kind::Edge,
    footprint: (1, 1),
    fence: true,
    wall: true,
    gate: false,
    gated: false,
    tank: TankKind {
        wall: false,
        show: false,
        ..TankKind::NONE
    },
};

const NOTHING: Shape = Shape {
    kind: Kind::Fixed,
    footprint: (1, 1),
    fence: false,
    wall: false,
    gate: false,
    gated: false,
    tank: TankKind {
        wall: false,
        show: false,
        ..TankKind::NONE
    },
};

const GATE: Shape = Shape {
    kind: Kind::Fixed,
    footprint: (2, 2),
    fence: false,
    wall: false,
    gate: true,
    gated: false,
    tank: TankKind {
        wall: false,
        show: false,
        ..TankKind::NONE
    },
};

/// The shape of the entities of these tests, by class.
fn shape(e: &Entity) -> Option<Shape> {
    match e.class.as_str() {
        "fences" => Some(FENCE),
        "gate" => Some(GATE),
        // An exhibit fence that is not the zoo wall.
        "plain" => Some(Shape {
            wall: false,
            ..FENCE
        }),
        "path" => Some(Shape {
            kind: Kind::Path,
            footprint: (2, 2),
            ..NOTHING
        }),
        "tent" => Some(Shape {
            kind: Kind::Fixed,
            footprint: (4, 2),
            ..NOTHING
        }),
        _ => None,
    }
}

/// An 8 by 8 map whose entrance gate stands on tile (1, 4) facing west, with a zoo wall on the
/// east edge of the tiles of column 4, except in the rows `open`.
fn walled(open: &[i32]) -> World {
    let mut zoo = zoo(8, 8, (1, 4));
    zoo.entities.push(entity("gate", 96, 288, 6));
    for row in (0..8).filter(|r| !open.contains(r)) {
        zoo.entities
            .push(entity("fences", 4 * 64 + 63, row * 64 + 32, 2));
    }
    World::new(zoo, shape)
}

fn zoo_tiles(world: &World) -> Vec<(i32, i32)> {
    (0..64)
        .filter(|&t| world.in_zoo(t))
        .map(|t| ((t % 8) as i32, (t / 8) as i32))
        .collect()
}

#[test]
fn the_fill_from_beyond_the_entrance_stops_at_the_wall() {
    let world = walled(&[]);
    // The gate holds tile (1, 4); the fill starts west of it, on tile (0, 4), goes round the gate
    // (it does not cross its tiles) and fills columns 0 to 4, not the tiles behind the wall.
    let inside = zoo_tiles(&world);
    assert_eq!(inside.len(), 25);
    assert!(
        inside.iter().all(|&(x, y)| x >= 5 || (x, y) == (1, 4)),
        "{inside:?}"
    );
    assert!(!world.in_zoo(100), "off the map");
}

#[test]
fn an_opening_in_the_wall_lets_the_fill_through() {
    let world = walled(&[2]);
    assert_eq!(zoo_tiles(&world), [(1, 4)], "only the tile of the gate");
    // Only the zoo wall stops it: a fence of an exhibit does not.
    let mut zoo = zoo(4, 1, (0, 0));
    zoo.entities.push(entity("gate", 32, 32, 2));
    zoo.entities.push(entity("plain", 64 + 63, 32, 2));
    assert_eq!(zoo_tiles(&World::new(zoo, shape)), [(0, 0)]);
}

#[test]
fn no_fill_enters_a_gate_and_a_far_gate_stops_it_too() {
    // The entrance gate faces east out of tile (0, 0): the fill starts on (1, 0). Another gate
    // on tile (2, 0) is not entered, so the tiles behind it stay in the zoo.
    let mut zoo = zoo(4, 1, (0, 0));
    zoo.entities.push(entity("gate", 32, 32, 2));
    zoo.entities.push(entity("gate", 2 * 64 + 32, 32, 2));
    assert_eq!(zoo_tiles(&World::new(zoo, shape)), [(0, 0), (2, 0), (3, 0)]);
}

#[test]
fn the_fill_needs_an_entrance_building_on_the_map() {
    let world = World::new(zoo(4, 4, (9, 9)), shape);
    assert_eq!(zoo_tiles(&world).len(), 16);
    let world = World::new(zoo(4, 4, (u32::MAX, 0)), shape);
    assert_eq!(zoo_tiles(&world).len(), 16);
    // Nothing holds the quadrant of the entrance tile: nothing is marked.
    let world = World::new(zoo(4, 4, (1, 1)), shape);
    assert_eq!(zoo_tiles(&world).len(), 16);
    // A building that faces off the map: nothing is marked either.
    let mut zoo = zoo(4, 4, (0, 1));
    zoo.entities.push(entity("gate", 32, 64 + 32, 6));
    assert_eq!(zoo_tiles(&World::new(zoo, shape)).len(), 16);
}

#[test]
fn an_added_entity_takes_a_fresh_id_and_shows_in_the_index() {
    let mut world = World::new(zoo(8, 8, (0, 0)), shape);
    let fence = world.add(entity("fences", 2 * 64 + 32, 3 * 64 + 63, 4), Some(FENCE));
    assert_eq!(fence, Some(101));
    let t = world.tile_index(2, 3).unwrap();
    let at = world.find(101).unwrap();
    assert_eq!(world.zoo.entities[at].common.as_ref().unwrap().id, 101);
    assert_eq!(world.edge(t, 4), Some(at));
    assert_eq!(world.edge(t, 0), None);
    // The same line is the slot of the tile across.
    assert_eq!(world.edge(world.tile_index(2, 4).unwrap(), 0), None);
    // A path holds the path of its tile; a tent of 4 by 2 half tiles holds the four quadrants
    // of each of two tiles side by side.
    world.add(entity("path", 32, 32, 0), shape(&entity("path", 0, 0, 0)));
    world.add(
        entity("tent", 6 * 64, 6 * 64 + 32, 0),
        shape(&entity("tent", 0, 0, 0)),
    );
    assert_eq!(world.path(0), world.find(102));
    let tent = world.find(103);
    let (left, right) = (
        world.tile_index(5, 6).unwrap(),
        world.tile_index(6, 6).unwrap(),
    );
    assert_eq!(world.quadrant(left, 1), tent);
    assert_eq!(world.quadrant(left, 3), tent);
    assert_eq!(world.quadrant(right, 2), tent);
    assert_eq!(world.quadrant(world.tile_index(7, 6).unwrap(), 0), None);
    assert!(world.take_dirty() && !world.take_dirty());
    // Removing frees the slots; an id that is not there removes nothing.
    assert!(world.remove(103));
    assert!(!world.remove(103));
    assert_eq!(world.quadrant(left, 1), None);
    assert!(world.take_dirty());
    assert_eq!(world.zoo.id_counter, 103, "an id is not handed out twice");
}

#[test]
fn an_entity_without_a_body_is_not_added_and_one_without_a_shape_is_not_indexed() {
    let mut world = World::new(zoo(2, 2, (0, 0)), shape);
    let mut bodyless = entity("fences", 0, 0, 0);
    bodyless.common = None;
    assert_eq!(world.add(bodyless, Some(FENCE)), None);
    assert!(!world.take_dirty());
    assert_eq!(world.add(entity("odd", 32, 32, 0), None), Some(101));
    assert!((0..4).all(|q| world.quadrant(0, q).is_none()) && world.path(0).is_none());
}

#[test]
fn undo_takes_back_the_last_press_and_a_new_press_forgets_the_old_one() {
    let mut world = walled(&[]);
    let before = world.zoo.clone();
    world.begin();
    let a = world.add(entity("path", 32, 32, 0), shape(&entity("path", 0, 0, 0)));
    let first = world.zoo.entities[0].common.as_ref().unwrap().id;
    assert!(world.remove(first));
    assert!(world.set_tile(2, 2, 3, 0x05, 4));
    assert_eq!(a, Some(101));
    assert!(!world.undo().is_empty());
    // The id counter is not given back, everything else is.
    assert_eq!(world.zoo.entities, before.entities);
    assert_eq!(world.zoo.tiles, before.tiles);
    assert_eq!(world.edge(world.tile_index(4, 0).unwrap(), 2), Some(1));
    assert!(world.undo().is_empty(), "once");
    // The first operation after a new press empties the list; a press alone keeps it.
    world.add(entity("path", 32, 32, 0), shape(&entity("path", 0, 0, 0)));
    world.begin();
    world.begin();
    world.set_tile(0, 0, 1, 0, 0);
    assert!(!world.undo().is_empty());
    assert_eq!(world.zoo.tiles[0].elevation, 0);
    assert_eq!(
        world.path(0),
        world.find(102),
        "the path of the old press stays"
    );
}

/// An entity of a type, for the tests of replacing.
fn typed(class: &str, name: &str, x: i32, y: i32, rotation: u32) -> Entity {
    let mut e = entity(class, x, y, rotation);
    e.type_name = name.into();
    e
}

#[test]
fn an_undo_hands_back_the_prices_the_actions_moved_last_first() {
    let mut world = walled(&[]);
    world.begin();
    let path = world.add(entity("path", 32, 32, 0), shape(&entity("path", 0, 0, 0)));
    world.priced(10.0);
    let tent = world.add(
        entity("tent", 6 * 64, 6 * 64 + 32, 0),
        shape(&entity("tent", 0, 0, 0)),
    );
    world.priced(40.0);
    assert!(world.remove(tent.unwrap()));
    world.priced(32.0);
    assert!(world.set_tile(2, 2, 3, 0x05, 4));
    world.priced(6.0);
    assert!(world.can_undo());
    let done = world.undo();
    assert_eq!(done.blocked, None);
    let steps: Vec<String> = done
        .steps
        .iter()
        .map(|s| match s {
            Step::Tiles(n, p) => format!("tiles {n} {p}"),
            Step::Back(spot, p) => format!("back {} {p}", spot.id),
            Step::Removed(spot, p) => format!("removed {} {p}", spot.id),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        steps,
        [
            "tiles 1 6",
            "back 102 32",
            "removed 102 40",
            "removed 101 10"
        ]
    );
    assert_eq!(path, Some(101));
    assert!(!world.can_undo(), "the list is empty");
    assert_eq!(world.zoo.entities.len(), 9, "the wall and the entrance");
}

#[test]
fn a_piece_of_the_same_type_and_a_path_of_another_type_keep_their_id_and_place() {
    let mut world = walled(&[]);
    let path = |name: &str| typed("path", name, 32, 32, 0);
    let a = world.add(path("dirt"), shape(&path("dirt"))).unwrap();
    let other = world.add(entity("tent", 6 * 64, 6 * 64 + 32, 0), Some(NOTHING));
    world.begin();
    // A path of another type takes the new type in place: same id, same place in the list.
    let at = world.find(a).unwrap();
    assert_eq!(world.swap(a, path("stone"), shape(&path("stone"))), Some(a));
    world.priced(20.0);
    assert_eq!(world.find(a), Some(at));
    assert_eq!(world.zoo.entities[at].type_name, "stone");
    assert_eq!(world.zoo.id_counter, 102, "no id was used");
    assert_eq!(world.path(0), Some(at));
    let done = world.undo();
    assert!(matches!(done.steps[..], [Step::Retyped(_, p)] if p == 20.0));
    assert_eq!(world.zoo.entities[at].type_name, "dirt");
    assert_eq!(world.find(other.unwrap()), Some(at + 1));
    // A fence of the same type keeps its entity, made healthy; another type takes the old one's
    // place of the list under a fresh id; the undo gives each its old state back.
    let wall = |name: &str, life: u32| {
        let mut e = typed("plain", name, 2 * 64 + 32, 64, 0);
        e.common.as_mut().unwrap().fence = Some(crate::zoo::Fence {
            strength: 300,
            life,
            timers: [0; 2],
            portal: false,
        });
        e
    };
    let f = world.add(wall("iron", 3), Some(shape(&wall("iron", 3)).unwrap()));
    let f = f.unwrap();
    world.begin();
    let at = world.find(f).unwrap();
    let new = world.swap(f, wall("iron", 10), shape(&wall("iron", 10)));
    assert_eq!(new, Some(f));
    let life = |w: &World, at: usize| {
        w.zoo.entities[at]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap()
            .life
    };
    assert_eq!((world.find(f), life(&world, at)), (Some(at), 10));
    assert!(!world.undo().is_empty());
    assert_eq!((world.find(f), life(&world, at)), (Some(at), 3));
    let new = world
        .swap(f, wall("steel", 10), shape(&wall("steel", 10)))
        .unwrap();
    assert_ne!(new, f);
    assert_eq!((world.find(new), world.find(f)), (Some(at), None));
    assert_eq!(world.edge(world.tile_index(2, 1).unwrap(), 0), Some(at));
    assert!(!world.undo().is_empty());
    assert_eq!((world.find(f), world.find(new)), (Some(at), None));
    assert_eq!(world.zoo.entities[at].type_name, "iron");
}

/// Puts an entity in the way, as something the undo list does not know.
fn block(world: &mut World, e: Entity) {
    let shape = shape(&e);
    world.zoo.entities.push(e);
    world.shapes.push(shape);
    world.changed();
}

#[test]
fn a_removed_object_that_cannot_be_put_back_stops_the_undo_or_is_passed_over() {
    let tent = |tx: i32| entity("tent", tx * 64, 6 * 64 + 32, 0);
    let mut world = walled(&[]);
    let (a, b) = (
        world.add(tent(2), Some(NOTHING)).unwrap(),
        world.add(tent(5), Some(NOTHING)).unwrap(),
    );
    world.begin();
    assert!(world.remove(a));
    world.priced(5.0);
    // One removed object that does not fit: the undo stops, the list is kept for a later try.
    block(&mut world, tent(2));
    let done = world.undo();
    assert!(done.steps.is_empty());
    assert_eq!(done.blocked.map(|e| e.class), Some("tent".to_string()));
    assert!(world.can_undo());
    assert_eq!(world.find(a), None);
    // The obstacle goes: the next try puts it back.
    let last = world.zoo.entities.len() - 1;
    world.zoo.entities.remove(last);
    world.shapes.remove(last);
    world.changed();
    let done = world.undo();
    assert!(matches!(done.steps[..], [Step::Back(_, p)] if p == 5.0));
    assert!(!world.can_undo());
    // Two removed objects: the one that does not fit is passed over, the other comes back.
    world.begin();
    assert!(world.remove(a) && world.remove(b));
    world.priced(7.0);
    block(&mut world, tent(5));
    let done = world.undo();
    assert_eq!(done.blocked, None);
    assert!(matches!(done.steps[..], [Step::Skipped(_), Step::Back(_, p)] if p == 0.0));
    assert!(world.find(a).is_some() && world.find(b).is_none());
    assert!(!world.can_undo(), "the list is emptied as usual");
}

#[test]
fn a_tile_edit_writes_the_edge_states_of_the_tile_and_its_neighbours() {
    let mut world = World::new(zoo(3, 3, (0, 0)), shape);
    assert!(world.set_tile(1, 1, 1, 0, 2));
    let edges = |x: usize, y: usize| world.zoo.tiles[y * 3 + x].edges;
    let centre = world.zoo.tiles[4];
    assert_eq!((centre.elevation, centre.shape, centre.terrain), (1, 0, 2));
    // The centre stands a step above all four: state 3 on every side. The tile above sees the
    // centre rise by a step across its edge 4: state 2 there, joined (0) elsewhere.
    assert_eq!(edges(1, 1), 0b1111_1111);
    assert_eq!(edges(1, 0), 0b10 << 2);
    assert_eq!(edges(0, 1), 0b10 << 4);
    assert_eq!(edges(1, 2), 0b10 << 6);
    assert_eq!(edges(2, 1), 0b10);
    assert_eq!(edges(0, 0), 0, "a corner neighbour is not touched");
    // The east side of the centre rises a step (corners 1 and 3 up) above the flat tile there.
    assert!(world.set_tile(1, 1, 0, 0x50, 0));
    assert_eq!(
        world.zoo.tiles[4].edges >> 4 & 3,
        3,
        "a step against the flat tile"
    );
    assert!(!world.set_tile(3, 0, 0, 0, 0) && !world.set_tile(-1, 0, 0, 0, 0));
}

#[test]
fn edge_states_follow_the_rule_for_slopes() {
    let mut world = World::new(zoo(2, 1, (0, 0)), shape);
    // Tile 0 rises towards its east side: corners 1 and 3 are one step up. Tile 1 is flat at
    // the height of those corners: the two meet along the edge, state 0 for equal heights.
    world.zoo.tiles[1].elevation = 1;
    world.set_tile(0, 0, 0, 0b0101_0000, 0);
    assert_eq!(world.zoo.tiles[0].edges >> 4 & 3, 0);
    assert_eq!(world.zoo.tiles[1].edges & 3, 0);
    // Tile 1 flat at 0: the east side of tile 0 rises above it by a step (t0 == t1 == 1, n == 0).
    world.set_tile(1, 0, 0, 0, 0);
    assert_eq!(world.zoo.tiles[0].edges >> 4 & 3, 3);
    assert_eq!(world.zoo.tiles[1].edges & 3, 2);
    // Tile 0 sloped along the edge (corner 1 up, corner 3 flat): the edge is a slope, state 1
    // against a tile that follows the same line.
    world.set_tile(0, 0, 0, 0b0100_0000, 0);
    world.set_tile(1, 0, 1, 0b0000_0001, 0);
    assert_eq!(world.zoo.tiles[0].edges >> 4 & 3, 1);
}

/// The `.zoo` members of the game or trial in `ZT_DATA_DIR`.
fn shipped() -> (crate::vfs::Vfs, Vec<String>) {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let maps = vfs
        .list(None)
        .into_iter()
        .filter(|m| m.to_ascii_lowercase().ends_with(".zoo"))
        .collect();
    (vfs, maps)
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_exterior_matches_the_original_on_the_shipped_maps() {
    use crate::entity::Catalogue;
    use crate::render::MapRenderer;
    let (vfs, _) = shipped();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let renderer = MapRenderer::new(&vfs, &catalogue);
    for (map, want) in [
        ("lvalley", 14150),
        ("svolcano", 3324),
        ("tundra", 4014),
        ("lunar", 8478),
        ("lg_sandy", 9735),
        ("crater", 14407),
        ("default", 3075),
    ] {
        let zoo = Zoo::parse(&vfs.read(&format!("maps/{map}.zoo")).unwrap()).unwrap();
        let tiles = zoo.tiles.len();
        let world = World::new(zoo, |e| renderer.shape(e));
        assert_eq!(
            (0..tiles).filter(|&t| world.in_zoo(t)).count(),
            want,
            "{map}"
        );
    }
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn edge_states_match_the_shipped_maps() {
    let (vfs, maps) = shipped();
    assert!(!maps.is_empty());
    for map in maps {
        let Ok(zoo) = Zoo::parse(&vfs.read(&map).unwrap()) else {
            continue;
        };
        let wrong = (0..zoo.tiles.len())
            .filter(|&t| edge_states(&zoo, t) != zoo.tiles[t].edges)
            .count();
        assert_eq!(wrong, 0, "{map}");
    }
}
