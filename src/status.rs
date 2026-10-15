//! The Finances tab of the zoo status window (exhibits and money 6.3): the income and expenses
//! table of the current month and the three before it, written from the accounts into the texts
//! of the layout.

use crate::money::Money;
use crate::text::format::{Strings, graph_month, integer, money, whole_units};
use crate::text::to_cp1252;
use crate::ui::interface::Interface;
use std::collections::HashMap;

/// The Finances layout, and the button the Z key presses to open the window.
pub const FINANCES: i32 = 110;
pub const STATUS_BUTTON: i32 = 1014;

/// The first string of the currency symbol and the month names (text 8.3 and 8.5).
const WORDS: u32 = 22100;
/// The first cell of the current month's column; a column is 15 ids further.
const CELLS: i32 = 3930;
/// The header of the current column; the older ones follow.
const HEADER: i32 = 3920;
const TOTAL_CASH: i32 = 3926;
const ZOO_VALUE: i32 = 3928;
const NET_INCOME: i32 = 3991;
/// A cell that gives its colour to the coloured ones: the admissions income cell for a sum of 0
/// or more, the construction cell for a negative one (6.3).
const INCOME_CELL: i32 = 3931;
const EXPENSE_CELL: i32 = 3932;

const ZOO_VALUE_CATEGORY: usize = 15;
const NET: usize = crate::money::NET;

/// The thirteen rows: the categories added up and the sign of the sum. The first is a number of
/// guests, not money; the TOTALS row is the net category.
const ROWS: [(&[usize], f64); 13] = [
    (&[4], 1.0),
    (&[5], 1.0),
    (&[3], -1.0),
    (&[2], -1.0),
    (&[0, 1, 28], -1.0),
    (&[19], -1.0),
    (&[6, 7], 1.0),
    (&[11, 12, 13], -1.0),
    (&[8, 20], 1.0),
    (&[9, 10], 1.0),
    (&[29], -1.0),
    (&[NET], 1.0),
    (&[30], 1.0),
];
const ADMISSIONS: usize = 0;
const TOTALS: usize = 11;

/// The strings 22100 to 22112: the symbol of the currency and the names of the months.
pub struct Words(Vec<String>);

impl Words {
    pub fn new(string: impl Fn(u32) -> String) -> Words {
        Words((WORDS..=WORDS + 12).map(string).collect())
    }
}

impl Strings for Words {
    fn string(&self, id: u32) -> &str {
        id.checked_sub(WORDS)
            .and_then(|i| self.0.get(i as usize))
            .map_or("", String::as_str)
    }
}

/// A text of the table, with the colour the code gives it where the layout's own does not
/// serve: the TOTALS cells and the three amounts below the table take one by their sign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub id: i32,
    pub text: String,
    pub colour: Option<Colour>,
}

/// Which cell of the layout gives its colour: the admissions income cell of `column` for a sum
/// of 0 or more, its construction cell for a negative one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour {
    pub column: i32,
    pub negative: bool,
}

/// Month `back` months before the current one, as month 1 to 12 and a year that is below 0 when
/// it falls before the first year of the accounts.
fn month_before(money: &Money, back: i32) -> (u32, i64) {
    let month = money.month.clamp(1, 12) as i32 - back;
    match month < 1 {
        true => ((month + 12) as u32, i64::from(money.year) - 1),
        false => (month as u32, i64::from(money.year)),
    }
}

/// Every text of the table for these accounts: the four headers and thirteen rows of the four
/// columns (blank for a month before the accounts began), then Total Cash, Zoo Value and Net
/// Income.
pub fn cells(accounts: &Money, words: &impl Strings) -> Vec<Cell> {
    let mut cells = Vec::new();
    let write = |units: i64| money(words, units, true);
    for column in 0..4 {
        // Column 0 is the current month; 1 to 3 are the older ones, the oldest first.
        let back = if column == 0 { 0 } else { 4 - column };
        let (month, year) = month_before(accounts, back);
        let shown = year >= 0;
        let text = |text: String| if shown { text } else { String::new() };
        cells.push(Cell {
            id: HEADER + column,
            text: text(graph_month(words, month)),
            colour: None,
        });
        for (r, (categories, sign)) in ROWS.iter().enumerate() {
            let sum: f64 = categories
                .iter()
                .map(|&c| f64::from(accounts.monthly[c][month as usize - 1]))
                .sum();
            let units = whole_units(sum * sign);
            cells.push(Cell {
                id: CELLS + 15 * column + r as i32,
                text: text(match r {
                    ADMISSIONS => integer(units, true),
                    _ => write(units),
                }),
                colour: (r == TOTALS && shown).then_some(Colour {
                    column,
                    negative: units < 0,
                }),
            });
        }
    }
    let month = accounts.month.clamp(1, 12) as usize - 1;
    for (id, units) in [
        (TOTAL_CASH, whole_units(f64::from(accounts.cash))),
        (
            ZOO_VALUE,
            whole_units(f64::from(accounts.monthly[ZOO_VALUE_CATEGORY][month])),
        ),
        (NET_INCOME, whole_units(f64::from(accounts.total[NET]))),
    ] {
        cells.push(Cell {
            id,
            text: write(units),
            colour: Some(Colour {
                column: 0,
                negative: units < 0,
            }),
        });
    }
    cells
}

/// What the table holds in the interface, so that only a change is written.
#[derive(Default)]
pub struct Finances {
    written: HashMap<i32, (String, [u8; 3])>,
}

impl Finances {
    /// Writes the table into the layout while it is shown, the cells that changed: the log
    /// lines `cell ID RRGGBB TEXT`, one for each. The other tabs stay as they are.
    pub fn refresh(&mut self, accounts: &Money, ui: &mut Interface) -> Vec<String> {
        let mut lines = Vec::new();
        if !ui.shown(FINANCES) {
            return lines;
        }
        let words = Words::new(|id| ui.string(id as i32));
        let colour_of = |ui: &Interface, id: i32| {
            let i = ui.tree.find(id)?;
            Some(ui.tree.elements[i].look.fore_color)
        };
        for cell in cells(accounts, &words) {
            let colour = match cell.colour {
                Some(c) => {
                    let own = if c.negative {
                        EXPENSE_CELL
                    } else {
                        INCOME_CELL
                    };
                    colour_of(ui, own + 15 * c.column)
                }
                None => colour_of(ui, cell.id),
            };
            let Some(colour) = colour else { continue };
            let now = (cell.text, colour);
            if self.written.get(&cell.id) == Some(&now) {
                continue;
            }
            if ui.set_text(cell.id, &to_cp1252(&now.0)) {
                if cell.colour.is_some() {
                    ui.set_fore_color(cell.id, colour);
                }
                lines.push(format!(
                    "cell {} {:02x}{:02x}{:02x} {}",
                    cell.id, colour[0], colour[1], colour[2], now.0
                ));
                self.written.insert(cell.id, now);
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests;
