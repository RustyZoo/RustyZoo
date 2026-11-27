//! The window and its renderer: the game size, the factor of the display scale, the lines that
//! tell how they came out.

use rustyzoo::config::Config;
use sdl3::pixels::{Color, PixelFormat};
use sdl3::render::{Canvas, FRect, ScaleMode, Texture};
use sdl3::surface::Surface;
use sdl3::video::{Window, WindowPos};

use super::fullscreen;
use super::size::{clamp, ini_size, plan};
use crate::CliError;

/// An open window.
pub struct Screen {
    /// The enlarged picture that is smoothed to the window, kept while its size stays. It is
    /// declared before the canvas so that it is dropped first.
    big: Option<Texture>,
    pub canvas: Canvas<Window>,
    /// The game size, not the window's.
    pub size: (u32, u32),
    /// The game is full screen (the camera's rules apply). It follows the last switch that the
    /// window accepted, and what the window reports on its own.
    pub fullscreen: bool,
    /// The window waits for a fit to the game size.
    fit: Fit,
    /// The picture cannot be smoothed: it was warned once, and the nearest copy is used from then on.
    no_smooth: bool,
}

/// When the window is fitted to the game size: at once in a window, and when a full screen is
/// left if the game was resized in it.
#[derive(Default)]
struct Fit {
    waiting: bool,
}

impl Fit {
    /// The game was resized: whether to fit the window now.
    fn resized(&mut self, fullscreen: bool) -> bool {
        self.waiting |= fullscreen;
        !fullscreen
    }

    /// Whether the window is due a fit: it was waiting and is not in full screen any more.
    fn due(&mut self, fullscreen: bool) -> bool {
        !fullscreen && std::mem::take(&mut self.waiting)
    }
}

impl Screen {
    /// Copies the game picture to the window, centred on black, as large as it can be at its
    /// aspect ratio (see [`fullscreen::fit`]). It is enlarged by the whole factor with the nearest
    /// pixel, then smoothed to the rectangle if that is not a whole factor ("sharp bilinear");
    /// at the size the window opens with, it fills the window.
    pub fn copy(&mut self, texture: &Texture) -> Result<(), CliError> {
        let out = self.canvas.output_size()?;
        let (x, y, w, h) = fullscreen::fit(out, self.size);
        let k = fullscreen::whole(out, self.size);
        self.canvas.set_draw_color(Color::RGB(0, 0, 0));
        self.canvas.clear();
        let to = FRect::new(x, y, w, h);
        let exact = (w, h) == ((self.size.0 * k) as f32, (self.size.1 * k) as f32);
        if k > 0 && !exact && !self.no_smooth {
            match self.smooth(texture, k, to) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    self.no_smooth = true;
                    self.big = None;
                    rustyzoo::logln!("rustyzoo: cannot smooth the picture: {e}");
                }
            }
        }
        Ok(self.canvas.copy(texture, None, to)?)
    }

    /// Enlarges the game picture `k` times with the nearest pixel into the kept texture (made
    /// again when its size changes), and copies that to `to` with a linear filter.
    fn smooth(&mut self, texture: &Texture, k: u32, to: FRect) -> Result<(), String> {
        let want = (self.size.0 * k, self.size.1 * k);
        if self.big.as_ref().map(|b| (b.width(), b.height())) != Some(want) {
            self.big = None;
            let mut big = self
                .canvas
                .texture_creator()
                .create_texture_target(PixelFormat::RGBA32, want.0, want.1)
                .map_err(|e| e.to_string())?;
            big.set_scale_mode(ScaleMode::Linear);
            self.big = Some(big);
        }
        let big = self.big.as_mut().ok_or("no texture")?;
        let mut copied = Ok(());
        self.canvas
            .with_texture_canvas(big, |c| copied = c.copy(texture, None, None))
            .map_err(|e| e.to_string())?;
        copied.map_err(|e| e.to_string())?;
        self.canvas.copy(big, None, to).map_err(|e| e.to_string())
    }

    /// Asks the window to go to full screen or back; whether it was done. If it cannot, it
    /// stays as it is.
    pub fn request(&mut self, on: bool) -> bool {
        let done = self.canvas.window_mut().set_fullscreen(on);
        if let Err(e) = &done {
            rustyzoo::logln!(
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
        if self.fit.resized(self.fullscreen)
            && let Err(e) = self.fit_window(video, size)
        {
            rustyzoo::logln!("rustyzoo: cannot resize to {}x{}: {e}", size.0, size.1);
            return false;
        }
        self.size = size;
        true
    }

    /// Fits the window to the game size once the window is back from a full screen in which the
    /// game was resized. A failure warns.
    pub fn refit(&mut self, video: &sdl3::VideoSubsystem) {
        if self.fit.due(self.fullscreen)
            && let Err(e) = self.fit_window(video, self.size)
        {
            rustyzoo::logln!("rustyzoo: cannot fit the window: {e}");
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
    let mut window = video
        .window("RustyZoo", want.0, want.1)
        .high_pixel_density()
        // The maximise button, which goes to full screen, needs a resizable window.
        .resizable()
        .build()?;
    set_icon(&mut window);
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
    // At a fractional scale the window may have an odd pixel more, left as a border.
    if fullscreen::whole(pixels, size) != k {
        rustyzoo::logln!("rustyzoo: the window has {pixels:?} pixels, not the game size times {k}");
    }
    Ok(Screen {
        big: None,
        canvas,
        size,
        fullscreen: false,
        fit: Fit::default(),
        no_smooth: false,
    })
}

/// The application icon, the native 32 pixel drawing (packaging/icon).
const ICON: &[u8] = include_bytes!("../../packaging/icon/png/rustyzoo-32.png");

/// The 32 by 32 RGBA pixels of the icon.
fn icon_pixels() -> Result<(u32, Vec<u8>), png::DecodingError> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(ICON)).read_info()?;
    let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    Ok((info.width, pixels))
}

/// Gives the window the application icon; a failure only prints a warning.
fn set_icon(window: &mut Window) {
    let made = icon_pixels()
        .map_err(|e| e.to_string())
        .and_then(|(side, mut rgba)| {
            let surface = Surface::from_data(&mut rgba, side, side, side * 4, PixelFormat::RGBA32)
                .map_err(|e| e.to_string())?;
            window.set_icon(surface);
            Ok(())
        });
    if let Err(e) = made {
        rustyzoo::logln!("rustyzoo: warning: window icon: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The embedded icon is a 32 by 32 picture with a clear corner and a drawn centre.
    #[test]
    fn the_icon_decodes_to_32_square_rgba() {
        let (side, pixels) = icon_pixels().unwrap();
        assert_eq!((side, pixels.len()), (32, 32 * 32 * 4));
        assert_eq!(pixels[3], 0, "corner is transparent");
        assert_eq!(pixels[(16 * 32 + 16) * 4 + 3], 255, "centre is drawn");
    }

    /// A game resized in full screen sizes the window for the way back (16.9 point 2): nothing
    /// at the resize, one fit when the window is back, none after.
    #[test]
    fn a_resize_in_full_screen_fits_the_window_when_it_is_left() {
        let mut fit = Fit::default();
        assert!(!fit.resized(true), "no window to size in full screen");
        assert!(!fit.due(true), "still full screen");
        assert!(fit.due(false), "back in a window");
        assert!(!fit.due(false), "once");
        assert!(
            fit.resized(false) && !fit.due(false),
            "in a window: at once"
        );
    }
}
