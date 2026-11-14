//! The size of the game and of the window: the ini file's or the default, cut to the display, and
//! the whole factor between a pixel of the game and a pixel of the screen.

use rustyzoo::config::Config;
use rustyzoo::timing::user_whole;

/// The window when the ini file does not say.
pub const DEFAULT_SIZE: (u32, u32) = (1024, 768);

/// `[user] screenwidth` and `screenheight`; the default if either is missing or not positive.
pub fn ini_size(ini: &Config) -> (u32, u32) {
    let side = |key| user_whole(ini, key).and_then(|n| u32::try_from(n).ok());
    match (side("screenwidth"), side("screenheight")) {
        (Some(w @ 1..), Some(h @ 1..)) => (w, h),
        _ => DEFAULT_SIZE,
    }
}

/// `[user] fullscreen`: the game starts in full screen when it is a number other than 0. Missing,
/// empty or not a number is windowed (the game's own value, when it is not 0 or 1, is not known).
pub fn ini_fullscreen(ini: &Config) -> bool {
    user_whole(ini, "fullscreen").is_some_and(|n| n != 0)
}

/// `size` cut down to the usable area of the display, in the same unit; an area of 0 means that
/// the system does not say, and cuts nothing.
pub fn clamp(size: (u32, u32), usable: (u32, u32)) -> (u32, u32) {
    let cut = |n: u32, max: u32| if max == 0 { n } else { n.min(max) };
    (cut(size.0, usable.0), cut(size.1, usable.1))
}

