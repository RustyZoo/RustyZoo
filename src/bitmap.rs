//! Root bitmaps (grid lines, highlight, cliff pieces) turned into sprites.
//!
//! The game ships these pictures as bitmaps and, in `ztst/`, as converted sprites with a palette.
//! Some data has only the bitmaps: [`convert`] makes the sprite and the palette from one, with
//! the same pixels, transparency, size and offsets as the shipped copies. Nothing is written.
//!
//! Conversion: each colour is reduced to 5-6-5 bits and expanded back; the reduced colour of the
//! top-left pixel is the colour key (transparent); the sprite is the bounding box of the other
//! pixels, anchored at the centre of the bitmap; the palette holds the distinct reduced colours.

use crate::Framebuffer;
use crate::config::Config;
use crate::sprite::{Frame, MAX_COLORS, MAX_DIMENSION, Palette};
use crate::vfs::Vfs;

/// A bitmap: `(width, height)` and the RGB pixels, top row first.
pub(crate) type Pixels = (usize, usize, Vec<[u8; 3]>);

fn le(data: &[u8], at: usize, len: usize) -> Result<u32, String> {
    let bytes = data.get(at..at + len).ok_or("truncated bitmap")?;
    Ok(bytes
        .iter()
        .rev()
        .fold(0, |value, &byte| value << 8 | u32::from(byte)))
}

fn reduce(r: u8, g: u8, b: u8) -> [u8; 3] {
    let to = |value: u8, max: u32| (u32::from(value) * max / 255 * 255 / max) as u8;
    [to(r, 31), to(g, 63), to(b, 31)]
}

/// Reads an uncompressed 24-bit or 8-bit Windows bitmap; the colours are as stored.
pub(crate) fn read(data: &[u8]) -> Result<Pixels, String> {
    if data.get(..2) != Some(b"BM") {
        return Err("not a bitmap".into());
    }
    let (start, header) = (le(data, 10, 4)? as usize, le(data, 14, 4)? as usize);
    let (width, height) = (le(data, 18, 4)? as i32, le(data, 22, 4)? as i32);
    let (bits, compression) = (le(data, 28, 2)?, le(data, 30, 4)?);
    if header < 40 || compression != 0 || !matches!(bits, 8 | 24) {
        return Err(format!(
            "unsupported bitmap ({bits} bits, compression {compression})"
        ));
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let limit = usize::from(MAX_DIMENSION);
    if width <= 0 || h == 0 || w > limit || h > limit {
        return Err(format!("bitmap size {width}x{height} not supported"));
    }
    let table = (14 + header, 1usize << 8);
    let stride = (w * bits as usize).div_ceil(32) * 4;
    let rows = data.get(start..).ok_or("truncated bitmap")?;
    if rows.len() < stride * h || (bits == 8 && data.len() < table.0 + table.1 * 4) {
        return Err("truncated bitmap".into());
    }
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        // Bottom-up unless the height is negative.
        let row = &rows[(if height > 0 { h - 1 - y } else { y }) * stride..];
        for x in 0..w {
            pixels.push(match bits {
                24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3]],
                _ => {
                    let entry = &data[table.0 + usize::from(row[x]) * 4..];
                    [entry[2], entry[1], entry[0]]
                }
            });
        }
    }
    Ok((w, h, pixels))
}

/// The order of a colour in the palette: the interleaved bits `r7 g7 b7 r6 g6 b6 ...`.
fn palette_order(color: &[u8; 3]) -> u32 {
    (0..24).fold(0, |key, i| {
        key << 1 | u32::from(color[(i % 3) as usize] >> (7 - i / 3) & 1)
    })
}

