//! Switching between window and full screen (input 3.4, camera 8.3): when to switch, and how the
//! game picture sits on a display that is larger than the game.
//!
//! The game keeps its size. In full screen the picture is scaled by the largest whole factor
//! that fits and centred, with black around it, so the pixels stay sharp on any display; the
//! cursor is mapped back, and a cursor over the black is taken for the nearest edge pixel, so the
//! scroll band is reached at the edge of the display.

use sdl3::video::WindowFlags;

use super::events::asleep;

/// What the window has to do about full screen, if anything: `Some(on)` is a request to switch to
/// `on`. `toggle` is Alt+Enter, which acts only while the window is active; `maximized` is the
/// maximise button, which only a windowed window can press. The window flags are read, not
/// followed through events.
pub fn request(flags: WindowFlags, toggle: bool, maximized: bool) -> Option<bool> {
    let full = flags.contains(WindowFlags::FULLSCREEN);
    match () {
        _ if asleep(flags) => None,
        _ if toggle && flags.contains(WindowFlags::INPUT_FOCUS) => Some(!full),
        _ if maximized && !full && flags.contains(WindowFlags::MAXIMIZED) => Some(true),
        _ => None,
    }
}

/// Where the picture of `game` pixels goes on an output of `out` pixels: `(x, y, width, height)`.
/// The factor is whole when the output is at least as large as the game; a smaller output scales
/// the picture down to fit.
pub fn fit(out: (u32, u32), game: (u32, u32)) -> (f32, f32, f32, f32) {
    let (gw, gh) = (game.0.max(1), game.1.max(1));
    let k = match (out.0 / gw).min(out.1 / gh) {
        0 => (out.0 as f32 / gw as f32).min(out.1 as f32 / gh as f32),
        k => k as f32,
    };
    let (w, h) = (gw as f32 * k, gh as f32 * k);
    (
        ((out.0 as f32 - w) / 2.0).floor(),
        ((out.1 as f32 - h) / 2.0).floor(),
        w,
        h,
    )
}

/// The pixel of the game under the cursor at `at` (points of the window, whose size is `points`)
/// when the picture is placed by [`fit`] on `out` pixels: always inside the game.
pub fn pointer(
    at: (f32, f32),
    points: (u32, u32),
    out: (u32, u32),
    game: (u32, u32),
) -> (i32, i32) {
    let (x, y, w, h) = fit(out, game);
    let axis = |p: f32, points: u32, out: u32, from: f32, len: f32, game: u32| {
        let pixel = p * out as f32 / points.max(1) as f32;
        let g = ((pixel - from) * game as f32 / len.max(1.0)).floor();
        g.clamp(0.0, game.saturating_sub(1) as f32) as i32
    };
    (
        axis(at.0, points.0, out.0, x, w, game.0),
        axis(at.1, points.1, out.1, y, h, game.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: WindowFlags = WindowFlags::FULLSCREEN;
    const FOCUS: WindowFlags = WindowFlags::INPUT_FOCUS;

    #[test]
    fn alt_enter_and_the_maximise_button_switch() {
        assert_eq!(request(FOCUS, true, false), Some(true));
        assert_eq!(request(FS | FOCUS, true, false), Some(false));
        assert_eq!(request(WindowFlags::empty(), true, false), None, "inactive");
        assert_eq!(request(FOCUS | WindowFlags::MINIMIZED, true, false), None);
        assert_eq!(request(FOCUS, false, false), None);
        let max = WindowFlags::MAXIMIZED;
        assert_eq!(request(max, false, true), Some(true));
        assert_eq!(request(FS | max, false, true), None, "already full screen");
        assert_eq!(
            request(FOCUS, false, true),
            None,
            "a restore is not a maximise"
        );
        assert_eq!(request(max | WindowFlags::HIDDEN, false, true), None);
    }

    #[test]
    fn the_picture_is_scaled_by_a_whole_factor_and_centred() {
        assert_eq!(fit((1920, 1080), (800, 600)), (560.0, 240.0, 800.0, 600.0));
        assert_eq!(
            fit((3840, 2160), (800, 600)),
            (720.0, 180.0, 2400.0, 1800.0)
        );
        assert_eq!(fit((800, 600), (800, 600)), (0.0, 0.0, 800.0, 600.0));
        // A display smaller than the game: scaled down to fit, never to 0.
        assert_eq!(fit((640, 480), (1280, 960)), (0.0, 0.0, 640.0, 480.0));
        assert_eq!(fit((0, 0), (800, 600)).2, 0.0);
        assert_eq!(fit((100, 100), (0, 0)).2, 100.0, "no division by 0");
    }

    #[test]
    fn the_cursor_is_mapped_into_the_game_and_the_black_counts_as_the_edge() {
        let (out, game) = ((1920, 1080), (800, 600));
        let at = |x, y| pointer((x, y), (1920, 1080), out, game);
        assert_eq!(at(560.0, 240.0), (0, 0));
        assert_eq!(at(960.0, 540.0), (400, 300));
        assert_eq!(at(1359.0, 839.0), (799, 599));
        assert_eq!(at(0.0, 0.0), (0, 0), "the corner of the display");
        assert_eq!(at(1919.0, 1079.0), (799, 599));
        assert_eq!(at(100.0, 540.0), (0, 300), "the left bar");
        // Points of the window are not pixels of the output (a high-density display).
        assert_eq!(
            pointer((480.0, 270.0), (960, 540), (1920, 1080), game),
            (400, 300)
        );
        // Nonsense never panics.
        assert_eq!(pointer((f32::NAN, 5.0), (0, 0), (0, 0), (0, 0)).1, 0);
    }
}
