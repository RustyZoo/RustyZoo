use super::*;
use crate::app::Mouse;
use crate::fixtures::Install;
use crate::text::fill_color;
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

fn interface(name: &str, root: &str) -> Interface {
    let install = Install::new(
        &format!("ui-interface-{name}"),
        &[("ui/root.lyt", root.as_bytes().to_vec())],
    );
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(64, 48, |_| 12);
    Interface::new(tree, Box::new(NoTexts))
}

const ROOT: &str = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
    [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
    [b]\ntype=UIText\nid=3\ndx=10\ndy=10\ntimed=1\ntimerDuration=100\n\
    [c]\ntype=UIEditableText\nid=4\ndx=10\ndy=10\n";

#[test]
fn the_map_view_is_shown_and_has_the_keys_until_another_element_has_the_focus() {
    let mut ui = interface("keys", ROOT);
    assert!(ui.dirty());
    assert!(ui.map_view_has_the_keys());
    ui.tree.elements[3].state |= FOCUSED;
    assert!(!ui.map_view_has_the_keys());
    ui.tree.elements[3].state &= !FOCUSED;
    ui.tree.elements[1].state |= HIDDEN;
    assert!(!ui.map_view_has_the_keys(), "not shown");
    let none = interface("nokeys", "[LayoutInfo]\nid=1\n");
    assert!(!none.map_view_has_the_keys());
}

#[test]
fn a_paint_clears_dirty_and_a_show_sets_it() {
    let mut ui = interface("dirty", ROOT);
    let mut frame = Framebuffer::new(64, 48);
    let mut called = 0;
    ui.paint(&mut frame, &mut |_, _| called += 1);
    assert_eq!((called, ui.dirty()), (1, false));
    assert!(ui.show(3));
    assert!(ui.dirty());
    assert!(!ui.show(99));
}

#[test]
fn a_timed_element_hides_itself_after_its_duration() {
    let mut ui = interface("timed", ROOT);
    ui.paint(&mut Framebuffer::new(64, 48), &mut |_, _| {});
    // Hidden elements do not count.
    ui.update(1000);
    assert!(!ui.dirty());
    ui.show(1);
    ui.tree.elements[2].state &= !HIDDEN;
    // The update that starts the count has used up 1 ms of it.
    ui.update(1033);
    assert_eq!(ui.tree.elements[2].countdown, Some(99));
    ui.update(1066);
    ui.update(1099);
    assert_eq!(
        ui.tree.elements[2].state & HIDDEN,
        0,
        "66 ms have not passed"
    );
    assert_eq!(ui.tree.elements[2].countdown, Some(33));
    ui.paint(&mut Framebuffer::new(64, 48), &mut |_, _| {});
    ui.take_events();
    ui.update(1132);
    assert_eq!(ui.tree.elements[2].state & HIDDEN, HIDDEN);
    assert!(ui.dirty());
    assert_eq!(ui.take_events(), ["hide 3"]);
}

#[test]
fn the_update_works_at_most_every_33_ms_and_counts_the_real_time_between_two_runs() {
    let mut ui = interface("gap", ROOT);
    ui.show(1);
    ui.tree.elements[2].state &= !HIDDEN;
    ui.update(500); // starts the count
    // Less than 33 ms after the last run: nothing, and that time is not lost.
    ui.update(520);
    ui.update(532);
    assert_eq!(ui.tree.elements[2].countdown, Some(99));
    ui.update(533);
    assert_eq!(ui.tree.elements[2].countdown, Some(66));
    // A long gap counts whole: the count is real time, not a number of updates.
    ui.update(5000);
    assert_eq!(ui.tree.elements[2].state & HIDDEN, HIDDEN);
    // A clock that goes back is no update.
    ui.update(10);
    ui.update(u64::MAX);
}

const LINE: &str = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
    [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
    [b]\ntype=ZTMessageQueue\nid=7001\nx=2\ny=2\ndx=60\ndy=20\nxborder=2\nyborder=2\n\
    justify=center\nforecolor=255\nforecolor=255\nforecolor=255\ntimed=1\ntimerDuration=100\n\
    state=1025\n";

/// How many pixels of the frame are not black.
fn lit(frame: &Framebuffer) -> usize {
    frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] != [0, 0, 0])
        .count()
}

