//! Drawing laid-out text into a framebuffer that serves as an element's canvas.

use super::layout::{Justify, Rect, layout};
use super::{Font, Glyph};
use crate::Framebuffer;
use crate::sprite::{SHIFT, screen, show};

/// How a text looks.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// The text colour, as written in the layout.
    pub color: [u8; 3],
    pub justify: Justify,
    /// A black copy under the text, moved left and down by the line height rule.
    pub shadow: bool,
}

/// The canvas colour for a layout colour, by the game's rule for fills: each channel goes to
/// 5-6-5 as `R * 31 / 255`, `G * 63 / 255`, `B * 31 / 255` and is shown with its high bits
/// repeated ([`crate::sprite::screen`]).
pub fn fill_color([r, g, b]: [u8; 3]) -> [u8; 4] {
    let [r, g, b] = screen([r, g, b]);
    [r, g, b, 255]
}

/// The colour of text pixels for a layout colour: the low bits are dropped (`R >> 3`, `G >> 2`,
/// `B >> 3`), as the system does when it converts a colour to the 16-bit surface. This differs
/// from [`fill_color`] by one level in some channels.
pub fn text_color(rgb: [u8; 3]) -> [u8; 4] {
    let [r, g, b] = [0, 1, 2].map(|c| show(c, u16::from(rgb[c] >> SHIFT[c])));
    [r, g, b, 255]
}

/// Draws `text` in `rect` of `target`: wrapped, aligned line by line from the top of the
/// rectangle, with the shadow first if asked, and clipped to the rectangle and the framebuffer.
/// Pixels with partial coverage are mixed with the pixel already there, so the canvas should
/// be filled with its background colour ([`fill_color`]) first.
pub fn draw(target: &mut Framebuffer, font: &mut Font, rect: Rect, text: &[u8], style: &Style) {
    if style.shadow {
        let d = if font.line_height() < 21 { 1 } else { 2 };
        pass(
            target,
            font,
            rect.moved(-d, d),
            text,
            [0, 0, 0],
            style.justify,
        );
    }
    pass(target, font, rect, text, style.color, style.justify);
}

/// One drawing of the text in one colour.
fn pass(
    target: &mut Framebuffer,
    font: &mut Font,
    rect: Rect,
    text: &[u8],
    color: [u8; 3],
    justify: Justify,
) {
    let clip = Rect {
        left: rect.left.max(0),
        top: rect.top.max(0),
        right: rect.right.min(target.width as i32),
        bottom: rect.bottom.min(target.height as i32),
    };
    let (line_height, ascent) = (font.line_height(), font.ascent());
    // Bold text is smoothed at every size; only regular text between 9 and 16 pixels is bilevel.
    let (bold, bilevel) = (
        font.is_bold(),
        !font.is_bold() && (9..=16).contains(&font.ppem()),
    );
    for (i, line) in layout(font, text, rect.width()).iter().enumerate() {
        let top = rect
            .top
            .saturating_add((i as i32).saturating_mul(line_height));
        if top >= clip.bottom {
            break;
        }
        let mut pen = justify.start(&rect, line.width);
        for &byte in &line.text {
            let advance = font.advance(byte);
            let glyph = font.glyph(byte);
            let (x, y) = (
                pen.saturating_add(glyph.left),
                top + ascent - glyph.bottom - glyph.height as i32,
            );
            put(target, &clip, glyph, (x, y), color, bilevel, bold);
            pen = pen.saturating_add(advance);
        }
    }
}

/// The coverage of a bold glyph pixel made heavier: the original's hinted bold stems are whole
/// pixels wide, which unhinted coverage cannot match; a square root of the coverage has the same
/// amount of ink.
pub(super) fn heavier(coverage: u32) -> u32 {
    (255.0 * (coverage as f32 / 255.0).sqrt()).round() as u32
}

/// Writes the coverage of `glyph` with its top left corner at `at`.
fn put(
    target: &mut Framebuffer,
    clip: &Rect,
    glyph: &Glyph,
    at: (i32, i32),
    color: [u8; 3],
    bilevel: bool,
    bold: bool,
) {
    let width = target.width as usize;
    for row in 0..glyph.height {
        let y = at.1 + row as i32;
        if y < clip.top || y >= clip.bottom {
            continue;
        }
        for col in 0..glyph.width {
            let x = at.0 + col as i32;
            let mut coverage = u32::from(glyph.coverage[row * glyph.width + col]);
            if x < clip.left || x >= clip.right || coverage == 0 {
                continue;
            }
            if bold {
                coverage = heavier(coverage);
            }
            if bilevel {
                if coverage < 128 {
                    continue;
                }
                coverage = 255;
            }
            let at = (y as usize * width + x as usize) * 4;
            let pixel = &mut target.pixels[at..at + 4];
            // Mix in 8 bits, then reduce like a text colour.
            let mixed = [0, 1, 2].map(|c| {
                ((u32::from(pixel[c]) * (255 - coverage) + u32::from(color[c]) * coverage + 127)
                    / 255) as u8
            });
            pixel.copy_from_slice(&text_color(mixed));
        }
    }
}
