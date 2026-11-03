//! The zoomed variants of the blitter, on synthetic frames that show where every pixel goes.

use super::*;
use crate::zoom::Zoom;
use std::collections::BTreeMap;

/// Where each written pixel of a 40 by 40 framebuffer is, with its palette index.
type Drawn = BTreeMap<(i64, i64), u8>;

/// An opaque `width` by `height` frame whose pixel (c, r) has the palette index
/// `1 + r * width + c` (so at most 255 pixels) and whose rows are one segment per pixel.
fn grid(width: u16, height: u16, y_offset: i16, x_offset: i16) -> Frame {
    let mut data = Vec::new();
    for field in [height, width, y_offset as u16, x_offset as u16, 0] {
        data.extend(field.to_le_bytes());
    }
    for r in 0..height {
        data.push(width as u8);
        for c in 0..width {
            data.extend([0, 1, (1 + r * width + c) as u8]);
        }
    }
    Frame::parse(&data).unwrap()
}

/// The colour of index `i` has the red code `i % 32` and the green code `i / 32`.
fn ramp() -> Palette {
    Palette {
        colors: (0..=255u16)
            .map(|i| {
                [
                    (i % 32 * 255).div_ceil(31) as u8,
                    (i / 32 * 255).div_ceil(63) as u8,
                    0,
                    255,
                ]
            })
            .collect(),
    }
}

fn drawn(image: &Framebuffer) -> Drawn {
    let mut out = Drawn::new();
    for (i, p) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[3] != 0 {
            out.insert(
                ((i % 40) as i64, (i / 40) as i64),
                (p[0] >> 3) + 32 * (p[1] >> 2),
            );
        }
    }
    out
}

fn draw(frame: &Frame, anchor: (i32, i32), mirror: bool, zoom: Zoom) -> Drawn {
    let mut image = Framebuffer::new(40, 40);
    frame.draw_zoomed(&ramp(), &mut image, anchor, mirror, None, zoom);
    drawn(&image)
}

/// The index of the pixel (c, r) of a `grid` of the given width.
fn index(width: i64, c: i64, r: i64) -> u8 {
    (1 + r * width + c) as u8
}

#[test]
fn normal_zoom_equals_the_plain_draws_at_the_same_anchor() {
    let frame = grid(5, 4, 3, 2);
    let palette = ramp();
    for mirror in [false, true] {
        let mut plain = Framebuffer::new(40, 40);
        let mut zoomed = Framebuffer::new(40, 40);
        // The anchor is (20, 15); the plain draws take the corner: anchor minus the offsets,
        // or, mirrored, anchor plus the x offset.
        match mirror {
            false => frame.draw(&palette, &mut plain, 18, 12),
            true => frame.draw_mirrored(&palette, &mut plain, 22, 12),
        }
        frame.draw_zoomed(&palette, &mut zoomed, (20, 15), mirror, None, Zoom::Normal);
        assert_eq!(plain.pixels, zoomed.pixels, "mirror {mirror}");
        assert_eq!(drawn(&plain).len(), 20);
    }
    // Mirrored, pixel (c, r) goes to (X + x_offset - c, Y - y_offset + r).
    let got = draw(&frame, (20, 15), true, Zoom::Normal);
    assert_eq!(got[&(22, 12)], index(5, 0, 0));
    assert_eq!(got[&(18, 15)], index(5, 4, 3));
}

#[test]
fn zoomed_in_every_pixel_is_a_2x2_block() {
    // Offsets: y 3, x 2; anchor (20, 15): pixel (c, r) fills x = 20 - 4 + 2c .. +1 and
    // y = 15 - 6 + 2r .. +1.
    let got = draw(&grid(5, 4, 3, 2), (20, 15), false, Zoom::In);
    assert_eq!(got.len(), 5 * 4 * 4);
    for r in 0..4 {
        for c in 0..5 {
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                assert_eq!(
                    got[&(16 + 2 * c + dx, 9 + 2 * r + dy)],
                    index(5, c, r),
                    "pixel ({c}, {r}) block ({dx}, {dy})"
                );
            }
        }
    }
}

