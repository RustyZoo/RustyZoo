use super::*;

/// A bar 100 long: arrows of 10 and 12, a thumb of 8; the thumb travels 70.
const L: Lengths = Lengths {
    bar: 100,
    up: 10,
    down: 12,
    thumb: 8,
};

fn bar(min: i32, max: i32, position: i32) -> Bar {
    Bar {
        min,
        max,
        position,
        ..Bar::default()
    }
}

#[test]
fn the_thumb_slides_between_the_arrows_by_the_position() {
    assert_eq!(bar(0, 100, 0).thumb_start(&L), 10);
    assert_eq!(bar(0, 100, 100).thumb_start(&L), 80);
    assert_eq!(bar(0, 100, 50).thumb_start(&L), 45);
    // The division truncates; a range that does not start at 0 counts from its minimum.
    assert_eq!(bar(5, 12, 6).thumb_start(&L), 20);
    // Nothing to scroll: the thumb rests under the up arrow. No room: the same.
    assert_eq!(bar(3, 3, 3).thumb_start(&L), 10);
    let tight = Lengths { bar: 30, ..L };
    assert_eq!(bar(0, 9, 9).thumb_start(&tight), 10);
    assert_eq!(bar(0, 9, 9).thumb_start(&Lengths::default()), 0);
}

#[test]
fn a_point_along_the_bar_is_an_arrow_the_thumb_or_the_track() {
    let b = bar(0, 100, 50);
    let parts = |at| b.part_at(&L, at);
    assert_eq!(parts(-5), Part::Up);
    assert_eq!((parts(0), parts(9)), (Part::Up, Part::Up));
    assert_eq!((parts(10), parts(44)), (Part::Track, Part::Track));
    assert_eq!((parts(45), parts(52)), (Part::Thumb, Part::Thumb));
    assert_eq!((parts(53), parts(87)), (Part::Track, Part::Track));
    assert_eq!((parts(88), parts(99)), (Part::Down, Part::Down));
    assert_eq!(parts(5000), Part::Down);
}

#[test]
fn the_thumb_position_comes_back_from_its_start() {
    // With as many pixels of travel as positions, every position is its own thumb start.
    let b = bar(0, 70, 0);
    for p in 0..=70 {
        let at = Bar {
            position: p,
            ..b.clone()
        }
        .thumb_start(&L);
        assert_eq!(b.position_for(&L, at), i64::from(p));
    }
    // Past either end the thumb stops; a range with no travel stays at its minimum.
    assert_eq!(b.position_for(&L, -50), 0);
    assert_eq!(b.position_for(&L, 5000), 70);
    assert_eq!(bar(4, 9, 4).position_for(&Lengths::default(), 7), 4);
    // Rounded down, not to the nearest.
    assert_eq!(bar(0, 7, 0).position_for(&L, 10 + 19), 1);
    assert_eq!(bar(0, 7, 0).position_for(&L, 10 + 29), 2);
    assert_eq!(bar(0, 7, 0).position_for(&L, 10 + 30), 3);
    assert_eq!(bar(0, 7, 0).position_for(&L, 10 + 69), 6);
    assert_eq!(bar(0, 7, 0).position_for(&L, 10 + 70), 7);
    assert_eq!(bar(0, 7, 0).thumb_range(&L), (10, 80));
    assert_eq!(bar(0, 7, 0).thumb_range(&Lengths::default()), (0, 0));
    assert!(bar(0, 7, 0).scrollable() && !bar(3, 3, 3).scrollable());
}

#[test]
fn a_bar_stays_in_its_range_and_pages_toward_the_pointer() {
    let mut b = bar(0, 10, 3);
    assert!(b.step(1) && b.position == 4);
    assert!(!b.step(0));
    assert!(b.step(1000) && b.position == 10);
    assert!(!b.step(i64::MAX / 2), "already at the end");
    assert!(b.step(i64::MIN / 2) && b.position == 0);
    assert!(!b.set_position(i64::MIN));
    b.set_range(5, 2, 0);
    assert_eq!((b.min, b.max, b.page, b.position), (5, 5, 1, 5));
    b.set_range(0, 100, 10);
    b.position = 50;
    assert_eq!(b.page_toward(&L, 20), -10);
    assert_eq!(b.page_toward(&L, 60), 10);
}

