use super::*;
use crate::fixtures::ty;
use crate::timing::DAY_MS;
use crate::unit::testing::{centre, entity, world_with};

const SELLS: &str = "[Characteristics/Integers]\ncCommerce=1\n[Satisfies]\nbuilding\n";

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for (name, ini) in [
        ("stall", SELLS),
        ("burger", SELLS),
        // Satisfies the need but charges nothing: not listed.
        ("bench", "[Satisfies]\nbuilding\n"),
        // Sells but is not a guest building: not listed.
        ("depot", "[Characteristics/Integers]\ncCommerce=1\n"),
    ] {
        cat.push(ty("building", name, "", ini));
    }
    cat
}

/// A building of type `kind` named `name`, with its guests since construction, income, upkeep and
/// the day it was built on.
struct Shop(&'static str, &'static str, u32, f32, f32, i64);

fn world(shops: &[Shop]) -> World {
    let mut world = world_with((12, 12), |zoo| {
        for (n, s) in shops.iter().enumerate() {
            let mut e = entity("building", s.0, n as u32 + 1, centre((n as i32, 1)), 0);
            e.common.as_mut().unwrap().name = s.1.into();
            zoo.entities.push(e);
        }
    });
    for (n, s) in shops.iter().enumerate() {
        let mut live = Live::default();
        (live.guests[2], live.income[2], live.upkeep) = (s.2, s.3, s.4);
        live.built = s.5 * DAY_MS as i64;
        world.units.buildings.insert(n as u32 + 1, live);
    }
    world
}

fn names(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|r| String::from_utf8(r.name.clone()).unwrap())
        .collect()
}

fn listed(shops: &[Shop], now_days: i64) -> Vec<Row> {
    let now = now_days * DAY_MS as i64;
    rows(&world(shops), &catalogue(), |_| "Default".into(), now)
}

#[test]
fn only_the_guest_buildings_that_sell_are_listed_in_the_order_of_the_world() {
    let rows = listed(
        &[
            Shop("stall", "B", 0, 0.0, 0.0, 0),
            Shop("bench", "Bench", 0, 0.0, 0.0, 0),
            Shop("depot", "Depot", 0, 0.0, 0.0, 0),
            Shop("burger", "A", 0, 0.0, 0.0, 0),
            Shop("unknown", "Z", 0, 0.0, 0.0, 0),
        ],
        0,
    );
    assert_eq!(names(&rows), ["B", "A"]);
    assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [1, 4]);
}

#[test]
fn a_building_with_no_name_shows_its_type_name() {
    let rows = listed(&[Shop("stall", "", 0, 0.0, 0.0, 0)], 0);
    assert_eq!(names(&rows), ["Default"]);
}

#[test]
fn the_figures_are_the_months_open_the_guests_and_the_profit_and_its_average() {
    // 61 days is 3 months open: (61 + 29) / 30.
    let rows = listed(&[Shop("stall", "S", 12, 700.5, 100.25, 0)], 61);
    let r = &rows[0];
    assert_eq!((r.months, r.guests), (3, 12));
    assert_eq!(r.profit, 600.25);
    assert_eq!(r.average, 600.25 / 3.0);
    // Never less than one month, and a loss stays a loss.
    let new = listed(&[Shop("stall", "S", 0, 0.0, 42.0, 5)], 5);
    assert_eq!(
        (new[0].months, new[0].profit, new[0].average),
        (1, -42.0, -42.0)
    );
    // Built after now: still one month.
    let future = listed(&[Shop("stall", "S", 0, 0.0, 0.0, 90)], 0);
    assert_eq!(future[0].months, 1);
}

fn row(id: u32, name: &str, subtype: &str, months: i64, guests: u32, profit: f32) -> Row {
    Row {
        id,
        name: name.as_bytes().to_vec(),
        subtype: subtype.as_bytes().to_vec(),
        months,
        guests,
        profit,
        average: profit / months as f32,
    }
}

fn ids(rows: &[Row]) -> Vec<u32> {
    rows.iter().map(|r| r.id).collect()
}

fn sorted(key: usize, ascending: bool) -> Vec<u32> {
    let mut rows = vec![
        row(1, "b", "x", 5, 30, 10.0),
        row(2, "a", "x", 2, 10, 50.0),
        row(3, "a", "w", 5, 20, -5.0),
        row(4, "ab", "x", 2, 20, 50.0),
    ];
    let mut sort = Sort {
        key,
        ..Sort::default()
    };
    sort.ascending[key] = ascending;
    order(&mut rows, &sort);
    ids(&rows)
}