/// The whole factor `k` between a pixel of the game and a pixel of the screen, and the window
/// size in points that gives it. `scale` is the display scale (the content scale of the display
/// times its pixel density: 2 at 200% on Windows and X11, where `density` is 1 and the window
/// is in pixels, and on macOS and Wayland, where the content scale is 1, `density` is 2 and the
/// window is in points). So `k` is the same everywhere: the scale rounded, at least 1, and
/// reduced until the window fits in `usable` (points, 0 meaning unknown), never below 1. The
/// points are rounded up, so that at a fractional density the window has at least `k` times the
/// game in pixels however the system rounds them, and an odd pixel left over is a border.
pub fn plan(game: (u32, u32), scale: f32, density: f32, usable: (u32, u32)) -> (u32, (u32, u32)) {
    let density = if density > 0.0 { density } else { 1.0 };
    let fit = |n: u32, max: u32| match max {
        0 => u32::MAX,
        _ => ((max as f32 * density) / n.max(1) as f32) as u32,
    };
    let k = (scale.round() as u32)
        .min(fit(game.0, usable.0))
        .min(fit(game.1, usable.1))
        .max(1);
    let points = |n: u32| (n.saturating_mul(k) as f32 / density).ceil() as u32;
    (k, (points(game.0), points(game.1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ini_file_starts_the_game_in_full_screen() {
        let full = |text: &str| ini_fullscreen(&Config::parse(text.as_bytes()).unwrap());
        assert!(full("[user]\nfullscreen=1\n") && full("[User]\nFullScreen = 2"));
        assert!(full("[user]\nfullscreen=-1\n") && full("[user]\nfullscreen=99999999999\n"));
        assert!(!full("[user]\nfullscreen=0\n") && !full("[user]\nfullscreen=-0\n"));
        assert!(!full("[user]\nfullscreen=\n") && !full("[user]\nfullscreen=yes\n"));
        assert!(!full("[user]\nscreenwidth=800\n") && !full("[ui]\nfullscreen=1\n"));
        assert!(!full("[x]\n"));
    }

    #[test]
    fn the_size_comes_from_the_ini_file_and_is_cut_to_the_display() {
        let ini = |text: &str| ini_size(&Config::parse(text.as_bytes()).unwrap());
        assert_eq!(
            ini("[user]\nScreenWidth=800\nscreenheight=600\n"),
            (800, 600)
        );
        for bad in [
            "",
            "[user]\nscreenwidth=800\n",
            "[user]\nscreenwidth=0\nscreenheight=5\n",
        ] {
            assert_eq!(ini(bad), DEFAULT_SIZE, "{bad:?}");
        }
        assert_eq!(clamp((1024, 768), (800, 1000)), (800, 768));
        assert_eq!(clamp((1024, 768), (0, 0)), (1024, 768), "unknown display");
    }

    #[test]
    fn the_window_is_enlarged_by_the_display_scale_on_every_system() {
        let game = (800, 600);
        // Windows and X11 at 200%: points are pixels, density 1, content scale 2.
        assert_eq!(plan(game, 2.0, 1.0, (2560, 1440)), (2, (1600, 1200)));
        // macOS and Wayland at scale 2: the window is in points, density 2.
        assert_eq!(plan(game, 2.0, 2.0, (1440, 900)), (2, (800, 600)));
        // No scaling.
        assert_eq!(plan(game, 1.0, 1.0, (1920, 1080)), (1, game));
        // Rounded to a whole number, at least 1; a bad value is 1.
        assert_eq!(plan(game, 1.5, 1.0, (0, 0)).0, 2);
        assert_eq!(plan(game, 1.25, 1.0, (0, 0)).0, 1);
        assert_eq!(plan(game, 0.0, 1.0, (0, 0)).0, 1);
        assert_eq!(plan(game, f32::NAN, 0.0, (0, 0)), (1, game));
        // The factor goes down, not the game, when the window does not fit.
        assert_eq!(plan(game, 2.0, 1.0, (1920, 1080)), (1, game));
        assert_eq!(plan((1024, 768), 2.0, 1.0, (1920, 1080)), (1, (1024, 768)));
        assert_eq!(plan((1024, 768), 3.0, 1.0, (2560, 1440)), (1, (1024, 768)));
        assert_eq!(plan((800, 600), 3.0, 1.0, (2560, 1440)), (2, (1600, 1200)));
        assert_eq!(plan((800, 600), 4.0, 2.0, (1440, 900)), (3, (1200, 900)));
        // Too small for even one: k stays 1 (the caller cut the game size).
        assert_eq!(plan(game, 2.0, 1.0, (640, 480)).0, 1);
        // The system does not say how large the display is.
        assert_eq!(plan(game, 2.0, 1.0, (0, 0)).0, 2);
    }

    /// The window's pixels at every display scale, however the system rounds points times the
    /// density: the picture is exactly `k` times the game, with any odd pixel left as a border.
    #[test]
    fn the_window_holds_the_game_times_k_at_fractional_scales() {
        use super::super::fullscreen::{fit, whole};
        // (game, scale, density, usable points, k)
        let cases = [
            // Retina, 3456x2234 pixels at 2: 1728x1117 points less the menu bar.
            ((1152, 744), 2.0, 2.0, (1728, 1079), 2),
            ((1024, 768), 2.0, 2.0, (1728, 1079), 2),
            // An odd game size at a whole density (2056x1329 at 2).
            ((2056, 1329), 2.0, 2.0, (2056, 1290), 1),
            // Wayland at 150% (2560x1440) and 125% (window in points, fractional density).
            ((800, 600), 1.5, 1.5, (1707, 930), 2),
            ((1024, 768), 1.5, 1.5, (1707, 930), 1),
            ((1024, 768), 1.25, 1.25, (2048, 1152), 1),
            ((801, 601), 1.25, 1.25, (0, 0), 1),
            // Windows and X11 at 150% (window in pixels), and no scaling.
            ((800, 600), 1.5, 1.0, (2560, 1400), 2),
            ((1024, 768), 1.0, 1.0, (1920, 1080), 1),
        ];
        for (game, scale, density, usable, k) in cases {
            let (got, points) = plan(game, scale, density, usable);
            assert_eq!(got, k, "{game:?} at {scale}");
            for round in [f32::floor, f32::round, f32::ceil] {
                let pixels = |n: u32| round(n as f32 * density) as u32;
                let out = (pixels(points.0), pixels(points.1));
                assert_eq!(whole(out, game), k, "{game:?} at {scale}: {out:?}");
                let (x, y, w, h) = fit(out, game);
                assert_eq!((w, h), ((game.0 * k) as f32, (game.1 * k) as f32));
                assert!(x + w <= out.0 as f32 && y + h <= out.1 as f32);
            }
        }
        // At 150% an 800x600 game is 1067x800 points, 1601x1200 pixels or 1600x1200.
        assert_eq!(plan((800, 600), 1.5, 1.5, (0, 0)).1, (1067, 400 * 2));
        // A huge game does not overflow.
        assert_eq!(plan((u32::MAX, 1), 3.0, 1.0, (0, 0)).0, 3);
    }
}
