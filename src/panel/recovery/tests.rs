use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::goal;
use crate::panel::follow::tests as follow;
use crate::ui::draw::Strings;
use crate::ui::layout::{SELECTED, Tree};
use crate::unit::testing::{centre, entity, world};
use crate::zoo::BuildingRecord;

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            SYMBOL => "$".into(),
            10316 => "Waiting".into(),
            10308 => "Darting %s".into(),
            _ => String::new(),
        }
    }

    fn text_entry(&self, _: &[u8]) -> Vec<u8> {
        vec![]
    }
}

/// The panel with the elements the refresh fills, a helicopter (entity 1) of a type with two training
/// icons and a base (entity 2) that fired `darts` darts.
fn panel_in(name: &str, darts: u32) -> (Interface, Vfs, World, Catalogue) {
    let one =
        |kind: &str, id: i32| format!("[e{id}]\ntype={kind}\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n");
    let elements: String = [
        ("UIEditableText", vec![NAME]),
        ("UIButton", vec![FOLLOW, FIRE]),
    ]
    .into_iter()
    .chain([
        ("UIText", vec![COST, DUTY, RECOVERIES]),
        ("UIImage", ICONS.to_vec()),
    ])
    .flat_map(|(kind, ids)| ids.into_iter().map(move |id| (kind, id)))
    .map(|(kind, id)| one(kind, id))
    .collect();
    let layout = format!("[LayoutInfo]\nid={LAYOUT}\nx=0\ny=0\ndx=90\ndy=90\n{elements}");
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.into_bytes()),
    ];
    let members = follow::members();
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = crate::fixtures::Install::new(name, &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(200, 120, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    ui.show(LAYOUT);
    let text = "[Characteristics/Strings]\ncTrainingIconName = ui/z/z\ncTrainingIconName = ui/z/z\n\
        [Characteristics/Integers]\ncMonthlyCost = 2000.4\ncTrainingTooltip = 11\ncTrainingTooltip = 12\n\
        [Animations]\nidle = ui/z/z.ani\n";
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "helicopter",
        type_name: b"helicopter".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 9550,
        footprint: [1, 1, 1],
        height: 0,
    });
    let mut world = world((4, 4), |_, e| {
        e.push(entity("helicopter", "helicopter", 1, centre((1, 1)), 0));
        e.push(entity("building", "base", 2, centre((2, 2)), 0));
    });
    let at = world.find(2).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().building = Some(BuildingRecord {
        guests: [0; 3],
        income: [0; 3],
        upkeep: None,
        upgrade: 0,
        price: 0,
        users: Vec::new(),
        users_now: 0,
        members: Vec::new(),
        fill: darts,
        built: 0,
    });
    if let Some(Part::Staff(s)) = world.units.get_mut(1).map(|u| &mut u.part) {
        s.heli.base = 2;
    }
    (ui, vfs, world, cat)
}

fn text(ui: &Interface, id: i32) -> String {
    String::from_utf8(ui.text(id).unwrap()).unwrap()
}

#[test]
fn the_panel_shows_the_cost_the_job_and_the_recoveries_of_the_team() {
    let (mut ui, vfs, mut world, cat) = panel_in("recovery-panel-text", 7);
    let mut panel = Panel::default();
    panel.select(1);
    world.units.get_mut(1).unwrap().goal.record.task = goal::USE;
    let told = panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
    // The monthly cost is rounded; the recoveries are the darts of the base.
    assert_eq!(text(&ui, COST), "$2,000");
    assert_eq!(text(&ui, DUTY), "Waiting");
    assert_eq!(text(&ui, RECOVERIES), "7");
    assert_eq!(
        told,
        "team 1 \"\" cost $2,000 duty \"Waiting\" recoveries 7"
    );
    // Nothing changed: nothing told.
    panel.clock.force = true;
    assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs), None);
    // Another dart and another task show at the next refresh.
    let at = world.find(2).unwrap();
    world.zoo.entities[at]
        .common
        .as_mut()
        .unwrap()
        .building
        .as_mut()
        .unwrap()
        .fill = 8;
    world.units.get_mut(1).unwrap().goal.record.task = 0;
    panel.clock.force = true;
    panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
    assert_eq!(
        (text(&ui, RECOVERIES).as_str(), text(&ui, DUTY).as_str()),
        ("8", "")
    );
}

#[test]
fn the_recoveries_are_zero_without_a_base_and_whole_for_a_huge_count() {
    for (darts, base, want) in [
        (0, 2, "0"),
        (u32::MAX, 2, "4294967295"),
        (5, 99, "0"),
        (5, 0, "0"),
    ] {
        let (mut ui, vfs, mut world, cat) = panel_in("recovery-panel-count", darts);
        if let Some(Part::Staff(s)) = world.units.get_mut(1).map(|u| &mut u.part) {
            s.heli.base = base;
        }
        let mut panel = Panel::default();
        panel.select(1);
        panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
        assert_eq!(text(&ui, RECOVERIES), want, "{darts} {base}");
    }
}

#[test]
fn the_name_and_the_follow_button_are_set_when_shown_and_the_button_is_enabled() {
    let (mut ui, vfs, mut world, cat) = panel_in("recovery-panel-name", 0);
    let at = world.find(1).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().name = "Heli".into();
    let mut panel = Panel::default();
    panel.select(1);
    panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
    let state = |ui: &Interface| ui.tree.elements[ui.tree.find(FOLLOW).unwrap()].state;
    assert_eq!(text(&ui, NAME), "Heli");
    assert_eq!(state(&ui) & (SELECTED | crate::ui::layout::DISABLED), 0);
    assert!(follow::shown(&ui, FOLLOW).0, "the team's icon");
    // The player follows and types: a refresh of the clock leaves both; a reselect starts over.
    ui.set_down(FOLLOW, true);
    ui.set_text(NAME, b"typed");
    panel.clock.force = true;
    panel.refresh(&mut ui, &world, &cat, &vfs);
    assert_eq!(text(&ui, NAME), "typed");
    assert_ne!(state(&ui) & SELECTED, 0);
    panel.reselect();
    panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
    assert_eq!(text(&ui, NAME), "Heli");
    assert_eq!(state(&ui) & SELECTED, 0);
    // Nothing selected, a missing entity or a unit that is no staff member: nothing to show.
    panel.deselect();
    assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs), None);
    panel.select(99);
    assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs), None);
    panel.select(2);
    assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs), None);
}

#[test]
fn the_duty_icons_show_for_the_bits_of_the_training_mask() {
    let (mut ui, vfs, mut world, cat) = panel_in("recovery-panel-icons", 0);
    let mut panel = Panel::default();
    panel.select(1);
    let shown = |ui: &Interface| {
        ICONS.map(|id| {
            let e = &ui.tree.elements[ui.tree.find(id).unwrap()];
            (
                e.look.normal.is_some() || !e.look.animation.is_empty(),
                e.help_id.clone(),
            )
        })
    };
    panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
    assert_eq!(
        shown(&ui),
        <[(bool, Vec<i32>); 5]>::default(),
        "no bit, no icon"
    );
    // Mark 2 sets bit 1: the second icon with its tooltip; mark 3 has no icon in the list.
    world.research.train(&cat, 9550, 2);
    world.research.train(&cat, 9550, 3);
    panel.clock.force = true;
    panel.refresh(&mut ui, &world, &cat, &vfs).unwrap();
    let icons = shown(&ui);
    assert_eq!(icons[1], (true, vec![12]));
    assert!(icons.iter().enumerate().all(|(i, s)| i == 1 || !s.0));
}
