//! The window and its renderer: the game size, the factor of the display scale, the lines that
//! tell how they came out.

use rustyzoo::config::Config;
use sdl3::render::Canvas;
use sdl3::video::Window;

use super::size::{clamp, ini_size, plan};
use crate::CliError;

/// An open window.
pub struct Screen {
    pub canvas: Canvas<Window>,
    /// The game size, not the window's.
    pub size: (u32, u32),
    /// The cursor is in points of the window; the game reads it in pixels of the game.
    pub to_game: (f32, f32),
}

/// Opens the window for the game size `size` (the ini file's, cut to the display, when `None`).
pub fn open(
    video: &sdl3::VideoSubsystem,
    ini: &Config,
    size: Option<(u32, u32)>,
) -> Result<Screen, CliError> {
    let display = video.get_primary_display()?;
    let usable = display.get_usable_bounds()?;
    let usable = (usable.width(), usable.height());
    // The scale of the display, before there is a window: its content scale and pixel density.
    let density = display.get_mode().map_or(1.0, |m| m.pixel_density);
    let scale = display.get_content_scale().unwrap_or(1.0) * density;
    // `--size` is not cut, but the factor still gives way when the window does not fit.
    let size = size.unwrap_or_else(|| {
        let pixels = |n: u32| (n as f32 * density) as u32;
        clamp(ini_size(ini), (pixels(usable.0), pixels(usable.1)))
    });
    let (k, want) = plan(size, scale, density, usable);
    let window = video
        .window("RustyZoo", want.0, want.1)
        .high_pixel_density()
        .build()?;
    // The game keeps its size; the window may come out another size.
    let points = window.size();
    let pixels = window.size_in_pixels();
    let (scale, density) = (window.display_scale(), window.pixel_density());
    let canvas = sdl3::render::create_renderer(window, None)?;
    let output = canvas.output_size()?;
    println!("game {}x{} factor {k}", size.0, size.1);
    println!(
        "window {}x{} pixels {}x{} output {}x{}",
        points.0, points.1, pixels.0, pixels.1, output.0, output.1
    );
    println!("display scale {scale} pixel density {density}");
    if output != pixels {
        return Err(
            format!("the output is {output:?} but the window has {pixels:?} pixels").into(),
        );
    }
    if pixels != (size.0 * k, size.1 * k) {
        eprintln!("rustyzoo: the window has {pixels:?} pixels, not the game size times {k}");
    }
    let to_game = (
        size.0 as f32 / points.0 as f32,
        size.1 as f32 / points.1 as f32,
    );
    Ok(Screen {
        canvas,
        size,
        to_game,
    })
}
