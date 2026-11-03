use super::blend::{code, darken};
use super::*;
use crate::vfs::Vfs;

/// Frame data: height (raw, may carry the shadow bit), width, offsets y = -3 and x = 5,
/// unknown 7, then the given row bytes.
fn frame_data(raw_height: u16, width: u16, rows: &[u8]) -> Vec<u8> {
    let mut data = Vec::new();
    for field in [raw_height, width, -3i16 as u16, 5, 7] {
        data.extend(field.to_le_bytes());
    }
    data.extend(rows);
    data
}

pub(crate) fn sized(data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_le_bytes().to_vec();
    out.extend(data);
    out
}

/// A sprite file with the given prefix flag (`None`: no prefix), frame count field and
/// frame records (already with their size fields). The palette path is `animals/Lion/lion.pal`.
pub(crate) fn sprite_file(prefix: Option<u8>, count: u32, frames: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(flag) = prefix {
        out.extend(b"FATZ\0\0\0\0");
        out.push(flag);
    }
    out.extend(125u32.to_le_bytes());
    let path = b"animals/Lion/lion.pal\0";
    out.extend((path.len() as u32).to_le_bytes());
    out.extend(path);
    out.extend(count.to_le_bytes());
    out.extend(frames.concat());
    out
}

/// Two rows of a 4 pixel wide frame. Row 0: skip 1, draw indexes 1 and 3 (3 pixels, it stops
/// short). Row 1: draw index 2, skip 1, draw index 0. The largest index, 3, is not the first
/// of its segment.
const ROWS: &[u8] = &[1, 1, 2, 1, 3, 2, 0, 1, 2, 1, 1, 0];
const PALETTE: [[u8; 4]; 5] = [
    [10, 10, 10, 255],
    [20, 0, 0, 255],
    [0, 30, 0, 255],
    [0, 0, 40, 255],
    [0, 0, 0, 0],
];

fn palette() -> Palette {
    Palette {
        colors: PALETTE.to_vec(),
    }
}

