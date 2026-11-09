use super::*;
use crate::fixtures::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{Kind as Element, Tree};
use crate::unit::testing::{centre, entity, world};

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            3825 => "All".into(),
            3826 => "Angry".into(),
            4233 => "Keepers".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The animal window (list 3805, counts 3816 to 3824 without the 3838's), the guest window's list
/// and the staff window's list with only the counts of its first four filters, each a list of
/// 5 lines of 12 pixels (a row is 12 pixels).
fn layout() -> String {
    let count = |id: i32| {
        format!(
            "[c{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=10\ndy=4\nforecolor=1\nforecolor=2\nforecolor=3\n"
        )
    };
    let list = |id: i32| format!("[l{id}]\ntype=UIListBox\nid={id}\nx=0\ny=0\ndx=60\ndy=60\n");
    let title = |id: i32| format!("[t{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n");
    let counts = [
        3816, 3818, 3820, 3822, 3824, 3839, 3716, 3718, 3720, 3722, 3724, 3726,
    ]
    .into_iter()
    .chain([4216, 4218, 4220, 4222])
    .map(count)
    .collect::<String>();
    let lists = [3805, 3705, 4205].map(list).concat() + &[3835, 3727, 4235].map(title).concat();
    format!("[LayoutInfo]\nid=16\nx=0\ny=0\ndx=80\ndy=80\n{counts}{lists}")
}

fn interface(name: &str) -> (Interface, Vfs) {
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let install = Install::new(
        name,
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/p.lyt", layout().into_bytes()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 100, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    ui.show(16);
    (ui, vfs)
}

fn named(mut e: Entity, name: &str) -> Entity {
    e.common.as_mut().unwrap().name = name.into();
    e
}

use crate::zoo::Entity;

fn animals() -> World {
    world((6, 6), |_, e| {
        e.push(named(
            entity("animals", "zebra", 5, centre((1, 1)), 0),
            "Zed",
        ));
        e.push(named(
            entity("animals", "lion", 2, centre((2, 1)), 0),
            "Abe",
        ));
        e.push(named(
            entity("animals", "lion", 9, centre((3, 1)), 0),
            "Abe",
        ));
        e.push(named(entity("animals", "lion", 7, centre((3, 2)), 0), "Ab"));
        e.push(named(
            entity("animals", "lion", 8, centre((4, 2)), 0),
            "Ab\u{e9}",
        ));
    })
}

fn animal(world: &mut World, id: u32) -> &mut crate::animal::Animal {
    match &mut world.units.get_mut(id).unwrap().part {
        Part::Animal(a) => a,
        _ => panic!("not an animal"),
    }
}

fn ids(list: &[Entry]) -> Vec<u32> {
    list.iter().map(|e| e.id).collect()
}

/// The flags of the hidden rows of list `id`.
fn hidden(ui: &Interface, id: i32) -> Vec<bool> {
    match &ui.tree.elements[ui.tree.find(id).unwrap()].kind {
        Element::ListBox(l) => (0..l.rows.rows.len()).map(|n| l.rows.hidden(n)).collect(),
        _ => panic!("not a list"),
    }
}

fn count(ui: &Interface, id: i32) -> String {
    String::from_utf8(ui.text(id).unwrap()).unwrap()
}

#[test]
fn the_animals_are_sorted_by_name_bytes_then_id_and_a_dying_one_is_left_out() {
    let mut w = animals();
    // Ab, Abe (2), Abe (9), the accented one (after every ASCII letter of its prefix), Zed.
    assert_eq!(ids(&entries(ANIMALS, &w)), [7, 2, 9, 8, 5]);
    animal(&mut w, 2).dying = true;
    assert_eq!(ids(&entries(ANIMALS, &w)), [7, 9, 8, 5]);
    // The guest and staff windows hold no animal.
    assert!(entries(GUESTS, &w).is_empty() && entries(STAFF, &w).is_empty());
}

#[test]
fn the_guests_and_the_staff_are_in_the_order_of_the_entity_list() {
    let w = world((6, 6), |_, e| {
        e.push(entity("guests", "man", 30, centre((1, 1)), 0));
        e.push(entity("keeper", "keeper", 4, centre((2, 1)), 0));
        e.push(entity("guests", "man", 10, centre((3, 1)), 0));
        e.push(entity("helicopter", "helicopter", 20, centre((3, 2)), 0));
        e.push(entity("maint", "maint", 3, centre((4, 2)), 0));
        e.push(entity("animals", "lion", 8, centre((4, 3)), 0));
    });
    assert_eq!(ids(&entries(GUESTS, &w)), [30, 10]);
    assert_eq!(
        ids(&entries(STAFF, &w)),
        [4, 20, 3],
        "the helicopter is staff"
    );
}

/// The window refilled with every filter in turn: the count of each.
fn counts(which: usize, w: &World, ui: &mut Interface, vfs: &Vfs) -> Vec<String> {
    let mut win = Window::default();
    let cat = Catalogue::default();
    win.refill(which, entries(which, w), (ui, w, &cat, vfs), None);
    SPECS[which]
        .filters
        .iter()
        .map(|f| count(ui, f.count))
        .collect()
}

#[test]
fn each_animal_filter_tests_its_threshold_and_counts_the_whole_list() {
    let (mut ui, vfs) = interface("unitlist-animals");
    let mut w = animals();
    // The threshold is 50: angry at or below -101, hungry above 50.
    animal(&mut w, 2).happiness = -101;
    animal(&mut w, 9).happiness = -100;
    animal(&mut w, 5).hunger = 51;
    animal(&mut w, 7).hunger = 50;
    animal(&mut w, 8).at_minimum_health = true;
    animal(&mut w, 8).escaped = true;
    animal(&mut w, 5).escaped = true;
    // On no exhibit the rating shown is 0, under every preference: each is unhappy, but not an
    // egg, and not one that is carried.
    animal(&mut w, 7).egg = true;
    w.units.get_mut(9).unwrap().carried = true;
    // all, angry, sick, hungry, unhappy with the exhibit, escaped
    assert_eq!(
        counts(ANIMALS, &w, &mut ui, &vfs),
        ["5", "1", "1", "1", "3", "2"]
    );
}

#[test]
fn the_filter_in_use_hides_the_rows_that_fail_it_and_the_title_and_colours_follow() {
    let (mut ui, vfs) = interface("unitlist-choose");
    let mut w = animals();
    animal(&mut w, 5).hunger = 99;
    animal(&mut w, 9).hunger = 99;
    let cat = Catalogue::default();
    let mut win = Window::default();
    win.choose(ANIMALS, 3, &mut ui);
    assert_eq!(
        ui.fore_color(3822),
        Some([255; 3]),
        "the count of the filter in use is white"
    );
    assert_eq!(
        ui.fore_color(3816),
        Some([1, 2, 3]),
        "the others keep the colour they had"
    );
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        None,
    );
    // Ab, Abe 2, Abe 9, Ab-acute, Zed: a row keeps its place, hidden or not.
    assert_eq!(hidden(&ui, 3805), [true, true, false, true, false]);
    assert_eq!(ui.row_text(3805, 2), Some(&b"Abe"[..]));
    assert_eq!(count(&ui, 3822), "2");
    assert_eq!(
        count(&ui, 3816),
        "5",
        "the counts are those of the whole list"
    );
    // Back to "all": the old count has its colour again, the title is the filter's.
    win.choose(ANIMALS, 0, &mut ui);
    assert_eq!(ui.fore_color(3822), Some([1, 2, 3]));
    assert_eq!(ui.fore_color(3816), Some([255; 3]));
    assert_eq!(ui.text(3835).unwrap(), b"All");
    // An index past the last filter is the last one; the missing counts of a window are skipped.
    win.choose(ANIMALS, 99, &mut ui);
    assert_eq!(win.filter, 5);
    win.choose(STAFF, 8, &mut ui);
    win.refill(STAFF, Vec::new(), (&mut ui, &w, &cat, &vfs), None);
}

#[test]
fn each_guest_need_filter_is_strictly_above_the_threshold() {
    let (mut ui, vfs) = interface("unitlist-guests");
    let mut w = world((6, 6), |_, e| {
        (30..34).for_each(|id| e.push(entity("guests", "man", id, centre((1, 1)), 0)));
    });
    let set =
        |w: &mut World, id: u32, values: [i32; 5]| match &mut w.units.get_mut(id).unwrap().part {
            Part::Guest(g) => g.values = values,
            _ => panic!("not a guest"),
        };
    // happiness, hunger, thirst, energy, bathroom: 50 is the threshold, never counted.
    set(&mut w, 30, [-101, 50, 50, 50, 50]);
    set(&mut w, 31, [-100, 51, 51, 51, 51]);
    set(&mut w, 32, [0, 51, 0, 0, 0]);
    set(&mut w, 33, [0, 0, 99, 0, 99]);
    // all, angry, thirsty, hungry, restroom, tired
    assert_eq!(
        counts(GUESTS, &w, &mut ui, &vfs),
        ["4", "1", "2", "2", "2", "1"]
    );
}

#[test]
fn the_staff_filters_go_by_the_type_name_and_the_assignments_of_keepers_and_guides() {
    let (mut ui, vfs) = interface("unitlist-staff");
    let w = world((6, 6), |_, e| {
        for (class, name, id) in [
            ("keeper", "keeper", 1),
            ("keeper", "keeper", 2),
            ("keeper", "scient", 3),
            ("keeper", "trainer", 4),
            ("tour", "tour", 5),
            ("maint", "maint", 6),
            ("helicopter", "helicopter", 7),
        ] {
            e.push(entity(class, name, id, centre((1, 1)), 0));
        }
    });
    let mut w = w;
    for (id, n) in [(1, 1), (3, 2), (5, 1), (6, 1), (7, 1)] {
        if let Part::Staff(s) = &mut w.units.get_mut(id).unwrap().part {
            s.assigned = vec![99; n];
        }
    }
    // all, keepers, tour guides, maintenance, assigned, unassigned, scientists, DRT, marine
    // The worker and the helicopter are neither assigned nor unassigned.
    let mut win = Window::default();
    let cat = Catalogue::default();
    win.refill(STAFF, entries(STAFF, &w), (&mut ui, &w, &cat, &vfs), None);
    let shown: Vec<String> = [4216, 4218, 4220, 4222].map(|id| count(&ui, id)).into();
    assert_eq!(shown, ["7", "2", "1", "1"]);
    let mut tests = Tests::new(&w, &cat, None);
    let list = entries(STAFF, &w);
    let mut total = |i: usize| list.iter().filter(|e| tests.pass(STAFF, i, e)).count();
    assert_eq!([4, 5, 6, 7, 8].map(&mut total), [3, 2, 1, 1, 1]);
}

#[test]
fn the_helicopter_row_shows_its_base_and_stays_as_it_was_without_one() {
    let (mut ui, vfs) = interface("unitlist-heli");
    let mut w = world((6, 6), |_, e| {
        e.push(named(
            entity("helicopter", "helicopter", 7, centre((1, 1)), 0),
            "Heli",
        ));
        e.push(named(
            entity("building", "drt", 8, centre((2, 2)), 0),
            "Base",
        ));
        e.push(named(
            entity("objects", "tree", 9, centre((3, 3)), 0),
            "Tree",
        ));
    });
    let cat = Catalogue::default();
    let mut win = Window::default();
    let fill = |w: &World, ui: &mut Interface, win: &mut Window, sel| {
        win.refill(STAFF, entries(STAFF, w), (ui, w, &cat, &vfs), sel);
    };
    let base = |w: &mut World, id: u32| match &mut w.units.get_mut(7).unwrap().part {
        Part::Staff(s) => s.heli.base = id,
        _ => panic!(),
    };
    fill(&w, &mut ui, &mut win, None);
    assert_eq!(ui.row_text(4205, 0), Some(&b"Heli"[..]), "no base");
    base(&mut w, 9);
    fill(&w, &mut ui, &mut win, None);
    assert_eq!(
        ui.row_text(4205, 0),
        Some(&b"Heli"[..]),
        "a base that is no building"
    );
    base(&mut w, 8);
    fill(&w, &mut ui, &mut win, Some(8));
    assert_eq!(ui.row_text(4205, 0), Some(&b"Base"[..]));
    assert_eq!(
        ui.selected_row(4205),
        None,
        "a refill selects no row of its own"
    );
    ui.select_row(4205, Some(0));
    fill(&w, &mut ui, &mut win, Some(8));
    assert_eq!(
        ui.selected_row(4205),
        Some(0),
        "the base is the selected entity of the row: the selection is left as it is"
    );
    // The base goes: the row is as it was. A filter it fails gives it its own name, hidden.
    base(&mut w, 0);
    fill(&w, &mut ui, &mut win, None);
    assert_eq!(ui.row_text(4205, 0), Some(&b"Base"[..]));
    base(&mut w, 8);
    win.filter = 1;
    fill(&w, &mut ui, &mut win, None);
    assert_eq!(
        (ui.row_text(4205, 0), hidden(&ui, 4205)[0]),
        (Some(&b"Heli"[..]), true)
    );
}

#[test]
fn a_refill_selects_the_selected_animals_row_keeps_the_scroll_and_hides_nothing_unseen() {
    let (mut ui, vfs) = interface("unitlist-select");
    let mut w = world((6, 6), |_, e| {
        for id in 1..=12 {
            e.push(named(
                entity("animals", "lion", id, centre((1, 1)), 0),
                &format!("A{id:02}"),
            ));
        }
    });
    let cat = Catalogue::default();
    let mut win = Window::default();
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        Some(4),
    );
    assert_eq!(
        ui.selected_row(3805),
        Some(3),
        "the row of the selected entity"
    );
    // Scrolled down, a refill puts it back where it was; a selected entity the filter hides
    // selects no row.
    ui.select_row(3805, Some(11));
    let top = ui.list_top(3805);
    assert!(top > 0);
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        Some(4),
    );
    assert_eq!(ui.list_top(3805), top);
    animal(&mut w, 4).hunger = 0;
    win.filter = 3;
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        Some(4),
    );
    assert_eq!(ui.selected_row(3805), None);
    // The selection of an entity outside the list selects none.
    win.filter = 0;
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        Some(99),
    );
    assert_eq!(ui.selected_row(3805), None);
}

