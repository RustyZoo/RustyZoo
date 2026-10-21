use super::order::{Glass, Item, Kind, Object, Scene, TankSpec, paint_order, project, sort_key};
use super::*;
use crate::sprite::{screen, tests::Install};
use crate::zoo::Tile;

pub(crate) fn flat() -> Tile {
    Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    }
}

/// A flat map of `width` by `height` tiles.
pub(crate) fn flat_map(width: usize, height: usize) -> Vec<Tile> {
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
        height: 0,
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
        height: 0,
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
        height: 0,
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
            height: 0,
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
        height: 0,
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
        height: 0,
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
        height: 0,
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
        height: 0,
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
pub(crate) fn frame_file() -> Vec<u8> {
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
        view: Default::default(),
        clocks: Default::default(),
        names: Default::default(),
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 2,
        height: 2,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
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
    let [r, g, b] = screen([200, 100, 50]);
    assert_eq!(at(32, 32), [r, g, b, 255]);
    assert_eq!(at(33, 32), [r, g, b, 255]);
    assert_eq!(at(0, 0), [0, 0, 0, 255]);
}

/// A palette file of the given colours (R, G, B), with the fourth byte 255.
pub(crate) fn palette_file(colors: &[[u8; 3]]) -> Vec<u8> {
    let mut out = (colors.len() as u32).to_le_bytes().to_vec();
    for [r, g, b] in colors {
        out.extend([*r, *g, *b, 255]);
    }
    out
}

/// A map of one flat tile with the given entities, which all stand in its middle.
pub(crate) fn tile_with(entities: &[(&str, &str, &str)]) -> Zoo {
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
            ..Default::default()
        }),
    };
    Zoo {
        view: Default::default(),
        clocks: Default::default(),
        names: Default::default(),
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 1,
        height: 1,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 0,
        tiles: flat_map(1, 1),
        entities: entities.iter().map(entity).collect(),
    }
}

/// An install whose terrain cache holds `frame` for the one grass tile.
pub(crate) fn cache_install(name: &str, frame: Vec<u8>) -> Install {
    cache_install_with(name, frame, &[])
}

/// As [`cache_install`], with more members.
pub(crate) fn cache_install_with(name: &str, frame: Vec<u8>, extra: &[(&str, Vec<u8>)]) -> Install {
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
    let mut members: Vec<(&str, Vec<u8>)> = members
        .iter()
        .map(|(n, c)| (n.as_str(), c.clone()))
        .collect();
    members.extend(extra.iter().cloned());
    Install::new(name, &members)
}

