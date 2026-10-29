//! The research panel (research 8): the Status tab with the current program of the first two
//! branches, and the Research and Conservation tabs with the category ticks, the funding buttons
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

/// The layout, the main strip's button, the tab buttons (Status, Research, Conservation).
pub const LAYOUT: i32 = 15;
pub const BUTTON: i32 = 1019;
pub const TABS: [i32; 3] = [4008, 4009, 4010];
/// The category list and the funding buttons.
pub const LIST: i32 = 4022;
pub const LESS: i32 = 4003;
pub const MORE: i32 = 4004;
/// The interface clock time between two refreshes (ms).
pub const PERIOD: u64 = 1000;
const TITLE: i32 = 4001;
/// The status page of one branch: program name, picture, bar, bar background.
const SIDES: [[i32; 4]; 2] = [[4016, 4019, 4018, 4017], [4012, 4020, 4014, 4013]];
/// The category page: tab title, funding text, category name, program name, picture, bar, bar
/// background.
const TAB_TITLE: i32 = 4021;
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

/// The branch the tab shows: 0 for Research, 1 for Conservation, none for Status.
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
    ) -> Option<String> {
        let branches = tree.map_or(&[][..], |t| &t.branches);
        let tab = TABS.iter().position(|&t| ui.is_down(t)).unwrap_or(0);
        let mut told = format!("research tab {tab}");
        let title = match tab {
            0 => ui.string(TITLE),
            n => branches
                .get(n - 1)
                .map_or(String::new(), |b| ui.string(b.name)),
        };
        ui.set_text(TITLE, &to_cp1252(&title));
        for (side, ids) in SIDES.iter().enumerate() {
            let tip = status(ui, vfs, branches.get(side), ids);
            told += &format!(" side {side} {tip}");
        }
        let branch = tab.checked_sub(1).zip(tree);
        let shown = branch.and_then(|(b, t)| Some((t.listed(b), t.branches.get(b)?)));
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
    let name = match branch {
        Some(_) => program_name(ui, branch),
        None => String::new(),
    };
    ui.set_text(ids[0], &to_cp1252(&name));
    set_picture(ui, vfs, branch, ids[1]);
    let tip = time_left(ui, branch, [ids[2], ids[3]]);
    format!("{name:?} {tip:?}")
}

/// The page of the Research or Conservation tab for `shown`, the categories listed and the
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
    ui.set_hidden(LIST, rows.is_empty());
    ui.refill_rows(LIST, rows);
    let title = branch.map_or(String::new(), |b| ui.string(b.name));
    ui.set_text(TAB_TITLE, &to_cp1252(&title));
    let words = Words::new(|id| ui.string(id as i32));
    let funding = branch
        .and_then(Branch::funding_text)
        .map_or(String::new(), |(name, cost)| {
            let money = format::money(&words, cost, true);
            format::expand(&words, &ui.string(name), &[Arg::Text(&money)])
        });
    ui.set_text(FUNDING, &to_cp1252(&funding));
    ui.set_enabled(LESS, branch.is_some_and(|b| b.decrease() != b.level));
    ui.set_enabled(MORE, branch.is_some_and(|b| b.increase() != b.level));
    let category = match branch.and_then(Branch::program) {
        Some((c, _)) => ui.string(c.name),
        None => ui.string(NO_CATEGORY),
    };
    let program = program_name(ui, branch);
    let (category, program) = match branch {
        Some(_) => (category, program),
        None => (String::new(), String::new()),
    };
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
