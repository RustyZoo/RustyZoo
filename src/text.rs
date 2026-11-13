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
//! pictures are the same on every system. Glyphs are fitted to the pixel grid and rasterised
//! by `raster`. Fonts are cached per (face, size) and glyphs per (font, character).

use skrifa::charmap::Charmap;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{HintingInstance, OutlineGlyphCollection};
use skrifa::{FontRef, GlyphId, MetadataProvider};
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
pub(crate) struct Glyph {
    pub width: usize,
    pub height: usize,
    pub left: i32,
    pub bottom: i32,
    pub coverage: Vec<u8>,
}

/// One font at one pixel size.
pub struct Font {
    face: &'static Face,
    hinter: Option<HintingInstance>,
    bold: bool,
    /// Whether the face string holds the word " Bold" (the small-font test of text 5.5).
    bold_word: bool,
    ppem: u32,
    advances: [u16; 256],
    glyphs: HashMap<u8, Glyph>,
}

/// The parsed font file of one weight.
struct Face {
    outlines: OutlineGlyphCollection<'static>,
    charmap: Charmap<'static>,
    font: FontRef<'static>,
}

impl Face {
    fn id(&self, byte: u8) -> GlyphId {
        let id = self.charmap.map(from_cp1252(byte));
        id.unwrap_or(GlyphId::NOTDEF)
    }
}

fn face(bold: bool) -> &'static Face {
    static FACES: [OnceLock<Face>; 2] = [OnceLock::new(), OnceLock::new()];
    FACES[usize::from(bold)].get_or_init(|| {
        let bytes = if bold { BOLD } else { REGULAR };
        // The embedded files are fixed and valid.
        let font = FontRef::new(bytes).expect("embedded font parses");
        Face {
            outlines: font.outline_glyphs(),
            charmap: font.charmap(),
            font,
        }
    })
}

/// Rounds `a / b` half up, for non-negative `a`.
fn div_round(a: i64, b: i64) -> i64 {
    (a * 2 + b) / (b * 2)
}

impl Font {
    /// The font a layout asks for: `face` is the face string and `size` the size string, whose
    /// leading digits are the size in points. A face holding a `BOLD_WORDS` word (or ",weight"
    /// with a weight of 600 or more) is Liberation Sans Bold, every other face Liberation Sans
    /// Regular. A size without digits is the default 8 points; sizes above 200 points (and
    /// numbers too long for 32 bits) are cut to 200.
    pub fn request(face: &str, size: &str) -> Font {
        let (bold, ppem) = parse_request(face, size);
        Font::new(bold, ppem, face.contains(" Bold"))
    }

    /// The font at `ppem` pixels per em (the character height); `bold_word` is whether the face
    /// string holds " Bold".
    pub(crate) fn new(bold: bool, ppem: u32, bold_word: bool) -> Font {
        let face = face(bold);
        // Unscaled, one pixel is one font unit.
        let metrics = face
            .font
            .glyph_metrics(Size::unscaled(), LocationRef::default());
        let mut advances = [0; 256];
        for (byte, advance) in advances.iter_mut().enumerate() {
            let units = metrics.advance_width(face.id(byte as u8)).unwrap_or(0.0);
            let px = div_round(units.round() as i64 * i64::from(ppem), UNITS);
            *advance = px.clamp(0, i64::from(u16::MAX)) as u16;
        }
        Font {
            face,
            hinter: raster::hinter(&face.outlines, ppem),
            bold,
            bold_word,
            ppem,
            advances,
            glyphs: HashMap::new(),
        }
    }

    pub(crate) fn is_bold(&self) -> bool {
        self.bold
    }

    /// Whether the face string holds " Bold", in any language's text (text 5.5).
    pub(crate) fn bold_word(&self) -> bool {
        self.bold_word
    }

    /// Pixels per em, the character height.
    pub(crate) fn ppem(&self) -> u32 {
        self.ppem
    }

    /// Arial's ascent and descent on Windows at the six shipped sizes, from its per-size table
    /// (text 6.6 points 3 and 4); `None` elsewhere.
    fn windows_rows(&self) -> Option<(i32, i32)> {
        match self.ppem {
            11 => Some((11, 3)),
            12 => Some((12, 3)),
            13 => Some((13, 3)),
            15 => Some((14, 4)),
            16 => Some((15, 4)),
            19 => Some((18, 4)),
            _ => None,
        }
    }

