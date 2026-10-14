//! `rustyzoo run`: the map view's game. With `--headless` it is driven by a scripted clock and a
//! script of commands, or by the log of an earlier run, without a window: it prints the size of
//! the window and the lines of [`App::log_lines`]. Without it the game opens a window (see
//! `window/`) and `--log` writes the same lines to a file, so a log replayed here gives the
//! same frame hashes.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use rustyzoo::app::{App, Command};
use rustyzoo::camera::Settings;
use rustyzoo::config::Config;
use rustyzoo::entity::Catalogue;
use rustyzoo::music::Music;
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
    headless: bool,
    /// Where a windowed run writes its log.
    log: Option<String>,
    /// `--timing`: a windowed run of this many milliseconds that prints the timer intervals.
    timing: Option<u64>,
    /// `--shot`: where a windowed run writes the pixels the renderer presents.
    shot: Option<String>,
    /// The passes to run; a windowed run without it ends when the player closes the window.
    passes: Option<u64>,
    /// Milliseconds the scripted clock advances per pass.
    clock: u64,
    script: Option<String>,
    png_every: Option<u64>,
    size: Option<(u32, u32)>,
    out: Option<String>,
    /// A log to run again instead of `passes` and `clock`.
    replay: Option<String>,
}

fn flags(mut words: &[&str]) -> Result<Flags, CliError> {
    let mut flags = Flags::default();
    let (mut passes, mut clock) = (false, false);
    let mut size = false;
    let number = |w: &str| w.parse::<u64>().map_err(|_| CliError::Usage);
    while let [word, rest @ ..] = words {
        words = match (*word, rest) {
            ("--headless", rest) => {
                flags.headless = true;
                rest
            }
            ("--log", [file, rest @ ..]) => {
                flags.log = Some(file.to_string());
                rest
            }
            ("--timing", [ms, rest @ ..]) => {
                flags.timing = Some(number(ms)?).filter(|&n| n > 0);
                if flags.timing.is_none() {
                    return Err(CliError::Usage);
                }
                rest
            }
            ("--shot", [file, rest @ ..]) => {
                flags.shot = Some(file.to_string());
                rest
            }
            ("--passes", [n, rest @ ..]) => {
                if number(n)? == 0 {
                    return Err("--passes: at least one pass".into());
                }
                (flags.passes, passes) = (Some(number(n)?), true);
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
            ("--size", [text, rest @ ..]) => {
                let (w, h) = text.split_once('x').ok_or(CliError::Usage)?;
                let side = |s: &str| s.parse::<u32>().map_err(|_| CliError::Usage);
                let (w, h) = (side(w)?, side(h)?);
                if w == 0 || h == 0 {
                    return Err("--size: the window needs at least one pixel".into());
                }
                (flags.size, size) = (Some((w, h)), true);
                rest
            }
            ("--replay", [file, rest @ ..]) => {
                flags.replay = Some(file.to_string());
                rest
            }
            ("--out", [dir, rest @ ..]) => {
                flags.out = Some(dir.to_string());
                rest
            }
            _ => return Err(CliError::Usage),
        };
    }
    // A replay brings its own passes, clock, commands and size. A window runs on the real clock
    // and writes a log instead of printing it.
    let scripted = passes && clock;
    let replayed = flags.replay.is_some() && !(passes || clock || size || flags.script.is_some());
    let ok = match flags.headless {
        true => {
            flags.log.is_none()
                && flags.timing.is_none()
                && flags.shot.is_none()
                && (scripted != flags.replay.is_some())
                && (scripted || replayed)
        }
        false => {
            !clock && flags.replay.is_none() && flags.png_every.is_none() && flags.out.is_none()
        }
    };
    match ok {
        true => Ok(flags),
        false => Err(CliError::Usage),
    }
}

/// The lines `<pass index> <command>` of a script, in file order; blank lines and lines that
/// start with `#` are skipped.
pub fn script(text: &str) -> Result<Vec<(u64, Command)>, String> {
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

/// One pass of a log: its index, its clock and the commands it applied.
type Logged = (u64, u64, Vec<Command>);

/// The size and the passes of a log, as [`App::log_header`] and [`App::log_lines`] print them. A
/// pass is a run of lines with the same index and clock; the lines of the timers only say that
/// the pass ran, and the commands are read back.
fn replay(text: &str) -> Result<((u32, u32), Vec<Logged>), String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty());
    let (_, header) = lines.next().ok_or("the log is empty")?;
    let size = header
        .strip_prefix("size ")
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .ok_or("the log does not start with its size")?;
    let mut passes: Vec<Logged> = Vec::new();
    for (n, line) in lines {
        let at = |why: String| format!("log line {}: {why}", n + 1);
        let mut words = line.splitn(4, ' ');
        let (Some("pass"), Some(pass), Some(clock), rest) =
            (words.next(), words.next(), words.next(), words.next())
        else {
            return Err(at(format!("not a log line: {line:?}")));
        };
        let number = |w: &str, prefix: &str| {
            w.strip_prefix(prefix)
                .and_then(|n| n.parse::<u64>().ok())
                .ok_or_else(|| at(format!("not a pass: {line:?}")))
        };
        let key = (number(pass, "")?, number(clock, "t=")?);
        if passes.last().is_none_or(|p| (p.0, p.1) != key) {
            passes.push((key.0, key.1, Vec::new()));
        }
        let rest = rest.unwrap_or("");
        if !["draw ", "interface ", "music ", "step "]
            .iter()
            .any(|k| rest.starts_with(k))
        {
            let last = passes.last_mut().expect("just pushed");
            last.2.push(rest.parse().map_err(at)?);
        }
    }
    Ok((size, passes))
}

