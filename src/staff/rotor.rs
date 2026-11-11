//! The rotor sound of the helicopter (staff 7.11.2): one sound object per helicopter, its fade term
//! `F` that the take-off and the landing move over 4 seconds, and the order of the steps of an
//! update. The object plays or not as the sound rules say (sound 4.2, 5); the program follows it
//! through [`Rotor::playing`].

use crate::camera::zoom_view;
use crate::music::{REFRESH_LIMIT, START_LIMIT, positional};
use crate::render::order::project;

/// The fade counter at the start and at the end of the fade, in milliseconds.
const FADE_MS: i32 = 4000;
/// The fade term `F` at the start (30 dB down).
const START_FADE: i32 = 3000;

/// Where the sounds are heard from and the global term: the centre of the map view in view space,
/// the camera that places a sound in it, and `G` (sound 4.3, 5). Outside a game: the point (0, 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Hearing {
    pub listener: (i64, i64),
    pub rotation: u8,
    pub zoom: i32,
    pub global: i32,
}

impl Hearing {
    /// The distance term `X` and the pan of a sound at the world point `at`.
    fn of(&self, (x, y, z): (i32, i32, i32)) -> (i32, i32) {
        positional(
            zoom_view(self.zoom, project(self.rotation, x, y, z)),
            self.listener,
        )
    }
}

/// The sound object of a helicopter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rotor {
    /// The file loaded into the object; none when the load failed, and then the object does
    /// nothing at all.
    pub file: Option<String>,
    /// `B`, the base term, and `F`, the fade term.
    base: i32,
    fade: i32,
    /// The fade counter, 4000 down to 0 while the helicopter flies, and the fade flag.
    counter: i32,
    rising: bool,
    pub playing: bool,
    /// `B + F` and the distance term of the volume the sound was last given, and its pan.
    pub heard: (i32, i32),
    pub pan: i32,
    /// Loaded in the air: the start of point 7 waits for the helicopter's first turn.
    pending: bool,
}

impl Rotor {
    /// The sound of a helicopter loaded in the air (point 7), started at its first turn, before its first movement.
    pub(crate) fn loaded_in_air() -> Rotor {
        Rotor {
            pending: true,
            ..Rotor::new(None, 0)
        }
    }

    fn new(file: Option<String>, base: i32) -> Rotor {
        Rotor {
            file,
            base,
            fade: 0,
            counter: 0,
            rising: false,
            playing: false,
            heard: (0, 0),
            pan: 0,
            pending: false,
        }
    }

    pub(crate) fn pending(&self) -> bool {
        self.pending
    }

    /// The fade term `F`.
    #[cfg(test)]
    pub(crate) fn level(&self) -> i32 {
        self.fade
    }

    /// Point 3: the object made with the sound `loaded` (its file and `B`; none for a load that
    /// failed, which keeps the object and does nothing more), at the helicopter's point `at`.
    pub(crate) fn start(
        loaded: Option<(String, i32)>,
        at: (i32, i32, i32),
        hear: &Hearing,
    ) -> Rotor {
        let Some((file, base)) = loaded else {
            return Rotor::new(None, 0);
        };
        let mut r = Rotor::new(Some(file), base);
        r.fade = START_FADE;
        let (x, pan) = hear.of(at);
        let sum = hear.global.saturating_add(base).saturating_add(START_FADE);
        if sum.saturating_add(x) <= START_LIMIT {
            r.playing = true;
            (r.heard, r.pan) = ((base.saturating_add(START_FADE), x), pan);
        }
        (r.rising, r.counter) = (true, FADE_MS);
        r
    }

    /// Point 7: what a start leaves to a helicopter that was loaded in the air.
    pub(crate) fn settle(&mut self) {
        (self.fade, self.rising, self.counter, self.pending) = (0, false, 0, false);
    }

    /// Landing: the fade flag goes up (point 5).
    pub(crate) fn raise(&mut self) {
        self.rising = true;
    }

    /// Point 4, after an update of `ms` milliseconds of a helicopter at `at` that `flying` or not:
    /// the position refresh, the fade with its volume refresh, the end of the fade. `false` when
    /// the sound was stopped and the object freed.
    pub(crate) fn update(
        &mut self,
        flying: bool,
        ms: u32,
        at: (i32, i32, i32),
        hear: &Hearing,
    ) -> bool {
        let loaded = self.file.is_some();
        if loaded && self.playing {
            let (x, pan) = hear.of(at);
            let sum = (hear.global.saturating_add(self.base)).saturating_add(self.fade);
            if sum.saturating_add(x) > REFRESH_LIMIT {
                self.playing = false;
            } else {
                (self.heard, self.pan) = ((self.base.saturating_add(self.fade), x), pan);
            }
        }
        if !self.rising {
            return true;
        }
        let k = if flying { -1 } else { 1 };
        let ms = i32::try_from(ms).unwrap_or(i32::MAX);
        self.counter = self.counter.saturating_add(k * ms).clamp(0, FADE_MS);
        self.fade = (f64::from(self.counter) * 0.00025 * 3000.0).round() as i32;
        // The volume refresh leaves the distance term out.
        self.heard = (self.base.saturating_add(self.fade), 0);
        match (flying, self.counter) {
            (true, 0) => self.rising = false,
            (false, FADE_MS) if loaded && self.playing => return false,
            _ => {}
        }
        true
    }
}

#[cfg(test)]
mod tests;
