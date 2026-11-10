//! The commands that draw a map.

use std::fs::File;
use std::io;

use super::{CliError, Report};
use rustyzoo::entity::Catalogue;
use rustyzoo::render::{MapRenderer, Options, Summary};
use rustyzoo::sprite::write_png;
use rustyzoo::vfs::Vfs;
use rustyzoo::zoo::Zoo;

/// The rotation and the options of `render-map`, from the words after the output file.
pub fn render_flags(mut flags: &[&str]) -> Result<(u8, Options), CliError> {
    let (mut rotation, mut options) = (0, Options::default());
    options.loaded = true;
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
            ("--grid", rest) => {
                options.grid = true;
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
pub fn render_map(
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
        Zoo::parse_in(&vfs.read(member)?, &catalogue).map_err(|e| format!("{member}: {e}"))?;
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
pub fn check_render(vfs: &Vfs, rotation: u8) -> Result<(), CliError> {
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
            .and_then(|bytes| Zoo::parse_in(&bytes, &catalogue).ok());
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

#[cfg(test)]
mod tests {
    use super::render_flags;

    #[test]
    fn render_map_flags_come_in_any_order_and_bad_ones_are_refused() {
        let ok = |flags: &[&str]| {
            render_flags(flags)
                .ok()
                .map(|(r, o)| (r, o.window, o.no_units, o.zoom))
        };
        assert_eq!(ok(&[]), Some((0, None, false, 0)));
        assert_eq!(ok(&["--zoom", "-2"]), Some((0, None, false, -2)));
        let grid = |flags: &[&str]| render_flags(flags).map(|(_, o)| o.grid).ok();
        assert_eq!(grid(&[]), Some(false));
        assert_eq!(grid(&["--grid", "--zoom", "2"]), Some(true));
        assert_eq!(grid(&["--grid", "1"]), None);
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