#[test]
fn zoomed_in_mirrored_blocks_end_on_the_reflected_column() {
    // x = X + 2 * x_offset - 2c - 1 .. X + 2 * x_offset - 2c, same rows.
    let got = draw(&grid(5, 4, 3, 2), (20, 15), true, Zoom::In);
    assert_eq!(got.len(), 80);
    for r in 0..4 {
        for c in 0..5 {
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                assert_eq!(
                    got[&(23 - 2 * c + dx, 9 + 2 * r + dy)],
                    index(5, c, r),
                    "pixel ({c}, {r})"
                );
            }
        }
    }
}

#[test]
fn zoomed_in_works_with_negative_offsets() {
    // Offsets y -2, x -1: pixel (0, 0) is at (X + 2, Y + 4).
    let got = draw(&grid(2, 2, -2, -1), (10, 10), false, Zoom::In);
    assert_eq!(got.keys().next(), Some(&(12, 14)));
    assert_eq!(got[&(12, 14)], 1);
    assert_eq!(got[&(15, 17)], 4);
    assert_eq!(got.len(), 16);
}

#[test]
fn zoomed_out_keeps_the_columns_of_the_parity_of_the_x_offset() {
    // x offset 3 (odd): columns 1 and 3 and 5 are at an even distance, at X - 1, X, X + 1.
    // y offset 4: rows 0, 2 (height 5 gives 2 rows) at Y - 2, Y - 1.
    let got = draw(&grid(7, 5, 4, 3), (20, 20), false, Zoom::Out);
    let want: Drawn = [(1, 0, 19, 18), (3, 0, 20, 18), (5, 0, 21, 18)]
        .into_iter()
        .chain([(1, 2, 19, 19), (3, 2, 20, 19), (5, 2, 21, 19)])
        .map(|(c, r, x, y)| ((x, y), index(7, c, r)))
        .collect();
    assert_eq!(got, want);
    // Even x offset: columns 0, 2, 4, 6.
    let got = draw(&grid(7, 5, 4, 2), (20, 20), false, Zoom::Out);
    let columns: Vec<i64> = got.keys().filter(|k| k.1 == 18).map(|k| k.0).collect();
    assert_eq!(columns, [19, 20, 21, 22]);
    assert_eq!(got[&(19, 18)], index(7, 0, 0));
    assert_eq!(got[&(22, 19)], index(7, 6, 2));
}

#[test]
fn zoomed_out_mirrored_reflects_the_columns_about_the_anchor() {
    // x offset 3: source column c is at distance d = c - 3 and goes to X - d / 2.
    let got = draw(&grid(7, 5, 4, 3), (20, 20), true, Zoom::Out);
    assert_eq!(got.len(), 6);
    assert_eq!(got[&(21, 18)], index(7, 1, 0));
    assert_eq!(got[&(20, 18)], index(7, 3, 0));
    assert_eq!(got[&(19, 18)], index(7, 5, 0));
}

/// The source rows drawn by a zoomed out `grid` of the given height and y offset, and the
/// framebuffer row of the first one.
fn out_rows(height: u16, y_offset: i16) -> (Vec<i64>, i64) {
    let got = draw(&grid(1, height, y_offset, 0), (20, 20), false, Zoom::Out);
    let rows = got.values().map(|&v| i64::from(v) - 1).collect();
    (rows, got.keys().next().map_or(0, |k| k.1))
}

#[test]
fn zoomed_out_row_count_is_half_the_height_less_one_for_an_odd_offset_and_even_height() {
    // (height, y offset, rows used)
    let table: [(u16, i16, &[i64]); 10] = [
        (5, 0, &[0, 2]),    // odd height, even offset: the last candidate row is dropped
        (6, 0, &[0, 2, 4]), // even height, even offset
        (6, 1, &[1, 3]),    // even height, odd offset: one less than height / 2
        (5, 1, &[1, 3]),    // odd height, odd offset: height / 2
        (23, 21, &[1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21]),
        (1, 0, &[]),
        (1, 1, &[]),
        (2, 1, &[]),
        (2, 0, &[0]),
        (3, 1, &[1]),
    ];
    for (height, y_offset, want) in table {
        assert_eq!(
            out_rows(height, y_offset).0,
            want,
            "{height} rows, {y_offset}"
        );
    }
}

