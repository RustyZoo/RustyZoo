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

use std::error::Error;
use std::fmt;
use std::io::Write;

use crate::Framebuffer;
use crate::vfs::Vfs;
use crate::zoom::Zoom;

/// The largest width or height of a frame: 4096. The largest in the game is 800 by 657.
pub const MAX_DIMENSION: u16 = 4096;

/// The most colours a palette keeps: a pixel indexes a palette with one byte.
pub const MAX_COLORS: usize = 256;

/// How strongly a shadow frame darkens what is behind it: three quarters of the destination
/// stay. The ground shadow of an entity is drawn at this strength.
pub const SHADOW_STRENGTH: f32 = 0.25;

/// A blended draw: `strength` is the weight of the sprite (0.25 or 0.5), `black` draws every
/// pixel of a normal frame in black instead of its palette colour (the shadow of a unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blend {
    pub strength: f32,
    pub black: bool,
}

/// The colour drawn for a pixel value that the palette lacks (a short palette file, or a value
/// beyond the count of the header). The original reads whatever lies past the colours, which
/// is not reproducible; the engine draws opaque black, as the specification recommends.
const MISSING_COLOR: [u8; 4] = [0, 0, 0, 255];

/// The smallest possible frame: its 10 byte header and no row.
const MIN_FRAME_SIZE: usize = 10;

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

/// A palette: up to [`MAX_COLORS`] colours as R, G, B, A.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Palette {
    colors: Vec<[u8; 4]>,
}

impl Palette {
    /// Decodes a palette file. A header that claims more colours than the file holds (a few
    /// palettes of the game do) is cut to the colours present; bytes after the colours the
    /// header counts (a few palettes of the game have them) are ignored. Fails if the file is
    /// shorter than its 4 byte header.
    pub fn parse(bytes: &[u8]) -> Result<Palette, SpriteError> {
        let mut reader = Reader { data: bytes };
        let count = reader.len("palette colour count")?;
        let colors = reader
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .take(count.min(MAX_COLORS))
            .copied()
            .collect();
        Ok(Palette { colors })
    }

    /// The number of colours.
    pub fn len(&self) -> usize {
        self.colors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }

    /// Whether pixel value 0 is transparent: only when entry 0 has red 255, green 0 and blue 255
    /// (the marker), whatever its fourth byte. The art never uses value 0 under the marker,
    /// apart from a few stray pixels that are plain defects; palettes that start with another
    /// colour (terrain, interface) use 0 as a colour.
    pub fn zero_is_transparent(&self) -> bool {
        matches!(self.color(0), Some([255, 0, 255, _]))
    }

    /// Overwrites the entries from `start` with the entries of `from` from `skip` on, at most
    /// `count` of them and none past the end of either palette.
    pub fn overwrite(&mut self, start: usize, from: &Palette, skip: usize, count: usize) {
        let source = from.colors.get(skip..).unwrap_or_default();
        for (slot, color) in self.colors.iter_mut().skip(start).zip(source).take(count) {
            *slot = *color;
        }
    }

    /// The palette with every colour replaced by `rgb`, the alpha bytes kept. The transparent
    /// marker (see [`Palette::zero_is_transparent`]) is kept as it is.
    pub fn solid(&self, rgb: [u8; 3]) -> Palette {
        let keep = usize::from(self.zero_is_transparent());
        let mut colors = self.colors.clone();
        for c in colors.iter_mut().skip(keep) {
            *c = [rgb[0], rgb[1], rgb[2], c[3]];
        }
        Palette { colors }
    }

    /// The colour of a palette index, as R, G, B, A.
    pub fn color(&self, index: u8) -> Option<[u8; 4]> {
        self.colors.get(usize::from(index)).copied()
    }

    /// The palette a tinted entity is drawn with (sprite drawing 7.4, 9.1): every colour becomes
    /// a value that drawing reduces to the per-channel average, rounded down, of its own code and
    /// the code of the tint (`R >> 3`, `G >> 2`, `B >> 3`). The marker of entry 0
    /// stays, so the pixels that were transparent still are.
    pub fn tinted(&self, tint: [u8; 3]) -> Palette {
        let marker = self.zero_is_transparent();
        let colors = self
            .colors
            .iter()
            .enumerate()
            .map(|(at, &[r, g, b, a])| {
                if marker && at == 0 {
                    return [r, g, b, a];
                }
                let mut out = [r, g, b];
                for (c, value) in out.iter_mut().enumerate() {
                    let max = 255 >> SHIFT[c];
                    let average = (code(c, *value) + u16::from(tint[c] >> SHIFT[c])) / 2;
                    // The smallest 8 bit value with that code: drawing reduces it to the code.
                    *value = (average * 255).div_ceil(max) as u8;
                }
                [out[0], out[1], out[2], a]
            })
            .collect();
        Palette { colors }
    }
}

/// One decoded frame: its geometry and its validated, still run-length coded rows.
#[derive(Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    /// Rows between the top of the frame and the anchor of the sprite (the point that is
    /// placed on the map): the frame is drawn at the anchor minus the offsets. This is what
    /// the data shows, not documented anywhere.
    pub y_offset: i16,
    /// Columns between the left of the frame and the anchor of the sprite, as `y_offset`.
    pub x_offset: i16,
    /// The u16 after the offsets. Its meaning is not known.
    pub unknown: u16,
    /// A shadow (or mask) frame: it has no palette indexes, only the shape of the shadow.
    pub shadow: bool,
    /// The largest palette index of the frame; `None` if it draws none (shadow frames, empty ones).
    pub max_index: Option<u8>,
    rows: Vec<u8>,
}

impl Frame {
    /// Decodes the data of one frame (without its size field). Bytes after the last row are
    /// ignored, as the game does.
    ///
    /// Fails if the data is truncated, if width or height exceeds [`MAX_DIMENSION`], or if a
    /// row draws past the width. A frame with width and height 0 is valid and draws nothing.
    pub fn parse(data: &[u8]) -> Result<Frame, SpriteError> {
        let mut reader = Reader { data };
        let raw_height = reader.u16("frame height")?;
        let width = reader.u16("frame width")?;
        let y_offset = reader.u16("frame y offset")? as i16;
        let x_offset = reader.u16("frame x offset")? as i16;
        let unknown = reader.u16("frame unknown field")?;
        let shadow = raw_height & 0x8000 != 0;
        let height = raw_height & 0x7fff;
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return error(format!(
                "frame size {width}x{height} exceeds {MAX_DIMENSION}"
            ));
        }

