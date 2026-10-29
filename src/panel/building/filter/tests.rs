use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::panel::follow::tests as follow;
use crate::sprite::tests::Install;
use crate::timing::{DAY_MS, file_time};
use crate::ui::draw::Strings;
use crate::ui::layout::{DISABLED, HIDDEN, Tree};
use crate::unit::testing::{centre, entity, world};
use crate::zoo::Filter;

const DAY: i64 = DAY_MS as i64;

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            4999 => "Oak".into(),
            4914 => "Elm".into(),
            4913 => "Fir".into(),
            4916 => "%s mo".into(),
            SYMBOL => "$".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The panel with the elements the refresh fills, a map with one filter (entity 1) of a type with
/// the information image of [`follow::members`] and the integer settings `ints`.
fn panel_in(name: &str, ints: &str) -> (Interface, Vfs, World, Catalogue) {
    let ids = [
        ("UIImage", [4903].as_slice()),
        ("UIEditableText", &[4904]),
        ("UIButton", &[4906]),
        ("UIText", &[4913, 4916, 4918, 4921, 4923, 4925]),
    ];
    let elements: String = ids
        .iter()
        .flat_map(|(kind, ids)| ids.iter().map(move |id| (kind, id)))
        .map(|(kind, id)| format!("[e{id}]\ntype={kind}\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n"))
        .collect();
    let text = format!("[LayoutInfo]\nid={LAYOUT}\nx=0\ny=0\ndx=90\ndy=90\n{elements}");
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", text.into_bytes()),
    ];
    let members = follow::members();
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = Install::new(name, &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(200, 120, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    ui.show(LAYOUT);
    let text = format!(
        "[Characteristics/Integers]\n{ints}[Characteristics/Strings]\ncInfoImageName = ui/z/z\n\
         [Animations]\nidle = ui/z/z.ani\n"
    );
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "filter",
        type_name: b"filter1".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let world = world((4, 4), |_, e| {
        e.push(entity("filter", "filter1", 1, centre((1, 1)), 0));
    });
    (ui, vfs, world, cat)
}

fn set(world: &mut World, f: Filter) {
    let at = world.find(1).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().filter = Some(f);
}

fn text(ui: &Interface, id: i32) -> String {
    String::from_utf8(ui.text(id).unwrap()).unwrap()
}

fn refreshed(ui: &mut Interface, vfs: &Vfs, world: &World, cat: &Catalogue, now: i64) -> Panel {
    let mut panel = Panel::default();
    panel.select(1, LAYOUT);
    panel.refresh(ui, world, cat, vfs, now).unwrap();
    panel
}

#[test]
fn the_state_text_follows_the_health_and_there_is_no_off_text() {
    let state = |health: u32, ints: &str| {
        let (mut ui, vfs, mut world, cat) = panel_in("filter-panel-state", ints);
        set(
            &mut world,
            Filter {
                health,
                ..Filter::default()
            },
        );
        refreshed(&mut ui, &vfs, &world, &cat, 0);
        text(&ui, 4913)
    };
    assert_eq!(
        [0, 1, 5, 6, 10].map(|h| state(h, "")),
        ["Oak", "Elm", "Elm", "Fir", "Fir"]
    );
    // The type's cDecayedHealth moves the threshold; 0, negative and huge values still answer.
    assert_eq!(state(1, "cDecayedHealth=0\n"), "Fir");
    assert_eq!(state(0, "cDecayedHealth=0\n"), "Oak");
    assert_eq!(state(1, "cDecayedHealth=-4\n"), "Fir");
    assert_eq!(state(u32::MAX, "cDecayedHealth=99999999999\n"), "Elm");
    assert_eq!(state(7, "cDecayedHealth=7\n"), "Elm");
}

#[test]
fn the_months_since_the_service_the_count_and_the_three_upkeeps_are_shown() {
    let (mut ui, vfs, mut world, cat) = panel_in("filter-panel-figures", "");
    let f = Filter {
        health: 10,
        upkeep: [12, 1250, 0],
        serviced: file_time(0),
        months: 3,
        ..Filter::default()
    };
    set(&mut world, f);
    let shown = |ui: &Interface| {
        [4916, 4918, 4921, 4923, 4925]
            .map(|id| text(ui, id))
            .join("|")
    };
    refreshed(&mut ui, &vfs, &world, &cat, 31 * DAY);
    assert_eq!(shown(&ui), "2 mo|3|$12|$1250|$0");
    // The day of the service is 0 months, then 1 for the days 1 to 30.
    for (now, months) in [(0, 0), (DAY - 1, 0), (DAY, 1), (30 * DAY, 1), (31 * DAY, 2)] {
        refreshed(&mut ui, &vfs, &world, &cat, now);
        assert_eq!(text(&ui, 4916), format!("{months} mo"), "{now}");
    }
    // The extremes show numbers and do not panic: a count below 0, the largest figures.
    set(
        &mut world,
        Filter {
            upkeep: [u32::MAX; 3],
            months: i32::MIN,
            ..f
        },
    );
    for now in [i64::MIN, i64::MAX] {
        refreshed(&mut ui, &vfs, &world, &cat, now);
    }
    assert_eq!(text(&ui, 4918), "-2147483648");
    assert_eq!(text(&ui, 4925), "$4294967295");
}

#[test]
fn the_picture_is_the_types_information_image_whatever_the_state() {
    let (mut ui, vfs, mut world, cat) = panel_in("filter-panel-picture", "");
    let at = ui.tree.find(4903).unwrap();
    let shown = |ui: &Interface| {
        let e = &ui.tree.elements[at];
        (
            e.state & HIDDEN == 0,
            e.look.animation.first().copied().flatten(),
        )
    };
    set(
        &mut world,
        Filter {
            health: 10,
            ..Filter::default()
        },
    );
    refreshed(&mut ui, &vfs, &world, &cat, 0);
    let healthy = shown(&ui);
    assert!(healthy.0 && healthy.1.is_some());
    set(
        &mut world,
        Filter {
            health: 0,
            ..Filter::default()
        },
    );
    refreshed(&mut ui, &vfs, &world, &cat, 0);
    assert_eq!(shown(&ui), healthy);
    // A type with no image shows none.
    let mut cat = cat;
    cat.types[0].config = Some(std::sync::Arc::new(Config::parse(b"").unwrap()));
    refreshed(&mut ui, &vfs, &world, &cat, 0);
    assert!(!shown(&ui).0);
}

#[test]
fn the_name_is_written_at_show_and_at_a_refresh_but_not_over_a_field_being_typed_in() {
    let (mut ui, vfs, mut world, cat) = panel_in("filter-panel-name", "");
    set(
        &mut world,
        Filter {
            health: 10,
            ..Filter::default()
        },
    );
    let at = world.find(1).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().name = "Tank Filter 1".into();
    let mut panel = refreshed(&mut ui, &vfs, &world, &cat, 0);
    assert_eq!(text(&ui, NAME), "Tank Filter 1");
    // Another name is shown at the next refresh...
    world.zoo.entities[at].common.as_mut().unwrap().name = "Pump".into();
    panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert_eq!(text(&ui, NAME), "Pump");
    // ...unless the field has the keyboard (open point O11).
    let field = ui.tree.find(NAME).unwrap();
    ui.tree.elements[field].state |= FOCUSED;
    ui.set_text(NAME, b"Pu");
    world.zoo.entities[at].common.as_mut().unwrap().name = "Other".into();
    panel.refresh(&mut ui, &world, &cat, &vfs, 0);
    assert_eq!(text(&ui, NAME), "Pu");
    // Shown again, it is filled whatever the focus.
    panel.reselect();
    panel.refresh(&mut ui, &world, &cat, &vfs, 0);
    assert_eq!(text(&ui, NAME), "Other");
}

#[test]
fn the_log_line_comes_when_something_changed_and_the_sell_button_is_on() {
    let (mut ui, vfs, mut world, cat) = panel_in("filter-panel-line", "");
    ui.set_enabled(SELL, false);
    set(
        &mut world,
        Filter {
            health: 10,
            months: 1,
            ..Filter::default()
        },
    );
    let mut panel = Panel::default();
    panel.select(1, LAYOUT);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.starts_with("filter 1 \"\" maximum service "), "{told}");
    assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs, 0), None);
    let state = ui.tree.elements[ui.tree.find(SELL).unwrap()].state;
    assert_eq!(state & DISABLED, 0);
    set(
        &mut world,
        Filter {
            health: 3,
            months: 1,
            ..Filter::default()
        },
    );
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.contains(" minimum "), "{told}");
    set(
        &mut world,
        Filter {
            health: 0,
            months: 1,
            ..Filter::default()
        },
    );
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.contains(" broken "), "{told}");
}

#[test]
fn nothing_is_written_for_a_selection_that_is_no_filter() {
    let (mut ui, vfs, world, cat) = panel_in("filter-panel-none", "");
    let mut panel = Panel::default();
    // Nothing selected, an entity that is gone, an entity with no filter record.
    assert_eq!(panel.filter(&mut ui, &world, &cat, &vfs, 0), None);
    panel.select(9, LAYOUT);
    assert_eq!(panel.filter(&mut ui, &world, &cat, &vfs, 0), None);
    panel.select(1, LAYOUT);
    assert_eq!(panel.filter(&mut ui, &world, &cat, &vfs, 0), None);
    assert_eq!(text(&ui, 4913), "");
}
