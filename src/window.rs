//! `rustyzoo run <map>` without `--headless`: the map view in an SDL3 window, driven by the
//! loop of the original (main loop 3.1, 3.2): take the waiting events, block while the window
//! is minimised or hidden, run one [`App::pass`], present a frame only when the pass painted
//! one, then sleep until the next timer is due instead of spinning.
//!
//! The frame is as large as the window in points. It goes to a streaming texture that SDL
//! scales to the pixels of the window with the nearest pixel, so a display with a high pixel
//! density shows the same picture with larger pixels. Only the program uses this module: the
//! core crate does not link to SDL. The keys become commands in `input`; the loop reads the cursor.

use std::fs::File;
use std::io::{BufWriter, Write};

use rustyzoo::app::{App, Command};
use rustyzoo::camera::Settings;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::music::Music;
use rustyzoo::render::MapRenderer;
use rustyzoo::timing::{Timers, user_whole};
use rustyzoo::vfs::Vfs;
use rustyzoo::zoo::Zoo;
use sdl3::event::{Event, WindowEvent};
use sdl3::pixels::PixelFormat;
use sdl3::render::ScaleMode;

use crate::CliError;
use crate::audio::{Audio, Voice};
use crate::input;

/// The window when the ini file does not say.
const DEFAULT_SIZE: (u32, u32) = (1024, 768);

/// While the window is minimised or hidden, the music is filled up at least this often.
const MUSIC_FEED_MS: u32 = 250;

/// Queues the music again when it runs low; a music that fails stops for good.
fn feed(music: &mut Option<Voice>) {
    if let Some(voice) = music
        && let Err(e) = voice.update()
    {
        eprintln!("rustyzoo: the menu music stops: {e}");
        *music = None;
    }
}

/// What the command line asks of a windowed run.
pub struct Flags<'a> {
    /// `--size`: the window, instead of the ini file's, and not clamped.
    pub size: Option<(u32, u32)>,
    /// `--log`: where to write the lines a headless run prints.
    pub log: Option<&'a str>,
    /// `--passes`: stop after this many passes; else the player closes the window.
    pub passes: Option<u64>,
    /// `--script`: the commands to apply before the pass of their index.
    pub script: Vec<(u64, Command)>,
}

/// `[user] screenwidth` and `screenheight`; the default if either is missing or not positive.
fn ini_size(ini: &Config) -> (u32, u32) {
    let side = |key| user_whole(ini, key).and_then(|n| u32::try_from(n).ok());
    match (side("screenwidth"), side("screenheight")) {
        (Some(w @ 1..), Some(h @ 1..)) => (w, h),
        _ => DEFAULT_SIZE,
    }
}

/// `size` cut down to the usable area of the display; an area of 0 means that the system does
/// not say, and cuts nothing.
fn clamp(size: (u32, u32), usable: (u32, u32)) -> (u32, u32) {
    let cut = |n: u32, max: u32| if max == 0 { n } else { n.min(max) };
    (cut(size.0, usable.0), cut(size.1, usable.1))
}

/// What the window says about itself: the loop blocks while it is not visible.
#[derive(Default)]
struct State {
    hidden: bool,
    minimised: bool,
    /// The system asks to draw the window again.
    exposed: bool,
}

impl State {
    /// Turns an event into a command or a change of state.
    fn note(&mut self, event: Event, commands: &mut Vec<Command>) {
        if let Some(command) = input::command(&event) {
            commands.push(command);
        }
        match event {
            Event::Quit { .. }
            | Event::Window {
                win_event: WindowEvent::CloseRequested,
                ..
            } => commands.push(Command::Quit),
            Event::Window { win_event, .. } => match win_event {
                WindowEvent::Hidden => self.hidden = true,
                WindowEvent::Shown => self.hidden = false,
                WindowEvent::Minimized => self.minimised = true,
                WindowEvent::Restored => self.minimised = false,
                WindowEvent::Exposed => self.exposed = true,
                _ => {}
            },
            _ => {}
        }
    }

    fn asleep(&self) -> bool {
        self.hidden || self.minimised
    }
}

