use super::*;
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
