//! Wrapping, alignment and measuring, as the original's `DrawText` with the flags word break,
//! no prefix and word ellipsis.

use super::Font;

/// A rectangle in pixels; right and bottom are exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    /// The rectangle moved by `(dx, dy)`.
    pub fn moved(&self, dx: i32, dy: i32) -> Rect {
        Rect {
            left: self.left.saturating_add(dx),
            top: self.top.saturating_add(dy),
            right: self.right.saturating_add(dx),
            bottom: self.bottom.saturating_add(dy),
        }
    }

    /// The text rectangle of an element of `width` by `height` pixels with the given borders.
    pub fn inset(width: i32, height: i32, xborder: i32, yborder: i32) -> Rect {
        Rect {
            left: xborder,
            top: yborder,
            right: width.saturating_sub(xborder),
            bottom: height.saturating_sub(yborder),
        }
    }

    /// The rectangle a button's text is laid out in: moved so that one line of the font is
    /// centred vertically. A text of more than one line therefore starts above the centre.
    pub fn centred_line(&self, font: &Font) -> Rect {
        let (h, t) = (self.bottom.saturating_sub(self.top), font.line_height());
        // Divisions truncate toward zero; the top moves down and the bottom up by the same.
        let d = h.saturating_sub(t) / 2;
        Rect {
            top: self.top.saturating_add(d),
            bottom: self.bottom.saturating_sub(d),
            ..*self
        }
    }
}

/// Horizontal alignment of each line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Left,
    Center,
    Right,
}

impl Justify {
    /// The value of the layout key `justify`, read by its first letter: `c` is centre, `r` is
    /// right, anything else (`left`, `top`, `bottom`, a number, a capital letter) is left.
    pub fn from_layout(value: &str) -> Justify {
        match value.bytes().next() {
            Some(b'c') => Justify::Center,
            Some(b'r') => Justify::Right,
            _ => Justify::Left,
        }
    }

    /// The left end of a line of `line` pixels in `rect`. A line wider than the rectangle gets
    /// a negative offset when centred.
    pub fn start(self, rect: &Rect, line: i32) -> i32 {
        match self {
            Justify::Left => rect.left,
            Justify::Center => rect
                .left
                .saturating_add(rect.width().saturating_sub(line) / 2),
            Justify::Right => rect.right.saturating_sub(line),
        }
    }
}

/// One line of laid-out text: the bytes to draw (a cut word ends with `...`) and their width.
#[derive(Debug, PartialEq, Eq)]
pub struct Line {
    pub text: Vec<u8>,
    pub width: i32,
}

/// Lays `text` out in lines of at most `width` pixels. A carriage return, a line feed or both
/// together end a line (a last line is not empty because the text ends in one); within a line
/// the words are filled in as they fit, the spaces between two words on a line kept and the
/// spaces at a break dropped. A word wider than `width` is cut to the longest beginning that fits
/// with `...` after it and ends its line; `...` itself is kept even if it does not fit, and the
/// drawing clips it.
pub fn layout(font: &Font, text: &[u8], width: i32) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < text.len() {
        if matches!(text[i], b'\r' | b'\n') {
            wrap(font, &text[start..i], width, &mut lines);
            if text[i] == b'\r' && text.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < text.len() {
        wrap(font, &text[start..], width, &mut lines);
    }
    lines
}

/// Appends the lines of one source line, which holds no line break.
fn wrap(font: &Font, source: &[u8], width: i32, out: &mut Vec<Line>) {
    let count = out.len();
    let mut current = Line {
        text: Vec::new(),
        width: 0,
    };
    let mut rest = source;
    let mut first = true;
    while !rest.is_empty() {
        let n = rest.iter().take_while(|&&b| b == b' ').count();
        let (spaces, tail) = rest.split_at(n);
        let n = tail.iter().take_while(|&&b| b != b' ').count();
        let (word, tail) = tail.split_at(n);
        rest = tail;
        if word.is_empty() {
            break; // spaces at the end of the line
        }
        let word_width = font.width(word);
        if !current.text.is_empty() {
            let joined = current.width + font.width(spaces) + word_width;
            if joined <= width {
                current.text.extend_from_slice(spaces);
                current.text.extend_from_slice(word);
                current.width = joined;
                continue;
            }
            out.push(std::mem::replace(
                &mut current,
                Line {
                    text: Vec::new(),
                    width: 0,
                },
            ));
        }
        // A word starts a line; the spaces before it are kept only at the start of the source.
        let lead = if first { spaces } else { &[] };
        first = false;
        let mut text = lead.to_vec();
        text.extend_from_slice(word);
        let total = font.width(&text);
        if total <= width {
            current = Line { text, width: total };
        } else {
            out.push(cut(font, &text, width));
        }
    }
    if !current.text.is_empty() {
        out.push(current);
    }
    if out.len() == count {
        out.push(Line {
            text: Vec::new(),
            width: 0,
        });
    }
}

/// The longest beginning of `word` that fits in `width` with `...`, followed by `...`.
fn cut(font: &Font, word: &[u8], width: i32) -> Line {
    let dots = font.width(b"...");
    let mut kept = 0;
    let mut used = 0;
    for &b in word {
        if used + font.advance(b) + dots > width {
            break;
        }
        used += font.advance(b);
        kept += 1;
    }
    let mut text = word[..kept].to_vec();
    text.extend_from_slice(b"...");
    Line {
        text,
        width: used + dots,
    }
}

/// The size of `text` laid out in `width` pixels: the widest line, and the number of lines times
/// the line height.
pub fn measure(font: &Font, text: &[u8], width: i32) -> (i32, i32) {
    let lines = layout(font, text, width);
    let w = lines.iter().map(|l| l.width).max().unwrap_or(0);
    (w, lines.len() as i32 * font.line_height())
}

/// The size of a tooltip of `text` in `font`, borders included. `aligned` is true when the
/// element's alignment is not left, and `max_width` the widest tooltip (the ini file's
/// `maxShortTooltipWidth`, or `maxLongTooltipWidth` with long help).
///
/// The text is measured on one line; a small font (under 9 points, or 9 points bold) with an
/// alignment other than left adds 2 pixels; if that is wider than `max_width` the width becomes
/// the smaller of `max_width` and three fifths of it, and the text is measured again wrapped in
/// that width, which gives the width and the height.
pub fn tooltip_size(
    font: &Font,
    text: &[u8],
    aligned: bool,
    max_width: i32,
    xborder: i32,
    yborder: i32,
) -> (i32, i32) {
    let (mut w, mut h) = measure(font, text, i32::MAX);
    let small = font.ppem() < 12 || (font.ppem() == 12 && font.is_bold());
    if small && aligned {
        w += 2;
    }
    if w > max_width {
        (w, h) = measure(font, text, max_width.min((i64::from(w) * 3 / 5) as i32));
    }
    (w + 2 * xborder, h + 2 * yborder)
}
