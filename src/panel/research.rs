//! The research panel (research 8): the Status tab with the current program of the first two
//! branches, and the two branch tabs with the category ticks, the funding buttons
//! and the current program of one branch. It is refreshed every second of the interface clock
//! while shown and at once by what the player does; [`Panel::refresh`] writes the layouts from the
//! tree and returns the `panel` log line when what they show changed.

use crate::panel::exhibit::Clock;
use crate::panel::staff::CHECKS;
use crate::research::{Branch, Tree};
use crate::status::Words;
use crate::text::format::{self, Arg};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use crate::ui::widgets::Row;
use crate::vfs::Vfs;

/// The layout, the main strip's button, the tab buttons (status and the two branches).
pub const LAYOUT: i32 = 15;
pub const BUTTON: i32 = 1019;
pub const TABS: [i32; 3] = [4008, 4009, 4010];
/// The expansion selector button, which the panel writes with the selected row.
const SELECTOR: i32 = 4021;
/// The category list and the funding buttons.
pub const LIST: i32 = 4022;
pub const LESS: i32 = 4003;
pub const MORE: i32 = 4004;
/// The interface clock time between two refreshes (ms).
pub const PERIOD: u64 = 1000;
const TITLE: i32 = 4001;
/// The status page of one branch: program name, picture, bar, bar background.
const SIDES: [[i32; 4]; 2] = [[4016, 4019, 4018, 4017], [4012, 4020, 4014, 4013]];
/// The category page: funding text, category name, program name, picture, bar, bar background,
/// and the text shown when the list has no row.
const NO_ITEM: i32 = 4030;
const FUNDING: i32 = 4006;
const CATEGORY: i32 = 4024;
const PROGRAM: i32 = 4025;
const PICTURE: i32 = 4029;
const BAR: [i32; 2] = [4027, 4026];
/// The strings: the tooltips of a day and of days left, not funded, no program, no category.
const DAY: i32 = 4026;
const DAYS: i32 = 4028;
const NOT_FUNDED: i32 = 23030;
const NO_PROGRAM: i32 = 23031;
const NO_CATEGORY: i32 = 23032;

/// The branch the tab shows: 0 for the first branch, 1 for the second, none for Status.
pub fn branch_of_tab(ui: &Interface) -> Option<usize> {
    [TABS[1], TABS[2]].iter().position(|&t| ui.is_down(t))
}

/// The refresh clock and what was last told.
#[derive(Debug, Default)]
pub struct Panel {
    pub clock: Clock,
    /// The layout was shown at the last look.
    pub shown: bool,
    told: String,
}

impl Panel {
    /// Writes the panel from `tree` (a game with none shows no program) and returns the log line
    /// when something it shows changed.
    pub fn refresh(
        &mut self,
        ui: &mut Interface,
        tree: Option<&Tree>,
        vfs: &Vfs,
        selector: (i32, &str),
    ) -> Option<String> {
        let branches = tree.map_or(&[][..], |t| &t.branches);
        let tab = TABS.iter().position(|&t| ui.is_down(t)).unwrap_or(0);
        let mut told = format!("research tab {tab}");
        // A missing branch leaves the title as it was (8.1).
        let title = match tab {
            0 => Some(ui.string(TITLE)),
            n => branches.get(n - 1).map(|b| ui.string(b.name)),
        };
        if let Some(title) = title {
            ui.set_text(TITLE, &to_cp1252(&title));
        }
        for (side, ids) in SIDES.iter().enumerate() {
            let tip = status(ui, vfs, branches.get(side), ids);
            told += &format!(" side {side} {tip}");
        }
        let branch = tab.checked_sub(1).zip(tree);
        let shown = branch.and_then(|(b, t)| Some((t.listed(b, selector.0), t.branches.get(b)?)));
        // The base trial's page has a plain text there, not the selector button (8.3).
        let button = ui.tree.find(SELECTOR).map(|i| &ui.tree.elements[i]);
        if button.is_some_and(|e| e.action == 1) {
            ui.set_text(SELECTOR, &to_cp1252(selector.1));
        }
        told += &category_page(ui, vfs, shown);
        match told == self.told {
            true => None,
            false => {
                self.told.clone_from(&told);
                Some(told)
            }
        }
    }
}

/// The name of the program, or the string `none`, in the font of the page.
fn program_name(ui: &Interface, branch: Option<&Branch>) -> String {
    match branch.and_then(Branch::program) {
        Some((_, p)) => ui.string(p.name),
        None => ui.string(NO_PROGRAM),
    }
}

