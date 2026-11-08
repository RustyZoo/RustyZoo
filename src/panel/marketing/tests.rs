use super::*;
use crate::fixtures::Install;
use crate::marketing::Level;
use crate::ui::draw::Strings;
use crate::ui::layout::{DISABLED, Tree as Layout};
use crate::vfs::Vfs;

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            22100 => "$".into(),
            23100 => "%s none".into(),
            23101 => "%s min".into(),
            23102 => "%s normal".into(),
            23103 => "%s max".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The zoo status window (layout 14) with the three marketing elements, hidden or shown.
fn window(name: &str, elements: &[i32]) -> Interface {
    let text = |id: i32| format!("[t{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n");
    let button = |id: i32| format!("[u{id}]\ntype=UIButton\nid={id}\nx=0\ny=0\ndx=4\ndy=4\n");
    let body: String = elements
        .iter()
        .map(|&id| if id == TEXT { text(id) } else { button(id) })
        .collect();
    let layout = format!("[LayoutInfo]\nid=14\nx=0\ny=0\ndx=40\ndy=40\n{body}");
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/w.lyt\n";
    let files = [
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/w.lyt", layout.into_bytes()),
    ];
    let install = Install::new(name, &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Layout::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    Interface::new(tree, Box::new(Texts))
}

fn level(name: i32, cost: f32) -> Level {
    Level {
        name,
        cost,
        benefit: 0,
    }
}

/// A marketing over `levels` at `level`, through the file word.
fn marketing(levels: Vec<Level>, level: u32) -> Marketing {
    let mut m = Marketing::with_levels(levels);
    m.load(level);
    m
}

fn disabled(ui: &Interface, id: i32) -> bool {
    ui.tree.elements[ui.tree.find(id).unwrap()].state & DISABLED != 0
}

fn text(ui: &Interface) -> String {
    String::from_utf8(ui.text(TEXT).unwrap()).unwrap()
}

const ALL: [i32; 3] = [LESS, MORE, TEXT];

fn four() -> Vec<Level> {
    vec![
        level(23100, 0.0),
        level(23101, 200.0),
        level(23102, 500.0),
        level(23103, 1000.0),
    ]
}

#[test]
fn the_text_is_the_name_over_the_whole_monthly_cost_without_grouping() {
    let mut ui = window("marketing-text", &ALL);
    ui.show(14);
    let mut f = Funding::default();
    let m = marketing(four(), 0);
    let line = f.refresh(&mut ui, &m, 0, false).unwrap();
    assert_eq!(text(&ui), "$0 none");
    assert_eq!(line, "funding \"$0 none\" less false more true");
    for (level, want) in [(1, "$200 min"), (3, "$1000 max")] {
        f.force = true;
        f.refresh(&mut ui, &marketing(four(), level), 0, false);
        assert_eq!(text(&ui), want);
    }
    // The cost is rounded to the nearest unit, halves to even, and a big one is not grouped.
    for (cost, want) in [
        (0.5, "$0 min"),
        (1.5, "$2 min"),
        (2.5, "$2 min"),
        (1234567.0, "$1234567 min"),
    ] {
        f.force = true;
        f.refresh(&mut ui, &marketing(vec![level(23101, cost)], 0), 0, false);
        assert_eq!(text(&ui), want, "{cost}");
    }
}

#[test]
fn the_buttons_are_disabled_at_the_ends_of_the_list() {
    let mut ui = window("marketing-buttons", &ALL);
    ui.show(14);
    let mut f = Funding::default();
    let mut seen = Vec::new();
    for level in [0, 1, 3] {
        f.force = true;
        f.refresh(&mut ui, &marketing(four(), level), 0, false);
        seen.push((disabled(&ui, LESS), disabled(&ui, MORE)));
    }
    assert_eq!(seen, [(true, false), (false, false), (false, true)]);
    // One level: both are disabled. An empty list: the same and an empty text.
    for levels in [vec![level(23100, 5.0)], vec![]] {
        f.force = true;
        f.refresh(&mut ui, &marketing(levels.clone(), 0), 0, false);
        assert_eq!((disabled(&ui, LESS), disabled(&ui, MORE)), (true, true));
        assert_eq!(text(&ui), if levels.is_empty() { "" } else { "$5 none" });
    }
}

#[test]
fn no_configuration_leaves_the_three_elements_alone() {
    let mut ui = window("marketing-none", &ALL);
    ui.show(14);
    ui.set_text(TEXT, b"as it was");
    let mut f = Funding::default();
    assert_eq!(f.refresh(&mut ui, &Marketing::default(), 0, true), None);
    assert_eq!(text(&ui), "as it was");
    assert!(!disabled(&ui, LESS) && !disabled(&ui, MORE));
}

#[test]
fn a_missing_element_is_skipped_and_the_rest_written() {
    let mut ui = window("marketing-missing", &[TEXT]);
    ui.show(14);
    let mut f = Funding::default();
    assert!(
        f.refresh(&mut ui, &marketing(four(), 1), 0, false)
            .is_some()
    );
    assert_eq!(text(&ui), "$200 min");
    let mut ui = window("marketing-missing2", &[LESS, MORE]);
    ui.show(14);
    let mut f = Funding::default();
    assert!(
        f.refresh(&mut ui, &marketing(four(), 3), 0, false)
            .is_some()
    );
    assert!(!disabled(&ui, LESS) && disabled(&ui, MORE));
}

#[test]
fn it_runs_when_the_window_is_shown_every_10_seconds_and_after_a_button() {
    let mut ui = window("marketing-clock", &ALL);
    let (mut f, m) = (Funding::default(), marketing(four(), 1));
    // Hidden window: nothing, not even at the interface update.
    assert_eq!(f.refresh(&mut ui, &m, 0, true), None);
    ui.show(14);
    // Shown: once at once; the same text is not told twice.
    assert!(f.refresh(&mut ui, &m, 5, false).is_some());
    // The first update runs at once and sets the next one 10 s later.
    let other = marketing(four(), 2);
    assert_eq!(f.refresh(&mut ui, &other, 6, false), None);
    assert_eq!(text(&ui), "$200 min");
    assert!(f.refresh(&mut ui, &other, 6, true).is_some());
    assert_eq!(text(&ui), "$500 normal");
    let third = marketing(four(), 3);
    assert_eq!(f.refresh(&mut ui, &third, 10_005, true), None);
    assert_eq!(text(&ui), "$500 normal");
    assert!(f.refresh(&mut ui, &third, 10_006, true).is_some());
    assert_eq!(text(&ui), "$1000 max");
    // A button asks for it at once.
    f.force = true;
    assert!(
        f.refresh(&mut ui, &marketing(four(), 0), 10_007, false)
            .is_some()
    );
    assert_eq!(text(&ui), "$0 none");
    // Hidden and shown again: written at once.
    ui.tree.hide(ui.tree.find(14).unwrap());
    f.refresh(&mut ui, &m, 10_008, false);
    ui.show(14);
    assert!(f.refresh(&mut ui, &m, 10_009, false).is_some());
    assert_eq!(text(&ui), "$200 min");
}
