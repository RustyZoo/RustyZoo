//! The menu music of a windowed run: opened, faded by the model, muted while the window lacks the
//! focus (a minimised window has none), and reported at the end. The loop is kept going by the
//! audio thread, so it runs on, silently, while the window sleeps. A missing sound
//! device or music file is silence, as in the game; a music that fails stops for good.

use rustyzoo::app::App;
use rustyzoo::config::{Config, to_int};
use rustyzoo::intro::{Intro, attenuation};
use rustyzoo::music::{Music, START_LIMIT, global_term};
use rustyzoo::vfs::Vfs;

use sdl3::audio::AudioFormat;
use std::collections::HashMap;

use crate::audio::{Audio, Sound as Wave, Voice};

pub struct Sound {
    /// The output device; kept open as long as the voice plays.
    audio: Option<Audio>,
    music: Option<Voice>,
    /// The voice of each interface sound, by name: a voice that is dropped stops, so it is kept
    /// until it has played.
    clicks: HashMap<&'static str, Voice>,
    /// The voice of each sound the units' animations began, by type and key: each key of
    /// each type has one sound object (sound 9.1), so two units that call it while it plays give
    /// one voice.
    units: HashMap<String, Voice>,
    /// The attenuation sum of the click: `G` from `userAttenuation`, `B` and `F` being 0.
    click_sum: i32,
    /// The output is silent because the window has no focus.
    quiet: Quiet,
    /// The game is paused: the voices that were playing at its start are muted (sound 10).
    paused: bool,
}

/// Whether the output is silenced for lack of the focus. The voices are not touched: they run
/// on, silent, and are heard again where they have got to, as when the system mutes a program
/// that is in the background (sound 10.4, O8).
#[derive(Default)]
struct Quiet(bool);

impl Quiet {
    /// Tells whether the window has the focus; `Some(mute)` when the output has to change.
    fn focus(&mut self, focused: bool) -> Option<bool> {
        (self.0 == focused).then(|| {
            self.0 = !focused;
            !focused
        })
    }
}

/// `[advanced] use8BitSound` is a number other than 0: the 8-bit set is played (sound 2.3). Read once, at start-up.
fn use_8_bit(ini: &Config) -> bool {
    let flag = (ini.section("advanced")).and_then(|s| s.get("use8BitSound"));
    flag.and_then(to_int).is_some_and(|n| n != 0)
}

impl Sound {
    /// Opens the default output device; a failure is a warning and silence.
    pub fn open(sdl: &sdl3::Sdl, ini: &Config) -> Sound {
        let audio = sdl
            .audio()
            .map_err(Into::into)
            .and_then(|a| Audio::open(&a));
        if let Err(e) = &audio {
            eprintln!("rustyzoo: no sound: {e}");
        }
        let mut audio = audio.ok();
        if let Some(audio) = &mut audio {
            audio.eight_bit = use_8_bit(ini);
        }
        Sound {
            audio,
            music: None,
            clicks: HashMap::new(),
            units: HashMap::new(),
            click_sum: global_term(ini),
            quiet: Quiet::default(),
            paused: false,
        }
    }

    /// Mutes the voices that are playing when the game becomes paused or a dialog opens, and
    /// gives them their volume back when it ends (sound 10.2). Sounds that start meanwhile are
    /// not muted. A dialog that closes over a pause does not mute twice: the original's bug
    /// (10.3) is not reproduced.
    pub fn pause(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        let voices = (self.music.iter())
            .chain(self.clicks.values())
            .chain(self.units.values());
        for voice in voices {
            if let Err(e) = voice.mute(paused) {
                eprintln!("rustyzoo: a sound cannot be muted: {e}");
            }
        }
    }

    /// Starts the menu music; the model of it is returned, which the game takes over.
    pub fn start_music(&mut self, vfs: &Vfs, ini: &Config) -> Option<Music> {
        let menu = Music::from_config(ini);
        if let (Some(audio), Some(menu)) = (&self.audio, &menu) {
            self.music = audio
                .menu_music(vfs, menu, true, false)
                .unwrap_or_else(|e| {
                    eprintln!("rustyzoo: no menu music: {e}");
                    None
                });
        }
        menu
    }

    /// Plays the sound of an intro movie, at its own volume, until the voice is dropped. A movie
    /// without sound, or a sound that cannot be played, is silence (the last with a warning).
    pub fn movie(&self, intro: &Intro) -> Option<Voice> {
        let (audio, format) = (self.audio.as_ref()?, intro.movie.audio_format()?);
        let sound = Wave {
            freq: i32::try_from(format.rate).ok()?,
            channels: u8::try_from(format.channels).ok()?,
            format: match format.bits {
                8 => AudioFormat::U8,
                _ => AudioFormat::S16LE,
            },
            data: intro.movie.audio(),
        };
        audio
            .start(sound, attenuation(intro.volume), false, false)
            .map_err(|e| eprintln!("rustyzoo: {}: {e}", intro.file))
            .ok()
    }

    /// Silences the output while the window has no focus, and restores it with the focus.
    pub fn focus(&mut self, focused: bool) {
        if let Some(mute) = self.quiet.focus(focused)
            && let Some(audio) = &self.audio
            && let Err(e) = audio.mute(mute)
        {
            eprintln!("rustyzoo: the sound cannot be muted: {e}");
        }
    }