fn rows(n: usize) -> Rows {
    Rows {
        rows: (0..n)
            .map(|i| Row {
                text: format!("row {i}").into_bytes(),
                ..Row::default()
            })
            .collect(),
        ..Rows::default()
    }
}

/// `n` rows of 12 pixels in a list that shows `visible` of them.
fn fit(n: usize, visible: i32) -> Heights {
    Heights {
        rows: vec![12; n],
        spacing: 0,
        inner: 12 * visible,
    }
}

#[test]
fn the_rows_that_fit_and_the_row_under_a_point() {
    let h = fit(10, 4);
    assert_eq!((h.fit_from(0), h.fit_from(8), h.max_top()), (4, 2, 6));
    assert_eq!(fit(3, 4).max_top(), 0, "everything fits");
    assert_eq!(fit(0, 4).max_top(), 0);
    assert_eq!(h.fit_from(99), 1, "at least one");
    assert_eq!(h.row_at(2, 0), Some(2));
    assert_eq!(h.row_at(2, 25), Some(4));
    assert_eq!(h.row_at(2, 12 * 8), None, "below the last row");
    assert_eq!(h.row_at(2, -1), None);
    assert_eq!(fit(0, 4).row_at(0, 0), None);
    // Rows of other heights, with 2 pixels between two of them: 30 tall inside.
    let h = Heights {
        rows: vec![10, 20, 8, 8, 40],
        spacing: 2,
        inner: 30,
    };
    assert_eq!((h.fit_from(0), h.fit_from(1), h.fit_from(4)), (1, 2, 1));
    assert_eq!(h.max_top(), 4, "the last row alone is taller than the list");
    assert_eq!(h.row_at(0, 9), Some(0));
    assert_eq!(h.row_at(0, 10), None, "between two rows");
    assert_eq!(h.row_at(0, 12), Some(1));
    assert_eq!(h.row_at(0, 33), None);
    assert_eq!(h.row_at(0, 34), Some(2));
    assert_eq!(h.row_at(1, 21), None);
    assert_eq!(h.row_at(1, 22), Some(2));
    let wide = Heights {
        rows: vec![20, 20],
        spacing: 0,
        inner: 41,
    };
    assert_eq!((wide.fit_from(0), wide.max_top()), (2, 0));
    let huge = Heights {
        rows: vec![i32::MAX; 3],
        spacing: i32::MAX,
        inner: i32::MAX,
    };
    assert_eq!((huge.fit_from(0), huge.max_top()), (1, 2));
    assert_eq!(huge.row_at(0, i32::MAX), None);
}

#[test]
fn the_keys_move_the_selection_and_keep_it_in_view() {
    let mut r = rows(10);
    let h = fit(10, 4);
    // Nothing selected: Down and Home start at the first row, Up and End at the last.
    assert!(r.select_by_key(ListKey::Down, &h) && r.selected == Some(0));
    r.selected = None;
    assert!(r.select_by_key(ListKey::Up, &h) && r.selected == Some(9));
    assert_eq!(r.top, 6, "scrolled to show it");
    assert!(r.select_by_key(ListKey::Home, &h) && r.selected == Some(0) && r.top == 0);
    assert!(!r.select_by_key(ListKey::Up, &h), "already first");
    // A page is the visible rows less one.
    assert!(r.select_by_key(ListKey::PageDown, &h) && r.selected == Some(3));
    assert!(r.select_by_key(ListKey::PageDown, &h) && r.selected == Some(6) && r.top == 3);
    assert!(r.select_by_key(ListKey::PageUp, &h) && r.selected == Some(3));
    assert!(r.select_by_key(ListKey::End, &h) && r.selected == Some(9));
    assert!(!r.select_by_key(ListKey::Down, &h));
    // One visible row, no rows.
    assert!(r.select_by_key(ListKey::PageUp, &fit(10, 1)) && r.selected == Some(8));
    assert!(!rows(0).select_by_key(ListKey::Down, &h));
    assert!(!rows(0).select_by_key(ListKey::End, &fit(0, 0)));
}

