use super::*;

const NAMED: i32 = 9500;

fn unit(kind: Kind, task: u8, job: u8, tile: Option<usize>, job_target: u32) -> Unit {
    let mut u = Unit::new(1, kind);
    let r = &mut u.goal.record;
    (r.task, r.job, r.target.tile, r.job_target.id) = (task, job, tile, job_target);
    u
}

/// The text of a unit: tile 1 is in the exhibit "Pen", tile 2 on the grounds, any other has none;
/// entity 7 is an egg, 8 an animal called "Rex", anything else no animal; show 5 is "Tank".
fn text(u: &Unit, type_id: i32) -> Option<(i32, Option<String>)> {
    let at = |t: Option<usize>| match t {
        Some(1) => Place::Exhibit("Pen".into()),
        Some(2) => Place::Grounds,
        _ => Place::None,
    };
    let animal = |id: u32| match id {
        7 | 8 => Some(Seen {
            egg: id == 7,
            name: "Rex".into(),
        }),
        _ => None,
    };
    let show = |id: u16| (id == 5).then(|| "Tank".to_string());
    duty(u, type_id, at, animal, show)
}

fn keeper(task: u8, job: u8, tile: Option<usize>, target: u32) -> Option<(i32, Option<String>)> {
    text(&unit(Kind::Keeper, task, job, tile, target), NAMED)
}

#[test]
fn a_keepers_text_follows_the_job_first_and_then_the_task() {
    let plain = |id| Some((id, None));
    assert_eq!(keeper(0x6e, 1, None, 0), plain(10300));
    assert_eq!(keeper(0x6e, 2, None, 0), plain(10300));
    assert_eq!(keeper(0x6e, 3, None, 8), plain(10301), "an animal to heal");
    assert_eq!(keeper(0x6e, 4, None, 7), plain(10321), "an egg");
    assert_eq!(keeper(0x6e, 3, None, 9), None, "no such animal");
    assert_eq!(keeper(0x6e, 5, None, 0), plain(10302));
    assert_eq!(keeper(0x6e, 6, None, 0), plain(10302));
    assert_eq!(keeper(0x6e, 24, None, 0), plain(10327));
    assert_eq!(keeper(0x6e, 25, None, 0), plain(10327));
    // The scientist rakes "poo" under another text.
    let scientist = unit(Kind::Keeper, 0x6e, 5, None, 0);
    assert_eq!(text(&scientist, 9551), plain(10514));
    // A job wins over the task of his round.
    assert_eq!(keeper(0x65, 1, Some(1), 0), plain(10300));
}

#[test]
fn a_keepers_walk_names_the_exhibit_and_a_wander_monitors() {
    let named = |id| Some((id, Some("Pen".to_string())));
    assert_eq!(keeper(0x65, 0, Some(1), 0), named(10303));
    assert_eq!(keeper(0x7e, 0, Some(1), 0), named(10303));
    assert_eq!(
        keeper(0x65, 0, Some(2), 0),
        Some((10306, None)),
        "the grounds"
    );
    assert_eq!(keeper(0x65, 0, None, 0), None, "no exhibit there");
    assert_eq!(keeper(0x65, 0, Some(99), 0), None, "a tile that is none");
    assert_eq!(keeper(0x7f, 0, None, 0), Some((10306, None)));
    assert_eq!(keeper(1, 0, None, 0), Some((10304, None)));
    // Between two jobs, in his duties, and with no task, he shows nothing.
    assert_eq!(keeper(0x6e, 0, None, 0), None);
    assert_eq!(keeper(0, 0, None, 0), None);
}

#[test]
fn a_captures_text_names_the_animal_and_the_crate_changes_it() {
    let capture = |job, animal, kind, task| {
        let mut u = unit(kind, task, job, None, 0);
        if let Part::Staff(s) = &mut u.part {
            s.captured = animal;
        }
        text(&u, NAMED)
    };
    let rex = |id| Some((id, Some("Rex".to_string())));
    assert_eq!(capture(13, 8, Kind::Keeper, 0x6f), rex(10308));
    assert_eq!(capture(14, 8, Kind::Keeper, 0x6f), rex(10308));
    assert_eq!(capture(15, 8, Kind::Keeper, 0x6f), rex(10309));
    assert_eq!(capture(13, 9, Kind::Keeper, 0x6f), None, "no such animal");
    assert_eq!(capture(13, 0, Kind::Keeper, 0x6f), None, "none taken");
    // The job text of a dung job still wins, and another task does not show the capture.
    assert_eq!(capture(5, 8, Kind::Keeper, 0x6f), Some((10302, None)));
    assert_eq!(capture(13, 8, Kind::Keeper, 0x6e), None);
}

