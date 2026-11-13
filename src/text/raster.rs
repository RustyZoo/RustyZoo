//! Grid-fitted glyph outlines and their coverage bitmaps.
//!
//! The original draws Arial through GDI, which runs the font's instructions in both directions:
//! stems, heights and side bearings sit on whole pixels and only curves and diagonals are grey
//! (text 6.5). The outline is fitted to the grid by the font's hinting, then the exact
//! coverage of each pixel is computed (16 sub-rows, exact horizontally).

use super::Glyph;
use skrifa::GlyphId;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{
    DrawSettings, Engine, HintingInstance, HintingOptions, OutlineGlyphCollection, OutlinePen,
    Target,
};

/// Sub-rows per pixel row.
const ROWS: usize = 16;
/// Segments per curve.
const STEPS: usize = 8;

/// The hinting of every size: stems, heights and side bearings fitted to whole pixels in both
/// directions. The font's own instructions would leave the horizontal direction alone.
pub(super) fn hinter(outlines: &OutlineGlyphCollection<'_>, ppem: u32) -> Option<HintingInstance> {
    let options = HintingOptions {
        engine: Engine::Auto(None),
        target: Target::Mono,
    };
    HintingInstance::new(
        outlines,
        Size::new(ppem as f32),
        LocationRef::default(),
        options,
    )
    .ok()
}

/// The coverage of a glyph at the hinted size; an empty glyph when it has no outline. A glyph that
/// hangs a whole pixel or more out of its `advance` (the bar of `T`, text 6.5 point 2) takes its
/// horizontal fit from the size one pixel smaller, which keeps it inside its cell.
pub(super) fn glyph(
    outlines: &OutlineGlyphCollection<'_>,
    hinter: Option<&HintingInstance>,
    id: GlyphId,
    ppem: u32,
    advance: i32,
) -> Glyph {
    let Some(outline) = outlines.get(id) else {
        return Path::default().fill();
    };
    let draw = |hinter: Option<&HintingInstance>, ppem: u32| {
        let settings = match hinter {
            Some(h) => DrawSettings::hinted(h, false),
            None => DrawSettings::unhinted(Size::new(ppem as f32), LocationRef::default()),
        };
        let mut path = Path::default();
        // A glyph that fails to draw stays empty.
        let _ = outline.draw(settings, &mut path);
        path
    };
    let mut path = draw(hinter, ppem);
    if hinter.is_some() && path.right() >= advance as f32 + 1.0 {
        let narrow = draw(self::hinter(outlines, ppem - 1).as_ref(), ppem - 1);
        if narrow.edges.len() == path.edges.len() {
            for (edge, n) in path.edges.iter_mut().zip(&narrow.edges) {
                edge[0] = n[0];
                edge[2] = n[2];
            }
        }
    }
    path.fill()
}

/// Line segments of a flattened outline, `(x0, y0, x1, y1)` with y up.
#[derive(Default)]
struct Path {
    edges: Vec<[f32; 4]>,
    start: (f32, f32),
    at: (f32, f32),
}

impl Path {
    fn line(&mut self, x: f32, y: f32) {
        self.edges.push([self.at.0, self.at.1, x, y]);
        self.at = (x, y);
    }

    /// The rightmost x of the outline.
    fn right(&self) -> f32 {
        self.edges
            .iter()
            .flat_map(|e| [e[0], e[2]])
            .fold(f32::MIN, f32::max)
    }

    /// Coverage of the pixels the edges cross, with non-zero winding.
    fn fill(&self) -> Glyph {
        let bound = |i: usize, pick: fn(f32, f32) -> f32, init: f32| {
            self.edges
                .iter()
                .flat_map(|e| [e[i], e[i + 2]])
                .fold(init, pick)
        };
        let (x0, x1) = (
            bound(0, f32::min, f32::MAX).floor(),
            bound(0, f32::max, f32::MIN).ceil(),
        );
        let (y0, y1) = (
            bound(1, f32::min, f32::MAX).floor(),
            bound(1, f32::max, f32::MIN).ceil(),
        );
        if self.edges.is_empty() || x1 <= x0 || y1 <= y0 {
            return Glyph {
                width: 0,
                height: 0,
                left: 0,
                bottom: 0,
                coverage: vec![],
            };
        }
        let (width, height) = ((x1 - x0) as usize, (y1 - y0) as usize);
        let mut coverage = vec![0f32; width * height];
        let mut cuts: Vec<(f32, i32)> = vec![];
        for sub in 0..height * ROWS {
            // From the top row down; the sample is the middle of the sub-row.
            let y = y1 - (sub as f32 + 0.5) / ROWS as f32;
            cuts.clear();
            for &[ax, ay, bx, by] in &self.edges {
                if (ay <= y) != (by <= y) {
                    let x = ax + (y - ay) / (by - ay) * (bx - ax);
                    cuts.push((x - x0, if by > ay { 1 } else { -1 }));
                }
            }
            cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
            let row = &mut coverage[sub / ROWS * width..][..width];
            let mut winding = 0;
            for pair in cuts.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    let (a, b) = (pair[0].0.max(0.0), pair[1].0.min(width as f32));
                    for (col, c) in row.iter_mut().enumerate().take(b.ceil() as usize) {
                        *c += (b.min(col as f32 + 1.0) - a.max(col as f32)).max(0.0);
                    }
                }
            }
        }
        Glyph {
            width,
            height,
            left: x0 as i32,
            bottom: y0 as i32,
            coverage: coverage
                .iter()
                .map(|c| (c / ROWS as f32 * 255.0).round().min(255.0) as u8)
                .collect(),
        }
    }
}

impl OutlinePen for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.start = (x, y);
        self.at = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.line(x, y);
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (px, py) = self.at;
        for i in 1..=STEPS {
            let t = i as f32 / STEPS as f32;
            let (u, a, b) = (1.0 - t, (1.0 - t) * (1.0 - t), t * t);
            self.line(
                a * px + 2.0 * u * t * cx + b * x,
                a * py + 2.0 * u * t * cy + b * y,
            );
        }
    }

    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (px, py) = self.at;
        for i in 1..=STEPS {
            let t = i as f32 / STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.line(
                a * px + b * c0x + c * c1x + d * x,
                a * py + b * c0y + c * c1y + d * y,
            );
        }
    }

    fn close(&mut self) {
        if self.at != self.start {
            self.line(self.start.0, self.start.1);
        }
    }
}