#[test]
fn the_message_line_shows_a_text_until_its_time_is_up() {
    let mut ui = interface("message", LINE);
    let mut frame = Framebuffer::new(64, 48);
    ui.paint(&mut frame, &mut |_, _| {});
    assert_eq!(lit(&frame), 0, "hidden until a message");
    assert!(ui.message(b"Nothing here", 3));
    ui.paint(&mut frame, &mut |_, _| {});
    assert!(lit(&frame) > 20, "the text is drawn");
    // The update that starts the count has used up 1 ms of it.
    ui.update(1000);
    ui.update(1066);
    assert_eq!(ui.tree.elements[2].countdown, Some(33));
    ui.update(1099);
    assert_eq!(ui.tree.elements[2].state & HIDDEN, HIDDEN);
    ui.paint(&mut frame, &mut |_, _| {});
    assert_eq!(lit(&frame), 0);
    // A tree without a message line, and an empty text.
    assert!(!interface("nomessage", ROOT).message(b"x", 3));
    assert!(ui.message(b"", 3));
}

/// The text the message line shows, and whether it is shown.
fn shown(ui: &Interface) -> (Vec<u8>, bool) {
    (
        ui.text(7001).unwrap(),
        ui.tree.elements[2].state & HIDDEN == 0,
    )
}

#[test]
fn messages_posted_together_come_in_order_one_per_duration() {
    let mut ui = interface("queue", LINE);
    ui.message(b"A", 3);
    ui.update(1000);
    ui.message(b"B", 3);
    ui.message(b"C", 3);
    assert_eq!(shown(&ui), (b"A".to_vec(), true), "shown until the update");
    ui.update(1033);
    assert_eq!(
        shown(&ui),
        (b"B".to_vec(), true),
        "a post ends the shown one"
    );
    ui.update(1066);
    ui.update(1132);
    assert_eq!(shown(&ui).0, b"B", "B has its whole count");
    ui.update(1166);
    assert_eq!(shown(&ui), (b"C".to_vec(), true));
    ui.update(1266);
    ui.update(1366);
    assert!(!shown(&ui).1, "nothing waits: the line hides");
    assert_eq!(ui.text(7001), Some(Vec::new()));
}

#[test]
fn a_paused_line_with_nothing_waiting_keeps_its_text_and_restarts_the_count() {
    let mut ui = interface("paused", LINE);
    ui.hold_message(true);
    ui.message(b"paused", 3);
    ui.update(1000);
    ui.update(1200);
    ui.update(1400);
    assert_eq!(shown(&ui), (b"paused".to_vec(), true), "stays");
    assert_eq!(
        ui.tree.elements[2].countdown,
        Some(99),
        "the count restarted"
    );
    // A message posted during the pause replaces it at once and is the one that stays.
    ui.message(b"other", 3);
    ui.update(1433);
    assert_eq!(shown(&ui).0, b"other");
    ui.update(1700);
    assert_eq!(shown(&ui), (b"other".to_vec(), true));
    // The unpause: what is shown goes at the next update.
    ui.hold_message(false);
    ui.end_message();
    ui.update(1733);
    assert!(!shown(&ui).1);
    // Paused, a waiting message is taken before the pause restarts the count.
    ui.hold_message(true);
    ui.message(b"1", 3);
    ui.update(1766);
    ui.message(b"2", 3);
    ui.message(b"3", 3);
    ui.update(1800);
    assert_eq!(shown(&ui).0, b"2");
    ui.update(1900);
    ui.update(2000);
    assert_eq!(shown(&ui).0, b"3", "the queue goes on while paused");
}

