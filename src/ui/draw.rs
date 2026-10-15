//! Drawing the interface tree into a frame.
//!
//! [`Painter::draw`] fills the frame with black and draws the tree depth first, lower layers
//! first (the children lists are sorted by [`Tree::resolve`]). An element is drawn when neither it
//! nor its layout is hidden, never shown by its layout, or a template. Pictures:
//!
//! * an image has its top left corner at the element's; an animation has its anchor at the centre
//!   of the element (`dx / 2`, `dy / 2`), shown at the frame [`Painter::animate`] has reached
//!   (0 at first) of the set the widget state chooses (a button: normal, highlighted, selected,
//!   grayed; the others: set 0);
//! * with `transparent` (the default for pictures and buttons) the pixels equal to the top left
//!   pixel of an image or a canvas are not copied;
//! * texts, lists and tiled pictures are drawn into a canvas of the element's size, filled with
//!   `backcolor`, and the canvas is copied. A canvas is built again only when the element has the
//!   dirty flag (which this clears) or its size changes;
//! * a button's icon (a picture the game's code gave it, such as an item of a scrolling region)
//!   is drawn at its first frame like the button's own picture, over it or, with `iconfirst`,
//!   under it;
//! * a button's text has no canvas: only its glyphs are drawn, over the frame, with the colour of
//!   the state.
//!
//! Colours are reduced to the 16-bit frame of the original: fills and pictures by the rule of
//! [`fill_color`], text by the rule of [`text_color`]. The map view calls a hook with its
//! rectangle, which draws the map there. A text scrolled by its bar starts at its line `top`; a
//! list draws its rows from `top`, the selected one with its colours and the hovered one framed;
//! a text field draws its selection in inverse colours and, focused, its caret; a scroll bar
//! draws its track (tiled along it), arrows and thumb, each in the look of its state (normal, hovered, held).
//! Status bars and the other widgets that later milestones fill draw nothing yet.

use std::collections::HashMap;

use super::layout::{
    DIRTY, DISABLED, Edit, Element, FOCUSED, HIDDEN, HIGHLIGHTED, Kind, ListBox, NEVER_SHOWN,
    PRESSING, Picture, Pictures, RELEASING, SELECTED, STICKY, ScrollBar, TEMPLATE, TOGGLE, Tree,
};
use super::widgets::{Heights, Lengths, Part, caret_for};
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

/// The alpha of canvas pixels that a transparent canvas copies though they are the colour it skips.
const FORCED: u8 = 254;

/// A rectangle as left, top, width, height.
pub type Rect4 = (i32, i32, i32, i32);