/// A palette colour as drawing writes it: reduced to 5-6-5 and shown.
fn shown([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let [r, g, b] = screen([r, g, b]);
    [r, g, b, a]
}

fn blank(width: u32, height: u32) -> Framebuffer {
    Framebuffer::new(width, height)
}

fn pixel(image: &Framebuffer, x: usize, y: usize) -> [u8; 4] {
    let at = (y * image.width as usize + x) * 4;
    image.pixels[at..at + 4].try_into().unwrap()
}

fn err_message<T: fmt::Debug>(result: Result<T, SpriteError>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn plain_file() {
    let frames = [
        sized(&frame_data(2, 4, ROWS)),
        sized(&frame_data(0, 0, &[])),
    ];
    let mut bytes = sprite_file(None, 2, &frames);
    bytes.extend(b"junk after the last frame");
    let sprite = Sprite::parse(&bytes).unwrap();
    assert_eq!(sprite.delay_ms, 125);
    assert_eq!(sprite.palette, "animals/Lion/lion.pal");
    assert_eq!(sprite.extra, None);
    assert_eq!(sprite.frames.len(), 2);
    let frame = &sprite.frames[0];
    assert_eq!((frame.width, frame.height), (4, 2));
    assert_eq!((frame.x_offset, frame.y_offset), (5, -3));
    assert_eq!(frame.unknown, 7);
    assert!(!frame.shadow);
    assert_eq!(frame.max_index, Some(3));
    assert!(sprite.needs_palette());
    // A frame of size 0 by 0 is valid and draws nothing.
    assert_eq!((sprite.frames[1].width, sprite.frames[1].height), (0, 0));
    assert_eq!(sprite.frames[1].max_index, None);
}

#[test]
fn fatz_prefix_with_and_without_extra_frame() {
    let one = sized(&frame_data(2, 4, ROWS));
    let extra = sized(&frame_data(1, 2, &[1, 0, 2, 5, 6]));

    let with = Sprite::parse(&sprite_file(Some(1), 1, &[one.clone(), extra])).unwrap();
    assert_eq!(with.frames.len(), 1);
    assert_eq!(
        with.extra.as_ref().map(|f| (f.width, f.height)),
        Some((2, 1))
    );
    assert_eq!(with.all_frames().count(), 2);

    let without = Sprite::parse(&sprite_file(Some(0), 1, &[one])).unwrap();
    assert_eq!((without.frames.len(), without.extra), (1, None));
}

#[test]
fn shadow_frame_has_no_pixel_bytes_and_darkens_the_destination() {
    // Height 2 with bit 15 set, 6 wide. Row 0: skip 1 and cover 2, skip 1 and cover 1.
    // Row 1: cover 3. Only skip and count bytes: a reader that took `count` pixel bytes
    // after each segment would lose its place.
    let rows = [2, 1, 2, 1, 1, 1, 0, 3];
    let sprite = Sprite::parse(&sprite_file(
        None,
        1,
        &[sized(&frame_data(0x8002, 6, &rows))],
    ))
    .unwrap();
    let frame = &sprite.frames[0];
    assert!(frame.shadow);
    assert_eq!((frame.width, frame.height), (6, 2));
    assert_eq!(frame.max_index, None);
    assert!(!sprite.needs_palette());

    let behind = shown([200, 100, 51, 255]);
    let mut image = blank(6, 2);
    for px in image.pixels.chunks_mut(4) {
        px.copy_from_slice(&behind);
    }
    frame.draw(&Palette::default(), &mut image, 0, 0);
    // Three quarters of the codes stay, rounded down: 24 -> 18, 24 -> 18, 6 -> 4. The alpha stays.
    let shadow = [show(0, 18), show(1, 18), show(2, 4), 255];
    for (x, y, expected) in [
        (0, 0, behind),
        (1, 0, shadow),
        (2, 0, shadow),
        (3, 0, behind),
        (4, 0, shadow),
        (5, 0, behind),
        (0, 1, shadow),
        (1, 1, shadow),
        (2, 1, shadow),
        (3, 1, behind),
    ] {
        assert_eq!(pixel(&image, x, y), expected, "pixel ({x}, {y})");
    }

    // The palette plays no part: a palette of one colour changes nothing.
    let mut again = blank(6, 2);
    again.pixels.copy_from_slice(&{
        let mut all = Vec::new();
        (0..12).for_each(|_| all.extend(behind));
        all
    });
    frame.draw(&palette(), &mut again, 0, 0);
    assert_eq!(again.pixels, image.pixels);

    // Over nothing there is nothing to darken.
    let mut empty = blank(6, 2);
    frame.draw(&palette(), &mut empty, 0, 0);
    assert_eq!(empty.pixels, [0; 48]);
}

/// A 4 by 2 picture of the grey 100 as the screen shows it: (99, 97, 99), the codes 12, 24, 12.
fn grey() -> Framebuffer {
    let mut image = blank(4, 2);
    for p in image.pixels.as_chunks_mut::<4>().0 {
        *p = [99, 97, 99, 255];
    }
    image
}

#[test]
fn blending_works_on_5_6_5_values() {
    // Red 100 is the code 12, green 100 the code 24; the source red 20 has the code 2.
    let mix = |src: [u8; 3], strength, black| {
        let mut pixel = [99, 97, 99, 77];
        super::blend::mix(&mut pixel, src, Blend { strength, black });
        pixel
    };
    // Half: (12 + 2) / 2 = 7, shown with its high bits repeated, 57.
    assert_eq!(mix([20, 0, 0], 0.5, false), [57, 48, 49, 77]);
    // A quarter of the sprite: ((12 + 2) / 2 + 12) / 2 = 9.
    assert_eq!(mix([20, 0, 0], 0.25, false), [74, 73, 74, 77]);
    assert_eq!(mix([20, 0, 0], 0.5, true), [49, 48, 49, 77]);
    // The code of a colour mixed with itself is kept.
    for v in 0..=255u8 {
        let [r, g, b] = screen([v, v, v]);
        let mut pixel = [r, g, b, 0];
        super::blend::mix(
            &mut pixel,
            [v, v, v],
            Blend {
                strength: 0.5,
                black: false,
            },
        );
        assert_eq!(pixel[..3], [r, g, b], "value {v}");
    }
    // Through a frame: the pixel with index 2 is green 30, the code 7: (24 + 7) / 2 = 15.
    let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
    let mut image = grey();
    let half = Blend {
        strength: 0.5,
        black: false,
    };
    frame.draw_blended(&palette(), &mut image, (0, 0), false, half);
    assert_eq!(pixel(&image, 0, 1), [49, 60, 49, 255]);
    assert_eq!(pixel(&image, 0, 0), [99, 97, 99, 255], "skipped");
    // A shadow frame (two pixels in the first row) halves the code.
    let shadow = Frame::parse(&frame_data(0x8002, 4, &[1, 0, 2, 0, 0])).unwrap();
    let mut image = grey();
    shadow.draw_blended(&palette(), &mut image, (0, 0), false, half);
    assert_eq!(pixel(&image, 1, 0), [49, 48, 49, 255]);
    assert_eq!(pixel(&image, 2, 0), [99, 97, 99, 255]);
}

#[test]
fn the_screen_reduces_to_codes_and_repeats_the_high_bits() {
    // Colours that were reduced before come out one code lower (map drawing 7.2).
    for (stored, shown) in [
        ([106, 76, 65], [99, 73, 57]),
        ([156, 113, 90], [148, 109, 82]),
        ([82, 56, 41], [74, 52, 33]),
        ([210, 168, 113], [206, 166, 107]),
    ] {
        assert_eq!(screen(stored), shown);
    }
    assert_eq!(screen([0, 0, 0]), [0, 0, 0]);
    assert_eq!(screen([255, 255, 255]), [255, 255, 255]);
    assert_eq!(
        screen([254, 254, 254]),
        [247, 251, 247],
        "254 gives the code 30, not 31"
    );
    for (c, shift) in SHIFT.into_iter().enumerate() {
        assert_eq!(u16::from(show(c, code(c, 255)) >> shift), code(c, 255));
    }
}

#[test]
fn darkening_strengths() {
    // A pixel of the codes (c, c, c), as the screen shows it: red and blue stop at 31.
    let at = |strength, code: u16| {
        let mut pixel = [show(0, code), show(1, code), show(2, code), 77];
        darken(&mut pixel, strength);
        pixel
    };
    for c in 0..=31u16 {
        let (q, h) = (at(0.25, c), at(0.5, c));
        assert_eq!(
            q,
            [
                show(0, c * 3 / 4),
                show(1, c * 3 / 4),
                show(2, c * 3 / 4),
                77
            ]
        );
        assert_eq!(h[0], show(0, c / 2), "half of {c}");
    }
    assert_eq!(at(0.25, 31)[0], show(0, 23), "floor(93 / 4), alpha kept");
    assert_eq!(at(0.25, 3)[0], show(0, 2), "floor(9 / 4)");
    assert_eq!(at(0.25, 0), [0, 0, 0, 77]);
    assert_eq!(at(0.5, 1)[0], 0);
    assert_eq!(at(0.75, 24)[0], show(0, 6));
    assert_eq!(at(0.0, 24)[0], show(0, 24));
    assert_eq!(at(1.0, 24), [0, 0, 0, 77]);
}

#[test]
fn a_file_without_frames_is_accepted() {
    let sprite = Sprite::parse(&sprite_file(None, 0, &[])).unwrap();
    assert_eq!((sprite.frames.len(), sprite.extra.as_ref()), (0, None));
    assert_eq!(sprite.all_frames().count(), 0);
    assert!(!sprite.needs_palette());
    assert!(sprite.check_palette(&Palette::default()).is_ok());
}

#[test]
fn bytes_after_the_last_row_of_a_frame_are_ignored() {
    let mut padded = frame_data(2, 4, ROWS);
    padded.extend([9, 9, 9, 9]);
    let sprite = Sprite::parse(&sprite_file(
        None,
        2,
        &[sized(&padded), sized(&frame_data(2, 4, ROWS))],
    ))
    .unwrap();
    assert_eq!(sprite.frames[0], sprite.frames[1]);
    assert_eq!(
        sprite.frames[0],
        Frame::parse(&frame_data(2, 4, ROWS)).unwrap()
    );
}

#[test]
fn draw_decodes_rows_and_leaves_skipped_pixels_alone() {
    let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
    let mut image = blank(4, 2);
    image.pixels.fill(99);
    frame.draw(&palette(), &mut image, 0, 0);
    assert_eq!(pixel(&image, 0, 0), [99; 4], "skipped");
    assert_eq!(pixel(&image, 1, 0), shown(PALETTE[1]));
    assert_eq!(pixel(&image, 2, 0), shown(PALETTE[3]));
    assert_eq!(pixel(&image, 3, 0), [99; 4], "the row stops short");
    assert_eq!(pixel(&image, 0, 1), shown(PALETTE[2]));
    assert_eq!(pixel(&image, 1, 1), [99; 4]);
    assert_eq!(pixel(&image, 2, 1), shown(PALETTE[0]));
}

#[test]
fn draw_clips_at_every_edge_and_far_outside() {
    let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
    let mut image = blank(2, 2);
    // The frame's pixel (1, 1) lands on (0, 0); its row 0 is off the framebuffer.
    frame.draw(&palette(), &mut image, -1, -1);
    assert_eq!(pixel(&image, 0, 0), [0; 4], "frame pixel (1,1) is skipped");
    assert_eq!(pixel(&image, 1, 0), shown(PALETTE[0]), "frame pixel (2,1)");
    assert_eq!(pixel(&image, 0, 1), [0; 4]);

    let mut image = blank(2, 2);
    // Row 0 of the frame lands on row 1; its pixel (2, 0) is clipped on the right.
    frame.draw(&palette(), &mut image, 0, 1);
    assert_eq!(
        pixel(&image, 1, 1),
        shown(PALETTE[1]),
        "row 0 of the frame on row 1"
    );
    assert_eq!(&image.pixels[..8], [0; 8], "row 0 untouched");

    for (x, y) in [
        (i32::MIN, 0),
        (i32::MAX, 0),
        (0, i32::MIN),
        (0, i32::MAX),
        (2, 2),
        (-4, -2),
    ] {
        let mut image = blank(2, 2);
        frame.draw(&palette(), &mut image, x, y);
        assert_eq!(image.pixels, [0; 16], "nothing visible at ({x}, {y})");
    }
    // A framebuffer without pixels is fine too.
    frame.draw(&palette(), &mut blank(0, 0), 0, 0);
}

#[test]
fn palette_alpha_is_ignored() {
    let frame = Frame::parse(&frame_data(1, 1, &[1, 0, 1, 1])).unwrap();
    // Half transparent and fully transparent colours are written opaque, replacing the
    // destination.
    for alpha in [128, 0] {
        let palette = Palette {
            colors: vec![[1, 2, 3, 255], [200, 10, 20, alpha]],
        };
        let mut image = blank(1, 1);
        image.pixels.copy_from_slice(&[0, 0, 100, 255]);
        frame.draw(&palette, &mut image, 0, 0);
        assert_eq!(
            pixel(&image, 0, 0),
            shown([200, 10, 20, 255]),
            "alpha {alpha}"
        );
        let mut image = blank(1, 1);
        frame.draw(&palette, &mut image, 0, 0);
        assert_eq!(
            pixel(&image, 0, 0),
            shown([200, 10, 20, 255]),
            "alpha {alpha}"
        );
    }
}

#[test]
fn value_zero_is_transparent_only_under_the_marker() {
    // One row, three pixels: values 0, 1, 0.
    let frame = Frame::parse(&frame_data(1, 3, &[1, 0, 3, 0, 1, 0])).unwrap();
    let with_entry_0 = |entry: [u8; 4]| Palette {
        colors: vec![entry, [9, 8, 7, 0]],
    };
    let draw = |palette: &Palette| {
        let mut image = blank(3, 1);
        image.pixels.fill(99);
        frame.draw(palette, &mut image, 0, 0);
        image
    };

    let marker = with_entry_0([255, 0, 255, 0]);
    assert!(marker.zero_is_transparent());
    let image = draw(&marker);
    assert_eq!(pixel(&image, 0, 0), [99; 4]);
    assert_eq!(pixel(&image, 1, 0), shown([9, 8, 7, 255]));
    assert_eq!(pixel(&image, 2, 0), [99; 4]);

    // The fourth byte of entry 0 plays no part: opaque magenta is the marker too.
    for alpha in [1, 128, 255] {
        let palette = with_entry_0([255, 0, 255, alpha]);
        assert!(palette.zero_is_transparent(), "alpha {alpha}");
        assert_eq!(draw(&palette).pixels, image.pixels, "alpha {alpha}");
    }

    // Any other entry 0 is a colour, whatever its alpha: transparent black, and magenta
    // with another red, green or blue.
    for entry in [
        [0, 0, 0, 0],
        [255, 0, 254, 0],
        [254, 0, 255, 255],
        [255, 1, 255, 0],
    ] {
        let palette = with_entry_0(entry);
        assert!(!palette.zero_is_transparent());
        let image = draw(&palette);
        let opaque = shown([entry[0], entry[1], entry[2], 255]);
        assert_eq!(pixel(&image, 0, 0), opaque, "entry {entry:?}");
        assert_eq!(pixel(&image, 2, 0), opaque);
    }
    assert!(!Palette::default().zero_is_transparent());
}

#[test]
fn mirrored_drawing_reflects_the_columns_about_x() {
    let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
    let mut image = blank(6, 2);
    // Pixel (c, r) of the frame goes to (4 - c, r): row 0 has values 1 and 3 at c = 1, 2.
    frame.draw_mirrored(&palette(), &mut image, 4, 0);
    assert_eq!(pixel(&image, 3, 0), shown(PALETTE[1]));
    assert_eq!(pixel(&image, 2, 0), shown(PALETTE[3]));
    assert_eq!(pixel(&image, 4, 0), [0; 4], "c = 0 is skipped");
    // Row 1: value 2 at c = 0, value 0 at c = 2.
    assert_eq!(pixel(&image, 4, 1), shown(PALETTE[2]));
    assert_eq!(pixel(&image, 2, 1), shown(PALETTE[0]));
    assert_eq!(pixel(&image, 5, 0), [0; 4]);

    // Clipped on the left: only c = 0 of row 1 lands on column 0.
    let mut image = blank(2, 2);
    frame.draw_mirrored(&palette(), &mut image, 0, 0);
    assert_eq!(pixel(&image, 0, 1), shown(PALETTE[2]));
    assert_eq!(
        image.pixels.iter().filter(|&&b| b != 0).count(),
        2,
        "one pixel"
    );
    // Clipped on the right: c = 1 of row 0 lands on column 5, c = 2 on column 6.
    let mut image = blank(6, 2);
    frame.draw_mirrored(&palette(), &mut image, 6, 0);
    assert_eq!(pixel(&image, 5, 0), shown(PALETTE[1]));
    assert_eq!(pixel(&image, 5, 1), [0; 4]);
    // Far outside nothing is drawn.
    for x in [i32::MIN, -1, 6 + 4, i32::MAX] {
        let mut image = blank(6, 2);
        frame.draw_mirrored(&palette(), &mut image, x, 0);
        assert_eq!(image.pixels, [0; 48], "x {x}");
    }
}

#[test]
fn a_value_the_palette_lacks_is_black_and_a_missing_palette_draws_nothing() {
    let frame = Frame::parse(&frame_data(2, 4, ROWS)).unwrap();
    let small = Palette {
        colors: PALETTE[..3].to_vec(),
    };
    // Value 3 is beyond the palette: opaque black, never an error. The others are as usual.
    let mut image = blank(4, 2);
    frame.draw(&small, &mut image, 0, 0);
    assert_eq!(pixel(&image, 1, 0), shown(PALETTE[1]));
    assert_eq!(pixel(&image, 2, 0), [0, 0, 0, 255]);
    assert_eq!(pixel(&image, 0, 1), shown(PALETTE[2]));
    // No palette at all: nothing is drawn.
    let mut image = blank(4, 2);
    frame.draw(&Palette::default(), &mut image, 0, 0);
    frame.draw_mirrored(&Palette::default(), &mut image, 3, 0);
    assert_eq!(image.pixels, [0; 32]);

    // The loader can still tell: the check names the first value that is missing.
    let message = err_message(frame.check_palette(&small));
    assert_eq!(message, "palette index 3 but the palette has 3 colours");
    assert!(frame.check_palette(&palette()).is_ok());
    let sprite = Sprite::parse(&sprite_file(None, 1, &[sized(&frame_data(2, 4, ROWS))])).unwrap();
    assert!(sprite.check_palette(&small).is_err());
    assert!(sprite.check_palette(&Palette::default()).is_err());
    assert!(sprite.check_palette(&palette()).is_ok());
}

#[test]
fn malformed_sprites_are_errors_with_a_message() {
    let good = sized(&frame_data(2, 4, ROWS));
    let file = sprite_file(Some(1), 1, &[good.clone(), good.clone()]);
    // Every truncation of a valid file is an error, never a panic.
    for len in 0..file.len() {
        let message = err_message(Sprite::parse(&file[..len]));
        assert!(!message.is_empty() && !message.contains('\n'), "{message}");
    }
    assert!(Sprite::parse(&file).is_ok());

    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        ("empty", vec![], "truncated: frame delay"),
        (
            "short prefix",
            b"FATZ\0\0".to_vec(),
            "truncated: FATZ prefix",
        ),
        (
            "frame count beyond the data",
            sprite_file(None, 2, std::slice::from_ref(&good)),
            "frame count 2 exceeds the data",
        ),
        (
            "huge frame count",
            sprite_file(None, u32::MAX, std::slice::from_ref(&good)),
            "frame count 4294967295 exceeds the data",
        ),
        (
            "missing extra frame",
            sprite_file(Some(1), 1, std::slice::from_ref(&good)),
            "extra frame: truncated: frame size",
        ),
        (
            "frame size beyond the data",
            sprite_file(None, 1, &[[&u32::MAX.to_le_bytes()[..], &[0; 10]].concat()]),
            "frame 0: truncated: frame data needs 4294967295 bytes",
        ),
        (
            "frame without header",
            sprite_file(None, 2, &[good.clone(), sized(&[1, 2, 3])]),
            "frame 1: truncated: frame width",
        ),
        (
            "row missing",
            sprite_file(None, 1, &[sized(&frame_data(2, 4, &[0]))]),
            "frame 0: truncated: row",
        ),
        (
            "segment cut",
            sprite_file(None, 1, &[sized(&frame_data(1, 4, &[1, 0]))]),
            "frame 0: truncated: segment",
        ),
        (
            "pixels cut",
            sprite_file(None, 1, &[sized(&frame_data(1, 4, &[1, 0, 3, 9]))]),
            "frame 0: truncated: segment pixels",
        ),
        (
            "row wider than the frame",
            sprite_file(None, 1, &[sized(&frame_data(1, 4, &[1, 2, 3, 1, 2]))]),
            "frame 0: row 0 draws past the frame width 4",
        ),
        (
            "skip alone past the width",
            sprite_file(None, 1, &[sized(&frame_data(1, 2, &[1, 3, 0]))]),
            "frame 0: row 0 draws past the frame width 2",
        ),
        (
            "segments that sum past u8",
            sprite_file(
                None,
                1,
                &[sized(&frame_data(
                    0x8001,
                    600,
                    &[3, 255, 255, 255, 255, 255, 255],
                ))],
            ),
            "frame 0: row 0 draws past the frame width 600",
        ),
        (
            "huge width",
            sprite_file(None, 1, &[sized(&frame_data(1, 4097, &[0]))]),
            "frame 0: frame size 4097x1 exceeds 4096",
        ),
        (
            "huge height",
            sprite_file(None, 1, &[sized(&frame_data(0x7fff, 1, &[]))]),
            "frame 0: frame size 1x32767 exceeds 4096",
        ),
        (
            "huge shadow height",
            sprite_file(None, 1, &[sized(&frame_data(0xffff, 1, &[]))]),
            "frame 0: frame size 1x32767 exceeds 4096",
        ),
    ];
    for (name, bytes, expected) in cases {
        let message = err_message(Sprite::parse(&bytes));
        assert!(message.contains(expected), "{name}: {message}");
    }
}

#[test]
fn a_tall_frame_without_rows_is_refused_without_allocating_for_it() {
    // 4096 rows are claimed, one byte is there: the error comes from the data, not from a
    // buffer sized after the header.
    let message = err_message(Frame::parse(&frame_data(4096, 4096, &[0])));
    assert!(message.contains("truncated: row"), "{message}");
}

#[test]
fn palettes() {
    let mut bytes = 2u32.to_le_bytes().to_vec();
    bytes.extend([1, 2, 3, 255, 4, 5, 6, 0]);
    let palette = Palette::parse(&bytes).unwrap();
    assert_eq!(palette.len(), 2);
    assert_eq!(palette.color(1), Some([4, 5, 6, 0]));
    assert_eq!(palette.color(2), None);

    // More bytes than the count says: the surplus is ignored.
    bytes.extend([7, 8, 9, 255]);
    assert_eq!(Palette::parse(&bytes).unwrap().len(), 2);
    // A count above the bytes present, ending in a partial entry: cut to what is there.
    bytes[0] = 9;
    bytes.extend([1, 2]);
    assert_eq!(Palette::parse(&bytes).unwrap().len(), 3);

    // At most 256 colours are kept, whatever the header claims.
    let mut big = u32::MAX.to_le_bytes().to_vec();
    big.extend(vec![0; 300 * 4]);
    assert_eq!(Palette::parse(&big).unwrap().len(), MAX_COLORS);

    assert_eq!(Palette::parse(&0u32.to_le_bytes()).unwrap().len(), 0);
    assert!(Palette::parse(&0u32.to_le_bytes()).unwrap().is_empty());
    assert!(err_message(Palette::parse(&[1, 0, 0])).contains("truncated"));
}

#[test]
fn overwrite_stays_within_both_palettes() {
    let source = Palette {
        colors: vec![[1; 4], [2; 4], [3; 4]],
    };
    let mut target = palette(); // 5 entries
    target.overwrite(3, &source, 1, usize::MAX);
    assert_eq!(target.colors[2..], [PALETTE[2], [2; 4], [3; 4]]);
    let mut target = palette();
    target.overwrite(0, &source, 0, 2);
    assert_eq!(target.colors[..3], [[1; 4], [2; 4], PALETTE[2]]);
    // Nothing to copy, or nowhere to put it: no change, no panic.
    for (start, skip) in [(9, 0), (usize::MAX, 0), (0, 9), (0, usize::MAX)] {
        let mut target = palette();
        target.overwrite(start, &source, skip, usize::MAX);
        assert_eq!(target, palette());
    }
}

#[test]
fn a_tint_averages_each_channel_on_5_6_5_values_and_keeps_the_marker() {
    let plain = Palette {
        colors: vec![
            [255, 0, 255, 0],
            [0, 0, 0, 255],
            [255, 255, 255, 255],
            [8, 100, 200, 9],
        ],
    };
    let tinted = plain.tinted([0, 225, 0]);
    assert!(tinted.zero_is_transparent());
    assert_eq!(tinted.color(0), plain.color(0));
    // The tint's codes are 0, 56 and 0. Black: green (0 + 56) / 2 = 28, shown as 28 << 2 | 1;
    // white: red and blue 15 (shown as 123), green 59 (shown as 239).
    // They are stored so that drawing reduces them to those codes.
    let drawn = |at| tinted.color(at).map(shown);
    assert_eq!(drawn(1), Some([0, 113, 0, 255]));
    assert_eq!(drawn(2), Some([123, 239, 123, 255]));
    assert_eq!(
        tinted.color(3).map(|c| c[3]),
        Some(9),
        "the fourth byte stays"
    );
    // Under a palette without the marker entry 0 is a colour like the others.
    let opaque = Palette {
        colors: vec![[255, 255, 255, 255]],
    };
    assert_ne!(opaque.tinted([0, 0, 0]).color(0), opaque.color(0));
    assert_eq!(Palette::default().tinted([1, 2, 3]), Palette::default());
}

#[test]
fn png_is_rgba_and_deterministic() {
    let mut image = blank(3, 2);
    image.fill_test_pattern();
    let (mut first, mut second) = (Vec::new(), Vec::new());
    write_png(&image, &mut first).unwrap();
    write_png(&image, &mut second).unwrap();
    assert_eq!(first, second);

    let mut reader = png::Decoder::new(std::io::Cursor::new(&first))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!((info.width, info.height), (3, 2));
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!(pixels, image.pixels);

    assert!(write_png(&blank(0, 0), Vec::new()).is_err());
}

#[test]
fn sprite_members_have_no_extension() {
    assert!(is_sprite_member("animals/lion/f/walk/e"));
    assert!(is_sprite_member("n"));
    assert!(!is_sprite_member("animals/lion/f/walk/walk.ani"));
    assert!(!is_sprite_member("animals/lion.d/n.pal"));
    assert!(is_sprite_member("animals/lion.d/n"));
    assert!(!is_sprite_member("animals/lion/"));
    assert!(!is_sprite_member(""));
}

/// An install folder with the given members in one archive, removed on drop.
pub(crate) struct Install(pub std::path::PathBuf);

impl Install {
    pub(crate) fn new(name: &str, members: &[(&str, Vec<u8>)]) -> Install {
        let dir = std::env::temp_dir().join(format!(
            "rustyzoo-sprite-{}-{}-{name}",
            std::process::id(),
            {
                static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("zoo.ini"), "[resource]\npath=.\n").unwrap();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, content) in members {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content).unwrap();
        }
        std::fs::write(dir.join("a.ztd"), zip.finish().unwrap().into_inner()).unwrap();
        Install(dir)
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A sprite file with one frame and the given palette path.
fn file_naming(path: &str, rows_raw_height: u16, rows: &[u8]) -> Vec<u8> {
    let mut out = 125u32.to_le_bytes().to_vec();
    out.extend((path.len() as u32).to_le_bytes());
    out.extend(path.as_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend(sized(&frame_data(rows_raw_height, 2, rows)));
    out
}

#[test]
fn load_finds_the_palette_and_applies_the_empty_path_rule() {
    let mut palette = 4u32.to_le_bytes().to_vec();
    palette.extend(PALETTE[..4].iter().flatten());
    let mut short = 2u32.to_le_bytes().to_vec();
    short.extend(PALETTE[..2].iter().flatten());
    let draws_3 = [1, 0, 2, 3, 3]; // one row: draws indexes 3 and 3
    let shadow = [1, 0, 2]; // one row: covers 2 pixels
    let install = Install::new(
        "load",
        &[
            ("pals/Main.PAL", palette),
            ("pals/short.pal", short),
            ("ok/n", file_naming("PALS\\main.pal", 1, &draws_3)),
            ("short/n", file_naming("pals/short.pal", 1, &draws_3)),
            ("missing/n", file_naming("pals/none.pal", 1, &draws_3)),
            ("nameless/n", file_naming("", 1, &draws_3)),
            ("shadow/n", file_naming("", 0x8001, &shadow)),
            (
                "shadow-named/n",
                file_naming("pals/none.pal", 0x8001, &shadow),
            ),
            ("bad-palette.pal", vec![1]),
            ("bad/n", file_naming("bad-palette.pal", 1, &draws_3)),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();

    let (sprite, palette, warning) = load(&vfs, "OK/n").unwrap();
    assert_eq!((sprite.frames.len(), palette.len(), warning), (1, 4, None));

    // A defective palette is a warning, not a failure; a missing or unreadable one is empty.
    let warning = |member: &str| {
        let (_, palette, warning) = load(&vfs, member).unwrap();
        (palette.len(), warning.unwrap())
    };
    assert_eq!(
        warning("short/n"),
        (
            2,
            "palette index 3 but the palette has 2 colours".to_string()
        )
    );
    let (len, message) = warning("missing/n");
    assert!(
        len == 0 && message.contains("no such member: pals/none.pal"),
        "{message}"
    );
    let (len, message) = warning("nameless/n");
    assert_eq!(
        (len, message.as_str()),
        (0, "palette index 3 but the palette has 0 colours")
    );
    let (len, message) = warning("bad/n");
    assert!(
        len == 0 && message.contains("palette \"bad-palette.pal\": truncated"),
        "{message}"
    );
    // Only a sprite file that cannot be found fails.
    assert!(err_message(load(&vfs, "nope/n")).contains("no such member: nope/n"));

    // No palette index drawn and no palette named: an empty palette, no warning.
    let (sprite, palette, warning) = load(&vfs, "shadow/n").unwrap();
    assert!(sprite.frames[0].shadow && palette.is_empty() && warning.is_none());
    // A named palette that is missing is a warning even when nothing needs it.
    let (_, _, warning) = load(&vfs, "shadow-named/n").unwrap();
    assert!(warning.unwrap().contains("no such member"));
}

/// The facts below were measured on Zoo Tycoon: Complete Collection.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_facts() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let load = |member: &str| {
        let (sprite, palette, warning) = super::load(&vfs, member).unwrap();
        assert_eq!(warning, None);
        (sprite, palette)
    };

    let (lion, palette) = load("animals/lion/f/walk/E");
    assert_eq!(lion.frames.len(), 23);
    assert_eq!(lion.extra, None);
    assert_eq!(lion.delay_ms, 83);
    assert_eq!(lion.palette, "animals/lion/lion.pal");
    let first = &lion.frames[0];
    assert_eq!((first.width, first.height), (47, 24));
    assert_eq!((first.x_offset, first.y_offset), (25, 21));
    assert!(!first.shadow);
    assert_eq!(palette.len(), 256);
    let mut image = Framebuffer::new(47, 24);
    first.draw(&palette, &mut image, 0, 0);
    assert!(image.pixels.chunks(4).any(|px| px[3] == 255));

    let (icon, _) = load("animals/lion/icflion/N");
    assert_eq!(icon.frames.len(), 1);
    assert_eq!((icon.frames[0].width, icon.frames[0].height), (44, 32));

    let (aviary, _) = load("objects/aviary/idle/NE");
    assert_eq!(aviary.frames.len(), 21);
    let extra = aviary.extra.as_ref().unwrap();
    assert_eq!((extra.width, extra.height), (407, 320));

    let (swim, _) = load("animals/asianele/m/ssurfswi/e");
    assert_eq!(swim.frames.len(), 34);
    assert!(swim.frames.iter().all(|frame| frame.shadow));
    assert_eq!((swim.frames[0].width, swim.frames[0].height), (86, 18));
}