#[test]
fn the_message_queue_holds_queue_size_minus_one_messages_and_drops_the_oldest() {
    let with = |size: &str| {
        let line = match size {
            "missing" => LINE.to_string(),
            _ => LINE.replace("timed=1", &format!("queueSize={size}\ntimed=1")),
        };
        let mut ui = interface(&format!("size{size}"), &line);
        ui.message(b"0", 3);
        ui.update(1000);
        for t in [b"1", b"2", b"3"] {
            ui.message(t, 3);
        }
        let mut seen = Vec::new();
        for n in 1..12 {
            ui.update(1000 + 100 * n);
            seen.push(shown(&ui));
        }
        seen.retain(|s| s.1);
        seen.dedup();
        seen.into_iter().map(|s| s.0).collect::<Vec<_>>()
    };
    assert_eq!(with("2"), [b"3".to_vec()]);
    assert_eq!(with("3"), [b"2".to_vec(), b"3".to_vec()]);
    // 1 and 0: none waits; a negative size never drops; a missing key is 25.
    assert!(with("1").is_empty());
    assert!(with("0").is_empty());
    assert_eq!(with("-5").len(), 3);
    assert_eq!(with("missing").len(), 3, "the default is 25");
    assert_eq!(with("2147483647").len(), 3);
}

#[test]
fn the_pointer_text_is_written_right_of_the_pointer_in_the_map_view_colour() {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\nforecolor=255\nforecolor=0\nforecolor=0\n";
    let mut ui = interface("pointer-text", root);
    ui.mouse(Mouse::Move(10, 24));
    let mut frame = Framebuffer::new(64, 48);
    ui.pointer_text(&mut frame, b"($25)", 23);
    let lit_columns: Vec<usize> = (0..64)
        .filter(|&x| (0..48).any(|y| frame.pixels[(y * 64 + x) * 4..][..3] != [0, 0, 0]))
        .collect();
    assert!(
        lit_columns.first().is_some_and(|&x| x >= 33),
        "{lit_columns:?}"
    );
    assert!(
        frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 200 && p[1] == 0 && p[2] == 0)
    );
    // The top of the line is the pointer's row: nothing is drawn above it.
    let top =
        (0..48).find(|&y| (0..64).any(|x| frame.pixels[(y * 64 + x) * 4..][..3] != [0, 0, 0]));
    assert!(top.is_some_and(|y| y >= 24), "{top:?}");
    // Without a map view, and a pointer far off the frame, nothing happens and nothing panics.
    interface("pointer-none", "[LayoutInfo]\nid=1\n").pointer_text(&mut frame, b"x", 23);
    ui.mouse(Mouse::Move(i32::MAX, i32::MIN));
    ui.pointer_text(&mut frame, b"($25)", i32::MAX);
}

/// The minimap 20 by 10 at (10, 10) with a layout of its own colours.
fn minimap_ui(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=ZTMiniMap\nid=1026\nx=10\ny=10\ndx=20\ndy=10\nlayer=5\n\
        backcolor=255\nbackcolor=0\nbackcolor=255\ncolormap=ui/miniclr.cfg\n";
    let mut ui = interface(name, root);
    ui.tree.elements[2].state &= !HIDDEN;
    ui
}

/// A picture of the minimap's size, all `back` but the pixel (5, 5), which is red.
fn minimap_picture() -> Framebuffer {
    let mut picture = Framebuffer::new(20, 10);
    picture.fill(fill_color([255, 0, 255]));
    let at = (5 * 20 + 5) * 4;
    picture.pixels[at..at + 4].copy_from_slice(&[255, 0, 0, 255]);
    picture
}

