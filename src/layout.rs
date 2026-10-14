//! The commands that show the interface tree.

use rustyzoo::ui::layout::{Tree, installed_expansions};
use rustyzoo::vfs::Vfs;

use crate::{CliError, Report};

/// The expansion mask of `--expansions N`, else the one the install gives.
fn expansions(vfs: &Vfs, flags: &[&str]) -> Result<i32, CliError> {
    match flags {
        [] => Ok(installed_expansions(vfs)),
        ["--expansions", n] => n
            .parse::<i32>()
            .ok()
            .filter(|n| *n >= 0)
            .ok_or(CliError::Usage),
        _ => Err(CliError::Usage),
    }
}

/// Loads `ui/gamescrn.lyt` and prints the counts; exit 1 if the load failed.
pub fn check_layouts(vfs: &Vfs, flags: &[&str]) -> Result<(), CliError> {
    let mask = expansions(vfs, flags)?;
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
