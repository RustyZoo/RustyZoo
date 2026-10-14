use super::*;

pub(crate) const WHITE: [u8; 3] = [255, 255, 255];
pub(crate) const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// A 24-bit bitmap from rows of RGB pixels (top row first); a negative `height` field is top-down.
pub(crate) fn bmp24(rows: &[Vec<[u8; 3]>], top_down: bool) -> Vec<u8> {
    let (w, h) = (rows[0].len(), rows.len());
    let stride = (w * 3).div_ceil(4) * 4;
    let mut out = header(w, h, top_down, 24, 54, stride * h);
    let order: Vec<&Vec<[u8; 3]>> = match top_down {
        true => rows.iter().collect(),
        false => rows.iter().rev().collect(),
    };
    for row in order {
        let mut line: Vec<u8> = row.iter().flat_map(|p| [p[2], p[1], p[0]]).collect();
        line.resize(stride, 0);
        out.extend(line);
    }
    out
}

/// An 8-bit bitmap with the given colour table, bottom-up.
fn bmp8(rows: &[Vec<u8>], table: &[[u8; 3]]) -> Vec<u8> {
    let (w, h) = (rows[0].len(), rows.len());
    let stride = w.div_ceil(4) * 4;
    let mut out = header(w, h, false, 8, 54 + 1024, stride * h);
    for i in 0..256 {
        let c = table.get(i).copied().unwrap_or_default();
        out.extend([c[2], c[1], c[0], 0]);
    }
    for row in rows.iter().rev() {
        let mut line = row.clone();
        line.resize(stride, 0);
        out.extend(line);
    }
    out
}

fn header(w: usize, h: usize, top_down: bool, bits: u16, start: usize, size: usize) -> Vec<u8> {
    let mut out = b"BM".to_vec();
    out.extend(((start + size) as u32).to_le_bytes());
    out.extend([0; 4]);
    out.extend((start as u32).to_le_bytes());
    out.extend(40u32.to_le_bytes());
    out.extend((w as i32).to_le_bytes());
    out.extend((if top_down { -(h as i32) } else { h as i32 }).to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(bits.to_le_bytes());
    out.extend([0; 24]);
    out
}

fn sample() -> Vec<Vec<[u8; 3]>> {
    vec![
        vec![WHITE, RED, WHITE, WHITE],
        vec![WHITE, WHITE, BLUE, WHITE],
    ]
}

#[test]
fn colours_are_reduced_to_five_six_five() {
    assert_eq!(reduce(113, 79, 66), [106, 76, 65]);
    assert_eq!(reduce(255, 255, 255), WHITE);
    assert_eq!(reduce(255, 0, 128), [255, 0, 123]);
}

#[test]
fn a_bitmap_becomes_a_cut_sprite_with_a_palette() {
    let (frame, palette) = convert(&bmp24(&sample(), false)).unwrap();
    // Height 2, width 2, offsets (1, 1) from the centre (2, 1) and the box (1, 0); the first row
    // ends with a transparent pixel, hence its last segment of count 0.
    let lle = [
        2, 0, 2, 0, 1, 0, 1, 0, 0, 0, // header
        2, 0, 1, 1, 1, 0, // row 0: red, then one transparent pixel
        1, 1, 1, 0, // row 1: one transparent pixel, then blue
    ];
    assert_eq!(frame, Frame::parse(&lle).unwrap());
    // Ordered by interleaved bits: blue, red, white.
    let pal = [
        3, 0, 0, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 255, 255, 255,
    ];
    assert_eq!(palette, Palette::parse(&pal).unwrap());
}

#[test]
fn a_top_down_or_8_bit_bitmap_gives_the_same_picture() {
    let expected = convert(&bmp24(&sample(), false)).unwrap();
    let top_down = convert(&bmp24(&sample(), true)).unwrap();
    assert_eq!(expected, top_down);
    let indexes = vec![vec![0, 1, 0, 0], vec![0, 0, 2, 0]];
    let eight = convert(&bmp8(&indexes, &[WHITE, RED, BLUE])).unwrap();
    assert_eq!(compare(&expected, &eight), Ok(()));
}

#[test]
fn a_colour_equal_to_the_key_after_reduction_is_transparent() {
    // 4 and 8 both reduce to 0, so the pixel is the key.
    let rows = vec![vec![[8, 0, 0], [4, 0, 0], [0, 255, 0]]];
    let (frame, _) = convert(&bmp24(&rows, false)).unwrap();
    assert_eq!((frame.width, frame.height, frame.x_offset), (1, 1, -1));
}

#[test]
fn a_bitmap_without_visible_pixel_gives_an_empty_sprite() {
    let (frame, _) = convert(&bmp24(&vec![vec![WHITE; 3]; 3], false)).unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.x_offset, frame.y_offset),
        (0, 0, 0, 0)
    );
}

