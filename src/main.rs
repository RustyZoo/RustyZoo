mod audio;
mod input;
mod layout;
mod probe;
mod run;
mod textcmd;
mod window;

use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use rustyzoo::anim::{self, AnimSet, Animation};
use rustyzoo::bitmap;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::ground::{self, Key, Renderer};
use rustyzoo::movie::Movie;
use rustyzoo::music::Music;
use rustyzoo::pe;
use rustyzoo::render::{MapRenderer, Options, Summary};
use rustyzoo::sprite::{self, write_png};
use rustyzoo::strings::StringTables;
use rustyzoo::timing::Timers;
use rustyzoo::userdir;
use rustyzoo::vfs::{self, Vfs};
use rustyzoo::zoo::{Zoo, ZooError};
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
  types               list the entity types: class, type, subtype, name id, footprint (half
                      tiles), the .ani member of the base animation and whether it exists;
                      then counts per class; warnings go to standard error; exit 1 only if the system
                      fails to read a file
  check-config        read every configuration file of the game, report empty files and ignored text
  dump-sprite <member> <dir>
                      write the frames of a sprite file as PNG files into <dir>, and print them
  check-sprites       decode every sprite file; list those that fail or have a palette
                      defect (a warning); exit 1 if one fails
  movie-info <member|path> [--dump K <dir>]
                      read an intro movie (an AVI file with Indeo 5 video and PCM audio): the
                      member of the game, or a file of this name outside the game, which wins;
                      print its picture size, frame count, frame rate, duration and audio
                      format, then decode every frame and print how many were decoded; exit 1
                      if the file cannot be read or a frame cannot be decoded. With --dump,
                      write every Kth decoded frame (K at least 1, frame 0 first) to
                      <dir>/frame-<number>.png
  map-info <member>   print the version, size, tiles and entities of a map or saved game (a .zoo)
  check-maps          read every .zoo file; list those that fail; exit 1 if one is corrupted
  render-ani <member.ani> <dir> [--anim NAME]
                      write frame 0 of an animation set as eight PNG files into <dir>, one per
                      screen direction, and print the anchor point; with --anim, the first
                      argument is an animation root (such as animals/lion/f) and NAME the
                      animation under it
  check-ground        render every ground picture of the terrain cache (ztats/, ztatb/) again and
                      compare it with the cached one; exit 1 if any covered pixel differs
  render-ground <key> <out.png>
                      render one ground picture to a PNG file; the key is written as in the
                      cache, for example ztats/000003 or ztatb/00/00000000/00000001/0003
  render-map <map> <out.png> [--rotation N] [--zoom Z] [--view X Y W H] [--no-units]
                      draw a whole map (a .zoo member) into a PNG, seen from rotation N (0 to
                      3, default 0) at zoom Z (-2 zoomed out, 0 normal, 2 zoomed in), and
                      print what was drawn: the tiles whose ground picture is in the game's
                      cache, the ground pictures missing, the cliff pieces, and the objects
                      drawn or lacking a type or an animation; with --view, draw only the
                      window of W by H pixels whose top left pixel is the view-space point
                      (X, Y) of that zoom, black where the map has nothing (the summary still
                      counts the whole map); with --no-units, leave out animals, guests, staff
                      and ambient animals, which then are not counted either
  check-render [--rotation N]
                      draw every .zoo file in memory and print the same summary per map and
                      for all of them; a file that cannot be read is skipped; exit 1 only if
                      a map cannot be drawn
  check-bitmaps       convert the root bitmaps of the terrain (grid lines, highlight, cliff
                      pieces) as the engine does when their copies in ztst/ are missing, and
                      compare each with its shipped copy; exit 1 if one differs or fails
  check-wavs          decode every .wav file; count them by format; exit 1 if one fails
  check-music         open the default audio device (SDL_AUDIO_DRIVER=dummy for none), queue the
                      menu music paused and print its format, gain and queued bytes
  check-anis          load every animation set and the sprite files it names; list the sets
                      as ok, warning (defective palette) or FAILED, then the counts; exit 1
                      if a set fails
  run <map> --headless (--passes N --clock MS [--script FILE] [--size WxH] | --replay LOG)
      [--png-every K] [--out DIR] [--show ID]...
                      play a map (a .zoo member) without a window. N passes of the main loop
                      with a scripted clock that advances MS milliseconds per pass, the
                      commands of the script applied before their pass (one `<pass> <command>`
                      per line: scroll DX DY, pointer X Y, pointer none, rotate cw|ccw, zoom
                      in|out, fullscreen on|off, pause, quit, and for the interface mouse X Y,
                      down, up, wheel N, key NAME [ctrl] [shift] with NAME enter, escape, tab,
                      backspace, delete, left, right, up, down, home, end, pageup, pagedown, y
                      or n, and char C with C a byte or one character; # starts a comment).
                      The interface is drawn over the map, with the layouts given by --show
                      (each ID shown, repeatable) and the map view; the keys and the edge
                      scroll of the map act only while the map view is shown and no field has
                      the focus. The window is 1024x768 unless
                      --size says otherwise. Prints the size of the window, then, for each pass
                      in which something happens: a line per command applied, then one per
                      timer that fires, draw (the hash of the frame painted, or - if none, and
                      the view), interface, or step (with the date). --replay runs again the
                      passes of such an output saved in LOG, at the size and the clock it
                      records, with its commands, and prints the same lines. A frame painted
                      in a pass whose index is a multiple of K is written to
                      DIR/frame-<pass>.png (DIR is by default the current folder)
  run <map> [--size WxH] [--passes N] [--script FILE] [--log FILE] [--timing MS] [--shot FILE]
      [--show ID]...
                      play a map with a game size of the ini file's screenwidth and
                      screenheight (cut to the display; --size overrides it, uncut), in a
                      window enlarged by the display scale rounded to a whole number (the
                      factor, at least 1, smaller if the window would not fit the display),
                      with the menu music; the ini file's fullscreen (not 0) starts it in full
                      screen, and every switch is written back to the settings of the user folder. The loop runs on the real clock, sleeps until the next timer
                      is due, blocks while the window is minimised or hidden, and shows a frame
                      only when one was painted. Closing the window ends it. --passes ends it
                      after N passes, --script applies commands as above before the pass of
                      their index (fullscreen asks the window to switch), --log writes the
                      lines a headless run prints, so that
                      `run --headless --replay` plays it again with the same frame hashes. It
                      prints the game size and the factor, the window size, the size in pixels
                      and the output size, then
                      the display scale and the pixel density (SDL_VIDEO_DRIVER=dummy shows
                      no window). --timing ends it after MS milliseconds and prints, per
                      timer (draw, interface, step), the count, minimum, median, 95th
                      percentile and maximum of the gaps between its firings, the period it
                      should keep, and a histogram of the gaps: information only.
                      --shot writes the pixels of the last frame, as the renderer has them
                      (at the size in pixels), to FILE as a PNG, and prints its size, number of
                      colours and hash; a blank picture is an error
  dump-layout [--size WxH] [--expansions N]
                      load the interface tree as check-layouts does and resolve it for a window
                      of W by H pixels (1024x768 by default; each side from 1 to 16384), then
                      print one line per element in drawing order: id, type, layer, state and
                      the rectangle on the screen (x, y, width, height), tab separated
  render-ui <out.png> [--size WxH] [--expansions N] [--show ID]... [--hover ID] [--press ID]
                      paint the interface into a PNG file (1024x768 by default): a black frame,
                      the root, and what is shown: each --show shows the layout with that id and
                      its children; --hover highlights an element and --press presses one. Prints
                      the file and its size
  text-width <text> [--font FACE] [--size POINTS]
                      print the width in pixels of the text on one line in the original's
                      font (Liberation Sans, whole-pixel advances, no kerning): FACE is a
                      face string, \"Arial\" (the default) or \"Arial Bold\", and POINTS a size
                      string (8 by default); needs no game data
  draw-text <out.png> --width W [--font FACE] [--size POINTS] [--justify left|center|right]
      [--shadow] [--fore R G B] [--back R G B] <text>
                      draw the text as the original does (wrapped at spaces and line feeds
                      to W pixels, a too long word cut with ...) on a canvas W pixels wide and
                      as tall as the text, filled with the back colour (white), text in the
                      fore colour (black), into a PNG file; prints the size and the hash of
                      the pixels; needs no game data
  check-layouts [--expansions N]
                      load ui/gamescrn.lyt and every layout it names into one tree, for the
                      expansions in the mask N (bit 1 and bit 2, the `id` of an xpac*.cfg; default: those
                      of the install), and print the elements per type, the duplicate ids and the
                      pictures loaded and missing (missing ones are warnings on standard error);
                      exit 1 if the load fails at a section without a known type or without a
                      layout; the interface is then dropped
  timing              print the periods and the cap the ini file gives the main loop, then how
                      many draws, interface updates and steps a ten second run makes with a
                      pass at every millisecond

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
        ["types"] => types(&open_game(data_dir)?),
        ["check-config"] => check_config(&open_game(data_dir)?),
        ["dump-sprite", member, dir] => dump_sprite(&open_game(data_dir)?, member, Path::new(dir)),
        ["check-sprites"] => check_sprites(&open_game(data_dir)?),
        ["which", member] => which(open_game(data_dir)?, member),
        ["movie-info", source] => movie_info(data_dir, source, None),
        ["movie-info", source, "--dump", k, dir] => {
            let k = k.parse().ok().filter(|k| *k > 0).ok_or(CliError::Usage)?;
            movie_info(data_dir, source, Some((k, Path::new(dir))))
        }
        ["map-info", member] => map_info(&open_game(data_dir)?, member),
        ["check-maps"] => check_maps(&open_game(data_dir)?),
        ["render-ani", member, dir] => {
            render_ani(&open_game(data_dir)?, member.to_string(), Path::new(dir))
        }
        ["render-ani", root, dir, "--anim", name] => render_ani(
            &open_game(data_dir)?,
            anim::resolve_path(root, name),
            Path::new(dir),
        ),
        ["check-bitmaps"] => check_bitmaps(&open_game(data_dir)?),
        ["check-wavs"] => check_wavs(&open_game(data_dir)?),
        ["check-music"] => check_music(&open_game(data_dir)?),
        ["check-anis"] => check_anis(&open_game(data_dir)?),
        ["text-width", text, flags @ ..] => textcmd::text_width(text, flags),
        ["draw-text", out, rest @ ..] => match rest.split_last() {
            Some((text, flags)) => textcmd::draw_text(out, text, flags),
            None => Err(CliError::Usage),
        },
        ["timing"] => timing(&open_game(data_dir)?),
        ["check-ground"] => check_ground(&open_game(data_dir)?),
        ["render-ground", key, out] => render_ground(&open_game(data_dir)?, key, Path::new(out)),
        ["render-map", member, out, flags @ ..] => {
            let (rotation, options) = render_flags(flags)?;
            render_map(&open_game(data_dir)?, member, out, rotation, &options)
        }
        ["check-layouts", flags @ ..] => layout::check_layouts(&open_game(data_dir)?, flags),
        ["dump-layout", flags @ ..] => layout::dump_layout(data_dir, flags),
        ["render-ui", out, flags @ ..] => layout::render_ui(data_dir, out, flags),
        ["run", member, flags @ ..] => run::run(&open_game(data_dir)?, member, flags),
        ["check-render"] => check_render(&open_game(data_dir)?, 0),
        ["check-render", "--rotation", n] => check_render(
            &open_game(data_dir)?,
            n.parse().map_err(|_| CliError::Usage)?,
        ),
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
    let mut vfs = Vfs::open(&game_folder(data_dir)?)?;
    for warning in vfs.warnings() {
        eprintln!("rustyzoo: warning: {warning}");
    }
    if let Some(dir) = userdir::dir()
        && let Err(e) = userdir::apply(&mut vfs, &dir)
    {
        eprintln!("rustyzoo: warning: settings of the user folder not used: {e}");
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

/// Prints one line per entity type, then the counts per class. Warnings go to standard error.
fn types(vfs: &Vfs) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    for warning in &catalogue.warnings {
        let _ = writeln!(io::stderr(), "rustyzoo: warning: {warning}");
    }
    let name = |bytes: &[u8]| escape(&String::from_utf8_lossy(bytes));
    let mut report = Report::new();
    for ty in &catalogue.types {
        let [x, y, z] = ty.footprint;
        let member = ty.base_animation().and_then(|animation| animation.member);
        let exists = member.as_deref().is_some_and(|member| vfs.contains(member));
        let member = member.as_deref().unwrap_or("-");
        report.line(format_args!(
            "{}\t{}\t{}\t{}\t{x}x{y}x{z}\t{}\t{}",
            ty.class,
            name(&ty.type_name),
            name(&ty.subtype),
            ty.name_id,
            escape(member),
            if exists { "yes" } else { "no" },
        ));
    }
    let counts: Vec<String> = catalogue
        .counts()
        .iter()
        .map(|(class, count)| format!("{class} {count}"))
        .collect();
    report.line(format_args!(
        "{} types: {}",
        catalogue.types.len(),
        counts.join(", ")
    ));
    report.finish()
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
    let (sprite, palette, warning) = sprite::load(vfs, member)?;
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
    if let Some(warning) = warning {
        report.line(format_args!("warning: {}", warning.escape_debug()));
    }
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
        if frame.shadow {
            // A shadow frame only darkens what is behind it: show it on white.
            image.pixels.fill(255);
        }
        frame.draw(&palette, &mut image, 0, 0);
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
    let (mut files, mut frames, mut shadows, mut extras, mut failed, mut warned) =
        (0, 0, 0, 0, 0, 0);
    for member in vfs.list(None) {
        if !sprite::is_sprite_member(&member) {
            continue;
        }
        files += 1;
        match sprite::load(vfs, &member) {
            Ok((sprite, _, warning)) => {
                if let Some(warning) = warning {
                    warned += 1;
                    report.line(format_args!(
                        "warning {}: {}",
                        member.escape_debug(),
                        warning.escape_debug()
                    ));
                }
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
         {extras} files with an extra frame, {failed} failed, {warned} with a warning"
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {files} sprite files failed").into()),
    }
}

/// Applies [`Zoo::check_types`] with the types of `catalogue`.
fn check_types(zoo: Zoo, catalogue: &Catalogue) -> Result<Zoo, ZooError> {
    let bytes = |s: &str| s.chars().map(|c| c as u8).collect::<Vec<u8>>();
    zoo.check_types(|e| {
        catalogue
            .find(&e.class, &bytes(&e.type_name), &bytes(&e.subtype))
            .is_some()
    })?;
    Ok(zoo)
}

/// How many entities `map-info` lists one by one.
const MAP_INFO_ENTITIES: usize = 5;

fn load_map(vfs: &Vfs, member: &str) -> Result<Zoo, CliError> {
    let bytes = vfs.read(member)?;
    Ok(Zoo::parse(&bytes).map_err(|e| format!("{member}: {e}"))?)
}

/// Reads a movie from a file, or else from the game, decodes it, and prints what it holds.
fn movie_info(
    data_dir: Option<OsString>,
    source: &str,
    dump: Option<(usize, &Path)>,
) -> Result<(), CliError> {
    let bytes = if Path::new(source).is_file() {
        std::fs::read(source).map_err(|e| format!("cannot read {}: {e}", source.escape_debug()))?
    } else {
        open_game(data_dir)?.read(source)?
    };
    let movie = Movie::parse(bytes)?;
    let mut report = Report::new();
    report.line(format_args!("movie: {}", source.escape_debug()));
    report.line(format_args!("size: {}x{}", movie.size().0, movie.size().1));
    report.line(format_args!("frames: {}", movie.frame_count()));
    let (rate, scale) = movie.frame_rate();
    report.line(format_args!(
        "frame rate: {rate}/{scale} ({:.2} per second)",
        f64::from(rate) / f64::from(scale)
    ));
    report.line(format_args!("duration: {} ms", movie.duration_ms()));
    match movie.audio_format() {
        Some(a) => report.line(format_args!(
            "audio: {} Hz, {} channels, {} bits, {} bytes",
            a.rate,
            a.channels,
            a.bits,
            movie.audio().len()
        )),
        None => report.line(format_args!("audio: none")),
    }
    if let Some((_, dir)) = dump {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let (mut decoded, mut dumped) = (0, 0);
    for (number, frame) in movie.frames().enumerate() {
        let frame = frame.map_err(|e| format!("frame {number}: {e}"))?;
        decoded += 1;
        if let Some((_, dir)) = dump.filter(|(k, _)| number % k == 0) {
            let path = dir.join(format!("frame-{number:06}.png"));
            let file = File::create(&path)
                .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
            write_png(&frame, io::BufWriter::new(file))
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            dumped += 1;
        }
    }
    report.line(format_args!("decoded: {decoded} frames"));
    if dump.is_some() {
        report.line(format_args!("dumped: {dumped} pictures"));
    }
    report.finish()
}

fn map_info(vfs: &Vfs, member: &str) -> Result<(), CliError> {
    let zoo = load_map(vfs, member)?;
    let mut report = Report::new();
    report.line(format_args!("member: {}", member.escape_debug()));
    report.line(format_args!("version: {}", zoo.version));
    report.line(format_args!("locale: {}", zoo.locale));
    report.line(format_args!("expansion: {}", zoo.expansion));
    report.line(format_args!("map size: {}x{}", zoo.width, zoo.height));
    report.line(format_args!(
        "entrance: {},{}",
        zoo.entrance.0, zoo.entrance.1
    ));
    report.line(format_args!("habitats: {}", zoo.habitat_count));
    report.line(format_args!("id counter: {}", zoo.id_counter));
    report.line(format_args!("tiles: {}", zoo.tiles.len()));
    let mut terrain = BTreeMap::new();
    for tile in &zoo.tiles {
        *terrain.entry(tile.terrain).or_insert(0usize) += 1;
    }
    for (kind, count) in terrain {
        report.line(format_args!("  terrain {kind}: {count}"));
    }
    let skipped = zoo.entities.iter().filter(|e| e.common.is_none()).count();
    report.line(format_args!(
        "entities: {} ({skipped} without a decoded body)",
        zoo.entities.len()
    ));
    let mut classes = BTreeMap::new();
    for entity in &zoo.entities {
        *classes.entry(entity.class.as_str()).or_insert(0usize) += 1;
    }
    for (class, count) in classes {
        report.line(format_args!("  class {}: {count}", escape(class)));
    }
    for entity in zoo.entities.iter().take(MAP_INFO_ENTITIES) {
        let names = format!(
            "{}/{}/{}",
            escape(&entity.class),
            escape(&entity.type_name),
            escape(&entity.subtype)
        );
        match &entity.common {
            Some(c) => report.line(format_args!(
                "  {names} tile {},{} position {},{},{} rotation {} id {} name \"{}\"",
                c.x.div_euclid(64),
                c.y.div_euclid(64),
                c.x,
                c.y,
                c.z,
                c.rotation,
                c.id,
                escape(&c.name)
            )),
            None => report.line(format_args!("  {names} (body skipped)")),
        }
    }
    report.finish()
}

/// Reads every `.zoo` member. A file the original refuses by its header (not a saved game, a
/// version below 38 or above 106) is a refusal, as expected from leftovers; anything else that
/// fails, including a version below 70 with an entity of an undefined type, is a corrupted
/// file, which makes the command fail.
fn check_maps(vfs: &Vfs) -> Result<(), CliError> {
    let catalogue = Catalogue::load(vfs)?;
    let mut report = Report::new();
    let (mut files, mut ok, mut refused, mut corrupted) = (0, 0, 0, 0);
    let (mut tiles, mut entities) = (0, 0);
    for member in vfs.list(None) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        files += 1;
        let name = member.escape_debug();
        let result = vfs
            .read(&member)
            .map_err(|e| ZooError::Corrupted(e.to_string()))
            .and_then(|bytes| Zoo::parse(&bytes))
            .and_then(|zoo| check_types(zoo, &catalogue));
        match result {
            Ok(zoo) => {
                ok += 1;
                tiles += zoo.tiles.len();
                entities += zoo.entities.len();
                report.line(format_args!(
                    "ok {name}: version {}, {}x{}, {} tiles, {} entities",
                    zoo.version,
                    zoo.width,
                    zoo.height,
                    zoo.tiles.len(),
                    zoo.entities.len()
                ));
            }
            Err(err) => {
                match err {
                    ZooError::Corrupted(_) => corrupted += 1,
                    _ => refused += 1,
                }
                // The text comes from the file system: keep it on one line.
                report.line(format_args!(
                    "FAILED {name}: {}",
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{files} map files, {ok} ok, {refused} refused by version or magic, \
         {corrupted} corrupted; {tiles} tiles, {entities} entities in the ok files"
    ));
    report.finish()?;
    match corrupted {
        0 => Ok(()),
        _ => Err(format!("{corrupted} of {files} map files are corrupted").into()),
    }
}

/// The names of the eight screen directions, in the order of the numbers 0 to 7.
const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// The largest width or height of the canvas of `render-ani`.
const MAX_CANVAS: i64 = sprite::MAX_DIMENSION as i64;

/// Draws frame 0 of the set for each of the eight screen directions on a transparent canvas
/// that holds the box of the set, the box of its frames and their mirror images, with the
/// anchor at (`m`, `top`) where `m` is the largest distance of the box from the anchor to the
/// left or right and `top` its distance above.
fn render_ani(vfs: &Vfs, member: String, dir: &Path) -> Result<(), CliError> {
    let set = AnimSet::load(vfs, &member)?;
    let animation = Animation::open(vfs, set).map_err(|(err, _)| err)?;
    let set = &animation.set;
    let [e0, f0, e1, f1] = animation.extents().unwrap_or_default();
    let (x0, y0, x1, y1) = (
        i64::from(set.x0.min(e0)),
        i64::from(set.y0.min(f0)),
        i64::from(set.x1.max(e1)),
        i64::from(set.y1.max(f1)),
    );
    let reach = x0.abs().max(x1.abs());
    let (anchor_x, anchor_y) = (reach, (-y0).max(0));
    let (width, height) = (2 * reach + 1, (anchor_y + y1.max(0)).max(1));
    if width > MAX_CANVAS || height > MAX_CANVAS {
        return Err(format!("the box {x0} {y0} {x1} {y1} is too large to draw").into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{}: {} files, box {x0} {y0} {x1} {y1}; canvas {width}x{height}, anchor ({anchor_x}, {anchor_y})",
        member.escape_debug(),
        animation.sprites.len(),
    ));
    if !matches!(animation.sprites.len(), 1 | 4 | 5) {
        report.line(format_args!(
            "note: a set of {} files is not chosen by screen direction; the images show the rule for it anyway",
            animation.sprites.len()
        ));
    }
    if let Some(warning) = &animation.warning {
        report.line(format_args!("warning: {}", warning.escape_debug()));
    }
    // A set of shadow frames only darkens what is behind it: show it on white.
    let on_white = animation
        .sprites
        .iter()
        .any(|loaded| loaded.0.frames.first().is_some_and(|frame| frame.shadow));
    for (direction, name) in DIRECTIONS.iter().enumerate() {
        let view = animation.view(direction as u8);
        let mut image = Framebuffer::new(width as u32, height as u32);
        if on_white {
            image.pixels.fill(255);
        }
        animation.draw(view, 0, &mut image, anchor_x as i32, anchor_y as i32);
        let file = format!("dir{direction}-{name}.png");
        let path = dir.join(&file);
        let out =
            File::create(&path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        write_png(&image, io::BufWriter::new(out))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        report.line(format_args!(
            "  {file}: file {} ({}){}",
            view.file,
            animation.set.files[view.file]
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .escape_debug(),
            if view.mirror { ", mirrored" } else { "" },
        ));
    }
    report.finish()
}

/// Loads every `.ani` member and the sprite files it names. A set that cannot be read, or one
/// with a sprite file that cannot be found, fails, as in the original. A defective palette is a
/// warning: the set loads.
fn check_anis(vfs: &Vfs) -> Result<(), CliError> {
    let mut report = Report::new();
    let (mut sets, mut files, mut missing, mut mirrored, mut failed, mut warned) =
        (0, 0, 0, 0, 0, 0);
    let mut sizes = std::collections::BTreeMap::new();
    let (mut exact, mut larger, mut different) = (0, 0, 0);
    for member in vfs.list(None) {
        if !member.ends_with(".ani") {
            continue;
        }
        sets += 1;
        let result = AnimSet::load(vfs, &member).and_then(|set| {
            files += set.files.len();
            *sizes.entry(set.files.len()).or_insert(0) += 1;
            mirrored += (0..8)
                .filter(|&u| anim::view_for(set.files.len(), u).mirror)
                .count();
            Animation::open(vfs, set).map_err(|(err, lost)| {
                missing += lost;
                err
            })
        });
        match result {
            Ok(animation) => {
                let set = &animation.set;
                let boxed = [set.x0, set.y0, set.x1, set.y1];
                let extents = animation.extents().unwrap_or_default();
                let holds = boxed[0] <= extents[0]
                    && boxed[1] <= extents[1]
                    && boxed[2] >= extents[2]
                    && boxed[3] >= extents[3];
                match (boxed == extents, holds) {
                    (true, _) => exact += 1,
                    (_, true) => larger += 1,
                    _ => different += 1,
                }
                match &animation.warning {
                    Some(warning) => {
                        warned += 1;
                        report.line(format_args!(
                            "warning {}: {}",
                            member.escape_debug(),
                            warning.escape_debug()
                        ));
                    }
                    None => report.line(format_args!("ok {}", member.escape_debug())),
                }
            }
            Err(err) => {
                failed += 1;
                // Names come from the files: print them with their control characters escaped.
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    let sizes: Vec<String> = sizes.iter().map(|(n, c)| format!("{n}:{c}")).collect();
    report.line(format_args!(
        "{sets} animation sets, {files} sprite file references, {missing} missing files, \
         {mirrored} mirrored views by the file-count rule, {failed} failed, \
         {warned} with a warning; sets by file count {}; \
         box: {exact} equal to the frame extents, {larger} larger, {different} different",
        sizes.join(" ")
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} of {sets} animation sets failed").into()),
    }
}

/// Differences between the rendered and the cached pictures of one terrain type.
struct GroundStats {
    pictures: usize,
    /// Pictures where some pixel is covered in only one of the two.
    differing: usize,
    /// Pixels covered in only one of the two.
    uncommon: usize,
    /// Pictures whose cut differs although the covered pixels do not.
    boxes: usize,
    /// Pixels covered in both, and the sum of the absolute differences per channel.
    covered: u64,
    sums: [u64; 3],
    /// How many (pixel, channel) pairs differ by each amount.
    histogram: Vec<u64>,
}

impl GroundStats {
    fn new() -> GroundStats {
        GroundStats {
            pictures: 0,
            differing: 0,
            uncommon: 0,
            boxes: 0,
            covered: 0,
            sums: [0; 3],
            histogram: vec![0; 256],
        }
    }

    fn add(&mut self, other: &GroundStats) {
        self.pictures += other.pictures;
        self.differing += other.differing;
        self.uncommon += other.uncommon;
        self.boxes += other.boxes;
        self.covered += other.covered;
        for c in 0..3 {
            self.sums[c] += other.sums[c];
        }
        for (a, b) in self.histogram.iter_mut().zip(&other.histogram) {
            *a += b;
        }
    }

    /// The smallest difference that at least `percent` of the (pixel, channel) pairs do not exceed.
    fn percentile(&self, percent: u64) -> usize {
        let total: u64 = self.histogram.iter().sum();
        let mut seen = 0;
        for (value, count) in self.histogram.iter().enumerate() {
            seen += count;
            if seen * 100 >= total * percent {
                return value;
            }
        }
        0
    }

    /// The counts, then after a semicolon the colour differences. `extra` goes at the end of the
    /// counts.
    fn describe(&self, extra: &str) -> String {
        let mean = self.sums.map(|s| s as f64 / self.covered.max(1) as f64);
        format!(
            "{} pictures, {} covered pixels, {} pictures with differing covered pixels, \
             {} differing pixels, {} pictures with another cut{extra}; colour difference per \
             channel over covered pixels: mean R {:.2} G {:.2} B {:.2}, 95th percentile {}, max {}",
            self.pictures,
            self.covered,
            self.differing,
            self.uncommon,
            self.boxes,
            mean[0],
            mean[1],
            mean[2],
            self.percentile(95),
            self.histogram.iter().rposition(|&n| n > 0).unwrap_or(0),
        )
    }
}

/// Renders every picture of the terrain cache again and compares the pixels both cover.
fn check_ground(vfs: &Vfs) -> Result<(), CliError> {
    let renderer = Renderer::load(vfs)?;
    let mut report = Report::new();
    let mut per_type: BTreeMap<(u8, bool), GroundStats> = Default::default();
    let mut failed = 0;
    let members = [vfs.list(Some("ztats/*.lle")), vfs.list(Some("ztatb/*.lle"))].concat();
    let palettes: HashSet<String> = [vfs.list(Some("ztats/*.pal")), vfs.list(Some("ztatb/*.pal"))]
        .concat()
        .into_iter()
        .collect();
    let (mut plain, mut blended) = (GroundStats::new(), GroundStats::new());
    let mut unusable = 0;
    for member in members {
        // The original renders a picture whose palette is missing, so there is nothing to compare.
        if Key::parse(&member).is_ok_and(|key| !palettes.contains(&key.palette())) {
            unusable += 1;
            continue;
        }
        let compared = ground::load_cached(vfs, &member).and_then(|(key, cached)| {
            let rendered = renderer.render(&key)?;
            Ok((key, cached, rendered))
        });
        let (key, cached, rendered) = match compared {
            Ok(compared) => compared,
            Err(err) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {}: {}",
                    member.escape_debug(),
                    err.to_string().escape_debug()
                ));
                continue;
            }
        };
        let mut one = GroundStats::new();
        one.pictures = 1;
        let (a, b) = (cached.on_canvas(), rendered.on_canvas());
        for (a, b) in a
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.pixels.as_chunks::<4>().0)
        {
            match (a[3] != 0, b[3] != 0) {
                (true, true) => {
                    one.covered += 1;
                    for c in 0..3 {
                        let difference = a[c].abs_diff(b[c]);
                        one.sums[c] += u64::from(difference);
                        one.histogram[usize::from(difference)] += 1;
                    }
                }
                (false, false) => {}
                _ => one.uncommon += 1,
            }
        }
        if one.uncommon > 0 {
            one.differing = 1;
            report.line(format_args!("DIFFERS {member}: {} pixels", one.uncommon));
        } else if (
            cached.image.width,
            cached.image.height,
            cached.x_offset,
            cached.y_offset,
        ) != (
            rendered.image.width,
            rendered.image.height,
            rendered.x_offset,
            rendered.y_offset,
        ) {
            one.boxes = 1;
            report.line(format_args!("CUT {member}"));
        }
        per_type
            .entry((key.terrain, key.is_uniform()))
            .or_insert_with(GroundStats::new)
            .add(&one);
        if key.is_uniform() {
            &mut plain
        } else {
            &mut blended
        }
        .add(&one);
    }
    let mut total = GroundStats::new();
    for ((terrain, uniform), stats) in &per_type {
        let kind = if *uniform { "plain" } else { "blended" };
        report.line(format_args!(
            "type {terrain:02x} {kind}: {}",
            stats.describe("")
        ));
        total.add(stats);
    }
    report.line(format_args!(
        "plain (ztats, lit in the cache, rendered unlit): {}",
        plain.describe("")
    ));
    report.line(format_args!("blended (ztatb): {}", blended.describe("")));
    report.line(format_args!(
        "total: {}",
        total.describe(&format!(", {unusable} without palette, {failed} failed"))
    ));
    report.finish()?;
    if total.uncommon > 0 || total.boxes > 0 || failed > 0 {
        return Err(format!(
            "{} pictures differ in covered pixels, {} in their cut, {failed} failed",
            total.differing, total.boxes
        )
        .into());
    }
    Ok(())
}

fn render_ground(vfs: &Vfs, key: &str, out: &Path) -> Result<(), CliError> {
    let key = Key::parse(key)?;
    let picture = Renderer::load(vfs)?.render(&key)?;
    if picture.image.width == 0 {
        return Err(format!("{} draws nothing", key.member()).into());
    }
    let file = File::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    write_png(&picture.image, io::BufWriter::new(file))
        .map_err(|e| format!("cannot write {}: {e}", out.display()))?;
    println!(
        "{}: {}x{} x offset {} y offset {}",
        key.member(),
        picture.image.width,
        picture.image.height,
        picture.x_offset,
        picture.y_offset
    );
    Ok(())
}

/// The rotation and the options of `render-map`, from the words after the output file.
fn render_flags(mut flags: &[&str]) -> Result<(u8, Options), CliError> {
    let (mut rotation, mut options) = (0, Options::default());
    let number = |word: &&str| word.parse::<i32>().map_err(|_| CliError::Usage);
    while let [flag, rest @ ..] = flags {
        flags = match (*flag, rest) {
            ("--rotation", [n, rest @ ..]) => {
                rotation = n.parse().map_err(|_| CliError::Usage)?;
                rest
            }
            ("--view", [x, y, w, h, rest @ ..]) => {
                let size = |word: &&str| {
                    u32::try_from(number(word)?)
                        .ok()
                        .filter(|&n| n > 0)
                        .ok_or(CliError::Usage)
                };
                options.window = Some((number(x)?, number(y)?, size(w)?, size(h)?));
                rest
            }
            ("--no-units", rest) => {
                options.no_units = true;
                rest
            }
            ("--zoom", [z, rest @ ..]) => {
                options.zoom = number(z)?;
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    Ok((rotation, options))
}

/// Draws one map into a PNG file and prints the summary.
fn render_map(
    vfs: &Vfs,
    member: &str,
    out: &str,
    rotation: u8,
    options: &Options,
) -> Result<(), CliError> {
    if rotation > 3 {
        return Err(CliError::Usage);
    }
    let catalogue = Catalogue::load(vfs)?;
    let zoo =
        check_types(load_map(vfs, member)?, &catalogue).map_err(|e| format!("{member}: {e}"))?;
    let (image, summary) =
        MapRenderer::new(vfs, &catalogue).render_with(&zoo, rotation, options)?;
    let file = File::create(out).map_err(|e| format!("cannot create {out}: {e}"))?;
    write_png(&image, io::BufWriter::new(file)).map_err(|e| format!("cannot write {out}: {e}"))?;
    let mut report = Report::new();
    report.line(format_args!(
        "{} -> {}x{} pixels: {summary}",
        member.escape_debug(),
        image.width,
        image.height
    ));
    report.finish()
}

/// Draws every map in memory and prints the summaries. A file the loader refuses is skipped,
/// as `check-maps` lists it; only a map that cannot be drawn fails the command.
fn check_render(vfs: &Vfs, rotation: u8) -> Result<(), CliError> {
    if rotation > 3 {
        return Err(CliError::Usage);
    }
    let catalogue = Catalogue::load(vfs)?;
    let mut renderer = MapRenderer::new(vfs, &catalogue);
    let mut report = Report::new();
    let (mut total, mut maps, mut skipped, mut failed) = (Summary::default(), 0, 0, 0);
    for member in vfs.list(None) {
        if !member.to_ascii_lowercase().ends_with(".zoo") {
            continue;
        }
        let name = member.escape_debug();
        let zoo = vfs
            .read(&member)
            .ok()
            .and_then(|bytes| Zoo::parse(&bytes).ok())
            .and_then(|zoo| check_types(zoo, &catalogue).ok());
        let Some(zoo) = zoo else {
            skipped += 1;
            report.line(format_args!("skipped {name}: cannot be loaded"));
            continue;
        };
        match renderer
            .stage(&zoo, rotation, false)
            .and_then(|staged| staged.size().map(|_| staged.summary))
        {
            Ok(summary) => {
                maps += 1;
                report.line(format_args!("ok {name}: {summary}"));
                total.add(&summary);
            }
            Err(err) => {
                failed += 1;
                report.line(format_args!(
                    "FAILED {name}: {}",
                    err.to_string().escape_debug()
                ));
            }
        }
    }
    report.line(format_args!(
        "{maps} maps drawn, {skipped} skipped, {failed} failed; totals: {total}"
    ));
    report.finish()?;
    match failed {
        0 => Ok(()),
        _ => Err(format!("{failed} maps could not be drawn").into()),
    }
}

/// The loop settings of the ini file, and a simulated run with a pass at every millisecond.
fn timing(vfs: &Vfs) -> Result<(), CliError> {
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
fn check_bitmaps(vfs: &Vfs) -> Result<(), CliError> {
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

fn check_wavs(vfs: &Vfs) -> Result<(), CliError> {
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

fn check_music(vfs: &Vfs) -> Result<(), CliError> {
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
    use super::{escape, frame_file_name, render_flags};

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

    #[test]
    fn render_map_flags_come_in_any_order_and_bad_ones_are_refused() {
        let ok = |flags: &[&str]| {
            render_flags(flags)
                .ok()
                .map(|(r, o)| (r, o.window, o.no_units, o.zoom))
        };
        assert_eq!(ok(&[]), Some((0, None, false, 0)));
        assert_eq!(ok(&["--zoom", "-2"]), Some((0, None, false, -2)));
        assert_eq!(
            ok(&[
                "--no-units",
                "--view",
                "-5",
                "7",
                "30",
                "20",
                "--rotation",
                "2"
            ]),
            Some((2, Some((-5, 7, 30, 20)), true, 0))
        );
        for bad in [
            &["--view", "1", "2", "3"][..],
            &["--view", "1", "2", "-3", "4"],
            &["--view", "1", "2", "0", "4"],
            &["--view", "1", "2", "3", "0"],
            &["--view", "1", "2", "3", "x"],
            &["--rotation"],
            &["--zoom"],
            &["--zoom", "x"],
            &["--unit"],
        ] {
            assert!(ok(bad).is_none(), "{bad:?}");
        }
    }
}
