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

mod cursor;
mod events;
mod fullscreen;
mod intro;
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
use rustyzoo::money::Money;
use rustyzoo::render::MapRenderer;
use rustyzoo::timing::Timers;
use rustyzoo::userdir;
use rustyzoo::vfs::Vfs;
use rustyzoo::zoo::Zoo;
use sdl3::pixels::PixelFormat;
use sdl3::render::ScaleMode;
use sdl3::video::WindowFlags;

use crate::CliError;
use crate::boot::boot;
use crate::probe::{self, Timing};
use cursor::Cursors;
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
    /// `--show`: layouts of the interface to show at the start.
    pub show: &'a [i32],
    /// `--rows` and `--text`: what to put into the interface at the start.
    pub fill: &'a crate::layout::Fill,
    /// `--bare`: the game shows the map view alone.
    pub bare: bool,
    /// `--seed`: a test run, the generator seeded from this number.
    pub seed: Option<u32>,
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

/// Writes the choice of the alternate cursors into the settings of the user folder
/// (`[UI] useAlternateCursors`), which the next start reads over the game's ini file. A failure
/// only warns.
fn remember_cursors(on: bool) {
    let Some(dir) = userdir::dir() else {
        eprintln!("rustyzoo: warning: no user folder, the cursors are not remembered");
        return;
    };
    if let Err(e) = userdir::save(
        &dir,
        "UI",
        "useAlternateCursors",
        if on { "1" } else { "0" },
    ) {
        eprintln!("rustyzoo: warning: {e}");
    }
}

/// Plays the map `loaded` (its types and the zoo), or, without one, starts up to the main menu.
pub fn run(
    vfs: &Vfs,
    loaded: Option<(Catalogue, Zoo, Money)>,
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

    let mut sound = Sound::open(&sdl, ini);
    let mut pump = sdl.event_pump()?;
    let mut script = flags.script.into_iter().peekable();
    let mut commands = Vec::new();
    // The ini file starts the game in full screen; the camera is told before the first pass, so
    // it applies the zoom limit of a full screen to the stored zoom (camera 4.2, 8.3). The mode
    // is not written back: it is what the settings already say.
    if ini_fullscreen(ini) && screen.request(true) {
        screen.fullscreen = true;
        commands.push(Command::Fullscreen(true));
    }
    let mut log = match flags.log {
        Some(file) => {
            let out = File::create(file).map_err(|e| format!("{file}: {e}"))?;
            Some(BufWriter::new(out))
        }
        None => None,
    };
    if let Some(log) = &mut log {
        writeln!(log, "size {}x{}", size.0, size.1)?;
    }
    let mut pass = 0;
    let (mut menu, mut booted, mut start_lines) = (None, None, Vec::new());
    if loaded.is_some() {
        menu = sound.start_music(vfs, ini);
    } else {
        // The intro movies, then the game data loaded behind the start-up picture, which is
        // presented each time the bar moves; the menu music starts in the middle of it.
        let mut stage = intro::Stage {
            sdl: &sdl,
            screen: &mut screen,
            texture: &mut texture,
            pump: &mut pump,
            sound: &mut sound,
        };
        if intro::play(
            &mut stage,
            vfs,
            ini,
            &mut log,
            &mut script,
            &mut pass,
            flags.passes,
        )? {
            if let Some(mut log) = log {
                log.flush()?;
            }
            return Ok(());
        }
        let mut present =
            |frame: &rustyzoo::Framebuffer| intro::show(&mut screen, &mut texture, frame);
        let mut music = || menu = sound.start_music(vfs, ini);
        let made = boot(vfs, ini, size, flags.show, &mut present, &mut music)?;
        start_lines = made.lines.clone();
        booted = Some(made);
    }
    let timers = Timers::from_config(ini);
    let mut timing = flags
        .timing
        .map(|ms| Timing::new(ms, timers.draw_period(), timers.update_period()));
    let catalogue;
    let mut app = match (loaded, booted) {
        (Some((owned, zoo, money)), _) => {
            catalogue = owned;
            let mut app = App::new(
                MapRenderer::new(vfs, &catalogue),
                zoo,
                Settings::from_config(ini),
                timers,
                size,
            )?;
            if let Some(ui) = crate::layout::load_interface(vfs, size, flags.show, flags.fill)? {
                app.set_interface(ui);
            }
            // The game starts with the run: the menu music fades out.
            if flags.bare {
                app.bare();
            }
            if let Some(seed) = flags.seed {
                app.set_seed(seed);
            }
            app.set_money(money);
            app.start_game(menu);
            app
        }
        (None, Some(booted)) => {
            catalogue = booted.catalogue;
            let mut app = App::menu(
                MapRenderer::new(vfs, &catalogue),
                Settings::from_config(ini),
                timers,
                size,
                booted.interface,
                booted.menu,
                menu,
            )?;
            if let Some(seed) = flags.seed {
                app.set_seed(seed);
            }
            app
        }
        (None, None) => unreachable!("a run without a map boots"),
    };
    let mut state = State {
        interface: app.interface().is_some(),
        alternate: rustyzoo::cursor::alternate(ini),
        ..State::default()
    };
    if state.interface {
        // The window gets typed characters as text input events.
        // SAFETY: the window is open and owned by `screen` for the whole run.
        unsafe { sdl3::sys::keyboard::SDL_StartTextInput(screen.canvas.window().raw()) };
    }
    if let Some(log) = &mut log {
        for line in &start_lines {
            writeln!(log, "startup {line}")?;
        }
    }
    // What the player did during the start-up is dropped, but not closing the window.
    while let Some(event) = pump.poll_event() {
        state.note(event, &mut commands);
    }
    state.mouse.clear();
    commands.retain(|c| matches!(c, Command::Quit | Command::Close | Command::Fullscreen(_)));
    let start = sdl3::timer::ticks();
    let mut shown = false;
    let mut pointer = None;
    // The dummy driver has no pointer and reports one in the corner, which would scroll the view.
    let has_pointer = video.current_video_driver() != "dummy";
    let mut wait = Wait::default();
    let mut seen = false;
    let mut cursors = Cursors::new(vfs.root());
    let mut alternate = state.alternate;
    loop {
        while let Some(event) = pump.poll_event() {
            state.note(event, &mut commands);
        }
        state.mouse_commands(|at| screen.pointer(at), &mut commands)?;
        // Without the focus the sound is silent, the voices running on.
        sound.focus(focused(screen.canvas.window().window_flags().into()));
        // Minimised or hidden: nothing runs until something happens (a close request still
        // does). The music runs on silently, the output being muted with the focus, and is heard
        // where it has got to at the restore.
        if asleep(screen.canvas.window().window_flags().into())
            && !commands
                .iter()
                .any(|c| matches!(c, Command::Quit | Command::Close))
        {
            state.note(pump.wait_event(), &mut commands);
            continue;
        }
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
        let at = (has_pointer
            && screen.canvas.window().has_input_focus()
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
        cursors.show(rustyzoo::cursor::number(app.cursor(), state.alternate));
        if state.alternate != alternate {
            alternate = state.alternate;
            remember_cursors(alternate);
        }
        sound.pause(app.paused());
        // Printed, not logged: the log is replayed headless, where nothing is heard.
        for line in sound.units(vfs, &app) {
            println!("pass {pass} t={now} {line}");
        }
        if let Some(log) = &mut log {
            for line in app.log_lines(pass, now) {
                writeln!(log, "{line}")?;
            }
        }
        sound.fade(&app);
        sound.restart(vfs, &app);
        sound.clicks(vfs, &app);
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