#[test]
fn long_runs_of_transparent_pixels_are_split_in_segments() {
    let mut row = vec![WHITE; 600];
    (row[0], row[599]) = (RED, RED);
    let (frame, palette) = convert(&bmp24(&[vec![WHITE; 600], row], false)).unwrap();
    assert_eq!((frame.width, frame.x_offset), (600, 300));
    let mut image = Framebuffer::new(600, 1);
    assert_eq!(frame.height, 1);
    frame.draw(&palette, &mut image, 0, 0);
    let visible: Vec<usize> = (0..600).filter(|&x| image.pixels[x * 4 + 3] != 0).collect();
    assert_eq!(visible, [0, 599]);
}

#[test]
fn too_many_colours_are_an_error() {
    let rows: Vec<Vec<[u8; 3]>> = (0..20)
        .map(|y| {
            (0..20)
                .map(|x| [x * 12, y * 12, (x + y) % 2 * 255])
                .collect()
        })
        .collect();
    assert!(
        convert(&bmp24(&rows, false))
            .unwrap_err()
            .contains("colours")
    );
}

#[test]
fn malformed_bitmaps_are_errors_not_panics() {
    let good = bmp24(&sample(), false);
    for len in 0..good.len() {
        assert!(convert(&good[..len]).is_err(), "prefix of {len} bytes");
    }
    let patch = |at: usize, bytes: &[u8]| {
        let mut data = good.clone();
        data[at..at + bytes.len()].copy_from_slice(bytes);
        convert(&data)
    };
    assert!(patch(0, b"XX").is_err());
    assert!(patch(30, &[1]).is_err(), "compressed");
    assert!(patch(28, &[16]).is_err(), "16 bits");
    assert!(patch(18, &[0; 4]).is_err(), "no width");
    assert!(patch(22, &[0; 4]).is_err(), "no height");
    assert!(patch(18, &[0xff; 4]).is_err(), "negative width");
    assert!(patch(18, &[0, 0, 0, 0x40]).is_err(), "huge width");
    assert!(patch(22, &[0, 0, 0, 0x80]).is_err(), "smallest height");
    assert!(patch(10, &[0xff; 4]).is_err(), "pixel data past the end");
    assert!(patch(14, &[12, 0, 0, 0]).is_err(), "old header");
}

#[test]
fn pictures_are_compared_by_size_offsets_and_pixels() {
    let one = convert(&bmp24(&sample(), false)).unwrap();
    assert_eq!(
        compare(&one, &convert(&bmp24(&sample(), true)).unwrap()),
        Ok(())
    );
    let mut moved = sample();
    moved[1][2] = RED;
    assert!(
        compare(&one, &convert(&bmp24(&moved, false)).unwrap())
            .unwrap_err()
            .contains("pixels")
    );
    let mut wider = sample();
    wider[0][3] = RED;
    assert!(
        compare(&one, &convert(&bmp24(&wider, false)).unwrap())
            .unwrap_err()
            .contains("box")
    );
}
