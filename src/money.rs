//! Cash and the accounts of the zoo (exhibits and money 4 and 5): the cash as one number, 31
//! categories each with a ring of 12 months, a ring of 20 years and a total, charges and refunds
//! that book into a category and the net line, their undo, the month change and the status bar.
//! Pure data: nothing here knows the interface or the clock.

use crate::text::format::{Strings, money, whole_units};

/// The categories of the accounts, by number.
pub const CATEGORIES: usize = 31;
pub const ANIMAL_PURCHASE: usize = 2;
pub const CONSTRUCTION: usize = 3;
pub const ADMISSIONS: usize = 4;
/// What the guests pay at the entrance.
pub const ADMISSIONS_INCOME: usize = 5;
pub const DONATIONS: usize = 8;
pub const CONSTRUCTION_REFUND: usize = 9;
pub const ANIMAL_REFUND: usize = 10;
/// The wage categories: what hiring a staff member of the class books, and its monthly wage.
pub const KEEPER_WAGES: usize = 11;
pub const GUIDE_WAGES: usize = 12;
pub const MAINT_WAGES: usize = 13;
/// The monthly upkeep of the guest buildings.
pub const BUILDING_UPKEEP: usize = 28;
/// The statistics of the zoo status (zoo status 13).
pub const ZOO_VALUE: usize = 15;
pub const RATING: usize = 16;
pub const ANIMAL_HAPPINESS: usize = 17;
pub const GUEST_HAPPINESS: usize = 18;
pub const GUESTS: usize = 25;
/// The counts of guests created and gone, and of the endowment members (net and joined).
pub const GUESTS_CREATED: usize = 26;
pub const GUESTS_GONE: usize = 27;
pub const MEMBERS: usize = 21;
pub const MEMBERS_JOINED: usize = 22;
/// Income minus expenses.
pub const NET: usize = 14;
/// Admissions since the start, copied at each month change.
pub const ADMISSIONS_TOTAL: usize = 24;

pub const MONTHS: usize = 12;
pub const YEARS: usize = 20;

/// What the `$` key donates (input 4.8).
pub const DOLLAR_KEY: f32 = 10_000.0;

/// The string number of the currency symbol.
pub const SYMBOL: i32 = 22100;

/// The currency symbol of the interface strings, to write amounts with.
pub struct Symbol(String);

impl Symbol {
    /// The symbol from string [`SYMBOL`]; a blank one reads as `$`.
    pub fn new(string: String) -> Symbol {
        match string.trim() {
            "" => Symbol("$".into()),
            _ => Symbol(string),
        }
    }

    /// An amount in whole units with grouping: `74878` is "$74,878", `-5` is "($5)".
    pub fn text(&self, units: i64) -> String {
        money(self, units, true)
    }
}

impl Strings for Symbol {
    fn string(&self, id: u32) -> &str {
        match id == SYMBOL as u32 {
            true => &self.0,
            false => "",
        }
    }
}

/// How a booking into a category moves the net line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Income,
    Expense,
    /// A count or a statistic: the net line is not touched.
    Other,
}

fn kind(category: usize) -> Kind {
    match category {
        5..=10 | 20 | 30 => Kind::Income,
        0..=3 | 11..=13 | 19 | 28 | 29 => Kind::Expense,
        _ => Kind::Other,
    }
}

/// The colour of the cash on the status bar (text 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Green,
    Yellow,
    Red,
}

impl Tone {
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Tone::Green => [83, 219, 83],
            Tone::Yellow => [255, 212, 60],
            Tone::Red => [186, 69, 33],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tone::Green => "green",
            Tone::Yellow => "yellow",
            Tone::Red => "red",
        }
    }
}

/// What a start does to the cash: sets it, then adds to it (main menu 8.3). A saved game does
/// neither; a freeform start sets the amount of its screen; a scenario has its `[start] setcash`
/// and `addcash`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StartCash {
    pub set: Option<f32>,
    pub add: Option<f32>,
}

/// The cash and the three tables of the accounts.
#[derive(Debug, Clone, PartialEq)]
pub struct Money {
    pub cash: f32,
    /// The column of `monthly` being filled, 1 to 12.
    pub month: u32,
    /// The column of `yearly` being filled, 0 to 19.
    pub year: u32,
    pub monthly: [[f32; MONTHS]; CATEGORIES],
    pub yearly: [[f32; YEARS]; CATEGORIES],
    pub total: [f32; CATEGORIES],
}

impl Default for Money {
    fn default() -> Money {
        Money {
            cash: 0.0,
            month: 1,
            year: 0,
            monthly: Default::default(),
            yearly: Default::default(),
            total: Default::default(),
        }
    }
}

impl Money {
    /// Sets the cash. Not booked. A value that is no number leaves the cash as it was.
    pub fn set_cash(&mut self, amount: f32) {
        if amount.is_finite() {
            self.cash = amount;
        }
    }

