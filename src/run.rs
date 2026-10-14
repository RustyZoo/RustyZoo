//! `rustyzoo run`: the map view's game driven by a scripted clock and a script of commands,
//! without a window. It prints the lines of [`App::log_lines`], the same ones a windowed run
//! prints, so a log replayed here gives the same frame hashes.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use rustyzoo::app::{App, Command};
use rustyzoo::camera::Settings;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::render::MapRenderer;
use rustyzoo::sprite::write_png;
use rustyzoo::timing::Timers;
use rustyzoo::vfs::Vfs;

use super::{CliError, Report, check_types, load_map};

/// The window of a run without `--size`.
const DEFAULT_SIZE: (u32, u32) = (1024, 768);

/// The words after `run <map>`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Flags {
    passes: u64,
    /// Milliseconds the scripted clock advances per pass.
    clock: u64,
    script: Option<String>,
    png_every: Option<u64>,
    size: Option<(u32, u32)>,
    out: Option<String>,
}

fn flags(mut words: &[&str]) -> Result<Flags, CliError> {
    let mut flags = Flags::default();
    let (mut headless, mut passes, mut clock) = (false, false, false);
    let number = |w: &str| w.parse::<u64>().map_err(|_| CliError::Usage);
    while let [word, rest @ ..] = words {
        words = match (*word, rest) {
            ("--headless", rest) => {
                headless = true;
                rest
            }
            ("--passes", [n, rest @ ..]) => {
                (flags.passes, passes) = (number(n)?, true);
                rest
            }
            ("--clock", [n, rest @ ..]) => {
                (flags.clock, clock) = (number(n)?, true);
                rest
            }
            ("--script", [file, rest @ ..]) => {
                flags.script = Some(file.to_string());
                rest
            }
            ("--png-every", [n, rest @ ..]) => {
                flags.png_every = Some(number(n)?).filter(|&k| k > 0);
                if flags.png_every.is_none() {
                    return Err(CliError::Usage);
                }
                rest
            }
            ("--size", [size, rest @ ..]) => {
                let (w, h) = size.split_once('x').ok_or(CliError::Usage)?;
                let side = |s: &str| s.parse::<u32>().map_err(|_| CliError::Usage);
                flags.size = Some((side(w)?, side(h)?));
                rest
            }
            ("--out", [dir, rest @ ..]) => {
                flags.out = Some(dir.to_string());
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    // The window is not there yet: only the headless run exists.
    match headless && passes && clock {
        true => Ok(flags),
        false => Err(CliError::Usage),
    }
}

/// The lines `<pass index> <command>` of a script, in file order; blank lines and lines that
/// start with `#` are skipped.
fn script(text: &str) -> Result<Vec<(u64, Command)>, String> {
    let mut commands = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let at = |why: String| format!("script line {}: {why}", n + 1);
        let (pass, command) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let pass = pass
            .parse()
            .map_err(|_| at(format!("not a pass: {pass:?}")))?;
        commands.push((pass, command.parse().map_err(at)?));
    }
    // Stable: the commands of one pass keep their order.
    commands.sort_by_key(|&(pass, _)| pass);
    Ok(commands)
}

pub fn run(vfs: &Vfs, member: &str, words: &[&str]) -> Result<(), CliError> {
    let flags = flags(words)?;
    let mut commands = match &flags.script {
        Some(file) => script(&std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?)?,
        None => Vec::new(),
    }
    .into_iter()
    .peekable();
    let catalogue = Catalogue::load(vfs)?;
    let zoo =
        check_types(load_map(vfs, member)?, &catalogue).map_err(|e| format!("{member}: {e}"))?;
    let ini = Config::parse(vfs.ini())?;
    let mut app = App::new(
        MapRenderer::new(vfs, &catalogue),
        zoo,
        Settings::from_config(&ini),
        Timers::from_config(&ini),
        flags.size.unwrap_or(DEFAULT_SIZE),
    )?;
    let out = PathBuf::from(flags.out.as_deref().unwrap_or("."));
    let mut report = Report::new();
    for pass in 0..flags.passes {
        let now = pass.saturating_mul(flags.clock);
        let mut due = Vec::new();
        while let Some((_, command)) = commands.next_if(|&(at, _)| at <= pass) {
            due.push(command);
        }
        if let Some(frame) = app.pass(now, &due)
            && flags.png_every.is_some_and(|k| pass % k == 0)
        {
            let file = out.join(format!("frame-{pass:06}.png"));
            let png = File::create(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            write_png(frame, BufWriter::new(png))
                .map_err(|e| format!("{}: {e}", file.display()))?;
        }
        for line in app.log_lines(pass, now) {
            report.line(format_args!("{line}"));
        }
        if app.quit() {
            break;
        }
    }
    report.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_come_in_any_order_and_bad_ones_are_refused() {
        let got = flags(&[
            "--clock",
            "20",
            "--headless",
            "--passes",
            "5",
            "--size",
            "64x48",
        ]);
        let got = got.unwrap_or_else(|_| panic!("refused"));
        assert_eq!((got.passes, got.clock, got.size), (5, 20, Some((64, 48))));
        for bad in [
            &["--passes", "5", "--clock", "20"][..],
            &["--headless", "--passes", "5"],
            &["--headless", "--passes", "x", "--clock", "1"],
            &[
                "--headless",
                "--passes",
                "1",
                "--clock",
                "1",
                "--png-every",
                "0",
            ],
            &[
                "--headless",
                "--passes",
                "1",
                "--clock",
                "1",
                "--size",
                "64",
            ],
            &["--headless", "--passes", "1", "--clock", "1", "--png-every"],
            &["--headless", "--passes", "1", "--clock", "1", "--window"],
        ] {
            assert!(flags(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_script_is_read_in_file_order_per_pass() {
        let text = "# comment\n\n3 pause\n0 scroll 1 0\n 3  rotate cw \n";
        assert_eq!(
            script(text).unwrap(),
            [
                (0, Command::Scroll(1, 0)),
                (3, Command::Pause),
                (3, Command::Rotate(true))
            ]
        );
        for bad in ["x pause", "3", "3 fly", "-1 pause"] {
            assert!(script(bad).is_err(), "{bad:?}");
        }
        assert!(script("4 fly").unwrap_err().starts_with("script line 1:"));
    }
}
