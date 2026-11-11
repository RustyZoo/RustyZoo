//! The worked examples of map drawing 10 on synthetic maps.

use super::*;
use crate::camera::Settings;
use crate::render::order::Kind;
use crate::world::{Shape, TankKind};
use crate::zoo::{Common, Entity, Tile, View, Zoo};

/// The grid of a square map of `n` tiles.
fn grid(n: i64, rotation: u8) -> Grid {
    Grid {
        w: n,
        h: n,
        rotation,
    }
}

const BEACH: Grid = Grid {
    w: 75,
    h: 75,
    rotation: 0,
};

#[test]
fn a_cell_shows_the_tile_of_each_rotation_and_the_turns_use_the_sizes() {
    let at = |rotation| grid(50, rotation).tile(3, 7);
    assert_eq!([0, 1, 2, 3].map(at), [(3, 7), (7, 47), (47, 43), (43, 3)]);
    // The edge lines (10.4 point 3): cells at 0 point off the map.
    assert_eq!(grid(50, 1).tile(0, 9), (9, 50));
    assert_eq!(grid(50, 2).tile(0, 9), (50, 41));
    assert_eq!(grid(50, 2).tile(9, 0), (41, 50));
    assert_eq!(grid(50, 3).tile(9, 0), (50, 9));
    // The other direction.
    let back = |rotation| grid(50, rotation).cell(3, 7);
    assert_eq!([0, 1, 2, 3].map(back), [(3, 7), (43, 3), (47, 43), (7, 47)]);
    for rotation in 0..4 {
        let g = grid(50, rotation);
        let (x, y) = g.tile(11, 20);
        assert_eq!(g.cell(x, y), (11, 20), "rotation {rotation}");
    }
}

#[test]
fn a_cell_is_the_pixel_of_the_diamond_it_belongs_to() {
    assert_eq!(BEACH.pixel(0, 0), (0, 37));
    assert_eq!(BEACH.pixel(14, 39), (53, 37 + 12));
    assert_eq!(BEACH.pixel(39, 14), (53, 37 - 13));
    let mut seen = std::collections::HashSet::new();
    assert!((0..75).all(|c| (0..75).all(|r| seen.insert(BEACH.pixel(c, r)))));
}

#[test]
fn the_block_of_a_large_map_follows_the_view_and_stops_at_the_edges() {
    // `beach`, view cell (14, 39): the block starts at (0, 5) and draws at X = px - 5.
    let block = Block::of(BEACH, (139, 69), (14, 39));
    assert_eq!((block.c0, block.r0, block.sx), (0, 5, 0));
    let (px, py) = BEACH.pixel(14, 39);
    assert_eq!(block.at(14, 39), (px - 5, py - 5));
    // Clicked (60, 60): the block is clamped at W - 69 and both cells are 6.
    let block = Block::of(BEACH, (139, 69), (60, 60));
    assert_eq!((block.c0, block.r0), (6, 6));
    let block = Block::of(BEACH, (139, 69), (1000, -1000));
    assert_eq!((block.c0, block.r0), (6, 0));
}

#[test]
fn a_small_map_is_centred_in_the_widget() {
    let g = grid(50, 0);
    let block = Block::of(g, (139, 69), (25, 25));
    assert_eq!((block.c0, block.r0, block.sx, block.cols), (0, 0, 19, 50));
    let xs: Vec<i64> = (0..50)
        .flat_map(|c| (0..50).map(move |r| (c, r)))
        .map(|(c, r)| block.at(c, r).0)
        .collect();
    let ys: Vec<i64> = (0..50)
        .flat_map(|c| (0..50).map(move |r| (c, r)))
        .map(|(c, r)| block.at(c, r).1)
        .collect();
    assert_eq!((xs.iter().min(), xs.iter().max()), (Some(&19), Some(&117)));
    assert_eq!((ys.iter().min(), ys.iter().max()), (Some(&9), Some(&58)));
}

#[test]
fn a_pixel_gives_back_its_cell_and_one_outside_the_block_gives_none() {
    for (g, centre) in [(BEACH, (14, 39)), (grid(50, 0), (25, 25)), (BEACH, (70, 3))] {
        let block = Block::of(g, (139, 69), centre);
        for c in block.c0..block.c0 + block.cols {
            for r in block.r0..block.r0 + block.rows {
                assert_eq!(block.cell_at(block.at(c, r)), Some((c, r)));
            }
        }
        assert_eq!(block.cell_at((-5, 0)), None);
        assert_eq!(block.cell_at((0, 0)), None);
    }
}

