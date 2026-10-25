//! The pass of the map view on a synthetic map: one grass tile picture of two pixels.

use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::menu::Menu;
use crate::render::Options;
use crate::render::order::project;
use crate::render::tests::{
    cache_install, cache_install_with, flat_map, frame_file, grid_members, tile_with,
};
use crate::sprite::tests::{Install, sized, sprite_file};
use crate::ui::interface::Interface;
use crate::vfs::Vfs;

/// A catalogue of a kiosk of 1 tile that costs 100, a hall of 2 x 2 tiles that costs 200 and a
/// fence; the kiosk is a recolourable building whose group gives it a default.
fn building_catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let ty = |class, name: &[u8], ini: &[u8], footprint| crate::entity::EntityType {
        class,
        type_name: name.to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini).unwrap())),
        name_id: 0,
        footprint,
        height: 0,
    };
    cat.push(ty(
        "building",
        b"kiosk",
        b"[Characteristics/Integers]\ncPurchaseCost=100\ncIsColorReplaced=1\n\
          [colorrep]\ncolor=cr_color\nreplace=cr_part1\ndefaultpal=pals/blue.pal\n\
          [cr_color]\nncolors=2\n",
        [2, 2, 1],
    ));
    cat.push(ty(
        "building",
        b"hall",
        b"[Characteristics/Integers]\ncPurchaseCost=200\n",
        [4, 4, 1],
    ));
    cat.push(ty("fences", b"iron", b"", [1, 1, 1]));
    cat
}

#[test]
fn a_building_is_placed_on_flat_ground_and_refused_on_a_hill_or_across_a_fence() {
    let install = cache_install_with(
        "app-building",
        frame_file(),
        &[(
            "building.ai",
            b"[cr_part1]\nncolors=2\npal=pals/red.pal\npal=pals/blue.pal\n".to_vec(),
        )],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = building_catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_interface(interface("building"));
    app.set_money(Money {
        cash: 450.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    // The view is set to look at the tile before each click (the window is tiny).
    let click = |app: &mut App, tx: i32, ty: i32| {
        let game = app.game.as_mut().unwrap();
        let centre = project(game.camera.rotation, 64 * tx + 32, 64 * ty + 32, 0);
        let (px, py) = crate::camera::zoom_view(game.camera.zoom, centre);
        game.camera.scroll = (px as i32 - SIZE.0 as i32 / 2, py as i32 - SIZE.1 as i32 / 2);
        let (x, y) = tile_point(&game.camera, tx, ty);
        [
            Command::Mouse(Mouse::Move(x, y)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ]
    };
    let lines = |app: &App, pass, now| -> Vec<String> {
        let all = app.log_lines(pass, now);
        let kept = all.iter().filter_map(|l| l.splitn(4, ' ').nth(3));
        let wanted = |l: &&str| {
            ["tool down", "tool up", "tool refused", "tool message"]
                .iter()
                .any(|k| l.starts_with(k))
                || l.starts_with("world ") && !l.starts_with("world stage")
                || l.starts_with("money cash")
        };
        kept.filter(wanted).map(str::to_string).collect()
    };
    // The kiosk on flat ground: the add with the four quadrants it holds, the charge, and the
    // colour choice of the group's default (the second variant) stored with it.
    app.game.as_mut().unwrap().build.hold(0);
    let go = click(&mut app, 20, 19);
    app.pass(20, &go);
    assert_eq!(
        lines(&app, 1, 20),
        [
            "tool down 20,19",
            "tool up 20,19",
            "world add building/kiosk/ 20,19 id 1 quadrants 4",
            "money cash 350"
        ]
    );
    let game = app.game.as_ref().unwrap();
    assert_eq!(
        game.world.zoo.entities[0].common.as_ref().unwrap().colors,
        [1]
    );
    // The same click finds the quadrants held: the message of something in the way.
    let go = click(&mut app, 20, 19);
    app.pass(40, &go);
    assert_eq!(
        lines(&app, 2, 40),
        ["tool down 20,19", "tool up 20,19", "tool message 10555"]
    );
    // On a hill nothing is added or charged and the refusal is logged.
    app.game
        .as_mut()
        .unwrap()
        .world
        .set_tile(22, 19, 0, 0x50, 0);
    let go = click(&mut app, 22, 19);
    app.pass(60, &go);
    assert_eq!(
        lines(&app, 3, 60),
        ["tool down 22,19", "tool up 22,19", "tool refused"]
    );
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 1);
    // A hall of 2 x 2 tiles over the east side of tile (29, 30) is refused by the fence inside.
    let fence = {
        let renderer = &app.renderer;
        let at = Pending {
            ty: 2,
            x: 29 * 64 + 63,
            y: 30 * 64 + 32,
            z: 0,
            facing: 2,
            tile: (29, 30),
            quadrant: 0,
            verdict: crate::place::Verdict::Valid,
            message: None,
            price: 0.0,
            paint: None,
            blocking: Vec::new(),
            wall: 0,
        };
        let e = crate::place::entity(&renderer.catalogue().types[2], &at, Vec::new());
        let shape = renderer.shape(&e);
        (e, shape)
    };
    app.game
        .as_mut()
        .unwrap()
        .world
        .add(fence.0, fence.1)
        .unwrap();
    app.game.as_mut().unwrap().build.hold(1);
    let go = click(&mut app, 30, 30);
    app.pass(80, &go);
    assert_eq!(
        lines(&app, 4, 80),
        ["tool down 30,30", "tool up 30,30", "tool refused"]
    );
    // Three tiles away from it the hall is placed.
    let go = click(&mut app, 34, 30);
    app.pass(100, &go);
    assert_eq!(
        lines(&app, 5, 100),
        [
            "tool down 34,30",
            "tool up 34,30",
            "world add building/hall/ 34,30 id 3 quadrants 16",
            "money cash 150"
        ]
    );
}

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
        zoom: camera.zoom,
        ..Options::default()
    };
    renderer
        .render_with(&map(), camera.rotation, &options)
        .unwrap()
        .0
}

/// Runs `body` with an app (periods 66 and 16, cap 200) and a second renderer of the same game.
fn with_app(name: &str, body: impl FnOnce(&mut App, &mut MapRenderer)) {
    with_grid_app(name, &[], body)
}

/// As [`with_app`], with more members in the install.
fn with_grid_app(
    name: &str,
    extra: &[(&str, Vec<u8>)],
    body: impl FnOnce(&mut App, &mut MapRenderer),
) {
    let install = cache_install_with(name, frame_file(), extra);
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
        let view = app.camera().unwrap().scroll;
        let lines = app.log_lines(0, 0);
        assert_eq!(lines.len(), 7);
        // The cash comes first (a game without a stored cash has none).
        assert_eq!(lines[0], "pass 0 t=0 money cash 0");
        let (draw, hash) = lines[1].split_once(" view=").unwrap();
        assert!(
            draw.starts_with("pass 0 t=0 draw ") && !draw.ends_with(" -"),
            "{draw}"
        );
        assert_eq!(hash, format!("{},{} rotation=0 zoom=0", view.0, view.1));
        assert_eq!(lines[2], "pass 0 t=0 interface 66");
        // The cap of 200 ms is 24 minutes of game time.
        assert_eq!(lines[3], "pass 0 t=0 step 200 2001-01-01 00:24:00.000");
        // The two seedings of the start tell the seconds the wall clock gave.
        assert!(
            lines[4..6]
                .iter()
                .all(|l| l.starts_with("pass 0 t=0 sim seed "))
        );
        assert_eq!(lines[6], "pass 0 t=0 sim step 200");
        // A draw that paints nothing says so.
        app.pass(20, &[]);
        assert_eq!(app.log_lines(1, 20).len(), 1);
        assert!(app.log_lines(1, 20)[0].starts_with("pass 1 t=20 draw - view="));
    });
}

#[test]
fn a_draw_paints_only_when_the_camera_moved_and_commands_act_in_their_own_pass() {
    with_app("app-commands", |app, other| {
        let start = app.camera().unwrap().scroll;
        // A draw in every pass (20 ms); the interface and the step are not due after the first.
        let first = app.pass(0, &[]).unwrap().pixels.clone();
        assert_eq!(first, reference(other, app.camera().unwrap()).pixels);
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
        assert!(app.camera().unwrap().scroll.0 > start.0);
        assert_eq!(frame, reference(other, app.camera().unwrap()).pixels);
        assert!(app.pass(80, &[]).is_none());
        // A rotation restages the picture.
        let frame = app
            .pass(100, &[Command::Rotate(true)])
            .unwrap()
            .pixels
            .clone();
        assert_eq!(app.camera().unwrap().rotation, 1);
        assert_eq!(frame, reference(other, app.camera().unwrap()).pixels);
        let frame = app
            .pass(120, &[Command::Rotate(false), Command::Rotate(false)])
            .unwrap()
            .pixels
            .clone();
        assert_eq!(app.camera().unwrap().rotation, 3);
        assert_eq!(frame, reference(other, app.camera().unwrap()).pixels);
        // A zoom changes the camera and is a frame of its own.
        let frame = app
            .pass(140, &[Command::Zoom(true)])
            .unwrap()
            .pixels
            .clone();
        assert_eq!(app.camera().unwrap().zoom, 2);
        assert_eq!(frame, reference(other, app.camera().unwrap()).pixels);
        app.pass(160, &[Command::Zoom(true)]);
        assert_eq!(app.camera().unwrap().zoom, 2, "the limit");
        // Zoomed out, the frame is the zoomed out window of the same camera.
        for (now, want) in [(180, 0), (200, -2)] {
            let frame = app
                .pass(now, &[Command::Zoom(false)])
                .unwrap()
                .pixels
                .clone();
            assert_eq!(app.camera().unwrap().zoom, want);
            assert_eq!(frame, reference(other, app.camera().unwrap()).pixels);
        }
        // Quit is a flag the shell reads. The pass that gets it runs no timer, and no command
        // after it; the log still tells the commands it applied.
        assert!(!app.quit());
        assert!(app.pass(220, &[Command::Quit, Command::Pause]).is_none());
        assert!(app.quit() && !app.paused() && !app.fired().draw);
        assert_eq!(app.log_lines(9, 220), ["pass 9 t=220 quit"]);
        assert!(app.pass(240, &[]).is_none() && app.log_lines(10, 240).is_empty());
    });
}