/// The members that give every shape of the one-tile map a grid line: a red pixel above the
/// ground picture's first pixel.
pub(crate) fn grid_members() -> Vec<(&'static str, Vec<u8>)> {
    use crate::bitmap::tests::{RED, WHITE, bmp24};
    let line = bmp24(&[vec![WHITE, RED], vec![WHITE, WHITE]], false);
    vec![
        (
            "tile.cfg",
            b"[0]\nnorthgrid=grid.bmp\neastgrid=grid.bmp\n".to_vec(),
        ),
        ("grid.bmp", line),
    ]
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
    let [r, g, b] = screen([200, 100, 50]);
    let (black, ground) = ([0, 0, 0, 255], [r, g, b, 255]);
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

/// Draws each window from `staged` and checks it against the same window cut from `whole`, the
/// picture of the whole map whose top left pixel is the view-space point `origin`, black outside.
fn check_windows(
    staged: &Staged,
    whole: &Framebuffer,
    origin: (i32, i32),
    windows: &[(i32, i32, u32, u32)],
) {
    for &(x, y, w, h) in windows {
        let mut canvas = Framebuffer::new(w, h);
        staged.draw(&mut canvas, x, y, 0, false);
        let mut want = Framebuffer::new(w, h);
        want.fill([0, 0, 0, 255]);
        for row in 0..h as i64 {
            for column in 0..w as i64 {
                let from = (
                    i64::from(x) + column - i64::from(origin.0),
                    i64::from(y) + row - i64::from(origin.1),
                );
                if (0..i64::from(whole.width)).contains(&from.0)
                    && (0..i64::from(whole.height)).contains(&from.1)
                {
                    let at = (from.1 as usize * whole.width as usize + from.0 as usize) * 4;
                    let to = (row as usize * w as usize + column as usize) * 4;
                    want.pixels[to..to + 4].copy_from_slice(&whole.pixels[at..at + 4]);
                }
            }
        }
        assert!(canvas.pixels == want.pixels, "window {x},{y} {w}x{h}");
    }
}

#[test]
fn one_staging_draws_any_window_like_the_whole_map() {
    let install = cache_install("render-staged", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = Zoo {
        width: 2,
        height: 2,
        tiles: flat_map(2, 2),
        ..tile_with(&[])
    };
    let mut renderer = MapRenderer::new(&vfs, &catalogue);
    let (whole, _) = renderer.render(&zoo, 0).unwrap();
    let staged = renderer.stage(&zoo, 0, false).unwrap();
    assert_eq!(staged.size().unwrap(), (whole.width, whole.height));
    let origin = staged.origin();
    // The ground of the first tile has its top left at (32, 0) of the view.
    assert_eq!(origin, (0, -32));
    let windows = [
        (origin.0, origin.1, whole.width, whole.height),
        (31, -1, 4, 3),
        (33, 0, 1, 1),
        (-5, -40, 200, 100),
        (60, 10, 30, 20),
        (i32::MIN, i32::MIN, 5, 5),
        (i32::MAX, i32::MAX, 5, 5),
        (i32::MIN, 0, 3, 3),
        (0, i32::MAX, 3, 3),
        (i32::MAX - 2, -1, 3, 3),
    ];
    check_windows(&staged, &whole, origin, &windows);
    // The same staging draws again, and the caches kept their pictures.
    assert!(!renderer.flats.is_empty());
    check_windows(&staged, &whole, origin, &windows[..3]);
}

#[test]
fn a_map_over_the_pixel_limit_is_refused_before_any_staging() {
    let install = cache_install("render-huge", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    // The largest map a 32 MiB file can hold: 1831 by 1831 tiles, 117184 by 58593 pixels.
    let zoo = Zoo {
        width: 1831,
        height: 1831,
        tiles: flat_map(1831, 1831),
        ..tile_with(&[])
    };
    let mut renderer = MapRenderer::new(&vfs, &catalogue);
    assert!(renderer.stage(&zoo, 0, false).is_err());
    assert!(renderer.flats.is_empty() && renderer.rendered.is_empty());
    let window = Options {
        window: Some((0, 0, 64, 64)),
        ..Options::default()
    };
    assert!(renderer.render_with(&zoo, 0, &window).is_err());
    assert!(renderer.render(&zoo, 0).is_err());
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

#[test]
fn a_unit_is_drawn_with_the_animation_and_frame_it_holds() {
    use crate::sprite::tests::{sized, sprite_file};
    let mut frame = Vec::new();
    for field in [1u16, 1, 0, 0, 0] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 1, 1]);
    let ani =
        |dir: &str| format!("[animation]\ndir0 = a\ndir1 = {dir}\nanimation = N\n").into_bytes();
    let install = Install::new(
        "render-shown",
        &[
            ("guests.cfg", b"[guests]\nman = man.ai\n".to_vec()),
            (
                "man.ai",
                b"[Animations]\nidle = idle\nwalk = walk\n".to_vec(),
            ),
            ("guests/man/idle/idle.ani", ani("idle")),
            ("guests/man/walk/walk.ani", ani("walk")),
            ("a/idle/N", sprite_file(None, 1, &[sized(&frame)])),
            (
                "a/walk/N",
                sprite_file(None, 3, &[sized(&frame), sized(&frame), sized(&frame)]),
            ),
            (
                "animals/Lion/lion.pal",
                palette_file(&[[255, 0, 255], [1, 2, 3]]),
            ),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = tile_with(&[("guests", "man", "")]);
    // The member and the frame of the picture of the guest, whose id is 1.
    let drawn = |shown: Option<(&str, u32)>| {
        let extra = Extra {
            shown: (shown.into_iter())
                .map(|(animation, frame)| Shown {
                    id: 1,
                    animation: animation.into(),
                    frame,
                })
                .collect(),
            ..Extra::default()
        };
        let staged = MapRenderer::new(&vfs, &catalogue)
            .stage_with(&zoo, 0, false, &extra)
            .unwrap();
        let pics: Vec<_> = staged
            .pictures()
            .filter_map(|(p, _)| match &p.pic {
                Pic::Anim(anim, _, _, frame) => Some((anim.set.files[0].clone(), *frame)),
                _ => None,
            })
            .collect();
        assert_eq!(pics.len(), 1);
        pics[0].clone()
    };
    let at = |file: &str, frame| (file.to_string(), frame);
    assert_eq!(drawn(None), at("a/idle/N", 0));
    assert_eq!(drawn(Some(("walk", 2))), at("a/walk/N", 2));
    // A frame the picture lacks is the last one; an animation the type lacks is the idle one.
    assert_eq!(drawn(Some(("walk", 99))), at("a/walk/N", 2));
    assert_eq!(drawn(Some(("run", 1))), at("a/idle/N", 0));
}

#[test]
fn a_trash_can_shows_empty_up_to_12_pieces_and_half_from_13_and_never_full() {
    use crate::sprite::tests::{sized, sprite_file};
    let mut frame = Vec::new();
    for field in [1u16, 1, 0, 0, 0] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 1, 1]);
    let ani =
        |dir: &str| format!("[animation]\ndir0 = a\ndir1 = {dir}\nanimation = N\n").into_bytes();
    let picture = || sprite_file(None, 1, &[sized(&frame)]);
    let install = Install::new(
        "render-can",
        &[
            ("bldg.cfg", b"[building]\ntrshcan = can.ai\n".to_vec()),
            (
                "can.ai",
                b"[Animations]\nempty = idle\nhalf = half\nfull = full\n".to_vec(),
            ),
            ("objects/trshcan/idle/idle.ani", ani("idle")),
            ("objects/trshcan/half/half.ani", ani("half")),
            ("objects/trshcan/full/full.ani", ani("full")),
            ("a/idle/N", picture()),
            ("a/half/N", picture()),
            ("a/full/N", picture()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let drawn = |pieces: i32| {
        let mut zoo = tile_with(&[("building", "building", "trshcan")]);
        zoo.entities[0].common.as_mut().unwrap().food = pieces;
        let staged = MapRenderer::new(&vfs, &catalogue)
            .stage_with(&zoo, 0, false, &Extra::default())
            .unwrap();
        let pics: Vec<_> = staged
            .pictures()
            .filter_map(|(p, _)| match &p.pic {
                Pic::Anim(anim, ..) => Some(anim.set.files[0].clone()),
                _ => None,
            })
            .collect();
        assert_eq!(pics.len(), 1);
        pics[0].clone()
    };
    for (pieces, member) in [
        (0, "a/idle/N"),
        (12, "a/idle/N"),
        (13, "a/half/N"),
        (25, "a/half/N"),
        (i32::MAX, "a/half/N"),
        (-1, "a/idle/N"),
    ] {
        assert_eq!(drawn(pieces), member, "{pieces} pieces");
    }
}

/// An install with four guest types (`small`, `mid`, `big` and `huge` by footprint, `small` with
/// the height 10, faces offsets of 20 for `mid`, -5 for `big` and 99999999999 for `huge`) whose pictures are one frame of height 1 with the offsets
/// (8, 4), the three marker pictures of one frame of height `marker` with its offsets at its
/// middle, and the two faces of three frames of 125 ms.
fn marker_install(name: &str, marker: u16) -> Install {
    use crate::sprite::tests::{sized, sprite_file};
    let record = |height: u16, (y, x): (u16, u16)| {
        let mut frame = Vec::new();
        for field in [height, 1, y, x, 0] {
            frame.extend(field.to_le_bytes());
        }
        // Each row covers its one pixel.
        frame.extend([1, 0, 1, 1].repeat(usize::from(height)));
        sized(&frame)
    };
    let one_frame = |height, offsets| sprite_file(None, 1, &[record(height, offsets)]);
    let (unit, selection) = (
        one_frame(1, (4, 8)),
        one_frame(marker, (marker / 2, marker / 2)),
    );
    let ani = |dirs: &str| format!("[animation]\n{dirs}animation = N\n").into_bytes();
    let ai = |keys: &str| format!("[Animations]\nidle = idle\n[Characteristics/Integers]\n{keys}");
    let mut members: Vec<(String, Vec<u8>)> = vec![
        (
            "guests.cfg".into(),
            b"[guests]\nsmall = small.ai\nmid = mid.ai\nbig = big.ai\nhuge = huge.ai\n".to_vec(),
        ),
        ("small.ai".into(), ai("cHeight = 10\n").into_bytes()),
        (
            "mid.ai".into(),
            ai("cFootprintX = 2\ncFootprintY = 4\ncFacesYOffset = 20\n").into_bytes(),
        ),
        (
            "big.ai".into(),
            ai("cFootprintX = 5\ncFacesYOffset = -5\n").into_bytes(),
        ),
        (
            "huge.ai".into(),
            ai("cFootprintX = 5\ncFacesYOffset = 99999999999\n").into_bytes(),
        ),
        (
            "animals/Lion/lion.pal".into(),
            palette_file(&[[255, 0, 255], [1, 2, 3]]),
        ),
    ];
    for name in ["small", "mid", "big", "huge"] {
        let dirs = format!("dir0 = guests\ndir1 = {name}\ndir2 = idle\n");
        members.push((format!("guests/{name}/idle/idle.ani"), ani(&dirs)));
        members.push((format!("guests/{name}/idle/N"), unit.clone()));
    }
    for name in ["selsmall", "selmed", "sellarge"] {
        let dirs = format!("dir0 = ui\ndir1 = select\ndir2 = {name}\n");
        members.push((format!("ui/select/{name}/{name}.ani"), ani(&dirs)));
        members.push((format!("ui/select/{name}/N"), selection.clone()));
    }
    let face = record(1, (0, 0));
    for name in ["smile", "frown"] {
        let dirs = format!("dir0 = objects\ndir1 = {name}\ndir2 = idle\n");
        members.push((format!("objects/{name}/idle/idle.ani"), ani(&dirs)));
        let frames = sprite_file(None, 3, &[face.clone(), face.clone(), face.clone()]);
        members.push((format!("objects/{name}/idle/N"), frames));
    }
    let members: Vec<_> = members
        .iter()
        .map(|(n, d)| (n.as_str(), d.clone()))
        .collect();
    Install::new(name, &members)
}

/// The sprite members, anchors and frames of the pictures of a zoo with one guest of type `ty`,
/// whose id is 1, with `extra`, and the number of pictures that pick an entity.
fn extra_pics(
    install: &Install,
    ty: &str,
    extra: &Extra,
) -> (Vec<(String, i32, i32, usize)>, usize) {
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let staged = MapRenderer::new(&vfs, &catalogue)
        .stage_with(&tile_with(&[("guests", ty, "")]), 0, false, extra)
        .unwrap();
    let pics = staged
        .pictures()
        .filter_map(|(p, _)| match &p.pic {
            Pic::Anim(anim, _, _, at) => Some((anim.set.files[0].clone(), p.x, p.y - p.lift, *at)),
            _ => None,
        })
        .collect();
    (pics, staged.hits.iter().flatten().count())
}

/// [`extra_pics`] without the frames, when `selected` carries the marker.
fn marker_pics(
    install: &Install,
    ty: &str,
    selected: Option<u32>,
) -> (Vec<(String, i32, i32)>, usize) {
    let extra = Extra {
        selected,
        ..Extra::default()
    };
    let (pics, picks) = extra_pics(install, ty, &extra);
    let pics = pics.into_iter().map(|(m, x, y, _)| (m, x, y)).collect();
    (pics, picks)
}

#[test]
fn the_selected_unit_gets_a_marker_by_its_footprint_after_its_picture() {
    let install = marker_install("render-marker", 1);
    let (alone, picks) = marker_pics(&install, "small", None);
    assert_eq!(alone.len(), 1);
    let (unit, x, y) = alone[0].clone();
    // Nothing selected, or another unit: no marker.
    assert_eq!(marker_pics(&install, "small", Some(2)).0, alone);
    // The unit frame's offsets are (8, 4): left 2, up 4 + 5 + half the marker's height (0).
    let (pics, marked) = marker_pics(&install, "small", Some(1));
    assert_eq!(
        pics,
        [(unit, x, y), ("ui/select/selsmall/N".into(), x - 2, y - 9)]
    );
    // The marker is not a picture of the unit: a pick cannot land on it.
    assert_eq!(marked, picks);
    assert_eq!(
        marker_pics(&install, "mid", Some(1)).0[1].0,
        "ui/select/selmed/N"
    );
    assert_eq!(
        marker_pics(&install, "big", Some(1)).0[1].0,
        "ui/select/sellarge/N"
    );
}

#[test]
fn the_marker_height_is_its_own_frame_height() {
    // The marker is 6 high and the type 10 (half 5): the shift is 4 + 5 + 3, and the marker's
    // bottom edge (anchor - offset + height, its offset being 3) ends 5 above the unit's frame.
    let install = marker_install("render-marker-height", 6);
    let pics = marker_pics(&install, "small", Some(1)).0;
    let (unit, marker) = (&pics[0], &pics[1]);
    assert_eq!(marker.2, unit.2 - 12);
    assert_eq!(marker.2 - 3 + 6, unit.2 - 4 - 5);
}

#[test]
fn each_reaction_draws_its_face_over_its_unit_at_the_frame_of_its_age() {
    let install = marker_install("render-faces", 1);
    let faces = |ty: &str, faces: &[(u32, bool, u32)]| {
        let extra = Extra {
            faces: faces.to_vec(),
            ..Extra::default()
        };
        extra_pics(&install, ty, &extra)
    };
    let (alone, picks) = faces("small", &[]);
    let (x, y) = (alone[0].1, alone[0].2);
    // Another unit's reaction draws nothing here.
    assert_eq!(faces("small", &[(2, true, 0)]).0, alone);
    // At the unit's anchor (no offset), a frame per 125 ms, held on the last one; not picked.
    let (pics, n) = faces("small", &[(1, true, 0), (1, false, 250), (1, true, 1249)]);
    let face = |m: &str, f| (format!("objects/{m}/idle/N"), x, y, f);
    assert_eq!(
        pics[1..],
        [face("smile", 0), face("frown", 2), face("smile", 2)]
    );
    assert_eq!(n, picks);
    // Up by the type's faces offset, after the marker.
    let extra = Extra {
        selected: Some(1),
        faces: vec![(1, false, 124)],
        ..Extra::default()
    };
    let pics = extra_pics(&install, "mid", &extra).0;
    assert_eq!(pics.len(), 3);
    assert_eq!(
        pics[2],
        ("objects/frown/idle/N".into(), pics[0].1, pics[0].2 - 20, 0)
    );
    // A negative offset puts it lower; a huge one is clamped, not a failure.
    let pics = extra_pics(&install, "big", &extra).0;
    assert_eq!(pics[2].2, pics[0].2 + 5);
    let pics = extra_pics(&install, "huge", &extra).0;
    assert_eq!(pics[2].2, pics[0].2.saturating_sub(i32::MAX));
}

#[test]
fn the_faces_offset_is_in_pixels_of_the_zoom() {
    let install = marker_install("render-faces-zoom", 1);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let extra = Extra {
        faces: vec![(1, true, 0)],
        ..Extra::default()
    };
    let staged = MapRenderer::new(&vfs, &catalogue)
        .stage_with(&tile_with(&[("guests", "mid", "")]), 0, false, &extra)
        .unwrap();
    assert_eq!(staged.own.len(), 2);
    // The face rises by the offset (20) at every zoom, and is not scaled with the unit.
    for zoom in [-2, 0, 2] {
        let level = Zoom::from_value(zoom);
        let [unit, face] = [0, 1].map(|i| staged.own[i].0.anchor(0, zoom, level));
        assert_eq!((unit.0, unit.1 - face.1), (face.0, 20), "zoom {zoom}");
    }
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
                height: 0,
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
            ..Default::default()
        }),
    };
    let zoo = Zoo {
        view: Default::default(),
        clocks: Default::default(),
        names: Default::default(),
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 3,
        height: 3,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
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
        view: Default::default(),
        clocks: Default::default(),
        names: Default::default(),
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 2,
        height: 2,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
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
        view: Default::default(),
        clocks: Default::default(),
        names: Default::default(),
        version: 106,
        locale: 1033,
        expansion: 0,
        width: 2,
        height: 2,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
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
fn the_grid_lines_of_every_tile_are_drawn_only_with_the_grid_on() {
    let install = cache_install_with("render-grid", frame_file(), &grid_members());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = tile_with(&[]);
    for zoom in [-2, 0, 2] {
        let draw = |grid| {
            let options = Options {
                grid,
                zoom,
                ..Options::default()
            };
            let (image, _) = MapRenderer::new(&vfs, &catalogue)
                .render_with(&zoo, 0, &options)
                .unwrap();
            image
        };
        let (off, on) = (draw(false), draw(true));
        assert_eq!(
            (off.width, off.height),
            (on.width, on.height),
            "zoom {zoom}"
        );
        let red = |image: &Framebuffer| {
            image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[..3] == [255, 0, 0])
                .count()
        };
        assert_eq!(red(&off), 0, "zoom {zoom}");
        // A one-pixel line is thinned away when zoomed out (map drawing 8).
        assert_eq!(red(&on) > 0, zoom >= 0, "zoom {zoom}");
    }
}

#[test]
fn a_renderer_keeps_the_tiles_of_a_staging_only_while_the_ground_and_the_rotation_are_the_same() {
    let install = cache_install_with("render-again", frame_file(), &grid_members());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut zoo = Zoo {
        width: 2,
        height: 1,
        tiles: flat_map(2, 1),
        ..tile_with(&[])
    };
    let picture = |renderer: &mut MapRenderer, zoo: &Zoo, rotation| {
        let staged = renderer.stage(zoo, rotation, false).unwrap();
        let (w, h) = staged.size().unwrap();
        let mut canvas = Framebuffer::new(w, h);
        let (x, y) = staged.origin();
        staged.draw(&mut canvas, x, y, 0, true);
        (canvas.pixels, staged.summary)
    };
    let mut renderer = MapRenderer::new(&vfs, &catalogue);
    let first = picture(&mut renderer, &zoo, 0);
    assert_eq!(picture(&mut renderer, &zoo, 0), first);
    // A raised corner: no picture in the cache and no grid lines for the shape.
    zoo.tiles[1].shape = 1;
    for rotation in [0, 1] {
        let fresh = picture(&mut MapRenderer::new(&vfs, &catalogue), &zoo, rotation);
        assert_ne!(fresh, first, "rotation {rotation}");
        assert_eq!(picture(&mut renderer, &zoo, rotation), fresh);
    }
}

#[test]
fn a_marked_tile_gets_its_highlight_in_the_colour_of_the_mark_and_frees_the_grid_behind_it() {
    use crate::bitmap::tests::{WHITE, bmp24};
    const BLUE: [u8; 3] = [0, 0, 255];
    let mut members = grid_members();
    members[0].1 = b"[0]\nnorthgrid=grid.bmp\neastgrid=grid.bmp\nhighlight=hl.bmp\n".to_vec();
    members.push((
        "hl.bmp",
        bmp24(&[vec![WHITE, BLUE], vec![WHITE, WHITE]], false),
    ));
    let install = cache_install_with("render-marks", frame_file(), &members);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = Zoo {
        width: 2,
        height: 1,
        tiles: flat_map(2, 1),
        ..tile_with(&[])
    };
    let mut renderer = MapRenderer::new(&vfs, &catalogue);
    let mut count = |marks: &[Mark], colour: [u8; 3]| {
        let staged = renderer
            .stage_with(
                &zoo,
                0,
                false,
                &Extra {
                    marks: marks.to_vec(),
                    ..Extra::default()
                },
            )
            .unwrap();
        let (w, h) = staged.size().unwrap();
        let mut canvas = Framebuffer::new(w, h);
        let (x, y) = staged.origin();
        staged.draw(&mut canvas, x, y, 0, true);
        let n = |c: [u8; 3]| {
            canvas
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[..3] == c)
                .count()
        };
        (n(colour), n(BLUE), n([255, 0, 0]))
    };
    let mark = |tile| Mark {
        tile,
        color: [64, 32, 96],
    };
    let (none, _, lines) = count(&[], screen([64, 32, 96]));
    assert_eq!(none, 0, "nothing is marked");
    let (marked, blue, _) = count(&[mark(0)], screen([64, 32, 96]));
    assert_eq!(
        (marked, blue),
        (1, 0),
        "one pixel, in the colour of the mark"
    );
    // The second tile is marked: the first one, behind it, draws one grid line less.
    let (_, _, fewer) = count(&[mark(1)], screen([64, 32, 96]));
    assert!(fewer < lines, "{fewer} against {lines}");
    // A mark on a tile that is not on the map is ignored.
    assert_eq!(count(&[mark(7)], screen([64, 32, 96])), (0, 0, lines));
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
                palette_file(&[MARKER, [16; 3], [32; 3], [48; 3]]),
            ),
            ("color.pal", palette_file(&[MARKER, [80; 3]])),
            (
                "pals/red.pal",
                palette_file(&[MARKER, [96, 0, 0], [112, 0, 0]]),
            ),
            (
                "pals/blue.pal",
                palette_file(&[MARKER, [0, 0, 192], [0, 0, 208]]),
            ),
        ]
    };
    // The colours of the three pixels, from the left of the sprite file (this view is mirrored).
    let draw = |name: &str, flag: &str, fullpal: &str, stored: &[u8]| {
        let install = Install::new(name, &members(flag, fullpal));
        let vfs = Vfs::open(&install.0).unwrap();
        let catalogue = Catalogue::load(&vfs).unwrap();
        let zoo = Zoo {
            view: Default::default(),
            clocks: Default::default(),
            names: Default::default(),
            version: 106,
            locale: 1033,
            expansion: 0,
            width: 1,
            height: 1,
            entrance: (0, 0),
            habitat_tiles: Vec::new(),
            habitat_gates: Vec::new(),
            habitat_info: Vec::new(),
            tanks: Vec::new(),
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
                    ..Default::default()
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
    // The screen shows the colours reduced to 5-6-5.
    let blue = [[80; 3], [0, 0, 192], [0, 0, 208]].map(screen);
    let red = [[80; 3], [96, 0, 0], [112, 0, 0]].map(screen);
    assert_eq!(draw("recolour-default", "1", full, &[]), blue);
    assert_eq!(draw("recolour-first", "1", full, &[0]), red);
    assert_eq!(draw("recolour-second", "1", full, &[1, 7]), blue);
    // A choice beyond the variants, or a type that does not ask for it, keeps the plain palette.
    let plain = [[32; 3], [48; 3]].map(screen);
    assert_eq!(
        draw("recolour-beyond", "1", full, &[5]),
        [blue[0], plain[0], plain[1]]
    );
    assert_eq!(
        draw("recolour-off", "0", full, &[0]),
        [screen([16; 3]), plain[0], plain[1]]
    );
    // A sprite whose palette is not the `fullpal` one keeps its own fixed colours; only the
    // group takes the variant.
    let other = draw("recolour-other", "1", "objects/other/other.pal", &[0]);
    assert_eq!(other, [screen([16; 3]), red[1], red[2]]);
}

#[test]
fn a_new_building_starts_with_the_default_variant_of_each_colour_group() {
    let shed = |flag: &str, defaults: &str| {
        format!(
            "[Characteristics/Integers]\ncIsColorReplaced = {flag}\n\
             [colorrep]\ncolor = cr_color\nreplace = cr_part1\nreplace = cr_part2\n{defaults}\
             [cr_color]\nncolors = 2\n"
        )
        .into_bytes()
    };
    let groups = b"[cr_part1]\nncolors = 2\npal = pals/red.pal\npal = pals/blue.pal\n\
                   [cr_part2]\nncolors = 2\npal = pals/one.pal\npal = pals/two.pal\n"
        .to_vec();
    let choices = |name: &str, flag: &str, defaults: &str| {
        let install = Install::new(
            name,
            &[
                ("bldg.cfg", b"[building]\nshed = shed.ai\n".to_vec()),
                ("shed.ai", shed(flag, defaults)),
                ("building.ai", groups.clone()),
            ],
        );
        let vfs = Vfs::open(&install.0).unwrap();
        let catalogue = Catalogue::load(&vfs).unwrap();
        let ty = catalogue.types.iter().find(|t| t.subtype == b"shed");
        MapRenderer::new(&vfs, &catalogue).default_colors(ty.expect("the type exists"))
    };
    let both = "defaultpal = PALS\\Blue.pal\ndefaultpal = pals/two.pal\n";
    assert_eq!(choices("colours-both", "1", both), [1, 1]);
    // A default that is not among the variants, or none at all, is the first variant.
    let odd = "defaultpal = pals/blue.pal\ndefaultpal = pals/none.pal\n";
    assert_eq!(choices("colours-odd", "1", odd), [1, 0]);
    assert_eq!(choices("colours-none", "1", ""), [0, 0]);
    // A type that is not recoloured has nothing to store.
    assert!(choices("colours-off", "0", both).is_empty());
    assert!(choices("colours-missing", "x", both).is_empty());
}

/// A 4 by 4 map with a tank on the middle four tiles, `height` and `level` steps, a wall on every
/// outer edge of it (objects 0 to 7) and then the `extra` objects.
fn tank_scene(r: u8, height: i32, level: i32, extra: Vec<Object>) -> Scene {
    let mut objects = Vec::new();
    for (tx, ty) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
        for facing in [0u32, 2, 4, 6] {
            // The wall stands on the edge if there is no tank tile across it.
            let (dx, dy) = [(0, -1), (1, 0), (0, 1), (-1, 0)][facing as usize / 2];
            if (1..=2).contains(&(tx + dx)) && (1..=2).contains(&(ty + dy)) {
                continue;
            }
            let (x, y) = match facing {
                0 => (64 * tx + 32, 64 * ty),
                2 => (64 * tx + 63, 64 * ty + 32),
                4 => (64 * tx + 32, 64 * ty + 63),
                _ => (64 * tx, 64 * ty + 32),
            };
            objects.push(Object {
                kind: Kind::Wall,
                facing,
                ..unit(x, y)
            });
        }
    }
    assert_eq!(objects.len(), 8);
    objects.extend(extra);
    let fences: Vec<bool> = objects.iter().map(|o| o.kind == Kind::Wall).collect();
    let mut scene = Scene::new(4, 4, r, &flat_map(4, 4), objects);
    scene.set_tanks(
        &[TankSpec {
            tile: (1, 1),
            gate: (-1, -1),
            height,
            level,
        }],
        &fences,
    );
    scene
}

