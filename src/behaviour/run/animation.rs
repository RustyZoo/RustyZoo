//! The animation a unit holds (sprite drawing 6).

use std::collections::VecDeque;

use super::Timing;

/// What happened that the log and the app are told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Note {
    /// A top set was started.
    Set(String),
    /// The animation shown changed.
    Anim(String),
    /// An entry with a sound began: the key in the unit's `[Sounds]`.
    Sound(String),
}

/// One animation of a sequence, played forward or reversed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(super) name: String,
    pub(super) reverse: bool,
    pub(super) sound: Option<String>,
}

impl Entry {
    pub(super) fn new(name: &str, reverse: bool) -> Entry {
        Entry {
            name: name.to_string(),
            reverse,
            sound: None,
        }
    }
}

/// The animation a unit holds: the sequence being played, the sequences queued, the base
/// animation that loops when nothing else plays, and the frame (sprite drawing 6).
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Animation {
    /// The animation shown.
    pub name: String,
    /// The frame counted up from 0; [`Animation::shown`] is the one drawn.
    pub frame: u32,
    /// Milliseconds the frame has been shown.
    pub elapsed: u32,
    pub(super) reverse: bool,
    /// The base animation plays reversed (`fMoveReverse`, `fFlyReverse`); not in the state hash.
    pub(super) base_reverse: bool,
    /// The frames of the animation shown; 0 if it has none.
    pub(super) count: u32,
    pub(super) base: String,
    /// The cut (sprite drawing 6.2): at the next advance it ends the sequence playing, or the loop
    /// of another animation than the base, and is cleared; it stays while the base shows.
    pub(super) cut: bool,
    /// The sequence being played; empty while the base animation shows.
    pub(super) seq: Vec<Entry>,
    /// The unit's stop-at-end flag: whatever sequence is current holds its last frame when it
    /// ends, for good (behaviours 5.2).
    pub(super) stop: bool,
    pub(super) stopped: bool,
    pub(super) entry: usize,
    pub(super) queue: VecDeque<Vec<Entry>>,
    /// The key of the sound of the current entry, started at the next advance.
    pub(super) sound: Option<String>,
    /// What happened since the notes were last taken.
    pub notes: Vec<Note>,
}

impl Default for Animation {
    fn default() -> Self {
        Animation {
            name: String::new(),
            frame: 0,
            elapsed: 0,
            reverse: false,
            base_reverse: false,
            count: 0,
            base: "idle".to_string(),
            cut: false,
            seq: Vec::new(),
            stop: false,
            stopped: false,
            entry: 0,
            queue: VecDeque::new(),
            sound: None,
            notes: Vec::new(),
        }
    }
}

/// The state hash reads this text: `base_reverse` is not in it, so the hashes stay as they were.
impl std::fmt::Debug for Animation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Animation")
            .field("name", &self.name)
            .field("frame", &self.frame)
            .field("elapsed", &self.elapsed)
            .field("reverse", &self.reverse)
            .field("count", &self.count)
            .field("base", &self.base)
            .field("cut", &self.cut)
            .field("seq", &self.seq)
            .field("stop", &self.stop)
            .field("stopped", &self.stopped)
            .field("entry", &self.entry)
            .field("queue", &self.queue)
            .field("sound", &self.sound)
            .field("notes", &self.notes)
            .finish()
    }
}

impl Animation {
    /// The frame of the animation to draw: the counted one, or counted from the end when the
    /// entry is reversed (sprite drawing 6.3).
    pub(crate) fn shown(&self) -> u32 {
        match self.reverse {
            true => (self.count.saturating_sub(1)).saturating_sub(self.frame),
            false => self.frame,
        }
    }

    /// Whether the base animation shows, nothing else playing.
    pub(crate) fn shows_base(&self) -> bool {
        self.seq.is_empty() && self.name == self.base
    }

    /// Makes `name` the base, without a cut and without clearing the player: what plays goes on
    /// to its end, the loop of the old base included (movement 8.2 step 1).
    pub(crate) fn base_to(&mut self, name: &str) {
        self.base = name.to_string();
        self.base_reverse = false;
    }

    /// Makes the base loop reversed, from its next start (`fMoveReverse`, `fFlyReverse`).
    pub(crate) fn reverse_base(&mut self) {
        self.base_reverse = true;
    }