#[test]
fn the_grid_command_toggles_the_grid_lines_in_the_frame() {
    with_grid_app("app-grid", &grid_members(), |app, other| {
        let off = app.pass(0, &[]).unwrap().pixels.clone();
        let grid_reference = |renderer: &mut MapRenderer, camera: &Camera| {
            let options = Options {
                window: Some((camera.scroll.0, camera.scroll.1, SIZE.0, SIZE.1)),
                zoom: camera.zoom,
                grid: true,
                ..Options::default()
            };
            renderer.render_with(&map(), 0, &options).unwrap().0
        };
        let on = app.pass(20, &[Command::Grid]).unwrap().pixels.clone();
        assert_ne!(on, off);
        assert_eq!(on, grid_reference(other, app.camera().unwrap()).pixels);
        assert!(app.log_lines(1, 20)[0].ends_with("20 grid"));
        // Scrolling keeps the grid; a second toggle gives the first frame back.
        let moved = app
            .pass(40, &[Command::Scroll(1, 0)])
            .unwrap()
            .pixels
            .clone();
        assert_eq!(moved, grid_reference(other, app.camera().unwrap()).pixels);
        app.pass(60, &[Command::Scroll(-1, 0)]);
        let back = app.pass(80, &[Command::Grid]).unwrap().pixels.clone();
        assert_eq!(back, off);
    });
}

#[test]
fn a_seeded_run_logs_the_seedings_and_each_step() {
    let sim_lines = |name: &str| {
        let mut lines = Vec::new();
        with_app(name, |app, _| {
            app.set_seed(1);
            for (pass, now) in [(0, 0), (1, 70), (2, 140)] {
                app.pass(now, &[]);
                lines.extend(
                    app.log_lines(pass, now)
                        .into_iter()
                        .filter(|l| l.contains(" sim ")),
                );
            }
            // The pause stops the managers: no `sim step` line.
            app.pass(210, &[Command::Pause]);
            assert!(app.log_lines(3, 210).iter().all(|l| !l.contains(" sim ")));
        });
        lines
    };
    let lines = sim_lines("app-sim-a");
    assert_eq!(
        lines,
        [
            "pass 0 t=0 sim seed 1",
            "pass 0 t=0 sim seed 2",
            "pass 0 t=0 sim step 200",
            "pass 1 t=70 sim step 70",
            "pass 2 t=140 sim step 70",
        ]
    );
    assert_eq!(lines, sim_lines("app-sim-b"));
}

