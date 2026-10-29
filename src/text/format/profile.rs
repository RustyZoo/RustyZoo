//! How numbers and money are written: the marks and the money forms of the user's regional
//! settings, as the original's number and currency formatters have them.

use super::locale;

/// The no-break space: what the original writes between the groups of a French number.
const NBSP: char = '\u{a0}';

/// The decimal mark, the group separator and the group size of numbers or of money.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Marks {
    pub decimal: String,
    pub group: String,
    /// Digits per group; 0 for no grouping at all.
    pub size: usize,
}

impl Marks {
    /// A white space as separator becomes a no-break space, so a number is never split over two
    /// lines.
    pub fn new(decimal: &str, group: &str, size: usize) -> Marks {
        let group = group.replace(char::is_whitespace, &NBSP.to_string());
        Marks {
            decimal: decimal.into(),
            group,
            size,
        }
    }

    /// `digits` (decimal digits only) with the separator between the groups.
    pub fn group(&self, digits: &str) -> String {
        let mut out = String::with_capacity(digits.len() * 2);
        for (i, c) in digits.chars().enumerate() {
            if self.size > 0 && i > 0 && (digits.len() - i).is_multiple_of(self.size) {
                out.push_str(&self.group);
            }
            out.push(c);
        }
        out
    }
}

/// The marks and the money forms of one locale. A form is a pattern: `S` is the symbol, `N` the
/// number, any other character is written as it is.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Profile {
    pub number: Marks,
    pub money: Marks,
    positive: String,
    negative: String,
}

/// The positive money forms of Windows (`LOCALE_ICURRENCY`, 0 to 3) as patterns.
const POSITIVE: [&str; 4] = ["SN", "NS", "S N", "N S"];

/// The negative money forms of Windows (`LOCALE_INEGCURR`, 0 to 15) as patterns.
const NEGATIVE: [&str; 16] = [
    "(SN)", "-SN", "S-N", "SN-", "(NS)", "-NS", "N-S", "NS-", "-N S", "-S N", "N S-", "S N-",
    "S -N", "N- S", "(S N)", "(N S)",
];

impl Profile {
    /// The US form: "1,234,567", "$74,878", "($1,234)".
    pub fn us() -> Profile {
        let marks = Marks::new(".", ",", 3);
        Profile {
            number: marks.clone(),
            money: marks,
            positive: POSITIVE[0].into(),
            negative: NEGATIVE[0].into(),
        }
    }

    /// The forms of the user's regional settings, or the US ones when `english` (the value of
    /// the environment variable of the English switch) is `1` or the system gives none.
    pub fn of_user(english: Option<&std::ffi::OsStr>) -> Profile {
        let system = english.is_none_or(|v| v != "1");
        system
            .then(|| locale::profile(""))
            .flatten()
            .map(Profile::in_code_page)
            .unwrap_or_else(Profile::us)
    }

    /// The marks with a character that Windows-1252 lacks (an Arabic decimal mark, say) replaced
    /// by the US ones, which the game can write; the money forms stay.
    pub fn in_code_page(mut self) -> Profile {
        let us = Marks::new(".", ",", 3);
        for marks in [&mut self.number, &mut self.money] {
            if !marks
                .decimal
                .chars()
                .chain(marks.group.chars())
                .all(crate::text::in_cp1252)
            {
                *marks = us.clone();
            }
        }
        self
    }

    /// The forms of the Windows currency settings (the numbers of the two items).
    #[cfg(any(windows, test))]
    pub fn from_windows(number: Marks, money: Marks, positive: usize, negative: usize) -> Profile {
        let pick = |table: &[&str], i, default: &str| table.get(i).unwrap_or(&default).to_string();
        Profile {
            number,
            money,
            positive: pick(&POSITIVE, positive, POSITIVE[0]),
            negative: pick(&NEGATIVE, negative, NEGATIVE[0]),
        }
    }

    /// The forms of the C library's `lconv`: for a positive and a negative amount, whether the
    /// symbol is first (`cs_precedes`, 0 or 1) and the space (`sep`, 0 for none); for a negative
    /// one also the sign position (0 parentheses, 1 before all, 2 after all, 3 before the
    /// symbol, 4 after it). A value out of range (unspecified: the C locale) keeps the US form.
    #[cfg(any(unix, test))]
    pub fn from_c(
        number: Marks,
        money: Marks,
        positive: (i32, i32),
        negative: (i32, i32, i32),
    ) -> Profile {
        let us = Profile::us();
        let core = |first: i32, sep: i32| {
            let gap = if sep > 0 { " " } else { "" };
            match first {
                1 => format!("S{gap}N"),
                _ => format!("N{gap}S"),
            }
        };
        let positive = match positive {
            (first @ (0 | 1), sep) => core(first, sep),
            _ => us.positive,
        };
        let negative = match negative {
            (first @ (0 | 1), sep, posn @ 0..=4) => {
                let (before, after) = match posn {
                    3 => ("-", ""),
                    4 => ("", "-"),
                    _ => ("", ""),
                };
                let body = core(first, sep).replace('S', &format!("{before}S{after}"));
                match posn {
                    0 => format!("({body})"),
                    1 => format!("-{body}"),
                    2 => format!("{body}-"),
                    _ => body,
                }
            }
            _ => us.negative,
        };
        Profile {
            number,
            money,
            positive,
            negative,
        }
    }

    /// `body` (the number) with the symbol, in the positive or the negative form.
    pub fn amount(&self, symbol: &str, body: &str, negative: bool) -> String {
        let pattern = if negative {
            &self.negative
        } else {
            &self.positive
        };
        let mut out = String::new();
        for c in pattern.chars() {
            match c {
                'S' => out.push_str(symbol),
                'N' => out.push_str(body),
                ' ' => out.push(NBSP),
                c => out.push(c),
            }
        }
        out
    }
}
