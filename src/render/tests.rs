use super::order::{Item, Kind, Object, Scene, paint_order, project, sort_key};
use super::*;
use crate::sprite::tests::Install;
use crate::zoo::Tile;

fn flat() -> Tile {
    Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    }
}

/// A flat map of `width` by `height` tiles.
fn flat_map(width: usize, height: usize) -> Vec<Tile> {
    vec![flat(); width * height]
}

/// Marks the edge of tile `(x, y)` in map direction `d` (0, 2, 4 or 6) as a step, on both sides.
fn step(tiles: &mut [Tile], width: usize, x: usize, y: usize, d: usize) {
    let (dx, dy) = [(0, -1), (0, 0), (1, 0), (0, 0), (0, 1), (0, 0), (-1, 0)][d];
    let other = (x as i32 + dx, y as i32 + dy);
    let bits = |d: usize| 2 * (3 - d / 2);
    tiles[y * width + x].edges |= 2 << bits(d);
    tiles[other.1 as usize * width + other.0 as usize].edges |= 2 << bits((d + 4) % 8);
}

fn unit(x: i32, y: i32) -> Object {
    Object {
        kind: Kind::Unit,
        x,
        y,
        z: 0,
        facing: 0,
        footprint: (1, 1),
        late: false,
    }
}

/// A unit in the middle of tile `(x, y)`.
fn unit_on(x: i32, y: i32) -> Object {
    unit(64 * x + 32, 64 * y + 32)
}

#[test]
fn projection_matches_the_worked_examples() {
    // The screen-left corner of tile (74, 0) of a 75 by 75 map at the four rotations: the
    // corner is the map corner (0,0), (0,64), (64,64), (64,0) of the tile.
    let tile = (64 * 74, 0);
    for (r, corner, want) in [
        (0, (0, 0), (2368, -1184)),
        (1, (0, 64), (2336, 1200)),
        (2, (64, 64), (-2432, 1184)),
        (3, (64, 0), (-2400, -1200)),
    ] {
        assert_eq!(
            project(r, tile.0 + corner.0, tile.1 + corner.1, 0),
            want,
            "rotation {r}"
        );
    }
    // One height unit is one pixel up; division truncates toward zero.
    assert_eq!(project(0, 0, 0, 16), (0, -16));
    assert_eq!(project(0, -3, 0, 0), (-1, 0));
    assert_eq!(project(0, 0, -3, 0), (-1, 0));
    assert_eq!(project(0, 0, 7, 0), (3, 1));
}

#[test]
fn tiles_are_sorted_by_row_then_position() {
    // Tile (1, 2) at each rotation: (row, pos).
    assert_eq!(sort_key(0, 1, 2), (-1, 2));
    assert_eq!(sort_key(1, 1, 2), (2, 1));
    assert_eq!(sort_key(2, 1, 2), (1, -2));
    assert_eq!(sort_key(3, 1, 2), (-2, -1));
}

#[test]
fn a_map_without_steps_paints_all_ground_then_the_objects_row_by_row() {
    for r in 0..4u8 {
        let objects: Vec<Object> = (0..9).map(|t| unit_on(t % 3, t / 3)).collect();
        let scene = Scene::new(3, 3, r, &flat_map(3, 3), objects);
        let items = paint_order(&scene);
        let mut want: Vec<usize> = (0..9).collect();
        want.sort_by_key(|&t| sort_key(r, (t % 3) as i32, (t / 3) as i32));
        let ground: Vec<Item> = want.iter().map(|&t| Item::Terrain(t)).collect();
        // Object `t` stands on tile `t`.
        let things: Vec<Item> = want.iter().map(|&t| Item::Object(t)).collect();
        assert_eq!(items[..9], ground, "rotation {r}");
        assert_eq!(items[9..], things, "rotation {r}");
    }
}

