//! The intro movies (main menu 3.2): which ones play, and the player that turns a clock reading
//! into the picture to show. No window and no sound device: the caller presents the picture and
//! plays the audio.

use crate::Framebuffer;
use crate::config::{Config, to_int};
use crate::movie::{Frames, Movie};
use crate::startup::copy_picture;
use crate::vfs::Vfs;

/// A movie that plays at start-up.
pub struct Intro {
    /// The file as the game names it.
    pub file: &'static str,
    pub movie: Movie,
    /// The movie's own volume, in hundredths of a decibel (`[UI] movievolume1` or `2`): zero or
    /// negative makes it quieter by that much, as a media player reads it.
    pub volume: i32,
}

/// The movies to play, in order: the first when `[UI] playMovie` is not 0, the second when
/// `[UI] playSecondMovie` is not 0 as well. A movie that is missing or cannot be read is skipped,
/// with its reason in the second value.
pub fn list(vfs: &Vfs, ini: &Config) -> (Vec<Intro>, Vec<String>) {
    let ui = ini.section("UI");
    let get = |key: &str| ui.and_then(|s| s.get(key)).and_then(to_int).unwrap_or(0);
    let mut movies = Vec::new();
    let mut skipped = Vec::new();
    let first = get("playMovie") != 0;
    for (file, play, volume) in [
        ("MSLOGO.AVI", first, "movievolume1"),
        (
            "BFLOGO.AVI",
            first && get("playSecondMovie") != 0,
            "movievolume2",
        ),
    ] {
        if !play {
            continue;
        }
        match vfs
            .read(file)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Movie::parse(bytes).map_err(|e| e.to_string()))
        {
            Ok(movie) => movies.push(Intro {
                file,
                movie,
                volume: get(volume),
            }),
            Err(why) => skipped.push(format!("{file} skipped: {why}")),
        }
    }
    (movies, skipped)
}

/// The decibels-to-attenuation sum of a movie volume: a media player's volume of -1000 is 10 dB
/// down, which is the sum 1000 of the sound specification; a positive volume is full level.
pub fn attenuation(volume: i32) -> i32 {
    volume.saturating_neg().max(0)
}

/// Plays one movie against a clock: [`Player::at`] gives the picture for a time, once per frame.
pub struct Player<'a> {
    movie: &'a Movie,
    frames: Frames<'a>,
    /// The index of the frame last given.
    shown: Option<usize>,
    /// The decoder failed or ran out of frames.
    ended: bool,
}

impl<'a> Player<'a> {
    pub fn new(movie: &'a Movie) -> Player<'a> {
        Player {
            movie,
            frames: movie.frames(),
            shown: None,
            ended: false,
        }
    }

    /// The frame that is due `ms` milliseconds after the start.
    fn due(&self, ms: u64) -> usize {
        let (rate, scale) = self.movie.frame_rate();
        let frame = u128::from(ms) * u128::from(rate) / (1000 * u128::from(scale.max(1)));
        usize::try_from(frame)
            .unwrap_or(usize::MAX)
            .min(self.movie.frame_count().saturating_sub(1))
    }

    /// The picture and its frame number when the frame due at `ms` is not the one given last; a
    /// frame that is late makes the ones before it be skipped. A frame that cannot be decoded
    /// ends the movie.
    pub fn at(&mut self, ms: u64) -> Option<(usize, Framebuffer)> {
        if self.ended || self.movie.frame_count() == 0 {
            return None;
        }
        let want = self.due(ms);
        let mut next = self.shown.map_or(0, |n| n + 1);
        let mut picture = None;
        while next <= want {
            match self.frames.next() {
                Some(Ok(frame)) => picture = Some((next, frame)),
                _ => {
                    self.ended = true;
                    break;
                }
            }
            next += 1;
        }
        if let Some((n, _)) = &picture {
            self.shown = Some(*n);
        }
        picture
    }

    /// Milliseconds from `ms` to the next thing that changes: the next frame, or the end.
    pub fn wait(&self, ms: u64) -> u64 {
        let (rate, scale) = self.movie.frame_rate();
        let next = self.shown.map_or(0, |n| n + 1);
        let due = (next as u128 * 1000 * u128::from(scale)).div_ceil(u128::from(rate.max(1)));
        let due = u64::try_from(due).unwrap_or(u64::MAX);
        let end = self.movie.duration_ms();
        let at = if next >= self.movie.frame_count() || self.ended {
            end
        } else {
            due.min(end)
        };
        at.saturating_sub(ms)
    }

    /// Whether the movie is over `ms` milliseconds after its start.
    pub fn done(&self, ms: u64) -> bool {
        self.ended || ms >= self.movie.duration_ms()
    }
}

/// The window of the frame: black, the picture of the movie in the middle.
pub fn compose(window: &mut Framebuffer, picture: &Framebuffer) {
    window.fill([0, 0, 0, 255]);
    let origin = (
        (window.width as i32 - picture.width as i32) / 2,
        (window.height as i32 - picture.height as i32) / 2,
    );
    copy_picture(window, picture, origin);
}

#[cfg(test)]
mod tests;
