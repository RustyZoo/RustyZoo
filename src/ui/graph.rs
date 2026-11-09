//! The graph widget (`UIGraph`, zoo status 21.3): its layout keys, its points, the scale of its
//! axes and the places of its points. Pure numbers; the painter draws them.

use super::layout::{Rgb, color};
use crate::config::{Section, to_flag, to_int};

/// How the points are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    #[default]
    Line,
    Bar,
    Point,
}

/// One point: its x, its value and the label of its x.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub label: Vec<u8>,
}

/// The keys of the widget and what it holds now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    pub style: Style,
    pub points: Vec<Point>,
    /// The currency symbol the money labels are written with.
    pub symbol: String,
    pub vert_padding: i32,
    pub horiz_padding: i32,
    pub x_label_padding: i32,
    pub y_labels: i32,
    pub point_label: bool,
    pub points_on_line: bool,
    pub max_bar: i32,
    pub bar_spacing: i32,
    /// -1: not forced.
    pub forced_min: i32,
    pub forced_max: i32,
    pub y_is_money: bool,
    pub line: Rgb,
    pub bar: Rgb,
    pub y_pos: Rgb,
    pub y_neg: Rgb,
    pub x_label: Rgb,
    pub point_label_color: Rgb,
    pub point_color: Rgb,
    pub axis: Rgb,
}

impl Graph {
    /// The widget of a layout section; a missing key keeps the value of the shipped layouts.
    pub(crate) fn read(s: &Section) -> Graph {
        let int = |key: &str, default: i32| {
            s.get(key)
                .and_then(to_int)
                .unwrap_or(default)
                .clamp(-(FAR as i32), FAR as i32)
        };
        let flag = |key: &str| s.get(key).and_then(to_flag).unwrap_or(false);
        let colour = |key: &str| color(s, key).unwrap_or([0; 3]);
        Graph {
            style: match s.get("graphType").and_then(|v| v.first()) {
                Some(b'b') => Style::Bar,
                Some(b'p') => Style::Point,
                _ => Style::Line,
            },
            points: Vec::new(),
            symbol: String::new(),
            vert_padding: int("VertPaddingPixels", 5),
            horiz_padding: int("HorizPaddingPixels", 20),
            x_label_padding: int("XValLabelPadding", 5),
            y_labels: int("NumYLabels", 10),
            point_label: flag("DrawPointLabel"),
            points_on_line: flag("DrawPointsLineGraph"),
            max_bar: int("MaxBarWidth", 50),
            bar_spacing: int("BarSpacing", 10),
            forced_min: int("ForcedMinY", -1),
            forced_max: int("ForcedMaxY", -1),
            y_is_money: flag("yIsMoney"),
            line: colour("LineColor"),
            bar: colour("BoxColor"),
            y_pos: colour("YLabelColorPos"),
            y_neg: colour("YLabelColorNeg"),
            x_label: colour("XLabelColor"),
            point_label_color: colour("PointLabelColor"),
            point_color: colour("PointColor"),
            axis: colour("AxisLineColor"),
        }
    }

    /// The lowest and highest value shown on the axis (21.3 step 4): 0 is always inside, a forced
    /// bound (not -1) stands, the other is rounded outwards by [`down`] and [`up`].
    pub fn range(&self) -> (i64, i64) {
        let values = self.points.iter().map(|p| i64::from(p.y));
        let (low, high) = (values.clone().min().unwrap_or(0), values.max().unwrap_or(0));
        let (low, high) = (low.min(0), high.max(0));
        let min = match self.forced_min {
            -1 => down(low),
            forced => i64::from(forced),
        };
        let max = match self.forced_max {
            -1 => up(high),
            forced => i64::from(forced),
        };
        (min, max)
    }

    /// The x of the first and the last point.
    fn x_span(&self) -> (i32, i32) {
        let xs = self.points.iter().map(|p| p.x);
        (xs.clone().min().unwrap_or(0), xs.max().unwrap_or(0))
    }

