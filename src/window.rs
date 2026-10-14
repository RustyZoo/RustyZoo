//! `rustyzoo run <map>` without `--headless`: the map view in an SDL3 window, driven by the
//! loop of the original (main loop 3.1, 3.2): take the waiting events, block while the window
//! is minimised or hidden, run one [`App::pass`], present a frame only when the pass painted
//! one, then sleep until the next timer is due instead of spinning.
//!
//! The frame is the game size (the ini file's or `--size`). It goes to a streaming texture that
//! SDL scales to the window with the nearest pixel, by a whole factor `k` of the display scale,
//! so the window has the size of the original's on every system and the picture stays sharp:
//! see [`plan`]. Only the program uses this module: the core crate does not link to SDL. The
//! keys become commands in `input`; the loop reads the cursor.

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
use sdl3::video::WindowFlags;

use crate::CliError;
use crate::audio::{Audio, Voice};
use crate::input;
use crate::probe::{self, Timing};

/// The window when the ini file does not say.
const DEFAULT_SIZE: (u32, u32) = (1024, 768);

/// Queues the music again when it runs low; a music that fails stops for good.
fn feed(music: &mut Option<Voice>) {
    if let Some(voice) = music
        && let Err(e) = voice.update()
    {
        eprintln!("rustyzoo: the menu music stops: {e}");
        *music = None;
    }
}

/// Pauses or resumes the music where it is; a music that fails stops for good.
fn hold(music: &mut Option<Voice>, hold: bool) {
    if let Some(voice) = music
        && let Err(e) = if hold { voice.pause() } else { voice.resume() }
    {
        eprintln!("rustyzoo: the menu music stops: {e}");
        *music = None;
    }
}

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

/// `[user] screenwidth` and `screenheight`; the default if either is missing or not positive.
fn ini_size(ini: &Config) -> (u32, u32) {
    let side = |key| user_whole(ini, key).and_then(|n| u32::try_from(n).ok());
    match (side("screenwidth"), side("screenheight")) {
        (Some(w @ 1..), Some(h @ 1..)) => (w, h),
        _ => DEFAULT_SIZE,
    }
}

/// `size` cut down to the usable area of the display, in the same unit; an area of 0 means that
/// the system does not say, and cuts nothing.
fn clamp(size: (u32, u32), usable: (u32, u32)) -> (u32, u32) {
    let cut = |n: u32, max: u32| if max == 0 { n } else { n.min(max) };
    (cut(size.0, usable.0), cut(size.1, usable.1))
}

/// The whole factor `k` between a pixel of the game and a pixel of the screen, and the window
/// size in points that gives it. `scale` is the display scale (the content scale of the display
/// times its pixel density: 2 at 200% on Windows and X11, where `density` is 1 and the window
/// is in pixels, and on macOS and Wayland, where the content scale is 1, `density` is 2 and the
/// window is in points). So `k` is the same everywhere: the scale rounded, at least 1, and
/// reduced until the window fits in `usable` (points, 0 meaning unknown), never below 1.
fn plan(game: (u32, u32), scale: f32, density: f32, usable: (u32, u32)) -> (u32, (u32, u32)) {
    let density = if density > 0.0 { density } else { 1.0 };
    let fit = |n: u32, max: u32| match max {
        0 => u32::MAX,
        _ => ((max as f32 * density) / n.max(1) as f32) as u32,
    };
    let k = (scale.round() as u32)
        .min(fit(game.0, usable.0))
        .min(fit(game.1, usable.1))
        .max(1);
    let points = |n: u32| ((n * k) as f32 / density).round() as u32;
    (k, (points(game.0), points(game.1)))
}

/// Whether the window is hidden or minimised, from its flags. The flags are read, not followed
/// through the events: a restore that comes as another event cannot leave the loop asleep.
fn asleep(flags: WindowFlags) -> bool {
    flags.intersects(WindowFlags::HIDDEN | WindowFlags::MINIMIZED)
}

