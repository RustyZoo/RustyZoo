use super::*;

#[test]
fn months_open_are_the_whole_days_plus_29_over_30() {
    let day = DAY_MS;
    let months = |built: u64, now| months(built as i64, now);
    // Built today: one month, and until day 30; day 31 is the second month (guest buildings 10.3).
    assert_eq!(months(0, 0), 1);
    assert_eq!(months(5 * day, 5 * day + day / 10), 1);
    assert_eq!(months(0, 30 * day), 1);
    assert_eq!(months(0, 31 * day - 1), 1);
    assert_eq!(months(0, 31 * day), 2);
    assert_eq!(months(0, 60 * day), 2);
    assert_eq!(months(0, 61 * day), 3);
    // A building "built" after now, and a huge time, never panic or give less than 1.
    assert_eq!(months(10, 0), 1);
    assert!(months(0, u64::MAX) > 1);
    // A construction date before the calendar's start counts the days before it too, and the
    // extremes do not overflow.
    let before = |days: i64, now| super::months(-days * DAY_MS as i64, now);
    assert_eq!(before(30, 0), 1);
    assert_eq!(before(31, 0), 2);
    assert_eq!(
        super::months(i64::MIN, u64::MAX),
        super::months(i64::MIN, 0)
    );
    assert_eq!(super::months(i64::MAX, 0), 1);
}

#[test]
fn the_figures_are_cut_toward_zero_and_divided_whole() {
    let live = Live {
        income: [0.0, 0.0, 10.9],
        upkeep: 50.6,
        ..Live::default()
    };
    // 10 - 50 = -40; per month of 3: truncated towards zero.
    assert_eq!(figures(&live, 1), ([10, 50, -40], [10, 50, -40]));
    assert_eq!(figures(&live, 3), ([10, 50, -40], [3, 16, -13]));
    // Halves are cut too: 0.5 is 0, 1.5 is 1, 3.5 is 3.
    let half = |v: f32| {
        figures(
            &Live {
                income: [0.0, 0.0, v],
                ..Live::default()
            },
            1,
        )
        .0[0]
    };
    assert_eq!([half(0.5), half(1.5), half(2.5), half(3.5)], [0, 1, 2, 3]);
    // Not numbers and huge values give no panic.
    for v in [f32::NAN, f32::INFINITY, f32::MIN, f32::MAX] {
        figures(
            &Live {
                income: [0.0, 0.0, v],
                upkeep: v,
                ..Live::default()
            },
            1,
        );
    }
}

#[test]
fn the_typed_text_loses_the_first_symbol_and_separator_and_reads_as_atof() {
    let t = |text: &str| typed(text.as_bytes(), b"$");
    assert_eq!([t("$4.25"), t("  4.25"), t("4.25$")], [4.25; 3]);
    // Only the first symbol and the first separator are cut: the second ones end the number.
    assert_eq!([t("$1,234.5"), t("1,2,3"), t("$$5")], [1234.5, 12.0, 0.0]);
    // No number, nothing, a symbol alone and a huge number.
    assert_eq!([t("abc"), t(""), t("$")], [0.0; 3]);
    assert!(t("1e999").is_infinite());
    // An empty symbol cuts nothing.
    assert_eq!(typed(b"$5", b""), 0.0);
}

use crate::config::Config;
use crate::entity::EntityType;
use crate::panel::follow::tests as follow;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{DISABLED, HIDDEN, Tree};
use crate::unit::testing::{centre, entity, world};

struct Texts;

