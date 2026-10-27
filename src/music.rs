//! The menu music's loudness and its fade when a game starts (sound specification 4 and 6),
//! without a sound device: the program plays the music and follows what this says.

use crate::config::Config;
use crate::single;
use crate::timing::{ini_value, ini_whole};

/// The most a sound may be attenuated for it to start (sound specification 4.2).
pub const START_LIMIT: i32 = 6000;
/// The most the fade term `F` may reach before the music is stopped.
const FADE_END: i32 = 3000;
/// A step of this many milliseconds or more does not advance the fade.
const LONG_STEP: u32 = 2000;
/// Interface updates that do nothing at the first fade of a session.
const FIRST_DELAY: u32 = 5;

/// The gain for the sum `S = G + B + F + X` of a sound's attenuation terms, in hundredths of a
/// decibel; positive is quieter: `10^(-S/2000)`. A negative sum is replaced by 1 (-0.01 dB): the
/// terms never make a sound louder than full level. The one place that turns attenuation into a
/// volume (sound specification 4.1).
pub fn gain(sum: i32) -> f32 {
    10f32.powf(-(sum.clamp(1, 10_000) as f32) / 2000.0)
}

/// The distance term `X` and the pan of a sound at the view-space point `sound` heard from the
/// view-space point `listener` (the centre of the map view): `X = min(trunc(2.75 * d), 10000)`
/// and `pan = clamp(6 * dx, -10000, 10000)` (sound specification 5). A positive pan quietens the
/// left channel.
pub fn positional(sound: (i64, i64), listener: (i64, i64)) -> (i32, i32) {
    let dx = sound.0.saturating_sub(listener.0);
    let dy = sound.1.saturating_sub(listener.1);
    // Every operation rounded to 24 bits (sound O9).
    let squares = single(single((dx as f64).powi(2)) + single((dy as f64).powi(2)));
    let distance = single(2.75 * single(squares.sqrt())).min(10_000.0) as i32;
    (distance, dx.saturating_mul(6).clamp(-10_000, 10_000) as i32)
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

/// The global term `G` of the `[UI] userAttenuation` setting of the ini file (0 when absent).
pub fn global_term(ini: &Config) -> i32 {
    user_term(ini_whole(ini, "UI", "userAttenuation").unwrap_or(0))
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
        if ini_whole(ini, "UI", "noMenuMusic") == Some(1) {
            return None;
        }
        let music = Music {
            file: ini_value(ini, "UI", "menuMusic").map_or_else(
                || "sounds/mainmenu.wav".to_string(),
                crate::vfs::name_from_bytes,
            ),
            global: global_term(ini),
            base: ini_whole(ini, "UI", "menuMusicAttenuation").unwrap_or(0),
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

    /// Whether it plays at full level and is not fading.
    pub fn playing(&self) -> bool {
        self.state == State::Playing
    }

    pub fn stopped(&self) -> bool {
        self.state == State::Stopped
    }

    /// A game starts: the fade begins if the music plays and does not fade already. The first
    /// fade of a session (`first`) waits five interface updates (specification 6.3).
    pub fn start_fade(&mut self, first: bool) {
        if self.state == State::Playing {
            self.state = match first {
                true => State::Waiting(FIRST_DELAY),
                false => State::Fading,
            };
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
                // A step of 2000 ms or more does not advance the fade. (The period of the
                // interface update is below 1000 ms: the ini rate is at least 1 per second.)
                if period >= LONG_STEP {
                    return false;
                }
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
    fn the_global_term_comes_from_the_user_attenuation_of_the_ini_file() {
        let term = |ini: &str| global_term(&Config::parse(ini.as_bytes()).unwrap());
        assert_eq!(term(""), 0);
        assert_eq!(term("[UI]\nuserAttenuation=600\n"), 600);
        assert_eq!(term("[UI]\nuserAttenuation=37\n"), 30);
        assert_eq!(term("[UI]\nuserAttenuation=-5\n"), 0);
        assert_eq!(term("[UI]\nuserAttenuation=1500\n"), 5000);
        assert_eq!(term("[UI]\nuserAttenuation=x\n"), 0);
        // Names ignore case and an integer reads like `%d`: no `0x` rule.
        assert_eq!(term("[ui]\nUSERATTENUATION=600\n"), 600);
        assert_eq!(term("[UI]\nuserAttenuation=0x258\n"), 0);
    }

    #[test]
    fn a_positional_sound_is_attenuated_and_panned_by_its_place() {
        assert_eq!(positional((1300, 900), (1012, 784)), (853, 1728));
        assert_eq!(positional((5, 5), (5, 5)), (0, 0));
        // Single precision all along: 5328, where double precision gives 5327.
        assert_eq!(positional((1927, 201), (0, 0)).0, 5328);
        assert_eq!(positional((0, 0), (0, 600)), (1650, 0));
        assert_eq!(positional((-300, 0), (0, 0)), (825, -1800));
        assert_eq!(positional((9000, 0), (0, 0)), (10_000, 10_000));
        assert_eq!(positional((i64::MAX, i64::MIN), (0, 0)), (10_000, 10_000));
        assert_eq!(positional((i64::MIN, 0), (i64::MAX, 0)), (10_000, -10_000));
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
        assert!(!m.update(66) && m.sum() == -1000 && m.playing());
        m.start_fade(true);
        assert!(!m.playing());
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
        m.start_fade(true);
        assert!(!m.update(66) && m.stopped());
    }

    #[test]
    fn only_the_first_fade_of_a_session_waits_and_a_long_step_does_not_advance_it() {
        // A later fade steps from its first update.
        let mut m = music("").unwrap();
        m.start_fade(false);
        assert!(m.update(66) && m.sum() == 33);
        // A step of 2000 ms counts as an update of the wait, and moves nothing once it is over.
        let mut m = music("").unwrap();
        m.start_fade(true);
        assert!(!m.update(2000));
        assert!((0..4).all(|_| !m.update(66)) && m.sum() == 0);
        assert!(m.update(66) && m.sum() == 33);
        assert!(!m.update(2000) && !m.update(u32::MAX) && m.sum() == 33);
        let mut m = music("").unwrap();
        m.start_fade(false);
        assert!(!m.update(2000) && m.sum() == 0);
    }

    #[test]
    fn a_fade_in_progress_is_not_started_again() {
        let mut m = music("").unwrap();
        m.start_fade(true);
        for _ in 0..7 {
            m.update(66);
        }
        let fade = m.sum();
        m.start_fade(false);
        assert!(m.update(66) && m.sum() == fade + 33);
    }

    #[test]
    fn a_music_too_quiet_to_start_does_not_exist() {
        assert!(music("[UI]\nuserAttenuation=1500\nmenuMusicAttenuation=1000\n").is_some());
        assert!(music("[UI]\nuserAttenuation=1500\nmenuMusicAttenuation=1001\n").is_none());
    }
}
