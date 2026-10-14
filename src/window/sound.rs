//! The menu music of a windowed run: opened, faded by the model, muted while the window lacks the
//! focus (a minimised window has none), and reported at the end. The loop is kept going by the
//! audio thread, so it runs on, silently, while the window sleeps. A missing sound
//! device or music file is silence, as in the game; a music that fails stops for good.

use rustyzoo::app::App;
use rustyzoo::config::Config;
use rustyzoo::music::Music;
use rustyzoo::vfs::Vfs;

use crate::audio::{Audio, Voice};

pub struct Sound {
    /// The output device; kept open as long as the voice plays.
    audio: Option<Audio>,
    music: Option<Voice>,
    /// The output is silent because the window has no focus.
    quiet: Quiet,
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

impl Sound {
    /// Opens the device and starts the menu music; the second value is the model of the music,
    /// which the game takes over.
    pub fn open(sdl: &sdl3::Sdl, vfs: &Vfs, ini: &Config) -> (Sound, Option<Music>) {
        let audio = sdl
            .audio()
            .map_err(Into::into)
            .and_then(|a| Audio::open(&a));
        let menu = Music::from_config(ini);
        let music = match (&audio, &menu) {
            (Ok(audio), Some(menu)) => {
                audio
                    .menu_music(vfs, menu, true, false)
                    .unwrap_or_else(|e| {
                        eprintln!("rustyzoo: no menu music: {e}");
                        None
                    })
            }
            (Err(e), _) => {
                eprintln!("rustyzoo: no sound: {e}");
                None
            }
            _ => None,
        };
        let sound = Sound {
            audio: audio.ok(),
            music,
            quiet: Quiet::default(),
        };
        (sound, menu)
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