#[test]
fn the_selected_entity_moves_the_row_and_the_old_ones_goes() {
    let (mut ui, vfs) = interface("unitlist-follow");
    let w = animals();
    let cat = Catalogue::default();
    let mut win = Window::default();
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        None,
    );
    win.follow(ANIMALS, &mut ui, Some(9));
    assert_eq!(ui.selected_row(3805), Some(2));
    win.follow(ANIMALS, &mut ui, Some(5));
    assert_eq!(ui.selected_row(3805), Some(4));
    // Nothing selected, or an entity that is not in the list: no row.
    win.follow(ANIMALS, &mut ui, None);
    assert_eq!(ui.selected_row(3805), None);
    win.follow(ANIMALS, &mut ui, Some(99));
    assert_eq!(ui.selected_row(3805), None);
}

#[test]
fn the_refill_is_due_when_asked_for_never_run_or_after_10_s_and_only_on_a_tick() {
    let mut win = Window::default();
    // Never run: due at the first tick, and not before.
    assert!(!win.due(0, false));
    assert!(win.due(0, true));
    assert!(!win.due(9_999, true));
    assert!(win.due(10_000, true));
    // A rename asks for it at the next tick; a paused or ticking-less look runs nothing.
    win.force = true;
    assert!(!win.due(10_001, false));
    assert!(win.due(10_001, true) && !win.force);
    // A filter chosen or the window shown runs at once, ticking or not.
    win.now = true;
    assert!(win.due(10_002, false) && !win.now);
    assert!(
        !win.due(20_000, true),
        "the time is 10 s from the last refill"
    );
    assert!(win.due(20_002, true));
    // A huge clock never overflows.
    assert!(win.due(u64::MAX, true));
}

