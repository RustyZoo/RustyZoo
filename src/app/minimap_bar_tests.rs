//! The minimap of the main bar in a game (map drawing 10): its picture and its click, on the
//! made-up map of the bulldozer tests (40 by 40 tiles).

use super::bulldozer_tests::{ROOT, Session, catalogue, with_game_of};
use super::*;

/// The minimap 60 by 30 at (10, 10) of the window, and its colour file.
fn root() -> String {
    format!(
        "{ROOT}[mm]\ntype=ZTMiniMap\nid=1026\nx=10\ny=10\ndx=60\ndy=30\nlayer=5\nstate=0\n\
         backcolor=255\nbackcolor=0\nbackcolor=255\ncolormap=ui/miniclr.cfg\n"
    )
}

const COLORS: &[u8] =
    b"[Grass]\ncolor=10\ncolor=200\ncolor=30\n[ScreenView]\ncolor=250\ncolor=250\ncolor=250\n";

fn play(name: &str, ini: &str, body: impl FnOnce(&mut Session)) {
    let more = [("ui/miniclr.cfg", COLORS.to_vec())];
    with_game_of(name, ini, (catalogue(), &root(), &more), |s| {
        s.app.interface().unwrap().set_hidden(1026, false);
        body(s)
    });
}

/// Passes of 20 ms until the interface has run twice (every 200 ms in these tests).
fn settle(s: &mut Session) {
    for _ in 0..22 {
        s.pass(&[]);
    }
}

/// The release of the left button at a point of the window.
fn click(x: i32, y: i32) -> [Command; 3] {
    [
        Command::Mouse(Mouse::Move(x, y)),
        Command::Mouse(Mouse::Down),
        Command::Mouse(Mouse::Up),
    ]
}

fn centre(s: &Session) -> (i64, i64) {
    let game = s.app.game.as_ref().unwrap();
    let (x, y, _) = game.camera.centre_point(&game.world.zoo);
    (x / 64, y / 64)
}

/// The tile of the widget pixel `at`, if the minimap has a cell there.
fn tile(s: &Session, at: (i32, i32)) -> Option<(i64, i64)> {
    s.app.game.as_ref()?.minimap.as_ref()?.tile_at(at)
}

#[test]
fn a_click_on_the_minimap_centres_the_view_on_the_tile_of_the_cell_and_one_on_the_rest_does_nothing()
 {
    play("minimap-click", "", |s| {
        settle(s);
        // A map of 40 tiles in a widget of 60 by 30: the diamond is 39 pixels high, so the
        // block shows only 30 rows of cells; pick a cell near the middle and one outside.
        let at = (30, 15);
        let target = tile(s, at).expect("a drawn cell in the middle");
        let before = centre(s);
        s.pass(&click(10 + at.0, 10 + at.1));
        assert_eq!(centre(s), target);
        assert_ne!(before, target);
        // The pixel (0, 0) of the widget is keyed out: the click goes through to the map view.
        assert_eq!(tile(s, (0, 0)), None);
        let before = centre(s);
        s.pass(&click(10, 10));
        assert_eq!(centre(s), before);
    });
}

#[test]
fn the_minimap_follows_the_view_at_the_next_pass() {
    play("minimap-follow", "", |s| {
        settle(s);
        let first = s
            .app
            .game
            .as_ref()
            .unwrap()
            .minimap
            .as_ref()
            .unwrap()
            .tile_at((30, 15));
        s.pass(&click(40, 25));
        settle(s);
        let moved = s
            .app
            .game
            .as_ref()
            .unwrap()
            .minimap
            .as_ref()
            .unwrap()
            .tile_at((30, 15));
        assert!(first.is_some() && moved.is_some());
        assert_ne!(first, moved, "the block went with the view");
    });
}

#[test]
fn a_minimap_that_is_hidden_is_not_made() {
    let more = [("ui/miniclr.cfg", COLORS.to_vec())];
    with_game_of("minimap-hidden", "", (catalogue(), &root(), &more), |s| {
        s.app.interface().unwrap().set_hidden(1026, true);
        settle(s);
        assert!(s.app.game.as_ref().unwrap().minimap.is_none());
    });
}