#[test]
fn the_frame_of_beach_is_the_rectangle_of_the_worked_example() {
    let f = Frame::of(BEACH, (1024, 768), 0, (14, 39));
    assert_eq!(
        (f.l, f.r, f.t, f.b),
        (Some(37), Some(69), Some(37), Some(61))
    );
    // The frame after the click of 10.8 point 4.
    let f = Frame::of(BEACH, (1024, 768), 0, (60, 60));
    assert_eq!(
        (f.l, f.r, f.t, f.b),
        (Some(104), Some(136), Some(25), Some(49))
    );
    // 33 by 25 pixels: its sides are on it, its inside and its outside are not.
    let f = Frame::of(BEACH, (1024, 768), 0, (30, 30));
    assert!(f.on((f.l.unwrap(), f.t.unwrap() + 5)) && f.on((f.l.unwrap() + 7, f.b.unwrap())));
    assert!(!f.on((f.l.unwrap() + 1, f.t.unwrap() + 1)) && !f.on((f.l.unwrap() - 1, f.t.unwrap())));
}

#[test]
fn the_frame_shrinks_with_the_zoom_and_loses_the_bounds_off_the_map() {
    let width = |zoom| {
        let f = Frame::of(BEACH, (1024, 768), zoom, (30, 30));
        f.r.unwrap() - f.l.unwrap()
    };
    assert_eq!([width(-2), width(0), width(2)], [64, 32, 16]);
    // A view far off the map has no bound and so no frame.
    let f = Frame::of(BEACH, (1024, 768), 0, (500, 500));
    assert_eq!((f.l, f.r, f.t, f.b), (None, None, None, None));
    assert!(!f.on((100, 37)));
    // A huge cell is as quick as any other.
    let f = Frame::of(BEACH, (1024, 768), 0, (i64::MAX / 4, i64::MIN / 4));
    assert_eq!(f.l, None);
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
            rotation,
            ..Default::default()
        }),
    }
}

const NOTHING: Shape = Shape {
    kind: Kind::Fixed,
    footprint: (1, 1),
    fence: false,
    wall: false,
    gate: false,
    gated: false,
    walkable: (false, false),
    forms: None,
    rubbleable: false,
    tank: TankKind::NONE,
};

fn shape(e: &Entity) -> Option<Shape> {
    let fence = Shape {
        kind: Kind::Edge,
        fence: true,
        ..NOTHING
    };
    Some(match e.class.as_str() {
        // A zoo wall that closes no exhibit.
        "zwall" => Shape {
            wall: true,
            fence: false,
            ..fence
        },
        "plain" => fence,
        "twall" => Shape {
            kind: Kind::Wall,
            tank: TankKind {
                wall: true,
                ..TankKind::NONE
            },
            ..fence
        },
        "path" => Shape {
            kind: Kind::Path,
            footprint: (2, 2),
            ..NOTHING
        },
        "gate" => Shape {
            footprint: (2, 2),
            gate: true,
            ..NOTHING
        },
        "animals" | "keeper" | "guests" => Shape {
            kind: Kind::Unit,
            ..NOTHING
        },
        _ => return None,
    })
}

const COLORS: &str = "; test\n[Grass]\ncolor=100\ncolor=110\ncolor=120\n\
    [Sand]\ncolor = 231\ncolor = 190\ncolor = 91\n[keeper]\ncolor=203\ncolor=174\ncolor=87\n\
    [animal]\ncolor=0\ncolor=255\ncolor=138\n[fgate]\ncolor=246\ncolor=255\ncolor=0\n\
    [zoowall]\ncolor=196\ncolor=202\ncolor=188\n[tankwall]\ncolor=128\ncolor=188\ncolor=202\n\
    [freshtank]\ncolor=92\ncolor=185\ncolor=228\n[salttank]\ncolor=70\ncolor=150\ncolor=210\n\
    [showtank]\ncolor=32\ncolor=128\ncolor=255\n[ScreenView]\ncolor=255\ncolor=223\ncolor=115\n\
    [Dirt]\ncolor=1\ncolor=2\n[Gravel]\ncolor=1\ncolor=2\ncolor=3\ncolor=4\ncolor=5\ncolor=6\n";

