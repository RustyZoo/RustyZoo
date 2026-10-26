//! The mouse cursors of the original: Windows cursor resources of the `res*.dll` libraries of the
//! install, read at run time (string tables 6). A cursor is named by a number; with the
//! alternate cursors on, 50 is added to it first. The window shows them; this module only
//! decodes one into pixels.

use std::path::Path;

use crate::config::Config;
use crate::pe::{self, ResKey, Resource};
use crate::strings::library_names;
use crate::timing::ini_whole;
use crate::vfs::MAX_MEMBER_SIZE;

/// The cursor of the idle pointer, and those of the terrain tool (terrain editing 7.1).
pub const NORMAL: u32 = 209;
pub const TERRAIN_IDLE: u32 = 203;
pub const TERRAIN_DRAG: u32 = 204;
pub const TERRAIN_LEVEL: u32 = 205;
pub const TERRAIN_PAINT: u32 = 206;

/// The resource types of a cursor and of the group that names it.
const RT_CURSOR: u32 = 1;
const RT_GROUP_CURSOR: u32 = 12;
/// The largest side of a cursor that is read (the originals are 32).
const MAX_SIDE: usize = 128;

/// Whether the ini file asks for the alternate cursors (`[UI] useAlternateCursors`, shipped 1):
/// any whole number but 0. Missing or not a number: off.
pub fn alternate(ini: &Config) -> bool {
    ini_whole(ini, "UI", "useAlternateCursors").is_some_and(|n| n != 0)
}

/// The resource number of cursor `base` with the alternate cursors on or off.
pub fn number(base: u32, alternate: bool) -> u32 {
    base.saturating_add(if alternate { 50 } else { 0 })
}

/// A cursor as pixels.
#[derive(Debug, PartialEq)]
pub struct Cursor {
    pub width: u32,
    pub height: u32,
    /// The pixel that is the pointer's place.
    pub hot: (u32, u32),
    /// Rows from the top, R G B A. A pixel the mask leaves open is transparent.
    pub rgba: Vec<u8>,
}

/// Cursor `number` from the first library `res*.dll` of the install folder (the greatest name
/// first) that has it; `None` if none does or it cannot be read.
pub fn load(install: &Path, number: u32) -> Option<Cursor> {
    library_names(install, "res")
        .ok()?
        .into_iter()
        .find_map(|name| {
            let path = install.join(name);
            if path.metadata().ok()?.len() > MAX_MEMBER_SIZE {
                return None;
            }
            let file = std::fs::read(path).ok()?;
            find(&pe::resources(&file).ok()?, number)
        })
}

/// Cursor `number` of the resources of one library: its group names the image.
fn find(resources: &[Resource], number: u32) -> Option<Cursor> {
    let of = |kind, id| {
        resources
            .iter()
            .find(|r| r.kind == ResKey::Id(kind) && r.name == ResKey::Id(id))
            .map(|r| r.data)
    };
    let group = of(RT_GROUP_CURSOR, number)?;
    // Header of 6 bytes, then entries of 14: the image number is the last two bytes of the first.
    let image = u16::from_le_bytes(group.get(18..20)?.try_into().ok()?);
    decode(of(RT_CURSOR, u32::from(image))?)
}

