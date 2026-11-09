use super::*;
use crate::config::Config;

fn graph(keys: &str) -> Graph {
    let config = Config::parse(format!("[g]\n{keys}").as_bytes()).unwrap();
    Graph::read(config.section("g").unwrap())
}

fn with(points: &[i32], keys: &str) -> Graph {
    let mut g = graph(keys);
    g.points = points
        .iter()
        .enumerate()
        .map(|(x, &y)| Point {
            x: x as i32,
            y,
            label: Vec::new(),
        })
        .collect();
    g
}

#[test]
fn the_top_of_the_axis_goes_up_by_the_leading_digit() {
    let up = |v| super::up(v);
    assert_eq!([up(0), up(9), up(-1), up(-9)], [10, 10, 0, 0]);
    assert_eq!([up(10), up(57), up(99), up(100), up(999)], [20, 60, 100, 200, 1000]);
    assert_eq!(up(1234), 2000);
}

#[test]
fn the_bottom_of_the_axis_keeps_two_digits_below_minus_100_and_rounds_toward_zero_above() {
    let down = |v| super::down(v);
    assert_eq!([down(0), down(9), down(-1), down(-9)], [0, 0, -10, -10]);
    // The original's rounding: the lowest point is drawn below the axis.
    assert_eq!([down(-10), down(-57), down(-99)], [-10, -50, -90]);
    assert_eq!([down(-100), down(-1234), down(-999)], [-110, -1300, -1000]);
    assert_eq!(down(-1000), -1100);
}

#[test]
fn the_extremes_of_the_integers_do_not_overflow() {
    assert_eq!(up(i64::from(i32::MAX)), 3_000_000_000);
    assert_eq!(down(i64::from(i32::MIN)), -2_200_000_000);
}

#[test]
fn zero_is_always_inside_the_range_and_a_forced_bound_stands() {
    assert_eq!(with(&[], "").range(), (0, 10));
    assert_eq!(with(&[57, 30], "").range(), (0, 60));
    // Zero is a point of the graph, so the top is the one of 0.
    assert_eq!(with(&[-57, -30], "").range(), (-50, 10));
    assert_eq!(with(&[-2756, 123_351], "").range(), (-2800, 200_000));
    assert_eq!(with(&[57], "ForcedMinY=0\nForcedMaxY=100\n").range(), (0, 100));
    assert_eq!(with(&[-57], "ForcedMinY=0\n").range(), (0, 10));
    // -1 is not forced.
    assert_eq!(with(&[57], "ForcedMinY=-1\nForcedMaxY=-1\n").range(), (0, 60));
}

#[test]
fn the_labels_of_the_rating_graph_run_from_100_to_0() {
    let g = with(&[50], "ForcedMinY=0\nForcedMaxY=100\nNumYLabels=10\n");
    assert_eq!(g.y_values(), [100, 88, 77, 66, 55, 44, 33, 22, 11, 0]);
}

#[test]
fn the_label_step_is_at_least_one_and_a_count_of_0_or_1_is_safe() {
    let g = with(&[], "ForcedMinY=0\nForcedMaxY=3\nNumYLabels=5\n");
    assert_eq!(g.y_values(), [3, 2, 1, 0, -1]);
    for keys in ["NumYLabels=0\n", "NumYLabels=1\n", "NumYLabels=-4\n", "NumYLabels=99999999\n"] {
        let n = with(&[5], keys).y_values().len();
        assert!(n <= 1000, "{keys}: {n}");
    }
    assert_eq!(with(&[5], "NumYLabels=1\n").y_values(), [10]);
    assert!(with(&[5], "NumYLabels=0\n").y_values().is_empty());
}

#[test]
fn a_value_is_placed_from_the_bottom_less_the_padding() {
    let g = with(&[0, 100], "ForcedMinY=0\nForcedMaxY=100\nVertPaddingPixels=5\n");
    // The plot spans rows 0 to 100: 100 is at the top padding, 0 at the bottom padding.
    assert_eq!(g.y_at(100, (0, 100)), 5);
    assert_eq!(g.y_at(0, (0, 100)), 95);
    // Every product is cut toward zero.
    assert_eq!(g.y_at(50, (0, 100)), 50);
    assert_eq!(g.y_at(33, (0, 100)), 95 - 29);
}

#[test]
fn an_empty_range_puts_everything_on_the_bottom() {
    let g = with(&[0], "ForcedMinY=0\nForcedMaxY=0\n");
    assert_eq!(g.y_at(7, (0, 100)), 95);
}

#[test]
fn a_line_graph_spreads_the_points_between_the_paddings() {
    let g = with(&[1, 2, 3], "HorizPaddingPixels=20\n");
    let xs: Vec<i32> = (0..3).map(|i| g.x_at_line(i, (10, 210))).collect();
    assert_eq!(xs, [30, 110, 190]);
    // One point sits on the left padding.
    assert_eq!(with(&[1], "").x_at_line(0, (10, 210)), 30);
    assert_eq!(with(&[], "").x_at_line(0, (10, 210)), 30);
}

#[test]
fn a_bar_graph_takes_a_step_of_the_room_cut_to_the_widest_bar() {
    let g = with(&[1; 12], "HorizPaddingPixels=20\nBarSpacing=10\nMaxBarWidth=50\n");
    // (10 + 460 - 40) / 12 = 35.
    assert_eq!(g.bar_at(0, (0, 460)), (35, 20));
    assert_eq!(g.bar_at(2, (0, 460)), (35, 90));
    assert_eq!(g.bar_width((0, 460)), 460 / 12 - 10);
    let few = with(&[1, 2], "MaxBarWidth=50\n");
    assert_eq!(few.bar_at(1, (0, 460)).0, 50);
    // A room below 0 does not wrap around.
    assert_eq!(with(&[1], "").bar_at(0, (0, 5)).0, 0);
}

#[test]
fn the_layout_keys_are_read_and_a_missing_key_keeps_the_shipped_value() {
    let g = graph("graphType=b\nyIsMoney=1\nLineColor=1\nLineColor=2\nLineColor=3\nForcedMinY=0\n");
    assert_eq!(g.style, Style::Bar);
    assert!(g.y_is_money);
    assert_eq!(g.line, [1, 2, 3]);
    assert_eq!((g.forced_min, g.forced_max), (0, -1));
    assert_eq!((g.vert_padding, g.horiz_padding, g.y_labels), (5, 20, 10));
    assert_eq!(graph("graphType=p\n").style, Style::Point);
    assert_eq!(graph("graphType=l\n").style, Style::Line);
    assert_eq!(graph("").style, Style::Line);
    // A colour written fewer than three times is black.
    assert_eq!(graph("LineColor=9\nLineColor=9\n").line, [0, 0, 0]);
}
