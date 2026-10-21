use super::*;

#[test]
fn months_open_are_the_whole_days_plus_29_over_30() {
    let day = DAY_MS;
    // Built today: one month, and until day 30; day 31 is the second month (guest buildings 10.3).
    assert_eq!(months(0, 0), 1);
    assert_eq!(months(5 * day, 5 * day + day / 10), 1);
    assert_eq!(months(0, 30 * day), 1);
    assert_eq!(months(0, 31 * day - 1), 1);
    assert_eq!(months(0, 31 * day), 2);
    assert_eq!(months(0, 60 * day), 2);
    assert_eq!(months(0, 61 * day), 3);
    // A building "built" after now, and a huge time, never panic or give less than 1.
    assert_eq!(months(10, 0), 1);
    assert!(months(0, u64::MAX) > 1);
}

#[test]
fn the_figures_are_cut_toward_zero_and_divided_whole() {
    let live = Live {
        income: [0.0, 0.0, 10.9],
        upkeep: 50.6,
        ..Live::default()
    };
    // 10 - 50 = -40; per month of 3: truncated towards zero.
    assert_eq!(figures(&live, 1), ([10, 50, -40], [10, 50, -40]));
    assert_eq!(figures(&live, 3), ([10, 50, -40], [3, 16, -13]));
    // Halves are cut too: 0.5 is 0, 1.5 is 1, 3.5 is 3.
    let half = |v: f32| {
        figures(
            &Live {
                income: [0.0, 0.0, v],
                ..Live::default()
            },
            1,
        )
        .0[0]
    };
    assert_eq!([half(0.5), half(1.5), half(2.5), half(3.5)], [0, 1, 2, 3]);
    // Not numbers and huge values give no panic.
    for v in [f32::NAN, f32::INFINITY, f32::MIN, f32::MAX] {
        figures(
            &Live {
                income: [0.0, 0.0, v],
                upkeep: v,
                ..Live::default()
            },
            1,
        );
    }
}

#[test]
fn the_typed_text_loses_the_first_symbol_and_separator_and_reads_as_atof() {
    let t = |text: &str| typed(text.as_bytes(), b"$");
    assert_eq!([t("$4.25"), t("  4.25"), t("4.25$")], [4.25; 3]);
    // Only the first symbol and the first separator are cut: the second ones end the number.
    assert_eq!([t("$1,234.5"), t("1,2,3"), t("$$5")], [1234.5, 12.0, 0.0]);
    // No number, nothing, a symbol alone and a huge number.
    assert_eq!([t("abc"), t(""), t("$")], [0.0; 3]);
    assert!(t("1e999").is_infinite());
    // An empty symbol cuts nothing.
    assert_eq!(typed(b"$5", b""), 0.0);
}
