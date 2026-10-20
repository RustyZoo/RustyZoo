//! The start-up picture and its progress bar (main menu 3.3), and the product version the main
//! menu shows. No window: the caller presents [`Startup::frame`] when a step says so.

use std::path::Path;

use crate::Framebuffer;
use crate::bitmap;
use crate::config::Config;
use crate::pe::{self, ResKey};
use crate::sprite::show;
use crate::strings::library_names;
use crate::text::fill_color;
use crate::timing::ini_whole;
use crate::vfs::{MAX_MEMBER_SIZE, find_ignoring_case};

/// The bitmap resource of the start-up picture for the expansion flags (0 to 3).
pub fn picture_number(expansions: i32) -> u32 {
    502 + (expansions & 3) as u32
}

/// The data of the first resource of `kind` numbered `number` in the libraries of the install
/// folder: the language libraries (`lang*.dll`), then the resource libraries (`res*.dll`), the
/// greatest name first.
fn find_resource(install: &Path, kind: u32, number: u32) -> Option<Vec<u8>> {
    ["lang", "res"]
        .iter()
        .filter_map(|prefix| library_names(install, prefix).ok())
        .flatten()
        .find_map(|name| {
            let path = install.join(name);
            if path.metadata().ok()?.len() > MAX_MEMBER_SIZE {
                return None;
            }
            let file = std::fs::read(path).ok()?;
            let resources = pe::resources(&file).ok()?;
            let found = resources
                .iter()
                .find(|r| r.kind == ResKey::Id(kind) && r.name == ResKey::Id(number))?;
            Some(found.data.to_vec())
        })
}

/// The start-up picture as the original's screen copy shows it: each channel cut to 5 bits
/// (`c >> 3`), green made a 6 bit code `(g5 << 1) | (g5 >> 4)`, then the codes shown with their
/// high bits repeated (main menu 3.3 item 7); `None` if no library has it or it is not
/// a bitmap of 8 or 24 bits.
pub fn picture(install: &Path, expansions: i32) -> Option<Framebuffer> {
    decode(&find_resource(
        install,
        pe::RT_BITMAP,
        picture_number(expansions),
    )?)
}

/// A bitmap resource, which is a bitmap file without its 14-byte header, as a picture.
fn decode(dib: &[u8]) -> Option<Framebuffer> {
    let header = u32::from_le_bytes(dib.get(..4)?.try_into().ok()?) as usize;
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let used = u32::from_le_bytes(dib.get(32..36)?.try_into().ok()?) as usize;
    let table = match bits {
        8 => 4 * if used == 0 { 256 } else { used.min(256) },
        _ => 0,
    };
    let mut file = b"BM".to_vec();
    file.extend((14 + dib.len() as u32).to_le_bytes());
    file.extend([0; 4]);
    file.extend(((14 + header + table) as u32).to_le_bytes());
    file.extend(dib);
    let (w, h, pixels) = bitmap::read(&file).ok()?;
    let mut frame = Framebuffer::new(w as u32, h as u32);
    for (to, from) in frame.pixels.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
        let [r, g, b] = from.map(|c| u16::from(c >> 3));
        *to = [show(0, r), show(1, g << 1 | g >> 4), show(2, b), 255];
    }
    Some(frame)
}

/// The product version of the program in the install folder (`zoo.exe`, or `zoodemo.exe` of the
/// base trial), as its version resource writes it: `12.04.00.0009`.
pub fn product_version(install: &Path) -> Option<String> {
    let path = ["zoo.exe", "zoodemo.exe"]
        .iter()
        .find_map(|name| find_ignoring_case(install, name, false).ok().flatten())?;
    if path.metadata().ok()?.len() > MAX_MEMBER_SIZE {
        return None;
    }
    let file = std::fs::read(path).ok()?;
    let resources = pe::resources(&file).ok()?;
    version_in(
        resources
            .iter()
            .find(|r| r.kind == ResKey::Id(pe::RT_VERSION))?
            .data,
    )
}

/// The `ProductVersion` of the data of a version resource.
fn version_in(data: &[u8]) -> Option<String> {
    let units: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    let key: Vec<u16> = "ProductVersion\0".encode_utf16().collect();
    let at = units.windows(key.len()).position(|w| w == key)? + key.len();
    // The value follows the key, after padding to a whole number of words of 32 bits.
    let value: Vec<u16> = units[at..]
        .iter()
        .copied()
        .skip_while(|&u| u == 0)
        .take_while(|&u| u != 0)
        .collect();
    Some(String::from_utf16_lossy(&value)).filter(|v| !v.is_empty())
}

/// The progress bar: where it is, its colours and how fast it grows.
struct Bar {
    /// Left, top, right and bottom, relative to the corner of the picture.
    rect: (i32, i32, i32, i32),
    color: [u8; 4],
    shadow: [u8; 4],
    offset: (i32, i32),
    /// The progress steps the bar is meant to take.
    calls: i32,
    /// The length the steps have added up to, and its rounded value after the last step.
    length: f32,
    shown: i32,
}

