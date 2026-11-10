//! The window and its renderer: the game size, the factor of the display scale, the lines that
//! tell how they came out.

use rustyzoo::config::Config;
use sdl3::pixels::Color;
use sdl3::render::{Canvas, FRect, Texture};
use sdl3::video::{Window, WindowPos};

use super::fullscreen;
use super::size::{clamp, ini_size, plan};
use crate::CliError;

/// An open window.
pub struct Screen {
    pub canvas: Canvas<Window>,
    /// The game size, not the window's.
    pub size: (u32, u32),
    /// The game is full screen (the camera's rules apply). It follows the last switch that the
    /// window accepted, and what the window reports on its own.
    pub fullscreen: bool,
    /// The game was resized in full screen: the window is fitted to the size when it is left.
    unfitted: bool,
}

impl Screen {
    /// Copies the game picture to the window, scaled by a whole factor and centred on black
    /// (see [`fullscreen::fit`]); at the size the window opens with, it fills the window.
    pub fn copy(&mut self, texture: &Texture) -> Result<(), CliError> {
        let out = self.canvas.output_size()?;
        let (x, y, w, h) = fullscreen::fit(out, self.size);
        self.canvas.set_draw_color(Color::RGB(0, 0, 0));
        self.canvas.clear();
        Ok(self.canvas.copy(texture, None, FRect::new(x, y, w, h))?)
    }

    /// Asks the window to go to full screen or back; whether it was done. If it cannot, it
    /// stays as it is.
    pub fn request(&mut self, on: bool) -> bool {
        let done = self.canvas.window_mut().set_fullscreen(on);
        if let Err(e) = &done {
            eprintln!(
                "rustyzoo: cannot switch to {}: {e}",
                if on { "full screen" } else { "a window" }
            );
        }
        done.is_ok()
    }

    /// Gives the game another size (16.9 point 2): the window takes the new size times the factor
    /// of the display and is centred on the primary screen. In full screen the picture is scaled
    /// to the display, so only the size changes, and the window is fitted when it is left
    /// ([`Screen::refit`]). Whether it was done; a failure warns and changes nothing.
    pub fn resize(&mut self, video: &sdl3::VideoSubsystem, size: (u32, u32)) -> bool {
        if size == self.size {
            return false;
        }
        if self.fullscreen {
            self.unfitted = true;
        } else if let Err(e) = self.fit_window(video, size) {
            eprintln!("rustyzoo: cannot resize to {}x{}: {e}", size.0, size.1);
            return false;
        }
        self.size = size;
        true
    }

    /// Fits the window to the game size once the window is back from a full screen in which the
    /// game was resized. A failure warns.
    pub fn refit(&mut self, video: &sdl3::VideoSubsystem) {
        if self.unfitted && !self.fullscreen {
            self.unfitted = false;
            if let Err(e) = self.fit_window(video, self.size) {
                eprintln!("rustyzoo: cannot fit the window: {e}");
            }
        }
    }

    fn fit_window(
        &mut self,
        video: &sdl3::VideoSubsystem,
        size: (u32, u32),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let usable = video.get_primary_display()?.get_usable_bounds()?;
        let window = self.canvas.window_mut();
        let (_, want) = plan(
            size,
            window.display_scale(),
            window.pixel_density(),
            (usable.width(), usable.height()),
        );
        window.set_size(want.0, want.1)?;
        window.set_position(WindowPos::Centered, WindowPos::Centered);
        window.sync();
        Ok(())
    }

    /// The pixel of the game under the cursor at `at`, in points of the window.
    pub fn pointer(&self, at: (f32, f32)) -> Result<(i32, i32), CliError> {
        let out = self.canvas.output_size()?;
        Ok(fullscreen::pointer(
            at,
            self.canvas.window().size(),
            out,
            self.size,
        ))
    }
}

/// The desktop of the primary display: width, height and bits per pixel (ui layout 16.3).
pub fn desktop(video: &sdl3::VideoSubsystem) -> Option<rustyzoo::app::Display> {
    let mode = video.get_primary_display().ok()?.get_mode().ok()?;
    Some((mode.w, mode.h, i32::from(mode.format.bits_per_pixel())))
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
        // The maximise button, which goes to full screen, needs a resizable window.
        .resizable()
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
    Ok(Screen {
        canvas,
        size,
        fullscreen: false,
        unfitted: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game resized in full screen sizes the window for the way back: the window keeps its size
    /// in full screen, and takes the new one once the window is back (16.9 point 2).
    #[test]
    fn a_resize_in_full_screen_sizes_the_window_when_it_is_left() {
        sdl3::hint::set("SDL_VIDEO_DRIVER", "dummy");
        let sdl = sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        let ini = Config::parse(b"").unwrap();
        let mut screen = open(&video, &ini, Some((1024, 768))).ok().unwrap();
        let before = screen.canvas.window().size();
        screen.fullscreen = true;
        assert!(screen.resize(&video, (800, 600)));
        assert_eq!(screen.size, (800, 600));
        assert_eq!(screen.canvas.window().size(), before);
        screen.refit(&video);
        assert_eq!(screen.canvas.window().size(), before, "still full screen");
        screen.fullscreen = false;
        screen.refit(&video);
        let (_, want) = plan(
            (800, 600),
            screen.canvas.window().display_scale(),
            screen.canvas.window().pixel_density(),
            (0, 0),
        );
        assert_eq!(screen.canvas.window().size(), want);
        assert_ne!(want, before);
        assert!(!screen.resize(&video, (800, 600)), "the same size changes nothing");
    }
}