#[test]
fn the_minimap_shows_the_picture_with_its_background_keyed_out_and_a_release_on_it_is_an_action() {
    let mut ui = minimap_ui("minimap");
    assert_eq!(ui.minimap_file(), Some(&b"ui/miniclr.cfg"[..]));
    assert_eq!(ui.minimap_slot(), Some(((20, 10), [255, 0, 255])));
    let click = |ui: &mut Interface, at: (i32, i32)| {
        ui.mouse(Mouse::Move(at.0, at.1));
        ui.mouse(Mouse::Down);
        ui.mouse(Mouse::Up);
        ui.take_actions()
    };
    // Before a picture there is nothing to hit.
    assert_eq!(click(&mut ui, (15, 15)), []);
    ui.dirty = false;
    ui.set_minimap(minimap_picture());
    assert!(ui.dirty());
    ui.dirty = false;
    ui.set_minimap(minimap_picture());
    assert!(!ui.dirty(), "the same picture changes nothing");
    let mut frame = Framebuffer::new(64, 48);
    frame.fill([9, 9, 9, 255]);
    ui.paint(&mut frame, &mut |_, _| {});
    let at = |x: usize, y: usize| frame.pixels[(y * 64 + x) * 4..][..3].to_vec();
    assert_eq!(at(15, 15), [255, 0, 0]);
    assert_eq!(
        at(16, 15),
        [0, 0, 0],
        "the keyed-out pixel shows what is below"
    );
    // The release on the red pixel is an action with the point in the widget; on a keyed-out
    // pixel, or off the widget, it is none.
    assert_eq!(click(&mut ui, (15, 15)), [Action::Minimap(5, 5)]);
    assert_eq!(click(&mut ui, (16, 15)), []);
    assert_eq!(click(&mut ui, (0, 0)), []);
    // Pressed on the pixel and released off it: none.
    ui.mouse(Mouse::Move(15, 15));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Move(40, 40));
    ui.mouse(Mouse::Up);
    assert_eq!(ui.take_actions(), []);
    // Hidden: no slot.
    ui.tree.elements[2].state |= HIDDEN;
    assert_eq!(ui.minimap_slot(), None);
    assert_eq!(
        interface("none", "[LayoutInfo]\nid=1\n").minimap_file(),
        None
    );
}

/// The message line with a colour for each kind, in the interface's clock.
fn coloured(name: &str, extra: &str) -> Interface {
    let line = LINE.replace(
        "timed=1",
        &format!(
            "generalcolor=1\ngeneralcolor=2\ngeneralcolor=3\ngoodcolor=0\ngoodcolor=255\n\
             goodcolor=0\nurgentcolor=255\nurgentcolor=0\nurgentcolor=0\n{extra}timed=1"
        ),
    );
    interface(name, &line)
}

