//! A staged map: its pictures in paint order, and drawing any window of it at any zoom, whole or in
//! parts.

use super::placed::{Boxed, clamp32};
use super::*;

/// A map ready to be drawn from one rotation: its pictures placed in view space at normal zoom,
/// in paint order, each with the box it can cover. Any window of the view, at any zoom, is drawn
/// from it.
pub struct Staged {
    pub(super) rotation: u8,
    /// The pictures of the tiles, which the renderer keeps for the next staging.
    pub(super) grounds: Rc<Grounds>,
    /// The paint order.
    pub(super) items: Vec<Entry>,
    /// The pictures of the items that are not tiles, in paint order.
    pub(super) own: Vec<Boxed>,
    /// For each picture of `own`, the entity of the zoo it was placed for (picking).
    pub(super) hits: Vec<Option<pick::Hit>>,
    /// The pictures of `own` that are picked but not drawn (a building without its picture).
    pub(super) unseen: Vec<usize>,
    /// The pictures of `own` that play an idle animation, with the entity whose clock gives
    /// their frame.
    pub(super) idle: Vec<(usize, u32)>,
    /// The box `[left, top, right, bottom)` of every tile.
    pub(super) tile_bounds: [i64; 4],
    /// The box of every tile and every picture.
    pub(super) bounds: [i64; 4],
    pub summary: Summary,
}

impl Staged {
    /// Every picture with its box, in paint order.
    pub(super) fn pictures(&self) -> impl Iterator<Item = &Boxed> {
        self.items.iter().flat_map(|&e| match e {
            Entry::Tile(t) => self.grounds.placed[t].0.iter(),
            Entry::Own(i) => std::slice::from_ref(&self.own[i]).iter(),
        })
    }

    /// The size of the picture of the whole map at normal zoom, or an error if it is empty or
    /// too big.
    pub fn size(&self) -> Result<(u32, u32), RenderError> {
        let (w, h) = check_size(&self.bounds)?;
        Ok((w as u32, h as u32))
    }

    /// The view-space point of the top left pixel of the picture of the whole map at normal zoom.
    #[cfg(test)]
    pub(crate) fn origin(&self) -> (i32, i32) {
        (self.bounds[0] as i32, self.bounds[1] as i32)
    }

    /// The box `[left, top, right, bottom)` of the whole map in the view space of the zoom value
    /// `zoom`.
    pub(crate) fn bounds_at(&self, zoom: i32) -> [i64; 4] {
        let level = Zoom::from_value(zoom);
        if level == Zoom::Normal {
            return self.bounds;
        }
        // The tiles scale by the zoom factor, with two pixels to spare.
        let scale = |n: i64| match zoom {
            ..=-2 => n.div_euclid(-i64::from(zoom)),
            _ => n.saturating_mul(i64::from(zoom)),
        };
        let [l, t, r, b] = self.tile_bounds.map(scale);
        let tiles = [l - 2, t - 2, r + 2, b + 2];
        self.pictures()
            .filter(|(p, _)| !p.grid)
            .fold(tiles, |all, (placed, b)| {
                let anchor = placed.anchor(self.rotation, zoom, level);
                union(all, placed.zoomed_box(b, anchor, level))
            })
    }

    /// Whether a picture of the entity `id`, drawn or not, meets the window of `size` pixels whose
    /// top left is the view point `view` at `zoom`, enlarged as the tiles' visibility test is (map
    /// drawing 9.3): by 50 pixels (25 zoomed in, 100 out) and at the top by 275 more (500 out,
    /// none in).
    pub(crate) fn in_view(
        &self,
        id: u32,
        (width, height): (u32, u32),
        view: (i32, i32),
        zoom: i32,
    ) -> bool {
        let level = Zoom::from_value(zoom);
        let (side, top) = match level {
            Zoom::In => (25, 0),
            Zoom::Normal => (50, 275),
            Zoom::Out => (100, 500),
        };
        let (left, up) = (i64::from(view.0) - side, i64::from(view.1) - side - top);
        let (right, down) = (
            i64::from(view.0) + i64::from(width) + side,
            i64::from(view.1) + i64::from(height) + side,
        );
        let ours = self.own.iter().zip(&self.hits);
        ours.filter(|(_, hit)| hit.is_some_and(|h| h.id == id))
            .any(|((placed, b), _)| {
                let anchor = placed.anchor(self.rotation, zoom, level);
                let b = placed.zoomed_box(b, anchor, level);
                b[0] < right && b[2] > left && b[1] < down && b[3] > up
            })
    }

