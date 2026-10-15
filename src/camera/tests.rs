//! The worked examples of the camera spec (12), on synthetic flat maps.

use super::*;
use crate::config::Config;
use crate::render::order::ground_height;
use crate::zoo::Tile;

fn map(width: u32, height: u32, elevation: i32) -> Zoo {
    let tile = Tile {
        elevation,
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
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        tanks: Vec::new(),
        id_counter: 0,
        tiles: vec![tile; (width * height) as usize],
        entities: Vec::new(),
        view: View::default(),
        clocks: Default::default(),
    }
}

fn camera(zoom: i32, rotation: i32, scroll: (i32, i32), brush: u32, size: (i32, i32)) -> Camera {
    let view = View {
        zoom,
        rotation,
        scroll,
        brush,
    };
    Camera::open(&view, size, Settings::default())
}

const BEACH: (i32, i32) = (1024, 768);

#[test]
fn the_range_check_takes_a_key_step_back_to_the_edge() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (3328, 16), 1, BEACH);
    c.key_scroll(&beach, (1, 0));
    assert_eq!(c.scroll, (3371, 9));
    // Pressing on slides along the edge.
    for want in [(3400, -5), (3428, -19), (3456, -33)] {
        c.key_scroll(&beach, (1, 0));
        assert_eq!(c.scroll, want);
    }
}

#[test]
fn the_centre_point_lies_behind_the_true_one_on_flat_ground() {
    let beach = map(75, 75, 0);
    let c = camera(0, 0, (3392, 16), 1, BEACH);
    assert_eq!(c.centre_point(&beach), (3096, 4712, 0));
    // The height is that of the tile: elevation 1 is 16 units, found at the trial height 20.
    let raised = map(75, 75, 1);
    assert_eq!(
        camera(0, 0, (1000, 100), 1, BEACH).centre_point(&raised).2,
        16
    );
    // A view over no tile: the point of the trial height 0, height 0.
    let c = camera(0, 0, (-3000, -3000), 1, BEACH);
    assert_eq!(c.centre_point(&beach), (2744, -7720, 0));
}

#[test]
fn a_rotation_centres_the_tile_under_the_centre() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (1216, 16), 1, BEACH);
    assert_eq!(c.centre_point(&beach), (920, 2536, 0));
    c.rotate(&beach, None, true);
    assert_eq!(c.rotation, 1);
    assert_eq!(c.scroll, (-1312, 480));
    assert_eq!(c.selection, (14, 39));
}

#[test]
fn a_zoom_change_centres_the_tile_too() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (1216, 16), 1, BEACH);
    c.zoom(&beach, None, true);
    assert_eq!((c.zoom, c.scroll), (2, (2944, 416)));
    c.zoom(&beach, None, true);
    assert_eq!(c.zoom, 2, "the button is disabled at the limit");
    let mut c = camera(0, 0, (1216, 16), 1, BEACH);
    c.zoom(&beach, None, false);
    assert_eq!((c.zoom, c.scroll), (-2, (352, -184)));
    c.zoom(&beach, None, false);
    assert_eq!(c.zoom, -2);
}

#[test]
fn a_zoom_change_keeps_the_scroll_when_the_tile_stays_in_the_window() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 1, (208, 84), 1, (800, 600));
    c.selection = (21, 2);
    assert_eq!(c.centre_point(&beach), (1384, 168, 0));
    c.zoom(&beach, None, false);
    assert_eq!((c.zoom, c.scroll), (-2, (208, 84)));
}

#[test]
fn four_quarter_turns_do_not_come_back_to_the_same_scroll() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (3764, -187), 1, BEACH);
    for _ in 0..4 {
        c.rotate(&beach, None, true);
    }
    assert_eq!((c.rotation, c.scroll), (0, (3744, -192)));
}

#[test]
fn a_selected_entity_gives_the_tile_and_the_selection_stays_inside_the_map() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (1216, 16), 3, BEACH);
    assert_eq!(c.brush, 3);
    c.rotate(&beach, Some((10, 64 * 74 + 5)), false);
    assert_eq!((c.rotation, c.selection), (3, (0, 72)));
}

