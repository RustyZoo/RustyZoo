//! The commands that show the interface tree.

use std::ffi::OsString;
use std::fs::File;

use rustyzoo::Framebuffer;
use rustyzoo::sprite::write_png;
use rustyzoo::ui::draw::Painter;
use rustyzoo::ui::interface::{Buttons, Interface, Settings};
use rustyzoo::ui::layout::{DIRTY, HIGHLIGHTED, SELECTED};
use rustyzoo::ui::widgets::Row;

use rustyzoo::config::Config;
use rustyzoo::strings::StringTables;
use rustyzoo::text::{Fonts, to_cp1252};
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

/// The tree of `ui/gamescrn.lyt` for the expansions of `mask`, resolved for a window; its
/// warnings are printed.
fn resolve_tree(vfs: &Vfs, tables: &StringTables, size: (i32, i32), mask: i32) -> Tree {
    let mut tree = Tree::load(vfs, "ui/gamescrn.lyt", mask);
    let mut fonts = Fonts::default();
    tree.resolve(size.0, size.1, |font| {
        // Strings that do not exist are empty texts: the default font, 8 points.
        let (face, points) = font.map_or(("Arial", "8"), |(face, points)| {
            let string = |n: i32| u32::try_from(n).map_or("", |n| tables.string(n));
            (string(face), string(points))
        });
        fonts.get(face, points).line_height()
    });
    for warning in &tree.warnings {
        eprintln!("rustyzoo: warning: {warning}");
    }
    tree
}

/// The tree resolved for a window, with the strings of the install.
fn resolved_tree(
    data_dir: Option<OsString>,
    size: (i32, i32),
    mask: Option<&str>,
) -> Result<(Tree, StringTables), CliError> {
    let tables = StringTables::open(&game_folder(data_dir.clone())?)?;
    let vfs = open_game(data_dir)?;
    let mask = expansions(&vfs, mask)?;
    Ok((resolve_tree(&vfs, &tables, size, mask), tables))
}

/// What a run puts into the interface at the start, for a script to work on: list boxes filled
/// with that many rows (`Row 1`, `Row 2`, ...), texts given to elements and a question opened.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Fill {
    pub rows: Vec<(i32, usize)>,
    pub texts: Vec<(i32, String)>,
    /// A question to open: its layout and its buttons.
    pub ask: Option<(i32, Buttons)>,
}

/// The interface of a game in a window of `size` pixels: the tree, with the layouts of `show`
/// shown and `fill` put in. A tree that cannot be loaded is a warning and no interface, unless
/// layouts or something to fill were asked for.
pub fn load_interface(
    vfs: &Vfs,
    size: (u32, u32),
    show: &[i32],
    fill: &Fill,
) -> Result<Option<Interface>, CliError> {
    let size = (
        i32::try_from(size.0).unwrap_or(i32::MAX),
        i32::try_from(size.1).unwrap_or(i32::MAX),
    );
    let made = StringTables::open(vfs.root())
        .map_err(|e| e.to_string())
        .and_then(|tables| {
            let tree = resolve_tree(vfs, &tables, size, installed_expansions(vfs));
            match tree.failure.clone() {
                Some(failure) => Err(format!("load stopped at {failure}")),
                None => Ok(Interface::new(tree, Box::new(tables))),
            }
        });
    match made {
        Ok(mut ui) => {
            match Config::parse(vfs.ini()) {
                Ok(ini) => ui.set_settings(Settings::from_config(&ini)),
                Err(e) => eprintln!("rustyzoo: warning: ini file: {e}"),
            }
            for &id in show {
                if !ui.show(id) {
                    return Err(format!("no layout with id {id}").into());
                }
            }
            for &(id, count) in &fill.rows {
                let list = (1..=count).map(|n| Row {
                    text: format!("Row {n}").into_bytes(),
                    ..Row::default()
                });
                if !ui.set_rows(id, list.collect()) {
                    return Err(format!("no list box with id {id}").into());
                }
            }
            if let Some((id, buttons)) = fill.ask
                && !ui.ask(id, b"Question", buttons)
            {
                return Err(format!("no question layout with id {id}").into());
            }
            for (id, text) in &fill.texts {
                if !ui.set_text(*id, &to_cp1252(text)) {
                    return Err(format!("no element with id {id}").into());
                }
            }
            Ok(Some(ui))
        }
        Err(why) if show.is_empty() && *fill == Fill::default() => {
            eprintln!("rustyzoo: warning: no interface: {why}");
            Ok(None)
        }
        Err(why) => Err(format!("no interface: {why}").into()),
    }
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
    let (tree, _) = resolved_tree(data_dir, size, mask)?;
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

/// Paints the interface into a PNG file: the root, with the layouts of `--show` shown, the
/// element of `--hover` highlighted and the one of `--press` pressed.
pub fn render_ui(data_dir: Option<OsString>, out: &str, flags: &[&str]) -> Result<(), CliError> {
    let (mut size, mut mask) = (DEFAULT_SIZE, None);
    let (mut show, mut hover, mut press) = (vec![], None, None);
    let mut rest = flags;
    let id = |value: &str| value.parse::<i32>().map_err(|_| CliError::Usage);
    while let [flag, value, tail @ ..] = rest {
        match *flag {
            "--size" => size = window_size(value)?,
            "--expansions" => mask = Some(*value),
            "--show" => show.push(id(value)?),
            "--hover" => hover = Some(id(value)?),
            "--press" => press = Some(id(value)?),
            _ => return Err(CliError::Usage),
        }
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(CliError::Usage);
    }
    let (mut tree, tables) = resolved_tree(data_dir, size, mask)?;
    if let Some(failure) = &tree.failure {
        return Err(format!("load stopped at {failure}").into());
    }
    for layout in show {
        if !tree.show(layout) {
            return Err(format!("no element with id {layout}").into());
        }
    }
    for (id, flag) in [(hover, HIGHLIGHTED), (press, SELECTED)] {
        if let Some(id) = id {
            let i = tree
                .find(id)
                .ok_or_else(|| format!("no element with id {id}"))?;
            tree.elements[i].state |= flag | DIRTY;
        }
    }
    let mut frame = Framebuffer::new(size.0 as u32, size.1 as u32);
    Painter::default().draw(&mut tree, &mut frame, &tables, &mut |_, _| {});
    let file = File::create(out).map_err(|e| format!("cannot write {out}: {e}"))?;
    write_png(&frame, file)?;
    println!("{out}: {}x{}", size.0, size.1);
    Ok(())
}
