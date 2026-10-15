use super::*;
use crate::Framebuffer;
use crate::app::Mouse;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{HIDDEN, Picture, Tree};
use crate::vfs::Vfs;

struct NoTexts;

impl Strings for NoTexts {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// A TGA image of one colour.
fn solid(w: u16, h: u16, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    out.extend(w.to_le_bytes());
    out.extend(h.to_le_bytes());
    out.extend([24, 0x20]);
    for _ in 0..usize::from(w) * usize::from(h) {
        out.extend([rgb[2], rgb[1], rgb[0]]);
    }
    out
}

const BAR: &str = "type=UIScrollBar\ndx=8\nupArrow=ui/up.tga\ndownArrow=ui/down.tga\n\
                   thumb=ui/thumb.tga\n";

/// A window of 100 x 80 pixels. Layout 10 holds a list 20 (40 x 36, three rows of 12 pixels)
/// with its bar 21 (arrows and thumb of 8 pixels, so the thumb travels 12 pixels of the 36), a
/// text field 30 (at most 5 characters), a digits-only field 31, a read-only list 22 with the
/// bar 23, a bar 40 with no owner, and a text 50 of three lines with its bar 51.
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/panel.lyt\n";
    let panel = format!(
        "[LayoutInfo]\nid=10\nlayer=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
         [a]\ntype=UIListBox\nid=20\nx=0\ny=0\ndx=40\ndy=36\nscrollbar=21\n\
         [b]\nid=21\nx=77\ny=77\n{BAR}\
         [c]\ntype=UIEditableText\nid=30\nx=50\ny=0\ndx=40\ndy=14\ncharlimit=5\ntransparent=0\n\
         [d]\ntype=UIEditableText\nid=31\nx=50\ny=20\ndx=40\ndy=14\nnumeric=1\ntransparent=0\n\
         [e]\ntype=UIListBox\nid=22\nx=50\ny=40\ndx=20\ndy=24\nstate=262144\nscrollbar=23\n\
         [f]\nid=23\n{BAR}\
         [g]\nid=40\nx=0\ny=40\n{BAR}\
         [h]\ntype=UIText\nid=50\nx=60\ny=66\ndx=24\ndy=14\nscrollbar=51\nwrappable=1\n\
         [i]\nid=51\n{BAR}"
    );
    let install = Install::new(
        &format!("widgets-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/panel.lyt", panel.into_bytes()),
            ("ui/up.tga", solid(8, 8, [255, 0, 0])),
            ("ui/down.tga", solid(8, 8, [0, 255, 0])),
            ("ui/thumb.tga", solid(8, 8, [0, 0, 255])),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    assert_eq!(tree.warnings, Vec::<String>::new());
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    ui.show(10);
    ui.take_events();
    ui
}

/// An interface update 33 ms after the last one.
fn tick(ui: &mut Interface, now: &mut u64) {
    *now += 33;
    ui.update(*now);
}

fn rows(n: usize) -> Vec<Row> {
    (0..n)
        .map(|i| Row {
            text: format!("row {i}").into_bytes(),
            ..Row::default()
        })
        .collect()
}

fn at(ui: &mut Interface, x: i32, y: i32) {
    ui.mouse(Mouse::Move(x, y));
}

fn press(ui: &mut Interface, x: i32, y: i32) -> Vec<String> {
    at(ui, x, y);
    ui.take_events();
    ui.mouse(Mouse::Down);
    ui.take_events()
}

fn release(ui: &mut Interface) -> Vec<String> {
    ui.mouse(Mouse::Up);
    ui.take_events()
}

fn click(ui: &mut Interface, x: i32, y: i32) -> Vec<String> {
    let mut events = press(ui, x, y);
    events.extend(release(ui));
    events
}

fn position(ui: &Interface, id: i32) -> i32 {
    match &ui.tree.elements[ui.tree.find(id).unwrap()].kind {
        Kind::ScrollBar(s) => s.bar.position,
        _ => panic!("not a bar"),
    }
}

fn top(ui: &Interface, id: i32) -> usize {
    match &ui.tree.elements[ui.tree.find(id).unwrap()].kind {
        Kind::ListBox(l) => l.rows.top,
        Kind::Text(t) => t.top,
        _ => panic!("no top"),
    }
}

fn state(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

fn text(ui: &Interface, id: i32) -> String {
    String::from_utf8(ui.text(id).unwrap()).unwrap()
}

#[test]
fn a_bar_is_placed_by_its_owner_whatever_its_own_position_says() {
    let ui = interface("placed");
    let rect = |id| ui.tree.rect(ui.tree.find(id).unwrap());
    // Right of the list, as long as it.
    assert_eq!(rect(21), (40, 0, 8, 36));
    assert_eq!(rect(23), (70, 40, 8, 24));
    // A bar with no owner keeps its place.
    assert_eq!(rect(40), (0, 40, 8, 0));
}

#[test]
fn a_bar_follows_the_rows_of_its_list() {
    let mut ui = interface("range");
    let range = |ui: &Interface| match &ui.tree.elements[ui.tree.find(21).unwrap()].kind {
        Kind::ScrollBar(s) => (s.bar.min, s.bar.max, s.bar.page, s.bar.position),
        _ => unreachable!(),
    };
    assert_eq!(range(&ui), (0, 0, 1, 0), "no rows, nothing to scroll");
    assert!(ui.set_rows(20, rows(10)));
    // Three rows show: seven are beyond; a page is the rows shown less one.
    assert_eq!(range(&ui), (0, 7, 2, 0));
    assert!(ui.set_rows(20, rows(2)));
    assert_eq!(range(&ui), (0, 0, 1, 0), "both rows show");
    assert!(!ui.set_rows(21, rows(1)), "not a list");
    assert!(!ui.set_rows(999, rows(1)));
}

#[test]
fn the_arrows_step_by_one_and_repeat_while_held_on_the_same_part() {
    let mut ui = interface("arrows");
    let mut now = 0u64;
    ui.set_rows(20, rows(10));
    // The down arrow is the last 8 pixels of the bar (x 40 to 47, y 28 to 35).
    let events = press(&mut ui, 44, 30);
    assert_eq!(events, ["scroll 21 1"]);
    assert_eq!((position(&ui, 21), top(&ui, 20)), (1, 1));
    assert_eq!(ui.take_actions(), [Action::Scroll(21, 1)]);
    // The first repeat comes 500 ms after the press, then every 30 ms; the update runs every 33.
    for _ in 0..15 {
        tick(&mut ui, &mut now);
    }
    assert_eq!(position(&ui, 21), 1, "495 ms");
    tick(&mut ui, &mut now);
    assert_eq!(position(&ui, 21), 2, "528 ms");
    tick(&mut ui, &mut now);
    assert_eq!(position(&ui, 21), 3);
    // Off the part (onto the track): no step, and it keeps looking.
    at(&mut ui, 44, 15);
    tick(&mut ui, &mut now);
    tick(&mut ui, &mut now);
    assert_eq!(position(&ui, 21), 3);
    // Back on it: the steps go on. Released: they stop.
    at(&mut ui, 44, 30);
    tick(&mut ui, &mut now);
    assert_eq!(position(&ui, 21), 4);
    release(&mut ui);
    tick(&mut ui, &mut now);
    tick(&mut ui, &mut now);
    assert_eq!(position(&ui, 21), 4);
    // The up arrow goes back, and stops at the first row.
    for _ in 0..6 {
        click(&mut ui, 44, 3);
    }
    assert_eq!((position(&ui, 21), top(&ui, 20)), (0, 0));
}

#[test]
fn the_end_of_the_range_ends_a_step_and_says_nothing() {
    let mut ui = interface("end");
    ui.set_rows(20, rows(10));
    assert!(click(&mut ui, 44, 3).is_empty(), "already at the first row");
    ui.take_actions();
    assert!(ui.take_actions().is_empty());
}

#[test]
fn a_press_on_the_track_moves_a_page_toward_the_pointer_until_the_thumb_is_there() {
    let mut ui = interface("track");
    let mut now = 0u64;
    ui.set_rows(20, rows(10));
    // The thumb starts under the up arrow (y 8 to 15); a press below it pages down by two.
    assert_eq!(press(&mut ui, 44, 25), ["scroll 21 2"]);
    // The thumb has moved to y 8 + 12 * 2 / 7 = 11, to 18. Held there, the track below keeps
    // paging; the pointer at y 14 is now over the thumb, so the paging stops.
    at(&mut ui, 44, 14);
    for _ in 0..40 {
        tick(&mut ui, &mut now);
    }
    assert_eq!(position(&ui, 21), 2);
    release(&mut ui);
    // A press above the thumb pages back up.
    assert_eq!(press(&mut ui, 44, 9), ["scroll 21 0"]);
}

#[test]
fn the_thumb_follows_the_pointer_where_it_was_grabbed() {
    let mut ui = interface("thumb");
    ui.set_rows(20, rows(10));
    // Grab the thumb 4 pixels from its start (y 8) and pull down by 6 pixels: it starts at 14,
    // 6 of 12 pixels of travel, 3.5 of 7 rows, rounded down.
    assert!(
        press(&mut ui, 44, 12).is_empty(),
        "a press on the thumb moves nothing"
    );
    at(&mut ui, 44, 18);
    assert_eq!(ui.take_events(), ["scroll 21 3"]);
    assert_eq!(top(&ui, 20), 3);
    // Further than the bar: the end; the pointer may leave the bar sideways.
    at(&mut ui, 90, 500);
    assert_eq!(position(&ui, 21), 7);
    at(&mut ui, 90, -500);
    assert_eq!(position(&ui, 21), 0);
    release(&mut ui);
    at(&mut ui, 44, 30);
    assert_eq!(position(&ui, 21), 0, "no more drag");
}

#[test]
fn a_bar_with_no_owner_only_tells_its_position() {
    let mut ui = interface("ownerless");
    // Its own length is 0: nothing to hit, so give it some.
    let i = ui.tree.find(40).unwrap();
    ui.tree.elements[i].placed.as_mut().unwrap().dy = 40;
    // Range 0 to 100 by default, page 10; the track below the thumb pages down.
    assert_eq!(press(&mut ui, 4, 60), ["scroll 40 10"]);
    assert_eq!(ui.take_actions(), [Action::Scroll(40, 10)]);
}

#[test]
fn the_wheel_scrolls_two_rows_a_notch_when_there_is_more_than_shown() {
    let mut ui = interface("wheel");
    ui.set_rows(20, rows(10));
    at(&mut ui, 5, 5);
    ui.take_events();
    ui.mouse(Mouse::Wheel(-1));
    assert_eq!((top(&ui, 20), position(&ui, 21)), (2, 2));
    assert_eq!(ui.take_events(), ["scroll 21 2"]);
    ui.mouse(Mouse::Wheel(-100));
    assert_eq!(top(&ui, 20), 7, "to the last screenful");
    ui.mouse(Mouse::Wheel(1));
    assert_eq!(top(&ui, 20), 5);
    ui.mouse(Mouse::Wheel(i32::MAX));
    ui.mouse(Mouse::Wheel(i32::MIN));
    assert_eq!(top(&ui, 20), 7);
    ui.mouse(Mouse::Wheel(0));
    assert_eq!(top(&ui, 20), 7);
    // Three rows fit: the wheel does nothing.
    ui.set_rows(20, rows(3));
    ui.take_events();
    ui.mouse(Mouse::Wheel(-1));
    assert_eq!(top(&ui, 20), 0);
    assert!(ui.take_events().is_empty());
}

#[test]
fn the_wheel_scrolls_a_text_by_its_bar_and_ignores_other_elements() {
    let mut ui = interface("textwheel");
    // Four lines of a text 24 wide and 14 high (one line of 12 shows).
    ui.set_text(50, b"a\nb\nc\nd");
    at(&mut ui, 62, 70);
    ui.take_events();
    ui.mouse(Mouse::Wheel(-1));
    assert_eq!((top(&ui, 50), position(&ui, 51)), (2, 2));
    // Over a field, or over nothing: no scroll.
    at(&mut ui, 60, 5);
    ui.mouse(Mouse::Wheel(-1));
    assert_eq!(top(&ui, 50), 2);
}

#[test]
fn pressing_a_row_selects_it_and_releasing_runs_the_select_action() {
    let mut ui = interface("rows");
    ui.set_rows(20, rows(10));
    assert_eq!(press(&mut ui, 10, 14), ["focus 20", "row 20 1"]);
    assert_eq!(release(&mut ui), ["select 20 1"]);
    assert_eq!(ui.take_actions(), [Action::Row(20, 1)]);
    assert_eq!(ui.selected_row(20), Some(1));
    // The selection follows the pointer while held; the action runs at the release.
    press(&mut ui, 10, 3);
    at(&mut ui, 10, 30);
    at(&mut ui, 10, 25);
    assert_eq!(ui.take_events(), ["row 20 2"]);
    assert_eq!(release(&mut ui), ["select 20 2"]);
    // Below the last row: none, and the action says -1. Rows scrolled out of sight count.
    ui.set_rows(20, rows(2));
    assert_eq!(press(&mut ui, 10, 30), Vec::<String>::new());
    assert_eq!(release(&mut ui), ["select 20 -1"]);
    ui.set_rows(20, rows(10));
    ui.mouse(Mouse::Wheel(-1));
    assert_eq!(click(&mut ui, 10, 1), ["row 20 2", "select 20 2"]);
}

#[test]
fn a_disabled_row_is_selected_but_the_action_says_minus_one() {
    let mut ui = interface("disabled");
    let mut list = rows(3);
    list[1].disabled = true;
    ui.set_rows(20, list);
    assert_eq!(
        click(&mut ui, 10, 14),
        ["focus 20", "row 20 1", "select 20 -1"]
    );
}

#[test]
fn a_read_only_list_takes_no_mouse_and_no_hover() {
    let mut ui = interface("readonly");
    ui.set_rows(22, rows(5));
    at(&mut ui, 55, 45);
    let hover = match &ui.tree.elements[ui.tree.find(22).unwrap()].kind {
        Kind::ListBox(l) => l.rows.hover,
        _ => unreachable!(),
    };
    assert_eq!(hover, None);
    let events = click(&mut ui, 55, 45);
    assert!(
        events
            .iter()
            .all(|e| !e.starts_with("row ") && !e.starts_with("select "))
    );
    assert_eq!(ui.selected_row(22), None);
    assert!(!ui.focus(22), "it takes no focus either");
}

#[test]
fn the_keys_of_a_focused_list_move_the_selection() {
    let mut ui = interface("listkeys");
    ui.set_rows(20, rows(10));
    // Not focused: the keys are not for the list.
    assert!(!ui.key(Key::Down, false));
    click(&mut ui, 10, 3);
    ui.take_events();
    ui.take_actions();
    assert_eq!(state(&ui, 20) & FOCUSED, FOCUSED);
    assert!(ui.key(Key::Down, false));
    assert_eq!(ui.take_events(), ["row 20 1"]);
    assert!(
        ui.take_actions().is_empty(),
        "no select action without state 131072"
    );
    assert!(ui.key(Key::End, false));
    assert_eq!((ui.selected_row(20), top(&ui, 20)), (Some(9), 7));
    assert_eq!(ui.take_events(), ["row 20 9", "scroll 21 7"]);
    ui.take_actions();
    assert!(ui.key(Key::PageUp, false));
    assert_eq!((ui.selected_row(20), top(&ui, 20)), (Some(7), 7));
    assert!(!ui.key(Key::Tab, false));
    // With state 131072 each key also runs the select action.
    let i = ui.tree.find(20).unwrap();
    ui.tree.elements[i].state |= SELECT_ON_KEYS;
    ui.take_events();
    assert!(ui.key(Key::Home, false));
    assert_eq!(ui.take_events(), ["row 20 0", "scroll 21 0", "select 20 0"]);
    assert_eq!(
        ui.take_actions(),
        [Action::Scroll(21, 0), Action::Row(20, 0)]
    );
    // Even a key that changes nothing runs it.
    assert!(ui.key(Key::Up, false));
    assert_eq!(ui.take_events(), ["select 20 0"]);
}

#[test]
fn programmatic_selection_shows_the_row_and_runs_nothing() {
    let mut ui = interface("select");
    ui.set_rows(20, rows(10));
    assert!(ui.select_row(20, Some(8)));
    assert_eq!(
        (ui.selected_row(20), top(&ui, 20), position(&ui, 21)),
        (Some(8), 6, 6)
    );
    assert!(!ui.select_row(20, Some(10)));
    assert!(ui.select_row(20, None));
    assert_eq!(ui.selected_row(20), None);
    assert!(ui.take_actions().is_empty());
}

#[test]
fn the_hovered_part_of_a_bar_and_row_of_a_list_are_tracked() {
    let mut ui = interface("hover");
    ui.set_rows(20, rows(10));
    let parts = |ui: &Interface| match &ui.tree.elements[ui.tree.find(21).unwrap()].kind {
        Kind::ScrollBar(s) => (s.bar.hover, s.bar.held),
        _ => unreachable!(),
    };
    let row = |ui: &Interface| match &ui.tree.elements[ui.tree.find(20).unwrap()].kind {
        Kind::ListBox(l) => l.rows.hover,
        _ => unreachable!(),
    };
    at(&mut ui, 44, 3);
    assert_eq!(parts(&ui), (Some(Part::Up), None));
    at(&mut ui, 44, 12);
    assert_eq!(parts(&ui).0, Some(Part::Thumb));
    at(&mut ui, 44, 20);
    assert_eq!(parts(&ui).0, Some(Part::Track));
    press(&mut ui, 44, 30);
    assert_eq!(parts(&ui), (Some(Part::Down), Some(Part::Down)));
    release(&mut ui);
    assert_eq!(parts(&ui).1, None);
    at(&mut ui, 95, 70);
    assert_eq!(parts(&ui).0, None, "the pointer left the bar");
    // The press on the down arrow scrolled by one row.
    at(&mut ui, 10, 27);
    assert_eq!(row(&ui), Some(3));
    ui.mouse(Mouse::Wheel(-1));
    at(&mut ui, 10, 35);
    assert_eq!(row(&ui), Some(5), "two rows further down");
    at(&mut ui, 90, 70);
    assert_eq!(row(&ui), None);
}

#[test]
fn a_click_focuses_a_field_and_types_with_its_limits() {
    let mut ui = interface("field");
    assert_eq!(click(&mut ui, 55, 5), ["focus 30"]);
    assert_eq!(state(&ui, 30) & FOCUSED, FOCUSED);
    for byte in *b"abcdefg" {
        assert!(ui.char(byte));
    }
    assert_eq!(text(&ui, 30), "abcde", "at most five");
    let events = ui.take_events();
    assert!(events.contains(&"text 30 abcde".to_string()), "{events:?}");
    assert_eq!(
        events.last().unwrap(),
        "beep 30",
        "the sixth and seventh are refused"
    );
    assert!(ui.key(Key::Backspace, false));
    assert!(ui.key(Key::Left, false));
    assert!(ui.key(Key::Left, true));
    assert!(ui.char(b'X'));
    assert_eq!(text(&ui, 30), "abXd");
    assert!(ui.key(Key::Home, false));
    assert!(ui.key(Key::Delete, false));
    assert_eq!(text(&ui, 30), "bXd");
    // Control characters are not handled.
    assert!(ui.char(7));
    assert_eq!(text(&ui, 30), "bXd");
    // A click elsewhere takes the focus.
    assert_eq!(click(&mut ui, 95, 75), ["unfocus 30"]);
    assert!(!ui.char(b'a'), "no field has the keys");
}

#[test]
fn a_numeric_field_takes_digits_only() {
    let mut ui = interface("numeric");
    click(&mut ui, 55, 25);
    for byte in *b"1a2-3 4" {
        ui.char(byte);
    }
    assert_eq!(text(&ui, 31), "1234");
    // Y and N come as characters too: the keys are taken and do nothing.
    assert!(ui.key(Key::Y, false));
    assert_eq!(text(&ui, 31), "1234");
}

#[test]
fn a_click_puts_the_caret_between_two_characters_and_a_drag_selects() {
    let mut ui = interface("caret");
    ui.set_text(31, b"1234");
    // The click that focuses the field selects all of its text and does not place the caret.
    click(&mut ui, 80, 25);
    ui.key(Key::Home, false);
    // A click in a focused field puts the caret there: far left of the first character, 0.
    click(&mut ui, 50, 25);
    ui.char(b'9');
    assert_eq!(text(&ui, 31), "91234");
    // A press and a drag to the right selects what is between; a digit replaces it.
    press(&mut ui, 50, 25);
    at(&mut ui, 400, 25);
    release(&mut ui);
    ui.char(b'7');
    assert_eq!(text(&ui, 31), "7");
    // A second click in a focused field only moves the caret.
    ui.set_text(31, b"55");
    click(&mut ui, 89, 25);
    ui.char(b'6');
    assert_eq!(text(&ui, 31), "556");
}

#[test]
fn enter_keeps_the_text_and_escape_puts_back_the_old_one_both_pass_the_key_to_the_layout() {
    let mut ui = interface("enter");
    ui.set_text(30, b"old");
    ui.focus(30);
    ui.key(Key::End, false);
    ui.char(b'!');
    assert_eq!(text(&ui, 30), "old!");
    ui.take_events();
    assert!(ui.key(Key::Enter, false));
    assert_eq!(ui.take_events(), ["unfocus 30", "key 10 enter"]);
    assert_eq!(text(&ui, 30), "old!");
    assert_eq!(
        ui.take_actions(),
        [Action::Edited(30), Action::Key(10, Key::Enter)]
    );
    assert_eq!(state(&ui, 30) & FOCUSED, 0);
    ui.focus(30);
    ui.key(Key::End, false);
    ui.char(b'?');
    ui.take_events();
    assert!(ui.key(Key::Escape, false));
    assert_eq!(text(&ui, 30), "old!");
    assert_eq!(
        ui.take_events(),
        ["text 30 old!", "unfocus 30", "key 10 escape"]
    );
    assert_eq!(
        ui.take_actions(),
        [Action::Edited(30), Action::Key(10, Key::Escape)]
    );
    // Keys the field does not know go on unhandled.
    ui.focus(30);
    assert!(!ui.key(Key::Tab, false));
    assert!(!ui.key(Key::Up, false));
}

#[test]
fn a_hidden_or_disabled_field_takes_no_focus() {
    let mut ui = interface("nofocus");
    let i = ui.tree.find(30).unwrap();
    ui.tree.elements[i].state |= crate::ui::layout::DISABLED;
    assert!(!ui.focus(30));
    ui.tree.elements[i].state &= !crate::ui::layout::DISABLED;
    ui.tree.hide(i);
    assert_eq!(click(&mut ui, 55, 5), Vec::<String>::new());
    assert_eq!(state(&ui, 30) & HIDDEN, HIDDEN);
    assert!(!ui.focus(21), "a bar takes none");
    assert!(!ui.focus(999));
}

#[test]
fn text_is_set_and_read_back_as_shown() {
    let mut ui = interface("text");
    assert_eq!(ui.text(30), Some(vec![]));
    assert!(ui.set_text(30, b"caf\xe9"));
    assert_eq!(ui.text(30), Some(b"caf\xe9".to_vec()));
    assert_eq!(ui.text(999), None);
    assert!(!ui.set_text(999, b"x"));
}

#[test]
fn a_press_that_hits_nothing_keeps_the_focus_but_one_on_a_control_moves_it() {
    let mut ui = interface("keepfocus");
    click(&mut ui, 55, 5);
    // With the map view out of the way a press on empty space reaches no element.
    let map = ui.tree.find(7000).unwrap();
    ui.tree.hide(map);
    assert_eq!(click(&mut ui, 95, 75), Vec::<String>::new());
    assert_eq!(state(&ui, 30) & FOCUSED, FOCUSED);
    assert_eq!(click(&mut ui, 55, 25), ["unfocus 30", "focus 31"]);
}

#[test]
fn a_field_is_hit_over_its_whole_rectangle_and_the_focusing_click_selects_all_its_text() {
    let mut ui = interface("whole");
    // Transparent, as every shipped field is, and with no text under the pointer.
    let i = ui.tree.find(31).unwrap();
    ui.tree.elements[i].transparent = true;
    ui.set_text(31, b"12");
    assert_eq!(click(&mut ui, 88, 33), ["focus 31"]);
    // Nothing moved the caret: all the text is selected, and a character replaces it.
    ui.char(b'7');
    assert_eq!(text(&ui, 31), "7");
    // Focus by the code selects all as well.
    ui.set_text(30, b"abc");
    ui.focus(30);
    ui.char(b'x');
    assert_eq!(text(&ui, 30), "x");
}

#[test]
fn a_character_that_would_overflow_the_field_is_refused_with_a_beep() {
    let mut ui = interface("overflow");
    ui.focus(31);
    ui.take_events();
    // 40 pixels wide: the digits come in until the next would not fit.
    let width = crate::text::Font::request("Arial", "8").width(b"0");
    let fit = (40 / width) as usize;
    for _ in 0..fit + 3 {
        ui.char(b'0');
    }
    assert_eq!(text(&ui, 31).len(), fit);
    let events = ui.take_events();
    assert_eq!(events.iter().filter(|e| *e == "beep 31").count(), 3);
    // The same for a character the field does not take.
    ui.char(b'x');
    assert_eq!(ui.take_events(), ["beep 31"]);
    // Deleting and typing a narrower one still works.
    ui.key(Key::Backspace, false);
    ui.char(b'1');
    assert_eq!(text(&ui, 31).len(), fit);
}

#[test]
fn a_bar_with_nothing_to_scroll_is_not_there_to_be_hit_or_drawn() {
    let mut ui = interface("hidebar");
    // 3 rows fit in the list: its bar is neither hit nor drawn; the map view is.
    ui.set_rows(20, rows(3));
    assert!(ui.mouse(Mouse::Move(44, 5)), "the map view");
    ui.take_events();
    let mut frame = Framebuffer::new(100, 80);
    ui.paint(&mut frame, &mut |_, _| {});
    let thumb = frame.pixels[(12 * 100 + 44) * 4..][..3].to_vec();
    assert_ne!(thumb, [0, 0, 255], "no thumb");
    // More rows: it is there.
    ui.set_rows(20, rows(4));
    assert!(!ui.mouse(Mouse::Move(44, 6)));
    ui.paint(&mut frame, &mut |_, _| {});
    assert_ne!(frame.pixels[(12 * 100 + 44) * 4..][..3].to_vec(), [0, 0, 0]);
    // The text with its bar: one line of text shows, three do not fit.
    ui.set_text(50, b"a");
    assert!(ui.mouse(Mouse::Move(88, 70)), "the map view");
    ui.set_text(50, b"a\nb\nc");
    assert!(!ui.mouse(Mouse::Move(88, 70)));
}

#[test]
fn the_wheel_scrolls_a_list_only_when_it_has_a_bar_and_the_dragged_thumb_snaps_on_release() {
    let mut ui = interface("wheel2");
    // The list 22 has a bar 23 but is read-only: the wheel still reaches it (rows 5, 3 fit).
    ui.set_rows(22, rows(10));
    ui.mouse(Mouse::Move(55, 45));
    ui.mouse(Mouse::Wheel(-1));
    assert!(top(&ui, 22) > 0);
    // A list whose bar is not a bar does not scroll.
    let i = ui.tree.find(20).unwrap();
    if let Kind::ListBox(l) = &mut ui.tree.elements[i].kind {
        l.scroll_bar = 0;
    }
    ui.set_rows(20, rows(10));
    ui.mouse(Mouse::Move(5, 5));
    ui.mouse(Mouse::Wheel(-1));
    assert_eq!(top(&ui, 20), 0);
    // A dragged thumb follows the pointer within its travel and snaps when released.
    let mut ui = interface("snap");
    ui.set_rows(20, rows(10));
    press(&mut ui, 44, 12);
    at(&mut ui, 44, 18);
    let dragged = |ui: &Interface| match &ui.tree.elements[ui.tree.find(21).unwrap()].kind {
        Kind::ScrollBar(s) => s.bar.dragged,
        _ => unreachable!(),
    };
    assert_eq!(
        dragged(&ui),
        Some(14),
        "under the pointer, 6 further than the first place"
    );
    at(&mut ui, 44, 500);
    assert_eq!(dragged(&ui), Some(20), "kept to the travel");
    release(&mut ui);
    assert_eq!(dragged(&ui), None);
}

#[test]
fn a_list_draws_whole_rows_inside_its_borders_and_finds_rows_under_them() {
    let mut ui = interface("rowsfit");
    let i = ui.tree.find(20).unwrap();
    // A border of 3 pixels: 36 - 6 = 30 pixels hold two rows of 12.
    ui.tree.elements[i].y_border = 3;
    ui.set_rows(20, rows(10));
    let max_top = match &ui.tree.elements[ui.tree.find(21).unwrap()].kind {
        Kind::ScrollBar(s) => s.bar.max,
        _ => unreachable!(),
    };
    assert_eq!(max_top, 8, "two rows shown, eight beyond");
    // The first row is 3 to 15 pixels down.
    assert_eq!(press(&mut ui, 10, 4), ["focus 20", "row 20 0"]);
    release(&mut ui);
    assert_eq!(press(&mut ui, 10, 16), ["row 20 1"]);
}

#[test]
fn a_double_click_acts_only_on_a_list_that_handles_it() {
    let mut ui = interface("double");
    ui.set_rows(20, rows(3));
    let double = |ui: &mut Interface, x, y| {
        at(ui, x, y);
        ui.take_events();
        ui.take_actions();
        let belongs = ui.mouse(Mouse::Double);
        (belongs, ui.take_events(), ui.take_actions())
    };
    // The list does not handle double clicks: nothing happens.
    assert_eq!(double(&mut ui, 5, 17), (false, vec![], vec![]));
    assert_eq!(ui.selected_row(20), None);
    let list = ui.tree.find(20).unwrap();
    let Kind::ListBox(l) = &mut ui.tree.elements[list].kind else {
        panic!("not a list");
    };
    l.handle_double_clicks = true;
    // It selects the row as a press does and tells the game, whatever the row it was.
    let (belongs, events, actions) = double(&mut ui, 5, 17);
    assert!(!belongs);
    assert_eq!(events, ["focus 20", "row 20 1", "double 20 1"]);
    assert_eq!(actions, [Action::Double(20, 1)]);
    assert_eq!(ui.selected_row(20), Some(1));
    ui.mouse(Mouse::Up);
    // A click below the last row selects none: the action says -1.
    ui.set_rows(20, rows(1));
    assert_eq!(double(&mut ui, 5, 30).2, [Action::Double(20, -1)]);
    ui.mouse(Mouse::Up);
    // A read-only list and a field do nothing.
    assert_eq!(double(&mut ui, 55, 50), (false, vec![], vec![]));
    assert_eq!(double(&mut ui, 60, 5), (false, vec![], vec![]));
    // Over the map view, the press is the map view's.
    assert_eq!(double(&mut ui, 99, 79), (true, vec![], vec![]));
    // Over the held button of a press the double click is ignored.
    press(&mut ui, 5, 5);
    assert!(!ui.mouse(Mouse::Double));
    assert!(
        ui.take_actions()
            .iter()
            .all(|a| *a != Action::Double(20, 0))
    );
}

#[test]
fn leaving_a_field_tells_the_game_whichever_way_it_goes() {
    let mut ui = interface("edited");
    ui.focus(30);
    ui.take_actions();
    // A click elsewhere that takes the focus away.
    click(&mut ui, 5, 5);
    assert_eq!(ui.take_actions(), [Action::Edited(30), Action::Row(20, -1)]);
    ui.focus(30);
    ui.take_actions();
    ui.set_text(31, b"1");
    click(&mut ui, 55, 25);
    assert_eq!(ui.take_actions(), [Action::Edited(30)]);
    // Focusing another field leaves the first.
    assert!(ui.focus(30));
    assert_eq!(ui.take_actions(), [Action::Edited(31)]);
    // A list that loses the focus is not an edit.
    ui.focus(20);
    assert_eq!(ui.take_actions(), [Action::Edited(30)]);
    ui.focus(30);
    assert_eq!(ui.take_actions(), []);
}

#[test]
fn a_picture_is_put_into_a_picture_element_which_takes_its_size_and_none_hides_it() {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=UIImage\nid=60\nx=3\ny=4\n\
        [c]\ntype=UIText\nid=61\nx=0\ny=40\ndx=10\ndy=10\n";
    let install = Install::new(
        "widgets-picture",
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/red.tga", solid(5, 3, [255, 0, 0])),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let red = tree.picture(&vfs, "ui/red.tga");
    assert!(matches!(red, Some(Picture::Image(_))));
    assert_eq!(tree.picture(&vfs, "ui/red.tga"), red, "loaded once");
    assert_eq!(tree.picture(&vfs, "ui/gone.tga"), None);
    assert_eq!(tree.picture(&vfs, ""), None);
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    ui.show(1);
    let image = ui.tree.find(60).unwrap();
    assert_eq!(ui.tree.rect(image), (3, 4, 0, 0));
    assert!(ui.set_picture(60, red));
    assert_eq!(ui.tree.rect(image), (3, 4, 5, 3));
    assert_eq!(state(&ui, 60) & HIDDEN, 0);
    assert!(ui.set_picture(60, None));
    assert_ne!(state(&ui, 60) & HIDDEN, 0);
    assert!(ui.set_picture(60, red));
    assert_eq!(state(&ui, 60) & HIDDEN, 0);
    // Not a picture element, and no element at all.
    assert!(!ui.set_picture(61, red) && !ui.set_picture(5, red));
    // An element is hidden and shown on its own.
    assert!(ui.set_hidden(61, true));
    assert_ne!(state(&ui, 61) & HIDDEN, 0);
    assert!(ui.set_hidden(61, false));
    assert_eq!(state(&ui, 61) & HIDDEN, 0);
    assert!(!ui.set_hidden(5, true));
    assert_eq!(ui.text_entry(b"ANY"), Vec::<u8>::new());
}

#[test]
fn an_image_set_draws_the_picture_the_game_chooses() {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=UIImageSet\nid=62\nx=2\ny=3\ndx=5\ndy=3\ntransparent=0\n\
        [c]\ntype=UIText\nid=61\nx=0\ny=40\ndx=10\ndy=10\n";
    let install = Install::new(
        "widgets-imageset",
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/red.tga", solid(5, 3, [255, 0, 0])),
            ("ui/blue.tga", solid(5, 3, [0, 0, 255])),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    // The pictures of the set are animations in the game's files; any picture does here.
    let pictures = [
        tree.picture(&vfs, "ui/red.tga"),
        tree.picture(&vfs, "ui/blue.tga"),
    ];
    let set = tree.find(62).unwrap();
    tree.elements[set].look.animation = pictures.to_vec();
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    ui.show(1);
    let pixel = |ui: &mut Interface| {
        let mut frame = Framebuffer::new(100, 80);
        ui.paint(&mut frame, &mut |_, _| {});
        let at = (3 * 100 + 2) * 4;
        frame.pixels[at..at + 3].to_vec()
    };
    assert_eq!(pixel(&mut ui), [255, 0, 0], "the first picture");
    assert!(ui.set_image(62, 1));
    assert_eq!(pixel(&mut ui), [0, 0, 255]);
    assert!(ui.set_image(62, 0));
    assert_eq!(pixel(&mut ui), [255, 0, 0]);
    // A picture that does not exist, an element that is not an image set, and none at all.
    assert!(!ui.set_image(62, 2) && !ui.set_image(62, usize::MAX));
    assert!(!ui.set_image(61, 0) && !ui.set_image(5, 0));
    assert_eq!(pixel(&mut ui), [255, 0, 0]);
}

#[test]
fn texts_fields_lists_and_bars_are_hit_over_their_whole_rectangle_even_when_transparent() {
    let mut ui = interface("whole");
    ui.set_rows(20, rows(3));
    ui.set_text(50, b"x");
    for id in [20, 21, 30, 50, 7000] {
        let i = ui.tree.find(id).unwrap();
        ui.tree.elements[i].transparent = true;
    }
    // Painted once, so that the pictures and canvases of the pixel test exist.
    ui.paint(&mut Framebuffer::new(100, 80), &mut |_, _| {});
    // A point of the list that no glyph is at, and one of its bar between the arrows and the
    // thumb, are the list's and the bar's.
    let events = click(&mut ui, 38, 5);
    assert!(events.contains(&"row 20 0".to_string()), "{events:?}");
    let hit = |ui: &mut Interface, x, y| {
        ui.mouse(Mouse::Move(x, y));
        ui.take_events()
    };
    assert_eq!(
        hit(&mut ui, 78, 70),
        ["hover 50"],
        "a text, where it draws nothing"
    );
    ui.set_rows(20, rows(10));
    assert_eq!(hit(&mut ui, 44, 30), ["hover 21"], "the track of the bar");
    assert_eq!(hit(&mut ui, 60, 5), ["hover 30"], "a field");
}
