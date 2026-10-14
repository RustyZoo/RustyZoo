//! `rustyzoo run <map>` without `--headless`: the map view in an SDL3 window, driven by the
//! loop of the original (main loop 3.1, 3.2): take the waiting events, block while the window
//! is minimised or hidden, run one [`App::pass`], present a frame only when the pass painted
//! one, then sleep until the next timer is due instead of spinning.
//!
//! The frame is the game size (the ini file's or `--size`). It goes to a streaming texture that
//! SDL scales to the window with the nearest pixel, by a whole factor `k` of the display scale,
//! so the window has the size of the original's on every system and the picture stays sharp:
//! see [`size::plan`]. Only the program uses this module: the core crate does not link to SDL.
//! The keys become commands in `input`; the loop reads the cursor.
//!
//! The parts: `size` (game and window size), `screen` (the window), `events`, `sound` (the
//! menu music), `wait` (the pacing).

mod events;
mod fullscreen;
mod screen;
mod size;
mod sound;
mod wait;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::take;

use rustyzoo::app::{App, Command};
use rustyzoo::camera::Settings;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::render::MapRenderer;
use rustyzoo::timing::Timers;
use rustyzoo::userdir;
use rustyzoo::vfs::Vfs;
use rustyzoo::zoo::Zoo;
use sdl3::pixels::PixelFormat;
use sdl3::render::ScaleMode;
use sdl3::video::WindowFlags;

use crate::CliError;
use crate::probe::{self, Timing};
use events::{State, asleep, focused, maximised};
use screen::Screen;
use size::ini_fullscreen;
use sound::Sound;
use wait::Wait;

/// What the command line asks of a windowed run.
pub struct Flags<'a> {
    /// `--size`: the game size, instead of the ini file's, and not clamped.
    pub size: Option<(u32, u32)>,
    /// `--log`: where to write the lines a headless run prints.
    pub log: Option<&'a str>,
    /// `--passes`: stop after this many passes; else the player closes the window.
    pub passes: Option<u64>,
    /// `--script`: the commands to apply before the pass of their index.
    pub script: Vec<(u64, Command)>,
    /// `--timing`: run this many milliseconds, then print the intervals of the three timers.
    pub timing: Option<u64>,
    /// `--shot`: write the pixels of the last frame, as the renderer has them, to this PNG.
    pub shot: Option<&'a str>,
}

/// Makes the game full screen or windowed, after asking the window to do it if `ask`; the game is
/// told with a command, and the picture is drawn again whether or not the game paints.
fn switch(
    screen: &mut Screen,
    state: &mut State,
    commands: &mut Vec<Command>,
    on: bool,
    ask: bool,
) {
    if on != screen.fullscreen && (!ask || screen.request(on)) {
        screen.fullscreen = on;
        state.exposed = true;
        commands.push(Command::Fullscreen(on));
        remember(on);
    }
}