#[test]
fn the_pause_stops_the_calendar_but_not_the_timers() {
    with_app("app-pause", |app, _| {
        let date = |app: &App| {
            let d = app.calendar().unwrap().date();
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
        let start = app.camera().unwrap().scroll;
        // The pointer on the left band, halfway down. The interface fires at 0 and 70 ms.
        app.pass(0, &[Command::Pointer(Some((0, 24)))]);
        assert_eq!(app.camera().unwrap().scroll, (start.0 - 32, start.1));
        // The frame of the next draw shows it.
        assert!(app.pass(20, &[]).is_some());
        assert_eq!(
            app.camera().unwrap().scroll,
            (start.0 - 32, start.1),
            "no interface pass"
        );
        app.pass(70, &[]);
        assert_eq!(app.camera().unwrap().scroll, (start.0 - 64, start.1));
        // Away from the band, or outside the window, nothing scrolls.
        app.pass(140, &[Command::Pointer(Some((32, 24)))]);
        app.pass(210, &[Command::Pointer(None)]);
        assert_eq!(app.camera().unwrap().scroll, (start.0 - 64, start.1));
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
        Command::Grid,
        Command::DevMode,
        Command::Fullscreen(true),
        Command::Fullscreen(false),
        Command::Pause,
        Command::Quit,
        Command::Close,
        Command::Mouse(Mouse::Move(-1, 77)),
        Command::Mouse(Mouse::Down),
        Command::Mouse(Mouse::Up),
        Command::Mouse(Mouse::Double),
        Command::Mouse(Mouse::Right),
        Command::Mouse(Mouse::Wheel(-2)),
        Command::Key(Key::PageDown, false, false),
        Command::Key(Key::Enter, true, false),
        Command::Key(Key::Y, true, true),
        Command::Char(0),
        Command::Char(255),
        Command::Skip,
        Command::Save("my zoo.zoo"),
        Command::Load("saved games/a.zoo"),
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
        "fullscreen",
        "fullscreen 1",
        "right 1",
        "stop",
        "mouse 1",
        "mouse x 1",
        "wheel",
        "wheel 1.5",
        "down 1",
        "double 1",
        "close now",
        "key",
        "key f1",
        "key enter alt",
        "char",
        "char 256",
        "char -1",
        "char ab",
        "skip 1",
        "save",
        "load   ",
    ] {
        assert!(bad.parse::<Command>().is_err(), "{bad:?}");
    }
    // A character may be written as itself.
    assert_eq!("char A".parse(), Ok(Command::Char(65)));
    assert_eq!("char 7".parse(), Ok(Command::Char(7)));
    assert_eq!(
        "key left shift ctrl".parse(),
        Ok(Command::Key(Key::Left, true, true))
    );
    assert_eq!(
        Command::Key(Key::Left, true, true).to_string(),
        "key left ctrl shift"
    );
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

#[test]
fn the_menu_music_fades_from_the_start_of_the_game_and_the_log_tells_it() {
    with_app("app-music", |app, _| {
        let ini = Config::parse(b"[UI]\nmenuMusicAttenuation=-1000\n").unwrap();
        // Once the game has started, the fade waits
        // five interface updates and the log tells each step, then the stop.
        app.start_game(Music::from_config(&ini));
        let music: Vec<_> = (0..100)
            .flat_map(|pass| {
                app.pass(pass * 66, &[]);
                app.log_lines(pass, pass * 66)
            })
            .filter(|line| line.contains(" music "))
            .collect();
        assert_eq!(music.len(), 91);
        assert_eq!(music[0], "pass 5 t=330 music fade 33 sum -967 gain 0.999");
        assert_eq!(
            music[89],
            "pass 94 t=6204 music fade 2970 sum 1970 gain 0.104"
        );
        assert_eq!(music[90], "pass 95 t=6270 music stopped");
        assert!(
            app.music()
                .is_some_and(|(m, changed)| m.stopped() && !changed)
        );
        app.start_game(None);
        assert!(app.music().is_none());
    });
}

struct NoTexts;

impl crate::ui::draw::Strings for NoTexts {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// An interface of a map view over the whole window and, hidden, a layout `a` (id 10) with a
/// 40 x 20 text widget on a red background, and a text field (id 4).
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        forecolor=255\nforecolor=218\nforecolor=90\n\
        [b]\ntype=UILayout\nlayout=ui/a.lyt\nstate=1\n\
        [d]\ntype=UILayout\nlayout=ui/q.lyt\nstate=1\n\
        [c]\ntype=UIEditableText\nid=4\nx=50\ny=40\ndx=4\ndy=4\n";
    let a = "[LayoutInfo]\nid=10\nlayer=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [x]\ntype=UIText\nid=11\nx=2\ny=2\ndx=40\ndy=20\ntransparent=0\n\
        backcolor=255\nbackcolor=0\nbackcolor=0\n";
    let q = "[LayoutInfo]\nid=45000\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n\
        [a]\ntype=UIButton\nid=45010\ndx=8\ndy=8\ntarget=6\naction=5\ncolorasicon=1\n";
    let install = crate::sprite::tests::Install::new(
        &format!("app-ui-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/a.lyt", a.as_bytes().to_vec()),
            ("ui/q.lyt", q.as_bytes().to_vec()),
        ],
    );
    let mut tree = crate::ui::layout::Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    Interface::new(tree, Box::new(NoTexts))
}

#[test]
fn with_an_interface_the_map_view_paints_the_map_and_a_paint_waits_for_a_reason() {
    with_app("app-ui", |app, other| {
        app.set_interface(interface("paint"));
        // Only the map view is shown: the frame is the map, as without an interface.
        let first = app.pass(0, &[]).unwrap().pixels.clone();
        assert_eq!(first, reference(other, app.camera().unwrap()).pixels);
        assert!(
            app.pass(20, &[]).is_none() && app.fired().draw,
            "nothing changed"
        );
        // The mouse over the map changes nothing, so nothing is painted.
        let moves = [
            Command::Mouse(Mouse::Move(5, 5)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
            Command::Mouse(Mouse::Wheel(1)),
        ];
        assert!(app.pass(40, &moves).is_none());
        // A change of the interface paints again; the map comes from the picture kept.
        app.interface().unwrap().touch();
        assert_eq!(app.pass(60, &[]).unwrap().pixels, first);
        // A shown layout is painted over the map.
        assert!(app.interface().unwrap().show(10));
        let over = app.pass(80, &[]).unwrap().pixels.clone();
        assert_ne!(over, first);
        let at = |x: usize, y: usize| over[(y * SIZE.0 as usize + x) * 4..][..3].to_vec();
        assert_eq!(at(2, 2), [255, 0, 0]);
        assert_eq!(at(41, 21), [255, 0, 0]);
        assert_eq!(
            at(42, 22)[..],
            first[(22 * SIZE.0 as usize + 42) * 4..][..3]
        );
        // The camera moving paints too, the layout staying over the map.
        let moved = app
            .pass(100, &[Command::Zoom(true)])
            .unwrap()
            .pixels
            .clone();
        assert_ne!(moved, over);
        assert_eq!(moved[(2 * SIZE.0 as usize + 2) * 4..][..3], [255, 0, 0]);
    });
}

#[test]
fn the_keys_of_the_map_view_wait_while_a_field_has_the_focus_or_the_map_is_not_shown() {
    with_app("app-ui-keys", |app, _| {
        app.set_interface(interface("keys"));
        app.pass(0, &[]);
        let start = app.camera().unwrap().scroll;
        app.pass(20, &[Command::Scroll(1, 0)]);
        assert_ne!(
            app.camera().unwrap().scroll,
            start,
            "the map view has the keys"
        );
        let moved = app.camera().unwrap().scroll;
        let field = app.interface().unwrap().tree.find(4).unwrap();
        app.interface().unwrap().tree.elements[field].state |= crate::ui::layout::FOCUSED;
        app.pass(
            40,
            &[Command::Scroll(1, 0), Command::Space, Command::Zoom(true)],
        );
        assert_eq!(app.camera().unwrap().scroll, moved);
        assert!(!app.paused(), "the space bar waits for the focus");
        // The Pause key is an application key: it pauses whatever has the focus.
        app.pass(50, &[Command::Pause]);
        assert!(app.paused());
        app.pass(55, &[Command::Pause]);
        assert!(!app.paused());
        app.interface().unwrap().tree.elements[field].state &= !crate::ui::layout::FOCUSED;
        app.pass(60, &[Command::Space]);
        assert!(app.paused());
    });
}

/// Runs `body` with the main menu of a window of 64 x 48 over a map view: Exit (7116) and Continue
/// (7113) are 10 x 10 buttons at the top left.
fn with_menu(name: &str, body: impl FnOnce(&mut App)) {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=UILayout\nlayout=ui/startup.lyt\n";
    let menu = "[LayoutInfo]\nid=26\nlayer=1\nx=0\ny=0\ndx=whole\ndy=whole\nstate=1\n\
        [x]\ntype=UIButton\nid=7116\nx=0\ny=0\ndx=10\ndy=10\ncolorasicon=1\n\
        [y]\ntype=UIButton\nid=7113\nx=20\ny=0\ndx=10\ndy=10\ncolorasicon=1\n";
    let files = Install::new(
        &format!("app-menu-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/startup.lyt", menu.as_bytes().to_vec()),
        ],
    );
    let mut tree = crate::ui::layout::Tree::load(&Vfs::open(&files.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    let ui = Interface::new(tree, Box::new(NoTexts));
    let menu = Menu::new(Config::default(), &files.0, None, None, 0);
    let install = cache_install(&format!("app-menu-game-{name}"), frame_file());
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = Catalogue::load(&vfs).unwrap();
    let mut app = App::menu(
        MapRenderer::new(&vfs, &catalogue),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
        ui,
        menu,
        None,
    )
    .unwrap();
    body(&mut app);
}

#[test]
fn the_menu_has_no_game_and_paints_the_interface_alone() {
    with_menu("paint", |app| {
        assert!(app.camera().is_none() && app.calendar().is_none() && !app.paused());
        let frame = app.pass(0, &[]).unwrap();
        assert!(frame.pixels.chunks(4).all(|p| p[3] == 255), "opaque");
        let lines = app.log_lines(0, 0);
        assert_eq!(lines[0], "pass 0 t=0 menu continue disabled");
        assert!(lines[1].starts_with("pass 0 t=0 draw ") && !lines[1].contains("view="));
        assert_eq!(lines[2], "pass 0 t=0 interface 66");
        assert!(lines[3].starts_with("pass 0 t=0 sim seed "), "{lines:?}");
        assert_eq!(lines.len(), 4, "no step without a game: {lines:?}");
        // The keys of the map view and a switch to full screen change nothing.
        let keys = [
            Command::Scroll(1, 0),
            Command::Rotate(true),
            Command::Zoom(true),
            Command::Pause,
            Command::Fullscreen(true),
        ];
        assert!(app.pass(20, &keys).is_none());
        assert!(!app.paused() && !app.quit());
        // The menu says what it did once.
        assert!(app.pass(40, &[]).is_none());
        assert!(app.log_lines(2, 40).iter().all(|l| !l.contains(" menu ")));
    });
}

#[test]
fn a_click_on_exit_ends_the_game_and_a_click_on_another_button_does_not() {
    with_menu("exit", |app| {
        app.pass(0, &[]);
        let click = |x, y| {
            [
                Command::Mouse(Mouse::Move(x, y)),
                Command::Mouse(Mouse::Down),
                Command::Mouse(Mouse::Up),
            ]
        };
        app.pass(20, &click(25, 5));
        assert!(!app.quit());
        app.pass(40, &click(40, 40));
        assert!(!app.quit());
        // The pass of the click is the last: it logs the click and what the menu made of it.
        assert!(app.pass(60, &click(5, 5)).is_none());
        assert!(app.quit());
        let lines = app.log_lines(3, 60);
        assert_eq!(lines.last().unwrap(), "pass 3 t=60 menu exit");
        // One click, one selection and one exit.
        let count = |text: &str| lines.iter().filter(|l| l.ends_with(text)).count();
        assert_eq!((count("ui select 7116"), count("menu exit")), (1, 1));
        assert!(app.pass(80, &[]).is_none() && app.log_lines(4, 80).is_empty());
    });
    with_menu("close", |app| {
        app.pass(0, &[]);
        app.pass(20, &[Command::Quit]);
        assert!(app.quit());
    });
}

#[test]
fn a_visible_animation_that_changes_its_frame_paints_and_a_still_interface_does_not() {
    with_app("app-ui-anim", |app, _| {
        let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
            [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
            [b]\ntype=UILayout\nlayout=ui/a.lyt\nstate=1\n";
        let a = "[LayoutInfo]\nid=10\nlayer=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
            [x]\ntype=UIImage\nid=11\nx=2\ny=2\nanimation=ui/b/b\nupdateWhilePaused=0\n";
        let mut members = crate::ui::draw::tests::two_frames("b");
        members.push(("ui/root.lyt".to_string(), root.as_bytes().to_vec()));
        members.push(("ui/a.lyt".to_string(), a.as_bytes().to_vec()));
        let members: Vec<(&str, Vec<u8>)> = members
            .iter()
            .map(|(n, b)| (n.as_str(), b.clone()))
            .collect();
        let install = crate::sprite::tests::Install::new("app-ui-anim-files", &members);
        let mut tree =
            crate::ui::layout::Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
        assert_eq!(tree.failure, None);
        tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
        app.set_interface(Interface::new(tree, Box::new(NoTexts)));
        assert!(app.interface().unwrap().show(10));
        // Passes 16 ms apart (the draw period): a frame of 125 ms is replaced by the first draw
        // 125 ms or more after the previous change, so nothing is painted in between.
        let (mut seen, mut changes) = (std::collections::BTreeSet::new(), Vec::new());
        for pass in 0..90u64 {
            if let Some(hash) = app.pass(pass * 16, &[]).map(frame_hash) {
                if pass > 0 {
                    changes.push(pass * 16);
                }
                seen.insert(hash);
            }
        }
        assert_eq!(seen.len(), 2, "the two frames");
        let gaps: Vec<u64> = changes.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(
            gaps.len() >= 3 && gaps.iter().all(|g| (125..=144).contains(g)),
            "{gaps:?}"
        );
        // Paused: this element stands still, so nothing is painted again.
        app.pass(1440, &[Command::Pause]);
        assert!((1441..2000).step_by(16).all(|t| app.pass(t, &[]).is_none()));
    });
}

#[test]
fn an_open_question_forces_the_pause_and_the_game_gets_its_flag_back_when_it_closes() {
    with_app("app-ui-modal", |app, _| {
        app.set_interface(interface("modal"));
        app.pass(0, &[]);
        let before = app.calendar().unwrap().date();
        let moved = app.camera().unwrap().scroll;
        assert!(
            app.interface()
                .unwrap()
                .ask(45000, b"?", crate::ui::interface::Buttons::YES_NO)
        );
        assert!(app.paused());
        // The simulation stands still, a Pause key changes nothing that outlasts the question,
        // and the keys of the map view do not act.
        app.pass(
            70,
            &[
                Command::Pause,
                Command::Space,
                Command::Scroll(1, 0),
                Command::Zoom(true),
            ],
        );
        assert_eq!(app.calendar().unwrap().date(), before);
        assert_eq!(app.camera().unwrap().scroll, moved);
        assert!(
            app.log_lines(1, 70)
                .iter()
                .any(|l| l.contains(" step ") && l.ends_with(" paused"))
        );
        // A click on the button answers; the game runs again.
        let b = app.interface().unwrap().tree.find(45010).unwrap();
        let (x, y, _, _) = app.interface().unwrap().tree.rect(b);
        app.pass(
            140,
            &[
                Command::Mouse(Mouse::Move(x + 1, y + 1)),
                Command::Mouse(Mouse::Down),
                Command::Mouse(Mouse::Up),
            ],
        );
        assert_eq!(
            app.ui_actions().last(),
            Some(&crate::ui::interface::Action::Answer(6))
        );
        assert!(!app.paused());
        app.pass(210, &[]);
        assert_ne!(app.calendar().unwrap().date(), before);
    });
}

#[test]
fn a_game_paused_before_a_question_stays_paused_after_it() {
    with_app("app-ui-modal-paused", |app, _| {
        app.set_interface(interface("modalpaused"));
        app.pass(0, &[]);
        app.pass(20, &[Command::Pause]);
        assert!(app.paused());
        app.interface()
            .unwrap()
            .ask(45000, b"", crate::ui::interface::Buttons::OK);
        app.pass(40, &[Command::Pause]);
        app.interface().unwrap().answer(1);
        assert!(app.paused(), "the question gave the flag back as it was");
    });
}

/// The window point of the middle of tile (`tx`, `ty`) of the flat test map.
fn tile_point(camera: &Camera, tx: i32, ty: i32) -> (i32, i32) {
    let p = project(camera.rotation, 64 * tx + 32, 64 * ty + 32, 0);
    let (px, py) = crate::camera::zoom_view(camera.zoom, p);
    (
        (px - i64::from(camera.scroll.0)) as i32,
        (py - i64::from(camera.scroll.1)) as i32,
    )
}

#[test]
fn a_press_and_a_release_on_the_map_view_are_logged_with_the_tile_at_every_rotation_and_zoom() {
    with_app("app-tool", |app, _| {
        app.set_interface(interface("tool"));
        app.pass(0, &[]);
        let (mut now, mut tried) = (20, 0);
        for zoom in [
            Command::Zoom(false),
            Command::Zoom(true),
            Command::Zoom(true),
        ] {
            app.pass(now, &[zoom]);
            now += 20;
            for _ in 0..4 {
                let camera = app.camera().unwrap().clone();
                // The tiles around the middle whose middle is well inside the window.
                for (tx, ty) in (16..25).flat_map(|x| (16..25).map(move |y| (x, y))) {
                    let (x, y) = tile_point(&camera, tx, ty);
                    if !(4..SIZE.0 as i32 - 4).contains(&x) || !(4..SIZE.1 as i32 - 4).contains(&y)
                    {
                        continue;
                    }
                    let commands = [
                        Command::Mouse(Mouse::Move(x, y)),
                        Command::Mouse(Mouse::Down),
                        Command::Mouse(Mouse::Up),
                    ];
                    app.pass(now, &commands);
                    let lines = app.log_lines(now / 20, now);
                    let lead = format!("pass {} t={now}", now / 20);
                    assert!(
                        lines.contains(&format!("{lead} tool down {tx},{ty}"))
                            && lines.contains(&format!("{lead} tool up {tx},{ty}")),
                        "rotation {} zoom {}: {lines:?}",
                        camera.rotation,
                        camera.zoom
                    );
                    (now, tried) = (now + 20, tried + 1);
                }
                app.pass(now, &[Command::Rotate(true)]);
                now += 20;
            }
        }
        assert!(tried >= 12, "{tried}");
    });
}

#[test]
fn a_press_off_the_map_or_on_the_interface_logs_the_tile_or_nothing() {
    with_app("app-tool-off", |app, _| {
        app.set_interface(interface("tool-off"));
        app.pass(0, &[]);
        // Far from the map: a press on the map view, on no tile.
        app.pass(20, &[Command::Zoom(false), Command::Zoom(false)]);
        app.pass(
            40,
            &[
                Command::Mouse(Mouse::Move(0, 0)),
                Command::Mouse(Mouse::Down),
            ],
        );
        let lines = app.log_lines(2, 40);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("pass 2 t=40 tool down ")),
            "{lines:?}"
        );
        // The wheel and a move are not tool messages.
        app.pass(
            60,
            &[
                Command::Mouse(Mouse::Wheel(1)),
                Command::Mouse(Mouse::Move(3, 3)),
            ],
        );
        assert!(app.log_lines(3, 60).iter().all(|l| !l.contains(" tool ")));
        // The shown layout covers the pointer: the press is its, not the map's.
        assert!(app.interface().unwrap().show(10));
        app.pass(
            80,
            &[
                Command::Mouse(Mouse::Up),
                Command::Mouse(Mouse::Move(5, 5)),
                Command::Mouse(Mouse::Down),
            ],
        );
        assert!(
            app.log_lines(4, 80)
                .iter()
                .all(|l| !l.contains(" tool down"))
        );
    });
}

#[test]
fn an_edit_stages_the_map_again_once_at_the_next_draw() {
    with_app("app-restage", |app, other| {
        let first = app.pass(0, &[]).unwrap().pixels.clone();
        assert!(
            app.pass(20, &[]).is_none(),
            "nothing changed, nothing is staged"
        );
        assert!(app.log_lines(1, 20).iter().all(|l| !l.contains(" world ")));
        let world = &mut app.game.as_mut().unwrap().world;
        for (x, y) in [(19, 19), (19, 20), (20, 19), (20, 20)] {
            assert!(world.set_tile(x, y, 1, 0, 0));
        }
        let frame = app.pass(40, &[]).unwrap().pixels.clone();
        assert_ne!(
            frame_hash_of(&frame),
            frame_hash_of(&first),
            "the raised tile shows"
        );
        let lines = app.log_lines(2, 40);
        assert_eq!(lines[0], "pass 2 t=40 world stage");
        assert!(lines[1].starts_with("pass 2 t=40 draw "), "{lines:?}");
        // The same picture as a map that had the edit from the start.
        let mut zoo = map();
        for i in [19 * 40 + 19, 20 * 40 + 19, 19 * 40 + 20, 20 * 40 + 20] {
            zoo.tiles[i].elevation = 1;
        }
        let want = other
            .render_with(
                &zoo,
                0,
                &Options {
                    window: Some((
                        app.camera().unwrap().scroll.0,
                        app.camera().unwrap().scroll.1,
                        SIZE.0,
                        SIZE.1,
                    )),
                    ..Options::default()
                },
            )
            .unwrap()
            .0;
        assert_eq!(frame_hash_of(&frame), frame_hash_of(&want.pixels));
        assert!(app.pass(60, &[]).is_none());
        assert!(
            app.log_lines(3, 60).iter().all(|l| !l.contains(" world ")),
            "only once"
        );
    });
}

fn frame_hash_of(pixels: &[u8]) -> u64 {
    frame_hash(&Framebuffer {
        width: SIZE.0,
        height: SIZE.1,
        pixels: pixels.to_vec(),
    })
}

/// An interface of the map view over the window, a toggle button (1002, at 50, 40) that shows
/// the construction panel (layout 8, hidden), and nothing else.
fn panel_interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/b.lyt\n\
        [c]\ntype=UIButton\nid=1002\nx=50\ny=40\ndx=8\ndy=8\nstate=2048\n\
        action=3\ntarget=8\ncolorasicon=1\n\
        [d]\ntype=UILayout\nlayout=ui/q.lyt\nstate=1\n";
    let panel = "[LayoutInfo]\nid=8\nx=0\ny=0\ndx=20\ndy=20\nstate=1\n";
    let q = "[LayoutInfo]\nid=45000\nlayer=7\nx=0\ny=0\ndx=20\ndy=10\n";
    let install = crate::sprite::tests::Install::new(
        &format!("app-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/b.lyt", panel.as_bytes().to_vec()),
            ("ui/q.lyt", q.as_bytes().to_vec()),
        ],
    );
    let mut tree = crate::ui::layout::Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(SIZE.0 as i32, SIZE.1 as i32, |_| 12);
    Interface::new(tree, Box::new(NoTexts))
}

#[test]
fn escape_and_the_right_button_close_a_building_panel_before_the_menu_question() {
    with_app("app-panels", |app, _| {
        app.set_interface(panel_interface("panels"));
        app.interface().unwrap().set_hidden(1002, false);
        let click = [
            Command::Mouse(Mouse::Move(52, 42)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ];
        let tool = |app: &App, pass, now| -> Vec<String> {
            let lines = app.log_lines(pass, now);
            lines.into_iter().filter(|l| l.contains(" tool ")).collect()
        };
        let mut now = 0;
        let mut step = |app: &mut App, commands: &[Command]| {
            now += 20;
            app.pass(now, commands);
            tool(app, 1, now)
        };
        // The left bar button shows the panel: the app notices.
        assert_eq!(step(app, &click), ["pass 1 t=20 tool panel construction"]);
        assert!(!app.interface().unwrap().modal_open());
        // Escape closes it and does not ask; the button is released.
        let escape = [Command::Key(Key::Escape, false, false)];
        assert_eq!(step(app, &escape), ["pass 1 t=40 tool panel none"]);
        assert!(!app.interface().unwrap().modal_open());
        // The right button released over the map does the same.
        step(app, &click);
        let right = [
            Command::Mouse(Mouse::Move(5, 5)),
            Command::Mouse(Mouse::Right),
        ];
        assert_eq!(step(app, &right), ["pass 1 t=80 tool panel none"]);
        // With nothing open the right button does nothing, Escape asks.
        assert!(step(app, &right).is_empty());
        assert!(!app.interface().unwrap().modal_open());
        step(app, &escape);
        assert!(app.interface().unwrap().modal_open());
    });
}

/// A catalogue of one type, a tree (scenery of one quadrant) that costs 25 and has no picture.
fn tree_catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(crate::entity::EntityType {
        class: "objects",
        type_name: b"tree".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            Config::parse(b"[Characteristics/Integers]\ncPurchaseCost=25\ncAutoRotate=1\n")
                .unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

#[test]
fn the_pending_object_follows_the_pointer_with_its_cost_and_is_staged_when_it_changes() {
    let install = cache_install_with("app-pending", frame_file(), &[]);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = tree_catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_interface(interface("pending"));
    app.pass(0, &[]);
    let tool = |app: &App, pass, now| -> Vec<String> {
        let lines = app.log_lines(pass, now);
        lines.into_iter().filter(|l| l.contains(" tool ")).collect()
    };
    // Without a tool nothing follows the pointer.
    let camera = app.camera().unwrap().clone();
    let (x, y) = tile_point(&camera, 20, 19);
    let at = |dx: i32| Command::Mouse(Mouse::Move(x + dx, y));
    let hash = |app: &App| frame_hash(&app.frame);
    app.pass(20, &[at(0)]);
    let plain = hash(&app);
    assert!(tool(&app, 1, 20).is_empty());
    // With one, every move over the map tells the tile, the quadrant and the verdict, and the
    // picture is drawn again with the cost written 23 pixels right of the pointer.
    app.game.as_mut().unwrap().build.hold(0);
    app.pass(40, &[at(1)]);
    let first = hash(&app);
    assert_eq!(
        tool(&app, 2, 40),
        ["pass 2 t=40 tool pending objects/tree/ 20,19 q 2 valid"]
    );
    assert_ne!(first, plain, "the cost text is in the frame");
    let width = app.frame.width as usize;
    let yellow = |px: &[u8]| px[0] > 200 && px[1] > 150 && px[2] < 140;
    let columns: Vec<usize> = (0..width)
        .filter(|&c| {
            (0..app.frame.height as usize).any(|r| yellow(&app.frame.pixels[(r * width + c) * 4..]))
        })
        .collect();
    assert!(
        columns.first().is_some_and(|&c| c as i32 >= x + 1 + 23),
        "{columns:?} for a pointer at {x}"
    );
    // The same quarter tile again: the log tells it again, the picture follows the pointer.
    app.pass(60, &[at(2)]);
    assert_eq!(tool(&app, 3, 60).len(), 1);
    assert_ne!(hash(&app), first);
    // Off the map the pending object goes with its text.
    app.pass(80, &[Command::Mouse(Mouse::Move(-5, -5))]);
    assert!(tool(&app, 4, 80).is_empty());
    assert_eq!(hash(&app), plain);
    // It keeps its facing on screen when the view turns: on the map it turns the other way.
    app.pass(100, &[at(1)]);
    let facing = |app: &App| {
        app.game
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .map(|p| p.facing)
    };
    assert_eq!(facing(&app), Some(4));
    app.pass(120, &[Command::Rotate(true)]);
    let view = app.camera().unwrap().rotation;
    assert_eq!((view, facing(&app)), (1, Some(2)));
}

#[test]
fn a_release_places_the_object_pays_for_it_and_makes_a_new_pending_object() {
    let install = cache_install_with("app-place", frame_file(), &[]);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = tree_catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_interface(interface("place"));
    app.set_money(Money {
        cash: 60.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    app.game.as_mut().unwrap().build.hold(0);
    let camera = app.camera().unwrap().clone();
    let click = |tx: i32, ty: i32| {
        let (x, y) = tile_point(&camera, tx, ty);
        [
            Command::Mouse(Mouse::Move(x, y)),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ]
    };
    // The tool, world and money lines of a pass without their lead, the picture's and the
    // pointer's aside.
    let lines = |app: &App, pass, now| -> Vec<String> {
        let skip = ["tool pending", "world stage", "money bar"];
        let all = app.log_lines(pass, now);
        let kept = all.iter().filter_map(|l| l.splitn(4, ' ').nth(3));
        let wanted = |l: &&str| {
            ["tool ", "world ", "money "]
                .iter()
                .any(|k| l.starts_with(k))
        };
        let kept = kept
            .filter(wanted)
            .filter(|l| !skip.iter().any(|k| l.starts_with(k)));
        kept.map(str::to_string).collect()
    };
    let facing = |app: &App| {
        app.game
            .as_ref()
            .unwrap()
            .pending
            .as_ref()
            .map(|p| p.facing)
    };
    assert_eq!(facing(&app), None);
    // The click places a tree, charges 25, asks for the placement sound and turns the next one.
    app.pass(20, &click(20, 19));
    assert_eq!(facing(&app), Some(self::place::facing(1, 0)));
    assert_eq!(
        lines(&app, 1, 20),
        [
            "tool down 20,19",
            "tool up 20,19",
            "tool turn 1",
            "world add objects/tree/ 20,19 id 1 quadrants 1",
            "money cash 35"
        ]
    );
    assert_eq!(app.sounds(), ["place"]);
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 1);
    // The same quarter tile is in the way now: nothing is placed or charged, the message shows.
    app.pass(40, &click(20, 19));
    assert_eq!(
        lines(&app, 2, 40),
        ["tool down 20,19", "tool up 20,19", "tool message 10555"]
    );
    assert_eq!(app.sounds(), ["noplace"]);
    // Another tile is placed; with 10 left the third is too expensive.
    app.pass(60, &click(19, 19));
    assert_eq!(app.money().unwrap().cash, 10.0);
    app.pass(80, &click(19, 20));
    assert_eq!(
        lines(&app, 4, 80),
        ["tool down 19,20", "tool up 19,20", "tool message 10002"]
    );
    assert_eq!(app.sounds(), ["noplace"]);
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 2);
    // A release that did not start on the map places nothing and tells nothing.
    let (x, y) = tile_point(&camera, 19, 20);
    app.pass(
        100,
        &[Command::Mouse(Mouse::Move(x, y)), Command::Mouse(Mouse::Up)],
    );
    assert!(
        lines(&app, 6, 100).is_empty(),
        "the view never sees that release"
    );
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 2);
}

/// A catalogue of one type, a path that costs 10 and has no picture.
fn path_catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(crate::entity::EntityType {
        class: "paths",
        type_name: b"dirt".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            Config::parse(b"[Characteristics/Integers]\ncPurchaseCost=10\n").unwrap(),
        )),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    });
    cat
}

