use super::*;
use crate::research::{Category, Level, Program};
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{DISABLED, HIDDEN, Tree as Layout};

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            1 => "Research".into(),
            2 => "Conservation".into(),
            4001 => "Status".into(),
            4026 => "Done in %s day.".into(),
            4028 => "Done in %s days.".into(),
            22100 => "$".into(),
            23030 => "Not Funded".into(),
            23031 => "No Program".into(),
            23032 => "No Category".into(),
            23100 => "%s none".into(),
            23103 => "%s max".into(),
            n @ 10..=19 => format!("name{n}"),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

fn tga() -> Vec<u8> {
    let mut out = vec![0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 24, 0x20];
    out.extend([0, 255, 0]);
    out
}

/// The layout of the panel with the elements the refresh fills.
fn panel_in_game(name: &str) -> (Interface, Vfs) {
    let text = |id: i32| format!("[t{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n");
    let bar = |id: i32| {
        format!("[b{id}]\ntype=UIStatusImage\nid={id}\nx=0\ny=0\ndx=10\ndy=2\nimage=ui/g.tga\n")
    };
    let picture = |id: i32| format!("[p{id}]\ntype=UIImage\nid={id}\nx=0\ny=0\n");
    let button = |id: i32| format!("[u{id}]\ntype=UIButton\nid={id}\nx=0\ny=0\ndx=4\ndy=4\n");
    let layout = [4001, 4021, 4006, 4024, 4025, 4016, 4012]
        .map(text)
        .concat()
        + &[4018, 4014, 4027, 4017, 4013, 4026].map(bar).concat()
        + &[4019, 4020, 4029].map(picture).concat()
        + &[4003, 4004, 4008, 4009, 4010].map(button).concat()
        + "[l]\ntype=UIListBox\nid=4022\nx=0\ny=0\ndx=30\ndy=30\n";
    let layout = format!("[LayoutInfo]\nid=15\nx=0\ny=0\ndx=40\ndy=40\n{layout}");
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let files = [
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.into_bytes()),
        ("ui/g.tga", tga()),
    ];
    let install = Install::new(name, &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Layout::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    ui.show(15);
    (ui, vfs)
}

