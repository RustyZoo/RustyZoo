use super::*;

const NAMED: i32 = 9500;

fn unit(kind: Kind, task: u8, job: u8, tile: Option<usize>, job_target: u32) -> Unit {
    let mut u = Unit::new(1, kind);
    let r = &mut u.goal.record;
    (r.task, r.job, r.target.tile, r.job_target.id) = (task, job, tile, job_target);
    u
}

/// The text of a unit: tile 1 is in the exhibit "Pen", tile 2 on the grounds, any other has none;
/// entity 7 is an egg, 8 an animal, anything else no animal.
fn text(u: &Unit, type_id: i32) -> Option<(i32, Option<String>)> {
    let at = |t: Option<usize>| match t {
        Some(1) => Place::Exhibit("Pen".into()),
        Some(2) => Place::Grounds,
        _ => Place::None,
    };
    let animal = |id: u32| match id {
        7 => Some(true),
        8 => Some(false),
        _ => None,
    };
    duty(u, type_id, at, animal)
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
fn the_follow_button_shows_the_units_frame_when_shown_and_when_forced_only() {
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
    let install = crate::sprite::tests::Install::new("staff-panel-follow", &files);
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
    let frame = |ui: &Interface| follow::shown(ui, FOLLOW);
    assert_eq!(frame(&ui), (false, (0, 0, false)), "blank until shown");
    let refresh = |ui: &mut Interface, world: &World, panel: &mut Panel| {
        panel.refresh(ui, world, &cat, &vfs, 0, false);
    };
    panel.clock.due(0, true, PERIOD);
    refresh(&mut ui, &world, &mut panel);
    assert!(frame(&ui).0, "set when the panel is shown");
    assert_eq!(frame(&ui).1.1, 0);
    world.units.get_mut(1).unwrap().animation.frame = 1;
    assert!(!panel.clock.due(1, false, PERIOD));
    refresh(&mut ui, &world, &mut panel);
    assert_eq!(frame(&ui).1.1, 0, "an ordinary refresh leaves it");
    panel.clock.force = true;
    assert!(panel.clock.due(1, false, PERIOD));
    refresh(&mut ui, &world, &mut panel);
    assert_eq!(
        frame(&ui).1.1,
        1,
        "a forced one takes the frame of the moment"
    );
}
