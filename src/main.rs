use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustyzoo::Framebuffer;
use rustyzoo::anim::{self, AnimSet, Animation};
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::pe;
use rustyzoo::sprite::{self, write_png};
use rustyzoo::strings::StringTables;
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
  map-info <member>   print the version, size, tiles and entities of a map or saved game (a .zoo)
  check-maps          read every .zoo file; list those that fail; exit 1 if one is corrupted
  render-ani <member.ani> <dir> [--anim NAME]
                      write frame 0 of an animation set as eight PNG files into <dir>, one per
                      screen direction, and print the anchor point; with --anim, the first
                      argument is an animation root (such as animals/lion/f) and NAME the
                      animation under it
  check-anis          load every animation set and the sprite files it names; list the sets
                      as ok, warning (defective palette) or FAILED, then the counts; exit 1
                      if a set fails

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
        ["check-anis"] => check_anis(&open_game(data_dir)?),
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

/// How many entities `map-info` lists one by one.
const MAP_INFO_ENTITIES: usize = 5;

fn load_map(vfs: &Vfs, member: &str) -> Result<Zoo, CliError> {
    let bytes = vfs.read(member)?;
    Ok(Zoo::parse(&bytes).map_err(|e| format!("{member}: {e}"))?)
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
/// fails is a corrupted file, which makes the command fail.
fn check_maps(vfs: &Vfs) -> Result<(), CliError> {
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
            .and_then(|bytes| Zoo::parse(&bytes));
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
