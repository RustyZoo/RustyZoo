//! The pending object on synthetic maps.

use super::*;
use crate::world::{Shape, Step, TankKind};
use crate::zoo::{Common, Entity, Tile, View, Zoo};

#[test]
fn the_pending_object_keeps_its_facing_on_screen_when_the_view_turns() {
    // Step 0 faces direction 4 with the view unturned, then two more for each step.
    let faces: Vec<u32> = (0..4).map(|step| facing(step, 0)).collect();
    assert_eq!(faces, [4, 6, 0, 2]);
    for step in 0..4 {
        let screen = |view: u8| (facing(step, view) + 2 * u32::from(view)) % 8;
        for view in 1..4 {
            assert_eq!(screen(view), screen(0), "step {step} view {view}");
        }
    }
    // A turn of the view is a turn on the map the other way; steps and views wrap.
    assert_eq!(facing(0, 1), 2);
    assert_eq!(facing(7, 5), facing(3, 1));
}

#[test]
fn a_fence_takes_the_side_of_the_triangle_that_holds_the_point() {
    assert_eq!(side(10, 40), 6, "placing 10: lx < ly and lx + ly <= 64");
    assert_eq!(side(32, 0), 0);
    assert_eq!(side(63, 32), 2);
    assert_eq!(side(32, 63), 4);
    assert_eq!(side(0, 32), 6);
    // On the diagonals: lx == ly goes with the else branch.
    assert_eq!(side(20, 20), 0);
    assert_eq!(side(50, 50), 2);
}

#[test]
fn the_four_footprint_cases_snap_as_the_worked_examples_do() {
    // A quarter tile: the mouse at (600, 1000) is in tile (9, 15), local (24, 40).
    assert_eq!(snap(Class::Fixed, (600, 1000), (1, 1), 4), (592, 1008));
    // A path: the centre of the tile.
    assert_eq!(snap(Class::Path, (600, 1000), (2, 2), 0), (608, 992));
    // A fence: the middle of the side at the local point (10, 40) is side 6, local (0, 32).
    let (x, y) = (64 * 9 + 10, 64 * 15 + 40);
    assert_eq!(snap(Class::Fence, (x, y), (1, 1), side(10, 40)), (576, 992));
    assert_eq!(snap(Class::Fence, (x, y), (1, 1), 2), (64 * 9 + 63, 992));
    // Any other footprint, per axis: a restaurant of 10 x 8 half tiles, the mouse in tile (20, 30).
    assert_eq!(
        snap(Class::Fixed, (20 * 64 + 5, 30 * 64 + 60), (10, 8), 4),
        (1312, 1920)
    );
    // 2 and 3 half tiles go to the tile centre, 4 and 5 to the border, 1 to the quarter.
    for (f, local) in [(1, 48), (2, 32), (3, 32), (4, 0), (5, 0), (6, 32), (8, 0)] {
        assert_eq!(
            snap(Class::Fixed, (64 + 50, 64 + 50), (f, 2), 4).0,
            64 + local
        );
    }
    // A bench of 1 x 2: the quarter on one axis, the centre on the other.
    assert_eq!(
        snap(Class::Fixed, (64 + 50, 64 + 5), (1, 2), 4),
        (64 + 48, 64 + 32)
    );
    // A point on the negative side of the map and absurd footprints never panic.
    for f in [0, -3, i32::MIN, i32::MAX] {
        snap(Class::Fixed, (-5, -70), (f, f), 4);
    }
    snap(Class::Fixed, (i32::MAX, i32::MIN), (1, 1), 9);
}

fn ty(class: &'static str, name: &str, ini: &str, footprint: [i32; 3]) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint,
        height: 0,
    }
}

/// 0 tree (1 x 1), 11 iron fence, 12 elephant, 1 bench (placement 2 x 1 over a footprint of 1 x 1), 2 dirt path, 3 stone
/// x
fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let types = vec![
        ty(
            "objects",
            "tree",
            "[Characteristics/Integers]\ncPurchaseCost=25\n",
            [1, 1, 1],
        ),
        ty(
            "objects",
            "bench",
            "[Characteristics/Integers]\ncPlacementFootprintX=2\ncPurchaseCost=7\n",
            [1, 1, 1],
        ),
        ty(
            "paths",
            "dirt",
            "[Characteristics/Integers]\ncPurchaseCost=10\n",
            [2, 2, 1],
        ),
        ty(
            "paths",
            "stone",
            "[Characteristics/Integers]\ncPurchaseCost=20\n",
            [2, 2, 1],
        ),
        ty(
            "fences",
            "wire",
            "[Characteristics/Integers]\ncPurchaseCost=40\n",
            [1, 1, 1],
        ),
        ty("building", "restaurant", "", [10, 8, 1]),
        ty(
            "filter",
            "filter1",
            "[Characteristics/Integers]\ncPurchaseCost=100\n",
            [1, 1, 1],
        ),
        ty(
            "objects",
            "buoy",
            "[Characteristics/Integers]\ncSwims=1\ncOnlySwims=1\n",
            [1, 1, 1],
        ),
        ty(
            "objects",
            "duck",
            "[Characteristics/Integers]\ncSwims=1\ncLand=1\ncAutoRotate=1\n",
            [1, 1, 1],
        ),
        ty(
            "objects",
            "raft",
            "[Characteristics/Integers]\ncLand=0\ncSwims=1\n",
            [1, 1, 1],
        ),
        ty(
            "objects",
            "stage",
            "[Characteristics/Integers]\ncLand=0\n",
            [1, 1, 1],
        ),
        ty(
            "fences",
            "iron",
            "[Characteristics/Integers]\ncPurchaseCost=125\ncLife=10\ncDecayedLife=4\ncStrength=300\n",
            [1, 1, 1],
        ),
        ty("animals", "elephant", "", [6, 6, 1]),
        ty(
            "fences",
            "buoyrope",
            "[Characteristics/Integers]\ncPurchaseCost=50\ncSwims=1\ncLand=0\n",
            [1, 1, 1],
        ),
        ty(
            "fences",
            "chain",
            "[Characteristics/Integers]\ncPurchaseCost=50\ncLand=0\n",
            [1, 1, 1],
        ),
        ty(
            "building",
            "kiosk",
            "[Characteristics/Integers]\ncPurchaseCost=100\n",
            [2, 2, 1],
        ),
        ty(
            "building",
            "bouncy",
            "[Characteristics/Integers]\nwalkable=1\n",
            [2, 2, 1],
        ),
        ty(
            "tankwall",
            "glass",
            "[TankFence]\nx=1\n[Characteristics/Integers]\ncPurchaseCost=100\n",
            [1, 1, 1],
        ),
        ty(
            "tankwall",
            "show",
            "[TankFence]\nx=1\n[Characteristics/Integers]\ncPurchaseCost=150\ncIsShowFence=1\n",
            [1, 1, 1],
        ),
        ty(
            "tankwall",
            "swimglass",
            "[TankFence]\nx=1\n[Characteristics/Integers]\ncPurchaseCost=100\ncSwims=1\ncLand=0\n",
            [1, 1, 1],
        ),
    ];
    for t in types {
        cat.push(t);
    }
    cat
}

