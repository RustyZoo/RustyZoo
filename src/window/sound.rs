//! The menu music of a windowed run: opened, fed, held while the window sleeps, faded by the
//! model, and reported at the end. A missing sound device or music file is silence, as in the
//! game; a music that fails stops for good.

use rustyzoo::app::App;
use rustyzoo::config::Config;
use rustyzoo::music::Music;
use rustyzoo::vfs::Vfs;

use crate::audio::{Audio, Voice};

pub struct Sound {
    /// Kept open as long as the voice plays.
    _audio: Option<Audio>,
    music: Option<Voice>,
    /// The music is paused.
    held: bool,
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
            _audio: audio.ok(),
            music,
            held: false,
        };
        (sound, menu)
    }

    /// Queues the music again when it runs low.
    pub fn feed(&mut self) {
        if let Some(voice) = &self.music
            && let Err(e) = voice.update()
        {
            self.fail(e);
        }
    }

    /// Pauses or resumes the music where it is, once per change.
    pub fn hold(&mut self, hold: bool) {
        if hold == self.held {
            return;
        }
        self.held = hold;
        if let Some(voice) = &self.music
            && let Err(e) = if hold { voice.pause() } else { voice.resume() }
        {
            self.fail(e);
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