    /// Draws the window of the size of `target` whose top left pixel is the point (`view_x`,
    /// `view_y`) of the view space at the zoom value `zoom` (-2, 0 or 2; map drawing 2.4): black,
    /// then every picture whose box meets the window, in paint order, the grid lines of the tiles
    /// only when `grid` is set (those a cliff forces are always drawn). The anchors are the camera's
    /// view positions at the zoom, the tile pieces' those of map drawing 8, and the blitter has
    /// the variant of the zoom level.
    pub(crate) fn draw(
        &self,
        target: &mut Framebuffer,
        view_x: i32,
        view_y: i32,
        zoom: i32,
        grid: bool,
    ) {
        target.fill([0, 0, 0, 255]);
        let level = Zoom::from_value(zoom);
        let (left, top) = (i64::from(view_x), i64::from(view_y));
        let right = left + i64::from(target.width);
        let bottom = top + i64::from(target.height);
        for (placed, b) in self.pictures().filter(|(p, _)| grid || !p.grid) {
            let anchor = placed.anchor(self.rotation, zoom, level);
            let b = placed.zoomed_box(b, anchor, level);
            if b[0] < right && b[2] > left && b[1] < bottom && b[3] > top {
                placed.draw(target, (view_x, view_y), anchor, level);
            }
        }
    }

    /// The other pictures, in paint order, with the number of tiles before each (`grid` as in
    /// [`Staged::draw`]), by their print.
    pub(super) fn others(&self, grid: bool) -> Vec<(u64, usize, usize)> {
        let mut tiles = 0;
        let mut found = Vec::new();
        for &e in &self.items {
            match e {
                Entry::Tile(_) => tiles += 1,
                Entry::Own(i) if grid || !self.own[i].0.grid => {
                    found.push((self.own[i].0.print(), tiles, i));
                }
                Entry::Own(_) => {}
            }
        }
        found
    }

