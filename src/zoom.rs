//! The three zoom levels of the map view: the zoom value and the anchor of a tile piece (map
//! drawing 2.4, 4.4 and 8). The blitter of [`crate::sprite::Frame::draw_zoomed`] does the rest.
//! The view position of a world point is the camera's ([`crate::camera::zoom_view`]): the zoom
//! value is a factor there.

/// How the map is drawn: zoomed out (value -2), normal (0) or zoomed in (2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    Out,
    Normal,
    In,
}

impl Zoom {
    /// The level of a stored zoom value. Only "less than -1" and "greater than 1" are tested, so
    /// -1, 0 and 1 are all normal.
    pub fn from_value(value: i32) -> Zoom {
        match value {
            ..-1 => Zoom::Out,
            2.. => Zoom::In,
            _ => Zoom::Normal,
        }
    }
}

/// What is added to the view position of the tile's corner point to get its image origin
/// (map drawing 4.4), for a rotation and a zoom. The zoomed-out values hold an extra 16 that
/// [`piece_anchor`] takes off.
pub fn origin_offset(rotation: u8, zoom: Zoom) -> (i32, i32) {
    const TABLE: [[(i32, i32); 4]; 3] = [
        [(16, 0), (0, 8), (-16, 0), (0, -8)],
        [(0, -32), (-32, -16), (-64, -32), (-32, -48)],
        [(0, -64), (-64, -32), (-128, -64), (-64, -96)],
    ];
    TABLE[zoom as usize][usize::from(rotation % 4)]
}

/// The anchor of a tile piece (map drawing 8) whose normal-zoom offset from the image origin
/// `(ox, oy)` is `(x0, y0)` (32, 32 for the ground). The origin is the one of [`origin_offset`]
/// at the same zoom.
pub fn piece_anchor((ox, oy): (i32, i32), (x0, y0): (i32, i32), zoom: Zoom) -> (i32, i32) {
    match zoom {
        Zoom::Out => (ox - 16 + x0 / 2, oy - 16 + y0 / 2),
        Zoom::Normal => (ox + x0, oy + y0),
        Zoom::In => (ox + 2 * x0, oy + 2 * y0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::zoom_view;

    #[test]
    fn only_below_minus_one_and_above_one_change_the_size() {
        let levels = [-3, -2, -1, 0, 1, 2, 3].map(Zoom::from_value);
        use Zoom::*;
        assert_eq!(levels, [Out, Out, Normal, Normal, Normal, In, In]);
    }

    #[test]
    fn the_origin_offsets_are_16_plus_half_the_normal_ones_when_zoomed_out() {
        for r in 0..4 {
            let (nx, ny) = origin_offset(r, Zoom::Normal);
            let (ix, iy) = origin_offset(r, Zoom::In);
            assert_eq!((ix, iy), (2 * nx, 2 * ny), "rotation {r}");
            let (ox, oy) = origin_offset(r, Zoom::Out);
            assert_eq!((ox, oy), (16 + nx / 2, 16 + ny / 2), "rotation {r}");
        }
    }

    #[test]
    fn a_tile_piece_is_anchored_at_the_natural_position_at_every_zoom() {
        let corner = (100, 60);
        for r in 0..4 {
            let normal = piece_anchor(
                {
                    let (dx, dy) = origin_offset(r, Zoom::Normal);
                    (corner.0 + dx, corner.1 + dy)
                },
                (32, 32),
                Zoom::Normal,
            );
            // The corner point at normal zoom is `corner`; zoomed in, twice that; out, half.
            let at = |zoom: Zoom| {
                let (dx, dy) = origin_offset(r, zoom);
                let (cx, cy) = zoom_view(zoom as i32 * 2 - 2, corner);
                piece_anchor((cx as i32 + dx, cy as i32 + dy), (32, 32), zoom)
            };
            assert_eq!(at(Zoom::Normal), normal);
            assert_eq!(at(Zoom::In), (2 * normal.0, 2 * normal.1), "rotation {r}");
            assert_eq!(at(Zoom::Out), (normal.0 / 2, normal.1 / 2), "rotation {r}");
        }
        assert_eq!(piece_anchor((10, 20), (32, 32), Zoom::Out), (10, 20));
        assert_eq!(piece_anchor((10, 20), (32, 16), Zoom::In), (74, 52));
        assert_eq!(
            piece_anchor((10, 20), (33, 31), Zoom::Out),
            (10, 19),
            "trunc"
        );
    }
}