#[test]
fn a_drag_lays_an_l_of_paths_bought_in_order_with_the_cash_that_covers_them() {
    let install = cache_install_with("app-drag", frame_file(), &[]);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = path_catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_interface(interface("drag"));
    app.set_money(Money {
        cash: 100.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    app.game.as_mut().unwrap().build.hold(0);
    let camera = app.camera().unwrap().clone();
    let at = |tx: i32, ty: i32| {
        let (x, y) = tile_point(&camera, tx, ty);
        Command::Mouse(Mouse::Move(x, y))
    };
    let lines = |app: &App, pass, now| -> Vec<String> {
        let skip = ["tool pending", "world stage", "money bar"];
        let all = app.log_lines(pass, now);
        let kept = all.iter().filter_map(|l| l.splitn(4, ' ').nth(3));
        let wanted = |l: &&str| {
            ["tool ", "world ", "money "]
                .iter()
                .any(|k| l.starts_with(k))
        };
        let kept = kept
            .filter(wanted)
            .filter(|l| !skip.iter().any(|k| l.starts_with(k)));
        kept.map(str::to_string).collect()
    };
    let cost = |app: &App| app.game.as_ref().unwrap().cost();
    // Press on (20, 19), move along x first, then to (23, 21): the L has six pieces and the cost
    // text shows their sum. Nothing is bought before the release.
    app.pass(20, &[at(20, 19), Command::Mouse(Mouse::Down), at(21, 19)]);
    assert_eq!(cost(&app), Some(-20));
    app.pass(40, &[at(23, 21)]);
    assert_eq!(cost(&app), Some(-60));
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 0);
    app.pass(60, &[Command::Mouse(Mouse::Up)]);
    assert_eq!(
        lines(&app, 3, 60),
        [
            "tool up 23,21",
            "world add paths/dirt/ 20,19 id 1",
            "world add paths/dirt/ 21,19 id 2",
            "world add paths/dirt/ 22,19 id 3",
            "world add paths/dirt/ 23,19 id 4",
            "world add paths/dirt/ 23,20 id 5",
            "world add paths/dirt/ 23,21 id 6",
            "money cash 40"
        ]
    );
    assert_eq!(app.sounds(), ["place"]);
    // The drag is over: a pending piece follows the pointer again, on the last tile (orange).
    assert_eq!(cost(&app), Some(0));
    // The first move along y, then over to (21, 22), gives five pieces (the pointer is captured
    // by the press, so it may leave the tiny window of this test); with 40 in hand
    // only the first four are bought and the pieces the cash cut show no message.
    app.pass(80, &[at(19, 20), Command::Mouse(Mouse::Down), at(19, 21)]);
    app.pass(100, &[at(21, 22)]);
    assert_eq!(cost(&app), Some(-40), "the cash covers four of the five");
    app.pass(120, &[Command::Mouse(Mouse::Up)]);
    assert_eq!(
        lines(&app, 6, 120),
        [
            "tool up 21,22",
            "world add paths/dirt/ 19,20 id 7",
            "world add paths/dirt/ 19,21 id 8",
            "world add paths/dirt/ 19,22 id 9",
            "world add paths/dirt/ 20,22 id 10",
            "money cash 0"
        ]
    );
    let game = app.game.as_ref().unwrap();
    assert_eq!(game.world.zoo.entities.len(), 10);
    // A single press and release is one piece; with no cash at all nothing is bought.
    app.pass(
        140,
        &[
            at(19, 19),
            Command::Mouse(Mouse::Down),
            Command::Mouse(Mouse::Up),
        ],
    );
    assert_eq!(app.sounds(), ["noplace"]);
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 10);
    // One undo takes back the pieces of the last drag, and only those.
    assert!(!app.game.as_mut().unwrap().world.undo().is_empty());
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 6);
}

