//! Drawing the interface tree into a frame.
//!
//! [`Painter::draw`] fills the frame with black and draws the tree depth first, lower layers
//! first (the children lists are sorted by [`Tree::resolve`]). An element is drawn when neither it
//! nor its layout is hidden, never shown by its layout, or a template. Pictures:
//!
//! * an image has its top left corner at the element's; an animation has its anchor at the centre
//!   of the element (`dx / 2`, `dy / 2`), shown at frame 0 of the set the widget state chooses
//!   (a button: normal, highlighted, selected, grayed; the others: set 0);
//! * with `transparent` (the default for pictures and buttons) the pixels equal to the top left
//!   pixel of an image or a canvas are not copied;
//! * texts, lists and tiled pictures are drawn into a canvas of the element's size, filled with
//!   `backcolor`, and the canvas is copied. A canvas is built again only when the element has the
//!   dirty flag (which this clears) or its size changes;
//! * a button's text has no canvas: only its glyphs are drawn, over the frame, with the colour of
//!   the state.
//!
//! Colours are reduced to the 16-bit frame of the original: fills and pictures by the rule of
//! [`fill_color`], text by the rule of [`text_color`]. The map view calls a hook with its
//! rectangle, which draws the map there. Scroll bars, status bars and
//! the other widgets that later milestones fill draw nothing yet.

use std::collections::HashMap;

use super::layout::{
    DIRTY, DISABLED, Element, HIDDEN, HIGHLIGHTED, Kind, NEVER_SHOWN, PRESSING, Picture, Pictures,
    RELEASING, SELECTED, STICKY, TEMPLATE, TOGGLE, Tree,
};
use crate::Framebuffer;
use crate::anim::View;
use crate::strings::StringTables;
use crate::text::{self, Fonts, Rect, Style, fill_color, to_cp1252};

/// The largest side of a canvas or a picture, in pixels.
const MAX_SIDE: i32 = 4096;

/// Where the texts of the elements come from.
pub trait Strings {
    /// The string with this number; empty if there is none.
    fn string(&self, id: i32) -> String;
    /// The bytes of the `TEXT` entry with this name; empty if there is none.
    fn text_entry(&self, name: &[u8]) -> Vec<u8>;
}

impl Strings for StringTables {
    fn string(&self, id: i32) -> String {
        u32::try_from(id).map_or(String::new(), |id| {
            StringTables::string(self, id).to_string()
        })
    }

    fn text_entry(&self, name: &[u8]) -> Vec<u8> {
        self.text(&String::from_utf8_lossy(name)).to_vec()
    }
}

/// A picture ready to be copied: RGBA pixels reduced to 5-6-5, alpha 0 where nothing is drawn, and
/// where its top left pixel lies relative to the anchor (an animation) or the corner (an image).
struct Cut {
    pixels: Framebuffer,
    origin: (i32, i32),
    animation: bool,
}

/// What a canvas was built for.
#[derive(PartialEq, Eq)]
struct Made {
    size: (i32, i32),
}

/// A rectangle as left, top, width, height.
pub type Rect4 = (i32, i32, i32, i32);

/// Draws the tree. It keeps the fonts, the canvases of the elements and the pictures cut for
/// drawing.
#[derive(Default)]
pub struct Painter {
    fonts: Fonts,
    canvases: HashMap<usize, (Made, Framebuffer)>,
    cuts: HashMap<(Picture, usize), Option<Cut>>,
}

impl Painter {
    /// Draws the whole tree into `frame`, which is filled with black first.
    pub fn draw(
        &mut self,
        tree: &mut Tree,
        frame: &mut Framebuffer,
        strings: &dyn Strings,
        map: &mut dyn FnMut(&mut Framebuffer, Rect4),
    ) {
        frame.fill([0, 0, 0, 255]);
        let mut stack = if tree.elements.is_empty() {
            vec![]
        } else {
            vec![0]
        };
        while let Some(i) = stack.pop() {
            let e = &tree.elements[i];
            if e.state & (HIDDEN | NEVER_SHOWN | TEMPLATE) != 0 {
                continue;
            }
            if matches!(e.kind, Kind::Layout) {
                stack.extend(e.children.iter().rev());
            } else {
                self.widget(tree, i, frame, strings, map);
            }
        }
    }

