//! What a windowed run can report for a human or CI to read: the pixels the renderer presents
//! (`run --shot`). Information only: no value here fails a run.

use std::collections::HashSet;
use std::fs::File;
use std::io::BufWriter;

use rustyzoo::Framebuffer;
use rustyzoo::app::frame_hash;
use rustyzoo::sprite::write_png;
use sdl3::pixels::PixelFormat;
use sdl3::render::Canvas;
use sdl3::video::Window;

use crate::cli::CliError;

/// Reads back what the renderer has drawn, in the pixels of the output, and writes it as a PNG.
/// Prints the size, the number of distinct colours and the hash of the pixels; a blank picture
/// (one colour) is an error.
pub fn shot(canvas: &Canvas<Window>, file: &str) -> Result<(), CliError> {
    let surface = canvas
        .read_pixels(None)?
        .convert_format(PixelFormat::RGBA32)?;
    let (width, height) = surface.size();
    let (row, pitch) = (width as usize * 4, surface.pitch() as usize);
    let pixels = surface.with_lock(|bytes| {
        bytes
            .chunks(pitch)
            .take(height as usize)
            .flat_map(|r| r[..row].iter().copied())
            .collect()
    });
    let out = File::create(file).map_err(|e| format!("{file}: {e}"))?;
    let frame = Framebuffer {
        width,
        height,
        pixels,
    };
    write_png(&frame, BufWriter::new(out))?;
    let colours = frame.pixels.chunks(4).collect::<HashSet<_>>().len();
    println!(
        "shot {width}x{height} colours {colours} hash {:016x}",
        frame_hash(&frame)
    );
    match colours {
        0..=1 => Err("the picture read back is blank".into()),
        _ => Ok(()),
    }
}
