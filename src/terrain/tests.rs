//! The brush, the acceptance rules, the session and its cost, on synthetic maps.

use super::*;
use crate::render::order::Kind;
use crate::sprite::tests::Install;
use crate::world::Shape;
use crate::zoo::{Common, View, Zoo};

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
        "gate" => Some(Shape {
            footprint: (2, 2),
            gate: true,
            ..fixed
        }),
        "tree" | "buoy" | "pond" => Some(fixed),
        "animals" => Some(Shape {
            kind: Kind::Unit,
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
    tool.moved(&mut world, &habits, (5, 0), &tiles, &mut lines);
    assert!(lines.is_empty());
    // A long move is cut into ten points: the brush crosses the tiles 1, 2, 3 and 4.
    tool.moved(&mut world, &habits, (45, 0), &tiles, &mut lines);
    assert_eq!(tool.cost(&world), 30.0 * 12.0, "a strip of 6 by 2 tiles");
    for tx in 0..6 {
        assert_eq!(terrain_of(&world, tx, 0), SAND, "{tx}");
    }
    // The button went up: the brush follows without painting.
    tool.release();
    tool.moved(&mut world, &habits, (65, 0), &tiles, &mut lines);
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