#[test]
fn the_select_action_gets_the_row_or_minus_one() {
    let mut r = rows(3);
    assert_eq!(r.action_value(), -1);
    r.selected = Some(2);
    assert_eq!(r.action_value(), 2);
    r.rows[2].disabled = true;
    assert_eq!(r.action_value(), -1);
    r.selected = Some(7);
    assert_eq!(r.action_value(), -1, "a row that is not there");
}

fn typed(
    text: &str,
    caret: Caret,
    key: EditKey,
    shift: bool,
    limits: Limits,
) -> (String, Caret, Edited) {
    let mut bytes = text.as_bytes().to_vec();
    let mut caret = caret;
    let done = edit(&mut bytes, &mut caret, key, shift, limits);
    (String::from_utf8_lossy(&bytes).into_owned(), caret, done)
}

fn at(at: usize, anchor: usize) -> Caret {
    Caret { at, anchor }
}

const FREE: Limits = Limits {
    numeric: false,
    limit: 0,
};

#[test]
fn the_arrows_move_the_caret_and_shift_extends_the_selection() {
    let go = |c, k, s| typed("abcd", c, k, s, FREE);
    assert_eq!(
        go(at(2, 2), EditKey::Left, false),
        ("abcd".into(), at(1, 1), Edited::Moved)
    );
    assert_eq!(go(at(0, 0), EditKey::Left, false).1, at(0, 0));
    assert_eq!(go(at(4, 4), EditKey::Right, false).1, at(4, 4));
    assert_eq!(go(at(2, 2), EditKey::Right, true).1, at(3, 2));
    assert_eq!(go(at(3, 2), EditKey::Right, true).1, at(4, 2));
    assert_eq!(go(at(3, 2), EditKey::Left, true).1, at(2, 2));
    // Without Shift a selection collapses to the edge in the direction of the arrow.
    assert_eq!(go(at(1, 3), EditKey::Right, false).1, at(3, 3));
    assert_eq!(go(at(3, 1), EditKey::Left, false).1, at(1, 1));
    assert_eq!(go(at(2, 2), EditKey::Home, false).1, at(0, 0));
    assert_eq!(go(at(2, 2), EditKey::End, true).1, at(4, 2));
    // A caret past the end (the text was replaced) is brought back.
    assert_eq!(go(at(9, 9), EditKey::Left, false).1, at(3, 3));
    assert_eq!(go(at(9, 9), EditKey::Right, false).1, at(4, 4));
}

#[test]
fn delete_and_backspace_remove_the_selection_or_one_byte() {
    let go = |c, k| typed("abcd", c, k, false, FREE);
    assert_eq!(
        go(at(2, 2), EditKey::Backspace),
        ("acd".into(), at(1, 1), Edited::Changed)
    );
    assert_eq!(
        go(at(2, 2), EditKey::Delete),
        ("abd".into(), at(2, 2), Edited::Changed)
    );
    assert_eq!(
        go(at(1, 3), EditKey::Backspace),
        ("ad".into(), at(1, 1), Edited::Changed)
    );
    assert_eq!(
        go(at(3, 1), EditKey::Delete),
        ("ad".into(), at(1, 1), Edited::Changed)
    );
    // At the ends there is nothing to remove.
    assert_eq!(go(at(0, 0), EditKey::Backspace).2, Edited::Moved);
    assert_eq!(go(at(4, 4), EditKey::Delete).0, "abcd");
    assert_eq!(typed("", at(0, 0), EditKey::Delete, false, FREE).0, "");
}

