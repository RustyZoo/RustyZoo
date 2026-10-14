//! Positions, sizes and layers of the interface tree.
//!
//! [`Tree::resolve`] turns the words and numbers of the files into numbers for a window size:
//! sizes from the words (`whole`, `half`, `third`, `quarter`, `fitfont`) or from the picture,
//! positions from the words (`left`, `right`, `top`, `bottom`, `center`) or numbers, measured
//! from the anchor, or the stretch area between two anchors, or the layout, or the window. An
//! element waits until its layout and its anchors are resolved; whole passes over the tree repeat
//! until nothing waits. Anchors that never resolve (a cycle) are dropped after a pass that
//! resolves nothing, with a warning. The children of every layout are then sorted by `layer`, a
//! stable sort, so equal layers keep the order of the section names.
//!
//! The screen position of an element ([`Tree::screen`]) is computed when it is asked for, through
//! its anchors and layouts, so a moved layout carries its children along.

use super::layout::{Dim, Kind, Picture, Pos, Tree};

/// The numbers an element's words resolve to. Positions are relative to the reference (the
/// anchor, else the layout, else the window); a negative one counts from its far edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub x: i32,
    pub y: i32,
    pub alt_x: i32,
    pub alt_y: i32,
    pub dx: i32,
    pub dy: i32,
    /// The element positions are measured from: `anchor1`, if it names an element that
    /// resolved. The layout is the base when `None`.
    pub anchor: Option<usize>,
}