fn entity(class: &str, name: &str, x: i32, y: i32, rotation: u32) -> Entity {
    Entity {
        class: class.into(),
        type_name: name.into(),
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

/// How the entities of these tests take their place.
fn shape(e: &Entity) -> Option<Shape> {
    let (kind, footprint, fence, gate) = match e.class.as_str() {
        "objects" => (Kind::Fixed, (1, 1), false, false),
        "building" => (Kind::Fixed, (2, 2), false, false),
        "paths" => (Kind::Path, (2, 2), false, false),
        "fences" => (Kind::Edge, (1, 1), true, false),
        "tankwall" => (Kind::Wall, (1, 1), true, false),
        "gate" => (Kind::Fixed, (2, 2), false, true),
        "animals" | "helicopter" => (Kind::Unit, (1, 1), false, false),
        "gator" => (Kind::Unit, (3, 1), false, false),
        _ => return None,
    };
    Some(Shape {
        kind,
        footprint,
        fence,
        wall: fence && !matches!(e.type_name.as_str(), "wire" | "iron"),
        gate,
        gated: fence,
        tank: TankKind {
            wall: e.class == "tankwall",
            show: e.type_name == "show",
            ..TankKind::NONE
        },
    })
}

/// An 8 by 8 map of flat tiles with the entities.
fn world(entrance: (u32, u32), entities: Vec<Entity>, edit: impl Fn(&mut Vec<Tile>)) -> World {
    let flat = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let mut tiles = vec![flat; 64];
    edit(&mut tiles);
    World::new(
        Zoo {
            version: 0,
            locale: 0,
            expansion: 0,
            width: 8,
            height: 8,
            entrance,
            habitat_tiles: Vec::new(),
            habitat_gates: Vec::new(),
            tanks: Vec::new(),
            id_counter: 100,
            tiles,
            entities,
            view: View::default(),
        },
        shape,
    )
}

/// The pending object of the type `ty` for the point (`x`, `y`) of the world, view unturned.
fn at(w: &World, cat: &Catalogue, ty: usize, step: usize, x: i64, y: i64) -> Option<Pending> {
    pending(w, cat, &[false, true], &|_| None, true, ty, step, 0, (x, y))
}

#[test]
fn a_quarter_tile_object_is_valid_on_free_ground_and_costs_what_the_type_says() {
    let (cat, w) = (catalogue(), world((0, 0), vec![], |_| {}));
    let p = at(&w, &cat, 0, 0, 3 * 64 + 40, 2 * 64 + 10).unwrap();
    assert_eq!((p.x, p.y, p.z), (3 * 64 + 48, 2 * 64 + 16, 0));
    assert_eq!((p.tile, p.quadrant), ((3, 2), 1));
    assert_eq!(
        (p.verdict, p.message, p.amount()),
        (Verdict::Valid, None, -25)
    );
    assert_eq!(p.facing, facing(0, 0));
    assert!(p.blocking.is_empty());
    assert_eq!(cost_text("$".into(), p.amount()), "($25)");
    // Off the map and an unknown type give nothing.
    assert_eq!(at(&w, &cat, 0, 0, -1, 10), None);
    assert_eq!(at(&w, &cat, 0, 0, 8 * 64, 10), None);
    assert_eq!(at(&w, &cat, 99, 0, 10, 10), None);
}

#[test]
fn the_height_is_the_ground_height_under_the_snapped_position() {
    let cat = catalogue();
    // Shape 0x50 rises along x by lx / 4, from 16 * 2 at the corner.
    let w = world((0, 0), vec![], |t| {
        t[0].elevation = 2;
        t[0].shape = 0x50;
    });
    let p = at(&w, &cat, 0, 0, 5, 5).unwrap();
    assert_eq!((p.x, p.z), (16, 32 + 4));
    assert_eq!(at(&w, &cat, 0, 0, 40, 5).unwrap().z, 32 + 12, "local x 48");
}

#[test]
fn a_held_quadrant_makes_it_invalid_with_the_thing_in_the_way_and_its_message() {
    let cat = catalogue();
    let w = world((0, 0), vec![entity("objects", "tree", 16, 16, 0)], |_| {});
    let blocked = at(&w, &cat, 0, 0, 5, 5).unwrap();
    assert_eq!(blocked.verdict, Verdict::Invalid);
    assert_eq!(
        (blocked.message, blocked.blocking.clone()),
        (Some(IN_THE_WAY), vec![0])
    );
    assert_eq!(
        blocked.amount(),
        0,
        "the cost text of an invalid place is 0"
    );
    // The next quadrant is free.
    assert_eq!(at(&w, &cat, 0, 0, 40, 5).unwrap().verdict, Verdict::Valid);
    // A bench of 2 x 1 half tiles takes two quadrants along x: the one beside the tree is
    // taken, so is the first one, and it turns with the step (1 x 2 along y).
    let bench = at(&w, &cat, 1, 0, 5, 5).unwrap();
    assert_eq!(
        (bench.verdict, bench.blocking.clone()),
        (Verdict::Invalid, vec![0])
    );
    let turned = at(&w, &cat, 1, 1, 40, 5).unwrap();
    assert_eq!(turned.facing, 6);
    assert_eq!(
        turned.verdict,
        Verdict::Valid,
        "(48, 0) and (48, 32) are free"
    );
    assert_eq!(
        turned.y, 32,
        "two half tiles along y are a whole tile: its centre"
    );
}

#[test]
fn a_big_footprint_needs_every_quadrant_and_keeps_the_first_message() {
    let cat = catalogue();
    let w = world(
        (0, 0),
        vec![entity("objects", "tree", 5 * 64 + 16, 4 * 64 + 16, 0)],
        |_| {},
    );
    // 10 x 8 half tiles from the mouse in tile (3, 4): x from tile 1 to 5, y from tile 2 to 5
    // (swapped, so the tree on tile (5, 4) is inside).
    let p = at(&w, &cat, 5, 0, 3 * 64, 4 * 64).unwrap();
    assert_eq!((p.x, p.y), (3 * 64 + 32, 4 * 64));
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(IN_THE_WAY)));
    // Away from the tree and inside the map it is valid; near the edge a quadrant is off the
    // map, which fails without a message (only the object's own tile has the zoo message).
    assert_eq!(
        at(&w, &cat, 5, 0, 3 * 64, 4 * 64 - 2 * 64).unwrap().verdict,
        Verdict::Valid
    );
    let edge = at(&w, &cat, 5, 0, 5, 5).unwrap();
    assert_eq!((edge.verdict, edge.message), (Verdict::Invalid, None));
    // Turned by a step it takes 8 x 10 half tiles: tile 3 of the mouse leaves the tree out, tile
    // 4 puts it inside.
    assert_eq!(
        at(&w, &cat, 5, 1, 3 * 64, 4 * 64).unwrap().verdict,
        Verdict::Valid
    );
    let turned = at(&w, &cat, 5, 1, 4 * 64, 4 * 64).unwrap();
    assert_eq!(
        (turned.verdict, turned.blocking.clone()),
        (Verdict::Invalid, vec![0])
    );
    // The tile of the pointer decides which tiles it covers: asking twice agrees.
    assert_eq!(p, at(&w, &cat, 5, 0, 3 * 64, 4 * 64).unwrap());
}

#[test]
fn a_tile_outside_the_zoo_wall_gives_the_wall_message() {
    let cat = catalogue();
    // The entrance gate on tile (1, 4) faces west, so the exterior is west of it and behind the
    // wall on the east edge of column 4: the zoo is the columns from 5 on.
    let mut entities = vec![entity("gate", "", 96, 288, 6)];
    entities.extend((0..8).map(|row| entity("fences", "", 4 * 64 + 63, row * 64 + 32, 2)));
    let w = world((1, 4), entities, |_| {});
    assert!(w.in_zoo(w.tile_index(6, 3).unwrap()));
    let inside = at(&w, &cat, 0, 0, 6 * 64 + 10, 3 * 64 + 10).unwrap();
    assert_eq!(inside.verdict, Verdict::Valid);
    let outside = at(&w, &cat, 0, 0, 3 * 64 + 10, 3 * 64 + 10).unwrap();
    assert_eq!(
        (outside.verdict, outside.message),
        (Verdict::Invalid, Some(NOT_IN_THE_ZOO))
    );
    assert!(outside.blocking.is_empty());
    for ty in [2, 4, 5] {
        let p = at(&w, &cat, ty, 0, 3 * 64 + 10, 3 * 64 + 10).unwrap();
        assert_eq!(p.message, Some(NOT_IN_THE_ZOO), "type {ty}");
    }
}

#[test]
fn a_tank_filter_is_invalid_with_its_message_until_tanks_exist() {
    let cat = catalogue();
    let mut entities = vec![entity("gate", "", 96, 288, 6)];
    entities.extend((0..8).map(|row| entity("fences", "", 4 * 64 + 63, row * 64 + 32, 2)));
    let w = world((1, 4), entities, |_| {});
    let inside = at(&w, &cat, 6, 0, 6 * 64 + 10, 3 * 64 + 10).unwrap();
    assert_eq!(
        (inside.verdict, inside.message),
        (Verdict::Invalid, Some(NO_TANK))
    );
    assert_eq!(inside.amount(), 0);
    // Outside the zoo wall the zoo check comes first.
    let outside = at(&w, &cat, 6, 0, 3 * 64 + 10, 3 * 64 + 10).unwrap();
    assert_eq!(outside.message, Some(NOT_IN_THE_ZOO));
}

#[test]
fn a_path_on_a_path_and_a_fence_on_a_held_slot_replace_a_piece() {
    let cat = catalogue();
    let w = world(
        (0, 0),
        vec![
            entity("paths", "dirt", 3 * 64 + 32, 3 * 64 + 32, 0),
            entity("fences", "wire", 5 * 64 + 32, 5 * 64, 0),
        ],
        |_| {},
    );
    let (x, y) = (3 * 64 + 10, 3 * 64 + 10);
    // The same type costs nothing, another one the full price, a free tile the price.
    let same = at(&w, &cat, 2, 0, x, y).unwrap();
    assert_eq!((same.verdict, same.amount()), (Verdict::Overwrite, 0));
    let other = at(&w, &cat, 3, 0, x, y).unwrap();
    assert_eq!((other.verdict, other.amount()), (Verdict::Overwrite, -20));
    let free = at(&w, &cat, 2, 0, 10, 10).unwrap();
    assert_eq!((free.verdict, free.amount()), (Verdict::Valid, -10));
    assert_eq!((free.x, free.y, free.facing), (32, 32, 0));
    // A fence goes to the middle of the nearest side; on a held slot it replaces the piece.
    let held = at(&w, &cat, 4, 0, 5 * 64 + 32, 5 * 64 + 5).unwrap();
    assert_eq!((held.x, held.y, held.facing), (5 * 64 + 32, 5 * 64, 0));
    assert_eq!((held.verdict, held.amount()), (Verdict::Overwrite, 0));
    let next = at(&w, &cat, 4, 0, 5 * 64 + 32, 5 * 64 + 60).unwrap();
    assert_eq!(
        (next.facing, next.verdict, next.amount()),
        (4, Verdict::Valid, -40)
    );
    // The tint follows the verdict and the objects in the way turn red.
    let tints = Tints::default();
    let colour = |p: &Pending| p.extra(&tints).ghosts[0].tint;
    assert_eq!(colour(&free), tints.valid);
    assert_eq!(colour(&same), tints.overwrite);
    let tree = world((0, 0), vec![entity("objects", "tree", 16, 16, 0)], |_| {});
    let blocked = at(&tree, &cat, 0, 0, 5, 5).unwrap();
    assert_eq!(colour(&blocked), tints.invalid);
    assert_eq!(blocked.extra(&tints).tinted, [(0, tints.invalid)]);
}