    /// Adds to the cash (or takes from it, if negative). Not booked.
    pub fn add_cash(&mut self, amount: f32) {
        if amount.is_finite() {
            self.cash += amount;
        }
    }

    /// A new game starts: the accounts are cleared if the start asks for it (5.4), then the cash
    /// is set and added to. None of it is booked.
    pub fn start(&mut self, cash: StartCash, reset: bool) {
        if reset {
            self.reset();
        }
        if let Some(set) = cash.set {
            self.set_cash(set);
        }
        if let Some(add) = cash.add {
            self.add_cash(add);
        }
    }

    /// Whether an action of cost `cost` is allowed: free (or paying), or covered by the cash.
    pub fn can_pay(&self, cost: f32) -> bool {
        cost <= 0.0 || cost <= self.cash
    }

    /// Takes `cost` from the cash and books it in `category`, if the action is allowed;
    /// otherwise nothing changes.
    pub fn charge(&mut self, category: usize, cost: f32) -> bool {
        if category >= CATEGORIES || !cost.is_finite() || !self.can_pay(cost) {
            return false;
        }
        self.spend(category, cost);
        true
    }

    /// Takes `cost` from the cash and books it in `category` without the can-pay check: wages,
    /// upkeep, animal food and medicine are taken even when the cash does not cover them and
    /// can push it below 0 (4.3).
    pub fn spend(&mut self, category: usize, cost: f32) {
        if category < CATEGORIES && cost.is_finite() {
            self.cash -= cost;
            self.book(category, cost);
        }
    }

    /// Adds `amount` to the cash and books it in `category` (a refund, a donation, any income).
    pub fn refund(&mut self, category: usize, amount: f32) {
        if category < CATEGORIES && amount.is_finite() {
            self.cash += amount;
            self.book(category, amount);
        }
    }

    /// Undoes a charge: the cash comes back and the category goes down by `cost`; nothing is
    /// booked as a refund.
    pub fn undo_charge(&mut self, category: usize, cost: f32) {
        if category < CATEGORIES && cost.is_finite() {
            self.cash += cost;
            self.book(category, -cost);
        }
    }

    /// Undoes a refund: the cash goes down again and the refund category goes down by `amount`.
    pub fn undo_refund(&mut self, category: usize, amount: f32) {
        if category < CATEGORIES && amount.is_finite() {
            self.cash -= amount;
            self.book(category, -amount);
        }
    }

    /// Counts one in a count category (admissions, guests created or gone, members), in the
    /// current month, the current year and the total.
    pub fn count(&mut self, category: usize) {
        if category < CATEGORIES {
            self.book(category, 1.0);
        }
    }

    /// Sets a statistic of the current month, the current year and the total.
    pub fn set_statistic(&mut self, category: usize, value: f32) {
        if category < CATEGORIES && value.is_finite() {
            let (m, y) = (self.month_column(), self.year_column());
            self.monthly[category][m] = value;
            self.yearly[category][y] = value;
            self.total[category] = value;
        }
    }

    /// Adds `amount` to the category in the three tables, and to or from the net line.
    fn book(&mut self, category: usize, amount: f32) {
        let (m, y) = (self.month_column(), self.year_column());
        let mut add = |c: usize, a: f32| {
            self.monthly[c][m] += a;
            self.yearly[c][y] += a;
            self.total[c] += a;
        };
        add(category, amount);
        match kind(category) {
            Kind::Income => add(NET, amount),
            Kind::Expense => add(NET, -amount),
            Kind::Other => {}
        }
    }

    fn month_column(&self) -> usize {
        (self.month as usize).clamp(1, MONTHS) - 1
    }

    fn year_column(&self) -> usize {
        (self.year as usize).min(YEARS - 1)
    }

    /// The calendar enters a new month: the column of the new month is cleared (it held the
    /// same month of the last year), after the year's column on January (5.3).
    pub fn month_change(&mut self) {
        let (month, year) = (self.month_column() + 1, self.year_column());
        (self.month, self.year) = (month as u32 + 1, year as u32);
        if self.month > MONTHS as u32 {
            self.month = 1;
            self.year = (self.year + 1) % YEARS as u32;
            let y = self.year_column();
            for row in &mut self.yearly {
                row[y] = 0.0;
            }
        }
        let m = self.month_column();
        for row in &mut self.monthly {
            row[m] = 0.0;
        }
        self.monthly[ADMISSIONS_TOTAL][m] = self.total[ADMISSIONS];
    }

    /// Clears the three tables and goes back to January of the first year (5.4). The cash stays.
    pub fn reset(&mut self) {
        let cash = self.cash;
        *self = Money {
            cash,
            ..Money::default()
        };
    }

    /// The cash as the status bar shows it: whole units and the colour of its sign (text 6.1).
    pub fn bar(&self) -> (i64, Tone) {
        let units = whole_units(f64::from(self.cash));
        let tone = match units {
            ..0 => Tone::Red,
            0 => Tone::Yellow,
            _ => Tone::Green,
        };
        (units, tone)
    }
}

#[cfg(test)]
mod tests;
