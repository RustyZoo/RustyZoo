//! The brush, the acceptance rules, the session and its cost, on synthetic maps.

use super::*;
use crate::render::order::Kind;
use crate::sprite::tests::Install;
use crate::world::{Shape, TankKind};
use crate::zoo::{Common, View, Zoo};
use crate::zoom::Zoom;

const GRASS: u8 = 0;
const SAND: u8 = 2;
const DIRT: u8 = 3;
const FRESH: u8 = 9;
const SALT: u8 = 10;

fn kind(kind: u8, cost: f32, water: bool) -> Type {
    Type {
        kind,
        cost,
        water,
        icon: String::new(),
        help: 0,
        listed: true,
    }
}

fn table() -> Vec<Type> {
    vec![
        kind(GRASS, 40.0, false),
        kind(SAND, 30.0, false),
        kind(DIRT, 20.0, false),
        kind(FRESH, 30.0, true),
        kind(SALT, 60.0, true),
        Type {
            listed: false,
            ..kind(TRAMPLED, 30.0, false)
        },
    ]
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

/// The entity standing in the middle of tile (`tx`, `ty`).
fn on(class: &str, tx: i32, ty: i32) -> Entity {
    entity(class, tx * 64 + 32, ty * 64 + 32, 0)
}

fn shape(e: &Entity) -> Option<Shape> {
    let fixed = Shape {
        kind: Kind::Fixed,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: TankKind::NONE,
    };
    match e.class.as_str() {
        "paths" => Some(Shape {
            kind: Kind::Path,
            ..fixed
        }),
        "fences" | "wfence" => Some(Shape {
            kind: Kind::Edge,
            fence: true,
            wall: true,
            ..fixed
        }),
        "twall" => Some(Shape {
            kind: Kind::Edge,
            fence: true,
            tank: TankKind {
                wall: true,
                show: false,
                ..TankKind::NONE
            },
            ..fixed
        }),
        "gate" => Some(Shape {
            footprint: (2, 2),
            gate: true,
            ..fixed
        }),
        "tree" | "buoy" | "pond" => Some(fixed),
        "bench" => Some(Shape {
            footprint: (2, 1),
            ..fixed
        }),
        "bed" => Some(Shape {
            footprint: (2, 2),
            ..fixed
        }),
        "hall" => Some(Shape {
            footprint: (4, 4),
            ..fixed
        }),
        "animals" => Some(Shape {
            kind: Kind::Unit,
            ..fixed
        }),
        "elephant" => Some(Shape {
            kind: Kind::Unit,
            footprint: (4, 4),
            ..fixed
        }),
        _ => None,
    }
}

fn habits(e: &Entity) -> Habit {
    Habit {
        swims: matches!(e.class.as_str(), "buoy" | "pond"),
        only_swims: matches!(e.class.as_str(), "pond" | "wfence"),
    }
}

/// An 8 by 8 map of grass, all of it inside the zoo (the entrance is a gate on the last tile,
/// facing off the map), with these entities.
fn world(entities: Vec<Entity>) -> World {
    let mut world = map(entity("gate", 7 * 64 + 32, 7 * 64 + 32, 2), entities, false);
    assert!((0..64).all(|t| world.in_zoo(t)));
    world.begin();
    world
}

/// The same map with its entrance gate on tile (1, 4) facing west and a zoo wall on the east
/// edge of column 4: the tiles of the columns 0 to 4 are outside the zoo, except the gate's.
fn walled(entities: Vec<Entity>) -> World {
    map(entity("gate", 96, 288, 6), entities, true)
}

fn map(gate: Entity, entities: Vec<Entity>, wall: bool) -> World {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: GRASS,
        edges: 0,
    };
    let mut zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: 8,
        height: 8,
        entrance: if wall { (1, 4) } else { (7, 7) },
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; 64],
        entities: vec![gate],
        view: View::default(),
    };
    if wall {
        zoo.entities
            .extend((0..8).map(|row| entity("fences", 4 * 64 + 63, row * 64 + 32, 2)));
    }
    zoo.entities.extend(entities);
    World::new(zoo, shape)
}

fn tool() -> Tool {
    Tool::new(table(), 8.0)
}

fn terrain_of(world: &World, tx: usize, ty: usize) -> u8 {
    world.zoo.tiles[ty * 8 + tx].terrain
}

fn cash(amount: f32) -> Money {
    let mut money = Money::default();
    money.set_cash(amount);
    money
}

#[test]
fn the_brush_sits_around_the_tile_and_is_moved_back_inside_the_map() {
    // The centre tile of an even brush is at the upper left of its middle.
    assert_eq!(brush_origin(2, (4, 4), 8, 8), (4, 4));
    assert_eq!(brush_origin(1, (4, 4), 8, 8), (4, 4));
    assert_eq!(brush_origin(3, (4, 4), 8, 8), (3, 3));
    assert_eq!(brush_origin(4, (4, 4), 8, 8), (3, 3));
    assert_eq!(brush_origin(5, (4, 4), 8, 8), (2, 2));
    // At the corners and edges the size is kept and the brush shifts.
    assert_eq!(brush_origin(2, (0, 0), 8, 8), (0, 0));
    assert_eq!(brush_origin(2, (7, 7), 8, 8), (6, 6));
    assert_eq!(brush_origin(5, (0, 0), 8, 8), (0, 0));
    assert_eq!(brush_origin(5, (7, 3), 8, 8), (3, 1));
    assert_eq!(brush_origin(3, (-3, 20), 8, 8), (0, 5), "a centre far off");
    // A map narrower than the brush: the brush sticks out on the right and below.
    assert_eq!(brush_origin(5, (1, 1), 3, 2), (0, 0));
    // A size outside 1 to 5 is cut into it.
    assert_eq!(brush_origin(0, (4, 4), 8, 8), (4, 4));
    assert_eq!(brush_origin(99, (4, 4), 8, 8), (2, 2));
}

#[test]
fn the_brush_side_is_kept_between_1_and_5() {
    let mut tool = tool();
    assert_eq!(tool.size(), 2);
    assert!(!tool.set_size(0) && !tool.set_size(6) && !tool.set_size(usize::MAX));
    assert_eq!(tool.size(), 2);
    assert!(tool.set_size(1) && tool.set_size(5));
    assert_eq!(tool.size(), 5);
    assert!(!tool.set_kind(TRAMPLED) && !tool.set_kind(77), "not listed");
    assert!(tool.set_kind(SAND));
    assert_eq!(tool.kind(), SAND);
}