#[test]
fn a_list_notices_a_unit_that_came_or_went_and_a_rename() {
    let mut w = animals();
    let mut win = Window::default();
    let (mut ui, vfs) = interface("unitlist-changed");
    let cat = Catalogue::default();
    assert!(win.changed(&entries(ANIMALS, &w)), "nothing listed yet");
    win.refill(
        ANIMALS,
        entries(ANIMALS, &w),
        (&mut ui, &w, &cat, &vfs),
        None,
    );
    assert!(!win.changed(&entries(ANIMALS, &w)) && !win.renamed(&entries(ANIMALS, &w)));
    let at = w.find(5).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().name = "Zebu".into();
    assert!(!win.changed(&entries(ANIMALS, &w)) && win.renamed(&entries(ANIMALS, &w)));
    animal(&mut w, 5).dying = true;
    assert!(win.changed(&entries(ANIMALS, &w)));
}

#[test]
fn an_extreme_threshold_never_overflows() {
    let (mut ui, vfs) = interface("unitlist-extreme");
    let mut w = animals();
    for t in [0, -1, i32::MIN, i32::MAX] {
        w.units.class.ui_threshold = t;
        animal(&mut w, 5).happiness = i32::MIN;
        animal(&mut w, 5).hunger = i32::MAX;
        let shown = counts(ANIMALS, &w, &mut ui, &vfs);
        assert_eq!(shown[0], "5", "threshold {t}");
    }
}