#[test]
fn ripples_come_with_the_water_and_go_with_the_level_and_the_purity() {
    let look = |scum| tank::Look {
        water: "salt".into(),
        glass: "glass".into(),
        scum,
    };
    let ripples = |level: i32, look: tank::Look| {
        let mut scene = tank_scene(0, 5, level, Vec::new());
        let before = scene.objects.len();
        let added = add_ripples(&mut scene, &[look], 16);
        assert_eq!(scene.objects.len(), before + added);
        added
    };
    // One for each of the eight walls on the edges of the four tiles, at any level above 0.
    assert_eq!(ripples(0, look(None)), 0);
    assert_eq!(ripples(1, look(None)), 8);
    assert_eq!(ripples(4, look(None)), 8);
    // Not on water below the murky purity, and none for a tank without a look.
    assert_eq!(ripples(3, look(Some(([220, 230, 240], 200)))), 0);
    let mut scene = tank_scene(0, 5, 3, Vec::new());
    assert_eq!(add_ripples(&mut scene, &[], 16), 0);
}

fn position(items: &[Item], wanted: impl Fn(&Item) -> bool) -> usize {
    items.iter().position(wanted).expect("the item is painted")
}

#[test]
fn a_tank_is_painted_in_layers_floor_walls_water_contents_front_walls() {
    // A unit under the water (surface at 16 * 2 = 32), one above it.
    let under = Object {
        z: 8,
        ..unit_on(1, 1)
    };
    let above = Object {
        z: 48,
        ..unit_on(2, 2)
    };
    for r in 0..4u8 {
        let scene = tank_scene(r, 3, 2, vec![under, above]);
        // The tiles of the tank and its eight walls are found.
        assert!(
            (0..8).all(|w| scene.wall_tank(w) == Some(0)),
            "rotation {r}"
        );
        let tiles = [5, 6, 9, 10];
        assert!(tiles.iter().all(|&t| scene.tank_of(t) == Some(0)));
        assert_eq!(scene.tank_of(0), None);
        let items = paint_order(&scene);
        let at = |wanted: Item| position(&items, |&i| i == wanted);
        let count = |wanted: fn(&Item) -> bool| items.iter().filter(|i| wanted(i)).count();
        // Which walls are at the back and at the front of the view.
        let front = [
            (4 + 8 - 2 * usize::from(r)) % 8,
            (6 + 8 - 2 * usize::from(r)) % 8,
        ];
        let is_front = |w: usize| front.contains(&(scene.objects[w].facing as usize));
        let (back, front): (Vec<usize>, Vec<usize>) = (0..8).partition(|&w| !is_front(w));

        let floor = tiles.iter().map(|&t| at(Item::Terrain(t))).max().unwrap();
        let (u, a) = (at(Item::Object(8)), at(Item::Object(9)));
        let surface = position(&items, |i| matches!(i, Item::Surface { .. }));
        let last_surface = items
            .iter()
            .rposition(|i| matches!(i, Item::Surface { .. }))
            .unwrap();
        for &w in &back {
            assert!(
                floor < at(Item::Edge(w)) && at(Item::Edge(w)) < u,
                "back wall {w}"
            );
        }
        // The shadow, then the submerged unit, the underwater pictures of the unit above, the
        // four pieces of the surface, and the unit above after all of them.
        assert!(at(Item::Shadow(8)) < u && u < at(Item::Under(9)));
        assert!(at(Item::Under(9)) < surface);
        assert!(last_surface < a);
        // The front walls come last, each after its faces.
        for &w in &front {
            assert!(a < at(Item::Edge(w)), "front wall {w}");
        }
        let first_face = position(&items, |i| matches!(i, Item::Face { .. }));
        assert!(a < first_face);
        assert_eq!(count(|i| matches!(i, Item::Surface { .. })), 4);
        assert_eq!(count(|i| matches!(i, Item::Quarter { .. })), 0);
        // Water faces up to the level and glass from there to the top of the walls.
        assert_eq!(count(|i| matches!(i, Item::Face { .. })), 4 * 2);
        let panes = |glass: fn(&Glass) -> bool| {
            items
                .iter()
                .filter(|i| matches!(i, Item::Pane { glass: g, .. } if glass(g)))
                .count()
        };
        assert_eq!(
            panes(|g| matches!(g, Glass::UpperLeft | Glass::UpperRight)),
            4
        );
        assert_eq!(
            panes(|g| matches!(g, Glass::LowerLeft | Glass::LowerRight)),
            4
        );
        // Every wall is painted once.
        for w in 0..8 {
            assert_eq!(items.iter().filter(|&&i| i == Item::Edge(w)).count(), 1);
        }
    }
}