impl Bar {
    fn from_config(ini: &Config) -> Bar {
        let get = |key: &str, default: i32| ini_whole(ini, "UI", key).unwrap_or(default);
        let rgb = |prefix: &str, default: [i32; 3]| {
            let channel = |name: &str, i: usize| get(&format!("{prefix}{name}"), default[i]);
            fill_color([
                channel("Red", 0).clamp(0, 255) as u8,
                channel("Green", 1).clamp(0, 255) as u8,
                channel("Blue", 2).clamp(0, 255) as u8,
            ])
        };
        Bar {
            rect: (
                get("progressLeft", 230),
                get("progressTop", 480),
                get("progressRight", 570),
                get("progressBottom", 495),
            ),
            color: rgb("progress", [255, 184, 43]),
            shadow: rgb("progressShadow", [32, 32, 32]),
            offset: (
                get("progressShadowXOffset", -2),
                get("progressShadowYOffset", 2),
            ),
            calls: get("progresscalls", 957).max(1),
            length: 0.0,
            shown: 0,
        }
    }

    fn width(&self) -> i32 {
        self.rect.2.saturating_sub(self.rect.0).max(0)
    }

    /// One progress step: the increment is added while the length is below the width (so it may
    /// end less than one increment past it), single precision. Once the rounded length is not 0
    /// the shadow and the bar are drawn, with the rounded length after the previous step, and the
    /// screen must be flipped.
    fn step(&mut self, frame: &mut Framebuffer, origin: (i32, i32)) -> bool {
        let width = self.width() as f32;
        if self.length < width {
            self.length += width / self.calls as f32;
        }
        let before = self.shown;
        self.shown = self.length.round() as i32;
        if self.shown == 0 {
            return false;
        }
        let (left, top, _, bottom) = self.rect;
        let at = |dx: i32, dy: i32| {
            (
                origin.0.saturating_add(left).saturating_add(dx),
                origin.1.saturating_add(top).saturating_add(dy),
                before,
                bottom.saturating_sub(top).max(0),
            )
        };
        fill(frame, at(self.offset.0, self.offset.1), self.shadow);
        fill(frame, at(0, 0), self.color);
        true
    }
}

/// Fills the rectangle (left, top, width, height) of the frame, clipped.
fn fill(frame: &mut Framebuffer, (x, y, w, h): (i32, i32, i32, i32), rgba: [u8; 4]) {
    let (fw, fh) = (i64::from(frame.width), i64::from(frame.height));
    let (x, y) = (i64::from(x), i64::from(y));
    let (x0, x1) = (x.clamp(0, fw), (x + i64::from(w)).clamp(0, fw));
    let (y0, y1) = (y.clamp(0, fh), (y + i64::from(h)).clamp(0, fh));
    for row in y0..y1 {
        let start = (row * fw + x0) as usize * 4;
        let end = (row * fw + x1) as usize * 4;
        frame.pixels[start..end].as_chunks_mut::<4>().0.fill(rgba);
    }
}

/// Copies `picture` into `frame` with its top left corner at `origin`, clipped.
fn copy_picture(frame: &mut Framebuffer, picture: &Framebuffer, origin: (i32, i32)) {
    let (fw, fh) = (i64::from(frame.width), i64::from(frame.height));
    let (pw, ph) = (i64::from(picture.width), i64::from(picture.height));
    let (ox, oy) = (i64::from(origin.0), i64::from(origin.1));
    let (x0, x1) = (ox.clamp(0, fw), (ox + pw).clamp(0, fw));
    for row in oy.clamp(0, fh)..(oy + ph).clamp(0, fh) {
        let from = ((row - oy) * pw + (x0 - ox)) as usize * 4;
        let to = (row * fw + x0) as usize * 4;
        let len = (x1 - x0) as usize * 4;
        frame.pixels[to..to + len].copy_from_slice(&picture.pixels[from..from + len]);
    }
}

/// The start-up screen: black, the picture in the middle and the bar over it.
pub struct Startup {
    frame: Framebuffer,
    /// The corner of the picture (0, 0 when there is none).
    origin: (i32, i32),
    bar: Bar,
    steps: u32,
    /// What the picture was: its resource number and corner, for the log.
    picture: Option<(u32, (i32, i32))>,
}

impl Startup {
    /// The screen of a window of `size` pixels, with the picture of the expansion flags drawn.
    pub fn new(install: &Path, ini: &Config, expansions: i32, size: (u32, u32)) -> Startup {
        let mut frame = Framebuffer::new(size.0, size.1);
        frame.fill([0, 0, 0, 255]);
        let mut origin = (0, 0);
        let mut shown = None;
        if let Some(picture) = picture(install, expansions) {
            origin = (
                (size.0 as i32 - picture.width as i32) / 2,
                (size.1 as i32 - picture.height as i32) / 2,
            );
            copy_picture(&mut frame, &picture, origin);
            shown = Some((picture_number(expansions), origin));
        }
        Startup {
            frame,
            origin,
            bar: Bar::from_config(ini),
            steps: 0,
            picture: shown,
        }
    }

    /// One progress step of the loading; whether the frame changed.
    pub fn step(&mut self) -> bool {
        self.steps += 1;
        self.bar.step(&mut self.frame, self.origin)
    }

    pub fn frame(&self) -> &Framebuffer {
        &self.frame
    }

    /// The steps taken so far.
    pub fn steps(&self) -> u32 {
        self.steps
    }

    /// The line for the log that tells which picture was drawn where.
    pub fn log_line(&self) -> String {
        match self.picture {
            Some((number, (x, y))) => format!("picture {number} at {x},{y}"),
            None => "no picture".to_string(),
        }
    }
}

#[cfg(test)]
mod tests;