#[test]
fn absurd_footprints_and_costs_never_panic() {
    let mut cat = catalogue();
    for (x, y) in [(0, 0), (-5, 7), (1_000_000, 3), (65, 65)] {
        for ini in [
            "[Characteristics/Integers]\ncPlacementFootprintX=-4\ncPlacementFootprintY=999999999999\n",
            "[Characteristics/Integers]\ncPlacementFootprintX=65\ncPlacementFootprintY=65\ncPurchaseCost=-5\n",
            "[Characteristics/Integers]\ncPlacementFootprintX=0\ncPurchaseCost=1e99\n",
            "[Characteristics/Integers]\ncPurchaseCost=-1e38\n",
            "[Characteristics/Integers]\ncPurchaseCost=nan\n",
        ] {
            cat.types[0] = ty("objects", "tree", ini, [1, 1, 1]);
            let w = world((0, 0), vec![], |_| {});
            for step in 0..4 {
                if let Some(p) = at(&w, &cat, 0, step, x, y) {
                    // A footprint that is empty, negative or over 64 half tiles is no place.
                    let odd = ["X=-4", "X=0", "=65"].iter().any(|k| ini.contains(k));
                    assert_eq!(p.verdict == Verdict::Invalid, odd, "{ini}");
                    let _ = p.amount();
                }
            }
        }
    }
    // A missing, zero and negative cost: a valid place costs what is left of it.
    let w = world((0, 0), vec![], |_| {});
    cat.types[0] = ty("objects", "tree", "", [1, 1, 1]);
    assert_eq!(at(&w, &cat, 0, 0, 5, 5).unwrap().amount(), 0);
    cat.types[0] = ty(
        "objects",
        "tree",
        "[Characteristics/Integers]\ncPurchaseCost=-5\n",
        [1, 1, 1],
    );
    assert_eq!(at(&w, &cat, 0, 0, 5, 5).unwrap().amount(), 5);
}

#[test]
fn the_tints_come_from_the_map_view_of_the_layout_and_default_to_the_shipped_ones() {
    let shipped = Tints::default();
    assert_eq!(
        (shipped.valid, shipped.overwrite, shipped.invalid),
        ([0, 225, 0], [225, 112, 0], [225, 0, 0])
    );
    let install = crate::sprite::tests::Install::new(
        "place-tints",
        &[(
            "ui/gamescrn.lyt",
            b"[MapView]\ntype=ZTMapView\nentityvalidcolor=1\nentityvalidcolor=2\nentityvalidcolor=3\n\
              entityinvalidcolor=9\nentityinvalidcolor=9\n"
                .to_vec(),
        )],
    );
    let tints = Tints::load(&crate::vfs::Vfs::open(&install.0).unwrap());
    // A colour with fewer than three values keeps the shipped one.
    assert_eq!(tints.valid, [1, 2, 3]);
    assert_eq!(tints.invalid, shipped.invalid);
    assert_eq!(tints.overwrite, shipped.overwrite);
    // A missing layout gives the shipped colours.
    let empty = crate::sprite::tests::Install::new("place-tints-none", &[]);
    assert_eq!(
        Tints::load(&crate::vfs::Vfs::open(&empty.0).unwrap()),
        shipped
    );
}

#[test]
fn more_than_a_quarter_tile_needs_flat_ground_at_one_height() {
    let cat = catalogue();
    let w = world((0, 0), vec![], |t| {
        // Tile (1, 0) slopes, tile (2, 0) is flat one step higher.
        t[1].shape = 0x50;
        t[2].elevation = 1;
    });
    // A tree stands on a slope; a bench (2 x 1) does not, and a slope gives no message.
    assert_eq!(
        at(&w, &cat, 0, 0, 64 + 10, 10).unwrap().verdict,
        Verdict::Valid
    );
    let bench = at(&w, &cat, 1, 0, 64 + 10, 10).unwrap();
    assert_eq!((bench.verdict, bench.message), (Verdict::Invalid, None));
    assert!(bench.blocking.is_empty());
    // Flat ground is fine, and so is a flat tile raised as a whole.
    assert_eq!(
        at(&w, &cat, 1, 0, 3 * 64 + 10, 10).unwrap().verdict,
        Verdict::Valid
    );
    assert_eq!(
        at(&w, &cat, 1, 0, 2 * 64 + 10, 10).unwrap().verdict,
        Verdict::Valid
    );
    // The restaurant spans tiles 1 to 5 by 0 to 3 here: the slope and the step are inside.
    let building = at(&w, &cat, 5, 0, 3 * 64, 2 * 64).unwrap();
    assert_eq!(
        (building.verdict, building.message),
        (Verdict::Invalid, None)
    );
    let row = at(&w, &cat, 5, 0, 3 * 64, 6 * 64).unwrap();
    assert_eq!(row.verdict, Verdict::Valid);
}

#[test]
fn a_footprint_over_a_tile_holds_no_fence_inside() {
    let cat = catalogue();
    // The restaurant at the mouse in tile (3, 4) covers tiles 1 to 5 by 2 to 5.
    let fence = |tx: i32, ty: i32, side: u32| {
        let (x, y) = [(32, 0), (63, 32), (32, 63), (0, 32)][side as usize / 2];
        entity("fences", "", tx * 64 + x, ty * 64 + y, side)
    };
    let check = |fences: Vec<Entity>| {
        let w = world((0, 0), fences, |_| {});
        let p = at(&w, &cat, 5, 0, 3 * 64, 4 * 64).unwrap();
        (p.verdict, p.message)
    };
    let invalid = (Verdict::Invalid, None);
    assert_eq!(check(vec![]), (Verdict::Valid, None));
    assert_eq!(check(vec![fence(3, 3, 2)]), invalid, "east side of (3, 3)");
    assert_eq!(
        check(vec![fence(4, 3, 6)]),
        invalid,
        "its west side from the other tile"
    );
    assert_eq!(check(vec![fence(2, 4, 4)]), invalid, "a south side");
    // On the border of the footprint a fence is not inside.
    assert_eq!(
        check(vec![
            fence(1, 2, 0),
            fence(5, 3, 2),
            fence(5, 5, 4),
            fence(1, 3, 6)
        ])
        .0,
        Verdict::Valid
    );
    // A footprint of one tile or less has no inside.
    let w = world((0, 0), vec![fence(3, 3, 2)], |_| {});
    assert_eq!(
        at(&w, &cat, 1, 0, 3 * 64 + 10, 3 * 64 + 10)
            .unwrap()
            .verdict,
        Verdict::Valid
    );
}

#[test]
fn land_and_water_decide_what_stands_where() {
    let cat = catalogue();
    let w = world((0, 0), vec![], |t| t[1].terrain = 1);
    let land = |ty| at(&w, &cat, ty, 0, 10, 10).unwrap();
    let wet = |ty| at(&w, &cat, ty, 0, 64 + 10, 10).unwrap();
    let verdict = |p: Pending| (p.verdict, p.message);
    // A tree cannot stand in water, no message; a duck that swims can.
    assert_eq!(verdict(land(0)), (Verdict::Valid, None));
    assert_eq!(verdict(wet(0)), (Verdict::Invalid, None));
    assert_eq!(verdict(wet(8)), (Verdict::Valid, None));
    assert_eq!(verdict(land(8)), (Verdict::Valid, None));
    // A buoy only swims: it fails on land without a message of its own. A raft is not a land
    // type: on land it says so, in water it is fine.
    assert_eq!(verdict(land(7)), (Verdict::Invalid, None));
    assert_eq!(verdict(wet(7)), (Verdict::Valid, None));
    assert_eq!(verdict(land(9)), (Verdict::Invalid, Some(NOT_ON_WATER)));
    assert_eq!(verdict(wet(9)), (Verdict::Valid, None));
    // A stage is not a land type and does not swim: land says so, water refuses it silently.
    assert_eq!(verdict(land(10)), (Verdict::Invalid, Some(NOT_ON_WATER)));
    assert_eq!(verdict(wet(10)), (Verdict::Invalid, None));
    // The water message of a type that is not a land type comes before the one in the way.
    let held = world((0, 0), vec![entity("objects", "tree", 16, 16, 0)], |_| {});
    assert_eq!(
        at(&held, &cat, 9, 0, 5, 5).unwrap().message,
        Some(NOT_ON_WATER)
    );
    // A buoy over a tree on land gives the one in the way.
    let p = at(&held, &cat, 7, 0, 5, 5).unwrap();
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(IN_THE_WAY)));
    // A terrain type the list does not know is land.
    let odd = world((0, 0), vec![], |t| t[0].terrain = 200);
    assert_eq!(at(&odd, &cat, 0, 0, 5, 5).unwrap().verdict, Verdict::Valid);
}

