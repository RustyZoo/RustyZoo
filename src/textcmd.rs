//! The commands that show how text is measured and drawn. They need no game data.

use std::fs::File;
use std::io::{self, Write};

use rustyzoo::Framebuffer;
use rustyzoo::app::frame_hash;
use rustyzoo::sprite::write_png;
use rustyzoo::text::{Font, Justify, Rect, Style, draw, fill_color, measure, to_cp1252};

use crate::CliError;

/// The largest picture `draw-text` makes, in each direction.
const MAX_SIDE: i64 = 16384;

/// Parses a decimal option value; `None` if it is not a number.
fn number(word: &str) -> Result<i64, CliError> {
    word.parse().map_err(|_| CliError::Usage)
}

fn color(words: [&str; 3]) -> Result<[u8; 3], CliError> {
    let channel = |w: &str| u8::try_from(number(w)?).map_err(|_| CliError::Usage);
    Ok([channel(words[0])?, channel(words[1])?, channel(words[2])?])
}

/// The face and size strings of `--font` and `--size` (Arial and 8 when absent); `draw-text`
/// also takes `--width`, `--justify`, `--shadow`, `--fore R G B` and `--back R G B`.
struct Flags<'a> {
    face: &'a str,
    size: &'a str,
    width: Option<i64>,
    justify: Justify,
    shadow: bool,
    fore: [u8; 3],
    back: [u8; 3],
}

fn flags<'a>(mut words: &[&'a str], drawing: bool) -> Result<Flags<'a>, CliError> {
    let mut f = Flags {
        face: "Arial",
        size: "8",
        width: None,
        justify: Justify::Left,
        shadow: false,
        fore: [0, 0, 0],
        back: [255, 255, 255],
    };
    while let [flag, rest @ ..] = words {
        words = match (*flag, rest) {
            ("--font", [value, rest @ ..]) => {
                f.face = value;
                rest
            }
            ("--size", [value, rest @ ..]) => {
                f.size = value;
                rest
            }
            ("--width", [value, rest @ ..]) if drawing => {
                f.width = Some(number(value)?);
                rest
            }
            ("--justify", [value, rest @ ..]) if drawing => {
                f.justify = Justify::from_layout(value);
                rest
            }
            ("--shadow", rest) if drawing => {
                f.shadow = true;
                rest
            }
            ("--fore", [r, g, b, rest @ ..]) if drawing => {
                f.fore = color([r, g, b])?;
                rest
            }
            ("--back", [r, g, b, rest @ ..]) if drawing => {
                f.back = color([r, g, b])?;
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    Ok(f)
}

/// Prints the width in pixels of `text` on one line.
pub fn text_width(text: &str, words: &[&str]) -> Result<(), CliError> {
    let f = flags(words, false)?;
    let width = Font::request(f.face, f.size).width(&to_cp1252(text));
    crate::ignore_closed_pipe(writeln!(io::stdout().lock(), "{width}"))
}

/// Draws `text` wrapped to `--width` pixels on a canvas as tall as the text, into a PNG file,
/// and prints the size of the picture and the hash of its pixels.
pub fn draw_text(out: &str, text: &str, words: &[&str]) -> Result<(), CliError> {
    let f = flags(words, true)?;
    let width = f
        .width
        .filter(|w| (1..=MAX_SIDE).contains(w))
        .ok_or("--width is needed and must be 1 to 16384")? as i32;
    let mut font = Font::request(f.face, f.size);
    let text = to_cp1252(text);
    let (_, height) = measure(&font, &text, width);
    if i64::from(height) > MAX_SIDE {
        return Err("the text is too tall for a picture".into());
    }
    let mut canvas = Framebuffer::new(width as u32, height.max(1) as u32);
    canvas.fill(fill_color(f.back));
    let style = Style {
        color: f.fore,
        justify: f.justify,
        shadow: f.shadow,
    };
    draw(
        &mut canvas,
        &mut font,
        Rect::inset(width, height, 0, 0),
        &text,
        &style,
    );
    let file = File::create(out).map_err(|e| format!("cannot create {out}: {e}"))?;
    write_png(&canvas, io::BufWriter::new(file)).map_err(|e| format!("cannot write {out}: {e}"))?;
    crate::ignore_closed_pipe(writeln!(
        io::stdout().lock(),
        "{}x{} {:016x}",
        canvas.width,
        canvas.height,
        frame_hash(&canvas)
    ))
}