#[test]
fn a_tank_without_water_has_no_surface_and_no_shadows() {
    let scene = tank_scene(0, 3, 0, vec![unit_on(1, 1)]);
    let items = paint_order(&scene);
    assert!(!items.iter().any(|i| matches!(
        i,
        Item::Surface { .. } | Item::Face { .. } | Item::Shadow(_)
    )));
    // The glass goes all the way down.
    let panes = items
        .iter()
        .filter(|i| matches!(i, Item::Pane { .. }))
        .count();
    assert_eq!(panes, 8 * 3);
}

#[test]
fn an_object_that_reaches_the_surface_is_painted_with_quarters_of_the_water() {
    let quarters_of = |items: &[Item]| -> Vec<(usize, u8)> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Quarter { tile, quad, .. } => Some((*tile, *quad)),
                _ => None,
            })
            .collect()
    };
    let whole = |items: &[Item]| {
        items
            .iter()
            .filter(|i| matches!(i, Item::Surface { .. }))
            .count()
    };
    // A tree 5 steps tall in a tank with 2 steps of water. A tile with such an object starts a
    // sub-group of its own, painted tile by tile: the three other tiles keep a whole surface,
    // and the tile of the tree gets the quarters of the top and of the right quadrant (the tree
    // stands in the right one at this rotation) before the tree, the others after it.
    let tree = |tx, ty| Object {
        kind: Kind::Fixed,
        height: 5,
        ..unit_on(tx, ty)
    };
    for (tx, ty, tile) in [(1, 1, 5), (2, 2, 10), (2, 1, 6), (1, 2, 9)] {
        let items = paint_order(&tank_scene(0, 3, 2, vec![tree(tx, ty)]));
        let quarters = quarters_of(&items);
        assert_eq!(quarters.len(), 4, "tree on tile {tile}");
        assert!(quarters.iter().all(|&(t, _)| t == tile));
        assert_eq!(whole(&items), 3, "tree on tile {tile}");
        let tree_at = position(&items, |&i| i == Item::Object(8));
        assert_eq!(quarters_of(&items[..tree_at]), [(tile, 0), (tile, 1)]);
    }
}

