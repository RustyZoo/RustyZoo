//! The folder picker of a windowed game that finds no game folder.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustyzoo::install;
use sdl3::dialog::{DialogError, show_open_folder_dialog};
use sdl3::video::Window;

use super::COMMANDS;

/// Whether the command `words` opens a window, the only kind of run that may show the picker.
pub fn wanted(words: &[&str]) -> bool {
    let game = match words.first() {
        Some(&"run") => true,
        Some(first) => !COMMANDS.contains(first) && !first.starts_with('-'),
        None => false,
    };
    game && !words.contains(&"--headless")
}

/// The folder the player picks, kept for the next start when it holds an install. A picker that
/// cannot open, is cancelled or gives another folder gives none, with a warning.
pub fn folder() -> Option<PathBuf> {
    eprintln!(
        "rustyzoo: no game folder found; pick the folder of Zoo Tycoon (the one with zoo.ini)"
    );
    let dir = match ask() {
        Ok(dir) => dir?,
        Err(e) => {
            eprintln!("rustyzoo: warning: no folder picker: {e}");
            return None;
        }
    };
    if !install::usable(&dir) {
        eprintln!(
            "rustyzoo: warning: {} holds no Zoo Tycoon install",
            dir.display()
        );
        return None;
    }
    if let Err(e) = install::remember(&dir) {
        eprintln!("rustyzoo: warning: the game folder is not remembered: {e}");
    }
    Some(dir)
}

/// Shows SDL's folder dialog and waits for the answer; `None` when it is cancelled.
fn ask() -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let sdl = sdl3::init()?;
    let _video = sdl.video()?;
    let mut pump = sdl.event_pump()?;
    let answer = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&answer);
    show_open_folder_dialog(
        None::<PathBuf>,
        false,
        None::<&Window>,
        Box::new(move |result, _| {
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(result);
            }
        }),
    );
    loop {
        pump.wait_event_timeout(Duration::from_millis(100));
        let Some(result) = answer.lock().ok().and_then(|mut slot| slot.take()) else {
            continue;
        };
        return match result {
            Ok(dirs) => Ok(dirs.into_iter().next()),
            Err(DialogError::Canceled) => Ok(None),
            Err(e) => Err(e.to_string().into()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::wanted;

    #[test]
    fn only_a_windowed_game_shows_the_picker() {
        assert!(wanted(&["run"]));
        assert!(wanted(&["run", "maps/default.zoo"]));
        assert!(wanted(&["Saved Games/test.zoo"]));
        assert!(!wanted(&[
            "run",
            "maps/default.zoo",
            "--headless",
            "--passes",
            "1"
        ]));
        assert!(!wanted(&["run", "--headless"]));
        assert!(!wanted(&["Saved Games/test.zoo", "--headless"]));
        assert!(!wanted(&["ls"]));
        assert!(!wanted(&["mcp"]));
        assert!(!wanted(&["--headless"]));
        assert!(!wanted(&[]));
    }
}