    /// Sets the cut (sprite drawing 6.2): a forced set start, the stop of a walk, the resets of
    /// the move and idle animations.
    pub(crate) fn cut(&mut self) {
        self.cut = true;
    }

    /// The animation of the current entry; with no sequence, the loop that was installed.
    pub(super) fn current(&self) -> &str {
        self.seq.get(self.entry).map_or(&self.name, |e| &e.name)
    }

    /// Shows the current entry from its first frame: a new name is noted, its sound waits for the
    /// next advance.
    pub(super) fn enter(&mut self, timing: &dyn Fn(&str) -> Option<Timing>) {
        let entry = self.seq.get(self.entry);
        let (name, reverse) = entry.map_or((self.base.clone(), self.base_reverse), |e| {
            (e.name.clone(), e.reverse)
        });
        self.sound = entry.and_then(|e| e.sound.clone());
        self.count = timing(&name).map_or(0, |t| t.count);
        self.reverse = reverse;
        if self.name != name {
            self.notes.push(Note::Anim(name.clone()));
            self.name = name;
        }
    }

    /// The sequence ends: the next queued one starts, else the base animation (sprite drawing
    /// 6.2).
    pub(super) fn finish(&mut self, timing: &dyn Fn(&str) -> Option<Timing>) {
        self.seq = self.queue.pop_front().unwrap_or_default();
        (self.entry, self.frame, self.elapsed) = (0, 0, 0);
        self.enter(timing);
    }

    /// Advances by `dt` milliseconds (sprite drawing 6.2). A frame lasts the delay of its file;
    /// the time left when a sequence ends is dropped.
    pub(crate) fn advance(&mut self, dt: u32, timing: &dyn Fn(&str) -> Option<Timing>) {
        if self.stopped {
            return;
        }
        if self.cut && (!self.queue.is_empty() || !self.shows_base()) {
            self.cut = false;
            self.finish(timing);
        }
        let Some(mut now) = timing(&self.name).filter(|_| self.name == self.current()) else {
            // An entry with no animation (the empty default one) still sounds.
            if dt != 0 && timing(&self.name).is_none() {
                self.notes.extend(self.sound.take().map(Note::Sound));
            }
            self.finish(timing);
            return;
        };
        if self.shows_base() && now.count == 1 && self.queue.is_empty() {
            return;
        }
        if dt != 0 {
            self.notes.extend(self.sound.take().map(Note::Sound));
        }
        self.elapsed = self.elapsed.saturating_add(dt);
        let entries = self.seq.len().max(1);
        while now.delay > 0 && self.elapsed >= now.delay {
            self.elapsed -= now.delay;
            self.frame += 1;
            if self.frame < now.count {
                continue;
            }
            if self.stop {
                (self.stopped, self.elapsed, self.frame) = (true, 0, now.count.saturating_sub(1));
                return;
            }
            // The next entry starts, and whole animations the time covers are skipped.
            loop {
                self.entry += 1;
                if self.entry >= entries {
                    self.finish(timing);
                    return;
                }
                self.frame = 0;
                self.enter(timing);
                let Some(next) = timing(&self.name) else {
                    self.finish(timing);
                    return;
                };
                now = next;
                let whole = now.count.saturating_mul(now.delay);
                if self.elapsed <= whole {
                    break;
                }
                self.elapsed -= whole;
            }
        }
    }

    /// Drops the queued sequences and queues `seq` behind the one playing, the base loop
    /// included: the player takes it when that ends. `base` is the unit's idle animation
    /// (behaviours 5.2, 5.10).
    pub(super) fn queue(&mut self, seq: Vec<Entry>, base: &str) {
        self.queue.clear();
        self.queue.push_back(seq);
        self.base = base.to_string();
    }

    /// Clears the player and shows `name` from its first frame as the base (`fSetBase`).
    pub(crate) fn set_base(&mut self, name: &str, timing: &dyn Fn(&str) -> Option<Timing>) {
        self.reset();
        self.base = name.to_string();
        self.finish(timing);
    }

    /// Drops what is playing and queued; the base animation shows from the next advance.
    pub(super) fn reset(&mut self) {
        self.seq = Vec::new();
        (self.entry, self.frame, self.elapsed) = (0, 0, 0);
        self.queue.clear();
        self.name.clear();
    }
}