    /// The rectangles `[left, top, right, bottom)` of a window of `size` pixels, whose top left is
    /// the view point `view` at `zoom`, that look different drawn from `self` than from the
    /// staging `old` of the same map, which was drawn there before: the boxes of the pictures
    /// that are in one and not in the other. `None` when that cannot be told (another rotation or
    /// ground, pictures that changed their order) or the rectangles cover too much to be worth it.
    pub(crate) fn changes(
        &self,
        old: &Staged,
        (width, height): (u32, u32),
        (view_x, view_y): (i32, i32),
        zoom: i32,
        grid: bool,
    ) -> Option<Vec<[i32; 4]>> {
        let tiles = |s: &Staged| -> Vec<usize> {
            (s.items.iter())
                .filter_map(|&e| match e {
                    Entry::Tile(t) => Some(t),
                    Entry::Own(_) => None,
                })
                .collect()
        };
        if self.rotation != old.rotation
            || !Rc::ptr_eq(&self.grounds, &old.grounds)
            || tiles(self) != tiles(old)
        {
            return None;
        }
        let (before, after) = (old.others(grid), self.others(grid));
        let key = |&(print, tiles, _): &(u64, usize, usize)| (print, tiles);
        let (mut sorted_before, mut sorted_after) = (before.clone(), after.clone());
        sorted_before.sort_unstable();
        sorted_after.sort_unstable();
        // The pictures of each side that are on the other one too.
        let (mut kept_before, mut kept_after) =
            (vec![false; old.own.len()], vec![false; self.own.len()]);
        old.unseen.iter().for_each(|&i| kept_before[i] = true);
        self.unseen.iter().for_each(|&i| kept_after[i] = true);
        let (mut i, mut j) = (0, 0);
        while i < sorted_before.len() && j < sorted_after.len() {
            let (b, a) = (&sorted_before[i], &sorted_after[j]);
            match key(b).cmp(&key(a)) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    if old.own[b.2].0.same(&self.own[a.2].0) {
                        (kept_before[b.2], kept_after[a.2]) = (true, true);
                    }
                    (i, j) = (i + 1, j + 1);
                }
            }
        }
        // Pictures that stay must stay in the same order, or what covers what changes.
        let kept = |all: &[(u64, usize, usize)], flags: &[bool]| -> Vec<(u64, usize)> {
            all.iter().filter(|e| flags[e.2]).map(key).collect()
        };
        if kept(&before, &kept_before) != kept(&after, &kept_after) {
            return None;
        }
        let mut rects: Vec<[i32; 4]> = Vec::new();
        for (s, flags) in [(old, &kept_before), (self, &kept_after)] {
            for (i, (placed, b)) in s.own.iter().enumerate() {
                if flags[i] || (placed.grid && !grid) {
                    continue;
                }
                let rect = self.window_rect(placed, b, (width, height), (view_x, view_y), zoom);
                if rect[0] < rect[2] && rect[1] < rect[3] {
                    rects.push(rect);
                    if rects.len() > MAX_CHANGES {
                        return None;
                    }
                }
            }
        }
        worth_redrawing(rects, (width, height))
    }

    /// The box of a picture in the window of `size` pixels whose top left is the view point
    /// `view` at `zoom`, clamped to the window (empty when it is outside).
    fn window_rect(
        &self,
        placed: &Placed,
        b: &[i64; 4],
        (width, height): (u32, u32),
        (view_x, view_y): (i32, i32),
        zoom: i32,
    ) -> [i32; 4] {
        let level = Zoom::from_value(zoom);
        let anchor = placed.anchor(self.rotation, zoom, level);
        let b = placed.zoomed_box(b, anchor, level);
        [
            (b[0] - i64::from(view_x)).clamp(0, i64::from(width)) as i32,
            (b[1] - i64::from(view_y)).clamp(0, i64::from(height)) as i32,
            (b[2] - i64::from(view_x)).clamp(0, i64::from(width)) as i32,
            (b[3] - i64::from(view_y)).clamp(0, i64::from(height)) as i32,
        ]
    }

    /// Puts the frame the clocks give (`frames`) on the pictures that play an idle animation,
    /// without staging the map again, and returns what [`Staged::changes`] would for the window:
    /// the rectangles of the pictures that changed. `None` when they cover too much to be worth
    /// drawing in parts; the frames are set all the same.
    pub(crate) fn reframe(
        &mut self,
        frames: &Frames,
        size: (u32, u32),
        view: (i32, i32),
        zoom: i32,
    ) -> Option<Vec<[i32; 4]>> {
        let mut rects: Vec<[i32; 4]> = Vec::new();
        for k in 0..self.idle.len() {
            let (i, id) = self.idle[k];
            let Some(&at) = frames.0.get(&id) else {
                continue;
            };
            let Pic::Anim(anim, v, _, frame) = &self.own[i].0.pic else {
                continue;
            };
            let last = (anim.sprites.get(v.file)).map_or(0, |s| s.0.frames.len().saturating_sub(1));
            let at = (at as usize).min(last);
            if *frame == at {
                continue;
            }
            rects.push(self.window_rect(&self.own[i].0, &self.own[i].1, size, view, zoom));
            if let Pic::Anim(_, _, _, frame) = &mut self.own[i].0.pic {
                *frame = at;
            }
            if let Some(b) = self.own[i].0.bounds() {
                self.own[i].1 = b;
                self.bounds = union(self.bounds, b);
            }
            rects.push(self.window_rect(&self.own[i].0, &self.own[i].1, size, view, zoom));
        }
        rects.retain(|r| r[0] < r[2] && r[1] < r[3]);
        match rects.len() > MAX_CHANGES {
            true => None,
            false => worth_redrawing(rects, size),
        }
    }

    /// Draws again only the rectangles `rects` (as [`Staged::changes`] gives them) of `target`,
    /// which holds a drawing of the window `view_x`, `view_y`, `zoom`, `grid` from another staging
    /// of the same map: the result is what [`Staged::draw`] gives.
    pub(crate) fn redraw(
        &self,
        target: &mut Framebuffer,
        view_x: i32,
        view_y: i32,
        zoom: i32,
        grid: bool,
        rects: &[[i32; 4]],
    ) {
        let level = Zoom::from_value(zoom);
        let mut parts: Vec<Framebuffer> = rects
            .iter()
            .map(|r| {
                let mut part = Framebuffer::new((r[2] - r[0]) as u32, (r[3] - r[1]) as u32);
                part.fill([0, 0, 0, 255]);
                part
            })
            .collect();
        for (placed, b) in self.pictures().filter(|(p, _)| grid || !p.grid) {
            let anchor = placed.anchor(self.rotation, zoom, level);
            // As `draw` culls by the window, then where it draws.
            let b = placed.zoomed_box(b, anchor, level);
            let (w, h) = (i64::from(target.width), i64::from(target.height));
            let (vx, vy) = (i64::from(view_x), i64::from(view_y));
            if !(b[0] < vx + w && b[2] > vx && b[1] < vy + h && b[3] > vy) {
                continue;
            }
            for (part, r) in parts.iter_mut().zip(rects) {
                let (left, top) = (
                    i64::from(view_x) + i64::from(r[0]),
                    i64::from(view_y) + i64::from(r[1]),
                );
                let (right, bottom) = (
                    i64::from(view_x) + i64::from(r[2]),
                    i64::from(view_y) + i64::from(r[3]),
                );
                if b[0] < right && b[2] > left && b[1] < bottom && b[3] > top {
                    placed.draw(part, (clamp32(left), clamp32(top)), anchor, level);
                }
            }
        }
        for (part, r) in parts.iter().zip(rects) {
            let row = part.width as usize * 4;
            for (k, line) in part.pixels.chunks_exact(row).enumerate() {
                let at = ((r[1] as usize + k) * target.width as usize + r[0] as usize) * 4;
                target.pixels[at..at + row].copy_from_slice(line);
            }
        }
    }
}