/// The window point of the world point (`wx`, `wy`) on the flat test map.
fn world_point(camera: &Camera, wx: i32, wy: i32) -> (i32, i32) {
    let p = project(camera.rotation, wx, wy, 0);
    let (px, py) = crate::camera::zoom_view(camera.zoom, p);
    (
        (px - i64::from(camera.scroll.0)) as i32,
        (py - i64::from(camera.scroll.1)) as i32,
    )
}

/// A catalogue of a fence that costs 125 and a hall of 2 x 2 tiles.
fn fence_catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let ty = |class, name: &[u8], ini: &[u8], footprint| crate::entity::EntityType {
        class,
        type_name: name.to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini).unwrap())),
        name_id: 0,
        footprint,
        height: 0,
    };
    cat.push(ty(
        "fences",
        b"iron",
        b"[Characteristics/Integers]\ncPurchaseCost=125\n",
        [1, 1, 1],
    ));
    cat.push(ty("objects", b"hall", b"", [4, 4, 1]));
    cat
}

#[test]
fn a_fence_drag_buys_pieces_on_the_side_of_the_press_and_each_tile_has_its_own_slot() {
    let install = cache_install_with("app-fence", frame_file(), &[]);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = fence_catalogue();
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        SIZE,
    )
    .unwrap();
    app.set_interface(interface("fence"));
    app.set_money(Money {
        cash: 50_000.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    app.game.as_mut().unwrap().build.hold(0);
    let camera = app.camera().unwrap().clone();
    // A window point inside tile (`tx`, `ty`), at the local point (`lx`, `ly`).
    let at = |tx: i32, ty: i32, lx: i32, ly: i32| {
        let (x, y) = world_point(&camera, 64 * tx + lx, 64 * ty + ly);
        Command::Mouse(Mouse::Move(x, y))
    };
    let lines = |app: &App, pass, now| -> Vec<String> {
        let all = app.log_lines(pass, now);
        let kept = all.iter().filter_map(|l| l.splitn(4, ' ').nth(3));
        let wanted = |l: &&str| {
            l.starts_with("world ") && !l.starts_with("world stage") || l.starts_with("money cash")
        };
        kept.filter(wanted).map(str::to_string).collect()
    };
    let cost = |app: &App| app.game.as_ref().unwrap().cost();
    // Press near the upper side of tile (20, 20), drag along x to tile (25, 20): six pieces on
    // side 0, "($750)", nothing bought before the release.
    app.pass(
        20,
        &[
            at(20, 20, 32, 12),
            Command::Mouse(Mouse::Down),
            at(21, 20, 32, 12),
        ],
    );
    app.pass(40, &[at(25, 20, 32, 12)]);
    assert_eq!(cost(&app), Some(-750));
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 0);
    // The selection is not drawn while the drag is held.
    assert_eq!(app.game.as_ref().unwrap().selection, None);
    app.pass(60, &[Command::Mouse(Mouse::Up)]);
    // The release shows it again, on the tile under the pointer.
    let game = app.game.as_ref().unwrap();
    assert_eq!(
        game.selection.map(|m| m.tile),
        game.world.tile_index(25, 20)
    );
    let adds: Vec<String> = (20..=25)
        .map(|x| format!("world add fences/iron/ {x},20 side 0 id {}", x - 19))
        .chain(["money cash 49250".to_string()])
        .collect();
    assert_eq!(lines(&app, 3, 60), adds);
    assert_eq!(app.sounds(), ["place"]);
    // The same line is side 4 of the tiles above: their own slots, charged in full.
    app.pass(
        80,
        &[
            at(20, 19, 20, 52),
            Command::Mouse(Mouse::Down),
            at(23, 19, 32, 52),
        ],
    );
    app.pass(90, &[at(25, 19, 32, 52)]);
    app.pass(100, &[Command::Mouse(Mouse::Up)]);
    let adds: Vec<String> = (20..=25)
        .map(|x| format!("world add fences/iron/ {x},19 side 4 id {}", x - 13))
        .chain(["money cash 48500".to_string()])
        .collect();
    assert_eq!(lines(&app, 6, 100), adds);
    // Over its own slots again the same pieces replace themselves for nothing: the old pieces
    // stay, with their ids, and take the type again.
    app.pass(
        120,
        &[
            at(20, 20, 32, 12),
            Command::Mouse(Mouse::Down),
            at(25, 20, 32, 12),
        ],
    );
    assert_eq!(cost(&app), Some(0));
    app.pass(140, &[Command::Mouse(Mouse::Up)]);
    let told = lines(&app, 8, 140);
    assert_eq!(told.len(), 6, "a retype for each piece, no cash line");
    assert_eq!(told[0], "world retype fences/iron/ 20,20 side 0 id 1");
    assert_eq!(app.money().unwrap().cash, 48_500.0);
    assert_eq!(app.game.as_ref().unwrap().world.zoo.entities.len(), 12);
    // A hall of 2 x 2 tiles on the corner of tiles (19..20, 22..23): a drag along the east
    // sides of column 19 adds nothing where the hall spans the tiles on both sides of the line.
    let hall = {
        let renderer = &app.renderer;
        let mut e = crate::place::entity(
            &renderer.catalogue().types[1],
            &Pending {
                ty: 1,
                x: 20 * 64,
                y: 23 * 64,
                z: 0,
                facing: 0,
                tile: (20, 23),
                quadrant: 0,
                verdict: crate::place::Verdict::Valid,
                message: None,
                price: 0.0,
                paint: None,
                blocking: Vec::new(),
                wall: 0,
            },
            Vec::new(),
        );
        e.common.as_mut().unwrap().id = 0;
        let shape = renderer.shape(&e);
        (e, shape)
    };
    let id = app
        .game
        .as_mut()
        .unwrap()
        .world
        .add(hall.0, hall.1)
        .unwrap();
    assert_eq!(id, 13);
    app.game.as_mut().unwrap().world.begin();
    app.pass(
        160,
        &[
            at(19, 20, 52, 32),
            Command::Mouse(Mouse::Down),
            at(19, 21, 52, 32),
        ],
    );
    app.pass(170, &[at(19, 25, 52, 32)]);
    app.pass(180, &[Command::Mouse(Mouse::Up)]);
    let adds: Vec<String> = [20, 21, 24, 25]
        .into_iter()
        .zip(14..)
        .map(|(y, id)| format!("world add fences/iron/ 19,{y} side 2 id {id}"))
        .chain(["money cash 48000".to_string()])
        .collect();
    assert_eq!(lines(&app, 11, 180), adds);
}