    fn widget(
        &mut self,
        tree: &mut Tree,
        i: usize,
        frame: &mut Framebuffer,
        strings: &dyn Strings,
        map: &mut dyn FnMut(&mut Framebuffer, Rect4),
    ) {
        let Some(placed) = tree.elements[i].placed else {
            return;
        };
        let (sx, sy) = tree.screen(i);
        let (w, h) = (placed.dx, placed.dy);
        if !(1..=MAX_SIDE).contains(&w) || !(1..=MAX_SIDE).contains(&h) {
            return;
        }
        let e = &tree.elements[i];
        let pictures = &tree.pictures;
        let dirty = e.state & DIRTY != 0;
        let screen = (sx, sy, w, h);
        match &e.kind {
            Kind::Image if e.dynamic_width || e.dynamic_height => {
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.tile(pictures, e, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::Image => {
                self.picture(pictures, e, 0, screen, frame);
                color_as_icon(frame, e, screen);
            }
            Kind::Button(_) => {
                let set = button_set(e.state);
                self.picture(pictures, e, set, screen, frame);
                color_as_icon(frame, e, screen);
                let text = element_text(e, strings);
                if !text.is_empty() {
                    self.button_text(e, strings, &text, frame, screen);
                }
            }
            Kind::Text(t) => {
                let border = t.show_border && w >= 3 && h >= 3;
                let text = element_text(e, strings);
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.text_widget(pictures, e, strings, &text, border, canvas);
                });
                copy_canvas(
                    frame,
                    canvas,
                    (sx, sy),
                    e,
                    if border { (1, 1) } else { (0, 0) },
                );
            }
            Kind::EditableText(_) => {
                let text = element_text(e, strings);
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.text_widget(pictures, e, strings, &text, false, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::ListBox(_) => {
                let canvas = self.canvas(i, (w, h), dirty, |_, canvas| {
                    canvas.fill(fill_color(e.look.back_color));
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::MapView => map(frame, screen),
            _ => {}
        }
        tree.elements[i].state &= !DIRTY;
    }

    /// The canvas of element `i`, built by `build` on a canvas filled with nothing when it is
    /// missing, dirty or of another size.
    fn canvas(
        &mut self,
        i: usize,
        size: (i32, i32),
        dirty: bool,
        build: impl FnOnce(&mut Painter, &mut Framebuffer),
    ) -> &Framebuffer {
        let made = Made { size };
        let fresh = !dirty && self.canvases.get(&i).is_some_and(|(m, _)| *m == made);
        if !fresh {
            let mut canvas = Framebuffer::new(size.0 as u32, size.1 as u32);
            build(self, &mut canvas);
            self.canvases.insert(i, (made, canvas));
        }
        &self.canvases[&i].1
    }

    /// The picture of a widget at `set`, drawn straight into the frame.
    fn picture(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        set: usize,
        (sx, sy, w, h): (i32, i32, i32, i32),
        frame: &mut Framebuffer,
    ) {
        let Some((picture, set)) = current(e, set) else {
            return;
        };
        let Some(cut) = self.cut(pictures, picture, set) else {
            return;
        };
        let (x, y) = if cut.animation {
            (sx + w / 2 + cut.origin.0, sy + h / 2 + cut.origin.1)
        } else {
            (sx, sy)
        };
        let key = (e.transparent && !cut.animation).then(|| top_left(&cut.pixels));
        blit(frame, &cut.pixels, (x, y), key);
    }

    /// The picture ready to copy; `set` is the sprite file of an animation (0 if it has no such).
    fn cut(&mut self, pictures: &Pictures, picture: Picture, set: usize) -> Option<&Cut> {
        let set = match picture {
            Picture::Image(_) => 0,
            Picture::Animation(a) => {
                let files = pictures.animations.get(a)?.sprites.len();
                if set < files { set } else { 0 }
            }
        };
        self.cuts
            .entry((picture, set))
            .or_insert_with(|| make_cut(pictures, picture, set))
            .as_ref()
    }

    /// A tiled picture: the canvas filled with `backcolor` and the picture repeated along the
    /// width (or, without `dynamicwidth`, the height), the last copy cut at the edge.
    fn tile(&mut self, pictures: &Pictures, e: &Element, canvas: &mut Framebuffer) {
        canvas.fill(fill_color(e.look.back_color));
        let Some((picture, set)) = current(e, 0) else {
            return;
        };
        let (w, h) = (canvas.width as i32, canvas.height as i32);
        let Some(cut) = self.cut(pictures, picture, set) else {
            return;
        };
        let (cw, ch) = (cut.pixels.width as i32, cut.pixels.height as i32);
        let by_width = e.dynamic_width;
        let step = if by_width { cw } else { ch };
        if step <= 0 {
            return;
        }
        let mut at = 0;
        while at < if by_width { w } else { h } {
            // The tile: from `at`, as wide as the picture (or as high), the other side whole.
            let (tx, ty, tw, th) = if by_width {
                (at, 0, step, h)
            } else {
                (0, at, w, step)
            };
            let (x, y) = if cut.animation {
                (tx + tw / 2 + cut.origin.0, ty + th / 2 + cut.origin.1)
            } else {
                (tx, ty)
            };
            blit(canvas, &cut.pixels, (x, y), None);
            at += step;
        }
    }

    /// The canvas of a `UIText`: background, frame, picture, text.
    fn text_widget(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        strings: &dyn Strings,
        text: &[u8],
        border: bool,
        canvas: &mut Framebuffer,
    ) {
        let (w, h) = (canvas.width as i32, canvas.height as i32);
        let back = fill_color(e.look.back_color);
        if border {
            canvas.fill(fill_color(e.look.highlight_color));
            fill_rect(canvas, (1, 1, w - 1, h - 1), back);
        } else {
            canvas.fill(back);
        }
        if let Some((picture, set)) = current(e, 0)
            && let Some(cut) = self.cut(pictures, picture, set)
        {
            let (x, y) = if cut.animation {
                (w / 2 + cut.origin.0, h / 2 + cut.origin.1)
            } else {
                (
                    (w - cut.pixels.width as i32) / 2,
                    (h - cut.pixels.height as i32) / 2,
                )
            };
            blit(canvas, &cut.pixels, (x, y), None);
        }
        let rect = Rect::inset(w, h, e.x_border, e.y_border);
        self.text(e, strings, text, canvas, rect, e.look.fore_color, false);
    }

    /// A button's text drawn over the frame: the colour follows the state; the rectangle loses the
    /// border on the left and top only, and holds one line of text centred vertically.
    fn button_text(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        text: &[u8],
        frame: &mut Framebuffer,
        (sx, sy, w, h): (i32, i32, i32, i32),
    ) {
        let Kind::Button(b) = &e.kind else { return };
        let color = match (button_set(e.state), e.state & HIGHLIGHTED != 0) {
            (3, _) => b.gray_color,
            (2, _) => b.select_color,
            (1, _) | (0, true) => e.look.highlight_color,
            _ => e.look.fore_color,
        };
        let rect = Rect {
            left: sx + e.x_border,
            top: sy + e.y_border,
            right: sx + w,
            bottom: sy + h,
        };
        self.text(e, strings, text, frame, rect, color, true);
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        text: &[u8],
        canvas: &mut Framebuffer,
        rect: Rect,
        color: [u8; 3],
        centre_line: bool,
    ) {
        let (face, size) = match e.look.font {
            Some((face, size)) => (strings.string(face), strings.string(size)),
            None => ("Arial".to_string(), "8".to_string()),
        };
        let font = self.fonts.get(&face, &size);
        let rect = if centre_line {
            rect.centred_line(font)
        } else {
            rect
        };
        let style = Style {
            color,
            justify: match e.look.justify {
                super::layout::Justify::Left => text::Justify::Left,
                super::layout::Justify::Center => text::Justify::Center,
                super::layout::Justify::Right => text::Justify::Right,
            },
            shadow: e.look.shadow_text,
        };
        text::draw(canvas, font, rect, text, &style);
    }
}

/// The text of an element: its `TEXT` entry, else its first string, as Windows-1252 bytes.
fn element_text(e: &Element, strings: &dyn Strings) -> Vec<u8> {
    if !e.look.text_name.is_empty() {
        return strings.text_entry(&e.look.text_name);
    }
    e.look
        .text_id
        .first()
        .map_or(vec![], |&id| to_cp1252(&strings.string(id)))
}

/// The sprite file of a button's animation for its state: 0 normal, 1 highlighted, 2 selected,
/// 3 grayed.
fn button_set(state: u32) -> usize {
    let held = state & (TOGGLE | STICKY) != 0 && state & PRESSING != 0;
    if state & DISABLED != 0 {
        3
    } else if state & TOGGLE != 0 && state & SELECTED != 0 && state & RELEASING != 0 {
        usize::from(state & HIGHLIGHTED != 0)
    } else if state & SELECTED != 0 || held {
        2
    } else {
        usize::from(state & HIGHLIGHTED != 0)
    }
}

/// The picture a widget shows and the set to draw: the first `animation` value, else `normal`;
/// a button without animation has an image per state.
fn current(e: &Element, set: usize) -> Option<(Picture, usize)> {
    if let Some(first) = e.look.animation.first() {
        return first.map(|p| (p, set));
    }
    let image = match &e.kind {
        Kind::Button(b) => [e.look.normal, b.highlighted, b.selected, b.grayed]
            .get(set)
            .copied()
            .flatten()
            .or(e.look.normal),
        _ => e.look.normal,
    };
    image.map(|p| (p, 0))
}

/// Without an icon, `colorasicon` fills the inner rectangle with the text colour.
fn color_as_icon(frame: &mut Framebuffer, e: &Element, (sx, sy, w, h): (i32, i32, i32, i32)) {
    if e.look.color_as_icon {
        let (bx, by) = (e.x_border, e.y_border);
        let rect = (
            sx.saturating_add(bx),
            sy.saturating_add(by),
            sx.saturating_add(w).saturating_sub(bx),
            sy.saturating_add(h).saturating_sub(by),
        );
        fill_rect(frame, rect, fill_color(e.look.fore_color));
    }
}

/// Copies a canvas to the frame; with `transparent` the pixels equal to the canvas pixel `key`
/// are skipped.
fn copy_canvas(
    frame: &mut Framebuffer,
    canvas: &Framebuffer,
    at: (i32, i32),
    e: &Element,
    key: (usize, usize),
) {
    let key = e.transparent.then(|| {
        let i = (key.1 * canvas.width as usize + key.0) * 4;
        let p = &canvas.pixels[i..i + 3];
        [p[0], p[1], p[2]]
    });
    blit(frame, canvas, at, key);
}

fn top_left(image: &Framebuffer) -> [u8; 3] {
    let p = &image.pixels;
    [p[0], p[1], p[2]]
}

/// Copies the pixels of `src` with alpha to `dst` with the top left corner at `at`, clipped;
/// pixels whose colour is `key` are skipped.
fn blit(dst: &mut Framebuffer, src: &Framebuffer, at: (i32, i32), key: Option<[u8; 3]>) {
    let (dw, dh) = (i64::from(dst.width), i64::from(dst.height));
    for row in 0..i64::from(src.height) {
        let y = i64::from(at.1) + row;
        if !(0..dh).contains(&y) {
            continue;
        }
        let line = &src.pixels[row as usize * src.width as usize * 4..][..src.width as usize * 4];
        for (col, pixel) in line.as_chunks::<4>().0.iter().enumerate() {
            let x = i64::from(at.0) + col as i64;
            if !(0..dw).contains(&x) || pixel[3] == 0 || key == Some([pixel[0], pixel[1], pixel[2]])
            {
                continue;
            }
            let to = (y * dw + x) as usize * 4;
            dst.pixels[to..to + 4].copy_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
}

/// Fills the rectangle (left, top, right, bottom), clipped.
fn fill_rect(dst: &mut Framebuffer, (l, t, r, b): (i32, i32, i32, i32), color: [u8; 4]) {
    let (w, h) = (dst.width as i32, dst.height as i32);
    for y in t.max(0)..b.min(h) {
        for x in l.max(0)..r.min(w) {
            let to = (y as usize * w as usize + x as usize) * 4;
            dst.pixels[to..to + 4].copy_from_slice(&color);
        }
    }
}

/// Cuts a picture for drawing: an image reduced to 5-6-5; an animation's set drawn once on a
/// transparent picture of its box, the pixels it drew reduced.
fn make_cut(pictures: &Pictures, picture: Picture, set: usize) -> Option<Cut> {
    match picture {
        Picture::Image(m) => {
            let image = pictures.images.get(m)?;
            let mut pixels = Framebuffer::new(image.width as u32, image.height as u32);
            for (to, from) in pixels
                .pixels
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(image.rgb.as_chunks::<3>().0)
            {
                *to = fill_color(*from);
            }
            Some(Cut {
                pixels,
                origin: (0, 0),
                animation: false,
            })
        }
        Picture::Animation(a) => {
            let animation = pictures.animations.get(a)?;
            let s = &animation.set;
            let (w, h) = (
                i64::from(s.x1) - i64::from(s.x0),
                i64::from(s.y1) - i64::from(s.y0),
            );
            if !(1..=i64::from(MAX_SIDE)).contains(&w) || !(1..=i64::from(MAX_SIDE)).contains(&h) {
                return None;
            }
            let mut pixels = Framebuffer::new(w as u32, h as u32);
            let view = View {
                file: set,
                mirror: false,
            };
            animation.draw(view, 0, &mut pixels, -s.x0, -s.y0);
            for pixel in pixels.pixels.as_chunks_mut::<4>().0 {
                if pixel[3] != 0 {
                    *pixel = fill_color([pixel[0], pixel[1], pixel[2]]);
                }
            }
            Some(Cut {
                pixels,
                origin: (s.x0, s.y0),
                animation: true,
            })
        }
    }
}

#[cfg(test)]
mod tests;
