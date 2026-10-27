use super::*;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{Kind, Tree};

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            101 => "Animal care",
            102 => "Exhibit design",
            _ => "",
        }
        .into()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

const FILE: &str = "[Version]\nversion = 1\n[Awards]\naward = a\naward = b\naward = c\n\
    [a]\nid = 1\nnameID = 102\ntooltipID = 5000\n\
    [b]\nid = 8\nnameID = 101\ntooltipID = 0\nicon = missing/icon\n\
    [c]\nid = 2147483647\nnameID = 101\ntooltipID = 7\n";

/// The status window (layout 14) holding the list box of the page, and the install with the
/// definitions of the three awards 1, 8 and the largest id.
fn setup(name: &str) -> (Interface, Vfs, Definitions, Install) {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [c]\ntype=UILayout\nlayout=ui/status.lyt\n";
    let status = "[LayoutInfo]\nid=14\nx=0\ny=0\ndx=64\ndy=48\nstate=1\n\
        [l]\ntype=UIListBox\nid=4124\nx=5\ny=5\ndx=50\ndy=40\nminiconwidth=8\n";
    let files = Install::new(
        &format!("status-awards-{name}"),
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/status.lyt", status.as_bytes().to_vec()),
            ("award001.cfg", FILE.as_bytes().to_vec()),
        ],
    );
    let vfs = Vfs::open(&files.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    assert_eq!(tree.failure, None);
    tree.resolve(64, 48, |_| 12);
    let defs = Definitions::load(&vfs);
    let mut ui = Interface::new(tree, Box::new(Texts));
    assert!(ui.show(WINDOW));
    (ui, vfs, defs, files)
}

/// The rows of the list: the text and the help.
fn rows(ui: &Interface) -> Vec<(String, i32)> {
    let Kind::ListBox(l) = &ui.tree.elements[ui.tree.find(LIST).unwrap()].kind else {
        panic!("not a list");
    };
    let text = |r: &Row| String::from_utf8(r.text.clone()).unwrap();
    l.rows.rows.iter().map(|r| (text(r), r.help)).collect()
}

fn hide(ui: &mut Interface) {
    let i = ui.tree.find(WINDOW).unwrap();
    ui.tree.hide(i);
}

#[test]
fn the_list_has_a_row_for_each_held_award_that_has_a_definition_in_the_held_order() {
    let (mut ui, vfs, defs, _files) = setup("rows");
    // The held list is largest first; 0, -1, the smallest id and 99 have no definition, 8 has
    // an icon that is missing, the largest id is a huge one.
    let held = [i32::MAX, 99, 8, 1, 0, -1, i32::MIN];
    Awards::default().refresh(&mut ui, &held, &defs, &vfs);
    assert_eq!(
        rows(&ui),
        [
            ("Animal care".to_string(), 7),
            ("Animal care".to_string(), 0),
            ("Exhibit design".to_string(), 5000),
        ]
    );
}

#[test]
fn an_empty_held_list_or_no_definitions_give_an_empty_list() {
    let (mut ui, vfs, defs, _files) = setup("empty");
    Awards::default().refresh(&mut ui, &[], &defs, &vfs);
    assert!(rows(&ui).is_empty());
    Awards::default().refresh(&mut ui, &[1, 8], &Definitions::default(), &vfs);
    assert!(rows(&ui).is_empty());
}

#[test]
fn the_list_is_rebuilt_when_the_window_is_shown_and_only_then() {
    let (mut ui, vfs, defs, _files) = setup("show");
    let mut page = Awards::default();
    // Hidden: nothing is written.
    hide(&mut ui);
    page.refresh(&mut ui, &[1], &defs, &vfs);
    assert!(rows(&ui).is_empty());
    // Shown: written at once.
    ui.show(WINDOW);
    page.refresh(&mut ui, &[1], &defs, &vfs);
    assert_eq!(rows(&ui).len(), 1);
    // An award given while it stays open appears at the next opening.
    page.refresh(&mut ui, &[8, 1], &defs, &vfs);
    assert_eq!(rows(&ui).len(), 1);
    hide(&mut ui);
    page.refresh(&mut ui, &[8, 1], &defs, &vfs);
    assert_eq!(rows(&ui).len(), 1);
    ui.show(WINDOW);
    page.refresh(&mut ui, &[8, 1], &defs, &vfs);
    assert_eq!(rows(&ui).len(), 2);
}