fn minimap() -> Minimap {
    Minimap::new(COLORS.as_bytes(), b"")
}

/// A 12 by 12 map of grass whose entrance gate stands on tile (1, 1), with a zoo wall east of
/// column 3 so that the tiles beyond it are inside the zoo, and `entities` added.
fn world(entities: Vec<Entity>) -> World {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let mut zoo = Zoo {
        width: 12,
        height: 12,
        entrance: (1, 1),
        id_counter: 100,
        tiles: vec![tile; 144],
        ..Default::default()
    };
    zoo.entities.push(entity("gate", 96, 96, 6));
    for row in 0..12 {
        zoo.entities
            .push(entity("zwall", 3 * 64 + 63, row * 64 + 32, 2));
    }
    zoo.entities.extend(entities);
    World::new(zoo, shape)
}

/// An entity standing on the middle of tile `(x, y)`.
fn on(class: &str, x: i32, y: i32, direction: u32) -> Entity {
    entity(class, 64 * x + 32, 64 * y + 32, direction)
}

#[test]
fn the_colour_file_keeps_three_bytes_per_name_and_black_for_the_rest() {
    let colors = Colors::parse(COLORS.as_bytes());
    assert_eq!(colors.get("Sand"), [231, 190, 91]);
    assert_eq!(colors.get("Missing"), [0; 3]);
    // Fewer than three values: ignored; a section met twice: its last three.
    assert_eq!(colors.get("Dirt"), [0; 3]);
    assert_eq!(colors.get("Gravel"), [4, 5, 6]);
    let twice = Colors::parse(
        b"[a]\ncolor=1\ncolor=2\ncolor=3\n[b]\ncolor=9\n[a]\ncolor=7\ncolor=8\ncolor=300\n",
    );
    assert_eq!((twice.get("a"), twice.get("b")), ([7, 8, 44], [0; 3]));
}

#[test]
fn a_tile_takes_the_colour_of_its_terrain_and_four_times_its_elevation() {
    let mut w = world(vec![]);
    let m = minimap();
    assert_eq!(m.cell_colour(&w, None, (5, 5)), [100, 110, 120]);
    w.zoo.tiles[5 * 12 + 5].terrain = 2;
    w.zoo.tiles[5 * 12 + 5].elevation = 1;
    assert_eq!(m.cell_colour(&w, None, (5, 5)), [235, 194, 95]);
    // A terrain with no name, a negative elevation clamped at 0, and a tile off the map.
    w.zoo.tiles[5 * 12 + 5].terrain = 99;
    w.zoo.tiles[5 * 12 + 5].elevation = -3;
    assert_eq!(m.cell_colour(&w, None, (5, 5)), [0; 3]);
    w.zoo.tiles[5 * 12 + 5].terrain = 0;
    assert_eq!(m.cell_colour(&w, None, (5, 5)), [88, 98, 108]);
    for off in [(12, 0), (0, 12), (-1, 3), (3, -1), (i64::MAX, 0)] {
        assert_eq!(m.cell_colour(&w, None, off), [0; 3], "{off:?}");
    }
}

#[test]
fn the_first_case_of_the_list_decides_the_colour() {
    let m = minimap();
    let colour = |entities: Vec<Entity>, gate| m.cell_colour(&world(entities), gate, (5, 5));
    // Staff over an animal over the rest; a guest has no colour of its own.
    let both = vec![
        on("animals", 5, 5, 0),
        on("keeper", 5, 5, 0),
        on("path", 5, 5, 0),
    ];
    assert_eq!(colour(both.clone(), None), [203, 174, 87]);
    assert_eq!(colour(both[..1].to_vec(), None), [0, 255, 138]);
    assert_eq!(colour(vec![on("guests", 5, 5, 0)], None), [100, 110, 120]);
    // A path: minus 16; an exhibit fence on the edge: plus 16, 16, minus 16; both: minus 16.
    assert_eq!(colour(vec![on("path", 5, 5, 0)], None), [84, 94, 104]);
    assert_eq!(colour(vec![on("plain", 5, 5, 2)], None), [116, 126, 104]);
    assert_eq!(
        colour(vec![on("plain", 5, 5, 2), on("path", 5, 5, 0)], None),
        [84, 94, 104]
    );
    // The zoo wall wins over a tank wall, in any order, and both win over the path.
    let wall = |class| on(class, 5, 5, 4);
    assert_eq!(
        colour(vec![wall("twall"), on("zwall", 5, 5, 0)], None),
        [196, 202, 188]
    );
    assert_eq!(
        colour(vec![wall("twall"), on("path", 5, 5, 0)], None),
        [128, 188, 202]
    );
    // The entrance building holds the middle of its tile.
    let gate = colour(vec![], Some(0));
    assert_eq!(gate, [100, 110, 120], "tile (5, 5) is not the gate's");
    assert_eq!(
        m.cell_colour(&world(vec![]), Some(0), (1, 1)),
        [246, 255, 0]
    );
    assert_eq!(m.cell_colour(&world(vec![]), None, (1, 1)), [100, 110, 120]);
    // The elevation comes last, on every case.
    let mut w = world(vec![on("keeper", 5, 5, 0)]);
    w.zoo.tiles[5 * 12 + 5].elevation = 2;
    assert_eq!(m.cell_colour(&w, None, (5, 5)), [211, 182, 95]);
}

