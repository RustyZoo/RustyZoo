use super::*;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{FOCUSED, Tree};
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

/// A 4 x 2 image whose pixels are all the key colour, but the right half.
fn tga() -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 2, 0, 24, 0x20];
    for _ in 0..2 {
        out.extend([10, 10, 10, 10, 10, 10, 200, 0, 0, 200, 0, 0]);
    }
    out
}

const BUTTON: &str = "type=UIButton\ncolorasicon=1\ndx=10\ndy=10\ny=0\n";

/// A window of 64 x 48: the map view, a layout 10 with the buttons, layouts 30 to 32 hidden, a
/// draggable layout 40, and an image 50 that is transparent on its left half.
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\ntransparent=0\n\
        [b]\ntype=UILayout\nlayout=ui/buttons.lyt\n\
        [c]\ntype=UILayout\nlayout=ui/l30.lyt\n[d]\ntype=UILayout\nlayout=ui/l31.lyt\n\
        [e]\ntype=UILayout\nlayout=ui/l32.lyt\n[f]\ntype=UILayout\nlayout=ui/drag.lyt\n";
    let buttons = format!(
        "[LayoutInfo]\nid=10\nlayer=1\nx=0\ny=0\ndx=whole\ndy=20\n\
         [a]\nid=20\nx=0\naction=1\ntarget=30\n{BUTTON}\
         [b]\nid=21\nx=12\nstate=2048\naction=3\ntarget=31\n{BUTTON}\
         [c]\nid=22\nx=24\nstate=2048\naction=3\ntarget=32\n{BUTTON}\
         [d]\nid=23\nx=36\nstate=4096\n{BUTTON}\
         [e]\nid=24\nx=48\nrepeating=1\n{BUTTON}\
         [f]\ntype=UIRadioSet\nid=25\nbutton=21\nbutton=22\n\
         [g]\nid=26\nx=0\ny=12\nstate=2\naction=1\ntarget=30\n{BUTTON}\
         [h]\ntype=UIImage\nid=50\nx=40\ny=12\nnormal=ui/half.tga\n"
    );
    let layout =
        |id: i32| format!("[LayoutInfo]\nid={id}\nlayer=2\nx=0\ny=30\ndx=5\ndy=5\nstate=1\n");
    let drag = "[LayoutInfo]\nid=40\nlayer=3\nx=20\ny=24\ndx=20\ndy=20\n\
        [a]\ntype=UIImage\nid=41\ndx=20\ndy=20\nalias=-1\ntransparent=0\n";
    let install = Install::new(
        &format!("input-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/buttons.lyt", buttons.into_bytes()),
            ("ui/l30.lyt", layout(30).into_bytes()),
            ("ui/l31.lyt", layout(31).into_bytes()),
            ("ui/l32.lyt", layout(32).into_bytes()),
            ("ui/drag.lyt", drag.as_bytes().to_vec()),
            ("ui/half.tga", tga()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    assert_eq!(tree.warnings, Vec::<String>::new());
    tree.resolve(64, 48, |_| 12);
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    for id in [10, 40] {
        ui.show(id);
    }
    ui.take_events();
    ui
}

/// Moves to a point, presses and releases there, and returns the events.
fn click(ui: &mut Interface, at: (i32, i32)) -> Vec<String> {
    ui.mouse(Mouse::Move(at.0, at.1));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Up);
    ui.take_events()
}