fn zoo_money(cash: f32) -> Money {
    Money {
        cash,
        ..Money::default()
    }
}

#[test]
fn a_valid_place_gets_the_object_and_takes_its_price_as_construction() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![], |_| {});
    let mut money = zoo_money(50_000.0);
    let p = at(&w, &cat, 0, 0, 3 * 64 + 40, 2 * 64 + 10).unwrap();
    let outcome = place(
        &mut w,
        &mut money,
        &cat.types[0],
        &p,
        &[false, true],
        Vec::new(),
        shape,
    );
    assert_eq!(outcome, Outcome::Placed(101));
    assert_eq!(money.cash, 49_975.0);
    assert_eq!(money.monthly[CONSTRUCTION][0], 25.0);
    let e = w.zoo.entities.last().unwrap();
    assert_eq!(
        (e.class.as_str(), e.type_name.as_str()),
        ("objects", "tree")
    );
    let c = e.common.as_ref().unwrap();
    assert_eq!(
        (c.x, c.y, c.z, c.rotation, c.id),
        (p.x, p.y, 0, p.facing, 101)
    );
    assert!(w.take_dirty());
    // The quarter tile is held now: the same place is in the way, nothing is charged.
    let again = at(&w, &cat, 0, 0, 3 * 64 + 40, 2 * 64 + 10).unwrap();
    assert_eq!(
        place(
            &mut w,
            &mut money,
            &cat.types[0],
            &again,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Refused(Some(IN_THE_WAY))
    );
    assert_eq!((money.cash, w.zoo.entities.len()), (49_975.0, 1));
    // Undo takes it back with the full price.
    assert!(!w.undo().is_empty());
    assert!(w.zoo.entities.is_empty());
}

#[test]
fn the_cash_must_cover_the_price_of_what_is_placed() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![], |_| {});
    let p = at(&w, &cat, 0, 0, 10, 10).unwrap();
    // 24 is not enough for 25; exactly 25 is, and a free object is placed with no cash at all.
    let mut poor = zoo_money(24.0);
    assert_eq!(
        place(
            &mut w,
            &mut poor,
            &cat.types[0],
            &p,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Refused(Some(NO_FUNDS))
    );
    assert_eq!((poor.cash, w.zoo.entities.len()), (24.0, 0));
    let mut exact = zoo_money(25.0);
    assert!(matches!(
        place(
            &mut w,
            &mut exact,
            &cat.types[0],
            &p,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Placed(_)
    ));
    assert_eq!(exact.cash, 0.0);
    let free = Pending {
        price: 0.0,
        ..at(&w, &cat, 0, 0, 40, 40).unwrap()
    };
    let mut broke = zoo_money(-5.0);
    assert!(matches!(
        place(
            &mut w,
            &mut broke,
            &cat.types[0],
            &free,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Placed(_)
    ));
    assert_eq!(broke.cash, -5.0);
    // An invalid place gives its message, whatever the cash.
    let held = at(&w, &cat, 0, 0, 10, 10).unwrap();
    assert_eq!(
        place(
            &mut w,
            &mut exact,
            &cat.types[0],
            &held,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Refused(Some(IN_THE_WAY))
    );
    // A NaN price is never booked.
    let nan = Pending {
        price: f32::NAN,
        ..at(&w, &cat, 0, 0, 3 * 64, 3 * 64).unwrap()
    };
    let mut money = zoo_money(100.0);
    place(
        &mut w,
        &mut money,
        &cat.types[0],
        &nan,
        &[false, true],
        Vec::new(),
        shape,
    );
    assert_eq!(money.cash, 100.0);
}

#[test]
fn the_water_of_the_terrain_types_is_listed_by_type_number() {
    let kind = |kind, water| terrain::Type {
        kind,
        cost: 0.0,
        water,
        icon: String::new(),
        help: 0,
        listed: true,
    };
    assert!(water(&[]).is_empty());
    assert_eq!(
        water(&[kind(0, false), kind(3, true)]),
        [false, false, false, true]
    );
    assert_eq!(water(&[kind(255, true)]).len(), 256);
}

#[test]
fn a_quadrant_outside_the_zoo_wall_fails_without_a_message() {
    let cat = catalogue();
    let mut entities = vec![entity("gate", "", 96, 288, 6)];
    entities.extend((0..8).map(|row| entity("fences", "", 4 * 64 + 63, row * 64 + 32, 2)));
    let w = world((1, 4), entities, |_| {});
    // The restaurant centred on tile (5, 3) takes tiles 3 to 7: its own tile is inside, two of
    // its columns are outside the wall.
    let p = at(&w, &cat, 5, 0, 5 * 64 + 10, 3 * 64).unwrap();
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, None));
    // The same on its own tile outside the wall gives the wall message.
    let p = at(&w, &cat, 5, 0, 3 * 64 + 10, 3 * 64).unwrap();
    assert_eq!(p.message, Some(NOT_IN_THE_ZOO));
}

#[test]
fn a_release_over_a_water_only_type_on_land_shows_the_water_message_last() {
    let cat = catalogue();
    let water = [false, true];
    let mut w = world(
        (0, 0),
        vec![entity("objects", "tree", 64 + 16, 16, 0)],
        |t| t[3].terrain = 1,
    );
    let mut money = zoo_money(100.0);
    let mut release = |w: &mut World, x: i64, y: i64| {
        let p = at(w, &cat, 7, 0, x, y).unwrap();
        place(w, &mut money, &cat.types[7], &p, &water, Vec::new(), shape)
    };
    // On land the check stores nothing, the second look gives the water message.
    assert_eq!(
        release(&mut w, 10, 10),
        Outcome::Refused(Some(NOT_ON_WATER))
    );
    // Over a tree the message of the check stays.
    assert_eq!(
        release(&mut w, 64 + 5, 5),
        Outcome::Refused(Some(IN_THE_WAY))
    );
    // On water it is placed.
    assert!(matches!(
        release(&mut w, 3 * 64 + 10, 10),
        Outcome::Placed(_)
    ));
    // A type that is not only-water on land has no second look: a bench on a slope has none.
    let mut slope = world((0, 0), vec![], |t| t[0].shape = 0x50);
    let p = at(&slope, &cat, 1, 0, 10, 10).unwrap();
    assert_eq!(
        place(
            &mut slope,
            &mut money,
            &cat.types[1],
            &p,
            &water,
            Vec::new(),
            shape
        ),
        Outcome::Refused(None)
    );
}

#[test]
fn the_first_tile_the_mouse_enters_decides_the_first_leg_of_a_path_drag() {
    // Placing 10: from (10, 10) the first move along x, then to (14, 13): x first, 8 pieces.
    let start = (10, 10);
    let mut leg = None;
    assert_eq!(path_run(start, start, &mut leg), [start]);
    assert_eq!(leg, None);
    assert_eq!(path_run(start, (11, 10), &mut leg), [(10, 10), (11, 10)]);
    assert_eq!(leg, Some(Leg::X));
    let l = path_run(start, (14, 13), &mut leg);
    assert_eq!(
        l,
        [
            (10, 10),
            (11, 10),
            (12, 10),
            (13, 10),
            (14, 10),
            (14, 11),
            (14, 12),
            (14, 13)
        ]
    );
    // Back on the start the mode is cleared; entering along y gives the mirrored L.
    path_run(start, start, &mut leg);
    assert_eq!(leg, None);
    path_run(start, (10, 11), &mut leg);
    let mirrored = path_run(start, (14, 13), &mut leg);
    assert_eq!(leg, Some(Leg::Y));
    assert_eq!(mirrored.len(), 8);
    let swapped: Vec<_> = l.iter().map(|&(x, y)| (y, x)).collect();
    assert_eq!(
        mirrored,
        [
            (10, 10),
            (10, 11),
            (10, 12),
            (10, 13),
            (11, 13),
            (12, 13),
            (13, 13),
            (14, 13)
        ]
    );
    assert_eq!(swapped[1], (10, 11), "x and y exchanged, around the start");
    // A diagonal step gives y first; negative directions walk backwards; a straight line is one leg.
    let mut leg = None;
    assert_eq!(
        path_run(start, (9, 9), &mut leg),
        [(10, 10), (10, 9), (9, 9)]
    );
    assert_eq!(leg, Some(Leg::Y));
    let mut leg = None;
    assert_eq!(
        path_run(start, (7, 10), &mut leg),
        [(10, 10), (9, 10), (8, 10), (7, 10)]
    );
    // The negative corner of the map and huge distances do not panic.
    let mut leg = None;
    path_run((0, 0), (-3, 2), &mut leg);
    path_run((0, 0), (0, 5000), &mut leg);
}

#[test]
fn a_path_takes_a_flat_tile_or_one_raised_side_only() {
    let cat = catalogue();
    for (shape, valid) in [
        (0x00, true),
        (0x05, true),
        (0x14, true),
        (0x41, true),
        (0x50, true),
        // A corner alone, a saddle, a steep shape and the other raised sides.
        (0x01, false),
        (0x04, false),
        (0x11, false),
        (0x55, false),
        (0x0f, false),
        (0xff, false),
    ] {
        let w = world((0, 0), vec![], |t| t[0].shape = shape);
        let p = at(&w, &cat, 2, 0, 10, 10).unwrap();
        assert_eq!(p.verdict != Verdict::Invalid, valid, "shape {shape:#x}");
        assert_eq!(p.message, None);
    }
}

/// The dirt of these tests: painting a water tile costs 5 and is possible on tile 3 only.
fn paintable(t: usize) -> Option<(u8, f32)> {
    (t == 3).then_some((3, 5.0))
}

#[test]
fn a_path_on_water_needs_dirt_to_be_painted_and_costs_the_paint_too() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![], |t| {
        t[3].terrain = 1;
        t[4].terrain = 1;
    });
    let on = |w: &World, tile: i64| {
        pending(
            w,
            &cat,
            &[false, true],
            &paintable,
            true,
            2,
            0,
            0,
            (tile * 64 + 8, 8),
        )
        .unwrap()
    };
    let p = on(&w, 3);
    assert_eq!(
        (p.verdict, p.paint, p.price, p.message),
        (Verdict::Valid, Some(3), 15.0, None)
    );
    // A tile the terrain tool would refuse is not valid, and says nothing.
    let refused = on(&w, 4);
    assert_eq!(
        (refused.verdict, refused.paint, refused.message),
        (Verdict::Invalid, None, None)
    );
    // Land is just the path.
    let land = on(&w, 1);
    assert_eq!(
        (land.verdict, land.paint, land.price),
        (Verdict::Valid, None, 10.0)
    );
    // Placing paints the tile first, then lays the path and takes both prices.
    let mut money = zoo_money(100.0);
    assert!(matches!(
        place(
            &mut w,
            &mut money,
            &cat.types[2],
            &p,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Placed(_)
    ));
    assert_eq!(money.cash, 85.0);
    assert_eq!(w.zoo.tiles[3].terrain, 3);
    assert!(w.path(3).is_some());
    // One undo gives the water back and takes the path away, and hands back both prices: the
    // path's in the removal, the paint's with the tile.
    let done = w.undo();
    assert_eq!((w.zoo.tiles[3].terrain, w.path(3)), (1, None));
    let prices: Vec<f32> = done
        .steps
        .iter()
        .map(|s| match s {
            Step::Removed(_, p) | Step::Tiles(_, p) => *p,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(prices, [10.0, 5.0]);
}

#[test]
fn a_path_on_a_path_replaces_it_and_pays_only_for_another_type() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![entity("paths", "dirt", 32, 32, 0)], |_| {});
    let mut money = zoo_money(100.0);
    let go = |w: &mut World, money: &mut Money, ty: usize| {
        let p = at(w, &cat, ty, 0, 10, 10).unwrap();
        place(
            w,
            money,
            &cat.types[ty],
            &p,
            &[false, true],
            Vec::new(),
            shape,
        )
    };
    // The same type: valid, free, and still one piece.
    assert!(matches!(go(&mut w, &mut money, 2), Outcome::Placed(_)));
    assert_eq!((money.cash, w.zoo.entities.len()), (100.0, 1));
    // Another type: the full price, and the old piece stays (its id, its place) with the new
    // type; the undo gives the old type and the price back.
    let id = w.zoo.entities[0].common.as_ref().unwrap().id;
    assert_eq!(go(&mut w, &mut money, 3), Outcome::Placed(id));
    assert_eq!((money.cash, w.zoo.entities.len()), (80.0, 1));
    assert_eq!(w.zoo.entities[0].type_name, "stone");
    let done = w.undo();
    assert!(matches!(done.steps[..], [Step::Retyped(_, p), ..] if p == 20.0));
    assert_eq!(w.zoo.entities[0].type_name, "dirt");
}