#[test]
fn zoomed_out_rows_go_to_the_anchor_minus_half_the_offset_rounded_toward_zero() {
    // Positive odd offset 3: row 1 is 2 above the anchor, at Y + (1 - 3) / 2 = Y - 1.
    assert_eq!(out_rows(6, 3).1, 19);
    // Even offsets, plain halves.
    assert_eq!(out_rows(6, 4).1, 18);
    assert_eq!(out_rows(6, -2).1, 21, "row 0 is 2 below the anchor");
    assert_eq!(out_rows(6, 0).1, 20);
    // Each following drawn row is one lower.
    let got = draw(&grid(1, 6, 4, 0), (20, 20), false, Zoom::Out);
    assert_eq!(got.keys().map(|k| k.1).collect::<Vec<_>>(), [18, 19, 20]);
}

#[test]
fn zoomed_out_negative_odd_offset_draws_one_pixel_higher() {
    // Offset -3, height 6: rows 1 and 3 are used. By the distance rule row 1 (4 below the
    // anchor) would go to Y + 2; the original puts it at Y - trunc(-3 / 2) = Y + 1.
    let (rows, first) = out_rows(6, -3);
    assert_eq!(rows, [1, 3]);
    assert_eq!(first, 21);
    // Offset -1: row 1 would go to Y + 1, it goes to Y.
    assert_eq!(out_rows(6, -1).1, 20);
    // The even offset next to it has no exception: offset -2, row 0 at Y + 1.
    assert_eq!(out_rows(6, -2).1, 21);
    // Mirrored the rows are the same.
    let got = draw(&grid(1, 6, -3, 0), (20, 20), true, Zoom::Out);
    assert_eq!(got.keys().map(|k| k.1).collect::<Vec<_>>(), [21, 22]);
}

#[test]
fn zoomed_out_example_of_an_odd_by_odd_frame() {
    // 47 by 24, offsets 25 across and 21 down: source columns 1, 3, ..., 45 go to X - 12 ..
    // X + 10 and source rows 1, 3, ..., 21 to Y - 10 .. Y.
    let mut image = Framebuffer::new(60, 40);
    let mut data = Vec::new();
    for field in [24u16, 47, 21, 25, 0] {
        data.extend(field.to_le_bytes());
    }
    for _ in 0..24 {
        data.extend([1, 0, 47]);
        data.extend([1; 47]);
    }
    let frame = Frame::parse(&data).unwrap();
    frame.draw_zoomed(&ramp(), &mut image, (30, 20), false, None, Zoom::Out);
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (i, p) in image.pixels.as_chunks::<4>().0.iter().enumerate() {
        if p[3] != 0 {
            xs.push(i % 60);
            ys.push(i / 60);
        }
    }
    assert_eq!(xs.len(), 23 * 11);
    assert_eq!((xs.iter().min(), xs.iter().max()), (Some(&18), Some(&40)));
    assert_eq!((ys.iter().min(), ys.iter().max()), (Some(&10), Some(&20)));
}

#[test]
fn dropped_rows_do_not_disturb_the_rows_after_them() {
    // Row 0 has two segments (columns 0 and 2), row 1 two others (columns 2 and 4), row 2 none.
    let rows = [2, 0, 1, 7, 1, 1, 8, 2, 2, 1, 9, 1, 1, 10, 0];
    let mut data = Vec::new();
    for field in [3u16, 5, 1, 0, 0] {
        data.extend(field.to_le_bytes());
    }
    data.extend(rows);
    let frame = Frame::parse(&data).unwrap();
    // y offset 1: the only row used is row 1, at Y; columns 2 and 4 at X + 1 and X + 2.
    let got = draw(&frame, (10, 10), false, Zoom::Out);
    assert_eq!(got, Drawn::from([((11, 10), 9), ((12, 10), 10)]));
    // Zoomed in, row 0 is drawn twice and row 1 still follows it.
    let got = draw(&frame, (10, 10), false, Zoom::In);
    assert_eq!(got[&(10, 8)], 7);
    assert_eq!(got[&(14, 9)], 8);
    assert_eq!(got[&(14, 10)], 9);
    assert_eq!(got[&(19, 11)], 10);
    assert_eq!(got.len(), 4 * 4);
}

