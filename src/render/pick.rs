//! Picking the entity under a point of the view (placing 7.1, sprite drawing 3.3 and 5.4): the
//! map is picked as it is drawn. The last entity in paint order that has a drawn pixel of its
//! frame under the point wins, widened by the `cHitThreshold` of its type.

use super::*;

/// What the staging keeps of the entity a picture was placed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Hit {
    pub(super) id: u32,
    pub(super) threshold: i32,
}

/// `cHitThreshold` of a type: plain `[Characteristics/Integers]`, 0 when missing.
pub(super) fn threshold(ty: &EntityType) -> i32 {
    ty.config
        .as_deref()
        .and_then(|c| c.section("Characteristics/Integers"))
        .and_then(|s| s.get("cHitThreshold"))
        .and_then(to_int)
        .unwrap_or(0)
}

/// Whether the point falls on one of the `frames` (the current frame and its background frame),
/// drawn with the anchor `anchor` at normal zoom, mirrored or not. The row under the point is
/// searched over `threshold` pixels (at least one) from `threshold / 2` left of the point for a
/// drawn pixel; a negative threshold takes any point inside the box of a frame. A palette whose
/// entry 0 is transparent leaves the pixels of value 0 out, as the drawing does.
pub(crate) fn hits(
    frames: &[&Frame],
    palette: &Palette,
    mirror: bool,
    anchor: (i64, i64),
    threshold: i32,
    point: (i64, i64),
) -> bool {
    let (from, count) = match threshold {
        ..0 => (point.0, 1),
        t => (point.0 - i64::from(t / 2), i64::from(t.max(1))),
    };
    frames.iter().any(|frame| {
        let row = point.1 - anchor.1 + i64::from(frame.y_offset);
        (from..from + count).any(|x| {
            let col = match mirror {
                false => x - anchor.0 + i64::from(frame.x_offset),
                true => anchor.0 + i64::from(frame.x_offset) - x,
            };
            let (Ok(col), Ok(row)) = (i32::try_from(col), i32::try_from(row)) else {
                return false;
            };
            match threshold < 0 {
                true => {
                    (0..i32::from(frame.width)).contains(&col)
                        && (0..i32::from(frame.height)).contains(&row)
                }
                false => frame
                    .pixel_at(col, row)
                    .is_some_and(|v| v != 0 || !palette.zero_is_transparent()),
            }
        })
    })
}