#[test]
fn a_big_object_that_reaches_the_surface_keeps_its_tiles_in_one_sub_group() {
    // A statue of 2 by 2 tiles in the middle of the tank, taller than the water is deep: its
    // sub-group goes on over the tiles it stands on, so the whole tank is painted tile by tile.
    let statue = Object {
        kind: Kind::Fixed,
        height: 5,
        footprint: (4, 4),
        ..unit(128, 128)
    };
    let items = paint_order(&tank_scene(0, 3, 2, vec![statue]));
    let quarters = items
        .iter()
        .filter(|i| matches!(i, Item::Quarter { .. }))
        .count();
    assert_eq!(quarters, 16);
    assert!(!items.iter().any(|i| matches!(i, Item::Surface { .. })));
    // The statue is painted once.
    assert_eq!(items.iter().filter(|&&i| i == Item::Object(8)).count(), 1);
}

#[test]
fn the_area_of_a_tank_is_filled_up_to_the_habitat_fences_whatever_the_heights() {
    // A 4 by 4 map with a fence around the 2 by 2 tiles at the top left, and a cliff between two
    // of them: the fill does not look at heights.
    let mut tiles = flat_map(4, 4);
    step(&mut tiles, 4, 1, 0, 6);
    let mut fences = Vec::new();
    let mut objects = Vec::new();
    for (x, y, facing) in [
        (32, 128 - 1, 4),
        (96, 128 - 1, 4),
        (128 - 1, 32, 2),
        (128 - 1, 96, 2),
    ] {
        objects.push(Object {
            kind: Kind::Edge,
            facing,
            ..unit(x, y)
        });
        fences.push(true);
    }
    // A fence that is not a habitat fence ends nothing.
    objects.push(Object {
        kind: Kind::Edge,
        facing: 2,
        ..unit(64 * 2 + 63, 32)
    });
    fences.push(false);
    let mut scene = Scene::new(4, 4, 0, &tiles, objects);
    let spec = |tile| TankSpec {
        tile,
        gate: (-1, -1),
        height: 3,
        level: 2,
    };
    scene.set_tanks(&[spec((0, 0))], &fences);
    let inside: Vec<usize> = (0..16).filter(|&t| scene.tank_of(t).is_some()).collect();
    assert_eq!(inside, [0, 1, 4, 5]);
    // A second exhibit that starts inside takes the tiles over; one that starts outside stays
    // outside the fence.
    scene.set_tanks(&[spec((0, 0)), spec((1, 1))], &fences);
    assert!(inside.iter().all(|&t| scene.tank_of(t) == Some(1)));
    scene.set_tanks(&[spec((0, 0)), spec((3, 3))], &fences);
    assert_eq!(scene.tank_of(15), Some(1));
    assert_eq!(scene.tank_of(0), Some(0));
    assert_eq!((0..16).filter(|&t| scene.tank_of(t) == Some(1)).count(), 12);
}