#[test]
fn objects_of_a_tile_go_by_view_y_then_x_and_the_top_quadrant_goes_behind() {
    // Rotation 0: a unit higher up the screen than a one quarter tile object in the top quadrant.
    // By position the unit would be first; the top quadrant object is forced behind it.
    let tree = Object {
        kind: Kind::Fixed,
        x: 48,
        y: 16,
        z: 0,
        facing: 0,
        footprint: (1, 1),
        late: false,
    };
    let high = unit(48, 0);
    let low = unit(16, 48);
    let scene = Scene::new(1, 1, 0, &flat_map(1, 1), vec![low, tree, high]);
    let items = paint_order(&scene);
    assert_eq!(
        items,
        [
            Item::Terrain(0),
            Item::Object(1),
            Item::Object(2),
            Item::Object(0)
        ]
    );
    // The same tree in the left quadrant has no forced place: the unit above it goes first.
    let tree = Object {
        x: 16,
        y: 16,
        ..tree
    };
    let scene = Scene::new(1, 1, 0, &flat_map(1, 1), vec![low, tree, high]);
    let items = paint_order(&scene);
    assert_eq!(
        items[1..],
        [Item::Object(2), Item::Object(1), Item::Object(0)]
    );
}

#[test]
fn a_big_object_is_painted_once_on_its_front_tile_and_in_front_of_the_units_there() {
    // A building two tiles by two, its centre on the corner shared by tiles (0,0) to (1,1).
    let building = Object {
        kind: Kind::Fixed,
        x: 64,
        y: 64,
        z: 0,
        facing: 0,
        footprint: (4, 4),
        late: false,
    };
    for r in 0..4u8 {
        let mut objects = vec![building];
        objects.extend([(0, 0), (1, 0), (0, 1), (1, 1)].map(|(x, y)| unit_on(x, y)));
        let scene = Scene::new(3, 3, r, &flat_map(3, 3), objects);
        let items = paint_order(&scene);
        let at = |o: usize| items.iter().position(|&i| i == Item::Object(o)).unwrap();
        let big: Vec<usize> = (0..items.len())
            .filter(|&i| items[i] == Item::Object(0))
            .collect();
        assert_eq!(big.len(), 1, "painted once at rotation {r}");
        // The front tile is the one whose centre is lowest on the screen.
        let front = (1..5)
            .max_by_key(|&o| project(r, objects_xy(o).0, objects_xy(o).1, 0).1)
            .unwrap();
        assert_eq!(
            at(0),
            at(front) + 1,
            "rotation {r}: after the units of the front tile"
        );
        for o in (1..5).filter(|&o| o != front) {
            assert!(at(o) < at(0), "rotation {r}");
        }
    }
}

/// The position of unit `o` of the previous test (1 to 4).
fn objects_xy(o: usize) -> (i32, i32) {
    let (x, y) = [(0, 0), (1, 0), (0, 1), (1, 1)][o - 1];
    (64 * x + 32, 64 * y + 32)
}

/// Checks that the ground and units of the tiles with `y < 2` of a 5 by 5 map are all painted
/// before the ground of any other tile, at rotation 0, and that everything is painted once.
fn assert_group_behind_first(tiles: &[Tile], extra: Vec<Object>) {
    let mut objects: Vec<Object> = (0..25).map(|t| unit_on(t % 5, t / 5)).collect();
    objects.extend(extra);
    let scene = Scene::new(5, 5, 0, tiles, objects);
    let items = paint_order(&scene);
    let tile = |item: &Item| match *item {
        Item::Terrain(t) | Item::Object(t) if t < 25 => Some(t),
        _ => None,
    };
    let behind = |item: &Item| tile(item).is_some_and(|t| t / 5 < 2);
    let front = |item: &Item| tile(item).is_some_and(|t| t / 5 >= 2);
    let last_behind = items.iter().rposition(behind).unwrap();
    let first_front = items.iter().position(front).unwrap();
    assert!(last_behind < first_front);
    for t in 0..25 {
        for item in [Item::Terrain(t), Item::Object(t)] {
            assert_eq!(items.iter().filter(|&&i| i == item).count(), 1);
        }
    }
}

#[test]
fn a_step_ends_a_group_and_the_group_behind_is_painted_whole_first() {
    // At rotation 0 the tiles with y < 2 are behind those with y >= 2.
    let mut tiles = flat_map(5, 5);
    for x in 0..5 {
        step(&mut tiles, 5, x, 2, 0);
    }
    assert_group_behind_first(&tiles, Vec::new());
}