#[test]
fn a_conducting_keeper_names_his_show_or_the_unknown_one() {
    let conduct = |job, show| {
        let mut u = unit(Kind::Keeper, 0x6e, job, None, 0);
        u.show = show;
        text(&u, 9552)
    };
    assert_eq!(conduct(19, 5), Some((10326, Some("Tank".into()))));
    assert_eq!(conduct(19, 0), Some((10325, None)), "no show");
    assert_eq!(conduct(19, 6), Some((10325, None)), "a gone show");
    assert_eq!(conduct(19, u16::MAX), Some((10325, None)));
    assert_eq!(conduct(23, 5), None, "the way to the ladder");
    // His round before the job: the monitor task shows first.
    let mut u = unit(Kind::Keeper, 1, 19, None, 0);
    u.show = 5;
    assert_eq!(text(&u, 9552), Some((10304, None)));
}

#[test]
fn a_helicopters_text_follows_its_task() {
    let heli = |task, animal| {
        let mut u = unit(Kind::Heli, task, 0, None, 0);
        if let Part::Staff(s) = &mut u.part {
            s.captured = animal;
        }
        text(&u, 9550)
    };
    assert_eq!(heli(0x6a, 0), Some((10316, None)));
    assert_eq!(heli(0x68, 0), Some((10317, None)));
    assert_eq!(heli(0x69, 0), Some((10318, None)));
    assert_eq!(heli(0x67, 0), Some((10319, None)));
    assert_eq!(heli(0x6f, 8), Some((10308, Some("Rex".into()))));
    assert_eq!(heli(0x6f, 9), None, "no such animal");
    assert_eq!(heli(0x6f, 0), None);
    assert_eq!(heli(1, 0), None);
    assert_eq!(heli(0, 0), None);
    assert_eq!(heli(u8::MAX, 0), None);
}

#[test]
fn a_workers_text_is_the_job_or_monitoring_with_none() {
    let worker = |task, job| text(&unit(Kind::Worker, task, job, None, 0), 9501);
    for (job, id) in [
        (7, 10500),
        (8, 10501),
        (9, 10502),
        (10, 10503),
        (11, 10504),
        (12, 10505),
    ] {
        assert_eq!(worker(0x78, job), Some((id, None)), "job {job}");
    }
    for (job, id) in [(16, 10509), (17, 10510), (26, 10515), (27, 10516)] {
        assert_eq!(worker(0x78, job), Some((id, None)), "job {job}");
    }
    assert_eq!(worker(1, 0), Some((10506, None)));
    assert_eq!(worker(0x78, 0), None);
    assert_eq!(worker(0, 0), None);
    assert_eq!(worker(1, 3), None, "a job of a keeper");
    assert_eq!(worker(1, u8::MAX), None);
}

#[test]
fn a_guides_text_is_going_speaking_or_monitoring() {
    let guide = |task, tile| text(&unit(Kind::Guide, task, 0, tile, 0), 9502);
    assert_eq!(guide(0x76, Some(1)), Some((10400, Some("Pen".into()))));
    assert_eq!(guide(0x76, Some(2)), None, "the grounds");
    assert_eq!(guide(0x76, None), None);
    assert_eq!(guide(0x77, None), Some((10401, None)));
    assert_eq!(guide(1, None), Some((10402, None)));
    assert_eq!(guide(0, None), Some((10402, None)));
}

#[test]
fn units_that_are_not_staff_have_no_text() {
    assert_eq!(text(&unit(Kind::Animal, 1, 0, None, 0), 0), None);
    assert_eq!(text(&unit(Kind::Guest, 1, 0, None, 0), 0), None);
}

