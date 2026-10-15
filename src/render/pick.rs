//! Picking the entity under a point of the view (placing 7.1, sprite drawing 3.3 and 5.4): the
//! map is picked as it is drawn. The last entity in paint order that has a drawn pixel of its
//! frame under the point wins, the row searched over a width from the `cHitThreshold` of its type.

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
/// drawn with the anchor `anchor` at normal zoom, mirrored or not (placing 7.1, the pick). With
/// the width `w` the search starts at `point - trunc(w / 2)`, which must lie inside the frame; a
/// negative `w` then hits at once, else a stored pixel of the row from the column left of the
/// start to `w - 1` columns right of it (frame columns, so a mirrored picture is searched the
/// other way) is a hit, whatever its palette index.
pub(crate) fn hits(
    frames: &[&Frame],
    mirror: bool,
    anchor: (i64, i64),
    w: i32,
    point: (i64, i64),
) -> bool {
    let start = point.0.saturating_sub(i64::from(w) / 2);
    frames.iter().any(|frame| {
        let dx = start.saturating_sub(anchor.0);
        let col = match mirror {
            false => dx,
            true => dx.saturating_neg(),
        }
        .saturating_add(i64::from(frame.x_offset));
        let row = point
            .1
            .saturating_sub(anchor.1)
            .saturating_add(i64::from(frame.y_offset));
        let (width, height) = (i64::from(frame.width), i64::from(frame.height));
        if !(0..width).contains(&col) || !(0..height).contains(&row) {
            return false;
        }
        // The columns cut to the frame: a huge width costs no more than the frame's.
        let last = col.saturating_add(i64::from(w) - 1).min(width - 1);
        w < 0 || (col - 1..=last).any(|c| frame.pixel_at(c as i32, row as i32).is_some())
    })
}