#[test]
fn a_fence_ends_a_group_like_a_step() {
    let fences = (0..5)
        .map(|x| Object {
            kind: Kind::Edge,
            x: 64 * x + 32,
            y: 128,
            z: 0,
            facing: 0,
            footprint: (1, 1),
            late: false,
        })
        .collect();
    assert_group_behind_first(&flat_map(5, 5), fences);
}

#[test]
fn a_fence_stands_on_the_edge_its_facing_names_and_is_painted_once() {
    let fence = |x: i32, y: i32, facing: u32| Object {
        kind: Kind::Edge,
        x,
        y,
        z: 0,
        facing,
        footprint: (1, 1),
        late: false,
    };
    // The upper-left and the lower-right edge of tile (1,1) at rotation 0, and a second fence on
    // the first edge, which finds the slot taken: the original drops it at load.
    let objects = vec![
        fence(64 + 32, 64, 0),
        fence(64 + 32, 64 + 63, 4),
        fence(64 + 32, 64, 0),
    ];
    let scene = Scene::new(2, 2, 0, &flat_map(2, 2), objects);
    assert_eq!(scene.edge_object(3, 0), Some(0));
    assert_eq!(scene.edge_object(3, 4), Some(1));
    let items = paint_order(&scene);
    for item in [Item::Edge(0), Item::Edge(1)] {
        assert_eq!(items.iter().filter(|&&i| i == item).count(), 1, "{item:?}");
    }
    assert!(!scene.is_placed(2));
    assert!(
        !items
            .iter()
            .any(|i| matches!(i, Item::Object(2) | Item::Edge(2)))
    );
    // A fence with an odd rotation takes no slot either.
    let scene = Scene::new(2, 2, 0, &flat_map(2, 2), vec![fence(64 + 32, 64, 1)]);
    assert!(!scene.is_placed(0));
}

#[test]
fn an_object_takes_exactly_its_quadrants_or_none() {
    let thing = |footprint: (i32, i32), x: i32, y: i32, facing: u32| Object {
        kind: Kind::Fixed,
        x,
        y,
        z: 0,
        facing,
        footprint,
        late: false,
    };
    let quadrants = |objects: Vec<Object>| {
        let scene = Scene::new(4, 4, 0, &flat_map(4, 4), objects);
        (0..scene.objects.len())
            .map(|o| (scene.is_placed(o), scene.quadrants(o)))
            .collect::<Vec<_>>()
    };
    // An odd footprint on a quadrant corner takes the quadrant that holds the point and one on
    // each side, never one more.
    assert_eq!(quadrants(vec![thing((3, 1), 128, 32, 0)]), [(true, 3)]);
    // The footprint swaps for every facing that is not 0 or 4: here 2 by 4 half tiles.
    assert_eq!(quadrants(vec![thing((4, 2), 32, 160, 1)]), [(true, 8)]);
    assert_eq!(quadrants(vec![thing((4, 2), 32, 160, 5)]), [(true, 8)]);
    assert_eq!(quadrants(vec![thing((4, 2), 64, 128, 4)]), [(true, 8)]);
    // Partly off the map: dropped. On a quadrant that is held: dropped, the first one wins.
    assert_eq!(quadrants(vec![thing((2, 2), 0, 0, 0)]), [(false, 0)]);
    let quarter = thing((1, 1), 16, 16, 0);
    assert_eq!(
        quadrants(vec![quarter, thing((2, 2), 32, 32, 0), quarter]),
        [(true, 1), (false, 0), (false, 0)]
    );
    let scene = Scene::new(4, 4, 0, &flat_map(4, 4), vec![quarter, quarter]);
    assert_eq!(paint_order(&scene).len(), 16 + 1);
}

#[test]
fn a_run_along_a_row_stops_at_the_tiles_of_a_big_object() {
    // A building two tiles by two on the tiles x 2 and 3, y 1 and 2 of a flat 4 by 4 map.
    let building = Object {
        kind: Kind::Fixed,
        x: 192,
        y: 128,
        z: 0,
        facing: 0,
        footprint: (4, 4),
        late: false,
    };
    let scene = Scene::new(4, 4, 0, &flat_map(4, 4), vec![building]);
    let t = |x: usize, y: usize| Item::Terrain(y * 4 + x);
    assert_eq!(
        paint_order(&scene),
        [
            t(3, 0),
            t(2, 0),
            t(1, 0),
            t(0, 0),
            t(3, 1),
            t(3, 2),
            t(2, 1),
            t(2, 2),
            Item::Object(0),
            t(3, 3),
            t(2, 3),
            t(1, 1),
            t(1, 2),
            t(1, 3),
            t(0, 1),
            t(0, 2),
            t(0, 3),
        ]
    );
}

