//! The picture file of the snapshot button (map drawing 15.2): the folder, the next free numbered
//! name `Zoo001.jpg`, and the two file formats, JPEG (quality `[UI] JPEGQuality`) and BMP.

use std::path::{Path, PathBuf};

use crate::Framebuffer;
use crate::config::to_int;

/// The quality of a JPEG when `[UI] JPEGQuality` is missing.
pub(crate) const DEFAULT_QUALITY: i32 = 75;

/// The file name for the next snapshot in `dir`: `Zoo` and a number of three digits, one more
/// than the largest of the three characters before the dot of the existing `Zoo*.<ext>` files
/// (0 when there is none), and 0 after 999.
fn next_name(dir: &Path, ext: &str) -> String {
    let dot = format!(".{ext}");
    let mut largest = 0;
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.len().checked_sub(dot.len()).and_then(|n| {
            let (stem, tail) = (name.get(..n)?, name.get(n..)?);
            tail.eq_ignore_ascii_case(&dot).then_some(stem)
        }) else {
            continue;
        };
        if !stem.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("zoo")) {
            continue;
        }
        let digits = stem.get(stem.len().saturating_sub(3)..).unwrap_or("");
        largest = largest.max(to_int(digits.as_bytes()).unwrap_or(0));
    }
    let number = largest.saturating_add(1);
    format!("Zoo{:03}{dot}", if number > 999 { 0 } else { number })
}

/// The picture as a JPEG of `quality` (cut to 1 to 100), or as a 24-bit BMP.
fn encode(image: &Framebuffer, jpeg: bool, quality: i32) -> Result<Vec<u8>, String> {
    let (w, h) = (image.width as usize, image.height as usize);
    if w == 0 || h == 0 || image.pixels.len() < w * h * 4 {
        return Err("the picture is empty".into());
    }
    if jpeg {
        let (w16, h16) = (u16::try_from(w), u16::try_from(h));
        let (Ok(w16), Ok(h16)) = (w16, h16) else {
            return Err("the picture is too large for a JPEG".into());
        };
        let mut out = Vec::new();
        let quality = quality.clamp(1, 100) as u8;
        jpeg_encoder::Encoder::new(&mut out, quality)
            .encode(&image.pixels, w16, h16, jpeg_encoder::ColorType::Rgba)
            .map_err(|e| e.to_string())?;
        return Ok(out);
    }
    let row = (w * 3).next_multiple_of(4);
    let size = u32::try_from(54 + row * h).map_err(|_| "the picture is too large for a BMP")?;
    let mut out = Vec::with_capacity(size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&image.width.to_le_bytes());
    out.extend_from_slice(&image.height.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    // Rows from the bottom up, blue first.
    for line in image.pixels.chunks_exact(w * 4).rev() {
        out.extend(line.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]));
        out.resize(out.len() + row - w * 3, 0);
    }
    Ok(out)
}

/// Writes `image` as the next numbered file of `dir`, which is made when missing, and gives the
/// path of the file.
pub(crate) fn write(
    dir: &Path,
    image: &Framebuffer,
    jpeg: bool,
    quality: i32,
) -> Result<PathBuf, String> {
    let bytes = encode(image, jpeg, quality)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
    let path = dir.join(next_name(dir, if jpeg { "jpg" } else { "bmp" }));
    std::fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::TempDir;

    fn picture(w: u32, h: u32) -> Framebuffer {
        let mut f = Framebuffer::new(w, h);
        for (i, p) in f.pixels.chunks_exact_mut(4).enumerate() {
            p.copy_from_slice(&[(i * 7) as u8, 200, (i * 3) as u8, 255]);
        }
        f
    }

    #[test]
    fn the_name_is_one_more_than_the_largest_number_and_wraps_after_999() {
        let dir = TempDir::new("snapshot-names");
        let names = |ext| next_name(&dir.0, ext);
        assert_eq!(names("jpg"), "Zoo001.jpg", "an empty folder");
        for file in [
            "Zoo007.jpg",
            "zoo012.JPG",
            "Zoo003.bmp",
            "Zoo.jpg",
            "Pic500.jpg",
        ] {
            std::fs::write(dir.0.join(file), b"x").unwrap();
        }
        // The bmp 003, the file with no digits and the other prefix do not count for jpg.
        assert_eq!(names("jpg"), "Zoo013.jpg");
        assert_eq!(names("bmp"), "Zoo004.bmp");
        std::fs::write(dir.0.join("Zoo999.jpg"), b"x").unwrap();
        assert_eq!(names("jpg"), "Zoo000.jpg", "0 after 999");
        std::fs::write(dir.0.join("Zoo-05.jpg"), b"x").unwrap();
        assert_eq!(names("jpg"), "Zoo000.jpg", "a negative number is no larger");
        assert_eq!(next_name(&dir.0.join("missing"), "jpg"), "Zoo001.jpg");
    }

    #[test]
    fn a_bmp_holds_the_rows_bottom_up_in_blue_green_red() {
        let image = picture(3, 2);
        let bmp = encode(&image, false, 75).unwrap();
        // 3 pixels of 3 bytes pad to 12 per row.
        assert_eq!(bmp.len(), 54 + 12 * 2);
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()), 78);
        assert_eq!(u32::from_le_bytes(bmp[18..22].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(bmp[22..26].try_into().unwrap()), 2);
        // The first stored row is the last row of the picture: pixel 3, red 21 and blue 9.
        assert_eq!(&bmp[54..57], &[9, 200, 21]);
        assert_eq!(&bmp[66..69], &[0, 200, 0]);
    }

    #[test]
    fn a_jpeg_is_a_jpeg_at_any_quality_and_an_empty_picture_is_refused() {
        let image = picture(40, 30);
        for quality in [i32::MIN, -1, 0, 1, 75, 100, 101, i32::MAX] {
            let jpeg = encode(&image, true, quality).unwrap();
            assert_eq!(&jpeg[..2], &[0xff, 0xd8], "quality {quality}");
            assert_eq!(&jpeg[jpeg.len() - 2..], &[0xff, 0xd9], "quality {quality}");
        }
        let low = encode(&image, true, 1).unwrap().len();
        assert!(low < encode(&image, true, 100).unwrap().len());
        assert_eq!(
            encode(&image, true, 0).unwrap(),
            encode(&image, true, 1).unwrap()
        );
        assert_eq!(
            encode(&image, true, 500).unwrap(),
            encode(&image, true, 100).unwrap()
        );
        assert!(encode(&Framebuffer::new(0, 5), true, 75).is_err());
        assert!(encode(&Framebuffer::new(5, 0), false, 75).is_err());
        assert!(encode(&Framebuffer::new(70_000, 1), true, 75).is_err());
    }

    #[test]
    fn write_makes_the_folder_and_numbers_the_files() {
        let dir = TempDir::new("snapshot-write");
        let folder = dir.0.join("screens");
        let image = picture(8, 8);
        let first = write(&folder, &image, true, 75).unwrap();
        assert_eq!(first, folder.join("Zoo001.jpg"));
        let second = write(&folder, &image, true, 75).unwrap();
        assert_eq!(second, folder.join("Zoo002.jpg"));
        assert_eq!(
            write(&folder, &image, false, 75).unwrap(),
            folder.join("Zoo001.bmp")
        );
        assert!(std::fs::metadata(&first).unwrap().len() > 100);
        // A folder that is a file cannot be made: an error, no panic.
        assert!(write(&first, &image, true, 75).is_err());
    }
}