#[test]
fn the_ground_height_follows_the_shape() {
    use super::order::ground_height;
    let tile = |elevation, shape| Tile {
        elevation,
        shape,
        ..flat()
    };
    assert_eq!(ground_height(&tile(3, 0x00), 32, 32), 48);
    assert_eq!(ground_height(&tile(0, 0x50), 64, 0), 16);
    assert_eq!(ground_height(&tile(0, 0x50), 32, 32), 8);
    assert_eq!(ground_height(&tile(0, 0x54), 32, 32), 16);
    assert_eq!(ground_height(&tile(-2, 0x01), 32, 32), -48);
}

#[test]
fn odd_tank_values_never_break_the_order() {
    let under = Object {
        z: -500,
        ..unit_on(1, 1)
    };
    for (height, level) in [(0, 0), (0, 5), (1, 1), (3, 9), (64, 64), (64, 0), (-3, -2)] {
        for r in 0..4u8 {
            let scene = tank_scene(r, height, level, vec![under]);
            let items = paint_order(&scene);
            for t in 0..16 {
                let n = items.iter().filter(|&&i| i == Item::Terrain(t)).count();
                assert_eq!(n, 1, "tile {t} at height {height}, level {level}");
            }
        }
    }
    // A tank whose tile is off the map owns no tile.
    let mut scene = Scene::new(2, 2, 0, &flat_map(2, 2), Vec::new());
    scene.set_tanks(
        &[TankSpec {
            tile: (-1, 7),
            gate: (5, 5),
            height: 3,
            level: 2,
        }],
        &[],
    );
    assert!((0..4).all(|t| scene.tank_of(t).is_none()));
    assert_eq!(paint_order(&scene).len(), 4);
}