#[test]
fn the_pieces_the_cash_covers_are_valid_and_the_cost_is_their_sum() {
    let cat = catalogue();
    let entities = vec![entity("paths", "dirt", 3 * 64 + 32, 32, 0)];
    let w = world((0, 0), entities, |t| t[5].shape = 0x01);
    let piece = |ty: usize, tile: i64| at(&w, &cat, ty, 0, tile * 64 + 8, 8).unwrap();
    // 10, 10, a free replacement, 10, and a piece that is not valid anyway.
    let mut run = vec![
        piece(2, 0),
        piece(2, 1),
        piece(2, 3),
        piece(2, 4),
        piece(2, 5),
    ];
    assert_eq!(run_amount(&run), -30);
    afford(&mut run, &zoo_money(25.0));
    let verdicts: Vec<_> = run.iter().map(|p| p.verdict).collect();
    assert_eq!(
        verdicts,
        [
            Verdict::Valid,
            Verdict::Valid,
            Verdict::Overwrite,
            Verdict::Invalid,
            Verdict::Invalid
        ]
    );
    assert_eq!(run[3].message, None, "the cash walk stores no message");
    assert_eq!(run_amount(&run), -20);
    // The cash that covers the sum exactly changes nothing; a cheaper piece after a refused one
    // still fits.
    let mut exact = vec![piece(2, 0), piece(3, 1), piece(2, 2)];
    let mut covered = exact.clone();
    afford(&mut covered, &zoo_money(40.0));
    assert!(covered.iter().all(|p| p.verdict == Verdict::Valid));
    afford(&mut exact, &zoo_money(25.0));
    assert_eq!(exact[1].verdict, Verdict::Invalid);
    assert_eq!(exact[2].verdict, Verdict::Valid);
    assert_eq!(run_amount(&exact), -20);
    // No cash at all buys nothing; no pieces cost nothing.
    afford(&mut exact, &zoo_money(-1.0));
    assert_eq!(run_amount(&exact), 0);
    assert_eq!(run_amount(&[]), 0);
    // Every piece is drawn, the things in the way once.
    let tints = Tints::default();
    assert_eq!(extra_of(&run, &tints).ghosts.len(), 5);
}

#[test]
fn a_path_goes_under_objects_with_no_footprint_check_and_no_message() {
    let cat = catalogue();
    let mut w = world(
        (0, 0),
        vec![
            entity("objects", "tree", 16, 16, 0),
            entity("objects", "tree", 48, 48, 0),
        ],
        |_| {},
    );
    let p = at(&w, &cat, 2, 0, 10, 10).unwrap();
    assert_eq!(
        (p.verdict, p.message, p.amount(), p.blocking.is_empty()),
        (Verdict::Valid, None, -10, true)
    );
    let mut money = zoo_money(100.0);
    assert!(matches!(
        place(
            &mut w,
            &mut money,
            &cat.types[2],
            &p,
            &[false, true],
            Vec::new(),
            shape
        ),
        Outcome::Placed(_)
    ));
    // The trees stay, and the path is the tile's path.
    assert_eq!((w.zoo.entities.len(), money.cash), (3, 90.0));
    assert!(w.path(0).is_some() && w.quadrant(0, 0).is_some());
}

/// The piece of the fence drag from `start` along side `first`, the mouse at tile `cell` and the
/// point (`lx`, `ly`) of it.
fn run(
    start: (i32, i32),
    first: u32,
    cell: (i64, i64),
    local: (i64, i64),
) -> Vec<((i32, i32), u32)> {
    fence_run(start, first, (cell.0 * 64 + local.0, cell.1 * 64 + local.1))
}