/// Writes the mode into the settings of the user folder (`[user] fullscreen`), which the next
/// start reads over the game's ini file. A failure only warns.
fn remember(on: bool) {
    let Some(dir) = userdir::dir() else {
        eprintln!("rustyzoo: warning: no user folder, the screen mode is not remembered");
        return;
    };
    if let Err(e) = userdir::save(&dir, "user", "fullscreen", if on { "1" } else { "0" }) {
        eprintln!("rustyzoo: warning: {e}");
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
    let mut screen = screen::open(&video, ini, flags.size)?;
    let size = screen.size;
    let textures = screen.canvas.texture_creator();
    let mut texture = textures.create_texture_streaming(PixelFormat::RGBA32, size.0, size.1)?;
    texture.set_scale_mode(ScaleMode::Nearest);

    let (mut sound, menu) = Sound::open(&sdl, vfs, ini);
    let timers = Timers::from_config(ini);
    let mut timing = flags
        .timing
        .map(|ms| Timing::new(ms, timers.draw_period(), timers.update_period()));
    let mut app = App::new(
        MapRenderer::new(vfs, catalogue),
        zoo,
        Settings::from_config(ini),
        timers,
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
    // The ini file starts the game in full screen; the camera is told before the first pass, so
    // it applies the zoom limit of a full screen to the stored zoom (camera 4.2, 8.3). The mode
    // is not written back: it is what the settings already say.
    if ini_fullscreen(ini) && screen.request(true) {
        screen.fullscreen = true;
        commands.push(Command::Fullscreen(true));
    }
    let start = sdl3::timer::ticks();
    let mut pass = 0;
    let mut shown = false;
    let mut pointer = None;
    let mut wait = Wait::default();
    let mut seen = false;
    loop {
        while let Some(event) = pump.poll_event() {
            state.note(event, &mut commands);
        }
        // Without the focus the sound is silent, the voices running on.
        sound.focus(focused(screen.canvas.window().window_flags().into()));
        // Minimised or hidden: nothing runs until something happens (a close request still
        // does). The music waits where it is and goes on at the restore, not from its beginning.
        if asleep(screen.canvas.window().window_flags().into())
            && !commands.contains(&Command::Quit)
        {
            sound.hold(true);
            state.note(pump.wait_event(), &mut commands);
            continue;
        }
        sound.hold(false);
        // Alt+Enter and a script ask for the switch and the window makes it; a switch that the
        // window makes on its own (a key of the window manager) is followed. The game is told.
        let window_flags: WindowFlags = screen.canvas.window().window_flags().into();
        if take(&mut state.toggle) && focused(window_flags) {
            let on = !screen.fullscreen;
            switch(&mut screen, &mut state, &mut commands, on, true);
        }
        // The maximise button of the title bar goes to full screen (input 3.4). The window is
        // restored first, so that leaving full screen gives a normal window, not a maximised one.
        if maximised(window_flags, screen.fullscreen) {
            let window = screen.canvas.window_mut();
            window.restore();
            window.sync();
            switch(&mut screen, &mut state, &mut commands, true, true);
        }
        let full = window_flags.contains(WindowFlags::FULLSCREEN);
        if full != seen {
            seen = full;
            switch(&mut screen, &mut state, &mut commands, full, false);
        }
        while let Some((_, command)) = script.next_if(|&(at, _)| at <= pass) {
            match command {
                Command::Fullscreen(on) => switch(&mut screen, &mut state, &mut commands, on, true),
                command => commands.push(command),
            }
        }
        // The edge scroll reads the cursor at its pass, not from motion events: a window
        // that is not active gets none, one that gets the focus back under the pointer does.
        let at = (screen.canvas.window().has_input_focus()
            && screen.canvas.window().has_mouse_focus())
        .then(|| pump.mouse_state())
        .map(|m| screen.pointer((m.x(), m.y())))
        .transpose()?;
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
        if let Some(timing) = &mut timing {
            timing.note(now, app.fired());
        }
        shown |= painted;
        if painted || (shown && std::mem::take(&mut state.exposed)) {
            screen.copy(&texture)?;
            if !screen.canvas.present() {
                return Err(sdl3::get_error().into());
            }
        }
        if let Some(log) = &mut log {
            for line in app.log_lines(pass, now) {
                writeln!(log, "{line}")?;
            }
        }
        sound.fade(&app);
        sound.feed();
        pass += 1;
        let timed = timing.as_ref().is_some_and(|t| t.done(now));
        if app.quit() || timed || flags.passes.is_some_and(|n| pass >= n) {
            break;
        }
        wait.next(&mut pump, &app, start, &mut state, &mut commands);
    }
    if let Some(mut log) = log {
        log.flush()?;
    }
    sound.summary(&app);
    if let Some(timing) = timing {
        timing.summary().iter().for_each(|line| println!("{line}"));
    }
    if let Some(file) = flags.shot {
        // The texture goes to the renderer again, and its pixels are read before the present.
        screen.copy(&texture)?;
        probe::shot(&screen.canvas, file)?;
        screen.canvas.present();
    }
    Ok(())
}