#[test]
fn an_animal_outside_the_tank_movement_mode_casts_no_shadow() {
    let config = std::sync::Arc::new(
        Config::parse(b"[Characteristics/Integers]\ncUsesRealShadows = 1\n").unwrap(),
    );
    let ty = |class| EntityType {
        class,
        type_name: Vec::new(),
        subtype: Vec::new(),
        file: None,
        config: Some(config.clone()),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let meta = |ty, mode| Meta {
        ty,
        colors: &[],
        life: None,
        mode,
        portal: false,
        food: 0,
        tint: None,
        wall: 0,
        shown: None,
    };
    let (animal, guest) = (ty("animals"), ty("guests"));
    assert!(tank::casts_shadow(&meta(&animal, 3)));
    assert!(!tank::casts_shadow(&meta(&animal, 2)));
    assert!(tank::casts_shadow(&meta(&guest, 2)));
    let plain = EntityType {
        config: None,
        ..ty("animals")
    };
    assert!(!tank::casts_shadow(&meta(&plain, 3)));
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn on_real_maps_any_window_of_one_staging_is_a_window_of_the_whole_map() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut renderer = MapRenderer::new(&vfs, &catalogue);
    for member in ["maps/default.zoo", "maps/med_kids.zoo"] {
        let zoo = Zoo::parse(&vfs.read(member).unwrap()).unwrap();
        for rotation in [0, 1] {
            let (whole, _) = renderer.render(&zoo, rotation).unwrap();
            let staged = renderer.stage(&zoo, rotation, false).unwrap();
            let origin = staged.origin();
            let (w, h) = (whole.width as i32, whole.height as i32);
            let windows = [
                (origin.0, origin.1, whole.width, whole.height),
                (origin.0 + w / 2 - 512, origin.1 + h / 2 - 384, 1024, 768),
                (origin.0 - 100, origin.1 - 100, 1024, 768),
                (origin.0 + w - 900, origin.1 + h - 700, 1024, 768),
                (origin.0 + 1000, origin.1 + 300, 1, 1),
                (origin.0 + 777, origin.1 + 555, 640, 480),
                (i32::MIN, i32::MIN, 1024, 768),
                (i32::MAX, i32::MAX, 1024, 768),
                (i32::MAX - 500, origin.1, 1024, 768),
            ];
            check_windows(&staged, &whole, origin, &windows);
            let mut canvas = Framebuffer::new(1024, 768);
            let start = std::time::Instant::now();
            for _ in 0..20 {
                staged.draw(
                    &mut canvas,
                    origin.0 + w / 2 - 512,
                    origin.1 + h / 2 - 384,
                    0,
                    false,
                );
            }
            eprintln!(
                "{member} rotation {rotation}: {:?} per 1024x768 draw",
                start.elapsed() / 20
            );
        }
    }
}

/// Draws the one-tile map `zoo` whole at a zoom value; the picture and the view-space point of
/// its top left pixel.
fn at_zoom(vfs: &Vfs, zoo: &Zoo, zoom: i32) -> (Framebuffer, (i64, i64)) {
    let catalogue = Catalogue::load(vfs).unwrap();
    let options = Options {
        zoom,
        ..Options::default()
    };
    let mut renderer = MapRenderer::new(vfs, &catalogue);
    let (image, _) = renderer.render_with(zoo, 0, &options).unwrap();
    let b = renderer.stage(zoo, 0, false).unwrap().bounds_at(zoom);
    (image, (b[0], b[1]))
}

/// The view-space points of the pixels of `image` that are not black.
fn lit(image: &Framebuffer, (left, top): (i64, i64)) -> Vec<(i64, i64)> {
    let w = image.width as usize;
    let mut points = Vec::new();
    for (i, pixel) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
        if pixel[..3] != [0, 0, 0] {
            points.push((left + (i % w) as i64, top + (i / w) as i64));
        }
    }
    points
}