#[test]
fn ambient_animals_are_overlays_painted_after_the_objects_of_their_tile() {
    let at = |kind| Object {
        kind,
        x: 16,
        y: 16,
        z: 0,
        facing: 0,
        footprint: (1, 1),
        late: false,
    };
    // The bird is first in the file but holds no quadrant and is painted after the scenery.
    let scene = Scene::new(
        1,
        1,
        0,
        &flat_map(1, 1),
        vec![at(Kind::Overlay), at(Kind::Fixed)],
    );
    assert_eq!(
        paint_order(&scene),
        [Item::Terrain(0), Item::Object(1), Item::Object(0)]
    );
}

#[test]
fn an_object_that_paints_last_goes_after_the_others_of_its_tile() {
    let unit = |y: i32, late: bool| Object {
        late,
        ..unit(32, y)
    };
    // Rotation 0: the smaller y is higher on the screen and goes first, unless it paints last.
    let scene = Scene::new(
        1,
        1,
        0,
        &flat_map(1, 1),
        vec![unit(10, true), unit(50, false), unit(30, false)],
    );
    assert_eq!(
        paint_order(&scene)[1..],
        [Item::Object(2), Item::Object(1), Item::Object(0)]
    );
}

/// A frame of 2 pixels, palette index 1, with the given offsets.
fn frame_file() -> Vec<u8> {
    let mut out = Vec::new();
    for field in [1u16, 2, 0, 0, 0] {
        out.extend(field.to_le_bytes());
    }
    out.extend([1, 0, 2, 1, 1]);
    out
}

#[test]
fn a_map_is_drawn_from_the_cache_on_a_black_background() {
    let mut palette = 2u32.to_le_bytes().to_vec();
    palette.extend([0, 0, 0, 255, 200, 100, 50, 255]);
    let mut members = vec![
        (
            "terrain/tiletex.cfg".to_string(),
            b"[ttGrass]\ntype=0\n".to_vec(),
        ),
        ("ztats/00.pal".to_string(), palette),
    ];
    for variation in 0..9 {
        members.push((format!("ztats/0000{variation:02x}.lle"), frame_file()));
    }
    let members: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|(n, c)| (n.as_str(), c.clone()))
        .collect();
    let install = Install::new("render", &members);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = Zoo {
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 2,
        height: 2,
        entrance: (0, 0),
        habitat_count: 0,
        id_counter: 0,
        tiles: flat_map(2, 2),
        entities: Vec::new(),
    };
    let (image, summary) = MapRenderer::new(&vfs, &catalogue).render(&zoo, 0).unwrap();
    assert_eq!(
        summary,
        Summary {
            width: 2,
            height: 2,
            tiles_cached: 4,
            ..Summary::default()
        }
    );
    // The canvas holds the four diamonds: 128 by 65 pixels, tile (0,0)'s ground picture with its
    // top left at (32, 0) of the view, which is (32, 32) here.
    assert_eq!((image.width, image.height), (128, 65));
    let at = |x: usize, y: usize| image.pixels[(y * 128 + x) * 4..][..4].to_vec();
    assert_eq!(at(32, 32), [200, 100, 50, 255]);
    assert_eq!(at(33, 32), [200, 100, 50, 255]);
    assert_eq!(at(0, 0), [0, 0, 0, 255]);
}

/// A palette file of the given colours (R, G, B), with the fourth byte 255.
fn palette_file(colors: &[[u8; 3]]) -> Vec<u8> {
    let mut out = (colors.len() as u32).to_le_bytes().to_vec();
    for [r, g, b] in colors {
        out.extend([*r, *g, *b, 255]);
    }
    out
}

