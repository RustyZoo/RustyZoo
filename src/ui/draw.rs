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
//! An image set draws the one of its pictures that the game's code chose; a status bar draws the
//! picture of its level over the part of its width that its value fills.
//! The other widgets that later milestones fill draw nothing yet.

mod controls;
mod pixels;

use std::collections::HashMap;

use super::layout::{
    DIRTY, DISABLED, Edit, Element, FOCUSED, HIDDEN, HIGHLIGHTED, Kind, Levels, ListBox,
    NEVER_SHOWN, PRESSING, Picture, Pictures, RELEASING, SELECTED, STICKY, ScrollBar, TEMPLATE,
    TOGGLE, Tree,
};
use super::widgets::{Heights, Lengths, Part, caret_for};
use crate::Framebuffer;
use crate::anim::View;
use crate::strings::StringTables;
use crate::text::{self, Fonts, Rect, Style, fill_color, to_cp1252};
use pixels::{
    alpha_at, blit, color_as_icon, copy_canvas, fill_rect, frame_rect, make_cut, pixel_differs,
    top_left,
};

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
        to_cp1252(self.text(&String::from_utf8_lossy(name)))
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
pub(crate) type Rect4 = (i32, i32, i32, i32);

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
    /// The same for the animated icon of a button (its own clock, started when the icon is set).
    icons: HashMap<usize, (usize, u64, u64)>,
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
    /// whether some frame changed, so the interface has to be painted again. The animated icon of
    /// a button (information panels 4.3.1) has its own clock, which [`Painter::restart_icon`]
    /// starts over, and the same pause rule.
    pub(crate) fn animate(&mut self, tree: &Tree, now: u64, paused: bool) -> bool {
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
            let run = !paused || e.look.update_while_paused;
            changed |= step(
                self.frames.entry(i).or_insert((0, 0, now)),
                (count, delay),
                now,
                run,
            );
        }
        for i in shown(tree) {
            let e = &tree.elements[i];
            let (Some(Picture::Animation(a)), Some(file)) = (e.look.icon, e.look.icon_file) else {
                continue;
            };
            let Some(sprite) = (tree.pictures.animations.get(a))
                .and_then(|a| a.sprites.get(file))
                .map(|s| &s.0)
            else {
                continue;
            };
            let (count, delay) = (sprite.frames.len(), u64::from(sprite.delay_ms));
            let run = !paused || e.look.update_while_paused;
            changed |= step(
                self.icons.entry(i).or_insert((0, 0, now)),
                (count, delay),
                now,
                run,
            );
        }
        changed
    }

    /// Starts the icon clock of element `i` over: its next frame is the first.
    pub(crate) fn restart_icon(&mut self, i: usize) {
        self.icons.remove(&i);
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
            Kind::Image | Kind::ImageSet(_) => {
                self.picture(pictures, i, e, 0, screen, frame);
                color_as_icon(frame, e, screen);
            }
            Kind::Button(_) => {
                let set = button_set(e.state);
                if e.look.icon_first {
                    self.icon(pictures, i, e, screen, frame);
                }
                self.picture(pictures, i, e, set, screen, frame);
                if !e.look.icon_first {
                    self.icon(pictures, i, e, screen, frame);
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
                // The key is the cleared canvas, not the top left pixel a row's icon may cover.
                let [r, g, b, _] = fill_color(e.look.back_color);
                blit(frame, canvas, (sx, sy), e.transparent.then_some([r, g, b]));
            }
            // A bar with nothing to scroll is not shown.
            Kind::ScrollBar(bar) if bar.bar.scrollable() => {
                let lengths = tree.bar_lengths(i);
                let canvas = self.canvas(i, (w, h), dirty, |painter, canvas| {
                    painter.scroll_bar(pictures, e, bar, lengths, canvas);
                });
                copy_canvas(frame, canvas, (sx, sy), e, (0, 0));
            }
            Kind::StatusImage(levels) => self.bar(pictures, levels, screen, frame),
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

    /// A status bar (ui layout 4.9): the picture of its level, drawn side by side from the left
    /// edge `max(width, height) * value / 100` times, at least once. The level is the first
    /// picture whose transition is above the value, else the last.
    fn bar(
        &mut self,
        pictures: &Pictures,
        levels: &Levels,
        (sx, sy, w, h): (i32, i32, i32, i32),
        frame: &mut Framebuffer,
    ) {
        let value = levels.value.clamp(0, 100);
        let length = (i64::from(w.max(h)) * i64::from(value) / 100).max(1);
        let level = levels.level.unwrap_or_else(|| {
            (levels.transitions.iter())
                .position(|&t| t > value)
                .unwrap_or(levels.images.len().saturating_sub(1))
        });
        let Some(Some(picture)) = levels.images.get(level) else {
            return;
        };
        let Some(cut) = self.cut(pictures, *picture, 0, 0) else {
            return;
        };
        let step = cut.pixels.width.max(1) as i32;
        for k in 0..length as i32 {
            blit(frame, &cut.pixels, (sx + k * step, sy), None);
        }
    }

    /// The icon of a button, placed like the button's picture, at the frame of its own clock. An
    /// icon with a sprite file (the unit's picture on a panel's follow button) is centred in the
    /// button and clipped to its rectangle.
    fn icon(
        &mut self,
        pictures: &Pictures,
        i: usize,
        e: &Element,
        (sx, sy, w, h): (i32, i32, i32, i32),
        frame: &mut Framebuffer,
    ) {
        let Some(icon) = e.look.icon else { return };
        let file = e.look.icon_file;
        let at = file.and_then(|_| self.icons.get(&i)).map_or(0, |f| f.0);
        let Some(cut) = self.cut(pictures, icon, file.unwrap_or(0), at) else {
            return;
        };
        let (x, y) = match cut.animation {
            true => (sx + w / 2 + cut.origin.0, sy + h / 2 + cut.origin.1),
            false => (sx, sy),
        };
        if file.is_none() || w <= 0 || h <= 0 {
            return blit(frame, &cut.pixels, (x, y), None);
        }
        let mut inside = Framebuffer::new(w as u32, h as u32);
        blit(&mut inside, &cut.pixels, (x - sx, y - sy), None);
        blit(frame, &inside, (sx, sy), None);
    }

    /// Whether the point (`x`, `y`) of the screen is on an opaque pixel of what the element shows:
    /// its current picture (for a button, the set of its state) or, for a widget with a canvas,
    /// its canvas. `None` if it shows nothing yet. Transparent is an animation's pixel without a
    /// run, an image's or canvas's pixel equal to its top left one (to (1, 1) for a framed text).
    pub(crate) fn opaque_at(&mut self, tree: &Tree, i: usize, (x, y): (i32, i32)) -> Option<bool> {
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

    /// A text over the frame whose left is `x` and whose line starts at the row `y`, in the font and
    /// the shadow of the element `e` (or none), as the map view writes a cost next to the pointer.
    pub(crate) fn text_at(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        frame: &mut Framebuffer,
        (x, y): (i32, i32),
        text: &[u8],
        (color, shadow): ([u8; 3], bool),
    ) {
        let font = self.font(e, strings);
        let rect = Rect {
            left: x,
            top: y,
            right: x.saturating_add(font.width(text)),
            bottom: y.saturating_add(font.line_height()),
        };
        let style = Style {
            color,
            justify: text::Justify::Left,
            shadow,
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
    pub(crate) fn text_width(&mut self, e: &Element, strings: &dyn Strings, text: &[u8]) -> i32 {
        self.font(e, strings).width(text)
    }

    /// The height of a line of the element's font.
    pub(crate) fn line_height(&mut self, e: &Element, strings: &dyn Strings) -> i32 {
        self.font(e, strings).line_height()
    }

    /// How many lines a text takes in a `UIText` of the element's size, and how many of them fit
    /// in its height (at least one).
    pub(crate) fn text_lines(
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
    pub(crate) fn tooltip_size(
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
pub(crate) fn element_text(e: &Element, strings: &dyn Strings) -> Vec<u8> {
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

/// Advances the clock `(frame, time, seen)` of a sprite file of `(count, delay)` frames and ms to
/// `now`: by the time since it was last seen when `run`, the sequence starting again at its end
/// with the time left over dropped. Returns whether the frame changed.
fn step(clock: &mut (usize, u64, u64), (count, delay): (usize, u64), now: u64, run: bool) -> bool {
    let (frame, time, seen) = clock;
    let ms = now.saturating_sub(*seen);
    *seen = now;
    if *frame >= count {
        (*frame, *time) = (0, 0);
    }
    if count < 2 || delay == 0 || !run {
        return false;
    }
    let mut changed = false;
    *time = time.saturating_add(ms);
    while *time >= delay {
        *time -= delay;
        *frame += 1;
        changed = true;
        if *frame >= count {
            (*frame, *time) = (0, 0);
        }
    }
    changed
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

/// The picture a widget shows and the set to draw: the first `animation` value (an image set
/// shows the one the game chose), else `normal`;
/// a button without animation has an image per state.
fn current(e: &Element, set: usize) -> Option<(Picture, usize)> {
    if let Kind::ImageSet(levels) = &e.kind {
        return e
            .look
            .animation
            .get(levels.shown)
            .copied()
            .flatten()
            .map(|p| (p, set));
    }
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

#[cfg(test)]
pub(crate) mod tests;