#[test]
fn pixels_are_clipped_one_by_one_at_every_zoom() {
    let frame = grid(5, 4, 3, 2);
    for zoom in [Zoom::Out, Zoom::Normal, Zoom::In] {
        for mirror in [false, true] {
            let full = draw(&frame, (20, 20), mirror, zoom);
            // Slide the anchor so that parts fall off each side; the visible part is always the
            // full picture cut by the framebuffer.
            for (dx, dy) in [(-22, 0), (22, 0), (0, -22), (0, 22), (-30, -30)] {
                let got = draw(&frame, (20 + dx, 20 + dy), mirror, zoom);
                let want: Drawn = full
                    .iter()
                    .filter_map(|(&(x, y), &v)| {
                        let (x, y) = (x + i64::from(dx), y + i64::from(dy));
                        ((0..40).contains(&x) && (0..40).contains(&y)).then_some(((x, y), v))
                    })
                    .collect();
                assert_eq!(got, want, "{zoom:?} mirror {mirror} shift ({dx}, {dy})");
            }
        }
    }
    for zoom in [Zoom::Out, Zoom::In] {
        for anchor in [(i32::MIN, 0), (i32::MAX, i32::MAX), (0, i32::MIN)] {
            assert!(draw(&frame, anchor, false, zoom).is_empty());
        }
    }
}

#[test]
fn shadow_and_blended_frames_follow_the_same_placement() {
    // A shadow frame of 2 by 1 pixels, y offset 0 and x offset 0, drawn on grey.
    let mut data = Vec::new();
    for field in [0x8001u16, 2, 0, 0, 0] {
        data.extend(field.to_le_bytes());
    }
    data.extend([1, 0, 2]);
    let shadow = Frame::parse(&data).unwrap();
    let grey = || {
        let mut image = Framebuffer::new(8, 8);
        image.pixels.fill(99); // code 12
        image
    };
    let dark = show(0, 12 * 3 / 4);
    let mut image = grey();
    shadow.draw_zoomed(&ramp(), &mut image, (2, 2), false, None, Zoom::In);
    let darkened: Vec<(usize, usize)> = (0..64)
        .filter(|&i| image.pixels[i * 4] == dark)
        .map(|i| (i % 8, i / 8))
        .collect();
    assert_eq!(darkened.len(), 8, "2 pixels, each a 2x2 block");
    assert_eq!(darkened[0], (2, 2));
    assert_eq!(darkened[7], (5, 3));

    // Blended at 0.5, mirrored, zoomed in: red 200 over 100 on the 5-6-5 scale.
    let frame = grid(1, 1, 0, 0);
    let mut palette = ramp();
    palette.colors[1] = [200, 0, 0, 255];
    let mut image = grey();
    let blend = Blend {
        strength: 0.5,
        black: false,
    };
    frame.draw_zoomed(&palette, &mut image, (4, 2), true, Some(blend), Zoom::In);
    let mut plain = grey();
    frame.draw_blended(&palette, &mut plain, (4, 2), true, blend);
    // The 1x1 blended pixel at (4, 2) is the colour of every pixel of the 2x2 block at
    // (3..=4, 2..=3).
    for (x, y) in [(3, 2), (4, 2), (3, 3), (4, 3)] {
        assert_eq!(
            image.pixels[(y * 8 + x) * 4..][..4],
            plain.pixels[(2 * 8 + 4) * 4..][..4]
        );
    }
    assert_eq!(
        image.pixels[(2 * 8 + 5) * 4],
        99,
        "nothing beside the block"
    );
}
