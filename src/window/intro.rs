//! The intro movies of a windowed run (main menu 3.2): before the start-up picture, in the
//! window, with their sound; a key press or a left or right button press stops the movie that
//! plays and the next one follows; closing the window ends the program. In full screen the
//! pointer is hidden meanwhile.

use std::io::Write;
use std::iter::Peekable;
use std::vec::IntoIter;

use rustyzoo::Framebuffer;
use rustyzoo::app::{Command, Mouse, frame_hash};
use rustyzoo::config::Config;
use rustyzoo::intro::{self, Player};
use rustyzoo::vfs::Vfs;
use sdl3::EventPump;
use sdl3::event::{Event, WindowEvent};
use sdl3::mouse::MouseButton;
use sdl3::render::Texture;
use sdl3::video::WindowFlags;

use super::events::focused;
use super::screen::Screen;
use super::sound::Sound;
use crate::CliError;

/// What an event does to a movie that plays.
#[derive(Debug, PartialEq)]
enum Reaction {
    Nothing,
    /// Stop the movie: a key press, or a left or right button press.
    Skip,
    /// The window was asked to close.
    Quit,
    /// The window must be drawn again.
    Redraw,
}

fn react(event: &Event) -> Reaction {
    match event {
        Event::KeyDown { .. }
        | Event::MouseButtonDown {
            mouse_btn: MouseButton::Left | MouseButton::Right,
            ..
        } => Reaction::Skip,
        Event::Quit { .. }
        | Event::Window {
            win_event: WindowEvent::CloseRequested,
            ..
        } => Reaction::Quit,
        Event::Window {
            win_event:
                WindowEvent::Exposed | WindowEvent::Resized(..) | WindowEvent::PixelSizeChanged(..),
            ..
        } => Reaction::Redraw,
        _ => Reaction::Nothing,
    }
}

/// What the intro needs of the window.
pub struct Stage<'a, 'b> {
    pub sdl: &'a sdl3::Sdl,
    pub screen: &'a mut Screen,
    pub texture: &'a mut Texture<'b>,
    pub pump: &'a mut EventPump,
    pub sound: &'a mut Sound,
}

/// Presents `frame` in the window.
pub fn show(
    screen: &mut Screen,
    texture: &mut Texture,
    frame: &Framebuffer,
) -> Result<(), CliError> {
    texture.update(None, &frame.pixels, frame.width as usize * 4)?;
    screen.copy(texture)?;
    match screen.canvas.present() {
        true => Ok(()),
        false => Err(sdl3::get_error().into()),
    }
}

/// Plays the movies the settings ask for. Writes to the log the lines a headless run prints
/// (`startup movie ...`, then `pass N t=T ...` for what happens), and counts `pass` up for each
/// pass of the loop. Returns whether the run is over: the window was closed or the passes ran out (`limit`).
pub fn play(
    stage: &mut Stage,
    vfs: &Vfs,
    ini: &Config,
    log: &mut Option<impl Write>,
    script: &mut Peekable<IntoIter<(u64, Command)>>,
    pass: &mut u64,
    limit: Option<u64>,
) -> Result<bool, CliError> {
    let (intros, skipped) = intro::list(vfs, ini);
    for why in skipped {
        eprintln!("rustyzoo: warning: {why}");
    }
    if intros.is_empty() {
        return Ok(false);
    }
    let size = stage.screen.size;
    let mut window = Framebuffer::new(size.0, size.1);
    window.fill([0, 0, 0, 255]);
    show(stage.screen, stage.texture, &window)?;
    let hide = stage.screen.fullscreen;
    if hide {
        stage.sdl.mouse().show_cursor(false);
    }
    let result = movies(stage, &intros, &mut window, log, script, pass, limit);
    if hide {
        stage.sdl.mouse().show_cursor(true);
    }
    result
}

