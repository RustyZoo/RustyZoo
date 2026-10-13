use std::error::Error;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use rustyzoo::pe;
use rustyzoo::vfs::{self, Vfs};
use sdl3::audio::AudioSpec;
use sdl3::pixels::PixelFormat;

const USAGE: &str = "\
usage: rustyzoo [--data <dir>] <command>

  window-smoke        open a window and the default audio device, present one frame, exit
  ls [pattern]        list the game's files, sorted; in the pattern * matches any text
  cat <member>        write one of the game's files to standard output
  dump-strings <dll> [--text <name>]
                      print the string table of a resource DLL of the install folder (for
                      example lang0.dll) as ID<TAB>text lines sorted by ID, with \\n, \\t, \\r,
                      \\\\ and \\u{XXXX} (other control characters) escaped, then the name and
                      size of its TEXT entries; with --text, write the bytes of one TEXT
                      entry to standard output instead

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
        ["dump-strings", dll] => dump_strings(game_folder(data_dir)?, dll, None),
        ["dump-strings", dll, "--text", name] => {
            dump_strings(game_folder(data_dir)?, dll, Some(name))
        }
        _ => Err(CliError::Usage),
    }
}

/// The game folder given by `--data`, or else by `ZT_DATA_DIR`.
fn game_folder(data_dir: Option<OsString>) -> Result<PathBuf, CliError> {
    data_dir
        .or_else(|| std::env::var_os("ZT_DATA_DIR").filter(|dir| !dir.is_empty()))
        .map(PathBuf::from)
        .ok_or_else(|| "no game folder: pass --data <dir> or set ZT_DATA_DIR".into())
}

fn open_game(data_dir: Option<OsString>) -> Result<Vfs, CliError> {
    Ok(Vfs::open(&game_folder(data_dir)?)?)
}

/// Prints the string table and the `TEXT` entries of `dll`, a file of the install folder
/// whose name is matched ignoring letter case, or only the bytes of the `TEXT` entry `text`.
fn dump_strings(dir: PathBuf, dll: &str, text: Option<&str>) -> Result<(), CliError> {
    let path = std::fs::read_dir(&dir)
        .map_err(|e| format!("cannot read folder {}: {e}", dir.display()))?
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(dll))
                && entry.path().is_file()
        })
        .map(|entry| entry.path())
        .ok_or_else(|| format!("no file {dll} in {}", dir.display()))?;
    let size = std::fs::metadata(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .len();
    if size > vfs::MAX_MEMBER_SIZE {
        return Err(format!("{} is too big for a resource DLL", path.display()).into());
    }
    let file = std::fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let resources = pe::resources(&file).map_err(|e| format!("{dll}: {e}"))?;
    let texts = pe::texts(&resources);

    if let Some(name) = text {
        let (_, data) = texts
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("{dll} has no TEXT entry named {name}"))?;
        return ignore_closed_pipe(io::stdout().lock().write_all(data));
    }
    let strings = pe::strings(&resources).map_err(|e| format!("{dll}: {e}"))?;
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = strings
        .iter()
        .try_for_each(|(id, text)| writeln!(out, "{id}\t{}", escape(text)))
        .and_then(|()| {
            texts.iter().try_for_each(|(name, data)| {
                writeln!(out, "TEXT\t{}\t{}", escape(name), data.len())
            })
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// The text on one line: backslash, newline, carriage return and tab become `\\`, `\n`, `\r`
/// and `\t`, and any other control character becomes `\u{XXXX}`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:04x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escape_keeps_a_text_on_one_line() {
        assert_eq!(escape("a\\b\n\tc\rd"), "a\\\\b\\n\\tc\\rd");
        assert_eq!(
            escape("\0\u{1b}\u{7f}\u{85}é"),
            "\\u{0000}\\u{001b}\\u{007f}\\u{0085}é"
        );
    }
}