        let rows_start = reader.data;
        let mut max_index = None;
        for row in 0..height {
            let segments = reader.take(1, "row")?[0];
            let mut x = 0usize;
            for _ in 0..segments {
                let [skip, count] = reader.take(2, "segment")?.try_into().unwrap();
                x += usize::from(skip) + usize::from(count);
                if x > usize::from(width) {
                    return error(format!("row {row} draws past the frame width {width}"));
                }
                if !shadow {
                    let indexes = reader.take(usize::from(count), "segment pixels")?;
                    max_index = max_index.max(indexes.iter().max().copied());
                }
            }
        }
        let rows_len = rows_start.len() - reader.data.len();
        Ok(Frame {
            width,
            height,
            y_offset,
            x_offset,
            unknown,
            shadow,
            max_index,
            rows: rows_start[..rows_len].to_vec(),
        })
    }

    /// The palette index stored at column `col` of row `row` of the frame, or `None` where the
    /// frame is transparent, outside it or a shadow frame (which has no indexes).
    pub fn pixel_at(&self, col: i32, row: i32) -> Option<u8> {
        let inside =
            (0..i32::from(self.width)).contains(&col) && (0..i32::from(self.height)).contains(&row);
        if self.shadow || !inside {
            return None;
        }
        // The rows were checked by `parse`: each has its segments and their values.
        let mut data = &self.rows[..];
        for r in 0..=row {
            let segments = data[0];
            data = &data[1..];
            let mut x = 0;
            for _ in 0..segments {
                let (skip, count) = (i32::from(data[0]), usize::from(data[1]));
                let (values, rest) = data[2..].split_at(count);
                data = rest;
                x += skip;
                if r == row && (x..x + count as i32).contains(&col) {
                    return Some(values[(col - x) as usize]);
                }
                x += count as i32;
            }
        }
        None
    }

    /// Draws the frame with its top left corner at (`x`, `y`) in the framebuffer, clipped to
    /// it. The offsets of the frame are not applied: the caller subtracts them from the anchor.
    ///
    /// Every stored pixel is written opaque with the colour of its palette entry as the 16 bit
    /// screen shows it ([`screen`]), replacing what was there; the fourth byte of a palette entry
    /// is ignored. The one exception is pixel value 0 when [`Palette::zero_is_transparent`]: it
    /// is skipped. The pixels of a shadow frame darken the destination at [`SHADOW_STRENGTH`] and never use the palette.
    ///
    /// Never fails. A value the palette lacks is drawn black; an empty palette (its file is
    /// missing) draws no ordinary frame at all.
    pub fn draw(&self, palette: &Palette, target: &mut Framebuffer, x: i32, y: i32) {
        self.draw_at(
            palette,
            target,
            self.anchor(x, y, false),
            false,
            None,
            Zoom::Normal,
        );
    }

    /// Like [`Frame::draw`], reflected left to right: pixel (`c`, `r`) of the frame goes to
    /// (`x - c`, `y + r`), so the frame covers the columns `x - (width - 1)` to `x`.
    pub fn draw_mirrored(&self, palette: &Palette, target: &mut Framebuffer, x: i32, y: i32) {
        self.draw_at(
            palette,
            target,
            self.anchor(x, y, true),
            true,
            None,
            Zoom::Normal,
        );
    }

    /// Like [`Frame::draw`] or [`Frame::draw_mirrored`], but blended with what is behind. Each
    /// channel code of a normal frame becomes `floor((dst + src) / 2)` at strength 0.5 and
    /// `floor((floor((dst + src) / 2) + dst) / 2)` at 0.25 (sprite drawing 7.5); a shadow frame
    /// darkens as in [`Frame::draw`] but at the strength given.
    pub fn draw_blended(
        &self,
        palette: &Palette,
        target: &mut Framebuffer,
        (x, y): (i32, i32),
        mirror: bool,
        blend: Blend,
    ) {
        let anchor = self.anchor(x, y, mirror);
        self.draw_at(palette, target, anchor, mirror, Some(blend), Zoom::Normal);
    }

    /// Draws the frame at a zoom with its anchor (the point the frame is placed by) at
    /// `anchor` in the framebuffer, `blend` and `mirror` as in [`Frame::draw_blended`]
    /// (sprite drawing 5.2 and 5.3, map drawing 8). Here `anchor` is already at the zoom.
    ///
    /// Normal: pixel (`c`, `r`) goes to (`X - x_offset + c`, `Y - y_offset + r`). Zoomed in:
    /// every pixel becomes a 2x2 block, from (`X - 2 * x_offset + 2c`, `Y - 2 * y_offset + 2r`).
    /// Zoomed out: only the pixels at an even distance from the anchor in both directions are
    /// drawn, at half that distance. Mirrored, the columns are reflected about `X`.
    pub fn draw_zoomed(
        &self,
        palette: &Palette,
        target: &mut Framebuffer,
        anchor: (i32, i32),
        mirror: bool,
        blend: Option<Blend>,
        zoom: Zoom,
    ) {
        let anchor = (i64::from(anchor.0), i64::from(anchor.1));
        self.draw_at(palette, target, anchor, mirror, blend, zoom);
    }

    /// The anchor of a frame whose pixel (0, 0) is at (`x`, `y`), or, mirrored, whose pixel
    /// (0, 0) is at (`x`, `y`) and which extends to the left.
    fn anchor(&self, x: i32, y: i32, mirror: bool) -> (i64, i64) {
        let x_offset = i64::from(self.x_offset);
        let x = i64::from(x) + if mirror { -x_offset } else { x_offset };
        (x, i64::from(y) + i64::from(self.y_offset))
    }

    fn draw_at(
        &self,
        palette: &Palette,
        target: &mut Framebuffer,
        (x, y): (i64, i64),
        mirror: bool,
        blend: Option<Blend>,
        zoom: Zoom,
    ) {
        let place = Place {
            x,
            y,
            mirror,
            zoom,
            x_offset: i64::from(self.x_offset),
            y_offset: i64::from(self.y_offset),
            height: i64::from(self.height),
        };
        let skip_zero = palette.zero_is_transparent();
        let drawn = |value: u8| !palette.is_empty() && !(value == 0 && skip_zero);
        let color = |value: u8| {
            let [r, g, b, _] = palette.color(value).unwrap_or(MISSING_COLOR);
            [r, g, b]
        };
        // What each value puts unblended, worked out once for the frame instead of per pixel.
        let shown: [Option<[u8; 4]>; 256] = std::array::from_fn(|value| {
            let value = value as u8;
            drawn(value).then(|| {
                let [r, g, b] = screen(color(value));
                [r, g, b, 255]
            })
        });
        let (width, height) = (i64::from(target.width), i64::from(target.height));
        // `parse` validated the rows, so the `else` and `unwrap_or_default` cases never happen.
        let mut data = self.rows.as_slice();
        for row in 0..i64::from(self.height) {
            let (first, count) = place.row(row);
            // A dropped row is still read, to get to the next one.
            let mut rest = data;
            for line_y in first..first + count.max(1) {
                let line = (count > 0 && (0..height).contains(&line_y)).then(|| {
                    let start = line_y as usize * target.width as usize * 4;
                    &mut target.pixels[start..start + target.width as usize * 4]
                });
                rest = self.draw_row(data, line, width, &place, |pixel, value| {
                    if self.shadow {
                        darken(pixel, blend.map_or(SHADOW_STRENGTH, |b| b.strength));
                    } else if let Some(value) = value {
                        match blend {
                            None => {
                                if let Some(shown) = &shown[usize::from(value)] {
                                    pixel.copy_from_slice(shown);
                                }
                            }
                            Some(blend) if drawn(value) => mix(pixel, color(value), blend),
                            Some(_) => {}
                        }
                    }
                });
            }
            data = rest;
        }
    }

    /// Draws one row of the frame, starting at `data`, on `line` (the pixels of one row of the
    /// framebuffer, `width` wide) with `put`, and returns the data after the row.
    fn draw_row<'a>(
        &self,
        mut data: &'a [u8],
        mut line: Option<&mut [u8]>,
        width: i64,
        place: &Place,
        put: impl Fn(&mut [u8], Option<u8>),
    ) -> &'a [u8] {
        let Some((&segments, rest)) = data.split_first() else {
            return data;
        };
        data = rest;
        // The column of the frame where the next segment starts.
        let mut column = 0i64;
        for _ in 0..segments {
            let Some((&[skip, count], rest)) = data.split_first_chunk() else {
                break;
            };
            data = rest;
            let (pixels, rest) = match self.shadow {
                true => (&[][..], data),
                false => data
                    .split_at_checked(usize::from(count))
                    .unwrap_or_default(),
            };
            data = rest;
            column += i64::from(skip);
            if let Some(line) = line.as_deref_mut() {
                for i in 0..i64::from(count) {
                    let (first, copies) = place.column(column + i);
                    for at in first..first + copies {
                        if (0..width).contains(&at) {
                            put(
                                &mut line[at as usize * 4..][..4],
                                pixels.get(i as usize).copied(),
                            );
                        }
                    }
                }
            }
            column += i64::from(count);
        }
        data
    }

    /// Fails if the frame uses a palette index that `palette` lacks. Drawing does not care (see
    /// [`Frame::draw`]); this is how a loader notices the defect.
    pub fn check_palette(&self, palette: &Palette) -> Result<(), SpriteError> {
        match self.max_index {
            Some(index) if usize::from(index) >= palette.len() => error(format!(
                "palette index {index} but the palette has {} colours",
                palette.len()
            )),
            _ => Ok(()),
        }
    }
}

/// Where the pixels of a picture land: its anchor, mirroring, zoom and geometry.
pub(crate) struct Place {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) mirror: bool,
    pub(crate) zoom: Zoom,
    pub(crate) x_offset: i64,
    pub(crate) y_offset: i64,
    pub(crate) height: i64,
}

impl Place {
    /// The first framebuffer column and the number of columns (0: not drawn) of source column `c`.
    pub(crate) fn column(&self, c: i64) -> (i64, i64) {
        let d = c - self.x_offset;
        match (self.zoom, self.mirror) {
            (Zoom::Normal, false) => (self.x + d, 1),
            (Zoom::Normal, true) => (self.x - d, 1),
            (Zoom::In, false) => (self.x + 2 * d, 2),
            (Zoom::In, true) => (self.x - 2 * d - 1, 2),
            (Zoom::Out, false) => (self.x + d / 2, i64::from(d % 2 == 0)),
            (Zoom::Out, true) => (self.x - d / 2, i64::from(d % 2 == 0)),
        }
    }

    /// The first framebuffer row and the number of rows (0: not drawn) of source row `r`.
    pub(crate) fn row(&self, r: i64) -> (i64, i64) {
        let d = r - self.y_offset;
        match self.zoom {
            Zoom::Normal => (self.y + d, 1),
            Zoom::In => (self.y + 2 * d, 2),
            Zoom::Out => {
                // The rows used start at `p`, the parity of the offset, and take every second one;
                // a last row without a partner is dropped. The first one goes to `y - offset / 2`
                // rounded toward zero, which for a negative odd offset is one row higher than
                // the half distance to the anchor.
                let p = self.y_offset.rem_euclid(2);
                let k = (r - p) / 2;
                let kept = r >= p && (r - p) % 2 == 0 && k < (self.height - p) / 2;
                (self.y - self.y_offset / 2 + k, i64::from(kept))
            }
        }
    }
}

/// The low bits each channel drops on the original's 5-6-5 surface: red 8 bits to 5, green 8 to
/// 6, blue 8 to 5. The codes run to `255 >> SHIFT`: 31, 63 and 31.
pub(crate) const SHIFT: [u32; 3] = [3, 2, 3];