pub fn run(vfs: &Vfs, member: &str, words: &[&str]) -> Result<(), CliError> {
    let flags = flags(words)?;
    let read = |file: &str| std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"));
    let (logged, size) = match &flags.replay {
        Some(file) => {
            let (size, passes) = replay(&read(file)?).map_err(|e| format!("{file}: {e}"))?;
            (Some(passes), Some(size))
        }
        None => (None, flags.size),
    };
    let mut commands = match &flags.script {
        Some(file) => script(&read(file)?)?,
        None => Vec::new(),
    }
    .into_iter()
    .peekable();
    let catalogue = Catalogue::load(vfs)?;
    let zoo =
        check_types(load_map(vfs, member)?, &catalogue).map_err(|e| format!("{member}: {e}"))?;
    let ini = Config::parse(vfs.ini())?;
    if !flags.headless {
        let window = crate::window::Flags {
            size: flags.size,
            log: flags.log.as_deref(),
            passes: flags.passes,
            script: commands.collect(),
            timing: flags.timing,
            shot: flags.shot.as_deref(),
        };
        return crate::window::run(vfs, &catalogue, zoo, &ini, window);
    }
    let mut app = App::new(
        MapRenderer::new(vfs, &catalogue),
        zoo,
        Settings::from_config(&ini),
        Timers::from_config(&ini),
        size.unwrap_or(DEFAULT_SIZE),
    )?;
    // The game starts with the run: the menu music, which only the log tells of here, fades.
    app.start_game(Music::from_config(&ini));
    let out = PathBuf::from(flags.out.as_deref().unwrap_or("."));
    let mut report = Report::new();
    report.line(format_args!("{}", app.log_header()));
    // Every pass of the scripted clock, or only the logged ones: a pass in which nothing fires
    // and no command comes changes nothing.
    let mut play = |pass: u64, now: u64, due: &[Command]| -> Result<bool, CliError> {
        if let Some(frame) = app.pass(now, due)
            && flags.png_every.is_some_and(|k| pass.is_multiple_of(k))
        {
            let file = out.join(format!("frame-{pass:06}.png"));
            let png = File::create(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            write_png(frame, BufWriter::new(png))
                .map_err(|e| format!("{}: {e}", file.display()))?;
        }
        for line in app.log_lines(pass, now) {
            report.line(format_args!("{line}"));
        }
        Ok(app.quit())
    };
    if let Some(passes) = logged {
        for (pass, now, due) in passes {
            if play(pass, now, &due)? {
                break;
            }
        }
    } else {
        for pass in 0..flags.passes.unwrap_or(0) {
            let mut due = Vec::new();
            while let Some((_, command)) = commands.next_if(|&(at, _)| at <= pass) {
                due.push(command);
            }
            if play(pass, pass.saturating_mul(flags.clock), &due)? {
                break;
            }
        }
    }
    drop(play);
    report.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_come_in_any_order_and_bad_ones_are_refused() {
        assert!(flags(&["--headless", "--replay", "log", "--png-every", "2"]).is_ok());
        assert!(matches!(
            flags(&[
                "--headless",
                "--passes",
                "1",
                "--clock",
                "1",
                "--size",
                "0x0"
            ]),
            Err(CliError::Failed(_))
        ));
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
        assert_eq!(
            (got.passes, got.clock, got.size),
            (Some(5), 20, Some((64, 48)))
        );
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
            &[
                "--headless",
                "--replay",
                "log",
                "--passes",
                "1",
                "--clock",
                "1",
            ],
            &["--headless", "--replay", "log", "--size", "1x1"],
            &["--headless", "--replay", "log", "--script", "s"],
        ] {
            assert!(flags(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_window_takes_its_own_flags() {
        let got = flags(&[
            "--size", "64x48", "--log", "l", "--passes", "3", "--script", "s",
        ]);
        let got = got.unwrap_or_else(|_| panic!("refused"));
        assert!(!got.headless);
        assert_eq!((got.log.as_deref(), got.passes), (Some("l"), Some(3)));
        let got = flags(&["--timing", "500", "--shot", "s.png"]);
        let got = got.unwrap_or_else(|_| panic!("refused"));
        assert_eq!(
            (got.timing, got.shot.as_deref()),
            (Some(500), Some("s.png"))
        );
        assert!(flags(&[]).is_ok());
        for bad in [
            &["--clock", "1"][..],
            &["--replay", "log"],
            &["--png-every", "1"],
            &["--out", "d"],
            &["--timing", "0"],
            &[
                "--headless",
                "--passes",
                "1",
                "--clock",
                "1",
                "--timing",
                "5",
            ],
            &[
                "--headless",
                "--passes",
                "1",
                "--clock",
                "1",
                "--shot",
                "s.png",
            ],
            &["--headless", "--passes", "1", "--clock", "1", "--log", "l"],
            &["--passes", "0"],
            &["--headless", "--passes", "0", "--clock", "1"],
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

    #[test]
    fn a_log_is_read_back_as_passes_with_their_clock_and_commands() {
        let log = "size 64x48\n\
                   pass 0 t=0 draw 0123456789abcdef view=1,2 rotation=0 zoom=0\n\
                   pass 0 t=0 step 200 2001-01-01 00:24:00.000\n\
                   pass 3 t=67 scroll -1 0\n\
                   pass 3 t=67 pointer none\n\
                   pass 3 t=67 interface 66\n\
                   pass 3 t=67 music fade 33 sum -967 gain 0.999\n\
                   pass 4 t=70 quit\n";
        assert_eq!(
            replay(log).unwrap(),
            (
                (64, 48),
                vec![
                    (0, 0, vec![]),
                    (3, 67, vec![Command::Scroll(-1, 0), Command::Pointer(None)]),
                    (4, 70, vec![Command::Quit]),
                ]
            )
        );
        for bad in [
            "",
            "pass 0 t=0 draw -",
            "size 64\n",
            "size 64x48\npass 0 draw",
            "size 64x48\npass x t=0 pause",
            "size 64x48\npass 0 t=0 fly",
        ] {
            assert!(replay(bad).is_err(), "{bad:?}");
        }
    }
}