#[test]
fn an_exhibit_that_is_not_a_tank_is_greener_and_a_tank_takes_the_colour_of_its_water() {
    let m = minimap();
    // A pen of 3 by 3 tiles, (5, 5) to (7, 7), fenced on its outside edges.
    let pen = |class| {
        let mut fences = Vec::new();
        for i in 5..8 {
            fences.extend([on(class, i, 5, 0), on(class, i, 7, 4)]);
            fences.extend([on(class, 5, i, 6), on(class, 7, i, 2)]);
        }
        fences
    };
    let w = world(pen("plain"));
    assert_eq!(m.cell_colour(&w, None, (6, 6)), [68, 142, 88]);
    // Its edge tiles take the fence colour (the fence case comes first), the ground around it
    // nothing.
    assert_eq!(m.cell_colour(&w, None, (5, 6)), [116, 126, 104]);
    assert_eq!(m.cell_colour(&w, None, (9, 6)), [100, 110, 120]);
    let mut w = world(pen("twall"));
    let id = w.tank_at(6 * 12 + 6).expect("a tank");
    fn tank(w: &mut World, id: u32) -> &mut crate::exhibit::Tank {
        w.exhibits_mut().get_mut(id).unwrap().tank.as_mut().unwrap()
    }
    tank(&mut w, id).salt = false;
    assert_eq!(m.cell_colour(&w, None, (6, 6)), [92, 185, 228]);
    assert_eq!(m.cell_colour(&w, None, (5, 6)), [128, 188, 202]);
    w.exhibits_mut()
        .get_mut(id)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .salt = true;
    assert_eq!(m.cell_colour(&w, None, (6, 6)), [70, 150, 210]);
    w.exhibits_mut()
        .get_mut(id)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .show = true;
    assert_eq!(m.cell_colour(&w, None, (6, 6)), [32, 128, 255]);
}

#[test]
fn the_picture_has_the_diamond_the_frame_and_keys_out_the_rest() {
    let w = world(vec![]);
    let mut m = minimap();
    let view = View::default();
    let camera = Camera::open(&view, (1024, 768), Settings::default());
    let back = [255, 0, 255];
    let picture = m.draw(&w, &camera, None, (139, 69), back);
    assert_eq!((picture.width, picture.height), (139, 69));
    let at = |x: usize, y: usize| picture.pixels[(y * 139 + x) * 4..][..3].to_vec();
    // The corners are outside the diamond; the 12 by 12 map is centred: 63 pixels from X = 63.
    assert_eq!(at(0, 0), fill_color(back)[..3]);
    assert_eq!(at(138, 68), fill_color(back)[..3]);
    let grass = fill_color([100, 110, 120])[..3].to_vec();
    // Every pixel is back, the grass colour or the frame.
    let view_colour = fill_color([255, 223, 115])[..3].to_vec();
    let wall = fill_color([196, 202, 188])[..3].to_vec();
    let others = picture
        .pixels
        .chunks(4)
        .filter(|p| {
            p[..3] != fill_color(back)[..3]
                && p[..3] != grass[..]
                && p[..3] != view_colour[..]
                && p[..3] != wall[..]
        })
        .count();
    assert_eq!(others, 0);
    // One pixel for each of the 144 cells.
    let drawn = picture
        .pixels
        .chunks(4)
        .filter(|p| p[..3] != fill_color(back)[..3])
        .count();
    assert_eq!(drawn, 144);
}