/// The 5-6-5 code of the 8 bit channel `c` (0 red, 1 green, 2 blue): `value * max / 255`, rounded
/// down (sprite drawing 7.1). Not `value >> shift`: 254 gives 30.
pub(crate) const fn code(c: usize, value: u8) -> u16 {
    value as u16 * (255 >> SHIFT[c]) / 255
}

/// What the display shows for a code: its high bits repeated (map drawing 7.2).
pub(crate) const fn show(c: usize, code: u16) -> u8 {
    let s = SHIFT[c];
    (code << s | code >> (8 - 2 * s)) as u8
}

/// A colour as the 16 bit screen shows it: reduced to codes, then shown (map drawing 7.2).
/// Colours that were reduced before (palettes) come out one code lower; that is the original's.
pub fn screen(rgb: [u8; 3]) -> [u8; 3] {
    [0, 1, 2].map(|c| SCREENED[c][usize::from(rgb[c])])
}

/// What the screen shows for each value of each channel.
const SCREENED: [[u8; 256]; 3] = {
    let mut table = [[0; 256]; 3];
    let mut c = 0;
    while c < 3 {
        let mut value = 0;
        while value < 256 {
            table[c][value] = show(c, code(c, value as u8));
            value += 1;
        }
        c += 1;
    }
    table
};

/// The code of a channel of a frame pixel, which always holds a shown value.
fn shown_code(c: usize, value: u8) -> u16 {
    u16::from(value >> SHIFT[c])
}

/// Blends the colour `src` into `pixel` (R, G, B, then alpha, which stays) on the codes of the
/// frame: the colour is reduced, blended with the code behind it and shown again.
fn mix(pixel: &mut [u8], src: [u8; 3], blend: Blend) {
    for (c, channel) in pixel[..3].iter_mut().enumerate() {
        let dst = shown_code(c, *channel);
        let src = code(c, src[c]) * u16::from(!blend.black);
        let half = (dst + src) / 2;
        let out = if blend.strength == 0.25 {
            (half + dst) / 2
        } else {
            half
        };
        *channel = show(c, out);
    }
}