#[test]
fn a_map_is_opened_as_stored() {
    let view = View {
        zoom: 2,
        rotation: 7,
        scroll: (-3000, -3000),
        brush: 0x104,
    };
    let c = Camera::open(&view, BEACH, Settings::default());
    assert_eq!(
        (c.zoom, c.rotation, c.scroll, c.brush),
        (2, 0, (-3000, -3000), 4)
    );
    for (stored, want) in [(0, 1), (1, 1), (2, 2), (4, 4), (5, 5), (9, 5)] {
        let view = View {
            brush: stored,
            ..view
        };
        assert_eq!(Camera::open(&view, BEACH, Settings::default()).brush, want);
    }
    // The first key press applies the range check to a view outside the map.
    let beach = map(75, 75, 0);
    let mut c = Camera::open(&view, BEACH, Settings::default());
    c.key_scroll(&beach, (1, 0));
    assert_ne!(c.scroll.0, -3000 + 64);
    let (wx, wy, _) = c.centre_point(&beach);
    assert!((64..4800 - 64).contains(&wx) && (64..4800 - 64).contains(&wy));
}

#[test]
fn edge_scrolling_uses_the_band_the_step_and_the_delay() {
    let beach = map(75, 75, 0);
    let start = (1888, -384);
    let at = |pointer, now, delay| {
        let mut c = camera(0, 0, start, 1, BEACH);
        c.settings.delay = delay;
        c.edge_scroll(&beach, now, pointer);
        c.scroll
    };
    let (x, y) = start;
    assert_eq!(at(Some((0, 300)), 0, 1), (x - 32, y));
    assert_eq!(at(Some((7, 300)), 0, 1), (x - 32, y));
    assert_eq!(at(Some((8, 300)), 0, 1), start);
    assert_eq!(at(Some((1016, 300)), 0, 1), start);
    assert_eq!(at(Some((1017, 300)), 0, 1), (x + 32, y));
    assert_eq!(at(Some((300, 7)), 0, 1), (x, y - 32));
    assert_eq!(at(Some((300, 761)), 0, 1), (x, y + 32));
    assert_eq!(at(Some((1023, 767)), 0, 1), (x + 32, y + 32));
    assert_eq!(at(Some((0, 0)), 0, 1), (x - 32, y - 32));
    assert_eq!(at(Some((1024, 300)), 0, 1), start, "outside the window");
    assert_eq!(at(None, 0, 1), start);

    // The steps fall on the first pass at or after the next allowed time.
    let mut c = camera(0, 0, start, 1, BEACH);
    c.settings.delay = 200;
    let mut steps = Vec::new();
    for pass in 0..9u64 {
        let before = c.scroll.0;
        c.edge_scroll(&beach, pass * 66, Some((1023, 300)));
        if c.scroll.0 != before {
            steps.push(pass * 66);
        }
    }
    assert_eq!(steps, [0, 264, 528]);
}

#[test]
fn the_settings_come_from_the_ini_with_the_shipped_values_as_defaults() {
    let shipped = Settings::default();
    assert_eq!(
        (shipped.key, shipped.mouse, shipped.delay),
        ((64, 64), 32, 1)
    );
    let ini = Config::parse(
        b"[UI]\nkeyScrollX=30\nkeyScrollY=50\nmouseScrollX=10\nmouseScrollY=20\nmouseScrollDelay=200\n",
    )
    .unwrap();
    let s = Settings::from_config(&ini);
    assert_eq!((s.key, s.mouse, s.delay), ((30, 50), 10, 200));
    assert_eq!(
        Settings::from_config(&Config::parse(b"[UI]\n").unwrap()),
        shipped
    );
    assert_eq!(Settings::from_config(&Config::default()), shipped);

    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (1888, -384), 1, BEACH);
    c.settings = s;
    c.key_scroll(&beach, (1, -1));
    assert_eq!(c.scroll, (1918, -434), "the key steps are in view pixels");
    let mut c = camera(-2, 0, (944, -192), 1, BEACH);
    c.settings = s;
    c.key_scroll(&beach, (-1, 1));
    assert_eq!(c.scroll, (914, -142), "whatever the zoom");
}

