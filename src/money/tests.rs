use super::*;

fn money(cash: f32) -> Money {
    Money {
        cash,
        ..Money::default()
    }
}

/// The value of `category` in the current month, the current year and in total.
fn row(m: &Money, category: usize) -> (f32, f32, f32) {
    (
        m.monthly[category][m.month as usize - 1],
        m.yearly[category][m.year as usize],
        m.total[category],
    )
}

#[test]
fn a_charge_a_refund_and_an_undo_book_into_the_three_tables_and_the_net_line() {
    let mut m = money(1000.0);
    m.month = 3;
    assert!(m.charge(CONSTRUCTION, 300.0));
    assert_eq!(m.cash, 700.0);
    assert_eq!(row(&m, CONSTRUCTION), (300.0, 300.0, 300.0));
    assert_eq!(row(&m, NET), (-300.0, -300.0, -300.0));
    // The column is the month's, the others stay empty.
    assert_eq!(m.monthly[CONSTRUCTION][2], 300.0);
    assert_eq!(m.monthly[CONSTRUCTION].iter().sum::<f32>(), 300.0);

    m.refund(CONSTRUCTION_REFUND, 120.0);
    assert_eq!(m.cash, 820.0);
    assert_eq!(row(&m, CONSTRUCTION_REFUND), (120.0, 120.0, 120.0));
    assert_eq!(row(&m, NET), (-180.0, -180.0, -180.0));

    // Undoing the purchase gives the price back and lowers its category; no refund is booked.
    m.undo_charge(CONSTRUCTION, 300.0);
    assert_eq!(m.cash, 1120.0);
    assert_eq!(row(&m, CONSTRUCTION), (0.0, 0.0, 0.0));
    assert_eq!(row(&m, CONSTRUCTION_REFUND), (120.0, 120.0, 120.0));
    assert_eq!(row(&m, NET), (120.0, 120.0, 120.0));

    // Undoing the sale takes the refund back and lowers the refund category.
    m.undo_refund(CONSTRUCTION_REFUND, 120.0);
    assert_eq!(m.cash, 1000.0);
    assert_eq!(m.monthly, money(0.0).monthly);
    assert_eq!(m.yearly, money(0.0).yearly);
    assert_eq!(m.total, money(0.0).total);
}

#[test]
fn income_adds_to_the_net_line_and_a_count_does_not_touch_it() {
    let mut m = money(0.0);
    m.refund(DONATIONS, DOLLAR_KEY);
    m.refund(ADMISSIONS, 5.0);
    m.charge(ANIMAL_PURCHASE, 0.0);
    assert_eq!(row(&m, DONATIONS), (10_000.0, 10_000.0, 10_000.0));
    assert_eq!(row(&m, NET), (10_000.0, 10_000.0, 10_000.0));
    assert_eq!(m.total[ADMISSIONS], 5.0);
    assert_eq!(m.cash, 10_005.0);
}

#[test]
fn an_action_is_allowed_when_it_costs_nothing_or_at_most_the_cash() {
    let mut m = money(100.0);
    for (cost, allowed) in [
        (0.0, true),
        (-5.0, true),
        (99.5, true),
        (100.0, true),
        (100.5, false),
        (f32::INFINITY, false),
        (f32::NAN, false),
    ] {
        assert_eq!(m.can_pay(cost), allowed, "{cost}");
    }
    // A refused charge costs nothing and books nothing.
    let before = m.clone();
    assert!(!m.charge(CONSTRUCTION, 100.5));
    assert!(!m.charge(CONSTRUCTION, f32::NAN));
    assert!(!m.charge(CATEGORIES, 1.0));
    assert_eq!(m, before);
    // Free actions pass even with no cash at all, or less than none.
    let mut broke = money(-10.0);
    assert!(broke.charge(CONSTRUCTION, 0.0));
    assert!(!broke.charge(CONSTRUCTION, 1.0));
    assert!(m.charge(CONSTRUCTION, 100.0));
    assert_eq!(m.cash, 0.0);
}

#[test]
fn what_is_not_a_number_or_a_category_changes_nothing() {
    let mut m = money(50.0);
    let before = m.clone();
    m.refund(DONATIONS, f32::NAN);
    m.refund(CATEGORIES, 1.0);
    m.undo_charge(CONSTRUCTION, f32::INFINITY);
    m.undo_refund(99, 1.0);
    m.set_cash(f32::NAN);
    m.add_cash(f32::INFINITY);
    assert_eq!(m, before);
}

