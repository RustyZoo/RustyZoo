use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use rustyzoo::config::Config;
use rustyzoo::pe;
use rustyzoo::sprite::{self, write_png};
use rustyzoo::strings::StringTables;
use rustyzoo::vfs::{self, Vfs};
use sdl3::audio::AudioSpec;
use sdl3::pixels::PixelFormat;

const USAGE: &str = "\
usage: rustyzoo [--data <dir>] <command>

  window-smoke        open a window and the default audio device, present one frame, exit
  ls [pattern]        list the game's files, sorted; in the pattern * matches any text
  cat <member>        write one of the game's files to standard output
  which <member>      show which file or archive supplies it, and the copies it hides
  dump-strings <dll> [--text <name>]
                      print the string table of a resource DLL of the install folder (for
                      example lang0.dll) as ID<TAB>text lines sorted by ID, with \\n, \\t, \\r,
                      \\\\ and \\u{XXXX} (other control characters) escaped, then the name and
                      size of its TEXT entries; with --text, write the bytes of one TEXT
                      entry to standard output instead
  strings             print the texts of all lang*.dll of the install folder as the game
                      resolves them: the libraries in lookup order (LIB lines), then
                      ID<TAB>library<TAB>text and TEXT<TAB>name<TAB>library<TAB>bytes
  check-config        read every configuration file of the game, report empty files and ignored text
  dump-sprite <member> <dir>
                      write the frames of a sprite file as PNG files into <dir>, and print them
  check-sprites       decode every sprite file; list those that fail; exit 1 if any does

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
            // A closed standard error is not worth a panic, which `eprintln!` would raise.
            let _ = writeln!(io::stderr(), "{USAGE}");
            ExitCode::from(2)
        }
        Err(CliError::Failed(err)) => {
            let _ = writeln!(io::stderr(), "rustyzoo: {err}");
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
        ["strings"] => merged_strings(&game_folder(data_dir)?),
        ["check-config"] => check_config(&open_game(data_dir)?),
        ["dump-sprite", member, dir] => dump_sprite(&open_game(data_dir)?, member, Path::new(dir)),
        ["check-sprites"] => check_sprites(&open_game(data_dir)?),
        ["which", member] => which(open_game(data_dir)?, member),
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
    let vfs = Vfs::open(&game_folder(data_dir)?)?;
    for warning in vfs.warnings() {
        eprintln!("rustyzoo: warning: {warning}");
    }
    Ok(vfs)
}

fn which(vfs: Vfs, member: &str) -> Result<(), CliError> {
    let copies = vfs.which(member)?;
    if copies.is_empty() {
        return Err(format!("no such member: {member}").into());
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = copies
        .iter()
        .enumerate()
        .try_for_each(|(i, copy)| {
            let label = if i == 0 { "supplied by" } else { "shadowed" };
            let kind = if copy.in_archive {
                "archive"
            } else {
                "loose file"
            };
            let empty = if copy.size == 0 && !copy.in_archive {
                ", empty: reading fails"
            } else {
                ""
            };
            writeln!(
                out,
                "{label:<11} {kind} {} ({} bytes{empty})",
                escape(&copy.path.to_string_lossy()),
                copy.size
            )
        })
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
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

/// Prints the merged view of the `lang*.dll` libraries of the install folder.
fn merged_strings(dir: &Path) -> Result<(), CliError> {
    let tables = StringTables::open(dir)?;
    if tables.mixed_case() {
        let _ = writeln!(
            io::stderr(),
            "rustyzoo: warning: the lang*.dll names differ in letter case; they are ordered by their bytes"
        );
    }
    let mut out = io::BufWriter::new(io::stdout().lock());
    let written = tables
        .libraries()
        .try_for_each(|name| writeln!(out, "LIB\t{name}"))
        .and_then(|()| {
            tables
                .strings()
                .iter()
                .try_for_each(|(id, (lib, text))| writeln!(out, "{id}\t{lib}\t{}", escape(text)))
        })
        .and_then(|()| {
            tables.texts().iter().try_for_each(|(name, (lib, data))| {
                writeln!(out, "TEXT\t{}\t{lib}\t{}", escape(name), data.len())
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
        .try_for_each(|path| writeln!(out, "{}", escape(path)))
        .and_then(|()| out.flush());
    ignore_closed_pipe(written)
}

/// The members written in the INI-like dialect (see `rustyzoo::config`).
const CONFIG_EXTENSIONS: [&str; 9] = [
    ".ai", ".cfg", ".ani", ".scn", ".lyt", ".uca", ".ucb", ".ucs", ".ini",
];

/// Parses every configuration member and reports what the original would ignore. Nothing in a
/// file is an error; only a member that cannot be read or is too large makes the command fail.
fn check_config(vfs: &Vfs) -> Result<(), CliError> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    let mut failed = 0;
    let written = report_config(vfs, &mut out, &mut failed).and_then(|()| out.flush());
    ignore_closed_pipe(written)?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} configuration files could not be read").into()),
    }
}

fn report_config(vfs: &Vfs, out: &mut impl Write, failed: &mut usize) -> io::Result<()> {
    let (mut files, mut sections, mut keys, mut values) = (0, 0, 0, 0);
    let (mut empty, mut before, mut after) = (0, 0, 0);
    for path in vfs.list(None) {
        if !CONFIG_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) {
            continue;
        }
        files += 1;
        let config = vfs
            .read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Config::parse(&bytes).map_err(|e| e.to_string()));
        let config = match config {
            Ok(config) => config,
            Err(message) => {
                *failed += 1;
                writeln!(out, "{path}: {message}")?;
                continue;
            }
        };
        for (_, section) in config.sections() {
            sections += 1;
            for key in section.keys() {
                keys += 1;
                values += section.get_all(key).len();
            }
        }
        if config.is_empty() {
            empty += 1;
            writeln!(out, "{path}: no section")?;
        }
        let (b, a) = (
            config.ignored_before_first_header,
            config.ignored_after_broken_header,
        );
        if b + a > 0 {
            writeln!(
                out,
                "{path}: ignored lines: {b} before the first header, {a} after a broken header"
            )?;
        }
        before += b;
        after += a;
    }
    writeln!(
        out,
        "{files} files, {sections} sections, {keys} keys, {values} values; \
         {empty} files without a section; ignored lines: {before} before the first header, \
         {after} after a broken header; {failed} unreadable",
        failed = *failed
    )
}

/// A reader that stops early (`| head`) is not an error of ours.
fn ignore_closed_pipe(result: io::Result<()>) -> Result<(), CliError> {
    match result {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err.into()),
        _ => Ok(()),
    }
}

