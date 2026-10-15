use super::*;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::interface::Action;
use crate::ui::layout::{SELECTED, Tree};
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

/// A layout at (0, 0) with a region 22 x 24 whose buttons are 10 x 6 with spacing 2 and 4: two
/// columns and two whole rows, a bar to its right.
fn interface(name: &str) -> Interface {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n\
        [b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let panel = "[LayoutInfo]\nid=8\nx=0\ny=0\ndx=60\ndy=60\nstate=1\n\
        [r]\ntype=UIScrollingRegion\nid=100\nx=5\ny=7\ndx=22\ndy=24\nradioset=101\n\
        template=102\nscrollbar=103\nspacing=2\nspacing=4\n\
        [s]\ntype=UIRadioSet\nid=101\n\
        [t]\ntype=UIButton\nid=102\ndx=10\ndy=6\ntransparent=0\nstate=256\nstate=4096\n\
        [u]\ntype=UIScrollBar\nid=103\ndx=3\ndy=10\n";
    let install = Install::new(
        &format!("region-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/p.lyt", panel.as_bytes().to_vec()),
        ],
    );
    let mut tree = Tree::load(&Vfs::open(&install.0).unwrap(), "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(100, 100, |_| 12);
    let mut ui = Interface::new(tree, Box::new(NoTexts));
    ui.show(8);
    ui
}

fn button(k: usize) -> i32 {
    COPY_IDS + 100 * 1000 + k as i32
}

/// The rows of buttons that show, as the first item of each: a row is shown when its first
/// button is.
fn shown(ui: &Interface, n: usize) -> Vec<usize> {
    (0..n)
        .filter(|&k| {
            let i = ui.tree.find(button(k)).unwrap();
            ui.tree.elements[i].state & (HIDDEN | NEVER_SHOWN) == 0
        })
        .collect()
}

fn at(ui: &Interface, k: usize) -> (i32, i32) {
    ui.tree.screen(ui.tree.find(button(k)).unwrap())
}

fn bar(ui: &Interface) -> (i32, i32, i32) {
    match &ui.tree.elements[ui.tree.find(103).unwrap()].kind {
        Kind::ScrollBar(s) => (s.bar.min, s.bar.max, s.bar.position),
        _ => unreachable!(),
    }
}

#[test]
fn a_region_makes_a_button_per_item_in_two_columns_of_whole_rows() {
    let mut ui = interface("grid");
    assert!(ui.set_items(100, &[None; 7]));
    // 7 items in 2 columns: 4 rows, 2 fit; the grid fills the width and is centred in the
    // height of the 22 x 24 region at (5, 7): 4 above and below the 16 of two rows.
    assert_eq!(shown(&ui, 7), [0, 1, 2, 3]);
    assert_eq!(at(&ui, 0), (5, 7 + 4));
    assert_eq!(at(&ui, 1), (5 + 12, 7 + 4));
    assert_eq!(at(&ui, 2), (5, 7 + 4 + 10));
    // The bar sits right of the region, as long as it, and ranges over the 2 rows beyond.
    let bar_at = ui.tree.rect(ui.tree.find(103).unwrap());
    assert_eq!(bar_at, (27, 7, 3, 24));
    assert_eq!(bar(&ui), (0, 2, 0));
    // The buttons join the radio set.
    let set = ui.tree.find(101).unwrap();
    assert_eq!(
        ui.tree.elements[set].kind,
        Kind::RadioSet((0..7).map(button).collect())
    );
    assert_eq!(ui.item_of(100, button(5)), Some(5));
    assert_eq!(ui.item_of(100, 102), None, "the template is no item");
    assert_eq!(ui.item_of(99, button(0)), None);
}

#[test]
fn the_bar_scrolls_by_rows_and_what_does_not_show_is_never_shown() {
    let mut ui = interface("scroll");
    ui.set_items(100, &[None; 7]);
    let region = ui.tree.find(100).unwrap();
    ui.scroll_owner(region, 1);
    assert_eq!(shown(&ui, 7), [2, 3, 4, 5]);
    assert_eq!(at(&ui, 2), (5, 7 + 4), "row 1 is now the first");
    assert_eq!(bar(&ui).2, 1);
    ui.scroll_owner(region, 99);
    assert_eq!(shown(&ui, 7), [4, 5, 6]);
    assert_eq!(bar(&ui).2, 2, "the last row is the last first row");
    // Showing the layout again does not bring the hidden rows back.
    ui.tree.hide(ui.tree.find(8).unwrap());
    ui.show(8);
    assert_eq!(shown(&ui, 7), [4, 5, 6]);
    // The wheel over a button turns the region: two rows a notch, up for a positive number.
    ui.wheel(ui.tree.find(button(4)).unwrap(), 1);
    assert_eq!(shown(&ui, 7), [0, 1, 2, 3]);
}

#[test]
fn a_refill_reuses_the_buttons_clears_the_selection_and_hides_an_empty_region() {
    let mut ui = interface("refill");
    ui.set_items(100, &[None; 5]);
    assert!(ui.select_item(100, Some(4)));
    let region = ui.tree.find(100).unwrap();
    assert_eq!(
        shown(&ui, 5),
        [2, 3, 4],
        "the selection is scrolled into view"
    );
    let made = ui.tree.elements.len();
    ui.set_items(100, &[None; 3]);
    assert_eq!(ui.tree.elements.len(), made, "no new button");
    assert_eq!(shown(&ui, 5), [0, 1, 2]);
    let selected = (0..5)
        .filter(|&k| ui.tree.elements[ui.tree.find(button(k)).unwrap()].state & SELECTED != 0)
        .count();
    assert_eq!(selected, 0);
    assert!(!ui.select_item(100, Some(3)), "no such item");
    assert!(ui.select_item(100, None));
    assert_eq!(ui.tree.elements[region].state & HIDDEN, 0);
    ui.set_items(100, &[]);
    assert_eq!(ui.tree.elements[region].state & HIDDEN, HIDDEN);
    assert!(shown(&ui, 5).is_empty());
    assert_eq!(bar(&ui), (0, 0, 0));
    assert!(!ui.set_items(99, &[None]), "no such region");
    assert!(!ui.set_items(102, &[None]), "a button is no region");
}

#[test]
fn at_most_a_thousand_items_are_made() {
    let mut ui = interface("many");
    ui.set_items(100, &vec![None; MAX_ITEMS + 50]);
    match &ui.tree.elements[ui.tree.find(100).unwrap()].kind {
        Kind::ScrollingRegion(r) => assert_eq!((r.items, r.copies.len()), (MAX_ITEMS, MAX_ITEMS)),
        _ => unreachable!(),
    }
    assert!(ui.tree.find(button(MAX_ITEMS - 1)).is_some());
}

#[test]
fn pressing_an_item_selects_it_and_deselects_the_others() {
    let mut ui = interface("press");
    ui.set_items(100, &[None; 4]);
    // The first button is at (5, 11): press and release on it, then on the second.
    for (x, y, id) in [(8, 13, 0), (20, 13, 1)] {
        ui.mouse(crate::app::Mouse::Move(x, y));
        ui.mouse(crate::app::Mouse::Down);
        ui.mouse(crate::app::Mouse::Up);
        assert_eq!(ui.take_selected(), [button(id)]);
    }
    let state = |k| ui.tree.elements[ui.tree.find(button(k)).unwrap()].state & SELECTED;
    assert_eq!((state(0), state(1)), (0, SELECTED), "the radio set moved");
    assert!(
        ui.take_actions()
            .iter()
            .all(|a| !matches!(a, Action::Row(..)))
    );
}
