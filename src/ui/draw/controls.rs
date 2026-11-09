//! Drawing the text-bearing controls: fields, lists, scroll bars and button texts.

use super::*;

impl Painter {
    /// The text of a field on one line, without shadow: the bytes before the selection and after
    /// it in the text colour, the selected ones in the background colour on a rectangle of the
    /// text colour; a focused field with nothing selected shows its caret as a line one pixel
    /// wide. The start of the line is that of the whole text, as `justify` says.
    pub(super) fn field(
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
    pub(super) fn list(
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
        let shown = (l.rows.rows.iter().enumerate()).filter(|&(n, _)| !l.rows.hidden(n));
        for (place, (n, row)) in shown.enumerate().skip(l.rows.top) {
            let row_height = heights.rows.get(place).copied().unwrap_or(line);
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
                _ => l
                    .rows
                    .colors
                    .get(n)
                    .copied()
                    .flatten()
                    .unwrap_or(e.look.fore_color),
            };
            let icon = match row.disabled {
                true => row.grayed_icon.or(row.icon),
                false => row.icon,
            };
            if let Some(icon) = icon
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
    pub(super) fn text_height(
        &mut self,
        e: &Element,
        strings: &dyn Strings,
        text: &[u8],
        width: i32,
    ) -> i32 {
        let font = self.font(e, strings);
        let lines = text::layout(font, text, width).len().max(1);
        i32::try_from(lines)
            .unwrap_or(i32::MAX)
            .saturating_mul(font.line_height())
    }

    /// The height of each row of a list: its wrapped text or its icon's slot, whichever is taller
    /// (the slot being `miniconheight` or the icon's own height if that is more), with the space
    /// between rows and the height inside the borders.
    pub(crate) fn row_heights(
        &mut self,
        pictures: &Pictures,
        e: &Element,
        l: &ListBox,
        strings: &dyn Strings,
    ) -> Heights {
        let (dx, dy) = e.placed.map_or((0, 0), |p| (p.dx, p.dy));
        let width = dx.saturating_sub(e.x_border.saturating_mul(2)) - l.minicon_width;
        let shown: Vec<_> = (l.rows.rows.iter().enumerate())
            .filter(|&(n, _)| !l.rows.hidden(n))
            .collect();
        let rows = shown
            .iter()
            .map(|(_, row)| {
                let icon = row.icon.map_or(0, |p| pictures.size(p).1);
                self.text_height(e, strings, &row.text, width)
                    .max(l.minicon_height.max(icon))
            })
            .collect();
        Heights {
            rows,
            spacing: l.spacing.max(0),
            inner: dy.saturating_sub(e.y_border.saturating_mul(2)),
            map: (l.rows.hidden.iter().any(|&h| h))
                .then(|| shown.iter().map(|&(n, _)| n).collect()),
        }
    }

    /// A scroll bar: its track, then the two arrows and the thumb, each in the set of its look
    /// (0 normal, 1 under the pointer, 2 held), the animation's box centred on its part.
    pub(super) fn scroll_bar(
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
    pub(super) fn button_text(
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
}
