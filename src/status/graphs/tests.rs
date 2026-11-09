use super::*;
use crate::money::{DONATIONS, ENDOWMENT, GUESTS, NET, RATING};

struct None_;

impl Strings for None_ {
    fn string(&self, _: u32) -> &str {
        ""
    }
}

fn accounts(month: u32, year: u32) -> Money {
    let mut money = Money::default();
    (money.month, money.year) = (month, year);
    money
}

fn xs(points: &[Point]) -> Vec<i32> {
    points.iter().map(|p| p.x).collect()
}

fn labels(points: &[Point]) -> Vec<String> {
    (points.iter())
        .map(|p| String::from_utf8(p.label.clone()).unwrap())
        .collect()
}

#[test]
fn the_first_year_has_the_months_from_january_to_the_current_one() {
    let points = points(&accounts(3, 0), 0, &None_);
    assert_eq!(xs(&points), [9, 10, 11]);
    assert_eq!(labels(&points).len(), 3);
    assert_eq!(labels(&points)[0], graph_month(&None_, 1));
    assert_eq!(labels(&points)[2], graph_month(&None_, 3));
    // January of the first year is a single point at the right end.
    assert_eq!(xs(&super::points(&accounts(1, 0), 0, &None_)), [11]);
}

#[test]
fn later_years_have_twelve_points_ending_with_the_current_month() {
    let points = points(&accounts(3, 1), 0, &None_);
    assert_eq!(xs(&points), (0..12).collect::<Vec<_>>());
    let names: Vec<String> = [4, 5, 6, 7, 8, 9, 10, 11, 12, 1, 2, 3]
        .iter()
        .map(|&m| graph_month(&None_, m))
        .collect();
    assert_eq!(labels(&points), names);
    // December: the same year from January.
    let december = super::points(&accounts(12, 1), 0, &None_);
    assert_eq!(labels(&december)[0], graph_month(&None_, 1));
    assert_eq!(labels(&december)[11], graph_month(&None_, 12));
}

#[test]
fn a_value_is_the_monthly_slot_cut_toward_zero() {
    let mut money = accounts(2, 0);
    money.monthly[NET][0] = -2756.9;
    money.monthly[NET][1] = 123_351.9;
    let net: Vec<i32> = points(&money, 2, &None_).iter().map(|p| p.y).collect();
    assert_eq!(net, [-2756, 123_351]);
    money.monthly[RATING][1] = 66.9;
    money.monthly[GUESTS][1] = 12.0;
    assert_eq!(points(&money, 0, &None_)[1].y, 66);
    assert_eq!(points(&money, 3, &None_)[1].y, 12);
}

#[test]
fn donations_add_the_cut_donations_and_the_cut_endowment() {
    let mut money = accounts(1, 0);
    money.monthly[DONATIONS][0] = 100.9;
    money.monthly[ENDOWMENT][0] = 200.9;
    assert_eq!(points(&money, 1, &None_)[0].y, 300);
    money.monthly[DONATIONS][0] = -0.5;
    assert_eq!(points(&money, 1, &None_)[0].y, 200);
}

#[test]
fn the_earlier_months_come_from_the_ring_of_the_table() {
    let mut money = accounts(2, 1);
    for m in 0..12 {
        money.monthly[RATING][m] = (m + 1) as f32;
    }
    let rating: Vec<i32> = points(&money, 0, &None_).iter().map(|p| p.y).collect();
    assert_eq!(rating, [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 1, 2]);
}

#[test]
fn a_month_or_a_year_out_of_range_never_panics() {
    for (month, year) in [(0, 0), (13, 0), (u32::MAX, 0), (1, u32::MAX), (12, 19)] {
        let n = points(&accounts(month, year), 0, &None_).len();
        assert!(n <= 12, "{month} {year}: {n}");
    }
}

#[test]
fn a_switch_button_gives_line_or_bar_to_its_graph() {
    assert_eq!(switched(4151), Some((4150, Style::Line)));
    assert_eq!(switched(4152), Some((4150, Style::Bar)));
    assert_eq!(switched(13991), Some((13978, Style::Line)));
    assert_eq!(switched(13992), Some((13978, Style::Bar)));
    assert_eq!(switched(13994), Some((13967, Style::Line)));
    assert_eq!(switched(13995), Some((13967, Style::Bar)));
    assert_eq!(switched(13997), Some((13965, Style::Line)));
    assert_eq!(switched(13998), Some((13965, Style::Bar)));
    assert_eq!([switched(0), switched(-1), switched(i32::MAX)], [None; 3]);
}
