//! The commands that show how text is measured and drawn. They need no game data.

use std::io::{self, Write};

use rustyzoo::text::{Font, to_cp1252};

use crate::CliError;

/// The face and size strings of the flags `--font` and `--size` (Arial and 8 when absent).
pub fn font_flags<'a>(mut flags: &[&'a str]) -> Result<(&'a str, &'a str), CliError> {
    let (mut face, mut size) = ("Arial", "8");
    while let [flag, rest @ ..] = flags {
        flags = match (*flag, rest) {
            ("--font", [value, rest @ ..]) => {
                face = value;
                rest
            }
            ("--size", [value, rest @ ..]) => {
                size = value;
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    Ok((face, size))
}

/// Prints the width in pixels of `text` on one line.
pub fn text_width(text: &str, flags: &[&str]) -> Result<(), CliError> {
    let (face, size) = font_flags(flags)?;
    let width = Font::request(face, size).width(&to_cp1252(text));
    crate::ignore_closed_pipe(writeln!(io::stdout().lock(), "{width}"))
}
