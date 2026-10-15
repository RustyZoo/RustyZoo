//! Templates, numbers, money and dates as the game writes them, with fixed US English
//! conventions: "," groups thousands, "." is the decimal point, a negative amount of money is in
//! parentheses, and the month and weekday names are English.

use crate::strings::StringTables;
use crate::timing::Date;

/// The strings of the game, by number. [`StringTables`] is the real one.
pub trait Strings {
    /// The text of string `id`, empty if there is none.
    fn string(&self, id: u32) -> &str;
}

impl Strings for StringTables {
    fn string(&self, id: u32) -> &str {
        StringTables::string(self, id)
    }
}

/// The string number of the currency symbol.
const SYMBOL: u32 = 22100;
/// The month names of the graphs are the strings after this number, one to twelve.
const GRAPH_MONTH: u32 = 22100;

/// A value for a placeholder of a template.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arg<'a> {
    /// For `%s`.
    Text(&'a str),
    /// For `%d` (digits), `%m` (money) and `%r` (a string number); `%s` writes its digits.
    Int(i64),
}

/// Writes string `id` with its placeholders filled in: the game's `buildString`.
pub fn build(strings: &impl Strings, id: u32, args: &[Arg]) -> String {
    expand(strings, strings.string(id), args)
}

/// Fills in the placeholders of `template`.
///
/// `%s` is a text, `%d` an integer without grouping, `%m` an amount in whole units of money and
/// `%r` another string, by number; `%1s` to `%9r` are the positional forms; `%%` is a percent
/// sign, and a `%` before any other character gives that character. Values are taken in this
/// order: first one for each position used, by the position's digit (position 3 with positions
/// 1 and 5 is the second value, wherever it stands in the template), then the plain
/// placeholders in the order they appear. A position can be used twice. A `%` at the end is
/// dropped, and a missing value, or one of the wrong kind, writes nothing.
pub fn expand(strings: &impl Strings, template: &str, args: &[Arg]) -> String {
    enum Part {
        Char(char),
        /// The position digit if there is one, and the kind letter.
        Slot(Option<u32>, char),
    }
    const KINDS: &str = "sdmr";
    let mut parts = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            parts.push(Part::Char(c));
            continue;
        }
        match chars.next() {
            None => {}
            Some(digit @ '1'..='9') => match chars.next_if(|k| KINDS.contains(*k)) {
                Some(kind) => parts.push(Part::Slot(digit.to_digit(10), kind)),
                None => parts.push(Part::Char(digit)),
            },
            Some(kind) if KINDS.contains(kind) => parts.push(Part::Slot(None, kind)),
            Some(other) => parts.push(Part::Char(other)),
        }
    }
    let mut positions: Vec<u32> = parts
        .iter()
        .filter_map(|p| match p {
            Part::Slot(Some(n), _) => Some(*n),
            _ => None,
        })
        .collect();
    positions.sort_unstable();
    positions.dedup();
    let mut plain = positions.len();
    let mut out = String::new();
    for part in &parts {
        let (index, kind) = match part {
            Part::Char(c) => {
                out.push(*c);
                continue;
            }
            Part::Slot(Some(n), kind) => (positions.iter().position(|p| p == n).unwrap_or(0), kind),
            Part::Slot(None, kind) => {
                plain += 1;
                (plain - 1, kind)
            }
        };
        match (kind, args.get(index)) {
            ('s', Some(Arg::Text(text))) => out.push_str(text),
            ('s' | 'd', Some(Arg::Int(n))) => out.push_str(&integer(*n, false)),
            ('m', Some(Arg::Int(n))) => out.push_str(&money(strings, *n, true)),
            ('r', Some(Arg::Int(n))) => {
                out.push_str(u32::try_from(*n).map_or("", |id| strings.string(id)))
            }
            _ => {}
        }
    }
    out
}