/// Lines for standard output that keep the work going when the reader has gone away: after the
/// first failed write nothing more is written, and [`Report::finish`] treats a closed pipe like
/// `ls` and `cat` do.
struct Report {
    out: io::StdoutLock<'static>,
    written: io::Result<()>,
}

impl Report {
    fn new() -> Report {
        Report {
            out: io::stdout().lock(),
            written: Ok(()),
        }
    }

    fn line(&mut self, line: fmt::Arguments) {
        if self.written.is_ok() {
            self.written = writeln!(self.out, "{line}");
        }
    }

    fn finish(mut self) -> Result<(), CliError> {
        let flushed = self.out.flush();
        ignore_closed_pipe(self.written.and(flushed))
    }
}

/// The file of a frame in `dump-sprite`: the frame number only, never anything from the sprite.
fn frame_file_name(number: usize, shadow: bool, extra: bool) -> String {
    let shadow = if shadow { "-shadow" } else { "" };
    let extra = if extra { "-extra" } else { "" };
    format!("{number:03}{shadow}{extra}.png")
}

fn dump_sprite(vfs: &Vfs, member: &str, dir: &Path) -> Result<(), CliError> {
    let (sprite, palette) = sprite::load(vfs, member)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{member}: {} frames{}, delay {} ms, palette {:?}",
        sprite.frames.len(),
        if sprite.extra.is_some() {
            " + 1 extra"
        } else {
            ""
        },
        sprite.delay_ms,
        sprite.palette,
    ));
    for (number, frame) in sprite.all_frames().enumerate() {
        let name = frame_file_name(number, frame.shadow, number == sprite.frames.len());
        let empty = frame.width == 0 || frame.height == 0;
        report.line(format_args!(
            "  {name}: {}x{} x offset {} y offset {} unknown {}{}",
            frame.width,
            frame.height,
            frame.x_offset,
            frame.y_offset,
            frame.unknown,
            if empty { " (empty, no file)" } else { "" },
        ));
        if empty {
            continue;
        }
        let mut image = Framebuffer::new(frame.width.into(), frame.height.into());
        frame.draw(&palette, &mut image, 0, 0)?;
        let path = dir.join(&name);
        let file =
            File::create(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        write_png(&image, io::BufWriter::new(file))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    report.finish()
}

/// Decodes every sprite file (a member whose last path component has no extension).
fn check_sprites(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut files, mut frames, mut shadows, mut extras, mut failed) = (0, 0, 0, 0, 0);
    for member in vfs.list(None) {
        if !sprite::is_sprite_member(&member) {
            continue;
        }
        files += 1;
        match sprite::load(vfs, &member) {
            Ok((sprite, _)) => {
                frames += sprite.frames.len();
                shadows += sprite.all_frames().filter(|frame| frame.shadow).count();
                extras += usize::from(sprite.extra.is_some());
            }
            Err(err) => {
                failed += 1;
                // Paths come from the files: print them with their control characters escaped.
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{files} sprite files, {frames} frames, {shadows} shadow frames, \
         {extras} files with an extra frame, {failed} failed"
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {files} sprite files failed").into()),
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
    use super::{escape, frame_file_name};

    #[test]
    fn frame_files_are_named_by_number_shadow_and_extra() {
        assert_eq!(frame_file_name(0, false, false), "000.png");
        assert_eq!(frame_file_name(12, true, false), "012-shadow.png");
        assert_eq!(frame_file_name(1234, false, true), "1234-extra.png");
        assert_eq!(frame_file_name(21, true, true), "021-shadow-extra.png");
    }

    #[test]
    fn escape_keeps_a_text_on_one_line() {
        assert_eq!(escape("a\\b\n\tc\rd"), "a\\\\b\\n\\tc\\rd");
        assert_eq!(
            escape("\0\u{1b}\u{7f}\u{85}é"),
            "\\u{0000}\\u{001b}\\u{007f}\\u{0085}é"
        );
    }
}