impl Staged {
    /// The id of the entity under `point`, a point of the view space at normal zoom (the
    /// camera's `view_point`): the last one drawn that is hit and that `wanted` accepts. Only
    /// entities of the zoo that were drawn can be hit.
    pub fn pick(&self, point: (i64, i64), mut wanted: impl FnMut(u32) -> bool) -> Option<u32> {
        let it = self.placed.iter().zip(&self.hits).rev();
        it.filter_map(|((placed, b), hit)| Some((placed, b, (*hit)?)))
            .filter(|(_, b, hit)| {
                let reach = i64::from(hit.threshold.max(0));
                (b[0] - reach..b[2] + reach).contains(&point.0) && (b[1]..b[3]).contains(&point.1)
            })
            .find(|(placed, _, hit)| {
                let (Pic::Anim(anim, view, _) | Pic::Blended(anim, view, _)) = &placed.pic else {
                    return false;
                };
                let Some(loaded) = anim.sprites.get(view.file) else {
                    return false;
                };
                let (sprite, palette) = (&loaded.0, &loaded.1);
                let frames: Vec<&Frame> =
                    sprite.extra.iter().chain(sprite.frames.first()).collect();
                let anchor = (i64::from(placed.x), i64::from(placed.y));
                let t = hit.threshold;
                hits(&frames, palette, view.mirror, anchor, t, point) && wanted(hit.id)
            })
            .map(|(_, _, hit)| hit.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of 4 by 2 pixels with the given offsets: drawn are (1, 0), (2, 0) and (3, 1); the
    /// last one has the palette index `last`.
    fn frame(x_offset: u16, y_offset: u16, last: u8) -> Frame {
        let mut data = Vec::new();
        for field in [2u16, 4, y_offset, x_offset, 0] {
            data.extend(field.to_le_bytes());
        }
        data.extend([1, 1, 2, 1, 1, 1, 3, 1, last]);
        Frame::parse(&data).unwrap()
    }

    fn palette(first: [u8; 3]) -> Palette {
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend([first[0], first[1], first[2], 255]);
        Palette::parse(&bytes).unwrap()
    }

    /// Whether a point hits the frame placed with its anchor at (100, 50).
    fn hit(f: &Frame, mirror: bool, threshold: i32, point: (i64, i64)) -> bool {
        hits(
            &[f],
            &palette([1, 2, 3]),
            mirror,
            (100, 50),
            threshold,
            point,
        )
    }

    #[test]
    fn a_point_hits_a_drawn_pixel_of_the_frame_and_nothing_else() {
        let f = frame(0, 0, 1);
        assert!(hit(&f, false, 0, (101, 50)));
        assert!(hit(&f, false, 0, (102, 50)));
        assert!(hit(&f, false, 0, (103, 51)));
        // A transparent pixel, the other row's pixel and points outside the frame miss.
        assert!(!hit(&f, false, 0, (100, 50)));
        assert!(!hit(&f, false, 0, (101, 51)));
        assert!(!hit(&f, false, 0, (104, 51)));
        assert!(!hit(&f, false, 0, (101, 49)));
        assert!(!hit(&f, false, 0, (101, 52)));
        // The offsets move the frame: the anchor is 2 columns and 1 row into it.
        let f = frame(2, 1, 1);
        assert!(hit(&f, false, 0, (99, 49)));
        assert!(hit(&f, false, 0, (101, 50)));
        assert!(!hit(&f, false, 0, (100, 50)));
    }

    #[test]
    fn a_mirrored_frame_is_tested_where_it_is_drawn() {
        let f = frame(0, 0, 1);
        assert!(hit(&f, true, 0, (99, 50)));
        assert!(hit(&f, true, 0, (98, 50)));
        assert!(hit(&f, true, 0, (97, 51)));
        assert!(!hit(&f, true, 0, (101, 50)));
        // With an offset the frame reaches from `X + offset - (width - 1)` to `X + offset`.
        let f = frame(2, 0, 1);
        assert!(hit(&f, true, 0, (101, 50)));
        assert!(!hit(&f, true, 0, (103, 50)));
    }

    #[test]
    fn the_threshold_searches_the_row_from_half_of_it_left_of_the_point() {
        let f = frame(0, 0, 1);
        // The pixel to the right is at 101: 0 and 1 look at the point alone.
        for t in [0, 1] {
            assert!(!hit(&f, false, t, (100, 50)), "{t}");
        }
        // 2 looks at 99 and 100, 3 at 99 to 101, 4 at 98 to 101.
        assert!(!hit(&f, false, 2, (100, 50)));
        assert!(hit(&f, false, 3, (100, 50)));
        assert!(hit(&f, false, 4, (100, 50)));
        assert!(!hit(&f, false, 4, (99, 50)));
        // It reaches to the left too: from 3 left of the point for 6 pixels, 102 to 107.
        assert!(hit(&f, false, 6, (105, 50)));
        assert!(!hit(&f, false, 6, (106, 50)));
        // Only the row of the point is searched.
        assert!(!hit(&f, false, 8, (100, 52)));
        // A huge threshold does not hang or overflow.
        assert!(!hit(&f, false, 1000, (100, 49)));
    }

    #[test]
    fn a_negative_threshold_takes_any_point_in_the_box_of_the_frame() {
        let f = frame(0, 0, 1);
        for t in [-1, -9, i32::MIN] {
            assert!(hit(&f, false, t, (100, 50)), "{t}");
            assert!(hit(&f, false, t, (103, 50)));
            assert!(hit(&f, false, t, (100, 51)));
            assert!(!hit(&f, false, t, (104, 50)));
            assert!(!hit(&f, false, t, (99, 50)));
            assert!(!hit(&f, false, t, (100, 52)));
            assert!(!hit(&f, false, t, (100, 49)));
        }
        // Mirrored the box is on the other side of the anchor.
        assert!(hit(&f, true, -1, (97, 50)));
        assert!(!hit(&f, true, -1, (101, 50)));
    }

    #[test]
    fn the_value_zero_is_transparent_only_under_the_marker_colour() {
        let f = frame(0, 0, 0);
        assert!(!hits(
            &[&f],
            &palette([255, 0, 255]),
            false,
            (100, 50),
            0,
            (103, 51)
        ));
        assert!(hits(
            &[&f],
            &palette([1, 2, 3]),
            false,
            (100, 50),
            0,
            (103, 51)
        ));
        // The box test does not look at pixels.
        assert!(hits(
            &[&f],
            &palette([255, 0, 255]),
            false,
            (100, 50),
            -1,
            (103, 51)
        ));
    }

    #[test]
    fn the_background_frame_counts_with_the_current_one() {
        let (current, background) = (frame(0, 0, 1), frame(0, -8i16 as u16, 1));
        let both = [&background, &current];
        let p = palette([1, 2, 3]);
        assert!(hits(&both, &p, false, (100, 50), 0, (101, 50)));
        assert!(hits(&both, &p, false, (100, 50), 0, (101, 58)));
        assert!(!hits(&both, &p, false, (100, 50), 0, (101, 51)));
    }
}