/// A cursor image: the hot spot, then a bitmap with a palette or 24 bits whose height counts the
/// colours and the mask (AND) above each other. A masked pixel is transparent; the inverting
/// pixel of the system is drawn transparent too.
fn decode(data: &[u8]) -> Option<Cursor> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?));
    let start = 4;
    let (head, bits) = (u32_at(start)? as usize, usize::from(u16_at(start + 14)?));
    let width = u32_at(start + 4)? as usize;
    let height = u32_at(start + 8)? as usize / 2;
    if head < 40 || u32_at(start + 16)? != 0 || !matches!(bits, 1 | 4 | 8 | 24) {
        return None;
    }
    if !(1..=MAX_SIDE).contains(&width) || !(1..=MAX_SIDE).contains(&height) {
        return None;
    }
    let colors = match bits {
        24 => 0,
        _ => match u32_at(start + 32)? as usize {
            0 => 1 << bits,
            n => n.min(1 << bits),
        },
    };
    let palette = start + head;
    let image = palette + 4 * colors;
    let row = (width * bits).div_ceil(32) * 4;
    let mask = image + row * height;
    let mask_row = width.div_ceil(32) * 4;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        // The rows are stored from the bottom.
        let up = height - 1 - y;
        for x in 0..width {
            let closed = data.get(mask + up * mask_row + x / 8)? >> (7 - x % 8) & 1 == 1;
            let at = image + up * row;
            let color = match bits {
                24 => data.get(at + 3 * x..at + 3 * x + 3)?.to_vec(),
                _ => {
                    let byte = *data.get(at + x * bits / 8)?;
                    let index = match bits {
                        8 => byte,
                        4 => byte >> (4 * (1 - x % 2)) & 15,
                        _ => byte >> (7 - x % 8) & 1,
                    };
                    let index = usize::from(index);
                    if index >= colors {
                        return None;
                    }
                    data.get(palette + 4 * index..palette + 4 * index + 3)?
                        .to_vec()
                }
            };
            // Stored blue, green, red.
            rgba.extend([color[2], color[1], color[0], if closed { 0 } else { 255 }]);
        }
    }
    let hot = |at| u32::from(u16_at(at).unwrap_or(0)).min(width as u32 - 1);
    Some(Cursor {
        width: width as u32,
        height: height as u32,
        hot: (hot(0), hot(2).min(height as u32 - 1)),
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cursor image of 2 by 2 pixels with 1 bit per pixel, the palette black and white, the
    /// bottom row first: white, black; the top row black, white; the mask closes the top right
    /// pixel.
    fn image(width: u32, height: u32) -> Vec<u8> {
        let mut data = vec![1, 0, 1, 0]; // hot spot (1, 1)
        for field in [40, width, height.wrapping_mul(2), 0x10001, 0, 0, 0, 0, 0, 0] {
            data.extend(u32::to_le_bytes(field));
        }
        data.extend([0, 0, 0, 0, 255, 255, 255, 0]);
        data.extend([0b1000_0000, 0, 0, 0, 0b0100_0000, 0, 0, 0]);
        data.extend([0, 0, 0, 0, 0b0100_0000, 0, 0, 0]);
        data
    }

    #[test]
    fn a_cursor_image_becomes_pixels_with_its_mask_and_hot_spot() {
        let cursor = decode(&image(2, 2)).unwrap();
        assert_eq!((cursor.width, cursor.height, cursor.hot), (2, 2, (1, 1)));
        let black = [0, 0, 0, 255];
        let white = [255, 255, 255, 255];
        let open = [255, 255, 255, 0];
        let want: Vec<u8> = [black, open, white, black].concat();
        assert_eq!(cursor.rgba, want);
    }

    #[test]
    fn a_bad_image_is_none() {
        let good = image(2, 2);
        for cut in 0..good.len() - 3 {
            assert_eq!(decode(&good[..cut]), None, "cut at {cut}");
        }
        let side = |w, h| decode(&image(w, h));
        // A wide image with all its data is refused for its width alone.
        let mut wide = vec![0; 4];
        for field in [40, 129, 2, 0x10001, 0, 0, 0, 0, 0, 0] {
            wide.extend(u32::to_le_bytes(field));
        }
        wide.extend(vec![0; 8 + 20 + 20]);
        assert!(decode(&wide).is_none());
        wide[4 + 4..4 + 8].copy_from_slice(&128u32.to_le_bytes());
        wide.truncate(4 + 40 + 8);
        wide.extend(vec![0; 16 + 16]);
        assert!(decode(&wide).is_some(), "128 is the limit");
        assert!(side(0, 2).is_none() && side(2, 0).is_none());
        assert!(side(u32::MAX, 2).is_none() && side(2, u32::MAX).is_none());
        assert!(side(MAX_SIDE as u32 + 1, 2).is_none());
        let mut packed = good.clone();
        packed[4 + 16] = 1; // a compressed bitmap
        assert!(decode(&packed).is_none());
        let mut deep = good.clone();
        deep[4 + 14] = 16; // 16 bits per pixel
        assert!(decode(&deep).is_none());
        let mut head = good;
        head[4] = 8; // a header too short
        assert!(decode(&head).is_none());
    }

    #[test]
    fn a_hot_spot_outside_the_image_is_moved_inside() {
        let mut data = image(2, 2);
        data[..4].copy_from_slice(&[9, 9, 0xFF, 0xFF]);
        assert_eq!(decode(&data).unwrap().hot, (1, 1));
    }

    #[test]
    fn a_group_names_the_image() {
        let mut group = vec![0, 0, 2, 0, 1, 0];
        group.extend([2, 2, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 7, 0]);
        let resource = |kind, name, data| Resource {
            kind: ResKey::Id(kind),
            name: ResKey::Id(name),
            language: 0,
            data,
        };
        let data = image(2, 2);
        let list = [resource(12, 209, &group), resource(1, 7, &data)];
        assert!(find(&list, 209).is_some());
        assert!(find(&list, 259).is_none(), "no such group");
        assert!(find(&list[..1], 209).is_none(), "the image is missing");
        assert!(find(&[resource(12, 5, &group[..10])], 5).is_none());
    }

    #[test]
    fn the_ini_file_turns_the_alternate_set_on_by_any_number_but_0() {
        let on = |text: &str| alternate(&Config::parse(text.as_bytes()).unwrap());
        assert!(on("[UI]\nuseAlternateCursors=1"));
        assert!(!on("[UI]\nuseAlternateCursors=0"));
        assert!(on("[UI]\nuseAlternateCursors=-1"));
        assert!(on("[UI]\nuseAlternateCursors=99999"));
        assert!(!on("[UI]\nuseAlternateCursors=yes"));
        assert!(!on("[UI]\nuseAlternateCursors="));
        assert!(!on("[UI]\nkeyScrollX=1"), "missing");
        assert!(!on(""));
    }

    #[test]
    fn the_alternate_set_is_fifty_further() {
        assert_eq!(number(NORMAL, false), 209);
        assert_eq!(number(NORMAL, true), 259);
        assert_eq!(number(u32::MAX, true), u32::MAX);
    }

    #[test]
    #[ignore = "needs ZT_DATA_DIR"]
    fn the_cursors_of_the_full_game_load() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR");
        for base in 200..=211 {
            for alternate in [false, true] {
                let cursor = load(Path::new(&dir), number(base, alternate)).unwrap();
                assert_eq!((cursor.width, cursor.height), (32, 32));
                assert!(cursor.rgba.chunks(4).any(|p| p[3] == 255));
            }
        }
        assert!(load(Path::new(&dir), 1).is_none());
    }
}