impl Strings for Texts {
    fn string(&self, _: i32) -> String {
        String::new()
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The panel `layout` with the elements the refresh fills, a map with one building (entity 1) of
/// a type with the idle animation of [`follow::members`] and the integer settings `ints`.
fn panel_in(name: &str, layout: i32, ints: &str) -> (Interface, Vfs, World, Catalogue) {
    let ids = [
        ("UIImage", [4550, 4650, 4567, 4568].as_slice()),
        ("UIEditableText", &[4502, 4602, 4555]),
        (
            "UIButton",
            &[4557, 4556, 4516, 4580, 4616, 4575, 4576, 4577],
        ),
        (
            "UIText",
            &[
                4531, 4533, 4535, 4559, 4561, 4591, 4573, 4563, 4565, 4592, 4664, 4666, 4662, 4668,
                4696,
            ],
        ),
    ];
    let elements: String = ids
        .iter()
        .flat_map(|(kind, ids)| ids.iter().map(move |id| (kind, id)))
        .map(|(kind, id)| format!("[e{id}]\ntype={kind}\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n"))
        .collect();
    let text = format!("[LayoutInfo]\nid={layout}\nx=0\ny=0\ndx=90\ndy=90\n{elements}");
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
    ui.show(layout);
    let text = format!(
        "[Characteristics/Integers]\n{ints}[Characteristics/Floats]\ncDefaultCost=2\ncHighCost=150\n\
         [Characteristics/Strings]\ncInfoImageName = ui/z/z\n[Animations]\nidle = ui/z/z.ani\n"
    );
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "building",
        type_name: b"stand".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let world = world((4, 4), |_, e| {
        e.push(entity("building", "stand", 1, centre((1, 1)), 0));
    });
    (ui, vfs, world, cat)
}

fn state(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

#[test]
fn both_panels_show_the_types_information_image_when_shown_only() {
    for (layout, picture) in [(CHARGING, PICTURE), (PLAIN, PLAIN_PICTURE)] {
        let (mut ui, vfs, world, cat) = panel_in("building-panel-picture", layout, "");
        let mut panel = Panel::default();
        panel.select(1, layout);
        let at = ui.tree.find(picture).unwrap();
        let animated = |ui: &Interface| {
            let e = &ui.tree.elements[at];
            (
                e.state & HIDDEN == 0,
                e.look.animation.first().copied().flatten().is_some(),
            )
        };
        panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
        assert_eq!(
            animated(&ui),
            (true, true),
            "the animation named by the type"
        );
        // The element keeps its place and takes the size of the animation.
        let placed = ui.tree.elements[at].placed.unwrap();
        assert_eq!((placed.x, placed.y), (0, 0));
        assert!(placed.dx > 0 && placed.dy > 0);
        // A refresh of the clock leaves it; showing the panel again sets it again.
        ui.set_picture(picture, None);
        panel.clock.force = true;
        panel.refresh(&mut ui, &world, &cat, &vfs, 0);
        assert!(!animated(&ui).0, "a refresh leaves it hidden");
        panel.reselect();
        panel.refresh(&mut ui, &world, &cat, &vfs, 0);
        assert_eq!(animated(&ui), (true, true));
    }
}

#[test]
fn a_type_with_no_information_image_or_a_missing_one_shows_no_picture() {
    let (mut ui, vfs, world, mut cat) = panel_in("building-panel-no-image", CHARGING, "");
    let at = ui.tree.find(PICTURE).unwrap();
    for text in [
        "",
        "[Characteristics/Strings]\ncInfoImageName =\n",
        "[Characteristics/Strings]\ncInfoImageName = ui/nothing/here\n",
    ] {
        cat.types[0].config = Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap()));
        let mut panel = Panel::default();
        panel.select(1, CHARGING);
        let shown = ui.tree.picture(&vfs, "ui/z/z");
        ui.set_picture(PICTURE, shown);
        panel.refresh(&mut ui, &world, &cat, &vfs, 0);
        assert_ne!(ui.tree.elements[at].state & HIDDEN, 0, "{text:?}");
    }
}

#[test]
fn the_name_field_shows_the_stored_name_again_after_a_reselect() {
    let (mut ui, vfs, mut world, cat) = panel_in("building-panel-name", CHARGING, "");
    let mut panel = Panel::default();
    panel.select(1, CHARGING);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.starts_with("building 1 \"\" "), "{told}");
    let at = world.find(1).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().name = "Snacks".into();
    ui.set_text(NAME, b"typed");
    panel.reselect();
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.starts_with("building 1 \"Snacks\" "), "{told}");
    assert_eq!(ui.text(NAME).unwrap(), b"Snacks");
    // Nothing selected, nothing to show again.
    panel.deselect();
    panel.reselect();
    assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs, 0), None);
}

#[test]
fn hiding_the_cost_change_hides_the_buttons_and_disables_the_field() {
    let hidden = |ints: &str, name: &str| {
        let (mut ui, vfs, world, cat) = panel_in(name, CHARGING, ints);
        let mut panel = Panel::default();
        panel.select(1, CHARGING);
        panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
        [UP, DOWN, PRICE].map(|id| (state(&ui, id) & HIDDEN != 0, state(&ui, id) & DISABLED != 0))
    };
    let normal = hidden("cCommerce = 1\n", "building-panel-cost");
    assert_eq!(normal, [(false, false), (false, false), (false, false)]);
    let fixed = hidden(
        "cCommerce = 1\ncHideCostChange = 1\n",
        "building-panel-cost-hidden",
    );
    assert_eq!(fixed, [(true, false), (true, false), (false, true)]);
    let (mut ui, vfs, world, cat) = panel_in(
        "building-panel-fixed-line",
        CHARGING,
        "cCommerce = 1\ncHideCostChange = 1\n",
    );
    let mut panel = Panel::default();
    panel.select(1, CHARGING);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.contains(" price $2.00 fixed income "), "{told}");
}

