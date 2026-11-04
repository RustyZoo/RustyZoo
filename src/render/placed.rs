//! The pictures of a staged map: a cached frame or an animation placed in view space, and how each
//! is drawn and compared.

use super::staged::blend;
use super::*;

/// A picture of the cache: a frame and its palette.
pub(super) type Flat = Rc<(Frame, Palette)>;

pub(super) enum Pic {
    Flat(Flat),
    /// A picture drawn with one colour in place of its palette: the highlight of a tile.
    Mark(Flat, Palette),
    /// A ground picture rendered at run time; its pixels have an alpha.
    Rendered(Rc<Picture>),
    /// A frame of an animation seen from a view, with the palette to draw it with in place of
    /// its own.
    Anim(Rc<Animation>, View, Option<Palette>, usize),
    /// An animation drawn blended with what is behind it.
    Blended(Rc<Animation>, View, Blend),
}

impl Pic {
    /// Whether both draw the same pixels: the cached pictures are the same copy.
    pub(super) fn same(&self, other: &Pic) -> bool {
        match (self, other) {
            (Pic::Flat(a), Pic::Flat(b)) => Rc::ptr_eq(a, b),
            (Pic::Mark(a, p), Pic::Mark(b, q)) => Rc::ptr_eq(a, b) && p == q,
            (Pic::Rendered(a), Pic::Rendered(b)) => Rc::ptr_eq(a, b),
            (Pic::Anim(a, v, p, f), Pic::Anim(b, w, q, g)) => {
                Rc::ptr_eq(a, b) && v == w && p == q && f == g
            }
            (Pic::Blended(a, v, x), Pic::Blended(b, w, y)) => Rc::ptr_eq(a, b) && v == w && x == y,
            _ => false,
        }
    }

    /// A number that is the same for pictures that are [`Pic::same`].
    pub(super) fn print(&self) -> u64 {
        let (kind, copy, frame) = match self {
            Pic::Flat(a) => (0, Rc::as_ptr(a) as usize, 0),
            Pic::Mark(a, _) => (1, Rc::as_ptr(a) as usize, 0),
            Pic::Rendered(a) => (2, Rc::as_ptr(a) as usize, 0),
            Pic::Anim(a, v, _, f) => (
                3,
                Rc::as_ptr(a) as usize,
                f * 4 + v.file * 2 + usize::from(v.mirror),
            ),
            Pic::Blended(a, v, _) => (
                4,
                Rc::as_ptr(a) as usize,
                v.file * 2 + usize::from(v.mirror),
            ),
        };
        [copy as u64, frame as u64]
            .iter()
            .fold(kind, |h, &n| mix(h, n))
    }
}

/// One step of a cheap hash.
pub(super) fn mix(hash: u64, n: u64) -> u64 {
    (hash.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95)
}

impl Placed {
    /// Whether both are the same picture at the same place.
    pub(super) fn same(&self, other: &Placed) -> bool {
        (self.x, self.y, self.piece, self.grid, self.lift)
            == (other.x, other.y, other.piece, other.grid, other.lift)
            && self.pic.same(&other.pic)
    }

    /// A number that is the same for pictures that are [`Placed::same`].
    pub(super) fn print(&self) -> u64 {
        let piece = self.piece.map_or([0; 4], |p| {
            [p.corner.0, p.corner.1, p.offset.0, p.offset.1].map(|n| n as u32 as u64 + 1)
        });
        [self.x, self.y, self.lift]
            .iter()
            .map(|&n| n as u32 as u64)
            .chain(piece)
            .chain([u64::from(self.grid), self.pic.print()])
            .fold(0, mix)
    }
}

/// A piece of a tile: where its tile's corner is in view space at normal zoom, and the offset of
/// the piece's anchor from the tile's image origin there (map drawing 8).
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Piece {
    pub(super) corner: (i32, i32),
    pub(super) offset: (i32, i32),
}

/// The frames of a view drawn for frame `frame`: its background frame, then the frame itself.
pub(super) fn shown_frames<'a>(
    anim: &'a Animation,
    view: &View,
    frame: usize,
) -> Option<impl Iterator<Item = &'a Frame>> {
    let sprite = &anim.sprites.get(view.file)?.0;
    Some(sprite.extra.iter().chain(sprite.frames.get(frame)))
}

/// A placed picture with the box `[left, top, right, bottom)` of the pixels it can cover.
pub(super) type Boxed = (Placed, [i64; 4]);

/// A picture placed in view space: `(x, y)` is its anchor at normal zoom.
pub(super) struct Placed {
    pub(super) x: i32,
    pub(super) y: i32,
    /// Set for the pieces of a tile, which have their own anchors at the other zooms.
    pub(super) piece: Option<Piece>,
    /// Set for a grid line that only a switched-on grid draws; it never counts in the bounds.
    pub(super) grid: bool,
    /// Pixels the picture is raised after the zoom (a face's `cFacesYOffset`): not scaled by it.
    pub(super) lift: i32,
    pub(super) pic: Pic,
}