#[test]
fn the_price_of_a_step_is_read_from_the_ini_file() {
    let read = |ini: &str| elevation_cost(ini.as_bytes());
    assert_eq!(read(""), 8.0, "missing");
    assert_eq!(
        read("[Map]\nterrainCost=3\n"),
        8.0,
        "the other key is not used"
    );
    assert_eq!(read("[Map]\nelevationCost=12\n"), 12.0);
    assert_eq!(read("[map]\nELEVATIONCOST=7.5\n"), 7.5, "any case");
    assert_eq!(read("[Map]\nelevationCost=2.5e1\n"), 25.0);
    assert_eq!(read("[Map]\nelevationCost=0\n"), 0.0);
    assert_eq!(read("[Map]\nelevationCost=-3\n"), 8.0, "negative");
    assert_eq!(read("[Map]\nelevationCost=1e30\n"), 8.0, "huge");
    assert_eq!(
        read("[Map]\nelevationCost=nan\n"),
        0.0,
        "not a number reads as 0"
    );
    assert_eq!(read("[Map]\nelevationCost=\n"), 8.0, "no value");
}

#[test]
fn the_types_are_read_from_every_terrain_file_in_order_of_their_numbers() {
    let tex = "[ttGrass]\ntype=0\ncost=40\nhelpid=3365\nicon=terrain/icgrass/icgrass\n\
               [ttFresh]\ntype=9\ncost=30\nwater=1\n\
               [ttFall]\ntype=12\ncost=100\nwater=1\n\
               [ttBad]\ntype=300\n\
               [ttNegative]\ntype=4\ncost=-9\n\
               [ttHuge]\ntype=5\ncost=1e30\n\
               [ttNothing]\nicon=x\n";
    let more = "[ttGunnite]\ntype=17\ncost=30\n[ttGrass2]\ntype=0\ncost=45\n";
    let install = Install::new(
        "terrain-types",
        &[
            ("terrain/tiletex.cfg", tex.as_bytes().to_vec()),
            ("terrain/tiletex1.cfg", more.as_bytes().to_vec()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let found = types(&vfs);
    let summary: Vec<_> = found
        .iter()
        .map(|t| (t.kind, t.cost, t.water, t.listed))
        .collect();
    assert_eq!(
        summary,
        [
            (0, 45.0, false, true), // the later file wins
            (4, 0.0, false, true),  // a negative price is 0
            (5, 0.0, false, true),  // a huge one too
            (9, 30.0, true, true),
            (12, 100.0, true, false), // the waterfall is not in the list
            (17, 30.0, false, false),
        ]
    );
    assert!(types(&Vfs::open(&Install::new("terrain-none", &[]).0).unwrap()).is_empty());
}

#[test]
fn a_tile_accepts_a_type_inside_the_zoo_when_it_does_not_have_it() {
    let world = walled(vec![]);
    let types = table();
    let accepts = |t, kind| accepts(&world, &habits, &types, t, kind);
    assert!(accepts(5 + 4 * 8, SAND));
    assert!(!accepts(5 + 4 * 8, GRASS), "already grass");
    assert!(!accepts(2 + 4 * 8, SAND), "outside the zoo");
    assert!(!accepts(64, SAND), "off the map");
}

#[test]
fn water_goes_on_flat_ground_with_no_unit_and_only_objects_that_swim() {
    let mut world = world(vec![
        on("tree", 1, 1),
        on("buoy", 2, 1),
        on("animals", 3, 1),
        on("pond", 0, 2),
        on("animals", 1, 2),
    ]);
    // The tile (3, 2) is a slope; the tile (2, 2) is fresh water already; the tile (0, 2) holds a
    // pond object that lives in water alone and is salt water now.
    world.set_tile(3, 2, 0, 1, GRASS);
    world.set_tile(2, 2, 0, 0, FRESH);
    world.set_tile(0, 2, 0, 0, FRESH);
    let types = table();
    let accepts = |tx: usize, ty: usize, kind| accepts(&world, &habits, &types, ty * 8 + tx, kind);
    assert!(accepts(0, 0, FRESH), "flat, empty");
    assert!(accepts(0, 0, SALT));
    assert!(!accepts(1, 1, FRESH), "a tree does not swim");
    assert!(accepts(2, 1, FRESH), "a buoy swims");
    assert!(!accepts(3, 1, FRESH), "an animal stands there");
    assert!(!accepts(3, 2, FRESH), "a slope");
    // Fresh water to salt water has no condition, whatever stands there.
    assert!(accepts(2, 2, SALT));
    // Land under an object that lives in water alone is refused; so is land under a unit.
    assert!(!accepts(0, 2, GRASS), "the pond holds water");
    world.set_tile(1, 2, 0, 0, FRESH);
    let accepts =
        |tx: usize, ty: usize, kind| super::accepts(&world, &habits, &types, ty * 8 + tx, kind);
    assert!(!accepts(1, 2, DIRT), "an animal stands there");
    assert!(accepts(2, 2, DIRT), "water into land with nothing on it");
    // Land to land does not mind a tree, a unit or a slope.
    assert!(accepts(1, 1, SAND) && accepts(3, 1, SAND) && accepts(3, 2, SAND));
}

#[test]
fn a_path_refuses_both_changes_and_a_fence_on_a_side_stops_only_water_that_swims_alone() {
    // A fence on the east side of (1, 3) and of (3, 3), the second one lives in water alone; a
    // path on (2, 3) and on (5, 3).
    let mut world = world(vec![
        entity("fences", 64 + 63, 3 * 64 + 32, 2),
        entity("wfence", 3 * 64 + 63, 3 * 64 + 32, 2),
        on("paths", 2, 3),
        on("paths", 5, 3),
    ]);
    world.set_tile(3, 3, 0, 0, FRESH);
    world.set_tile(5, 3, 0, 0, FRESH);
    let types = table();
    let accepts = |tx: usize, kind| accepts(&world, &habits, &types, 3 * 8 + tx, kind);
    assert!(
        world.edge(3 * 8 + 1, 2).is_some(),
        "the fence is on the side"
    );
    assert!(accepts(1, FRESH), "a fence on a side never stops water");
    assert!(!accepts(2, FRESH), "a path refuses land into water");
    assert!(
        !accepts(3, GRASS),
        "a fence that lives in water alone refuses land"
    );
    assert!(!accepts(5, GRASS), "a path refuses water into land");
}

#[test]
fn cancel_gives_back_the_type_only_when_the_tile_accepts_it_before_its_shape() {
    let mut world = world(vec![]);
    // A sloped water tile: land now, the water would not be accepted on a slope.
    world.set_tile(1, 1, 0, 0b11_00_00_00, FRESH);
    // A flat water tile.
    world.set_tile(2, 1, 0, 0, FRESH);
    let (mut tool, mut lines) = (tool(), Vec::new());
    tool.enter(Mode::Paint);
    tool.set_kind(GRASS);
    tool.set_size(5);
    tool.press(&mut world, &habits, (0, 0), Some((2, 1)), &mut lines);
    assert_eq!(terrain_of(&world, 1, 1), GRASS);
    assert_eq!(terrain_of(&world, 2, 1), GRASS);
    tool.cancel(&mut world, &habits, &mut lines);
    assert_eq!(terrain_of(&world, 1, 1), GRASS, "sloped: stays land");
    assert_eq!(terrain_of(&world, 2, 1), FRESH, "flat: water again");
    assert_eq!(world.zoo.tiles[9].shape, 0b11_00_00_00, "the shape is back");
}

#[test]
fn a_tile_painted_twice_costs_the_last_type_only_and_painted_back_nothing() {
    let mut world = world(vec![]);
    let (mut tool, mut lines) = (tool(), Vec::new());
    tool.enter(Mode::Paint);
    assert!(tool.set_kind(SAND));
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
    assert_eq!(tool.cost(&world), 120.0, "four tiles of sand");
    assert_eq!(lines, ["paint 4", "cost 120"]);
    tool.release();
    assert!(tool.set_kind(DIRT));
    lines.clear();
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
    assert_eq!(
        tool.cost(&world),
        80.0,
        "four tiles of dirt, not sand then dirt"
    );
    assert_eq!(lines, ["paint 4", "cost 80"]);
    tool.release();
    assert!(tool.set_kind(GRASS));
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
    assert_eq!(tool.cost(&world), 0.0, "grass again: nothing to pay");
    assert_eq!(terrain_of(&world, 1, 1), GRASS);
}

#[test]
fn the_pointer_shows_minus_the_brush_cost_in_the_paint_tab_and_the_help_of_a_new_tile() {
    let mut world = world(vec![]);
    let mut types = table();
    types[0].help = 3365;
    types[1].help = 3367;
    let (mut tool, mut lines) = (Tool::new(types, 8.0), Vec::new());
    assert_eq!(tool.pointer_amount(), None, "no tab");
    assert_eq!(tool.help_at(&world, Some((1, 1))), None, "no tab");
    tool.enter(Mode::Modify);
    assert_eq!(
        (tool.pointer_amount(), tool.help_at(&world, Some((1, 1)))),
        (None, None)
    );
    tool.enter(Mode::Paint);
    assert!(tool.set_kind(SAND));
    assert_eq!(tool.pointer_amount(), Some(0), "no brush yet");
    tool.place(&world, &habits, Some((1, 1)));
    assert_eq!(tool.pointer_amount(), Some(-120));
    // A tile once, the help of its type; a tile off the map or with a type without help: none.
    assert_eq!(tool.help_at(&world, Some((1, 1))), Some(3365));
    assert_eq!(tool.help_at(&world, Some((1, 1))), None);
    assert_eq!(tool.help_at(&world, None), None);
    assert_eq!(
        tool.help_at(&world, Some((1, 1))),
        None,
        "a tile off the map does not make the last one forgotten"
    );
    assert_eq!(tool.help_at(&world, Some((2, 1))), Some(3365));
    world.set_tile(3, 1, 0, 0, DIRT);
    assert_eq!(tool.help_at(&world, Some((3, 1))), None, "no help string");
    // A held button asks for nothing; a paint makes the amount 0.
    assert_eq!(tool.help_at(&world, Some((4, 1))), Some(3365));
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
    assert_eq!(tool.help_at(&world, Some((5, 1))), None, "button down");
    assert_eq!(tool.pointer_amount(), Some(0));
    tool.release();
    assert_eq!(
        tool.help_at(&world, Some((5, 1))),
        Some(3365),
        "released on another tile"
    );
}

#[test]
fn the_cost_counts_the_largest_corner_change_of_each_touched_tile_and_the_type() {
    let mut world = world(vec![]);
    let types = table();
    let mut session = Session::default();
    let old = world.zoo.tiles[9];
    session.touch(9, old);
    // A tile raised by two steps, and painted.
    world.set_tile(1, 1, 2, 0, SAND);
    assert_eq!(session.cost(&world, 8.0, &types), 2.0 * 8.0 + 30.0);
    // The first touch counts: a later one does not overwrite it.
    session.touch(9, world.zoo.tiles[9]);
    assert_eq!(session.cost(&world, 8.0, &types), 46.0);
    // A tile with one corner raised by three steps and the others by one: the largest counts.
    session.touch(10, world.zoo.tiles[10]);
    world.set_tile(2, 1, 1, 0b11_00_00_00, GRASS);
    assert_eq!(session.cost(&world, 8.0, &types), 46.0 + 4.0 * 8.0);
    assert_eq!(session.cost(&world, 0.0, &types), 30.0, "a free step");
    session.restore(&mut world, &habits, &types);
    assert_eq!(world.zoo.tiles[9], old);
    assert_eq!(session.cost(&world, 8.0, &types), 0.0);
}

#[test]
fn accept_pays_and_books_the_session_and_cancel_gives_the_tiles_back() {
    let mut world = world(vec![]);
    let (mut tool, mut money, mut lines) = (tool(), cash(50_000.0), Vec::new());
    tool.enter(Mode::Paint);
    tool.set_kind(SAND);
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
    tool.release();
    lines.clear();
    // Cancel: the grass is back, nothing was paid.
    let before = world.zoo.tiles.clone();
    tool.cancel(&mut world, &habits, &mut lines);
    assert_eq!(lines, ["cancel 120"]);
    assert_eq!(terrain_of(&world, 1, 1), GRASS);
    assert_eq!((tool.cost(&world), money.cash), (0.0, 50_000.0));
    assert_ne!(before, world.zoo.tiles);
    // Accept: the cost goes from the cash into the construction account, and a new session starts.
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
    lines.clear();
    assert!(tool.accept(&world, &habits, &mut money, &mut lines));
    assert_eq!(lines, ["accept 120"]);
    assert_eq!(money.cash, 49_880.0);
    assert_eq!(money.total[CONSTRUCTION], 120.0);
    assert_eq!(tool.cost(&world), 0.0);
    assert_eq!(terrain_of(&world, 1, 1), SAND);
    // Nothing to accept now, and a cancel does not give back what was paid.
    assert!(!tool.accept(&world, &habits, &mut money, &mut lines));
    tool.cancel(&mut world, &habits, &mut lines);
    assert_eq!(terrain_of(&world, 1, 1), SAND);
}

#[test]
fn leaving_accepts_what_the_cash_covers_and_cancels_what_it_does_not() {
    for (cash_at_start, left, paid) in [(120.0, Left::Kept, true), (119.0, Left::Short, false)] {
        let mut world = world(vec![]);
        let (mut tool, mut money, mut lines) = (tool(), cash(cash_at_start), Vec::new());
        tool.enter(Mode::Paint);
        tool.set_kind(SAND);
        tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut lines);
        assert_eq!(
            (tool.cost(&world), money.can_pay(tool.cost(&world))),
            (120.0, paid)
        );
        assert_eq!(
            tool.leave(&mut world, &habits, &mut money, &mut lines),
            left
        );
        assert_eq!(tool.mode(), None);
        let (sand, cash_now) = match paid {
            true => (SAND, cash_at_start - 120.0),
            false => (GRASS, cash_at_start),
        };
        assert_eq!((terrain_of(&world, 1, 1), money.cash), (sand, cash_now));
    }
    // A session that costs nothing leaves without a word.
    let mut world = world(vec![]);
    let (mut tool, mut money, mut lines) = (tool(), cash(0.0), Vec::new());
    tool.enter(Mode::Paint);
    assert_eq!(
        tool.leave(&mut world, &habits, &mut money, &mut lines),
        Left::Kept
    );
    assert!(lines.is_empty());
}

#[test]
fn a_stroke_paints_along_the_pointer_and_only_where_the_brush_moved() {
    let mut world = world(vec![]);
    let (mut tool, mut lines) = (tool(), Vec::new());
    tool.enter(Mode::Paint);
    tool.set_kind(SAND);
    // The tile under the window point (x, y) is (x / 10, 0) here.
    let tiles = |_: &World, (x, _): (i32, i32)| u32::try_from(x / 10).ok().map(|x| (x, 0));
    tool.press(&mut world, &habits, (0, 0), Some((0, 0)), &mut lines);
    assert_eq!(lines, ["paint 4", "cost 120"]);
    lines.clear();
    // A small step within the same tile paints nothing more.
    tool.moved(
        &mut world,
        &habits,
        (5, 0),
        &tiles,
        Zoom::Normal,
        &mut lines,
    );
    assert!(lines.is_empty());
    // A long move is cut into ten points: the brush crosses the tiles 1, 2, 3 and 4.
    tool.moved(
        &mut world,
        &habits,
        (45, 0),
        &tiles,
        Zoom::Normal,
        &mut lines,
    );
    assert_eq!(tool.cost(&world), 30.0 * 12.0, "a strip of 6 by 2 tiles");
    for tx in 0..6 {
        assert_eq!(terrain_of(&world, tx, 0), SAND, "{tx}");
    }
    // The button went up: the brush follows without painting.
    tool.release();
    tool.moved(
        &mut world,
        &habits,
        (65, 0),
        &tiles,
        Zoom::Normal,
        &mut lines,
    );
    assert_eq!(terrain_of(&world, 6, 0), GRASS);
    assert_eq!(tool.cost(&world), 360.0);
}

#[test]
fn the_brush_highlight_is_green_where_it_may_change_and_red_where_it_may_not() {
    let world = walled(vec![on("tree", 6, 6)]);
    let mut tool = tool();
    tool.enter(Mode::Paint);
    tool.set_kind(FRESH);
    // Two tiles outside the zoo, two inside (the wall is on the east edge of column 4).
    tool.place(&world, &habits, Some((4, 4)));
    let marks = tool.marks(&world, &habits);
    let colours: Vec<_> = marks.iter().map(|m| (m.tile, m.color)).collect();
    assert_eq!(
        colours,
        [
            (36, INVALID),
            (37, SELECTION),
            (44, INVALID),
            (45, SELECTION)
        ]
    );
    assert_eq!(tool.brush_cost(), 60.0, "the two tiles that accept water");
    // A brush on a tree: water is refused there, so red; the same tile is green when it has
    // the type already.
    tool.place(&world, &habits, Some((6, 6)));
    let marks = tool.marks(&world, &habits);
    assert_eq!(marks[0].color, INVALID, "the tree does not swim");
    assert_eq!(tool.brush_cost(), 90.0);
    tool.set_kind(GRASS);
    tool.refresh(&world, &habits);
    assert!(
        tool.marks(&world, &habits)[..4]
            .iter()
            .all(|m| m.color == SELECTION)
    );
    assert_eq!(tool.brush_cost(), 0.0, "nothing to paint");
    // The modify mode has no types: inside is green and outside red; nothing else is marked.
    tool.enter(Mode::Modify);
    tool.place(&world, &habits, Some((4, 4)));
    let marks = tool.marks(&world, &habits);
    assert_eq!(marks.len(), 4);
    assert_eq!((marks[0].color, marks[1].color), (INVALID, SELECTION));
    assert_eq!(tool.brush_cost(), 0.0);
    assert!(
        Tool::new(vec![], 8.0).marks(&world, &habits).is_empty(),
        "no tool"
    );
}

#[test]
fn trampled_ground_is_marked_in_the_paint_mode_only() {
    let mut world = world(vec![]);
    world.set_tile(6, 6, 0, 0, TRAMPLED);
    let mut tool = tool();
    tool.enter(Mode::Modify);
    assert!(tool.marks(&world, &habits).is_empty());
    tool.enter(Mode::Paint);
    let marks = tool.marks(&world, &habits);
    assert_eq!(
        marks,
        [Mark {
            tile: 54,
            color: TRAMPLED_COLOR
        }]
    );
}

#[test]
fn the_habit_of_a_type_comes_from_its_characteristics() {
    use crate::config::Config;
    use crate::entity::EntityType;
    let habit = |ini: &str| {
        type_habit(&EntityType {
            class: "objects",
            type_name: "t".into(),
            subtype: "".into(),
            file: None,
            config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        })
    };
    let c = |keys: &str| habit(&format!("[Characteristics/Integers]\n{keys}"));
    let (no, swims, only) = (
        Habit::default(),
        Habit {
            swims: true,
            only_swims: false,
        },
        Habit {
            swims: true,
            only_swims: true,
        },
    );
    assert_eq!(c("cLand=1\n"), no);
    assert_eq!(c("cSwims=1\ncLand=1\n"), swims);
    assert_eq!(c("cSwims=1\n"), swims, "land when missing");
    assert_eq!(c("cSwims=2\n"), no, "only the value 1 is on");
    assert_eq!(c("cSwims=1\ncLand=0\n"), only, "a water type");
    assert_eq!(c("cSwims=1\ncLand=0\ncUnderwater=1\n"), swims);
    assert_eq!(
        c("cOnlySwims=1\n"),
        Habit {
            swims: false,
            only_swims: true
        }
    );
    assert_eq!(habit(""), no, "no characteristics");
}

/// Presses on tile (`tx`, `ty`) and drags straight up by `up` pixels (down when negative) at
/// the zoom `zoom`, the pointer staying over the same tile; the lines.
fn drag(tool: &mut Tool, world: &mut World, (tx, ty): Cell, up: i32, zoom: Zoom) -> Vec<String> {
    let mut lines = Vec::new();
    let tiles = move |_: &World, _| Some((tx, ty));
    tool.press(world, &habits, (100, 100), Some((tx, ty)), &mut lines);
    tool.moved(world, &habits, (100, 100 - up), &tiles, zoom, &mut lines);
    tool.release();
    lines
}

/// The elevation of the corner point (`px`, `py`) as the tile that has it as corner 7 says.
fn point(world: &World, px: usize, py: usize) -> i32 {
    world.zoo.tiles[py * 8 + px].elevation
}

fn modify(size: usize, contiguous: bool, level: bool) -> Tool {
    let mut tool = tool();
    tool.enter(Mode::Modify);
    tool.set_size(size);
    tool.set_shaping(contiguous, level);
    tool
}

fn edges_joined(world: &World) -> bool {
    world
        .zoo
        .tiles
        .iter()
        .all(|t| (0..4).all(|d| t.edges >> (2 * d) & 3 < 2))
}

#[test]
fn a_contiguous_raise_lifts_the_brush_points_and_costs_the_tiles_around_it_too() {
    let mut world = world(vec![]);
    let mut tool = modify(2, true, false);
    // The drag of 10 pixels does nothing; 16 make one raise.
    assert!(drag(&mut tool, &mut world, (3, 3), 10, Zoom::Normal).is_empty());
    assert_eq!(tool.cost(&world), 0.0);
    let lines = drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(lines, ["shape 16", "cost 128"]);
    for (x, y) in [(3, 3), (5, 3), (5, 5), (3, 5), (4, 4)] {
        assert_eq!(point(&world, x, y), 1, "{x},{y}");
    }
    assert_eq!(
        point(&world, 2, 3) + point(&world, 6, 3) + point(&world, 3, 2),
        0
    );
    assert!(edges_joined(&world));
    // A second raise in a second stroke of the same session: 384 in all.
    let lines = drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(lines, ["shape 32", "cost 384"]);
    // The corner points of the second raise: 2 under the brush, 1 beside it, 0 on the diagonals.
    let table: Vec<Vec<i32>> = (1..=7)
        .map(|y| (1..=6).map(|x| point(&world, x, y)).collect())
        .collect();
    assert_eq!(
        table,
        [
            [0, 0, 0, 0, 0, 0],
            [0, 0, 1, 1, 1, 0],
            [0, 1, 2, 2, 2, 1],
            [0, 1, 2, 2, 2, 1],
            [0, 1, 2, 2, 2, 1],
            [0, 0, 1, 1, 1, 0],
            [0, 0, 0, 0, 0, 0],
        ]
    );
    assert!(edges_joined(&world));
}

#[test]
fn a_raise_then_a_lower_gives_the_ground_back_for_nothing() {
    let mut world = world(vec![]);
    let mut tool = modify(2, true, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    let lines = drag(&mut tool, &mut world, (3, 3), -16, Zoom::Normal);
    assert_eq!(lines, ["shape 16", "cost 0"]);
    assert!(
        world
            .zoo
            .tiles
            .iter()
            .all(|t| (t.elevation, t.shape) == (0, 0))
    );
}

#[test]
fn two_steps_in_one_drag_cost_the_same_as_two_strokes() {
    let mut world = world(vec![]);
    let mut tool = modify(2, true, false);
    let lines = drag(&mut tool, &mut world, (3, 3), 32, Zoom::Normal);
    assert_eq!(lines, ["shape 32", "cost 384"]);
}

#[test]
fn a_raise_that_is_not_contiguous_leaves_cliffs_and_costs_the_brush_only() {
    let mut world = world(vec![]);
    let mut tool = modify(2, false, false);
    let lines = drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(lines, ["shape 4", "cost 32"]);
    assert!(!edges_joined(&world));
    let at = |x: usize, y: usize| world.zoo.tiles[y * 8 + x];
    assert_eq!(
        (at(3, 3).elevation, at(4, 4).elevation, at(2, 3).elevation),
        (1, 1, 0)
    );
    assert_eq!(at(2, 3).edges & 3, 0, "the west neighbour is not joined");
    assert!(
        at(3, 3).edges >> 6 & 3 >= 2,
        "the brush has a cliff on its north side"
    );
}

/// The elevations of the tiles of rows `y0` to `y1` and columns `x0` to `x1`, row by row.
fn heights(world: &World, (x0, x1): (usize, usize), (y0, y1): (usize, usize)) -> Vec<i32> {
    (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| (x, y)))
        .map(|(x, y)| world.zoo.tiles[y * 8 + x].elevation)
        .collect()
}

#[test]
fn a_non_contiguous_step_carries_the_other_tiles_of_a_large_object_along() {
    // A hall of 2 by 2 tiles on (3..4, 3..4); the brush is its first tile.
    let mut world = world(vec![entity("hall", 4 * 64, 4 * 64, 0)]);
    assert_eq!(world.carried(3 * 8 + 3), [27, 28, 35, 36]);
    let mut tool = modify(1, false, false);
    let lines = drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(
        lines,
        ["shape 4", "cost 32"],
        "the cost is that of four tiles"
    );
    assert_eq!(
        heights(&world, (2, 5), (2, 5)),
        [0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0]
    );
    // The ring of the brush and the bounding rectangle of the hall, grown by one tile, are in the
    // snapshot: 16 tiles, 4 of them changed.
    assert_eq!(tool.session.touched.len(), 16);
    // Cancel gives everything back.
    tool.cancel(&mut world, &habits, &mut Vec::new());
    assert!(
        world
            .zoo
            .tiles
            .iter()
            .all(|t| t.elevation == 0 && t.shape == 0)
    );
}

#[test]
fn the_tiles_carried_are_those_of_the_points_of_the_footprint_not_its_quadrants() {
    // A hall of 2 by 2 tiles centred on the tile centre (288, 288): the points (224, 224),
    // (288, 224), (224, 288) and (288, 288) are on the tiles 3 and 4 in both directions, while its
    // quadrants reach the tiles 3 to 5.
    let mut world = world(vec![entity("hall", 4 * 64 + 32, 4 * 64 + 32, 0)]);
    assert_eq!(world.carried(4 * 8 + 4), [27, 28, 35, 36]);
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (4, 4), 16, Zoom::Normal);
    assert_eq!(heights(&world, (3, 5), (3, 5)), [1, 1, 0, 1, 1, 0, 0, 0, 0]);
    // A unit near the corner of the map: points less than 64 left of or above it count in column
    // or row 0, the others are off the map.
    let world = self::world(vec![entity("elephant", 16, 16, 0)]);
    assert_eq!(world.carried(0), [0]);
    let world = self::world(vec![entity("elephant", 16 + 64, 16, 0)]);
    assert_eq!(world.carried(1), [0, 1]);
}

#[test]
fn a_brush_half_on_a_building_takes_the_rest_of_it_along() {
    // The brush covers (2..3, 3..4): the hall's other column, (4, 3) and (4, 4), is carried.
    let mut world = world(vec![entity("hall", 4 * 64, 4 * 64, 0)]);
    let mut tool = modify(2, false, false);
    let lines = drag(&mut tool, &mut world, (2, 3), 16, Zoom::Normal);
    assert_eq!(lines, ["shape 6", "cost 48"]);
    assert_eq!(
        heights(&world, (1, 5), (2, 5)),
        [0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn a_carried_tile_moves_only_when_it_stands_as_low_as_the_brush_and_inside_the_zoo() {
    let mut world = world(vec![entity("hall", 4 * 64, 4 * 64, 0)]);
    // The tile (4, 4) is higher than the goal of the raise: it stays.
    world.set_tile(4, 4, 2, 0, GRASS);
    world.begin();
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(heights(&world, (3, 4), (3, 4)), [1, 1, 1, 2]);
    // A lower: the hall tiles that are not above the goal stay, the others come down.
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (4, 4), -16, Zoom::Normal);
    assert_eq!(heights(&world, (3, 4), (3, 4)), [1, 1, 1, 1]);
    // Outside the zoo nothing is carried: the hall stands on the wall's east side too.
    let mut world = walled(vec![entity("hall", 5 * 64, 4 * 64, 0)]);
    assert!(world.in_zoo(3 * 8 + 5) && !world.in_zoo(3 * 8 + 4));
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (5, 3), 16, Zoom::Normal);
    assert_eq!(heights(&world, (4, 5), (3, 4)), [0, 1, 0, 1]);
}

#[test]
fn a_large_unit_is_carried_only_from_a_tile_whose_first_quarter_is_empty() {
    // An elephant of 2 by 2 tiles stands on (4, 4): its tiles are (3..4, 3..4).
    let unit = entity("elephant", 4 * 64, 4 * 64, 0);
    let mut world = world(vec![unit.clone()]);
    assert_eq!(world.carried(4 * 8 + 4), [27, 28, 35, 36]);
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (4, 4), 16, Zoom::Normal);
    assert_eq!(heights(&world, (3, 4), (3, 4)), [1, 1, 1, 1]);
    // A small object in the first quarter of the tile is the one looked at: it takes nothing.
    let mut world = self::world(vec![unit, entity("tree", 4 * 64 + 16, 4 * 64 + 16, 0)]);
    assert!(world.carried(4 * 8 + 4).is_empty());
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (4, 4), 16, Zoom::Normal);
    assert_eq!(heights(&world, (3, 4), (3, 4)), [0, 0, 0, 1]);
}