#[test]
fn sell_is_always_on_and_pick_up_shows_for_a_moveable_type_only() {
    let buttons = |ints: &str, name: &str, layout| {
        let (mut ui, vfs, world, cat) = panel_in(name, layout, ints);
        let mut panel = Panel::default();
        panel.select(1, layout);
        panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
        [SELL, PLAIN_SELL, PICK_UP].map(|id| {
            let s = state(&ui, id);
            (s & HIDDEN != 0, s & DISABLED != 0)
        })
    };
    let (on, off) = ((false, false), (true, true));
    for (ints, pick) in [
        ("cCommerce = 1\ncMoveable = 1\n", on),
        ("cCommerce = 1\n", off),
        ("cCommerce = 1\ncMoveable = 0\n", off),
        ("cCommerce = 1\ncMoveable = -7\n", off),
        ("cCommerce = 1\ncMoveable = 99999999999\n", off),
    ] {
        let name = format!("building-panel-move-{}", ints.len());
        let got = buttons(ints, &name, CHARGING);
        assert_eq!(got, [on, on, pick], "{ints:?}");
    }
    // The panel of a building that charges nothing has the same rule (its layout has only the
    // sell button, the other ids are not there).
    assert_eq!(
        buttons("cMoveable = 1\n", "building-panel-move-plain", PLAIN),
        [on, on, on]
    );
}

#[test]
fn the_up_button_is_enabled_at_the_economy_maximum_and_not_above() {
    let (mut ui, vfs, mut world, cat) = panel_in("building-panel-max", CHARGING, "cCommerce = 1\n");
    let mut panel = Panel::default();
    panel.select(1, CHARGING);
    for (price, off) in [(100.0, false), (100.25, true)] {
        let live = Live {
            price,
            ..Live::default()
        };
        world.units.buildings.insert(1, live);
        panel.clock.force = true;
        panel.refresh(&mut ui, &world, &cat, &vfs, 0);
        assert_eq!(state(&ui, UP) & DISABLED != 0, off, "{price}");
    }
}

#[test]
fn hiding_the_commerce_info_leaves_everything_below_the_price_alone() {
    let run = |ints: &str, name: &str| {
        let (mut ui, vfs, mut world, mut cat) = panel_in(name, CHARGING, ints);
        // The building sells an item with an icon.
        cat.types[0].config = {
            let text = format!(
                "[Characteristics/Integers]\n{ints}[Sells]\nhotdog\n[Characteristics/Floats]\n\
                 cDefaultCost=2\ncHighCost=150\n[Characteristics/Strings]\ncInfoImageName = ui/z/z\n"
            );
            Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap()))
        };
        cat.items.insert(
            "hotdog".into(),
            crate::building::items::Item {
                id: 6303,
                icon: "ui/z/z".into(),
                ..Default::default()
            },
        );
        world.units.buildings.clear();
        let ids = [TOTALS, MONTHLY].concat();
        for id in ids.iter().chain(&GUESTS).chain([&MONTHS]) {
            ui.set_text(*id, b"layout");
        }
        ui.set_hidden(ITEMS[0], true);
        let mut panel = Panel::default();
        panel.select(1, CHARGING);
        let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
        let texts = ids.iter().chain(&GUESTS).chain([&MONTHS]);
        let kept = texts
            .map(|&id| ui.text(id).unwrap())
            .all(|t| t == b"layout");
        let items = state(&ui, ITEMS[0]) & HIDDEN != 0;
        let tabs = [4575, 4576, 4577]
            .map(|id| (state(&ui, id) & HIDDEN != 0, state(&ui, id) & DISABLED != 0));
        (told, kept, items, tabs)
    };
    let (told, kept, items, tabs) = run("cCommerce = 1\n", "building-panel-info");
    assert!(!kept && told.contains(" income $0 "), "{told}");
    assert!(!items, "the item icon is shown");
    assert_eq!(tabs, [(false, false); 3]);
    let (told, kept, items, tabs) = run(
        "cCommerce = 1\ncHideCommerceInfo = 1\n",
        "building-panel-info-hidden",
    );
    assert!(kept, "the layout's text stays");
    assert!(items, "the item row is not touched");
    assert_eq!(told, "building 1 \"\" price $2.00");
    // The status and finance tabs are hidden and disabled; the colour tab stays.
    assert_eq!(tabs, [(true, true), (true, true), (false, false)]);
}

#[test]
fn the_panel_of_a_building_that_charges_nothing_does_not_test_the_commerce_flag() {
    let (mut ui, vfs, world, cat) = panel_in(
        "building-panel-plain-info",
        PLAIN,
        "cHideCommerceInfo = 1\ncHideCostChange = 1\n",
    );
    let mut panel = Panel::default();
    panel.select(1, PLAIN);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.contains(" months 1"), "{told}");
    assert_eq!(ui.text(PLAIN_MONTHS).unwrap(), b"1");
    assert_eq!(ui.text(PLAIN_GUESTS[0]).unwrap(), b"0");
}