#[test]
fn the_starting_cash_is_set_then_added_and_never_booked() {
    let mut m = money(7.0);
    m.total[CONSTRUCTION] = 9.0;
    m.start(StartCash::default(), false);
    assert_eq!(m.cash, 7.0, "a saved game keeps its cash");
    m.start(
        StartCash {
            set: Some(100_000.0),
            add: Some(200.0),
        },
        false,
    );
    assert_eq!(m.cash, 100_200.0);
    assert_eq!(m.total[NET], 0.0);
    assert_eq!(m.total[CONSTRUCTION], 9.0, "no reset asked");
    m.start(
        StartCash {
            set: Some(50_000.0),
            add: None,
        },
        true,
    );
    assert_eq!(m.cash, 50_000.0);
    assert_eq!(m.total, [0.0; CATEGORIES], "reset clears the accounts");
    assert_eq!((m.month, m.year), (1, 0));
    // A negative amount is a plain number; so is zero.
    m.start(
        StartCash {
            set: Some(-5.0),
            add: Some(0.0),
        },
        false,
    );
    assert_eq!(m.cash, -5.0);
}

#[test]
fn a_month_change_clears_the_new_month_and_a_january_clears_the_year() {
    let mut m = money(0.0);
    m.month = 11;
    m.year = 2;
    m.total[ADMISSIONS] = 40.0;
    // Last year's values sit in the columns that come next.
    m.monthly[CONSTRUCTION][11] = 5.0;
    m.monthly[CONSTRUCTION][0] = 6.0;
    m.yearly[CONSTRUCTION][3] = 7.0;
    m.yearly[CONSTRUCTION][2] = 8.0;
    m.month_change();
    assert_eq!((m.month, m.year), (12, 2));
    assert_eq!(m.monthly[CONSTRUCTION][11], 0.0, "December is cleared");
    assert_eq!(m.monthly[CONSTRUCTION][0], 6.0, "January is not");
    assert_eq!(m.yearly[CONSTRUCTION][3], 7.0);
    assert_eq!(m.monthly[ADMISSIONS_TOTAL][11], 40.0);
    m.month_change();
    assert_eq!((m.month, m.year), (1, 3));
    assert_eq!(m.monthly[CONSTRUCTION][0], 0.0);
    assert_eq!(
        m.yearly[CONSTRUCTION][3], 0.0,
        "the new year's column is cleared"
    );
    assert_eq!(m.yearly[CONSTRUCTION][2], 8.0, "the old one stays");
    // The year ring wraps from 19 to 0.
    m.month = 12;
    m.year = 19;
    m.yearly[CONSTRUCTION][0] = 1.0;
    m.month_change();
    assert_eq!((m.month, m.year), (1, 0));
    assert_eq!(m.yearly[CONSTRUCTION][0], 0.0);
    // The cash is not touched by a month change.
    assert_eq!(m.cash, 0.0);
}

#[test]
fn a_booking_goes_to_the_column_of_the_month_and_the_year() {
    let mut m = money(1000.0);
    m.charge(CONSTRUCTION, 10.0);
    m.month_change();
    m.charge(CONSTRUCTION, 20.0);
    assert_eq!(m.monthly[CONSTRUCTION][..3], [10.0, 20.0, 0.0]);
    assert_eq!(
        (m.yearly[CONSTRUCTION][0], m.total[CONSTRUCTION]),
        (30.0, 30.0)
    );
    // Indices out of range (a stored file is clamped, but the fields are public) do not panic.
    m.month = 0;
    m.year = 99;
    m.charge(CONSTRUCTION, 1.0);
    m.month_change();
    assert_eq!((m.month, m.year), (2, 19));
}

#[test]
fn the_status_bar_shows_whole_units_in_one_of_three_colours() {
    let bar = |cash: f32| money(cash).bar();
    assert_eq!(bar(50_000.0), (50_000, Tone::Green));
    assert_eq!(bar(0.6), (1, Tone::Green));
    assert_eq!(bar(0.5), (0, Tone::Yellow), "halves go to even");
    assert_eq!(bar(0.4), (0, Tone::Yellow));
    assert_eq!(bar(1.5), (2, Tone::Green));
    assert_eq!(bar(-0.4), (0, Tone::Yellow));
    assert_eq!(bar(-0.6), (-1, Tone::Red));
    assert_eq!(bar(-1234.0), (-1234, Tone::Red));
    assert_eq!(bar(f32::MAX).1, Tone::Green);
    assert_eq!(Tone::Green.rgb(), [83, 219, 83]);
    assert_eq!(Tone::Yellow.rgb(), [255, 212, 60]);
    assert_eq!(Tone::Red.rgb(), [186, 69, 33]);
    let us = Symbol::new("$".into());
    assert_eq!(us.text(74_878), "$74,878");
    assert_eq!(us.text(0), "$0");
    assert_eq!(us.text(-1234), "($1,234)");
    assert_eq!(
        Symbol::new(" ".into()).text(5),
        "$5",
        "a blank symbol reads as a dollar"
    );
}
