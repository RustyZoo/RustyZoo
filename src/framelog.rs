//! The log lines of a run, with the hash of each drawn frame computed beside the passes.

use std::collections::VecDeque;
use std::thread::JoinHandle;

use crate::Framebuffer;
use crate::app::{App, frame_hashes};

/// What stands for the hash in the line of a drawn frame until the hash is known.
const HOLE: &str = "\u{1}";
const DRAW_HOLE: &str = " draw \u{1}";

/// The passes held back when the oldest is written, so that its hash was computed meanwhile.
const HELD: usize = 12;

/// The passes held back from the output: the hash of a frame takes longer than a pass, so the
/// frames are hashed four at a time in a thread (see [`frame_hashes`]) while the next passes run,
/// and the lines of a pass are written once its hash is known.
#[derive(Default)]
pub struct Lines {
    /// The lines of the passes, oldest first, and whether the pass drew a frame.
    held: VecDeque<(Vec<String>, bool)>,
    /// The frames not sent to a thread yet.
    waiting: Vec<Framebuffer>,
    /// The threads, oldest first, with the frames they hash.
    running: VecDeque<JoinHandle<(Vec<u64>, Vec<Framebuffer>)>>,
    /// The pixel buffers of hashed frames, kept for the next frames.
    spare: Vec<Vec<u8>>,
    /// The hashes of the oldest frames, oldest first.
    known: VecDeque<u64>,
}

impl Lines {
    /// Takes the lines of the pass `pass` of `app` and writes, with `out`, those of the passes
    /// whose frame is hashed by now.
    pub fn pass(
        &mut self,
        app: &App,
        pass: u64,
        now: u64,
        out: &mut dyn FnMut(&str),
    ) -> Result<(), String> {
        let mut frame = None;
        let lines = app.log_lines_with(pass, now, |drawn| {
            let mut pixels = self.spare.pop().unwrap_or_default();
            pixels.clear();
            pixels.extend_from_slice(&drawn.pixels);
            frame = Some(Framebuffer {
                width: drawn.width,
                height: drawn.height,
                pixels,
            });
            HOLE.into()
        });
        self.push(out, lines, frame)
    }

    fn push(
        &mut self,
        out: &mut dyn FnMut(&str),
        lines: Vec<String>,
        frame: Option<Framebuffer>,
    ) -> Result<(), String> {
        self.held.push_back((lines, frame.is_some()));
        self.waiting.extend(frame);
        if self.waiting.len() == 4 {
            self.start();
        }
        while self.held.len() > HELD {
            self.write_oldest(out)?;
        }
        Ok(())
    }

    /// Writes the lines of the passes still held.
    pub fn finish(&mut self, out: &mut dyn FnMut(&str)) -> Result<(), String> {
        while !self.held.is_empty() {
            self.write_oldest(out)?;
        }
        Ok(())
    }

    /// Sends the waiting frames to a thread.
    fn start(&mut self) {
        let frames = std::mem::take(&mut self.waiting);
        self.running.push_back(std::thread::spawn(move || {
            let hashes = frame_hashes(&frames.iter().collect::<Vec<_>>());
            (hashes, frames)
        }));
    }

    fn write_oldest(&mut self, out: &mut dyn FnMut(&str)) -> Result<(), String> {
        let Some((lines, drawn)) = self.held.pop_front() else {
            return Ok(());
        };
        let mut hash = None;
        if drawn {
            if self.known.is_empty() {
                if self.running.is_empty() {
                    self.start();
                }
                let thread = (self.running.pop_front()).ok_or("the frame hash is missing")?;
                let (hashes, frames) = thread.join().map_err(|_| "the frame hash failed")?;
                self.known.extend(hashes);
                self.spare.extend(frames.into_iter().map(|f| f.pixels));
            }
            let first = self.known.pop_front().ok_or("the frame hash is missing")?;
            hash = Some(format!("{first:016x}"));
        }
        for line in lines {
            match &hash {
                Some(hash) if line.contains(DRAW_HOLE) => out(&line.replacen(HOLE, hash, 1)),
                _ => out(&line),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::frame_hash;

    #[test]
    fn lines_wait_for_their_frame_hash_and_keep_their_order() {
        let frame = |k: usize| Framebuffer {
            width: 3,
            height: 1,
            pixels: (0..12).map(|i| (i * k) as u8).collect(),
        };
        let mut got = Vec::new();
        let mut want = Vec::new();
        let mut lines = Lines::default();
        // Passes that draw (every third does not), more than the passes held back.
        for pass in 0..50 {
            let drawn = pass % 3 != 0;
            let hash = format!("{:016x}", frame_hash(&frame(pass + 1)));
            let shown = if drawn { HOLE } else { "-" };
            let log = vec![
                format!("pass {pass} a"),
                format!("pass {pass} draw {shown}"),
            ];
            want.push(log[0].clone());
            want.push(format!(
                "pass {pass} draw {}",
                if drawn { &hash } else { "-" }
            ));
            let frame = drawn.then(|| frame(pass + 1));
            lines
                .push(&mut |l| got.push(l.to_string()), log, frame)
                .unwrap();
        }
        // The held passes come out at the end, a batch of fewer than four frames included.
        assert!(got.len() < want.len());
        lines.finish(&mut |l| got.push(l.to_string())).unwrap();
        assert_eq!(got, want);
        assert!(lines.held.is_empty() && lines.known.is_empty() && lines.running.is_empty());
        assert!(lines.waiting.is_empty() && !lines.spare.is_empty());
    }
}
