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

/// The largest width or height of a frame: 4096. The largest in the game is 800 by 657.
pub const MAX_DIMENSION: u16 = 4096;

/// The most colours a palette keeps: a pixel indexes a palette with one byte.
pub const MAX_COLORS: usize = 256;

/// Opacity of the black pixels of a shadow frame when drawn: about one half.
pub const SHADOW_ALPHA: u8 = 128;

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
#[derive(Debug, Default, PartialEq, Eq)]
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

    /// The colour of a palette index, as R, G, B, A.
    pub fn color(&self, index: u8) -> Option<[u8; 4]> {
        self.colors.get(usize::from(index)).copied()
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

    /// Draws the frame with its top left corner at (`x`, `y`) in the framebuffer, clipped to
    /// it. The offsets of the frame are not applied: the caller subtracts them from the anchor.
    /// Palette colours with alpha below 255 are blended over what is there; the pixels of a
    /// shadow frame are black with [`SHADOW_ALPHA`].
    ///
    /// Fails, before drawing anything, if the frame uses a palette index the palette lacks.
    pub fn draw(
        &self,
        palette: &Palette,
        target: &mut Framebuffer,
        x: i32,
        y: i32,
    ) -> Result<(), SpriteError> {
        self.check_palette(palette)?;
        let (width, height) = (i64::from(target.width), i64::from(target.height));
        // `parse` validated the rows, so the `else` and `unwrap_or_default` cases never happen.
        let mut data = self.rows.as_slice();
        for row in 0..i64::from(self.height) {
            let Some((&segments, rest)) = data.split_first() else {
                break;
            };
            data = rest;
            // The pixels of this row of the framebuffer, if it is inside.
            let line_y = i64::from(y) + row;
            let mut line = (0..height).contains(&line_y).then(|| {
                let start = line_y as usize * target.width as usize * 4;
                &mut target.pixels[start..start + target.width as usize * 4]
            });
            let mut column = i64::from(x);
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
                // Only the part of the segment inside the framebuffer is drawn.
                if let Some(line) = line.as_deref_mut() {
                    for at in column.max(0)..(column + i64::from(count)).min(width) {
                        let color = match self.shadow {
                            true => [0, 0, 0, SHADOW_ALPHA],
                            false => pixels
                                .get((at - column) as usize)
                                .and_then(|&p| palette.color(p))
                                .unwrap_or_default(),
                        };
                        blend(&mut line[at as usize * 4..][..4], color);
                    }
                }
                column += i64::from(count);
            }
        }
        Ok(())
    }

    /// Fails if the frame uses a palette index that `palette` lacks.
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