    /// Plays the sounds the interface asked for in the last pass (`sounds/<name>.wav`). Each
    /// sound has one voice: a sound asked for while its last one plays continues it, at the
    /// volume of the new start, and one that has played to its end starts again. A sound that cannot be played is a
    /// warning.
    pub fn clicks(&mut self, vfs: &Vfs, app: &App) {
        let Some(audio) = &self.audio else { return };
        for name in app.sounds() {
            if let Some(voice) = self.clicks.get(name)
                && voice.queued_bytes().is_ok_and(|bytes| bytes > 0)
            {
                if let Err(e) = voice.set_sum(self.click_sum) {
                    eprintln!("rustyzoo: sounds/{name}.wav: {e}");
                }
                continue;
            }
            let file = format!("sounds/{name}.wav");
            let played = audio
                .load(vfs, &file)
                .and_then(|wave| audio.play(wave, self.click_sum, false, false));
            match played {
                Ok(Some(voice)) => {
                    self.clicks.insert(name, voice);
                }
                Ok(None) => {}
                Err(e) => eprintln!("rustyzoo: {e}"),
            }
        }
    }

    /// Plays the sounds the units' animations began in the last pass as positional one-shots:
    /// `S = G + B + X` with the base term of the key, the distance term of the unit's place, the pan of it, and not started over
    /// 6,000 (sound 4.2, 5). A file that is playing continues with the new volume and pan (9.1). Returns
    /// a line for each, `sound unit ID FILE at X,Y sum S pan P` and what became of it:
    /// `started`, `continued`, `too far` or `failed` (a file that cannot be read or decoded is
    /// silence and a warning).
    pub fn units(&mut self, vfs: &Vfs, app: &App) -> Vec<String> {
        let Some(audio) = &self.audio else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for s in app.unit_sounds() {
            let sum = (self.click_sum.saturating_add(s.base)).saturating_add(s.distance);
            let what = if let Some(voice) = self.units.get(&s.voice)
                && voice.queued_bytes().is_ok_and(|bytes| bytes > 0)
            {
                // Too far: the playing voice stays as it is (sound 4.2).
                if sum > START_LIMIT {
                    "too far"
                } else {
                    (voice.set_sum_x(sum, s.distance))
                        .and_then(|()| voice.set_pan(s.pan))
                        .map_or("failed", |()| "continued")
                }
            } else {
                let played = (audio.load(vfs, &s.file))
                    .and_then(|wave| audio.play_at(wave, s.pan, sum, s.distance));
                match played {
                    Ok(Some(voice)) => {
                        self.units.insert(s.voice.clone(), voice);
                        "started"
                    }
                    Ok(None) => "too far",
                    Err(e) => {
                        eprintln!("rustyzoo: {e}");
                        "failed"
                    }
                }
            };
            lines.push(format!(
                "sound unit {} {} at {},{} sum {sum} pan {} {what}",
                s.id, s.file, s.at.0, s.at.1, s.pan
            ));
        }
        lines
    }

    /// The fade of the menu music: the volume follows the model, which ends by stopping it.
    pub fn fade(&mut self, app: &App) {
        if let Some((model, true)) = app.music()
            && let Some(voice) = &self.music
        {
            match model.stopped() {
                true => self.music = None,
                false => {
                    if let Err(e) = voice.set_sum(model.sum()) {
                        self.fail(e);
                    }
                }
            }
        }
    }

    /// The menu music starts again from its beginning when the game asks: the voice that was
    /// playing, if any, is replaced.
    pub fn restart(&mut self, vfs: &Vfs, app: &App) {
        if !app.music_restarted() {
            return;
        }
        self.music = None;
        if let (Some(audio), Some((model, _))) = (&self.audio, app.music()) {
            self.music = audio
                .menu_music(vfs, model, true, false)
                .unwrap_or_else(|e| {
                    eprintln!("rustyzoo: no menu music: {e}");
                    None
                });
        }
    }

    /// Prints the state of the music at the end of the run.
    pub fn summary(&self, app: &App) {
        match (&self.music, app.music()) {
            (Some(voice), _) => match (voice.gain(), voice.queued_bytes()) {
                (Ok(gain), Ok(bytes)) => println!("music gain {gain:.3} queued {bytes} bytes"),
                (Err(e), _) | (_, Err(e)) => eprintln!("rustyzoo: the menu music state: {e}"),
            },
            (None, Some((model, _))) if model.stopped() => println!("music stopped"),
            _ => println!("music none"),
        }
    }

    fn fail(&mut self, e: Box<dyn std::error::Error>) {
        eprintln!("rustyzoo: the menu music stops: {e}");
        self.music = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_8_bit_set_is_asked_for_by_a_number_other_than_0() {
        let ini = |text: &str| use_8_bit(&Config::parse(text.as_bytes()).unwrap());
        assert!(ini("[advanced]\nuse8BitSound=1\n"));
        assert!(ini("[advanced]\nuse8BitSound = 1\n"));
        for on in ["2", "-1", "1;", "0x10", "99999999999"] {
            assert!(ini(&format!("[advanced]\nuse8BitSound={on}\n")), "{on}");
        }
        for off in [
            "",
            "[advanced]\n",
            "[advanced]\nuse8BitSound=0\n",
            "[advanced]\nuse8BitSound=\n",
            "[advanced]\nuse8BitSound=x\n",
            "[ui]\nuse8BitSound=1\n",
        ] {
            assert!(!ini(off), "{off:?}");
        }
    }

    #[test]
    fn the_output_follows_the_focus_once_per_change() {
        let mut quiet = Quiet::default();
        assert_eq!(
            quiet.focus(true),
            None,
            "a window that starts with the focus"
        );
        assert_eq!(quiet.focus(false), Some(true));
        assert_eq!(quiet.focus(false), None);
        assert_eq!(quiet.focus(true), Some(false));
        assert_eq!(quiet.focus(true), None);
        let mut quiet = Quiet::default();
        assert_eq!(
            quiet.focus(false),
            Some(true),
            "a window that starts without it"
        );
    }
}
