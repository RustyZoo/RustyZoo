//! One frame of a sprite: decoding its rows and drawing them.

use super::blend::{darken, mix, screen};
use super::{Blend, MAX_DIMENSION, Palette, Reader, SHADOW_STRENGTH, SpriteError, error};
use crate::Framebuffer;
use crate::zoom::Zoom;

#[cfg(test)]
mod draws;

/// The colour drawn for a pixel value that the palette lacks (a short palette file, or a value
/// beyond the count of the header). The original reads whatever lies past the colours, which
/// is not reproducible; the engine draws opaque black, as the specification recommends.
const MISSING_COLOR: [u8; 4] = [0, 0, 0, 255];

/// The smallest possible frame: its 10 byte header and no row.
pub(super) const MIN_FRAME_SIZE: usize = 10;

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
    pub(crate) fn parse(data: &[u8]) -> Result<Frame, SpriteError> {
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
    pub(crate) fn pixel_at(&self, col: i32, row: i32) -> Option<u8> {
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

    /// Draws the frame at a zoom with its anchor (the point the frame is placed by) at
    /// `anchor` in the framebuffer, `blend` and `mirror` as in [`Frame::draw_blended`]
    /// (sprite drawing 5.2 and 5.3, map drawing 8). Here `anchor` is already at the zoom.
    ///
    /// Normal: pixel (`c`, `r`) goes to (`X - x_offset + c`, `Y - y_offset + r`). Zoomed in:
    /// every pixel becomes a 2x2 block, from (`X - 2 * x_offset + 2c`, `Y - 2 * y_offset + 2r`).
    /// Zoomed out: only the pixels at an even distance from the anchor in both directions are
    /// drawn, at half that distance. Mirrored, the columns are reflected about `X`.
    pub(crate) fn draw_zoomed(
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
    pub(crate) fn check_palette(&self, palette: &Palette) -> Result<(), SpriteError> {
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