#[test]
fn a_fence_drag_goes_along_the_side_of_the_press_then_turns_as_the_table_says() {
    // The worked examples of placing 10: side 0 of (10, 10), the mouse below and to the right.
    let right = run((10, 10), 0, (14, 12), (54, 32));
    let want = [
        (10, 10, 0),
        (11, 10, 0),
        (12, 10, 0),
        (13, 10, 0),
        (14, 10, 0),
    ]
    .into_iter()
    .chain([(14, 10, 2), (14, 11, 2), (14, 12, 2)]);
    assert_eq!(right, want.map(|(x, y, s)| ((x, y), s)).collect::<Vec<_>>());
    // Above the line the second leg lies on the far edge of the last column: a shift, no corner.
    let above = run((10, 10), 0, (14, 9), (54, 24));
    let want = [
        (10, 10, 0),
        (11, 10, 0),
        (12, 10, 0),
        (13, 10, 0),
        (14, 10, 0),
        (15, 9, 6),
    ];
    assert_eq!(above, want.map(|(x, y, s)| ((x, y), s)));
    // The other three starting sides, turned.
    let south = run((10, 10), 4, (14, 12), (54, 32));
    assert_eq!(south.len(), 7);
    assert_eq!(
        (south[4], south[5], south[6]),
        (((14, 10), 4), ((15, 11), 6), ((15, 12), 6))
    );
    let east = run((10, 10), 2, (12, 14), (32, 54));
    assert_eq!(east.len(), 7);
    assert_eq!(
        (east[4], east[5], east[6]),
        (((10, 14), 2), ((11, 15), 0), ((12, 15), 0))
    );
    let west = run((10, 10), 6, (7, 7), (52, 52));
    assert_eq!(west.len(), 7);
    assert_eq!(
        (west[3], west[4], west[6]),
        (((10, 7), 6), ((9, 6), 4), ((7, 6), 4))
    );
    // The mouse on the start tile: the piece alone, or a hook on the side the mouse is on.
    assert_eq!(run((10, 10), 0, (10, 10), (60, 20)), [((10, 10), 0)]);
    assert_eq!(
        run((10, 10), 0, (10, 10), (60, 50)),
        [((10, 10), 0), ((10, 10), 2)]
    );
    assert_eq!(
        run((10, 10), 0, (10, 10), (10, 50)),
        [((10, 10), 0), ((10, 10), 6)]
    );
    // A straight line along the side: no corner piece while the mouse is above the line.
    let line = run((10, 10), 0, (13, 10), (40, 20));
    assert_eq!(line.len(), 4);
    assert!(line.iter().all(|&(_, s)| s == 0));
    // A side that is not 0, 2, 4 or 6 is read modulo; a point far off the map stays finite.
    assert_eq!(run((10, 10), 8, (10, 10), (60, 20)), [((10, 10), 0)]);
    assert!(fence_run((0, 0), 2, (i64::MIN, i64::MAX)).len() < 40_000);
    assert!(fence_run((0, 0), 6, (i64::MAX, i64::MIN)).len() < 40_000);
}

/// A point inside tile (`tx`, `ty`) that picks the side `s` of it.
fn on_side(tx: i64, ty: i64, s: u32) -> (i64, i64) {
    let (x, y) = [(32, 5), (59, 32), (32, 59), (5, 32)][s as usize / 2];
    (tx * 64 + x, ty * 64 + y)
}

fn fence_at(
    w: &World,
    cat: &Catalogue,
    smart: bool,
    ty: usize,
    tx: i64,
    ty_: i64,
    s: u32,
) -> Pending {
    let (x, y) = on_side(tx, ty_, s);
    pending(w, cat, &[false, true], &|_| None, smart, ty, 0, 0, (x, y)).unwrap()
}

#[test]
fn a_fence_stands_on_its_own_tile_s_slot_and_the_zoo_wall_cannot_be_replaced() {
    let cat = catalogue();
    // A zoo wall piece on the east side of tile (4, 3), as a loaded map holds it.
    let w = world(
        (0, 0),
        vec![entity("fences", "", 4 * 64 + 63, 3 * 64 + 32, 2)],
        |_| {},
    );
    assert!(w.wall(w.tile_index(4, 3).unwrap(), 2));
    let on = |ty, tx, ty_, s| fence_at(&w, &cat, true, ty, tx, ty_, s);
    let wall = on(11, 4, 3, 2);
    assert_eq!(
        (wall.verdict, wall.message, wall.amount()),
        (Verdict::Invalid, None, 0)
    );
    // The same line is side 6 of the tile across: its own slot, free, at the full price.
    let across = on(11, 5, 3, 6);
    assert_eq!(
        (across.verdict, across.facing, across.amount()),
        (Verdict::Valid, 6, -125)
    );
    assert_eq!((across.x, across.y), (5 * 64, 3 * 64 + 32));
    // Another side of the same tile too.
    assert_eq!(on(11, 4, 3, 0).verdict, Verdict::Valid);
}

#[test]
fn a_fence_that_only_swims_is_refused_on_dry_land_and_placed_on_water() {
    let cat = catalogue();
    let w = world((0, 0), vec![], |t| t[1].terrain = 1);
    let on = |ty, tx| fence_at(&w, &cat, true, ty, tx, 0, 0);
    let verdict = |p: Pending| (p.verdict, p.message, p.amount());
    // The buoy rope fence (cLand 0, swims): grass says 10564 and costs nothing, water is fine.
    assert_eq!(
        verdict(on(13, 0)),
        (Verdict::Invalid, Some(NOT_ON_WATER), 0)
    );
    assert_eq!(verdict(on(13, 1)), (Verdict::Valid, None, -50));
    // A type with cLand 0 that does not swim, and the ordinary fence, have no water test.
    assert_eq!(on(14, 0).verdict, Verdict::Valid);
    assert_eq!(on(11, 0).verdict, Verdict::Valid);
    assert_eq!(on(11, 1).verdict, Verdict::Valid);
    // A piece the fence check refused gets no 10564 from this test.
    let held = world((0, 0), vec![entity("fences", "", 63, 32, 2)], |_| {});
    let p = fence_at(&held, &cat, true, 13, 0, 0, 2);
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, None));
}

#[test]
fn a_fence_between_the_tiles_of_one_object_and_on_the_entrance_is_refused() {
    let cat = catalogue();
    // An object of 2 x 2 half tiles on the corner of tiles (1, 1), (2, 1), (1, 2) and (2, 2), and
    // the entrance gate in the middle of tile (5, 5).
    let w = world(
        (0, 0),
        vec![
            entity("gate", "", 128, 128, 0),
            entity("gate", "", 5 * 64 + 32, 5 * 64 + 32, 0),
        ],
        |_| {},
    );
    let on = |tx, ty, s| fence_at(&w, &cat, true, 11, tx, ty, s);
    // Between (1, 2) and (2, 2) the object spans both tiles; so does it between (2, 1) and (2, 2).
    assert_eq!(on(2, 2, 6).verdict, Verdict::Invalid);
    assert_eq!(on(2, 2, 0).verdict, Verdict::Invalid);
    assert_eq!(on(1, 2, 2).verdict, Verdict::Invalid);
    // The outer sides of those tiles are free.
    assert_eq!(on(2, 2, 2).verdict, Verdict::Valid);
    assert_eq!(on(2, 2, 4).verdict, Verdict::Valid);
    // Tile (1, 1) holds the last quarter of the first object: the entrance quadrant.
    assert_eq!(on(1, 1, 0).verdict, Verdict::Invalid);
    // The entrance gate stands in the middle of its tile: all four sides are refused.
    for s in [0, 2, 4, 6] {
        assert_eq!(on(5, 5, s).verdict, Verdict::Invalid, "side {s}");
    }
    // A tile beside it is not.
    assert_eq!(on(6, 5, 6).verdict, Verdict::Valid);
}

#[test]
fn the_smart_fences_test_keeps_fences_off_big_bodies_and_large_placement_rectangles() {
    let cat = catalogue();
    // A bench (placement 2 x 1 half tiles over a footprint of 1 x 1) in the last quarter of
    // tile (3, 3), at the local point (48, 48), and an elephant (6 x 6) in tile (5, 5).
    let w = world(
        (0, 0),
        vec![
            entity("objects", "bench", 3 * 64 + 48, 3 * 64 + 48, 0),
            entity("animals", "elephant", 5 * 64 + 32, 5 * 64 + 32, 0),
        ],
        |_| {},
    );
    let on = |smart, tx, ty, s| fence_at(&w, &cat, smart, 11, tx, ty, s).verdict;
    // The fence position (3 * 64 + 32, 3 * 64 + 63) is 16 right and 15 below the bench: inside
    // its rectangle of 32 by 16. With the setting off, or away from the rectangle, it is fine.
    assert_eq!(on(true, 3, 3, 4), Verdict::Invalid);
    assert_eq!(on(false, 3, 3, 4), Verdict::Valid);
    // The side 0 of the tile below is 16 units from the bench in y: not strictly inside.
    assert_eq!(on(true, 3, 4, 0), Verdict::Valid);
    // Side 0 of its tile is 48 units above the bench, side 2 is 16 away in y: not strictly inside.
    assert_eq!(on(true, 3, 3, 0), Verdict::Valid);
    assert_eq!(on(true, 3, 3, 2), Verdict::Valid);
    assert_eq!(on(true, 4, 3, 6), Verdict::Valid);
    // The elephant covers its own tile and the ones next to it (6 / 2 >= 2 * 1), not two away.
    for (tx, ty) in [(5, 5), (6, 5), (4, 6), (6, 6)] {
        assert_eq!(on(true, tx, ty, 0), Verdict::Invalid, "tile {tx},{ty}");
        assert_eq!(on(false, tx, ty, 0), Verdict::Valid, "tile {tx},{ty} off");
    }
    for (tx, ty) in [(7, 5), (5, 3), (2, 7)] {
        assert_eq!(on(true, tx, ty, 0), Verdict::Valid, "tile {tx},{ty}");
    }
}