/// A window with room for the four sides of a tile.
const BIG: (u32, u32) = (128, 96);

/// An exhibit fence with a gate of its own, as the catalogue of a install holds them.
/// The fences have the picture of `ani` when it is not empty.
fn exhibit_catalogue(ani: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    for subtype in [b"f", b"g"] {
        cat.push(crate::entity::EntityType {
            class: "fences",
            type_name: b"iron".to_vec(),
            subtype: subtype.to_vec(),
            file: None,
            config: Some(std::sync::Arc::new(
                Config::parse(
                    format!(
                        "[HabitatFence]\ncHeight=1\n[Characteristics/Integers]\ncPurchaseCost=125\n{}{}",
                        if subtype == b"g" { "cLife=7\n" } else { "" },
                        if ani.is_empty() { String::new() } else { format!("[Animations]\nidle={ani}\n") }
                    )
                    .as_bytes(),
                )
                .unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// An interface of a map view and the hidden name dialog (45300) with its message, its field
/// and its OK button.
fn name_interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [g]\ntype=UIButton\nid=3240\nx=118\ny=86\ndx=5\ndy=5\nstate=2048\n\
        [d]\ntype=UILayout\nlayout=ui/n.lyt\nstate=1\n";
    let n = "[LayoutInfo]\nid=45300\nlayer=7\nx=300\ny=300\ndx=80\ndy=40\n\
        [m]\ntype=UIText\nid=45306\nx=0\ny=0\ndx=80\ndy=10\n\
        [f]\ntype=UIEditableText\nid=45308\nx=0\ny=12\ndx=80\ndy=10\n\
        [o]\ntype=UIButton\nid=45312\nx=0\ny=24\ndx=8\ndy=8\ntarget=1\naction=5\ncolorasicon=1\n";
    let install = crate::sprite::tests::Install::new(
        &format!("app-name-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/n.lyt", n.as_bytes().to_vec()),
        ],
    );
    let mut tree = crate::ui::layout::Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(BIG.0 as i32, BIG.1 as i32, |_| 12);
    Interface::new(tree, Box::new(NoTexts))
}

#[test]
fn a_closed_tile_is_an_exhibit_the_player_names_in_a_dialog() {
    let install = cache_install_with("app-exhibit", frame_file(), &[]);
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = exhibit_catalogue("");
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        BIG,
    )
    .unwrap();
    let mut ui = name_interface("close");
    ui.set_string(6208, "Pen".into());
    app.set_interface(ui);
    app.set_money(Money {
        cash: 50_000.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    app.game.as_mut().unwrap().build.hold(0);
    let camera = app.camera().unwrap().clone();
    let at = |lx: i32, ly: i32| {
        let (x, y) = world_point(&camera, 64 * 20 + lx, 64 * 20 + ly);
        Command::Mouse(Mouse::Move(x, y))
    };
    let exhibit_lines = |app: &App| -> Vec<String> {
        let all = app.log_lines(0, 0);
        all.iter()
            .filter_map(|l| l.splitn(4, ' ').nth(3))
            .filter(|l| l.starts_with("exhibit "))
            .map(str::to_string)
            .collect()
    };
    // One piece on each side of tile (20, 20): the fourth closes it.
    let clicks = [(32, 12), (52, 32), (32, 52), (12, 32)];
    for (n, (lx, ly)) in clicks.into_iter().enumerate() {
        let pass = 20 * (n as u64 + 1);
        app.pass(pass, &[at(lx, ly)]);
        app.pass(pass + 5, &[Command::Mouse(Mouse::Down)]);
        app.pass(pass + 10, &[Command::Mouse(Mouse::Up)]);
        let told = exhibit_lines(&app);
        match n {
            3 => {
                assert_eq!(told, ["exhibit new 1 \"Pen 1\" tiles 1 gate none"]);
                let log = app.log_lines(0, 0);
                assert!(log.iter().any(|l| l.ends_with(" sound exhibit 1")));
            }
            _ => assert!(told.is_empty()),
        }
    }
    let game = app.game.as_ref().unwrap();
    assert_eq!(game.world.exhibits().list().len(), 1);
    let first = game.world.exhibits().get(1).unwrap();
    assert_eq!((first.name.as_str(), first.tiles), ("Pen 1", 1));
    assert!(app.paused(), "the name dialog is open");
    assert_eq!(
        game.world.gate_slot(1),
        None,
        "the gate comes after the name"
    );
    assert_eq!(
        app.sounds().last(),
        Some(&"exhibit"),
        "once, as it is named"
    );
    assert_eq!(app.sounds().iter().filter(|&&s| s == "exhibit").count(), 1);
    let ui = app.interface().unwrap();
    assert_eq!(ui.text(45308), Some(b"Pen 1".to_vec()));
    // Typing replaces the selected default name; Enter answers.
    let typed = [
        Command::Char(b'L'),
        Command::Char(b'i'),
        Command::Key(Key::Enter, false, false),
    ];
    app.pass(200, &typed);
    assert!(!app.paused());
    assert_eq!(
        exhibit_lines(&app),
        ["exhibit name 1 \"Li\"", "exhibit gate 1 20,20 side 6"]
    );
    assert_eq!(
        app.game
            .as_ref()
            .unwrap()
            .world
            .exhibits()
            .get(1)
            .unwrap()
            .name,
        "Li"
    );
}

/// What the gate tool run saw of the wear: the fence values of the gate the game chose, and of the
/// old gate before and after the player moved the gate.
struct Seen {
    chosen_gate: crate::zoo::Fence,
    old_gate: (crate::zoo::Fence, crate::zoo::Fence),
}

#[test]
fn the_gate_tool_moves_the_gate_of_a_closed_square_and_charges_a_decayed_piece() {
    gate_tool_run();
}

#[test]
fn the_old_gate_keeps_its_wear() {
    // It keeps the values it got as the gate (life 7 of the gate type, not 10 of the fence type),
    // the interval and the countdown included.
    let (before, after) = gate_tool_run().old_gate;
    assert_eq!(before.life, 7);
    assert_eq!(before, after);
}

#[test]
fn a_gate_chosen_by_the_game_is_made_healthy() {
    // The last piece of the square was placed as a fence (life 10) and became the gate (life 7).
    let fence = gate_tool_run().chosen_gate;
    assert_eq!(fence.life, 7);
    assert!(fence.timers[0] > 0 && fence.timers[0] == fence.timers[1]);
}

fn gate_tool_run() -> Seen {
    // Every fence piece is a square of 12 pixels around its anchor, which the gate tool picks.
    let mut square = Vec::new();
    for field in [12u16, 12, 6, 6, 0] {
        square.extend(field.to_le_bytes());
    }
    for _ in 0..12 {
        square.extend([1, 0, 12]);
        square.extend([1; 12]);
    }
    let install = cache_install_with(
        "app-gate",
        frame_file(),
        &[
            (
                "fence/f.ani",
                b"[animation]\ndir0=fence\ndir1=spr\nanimation=N\n".to_vec(),
            ),
            ("fence/spr/N", sprite_file(None, 1, &[sized(&square)])),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = exhibit_catalogue("fence/f.ani");
    let mut app = App::new(
        MapRenderer::new(&vfs, &catalogue),
        map(),
        Settings::default(),
        Timers::new(66, 16, 200),
        BIG,
    )
    .unwrap();
    app.set_interface(name_interface("gate"));
    app.set_money(Money {
        cash: 50_000.0,
        ..Money::default()
    });
    app.pass(0, &[]);
    app.game.as_mut().unwrap().build.hold(0);
    let camera = app.camera().unwrap().clone();
    let at = |tx: i32, ty: i32, lx: i32, ly: i32| {
        let (x, y) = world_point(&camera, 64 * tx + lx, 64 * ty + ly);
        Command::Mouse(Mouse::Move(x, y))
    };
    let told = |app: &App, prefixes: &[&str]| -> Vec<String> {
        let all = app.log_lines(0, 0);
        all.iter()
            .filter_map(|l| l.splitn(4, ' ').nth(3))
            .filter(|l| prefixes.iter().any(|p| l.starts_with(p)) && *l != "world stage")
            .map(str::to_string)
            .collect()
    };
    let click = |app: &mut App, n: u64, to: Command| {
        app.pass(n, &[to]);
        app.pass(n + 5, &[Command::Mouse(Mouse::Down)]);
        app.pass(n + 10, &[Command::Mouse(Mouse::Up)]);
    };
    let fence_of = |app: &App, id: u32| {
        let world = &app.game.as_ref().unwrap().world;
        world.zoo.entities[world.find(id).unwrap()]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap()
    };
    // A square on tile (20, 20), named at once (the gate goes on its last piece, side 6), and a
    // fence of no exhibit on tile (21, 21).
    for (n, (lx, ly)) in [(32, 12), (52, 32), (32, 52), (12, 32)]
        .into_iter()
        .enumerate()
    {
        click(&mut app, 20 * (n as u64 + 1), at(20, 20, lx, ly));
    }
    app.pass(120, &[Command::Key(Key::Enter, false, false)]);
    click(&mut app, 140, at(21, 21, 32, 12));
    let id_of = |app: &App, d: usize| {
        let world = &app.game.as_ref().unwrap().world;
        let e = world.edge(world.tile_index(20, 20).unwrap(), d).unwrap();
        world.zoo.entities[e].common.as_ref().unwrap().id
    };
    let (north, east, west) = (id_of(&app, 0), id_of(&app, 2), id_of(&app, 6));
    let lone = {
        let world = &app.game.as_ref().unwrap().world;
        world
            .zoo
            .entities
            .last()
            .unwrap()
            .common
            .as_ref()
            .unwrap()
            .id
    };
    // A window point `d` pixels from the anchor of a piece.
    let on = |app: &App, id: u32, d: (i32, i32)| {
        let world = &app.game.as_ref().unwrap().world;
        let c = world.zoo.entities[world.find(id).unwrap()]
            .common
            .as_ref()
            .unwrap();
        let (x, y) = world_point(&camera, c.x, c.y);
        Command::Mouse(Mouse::Move(x + d.0, y + d.1))
    };
    let chosen_gate = fence_of(&app, west);
    let world = &app.game.as_ref().unwrap().world;
    assert_eq!(world.gate_slot(1), world.tile_index(20, 20).zip(Some(6)));
    // The gate button: the tool is the gate and not the fence.
    app.interface().unwrap().set_button(3240, true, true);
    app.pass(190, &[]);
    assert!(app.game.as_ref().unwrap().build.gating());
    // Over the north piece: a valid gate, free, tinted valid, "$0".
    app.pass(200, &[on(&app, north, (0, 0))]);
    assert_eq!(
        told(&app, &["tool pending"]),
        ["tool pending fences/iron/g 20,20 q 1 valid"]
    );
    assert_eq!(app.game.as_ref().unwrap().cost(), Some(0));
    app.pass(205, &[Command::Mouse(Mouse::Down)]);
    app.pass(210, &[Command::Mouse(Mouse::Up)]);
    assert_eq!(
        told(&app, &["world", "exhibit"]),
        [
            format!("world ungate fences/iron/f 20,20 side 6 id {west}"),
            format!("world gate fences/iron/g 20,20 side 0 id {north}"),
            "exhibit gate 1 20,20 side 0".to_string()
        ]
    );
    assert_eq!(app.money().unwrap().cash, 50_000.0 - 5.0 * 125.0);
    // The undo list was cleared. The piece under the pointer has not changed: no pending gate
    // and no cost text, however the pointer moves over it (placing 6.4).
    assert!(app.game.as_mut().unwrap().world.undo().is_empty());
    for (n, d) in [(1, 0), (2, 1)].into_iter().enumerate() {
        app.pass(212 + n as u64, &[on(&app, north, d)]);
        assert!(told(&app, &["tool pending"]).is_empty());
        assert_eq!(app.game.as_ref().unwrap().cost(), None);
    }
    // Over another piece and back: the gate is now the current gate, not valid again.
    app.pass(216, &[on(&app, west, (0, 0))]);
    assert_eq!(
        told(&app, &["tool pending"]),
        ["tool pending fences/iron/g 20,20 q 2 valid"]
    );
    app.pass(220, &[on(&app, north, (0, 0))]);
    assert_eq!(
        told(&app, &["tool pending"]),
        ["tool pending fences/iron/g 20,20 q 1 invalid"]
    );
    let to = on(&app, north, (0, 0));
    click(&mut app, 230, to);
    assert!(told(&app, &["world", "exhibit", "tool message"]).is_empty());
    // The fence of no exhibit: nothing happens and nothing is shown.
    let to = on(&app, lone, (0, 0));
    click(&mut app, 260, to);
    assert!(told(&app, &["world", "exhibit", "tool message"]).is_empty());
    // A decayed piece costs the gate: shown as the price, refused without the cash.
    {
        let game = app.game.as_mut().unwrap();
        let at = game.world.find(east).unwrap();
        game.world.zoo.entities[at]
            .common
            .as_mut()
            .unwrap()
            .fence
            .as_mut()
            .unwrap()
            .life = 0;
    }
    app.pass(300, &[on(&app, east, (0, 0))]);
    assert_eq!(app.game.as_ref().unwrap().cost(), Some(125));
    app.set_money(Money {
        cash: 100.0,
        ..Money::default()
    });
    let to = on(&app, east, (0, 0));
    click(&mut app, 310, to);
    assert_eq!(
        told(&app, &["world", "tool message"]),
        ["tool message 10002"]
    );
    assert_eq!(app.money().unwrap().cash, 100.0);
    // The cash short when the pointer reaches the piece: the price is shown, the gate is not
    // valid, and a click does nothing and says nothing (10002 needs a place valid on arrival).
    app.pass(312, &[on(&app, west, (0, 0))]);
    app.pass(314, &[on(&app, east, (0, 0))]);
    assert_eq!(
        told(&app, &["tool pending"]),
        ["tool pending fences/iron/g 20,20 q 3 invalid"]
    );
    assert_eq!(app.game.as_ref().unwrap().cost(), Some(125));
    click(&mut app, 316, to);
    assert!(told(&app, &["world", "exhibit", "tool message"]).is_empty());
    assert_eq!(app.money().unwrap().cash, 100.0);
    // With the cash, the piece is made healthy and the price booked.
    app.set_money(Money {
        cash: 1_000.0,
        ..Money::default()
    });
    app.pass(322, &[on(&app, west, (0, 0))]);
    app.pass(324, &[on(&app, east, (0, 0))]);
    let old_before = fence_of(&app, north);
    click(&mut app, 330, to);
    let old_gate = (old_before, fence_of(&app, north));
    assert_eq!(
        told(&app, &["world"]),
        [
            format!("world ungate fences/iron/f 20,20 side 0 id {north}"),
            format!("world gate fences/iron/g 20,20 side 2 id {east}")
        ]
    );
    assert_eq!(app.money().unwrap().cash, 875.0);
    let game = app.game.as_ref().unwrap();
    let piece = &game.world.zoo.entities[game.world.find(east).unwrap()];
    let fence = piece.common.as_ref().unwrap().fence.unwrap();
    assert_eq!(fence.life, 7);
    // And it drew a wear interval, which the countdown starts at.
    assert!(fence.timers[0] > 0 && fence.timers[0] == fence.timers[1]);
    Seen {
        chosen_gate,
        old_gate,
    }
}

/// The `dev` lines and the step of a pass.
fn dev_and_step(app: &App, pass: u64, now: u64) -> (Vec<String>, Option<u32>) {
    let dev = app.log_lines(pass, now).into_iter();
    let dev = dev.filter(|l| l.contains(" dev ")).collect();
    (dev, app.fired().step)
}

#[test]
fn the_speed_keys_work_in_developer_mode_only_and_only_with_the_ini_key() {
    let (dot, comma, slash) = (
        Command::Char(b'.'),
        Command::Char(b','),
        Command::Char(b'/'),
    );
    // Without the ini key nothing changes: the steps stay those of speed 1.0.
    with_app("app-speed-off", |app, _| {
        app.pass(0, &[]);
        app.pass(70, &[Command::DevMode, dot, dot]);
        assert_eq!(dev_and_step(app, 1, 70), (vec![], Some(70)));
        app.pass(140, &[]);
        assert_eq!(app.fired().step, Some(70));
    });
    with_app("app-speed-on", |app, _| {
        app.timers.speed = crate::timing::Speed::new(true);
        app.pass(0, &[]);
        // The keys do nothing before the mode is on.
        app.pass(70, &[dot, comma, slash]);
        assert_eq!(dev_and_step(app, 1, 70), (vec![], Some(70)));
        // Two `.`: 1.2000000000000002, as the original gives; the step of 70 ms is 84 ms.
        app.pass(140, &[Command::DevMode, dot, dot]);
        assert_eq!(
            dev_and_step(app, 2, 140),
            (
                vec![
                    "pass 2 t=140 dev on".into(),
                    "pass 2 t=140 dev speed 1.1".into(),
                    "pass 2 t=140 dev speed 1.2000000000000002".into()
                ],
                Some(84)
            )
        );
        // `/` sets 1.0 again; the keys are not passed on while the mode is on.
        app.pass(210, &[slash]);
        assert_eq!(
            dev_and_step(app, 3, 210),
            (vec!["pass 3 t=210 dev speed 1".into()], Some(70))
        );
        // Downward the tenths stop at 0.10000000000000014: a step of 70 ms is 7 ms.
        app.pass(280, &[comma; 20]);
        assert_eq!(app.timers.speed.factor(), 0.10000000000000014);
        app.pass(350, &[]);
        assert_eq!(app.fired().step, Some(7));
        // Ctrl+D again: the mode is off, the speed stays, the keys do nothing.
        app.pass(420, &[Command::DevMode, dot]);
        assert_eq!(dev_and_step(app, 6, 420).0, ["pass 6 t=420 dev off"]);
        assert_eq!(app.timers.speed.factor(), 0.10000000000000014);
    });
}

#[test]
fn developer_mode_draws_its_name_over_the_map() {
    with_app("app-speed-text", |app, _| {
        app.timers.speed = crate::timing::Speed::new(true);
        app.set_interface(interface("speed"));
        let plain = app.pass(0, &[]).unwrap().pixels.clone();
        let on = app.pass(20, &[Command::DevMode]).unwrap().pixels.clone();
        assert_ne!(on, plain);
        let off = app.pass(40, &[Command::DevMode]).unwrap().pixels.clone();
        assert_eq!(off, plain);
    });
}
