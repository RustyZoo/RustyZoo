//! The pass of the map view on a synthetic map: one grass tile picture of two pixels.

use super::*;
use crate::entity::Catalogue;
use crate::render::Options;
use crate::render::order::project;
use crate::render::tests::{cache_install, flat_map, frame_file, tile_with};
use crate::vfs::Vfs;

const SIZE: (u32, u32) = (64, 48);

/// A flat map of 40 by 40 tiles whose camera looks at the middle of it.
fn map() -> Zoo {
    let (px, py) = project(0, 1280, 1280, 0);
    let mut zoo = tile_with(&[]);
    zoo.width = 40;
    zoo.height = 40;
    zoo.tiles = flat_map(40, 40);
    zoo.view.scroll = (px - SIZE.0 as i32 / 2, py - SIZE.1 as i32 / 2);
    zoo
}

/// The window of the camera drawn as `render-map --view` draws it.
fn reference(renderer: &mut MapRenderer, camera: &Camera) -> Framebuffer {
    let options = Options {
        window: Some((camera.scroll.0, camera.scroll.1, SIZE.0, SIZE.1)),
        ..Options::default()
    };
    renderer
        .render_with(&map(), camera.rotation, &options)
        .unwrap()
        .0
}

/// Runs `body` with an app (periods 66 and 16, cap 200) and a second renderer of the same game.
fn with_app(name: &str, body: impl FnOnce(&mut App, &mut MapRenderer)) {
    let install = cache_install(name, frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    body(&mut app, &mut MapRenderer::new(&vfs, &catalogue));
}

fn firing(app: &App) -> (bool, bool, Option<u32>) {
    let f = app.fired();
    (f.draw, f.interface.is_some(), f.step)
}

#[test]
fn the_timers_fire_in_the_sequence_of_a_scripted_clock() {
    with_app("app-timing", |app, _| {
        // A pass every 10 ms: the first fires all three, the step with the cap; then the draw
        // every 20 ms; at 70 ms the interface and the step (more than 66 ms) together.
        let got: Vec<_> = (0..9)
            .map(|pass| {
                app.pass(pass * 10, &[]);
                firing(app)
            })
            .collect();
        let (none, draw) = ((false, false, None), (true, false, None));
        let first = (true, true, Some(200));
        let both = (false, true, Some(70));
        assert_eq!(got, [first, none, draw, none, draw, none, draw, both, draw]);
    });
}

#[test]
fn the_log_tells_the_commands_of_a_pass_before_its_timers() {
    with_app("app-commands-log", |app, _| {
        app.pass(0, &[]);
        app.pass(20, &[Command::Scroll(1, 0), Command::Pointer(None)]);
        let lines = app.log_lines(1, 20);
        assert_eq!(
            lines[..2],
            ["pass 1 t=20 scroll 1 0", "pass 1 t=20 pointer none"]
        );
        assert!(lines[2].starts_with("pass 1 t=20 draw "));
        assert_eq!(lines.len(), 3);
        assert_eq!(app.log_header(), "size 64x48");
        // A pass that applied nothing and fired nothing has no line.
        app.pass(25, &[]);
        assert!(app.log_lines(2, 25).is_empty());
    });
}

#[test]
fn the_log_has_one_line_per_firing_in_the_order_of_the_timers() {
    with_app("app-log", |app, _| {
        app.pass(0, &[]);
        let view = app.camera().scroll;
        let lines = app.log_lines(0, 0);
        assert_eq!(lines.len(), 3);
        let (draw, hash) = lines[0].split_once(" view=").unwrap();
        assert!(
            draw.starts_with("pass 0 t=0 draw ") && !draw.ends_with(" -"),
            "{draw}"
        );
        assert_eq!(hash, format!("{},{} rotation=0 zoom=0", view.0, view.1));
        assert_eq!(lines[1], "pass 0 t=0 interface 66");
        // The cap of 200 ms is 24 minutes of game time.
        assert_eq!(lines[2], "pass 0 t=0 step 200 2001-01-01 00:24:00.000");
        // A draw that paints nothing says so.
        app.pass(20, &[]);
        assert_eq!(app.log_lines(1, 20).len(), 1);
        assert!(app.log_lines(1, 20)[0].starts_with("pass 1 t=20 draw - view="));
    });
}

#[test]
fn a_draw_paints_only_when_the_camera_moved_and_commands_act_in_their_own_pass() {
    with_app("app-commands", |app, other| {
        let start = app.camera().scroll;
        // A draw in every pass (20 ms); the interface and the step are not due after the first.
        let first = app.pass(0, &[]).unwrap().pixels.clone();
        assert_eq!(first, reference(other, app.camera()).pixels);
        assert!(
            first.chunks(4).any(|p| p[..3] != [0, 0, 0]),
            "the map is in view"
        );
        assert!(
            app.pass(20, &[]).is_none() && app.fired().draw,
            "nothing moved"
        );
        assert!(app.pass(40, &[]).is_none());
        // A key scroll before the pass is in its frame.
        let frame = app
            .pass(60, &[Command::Scroll(1, 0)])
            .unwrap()
            .pixels
            .clone();
        assert!(app.camera().scroll.0 > start.0);
        assert_eq!(frame, reference(other, app.camera()).pixels);
        assert!(app.pass(80, &[]).is_none());
        // A rotation restages the picture.
        let frame = app
            .pass(100, &[Command::Rotate(true)])
            .unwrap()
            .pixels
            .clone();
        assert_eq!(app.camera().rotation, 1);
        assert_eq!(frame, reference(other, app.camera()).pixels);
        let frame = app
            .pass(120, &[Command::Rotate(false), Command::Rotate(false)])
            .unwrap()
            .pixels
            .clone();
        assert_eq!(app.camera().rotation, 3);
        assert_eq!(frame, reference(other, app.camera()).pixels);
        // A zoom changes the camera and is a frame of its own.
        assert!(app.pass(140, &[Command::Zoom(true)]).is_some());
        assert_eq!(app.camera().zoom, 2);
        app.pass(160, &[Command::Zoom(true)]);
        assert_eq!(app.camera().zoom, 2, "the limit");
        // Quit is a flag the shell reads. The pass that gets it runs no timer, and no command
        // after it; the log still tells the commands it applied.
        assert!(!app.quit());
        assert!(app.pass(180, &[Command::Quit, Command::Pause]).is_none());
        assert!(app.quit() && !app.paused() && !app.fired().draw);
        assert_eq!(app.log_lines(9, 180), ["pass 9 t=180 quit"]);
        assert!(app.pass(200, &[]).is_none() && app.log_lines(10, 200).is_empty());
    });
}

#[test]
fn the_pause_stops_the_calendar_but_not_the_timers() {
    with_app("app-pause", |app, _| {
        let date = |app: &App| {
            let d = app.calendar().date();
            (d.hour, d.minute, d.second)
        };
        // A pass every 70 ms is a step every pass.
        app.pass(0, &[]);
        assert_eq!(date(app), (0, 24, 0), "the first step is the cap");
        app.pass(70, &[]);
        assert_eq!(date(app), (0, 32, 24), "70 ms are 504 s");
        app.pass(140, &[Command::Pause]);
        assert!(app.paused());
        assert_eq!(app.fired().step, Some(70), "the step still fires");
        assert_eq!(date(app), (0, 32, 24));
        assert!(app.log_lines(2, 140).last().unwrap().ends_with(" paused"));
        app.pass(210, &[]);
        assert_eq!(date(app), (0, 32, 24));
        app.pass(280, &[Command::Pause]);
        assert_eq!(date(app), (0, 40, 48), "no time piled up during the pause");
    });
}

#[test]
fn the_interface_pass_scrolls_at_the_edge_where_the_pointer_is() {
    with_app("app-edge", |app, _| {
        let start = app.camera().scroll;
        // The pointer on the left band, halfway down. The interface fires at 0 and 70 ms.
        app.pass(0, &[Command::Pointer(Some((0, 24)))]);
        assert_eq!(app.camera().scroll, (start.0 - 32, start.1));
        // The frame of the next draw shows it.
        assert!(app.pass(20, &[]).is_some());
        assert_eq!(
            app.camera().scroll,
            (start.0 - 32, start.1),
            "no interface pass"
        );
        app.pass(70, &[]);
        assert_eq!(app.camera().scroll, (start.0 - 64, start.1));
        // Away from the band, or outside the window, nothing scrolls.
        app.pass(140, &[Command::Pointer(Some((32, 24)))]);
        app.pass(210, &[Command::Pointer(None)]);
        assert_eq!(app.camera().scroll, (start.0 - 64, start.1));
    });
}

#[test]
fn commands_have_a_text_form_that_reads_back() {
    for command in [
        Command::Scroll(-1, 1),
        Command::Scroll(0, 0),
        Command::Pointer(Some((-3, 700))),
        Command::Pointer(None),
        Command::Rotate(true),
        Command::Rotate(false),
        Command::Zoom(true),
        Command::Zoom(false),
        Command::Pause,
        Command::Quit,
    ] {
        assert_eq!(command.to_string().parse(), Ok(command));
    }
    for bad in [
        "",
        "scroll 2 0",
        "scroll 1",
        "rotate",
        "rotate left",
        "pointer x y",
        "zoom",
        "stop",
    ] {
        assert!(bad.parse::<Command>().is_err(), "{bad:?}");
    }
}

#[test]
fn a_window_of_no_size_or_beyond_the_pixel_limit_is_refused() {
    let install = cache_install("app-size", frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    for size in [(0, 10), (10, 0), (100_000, 100_000)] {
        let app = App::new(
            MapRenderer::new(&vfs, &catalogue),
            map(),
            Settings::default(),
            Timers::new(66, 16, 200),
            size,
        );
        assert!(app.is_err(), "{size:?}");
    }
}