/// What the window tells through its events.
#[derive(Default)]
struct State {
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
            Event::Window {
                win_event: WindowEvent::Exposed,
                ..
            } => self.exposed = true,
            _ => {}
        }
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
    let display = video.get_primary_display()?;
    let usable = display.get_usable_bounds()?;
    let usable = (usable.width(), usable.height());
    // The scale of the display, before there is a window: its content scale and pixel density.
    let density = display.get_mode().map_or(1.0, |m| m.pixel_density);
    let scale = display.get_content_scale().unwrap_or(1.0) * density;
    let (size, usable) = match flags.size {
        Some(size) => (size, (0, 0)),
        None => {
            let pixels = |n: u32| (n as f32 * density) as u32;
            let game = clamp(ini_size(ini), (pixels(usable.0), pixels(usable.1)));
            (game, usable)
        }
    };
    let (k, want) = plan(size, scale, density, usable);
    let window = video
        .window("RustyZoo", want.0, want.1)
        .high_pixel_density()
        .build()?;
    // The game keeps its size; the window may come out another size.
    let points = window.size();
    let pixels = window.size_in_pixels();
    let (scale, density) = (window.display_scale(), window.pixel_density());
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    let output = canvas.output_size()?;
    println!("game {}x{} factor {k}", size.0, size.1);
    println!(
        "window {}x{} pixels {}x{} output {}x{}",
        points.0, points.1, pixels.0, pixels.1, output.0, output.1
    );
    println!("display scale {scale} pixel density {density}");
    if output != pixels {
        return Err(
            format!("the output is {output:?} but the window has {pixels:?} pixels").into(),
        );
    }
    if pixels != (size.0 * k, size.1 * k) {
        eprintln!("rustyzoo: the window has {pixels:?} pixels, not the game size times {k}");
    }
    // The cursor is in points of the window; the game reads it in pixels of the game.
    let to_game = (
        size.0 as f32 / points.0 as f32,
        size.1 as f32 / points.1 as f32,
    );
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
    let start = sdl3::timer::ticks();
    let mut pass = 0;
    let mut shown = false;
    let mut pointer = None;
    let mut held = false;
    loop {
        while let Some(event) = pump.poll_event() {
            state.note(event, &mut commands);
        }
        // Minimised or hidden: nothing runs until something happens (a close request still
        // does). The music waits where it is and goes on at the restore, not from its beginning.
        if asleep(canvas.window().window_flags().into()) && !commands.contains(&Command::Quit) {
            if !held {
                held = true;
                hold(&mut music, true);
            }
            state.note(pump.wait_event(), &mut commands);
            continue;
        }
        if held {
            held = false;
            hold(&mut music, false);
        }
        while let Some((_, command)) = script.next_if(|&(at, _)| at <= pass) {
            commands.push(command);
        }
        // The edge scroll reads the cursor at its pass, not from motion events: a window
        // that is not active gets none, one that gets the focus back under the pointer does.
        let at = (canvas.window().has_input_focus() && canvas.window().has_mouse_focus())
            .then(|| pump.mouse_state())
            .map(|m| {
                (
                    (m.x() * to_game.0).floor() as i32,
                    (m.y() * to_game.1).floor() as i32,
                )
            });
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
        let timed = timing.as_ref().is_some_and(|t| t.done(now));
        if app.quit() || timed || flags.passes.is_some_and(|n| pass >= n) {
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
        (Some(voice), _) => match (voice.gain(), voice.queued_bytes()) {
            (Ok(gain), Ok(bytes)) => println!("music gain {gain:.3} queued {bytes} bytes"),
            (Err(e), _) | (_, Err(e)) => eprintln!("rustyzoo: the menu music state: {e}"),
        },
        (None, Some((model, _))) if model.stopped() => println!("music stopped"),
        _ => println!("music none"),
    }
    if let Some(timing) = timing {
        timing.summary().iter().for_each(|line| println!("{line}"));
    }
    if let Some(file) = flags.shot {
        // The texture goes to the renderer again, and its pixels are read before the present.
        canvas.copy(&texture, None, None)?;
        probe::shot(&canvas, file)?;
        canvas.present();
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
    fn the_window_is_enlarged_by_the_display_scale_on_every_system() {
        let game = (800, 600);
        // Windows and X11 at 200%: points are pixels, density 1, content scale 2.
        assert_eq!(plan(game, 2.0, 1.0, (2560, 1440)), (2, (1600, 1200)));
        // macOS and Wayland at scale 2: the window is in points, density 2.
        assert_eq!(plan(game, 2.0, 2.0, (1440, 900)), (2, (800, 600)));
        // No scaling.
        assert_eq!(plan(game, 1.0, 1.0, (1920, 1080)), (1, game));
        // Rounded to a whole number, at least 1; a bad value is 1.
        assert_eq!(plan(game, 1.5, 1.0, (0, 0)).0, 2);
        assert_eq!(plan(game, 1.25, 1.0, (0, 0)).0, 1);
        assert_eq!(plan(game, 0.0, 1.0, (0, 0)).0, 1);
        assert_eq!(plan(game, f32::NAN, 0.0, (0, 0)), (1, game));
        // The factor goes down, not the game, when the window does not fit.
        assert_eq!(plan(game, 2.0, 1.0, (1920, 1080)), (1, game));
        assert_eq!(plan((1024, 768), 2.0, 1.0, (1920, 1080)), (1, (1024, 768)));
        assert_eq!(plan((1024, 768), 3.0, 1.0, (2560, 1440)), (1, (1024, 768)));
        assert_eq!(plan((800, 600), 3.0, 1.0, (2560, 1440)), (2, (1600, 1200)));
        assert_eq!(plan((800, 600), 4.0, 2.0, (1440, 900)), (3, (1200, 900)));
        // Too small for even one: k stays 1 (the caller cut the game size).
        assert_eq!(plan(game, 2.0, 1.0, (640, 480)).0, 1);
        // The system does not say how large the display is.
        assert_eq!(plan(game, 2.0, 1.0, (0, 0)).0, 2);
    }

    #[test]
    fn window_flags_decide_whether_the_loop_blocks() {
        let window = |win_event| Event::Window {
            timestamp: 0,
            window_id: 1,
            win_event,
        };
        let (mut state, mut commands) = (State::default(), Vec::new());
        assert!(asleep(WindowFlags::MINIMIZED));
        assert!(asleep(WindowFlags::HIDDEN | WindowFlags::INPUT_FOCUS));
        assert!(
            !asleep(WindowFlags::MAXIMIZED),
            "a restore may come as Maximized"
        );
        assert!(
            !asleep(WindowFlags::empty()),
            "inactive but visible keeps running"
        );
        state.note(window(WindowEvent::FocusLost), &mut commands);
        assert!(!state.exposed && commands.is_empty());
        state.note(window(WindowEvent::Exposed), &mut commands);
        assert!(state.exposed);
        state.note(window(WindowEvent::CloseRequested), &mut commands);
        state.note(Event::Quit { timestamp: 0 }, &mut commands);
        assert_eq!(commands, [Command::Quit, Command::Quit]);
    }
}