fn movies(
    stage: &mut Stage,
    intros: &[intro::Intro],
    window: &mut Framebuffer,
    log: &mut Option<impl Write>,
    script: &mut Peekable<IntoIter<(u64, Command)>>,
    pass: &mut u64,
    limit: Option<u64>,
) -> Result<bool, CliError> {
    let mut say = |line: String| -> Result<(), CliError> {
        if let Some(log) = log {
            writeln!(log, "{line}")?;
        }
        Ok(())
    };
    let origin = sdl3::timer::ticks();
    for intro in intros {
        let movie = &intro.movie;
        let (w, h) = movie.size();
        say(format!(
            "startup movie {} {} frames {} ms {w}x{h}",
            intro.file,
            movie.frame_count(),
            movie.duration_ms()
        ))?;
        let _voice = stage.sound.movie(intro);
        let mut player = Player::new(movie);
        let start = sdl3::timer::ticks();
        let mut redraw = false;
        let mut waited = None;
        loop {
            if limit.is_some_and(|n| *pass >= n) {
                return Ok(true);
            }
            let (ms, now) = (sdl3::timer::ticks() - start, sdl3::timer::ticks() - origin);
            let lead = format!("pass {pass} t={now}");
            *pass += 1;
            let flags: WindowFlags = stage.screen.canvas.window().window_flags().into();
            stage.sound.focus(focused(flags));
            let this = *pass - 1;
            let mut reaction = Reaction::Nothing;
            // The commands of a script come as the keys and buttons would: any key or button
            // skips, the others are dropped (the game is not there yet).
            while let Some((_, command)) = script.next_if(|&(at, _)| at <= this) {
                say(format!("{lead} {command}"))?;
                match command {
                    Command::Quit => reaction = Reaction::Quit,
                    Command::Skip | Command::Key(..) | Command::Mouse(Mouse::Down)
                        if reaction == Reaction::Nothing =>
                    {
                        reaction = Reaction::Skip
                    }
                    _ => {}
                }
            }
            let scripted = reaction != Reaction::Nothing;
            while let Some(event) = waited.take().or_else(|| stage.pump.poll_event()) {
                match react(&event) {
                    Reaction::Nothing => {}
                    Reaction::Redraw => redraw = true,
                    first if reaction == Reaction::Nothing => reaction = first,
                    _ => {}
                }
            }
            match reaction {
                Reaction::Quit => {
                    if !scripted {
                        say(format!("{lead} quit"))?;
                    }
                    return Ok(true);
                }
                Reaction::Skip => {
                    if !scripted {
                        say(format!("{lead} skip"))?;
                    }
                    say(format!("{lead} movie {} skipped", intro.file))?;
                    break;
                }
                _ => {}
            }
            if let Some((n, picture)) = player.at(ms) {
                intro::compose(window, &picture);
                show(stage.screen, stage.texture, window)?;
                redraw = false;
                say(format!(
                    "{lead} movie {} frame {n} {:016x}",
                    intro.file,
                    frame_hash(window)
                ))?;
            } else if std::mem::take(&mut redraw) {
                show(stage.screen, stage.texture, window)?;
            }
            if player.done(ms) {
                say(format!("{lead} movie {} end", intro.file))?;
                break;
            }
            // Sleep until the next frame, or until something happens to the window.
            let wait = player.wait(ms).min(u64::from(u32::MAX));
            waited = stage.pump.wait_event_timeout_ms(wait as u32);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_the_left_and_right_buttons_skip_and_a_close_quits() {
        let key = Event::KeyDown {
            timestamp: 0,
            window_id: 1,
            keycode: None,
            scancode: None,
            keymod: sdl3::keyboard::Mod::NOMOD,
            repeat: false,
            which: 0,
            raw: 0,
        };
        assert_eq!(react(&key), Reaction::Skip);
        let button = |mouse_btn| Event::MouseButtonDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            mouse_btn,
            clicks: 1,
            x: 0.0,
            y: 0.0,
        };
        assert_eq!(react(&button(MouseButton::Left)), Reaction::Skip);
        assert_eq!(react(&button(MouseButton::Right)), Reaction::Skip);
        assert_eq!(react(&button(MouseButton::Middle)), Reaction::Nothing);
        assert_eq!(react(&Event::Quit { timestamp: 0 }), Reaction::Quit);
        let window = |win_event| Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event,
        };
        assert_eq!(react(&window(WindowEvent::CloseRequested)), Reaction::Quit);
        assert_eq!(react(&window(WindowEvent::Exposed)), Reaction::Redraw);
        assert_eq!(react(&window(WindowEvent::Hidden)), Reaction::Nothing);
    }
}
