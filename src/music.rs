//! The menu music's loudness and its fade when a game starts (sound specification 4 and 6),
//! without a sound device: the program plays the music and follows what this says.

use crate::config::{Config, to_int};

/// The most a sound may be attenuated for it to start (sound specification 4.2).
pub const START_LIMIT: i32 = 6000;
/// The most the fade term `F` may reach before the music is stopped.
const FADE_END: i32 = 3000;
/// Interface updates that do nothing at the first fade of a session.
const FIRST_DELAY: u32 = 5;

/// The gain for the sum `S = G + B + F + X` of a sound's attenuation terms, in hundredths of a
/// decibel; positive is quieter: `10^(-S/2000)`. A negative sum is replaced by 1 (-0.01 dB): the
/// terms never make a sound louder than full level. The one place that turns attenuation into a
/// volume (sound specification 4.1).
pub fn gain(sum: i32) -> f32 {
    10f32.powf(-(sum.clamp(1, 10_000) as f32) / 2000.0)
}

/// The global term `G` for the `[UI] userAttenuation` setting `u`: the slider position
/// `s = clamp(100 - u div 15, 0, 100)` gives `(100 - s) * 15`, or 5000 at 0 (specification 4.3).
pub fn user_term(u: i32) -> i32 {
    let slider = (100 - u / 15).clamp(0, 100);
    if slider == 0 {
        5000
    } else {
        (100 - slider) * 15
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Playing,
    /// The updates still to skip before the first step of the fade.
    Waiting(u32),
    Fading,
    Stopped,
}

/// The menu music: `S = G + B + F`, with `F` raised by the fade after a game starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Music {
    file: String,
    global: i32,
    base: i32,
    fade: i32,
    state: State,
}

