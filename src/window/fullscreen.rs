//! Full screen (input 3.4, camera 8.3): how the game picture sits on a display, or on a window
//! the player has resized, that is larger than the game.
//!
//! The game keeps its size. The picture is scaled to the largest rectangle of the game's aspect
//! ratio that fits, centred, with black on the two sides (or above and below) that are left: a
//! 1280x1024 game fills the height of a 3440x1440 display. The pixels stay sharp: the picture is
//! first enlarged by the largest whole factor (nearest pixel), and only the rest is smoothed
//! ([`Screen::copy`](super::screen::Screen::copy)). The cursor is mapped back, and a cursor over
//! the black is taken for the nearest edge pixel, so the scroll band is reached at the edge of the
//! display.

/// The largest whole factor by which `game` pixels fit in `out` pixels (0 when they do not fit).
/// It is counted in pixels of the output, not in points, so a high-density or fractional display
/// gets the factor its pixels allow. It is the factor of the sharp enlargement that comes before
/// the smooth one to the rectangle of [`fit`].
pub fn whole(out: (u32, u32), game: (u32, u32)) -> u32 {
    (out.0 / game.0.max(1)).min(out.1 / game.1.max(1))
}

/// Where the picture of `game` pixels goes on an output of `out` pixels: `(x, y, width, height)`.
/// The largest rectangle of the game's aspect ratio that fits, whole pixels, centred: it fills
/// the height or the width of the output (a smaller output scales the picture down).
pub fn fit(out: (u32, u32), game: (u32, u32)) -> (f32, f32, f32, f32) {
    let (gw, gh) = (u64::from(game.0.max(1)), u64::from(game.1.max(1)));
    let (ow, oh) = (u64::from(out.0), u64::from(out.1));
    let (w, h) = if ow * gh <= oh * gw {
        (ow, gh * ow / gw)
    } else {
        (gw * oh / gh, oh)
    };
    (
        ((ow - w) / 2) as f32,
        ((oh - h) / 2) as f32,
        w as f32,
        h as f32,
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

    #[test]
    fn the_picture_fills_the_height_or_the_width_and_is_centred() {
        assert_eq!(fit((1920, 1080), (800, 600)), (240.0, 0.0, 1440.0, 1080.0));
        assert_eq!(fit((3840, 2160), (800, 600)), (480.0, 0.0, 2880.0, 2160.0));
        assert_eq!(fit((800, 600), (800, 600)), (0.0, 0.0, 800.0, 600.0));
        assert_eq!(fit((1600, 1200), (800, 600)), (0.0, 0.0, 1600.0, 1200.0));
        // A 1280x1024 game on an ultra-wide display: bars at the sides only.
        assert_eq!(
            fit((3440, 1440), (1280, 1024)),
            (820.0, 0.0, 1800.0, 1440.0)
        );
        // A tall output: bars above and below.
        assert_eq!(fit((800, 1000), (800, 600)), (0.0, 200.0, 800.0, 600.0));
        // A display smaller than the game: scaled down to fit, never to 0.
        assert_eq!(fit((640, 480), (1280, 960)), (0.0, 0.0, 640.0, 480.0));
        assert_eq!(fit((0, 0), (800, 600)).2, 0.0);
        assert_eq!(fit((100, 100), (0, 0)).2, 100.0, "no division by 0");
        let max = fit((u32::MAX, u32::MAX), (1, 1));
        assert_eq!((max.2, max.3), (u32::MAX as f32, u32::MAX as f32));
    }

    #[test]
    fn full_screen_counts_the_factor_in_pixels_of_the_display() {
        // A 3456x2234 Retina panel is 1728x1117 points: a 1152x744 game fits once in points but
        // three times in pixels, and fills the width with a row of black above and below.
        assert_eq!(whole((3456, 2234), (1152, 744)), 3);
        assert_eq!(fit((3456, 2234), (1152, 744)), (0.0, 1.0, 3456.0, 2232.0));
        // Not a whole factor: the height is filled, the factor 2 is enlarged to it.
        assert_eq!(whole((3456, 2234), (1024, 768)), 2);
        assert_eq!(fit((3456, 2234), (1024, 768)), (239.0, 0.0, 2978.0, 2234.0));
        assert_eq!(whole((1920, 1080), (640, 360)), 3);
        assert_eq!(whole((2560, 1600), (1280, 800)), 2);
        assert_eq!(whole((0, 0), (800, 600)), 0);
        assert_eq!(whole((800, 600), (0, 0)), 600);
        assert_eq!(whole((u32::MAX, u32::MAX), (1, 1)), u32::MAX);
    }

    #[test]
    fn the_cursor_is_mapped_into_the_game_and_the_black_counts_as_the_edge() {
        let (out, game) = ((1920, 1080), (800, 600));
        let at = |x, y| pointer((x, y), (1920, 1080), out, game);
        assert_eq!(at(240.0, 0.0), (0, 0));
        assert_eq!(at(960.0, 540.0), (400, 300));
        assert_eq!(at(1679.0, 1079.0), (799, 599));
        assert_eq!(at(0.0, 0.0), (0, 0), "the corner of the display");
        assert_eq!(at(1919.0, 1079.0), (799, 599));
        assert_eq!(at(100.0, 540.0), (0, 300), "the left bar");
        assert_eq!(at(1800.0, 540.0), (799, 300), "the right bar");
        // Points of the window are not pixels of the output (a high-density display).
        assert_eq!(
            pointer((480.0, 270.0), (960, 540), (1920, 1080), game),
            (400, 300)
        );
        // Nonsense never panics.
        assert_eq!(pointer((f32::NAN, 5.0), (0, 0), (0, 0), (0, 0)).1, 0);
    }
}