#[test]
fn a_message_takes_the_colour_of_its_kind_modulo_3() {
    let colour = |ui: &Interface| ui.tree.elements[2].look.fore_color;
    for (n, (kind, want)) in [
        (0, [1, 2, 3]),
        (1, [0, 255, 0]),
        (2, [255, 0, 0]),
        (3, [1, 2, 3]),
        (4, [0, 255, 0]),
        (5, [255, 0, 0]),
        (i32::MAX, [0, 255, 0]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut ui = coloured(&format!("kinds{n}"), "");
        ui.message(b"k", kind);
        assert_eq!(colour(&ui), want, "kind {kind}");
    }
    let mut ui = coloured("kinds", "");
    // A queued message is coloured when it is shown, not when it is posted.
    ui.message(b"shown", 1);
    ui.message(b"waits", 2);
    assert_eq!(colour(&ui), [0, 255, 0]);
    ui.update(5000);
    assert_eq!(colour(&ui), [255, 0, 0]);
}

#[test]
fn a_negative_kind_takes_the_general_colour() {
    for (n, kind) in [-1, -2, -3, -4, i32::MIN].into_iter().enumerate() {
        let mut ui = coloured(&format!("negative{n}"), "");
        ui.message(b"k", kind);
        assert_eq!(
            ui.tree.elements[2].look.fore_color,
            [1, 2, 3],
            "kind {kind}"
        );
    }
}

#[test]
fn a_missing_colour_key_takes_the_built_in_colour() {
    let shown = |name: &str, extra: &str| {
        let line = LINE.replace("timed=1", &format!("{extra}timed=1"));
        [0, 1, 2].map(|kind| {
            let mut ui = interface(&format!("{name}{kind}"), &line);
            ui.message(b"k", kind);
            ui.tree.elements[2].look.fore_color
        })
    };
    let built_in = [[255; 3], [0, 255, 0], [255, 0, 0]];
    // No key: not the text colour (255, 255, 255 in this line is white too, so it is told apart
    // by the green and the red).
    assert_eq!(shown("no-keys", ""), built_in);
    // Fewer than 3 values: the built-in colour; a longer key: its first 3 values.
    let two = "goodcolor=9\ngoodcolor=9\nurgentcolor=7\n";
    assert_eq!(shown("two-values", two), built_in);
    let four = "goodcolor=1\ngoodcolor=2\ngoodcolor=3\ngoodcolor=4\n";
    assert_eq!(shown("four-values", four)[1], [1, 2, 3]);
}

#[test]
fn a_text_repeated_within_the_minimum_interval_is_dropped_below_kind_3() {
    let post = |ui: &mut Interface, at: u64, text: &[u8], kind: i32| {
        ui.set_time(at);
        ui.message(text, kind);
        ui.update(at);
        ui.text(7001).unwrap()
    };
    let mut ui = interface("repeat", LINE);
    assert_eq!(post(&mut ui, 0, b"A", 2), b"A");
    // Dropped: no queue, the count is not emptied.
    ui.set_time(10_000);
    ui.message(b"A", 0);
    assert_eq!(ui.waiting, 0);
    assert_eq!(ui.messages.len(), 1);
    // Other texts, and kinds of 3 and more, are not dropped.
    ui.message(b"B", 1);
    ui.message(b"A", 3);
    ui.message(b"A", 7);
    assert_eq!(ui.waiting, 3);
    // After the interval the same text goes through (59 999 ms: dropped, 60 000 ms: through).
    ui.set_time(20_000);
    ui.message(b"C", 0);
    let len = ui.waiting;
    ui.set_time(79_999);
    ui.message(b"C", 2);
    assert_eq!(ui.waiting, len);
    ui.set_time(80_000);
    ui.message(b"C", 2);
    assert_eq!(ui.waiting, len + 1);
}

#[test]
fn the_repeat_interval_is_a_32_bit_unsigned_product_in_ms() {
    // Whether a text posted at `first` and again at `second` (ms) is dropped.
    let dropped = |interval: Option<i32>, first: u64, second: u64| {
        let mut ui = interface("interval", LINE);
        if let Some(n) = interval {
            ui.set_settings(tooltip::Settings {
                message_interval: n,
                ..tooltip::Settings::default()
            });
        }
        ui.set_time(first);
        assert!(ui.message(b"A", 0));
        ui.set_time(second);
        let posted = ui.message(b"A", 0);
        assert_eq!(ui.messages.len(), if posted { 2 } else { 1 });
        !posted
    };
    assert!(dropped(None, 0, 59_999), "missing: 60 s");
    assert!(!dropped(None, 0, 60_000));
    assert!(dropped(Some(5), 0, 4_999));
    assert!(!dropped(Some(5), 0, 5_000));
    assert!(!dropped(Some(0), 0, 0), "0 never drops");
    // A negative interval is a huge unsigned one: every equal text still listed drops.
    assert!(dropped(Some(-60), 0, 0));
    assert!(dropped(Some(-1), 0, 4_000_000_000));
    assert!(!dropped(Some(i32::MIN), 0, 1_000), "2^31 * 1000 wraps to 0");
    // A product above 2^32 ms wraps: 4 294 968 s is 704 ms.
    assert!(dropped(Some(4_294_968), 0, 703));
    assert!(!dropped(Some(4_294_968), 0, 704));
    assert!(dropped(Some(i32::MAX), 0, 1_000_000));
    // The clock is 32 bits and the age an unsigned difference: across the wrap the text is
    // recent; a clock gone back is an entry about 2^32 ms old.
    assert!(dropped(None, u64::from(u32::MAX) - 10, 1 << 32 | 5));
    assert!(!dropped(None, 100_000, 0));
}

#[test]
fn the_list_forgets_its_oldest_entry_and_a_dropped_text_can_come_back() {
    let line = LINE.replace("timed=1", "queueSize=2\ntimed=1");
    let mut ui = interface("repeat-list", &line);
    ui.message(b"A", 0);
    ui.message(b"B", 0);
    ui.message(b"C", 0);
    // The list holds 2 entries: A is gone, so it is no repeat; B still is.
    let texts: Vec<_> = ui.messages.iter().map(|e| e.text.clone()).collect();
    assert_eq!(texts, [b"C".to_vec(), b"B".to_vec()]);
    ui.message(b"B", 0);
    assert_eq!(ui.messages.len(), 2);
    ui.message(b"A", 0);
    assert_eq!(ui.messages.front().unwrap().text, b"A");
}
