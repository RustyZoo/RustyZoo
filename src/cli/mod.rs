//! The command line: the words after `--data` pick a command, and each family of commands lives in
//! its own module.

mod checks;
mod files;
mod ground;
mod maps;
mod render;
mod sprites;
mod usage;

use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rustyzoo::anim;
use rustyzoo::userdir;
use rustyzoo::vfs::Vfs;

use crate::{findcmd, layout, run, simcmd, textcmd};

use usage::HELP;
pub use usage::USAGE;

pub enum CliError {
    Usage,
    Failed(Box<dyn Error>),
}

impl<E: Into<Box<dyn Error>>> From<E> for CliError {
    fn from(err: E) -> Self {
        CliError::Failed(err.into())
    }
}

pub fn run(args: Vec<OsString>) -> Result<(), CliError> {
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
        ["help" | "--help" | "-h"] => {
            ignore_closed_pipe(io::stdout().lock().write_all(HELP.as_bytes()))
        }
        ["window-smoke"] => Ok(checks::window_smoke()?),
        ["ls"] => files::list(open_game(data_dir)?, None),
        ["ls", pattern] => files::list(open_game(data_dir)?, Some(pattern)),
        ["cat", member] => {
            let bytes = open_game(data_dir)?.read(member)?;
            ignore_closed_pipe(io::stdout().lock().write_all(&bytes))
        }
        ["dump-strings", dll] => files::dump_strings(game_folder(data_dir)?, dll, None),
        ["dump-strings", dll, "--text", name] => {
            files::dump_strings(game_folder(data_dir)?, dll, Some(name))
        }
        ["strings"] => files::merged_strings(&game_folder(data_dir)?),
        ["types"] => files::types(&open_game(data_dir)?),
        ["check-config"] => files::check_config(&open_game(data_dir)?),
        ["check-behaviours"] => files::check_behaviours(&open_game(data_dir)?, None),
        ["check-behaviours", "--type", name] => {
            files::check_behaviours(&open_game(data_dir)?, Some(name))
        }
        ["check-sprites"] => sprites::check_sprites(&open_game(data_dir)?),
        ["which", member] => files::which(open_game(data_dir)?, member),
        ["movie-info", source] => maps::movie_info(data_dir, source, None),
        ["movie-info", source, "--dump", k, dir] => {
            let k = k.parse().ok().filter(|k| *k > 0).ok_or(CliError::Usage)?;
            maps::movie_info(data_dir, source, Some((k, Path::new(dir))))
        }
        ["check-maps"] => maps::check_maps(&open_game(data_dir)?),
        ["check-exhibits", map] => maps::check_exhibits(&open_game(data_dir)?, map),
        ["check-rating", map] => maps::check_rating(&open_game(data_dir)?, map),
        ["render-ani", member, dir] => {
            sprites::render_ani(&open_game(data_dir)?, member.to_string(), Path::new(dir))
        }
        ["render-ani", root, dir, "--anim", name] => sprites::render_ani(
            &open_game(data_dir)?,
            anim::resolve_path(root, name),
            Path::new(dir),
        ),
        ["check-bitmaps"] => checks::check_bitmaps(&open_game(data_dir)?),
        ["check-wavs"] => checks::check_wavs(&open_game(data_dir)?),
        ["check-music"] => checks::check_music(&open_game(data_dir)?),
        ["check-anis"] => sprites::check_anis(&open_game(data_dir)?),
        ["text-width", text, flags @ ..] => textcmd::text_width(text, flags),
        ["draw-text", out, rest @ ..] => match rest.split_last() {
            Some((text, flags)) => textcmd::draw_text(out, text, flags),
            None => Err(CliError::Usage),
        },
        ["check-ground"] => ground::check_ground(&open_game(data_dir)?),
        ["render-ground", key, out] => {
            ground::render_ground(&open_game(data_dir)?, key, Path::new(out))
        }
        ["render-map", member, out, flags @ ..] => {
            let (rotation, options) = render::render_flags(flags)?;
            render::render_map(&open_game(data_dir)?, member, out, rotation, &options)
        }
        ["check-layouts", flags @ ..] => layout::check_layouts(&open_game(data_dir)?, flags),
        ["dump-layout", flags @ ..] => layout::dump_layout(data_dir, flags),
        ["render-ui", out, flags @ ..] => layout::render_ui(data_dir, out, flags),
        ["run", flags @ ..] if flags.first().is_none_or(|w| w.starts_with("--")) => {
            run::run(&open_game(data_dir)?, None, flags)
        }
        ["run", member, flags @ ..] => run::run(&open_game(data_dir)?, Some(member), flags),
        ["find-path", member, flags @ ..] => {
            findcmd::find_path(&open_game(data_dir)?, member, flags)
        }
        ["sim", member, flags @ ..] => simcmd::sim(&open_game(data_dir)?, member, flags),
        ["check-render"] => render::check_render(&open_game(data_dir)?, 0),
        ["check-render", "--rotation", n] => render::check_render(
            &open_game(data_dir)?,
            n.parse().map_err(|_| CliError::Usage)?,
        ),
        _ => Err(CliError::Usage),
    }
}

/// The game folder given by `--data`, or else by `ZT_DATA_DIR`.
pub fn game_folder(data_dir: Option<OsString>) -> Result<PathBuf, CliError> {
    data_dir
        .or_else(|| std::env::var_os("ZT_DATA_DIR").filter(|dir| !dir.is_empty()))
        .map(PathBuf::from)
        .ok_or_else(|| "no game folder: pass --data <dir> or set ZT_DATA_DIR".into())
}

pub fn open_game(data_dir: Option<OsString>) -> Result<Vfs, CliError> {
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

/// A reader that stops early (`| head`) is not an error of ours.
pub fn ignore_closed_pipe(result: io::Result<()>) -> Result<(), CliError> {
    match result {
        Err(err) if err.kind() != io::ErrorKind::BrokenPipe => Err(err.into()),
        _ => Ok(()),
    }
}

/// Lines for standard output that keep the work going when the reader has gone away: after the
/// first failed write nothing more is written, and [`Report::finish`] treats a closed pipe like
/// `ls` and `cat` do.
pub struct Report {
    out: io::StdoutLock<'static>,
    written: io::Result<()>,
}

impl Report {
    pub fn new() -> Report {
        Report {
            out: io::stdout().lock(),
            written: Ok(()),
        }
    }

    pub fn line(&mut self, line: fmt::Arguments) {
        if self.written.is_ok() {
            self.written = writeln!(self.out, "{line}");
        }
    }

    pub fn finish(mut self) -> Result<(), CliError> {
        let flushed = self.out.flush();
        ignore_closed_pipe(self.written.and(flushed))
    }
}
