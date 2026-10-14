//! The commands that show the interface tree.

use std::ffi::OsString;

use rustyzoo::strings::StringTables;
use rustyzoo::text::Fonts;
use rustyzoo::ui::layout::{Tree, installed_expansions};
use rustyzoo::vfs::Vfs;

use crate::{CliError, Report, game_folder, open_game};

/// The window of `dump-layout` without `--size`.
const DEFAULT_SIZE: (i32, i32) = (1024, 768);
/// The largest side of `--size`, so that no position can overflow.
const MAX_SIDE: i32 = 16384;

/// The expansion mask of `--expansions N`, else the one the install gives.
fn expansions(vfs: &Vfs, flag: Option<&str>) -> Result<i32, CliError> {
    match flag {
        None => Ok(installed_expansions(vfs)),
        Some(n) => n
            .parse::<i32>()
            .ok()
            .filter(|n| *n >= 0)
            .ok_or(CliError::Usage),
    }
}

/// Loads `ui/gamescrn.lyt` and prints the counts; exit 1 if the load failed.
pub fn check_layouts(vfs: &Vfs, flags: &[&str]) -> Result<(), CliError> {
    let mask = match flags {
        [] => expansions(vfs, None)?,
        ["--expansions", n] => expansions(vfs, Some(n))?,
        _ => return Err(CliError::Usage),
    };
    let tree = Tree::load(vfs, "ui/gamescrn.lyt", mask);
    for warning in &tree.warnings {
        eprintln!("rustyzoo: warning: {warning}");
    }
    let mut report = Report::new();
    report.line(format_args!("expansions {mask}"));
    for (name, count) in tree.counts() {
        report.line(format_args!("{name} {count}"));
    }
    for &index in &tree.duplicates {
        report.line(format_args!(
            "duplicate id {} (element {index})",
            tree.elements[index].id
        ));
    }
    let layouts = tree.counts().get("UILayout").copied().unwrap_or(0);
    report.line(format_args!(
        "{} elements, {layouts} layouts, {} duplicate ids, {} images, {} animations, {} pictures missing",
        tree.elements.len(),
        tree.duplicates.len(),
        tree.pictures.images.len(),
        tree.pictures.animations.len(),
        tree.warnings.len(),
    ));
    report.finish()?;
    match tree.failure {
        Some(failure) => Err(format!("load stopped at {failure}").into()),
        None => Ok(()),
    }
}

/// `WxH`, each side from 1 to [`MAX_SIDE`].
fn window_size(text: &str) -> Result<(i32, i32), CliError> {
    let (w, h) = text.split_once('x').ok_or(CliError::Usage)?;
    let side = |s: &str| match s.parse::<i32>() {
        Ok(n) if (1..=MAX_SIDE).contains(&n) => Ok(n),
        _ => Err(CliError::Usage),
    };
    Ok((side(w)?, side(h)?))
}

/// Loads the tree, resolves it for a window and prints one line per element in the order they are
/// drawn: id, type, layer, state and the rectangle on the screen (x, y, width, height).
pub fn dump_layout(data_dir: Option<OsString>, flags: &[&str]) -> Result<(), CliError> {
    let (mut size, mut mask) = (DEFAULT_SIZE, None);
    let mut rest = flags;
    while let [flag, value, tail @ ..] = rest {
        match *flag {
            "--size" => size = window_size(value)?,
            "--expansions" => mask = Some(*value),
            _ => return Err(CliError::Usage),
        }
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(CliError::Usage);
    }
    let tables = StringTables::open(&game_folder(data_dir.clone())?)?;
    let vfs = open_game(data_dir)?;
    let mut tree = Tree::load(&vfs, "ui/gamescrn.lyt", expansions(&vfs, mask)?);
    let mut fonts = Fonts::default();
    tree.resolve(size.0, size.1, |font| {
        // Strings that do not exist are empty texts: the default font, 8 points.
        let (face, points) = font.map_or(("Arial", "8"), |(face, points)| {
            let string = |n: i32| u32::try_from(n).map_or("", |n| tables.string(n));
            (string(face), string(points))
        });
        fonts.get(face, points).line_height()
    });
    for warning in tree.warnings.iter() {
        eprintln!("rustyzoo: warning: {warning}");
    }
    let mut report = Report::new();
    for i in tree.draw_order() {
        let e = &tree.elements[i];
        let (x, y, w, h) = tree.rect(i);
        report.line(format_args!(
            "{}\t{}\t{}\t{}\t{x}\t{y}\t{w}\t{h}",
            e.id,
            e.kind.name(),
            e.layer,
            e.state
        ));
    }
    report.finish()?;
    match tree.failure {
        Some(failure) => Err(format!("load stopped at {failure}").into()),
        None => Ok(()),
    }
}