#[test]
fn the_confirm_sell_setting_reads_the_ini_file_and_defaults_to_on() {
    assert!(confirm_sell(b""));
    assert!(confirm_sell(b"[UI]\nconfirmSell=1\n"));
    assert!(!confirm_sell(b"[UI]\nconfirmSell=0\n"));
    assert!(!confirm_sell(b"[ui]\nCONFIRMSELL = 0\n"));
    assert!(confirm_sell(b"[UI]\nconfirmSell=-3\n"));
    assert!(confirm_sell(b"[UI]\nconfirmSell=2147483647\n"));
    assert!(confirm_sell(b"[UI]\nconfirmSell=yes\n"));
    assert!(confirm_sell(b"[UI]\nconfirmSell=\n"));
    assert!(confirm_sell(b"[Other]\nconfirmSell=0\n"));
    assert!(confirm_sell(b"\xff\x00\x01 not an ini"));
}

#[test]
fn the_smart_fences_setting_reads_the_ini_file_and_defaults_to_on() {
    assert!(smart_fences(b""));
    assert!(smart_fences(b"[UI]\nsmartFences=1\n"));
    assert!(!smart_fences(b"[UI]\nsmartFences=0\n"));
    assert!(!smart_fences(b"[ui]\nSMARTFENCES = 0\n"));
    assert!(smart_fences(b"[UI]\nsmartFences=-3\n"));
    assert!(smart_fences(b"[UI]\nsmartFences=lots\n"));
    assert!(smart_fences(b"[UI]\nsmartFences=\n"));
    assert!(smart_fences(b"[Other]\nsmartFences=0\n"));
    assert!(smart_fences(b"\xff\x00\x01 not an ini"));
}

#[test]
fn a_fence_on_a_held_slot_is_free_only_for_the_same_type_above_its_decayed_life() {
    let cat = catalogue();
    let old = |type_name: &str, life: Option<u32>| {
        let mut e = entity("fences", type_name, 5 * 64 + 32, 5 * 64, 0);
        e.common.as_mut().unwrap().fence = life.map(|life| Fence {
            strength: 300,
            life,
            timers: [0; 2],
            portal: false,
        });
        world((0, 0), vec![e], |_| {})
    };
    let price = |w: &World, ty| {
        let p = fence_at(w, &cat, true, ty, 5, 5, 0);
        (p.verdict, p.amount())
    };
    // Life 5 is above the decayed life 4: nothing to repair.
    assert_eq!(price(&old("iron", Some(5)), 11), (Verdict::Overwrite, 0));
    // At the decayed life, broken, or another type: the full price.
    assert_eq!(price(&old("iron", Some(4)), 11), (Verdict::Overwrite, -125));
    assert_eq!(price(&old("iron", Some(0)), 11), (Verdict::Overwrite, -125));
    assert_eq!(price(&old("wire", Some(9)), 11), (Verdict::Overwrite, -125));
    // A piece that stores no life (an older file) is healthy: the type's life, 10.
    assert_eq!(price(&old("iron", None), 11), (Verdict::Overwrite, 0));
    // The slot of a type without a life key: life 10, decayed life 0.
    assert_eq!(price(&old("wire", Some(1)), 4), (Verdict::Overwrite, 0));
    assert_eq!(price(&old("wire", Some(0)), 4), (Verdict::Overwrite, -40));
}

#[test]
fn a_fence_is_placed_healthy_replaces_the_old_piece_and_is_paid_once() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![], |_| {});
    let mut money = zoo_money(300.0);
    let p = fence_at(&w, &cat, true, 11, 3, 2, 2);
    assert_eq!(
        (p.x, p.y, p.facing, p.tile),
        (3 * 64 + 63, 2 * 64 + 32, 2, (3, 2))
    );
    let go = |w: &mut World, money: &mut Money, p: &Pending| {
        place(
            w,
            money,
            &cat.types[11],
            p,
            &[false, true],
            Vec::new(),
            shape,
        )
    };
    assert_eq!(go(&mut w, &mut money, &p), Outcome::Placed(101));
    assert_eq!(money.cash, 175.0);
    assert_eq!(money.monthly[CONSTRUCTION][0], 125.0);
    let c = w.zoo.entities[0].common.as_ref().unwrap();
    assert_eq!(
        (c.rotation, c.fence.map(|f| (f.strength, f.life))),
        (2, Some((300, 10)))
    );
    // The slot is held now; the same type above its decayed life replaces it for nothing, and
    // the old piece stays (same id, same place in the entity list), healthy again.
    let again = fence_at(&w, &cat, true, 11, 3, 2, 2);
    assert_eq!((again.verdict, again.amount()), (Verdict::Overwrite, 0));
    assert_eq!(go(&mut w, &mut money, &again), Outcome::Placed(101));
    assert_eq!((money.cash, w.zoo.entities.len()), (175.0, 1));
    let c = w.zoo.entities[0].common.as_ref().unwrap();
    assert_eq!((c.id, c.fence.map(|f| f.life)), (101, Some(10)));
    // A fence the cash cannot cover is refused with the message of insufficient funds.
    let mut poor = zoo_money(100.0);
    let other = fence_at(&w, &cat, true, 11, 3, 2, 0);
    assert_eq!(
        go(&mut w, &mut poor, &other),
        Outcome::Refused(Some(NO_FUNDS))
    );
    // A piece that is not valid shows its message, none for the fence check.
    let wall = world((0, 0), vec![entity("fences", "", 63, 32, 2)], |_| {});
    let refused = fence_at(&wall, &cat, true, 11, 0, 0, 2);
    let mut w2 = world((0, 0), vec![entity("fences", "", 63, 32, 2)], |_| {});
    assert_eq!(go(&mut w2, &mut money, &refused), Outcome::Refused(None));
    // Outside the zoo: the message of the zoo wall.
    let w3 = world((1, 4), vec![entity("gate", "", 96, 288, 6)], |_| {});
    let out = fence_at(&w3, &cat, true, 11, 0, 0, 0);
    assert_eq!(
        (out.verdict, out.message),
        (Verdict::Invalid, Some(NOT_IN_THE_ZOO))
    );
}

#[test]
fn a_unit_on_a_tile_of_the_footprint_whose_block_meets_it_is_in_the_way() {
    let cat = catalogue();
    let elephant = |x: i32, y: i32| entity("animals", "elephant", x, y, 0);
    let kiosk = |w: &World| at(w, &cat, 15, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    // The kiosk holds the four quadrants of tile (3, 2); its block is the tile.
    let w = world((0, 0), vec![elephant(3 * 64 + 20, 2 * 64 + 50)], |_| {});
    let p = kiosk(&w);
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(IN_THE_WAY)));
    assert_eq!(p.blocking, [0]);
    // The unit is highlighted with the objects: the staging tints it like them.
    let tints = Tints::default();
    assert_eq!(p.extra(&tints).tinted, [(0, tints.invalid)]);
    // A unit on a neighbouring tile is not on a tile of the footprint, so it is not in the way,
    // and neither is one that is gone (an entity of another class that has a shape).
    let w = world((0, 0), vec![elephant(4 * 64 + 20, 2 * 64 + 50)], |_| {});
    assert_eq!(kiosk(&w).verdict, Verdict::Valid);
    // A quarter tile tree is blocked by a unit in its own quadrant only.
    let w = world((0, 0), vec![elephant(3 * 64 + 50, 2 * 64 + 20)], |_| {});
    let tree = |x: i64, y: i64| at(&w, &cat, 0, 0, 3 * 64 + x, 2 * 64 + y).unwrap();
    assert_eq!(tree(40, 10).verdict, Verdict::Invalid);
    // The elephant's 1 x 1 footprint is a quarter tile around it: the far quadrant stays free.
    assert_eq!(tree(10, 50).verdict, Verdict::Valid);
}

#[test]
fn a_cliff_on_a_side_inside_the_footprint_refuses_the_place() {
    let cat = catalogue();
    // The restaurant at the mouse in tile (3, 4) covers tiles 1 to 5 by 2 to 5. Edge state 2 on
    // the east side of tile (3, 3): the states are stored for the directions 6, 4, 2, 0 from the
    // low bits up.
    let check = |tile: usize, bits: u8| {
        let w = world((0, 0), vec![], |t| t[tile].edges = bits);
        let p = at(&w, &cat, 5, 0, 3 * 64, 4 * 64).unwrap();
        (p.verdict, p.message)
    };
    let invalid = (Verdict::Invalid, None);
    assert_eq!(check(3 * 8 + 3, 2 << 4), invalid, "east of (3, 3)");
    assert_eq!(check(3 * 8 + 4, 3), invalid, "its west side: state 3");
    assert_eq!(
        check(3 * 8 + 3, 1 << 4).0,
        Verdict::Valid,
        "state 1 is joined"
    );
    // A cliff on the border of the footprint is not inside: the east side of tile (5, 3).
    assert_eq!(check(3 * 8 + 5, 2 << 4).0, Verdict::Valid);
}

