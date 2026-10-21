use super::*;

#[test]
fn months_open_round_up_from_the_days() {
    let day = DAY_MS;
    // Built today: one month, and for the first 29.9 days less a bit (round(days + 29.9) / 30).
    assert_eq!(months(0, 0), 1);
    assert_eq!(months(5 * day, 5 * day + day / 10), 1);
    assert_eq!(months(0, 29 * day), 1);
    // 30 days: round(59.9) = 60, two months; 60 days: round(89.9) = 90, three.
    assert_eq!(months(0, 30 * day), 2);
    assert_eq!(months(0, 59 * day), 2);
    assert_eq!(months(0, 60 * day), 3);
    // A building "built" after now, and a huge time, never panic or give less than 1.
    assert_eq!(months(10, 0), 1);
    assert!(months(0, u64::MAX) > 1);
}

#[test]
fn the_figures_are_rounded_before_the_difference_and_divided_whole() {
    let live = Live {
        income: [0.0, 0.0, 10.4],
        upkeep: 50.6,
        ..Live::default()
    };
    // 10 - 51 = -41; per month of 3: truncated towards zero.
    assert_eq!(figures(&live, 1), ([10, 51, -41], [10, 51, -41]));
    assert_eq!(figures(&live, 3), ([10, 51, -41], [3, 17, -13]));
    // Halves go to the even whole: 0.5 is 0, 1.5 and 2.5 are 2.
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
    assert_eq!([half(0.5), half(1.5), half(2.5)], [0, 2, 2]);
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