pub fn run(
    vfs: &Vfs,
    catalogue: &Catalogue,
    zoo: Zoo,
    ini: &Config,
    flags: Flags,
) -> Result<(), CliError> {
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let usable = video.get_primary_display()?.get_usable_bounds()?;
    let want = flags
        .size
        .unwrap_or_else(|| clamp(ini_size(ini), (usable.width(), usable.height())));
    let window = video
        .window("RustyZoo", want.0, want.1)
        .high_pixel_density()
        .build()?;
    // The system may give another size: the game is made for the window it got.
    let size = window.size();
    let pixels = window.size_in_pixels();
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    let output = canvas.output_size()?;
    println!(
        "window {}x{} pixels {}x{} output {}x{}",
        size.0, size.1, pixels.0, pixels.1, output.0, output.1
    );
    if output != pixels {
        return Err(
            format!("the output is {output:?} but the window has {pixels:?} pixels").into(),
        );
    }
    let textures = canvas.texture_creator();
    let mut texture = textures.create_texture_streaming(PixelFormat::RGBA32, size.0, size.1)?;
    texture.set_scale_mode(ScaleMode::Nearest);

    // A missing sound device or music file is silence, as in the game.
    let audio = sdl
        .audio()
        .map_err(Into::into)
        .and_then(|a| Audio::open(&a));
    let menu = Music::from_config(ini);
    let mut music = match (&audio, &menu) {
        (Ok(audio), Some(menu)) => audio
            .menu_music(vfs, menu, true, false)
            .unwrap_or_else(|e| {
                eprintln!("rustyzoo: no menu music: {e}");
                None
            }),
        (Err(e), _) => {
            eprintln!("rustyzoo: no sound: {e}");
            None
        }
        _ => None,
    };

    let mut app = App::new(
        MapRenderer::new(vfs, catalogue),
        zoo,
        Settings::from_config(ini),
        Timers::from_config(ini),
        size,
    )?;
    // The game starts with the run: the menu music fades out.
    app.start_game(menu);
    let mut log = match flags.log {
        Some(file) => {
            let out = File::create(file).map_err(|e| format!("{file}: {e}"))?;
            Some(BufWriter::new(out))
        }
        None => None,
    };
    if let Some(log) = &mut log {
        writeln!(log, "{}", app.log_header())?;
    }

    let mut pump = sdl.event_pump()?;
    let mut script = flags.script.into_iter().peekable();
    let (mut state, mut commands) = (State::default(), Vec::new());
    let start = sdl3::timer::ticks();
    let mut pass = 0;
    let mut shown = false;
    let mut pointer = None;
    loop {
        while let Some(event) = pump.poll_event() {
            state.note(event, &mut commands);
        }
        // Minimised or hidden: nothing runs until something happens (a close request still
        // does), but the music goes on, so it does not start again from its beginning.
        if state.asleep() && !commands.contains(&Command::Quit) {
            if let Some(event) = pump.wait_event_timeout_ms(MUSIC_FEED_MS) {
                state.note(event, &mut commands);
            }
            feed(&mut music);
            continue;
        }
        while let Some((_, command)) = script.next_if(|&(at, _)| at <= pass) {
            commands.push(command);
        }
        // The edge scroll reads the cursor at its pass, not from motion events: a window
        // that is not active gets none, one that gets the focus back under the pointer does.
        let at = (canvas.window().has_input_focus() && canvas.window().has_mouse_focus())
            .then(|| pump.mouse_state())
            .map(|m| (m.x().floor() as i32, m.y().floor() as i32));
        if at != pointer {
            pointer = at;
            commands.push(Command::Pointer(at));
        }
        let now = sdl3::timer::ticks() - start;
        let painted = match app.pass(now, &commands) {
            Some(frame) => {
                texture.update(None, &frame.pixels, size.0 as usize * 4)?;
                true
            }
            None => false,
        };
        commands.clear();
        shown |= painted;
        if painted || (shown && std::mem::take(&mut state.exposed)) {
            canvas.copy(&texture, None, None)?;
            if !canvas.present() {
                return Err(sdl3::get_error().into());
            }
        }
        if let Some(log) = &mut log {
            for line in app.log_lines(pass, now) {
                writeln!(log, "{line}")?;
            }
        }
        // The fade of the menu music: the volume follows the model, which ends by stopping it.
        if let Some((model, true)) = app.music()
            && let Some(voice) = &music
        {
            match model.stopped() {
                true => music = None,
                false => {
                    if let Err(e) = voice.set_sum(model.sum()) {
                        eprintln!("rustyzoo: the menu music stops: {e}");
                        music = None;
                    }
                }
            }
        }
        feed(&mut music);
        pass += 1;
        if app.quit() || flags.passes.is_some_and(|n| pass >= n) {
            break;
        }
        // Sleep until a timer is due or something happens.
        let due = app.next_due(sdl3::timer::ticks() - start);
        if let Some(event) = pump.wait_event_timeout_ms(due.min(u64::from(u32::MAX)) as u32) {
            state.note(event, &mut commands);
        }
    }
    if let Some(mut log) = log {
        log.flush()?;
    }
    match (&music, app.music()) {
        (Some(voice), _) => println!(
            "music gain {:.3} queued {} bytes",
            voice.gain()?,
            voice.queued_bytes()?
        ),
        (None, Some((model, _))) if model.stopped() => println!("music stopped"),
        _ => println!("music none"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_comes_from_the_ini_file_and_is_cut_to_the_display() {
        let ini = |text: &str| ini_size(&Config::parse(text.as_bytes()).unwrap());
        assert_eq!(
            ini("[user]\nScreenWidth=800\nscreenheight=600\n"),
            (800, 600)
        );
        for bad in [
            "",
            "[user]\nscreenwidth=800\n",
            "[user]\nscreenwidth=0\nscreenheight=5\n",
        ] {
            assert_eq!(ini(bad), DEFAULT_SIZE, "{bad:?}");
        }
        assert_eq!(clamp((1024, 768), (800, 1000)), (800, 768));
        assert_eq!(clamp((1024, 768), (0, 0)), (1024, 768), "unknown display");
    }

    #[test]
    fn window_events_decide_whether_the_loop_blocks() {
        let window = |win_event| Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event,
        };
        let (mut state, mut commands) = (State::default(), Vec::new());
        state.note(window(WindowEvent::Minimized), &mut commands);
        assert!(state.asleep());
        state.note(window(WindowEvent::Restored), &mut commands);
        state.note(window(WindowEvent::Hidden), &mut commands);
        assert!(state.asleep());
        state.note(window(WindowEvent::Shown), &mut commands);
        state.note(window(WindowEvent::FocusLost), &mut commands);
        assert!(!state.asleep(), "inactive but visible keeps running");
        assert!(commands.is_empty());
        state.note(window(WindowEvent::CloseRequested), &mut commands);
        state.note(Event::Quit { timestamp: 0 }, &mut commands);
        assert_eq!(commands, [Command::Quit, Command::Quit]);
    }
}