/// The picture of the current program, or the branch's own while it has none, with the tooltip of
/// the program's help string.
fn set_picture(ui: &mut Interface, vfs: &Vfs, branch: Option<&Branch>, id: i32) {
    let program = branch.and_then(Branch::program);
    let name = match program {
        Some((_, p)) if !p.picture.is_empty() => &p.picture,
        _ => branch.map_or("", |b| &b.no_icon),
    };
    let picture = (!name.is_empty())
        .then(|| ui.tree.picture(vfs, name))
        .flatten();
    ui.set_picture(id, picture);
    let help = program.map_or(0, |(_, p)| p.help);
    if let Some(i) = ui.tree.find(id) {
        ui.tree.elements[i].help_id = (help > 0).then_some(help).into_iter().collect();
    }
}

/// The bar and its "time left" tooltip (8.2); returns the tooltip.
fn time_left(ui: &mut Interface, branch: Option<&Branch>, bar: [i32; 2]) -> String {
    let days = branch.and_then(Branch::days_remaining).filter(|&d| d > 0.0);
    let percent = branch.and_then(Branch::percent_remaining);
    let (value, tip) = match (days, percent) {
        (Some(days), Some(percent)) => {
            let n = days.ceil() as i64;
            let template = ui.template(if days <= 1.0 { DAY } else { DAYS });
            let n = format::integer(n, false);
            (
                100i32.saturating_sub(percent),
                template.replacen("%s", &n, 1),
            )
        }
        _ => (0, ui.string(NOT_FUNDED)),
    };
    ui.set_status(bar[0], value);
    for id in bar {
        ui.set_tip(id, tip.clone());
    }
    tip
}

/// One branch of the Status tab; returns what it tells.
fn status(ui: &mut Interface, vfs: &Vfs, branch: Option<&Branch>, ids: &[i32; 4]) -> String {
    let name = program_name(ui, branch);
    ui.set_text(ids[0], &to_cp1252(&name));
    set_picture(ui, vfs, branch, ids[1]);
    let tip = time_left(ui, branch, [ids[2], ids[3]]);
    format!("{name:?} {tip:?}")
}

/// The page of a branch tab for `shown`, the categories listed and the
/// branch (none on the Status tab or for a missing branch); returns what it tells.
fn category_page(ui: &mut Interface, vfs: &Vfs, shown: Option<(Vec<usize>, &Branch)>) -> String {
    let branch = shown.as_ref().map(|(_, b)| *b);
    let rows: Vec<Row> = shown
        .iter()
        .flat_map(|(listed, b)| {
            listed
                .iter()
                .map(|&c| &b.categories[c])
                .map(|c| Row {
                    text: to_cp1252(&ui.string(c.name)),
                    help: c.help,
                    icon: ui.tree.picture(vfs, CHECKS[usize::from(c.tick)]),
                    ..Row::default()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let tick = |t: bool| if t { '1' } else { '0' };
    let ticks: String = (shown.iter())
        .flat_map(|(l, b)| l.iter().map(|&c| tick(b.categories[c].tick)))
        .collect();
    // The list is never hidden: the no-item text shows when it has no row (8.3).
    ui.set_hidden(NO_ITEM, !rows.is_empty());
    ui.refill_rows(LIST, rows);
    let words = Words::new(|id| ui.string(id as i32));
    // A missing branch leaves the funding text and buttons as they were (8.1).
    let mut funding = String::new();
    if let Some(b) = branch {
        if let Some((name, cost)) = b.funding_text() {
            let money = format::money(&words, cost, true);
            funding = format::expand(&words, &ui.string(name), &[Arg::Text(&money)]);
        }
        ui.set_text(FUNDING, &to_cp1252(&funding));
        ui.set_enabled(LESS, b.decrease() != b.level);
        ui.set_enabled(MORE, b.increase() != b.level);
    }
    let category = match branch.and_then(Branch::program) {
        Some((c, _)) => ui.string(c.name),
        None => ui.string(NO_CATEGORY),
    };
    let program = program_name(ui, branch);
    ui.set_text(CATEGORY, &to_cp1252(&category));
    ui.set_text(PROGRAM, &to_cp1252(&program));
    set_picture(ui, vfs, branch, PICTURE);
    let tip = time_left(ui, branch, BAR);
    format!(
        " ticks {ticks} funding {funding:?} category {category:?} program {program:?} bar {tip:?}"
    )
}

#[cfg(test)]
mod tests;
