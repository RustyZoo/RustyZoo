//! The pass of the map view on a synthetic map: one grass tile picture of two pixels.

use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::menu::Menu;
use crate::render::Options;
use crate::render::order::project;
use crate::render::tests::{cache_install, flat_map, frame_file, tile_with};
use crate::sprite::tests::Install;
use crate::ui::interface::Interface;
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
        let view = app.camera().unwrap().scroll;
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
        Command::Fullscreen(true),
        Command::Fullscreen(false),
        Command::Pause,
        Command::Quit,
        Command::Close,
        Command::Mouse(Mouse::Move(-1, 77)),
        Command::Mouse(Mouse::Down),
        Command::Mouse(Mouse::Up),
        Command::Mouse(Mouse::Double),
        Command::Mouse(Mouse::Wheel(-2)),
        Command::Key(Key::PageDown, false, false),
        Command::Key(Key::Enter, true, false),
        Command::Key(Key::Y, true, true),
        Command::Char(0),
        Command::Char(255),
        Command::Skip,
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
        assert_eq!(lines.len(), 3, "no step without a game: {lines:?}");
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
        assert!(lines.contains(&"pass 3 t=60 ui select 7116".to_string()));
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
