//! The pending object on synthetic maps.

use super::*;
use crate::world::Shape;
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

/// 0 tree (1 x 1), 1 bench (placement 2 x 1 over a footprint of 1 x 1), 2 dirt path, 3 stone
/// path, 4 fence, 5 restaurant (10 x 8), 6 tank filter, 7 buoy (only swims), 8 duck (land and
/// water), 9 raft (not a land type).
fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.types = vec![
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
            "[Characteristics/Integers]\ncLand=0\n",
            [1, 1, 1],
        ),
    ];
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
    let (kind, footprint, wall, gate) = match e.class.as_str() {
        "objects" => (Kind::Fixed, (1, 1), false, false),
        "paths" => (Kind::Path, (2, 2), false, false),
        "fences" => (Kind::Edge, (1, 1), true, false),
        "gate" => (Kind::Fixed, (2, 2), false, true),
        _ => return None,
    };
    Some(Shape {
        kind,
        footprint,
        fence: wall,
        wall,
        gate,
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
    pending(w, cat, &[false, true], ty, step, 0, (x, y))
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
    // map, which is the zoo message, and an object cannot hold the same entity twice in the list.
    assert_eq!(
        at(&w, &cat, 5, 0, 3 * 64, 4 * 64 - 2 * 64).unwrap().verdict,
        Verdict::Valid
    );
    let edge = at(&w, &cat, 5, 0, 5, 5).unwrap();
    assert_eq!(
        (edge.verdict, edge.message),
        (Verdict::Invalid, Some(NOT_IN_THE_ZOO))
    );
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
    let colour = |p: &Pending| p.extra(&tints).ghost.unwrap().tint;
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
    // A tree cannot stand in water, no message; a duck that swims can; a buoy and a raft need
    // water and say so on land.
    assert_eq!(verdict(land(0)), (Verdict::Valid, None));
    assert_eq!(verdict(wet(0)), (Verdict::Invalid, None));
    assert_eq!(verdict(wet(8)), (Verdict::Valid, None));
    assert_eq!(verdict(land(8)), (Verdict::Valid, None));
    for ty in [7, 9] {
        assert_eq!(
            verdict(land(ty)),
            (Verdict::Invalid, Some(NOT_ON_WATER)),
            "{ty}"
        );
        assert_eq!(verdict(wet(ty)), (Verdict::Valid, None), "{ty}");
    }
    // The first message stays: a held quadrant on a land tile is the water message.
    let held = world((0, 0), vec![entity("objects", "tree", 16, 16, 0)], |_| {});
    assert_eq!(
        at(&held, &cat, 7, 0, 5, 5).unwrap().message,
        Some(NOT_ON_WATER)
    );
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
    let outcome = place(&mut w, &mut money, &cat.types[0], &p, shape);
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
        place(&mut w, &mut money, &cat.types[0], &again, shape),
        Outcome::Refused(Some(IN_THE_WAY))
    );
    assert_eq!((money.cash, w.zoo.entities.len()), (49_975.0, 1));
    // Undo takes it back with the full price.
    assert!(w.undo());
    assert!(w.zoo.entities.is_empty());
}

#[test]
fn the_cash_must_cover_the_price_and_only_scenery_is_placed_here() {
    let cat = catalogue();
    let mut w = world((0, 0), vec![], |_| {});
    let p = at(&w, &cat, 0, 0, 10, 10).unwrap();
    // 24 is not enough for 25; exactly 25 is, and a free object is placed with no cash at all.
    let mut poor = zoo_money(24.0);
    assert_eq!(
        place(&mut w, &mut poor, &cat.types[0], &p, shape),
        Outcome::Refused(Some(NO_FUNDS))
    );
    assert_eq!((poor.cash, w.zoo.entities.len()), (24.0, 0));
    let mut exact = zoo_money(25.0);
    assert!(matches!(
        place(&mut w, &mut exact, &cat.types[0], &p, shape),
        Outcome::Placed(_)
    ));
    assert_eq!(exact.cash, 0.0);
    let free = Pending {
        price: 0.0,
        ..at(&w, &cat, 0, 0, 40, 40).unwrap()
    };
    let mut broke = zoo_money(-5.0);
    assert!(matches!(
        place(&mut w, &mut broke, &cat.types[0], &free, shape),
        Outcome::Placed(_)
    ));
    assert_eq!(broke.cash, -5.0);
    // An invalid place gives its message, whatever the cash; a path is not placed here.
    let held = at(&w, &cat, 0, 0, 10, 10).unwrap();
    assert_eq!(
        place(&mut w, &mut exact, &cat.types[0], &held, shape),
        Outcome::Refused(Some(IN_THE_WAY))
    );
    let path = at(&w, &cat, 2, 0, 3 * 64, 3 * 64).unwrap();
    assert_eq!(
        place(&mut w, &mut exact, &cat.types[2], &path, shape),
        Outcome::Refused(None)
    );
    // A NaN price is never booked.
    let nan = Pending {
        price: f32::NAN,
        ..at(&w, &cat, 0, 0, 3 * 64, 3 * 64).unwrap()
    };
    let mut money = zoo_money(100.0);
    place(&mut w, &mut money, &cat.types[0], &nan, shape);
    assert_eq!(money.cash, 100.0);
}
