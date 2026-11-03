//! The remaining checks of the game data and of the window.

use std::collections::BTreeMap;
use std::error::Error;
use std::io::{self, Write};

use super::{CliError, Report, ignore_closed_pipe};
use crate::audio;
use rustyzoo::Framebuffer;
use rustyzoo::bitmap;
use rustyzoo::config::Config;
use rustyzoo::music::Music;
use rustyzoo::timing::Timers;
use rustyzoo::vfs::Vfs;
use sdl3::audio::AudioSpec;
use sdl3::pixels::PixelFormat;

/// The loop settings of the ini file, and a simulated run with a pass at every millisecond.
pub fn timing(vfs: &Vfs) -> Result<(), CliError> {
    let mut timers = Timers::from_config(&Config::parse(vfs.ini())?);
    let mut out = io::stdout().lock();
    let first = writeln!(
        out,
        "periods: update {} ms, draw {} ms, cap {} ms",
        timers.update_period(),
        timers.draw_period(),
        timers.cap()
    );
    ignore_closed_pipe(first)?;
    timers.pass(0, 1.0);
    let (mut draws, mut interfaces, mut steps) = (0, 0, 0);
    for now in 1..=10_000 {
        let pass = timers.pass(now, 1.0);
        draws += u32::from(pass.draw);
        interfaces += u32::from(pass.interface.is_some());
        steps += u32::from(pass.step.is_some());
    }
    ignore_closed_pipe(writeln!(
        out,
        "ten seconds, clock of 1 ms: {draws} draws, {interfaces} interface updates, {steps} steps"
    ))
}

/// Converts every root bitmap of the terrain and compares it with its shipped copy.
pub fn check_bitmaps(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut same, mut differ, mut alone, mut failed) = (0, 0, 0, 0);
    let names = bitmap::names(vfs);
    for name in &names {
        let converted = vfs
            .read(name)
            .map_err(|e| e.to_string())
            .and_then(|bytes| bitmap::convert(&bytes));
        let shipped = bitmap::shipped(vfs, name);
        let name = name.escape_debug();
        match (converted, shipped) {
            (Err(why), _) => {
                failed += 1;
                report.line(format_args!("FAILED {name}: {}", why.escape_debug()));
            }
            (Ok(_), None) => {
                alone += 1;
                report.line(format_args!("converted {name}: no shipped copy"));
            }
            (Ok(ours), Some(theirs)) => match bitmap::compare(&ours, &theirs) {
                Ok(()) => {
                    same += 1;
                    report.line(format_args!("identical {name}"));
                }
                Err(why) => {
                    differ += 1;
                    report.line(format_args!("DIFFERS {name}: {why}"));
                }
            },
        }
    }
    report.line(format_args!(
        "{} bitmaps: {same} identical to the shipped copy, {differ} differ, \
         {alone} without a shipped copy, {failed} failed",
        names.len()
    ));
    report.finish()?;
    match differ + failed {
        0 => Ok(()),
        n => Err(format!("{n} bitmaps differ or failed").into()),
    }
}

pub fn check_wavs(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let mut formats = BTreeMap::new();
    let mut failed = 0;
    let names: Vec<String> = vfs
        .list(None)
        .into_iter()
        .filter(|name| name.ends_with(".wav"))
        .collect();
    for name in &names {
        let sound = vfs
            .read(name)
            .map_err(|e| e.to_string())
            .and_then(|bytes| audio::Sound::decode(&bytes).map_err(|e| e.to_string()));
        match sound {
            Ok(sound) => *formats.entry(sound.describe()).or_insert(0) += 1,
            Err(why) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {}: {}",
                    name.escape_debug(),
                    why.escape_debug()
                ));
            }
        }
    }
    for (format, count) in &formats {
        report.line(format_args!("{count} {format}"));
    }
    report.line(format_args!(
        "{} wavs: {} ok, {failed} failed",
        names.len(),
        names.len() - failed
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        n => Err(format!("{n} wavs failed").into()),
    }
}

pub fn check_music(vfs: &Vfs) -> Result<(), CliError> {
    let audio = audio::Audio::open(&sdl3::init()?.audio()?)?;
    let Some(model) = Music::from_config(&Config::parse(vfs.ini())?) else {
        println!("no menu music");
        return Ok(());
    };
    let Some(music) = audio.menu_music(vfs, &model, true, true)? else {
        println!("no menu music");
        return Ok(());
    };
    println!(
        "{}: {}, gain {:.3}, queued {} bytes, paused",
        model.file(),
        music.sound().describe(),
        music.gain()?,
        music.queued_bytes()?
    );
    // Exercises the rest of the interface; the device is dummy, so nothing is heard.
    music.resume()?;
    music.pause()?;
    Ok(())
}

pub fn window_smoke() -> Result<(), Box<dyn Error>> {
    let mut framebuffer = Framebuffer::new(800, 600);
    framebuffer.fill_test_pattern();

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let audio = sdl.audio()?;

    let window = video
        .window("RustyZoo", framebuffer.width, framebuffer.height)
        .build()?;
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    let textures = canvas.texture_creator();
    let mut texture = textures.create_texture_streaming(
        PixelFormat::RGBA32,
        framebuffer.width,
        framebuffer.height,
    )?;
    texture.update(None, &framebuffer.pixels, framebuffer.width as usize * 4)?;
    canvas.copy(&texture, None, None)?;
    if !canvas.present() {
        return Err(sdl3::get_error().into());
    }

    sdl.event_pump()?.pump_events();
    let _device = audio.open_playback_device(&AudioSpec::default())?;

    println!("SDL version:  {}", sdl3::version::version());
    println!("video driver: {}", video.current_video_driver());
    println!("audio driver: {}", audio.current_audio_driver());
    Ok(())
}