#[test]
fn the_first_key_sorts_by_type_then_name_byte_by_byte_with_a_prefix_first() {
    assert_eq!(sorted(0, true), [3, 2, 4, 1]);
    assert_eq!(sorted(0, false), [1, 4, 2, 3]);
}

#[test]
fn the_other_keys_sort_by_their_number_and_equal_keys_keep_the_order_of_the_list() {
    // Months: 2, 5, 2, 5 by id 2, 4, 1, 3.
    assert_eq!(sorted(1, true), [2, 4, 1, 3]);
    assert_eq!(sorted(1, false), [1, 3, 2, 4]);
    assert_eq!(sorted(2, true), [2, 3, 4, 1]);
    assert_eq!(sorted(2, false), [1, 3, 4, 2]);
    assert_eq!(sorted(3, true), [3, 1, 2, 4]);
    assert_eq!(sorted(3, false), [2, 4, 1, 3]);
    // The average: -2.5, 5, 25, 25.
    assert_eq!(sorted(4, true), [3, 1, 2, 4]);
    assert_eq!(sorted(4, false), [2, 4, 1, 3]);
}

#[test]
fn a_number_that_is_not_one_does_not_break_the_sort() {
    let mut rows = vec![
        row(1, "a", "x", 1, 0, f32::NAN),
        row(2, "b", "x", 1, 0, 1.0),
    ];
    for key in 0..=4 {
        order(
            &mut rows,
            &Sort {
                key,
                ..Sort::default()
            },
        );
    }
    assert_eq!(rows.len(), 2);
    // A key out of range uses the last one.
    order(
        &mut rows,
        &Sort {
            key: 99,
            ascending: [true; 5],
        },
    );
}

#[test]
fn the_defaults_are_the_type_key_ascending_and_the_others_descending() {
    let sort = Sort::default();
    assert_eq!(sort.key, 0);
    assert_eq!(sort.ascending, [true, false, false, false, false]);
}

#[test]
fn a_click_on_the_current_key_flips_it_and_another_key_keeps_the_direction_it_had() {
    let mut sort = Sort::default();
    assert!(sort.press(4175));
    assert_eq!((sort.key, sort.ascending[0]), (0, false));
    // Another key: no flip, its own direction (descending at the start).
    assert!(sort.press(4178));
    assert_eq!((sort.key, sort.ascending[3]), (3, false));
    assert!(sort.press(4178));
    assert!(sort.ascending[3]);
    // Back to the first key: it keeps the direction the flip gave it.
    assert!(sort.press(4175));
    assert_eq!((sort.key, sort.ascending[0]), (0, false));
    for other in [0, -1, 4174, 4180, i32::MAX] {
        assert!(!sort.press(other), "{other}");
    }
    assert_eq!(sort.key, 0);
}

#[test]
fn the_cells_are_centred_numbers_in_black_and_right_aligned_money_in_red_or_green() {
    let symbol = Symbol::new("$".into());
    let list = list(
        &[row(1, "a", "x", 2, 7, -1234.5), row(2, "b", "x", 4, 0, 8.0)],
        &symbol,
    );
    let (first, second) = (&list[0], &list[1]);
    assert_eq!(first.text, b"a");
    let cells = |r: &ListRow| {
        r.cells
            .iter()
            .map(|c| {
                (
                    String::from_utf8(c.text.clone()).unwrap(),
                    c.color,
                    c.justify,
                )
            })
            .collect::<Vec<_>>()
    };
    let black = Some([0, 0, 0]);
    let (red, green) = (Some([255, 0, 0]), Some([0, 255, 0]));
    let a = cells(first);
    assert_eq!(
        (a[0].0.as_str(), a[0].1, a[0].2),
        ("2", black, Justify::Center)
    );
    assert_eq!(
        (a[1].0.as_str(), a[1].1, a[1].2),
        ("7", black, Justify::Center)
    );
    assert_eq!((a[2].1, a[2].2), (red, Justify::Right));
    assert_eq!((a[3].1, a[3].2), (red, Justify::Right));
    let money = |v: f64| money_cents(&symbol, v, true);
    assert_eq!(
        (a[2].0.clone(), a[3].0.clone()),
        (money(-1234.5), money(-617.25))
    );
    let b = cells(second);
    assert_eq!((b[2].1, b[3].1), (green, green));
    assert_eq!((b[2].0.clone(), b[3].0.clone()), (money(8.0), money(2.0)));
}