impl Staged {
    /// The id of the entity under `point`, a point of the view space at normal zoom (the
    /// camera's `view_point`): the last one drawn that is hit and that `wanted` accepts. The
    /// width of the search is the `cHitThreshold` of the type plus `extra` (5 for the bulldozer
    /// and the gate tool, 0 for the selection). Only entities of the zoo that were drawn can be
    /// hit.
    pub fn pick(
        &self,
        point: (i64, i64),
        extra: i32,
        mut wanted: impl FnMut(u32) -> bool,
    ) -> Option<u32> {
        let it = self.placed.iter().zip(&self.hits).rev();
        it.filter_map(|((placed, b), hit)| Some((placed, b, (*hit)?)))
            .filter(|(_, b, hit)| {
                let start = point
                    .0
                    .saturating_sub(i64::from(hit.threshold.saturating_add(extra)) / 2);
                (b[0]..b[2]).contains(&start) && (b[1]..b[3]).contains(&point.1)
            })
            .find(|(placed, _, hit)| {
                let (anim, view, shown) = match &placed.pic {
                    Pic::Anim(anim, view, _, frame) => (anim, view, *frame),
                    Pic::Blended(anim, view, _) => (anim, view, 0),
                    _ => return false,
                };
                let Some(loaded) = anim.sprites.get(view.file) else {
                    return false;
                };
                let sprite = &loaded.0;
                let frames: Vec<&Frame> = sprite
                    .extra
                    .iter()
                    .chain(sprite.frames.get(shown))
                    .collect();
                let anchor = (i64::from(placed.x), i64::from(placed.y));
                let w = hit.threshold.saturating_add(extra);
                hits(&frames, view.mirror, anchor, w, point) && wanted(hit.id)
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

    /// Whether a point hits the frame placed with its anchor at (100, 50).
    fn hit(f: &Frame, mirror: bool, w: i32, point: (i64, i64)) -> bool {
        hits(&[f], mirror, (100, 50), w, point)
    }

    #[test]
    fn a_point_hits_a_stored_pixel_of_the_frame_and_nothing_else() {
        let f = frame(0, 0, 1);
        // With w 0 the one column searched is the one left of the point.
        assert!(hit(&f, false, 0, (102, 50)));
        assert!(hit(&f, false, 0, (103, 50)));
        // The start must lie in the frame: the last column is only found from the column left of
        // it.
        assert!(!hit(&f, false, 0, (104, 51)));
        assert!(hit(&f, false, 1, (103, 51)));
        // A gap, the other row's pixel and points outside the frame miss.
        assert!(!hit(&f, false, 0, (101, 50)));
        assert!(!hit(&f, false, 0, (100, 50)));
        assert!(!hit(&f, false, 0, (102, 51)));
        assert!(!hit(&f, false, 0, (105, 51)));
        assert!(!hit(&f, false, 0, (102, 49)));
        assert!(!hit(&f, false, 0, (102, 52)));
        // The offsets move the frame: the anchor is 2 columns and 1 row into it.
        let f = frame(2, 1, 1);
        assert!(hit(&f, false, 0, (100, 49)));
        assert!(hit(&f, false, 1, (101, 50)));
        assert!(!hit(&f, false, 1, (100, 50)));
    }

    #[test]
    fn any_stored_pixel_counts_whatever_its_palette_index() {
        let f = frame(0, 0, 0);
        assert!(hit(&f, false, 1, (103, 51)));
    }

    #[test]
    fn a_mirrored_frame_is_tested_where_it_is_drawn() {
        let f = frame(0, 0, 1);
        // Column k is drawn at 100 - k, and w 0 looks at the column left of the start.
        assert!(hit(&f, true, 0, (98, 50)));
        assert!(hit(&f, true, 0, (97, 50)));
        assert!(!hit(&f, true, 0, (96, 51)));
        assert!(!hit(&f, true, 0, (100, 50)));
        assert!(!hit(&f, true, 0, (101, 50)));
        // With an offset the frame reaches from `X + offset - (width - 1)` to `X + offset`.
        let f = frame(2, 0, 1);
        assert!(hit(&f, true, 0, (100, 50)));
        assert!(!hit(&f, true, 0, (102, 50)));
    }

    #[test]
    fn the_search_starts_in_the_frame_one_column_left_of_the_start_and_runs_w_columns() {
        let f = frame(0, 0, 1);
        // w 5 starts 2 left of the point: only the points 102 to 105 start in the frame.
        assert!(hit(&f, false, 5, (102, 50)));
        assert!(hit(&f, false, 5, (105, 50)));
        assert!(!hit(&f, false, 5, (101, 50)));
        assert!(!hit(&f, false, 5, (106, 50)));
        assert!(hit(&f, false, 5, (102, 51)));
        // w 1 starts at the point and also looks at the column left of it.
        assert!(!hit(&f, false, 1, (100, 50)));
        assert!(hit(&f, false, 1, (101, 50)));
        assert!(hit(&f, false, 1, (103, 50)));
        assert!(!hit(&f, false, 1, (103, 52)));
        // w 4 starts 2 left too, w 3 one left: the start 100 is the point 102 or 101.
        assert!(hit(&f, false, 4, (102, 50)));
        assert!(hit(&f, false, 3, (101, 50)));
        // Only the row of the point is searched.
        assert!(!hit(&f, false, 8, (104, 52)));
        // A mirrored frame is searched the other way: the start 99 is column 1 and the search
        // runs over columns 0 to 4, the pixel of row 51 (column 3) included.
        assert!(hit(&f, true, 5, (101, 51)));
        assert!(hit(&f, true, 5, (99, 50)));
        assert!(!hit(&f, true, 5, (103, 50)));
        assert!(!hit(&f, true, 5, (98, 50)));
    }

    #[test]
    fn a_huge_or_extreme_value_neither_hangs_nor_overflows() {
        let f = frame(0, 0, 1);
        assert!(hit(
            &f,
            false,
            i32::MAX,
            (100 + i64::from(i32::MAX / 2), 50)
        ));
        assert!(!hit(&f, false, i32::MAX, (100, 50)));
        for point in [
            (i64::MAX, 50),
            (i64::MIN, 50),
            (100, i64::MAX),
            (100, i64::MIN),
        ] {
            for w in [i32::MIN, -1, 0, 5, i32::MAX] {
                for mirror in [false, true] {
                    assert!(!hit(&f, mirror, w, point));
                }
            }
        }
    }

    #[test]
    fn a_negative_width_hits_any_start_point_inside_the_frame() {
        let f = frame(0, 0, 1);
        for w in [-1, -9, i32::MIN] {
            // The start is the point minus trunc(w / 2).
            let at = |x: i64| (x + i64::from(w / 2), 50);
            assert!(hit(&f, false, w, at(100)), "{w}");
            assert!(hit(&f, false, w, at(103)));
            assert!(hit(&f, false, w, (at(100).0, 51)));
            assert!(!hit(&f, false, w, at(104)));
            assert!(!hit(&f, false, w, at(99)));
            assert!(!hit(&f, false, w, (at(100).0, 52)));
            assert!(!hit(&f, false, w, (at(100).0, 49)));
        }
        // Mirrored the box is on the other side of the anchor.
        assert!(hit(&f, true, -1, (97, 50)));
        assert!(!hit(&f, true, -1, (101, 50)));
    }

    #[test]
    fn the_background_frame_counts_with_the_current_one() {
        let (current, background) = (frame(0, 0, 1), frame(0, -8i16 as u16, 1));
        let both = [&background, &current];
        assert!(hits(&both, false, (100, 50), 0, (102, 50)));
        assert!(hits(&both, false, (100, 50), 0, (102, 58)));
        assert!(!hits(&both, false, (100, 50), 0, (102, 51)));
    }

    #[test]
    fn the_threshold_of_a_type_is_read_from_its_plain_integers_and_is_0_when_missing() {
        let ty = |ini: &str| EntityType {
            class: "objects",
            type_name: b"tree".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(ini.as_bytes()).unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        };
        let read = |ini: &str| threshold(&ty(ini));
        let section = "[Characteristics/Integers]\n";
        assert_eq!(read(&format!("{section}cHitThreshold=-1\n")), -1);
        assert_eq!(read(&format!("{section}cHitThreshold = 12\n")), 12);
        assert_eq!(read(&format!("{section}cHitThreshold=lots\n")), 0);
        assert_eq!(read(&format!("{section}cPurchaseCost=5\n")), 0);
        assert_eq!(read("[Other]\ncHitThreshold=5\n"), 0);
        let mut bare = ty("");
        bare.config = None;
        assert_eq!(threshold(&bare), 0);
    }
}