#[test]
fn a_typed_byte_replaces_the_selection_within_the_limits() {
    let put = |t, c, b, l| typed(t, c, EditKey::Char(b), false, l);
    assert_eq!(
        put("ac", at(1, 1), b'b', FREE),
        ("abc".into(), at(2, 2), Edited::Changed)
    );
    assert_eq!(
        put("abc", at(0, 2), b'x', FREE),
        ("xc".into(), at(1, 1), Edited::Changed)
    );
    // Control characters and DEL are refused, the accented bytes of the code page are not.
    assert_eq!(put("a", at(1, 1), 7, FREE).2, Edited::Ignored);
    assert_eq!(put("a", at(1, 1), 127, FREE).2, Edited::Ignored);
    assert_eq!(put("a", at(1, 1), 233, FREE).2, Edited::Changed);
    let digits = Limits {
        numeric: true,
        limit: 0,
    };
    assert_eq!(put("1", at(1, 1), b'x', digits).2, Edited::Ignored);
    assert_eq!(put("1", at(1, 1), b'2', digits).0, "12");
    let two = Limits {
        numeric: false,
        limit: 2,
    };
    assert_eq!(put("ab", at(2, 2), b'c', two).2, Edited::Ignored);
    assert_eq!(
        put("ab", at(0, 2), b'c', two).0,
        "c",
        "the selection makes room"
    );
    assert_eq!(put("a", at(1, 1), b'b', two).0, "ab");
    let none = Limits {
        numeric: false,
        limit: 0,
    };
    assert_eq!(put("abc", at(3, 3), b'd', none).0, "abcd", "0 is no limit");
    // A text already over the limit (set by the code) loses nothing but takes nothing.
    assert_eq!(put("abc", at(3, 3), b'd', two).2, Edited::Ignored);
}

#[test]
fn a_click_puts_the_caret_on_the_nearest_boundary() {
    let w = |b: u8| if b == b'i' { 2 } else { 6 };
    let t = b"aib";
    // Boundaries at 0, 6, 8, 14; midpoints 3, 7, 11.
    assert_eq!(caret_for(t, w, -20), 0);
    assert_eq!(caret_for(t, w, 2), 0);
    assert_eq!(caret_for(t, w, 3), 1);
    assert_eq!(caret_for(t, w, 6), 1);
    assert_eq!(caret_for(t, w, 7), 2);
    assert_eq!(caret_for(t, w, 11), 3);
    assert_eq!(caret_for(t, w, i32::MAX), 3);
    assert_eq!(caret_for(b"", w, 5), 0);
}

#[test]
fn a_numeric_field_takes_a_leading_minus_and_one_point() {
    let digits = Limits {
        numeric: true,
        limit: 0,
    };
    let put = |t, c, b| typed(t, c, EditKey::Char(b), false, digits);
    assert_eq!(put("5", at(0, 0), b'-').0, "-5");
    assert_eq!(
        put("5", at(1, 1), b'-').2,
        Edited::Ignored,
        "not in the middle"
    );
    assert_eq!(put("-5", at(0, 0), b'-').2, Edited::Ignored, "one minus");
    assert_eq!(
        put("-5", at(0, 0), b'7').2,
        Edited::Ignored,
        "nothing before a minus"
    );
    assert_eq!(
        put("-5", at(0, 2), b'-').0,
        "-",
        "a selection that holds it makes room"
    );
    assert_eq!(put("12", at(1, 1), b'.').0, "1.2");
    assert_eq!(put("1.2", at(0, 0), b'.').2, Edited::Ignored, "one point");
    assert_eq!(
        put("1.2", at(0, 3), b'.').0,
        ".",
        "a selection that holds it makes room"
    );
    assert_eq!(put("", at(0, 0), b',').2, Edited::Ignored);
    assert_eq!(put("-5", at(0, 0), b'.').2, Edited::Ignored);
    // A field that is not numeric takes them all.
    assert_eq!(
        typed("a", at(1, 1), EditKey::Char(b'-'), false, FREE).0,
        "a-"
    );
}
