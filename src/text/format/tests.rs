use super::*;
use std::collections::HashMap;

struct Table(HashMap<u32, &'static str>);

impl Strings for Table {
    fn string(&self, id: u32) -> &str {
        self.0.get(&id).copied().unwrap_or("")
    }
}

fn table(entries: &[(u32, &'static str)]) -> Table {
    Table(entries.iter().copied().collect())
}

/// A table with the dollar sign, the shipped date patterns and a few other strings.
fn game() -> Table {
    table(&[
        (22100, "$"),
        (22012, "MMM',' 'Year' y"),
        (22013, "ddd'.' MMM d"),
        (22014, "MMM d', Year' y"),
        (1030, "Date: %s"),
        (154, "Are you sure you want to sell %s?\nRefund: "),
        (5504, "Whale Shark"),
        (7, "%1s has %2d legs, %1s!"),
    ])
}

use Arg::{Int, Text};

fn x(template: &str, args: &[Arg]) -> String {
    expand(&game(), template, args)
}

#[test]
fn each_placeholder_kind() {
    assert_eq!(x("a %s b", &[Text("lion")]), "a lion b");
    assert_eq!(x("%d animals", &[Int(1234567)]), "1234567 animals");
    assert_eq!(x("%d", &[Int(-5)]), "-5");
    assert_eq!(x("cost %m", &[Int(74878)]), "cost $74,878");
    assert_eq!(x("%m", &[Int(-1234)]), "($1,234)");
    assert_eq!(x("%r", &[Int(5504)]), "Whale Shark");
    assert_eq!(
        x("%s", &[Int(12)]),
        "12",
        "%s writes the digits of an integer"
    );
}

#[test]
fn plain_values_are_taken_in_order() {
    assert_eq!(x("%s %d %s", &[Text("a"), Int(2), Text("c")]), "a 2 c");
    assert_eq!(
        build(&game(), 154, &[Text("the lion")]),
        "Are you sure you want to sell the lion?\nRefund: "
    );
    assert_eq!(
        build(&game(), 1030, &[Text("Mon. Jan 1")]),
        "Date: Mon. Jan 1"
    );
}

#[test]
fn positions_take_their_values_first_in_digit_order() {
    // The values: position 1, position 2, then the plain placeholder.
    assert_eq!(
        x("%2s %1s %s", &[Text("one"), Text("two"), Text("tail")]),
        "two one tail"
    );
    // A position can be used twice.
    assert_eq!(
        build(&game(), 7, &[Text("Leo"), Int(4)]),
        "Leo has 4 legs, Leo!"
    );
    // Position digits that are used are ranked, whatever their numbers: 3 is the second value.
    assert_eq!(x("%3s%1s", &[Text("a"), Text("b")]), "ba");
    // The same value read as two kinds.
    assert_eq!(x("%1d / %1m", &[Int(1500)]), "1500 / $1,500");
}

#[test]
fn percent_signs_and_other_odd_templates() {
    assert_eq!(x("100%%", &[]), "100%");
    assert_eq!(
        x("50% off", &[]),
        "50 off",
        "a percent before a space gives the space"
    );
    assert_eq!(x("done%", &[]), "done", "a trailing percent is dropped");
    assert_eq!(x("%", &[]), "");
    assert_eq!(
        x("%1z %5", &[]),
        "1z 5",
        "an unknown kind gives the characters"
    );
    assert_eq!(x("%0s", &[Text("a")]), "0s");
    assert_eq!(x("é %s ü", &[Text("ñ")]), "é ñ ü");
    // Too few values, or the wrong kind, write nothing.
    assert_eq!(x("<%s|%d|%m|%r>", &[]), "<|||>");
    assert_eq!(x("<%d|%m|%r>", &[Text("a"), Text("b"), Text("c")]), "<||>");
    assert_eq!(x("%r|%r", &[Int(-1), Int(1 << 40)]), "|");
    assert_eq!(x("%9s", &[]), "");
}

#[test]
fn integers_with_and_without_grouping() {
    for (n, grouped, plain) in [
        (0, "0", "0"),
        (999, "999", "999"),
        (1000, "1,000", "1000"),
        (1234567, "1,234,567", "1234567"),
        (-5, "-5", "-5"),
        (-1000, "-1,000", "-1000"),
        (-123456, "-123,456", "-123456"),
        (
            i64::MIN,
            "-9,223,372,036,854,775,808",
            "-9223372036854775808",
        ),
        (i64::MAX, "9,223,372,036,854,775,807", "9223372036854775807"),
    ] {
        assert_eq!(integer(n, true), grouped);
        assert_eq!(integer(n, false), plain);
    }
}

#[test]
fn the_three_money_forms() {
    let g = game();
    // Whole units, grouped.
    assert_eq!(money(&g, 74878, true), "$74,878");
    assert_eq!(money(&g, 0, true), "$0");
    assert_eq!(money(&g, -1234, true), "($1,234)");
    // Whole units, not grouped (the cost at the pointer).
    assert_eq!(money(&g, 74878, false), "$74878");
    assert_eq!(money(&g, -1234, false), "($1234)");
    // With cents.
    assert_eq!(money_cents(&g, 12.5), "$12.50");
    assert_eq!(money_cents(&g, 0.0), "$0.00");
    assert_eq!(money_cents(&g, 1234567.891), "$1,234,567.89");
    assert_eq!(money_cents(&g, -1234.5), "($1,234.50)");
    assert_eq!(money_cents(&g, 0.004), "$0.00");
    assert_eq!(money_cents(&g, -0.004), "$0.00");
    assert_eq!(money_cents(&g, f64::NAN), "$0.00");
    assert_eq!(money_cents(&g, f64::NEG_INFINITY), "$0.00");
    assert!(money_cents(&g, 1e30).starts_with("$1,000,000,000,000,000"));
    assert_eq!(money(&g, i64::MIN, true), "($9,223,372,036,854,775,808)");
}

#[test]
fn the_symbol_comes_from_the_string_table() {
    // Empty, a single space and absent all give the dollar sign.
    for t in [table(&[]), table(&[(22100, "")]), table(&[(22100, " ")])] {
        assert_eq!(money(&t, 5, true), "$5");
    }
    let euro = table(&[(22100, "€")]);
    assert_eq!(money(&euro, -5, true), "(€5)");
    assert_eq!(money_cents(&euro, 5.0), "€5.00");
    assert_eq!(expand(&euro, "%m", &[Int(1000)]), "€1,000");
}

#[test]
fn amounts_round_to_whole_units_with_halves_to_even() {
    for (amount, whole) in [
        (74878.4, 74878),
        (-0.4, 0),
        (0.5, 0),
        (1.5, 2),
        (2.5, 2),
        (-2.5, -2),
        (-1234.0, -1234),
        (f64::NAN, 0),
        (1e300, i64::MAX),
        (-1e300, i64::MIN),
    ] {
        assert_eq!(whole_units(amount), whole, "{amount}");
    }
}

fn day(year: u32, month: u32, day: u32, weekday: u32) -> Date {
    Date {
        year,
        month,
        day,
        weekday,
        hour: 0,
        minute: 0,
        second: 0,
        millisecond: 0,
    }
}

#[test]
fn the_three_shipped_date_patterns() {
    let g = game();
    let first = day(2001, 1, 1, 0);
    assert_eq!(date(g.string(22012), &first), "Jan, Year 1");
    assert_eq!(date(g.string(22013), &first), "Mon. Jan 1");
    assert_eq!(date(g.string(22014), &first), "Jan 1, Year 1");
    let later = day(2010, 12, 25, 5);
    assert_eq!(date(g.string(22012), &later), "Dec, Year 10");
    assert_eq!(date(g.string(22013), &later), "Sat. Dec 25");
    assert_eq!(date(g.string(22014), &later), "Dec 25, Year 10");
}

#[test]
fn the_year_shows_its_last_two_digits() {
    let g = game();
    let year = |y| date(g.string(22012), &day(y, 1, 1, 0));
    assert_eq!(year(2009), "Jan, Year 9");
    assert_eq!(year(2099), "Jan, Year 99");
    // The original's wrap: year 100 is shown as 0, year 101 as 1.
    assert_eq!(year(2100), "Jan, Year 0");
    assert_eq!(year(2101), "Jan, Year 1");
    assert_eq!(year(2000), "Jan, Year 0");
    assert_eq!(year(u32::MAX), format!("Jan, Year {}", u32::MAX % 100));
}

#[test]
fn date_pattern_letters() {
    let d = day(2004, 2, 3, 1);
    for (pattern, text) in [
        ("d", "3"),
        ("dd", "03"),
        ("ddd", "Tue"),
        ("dddd", "Tuesday"),
        ("M", "2"),
        ("MM", "02"),
        ("MMM", "Feb"),
        ("MMMM", "February"),
        ("y", "4"),
        ("yy", "04"),
        ("yyyy", "2004"),
        ("d/M/yy", "3/2/04"),
        ("'d' d", "d 3"),
        ("'it''s' d", "it's 3"),
        ("a''b", "a'b"),
        ("'open", "open"),
        ("", ""),
        ("[ ] é", "[ ] é"),
    ] {
        assert_eq!(date(pattern, &d), text, "{pattern:?}");
    }
    // A date outside the calendar's range has no names and does not panic.
    let odd = day(2001, 0, 40, 9);
    assert_eq!(date("ddd MMM MMMM dddd", &odd), "   ");
    assert_eq!(date("MMM", &day(2001, 13, 1, 0)), "");
}

#[test]
fn the_names_of_every_month_and_weekday() {
    let months = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec";
    let got: Vec<String> = (1..=12).map(|m| date("MMM", &day(2001, m, 1, 0))).collect();
    assert_eq!(got.join(" "), months);
    let days: Vec<String> = (0..7).map(|w| date("ddd", &day(2001, 1, 1, w))).collect();
    assert_eq!(days.join(" "), "Mon Tue Wed Thu Fri Sat Sun");
}

#[test]
fn graph_months_come_from_the_string_table() {
    let t = table(&[(22101, "Jan"), (22102, "Feb."), (22112, "Dez")]);
    assert_eq!(graph_month(&t, 1), "Jan");
    assert_eq!(graph_month(&t, 2), "Feb.");
    assert_eq!(graph_month(&t, 12), "Dez");
    // An empty string falls back to the English abbreviation.
    assert_eq!(graph_month(&t, 3), "Mar");
    assert_eq!(graph_month(&table(&[]), 9), "Sep");
    // Months that do not exist have no name.
    assert_eq!(graph_month(&t, 0), "");
    assert_eq!(graph_month(&t, 13), "");
    assert_eq!(graph_month(&t, u32::MAX), "");
}

/// The game's own strings give the formats of the specification.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_installed_strings_format_as_the_game_does() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let tables = StringTables::open(std::path::Path::new(&dir)).unwrap();
    assert_eq!(money(&tables, 74878, true), "$74,878");
    let first = day(2001, 1, 1, 0);
    assert_eq!(date(tables.string(22012), &first), "Jan, Year 1");
    assert_eq!(date(tables.string(22013), &first), "Mon. Jan 1");
    assert_eq!(date(tables.string(22014), &first), "Jan 1, Year 1");
    assert_eq!(graph_month(&tables, 12), "Dec");
    assert_eq!(
        build(&tables, 1030, &[Text("Mon. Jan 1")]),
        "Date: Mon. Jan 1"
    );
}