    /// The pixel row of a value inside the plot rectangle `(top, bottom)` (21.3 step 5).
    pub fn y_at(&self, value: i32, (top, bottom): (i32, i32)) -> i32 {
        let (min, max) = self.range();
        let room = (i64::from(bottom) - i64::from(top) - 2 * i64::from(self.vert_padding)) as f32;
        let share = match max > min {
            true => (value as f32 - min as f32) / (max - min) as f32,
            false => 0.0,
        };
        px(i64::from(bottom) - i64::from((share * room) as i32) - i64::from(self.vert_padding))
    }

    /// The pixel column of the point of rank `rank` in a line or point graph inside the plot
    /// rectangle `(left, right)`.
    pub fn x_at_line(&self, rank: usize, (left, right): (i32, i32)) -> i32 {
        let (first, last) = self.x_span();
        let x = self.points.get(rank).map_or(first, |p| p.x);
        let from = i64::from(left) + i64::from(self.horiz_padding);
        if first == last {
            return px(from);
        }
        let share =
            (i64::from(x) - i64::from(first)) as f32 / (i64::from(last) - i64::from(first)) as f32;
        let room = i64::from(right) - 2 * i64::from(self.horiz_padding) - i64::from(left);
        px(i64::from((share * room as f32) as i32) + from)
    }

    /// The width a bar step takes and the left edge of the bar of rank `rank` (21.3 step 5).
    pub fn bar_at(&self, rank: usize, (left, right): (i32, i32)) -> (i32, i32) {
        let n = self.points.len().max(1) as i64;
        let room = i64::from(self.bar_spacing) + i64::from(right)
            - 2 * i64::from(self.horiz_padding)
            - i64::from(left);
        let step = (room.max(0) / n).min(i64::from(self.max_bar.max(0)));
        (
            px(step),
            px(step * rank as i64 + i64::from(left) + i64::from(self.horiz_padding)),
        )
    }

    /// The width a bar is drawn with: its step less the spacing (21.3 step 6).
    pub fn bar_width(&self, (left, right): (i32, i32)) -> i32 {
        let n = self.points.len().max(1) as i64;
        let step = ((i64::from(right) - i64::from(left)) / n).min(i64::from(self.max_bar.max(0)));
        px(step - i64::from(self.bar_spacing))
    }

    /// The text of the Y labels from the top: the value and its text, for the range `(min, max)`
    /// (21.3 step 7). The step is `max((max - min) / (n - 1), 1)` in single precision, the value
    /// cut toward zero.
    pub fn y_values(&self) -> Vec<i32> {
        let (min, max) = self.range();
        let n = self.y_labels.clamp(0, 1000);
        let step = ((max - min) as f32 / (n - 1).max(1) as f32).max(1.0);
        (0..n)
            .map(|k| (max as f32 - k as f32 * step) as i32)
            .collect()
    }
}

/// The farthest a pixel coordinate goes: a point beyond it is drawn there, out of any canvas.
const FAR: i64 = 1 << 14;

/// A pixel coordinate held in a range every sum of two of them fits in.
fn px(v: i64) -> i32 {
    v.clamp(-FAR, FAR) as i32
}

/// The power of ten of the leading digit of `v` (1 for 0).
fn power(v: i64) -> i64 {
    let mut p = 1i64;
    let mut rest = v.unsigned_abs();
    while rest >= 10 {
        rest /= 10;
        p *= 10;
    }
    p
}

/// The top of the axis for the highest value `v` (21.3 step 4): 10 for 0 to 9, 0 for -9 to -1,
/// else one more of the leading digit (57 is 60, 100 is 200, 999 is 1000).
pub fn up(v: i64) -> i64 {
    match v {
        0..=9 => 10,
        -9..=-1 => 0,
        _ => {
            let p = power(v);
            (v / p + 1) * p
        }
    }
}

/// The bottom of the axis for the lowest value `v`: 0 for 0 to 9, -10 for -9 to -1, for -100 and
/// below one more of the two leading digits (-1234 is -1300, -100 is -110), else the leading digit
/// alone, which rounds -99 to -10 toward zero (-57 is -50: the original's bug, the lowest point is
/// drawn below the axis).
pub fn down(v: i64) -> i64 {
    match v {
        0..=9 => 0,
        -9..=-1 => -10,
        ..=-100 => {
            let tenth = power(v) / 10;
            (v / tenth - 1) * tenth
        }
        _ => {
            let p = power(v);
            v / p * p
        }
    }
}

#[cfg(test)]
mod tests;