/// Draws the tree. It keeps the fonts, the canvases of the elements and the pictures cut for
/// drawing.
#[derive(Default)]
pub struct Painter {
    fonts: Fonts,
    canvases: HashMap<usize, (Made, Framebuffer)>,
    /// A cut of a picture, its set and its frame.
    cuts: HashMap<(Picture, usize, usize), Option<Cut>>,
    /// The frame an animated element shows, the milliseconds it has been shown and the clock when
    /// it was last drawn.
    frames: HashMap<usize, (usize, u64, u64)>,
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
        for i in shown(tree) {
            self.widget(tree, i, frame, strings, map);
        }
    }

    /// Advances the animations of the shown elements at a drawn frame, `now` being the clock in
    /// milliseconds: each by the time since it was last drawn, so one that was hidden is credited
    /// the whole time (sprite drawing 6.2: a frame lasts the delay of its sprite file, the
    /// sequence starts again at the end and the time left over is dropped). An element with
    /// `updateWhilePaused=0` does not advance while `paused`, but its time moves on. Returns
    /// whether some frame changed, so the interface has to be painted again.
    pub fn animate(&mut self, tree: &Tree, now: u64, paused: bool) -> bool {
        let mut changed = false;
        for i in shown(tree) {
            let e = &tree.elements[i];
            let Some((Picture::Animation(a), set)) = current(e, set_of(e)) else {
                continue;
            };
            let Some(sprite) = tree
                .pictures
                .animations
                .get(a)
                .and_then(|a| a.sprites.get(set).or(a.sprites.first()))
                .map(|s| &s.0)
            else {
                continue;
            };
            let (count, delay) = (sprite.frames.len(), u64::from(sprite.delay_ms));
            let (frame, time, seen) = self.frames.entry(i).or_insert((0, 0, now));
            let ms = now.saturating_sub(*seen);
            *seen = now;
            if *frame >= count {
                (*frame, *time) = (0, 0);
            }
            if count < 2 || delay == 0 || (paused && !e.look.update_while_paused) {
                continue;
            }
            *time = time.saturating_add(ms);
            while *time >= delay {
                *time -= delay;
                *frame += 1;
                changed = true;
                if *frame >= count {
                    (*frame, *time) = (0, 0);
                }
            }
        }
        changed
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
                    painter.tile(pictures, e, e.dynamic_width, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::Image => {
                self.picture(pictures, i, e, 0, screen, frame);
                color_as_icon(frame, e, screen);
            }
            Kind::Button(_) => {
                let set = button_set(e.state);
                if e.look.icon_first {
                    self.icon(pictures, e, screen, frame);
                }
                self.picture(pictures, i, e, set, screen, frame);
                if !e.look.icon_first {
                    self.icon(pictures, e, screen, frame);
                }
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
                    painter.background(pictures, e, border, canvas);
                    let rect = Rect::inset(w, h, e.x_border, e.y_border);
                    let up = (t.top as i64 * i64::from(painter.line_height(e, strings)))
                        .min(i64::from(i32::MAX)) as i32;
                    let rect = Rect {
                        top: rect.top.saturating_sub(up),
                        ..rect
                    };
                    painter.text(e, strings, &text, canvas, rect, e.look.fore_color, false);
                });
                copy_canvas(
                    frame,
                    canvas,
                    (sx, sy),
                    e,
                    if border { (1, 1) } else { (0, 0) },
                );
            }
            Kind::EditableText(edit) => {
                let text = element_text(e, strings);
                let focused = e.state & FOCUSED != 0;
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.background(pictures, e, false, canvas);
                    painter.field(e, edit, focused, strings, &text, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::ListBox(list) => {
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.list(pictures, e, list, strings, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            // A bar with nothing to scroll is not shown.
            Kind::ScrollBar(bar) if bar.bar.scrollable() => {
                let lengths = tree.bar_lengths(i);
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.scroll_bar(pictures, e, bar, lengths, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::MapView => map(frame, screen),
            // The message line: its picture and the text, centred in the rectangle the borders
            // leave (ui layout 4.12).
            Kind::MessageQueue => {
                self.picture(pictures, i, e, 0, screen, frame);
                let text = element_text(e, strings);
                let rect = Rect {
                    left: sx.saturating_add(e.x_border),
                    top: sy.saturating_add(e.y_border),
                    right: sx.saturating_add(w).saturating_sub(e.x_border),
                    bottom: sy.saturating_add(h).saturating_sub(e.y_border),
                };
                if !text.is_empty() {
                    self.text(e, strings, &text, frame, rect, e.look.fore_color, false);
                }
            }
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
        i: usize,
        e: &Element,
        set: usize,
        (sx, sy, w, h): (i32, i32, i32, i32),
        frame: &mut Framebuffer,
    ) {
        let Some((picture, set)) = current(e, set) else {
            return;
        };
        let at = self.frames.get(&i).map_or(0, |f| f.0);
        let Some(cut) = self.cut(pictures, picture, set, at) else {
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

    /// The icon of a button, at its first frame, placed like the button's picture.
    fn icon(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        (sx, sy, w, h): (i32, i32, i32, i32),
        frame: &mut Framebuffer,
    ) {
        let Some(icon) = e.look.icon else { return };
        let Some(cut) = self.cut(pictures, icon, 0, 0) else {
            return;
        };
        let (x, y) = match cut.animation {
            true => (sx + w / 2 + cut.origin.0, sy + h / 2 + cut.origin.1),
            false => (sx, sy),
        };
        blit(frame, &cut.pixels, (x, y), None);
    }

    /// Whether the point (`x`, `y`) of the screen is on an opaque pixel of what the element shows:
    /// its current picture (for a button, the set of its state) or, for a widget with a canvas,
    /// its canvas. `None` if it shows nothing yet. Transparent is an animation's pixel without a
    /// run, an image's or canvas's pixel equal to its top left one (to (1, 1) for a framed text).
    pub fn opaque_at(&mut self, tree: &Tree, i: usize, (x, y): (i32, i32)) -> Option<bool> {
        let e = &tree.elements[i];
        let (sx, sy) = tree.screen(i);
        let (w, h) = e.placed.map_or((0, 0), |p| (p.dx, p.dy));
        let canvas = matches!(
            e.kind,
            Kind::Text(_) | Kind::EditableText(_) | Kind::ListBox(_) | Kind::ScrollBar(_)
        ) || (matches!(e.kind, Kind::Image) && (e.dynamic_width || e.dynamic_height));
        if canvas {
            let (made, pixels) = self.canvases.get(&i)?;
            let key = match &e.kind {
                Kind::Text(t) if t.show_border && made.size.0 >= 3 && made.size.1 >= 3 => (1, 1),
                _ => (0, 0),
            };
            return Some(pixel_differs(pixels, (x - sx, y - sy), key));
        }
        let (picture, set) = current(e, set_of(e))?;
        let at = self.frames.get(&i).map_or(0, |f| f.0);
        let cut = self.cut(&tree.pictures, picture, set, at)?;
        Some(match cut.animation {
            true => {
                let (ox, oy) = (sx + w / 2 + cut.origin.0, sy + h / 2 + cut.origin.1);
                alpha_at(&cut.pixels, (x - ox, y - oy))
            }
            false => pixel_differs(&cut.pixels, (x - sx, y - sy), (0, 0)),
        })
    }

    /// The picture ready to copy; `set` is the sprite file of an animation (0 if it has no such)
    /// and `frame` its frame (0 if it has no such).
    fn cut(
        &mut self,
        pictures: &Pictures,
        picture: Picture,
        set: usize,
        frame: usize,
    ) -> Option<&Cut> {
        let (set, frame) = match picture {
            Picture::Image(_) => (0, 0),
            Picture::Animation(a) => {
                let sprites = &pictures.animations.get(a)?.sprites;
                let set = if set < sprites.len() { set } else { 0 };
                let frames = sprites.get(set).map_or(0, |s| s.0.frames.len());
                (set, if frame < frames { frame } else { 0 })
            }
        };
        self.cuts
            .entry((picture, set, frame))
            .or_insert_with(|| make_cut(pictures, picture, set, frame))
            .as_ref()
    }

    /// A tiled picture: the canvas filled with `backcolor` and the picture repeated along the
    /// width (`by_width`) or the height, the last copy cut at the edge.
    fn tile(&mut self, pictures: &Pictures, e: &Element, by_width: bool, canvas: &mut Framebuffer) {
        canvas.fill(fill_color(e.look.back_color));
        let Some((picture, set)) = current(e, 0) else {
            return;
        };
        let (w, h) = (canvas.width as i32, canvas.height as i32);
        let Some(cut) = self.cut(pictures, picture, set, 0) else {
            return;
        };
        let (cw, ch) = (cut.pixels.width as i32, cut.pixels.height as i32);
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

    /// The canvas of a text, a list or a bar before its content: the background, the frame, the
    /// picture under it.
    fn background(
        &mut self,
        pictures: &Pictures,
        e: &Element,
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
            && let Some(cut) = self.cut(pictures, picture, set, 0)
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
    }

    /// The text of a field on one line, without shadow: the bytes before the selection and after
    /// it in the text colour, the selected ones in the background colour on a rectangle of the
    /// text colour; a focused field with nothing selected shows its caret as a line one pixel
    /// wide. The start of the line is that of the whole text, as `justify` says.
    fn field(
        &mut self,
        e: &Element,
        edit: &Edit,
        focused: bool,
        strings: &dyn Strings,
        text: &[u8],
        canvas: &mut Framebuffer,
    ) {
        let (w, h) = (canvas.width as i32, canvas.height as i32);
        let rect = Rect::inset(w, h, e.x_border, e.y_border);
        let font = self.font(e, strings);
        let line = font.line_height();
        let start = justify(e).start(&rect, font.width(text));
        let (from, to) = match focused {
            true => edit.caret.range(text.len()),
            false => (0, 0),
        };
        let x_of = |font: &text::Font, n: usize| start.saturating_add(font.width(&text[..n]));
        let (x0, x1) = (x_of(font, from), x_of(font, to));
        if to > from {
            fill_rect(
                canvas,
                (x0, rect.top, x1, rect.top.saturating_add(line)),
                fill_color(e.look.fore_color),
            );
        }
        let runs = [
            (0, from, e.look.fore_color),
            (from, to, e.look.back_color),
            (to, text.len(), e.look.fore_color),
        ];
        for (a, b, color) in runs {
            let left = x_of(font, a);
            let run = Rect {
                left,
                right: left.saturating_add(MAX_SIDE * 16),
                ..rect
            };
            let style = Style {
                color,
                justify: text::Justify::Left,
                shadow: false,
            };
            text::draw(canvas, font, run, &text[a..b], &style);
        }
        if focused && from == to {
            let at = x_of(font, edit.caret.at.min(text.len()));
            fill_rect(
                canvas,
                (at, rect.top, at + 1, rect.top.saturating_add(line)),
                fill_color(e.look.fore_color),
            );
        }
    }

    /// The rows of a list from its first shown one: the selected row in its colour on its fill and
    /// frame, the hovered one framed, disabled ones grayed.
    fn list(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        l: &ListBox,
        strings: &dyn Strings,
        canvas: &mut Framebuffer,
    ) {
        let (w, h) = (canvas.width as i32, canvas.height as i32);
        canvas.fill(fill_color(e.look.back_color));
        let heights = self.row_heights(pictures, e, l, strings);
        // The columns: the icon from 0, the text from the width of the slot (0 without one) to
        // the width less two borders.
        let (text_left, right) = (l.minicon_width, w.saturating_sub(2 * e.x_border));
        let line = self.line_height(e, strings);
        // Whole rows only, inside the borders.
        let mut y = e.y_border;
        let end = h.saturating_sub(e.y_border);
        for (n, row) in l.rows.rows.iter().enumerate().skip(l.rows.top) {
            let row_height = heights.rows.get(n).copied().unwrap_or(line);
            if y.saturating_add(row_height) > end {
                break;
            }
            let rect = (0, y, w, y.saturating_add(row_height));
            let selected = l.highlight_selection && l.rows.selected == Some(n);
            if selected && let Some(fill) = l.select_back_color {
                // Shown even where it is the colour of the background: not the 255 of a pixel
                // that a transparent canvas skips.
                let [r, g, b, _] = fill_color(fill);
                fill_rect(canvas, rect, [r, g, b, FORCED]);
            }
            if l.rows.hover == Some(n)
                && let Some(color) = l.highlight_border_color
            {
                frame_rect(canvas, rect, fill_color(color));
            }
            if selected && let Some(color) = l.select_border_color {
                frame_rect(canvas, rect, fill_color(color));
            }
            let color = match (row.disabled, selected) {
                (true, _) => l.grayed_color,
                (false, true) => l.select_color,
                _ => e.look.fore_color,
            };
            if let Some(icon) = row.icon
                && let Some(cut) = self.cut(pictures, icon, 0, 0)
            {
                // An animation is anchored half its width from the left of the column and at the
                // middle of the slot's height, an image at the corner.
                let slot_h = match l.minicon_height {
                    0 => row_height,
                    h => h,
                };
                let at = match cut.animation {
                    true => (
                        cut.pixels.width as i32 / 2 + cut.origin.0,
                        y + slot_h / 2 + cut.origin.1,
                    ),
                    false => (0, y),
                };
                blit(canvas, &cut.pixels, at, None);
            }
            // Text on one line is centred in its row; a wrapped text fills it.
            let down =
                (row_height - self.text_height(e, strings, &row.text, right - text_left)) / 2;
            let text_rect = Rect {
                left: text_left,
                top: y.saturating_add(down.max(0)),
                right,
                bottom: rect.3,
            };
            self.text(e, strings, &row.text, canvas, text_rect, color, false);
            y = rect.3.saturating_add(l.spacing);
        }
    }

    /// The height of the lines a text takes in `width` pixels, at least one line.
    fn text_height(&mut self, e: &Element, strings: &dyn Strings, text: &[u8], width: i32) -> i32 {
        let font = self.font(e, strings);
        let lines = text::layout(font, text, width).len().max(1);
        i32::try_from(lines)
            .unwrap_or(i32::MAX)
            .saturating_mul(font.line_height())
    }

    /// The height of each row of a list: its wrapped text or its icon's slot, whichever is taller
    /// (the slot being `miniconheight` or the icon's own height if that is more), with the space
    /// between rows and the height inside the borders.
    pub fn row_heights(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        l: &ListBox,
        strings: &dyn Strings,
    ) -> Heights {
        let (dx, dy) = e.placed.map_or((0, 0), |p| (p.dx, p.dy));
        let width = dx.saturating_sub(e.x_border.saturating_mul(2)) - l.minicon_width;
        let rows = l
            .rows
            .rows
            .iter()
            .map(|row| {
                let icon = row.icon.map_or(0, |p| pictures.size(p).1);
                self.text_height(e, strings, &row.text, width)
                    .max(l.minicon_height.max(icon))
            })
            .collect();
        Heights {
            rows,
            spacing: l.spacing.max(0),
            inner: dy.saturating_sub(e.y_border.saturating_mul(2)),
        }
    }

    /// A scroll bar: its track, then the two arrows and the thumb, each in the set of its look
    /// (0 normal, 1 under the pointer, 2 held), the animation's box centred on its part.
    fn scroll_bar(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        s: &ScrollBar,
        lengths: (Lengths, bool),
        canvas: &mut Framebuffer,
    ) {
        let (l, vertical) = lengths;
        // The track is tiled along the bar.
        self.tile(pictures, e, !vertical, canvas);
        let (cw, ch) = (canvas.width as i32, canvas.height as i32);
        let parts = [
            (Part::Up, s.up_arrow, 0, l.up),
            (
                Part::Down,
                s.down_arrow,
                l.bar.saturating_sub(l.down),
                l.down,
            ),
            (
                Part::Thumb,
                s.thumb,
                s.bar.dragged.unwrap_or_else(|| s.bar.thumb_start(&l)),
                l.thumb,
            ),
        ];
        for (part, picture, start, length) in parts {
            let set = if s.bar.held == Some(part) {
                2
            } else {
                usize::from(s.bar.hover == Some(part))
            };
            let Some(cut) = picture.and_then(|p| self.cut(pictures, p, set, 0)) else {
                continue;
            };
            let (pw, ph) = (cut.pixels.width as i32, cut.pixels.height as i32);
            // The part's centre, for an animation; its top left corner, for an image.
            let (centre, corner) = match vertical {
                true => ((cw / 2, start + length / 2), ((cw - pw) / 2, start)),
                false => ((start + length / 2, ch / 2), (start, (ch - ph) / 2)),
            };
            let at = match cut.animation {
                true => (centre.0 + cut.origin.0, centre.1 + cut.origin.1),
                false => corner,
            };
            blit(canvas, &cut.pixels, at, None);
        }
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

    /// A text over the frame whose left is `x` and whose line is centred on `y`, in the font and
    /// the shadow of the element `e`, as the map view writes a cost next to the pointer.
    pub fn text_at(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        frame: &mut Framebuffer,
        (x, y): (i32, i32),
        text: &[u8],
        color: [u8; 3],
    ) {
        let font = self.font(e, strings);
        let top = y.saturating_sub(font.line_height() / 2);
        let rect = Rect {
            left: x,
            top,
            right: x.saturating_add(font.width(text)),
            bottom: top.saturating_add(font.line_height()),
        };
        let style = Style {
            color,
            justify: text::Justify::Left,
            shadow: e.look.shadow_text,
        };
        text::draw(frame, font, rect, text, &style);
    }

    /// The font an element names with its `font` and `fontsize` strings: Arial 8 without them.
    fn font(&mut self, e: &Element, strings: &dyn Strings) -> &mut text::Font {
        let (face, size) = match e.look.font {
            Some((face, size)) => (strings.string(face), strings.string(size)),
            None => ("Arial".to_string(), "8".to_string()),
        };
        self.fonts.get(&face, &size)
    }

    /// The width of a text on one line in the element's font.
    pub fn text_width(&mut self, e: &Element, strings: &dyn Strings, text: &[u8]) -> i32 {
        self.font(e, strings).width(text)
    }

    /// The height of a line of the element's font.
    pub fn line_height(&mut self, e: &Element, strings: &dyn Strings) -> i32 {
        self.font(e, strings).line_height()
    }

    /// How many lines a text takes in a `UIText` of the element's size, and how many of them fit
    /// in its height (at least one).
    pub fn text_lines(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        text: &[u8],
    ) -> (usize, usize) {
        let Some(placed) = e.placed else {
            return (0, 1);
        };
        let font = self.font(e, strings);
        let inner = Rect::inset(placed.dx, placed.dy, e.x_border, e.y_border);
        let total = text::layout(font, text, inner.width()).len();
        let room = inner.bottom.saturating_sub(inner.top) / font.line_height().max(1);
        (total, room.max(1) as usize)
    }

    /// The size of a tooltip of `text` in the element's font, borders included, at most
    /// `max_width` wide before it wraps (text 5.5).
    pub fn tooltip_size(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        text: &[u8],
        max_width: i32,
    ) -> (i32, i32) {
        let aligned = !matches!(e.look.justify, super::layout::Justify::Left);
        let font = self.font(e, strings);
        text::tooltip_size(font, text, aligned, max_width, e.x_border, e.y_border)
    }

    /// The caret position for a click at distance `x` from the left of a text field.
    pub fn caret_at(&mut self, e: &Element, strings: &dyn Strings, text: &[u8], x: i32) -> usize {
        let Some(placed) = e.placed else { return 0 };
        let rect = Rect::inset(placed.dx, placed.dy, e.x_border, e.y_border);
        let font = self.font(e, strings);
        let start = justify(e).start(&rect, font.width(text));
        caret_for(text, |b| font.advance(b), x.saturating_sub(start))
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
        let font = self.font(e, strings);
        let rect = if centre_line {
            rect.centred_line(font)
        } else {
            rect
        };
        let style = Style {
            color,
            justify: justify(e),
            shadow: e.look.shadow_text,
        };
        text::draw(canvas, font, rect, text, &style);
    }
}

fn justify(e: &Element) -> text::Justify {
    match e.look.justify {
        super::layout::Justify::Left => text::Justify::Left,
        super::layout::Justify::Center => text::Justify::Center,
        super::layout::Justify::Right => text::Justify::Right,
    }
}

/// The text of an element: the one the program gave it, else its `TEXT` entry, else its first
/// string, as Windows-1252 bytes.
pub fn element_text(e: &Element, strings: &dyn Strings) -> Vec<u8> {
    if let Some(text) = &e.text {
        return text.clone();
    }
    if !e.look.text_name.is_empty() {
        return strings.text_entry(&e.look.text_name);
    }
    e.look
        .text_id
        .first()
        .map_or(vec![], |&id| to_cp1252(&strings.string(id)))
}

/// The widgets to draw, in drawing order: depth first, lower layers first.
fn shown(tree: &Tree) -> Vec<usize> {
    let mut order = Vec::new();
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
            order.push(i);
        }
    }
    order
}

/// The set a widget draws: its state's for a button, else 0.
fn set_of(e: &Element) -> usize {
    match e.kind {
        Kind::Button(_) => button_set(e.state),
        _ => 0,
    }
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

/// Whether pixel (x, y) of `image` is not the colour of its pixel `key`; a point outside is not.
fn pixel_differs(image: &Framebuffer, (x, y): (i32, i32), key: (usize, usize)) -> bool {
    let at = |x: usize, y: usize| &image.pixels[(y * image.width as usize + x) * 4..][..3];
    match (usize::try_from(x), usize::try_from(y)) {
        (Ok(x), Ok(y)) if x < image.width as usize && y < image.height as usize => {
            at(x, y)
                != at(
                    key.0.min(image.width as usize - 1),
                    key.1.min(image.height as usize - 1),
                )
        }
        _ => false,
    }
}

/// Whether pixel (x, y) of a cut animation is drawn; a point outside is not.
fn alpha_at(image: &Framebuffer, (x, y): (i32, i32)) -> bool {
    match (usize::try_from(x), usize::try_from(y)) {
        (Ok(x), Ok(y)) if x < image.width as usize && y < image.height as usize => {
            image.pixels[(y * image.width as usize + x) * 4 + 3] != 0
        }
        _ => false,
    }
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
            if !(0..dw).contains(&x)
                || pixel[3] == 0
                || (pixel[3] != FORCED && key == Some([pixel[0], pixel[1], pixel[2]]))
            {
                continue;
            }
            let to = (y * dw + x) as usize * 4;
            dst.pixels[to..to + 4].copy_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
}

/// Draws a one-pixel frame just inside the rectangle (left, top, right, bottom), clipped.
fn frame_rect(dst: &mut Framebuffer, (l, t, r, b): (i32, i32, i32, i32), color: [u8; 4]) {
    fill_rect(dst, (l, t, r, t + 1), color);
    fill_rect(dst, (l, b - 1, r, b), color);
    fill_rect(dst, (l, t, l + 1, b), color);
    fill_rect(dst, (r - 1, t, r, b), color);
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

/// Cuts a picture for drawing: an image reduced to 5-6-5; a frame of an animation's set drawn once
/// on a transparent picture of its box, the pixels it drew reduced.
fn make_cut(pictures: &Pictures, picture: Picture, set: usize, frame: usize) -> Option<Cut> {
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
            animation.draw(view, frame, &mut pixels, -s.x0, -s.y0);
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
pub(crate) mod tests;