#[test]
fn a_building_holds_its_quadrants_takes_its_colours_and_its_price() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![], |_| {});
    let mut money = zoo_money(50_000.0);
    let p = at(&w, &cat, 15, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    assert_eq!((p.x, p.y), (3 * 64 + 32, 2 * 64 + 32));
    assert_eq!((p.verdict, p.amount()), (Verdict::Valid, -100));
    let outcome = place(
        &mut w,
        &mut money,
        &cat.types[15],
        &p,
        &[false, true],
        vec![3, 0],
        shape,
    );
    assert_eq!(outcome, Outcome::Placed(101));
    assert_eq!(money.cash, 49_900.0);
    let e = w.zoo.entities.last().unwrap();
    assert_eq!(e.common.as_ref().unwrap().colors, [3, 0]);
    let held = |q: usize| w.quadrant(2 * 8 + 3, q);
    assert_eq!([held(0), held(1), held(2), held(3)], [Some(0); 4]);
    // The same click again finds the four quadrants held.
    let again = at(&w, &cat, 15, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    assert_eq!((again.verdict, again.blocking), (Verdict::Invalid, vec![0]));
    // A hill under it, or water on a land building, refuses the place.
    let hill = world((0, 0), vec![], |t| t[2 * 8 + 3].shape = 0x50);
    let p = at(&hill, &cat, 15, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, None));
}

#[test]
fn a_walkable_type_ignores_units_but_a_refused_place_collects_every_unit_on_its_tiles() {
    let cat = catalogue();
    let bouncy = |w: &World| at(w, &cat, 16, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    let elephant = || entity("animals", "elephant", 3 * 64 + 20, 2 * 64 + 50, 0);
    // The walkable type is placed over the unit it would otherwise meet.
    let w = world((0, 0), vec![elephant()], |_| {});
    let p = bouncy(&w);
    assert_eq!(
        (p.verdict, p.message, p.blocking.len()),
        (Verdict::Valid, None, 0)
    );
    // Refused for another reason, it shows the unit like any other object.
    let hill = world((0, 0), vec![elephant()], |t| t[2 * 8 + 3].shape = 0x50);
    let p = bouncy(&hill);
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(IN_THE_WAY)));
    assert_eq!(p.blocking, [0]);
    // A quarter tile tree on water is refused without a message of its own; the unit in the far
    // quadrant of its tile, whose rectangle does not meet the tree's, is collected all the same.
    let wet = |t: &mut Vec<Tile>| t[2 * 8 + 3].terrain = 1;
    let w = world((0, 0), vec![elephant()], wet);
    let p = at(&w, &cat, 0, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(IN_THE_WAY)));
    assert_eq!(p.blocking, [0]);
    // A unit on another tile is not collected, and a valid place collects nothing.
    let w = world(
        (0, 0),
        vec![entity("animals", "elephant", 4 * 64 + 20, 2 * 64 + 50, 0)],
        wet,
    );
    let p = at(&w, &cat, 0, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    assert_eq!(
        (p.verdict, p.message, p.blocking.len()),
        (Verdict::Invalid, None, 0)
    );
}

#[test]
fn the_helicopter_lets_objects_through_but_is_collected_when_the_place_is_refused() {
    let cat = catalogue();
    let heli = || vec![entity("helicopter", "heli", 3 * 64 + 20, 2 * 64 + 50, 0)];
    let kiosk = |w: &World| at(w, &cat, 15, 0, 3 * 64 + 10, 2 * 64 + 10).unwrap();
    let p = kiosk(&world((0, 0), heli(), |_| {}));
    assert_eq!(
        (p.verdict, p.message, p.blocking.len()),
        (Verdict::Valid, None, 0)
    );
    let p = kiosk(&world((0, 0), heli(), |t| t[2 * 8 + 3].shape = 0x50));
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(IN_THE_WAY)));
    assert_eq!(p.blocking, [0]);
}

#[test]
fn a_units_rectangle_is_its_footprint_centred_with_the_larger_side_at_odd_rotations() {
    let cat = catalogue();
    // A 3 by 1 unit on the tile of a quarter tile tree, whose rectangle is x and y 0..32 of the
    // tile; the unit's own is 96 by 32 at rotation 0, 32 by 96 at rotation 2 and 96 by 96 at an
    // odd rotation, centred on (x, y).
    let verdict = |x: i32, y: i32, rotation: u32| {
        let gator = entity("gator", "gator", 3 * 64 + x, 2 * 64 + y, rotation);
        let w = world((0, 0), vec![gator], |_| {});
        at(&w, &cat, 0, 0, 3 * 64 + 10, 2 * 64 + 10)
            .unwrap()
            .verdict
    };
    let (valid, invalid) = (Verdict::Valid, Verdict::Invalid);
    // Wide: it reaches the tree along x, not once turned a quarter.
    assert_eq!([verdict(56, 16, 0), verdict(56, 16, 4)], [invalid; 2]);
    assert_eq!([verdict(56, 16, 2), verdict(56, 16, 6)], [valid; 2]);
    // Square at an odd rotation, whichever side it was long on.
    assert_eq!([verdict(60, 50, 0), verdict(60, 50, 2)], [valid; 2]);
    assert_eq!(
        [verdict(60, 50, 1), verdict(60, 50, 3), verdict(60, 50, 7)],
        [invalid; 3]
    );
    // Touching is not meeting: at y 48 the unit's near edge is y 32, the tree's far edge.
    assert_eq!(verdict(63, 16, 0), invalid);
    assert_eq!(verdict(63, 48, 0), valid);
}

/// The walls of class `tankwall` and type `name` round the tiles `(2..=3, 2..=3)`.
fn tank_walls(name: &str) -> Vec<Entity> {
    let mut walls = Vec::new();
    for t in 2..=3 {
        walls.push(entity("tankwall", name, t * 64 + 32, 2 * 64 + 32, 0));
        walls.push(entity("tankwall", name, t * 64 + 32, 3 * 64 + 32, 4));
        walls.push(entity("tankwall", name, 2 * 64 + 32, t * 64 + 32, 6));
        walls.push(entity("tankwall", name, 3 * 64 + 32, t * 64 + 32, 2));
    }
    walls
}

#[test]
fn only_a_tank_wall_of_the_kind_of_the_tank_stands_in_a_tank() {
    let cat = catalogue();
    // Glass walls make a normal tank, show walls a show tank; the line between its two upper
    // tiles is free.
    for (name, own, other) in [("glass", 17, 18), ("show", 18, 17)] {
        let w = world((7, 7), tank_walls(name), |_| {});
        assert!(w.tank_at(2 * 8 + 2).is_some());
        let inside = |ty| fence_at(&w, &cat, true, ty, 2, 2, 2);
        assert_eq!(inside(own).verdict, Verdict::Valid, "{name}");
        // The wall of the other kind and a fence that is no tank wall are refused, no message.
        for ty in [other, 11, 4] {
            let p = inside(ty);
            assert_eq!(
                (p.verdict, p.message),
                (Verdict::Invalid, None),
                "{name} {ty}"
            );
        }
        // Outside the tank everything stands; across the tank's wall too.
        for ty in [other, own, 11] {
            assert_eq!(
                fence_at(&w, &cat, true, ty, 5, 5, 2).verdict,
                Verdict::Valid
            );
        }
    }
}

#[test]
fn a_fence_that_only_swims_is_accepted_on_a_tank_tile() {
    let cat = catalogue();
    let w = world((7, 7), tank_walls("glass"), |_| {});
    // A tank wall that only swims (cLand 0, swims) stands on the grass under a tank, and on dry
    // land says 10564.
    let p = fence_at(&w, &cat, true, 19, 2, 2, 2);
    assert_eq!((p.verdict, p.message), (Verdict::Valid, None));
    let p = fence_at(&w, &cat, true, 19, 5, 5, 2);
    assert_eq!(
        (p.verdict, p.message),
        (Verdict::Invalid, Some(NOT_ON_WATER))
    );
    // The buoy rope fence is no tank wall: not in a tank.
    assert_eq!(
        fence_at(&w, &cat, true, 13, 2, 2, 2).verdict,
        Verdict::Invalid
    );
    // A buoy (cOnlySwims, a scenery object) is placed in the tank and refused on land; the
    // water message of the second look does not count a tank tile as land.
    let p = at(&w, &cat, 7, 0, 2 * 64 + 20, 2 * 64 + 20).unwrap();
    assert_eq!((p.verdict, p.message), (Verdict::Valid, None));
    let p = at(&w, &cat, 7, 0, 5 * 64 + 20, 5 * 64 + 20).unwrap();
    assert_eq!(p.verdict, Verdict::Invalid);
    // A raft (cLand 0) may stand in the tank without the water message.
    let p = at(&w, &cat, 9, 0, 2 * 64 + 20, 2 * 64 + 20).unwrap();
    assert_eq!((p.verdict, p.message), (Verdict::Valid, None));
}