fn clamp(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

/// One axis of a position. `lo..hi` is the reference on this axis, `size` the element's, `rs` the
/// size of the reference, `words` the near and far word of the axis.
struct Axis {
    lo: i64,
    hi: i64,
    size: i64,
    rs: i64,
    words: [Pos; 2],
}

impl Axis {
    fn at(&self, p: Pos, percent: bool) -> i64 {
        let w = self.hi - self.lo;
        let [near, far] = self.words;
        // A word that comes out negative keeps its meaning: the screen position adds the size of
        // the reference back.
        let word = |v: i64| if v < 0 { v - self.rs } else { v };
        match p {
            Pos::Center => word((w - self.size) / 2 + self.lo),
            p if p == far => word(self.hi - self.size),
            p if p == near => self.lo,
            // A number, or a word of the other axis, which is the number of its code (far outside
            // the screen, or 0 as a percentage).
            p if percent => (w - self.size) * p.code().clamp(0, 100) / 100 + self.lo,
            p => p.code(),
        }
    }
}

impl Tree {
    /// Resolves every element for a window of `width` x `height` pixels, and sorts the children of
    /// every layout by layer. `line_height` gives the height of the font an element names with
    /// its `font` and `fontsize` numbers (`None`: the default font), for `dy=fitfont`.
    ///
    /// Everything is resolved again, so it serves for a new window size too.
    pub fn resolve(
        &mut self,
        width: i32,
        height: i32,
        mut line_height: impl FnMut(Option<(i32, i32)>) -> i32,
    ) {
        for element in &mut self.elements {
            element.placed = None;
        }
        let mut strict = true;
        loop {
            let (mut waiting, mut progress) = (0, false);
            for i in 0..self.elements.len() {
                if self.elements[i].placed.is_none() {
                    match self.place(i, (width, height), strict, &mut line_height) {
                        true => progress = true,
                        false => waiting += 1,
                    }
                }
            }
            if waiting == 0 || (!progress && !strict) {
                break;
            }
            if !progress {
                let first = self.elements.iter().find(|e| e.placed.is_none());
                self.warnings.push(format!(
                    "{waiting} elements never resolve (anchors in a cycle), first id {}: treated as unanchored",
                    first.map_or(0, |e| e.id)
                ));
                strict = false;
            }
        }
        self.sort_layers();
    }

    /// Resolves element `i` if its layout and anchors are, and tells whether it did. Without
    /// `strict`, anchors are ignored.
    fn place(
        &mut self,
        i: usize,
        window: (i32, i32),
        strict: bool,
        line_height: &mut impl FnMut(Option<(i32, i32)>) -> i32,
    ) -> bool {
        let e = &self.elements[i];
        if e.parent.is_some_and(|p| self.elements[p].placed.is_none()) {
            return false;
        }
        // An id that names no element is no anchor. The second counts with the first only.
        let anchor = |id: i32| {
            if strict && id != 0 {
                self.find(id)
            } else {
                None
            }
        };
        let first = anchor(e.anchor);
        let second = first.and_then(|_| anchor(e.anchor2));
        if first
            .into_iter()
            .chain(second)
            .any(|a| self.elements[a].placed.is_none())
        {
            return false;
        }
        let size_of = |a: usize| {
            let p = self.elements[a].placed.expect("placed");
            (i64::from(p.dx), i64::from(p.dy))
        };
        // The reference rectangle (left, top, right, bottom) and its size.
        let (l, t, r, b, rw, rh);
        match (first, second) {
            (Some(a), Some(c)) => {
                let (aw, ah) = size_of(a);
                let (ax, ay) = self.screen(a);
                let (cx, cy) = self.screen(c);
                let (left, right) = ordered(i64::from(ax) + aw, i64::from(cx));
                let (top, bottom) = ordered(i64::from(ay) + ah, i64::from(cy));
                (l, t, r, b) = (
                    left - i64::from(ax),
                    top - i64::from(ay),
                    right - i64::from(ax),
                    bottom - i64::from(ay),
                );
                (rw, rh) = (aw, ah);
            }
            (Some(a), None) => {
                (rw, rh) = size_of(a);
                (l, t, r, b) = (0, 0, rw, rh);
            }
            _ => {
                (rw, rh) = match e.parent {
                    Some(p) => size_of(p),
                    None => (i64::from(window.0), i64::from(window.1)),
                };
                (l, t, r, b) = (0, 0, rw, rh);
            }
        }
        let (w, h) = (r - l, b - t);
        let (box_w, box_h) = self.animation_box(i);
        let e = &self.elements[i];
        let dx = match e.dx {
            Dim::Num(0) => box_w,
            d => size(d, w),
        };
        let dy = match e.dy {
            Dim::Num(0) => box_h,
            // A layout has no font.
            Dim::FitFont => {
                let font = match e.kind {
                    Kind::Layout => 0,
                    _ => i64::from(line_height(e.look.font)),
                };
                font + 2 * i64::from(e.y_border)
            }
            d => size(d, h),
        };
        let x = Axis {
            lo: l,
            hi: r,
            size: dx,
            rs: rw,
            words: [Pos::Left, Pos::Right],
        };
        let y = Axis {
            lo: t,
            hi: b,
            size: dy,
            rs: rh,
            words: [Pos::Top, Pos::Bottom],
        };
        // A size still 0 becomes that of the largest image, after the positions are known.
        let (image_w, image_h) = self.image_size(i);
        let placed = Placed {
            x: clamp(x.at(e.x, e.x_is_percent)),
            y: clamp(y.at(e.y, e.y_is_percent)),
            alt_x: clamp(x.at(e.alt_x, false)),
            alt_y: clamp(y.at(e.alt_y, false)),
            dx: clamp(if dx == 0 { image_w } else { dx }),
            dy: clamp(if dy == 0 { image_h } else { dy }),
            anchor: first,
        };
        self.elements[i].placed = Some(placed);
        true
    }

    /// The box of the animation an element shows (its first `animation` value), else 0.
    fn animation_box(&self, i: usize) -> (i64, i64) {
        match self.elements[i].look.animation.first() {
            Some(Some(Picture::Animation(a))) => {
                let set = &self.pictures.animations[*a].set;
                (
                    i64::from(set.x1) - i64::from(set.x0),
                    i64::from(set.y1) - i64::from(set.y0),
                )
            }
            _ => (0, 0),
        }
    }

    /// The largest width and the largest height of the images an element shows (`normal`, the
    /// images of a button's states or of a status bar, `.tga` values of `animation`), else 0.
    fn image_size(&self, i: usize) -> (i64, i64) {
        let e = &self.elements[i];
        let states = match &e.kind {
            Kind::Button(b) => vec![b.highlighted, b.selected, b.grayed],
            Kind::StatusImage(l) | Kind::ImageSet(l) => l.images.clone(),
            _ => vec![],
        };
        e.look
            .animation
            .iter()
            .copied()
            .chain([e.look.normal])
            .chain(states)
            .flatten()
            .filter_map(|picture| match picture {
                Picture::Image(m) => Some(&self.pictures.images[m]),
                Picture::Animation(_) => None,
            })
            .fold((0, 0), |(w, h), image| {
                (w.max(image.width as i64), h.max(image.height as i64))
            })
    }

    /// The top left corner of an element in window coordinates: its current position (the
    /// alternative one if the element is switched to it) from the screen position of its anchor,
    /// else of its layout, else from (0, 0); a negative coordinate is counted from the far edge
    /// of the reference. An element not resolved is at (0, 0) of its base.
    pub fn screen(&self, i: usize) -> (i32, i32) {
        // The chain of bases up to the root, walked without recursion: a layout can hold a long
        // chain of anchors.
        let mut chain = vec![i];
        while let Some(base) = self.base(*chain.last().expect("not empty")) {
            if chain.len() > self.elements.len() {
                break;
            }
            chain.push(base);
        }
        let (mut sx, mut sy) = (0i64, 0i64);
        for &c in chain.iter().rev() {
            let e = &self.elements[c];
            let Some(p) = e.placed else { continue };
            let (cx, cy) = if e.alt {
                (p.alt_x, p.alt_y)
            } else {
                (p.x, p.y)
            };
            let reference = self.base(c).and_then(|b| self.elements[b].placed);
            if let Some(r) = reference {
                if cx < 0 {
                    sx += i64::from(r.dx);
                }
                if cy < 0 {
                    sy += i64::from(r.dy);
                }
            }
            sx += i64::from(cx);
            sy += i64::from(cy);
        }
        (clamp(sx), clamp(sy))
    }

    /// The element positions are measured from: the anchor, else the layout.
    fn base(&self, i: usize) -> Option<usize> {
        let e = &self.elements[i];
        e.placed.and_then(|p| p.anchor).or(e.parent)
    }

    /// The rectangle of an element on the screen: left, top, width, height.
    pub fn rect(&self, i: usize) -> (i32, i32, i32, i32) {
        let (x, y) = self.screen(i);
        let (dx, dy) = self.elements[i].placed.map_or((0, 0), |p| (p.dx, p.dy));
        (x, y, dx, dy)
    }

    /// Sorts the children of every layout by `layer`, ascending and stable.
    pub fn sort_layers(&mut self) {
        for i in 0..self.elements.len() {
            let mut children = std::mem::take(&mut self.elements[i].children);
            children.sort_by_key(|&c| self.elements[c].layer);
            self.elements[i].children = children;
        }
    }

    /// The elements in the order they are drawn: depth first, a layout before its children, the
    /// children in list order (by layer, after [`Tree::sort_layers`]).
    pub fn draw_order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.elements.len());
        let mut stack = if self.elements.is_empty() {
            vec![]
        } else {
            vec![0]
        };
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(self.elements[i].children.iter().rev());
        }
        order
    }
}

/// A size word of a reference `w` wide; truncating division.
fn size(d: Dim, w: i64) -> i64 {
    match d {
        Dim::Num(n) => i64::from(n),
        Dim::Whole => w,
        Dim::Half => w / 2,
        Dim::Third => w / 3,
        Dim::Quarter => w / 4,
        Dim::FitFont => d.code(),
    }
}

fn ordered(a: i64, b: i64) -> (i64, i64) {
    (a.min(b), a.max(b))
}

#[cfg(test)]
mod tests;