#[test]
fn absurd_values_never_panic() {
    let tiny = map(1, 1, 0);
    let empty = map(0, 0, 0);
    for m in [&tiny, &empty] {
        for scroll in [(i32::MIN, i32::MAX), (i32::MAX, i32::MAX), (0, 0)] {
            for (zoom, rotation) in [(-9, 3), (9, 2), (0, 1)] {
                let mut c = camera(zoom, rotation, scroll, u32::MAX, (0, 0));
                c.key_scroll(m, (1, 1));
                c.edge_scroll(m, u64::MAX, Some((0, 0)));
                c.rotate(m, Some((i32::MIN, i32::MAX)), true);
                c.zoom(m, None, false);
                c.zoom(m, None, true);
            }
        }
    }
}

#[test]
fn a_scroll_by_zero_does_nothing_not_even_the_range_check() {
    let beach = map(75, 75, 0);
    let off_map = |key| {
        let view = View {
            zoom: 0,
            rotation: 0,
            scroll: (-3000, -3000),
            brush: 1,
        };
        Camera::open(
            &view,
            BEACH,
            Settings {
                key,
                mouse: 32,
                delay: 1,
            },
        )
    };
    let mut c = off_map((0, 64));
    c.key_scroll(&beach, (1, 0));
    assert_eq!(c.scroll, (-3000, -3000));
    // Pointer inside the window, outside the band.
    let mut c = off_map((64, 64));
    c.edge_scroll(&beach, 1000, Some((512, 384)));
    assert_eq!(c.scroll, (-3000, -3000));
}

#[test]
fn the_zoom_value_is_the_factor() {
    // The point (3, 7) is at (5, 1) before the zoom; 2.4: divide by -z below -1, multiply by z above 1.
    for (zoom, want) in [
        (-4, (1, 0)),
        (-3, (1, 0)),
        (-2, (2, 0)),
        (-1, (5, 1)),
        (1, (5, 1)),
        (2, (10, 2)),
        (4, (20, 4)),
    ] {
        assert_eq!(project_zoomed(0, zoom, 3, 7, 0), want, "zoom {zoom}");
    }
    // The picking does the reverse: at zoom 4 the view point (2000, 0) is the one at zoom 0 (500, 0).
    let beach = map(75, 75, 0);
    let at = |zoom, scroll| camera(zoom, 0, scroll, 1, (0, 0)).centre_point(&beach);
    assert_eq!(at(4, (2000, 0)), at(0, (500, 0)));
    assert_eq!(at(-4, (500, 0)), at(0, (2000, 0)));
}

#[test]
fn full_screen_has_a_band_of_two_pixels() {
    let beach = map(75, 75, 0);
    let start = (1888, -384);
    let at = |pointer, fullscreen| {
        let mut c = camera(0, 0, start, 1, BEACH);
        c.set_fullscreen(&beach, fullscreen);
        c.edge_scroll(&beach, 0, pointer);
        c.scroll
    };
    let (x, y) = start;
    assert_eq!(at(Some((2, 300)), true), (x - 32, y));
    assert_eq!(at(Some((3, 300)), true), start);
    assert_eq!(at(Some((1021, 300)), true), start);
    assert_eq!(at(Some((1022, 300)), true), (x + 32, y));
    assert_eq!(at(Some((300, 765)), true), start);
    assert_eq!(at(Some((300, 766)), true), (x, y + 32));
    assert_eq!(at(Some((3, 300)), false), (x - 32, y), "windowed: 7");
    assert_eq!(at(Some((1021, 300)), false), (x + 32, y));
}