impl Music {
    /// The music the `[UI]` section of the ini file asks for; `None` if `noMenuMusic` is exactly 1
    /// or its sum is over [`START_LIMIT`]: such a music does not start, so it does not fade.
    /// The file is `menuMusic` (`sounds/mainmenu.wav` when absent), `G` comes from
    /// `userAttenuation` and `B` from `menuMusicAttenuation` (0 when absent).
    pub fn from_config(ini: &Config) -> Option<Music> {
        let ui = ini.section("UI");
        let get = |key: &str| ui.and_then(|ui| ui.get(key));
        if get("noMenuMusic").and_then(to_int) == Some(1) {
            return None;
        }
        let music = Music {
            file: get("menuMusic").map_or_else(
                || "sounds/mainmenu.wav".to_string(),
                |name| String::from_utf8_lossy(name).into_owned(),
            ),
            global: user_term(get("userAttenuation").and_then(to_int).unwrap_or(0)),
            base: get("menuMusicAttenuation").and_then(to_int).unwrap_or(0),
            fade: 0,
            state: State::Playing,
        };
        (music.sum() <= START_LIMIT).then_some(music)
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    /// `S = G + B + F`.
    pub fn sum(&self) -> i32 {
        self.global
            .saturating_add(self.base)
            .saturating_add(self.fade)
    }

    pub fn gain(&self) -> f32 {
        gain(self.sum())
    }

    pub fn stopped(&self) -> bool {
        self.state == State::Stopped
    }

    /// A game starts: the fade begins if the music plays and does not fade already. The first
    /// fade waits five interface updates (specification 6.3).
    pub fn start_fade(&mut self) {
        if self.state == State::Playing {
            self.state = State::Waiting(FIRST_DELAY);
        }
    }

    /// One interface update with its nominal period in milliseconds. Returns whether it changed
    /// the music: the volume must be refreshed, or the music has just stopped (see
    /// [`Music::stopped`]). False while waiting, and once stopped.
    pub fn update(&mut self, period: u32) -> bool {
        match self.state {
            State::Playing | State::Stopped => false,
            State::Waiting(left) => {
                self.state = if left > 1 {
                    State::Waiting(left - 1)
                } else {
                    State::Fading
                };
                false
            }
            State::Fading => {
                // The period is below 1000 ms: the ini rate is at least 1 per second.
                self.fade += (period / 2) as i32;
                if self.fade > FADE_END {
                    self.state = State::Stopped;
                }
                true
            }
        }
    }

    /// The line a log gets for the update that last returned true, or for the one that stopped
    /// the music.
    pub fn log_line(&self) -> String {
        match self.stopped() {
            true => "music stopped".to_string(),
            false => format!(
                "music fade {} sum {} gain {:.3}",
                self.fade,
                self.sum(),
                self.gain()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn music(ini: &str) -> Option<Music> {
        Music::from_config(&Config::parse(ini.as_bytes()).unwrap())
    }

    #[test]
    fn positive_attenuation_is_quieter_and_the_sum_is_held_to_the_range() {
        assert_eq!(gain(0), gain(1));
        assert!((gain(-1000) - 0.999).abs() < 1e-3);
        assert!((gain(1000) - 0.316_227_77).abs() < 1e-6);
        assert!((gain(2000) - 0.1).abs() < 1e-6);
        assert_eq!(gain(i32::MAX), gain(10_000));
        assert_eq!(gain(i32::MIN), gain(1));
    }

    #[test]
    fn the_user_term_is_the_requantised_slider() {
        // Specification 4.3: 600 gives 600, 37 gives 30, 1500 and more give 5000.
        let terms: Vec<i32> = [0, 600, 37, 1485, 1499, 1500, 99_999, -5, i32::MIN]
            .map(user_term)
            .to_vec();
        assert_eq!(terms, [0, 600, 30, 1485, 1485, 5000, 5000, 0, 0]);
    }

    #[test]
    fn the_settings_make_the_music_or_none() {
        let m = music("[UI]\nuserAttenuation=600\nmenuMusicAttenuation=-1000\n").unwrap();
        assert_eq!((m.file(), m.sum()), ("sounds/mainmenu.wav", -400));
        let m = music("[UI]\nmenuMusic=sounds/x.wav\n").unwrap();
        assert_eq!((m.file(), m.sum()), ("sounds/x.wav", 0));
        assert!(music("").is_some());
        assert!(music("[UI]\nnoMenuMusic=1\n").is_none());
        assert!(music("[UI]\nnoMenuMusic=2\n").is_some());
    }

    #[test]
    fn the_fade_waits_five_updates_then_steps_until_it_stops() {
        let mut m = music("[UI]\nmenuMusicAttenuation=-1000\n").unwrap();
        // Nothing changes until a game starts.
        assert!(!m.update(66) && m.sum() == -1000);
        m.start_fade();
        assert!((0..5).all(|_| !m.update(66)) && m.sum() == -1000);
        // F goes 33, 66, ..., 2970: 90 steps; the next update stops the music.
        for step in 1..=90 {
            assert!(m.update(66), "step {step}");
            assert_eq!(m.sum(), -1000 + 33 * step);
        }
        assert_eq!(m.log_line(), "music fade 2970 sum 1970 gain 0.104");
        assert!(m.update(66) && m.stopped());
        assert_eq!(m.log_line(), "music stopped");
        // A second start does not bring it back; later updates change nothing.
        m.start_fade();
        assert!(!m.update(66) && m.stopped());
    }

    #[test]
    fn a_fade_in_progress_is_not_started_again() {
        let mut m = music("").unwrap();
        m.start_fade();
        for _ in 0..7 {
            m.update(66);
        }
        let fade = m.sum();
        m.start_fade();
        assert!(m.update(66) && m.sum() == fade + 33);
    }

    #[test]
    fn a_music_too_quiet_to_start_does_not_exist() {
        assert!(music("[UI]\nuserAttenuation=1500\nmenuMusicAttenuation=1000\n").is_some());
        assert!(music("[UI]\nuserAttenuation=1500\nmenuMusicAttenuation=1001\n").is_none());
    }
}
