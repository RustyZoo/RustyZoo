use super::*;
use crate::money::{CONSTRUCTION, DONATIONS};

struct Plain;

impl Strings for Plain {
    fn string(&self, _: u32) -> &str {
        ""
    }
}

/// The text of cell `id`.
fn text(cells: &[Cell], id: i32) -> &str {
    &cells.iter().find(|c| c.id == id).unwrap().text
}

fn colour(cells: &[Cell], id: i32) -> Option<Colour> {
    cells.iter().find(|c| c.id == id).unwrap().colour
}

#[test]
fn the_current_column_of_the_first_month_is_filled_and_the_older_ones_are_blank() {
    let mut m = Money::default();
    m.refund(DONATIONS, 1234.4);
    m.charge(CONSTRUCTION, 40.0);
    m.cash = 5000.0;
    let cells = cells(&m, &Plain);
    assert_eq!(cells.len(), 4 * 14 + 3);
    // The current column: the header, then the rows by number (3930 + r).
    assert_eq!(text(&cells, 3920), "Jan");
    assert_eq!(text(&cells, 3930), "0", "admissions are a number");
    assert_eq!(text(&cells, 3931), "$0");
    assert_eq!(text(&cells, 3930 + 2), "($40)");
    assert_eq!(
        text(&cells, 3930 + 3),
        "$0",
        "an expense of 0 is $0, not ($0)"
    );
    assert_eq!(text(&cells, 3930 + 8), "$1,234");
    assert_eq!(text(&cells, 3930 + 11), "$1,194");
    assert_eq!(
        colour(&cells, 3930 + 11),
        Some(Colour {
            column: 0,
            negative: false
        })
    );
    // Expenses are negative.
    let mut m2 = m.clone();
    m2.cash = 40_000.0;
    m2.charge(CONSTRUCTION, 12_636.0 - 40.0);
    let cells2 = super::cells(&m2, &Plain);
    assert_eq!(text(&cells2, 3930 + 2), "($12,636)");
    assert_eq!(text(&cells2, 3930 + 11), "($11,402)");
    assert_eq!(
        colour(&cells2, 3930 + 11),
        Some(Colour {
            column: 0,
            negative: true
        })
    );
    // The three older columns, headers included, are blank.
    for column in 1..4 {
        assert_eq!(text(&cells, 3920 + column), "");
        for r in 0..13 {
            assert_eq!(text(&cells, 3930 + 15 * column + r), "");
        }
        assert_eq!(colour(&cells, 3930 + 15 * column + 11), None);
    }
    // Total Cash, Zoo Value and Net Income, coloured by their own sign.
    assert_eq!(text(&cells, 3926), "$5,000");
    assert_eq!(text(&cells, 3928), "$0");
    assert_eq!(text(&cells, 3991), "$1,194");
    assert_eq!(text(&cells2, 3991), "($11,402)");
    assert_eq!(
        colour(&cells2, 3991),
        Some(Colour {
            column: 0,
            negative: true
        })
    );
}

#[test]
fn a_synthetic_table_fills_every_row_and_the_months_step_back_across_the_year() {
    let mut m = Money::default();
    // One distinct amount per category and month, so that a wrong sum or column shows.
    for category in 0..31 {
        for month in 0..12 {
            m.monthly[category][month] = (100 * category + month + 1) as f32;
        }
    }
    (m.month, m.year) = (2, 1);
    m.cash = -7.4;
    m.total[NET] = 99.6;
    let c = cells(&m, &Plain);
    // Columns left to right: November, December, January, and the current February.
    let months = [(1, 11), (2, 12), (3, 1), (0, 2)];
    for (column, month) in months {
        let names = [
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        assert_eq!(text(&c, 3920 + column), names[month as usize]);
        let at = |cats: &[usize]| -> i64 {
            cats.iter()
                .map(|&k| (100 * k) as i64 + i64::from(month))
                .sum()
        };
        let row = |r: i32| text(&c, 3930 + 15 * column + r).to_string();
        assert_eq!(row(0), crate::text::format::integer(at(&[4]), true));
        for (r, cats, sign) in [
            (1, &[5][..], 1),
            (2, &[3], -1),
            (3, &[2], -1),
            (4, &[0, 1, 28], -1),
            (5, &[19], -1),
            (6, &[6, 7], 1),
            (7, &[11, 12, 13], -1),
            (8, &[8, 20], 1),
            (9, &[9, 10], 1),
            (10, &[29], -1),
            (11, &[14], 1),
            (12, &[30], 1),
        ] {
            let want = crate::text::format::money(&Plain, sign * at(cats), true);
            assert_eq!(row(r), want, "column {column} row {r}");
        }
    }
    assert_eq!(text(&c, 3926), "($7)");
    assert_eq!(text(&c, 3928), "$1,502");
    assert_eq!(text(&c, 3991), "$100");
}

#[test]
fn a_month_change_moves_the_columns_and_a_year_before_the_first_is_blank() {
    let mut m = Money {
        cash: 1000.0,
        ..Money::default()
    };
    m.charge(CONSTRUCTION, 100.0);
    for _ in 0..2 {
        m.month_change();
        m.charge(CONSTRUCTION, 100.0);
    }
    // March of year 0: January, February and March are filled, the fourth column (December of
    // year -1) is blank.
    let c = cells(&m, &Plain);
    assert_eq!(text(&c, 3920), "Mar");
    assert_eq!(text(&c, 3920 + 1), "");
    assert_eq!(text(&c, 3920 + 2), "Jan");
    assert_eq!(text(&c, 3920 + 3), "Feb");
    assert_eq!(text(&c, 3930 + 15 * 2 + 2), "($100)");
    assert_eq!(text(&c, 3930 + 15 + 2), "");
    // The next January of year 1 shows December, November and October of year 0.
    for _ in 0..10 {
        m.month_change();
    }
    assert_eq!((m.month, m.year), (1, 1));
    let c = cells(&m, &Plain);
    assert_eq!(text(&c, 3920 + 1), "Oct");
    assert_eq!(text(&c, 3920), "Jan");
    assert_eq!(text(&c, 3930 + 2), "$0");
    // A month change clears the new column: February of year 1 does not show last year's.
    m.month_change();
    assert_eq!(text(&cells(&m, &Plain), 3930 + 2), "$0");
}

#[test]
fn bad_accounts_do_not_panic() {
    let mut m = Money::default();
    (m.month, m.year) = (0, 99);
    m.cash = f32::NAN;
    m.monthly[4][0] = f32::INFINITY;
    let c = cells(&m, &Plain);
    assert_eq!(c.len(), 4 * 14 + 3);
    (m.month, m.year) = (u32::MAX, 0);
    cells(&m, &Plain);
}