/// Clamps to the range of the view-space coordinates the drawing uses.
pub(super) fn clamp32(n: i64) -> i32 {
    n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl Placed {
    /// The box `[left, top, right, bottom)` of the pixels the picture can cover, raised by its lift.
    pub(super) fn bounds(&self) -> Option<[i64; 4]> {
        if let Pic::Rendered(picture) = &self.pic {
            let left = i64::from(self.x) - i64::from(picture.x_offset);
            let top = i64::from(self.y) - i64::from(self.lift) - i64::from(picture.y_offset);
            let (w, h) = (
                i64::from(picture.image.width),
                i64::from(picture.image.height),
            );
            return Some([left, top, left + w, top + h]);
        }
        let one = |frame: &Frame, mirror: bool| {
            let (w, h) = (i64::from(frame.width), i64::from(frame.height));
            let (xo, yo) = (i64::from(frame.x_offset), i64::from(frame.y_offset));
            let left = match mirror {
                false => i64::from(self.x) - xo,
                true => i64::from(self.x) + xo - (w - 1),
            };
            let top = i64::from(self.y) - i64::from(self.lift) - yo;
            [left, top, left + w, top + h]
        };
        let (frames, mirror) = match &self.pic {
            Pic::Flat(flat) | Pic::Mark(flat, _) => return Some(one(&flat.0, false)),
            Pic::Rendered(_) => return None,
            Pic::Anim(anim, view, _, frame) => (shown_frames(anim, view, *frame)?, view.mirror),
            Pic::Blended(anim, view, _) => (shown_frames(anim, view, 0)?, view.mirror),
        };
        frames.map(|frame| one(frame, mirror)).reduce(union)
    }

    /// The anchor at the zoom value `zoom` (level `level`), in the view space of that zoom: the
    /// camera's conversion of the view position, and for a tile piece the origin offset and the
    /// anchor of map drawing 4.4 and 8.
    pub(super) fn anchor(&self, rotation: u8, zoom: i32, level: Zoom) -> (i64, i64) {
        let (x, y) = match self.piece {
            Some(Piece { corner, offset }) if level != Zoom::Normal => {
                // `piece_anchor` adds its origin unchanged, so the corner is added in i64 after.
                let (cx, cy) = zoom_view(zoom, corner);
                let (x, y) = piece_anchor(origin_offset(rotation, level), offset, level);
                (cx + i64::from(x), cy + i64::from(y))
            }
            _ => zoom_view(zoom, (self.x, self.y)),
        };
        (x, y - i64::from(self.lift))
    }

    /// The box `b` of the picture at normal zoom, lift included, moved to the zoomed `anchor`
    /// (which is raised by the lift as well): scaled about the anchor by the blitter, with two
    /// pixels to spare for its rounding.
    pub(super) fn zoomed_box(&self, b: &[i64; 4], anchor: (i64, i64), level: Zoom) -> [i64; 4] {
        let y = i64::from(self.y) - i64::from(self.lift);
        let rel = [
            b[0] - i64::from(self.x),
            b[1] - y,
            b[2] - i64::from(self.x),
            b[3] - y,
        ];
        let rel = match level {
            Zoom::Normal => {
                return [
                    rel[0] + anchor.0,
                    rel[1] + anchor.1,
                    rel[2] + anchor.0,
                    rel[3] + anchor.1,
                ];
            }
            Zoom::In => rel.map(|n| n * 2),
            Zoom::Out => rel.map(|n| n.div_euclid(2)),
        };
        [
            rel[0] + anchor.0 - 2,
            rel[1] + anchor.1 - 2,
            rel[2] + anchor.0 + 2,
            rel[3] + anchor.1 + 2,
        ]
    }

    /// Draws the picture with its anchor at `anchor` of the view whose top left pixel is
    /// (`dx`, `dy`), at the zoom `level`.
    pub(super) fn draw(
        &self,
        target: &mut Framebuffer,
        (dx, dy): (i32, i32),
        anchor: (i64, i64),
        level: Zoom,
    ) {
        let at = (
            clamp32(anchor.0 - i64::from(dx)),
            clamp32(anchor.1 - i64::from(dy)),
        );
        match &self.pic {
            Pic::Flat(flat) => flat.0.draw_zoomed(&flat.1, target, at, false, None, level),
            Pic::Mark(flat, palette) => flat.0.draw_zoomed(palette, target, at, false, None, level),
            Pic::Rendered(picture) => blend(&picture.image, target, at, picture, level),
            Pic::Anim(anim, view, palette, frame) => {
                anim.draw_zoomed(*view, *frame, target, at, palette.as_ref(), None, level)
            }
            Pic::Blended(anim, view, blend) => {
                anim.draw_zoomed(*view, 0, target, at, None, Some(*blend), level)
            }
        }
    }
}