/// A map of one flat tile with the given entities, which all stand in its middle.
fn tile_with(entities: &[(&str, &str, &str)]) -> Zoo {
    let entity = |(class, type_name, subtype): &(&str, &str, &str)| crate::zoo::Entity {
        class: class.to_string(),
        type_name: type_name.to_string(),
        subtype: subtype.to_string(),
        body_size: None,
        common: Some(crate::zoo::Common {
            x: 32,
            y: 32,
            z: 0,
            rotation: 0,
            id: 1,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
        }),
    };
    Zoo {
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 1,
        height: 1,
        entrance: (0, 0),
        habitat_count: 0,
        id_counter: 0,
        tiles: flat_map(1, 1),
        entities: entities.iter().map(entity).collect(),
    }
}

/// An install whose terrain cache holds `frame` for the one grass tile.
fn cache_install(name: &str, frame: Vec<u8>) -> Install {
    let mut palette = 2u32.to_le_bytes().to_vec();
    palette.extend([0, 0, 0, 255, 200, 100, 50, 255]);
    let mut members = vec![
        (
            "terrain/tiletex.cfg".to_string(),
            b"[ttGrass]\ntype=0\n".to_vec(),
        ),
        ("ztats/00.pal".to_string(), palette),
    ];
    for variation in 0..9 {
        members.push((format!("ztats/0000{variation:02x}.lle"), frame.clone()));
    }
    let members: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|(n, c)| (n.as_str(), c.clone()))
        .collect();
    Install::new(name, &members)
}

#[test]
fn a_window_of_the_view_is_drawn_from_its_top_left_view_space_point() {
    // The ground picture of the one tile is two pixels, its top left at the view-space point
    // (32, 0).
    let install = cache_install("render-window", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = tile_with(&[]);
    let draw = |window| {
        let options = Options {
            window: Some(window),
            ..Options::default()
        };
        MapRenderer::new(&vfs, &catalogue).render_with(&zoo, 0, &options)
    };
    let (image, summary) = draw((31, -1, 4, 3)).unwrap();
    assert_eq!((image.width, image.height), (4, 3));
    let (black, ground) = ([0, 0, 0, 255], [200, 100, 50, 255]);
    let rows: Vec<&[[u8; 4]]> = image.pixels.as_chunks::<4>().0.chunks(4).collect();
    assert_eq!(rows[0], [black; 4]);
    assert_eq!(rows[1], [black, ground, ground, black]);
    assert_eq!(rows[2], [black; 4]);
    // The summary still counts the whole map.
    assert_eq!(summary.tiles_cached, 1);
    // A window far from the map, with extreme positions, is black and never panics.
    let (image, _) = draw((i32::MIN, i32::MAX, 3, 2)).unwrap();
    assert_eq!(image.pixels, [0, 0, 0, 255].repeat(6));
    // A window without area or beyond the pixel limit is an error.
    assert!(draw((0, 0, 0, 5)).is_err());
    assert!(draw((0, 0, u32::MAX, u32::MAX)).is_err());
}

#[test]
fn windows_at_the_extremes_of_view_space_never_overflow() {
    // The picture has offsets, which are subtracted from a position that is already far away.
    let mut frame = Vec::new();
    for field in [1u16, 2, 5, 7, 0] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 2, 1, 1]);
    let install = cache_install("render-window-extreme", frame);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = tile_with(&[]);
    for x in [i32::MIN, 0, i32::MAX] {
        for y in [i32::MIN, 0, i32::MAX] {
            let options = Options {
                window: Some((x, y, 2, 2)),
                ..Options::default()
            };
            let _ = MapRenderer::new(&vfs, &catalogue).render_with(&zoo, 0, &options);
        }
    }
}