fn state(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

fn shown(ui: &Interface, id: i32) -> bool {
    state(ui, id) & HIDDEN == 0
}

#[test]
fn the_pointer_highlights_the_element_under_it_and_the_map_view_is_not_highlighted() {
    let mut ui = interface("hover");
    assert!(!ui.mouse(Mouse::Move(3, 3)));
    assert_eq!(ui.take_events(), ["hover 20"]);
    assert_eq!(state(&ui, 20) & HIGHLIGHTED, HIGHLIGHTED);
    // Over the same button again: nothing. Between two buttons the map view is hit.
    ui.mouse(Mouse::Move(4, 4));
    assert!(ui.take_events().is_empty());
    assert!(ui.mouse(Mouse::Move(11, 3)));
    assert_eq!(ui.take_events(), ["hover none"]);
    assert_eq!(state(&ui, 20) & HIGHLIGHTED, 0);
    // A disabled button is hit and highlighted, but gets no press.
    ui.mouse(Mouse::Move(3, 15));
    assert_eq!(ui.take_events(), ["hover 26"]);
    assert!(click(&mut ui, (3, 15)).is_empty());
    assert!(ui.take_sounds().is_empty());
}

#[test]
fn a_plain_button_is_pressed_follows_the_pointer_and_acts_on_release_inside() {
    let mut ui = interface("plain");
    ui.mouse(Mouse::Move(3, 3));
    ui.take_events();
    ui.mouse(Mouse::Down);
    assert_eq!(ui.take_events(), ["sound uiclick", "press 20"]);
    assert_eq!(ui.take_sounds(), ["uiclick"]);
    assert_eq!(state(&ui, 20) & SELECTED, SELECTED);
    // Dragged off: deselected, and the highlight stays; back on: selected again.
    ui.mouse(Mouse::Move(30, 40));
    assert_eq!(state(&ui, 20) & (SELECTED | HIGHLIGHTED), HIGHLIGHTED);
    ui.mouse(Mouse::Move(4, 4));
    assert_eq!(state(&ui, 20) & SELECTED, SELECTED);
    assert!(!shown(&ui, 30));
    ui.mouse(Mouse::Up);
    assert_eq!(
        ui.take_events(),
        ["release 20 inside", "select 20", "show 30"]
    );
    assert!(shown(&ui, 30));
    assert_eq!(state(&ui, 20) & SELECTED, 0);
    // Released outside: nothing happens.
    ui.tree.hide(ui.tree.find(30).unwrap());
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Move(30, 40));
    ui.take_events();
    ui.mouse(Mouse::Up);
    assert_eq!(ui.take_events()[0], "release 20 outside");
    assert!(!shown(&ui, 30));
}

#[test]
fn toggle_buttons_of_a_radio_set_switch_and_deselect_each_other() {
    let mut ui = interface("radio");
    // The release updates the set first: every other member is deselected, its deselect action
    // run even though it was not selected.
    assert_eq!(
        click(&mut ui, (14, 3)),
        [
            "hover 21",
            "sound uiclick",
            "press 21",
            "release 21 inside",
            "deselect 22",
            "hide 32",
            "select 21",
            "show 31"
        ]
    );
    assert_eq!(state(&ui, 21) & SELECTED, SELECTED);
    assert!(shown(&ui, 31));
    // The second member selected: the first is deselected and its deselect action hides 31.
    let events = click(&mut ui, (26, 3));
    assert_eq!(
        events[4..],
        ["deselect 21", "hide 31", "select 22", "show 32"]
    );
    assert_eq!(state(&ui, 21) & SELECTED, 0);
    assert!(!shown(&ui, 31) && shown(&ui, 32));
    // Held on a selected toggle: drawn as released while the pointer is over it.
    ui.mouse(Mouse::Down);
    assert_eq!(state(&ui, 22) & RELEASING, RELEASING);
    ui.mouse(Mouse::Move(60, 40));
    assert_eq!(state(&ui, 22) & (RELEASING | PRESSING), 0);
    ui.mouse(Mouse::Move(26, 3));
    ui.mouse(Mouse::Up);
    // Released as deselected: every member is deselected and the target hidden.
    assert_eq!(state(&ui, 22) & SELECTED, 0);
    assert!(!shown(&ui, 32));
    // A toggle that is not selected is pressing while held.
    ui.mouse(Mouse::Down);
    assert_eq!(state(&ui, 22) & PRESSING, PRESSING);
}

#[test]
fn a_sticky_button_stays_selected_and_acts_at_every_release_inside() {
    let mut ui = interface("sticky");
    let first = click(&mut ui, (38, 3));
    assert_eq!(first[first.len() - 2..], ["release 23 inside", "select 23"]);
    assert_eq!(state(&ui, 23) & SELECTED, SELECTED);
    ui.mouse(Mouse::Down);
    assert_eq!(
        state(&ui, 23) & PRESSING,
        0,
        "selected: nothing on the press"
    );
    ui.mouse(Mouse::Up);
    assert_eq!(state(&ui, 23) & SELECTED, SELECTED);
    assert!(ui.take_events().iter().any(|e| e == "select 23"));
    // Released outside it is not selected.
    let mut ui = interface("sticky2");
    ui.mouse(Mouse::Move(38, 3));
    ui.mouse(Mouse::Down);
    assert_eq!(state(&ui, 23) & PRESSING, PRESSING);
    ui.mouse(Mouse::Move(60, 40));
    assert_eq!(state(&ui, 23) & PRESSING, 0);
    ui.mouse(Mouse::Up);
    assert_eq!(state(&ui, 23) & SELECTED, 0);
}