/// Darkens the codes of `pixel` (R, G, B, then alpha, which stays) by `strength`, the share of
/// the destination that is covered. At 0.25 three quarters stay (`floor(3c / 4)`), at 0.5 one
/// half (`floor(c / 2)`), as the original computes them; other strengths keep `1 - strength`.
fn darken(pixel: &mut [u8], strength: f32) {
    for (c, channel) in pixel[..3].iter_mut().enumerate() {
        let dst = shown_code(c, *channel);
        let out = if strength == 0.25 {
            dst * 3 / 4
        } else if strength == 0.5 {
            dst / 2
        } else {
            (f32::from(dst) * (1.0 - strength)) as u16
        };
        *channel = show(c, out);
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
    pub fn parse(bytes: &[u8]) -> Result<Sprite, SpriteError> {
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
        let palette = String::from_utf8_lossy(path).into_owned();
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
    pub fn check_palette(&self, palette: &Palette) -> Result<(), SpriteError> {
        self.all_frames()
            .try_for_each(|frame| frame.check_palette(palette))
    }

    /// Whether some frame draws palette indexes, that is, needs a palette.
    pub fn needs_palette(&self) -> bool {
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::vfs::Vfs;

    /// Frame data: height (raw, may carry the shadow bit), width, offsets y = -3 and x = 5,
    /// unknown 7, then the given row bytes.
    fn frame_data(raw_height: u16, width: u16, rows: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        for field in [raw_height, width, -3i16 as u16, 5, 7] {
            data.extend(field.to_le_bytes());
        }
        data.extend(rows);
        data
    }

    pub(crate) fn sized(data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_le_bytes().to_vec();
        out.extend(data);
        out
    }

    /// A sprite file with the given prefix flag (`None`: no prefix), frame count field and
    /// frame records (already with their size fields). The palette path is `animals/Lion/lion.pal`.
    pub(crate) fn sprite_file(prefix: Option<u8>, count: u32, frames: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(flag) = prefix {
            out.extend(b"FATZ\0\0\0\0");
            out.push(flag);
        }
        out.extend(125u32.to_le_bytes());
        let path = b"animals/Lion/lion.pal\0";
        out.extend((path.len() as u32).to_le_bytes());
        out.extend(path);
        out.extend(count.to_le_bytes());
        out.extend(frames.concat());
        out
    }

    /// Two rows of a 4 pixel wide frame. Row 0: skip 1, draw indexes 1 and 3 (3 pixels, it stops
    /// short). Row 1: draw index 2, skip 1, draw index 0. The largest index, 3, is not the first
    /// of its segment.
    const ROWS: &[u8] = &[1, 1, 2, 1, 3, 2, 0, 1, 2, 1, 1, 0];
    const PALETTE: [[u8; 4]; 5] = [
        [10, 10, 10, 255],
        [20, 0, 0, 255],
        [0, 30, 0, 255],
        [0, 0, 40, 255],
        [0, 0, 0, 0],
    ];

    fn palette() -> Palette {
        Palette {
            colors: PALETTE.to_vec(),
        }
    }

    /// A palette colour as drawing writes it: reduced to 5-6-5 and shown.
    fn shown([r, g, b, a]: [u8; 4]) -> [u8; 4] {
        let [r, g, b] = screen([r, g, b]);
        [r, g, b, a]
    }

    fn blank(width: u32, height: u32) -> Framebuffer {
        Framebuffer::new(width, height)
    }

    fn pixel(image: &Framebuffer, x: usize, y: usize) -> [u8; 4] {
        let at = (y * image.width as usize + x) * 4;
        image.pixels[at..at + 4].try_into().unwrap()
    }

    fn err_message<T: fmt::Debug>(result: Result<T, SpriteError>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn plain_file() {
        let frames = [
            sized(&frame_data(2, 4, ROWS)),
            sized(&frame_data(0, 0, &[])),
        ];
        let mut bytes = sprite_file(None, 2, &frames);
        bytes.extend(b"junk after the last frame");
        let sprite = Sprite::parse(&bytes).unwrap();
        assert_eq!(sprite.delay_ms, 125);
        assert_eq!(sprite.palette, "animals/Lion/lion.pal");
        assert_eq!(sprite.extra, None);
        assert_eq!(sprite.frames.len(), 2);
        let frame = &sprite.frames[0];
        assert_eq!((frame.width, frame.height), (4, 2));
        assert_eq!((frame.x_offset, frame.y_offset), (5, -3));
        assert_eq!(frame.unknown, 7);
        assert!(!frame.shadow);
        assert_eq!(frame.max_index, Some(3));
        assert!(sprite.needs_palette());
        // A frame of size 0 by 0 is valid and draws nothing.
        assert_eq!((sprite.frames[1].width, sprite.frames[1].height), (0, 0));
        assert_eq!(sprite.frames[1].max_index, None);
    }

    #[test]
    fn fatz_prefix_with_and_without_extra_frame() {
        let one = sized(&frame_data(2, 4, ROWS));
        let extra = sized(&frame_data(1, 2, &[1, 0, 2, 5, 6]));

        let with = Sprite::parse(&sprite_file(Some(1), 1, &[one.clone(), extra])).unwrap();
        assert_eq!(with.frames.len(), 1);
        assert_eq!(
            with.extra.as_ref().map(|f| (f.width, f.height)),
            Some((2, 1))
        );
        assert_eq!(with.all_frames().count(), 2);

        let without = Sprite::parse(&sprite_file(Some(0), 1, &[one])).unwrap();
        assert_eq!((without.frames.len(), without.extra), (1, None));
    }

    #[test]
    fn shadow_frame_has_no_pixel_bytes_and_darkens_the_destination() {
        // Height 2 with bit 15 set, 6 wide. Row 0: skip 1 and cover 2, skip 1 and cover 1.
        // Row 1: cover 3. Only skip and count bytes: a reader that took `count` pixel bytes
        // after each segment would lose its place.
        let rows = [2, 1, 2, 1, 1, 1, 0, 3];
        let sprite = Sprite::parse(&sprite_file(
            None,
            1,
            &[sized(&frame_data(0x8002, 6, &rows))],
        ))
        .unwrap();
        let frame = &sprite.frames[0];
        assert!(frame.shadow);
        assert_eq!((frame.width, frame.height), (6, 2));
        assert_eq!(frame.max_index, None);
        assert!(!sprite.needs_palette());

        let behind = shown([200, 100, 51, 255]);
        let mut image = blank(6, 2);
        for px in image.pixels.chunks_mut(4) {
            px.copy_from_slice(&behind);
        }
        frame.draw(&Palette::default(), &mut image, 0, 0);
        // Three quarters of the codes stay, rounded down: 24 -> 18, 24 -> 18, 6 -> 4. The alpha stays.
        let shadow = [show(0, 18), show(1, 18), show(2, 4), 255];
        for (x, y, expected) in [
            (0, 0, behind),
            (1, 0, shadow),
            (2, 0, shadow),
            (3, 0, behind),
            (4, 0, shadow),
            (5, 0, behind),
            (0, 1, shadow),
            (1, 1, shadow),
            (2, 1, shadow),
            (3, 1, behind),
        ] {
            assert_eq!(pixel(&image, x, y), expected, "pixel ({x}, {y})");
        }

        // The palette plays no part: a palette of one colour changes nothing.
        let mut again = blank(6, 2);
        again.pixels.copy_from_slice(&{
            let mut all = Vec::new();
            (0..12).for_each(|_| all.extend(behind));
            all
        });
        frame.draw(&palette(), &mut again, 0, 0);
        assert_eq!(again.pixels, image.pixels);

        // Over nothing there is nothing to darken.
        let mut empty = blank(6, 2);
        frame.draw(&palette(), &mut empty, 0, 0);
        assert_eq!(empty.pixels, [0; 48]);
    }

    /// A 4 by 2 picture of the grey 100 as the screen shows it: (99, 97, 99), the codes 12, 24, 12.
    fn grey() -> Framebuffer {
        let mut image = blank(4, 2);
        for p in image.pixels.as_chunks_mut::<4>().0 {
            *p = [99, 97, 99, 255];
        }
        image
    }

    #[test]
    fn blending_works_on_5_6_5_values() {
        // Red 100 is the code 12, green 100 the code 24; the source red 20 has the code 2.
        let mix = |src: [u8; 3], strength, black| {
            let mut pixel = [99, 97, 99, 77];
            super::mix(&mut pixel, src, Blend { strength, black });
            pixel
        };
        // Half: (12 + 2) / 2 = 7, shown with its high bits repeated, 57.
        assert_eq!(mix([20, 0, 0], 0.5, false), [57, 48, 49, 77]);
        // A quarter of the sprite: ((12 + 2) / 2 + 12) / 2 = 9.
        assert_eq!(mix([20, 0, 0], 0.25, false), [74, 73, 74, 77]);
        assert_eq!(mix([20, 0, 0], 0.5, true), [49, 48, 49, 77]);
        // The code of a colour mixed with itself is kept.
        for v in 0..=255u8 {
            let [r, g, b] = screen([v, v, v]);
            let mut pixel = [r, g, b, 0];
            super::mix(
                &mut pixel,
                [v, v, v],
                Blend {
                    strength: 0.5,
                    black: false,
                },
            );
            assert_eq!(pixel[..3], [r, g, b], "value {v}");
        }
        // Through a frame: the pixel with index 2 is green 30, the code 7: (24 + 7) / 2 = 15.
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let mut image = grey();
        let half = Blend {
            strength: 0.5,
            black: false,
        };
        frame.draw_blended(&palette(), &mut image, (0, 0), false, half);
        assert_eq!(pixel(&image, 0, 1), [49, 60, 49, 255]);
        assert_eq!(pixel(&image, 0, 0), [99, 97, 99, 255], "skipped");
        // A shadow frame (two pixels in the first row) halves the code.
        let shadow = Frame::parse(&frame_data(0x8002, 4, &[1, 0, 2, 0, 0])).unwrap();
        let mut image = grey();
        shadow.draw_blended(&palette(), &mut image, (0, 0), false, half);
        assert_eq!(pixel(&image, 1, 0), [49, 48, 49, 255]);
        assert_eq!(pixel(&image, 2, 0), [99, 97, 99, 255]);
    }

    #[test]
    fn the_screen_reduces_to_codes_and_repeats_the_high_bits() {
        // Colours that were reduced before come out one code lower (map drawing 7.2).
        for (stored, shown) in [
            ([106, 76, 65], [99, 73, 57]),
            ([156, 113, 90], [148, 109, 82]),
            ([82, 56, 41], [74, 52, 33]),
            ([210, 168, 113], [206, 166, 107]),
        ] {
            assert_eq!(screen(stored), shown);
        }
        assert_eq!(screen([0, 0, 0]), [0, 0, 0]);
        assert_eq!(screen([255, 255, 255]), [255, 255, 255]);
        assert_eq!(
            screen([254, 254, 254]),
            [247, 251, 247],
            "254 gives the code 30, not 31"
        );
        for (c, shift) in SHIFT.into_iter().enumerate() {
            assert_eq!(u16::from(show(c, code(c, 255)) >> shift), code(c, 255));
        }
    }

    #[test]
    fn darkening_strengths() {
        // A pixel of the codes (c, c, c), as the screen shows it: red and blue stop at 31.
        let at = |strength, code: u16| {
            let mut pixel = [show(0, code), show(1, code), show(2, code), 77];
            darken(&mut pixel, strength);
            pixel
        };
        for c in 0..=31u16 {
            let (q, h) = (at(0.25, c), at(0.5, c));
            assert_eq!(
                q,
                [
                    show(0, c * 3 / 4),
                    show(1, c * 3 / 4),
                    show(2, c * 3 / 4),
                    77
                ]
            );
            assert_eq!(h[0], show(0, c / 2), "half of {c}");
        }
        assert_eq!(at(0.25, 31)[0], show(0, 23), "floor(93 / 4), alpha kept");
        assert_eq!(at(0.25, 3)[0], show(0, 2), "floor(9 / 4)");
        assert_eq!(at(0.25, 0), [0, 0, 0, 77]);
        assert_eq!(at(0.5, 1)[0], 0);
        assert_eq!(at(0.75, 24)[0], show(0, 6));
        assert_eq!(at(0.0, 24)[0], show(0, 24));
        assert_eq!(at(1.0, 24), [0, 0, 0, 77]);
    }

    #[test]
    fn a_file_without_frames_is_accepted() {
        let sprite = Sprite::parse(&sprite_file(None, 0, &[])).unwrap();
        assert_eq!((sprite.frames.len(), sprite.extra.as_ref()), (0, None));
        assert_eq!(sprite.all_frames().count(), 0);
        assert!(!sprite.needs_palette());
        assert!(sprite.check_palette(&Palette::default()).is_ok());
    }

    #[test]
    fn bytes_after_the_last_row_of_a_frame_are_ignored() {
        let mut padded = frame_data(2, 4, ROWS);
        padded.extend([9, 9, 9, 9]);
        let sprite = Sprite::parse(&sprite_file(
            None,
            2,
            &[sized(&padded), sized(&frame_data(2, 4, ROWS))],
        ))
        .unwrap();
        assert_eq!(sprite.frames[0], sprite.frames[1]);
        assert_eq!(
            sprite.frames[0],
            Frame::parse(&frame_data(2, 4, ROWS)).unwrap()
        );
    }

    #[test]
    fn draw_decodes_rows_and_leaves_skipped_pixels_alone() {
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let mut image = blank(4, 2);
        image.pixels.fill(99);
        frame.draw(&palette(), &mut image, 0, 0);
        assert_eq!(pixel(&image, 0, 0), [99; 4], "skipped");
        assert_eq!(pixel(&image, 1, 0), shown(PALETTE[1]));
        assert_eq!(pixel(&image, 2, 0), shown(PALETTE[3]));
        assert_eq!(pixel(&image, 3, 0), [99; 4], "the row stops short");
        assert_eq!(pixel(&image, 0, 1), shown(PALETTE[2]));
        assert_eq!(pixel(&image, 1, 1), [99; 4]);
        assert_eq!(pixel(&image, 2, 1), shown(PALETTE[0]));
    }

    #[test]
    fn draw_clips_at_every_edge_and_far_outside() {
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let mut image = blank(2, 2);
        // The frame's pixel (1, 1) lands on (0, 0); its row 0 is off the framebuffer.
        frame.draw(&palette(), &mut image, -1, -1);
        assert_eq!(pixel(&image, 0, 0), [0; 4], "frame pixel (1,1) is skipped");
        assert_eq!(pixel(&image, 1, 0), shown(PALETTE[0]), "frame pixel (2,1)");
        assert_eq!(pixel(&image, 0, 1), [0; 4]);

        let mut image = blank(2, 2);
        // Row 0 of the frame lands on row 1; its pixel (2, 0) is clipped on the right.
        frame.draw(&palette(), &mut image, 0, 1);
        assert_eq!(
            pixel(&image, 1, 1),
            shown(PALETTE[1]),
            "row 0 of the frame on row 1"
        );
        assert_eq!(&image.pixels[..8], [0; 8], "row 0 untouched");

        for (x, y) in [
            (i32::MIN, 0),
            (i32::MAX, 0),
            (0, i32::MIN),
            (0, i32::MAX),
            (2, 2),
            (-4, -2),
        ] {
            let mut image = blank(2, 2);
            frame.draw(&palette(), &mut image, x, y);
            assert_eq!(image.pixels, [0; 16], "nothing visible at ({x}, {y})");
        }
        // A framebuffer without pixels is fine too.
        frame.draw(&palette(), &mut blank(0, 0), 0, 0);
    }

    #[test]
    fn palette_alpha_is_ignored() {
        let frame = Frame::parse(&frame_data(1, 1, &[1, 0, 1, 1])).unwrap();
        // Half transparent and fully transparent colours are written opaque, replacing the
        // destination.
        for alpha in [128, 0] {
            let palette = Palette {
                colors: vec![[1, 2, 3, 255], [200, 10, 20, alpha]],
            };
            let mut image = blank(1, 1);
            image.pixels.copy_from_slice(&[0, 0, 100, 255]);
            frame.draw(&palette, &mut image, 0, 0);
            assert_eq!(
                pixel(&image, 0, 0),
                shown([200, 10, 20, 255]),
                "alpha {alpha}"
            );
            let mut image = blank(1, 1);
            frame.draw(&palette, &mut image, 0, 0);
            assert_eq!(
                pixel(&image, 0, 0),
                shown([200, 10, 20, 255]),
                "alpha {alpha}"
            );
        }
    }

    #[test]
    fn value_zero_is_transparent_only_under_the_marker() {
        // One row, three pixels: values 0, 1, 0.
        let frame = Frame::parse(&frame_data(1, 3, &[1, 0, 3, 0, 1, 0])).unwrap();
        let with_entry_0 = |entry: [u8; 4]| Palette {
            colors: vec![entry, [9, 8, 7, 0]],
        };
        let draw = |palette: &Palette| {
            let mut image = blank(3, 1);
            image.pixels.fill(99);
            frame.draw(palette, &mut image, 0, 0);
            image
        };

        let marker = with_entry_0([255, 0, 255, 0]);
        assert!(marker.zero_is_transparent());
        let image = draw(&marker);
        assert_eq!(pixel(&image, 0, 0), [99; 4]);
        assert_eq!(pixel(&image, 1, 0), shown([9, 8, 7, 255]));
        assert_eq!(pixel(&image, 2, 0), [99; 4]);

        // The fourth byte of entry 0 plays no part: opaque magenta is the marker too.
        for alpha in [1, 128, 255] {
            let palette = with_entry_0([255, 0, 255, alpha]);
            assert!(palette.zero_is_transparent(), "alpha {alpha}");
            assert_eq!(draw(&palette).pixels, image.pixels, "alpha {alpha}");
        }

        // Any other entry 0 is a colour, whatever its alpha: transparent black, and magenta
        // with another red, green or blue.
        for entry in [
            [0, 0, 0, 0],
            [255, 0, 254, 0],
            [254, 0, 255, 255],
            [255, 1, 255, 0],
        ] {
            let palette = with_entry_0(entry);
            assert!(!palette.zero_is_transparent());
            let image = draw(&palette);
            let opaque = shown([entry[0], entry[1], entry[2], 255]);
            assert_eq!(pixel(&image, 0, 0), opaque, "entry {entry:?}");
            assert_eq!(pixel(&image, 2, 0), opaque);
        }
        assert!(!Palette::default().zero_is_transparent());
    }

    #[test]
    fn mirrored_drawing_reflects_the_columns_about_x() {
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let mut image = blank(6, 2);
        // Pixel (c, r) of the frame goes to (4 - c, r): row 0 has values 1 and 3 at c = 1, 2.
        frame.draw_mirrored(&palette(), &mut image, 4, 0);
        assert_eq!(pixel(&image, 3, 0), shown(PALETTE[1]));
        assert_eq!(pixel(&image, 2, 0), shown(PALETTE[3]));
        assert_eq!(pixel(&image, 4, 0), [0; 4], "c = 0 is skipped");
        // Row 1: value 2 at c = 0, value 0 at c = 2.
        assert_eq!(pixel(&image, 4, 1), shown(PALETTE[2]));
        assert_eq!(pixel(&image, 2, 1), shown(PALETTE[0]));
        assert_eq!(pixel(&image, 5, 0), [0; 4]);

        // Clipped on the left: only c = 0 of row 1 lands on column 0.
        let mut image = blank(2, 2);
        frame.draw_mirrored(&palette(), &mut image, 0, 0);
        assert_eq!(pixel(&image, 0, 1), shown(PALETTE[2]));
        assert_eq!(
            image.pixels.iter().filter(|&&b| b != 0).count(),
            2,
            "one pixel"
        );
        // Clipped on the right: c = 1 of row 0 lands on column 5, c = 2 on column 6.
        let mut image = blank(6, 2);
        frame.draw_mirrored(&palette(), &mut image, 6, 0);
        assert_eq!(pixel(&image, 5, 0), shown(PALETTE[1]));
        assert_eq!(pixel(&image, 5, 1), [0; 4]);
        // Far outside nothing is drawn.
        for x in [i32::MIN, -1, 6 + 4, i32::MAX] {
            let mut image = blank(6, 2);
            frame.draw_mirrored(&palette(), &mut image, x, 0);
            assert_eq!(image.pixels, [0; 48], "x {x}");
        }
    }

    #[test]
    fn a_value_the_palette_lacks_is_black_and_a_missing_palette_draws_nothing() {
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let small = Palette {
            colors: PALETTE[..3].to_vec(),
        };
        // Value 3 is beyond the palette: opaque black, never an error. The others are as usual.
        let mut image = blank(4, 2);
        frame.draw(&small, &mut image, 0, 0);
        assert_eq!(pixel(&image, 1, 0), shown(PALETTE[1]));
        assert_eq!(pixel(&image, 2, 0), [0, 0, 0, 255]);
        assert_eq!(pixel(&image, 0, 1), shown(PALETTE[2]));
        // No palette at all: nothing is drawn.
        let mut image = blank(4, 2);
        frame.draw(&Palette::default(), &mut image, 0, 0);
        frame.draw_mirrored(&Palette::default(), &mut image, 3, 0);
        assert_eq!(image.pixels, [0; 32]);

        // The loader can still tell: the check names the first value that is missing.
        let message = err_message(frame.check_palette(&small));
        assert_eq!(message, "palette index 3 but the palette has 3 colours");
        assert!(frame.check_palette(&palette()).is_ok());
        let sprite =
            Sprite::parse(&sprite_file(None, 1, &[sized(&frame_data(2, 4, ROWS))])).unwrap();
        assert!(sprite.check_palette(&small).is_err());
        assert!(sprite.check_palette(&Palette::default()).is_err());
        assert!(sprite.check_palette(&palette()).is_ok());
    }

    #[test]
    fn malformed_sprites_are_errors_with_a_message() {
        let good = sized(&frame_data(2, 4, ROWS));
        let file = sprite_file(Some(1), 1, &[good.clone(), good.clone()]);
        // Every truncation of a valid file is an error, never a panic.
        for len in 0..file.len() {
            let message = err_message(Sprite::parse(&file[..len]));
            assert!(!message.is_empty() && !message.contains('\n'), "{message}");
        }
        assert!(Sprite::parse(&file).is_ok());

        let cases: Vec<(&str, Vec<u8>, &str)> = vec![
            ("empty", vec![], "truncated: frame delay"),
            (
                "short prefix",
                b"FATZ\0\0".to_vec(),
                "truncated: FATZ prefix",
            ),
            (
                "frame count beyond the data",
                sprite_file(None, 2, std::slice::from_ref(&good)),
                "frame count 2 exceeds the data",
            ),
            (
                "huge frame count",
                sprite_file(None, u32::MAX, std::slice::from_ref(&good)),
                "frame count 4294967295 exceeds the data",
            ),
            (
                "missing extra frame",
                sprite_file(Some(1), 1, std::slice::from_ref(&good)),
                "extra frame: truncated: frame size",
            ),
            (
                "frame size beyond the data",
                sprite_file(None, 1, &[[&u32::MAX.to_le_bytes()[..], &[0; 10]].concat()]),
                "frame 0: truncated: frame data needs 4294967295 bytes",
            ),
            (
                "frame without header",
                sprite_file(None, 2, &[good.clone(), sized(&[1, 2, 3])]),
                "frame 1: truncated: frame width",
            ),
            (
                "row missing",
                sprite_file(None, 1, &[sized(&frame_data(2, 4, &[0]))]),
                "frame 0: truncated: row",
            ),
            (
                "segment cut",
                sprite_file(None, 1, &[sized(&frame_data(1, 4, &[1, 0]))]),
                "frame 0: truncated: segment",
            ),
            (
                "pixels cut",
                sprite_file(None, 1, &[sized(&frame_data(1, 4, &[1, 0, 3, 9]))]),
                "frame 0: truncated: segment pixels",
            ),
            (
                "row wider than the frame",
                sprite_file(None, 1, &[sized(&frame_data(1, 4, &[1, 2, 3, 1, 2]))]),
                "frame 0: row 0 draws past the frame width 4",
            ),
            (
                "skip alone past the width",
                sprite_file(None, 1, &[sized(&frame_data(1, 2, &[1, 3, 0]))]),
                "frame 0: row 0 draws past the frame width 2",
            ),
            (
                "segments that sum past u8",
                sprite_file(
                    None,
                    1,
                    &[sized(&frame_data(
                        0x8001,
                        600,
                        &[3, 255, 255, 255, 255, 255, 255],
                    ))],
                ),
                "frame 0: row 0 draws past the frame width 600",
            ),
            (
                "huge width",
                sprite_file(None, 1, &[sized(&frame_data(1, 4097, &[0]))]),
                "frame 0: frame size 4097x1 exceeds 4096",
            ),
            (
                "huge height",
                sprite_file(None, 1, &[sized(&frame_data(0x7fff, 1, &[]))]),
                "frame 0: frame size 1x32767 exceeds 4096",
            ),
            (
                "huge shadow height",
                sprite_file(None, 1, &[sized(&frame_data(0xffff, 1, &[]))]),
                "frame 0: frame size 1x32767 exceeds 4096",
            ),
        ];
        for (name, bytes, expected) in cases {
            let message = err_message(Sprite::parse(&bytes));
            assert!(message.contains(expected), "{name}: {message}");
        }
    }

    #[test]
    fn a_tall_frame_without_rows_is_refused_without_allocating_for_it() {
        // 4096 rows are claimed, one byte is there: the error comes from the data, not from a
        // buffer sized after the header.
        let message = err_message(Frame::parse(&frame_data(4096, 4096, &[0])));
        assert!(message.contains("truncated: row"), "{message}");
    }

    #[test]
    fn palettes() {
        let mut bytes = 2u32.to_le_bytes().to_vec();
        bytes.extend([1, 2, 3, 255, 4, 5, 6, 0]);
        let palette = Palette::parse(&bytes).unwrap();
        assert_eq!(palette.len(), 2);
        assert_eq!(palette.color(1), Some([4, 5, 6, 0]));
        assert_eq!(palette.color(2), None);

        // More bytes than the count says: the surplus is ignored.
        bytes.extend([7, 8, 9, 255]);
        assert_eq!(Palette::parse(&bytes).unwrap().len(), 2);
        // A count above the bytes present, ending in a partial entry: cut to what is there.
        bytes[0] = 9;
        bytes.extend([1, 2]);
        assert_eq!(Palette::parse(&bytes).unwrap().len(), 3);

        // At most 256 colours are kept, whatever the header claims.
        let mut big = u32::MAX.to_le_bytes().to_vec();
        big.extend(vec![0; 300 * 4]);
        assert_eq!(Palette::parse(&big).unwrap().len(), MAX_COLORS);

        assert_eq!(Palette::parse(&0u32.to_le_bytes()).unwrap().len(), 0);
        assert!(Palette::parse(&0u32.to_le_bytes()).unwrap().is_empty());
        assert!(err_message(Palette::parse(&[1, 0, 0])).contains("truncated"));
    }

    #[test]
    fn overwrite_stays_within_both_palettes() {
        let source = Palette {
            colors: vec![[1; 4], [2; 4], [3; 4]],
        };
        let mut target = palette(); // 5 entries
        target.overwrite(3, &source, 1, usize::MAX);
        assert_eq!(target.colors[2..], [PALETTE[2], [2; 4], [3; 4]]);
        let mut target = palette();
        target.overwrite(0, &source, 0, 2);
        assert_eq!(target.colors[..3], [[1; 4], [2; 4], PALETTE[2]]);
        // Nothing to copy, or nowhere to put it: no change, no panic.
        for (start, skip) in [(9, 0), (usize::MAX, 0), (0, 9), (0, usize::MAX)] {
            let mut target = palette();
            target.overwrite(start, &source, skip, usize::MAX);
            assert_eq!(target, palette());
        }
    }

    #[test]
    fn a_tint_averages_each_channel_on_5_6_5_values_and_keeps_the_marker() {
        let plain = Palette {
            colors: vec![
                [255, 0, 255, 0],
                [0, 0, 0, 255],
                [255, 255, 255, 255],
                [8, 100, 200, 9],
            ],
        };
        let tinted = plain.tinted([0, 225, 0]);
        assert!(tinted.zero_is_transparent());
        assert_eq!(tinted.color(0), plain.color(0));
        // The tint's codes are 0, 56 and 0. Black: green (0 + 56) / 2 = 28, shown as 28 << 2 | 1;
        // white: red and blue 15 (shown as 123), green 59 (shown as 239).
        // They are stored so that drawing reduces them to those codes.
        let drawn = |at| tinted.color(at).map(shown);
        assert_eq!(drawn(1), Some([0, 113, 0, 255]));
        assert_eq!(drawn(2), Some([123, 239, 123, 255]));
        assert_eq!(
            tinted.color(3).map(|c| c[3]),
            Some(9),
            "the fourth byte stays"
        );
        // Under a palette without the marker entry 0 is a colour like the others.
        let opaque = Palette {
            colors: vec![[255, 255, 255, 255]],
        };
        assert_ne!(opaque.tinted([0, 0, 0]).color(0), opaque.color(0));
        assert_eq!(Palette::default().tinted([1, 2, 3]), Palette::default());
    }

    #[test]
    fn png_is_rgba_and_deterministic() {
        let mut image = blank(3, 2);
        image.fill_test_pattern();
        let (mut first, mut second) = (Vec::new(), Vec::new());
        write_png(&image, &mut first).unwrap();
        write_png(&image, &mut second).unwrap();
        assert_eq!(first, second);

        let mut reader = png::Decoder::new(std::io::Cursor::new(&first))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(pixels, image.pixels);

        assert!(write_png(&blank(0, 0), Vec::new()).is_err());
    }

    #[test]
    fn sprite_members_have_no_extension() {
        assert!(is_sprite_member("animals/lion/f/walk/e"));
        assert!(is_sprite_member("n"));
        assert!(!is_sprite_member("animals/lion/f/walk/walk.ani"));
        assert!(!is_sprite_member("animals/lion.d/n.pal"));
        assert!(is_sprite_member("animals/lion.d/n"));
        assert!(!is_sprite_member("animals/lion/"));
        assert!(!is_sprite_member(""));
    }

    /// An install folder with the given members in one archive, removed on drop.
    pub(crate) struct Install(pub std::path::PathBuf);

    impl Install {
        pub(crate) fn new(name: &str, members: &[(&str, Vec<u8>)]) -> Install {
            let dir = std::env::temp_dir().join(format!(
                "rustyzoo-sprite-{}-{}-{name}",
                std::process::id(),
                {
                    static N: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                }
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("zoo.ini"), "[resource]\npath=.\n").unwrap();
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
            for (name, content) in members {
                zip.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(content).unwrap();
            }
            std::fs::write(dir.join("a.ztd"), zip.finish().unwrap().into_inner()).unwrap();
            Install(dir)
        }
    }

    impl Drop for Install {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A sprite file with one frame and the given palette path.
    fn file_naming(path: &str, rows_raw_height: u16, rows: &[u8]) -> Vec<u8> {
        let mut out = 125u32.to_le_bytes().to_vec();
        out.extend((path.len() as u32).to_le_bytes());
        out.extend(path.as_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(sized(&frame_data(rows_raw_height, 2, rows)));
        out
    }

    #[test]
    fn load_finds_the_palette_and_applies_the_empty_path_rule() {
        let mut palette = 4u32.to_le_bytes().to_vec();
        palette.extend(PALETTE[..4].iter().flatten());
        let mut short = 2u32.to_le_bytes().to_vec();
        short.extend(PALETTE[..2].iter().flatten());
        let draws_3 = [1, 0, 2, 3, 3]; // one row: draws indexes 3 and 3
        let shadow = [1, 0, 2]; // one row: covers 2 pixels
        let install = Install::new(
            "load",
            &[
                ("pals/Main.PAL", palette),
                ("pals/short.pal", short),
                ("ok/n", file_naming("PALS\\main.pal", 1, &draws_3)),
                ("short/n", file_naming("pals/short.pal", 1, &draws_3)),
                ("missing/n", file_naming("pals/none.pal", 1, &draws_3)),
                ("nameless/n", file_naming("", 1, &draws_3)),
                ("shadow/n", file_naming("", 0x8001, &shadow)),
                (
                    "shadow-named/n",
                    file_naming("pals/none.pal", 0x8001, &shadow),
                ),
                ("bad-palette.pal", vec![1]),
                ("bad/n", file_naming("bad-palette.pal", 1, &draws_3)),
            ],
        );
        let vfs = Vfs::open(&install.0).unwrap();

        let (sprite, palette, warning) = load(&vfs, "OK/n").unwrap();
        assert_eq!((sprite.frames.len(), palette.len(), warning), (1, 4, None));

        // A defective palette is a warning, not a failure; a missing or unreadable one is empty.
        let warning = |member: &str| {
            let (_, palette, warning) = load(&vfs, member).unwrap();
            (palette.len(), warning.unwrap())
        };
        assert_eq!(
            warning("short/n"),
            (
                2,
                "palette index 3 but the palette has 2 colours".to_string()
            )
        );
        let (len, message) = warning("missing/n");
        assert!(
            len == 0 && message.contains("no such member: pals/none.pal"),
            "{message}"
        );
        let (len, message) = warning("nameless/n");
        assert_eq!(
            (len, message.as_str()),
            (0, "palette index 3 but the palette has 0 colours")
        );
        let (len, message) = warning("bad/n");
        assert!(
            len == 0 && message.contains("palette \"bad-palette.pal\": truncated"),
            "{message}"
        );
        // Only a sprite file that cannot be found fails.
        assert!(err_message(load(&vfs, "nope/n")).contains("no such member: nope/n"));

        // No palette index drawn and no palette named: an empty palette, no warning.
        let (sprite, palette, warning) = load(&vfs, "shadow/n").unwrap();
        assert!(sprite.frames[0].shadow && palette.is_empty() && warning.is_none());
        // A named palette that is missing is a warning even when nothing needs it.
        let (_, _, warning) = load(&vfs, "shadow-named/n").unwrap();
        assert!(warning.unwrap().contains("no such member"));
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
        let load = |member: &str| {
            let (sprite, palette, warning) = super::load(&vfs, member).unwrap();
            assert_eq!(warning, None);
            (sprite, palette)
        };

        let (lion, palette) = load("animals/lion/f/walk/E");
        assert_eq!(lion.frames.len(), 23);
        assert_eq!(lion.extra, None);
        assert_eq!(lion.delay_ms, 83);
        assert_eq!(lion.palette, "animals/lion/lion.pal");
        let first = &lion.frames[0];
        assert_eq!((first.width, first.height), (47, 24));
        assert_eq!((first.x_offset, first.y_offset), (25, 21));
        assert!(!first.shadow);
        assert_eq!(palette.len(), 256);
        let mut image = Framebuffer::new(47, 24);
        first.draw(&palette, &mut image, 0, 0);
        assert!(image.pixels.chunks(4).any(|px| px[3] == 255));

        let (icon, _) = load("animals/lion/icflion/N");
        assert_eq!(icon.frames.len(), 1);
        assert_eq!((icon.frames[0].width, icon.frames[0].height), (44, 32));

        let (aviary, _) = load("objects/aviary/idle/NE");
        assert_eq!(aviary.frames.len(), 21);
        let extra = aviary.extra.as_ref().unwrap();
        assert_eq!((extra.width, extra.height), (407, 320));

        let (swim, _) = load("animals/asianele/m/ssurfswi/e");
        assert_eq!(swim.frames.len(), 34);
        assert!(swim.frames.iter().all(|frame| frame.shadow));
        assert_eq!((swim.frames[0].width, swim.frames[0].height), (86, 18));
    }
}

/// The zoomed variants of the blitter, on synthetic frames that show where every pixel goes.
#[cfg(test)]
mod zoom_tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Where each written pixel of a 40 by 40 framebuffer is, with its palette index.
    type Drawn = BTreeMap<(i64, i64), u8>;

    /// An opaque `width` by `height` frame whose pixel (c, r) has the palette index
    /// `1 + r * width + c` (so at most 255 pixels) and whose rows are one segment per pixel.
    fn grid(width: u16, height: u16, y_offset: i16, x_offset: i16) -> Frame {
        let mut data = Vec::new();
        for field in [height, width, y_offset as u16, x_offset as u16, 0] {
            data.extend(field.to_le_bytes());
        }
        for r in 0..height {
            data.push(width as u8);
            for c in 0..width {
                data.extend([0, 1, (1 + r * width + c) as u8]);
            }
        }
        Frame::parse(&data).unwrap()
    }

    /// The colour of index `i` has the red code `i % 32` and the green code `i / 32`.
    fn ramp() -> Palette {
        Palette {
            colors: (0..=255u16)
                .map(|i| {
                    [
                        (i % 32 * 255).div_ceil(31) as u8,
                        (i / 32 * 255).div_ceil(63) as u8,
                        0,
                        255,
                    ]
                })
                .collect(),
        }
    }

    fn drawn(image: &Framebuffer) -> Drawn {
        let mut out = Drawn::new();
        for (i, p) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
            if p[3] != 0 {
                out.insert(
                    ((i % 40) as i64, (i / 40) as i64),
                    (p[0] >> 3) + 32 * (p[1] >> 2),
                );
            }
        }
        out
    }

    fn draw(frame: &Frame, anchor: (i32, i32), mirror: bool, zoom: Zoom) -> Drawn {
        let mut image = Framebuffer::new(40, 40);
        frame.draw_zoomed(&ramp(), &mut image, anchor, mirror, None, zoom);
        drawn(&image)
    }

    /// The index of the pixel (c, r) of a `grid` of the given width.
    fn index(width: i64, c: i64, r: i64) -> u8 {
        (1 + r * width + c) as u8
    }

    #[test]
    fn normal_zoom_equals_the_plain_draws_at_the_same_anchor() {
        let frame = grid(5, 4, 3, 2);
        let palette = ramp();
        for mirror in [false, true] {
            let mut plain = Framebuffer::new(40, 40);
            let mut zoomed = Framebuffer::new(40, 40);
            // The anchor is (20, 15); the plain draws take the corner: anchor minus the offsets,
            // or, mirrored, anchor plus the x offset.
            match mirror {
                false => frame.draw(&palette, &mut plain, 18, 12),
                true => frame.draw_mirrored(&palette, &mut plain, 22, 12),
            }
            frame.draw_zoomed(&palette, &mut zoomed, (20, 15), mirror, None, Zoom::Normal);
            assert_eq!(plain.pixels, zoomed.pixels, "mirror {mirror}");
            assert_eq!(drawn(&plain).len(), 20);
        }
        // Mirrored, pixel (c, r) goes to (X + x_offset - c, Y - y_offset + r).
        let got = draw(&frame, (20, 15), true, Zoom::Normal);
        assert_eq!(got[&(22, 12)], index(5, 0, 0));
        assert_eq!(got[&(18, 15)], index(5, 4, 3));
    }

    #[test]
    fn zoomed_in_every_pixel_is_a_2x2_block() {
        // Offsets: y 3, x 2; anchor (20, 15): pixel (c, r) fills x = 20 - 4 + 2c .. +1 and
        // y = 15 - 6 + 2r .. +1.
        let got = draw(&grid(5, 4, 3, 2), (20, 15), false, Zoom::In);
        assert_eq!(got.len(), 5 * 4 * 4);
        for r in 0..4 {
            for c in 0..5 {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(
                        got[&(16 + 2 * c + dx, 9 + 2 * r + dy)],
                        index(5, c, r),
                        "pixel ({c}, {r}) block ({dx}, {dy})"
                    );
                }
            }
        }
    }

    #[test]
    fn zoomed_in_mirrored_blocks_end_on_the_reflected_column() {
        // x = X + 2 * x_offset - 2c - 1 .. X + 2 * x_offset - 2c, same rows.
        let got = draw(&grid(5, 4, 3, 2), (20, 15), true, Zoom::In);
        assert_eq!(got.len(), 80);
        for r in 0..4 {
            for c in 0..5 {
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(
                        got[&(23 - 2 * c + dx, 9 + 2 * r + dy)],
                        index(5, c, r),
                        "pixel ({c}, {r})"
                    );
                }
            }
        }
    }

    #[test]
    fn zoomed_in_works_with_negative_offsets() {
        // Offsets y -2, x -1: pixel (0, 0) is at (X + 2, Y + 4).
        let got = draw(&grid(2, 2, -2, -1), (10, 10), false, Zoom::In);
        assert_eq!(got.keys().next(), Some(&(12, 14)));
        assert_eq!(got[&(12, 14)], 1);
        assert_eq!(got[&(15, 17)], 4);
        assert_eq!(got.len(), 16);
    }

    #[test]
    fn zoomed_out_keeps_the_columns_of_the_parity_of_the_x_offset() {
        // x offset 3 (odd): columns 1 and 3 and 5 are at an even distance, at X - 1, X, X + 1.
        // y offset 4: rows 0, 2 (height 5 gives 2 rows) at Y - 2, Y - 1.
        let got = draw(&grid(7, 5, 4, 3), (20, 20), false, Zoom::Out);
        let want: Drawn = [(1, 0, 19, 18), (3, 0, 20, 18), (5, 0, 21, 18)]
            .into_iter()
            .chain([(1, 2, 19, 19), (3, 2, 20, 19), (5, 2, 21, 19)])
            .map(|(c, r, x, y)| ((x, y), index(7, c, r)))
            .collect();
        assert_eq!(got, want);
        // Even x offset: columns 0, 2, 4, 6.
        let got = draw(&grid(7, 5, 4, 2), (20, 20), false, Zoom::Out);
        let columns: Vec<i64> = got.keys().filter(|k| k.1 == 18).map(|k| k.0).collect();
        assert_eq!(columns, [19, 20, 21, 22]);
        assert_eq!(got[&(19, 18)], index(7, 0, 0));
        assert_eq!(got[&(22, 19)], index(7, 6, 2));
    }

    #[test]
    fn zoomed_out_mirrored_reflects_the_columns_about_the_anchor() {
        // x offset 3: source column c is at distance d = c - 3 and goes to X - d / 2.
        let got = draw(&grid(7, 5, 4, 3), (20, 20), true, Zoom::Out);
        assert_eq!(got.len(), 6);
        assert_eq!(got[&(21, 18)], index(7, 1, 0));
        assert_eq!(got[&(20, 18)], index(7, 3, 0));
        assert_eq!(got[&(19, 18)], index(7, 5, 0));
    }

    /// The source rows drawn by a zoomed out `grid` of the given height and y offset, and the
    /// framebuffer row of the first one.
    fn out_rows(height: u16, y_offset: i16) -> (Vec<i64>, i64) {
        let got = draw(&grid(1, height, y_offset, 0), (20, 20), false, Zoom::Out);
        let rows = got.values().map(|&v| i64::from(v) - 1).collect();
        (rows, got.keys().next().map_or(0, |k| k.1))
    }

    #[test]
    fn zoomed_out_row_count_is_half_the_height_less_one_for_an_odd_offset_and_even_height() {
        // (height, y offset, rows used)
        let table: [(u16, i16, &[i64]); 10] = [
            (5, 0, &[0, 2]),    // odd height, even offset: the last candidate row is dropped
            (6, 0, &[0, 2, 4]), // even height, even offset
            (6, 1, &[1, 3]),    // even height, odd offset: one less than height / 2
            (5, 1, &[1, 3]),    // odd height, odd offset: height / 2
            (23, 21, &[1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21]),
            (1, 0, &[]),
            (1, 1, &[]),
            (2, 1, &[]),
            (2, 0, &[0]),
            (3, 1, &[1]),
        ];
        for (height, y_offset, want) in table {
            assert_eq!(
                out_rows(height, y_offset).0,
                want,
                "{height} rows, {y_offset}"
            );
        }
    }

    #[test]
    fn zoomed_out_rows_go_to_the_anchor_minus_half_the_offset_rounded_toward_zero() {
        // Positive odd offset 3: row 1 is 2 above the anchor, at Y + (1 - 3) / 2 = Y - 1.
        assert_eq!(out_rows(6, 3).1, 19);
        // Even offsets, plain halves.
        assert_eq!(out_rows(6, 4).1, 18);
        assert_eq!(out_rows(6, -2).1, 21, "row 0 is 2 below the anchor");
        assert_eq!(out_rows(6, 0).1, 20);
        // Each following drawn row is one lower.
        let got = draw(&grid(1, 6, 4, 0), (20, 20), false, Zoom::Out);
        assert_eq!(got.keys().map(|k| k.1).collect::<Vec<_>>(), [18, 19, 20]);
    }

    #[test]
    fn zoomed_out_negative_odd_offset_draws_one_pixel_higher() {
        // Offset -3, height 6: rows 1 and 3 are used. By the distance rule row 1 (4 below the
        // anchor) would go to Y + 2; the original puts it at Y - trunc(-3 / 2) = Y + 1.
        let (rows, first) = out_rows(6, -3);
        assert_eq!(rows, [1, 3]);
        assert_eq!(first, 21);
        // Offset -1: row 1 would go to Y + 1, it goes to Y.
        assert_eq!(out_rows(6, -1).1, 20);
        // The even offset next to it has no exception: offset -2, row 0 at Y + 1.
        assert_eq!(out_rows(6, -2).1, 21);
        // Mirrored the rows are the same.
        let got = draw(&grid(1, 6, -3, 0), (20, 20), true, Zoom::Out);
        assert_eq!(got.keys().map(|k| k.1).collect::<Vec<_>>(), [21, 22]);
    }

    #[test]
    fn zoomed_out_example_of_an_odd_by_odd_frame() {
        // 47 by 24, offsets 25 across and 21 down: source columns 1, 3, ..., 45 go to X - 12 ..
        // X + 10 and source rows 1, 3, ..., 21 to Y - 10 .. Y.
        let mut image = Framebuffer::new(60, 40);
        let mut data = Vec::new();
        for field in [24u16, 47, 21, 25, 0] {
            data.extend(field.to_le_bytes());
        }
        for _ in 0..24 {
            data.extend([1, 0, 47]);
            data.extend([1; 47]);
        }
        let frame = Frame::parse(&data).unwrap();
        frame.draw_zoomed(&ramp(), &mut image, (30, 20), false, None, Zoom::Out);
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for (i, p) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
            if p[3] != 0 {
                xs.push(i % 60);
                ys.push(i / 60);
            }
        }
        assert_eq!(xs.len(), 23 * 11);
        assert_eq!((xs.iter().min(), xs.iter().max()), (Some(&18), Some(&40)));
        assert_eq!((ys.iter().min(), ys.iter().max()), (Some(&10), Some(&20)));
    }

    #[test]
    fn dropped_rows_do_not_disturb_the_rows_after_them() {
        // Row 0 has two segments (columns 0 and 2), row 1 two others (columns 2 and 4), row 2 none.
        let rows = [2, 0, 1, 7, 1, 1, 8, 2, 2, 1, 9, 1, 1, 10, 0];
        let mut data = Vec::new();
        for field in [3u16, 5, 1, 0, 0] {
            data.extend(field.to_le_bytes());
        }
        data.extend(rows);
        let frame = Frame::parse(&data).unwrap();
        // y offset 1: the only row used is row 1, at Y; columns 2 and 4 at X + 1 and X + 2.
        let got = draw(&frame, (10, 10), false, Zoom::Out);
        assert_eq!(got, Drawn::from([((11, 10), 9), ((12, 10), 10)]));
        // Zoomed in, row 0 is drawn twice and row 1 still follows it.
        let got = draw(&frame, (10, 10), false, Zoom::In);
        assert_eq!(got[&(10, 8)], 7);
        assert_eq!(got[&(14, 9)], 8);
        assert_eq!(got[&(14, 10)], 9);
        assert_eq!(got[&(19, 11)], 10);
        assert_eq!(got.len(), 4 * 4);
    }

    #[test]
    fn pixels_are_clipped_one_by_one_at_every_zoom() {
        let frame = grid(5, 4, 3, 2);
        for zoom in [Zoom::Out, Zoom::Normal, Zoom::In] {
            for mirror in [false, true] {
                let full = draw(&frame, (20, 20), mirror, zoom);
                // Slide the anchor so that parts fall off each side; the visible part is always the
                // full picture cut by the framebuffer.
                for (dx, dy) in [(-22, 0), (22, 0), (0, -22), (0, 22), (-30, -30)] {
                    let got = draw(&frame, (20 + dx, 20 + dy), mirror, zoom);
                    let want: Drawn = full
                        .iter()
                        .filter_map(|(&(x, y), &v)| {
                            let (x, y) = (x + i64::from(dx), y + i64::from(dy));
                            ((0..40).contains(&x) && (0..40).contains(&y)).then_some(((x, y), v))
                        })
                        .collect();
                    assert_eq!(got, want, "{zoom:?} mirror {mirror} shift ({dx}, {dy})");
                }
            }
        }
        for zoom in [Zoom::Out, Zoom::In] {
            for anchor in [(i32::MIN, 0), (i32::MAX, i32::MAX), (0, i32::MIN)] {
                assert!(draw(&frame, anchor, false, zoom).is_empty());
            }
        }
    }

    #[test]
    fn shadow_and_blended_frames_follow_the_same_placement() {
        // A shadow frame of 2 by 1 pixels, y offset 0 and x offset 0, drawn on grey.
        let mut data = Vec::new();
        for field in [0x8001u16, 2, 0, 0, 0] {
            data.extend(field.to_le_bytes());
        }
        data.extend([1, 0, 2]);
        let shadow = Frame::parse(&data).unwrap();
        let grey = || {
            let mut image = Framebuffer::new(8, 8);
            image.pixels.fill(99); // code 12
            image
        };
        let dark = show(0, 12 * 3 / 4);
        let mut image = grey();
        shadow.draw_zoomed(&ramp(), &mut image, (2, 2), false, None, Zoom::In);
        let darkened: Vec<(usize, usize)> = (0..64)
            .filter(|&i| image.pixels[i * 4] == dark)
            .map(|i| (i % 8, i / 8))
            .collect();
        assert_eq!(darkened.len(), 8, "2 pixels, each a 2x2 block");
        assert_eq!(darkened[0], (2, 2));
        assert_eq!(darkened[7], (5, 3));

        // Blended at 0.5, mirrored, zoomed in: red 200 over 100 on the 5-6-5 scale.
        let frame = grid(1, 1, 0, 0);
        let mut palette = ramp();
        palette.colors[1] = [200, 0, 0, 255];
        let mut image = grey();
        let blend = Blend {
            strength: 0.5,
            black: false,
        };
        frame.draw_zoomed(&palette, &mut image, (4, 2), true, Some(blend), Zoom::In);
        let mut plain = grey();
        frame.draw_blended(&palette, &mut plain, (4, 2), true, blend);
        // The 1x1 blended pixel at (4, 2) is the colour of every pixel of the 2x2 block at
        // (3..=4, 2..=3).
        for (x, y) in [(3, 2), (4, 2), (3, 3), (4, 3)] {
            assert_eq!(
                image.pixels[(y * 8 + x) * 4..][..4],
                plain.pixels[(2 * 8 + 4) * 4..][..4]
            );
        }
        assert_eq!(
            image.pixels[(2 * 8 + 5) * 4],
            99,
            "nothing beside the block"
        );
    }
}