#[test]
fn the_follow_button_shows_the_types_picture_when_shown_and_when_forced_only() {
    use crate::panel::follow::tests as follow;
    use crate::unit::testing::{centre, entity, world};
    let layout = "[LayoutInfo]\nid=17\nx=0\ny=0\ndx=40\ndy=40\n\
                  [u]\ntype=UIButton\nid=3515\nx=0\ny=0\ndx=4\ndy=4\n";
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
                [b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.as_bytes().to_vec()),
    ];
    let members = follow::members();
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = crate::fixtures::Install::new("staff-panel-follow", &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = crate::ui::layout::Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(follow::Texts));
    ui.show(17);
    let mut world = world((4, 4), |_, e| {
        e.push(entity("keeper", "keeper", 1, centre((1, 1)), 0));
    });
    let cat = follow::catalogue("keeper", "keeper");
    let mut panel = Panel::default();
    panel.select(1);
    let refresh = |ui: &mut Interface, world: &World, panel: &mut Panel| {
        panel.refresh(ui, world, &cat, &vfs, false);
    };
    let shown = |ui: &Interface| follow::shown(ui, FOLLOW);
    assert_eq!(shown(&ui), (false, None), "blank until shown");
    panel.clock.due(0, true, PERIOD);
    refresh(&mut ui, &world, &mut panel);
    let first = shown(&ui);
    assert!(first.0, "set when the panel is shown");
    // The picture is the type's: what the unit plays does not change it.
    let unit = world.units.get_mut(1).unwrap();
    (unit.animation.name, unit.animation.frame) = ("walk".into(), 1);
    ui.set_icon(FOLLOW, None);
    assert!(!panel.clock.due(1, false, PERIOD));
    refresh(&mut ui, &world, &mut panel);
    assert_eq!(shown(&ui), (false, None), "an ordinary refresh leaves it");
    panel.clock.force = true;
    assert!(panel.clock.due(1, false, PERIOD));
    refresh(&mut ui, &world, &mut panel);
    assert_eq!(shown(&ui), first, "a forced one sets it again");
    ui.set_icon(FOLLOW, None);
    assert!(panel.clock.due(PERIOD, true, PERIOD));
    refresh(&mut ui, &world, &mut panel);
    assert_eq!(shown(&ui), (false, None), "a periodic one does not");
}

#[test]
fn the_training_icons_show_for_the_bits_of_the_mask_with_the_tooltips_of_the_type() {
    use crate::panel::follow::tests as follow;
    use crate::unit::testing::{centre, entity, world};
    let image = |id: i32| format!("[i{id}]\ntype=UIImage\nid={id}\nx=0\ny=0\n");
    let layout = format!(
        "[LayoutInfo]\nid=17\nx=0\ny=0\ndx=40\ndy=40\n{}",
        ICONS.map(image).concat()
    );
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
                [b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.into_bytes()),
    ];
    let members = follow::members();
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = crate::fixtures::Install::new("staff-panel-training", &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = crate::ui::layout::Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(follow::Texts));
    ui.show(17);
    let mut world = world((4, 4), |_, e| {
        let mut keeper = entity("keeper", "keeper", 1, centre((1, 1)), 0);
        keeper.subtype = "m".into();
        e.push(keeper);
    });
    let mut cat = crate::entity::Catalogue::default();
    cat.push(EntityType {
        class: "keeper",
        type_name: b"keeper".to_vec(),
        subtype: b"m".to_vec(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(
                b"[m/Characteristics/Strings]\ncTrainingIconName = ui/z/z\ncTrainingIconName = ui/z/z\n\
                  [m/Characteristics/Integers]\ncTrainingTooltip = 24241\ncTrainingTooltip = 24243\n",
            )
            .unwrap(),
        )),
        name_id: 9552,
        footprint: [1, 1, 1],
        height: 0,
    });
    let mut panel = Panel::default();
    panel.select(1);
    let state = |ui: &Interface, id: i32| {
        let e = &ui.tree.elements[ui.tree.find(id).unwrap()];
        (
            e.look.normal.is_some() || !e.look.animation.is_empty(),
            e.help_id.clone(),
        )
    };
    let mut icons = |world: &World, ui: &mut Interface| {
        panel.clock.force = true;
        panel.refresh(ui, world, &cat, &vfs, false);
        ICONS.map(|id| state(ui, id))
    };
    let none: [(bool, Vec<i32>); 5] = Default::default();
    assert_eq!(icons(&world, &mut ui), none, "no mask, no icon");
    // Mark 2 of the program sets bit 1: the second icon and its tooltip.
    world.research.train(&cat, 9552, 2);
    let shown = icons(&world, &mut ui);
    assert_eq!(shown[0], (false, vec![]));
    assert_eq!(shown[1], (true, vec![24243]));
    // A bit with no entry in the list (the third) shows nothing.
    world.research.train(&cat, 9552, 3);
    let shown = icons(&world, &mut ui);
    assert_eq!(shown[2], (false, vec![]));
    assert!(shown[1].0);
}
