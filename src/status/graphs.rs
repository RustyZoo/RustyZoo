//! The Graphs tab of the zoo status window (zoo status 21.3): the points of the four graphs, one
//! for each of the last twelve months, and the line and bar switches.

use crate::money::{self, Money};
use crate::text::format::{Strings, graph_month};
use crate::text::to_cp1252;
use crate::ui::graph::{Point, Style};

/// The four graphs and the value each shows for a month.
pub(crate) const GRAPHS: [i32; 4] = [4150, 13978, 13967, 13965];

/// The line and bar buttons of each graph, in the order of [`GRAPHS`].
pub(crate) const SWITCHES: [[i32; 2]; 4] =
    [[4151, 4152], [13991, 13992], [13994, 13995], [13997, 13998]];

/// The style a switch button gives its graph, with the graph.
pub(crate) fn switched(button: i32) -> Option<(i32, Style)> {
    let at = SWITCHES.iter().position(|s| s.contains(&button))?;
    let style = match SWITCHES[at][0] == button {
        true => Style::Line,
        false => Style::Bar,
    };
    Some((GRAPHS[at], style))
}

/// The value of graph `graph` for the slot `m` (0 to 11) of the monthly table, each category cut
/// toward zero.
fn value(accounts: &Money, graph: usize, m: usize) -> i32 {
    let cut = |c: usize| accounts.monthly[c][m] as i32;
    match graph {
        0 => cut(money::RATING),
        1 => cut(money::DONATIONS).saturating_add(cut(money::ENDOWMENT)),
        2 => cut(money::NET),
        _ => cut(money::GUESTS),
    }
}

/// The points of graph `graph` (0 to 3, the order of [`GRAPHS`]): for each of twelve months ending
/// with the current one, a point at x 0 to 11 with the month's name, but none before the first
/// year.
pub(crate) fn points(accounts: &Money, graph: usize, words: &impl Strings) -> Vec<Point> {
    let current = accounts.month.clamp(1, 12) as i64;
    let (mut month, mut year) = (current - 11, i64::from(accounts.year));
    if month < 1 {
        (year, month) = (year - 1, current + 1);
    }
    let mut out = Vec::new();
    for x in 0..12 {
        if year >= 0 {
            out.push(Point {
                x,
                y: value(accounts, graph, month as usize - 1),
                label: to_cp1252(&graph_month(words, month as u32)),
            });
        }
        month += 1;
        if month > 12 {
            (month, year) = (1, year + 1);
        }
    }
    out
}

#[cfg(test)]
mod tests;
