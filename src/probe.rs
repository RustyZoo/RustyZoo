//! What a windowed run can report for a human or CI to read: the intervals between the
//! firings of the three timers on the real clock (`run --timing`), and the pixels the renderer
//! presents (`run --shot`). Information only: no value here fails a run.

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::BufWriter;

use rustyzoo::Framebuffer;
use rustyzoo::app::frame_hash;
use rustyzoo::sprite::write_png;
use rustyzoo::timing::Pass;
use sdl3::pixels::PixelFormat;
use sdl3::render::Canvas;
use sdl3::video::Window;

use crate::CliError;

/// The gaps (ms) between the firings of one timer.
struct Gaps {
    name: &'static str,
    /// The period the timer should keep on a machine that is fast enough.
    expected: u32,
    last: Option<u64>,
    gaps: Vec<u64>,
}

impl Gaps {
    fn fire(&mut self, now: u64) {
        if let Some(last) = self.last {
            self.gaps.push(now - last);
        }
        self.last = Some(now);
    }

    /// `name: n=.. min=.. median=.. p95=.. max=.. expected=..` then the gaps with their counts.
    fn summary(&self) -> String {
        let mut sorted = self.gaps.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        let at = |i: usize| sorted.get(i).copied().unwrap_or(0);
        let mut counts = BTreeMap::new();
        for gap in &sorted {
            *counts.entry(gap).or_insert(0) += 1;
        }
        let histogram: Vec<String> = counts.iter().map(|(g, c)| format!("{g}:{c}")).collect();
        format!(
            "timing {}: n={n} min={} median={} p95={} max={} expected={} gaps(ms:count) {}",
            self.name,
            at(0),
            at(n / 2),
            at((n * 95).div_ceil(100).saturating_sub(1)),
            at(n.saturating_sub(1)),
            self.expected,
            histogram.join(" ")
        )
    }
}

/// The measure of a run of `until` milliseconds.
pub struct Timing {
    until: u64,
    kinds: [Gaps; 3],
}

impl Timing {
    /// `draw` and `update` are the periods of the timers; the step fires when more than the
    /// update period has passed.
    pub fn new(until: u64, draw: u32, update: u32) -> Timing {
        let gaps = |name, expected| Gaps {
            name,
            expected,
            last: None,
            gaps: Vec::new(),
        };
        Timing {
            until,
            kinds: [
                gaps("draw", draw),
                gaps("interface", update),
                gaps("step", update + 1),
            ],
        }
    }

    /// The timers that fired in the pass at the clock `now`.
    pub fn note(&mut self, now: u64, fired: Pass) {
        let [draw, interface, step] = &mut self.kinds;
        if fired.draw {
            draw.fire(now);
        }
        if fired.interface.is_some() {
            interface.fire(now);
        }
        if fired.step.is_some() {
            step.fire(now);
        }
    }

    pub fn done(&self, now: u64) -> bool {
        now >= self.until
    }

    pub fn summary(&self) -> Vec<String> {
        self.kinds.iter().map(Gaps::summary).collect()
    }
}

/// Reads back what the renderer has drawn, in the pixels of the output, and writes it as a PNG.
/// Prints the size, the number of distinct colours and the hash of the pixels; a blank picture
/// (one colour) is an error.
pub fn shot(canvas: &Canvas<Window>, file: &str) -> Result<(), CliError> {
    let surface = canvas
        .read_pixels(None)?
        .convert_format(PixelFormat::RGBA32)?;
    let (width, height) = surface.size();
    let (row, pitch) = (width as usize * 4, surface.pitch() as usize);
    let pixels = surface.with_lock(|bytes| {
        bytes
            .chunks(pitch)
            .take(height as usize)
            .flat_map(|r| r[..row].iter().copied())
            .collect()
    });
    let out = File::create(file).map_err(|e| format!("{file}: {e}"))?;
    let frame = Framebuffer {
        width,
        height,
        pixels,
    };
    write_png(&frame, BufWriter::new(out))?;
    let colours = frame.pixels.chunks(4).collect::<HashSet<_>>().len();
    println!(
        "shot {width}x{height} colours {colours} hash {:016x}",
        frame_hash(&frame)
    );
    match colours {
        0..=1 => Err("the picture read back is blank".into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaps_are_summarised_with_their_histogram() {
        let mut timing = Timing::new(100, 16, 66);
        let fired = |draw, step: bool| Pass {
            draw,
            interface: None,
            step: step.then_some(67),
        };
        for now in [0, 16, 32, 50, 66, 82] {
            timing.note(now, fired(true, now % 2 == 0 && now < 70));
        }
        assert!(!timing.done(99) && timing.done(100));
        let lines = timing.summary();
        assert_eq!(
            lines[0],
            "timing draw: n=5 min=16 median=16 p95=18 max=18 expected=16 gaps(ms:count) 16:4 18:1"
        );
        assert!(
            lines[1].starts_with("timing interface: n=0 min=0 "),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("expected=67"), "{}", lines[2]);
    }
}