#[test]
fn a_repeating_button_acts_at_once_then_after_500_ms_every_30_while_held_over() {
    let mut ui = interface("repeat");
    ui.mouse(Mouse::Move(50, 3));
    ui.take_events();
    ui.mouse(Mouse::Down);
    let count = |ui: &mut Interface| {
        ui.take_events()
            .iter()
            .filter(|e| *e == "select 24")
            .count()
    };
    assert_eq!(count(&mut ui), 1);
    ui.update(499);
    assert_eq!(count(&mut ui), 0);
    ui.update(1);
    assert_eq!(count(&mut ui), 1);
    ui.update(29);
    assert_eq!(count(&mut ui), 0);
    ui.update(1);
    assert_eq!(count(&mut ui), 1);
    // Off the button it waits (1 ms) and fires again when the pointer is back.
    ui.mouse(Mouse::Move(60, 40));
    ui.update(60);
    assert_eq!(count(&mut ui), 0);
    ui.mouse(Mouse::Move(50, 3));
    ui.update(1);
    assert_eq!(count(&mut ui), 1);
    ui.mouse(Mouse::Up);
    ui.take_events();
    ui.update(1000);
    assert_eq!(count(&mut ui), 0, "released: no more");
}

#[test]
fn transparent_pixels_let_the_message_through_to_the_map() {
    let mut ui = interface("pixel");
    // The image is 4 x 2 at (40, 12): its left half is the key colour.
    assert!(ui.mouse(Mouse::Move(40, 12)), "key colour: the map view");
    ui.mouse(Mouse::Move(42, 12));
    assert_eq!(ui.take_events(), ["hover 50"]);
}

#[test]
fn the_map_view_holds_the_capture_of_a_press_that_started_on_it() {
    let mut ui = interface("capture");
    ui.mouse(Mouse::Move(60, 40));
    assert!(ui.mouse(Mouse::Down));
    // Moving over a button while the map view has the capture does not highlight it.
    assert!(ui.mouse(Mouse::Move(3, 3)));
    assert!(ui.take_events().is_empty());
    assert_eq!(state(&ui, 20) & HIGHLIGHTED, 0);
    assert!(ui.mouse(Mouse::Up));
}

#[test]
fn a_layout_is_dragged_by_its_aliased_element_and_kept_inside_the_window() {
    let mut ui = interface("drag");
    ui.mouse(Mouse::Move(30, 30));
    ui.take_events();
    ui.mouse(Mouse::Down);
    assert_eq!(ui.take_events(), ["press 40"]);
    ui.mouse(Mouse::Move(33, 28));
    assert_eq!(ui.take_events(), ["drag 40 23 22"]);
    let layout = ui.tree.find(40).unwrap();
    assert_eq!(ui.tree.screen(layout), (23, 22));
    // Far right and down: stopped at the window, the pointer's excess is remembered.
    ui.mouse(Mouse::Move(63, 47));
    assert_eq!(ui.tree.screen(layout), (44, 28));
    // Coming back: the layout waits until the pointer is where it was cut.
    ui.mouse(Mouse::Move(60, 44));
    assert_eq!(ui.tree.screen(layout), (44, 28));
    ui.mouse(Mouse::Move(40, 30));
    assert_eq!(ui.tree.screen(layout), (30, 24));
    ui.mouse(Mouse::Up);
    ui.mouse(Mouse::Move(0, 0));
    assert_eq!(ui.tree.screen(layout), (30, 24), "dropped");
    // It came to the front of its layout.
    let root = &ui.tree.elements[0];
    assert_eq!(root.children.last(), Some(&layout));
}

#[test]
fn a_press_on_nothing_keeps_the_focus_and_one_on_a_control_moves_it() {
    let mut ui = interface("focus");
    let field = ui.tree.find(20).unwrap();
    ui.tree.elements[field].state |= FOCUSED;
    // The map view is a control that is hit: it takes the focus. Outside the window nothing is.
    ui.mouse(Mouse::Move(100, 100));
    ui.mouse(Mouse::Down);
    ui.mouse(Mouse::Up);
    assert_eq!(state(&ui, 20) & FOCUSED, FOCUSED);
    ui.mouse(Mouse::Move(60, 40));
    ui.mouse(Mouse::Down);
    assert_eq!(state(&ui, 20) & FOCUSED, 0);
}

#[test]
fn the_wheel_and_a_press_on_nothing_reach_the_map() {
    let mut ui = interface("wheel");
    ui.mouse(Mouse::Move(60, 40));
    assert!(ui.mouse(Mouse::Wheel(1)));
    ui.mouse(Mouse::Move(3, 3));
    assert!(!ui.mouse(Mouse::Wheel(-1)));
}