/// Blends `color` over `pixel`, four bytes R, G, B, A.
fn blend(pixel: &mut [u8], color: [u8; 4]) {
    let alpha = u32::from(color[3]);
    match alpha {
        0 => {}
        255 => pixel.copy_from_slice(&color),
        _ => {
            let behind = u32::from(pixel[3]) * (255 - alpha) / 255;
            let total = alpha + behind;
            for c in 0..3 {
                let mixed = u32::from(color[c]) * alpha + u32::from(pixel[c]) * behind;
                pixel[c] = (mixed / total) as u8;
            }
            pixel[3] = total as u8;
        }
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

/// Reads the sprite member `member` and the palette it names, and checks that the palette
/// covers every index the sprite draws. A sprite that draws no palette index and names no
/// palette (an empty path) gets an empty one; if it names a palette, that must exist.
///
/// Fails if a member is missing or unreadable (the palette path is matched without regard to
/// letter case, as the `Vfs` does), if the sprite or the palette is malformed, or if the palette
/// is too short. The message is a single line, but it may hold the paths as they are in the file.
pub fn load(vfs: &Vfs, member: &str) -> Result<(Sprite, Palette), SpriteError> {
    let read = |path: &str| vfs.read(path).map_err(|e| SpriteError(e.to_string()));
    let sprite = Sprite::parse(&read(member)?)?;
    let palette = if sprite.palette.is_empty() && !sprite.needs_palette() {
        Palette::default()
    } else {
        let palette = Palette::parse(&read(&sprite.palette)?)
            .map_err(|e| SpriteError(format!("palette {:?}: {e}", sprite.palette)))?;
        sprite.check_palette(&palette)?;
        palette
    };
    Ok((sprite, palette))
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
mod tests {
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

    fn sized(data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_le_bytes().to_vec();
        out.extend(data);
        out
    }

    /// A sprite file with the given prefix flag (`None`: no prefix), frame count field and
    /// frame records (already with their size fields). The palette path is `animals/Lion/lion.pal`.
    fn sprite_file(prefix: Option<u8>, count: u32, frames: &[Vec<u8>]) -> Vec<u8> {
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
    fn shadow_frame_has_no_pixel_bytes_and_draws_translucent_black() {
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

        let mut image = blank(6, 2);
        frame.draw(&Palette::default(), &mut image, 0, 0).unwrap();
        let shadow = [0, 0, 0, SHADOW_ALPHA];
        for (x, y, expected) in [
            (0, 0, [0; 4]),
            (1, 0, shadow),
            (2, 0, shadow),
            (3, 0, [0; 4]),
            (4, 0, shadow),
            (5, 0, [0; 4]),
            (0, 1, shadow),
            (1, 1, shadow),
            (2, 1, shadow),
            (3, 1, [0; 4]),
        ] {
            assert_eq!(pixel(&image, x, y), expected, "pixel ({x}, {y})");
        }
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
        frame.draw(&palette(), &mut image, 0, 0).unwrap();
        assert_eq!(pixel(&image, 0, 0), [99; 4], "skipped");
        assert_eq!(pixel(&image, 1, 0), PALETTE[1]);
        assert_eq!(pixel(&image, 2, 0), PALETTE[3]);
        assert_eq!(pixel(&image, 3, 0), [99; 4], "the row stops short");
        assert_eq!(pixel(&image, 0, 1), PALETTE[2]);
        assert_eq!(pixel(&image, 1, 1), [99; 4]);
        assert_eq!(pixel(&image, 2, 1), PALETTE[0]);
    }

    #[test]
    fn draw_clips_at_every_edge_and_far_outside() {
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let mut image = blank(2, 2);
        // The frame's pixel (1, 1) lands on (0, 0); its row 0 is off the framebuffer.
        frame.draw(&palette(), &mut image, -1, -1).unwrap();
        assert_eq!(pixel(&image, 0, 0), [0; 4], "frame pixel (1,1) is skipped");
        assert_eq!(pixel(&image, 1, 0), PALETTE[0], "frame pixel (2,1)");
        assert_eq!(pixel(&image, 0, 1), [0; 4]);

        let mut image = blank(2, 2);
        // Row 0 of the frame lands on row 1; its pixel (2, 0) is clipped on the right.
        frame.draw(&palette(), &mut image, 0, 1).unwrap();
        assert_eq!(
            pixel(&image, 1, 1),
            PALETTE[1],
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
            frame.draw(&palette(), &mut image, x, y).unwrap();
            assert_eq!(image.pixels, [0; 16], "nothing visible at ({x}, {y})");
        }
        // A framebuffer without pixels is fine too.
        frame.draw(&palette(), &mut blank(0, 0), 0, 0).unwrap();
    }

    #[test]
    fn translucent_palette_colours_are_blended() {
        let frame = Frame::parse(&frame_data(1, 1, &[1, 0, 1, 0])).unwrap();
        // Index 0 is half transparent red here, over an opaque blue pixel.
        let half = Palette {
            colors: vec![[200, 0, 0, 128]],
        };
        let mut image = blank(1, 1);
        image.pixels.copy_from_slice(&[0, 0, 100, 255]);
        frame.draw(&half, &mut image, 0, 0).unwrap();
        assert_eq!(pixel(&image, 0, 0), [100, 0, 49, 255]);
        // Over a transparent pixel it keeps its colour and opacity.
        let mut image = blank(1, 1);
        frame.draw(&half, &mut image, 0, 0).unwrap();
        assert_eq!(pixel(&image, 0, 0), [200, 0, 0, 128]);
        // Fully transparent colours change nothing.
        let none = Palette {
            colors: vec![[1, 2, 3, 0]],
        };
        let mut image = blank(1, 1);
        frame.draw(&none, &mut image, 0, 0).unwrap();
        assert_eq!(pixel(&image, 0, 0), [0; 4]);
    }

    #[test]
    fn palette_index_past_the_palette_is_refused() {
        let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
        let small = Palette {
            colors: PALETTE[..3].to_vec(),
        };
        let mut image = blank(4, 2);
        let message = err_message(frame.draw(&small, &mut image, 0, 0));
        assert_eq!(message, "palette index 3 but the palette has 3 colours");
        assert_eq!(image.pixels, [0; 32], "nothing was drawn");
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
    struct Install(std::path::PathBuf);

    impl Install {
        fn new(name: &str, members: &[(&str, Vec<u8>)]) -> Install {
            let dir =
                std::env::temp_dir().join(format!("rustyzoo-sprite-{}-{name}", std::process::id()));
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

        let (sprite, palette) = load(&vfs, "OK/n").unwrap();
        assert_eq!((sprite.frames.len(), palette.len()), (1, 4));

        let message = err_message(load(&vfs, "short/n"));
        assert_eq!(message, "palette index 3 but the palette has 2 colours");
        assert!(err_message(load(&vfs, "missing/n")).contains("no such member: pals/none.pal"));
        assert!(err_message(load(&vfs, "nameless/n")).contains("no such member"));
        assert!(
            err_message(load(&vfs, "bad/n")).contains("palette \"bad-palette.pal\": truncated")
        );
        assert!(err_message(load(&vfs, "nope/n")).contains("no such member: nope/n"));

        // No palette index drawn and no palette named: an empty palette. A named one must exist.
        let (sprite, palette) = load(&vfs, "shadow/n").unwrap();
        assert!(sprite.frames[0].shadow && palette.is_empty());
        assert!(err_message(load(&vfs, "shadow-named/n")).contains("no such member"));
    }

    /// The facts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
        let load = |member: &str| {
            let sprite = Sprite::parse(&vfs.read(member).unwrap()).unwrap();
            let palette = Palette::parse(&vfs.read(&sprite.palette).unwrap()).unwrap();
            sprite.check_palette(&palette).unwrap();
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
        first.draw(&palette, &mut image, 0, 0).unwrap();
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
