//! Sprite files and palettes of the original game, and drawing of their frames.
//!
//! A sprite file is a list of frames, one per animation step, that share a palette. Frames are
//! run-length coded: every row is a list of segments, each skipping some transparent pixels and
//! then drawing some palette-indexed ones. All integers are little-endian.
//!
//! ```text
//! optional prefix, 9 bytes:  "FATZ", four zero bytes, flag byte (1 = one extra frame follows)
//! u32 frame delay in milliseconds
//! u32 L, then L bytes: palette member path, NUL-terminated
//! u32 N, then N frames; with the flag byte at 1, one more (extra) frame follows them
//! frame:  u32 size, then `size` bytes of frame data
//! frame data:
//!   u16 height (bit 15 set: shadow frame), u16 width, i16 y offset, i16 x offset, u16 unknown
//!   `height` rows: u8 segment count, then per segment u8 skip, u8 count and, except in
//!   shadow frames, `count` palette indexes
//! palette file:  u32 colour count, then that many R, G, B, A entries
//! ```
//!
//! Everything read from a file is untrusted. Malformed data gives a [`SpriteError`]; limits
//! are [`MAX_DIMENSION`] for frames, and the data itself for everything else: a count is
//! checked against the bytes left before anything is allocated for it.

mod blend;
mod frame;
mod palette;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod zoom_tests;

pub use blend::screen;
pub(crate) use blend::{Blend, SHIFT, show};
pub use frame::Frame;
use frame::MIN_FRAME_SIZE;
pub(crate) use frame::Place;
pub use palette::Palette;

use std::error::Error;
use std::fmt;
use std::io::Write;

use crate::Framebuffer;
use crate::vfs::Vfs;

/// The largest width or height of a frame: 4096. The largest in the game is 800 by 657.
pub const MAX_DIMENSION: u16 = 4096;

/// The most colours a palette keeps: a pixel indexes a palette with one byte.
pub(crate) const MAX_COLORS: usize = 256;

/// How strongly a shadow frame darkens what is behind it: three quarters of the destination
/// stay. The ground shadow of an entity is drawn at this strength.
pub(crate) const SHADOW_STRENGTH: f32 = 0.25;

/// Why a sprite or a palette could not be decoded or drawn. The message is a single line.
#[derive(Debug, PartialEq, Eq)]
pub struct SpriteError(String);

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for SpriteError {}

fn error<T>(message: impl Into<String>) -> Result<T, SpriteError> {
    Err(SpriteError(message.into()))
}

/// A little-endian reader over untrusted bytes.
struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize, what: &str) -> Result<&'a [u8], SpriteError> {
        if len > self.data.len() {
            return error(format!(
                "truncated: {what} needs {len} bytes, {} left",
                self.data.len()
            ));
        }
        let (head, tail) = self.data.split_at(len);
        self.data = tail;
        Ok(head)
    }

    fn u16(&mut self, what: &str) -> Result<u16, SpriteError> {
        Ok(u16::from_le_bytes(self.take(2, what)?.try_into().unwrap()))
    }

    fn u32(&mut self, what: &str) -> Result<u32, SpriteError> {
        Ok(u32::from_le_bytes(self.take(4, what)?.try_into().unwrap()))
    }

    /// A u32 length or count, as a `usize`.
    fn len(&mut self, what: &str) -> Result<usize, SpriteError> {
        usize::try_from(self.u32(what)?).or_else(|_| error(format!("{what} is too large")))
    }
}

/// A decoded sprite file.
#[derive(Debug, PartialEq, Eq)]
pub struct Sprite {
    /// Milliseconds each frame is shown.
    pub delay_ms: u32,
    /// The path of the palette member, as written in the file (any letter case, may be empty).
    pub palette: String,
    pub frames: Vec<Frame>,
    /// The frame after the last one that a `FATZ` prefix with flag 1 announces, if any. It is
    /// not counted in the frame count of the file.
    pub extra: Option<Frame>,
}