#[test]
fn a_cached_ground_picture_is_doubled_in_place_zoomed_in_and_thinned_zoomed_out() {
    // The two pixels of the picture are at the view-space points (32, 0) and (33, 0) at normal
    // zoom, the anchor being (32, 0) and the offsets 0.
    let install = cache_install("render-zoom-cached", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let zoo = tile_with(&[]);
    let (image, at) = at_zoom(&vfs, &zoo, 0);
    assert_eq!(lit(&image, at), [(32, 0), (33, 0)]);
    // In: each pixel is a block of 2 by 2 from (2 * 32, 0).
    let (image, at) = at_zoom(&vfs, &zoo, 2);
    let want: Vec<(i64, i64)> = (0..2).flat_map(|y| (64..68).map(move |x| (x, y))).collect();
    let mut got = lit(&image, at);
    got.sort_by_key(|&(x, y)| (y, x));
    assert_eq!(got, want);
    // Out: the column of an even distance to the anchor is kept, but a picture one row high
    // keeps no row.
    let (image, at) = at_zoom(&vfs, &zoo, -2);
    assert!(lit(&image, at).is_empty());
}

#[test]
fn a_ground_picture_rendered_at_run_time_follows_the_same_rules_at_every_zoom() {
    // A 6 by 6 red texture, 24 bits, rows bottom up.
    let mut texture = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 6, 0, 24, 0];
    texture.extend([0, 0, 255].repeat(36));
    let install = Install::new(
        "render-zoom-rendered",
        &[
            (
                "terrain/tiletex.cfg",
                b"[ttGrass]\ntype=0\ntexture=terrain/g.tga\n".to_vec(),
            ),
            ("terrain/g.tga", texture),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let zoo = tile_with(&[]);
    let (image, at) = at_zoom(&vfs, &zoo, 0);
    let normal = lit(&image, at);
    assert!(normal.len() > 1000);
    // The anchor of the ground is (32, 0) at normal zoom; its offsets are 32 and 16, as the
    // diamond's. In: every pixel becomes a block of 2 by 2 about twice as far from the anchor.
    let (image, at) = at_zoom(&vfs, &zoo, 2);
    let mut got = lit(&image, at);
    got.sort();
    let mut want: Vec<(i64, i64)> = normal
        .iter()
        .flat_map(|&(x, y)| {
            let (x, y) = (2 * (x - 32) + 64, 2 * y);
            [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)]
        })
        .collect();
    want.sort();
    assert_eq!(got, want);
    // Out: the pixels at an even distance from the anchor in both directions go to half of it; the
    // 33 rows of the picture keep 16, those from the row of the offset 16 (even) up by pairs.
    let (image, at) = at_zoom(&vfs, &zoo, -2);
    let mut got = lit(&image, at);
    got.sort();
    let mut want: Vec<(i64, i64)> = normal
        .iter()
        .filter(|&&(x, y)| (x - 32) % 2 == 0 && y % 2 == 0 && y < 16)
        .map(|&(x, y)| (16 + (x - 32) / 2, y / 2))
        .collect();
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn a_huge_zoom_factor_puts_no_tile_where_a_clamp_would() {
    // Every tile but the first is at least 32 view-space pixels from the origin, so 2^36 and
    // more at this factor: far from the window at 2^30, where a clamp of the corner would put
    // them.
    let install = cache_install("render-zoom-huge", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let zoo = Zoo {
        width: 40,
        height: 40,
        tiles: flat_map(40, 40),
        ..tile_with(&[])
    };
    let options = Options {
        zoom: i32::MAX,
        window: Some(((1 << 30) - 200, (1 << 30) - 200, 400, 400)),
        ..Options::default()
    };
    let (image, _) = MapRenderer::new(&vfs, &catalogue)
        .render_with(&zoo, 0, &options)
        .unwrap();
    assert!(lit(&image, (0, 0)).is_empty());
}

#[test]
fn a_path_picture_follows_the_slope_or_the_neighbours_that_hold_a_path() {
    // A flat tile: 100 plus a bit for each side with a path, the first side highest.
    assert_eq!(path_key(0, 0, [false; 4]), 100);
    assert_eq!(path_key(0, 0, [true, false, true, false]), 110);
    assert_eq!(path_key(0, 0, [false, false, false, true]), 101);
    assert_eq!(path_key(0, 3, [true; 4]), 115);
    // A tile with one side raised keys its shape byte turned to the view, whatever the neighbours.
    assert_eq!(path_key(0x05, 0, [true; 4]), 5);
    assert_eq!(path_key(0x05, 1, [false; 4]), 0x41);
    assert_eq!(path_key(0x14, 2, [false; 4]), 0x41);
    assert_eq!(path_key(0x50, 4, [false; 4]), 0x50);
    // The pictures are found by key, the isolated piece stands in for a missing one.
    let codes = [Some(5), Some(100), None, Some(110)];
    assert_eq!(shape_index(&codes, 110), 3);
    assert_eq!(shape_index(&codes, 5), 0);
    assert_eq!(shape_index(&codes, 107), 1);
    assert_eq!(shape_index(&[Some(5)], 107), 0);
    assert_eq!(shape_index(&[], 100), 0);
}

#[test]
fn a_piece_on_a_sloping_edge_takes_the_30p_or_30n_name_of_its_row() {
    let tile = Tile {
        shape: 0b11_00_00_00,
        ..flat()
    };
    // Corner 1 stands 3 steps above the others: edge 0 (corners 7, 1) rises, edge 1 (1, 3) falls.
    assert_eq!(slope_name("idle", &tile, 0), "idle30p");
    assert_eq!(slope_name("det", &tile, 3), "det30n");
    assert_eq!(slope_name("idle", &tile, 4), "idle");
    assert_eq!(slope_name("broke", &flat(), 0), "broke");
}
