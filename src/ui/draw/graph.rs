//! Drawing a graph (zoo status 21.3 steps 6 and 7): the axes, the labels, the line or the bars.

use super::*;
use crate::text::format::{self, integer, money};
use crate::ui::graph::{Graph, Style as GraphStyle};

/// The currency symbol as the string table of the money formats.
struct Symbol<'a>(&'a str);

impl format::Strings for Symbol<'_> {
    fn string(&self, id: u32) -> &str {
        if id == 22100 { self.0 } else { "" }
    }
}

/// A value as a label: whole units of money with grouping, or its digits.
fn label(g: &Graph, value: i64) -> Vec<u8> {
    let text = match g.y_is_money {
        true => money(&Symbol(&g.symbol), value, true),
        false => integer(value, false),
    };
    to_cp1252(&text)
}

/// The width in pixels of the Y labels column (21.3 step 7).
fn column(font: &text::Font, g: &Graph) -> i32 {
    let (min, max) = g.range();
    if g.y_is_money {
        let wide = font.width(&label(g, min)).max(font.width(&label(g, max)));
        return wide.saturating_add(12);
    }
    let digits = min
        .unsigned_abs()
        .to_string()
        .len()
        .max(max.unsigned_abs().to_string().len());
    (3 + digits as i32).saturating_mul(font.advance(b'0'))
}

/// A line of one pixel from `a` to `b`, clipped.
fn line(canvas: &mut Framebuffer, a: (i32, i32), b: (i32, i32), color: [u8; 4]) {
    let (mut x, mut y) = (i64::from(a.0), i64::from(a.1));
    let (dx, dy) = ((i64::from(b.0) - x).abs(), -(i64::from(b.1) - y).abs());
    let (sx, sy) = (
        if x < i64::from(b.0) { 1 } else { -1 },
        if y < i64::from(b.1) { 1 } else { -1 },
    );
    let mut error = dx + dy;
    // The longest line is a few thousand pixels.
    for _ in 0..=dx - dy {
        fill_rect(
            canvas,
            (x as i32, y as i32, x as i32 + 1, y as i32 + 1),
            color,
        );
        if (x, y) == (i64::from(b.0), i64::from(b.1)) {
            break;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x += sx;
        }
        if twice <= dx {
            error += dx;
            y += sy;
        }
    }
}

/// A disc inside the square `(x - 3, y - 3)` to `(x + 4, y + 4)`.
fn dot(canvas: &mut Framebuffer, (x, y): (i32, i32), color: [u8; 4]) {
    for dy in -3i32..=3 {
        for dx in -3i32..=3 {
            if dx * dx + dy * dy <= 12 {
                fill_rect(canvas, (x + dx, y + dy, x + dx + 1, y + dy + 1), color);
            }
        }
    }
}

impl Painter {
    /// The graph of element `e`: the background, the Y labels from the top, the axes, the points
    /// and their labels, and the X labels.
    pub(super) fn graph(
        &mut self,
        e: &Element,
        g: &Graph,
        strings: &dyn Strings,
        canvas: &mut Framebuffer,
    ) {
        let (w, h) = (canvas.width as i32, canvas.height as i32);
        canvas.fill(fill_color(e.look.back_color));
        let font = self.font(e, strings);
        let text_height = font.line_height();
        let (l, t) = (column(font, g).min(w - 1).max(0), 0);
        let (r, b) = (w, (h - g.x_label_padding - text_height).max(t + 1));
        let say = |canvas: &mut Framebuffer,
                   font: &mut text::Font,
                   text: &[u8],
                   (x, y): (i32, i32),
                   color: [u8; 3],
                   justify: text::Justify| {
            let width = font.width(text);
            let left = match justify {
                text::Justify::Left => x,
                text::Justify::Center => x - width / 2,
                text::Justify::Right => x - width,
            };
            let rect = Rect {
                left,
                top: y,
                right: left + width,
                bottom: y + text_height,
            };
            let style = Style {
                color,
                justify: text::Justify::Left,
                shadow: false,
            };
            text::draw(canvas, font, rect, text, &style);
        };
        // The Y labels, equally spaced from the top of the plot.
        let values = g.y_values();
        let (top, room) = (t + g.vert_padding, b - t - 2 * g.vert_padding);
        for (k, &value) in values.iter().enumerate() {
            let y = top + (room as f32 * k as f32 / (values.len() - 1).max(1) as f32) as i32;
            let color = if (value as f32) < -0.9 {
                g.y_neg
            } else {
                g.y_pos
            };
            let text = label(g, i64::from(value));
            say(
                canvas,
                font,
                &text,
                (l - 3, y - text_height / 2),
                color,
                text::Justify::Right,
            );
        }
        let axis = fill_color(g.axis);
        line(canvas, (l, t), (l, b), axis);
        line(canvas, (l, b), (r, b), axis);
        let n = g.points.len();
        let at = |i: usize| match g.style {
            GraphStyle::Bar => {
                let (_, x) = g.bar_at(i, (l, r));
                x + g.bar_width((l, r)) / 2
            }
            _ => g.x_at_line(i, (l, r)),
        };
        let ys: Vec<i32> = g.points.iter().map(|p| g.y_at(p.y, (t, b))).collect();
        match g.style {
            GraphStyle::Bar => {
                let zero = g.y_at(0, (t, b));
                for (i, &y) in ys.iter().enumerate() {
                    let (_, x) = g.bar_at(i, (l, r));
                    let rect = (x, y.min(zero), x + g.bar_width((l, r)), y.max(zero) + 1);
                    fill_rect(canvas, rect, fill_color(g.bar));
                }
            }
            style => {
                for i in 0..n {
                    let (x, y) = (g.x_at_line(i, (l, r)), ys[i]);
                    if style == GraphStyle::Line && i + 1 < n {
                        let gap = if g.points_on_line { 5 } else { 0 };
                        let next = (g.x_at_line(i + 1, (l, r)), ys[i + 1]);
                        line(canvas, (x + gap, y), next, fill_color(g.line));
                    }
                    if g.points_on_line || style == GraphStyle::Point {
                        dot(canvas, (x, y), fill_color(g.point_color));
                    }
                }
            }
        }
        for (i, p) in g.points.iter().enumerate() {
            if g.point_label {
                let text = label(g, i64::from(p.y));
                let half = font.width(&text) / 2;
                let x = at(i).clamp(l + half, (r - half).max(l + half));
                let y = (ys[i] - 2 - text_height).max(t);
                say(
                    canvas,
                    font,
                    &text,
                    (x, y),
                    g.point_label_color,
                    text::Justify::Center,
                );
            }
        }
        // The X labels stop at the first point whose x does not follow the one before.
        let mut previous = None;
        for (i, p) in g.points.iter().enumerate() {
            if previous.is_some_and(|x| p.x != x + 1) {
                break;
            }
            previous = Some(p.x);
            say(
                canvas,
                font,
                &p.label,
                (at(i), b + g.x_label_padding),
                g.x_label,
                text::Justify::Center,
            );
        }
    }
}