fn state(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

fn tip(ui: &Interface, id: i32) -> String {
    let help = &ui.tree.elements[ui.tree.find(id).unwrap()].help_id;
    help.first().map_or(String::new(), |&h| ui.string(h))
}

fn program(name: i32, cost: f32) -> Program {
    Program {
        name,
        cost,
        help: 4500,
        ..Program::default()
    }
}

/// Two branches, the first with two categories (the second of an expansion) at level 3 (max,
/// cost 2000, work 45), the second empty and unfunded.
fn tree() -> Tree {
    let levels = |n: i32| {
        [(23100, 0.0, 0.0), (23103, 2000.0, 45.0)].map(|(name, cost, work)| Level {
            name: name + n - n,
            cost,
            work,
        })
    };
    let category = |name, expansion, programs| Category {
        name,
        expansion,
        tick: true,
        programs,
        ..Category::default()
    };
    let mut tree = Tree {
        branches: vec![
            crate::research::Branch {
                name: 1,
                funding: levels(0).into(),
                level: 1,
                categories: vec![
                    category(10, 0, vec![program(11, 90.0)]),
                    category(12, 1, vec![program(13, 10.0)]),
                ],
                ..Default::default()
            },
            crate::research::Branch {
                name: 2,
                funding: levels(0).into(),
                ..Default::default()
            },
        ],
        ..Tree::default()
    };
    for b in &mut tree.branches {
        b.choose(&tree.off, &mut || 0);
    }
    tree
}

#[test]
fn the_status_tab_shows_the_programs_and_the_time_left() {
    let (mut ui, vfs) = panel_in_game("research-panel-status");
    let mut t = tree();
    let mut panel = Panel::default();
    let told = panel.refresh(&mut ui, Some(&t), &vfs).unwrap();
    assert!(told.starts_with("research tab 0 side 0 \"name11\" \"Done in 60 days.\" side 1 \"No Program\" \"Not Funded\""), "{told}");
    assert_eq!(ui.text(4001).unwrap(), b"Status");
    assert_eq!(ui.text(4016).unwrap(), b"name11");
    assert_eq!(ui.text(4012).unwrap(), b"No Program");
    // The bar and its background carry the tooltip; the other branch is not funded.
    assert_eq!(
        (tip(&ui, 4018), tip(&ui, 4017)),
        ("Done in 60 days.".into(), "Done in 60 days.".into())
    );
    assert_eq!(tip(&ui, 4014), "Not Funded");
    assert_eq!(panel.refresh(&mut ui, Some(&t), &vfs), None, "unchanged");
    // A day or less left takes the singular and rounds up: 0.5 day is 1 day.
    t.branches[0].categories[0].programs[0].progress = 89.0;
    panel.refresh(&mut ui, Some(&t), &vfs);
    assert_eq!(tip(&ui, 4018), "Done in 1 day.");
    t.branches[0].categories[0].programs[0].progress = 80.0;
    panel.refresh(&mut ui, Some(&t), &vfs);
    assert_eq!(tip(&ui, 4018), "Done in 7 days.", "10 * 30 / 45 = 6.67");
    // No funding is "Not Funded"; no tree and a tree of one branch leave the other side empty.
    t.branches[0].level = 0;
    panel.refresh(&mut ui, Some(&t), &vfs);
    assert_eq!(tip(&ui, 4018), "Not Funded");
    t.branches.truncate(1);
    panel.refresh(&mut ui, Some(&t), &vfs);
    assert_eq!(ui.text(4012).unwrap(), b"");
    panel.refresh(&mut ui, None, &vfs);
    assert_eq!(ui.text(4016).unwrap(), b"");
}

#[test]
fn a_category_tab_shows_the_list_the_funding_and_the_current_program() {
    let (mut ui, vfs) = panel_in_game("research-panel-tab");
    let mut t = tree();
    t.branches[0].categories[1].tick = false;
    ui.set_button(TABS[1], true, true);
    assert_eq!(branch_of_tab(&ui), Some(0));
    let mut panel = Panel::default();
    let told = panel.refresh(&mut ui, Some(&t), &vfs).unwrap();
    assert!(
        told.contains("ticks 10 funding \"$67 max\" category \"name10\" program \"name11\""),
        "{told}"
    );
    assert_eq!(ui.text(4001).unwrap(), b"Research");
    assert_eq!(ui.text(4021).unwrap(), b"Research");
    assert_eq!(ui.text(4024).unwrap(), b"name10");
    assert_eq!(ui.text(4025).unwrap(), b"name11");
    // At the last level the plus button is off, the minus button is on; the list is shown.
    assert_ne!(state(&ui, 4004) & DISABLED, 0);
    assert_eq!(state(&ui, 4003) & DISABLED, 0);
    assert_eq!(state(&ui, 4022) & HIDDEN, 0);
    t.fund(0, false);
    panel.refresh(&mut ui, Some(&t), &vfs);
    assert_eq!(state(&ui, 4004) & DISABLED, 0);
    assert_ne!(state(&ui, 4003) & DISABLED, 0);
    assert_eq!(tip(&ui, 4027), "Not Funded");
    // The second branch has no category and no program; its list is hidden.
    ui.set_button(TABS[1], false, true);
    ui.set_button(TABS[2], true, true);
    assert_eq!(branch_of_tab(&ui), Some(1));
    let told = panel.refresh(&mut ui, Some(&t), &vfs).unwrap();
    assert!(
        told.contains("ticks  funding \"$0 none\" category \"No Category\" program \"No Program\""),
        "{told}"
    );
    assert_ne!(state(&ui, 4022) & HIDDEN, 0);
    // A tab with no branch, or no tree, is empty; the Status tab has no branch.
    t.branches.truncate(1);
    let told = panel.refresh(&mut ui, Some(&t), &vfs).unwrap();
    assert!(
        told.contains("ticks  funding \"\" category \"\" program \"\""),
        "{told}"
    );
    assert_eq!(
        (
            state(&ui, 4003) & DISABLED != 0,
            state(&ui, 4004) & DISABLED != 0
        ),
        (true, true)
    );
    panel.refresh(&mut ui, None, &vfs);
    ui.set_button(TABS[2], false, true);
    ui.set_button(TABS[0], true, true);
    assert_eq!(branch_of_tab(&ui), None);
}