#[test]
fn no_units_leaves_out_animals_guests_staff_and_ambient_animals() {
    use crate::sprite::tests::{sized, sprite_file};
    let mut frame = Vec::new();
    for field in [1u16, 1, 0, 0, 0] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 1, 1]);
    let ani = b"[animation]\ndir0 = a\ndir1 = b\nanimation = N\nx1 = 3\n".to_vec();
    let definition = b"[Animations]\nidle = idle\n".to_vec();
    let install = Install::new(
        "render-no-units",
        &[
            (
                "scenery.cfg",
                b"[objects]\nfoliage\n[foliage]\ntree = tree.ai\n".to_vec(),
            ),
            ("guests.cfg", b"[guests]\nman = man.ai\n".to_vec()),
            ("ambient.cfg", b"[ambient]\nbird = bird.ai\n".to_vec()),
            ("tree.ai", definition.clone()),
            ("man.ai", definition.clone()),
            ("bird.ai", definition),
            ("objects/tree/idle/idle.ani", ani.clone()),
            ("guests/man/idle/idle.ani", ani.clone()),
            ("ambient/ambient/bird/idle/idle.ani", ani),
            ("a/b/N", sprite_file(None, 1, &[sized(&frame)])),
            (
                "animals/Lion/lion.pal",
                palette_file(&[[255, 0, 255], [1, 2, 3]]),
            ),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = tile_with(&[
        ("objects", "foliage", "tree"),
        ("guests", "man", ""),
        ("ambient", "ambient", "bird"),
    ]);
    let drawn = |no_units| {
        let options = Options {
            no_units,
            ..Options::default()
        };
        let (_, summary) = MapRenderer::new(&vfs, &catalogue)
            .render_with(&zoo, 0, &options)
            .unwrap();
        (summary.objects_drawn, summary.objects_missing)
    };
    assert_eq!(drawn(false), (3, 0));
    assert_eq!(drawn(true), (1, 0));
}

/// A small deterministic pseudo-random sequence.
fn random(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

#[test]
fn every_tile_and_object_is_painted_exactly_once_on_a_messy_map() {
    let mut seed = 7;
    for round in 0..40 {
        let size = 12;
        let mut tiles = flat_map(size, size);
        for _ in 0..30 {
            let (x, y) = (
                random(&mut seed) as usize % size,
                random(&mut seed) as usize % size,
            );
            let d = 2 * (random(&mut seed) as usize % 4);
            let (dx, dy) = [(0, -1), (0, 0), (1, 0), (0, 0), (0, 1), (0, 0), (-1, 0)][d];
            if x as i32 + dx >= 0
                && y as i32 + dy >= 0
                && x as i32 + dx < size as i32
                && y as i32 + dy < size as i32
            {
                step(&mut tiles, size, x, y, d);
            }
        }
        let mut objects = Vec::new();
        for _ in 0..120 {
            let kind = [
                Kind::Unit,
                Kind::Fixed,
                Kind::Edge,
                Kind::Path,
                Kind::Overlay,
            ][random(&mut seed) as usize % 5];
            objects.push(Object {
                kind,
                x: random(&mut seed) as i32 % (64 * size as i32),
                y: random(&mut seed) as i32 % (64 * size as i32),
                z: 0,
                facing: random(&mut seed) as u32 % 8,
                footprint: (
                    1 + random(&mut seed) as i32 % 6,
                    1 + random(&mut seed) as i32 % 6,
                ),
                late: random(&mut seed).is_multiple_of(4),
            });
        }
        let r = (round % 4) as u8;
        let scene = Scene::new(size, size, r, &tiles, objects);
        let items = paint_order(&scene);
        let count = |item: Item| items.iter().filter(|&&i| i == item).count();
        for t in 0..size * size {
            assert_eq!(count(Item::Terrain(t)), 1, "round {round}, tile {t}");
        }
        for (o, object) in scene.objects.iter().enumerate() {
            let item = match object.kind {
                _ if !scene.is_placed(o) => continue,
                Kind::Edge
                    if scene.edge_object(
                        scene.tile_at(object.x, object.y).unwrap(),
                        object.facing as usize,
                    ) == Some(o) =>
                {
                    Item::Edge(o)
                }
                Kind::Path
                    if scene.path_of(scene.tile_at(object.x, object.y).unwrap()) == Some(o) =>
                {
                    Item::Path(o)
                }
                _ => Item::Object(o),
            };
            assert_eq!(count(item), 1, "round {round}, object {o}");
        }
    }
}

#[test]
fn absurd_heights_and_positions_give_an_error_or_a_picture_never_a_panic() {
    let install = Install::new("render-absurd", &[]);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut tiles = flat_map(3, 3);
    for (i, tile) in tiles.iter_mut().enumerate() {
        tile.elevation = [i32::MAX, i32::MIN, 1_000_000, -7, 5][i % 5];
        tile.shape = (i * 37) as u8;
        tile.edges = (i * 91) as u8;
        tile.terrain = (i * 53) as u8;
    }
    let entity = |class: &str, x: i32, y: i32, z: i32| crate::zoo::Entity {
        class: class.into(),
        type_name: "x".into(),
        subtype: "y".into(),
        body_size: None,
        common: Some(crate::zoo::Common {
            x,
            y,
            z,
            rotation: u32::MAX,
            id: 1,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
        }),
    };
    let zoo = Zoo {
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 3,
        height: 3,
        entrance: (0, 0),
        habitat_count: 0,
        id_counter: 0,
        tiles,
        entities: vec![
            entity("objects", i32::MAX, i32::MIN, i32::MAX),
            entity("fences", -1, 5, i32::MIN),
            entity("paths", 100, 100, 0),
        ],
    };
    for r in 0..8 {
        let _ = MapRenderer::new(&vfs, &catalogue).render(&zoo, r);
    }
}

#[test]
fn a_tile_the_cache_lacks_is_rendered_at_run_time() {
    // A 6 by 6 red texture, 24 bits, rows bottom up.
    let mut texture = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 6, 0, 24, 0];
    texture.extend([0, 0, 255].repeat(36));
    let install = Install::new(
        "render-ground",
        &[
            (
                "terrain/tiletex.cfg",
                b"[ttGrass]\ntype=0\ntexture=terrain/g.tga\n".to_vec(),
            ),
            ("terrain/g.tga", texture),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = Zoo {
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 2,
        height: 2,
        entrance: (0, 0),
        habitat_count: 0,
        id_counter: 0,
        tiles: flat_map(2, 2),
        entities: Vec::new(),
    };
    let (image, summary) = MapRenderer::new(&vfs, &catalogue).render(&zoo, 0).unwrap();
    assert_eq!(
        (
            summary.tiles_cached,
            summary.tiles_rendered,
            summary.ground_missing
        ),
        (0, 4, 0)
    );
    assert_eq!(
        image.pixels[(32 * image.width as usize + 32) * 4..][..4],
        [255, 0, 0, 255]
    );
}

#[test]
fn cliffs_are_drawn_from_the_root_bitmaps_when_the_converted_copies_are_missing() {
    use crate::bitmap::tests::{RED, WHITE, bmp24};
    let mut fringe = String::new();
    for piece in (1..=5).chain(11..=15) {
        fringe += &format!("[{piece}]\nimage=fringe\\f.bmp\n");
    }
    let bitmap = bmp24(&[vec![WHITE, RED], vec![WHITE, WHITE]], false);
    let mut tiles = flat_map(2, 2);
    tiles[0].elevation = 2;
    step(&mut tiles, 2, 0, 0, 2);
    step(&mut tiles, 2, 0, 0, 4);
    let zoo = Zoo {
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 2,
        height: 2,
        entrance: (0, 0),
        habitat_count: 0,
        id_counter: 0,
        tiles,
        entities: Vec::new(),
    };
    let cliffs = |members: &[(&str, Vec<u8>)]| {
        let install = Install::new("render-bitmaps", members);
        let vfs = Vfs::open(&install.0).unwrap();
        let catalogue = Catalogue::load(&vfs).unwrap();
        let (_, summary) = MapRenderer::new(&vfs, &catalogue).render(&zoo, 0).unwrap();
        (summary.cliffs_drawn, summary.cliffs_missing)
    };
    let config = ("fringe.cfg", fringe.into_bytes());
    let without = cliffs(std::slice::from_ref(&config));
    assert!(without.1 > 0 && without.0 == 0, "{without:?}");
    let with = cliffs(&[config, ("fringe/f.bmp", bitmap)]);
    assert_eq!(with, (without.1, 0));
}

#[test]
fn a_recoloured_building_is_drawn_with_the_colours_its_record_stores() {
    use crate::sprite::tests::{sized, sprite_file};
    const MARKER: [u8; 3] = [255, 0, 255];
    // One row of three pixels with the palette values 1, 2 and 3, anchored at their left end.
    let mut frame = Vec::new();
    for field in [1u16, 3, 0, 0, 0] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 3, 1, 2, 3]);
    let sprite = sprite_file(None, 1, &[sized(&frame)]);
    let definition = |flag: &str, fullpal: &str| {
        format!(
            "[Characteristics/Integers]\ncIsColorReplaced = {flag}\n[Animations]\nidle = idle\n\
             [colorrep]\ncolor = cr_color\nreplace = cr_part1\ndefaultpal = Pals\\Blue.pal\n\
             [cr_color]\nncolors = 2\nfullpal = {fullpal}\ncolorpal = color.pal\n"
        )
        .into_bytes()
    };
    let members = |flag: &str, fullpal: &str| {
        vec![
            ("bldg.cfg", b"[building]\nshed = shed.ai\n".to_vec()),
            ("shed.ai", definition(flag, fullpal)),
            // A group of two colours after two fixed ones (16 in the real data).
            (
                "building.ai",
                b"[cr_part1]\nncolors = 2\npal = pals/red.pal\npal = pals/blue.pal\n".to_vec(),
            ),
            (
                "objects/shed/idle/idle.ani",
                b"[animation]\ndir0 = a\ndir1 = b\nanimation = N\nx1 = 3\n".to_vec(),
            ),
            ("a/b/N", sprite.clone()),
            (
                "animals/Lion/lion.pal",
                palette_file(&[MARKER, [1, 1, 1], [2, 2, 2], [3, 3, 3]]),
            ),
            ("color.pal", palette_file(&[MARKER, [9, 9, 9]])),
            (
                "pals/red.pal",
                palette_file(&[MARKER, [100, 0, 0], [101, 0, 0]]),
            ),
            (
                "pals/blue.pal",
                palette_file(&[MARKER, [0, 0, 200], [0, 0, 201]]),
            ),
        ]
    };
    // The colours of the three pixels, from the left of the sprite file (this view is mirrored).
    let draw = |name: &str, flag: &str, fullpal: &str, stored: &[u8]| {
        let install = Install::new(name, &members(flag, fullpal));
        let vfs = Vfs::open(&install.0).unwrap();
        let catalogue = Catalogue::load(&vfs).unwrap();
        let zoo = Zoo {
            version: 106,
            locale: 1033,
            expansion: 0,
            width: 1,
            height: 1,
            entrance: (0, 0),
            habitat_count: 0,
            id_counter: 0,
            tiles: flat_map(1, 1),
            entities: vec![crate::zoo::Entity {
                class: "building".into(),
                type_name: "building".into(),
                subtype: "shed".into(),
                body_size: None,
                common: Some(crate::zoo::Common {
                    x: 32,
                    y: 32,
                    z: 0,
                    rotation: 0,
                    id: 1,
                    name: String::new(),
                    colors: stored.to_vec(),
                    fence: None,
                }),
            }],
        };
        let (image, summary) = MapRenderer::new(&vfs, &catalogue).render(&zoo, 0).unwrap();
        assert_eq!(summary.objects_drawn, 1, "{summary}");
        let mut lit: Vec<[u8; 3]> = image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2]])
            .filter(|p| *p != [0, 0, 0])
            .collect();
        lit.reverse();
        lit
    };
    // The palette of the sprite file is the one `fullpal` names, written with other letters and
    // another separator.
    let full = "Animals\\LION\\lion.pal";
    // Nothing stored: the default variant, which the definition names the same way.
    let blue = [[9, 9, 9], [0, 0, 200], [0, 0, 201]];
    let red = [[9, 9, 9], [100, 0, 0], [101, 0, 0]];
    assert_eq!(draw("recolour-default", "1", full, &[]), blue);
    assert_eq!(draw("recolour-first", "1", full, &[0]), red);
    assert_eq!(draw("recolour-second", "1", full, &[1, 7]), blue);
    // A choice beyond the variants, or a type that does not ask for it, keeps the plain palette.
    let plain = [[2, 2, 2], [3, 3, 3]];
    assert_eq!(
        draw("recolour-beyond", "1", full, &[5]),
        [[9, 9, 9], plain[0], plain[1]]
    );
    assert_eq!(
        draw("recolour-off", "0", full, &[0]),
        [[1, 1, 1], plain[0], plain[1]]
    );
    // A sprite whose palette is not the `fullpal` one keeps its own fixed colours; only the
    // group takes the variant.
    let other = draw("recolour-other", "1", "objects/other/other.pal", &[0]);
    assert_eq!(other, [[1, 1, 1], [100, 0, 0], [101, 0, 0]]);
}