#[test]
fn a_contiguous_step_and_small_objects_carry_nothing() {
    let mut world = world(vec![entity("hall", 4 * 64, 4 * 64, 0)]);
    let mut tool = modify(1, true, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    // The spreading of 5.4 moves them as a whole, the ring is not snapshotted unchanged.
    assert_eq!(heights(&world, (3, 4), (3, 4)), [1, 1, 1, 1]);
    let mut world = self::world(vec![entity("bench", 3 * 64 + 32, 3 * 64 + 32, 0)]);
    assert!(world.carried(3 * 8 + 3).is_empty());
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(tool.session.touched.len(), 9, "the ring of the brush alone");
}

#[test]
fn the_pointer_makes_a_step_per_unit_of_drag_that_depends_on_the_zoom() {
    let steps = |up: i32, zoom: Zoom| {
        let mut world = world(vec![]);
        let mut tool = modify(1, false, false);
        drag(&mut tool, &mut world, (3, 3), up, zoom);
        world.zoo.tiles[3 * 8 + 3].elevation
    };
    // Normal zoom: 16 pixels a step, and a remainder of more than half rounds up.
    assert_eq!(
        [15, 16, 20, 24, 25, 31, 32, 47, 49].map(|up| steps(up, Zoom::Normal)),
        [0, 1, 1, 1, 2, 2, 2, 3, 3]
    );
    assert_eq!([7, 8, 16].map(|up| steps(up, Zoom::Out)), [0, 1, 2]);
    assert_eq!([31, 32, 64].map(|up| steps(up, Zoom::In)), [0, 1, 2]);
    assert_eq!(steps(-48, Zoom::Normal), -3);
    // No more than the limit.
    assert_eq!(steps(16 * 30, Zoom::Normal), 12);
    assert_eq!(steps(-16 * 30, Zoom::Normal), -12);
}

#[test]
fn the_brush_follows_the_pointer_by_the_rules_of_each_mode() {
    let tiles = |_: &World, (x, _): (i32, i32)| u32::try_from(x / 10).ok().map(|x| (x, 3));
    let centre = |contiguous: bool, level: bool, dx: i32, dy: i32| {
        let mut world = world(vec![]);
        let mut tool = modify(1, contiguous, level);
        let mut lines = Vec::new();
        tool.press(&mut world, &habits, (25, 100), Some((2, 3)), &mut lines);
        tool.moved(
            &mut world,
            &habits,
            (25 + dx, 100 + dy),
            &tiles,
            Zoom::Normal,
            &mut lines,
        );
        tool.tiles(&world)
    };
    // Contiguous drag: after more than 15 pixels across. Not contiguous: never. Level: also
    // after more than 7 up or down.
    assert_eq!(centre(true, false, 10, 0), [3 * 8 + 2]);
    assert_eq!(centre(true, false, 16, 0), [3 * 8 + 4]);
    assert_eq!(centre(false, false, 40, 0), [3 * 8 + 2]);
    assert_eq!(centre(true, true, 10, 0), [3 * 8 + 2]);
    assert_eq!(centre(true, true, 10, 8), [3 * 8 + 3]);
    assert_eq!(centre(false, true, 20, 0), [3 * 8 + 4]);
}

#[test]
fn the_level_modes_bring_the_brush_to_the_height_of_the_first_tile() {
    let hill = || {
        let mut world = world(vec![]);
        world.set_tile(5, 5, 2, 0, GRASS);
        world.begin();
        world
    };
    let tiles = |_: &World, _| Some((5, 5));
    // Not contiguous: a fill at once, then nothing more to do.
    let (mut world, mut lines) = (hill(), Vec::new());
    let mut tool = modify(2, false, true);
    tool.press(&mut world, &habits, (0, 0), Some((5, 5)), &mut lines);
    assert_eq!(lines, ["shape 3", "cost 48"]);
    assert!(
        [(5, 5), (6, 5), (5, 6), (6, 6)]
            .iter()
            .all(|&(x, y)| point(&world, x, y) == 2)
    );
    lines.clear();
    tool.moved(
        &mut world,
        &habits,
        (0, 1),
        &tiles,
        Zoom::Normal,
        &mut lines,
    );
    assert!(lines.is_empty());
    // Contiguous: one step a move, and the ground around follows.
    let (mut world, mut lines) = (hill(), Vec::new());
    let mut tool = modify(2, true, true);
    tool.press(&mut world, &habits, (0, 0), Some((5, 5)), &mut lines);
    assert_eq!(point(&world, 6, 6), 1);
    tool.moved(
        &mut world,
        &habits,
        (0, 1),
        &tiles,
        Zoom::Normal,
        &mut lines,
    );
    assert_eq!(point(&world, 6, 6), 2);
    // A hill higher than the brush is cut down to the target in the same way.
    let mut world = self::world(vec![]);
    for (x, y) in [(3, 3), (4, 3), (3, 4), (4, 4)] {
        world.set_tile(x, y, 3, 0, GRASS);
    }
    world.set_tile(1, 1, 1, 0, GRASS);
    world.begin();
    let mut tool = modify(2, false, true);
    tool.press(&mut world, &habits, (0, 0), Some((1, 1)), &mut Vec::new());
    assert_eq!(point(&world, 1, 1), 1);
    assert_eq!(point(&world, 3, 3), 3, "the brush is on tiles (1..2, 1..2)");
}

#[test]
fn water_and_large_objects_move_as_a_whole_and_the_outside_of_the_zoo_does_not_move() {
    // A water tile on the brush rises with its shape; the land beside it takes the usual step.
    let mut world = world(vec![]);
    world.set_tile(3, 3, 0, 0x50, FRESH);
    world.set_tile(4, 3, 0, 0, FRESH);
    world.begin();
    let mut tool = modify(2, false, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    let at = |x: usize, y: usize| world.zoo.tiles[y * 8 + x];
    assert_eq!(
        (at(3, 3).elevation, at(3, 3).shape),
        (0, 0x50),
        "the slope is higher than the goal"
    );
    assert_eq!((at(4, 3).elevation, at(4, 3).shape), (1, 0));
    assert_eq!((at(3, 4).elevation, at(3, 4).shape), (1, 0));
    // A building of two tiles by two is rigid: the tile under it keeps its slope while the
    // flat ground beside it comes up to the goal.
    let mut world = self::world(vec![entity("hall", 4 * 64, 4 * 64, 0)]);
    assert!(world.sized(3 * 8 + 3) && world.big(3 * 8 + 3));
    world.set_tile(4, 4, 0, 0x01, GRASS);
    for (x, y) in [(5, 4), (4, 5), (5, 5)] {
        world.set_tile(x, y, -1, 0, GRASS);
    }
    world.begin();
    let mut tool = modify(2, false, false);
    drag(&mut tool, &mut world, (4, 4), 16, Zoom::Normal);
    let at = |x: usize, y: usize| {
        (
            world.zoo.tiles[y * 8 + x].elevation,
            world.zoo.tiles[y * 8 + x].shape,
        )
    };
    assert_eq!((at(4, 4), at(5, 4), at(5, 5)), ((0, 0x01), (0, 0), (0, 0)));
    // Outside the zoo nothing changes.
    let mut world = walled(vec![]);
    let mut tool = modify(2, true, false);
    assert!(drag(&mut tool, &mut world, (2, 2), 16, Zoom::Normal).is_empty());
    assert!(world.zoo.tiles.iter().all(|t| t.elevation == 0));
    // Inside the zoo the spreading does not cross into the outside.
    let lines = drag(&mut tool, &mut world, (5, 2), 16, Zoom::Normal);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(world.zoo.tiles[2 * 8 + 4].elevation == 0 && world.zoo.tiles[2 * 8 + 5].elevation == 1);
}

#[test]
fn the_ground_around_a_rigid_building_is_not_pulled_out_of_shape() {
    // A water tile beside the brush is not moved by the spreading, a rigid land tile moves whole.
    let mut world = world(vec![entity("hall", 5 * 64, 3 * 64, 0)]);
    world.set_tile(2, 3, 0, 0, FRESH);
    world.begin();
    let mut tool = modify(2, true, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    // The hall holds the tiles (4..5, 2..3): the brush at (3..4, 3..4) has the tile (4, 3) in it.
    assert!(world.sized(3 * 8 + 4));
    assert_eq!(world.zoo.tiles[3 * 8 + 2].elevation, 0, "the water stays");
}

#[test]
fn the_corners_stay_within_twelve_steps() {
    let mut world = world(vec![]);
    for t in 0..64 {
        world.set_tile(t % 8, t / 8, 12, 0, GRASS);
    }
    world.begin();
    let mut tool = modify(2, true, false);
    // The goal would be 13.
    assert!(drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal).is_empty());
    let mut world = self::world(vec![]);
    world.set_tile(3, 3, 11, 0, GRASS);
    world.begin();
    let mut tool = modify(1, true, false);
    drag(&mut tool, &mut world, (3, 3), 160, Zoom::Normal);
    assert!(
        world
            .zoo
            .tiles
            .iter()
            .all(|t| (-12..=12).contains(&t.elevation))
    );
}

#[test]
fn objects_on_a_changed_tile_take_their_height_from_the_ground_and_undo_gives_it_back() {
    let mut world = world(vec![on("tree", 3, 3), on("tree", 6, 6)]);
    let z = |w: &World, n: usize| w.zoo.entities[n].common.as_ref().unwrap().z;
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!((z(&world, 1), z(&world, 2)), (16, 0));
    // A slope: the tree stands in the middle of the tile.
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(z(&world, 1), 32);
    assert!(world.undo());
    assert_eq!(z(&world, 1), 16);
}

#[test]
fn whatever_the_brush_does_every_changed_tile_has_a_legal_shape_and_a_contiguous_ground_stays_joined()
 {
    const LEGAL: [u8; 19] = [
        0x00, 0x01, 0x04, 0x05, 0x10, 0x11, 0x14, 0x15, 0x19, 0x40, 0x41, 0x44, 0x45, 0x46, 0x50,
        0x51, 0x54, 0x64, 0x91,
    ];
    let mut seed = 12345u64;
    let mut next = |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    for round in 0..40 {
        let mut world = world(vec![]);
        let mut tool = modify(1 + next(4) as usize, true, false);
        for _ in 0..30 {
            let tile = (1 + next(6) as u32, 1 + next(6) as u32);
            let up = (next(5) as i32 - 2) * 16;
            drag(&mut tool, &mut world, tile, up, Zoom::Normal);
            assert!(
                world.zoo.tiles.iter().all(|t| LEGAL.contains(&t.shape)),
                "round {round}"
            );
            assert!(
                edges_joined(&world),
                "round {round}: a cliff in a contiguous edit"
            );
        }
    }
}

#[test]
fn absurd_pointers_and_elevations_change_nothing_and_never_panic() {
    for elevation in [i32::MIN, -13, 13, i32::MAX] {
        for (contiguous, level) in [(true, false), (false, false), (true, true), (false, true)] {
            let mut world = world(vec![]);
            world.set_tile(3, 3, elevation, 0, GRASS);
            world.begin();
            let mut tool = modify(2, contiguous, level);
            let mut lines = Vec::new();
            let tiles = |_: &World, _| Some((3, 3));
            tool.press(
                &mut world,
                &habits,
                (i32::MAX, i32::MIN),
                Some((3, 3)),
                &mut lines,
            );
            for at in [(i32::MIN, i32::MAX), (0, 0), (i32::MAX, i32::MAX), (-1, -1)] {
                for zoom in [Zoom::Out, Zoom::Normal, Zoom::In] {
                    tool.moved(&mut world, &habits, at, &tiles, zoom, &mut lines);
                }
            }
            tool.release();
            assert!(tool.cost(&world) >= 0.0);
        }
    }
}

#[test]
fn any_object_that_is_not_a_quarter_tile_makes_its_tile_rigid() {
    // A bench (2 by 1 half tiles) on the tile beside the brush moves whole instead of taking the
    // slope; a quarter-tile object does not.
    for (class, rigid) in [("bench", true), ("bed", true), ("tree", false)] {
        let mut world = world(vec![entity(class, 2 * 64 + 32, 3 * 64 + 32, 0)]);
        assert_eq!(world.sized(3 * 8 + 2), rigid, "{class}");
        let mut tool = modify(2, true, false);
        drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
        drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
        let tile = world.zoo.tiles[3 * 8 + 2];
        assert_eq!(tile.shape == 0, rigid, "{class}: {tile:?}");
    }
}

#[test]
fn water_beside_the_brush_moves_only_under_an_object_larger_than_a_tile_in_its_last_quarter() {
    // The hall (2 by 2 tiles) is centred on the corner of the tiles (2..3, 3..4); the bed holds
    // the water tile alone and is not larger than a tile.
    for (class, (x, y), moved) in [
        ("hall", (3 * 64, 4 * 64), true),
        ("bed", (2 * 64 + 32, 3 * 64 + 32), false),
    ] {
        let mut world = world(vec![entity(class, x, y, 0)]);
        world.set_tile(2, 3, 0, 0, FRESH);
        world.begin();
        assert_eq!(world.big(3 * 8 + 2), moved, "{class}");
        let mut tool = modify(1, true, false);
        drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
        assert_eq!(
            world.zoo.tiles[3 * 8 + 2].elevation,
            i32::from(moved),
            "{class}"
        );
    }
}

/// The tank walls round the tiles `(x0..=x1, y0..=y1)`.
fn tank(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Entity> {
    let mut walls = Vec::new();
    for x in x0..=x1 {
        walls.push(entity("twall", x * 64 + 32, y0 * 64 + 32, 0));
        walls.push(entity("twall", x * 64 + 32, y1 * 64 + 32, 4));
    }
    for y in y0..=y1 {
        walls.push(entity("twall", x0 * 64 + 32, y * 64 + 32, 6));
        walls.push(entity("twall", x1 * 64 + 32, y * 64 + 32, 2));
    }
    walls
}

#[test]
fn a_tile_of_a_tank_accepts_no_type() {
    // The tank of (4..5, 3..4); water or land, nothing is painted in it, beside it everything is.
    let mut world = world(tank(4, 3, 5, 4));
    let types = table();
    let accepts = |tx: usize, ty: usize, kind| accepts(&world, &habits, &types, ty * 8 + tx, kind);
    for kind in [GRASS, SAND, FRESH, SALT] {
        assert!(!accepts(4, 3, kind) && !accepts(5, 4, kind), "{kind}");
    }
    assert!(accepts(3, 3, SAND) && accepts(6, 4, FRESH));
    // The rule is that of the tank, not of the tile's own state.
    world.set_tile(4, 3, 0, 0, FRESH);
    assert!(!tool().accepts(&world, &habits, 3 * 8 + 4));
}

#[test]
fn the_tiles_of_a_tank_move_whole_and_all_together_in_a_step_that_is_not_contiguous() {
    let mut world = world(tank(4, 3, 5, 4));
    assert_eq!(world.carried(3 * 8 + 5), [28, 29, 36, 37]);
    // The brush is one tile of the tank: the other three come along, and the cost is four tiles.
    let mut tool = modify(1, false, false);
    let lines = drag(&mut tool, &mut world, (4, 3), 16, Zoom::Normal);
    assert_eq!(lines, ["shape 4", "cost 32"]);
    assert_eq!(heights(&world, (4, 5), (3, 4)), [1, 1, 1, 1]);
    assert_eq!(heights(&world, (3, 3), (3, 4)), [0, 0]);
    // A brush beside the tank takes nothing of it.
    let mut world = self::world(tank(4, 3, 5, 4));
    let mut tool = modify(1, false, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    assert_eq!(heights(&world, (4, 5), (3, 4)), [0, 0, 0, 0]);
}

#[test]
fn a_tank_beside_a_contiguous_step_is_not_pulled_out_of_shape() {
    // The brush is the tile (3, 3); a ground tile beside it takes the slope, a tank tile moves
    // whole.
    let mut world = world(tank(4, 3, 5, 4));
    let mut tool = modify(1, true, false);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    drag(&mut tool, &mut world, (3, 3), 16, Zoom::Normal);
    let flat = |x: usize, y: usize| world.zoo.tiles[y * 8 + x].shape == 0;
    assert!(flat(4, 3), "the tank tile keeps its shape");
    assert!(!flat(3, 2), "the ground tile beside it slopes");
}