#[test]
fn full_screen_at_800_wide_does_not_zoom_in() {
    let beach = map(75, 75, 0);
    let mut c = camera(0, 0, (1216, 16), 1, (800, 600));
    c.set_fullscreen(&beach, true);
    c.zoom(&beach, None, true);
    assert_eq!(c.zoom, 0, "refused at zoom 0");
    c.zoom(&beach, None, false);
    c.zoom(&beach, None, true);
    assert_eq!(c.zoom, 0);
    // Windowed again, zoom 2 is offered.
    c.set_fullscreen(&beach, false);
    c.zoom(&beach, None, true);
    assert_eq!(c.zoom, 2);
    // Going to full screen zooms a view at zoom 2 out to 0, with the steps of a zoom change.
    let mut want = camera(2, 0, c.scroll, 1, (800, 600));
    want.zoom(&beach, None, false);
    c.set_fullscreen(&beach, true);
    assert_eq!((c.zoom, c.scroll), (want.zoom, want.scroll));
    assert_eq!(c.zoom, 0);
    // Other widths are not affected, nor is zoom out.
    let mut c = camera(0, 0, (1216, 16), 1, BEACH);
    c.set_fullscreen(&beach, true);
    c.zoom(&beach, None, true);
    assert_eq!(c.zoom, 2);
    c.set_fullscreen(&beach, true);
    assert_eq!(c.zoom, 2, "only at 800");
    let mut c = camera(0, 0, (1216, 16), 1, (800, 600));
    c.set_fullscreen(&beach, true);
    c.zoom(&beach, None, false);
    assert_eq!(c.zoom, -2);
}

/// A 12 by 12 map with a hill of two plateau tiles (one and two steps) on flat ground.
fn hill() -> Zoo {
    let mut zoo = map(12, 12, 0);
    zoo.tiles[5 * 12 + 5].elevation = 1;
    zoo.tiles[5 * 12 + 6].elevation = 2;
    zoo
}

/// The window point of the middle of a tile, at the ground height there.
fn window_point(c: &Camera, zoo: &Zoo, tx: u32, ty: u32) -> (i32, i32) {
    let z = ground_height(zoo.tile(tx, ty).unwrap(), 32, 32);
    let (px, py) = project_zoomed(
        c.rotation,
        c.zoom,
        64 * tx as i32 + 32,
        64 * ty as i32 + 32,
        z,
    );
    (
        (px - i64::from(c.scroll.0)) as i32,
        (py - i64::from(c.scroll.1)) as i32,
    )
}

#[test]
fn a_window_point_picks_the_tile_it_was_projected_from_at_every_rotation_and_zoom() {
    for zoo in [map(12, 12, 0), map(12, 12, 3), hill()] {
        for rotation in 0..4 {
            for zoom in [-2, 0, 2] {
                let c = camera(zoom, rotation, (-300, -200), 1, BEACH);
                // A hill hides what is behind it: only the tiles in the open and the plateaus
                // themselves are asked for.
                for (tx, ty) in [(0, 0), (11, 0), (0, 11), (11, 11), (3, 8), (8, 3), (6, 5)] {
                    let at = window_point(&c, &zoo, tx, ty);
                    assert_eq!(
                        c.tile_under(&zoo, at),
                        Some((tx, ty)),
                        "tile {tx},{ty} rotation {rotation} zoom {zoom}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_picked_height_is_the_ground_and_the_centre_is_the_pick_of_the_middle() {
    let zoo = hill();
    let c = camera(0, 0, (-300, -200), 1, BEACH);
    let at = window_point(&c, &zoo, 6, 5);
    assert_eq!(c.pick(&zoo, at).2, 32, "two steps");
    assert_eq!(
        c.pick(&zoo, (BEACH.0 / 2, BEACH.1 / 2)),
        c.centre_point(&zoo)
    );
}

#[test]
fn a_point_off_the_map_picks_no_tile() {
    let zoo = map(12, 12, 0);
    let c = camera(0, 0, (-300, -200), 1, BEACH);
    for at in [
        (-5000, 0),
        (0, -5000),
        (i32::MIN, i32::MIN),
        (i32::MAX, i32::MAX),
        (0, i32::MAX),
    ] {
        assert_eq!(c.tile_under(&zoo, at), None, "{at:?}");
    }
}