/// The most pictures that may have changed for a window to be drawn in parts.
pub(super) const MAX_CHANGES: usize = 400;

/// The rectangles joined, unless they cover more than half of the window.
fn worth_redrawing(rects: Vec<[i32; 4]>, (width, height): (u32, u32)) -> Option<Vec<[i32; 4]>> {
    let rects = merge(rects);
    let area: i64 = rects
        .iter()
        .map(|r| i64::from(r[2] - r[0]) * i64::from(r[3] - r[1]))
        .sum();
    (area * 2 <= i64::from(width) * i64::from(height)).then_some(rects)
}

/// The rectangles with the ones that touch each other joined.
pub(super) fn merge(mut rects: Vec<[i32; 4]>) -> Vec<[i32; 4]> {
    let mut done: Vec<[i32; 4]> = Vec::new();
    while let Some(mut r) = rects.pop() {
        while let Some(k) = done
            .iter()
            .position(|d| d[0] <= r[2] && r[0] <= d[2] && d[1] <= r[3] && r[1] <= d[3])
        {
            let d = done.swap_remove(k);
            r = [
                r[0].min(d[0]),
                r[1].min(d[1]),
                r[2].max(d[2]),
                r[3].max(d[3]),
            ];
        }
        done.push(r);
    }
    done
}

/// Draws an RGBA image whose anchor is at `at` at a zoom, source over, clipped. The pixels land
/// as those of a frame with the same offsets do.
pub(super) fn blend(
    image: &Framebuffer,
    target: &mut Framebuffer,
    (x, y): (i32, i32),
    picture: &Picture,
    zoom: Zoom,
) {
    let place = Place {
        x: i64::from(x),
        y: i64::from(y),
        mirror: false,
        zoom,
        x_offset: i64::from(picture.x_offset),
        y_offset: i64::from(picture.y_offset),
        height: i64::from(image.height),
    };
    let width = image.width as usize;
    for (row, line) in image.pixels.chunks_exact(width.max(1) * 4).enumerate() {
        let (first, count) = place.row(row as i64);
        for ty in first..first + count {
            if !(0..i64::from(target.height)).contains(&ty) {
                continue;
            }
            for (column, pixel) in line.as_chunks::<4>().0.iter().enumerate() {
                if pixel[3] == 0 {
                    continue;
                }
                let (first, copies) = place.column(column as i64);
                for tx in first..first + copies {
                    if !(0..i64::from(target.width)).contains(&tx) {
                        continue;
                    }
                    let at = (ty as usize * target.width as usize + tx as usize) * 4;
                    let back = &mut target.pixels[at..at + 4];
                    let alpha = u32::from(pixel[3]);
                    let mixed = [0, 1, 2].map(|c| {
                        ((u32::from(pixel[c]) * alpha + u32::from(back[c]) * (255 - alpha)) / 255)
                            as u8
                    });
                    let [r, g, b] = screen(mixed);
                    back.copy_from_slice(&[r, g, b, 255]);
                }
            }
        }
    }
}
