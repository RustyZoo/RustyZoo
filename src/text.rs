//! Text as the original draws it: Liberation Sans Regular and Bold, with the metrics GDI gives
//! Arial.
//!
//! A font is asked for by a face string and a size string, as the layout files name them
//! ("Arial Bold", "11"). The size in points becomes a pixel size (ppem) at 96 dpi, the line
//! height and baseline follow from the vertical metrics of Arial, and every advance is rounded
//! to whole pixels per character, so a string's width is the sum of whole advances with no
//! kerning. Strings are Windows-1252 bytes. Text is wrapped, aligned, clipped and drawn into a
//! canvas as `DrawText` does it (`layout`, `draw`); templates, numbers, money and dates are in
//! `format`.
//!
//! The font files are embedded unmodified (see `fonts/README.md`), so the metrics and the
//! pictures are the same on every system. Fonts are cached per (face, size) and glyphs per
//! (font, character).

use std::collections::HashMap;
use std::sync::OnceLock;

const REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");

/// Units per em of Liberation Sans, and the vertical metrics it shares with Arial.
const UNITS: i64 = 2048;
const WIN_ASCENT: i64 = 1854;
const WIN_DESCENT: i64 = 434;

/// The size of an element without font keys: 8 points. It also stands for a size string that
/// holds no digits, which GDI would take as "default height".
const DEFAULT_POINTS: u32 = 8;
/// Larger sizes are cut here (200 points): a size string is untrusted.
const MAX_POINTS: u32 = 200;

/// The glyph of one character: a coverage bitmap (0 to 255, rows from top to bottom) and where
/// it lies relative to the pen: `left` pixels right of the pen, `bottom` pixels above the
/// baseline (negative below it).
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    pub left: i32,
    pub bottom: i32,
    pub coverage: Vec<u8>,
}

/// One font at one pixel size.
pub struct Font {
    face: &'static fontdue::Font,
    bold: bool,
    ppem: u32,
    advances: [u16; 256],
    glyphs: HashMap<u8, Glyph>,
}

fn face(bold: bool) -> &'static fontdue::Font {
    static FACES: [OnceLock<fontdue::Font>; 2] = [OnceLock::new(), OnceLock::new()];
    FACES[usize::from(bold)].get_or_init(|| {
        let bytes = if bold { BOLD } else { REGULAR };
        // The embedded files are fixed and valid.
        fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .expect("embedded font parses")
    })
}

/// Rounds `a / b` half up, for non-negative `a`.
fn div_round(a: i64, b: i64) -> i64 {
    (a * 2 + b) / (b * 2)
}

impl Font {
    /// The font a layout asks for: `face` is the face string and `size` the size string, whose
    /// leading digits are the size in points. A face holding " Bold" (or ",weight" with a weight
    /// of 600 or more) is Liberation Sans Bold, every other face Liberation Sans Regular. A size
    /// without digits is the default 8 points; sizes above 200 points (and numbers too long for
    /// 32 bits) are cut to 200.
    pub fn request(face: &str, size: &str) -> Font {
        let (bold, ppem) = parse_request(face, size);
        Font::new(bold, ppem)
    }

    /// The font at `ppem` pixels per em (the character height).
    pub fn new(bold: bool, ppem: u32) -> Font {
        let face = face(bold);
        // At 2048 pixels per em one pixel is one font unit.
        let mut advances = [0; 256];
        for (byte, advance) in advances.iter_mut().enumerate() {
            let units = face
                .metrics(from_cp1252(byte as u8), UNITS as f32)
                .advance_width;
            let px = div_round(units.round() as i64 * i64::from(ppem), UNITS);
            *advance = px.clamp(0, i64::from(u16::MAX)) as u16;
        }
        Font {
            face,
            bold,
            ppem,
            advances,
            glyphs: HashMap::new(),
        }
    }

    pub fn is_bold(&self) -> bool {
        self.bold
    }

    /// Pixels per em, the character height.
    pub fn ppem(&self) -> u32 {
        self.ppem
    }

    /// Pixels from the top of a line to its baseline.
    pub fn ascent(&self) -> i32 {
        div_round(WIN_ASCENT * i64::from(self.ppem), UNITS) as i32
    }

    /// Pixels from the baseline to the bottom of a line.
    pub fn descent(&self) -> i32 {
        div_round(WIN_DESCENT * i64::from(self.ppem), UNITS) as i32
    }

    /// The height of a line: ascent plus descent, the distance between wrapped lines.
    pub fn line_height(&self) -> i32 {
        self.ascent() + self.descent()
    }

    /// The advance of one character in whole pixels.
    pub fn advance(&self, byte: u8) -> i32 {
        i32::from(self.advances[usize::from(byte)])
    }

    /// The width of `text` on one line: the sum of the advances.
    pub fn width(&self, text: &[u8]) -> i32 {
        text.iter().map(|&b| self.advance(b)).sum()
    }

    /// The glyph of a character, drawn at the first use.
    pub fn glyph(&mut self, byte: u8) -> &Glyph {
        let (face, ppem) = (self.face, self.ppem);
        self.glyphs.entry(byte).or_insert_with(|| {
            let (m, coverage) = face.rasterize(from_cp1252(byte), ppem as f32);
            Glyph {
                width: m.width,
                height: m.height,
                left: m.xmin,
                bottom: m.ymin,
                coverage,
            }
        })
    }
}

/// The fonts of a run, one per (face, size), made at the first request.
#[derive(Default)]
pub struct Fonts {
    fonts: HashMap<(bool, u32), Font>,
}

impl Fonts {
    /// The font for a face string and a size string (see [`Font::request`]).
    pub fn get(&mut self, face: &str, size: &str) -> &mut Font {
        let wanted = parse_request(face, size);
        self.fonts
            .entry(wanted)
            .or_insert_with(|| Font::new(wanted.0, wanted.1))
    }
}

/// Whether the request is for the bold face, and its pixel size.
fn parse_request(face: &str, size: &str) -> (bool, u32) {
    let weight = face
        .split_once(',')
        .map_or(0, |(_, w)| digits(w.trim_start()));
    let bold = face.contains(" Bold") || weight >= 600;
    let points = match digits(size) {
        0 => DEFAULT_POINTS,
        n => n.min(MAX_POINTS),
    };
    (bold, (points * 4 + 1) / 3)
}

/// The leading decimal digits of `s` as a number: 0 if there are none, `u32::MAX` if too many.
fn digits(s: &str) -> u32 {
    let end = s.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 {
        return 0;
    }
    s[..end].parse().unwrap_or(u32::MAX)
}

/// The Unicode character of a Windows-1252 byte.
pub fn from_cp1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9f => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// The Windows-1252 bytes of `text`; a character the code page lacks becomes `?`.
pub fn to_cp1252(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| (0..=255u8).find(|&b| from_cp1252(b) == c).unwrap_or(b'?'))
        .collect()
}

mod draw;
pub mod format;
mod layout;

pub use draw::{Style, draw, fill_color, text_color};
pub use layout::{Justify, Line, Rect, layout, measure, tooltip_size};

#[cfg(test)]
mod tests;