/// Converts the bytes of a bitmap into the sprite frame and the palette of the game.
///
/// Fails on a bitmap that is not an uncompressed 8 or 24 bit one, is larger than
/// [`MAX_DIMENSION`], has more than [`MAX_COLORS`] distinct reduced colours or has a row of more
/// than 255 segments.
pub fn convert(bitmap: &[u8]) -> Result<(Frame, Palette), String> {
    let (w, h, mut pixels) = read(bitmap)?;
    for p in &mut pixels {
        *p = reduce(p[0], p[1], p[2]);
    }
    let key = pixels[0];
    let mut colors: Vec<[u8; 3]> = pixels.clone();
    colors.sort_by_key(palette_order);
    colors.dedup();
    if colors.len() > MAX_COLORS {
        return Err(format!(
            "{} colours, at most {MAX_COLORS} are supported",
            colors.len()
        ));
    }
    let mut pal = (colors.len() as u32).to_le_bytes().to_vec();
    pal.extend(colors.iter().flat_map(|c| [c[0], c[1], c[2], 255]));

    // The bounding box of the visible pixels.
    let visible = |x: usize, y: usize| pixels[y * w + x] != key;
    let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
    for (x, y) in (0..h).flat_map(|y| (0..w).map(move |x| (x, y))) {
        if visible(x, y) {
            (left, top) = (left.min(x), top.min(y));
            (right, bottom) = (right.max(x + 1), bottom.max(y + 1));
        }
    }
    if left >= right {
        (left, top, right, bottom) = (0, 0, 0, 0);
    }
    let (y_offset, x_offset) = match right > 0 {
        true => ((h / 2) as i16 - top as i16, (w / 2) as i16 - left as i16),
        false => (0, 0),
    };
    let mut lle = Vec::new();
    for value in [bottom - top, right - left] {
        lle.extend((value as u16).to_le_bytes());
    }
    for value in [y_offset, x_offset, 0] {
        lle.extend(value.to_le_bytes());
    }
    for y in top..bottom {
        let mut segments = Vec::new();
        let mut x = left;
        while x < right {
            let skip = (x..right).take(255).take_while(|&i| !visible(i, y)).count();
            x += skip;
            let run: Vec<u8> = (x..right)
                .take(255)
                .take_while(|&i| visible(i, y))
                .map(|i| {
                    colors
                        .binary_search_by_key(&palette_order(&pixels[y * w + i]), palette_order)
                        .unwrap() as u8
                })
                .collect();
            x += run.len();
            segments.push((skip as u8, run));
        }
        if segments.len() > 255 {
            return Err(format!("row {y} has {} segments", segments.len()));
        }
        lle.push(segments.len() as u8);
        for (skip, run) in segments {
            lle.extend([skip, run.len() as u8]);
            lle.extend(run);
        }
    }
    let frame = Frame::parse(&lle).map_err(|e| e.to_string())?;
    Ok((frame, Palette::parse(&pal).map_err(|e| e.to_string())?))
}

/// The member path of a bitmap named in a configuration file.
pub fn member_name(value: &[u8]) -> String {
    crate::vfs::name_from_bytes(value).trim().replace('\\', "/")
}

/// The bitmaps the terrain loads: the `highlight`, `northgrid` and `eastgrid` of every section of
/// the `tile*.cfg` files and the `image` of every section of `fringe.cfg`, each once.
pub fn names(vfs: &Vfs) -> Vec<String> {
    let read = |member: &str| {
        (vfs.read(member).ok())
            .and_then(|bytes| Config::parse(&bytes).ok())
            .unwrap_or_default()
    };
    let mut names = Vec::new();
    let mut take = |config: Config, keys: &[&str]| {
        for (_, section) in config.sections() {
            for key in keys {
                names.extend(section.get(key).map(member_name));
            }
        }
    };
    for member in vfs.list(Some("tile*.cfg")) {
        if member.starts_with("tile") && !member.contains('/') {
            take(read(&member), &["highlight", "northgrid", "eastgrid"]);
        }
    }
    take(read("fringe.cfg"), &["image"]);
    let mut seen = std::collections::HashSet::new();
    names.retain(|name| seen.insert(name.clone()));
    names
}

/// The shipped copy of a bitmap, `ztst/<name>` without its extension and its palette, if both exist.
pub fn shipped(vfs: &Vfs, bitmap: &str) -> Option<(Frame, Palette)> {
    let stem = bitmap.rsplit_once('.').map_or(bitmap, |(stem, _)| stem);
    let frame = Frame::parse(&vfs.read(&format!("ztst/{stem}.lle")).ok()?).ok()?;
    let palette = Palette::parse(&vfs.read(&format!("ztst/{stem}.pal")).ok()?).ok()?;
    Some((frame, palette))
}

/// The picture of a bitmap: its shipped copy, or, when either file of the copy is missing, the
/// bitmap converted.
pub fn load(vfs: &Vfs, bitmap: &str) -> Option<(Frame, Palette)> {
    shipped(vfs, bitmap).or_else(|| convert(&vfs.read(bitmap).ok()?).ok())
}

/// Compares two pictures by their size, offsets and drawn pixels (colour and transparency), not by
/// the numbering of their palettes. Returns what differs first.
pub fn compare(a: &(Frame, Palette), b: &(Frame, Palette)) -> Result<(), String> {
    let geometry = |f: &Frame| (f.width, f.height, f.x_offset, f.y_offset);
    if geometry(&a.0) != geometry(&b.0) {
        return Err(format!(
            "box and offsets differ: {:?} against {:?}",
            geometry(&a.0),
            geometry(&b.0)
        ));
    }
    let draw = |(frame, palette): &(Frame, Palette)| {
        let mut image = Framebuffer::new(u32::from(frame.width), u32::from(frame.height));
        frame.draw(palette, &mut image, 0, 0);
        image.pixels
    };
    match draw(a).iter().zip(draw(b)).position(|(x, y)| *x != y) {
        None => Ok(()),
        Some(at) => Err(format!("pixels differ, first at byte {at}")),
    }
}

#[cfg(test)]
pub(crate) mod tests;
