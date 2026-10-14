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
    ui.update(33); // starts the count
    ui.update(33);
    ui.update(33);
    ui.update(33);
    assert_eq!(
        ui.tree.elements[2].state & HIDDEN,
        0,
        "100 ms have not passed"
    );
    assert!(ui.tree.elements[2].countdown.is_some());
    ui.paint(&mut Framebuffer::new(64, 48), &mut |_, _| {});
    ui.update(33);
    assert_eq!(ui.tree.elements[2].state & HIDDEN, HIDDEN);
    assert!(ui.dirty());
}