impl Sprite {
    /// Decodes a sprite file. Bytes after the last frame are ignored, and a file with no frame
    /// at all (a frame count of 0 and no extra frame) is accepted.
    ///
    /// Fails if the data is truncated, if a count or size exceeds the bytes left, or if a
    /// frame is malformed (see [`Frame::parse`]); the message says which frame.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Sprite, SpriteError> {
        let mut reader = Reader { data: bytes };
        let mut has_extra = false;
        if bytes.starts_with(b"FATZ") {
            let prefix = reader.take(9, "FATZ prefix")?;
            has_extra = prefix[8] == 1;
        }
        let delay_ms = reader.u32("frame delay")?;
        let path_len = reader.len("palette path length")?;
        let path = reader.take(path_len, "palette path")?;
        let path = path.split(|&b| b == 0).next().unwrap_or_default();
        let palette = crate::vfs::name_from_bytes(path);
        let count = reader.len("frame count")?;
        // Every frame takes at least its size field and a header: a larger count cannot be true.
        if count > reader.data.len() / (4 + MIN_FRAME_SIZE) {
            return error(format!("frame count {count} exceeds the data"));
        }

        // `count` is bounded by the bytes left, so this reservation is too.
        let mut frames = Vec::with_capacity(count);
        for number in 0..count {
            let frame =
                read_frame(&mut reader).map_err(|e| SpriteError(format!("frame {number}: {e}")))?;
            frames.push(frame);
        }
        let extra = if has_extra {
            let frame =
                read_frame(&mut reader).map_err(|e| SpriteError(format!("extra frame: {e}")))?;
            Some(frame)
        } else {
            None
        };
        Ok(Sprite {
            delay_ms,
            palette,
            frames,
            extra,
        })
    }

    /// Every frame, the extra one last.
    pub fn all_frames(&self) -> impl Iterator<Item = &Frame> {
        self.frames.iter().chain(&self.extra)
    }

    /// Fails if some frame uses a palette index that `palette` lacks.
    pub(crate) fn check_palette(&self, palette: &Palette) -> Result<(), SpriteError> {
        self.all_frames()
            .try_for_each(|frame| frame.check_palette(palette))
    }

    /// Whether some frame draws palette indexes, that is, needs a palette.
    #[cfg(test)]
    pub(crate) fn needs_palette(&self) -> bool {
        self.all_frames().any(|frame| frame.max_index.is_some())
    }
}

fn read_frame(reader: &mut Reader) -> Result<Frame, SpriteError> {
    let size = reader.len("frame size")?;
    Frame::parse(reader.take(size, "frame data")?)
}

/// Whether an archive member is a sprite file: its last path component has no extension.
pub fn is_sprite_member(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();
    !name.is_empty() && !name.contains('.')
}

/// Reads the sprite member `member` and the palette it names. The third value is a warning
/// when the palette is defective: the palette file is missing or unreadable (the palette is then
/// empty), or it lacks a colour that the sprite draws. The original does not fail on these
/// either; see [`Frame::draw`] for what is drawn. A sprite that names no palette (an empty
/// path) gets an empty one without a warning.
///
/// Fails if the sprite member is missing or unreadable, or malformed (the palette path is
/// matched without regard to letter case, as the `Vfs` does). The messages are single lines, but
/// they may hold the paths as they are in the file.
pub fn load(vfs: &Vfs, member: &str) -> Result<(Sprite, Palette, Option<String>), SpriteError> {
    let sprite = Sprite::parse(&vfs.read(member).map_err(|e| SpriteError(e.to_string()))?)?;
    if sprite.palette.is_empty() {
        let warning = sprite.check_palette(&Palette::default()).err();
        return Ok((sprite, Palette::default(), warning.map(|e| e.to_string())));
    }
    let palette = vfs
        .read(&sprite.palette)
        .map_err(|e| e.to_string())
        .and_then(|bytes| Palette::parse(&bytes).map_err(|e| e.to_string()));
    let (palette, warning) = match palette {
        Ok(palette) => {
            let warning = sprite.check_palette(&palette).err().map(|e| e.to_string());
            (palette, warning)
        }
        Err(e) => (
            Palette::default(),
            Some(format!("palette {:?}: {e}", sprite.palette)),
        ),
    };
    Ok((sprite, palette, warning))
}

/// Writes the framebuffer as an 8 bit RGBA PNG. The same pixels always give the same bytes.
/// Fails if the framebuffer has no pixel (PNG has no empty images) or on a write error.
pub fn write_png(image: &Framebuffer, out: impl Write) -> Result<(), Box<dyn Error>> {
    let mut encoder = png::Encoder::new(out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&image.pixels)?;
    Ok(())
}
