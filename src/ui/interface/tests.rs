use super::*;
use crate::app::Mouse;
use crate::sprite::tests::Install;
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
fn the_message_line_shows_a_text_until_its_time_is_up_and_a_new_text_starts_the_count_again() {
    let mut ui = interface("message", LINE);
    let mut frame = Framebuffer::new(64, 48);
    ui.paint(&mut frame, &mut |_, _| {});
    assert_eq!(lit(&frame), 0, "hidden until a message");
    assert!(ui.message(b"Nothing here"));
    ui.paint(&mut frame, &mut |_, _| {});
    assert!(lit(&frame) > 20, "the text is drawn");
    // The update that starts the count has used up 1 ms of it.
    ui.update(1000);
    ui.update(1066);
    assert_eq!(ui.tree.elements[2].countdown, Some(33));
    assert!(ui.message(b"Again"), "another text replaces it");
    assert_eq!(ui.tree.elements[2].countdown, None);
    ui.update(1100);
    ui.update(1166);
    assert_eq!(
        ui.tree.elements[2].state & HIDDEN,
        0,
        "the count began again"
    );
    ui.update(1233);
    assert_eq!(ui.tree.elements[2].state & HIDDEN, HIDDEN);
    ui.paint(&mut frame, &mut |_, _| {});
    assert_eq!(lit(&frame), 0);
    // A tree without a message line, and an empty text.
    assert!(!interface("nomessage", ROOT).message(b"x"));
    assert!(ui.message(b""));
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