#[test]
fn the_picture_turns_with_the_view() {
    let mut zoo_world = world(vec![]);
    zoo_world.zoo.tiles[3 * 12 + 8].terrain = 2;
    let mut m = minimap();
    let sand = fill_color([231, 190, 91])[..3].to_vec();
    let spot = |rotation: i32| {
        let view = View {
            rotation,
            ..View::default()
        };
        let camera = Camera::open(&view, (64, 32), Settings::default());
        let picture = m.draw(&zoo_world, &camera, None, (139, 69), [255, 0, 255]);
        let at = picture
            .pixels
            .chunks(4)
            .position(|p| p[..3] == sand[..])
            .unwrap();
        (at % 139, at / 139)
    };
    let spots: Vec<_> = (0..4).map(spot).collect();
    assert_eq!(
        spots.len(),
        spots.iter().collect::<std::collections::HashSet<_>>().len()
    );
}

#[test]
fn a_click_gives_the_tile_that_the_view_then_centres_on() {
    let w = world(vec![]);
    let mut m = minimap();
    let mut camera = Camera::open(&View::default(), (1024, 768), Settings::default());
    assert_eq!(m.tile_at((70, 34)), None, "before the first picture");
    let picture = m.draw(&w, &camera, None, (139, 69), [255, 0, 255]);
    let (grid, block) = m.shown.unwrap();
    for (c, r) in [(0, 0), (3, 7), (11, 11), (11, 0)] {
        let (x, y) = block.at(c, r);
        assert_eq!(m.tile_at((x as i32, y as i32)), Some((c, r)));
    }
    // A pixel the diamond does not cover: nothing, and the keyed-out colour is there.
    assert_eq!(m.tile_at((0, 0)), None);
    assert_eq!(picture.pixels[..3], fill_color([255, 0, 255])[..3]);
    let (x, y) = block.at(7, 3);
    let (tx, ty) = m.tile_at((x as i32, y as i32)).unwrap();
    assert_eq!((tx, ty), grid.tile(7, 3));
    camera.centre_on(&w.zoo, tx as u32, ty as u32);
    let (wx, wy, _) = camera.centre_point(&w.zoo);
    assert_eq!((wx / 64, wy / 64), (tx, ty));
    // The cells at 0 of a turned view show a tile off the map, and a click there centres off it.
    let view = View {
        rotation: 1,
        ..View::default()
    };
    let turned = Camera::open(&view, (1024, 768), Settings::default());
    m.draw(&w, &turned, None, (139, 69), [255, 0, 255]);
    let (_, block) = m.shown.unwrap();
    let (x, y) = block.at(0, 4);
    assert_eq!(m.tile_at((x as i32, y as i32)), Some((4, 12)));
}

#[test]
fn the_picture_is_due_on_a_view_move_and_every_interval() {
    let camera = Camera::open(&View::default(), (1024, 768), Settings::default());
    let due = |ini: &[u8], times: &[u64]| {
        let mut m = Minimap::new(COLORS.as_bytes(), ini);
        times.iter().map(|&t| m.due(t, &camera)).collect::<Vec<_>>()
    };
    let t = [0, 10, 20, 30];
    // Missing and 0: every pass; negative: every pass; huge: the first only (the view
    // does not move).
    assert_eq!(due(b"", &t), [true; 4]);
    assert_eq!(due(b"[UI]\nminimapUpdate=0\n", &t), [true; 4]);
    assert_eq!(due(b"[UI]\nminimapUpdate=-5\n", &t), [true; 4]);
    assert_eq!(
        due(b"[UI]\nminimapUpdate=2147483647\n", &t),
        [true, false, false, false]
    );
    assert_eq!(
        due(b"[UI]\nminimapUpdate=20\n", &t),
        [true, false, true, false]
    );
    // A view move is due at once.
    let mut m = Minimap::new(COLORS.as_bytes(), b"[UI]\nminimapUpdate=2147483647\n");
    let mut moved = camera.clone();
    assert!(m.due(0, &moved) && !m.due(5, &moved));
    moved.scroll.0 += 1;
    assert!(m.due(6, &moved) && !m.due(7, &moved));
    moved.rotation = 2;
    assert!(m.due(8, &moved));
}