    /// Pixels from the top of a line to the baseline the letters sit on: Windows' value at the
    /// shipped sizes, the rounding rule of text 4.1 elsewhere.
    pub(crate) fn baseline(&self) -> i32 {
        self.windows_rows().map_or(
            div_round(WIN_ASCENT * i64::from(self.ppem), UNITS) as i32,
            |r| r.0,
        )
    }

    /// Pixels from the baseline to the bottom of a line, chosen as [`Font::baseline`] is.
    pub(crate) fn descent(&self) -> i32 {
        self.windows_rows().map_or(
            div_round(WIN_DESCENT * i64::from(self.ppem), UNITS) as i32,
            |r| r.1,
        )
    }

    /// The height of a line: baseline plus descent, the distance between wrapped lines.
    pub fn line_height(&self) -> i32 {
        self.baseline() + self.descent()
    }

    /// The advance of one character in whole pixels.
    pub(crate) fn advance(&self, byte: u8) -> i32 {
        i32::from(self.advances[usize::from(byte)])
    }

    /// The width of `text` on one line: the sum of the advances.
    pub fn width(&self, text: &[u8]) -> i32 {
        text.iter().map(|&b| self.advance(b)).sum()
    }

    /// The glyph of a character, drawn at the first use.
    pub(crate) fn glyph(&mut self, byte: u8) -> &Glyph {
        let (face, hinter) = (self.face, self.hinter.as_ref());
        let (bold, ppem) = (self.bold, self.ppem);
        let advance = i32::from(self.advances[usize::from(byte)]);
        self.glyphs.entry(byte).or_insert_with(|| {
            raster::glyph(&face.outlines, hinter, face.id(byte), (bold, ppem), advance)
        })
    }
}

/// The fonts of a run, one per (face, size), made at the first request.
#[derive(Default)]
pub struct Fonts {
    fonts: HashMap<(bool, u32, bool), Font>,
}

impl Fonts {
    /// The font for a face string and a size string (see [`Font::request`]).
    pub fn get(&mut self, face: &str, size: &str) -> &mut Font {
        let (bold, ppem) = parse_request(face, size);
        let bold_word = face.contains(" Bold");
        self.fonts
            .entry((bold, ppem, bold_word))
            .or_insert_with(|| Font::new(bold, ppem, bold_word))
    }
}

/// The words that name the bold style in the face strings of each language: English, French,
/// German, Italian (" Gras" also holds " Grassetto"), Spanish and Portuguese.
const BOLD_WORDS: [&str; 5] = [" Bold", " Gras", " Fett", " Negrita", " Negrito"];

/// Whether the request is for the bold face, and its pixel size.
fn parse_request(face: &str, size: &str) -> (bool, u32) {
    let weight = face
        .split_once(',')
        .map_or(0, |(_, w)| digits(w.trim_start()));
    let bold = BOLD_WORDS.iter().any(|word| face.contains(word)) || weight >= 600;
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
pub(crate) fn from_cp1252(byte: u8) -> char {
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

/// Whether a name of these bytes, in the code page of `language`, holds a visible character: an
/// ASCII one from 0x21 to 0x7E, a double-byte lead byte, or on code page 932 a byte from 0xA1 to
/// 0xDF (ambients 3.3). Every name setter uses it.
pub(crate) fn visible(language: u32, bytes: &[u8]) -> bool {
    let lead = |b: u8| match language {
        1041 => matches!(b, 0x81..=0x9f | 0xe0..=0xfc),
        2052 | 4100 | 1042 | 1028 | 3076 | 5124 => matches!(b, 0x81..=0xfe),
        _ => false,
    };
    let kana = |b: u8| language == 1041 && matches!(b, 0xa1..=0xdf);
    bytes
        .iter()
        .any(|&b| b.is_ascii_graphic() || lead(b) || kana(b))
}

/// Whether Windows-1252 has the character `c`.
pub(crate) fn in_cp1252(c: char) -> bool {
    to_cp1252_exact(c).is_some()
}

/// The Windows-1252 byte of `c`, if the code page has it.
pub(crate) fn to_cp1252_exact(c: char) -> Option<u8> {
    (0..=255u8).find(|&b| from_cp1252(b) == c)
}

/// The Windows-1252 bytes of `text`; a character the code page lacks becomes `?`.
pub fn to_cp1252(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| to_cp1252_exact(c).unwrap_or(b'?'))
        .collect()
}

mod draw;
pub mod format;
mod layout;
mod raster;

pub use draw::{Style, draw, fill_color, text_color};
pub use layout::{Justify, Line, Rect, layout, measure, tooltip_size};

#[cfg(test)]
mod tests;
