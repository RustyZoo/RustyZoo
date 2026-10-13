use std::error::Error;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use rustyzoo::vfs::Vfs;
use sdl3::audio::AudioSpec;
use sdl3::pixels::PixelFormat;

const USAGE: &str = "\
usage: rustyzoo [--data <dir>] <command>

  window-smoke        open a window and the default audio device, present one frame, exit
  ls [pattern]        list the game's files, sorted; in the pattern * matches any text
  cat <member>        write one of the game's files to standard output

--data <dir> is the install folder of the game (it holds zoo.ini). Without it, the
ZT_DATA_DIR environment variable is used. window-smoke does not need it.";

enum CliError {
    Usage,
    Failed(Box<dyn Error>),
}

impl<E: Into<Box<dyn Error>>> From<E> for CliError {
    fn from(err: E) -> Self {
        CliError::Failed(err.into())
    }
}

fn main() -> ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage) => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(err)) => {
            eprintln!("rustyzoo: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<(), CliError> {
    let mut data_dir = None;
    let mut words = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--data" {
            data_dir = Some(args.next().ok_or(CliError::Usage)?);
        } else {
            words.push(arg.into_string().map_err(|_| CliError::Usage)?);
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();

    match words.as_slice() {
        ["window-smoke"] => Ok(window_smoke()?),
        ["ls"] => list(open_game(data_dir)?, None),
        ["ls", pattern] => list(open_game(data_dir)?, Some(pattern)),
        ["cat", member] => {
            let bytes = open_game(data_dir)?.read(member)?;
            ignore_closed_pipe(io::stdout().lock().write_all(&bytes))
        }
        _ => Err(CliError::Usage),
    }
}

/// Opens the game folder given by `--data`, or else by `ZT_DATA_DIR`.
fn open_game(data_dir: Option<OsString>) -> Result<Vfs, CliError> {
    let data_dir = data_dir
        .or_else(|| std::env::var_os("ZT_DATA_DIR").filter(|dir| !dir.is_empty()))
        .ok_or("no game folder: pass --data <dir> or set ZT_DATA_DIR")?;
    Ok(Vfs::open(Path::new(&data_dir))?)
}

fn list(vfs: Vfs, pattern: Option<&str>) -> Result<(), CliError> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = vfs
        .list(pattern)
        .iter()
        .try_for_each(|path| writeln!(out, "{path}"))
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// A reader that stops early (`| head`) is not an error of ours.
fn ignore_closed_pipe(result: io::Result<()>) -> Result<(), CliError> {
    match result {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err.into()),
        _ => Ok(()),
    }
}

fn window_smoke() -> Result<(), Box<dyn Error>> {
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