/// `digits` (decimal digits only) with a comma between every group of three.
fn group(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// An integer: `1234567` is "1,234,567" with grouping and "1234567" without, `-5` is "-5".
pub fn integer(n: i64, grouping: bool) -> String {
    let digits = n.unsigned_abs().to_string();
    let sign = if n < 0 { "-" } else { "" };
    match grouping {
        true => format!("{sign}{}", group(&digits)),
        false => format!("{sign}{digits}"),
    }
}

/// The symbol of the currency: string 22100, or "$" if that is empty or a single space.
fn symbol(strings: &impl Strings) -> &str {
    match strings.string(SYMBOL) {
        "" | " " => "$",
        symbol => symbol,
    }
}

/// `body` with the symbol before it, in parentheses if negative.
fn currency(strings: &impl Strings, body: &str, negative: bool) -> String {
    let symbol = symbol(strings);
    match negative {
        true => format!("({symbol}{body})"),
        false => format!("{symbol}{body}"),
    }
}

/// An amount in whole units: `74878` is "$74,878", `0` is "$0", `-1234` is "($1,234)"; without
/// `grouping` they are "$74878" and "($1234)".
pub fn money(strings: &impl Strings, amount: i64, grouping: bool) -> String {
    let digits = amount.unsigned_abs().to_string();
    let body = if grouping { group(&digits) } else { digits };
    currency(strings, &body, amount < 0)
}

/// The whole part and the two cent digits of `x` (finite, not negative) as the original's C
/// library writes `%0.2f`: it takes 17 significant digits and rounds those half up, so 1.125 is
/// "1.13" (round-half-even would give "1.12") while 1.005, which is a little below, stays "1.00".
fn cents(x: f64) -> (String, String) {
    let text = format!("{x:.16e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let point = exponent.parse::<i32>().unwrap_or(0) + 1;
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    // The value is 0.DIGITS times ten to the power `point`.
    let (whole, frac) = match usize::try_from(point) {
        Ok(p) if p >= digits.len() => (
            digits.clone() + &"0".repeat(p - digits.len()),
            String::new(),
        ),
        Ok(p) if p > 0 => (digits[..p].to_string(), digits[p..].to_string()),
        _ => (
            "0".to_string(),
            "0".repeat(point.unsigned_abs() as usize) + &digits,
        ),
    };
    let frac: Vec<u8> = frac
        .bytes()
        .chain(std::iter::repeat(b'0'))
        .take(3)
        .collect();
    let mut kept: Vec<u8> = whole.bytes().chain(frac[..2].iter().copied()).collect();
    if frac[2] >= b'5' {
        // Add one at the last digit, carrying leftwards.
        let mut i = kept.len();
        while i > 0 && kept[i - 1] == b'9' {
            kept[i - 1] = b'0';
            i -= 1;
        }
        match i {
            0 => kept.insert(0, b'1'),
            _ => kept[i - 1] += 1,
        }
    }
    let split = kept.len() - 2;
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    (text(&kept[..split]), text(&kept[split..]))
}

/// An amount with cents, written with two decimals as the original's C library rounds them
/// (see `cents`): `12.5` is "$12.50", `-1234.5` is "($1,234.50)", `1.125` is "$1.13". An amount
/// that rounds to zero is "$0.00" whatever its sign, and one that is not a number is 0. Without
/// `grouping` the whole part has no separators: "$1234.50".
pub fn money_cents(strings: &impl Strings, amount: f64, grouping: bool) -> String {
    let amount = if amount.is_finite() { amount } else { 0.0 };
    let (whole, cents) = cents(amount.abs());
    let negative = amount < 0.0 && (whole != "0" || cents != "00");
    let whole = if grouping { group(&whole) } else { whole };
    currency(strings, &format!("{whole}.{cents}"), negative)
}

/// An amount held as a decimal number, rounded to whole units: to the nearest, halves to even.
pub fn whole_units(amount: f64) -> i64 {
    amount.round_ties_even() as i64
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The first three letters of a name.
fn abbreviation(name: &str) -> &str {
    &name[..3.min(name.len())]
}

/// A date written from a pattern of the Windows date pictures: `d` the day, `dd` with a leading
/// zero, `ddd` and `dddd` the weekday's short and full name, `M` and `MM` the month number,
/// `MMM` and `MMMM` its short and full name, `y` the last two digits of the year without a
/// leading zero (the game shows year 2100 as "0"), `yy` with it, `yyy` and `yyyy` the whole year;
/// text in single quotes is written as it is, any other character too.
pub fn date(pattern: &str, date: &Date) -> String {
    let month = MONTHS.get((date.month as usize).wrapping_sub(1)).copied();
    let weekday = WEEKDAYS.get(date.weekday as usize).copied();
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.next_if_eq(&'\'').is_some() {
                out.push('\''); // two quotes in a row are one quote
                continue;
            }
            // A quoted run; two quotes inside it are one.
            loop {
                match chars.next() {
                    None => break,
                    Some('\'') if chars.next_if_eq(&'\'').is_some() => out.push('\''),
                    Some('\'') => break,
                    Some(c) => out.push(c),
                }
            }
            continue;
        }
        if !matches!(c, 'd' | 'M' | 'y') {
            out.push(c);
            continue;
        }
        let mut run = 1;
        while chars.next_if_eq(&c).is_some() {
            run += 1;
        }
        match (c, run) {
            ('d', 1) => out.push_str(&date.day.to_string()),
            ('d', 2) => out.push_str(&format!("{:02}", date.day)),
            ('d', 3) => out.push_str(weekday.map_or("", abbreviation)),
            ('d', _) => out.push_str(weekday.unwrap_or("")),
            ('M', 1) => out.push_str(&date.month.to_string()),
            ('M', 2) => out.push_str(&format!("{:02}", date.month)),
            ('M', 3) => out.push_str(month.map_or("", abbreviation)),
            ('M', _) => out.push_str(month.unwrap_or("")),
            (_, 1) => out.push_str(&(date.year % 100).to_string()),
            (_, 2) => out.push_str(&format!("{:02}", date.year % 100)),
            _ => out.push_str(&format!("{:04}", date.year)),
        }
    }
    out
}

/// The abbreviated name of month 1 to 12 on the axes of the graphs: strings 22101 to 22112,
/// or the English abbreviation where the string is empty. Any other month gives nothing.
pub fn graph_month(strings: &impl Strings, month: u32) -> String {
    let Some(name) = MONTHS.get((month as usize).wrapping_sub(1)) else {
        return String::new();
    };
    match strings.string(GRAPH_MONTH + month) {
        "" => abbreviation(name).to_string(),
        text => text.to_string(),
    }
}

#[cfg(test)]
mod tests;
