use super::*;
use crate::timing::Calendar;

fn guest(values: [i32; 5]) -> Guest {
    Guest {
        values,
        ..Guest::default()
    }
}

#[test]
fn the_bars_are_percentages_and_the_needs_are_turned_around() {
    // Happiness 50, hunger 60, thirst 30, energy 80, bathroom 10 (guests 5.5, information panels
    // 12.2): 75, 100 - 30, 100 - 60, 100 - 10, 100 - 80.
    let g = guest([50, 60, 30, 80, 10]);
    assert_eq!(bars(&g, 100), [75, 70, 40, 90, 20]);
    // A guest with nothing wrong and a worn out one.
    assert_eq!(bars(&guest([100, 0, 0, 0, 0]), 100), [100; 5]);
    assert_eq!(bars(&guest([-100, 100, 100, 100, 100]), 100), [0; 5]);
    // The energy's limit is the type's.
    assert_eq!(bars(&guest([0, 0, 0, 50, 0]), 200)[4], 75);
    // A limit of zero or below, or a huge value, never panics.
    for max in [0, -1, i32::MIN] {
        bars(&guest([0, 0, 0, 50, 0]), max);
    }
    bars(&guest([i32::MIN, i32::MAX, i32::MAX, i32::MIN, 0]), 100);
}

#[test]
fn the_time_in_the_park_is_banded_by_hours_of_calendar_time() {
    let start = Calendar::new().date();
    // A step of 1 ms is 7.2 s of game time, so 500 steps are an hour.
    let after = |hours: u32| {
        let mut c = Calendar::new();
        c.advance(hours * 500);
        stay(Some(start), c.date())
    };
    let shown: Vec<usize> = [0, 1439, 1440, 5759, 5760, 8639, 8640, 20_000]
        .map(after)
        .into();
    assert_eq!(shown, [0, 0, 1, 1, 2, 2, 3, 3]);
    assert_eq!(stay(None, start), 0, "no arrival date: just arrived");
    assert_eq!(
        stay(Some(start), Date::from_file_time(0)),
        0,
        "never negative"
    );
}

use crate::building::items::Item;
use crate::entity::Catalogue;
use crate::panel::follow::tests as follow;
use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{DISABLED, HIDDEN, Tree};
use crate::unit::testing::{centre, entity, world};

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            3404 => "H %d".into(),
            3420 => "T0".into(),
            3421 => "T1".into(),
            5000 => "Lion".into(),
            10074 => "Th1".into(),
            10102 => "Th2 %s".into(),
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

/// A panel with the elements the refresh fills, and a map with a zebra (1) and a guest (2).
fn panel_in_game(name: &str) -> (Interface, Vfs, World) {
    let bar = |id: i32| {
        format!("[b{id}]\ntype=UIStatusImage\nid={id}\nx=0\ny=0\ndx=10\ndy=2\nimage=ui/g.tga\n")
    };
    let text = |id: i32| format!("[t{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n");
    let button = |id: i32| format!("[u{id}]\ntype=UIButton\nid={id}\nx=0\ny=0\ndx=4\ndy=4\n");
    let picture = |id: i32| format!("[p{id}]\ntype=UIImage\nid={id}\nx=0\ny=0\nhelpid=3481\n");
    let layout = [3404, 3406, 3408, 3410, 3412].map(bar).concat()
        + &[3475, 3476, 3402].map(text).concat()
        + &[3415, 3416].map(button).concat()
        + &[3481, 3482, 3483].map(picture).concat()
        + "[l]\ntype=UIListBox\nid=3469\nx=0\ny=0\ndx=30\ndy=30\n";
    let layout = format!("[LayoutInfo]\nid=9\nx=0\ny=0\ndx=40\ndy=40\n{layout}");
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let mut files = vec![
        ("ui/root.lyt", root.as_bytes().to_vec()),
        ("ui/p.lyt", layout.into_bytes()),
        ("ui/g.tga", tga()),
    ];
    let members = follow::members();
    files.extend(members.iter().map(|(n, b)| (n.as_str(), b.clone())));
    let install = Install::new(name, &files);
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    ui.show(9);
    let world = world((4, 4), |_, e| {
        e.push(entity("animals", "zebra", 1, centre((1, 1)), 0));
        e.push(entity("guests", "guest", 2, centre((2, 1)), 0));
    });
    (ui, vfs, world)
}

fn state(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

#[test]
fn a_refresh_writes_the_bars_the_time_the_items_and_the_thoughts() {
    let (mut ui, vfs, mut world) = panel_in_game("guest-panel-refresh");
    let mut cat = Catalogue::default();
    let item = Item {
        id: 6303,
        icon: "ui/g.tga".into(),
        ..Item::default()
    };
    cat.items.insert("hotdog".into(), item);
    let now = Calendar::new().date();
    let Part::Guest(g) = &mut world.units.get_mut(2).unwrap().part else {
        panic!("not a guest");
    };
    (g.values, g.favourite, g.arrived) = ([50, 60, 30, 80, 10], Some(5000), Some(now));
    (g.items[0], g.items[3]) = (Some(6303), Some(6303));
    let at = world.find(1).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().name = "Zed".into();
    world.units.think(2, 10074, None);
    world.units.think(2, 10102, Some(1));
    world.units.think(1, 10074, None);
    let mut panel = Panel::default();
    // No selection, no such entity, and an entity that is not a guest: nothing.
    for id in [None, Some(99), Some(1)] {
        panel.selected = id;
        assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs, now), None);
    }
    panel.select(2);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, now).unwrap();
    assert_eq!(
        told,
        "guest 2 \"\" happiness 75 drink 70 food 40 restroom 90 energy 20 time \"T0\" \
         favourite \"Lion\" thoughts [\"Th2 Zed\", \"Th1\"]"
    );
    assert_eq!(ui.string(3404), "H 75", "the tooltip holds the value");
    assert_eq!(ui.text(3476).unwrap(), b"Lion");
    // The two items fill the first two pictures and the third is hidden.
    assert_eq!(state(&ui, 3481) & HIDDEN, 0);
    assert_eq!(state(&ui, 3482) & HIDDEN, 0);
    assert_ne!(state(&ui, 3483) & HIDDEN, 0);
    let help = |id: i32| ui.tree.elements[ui.tree.find(id).unwrap()].help_id.clone();
    assert_eq!((help(3481), help(3483)), (vec![6303], vec![]));
    assert_eq!(state(&ui, 3416) & DISABLED, 0);
    assert_eq!(
        panel.refresh(&mut ui, &world, &cat, &vfs, now),
        None,
        "unchanged"
    );
    // A carried guest cannot be picked up again; 1500 hours later it has stayed a short time.
    world.units.get_mut(2).unwrap().carried = true;
    let later = {
        let mut c = Calendar::new();
        c.advance(1500 * 500);
        c.date()
    };
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, later).unwrap();
    assert!(told.contains("time \"T1\""), "{told}");
    assert_ne!(state(&ui, 3416) & DISABLED, 0);
}

#[test]
fn the_follow_button_shows_the_types_picture_when_shown_and_when_forced_only() {
    let (mut ui, vfs, mut world) = panel_in_game("guest-panel-follow");
    let cat = follow::catalogue("guests", "guest");
    let mut panel = Panel::default();
    panel.select(2);
    let now = Calendar::new().date();
    let shown = |ui: &Interface| follow::shown(ui, FOLLOW);
    assert_eq!(shown(&ui), (false, None), "blank until shown");
    panel.clock.due(0, true, PERIOD);
    panel.refresh(&mut ui, &world, &cat, &vfs, now);
    let first = shown(&ui);
    assert!(first.0, "set when the panel is shown");
    // The picture is the type's: what the unit plays does not change it.
    let unit = world.units.get_mut(2).unwrap();
    (unit.animation.name, unit.animation.frame) = ("walk".into(), 1);
    ui.set_icon(FOLLOW, None);
    assert!(!panel.clock.due(1, false, PERIOD));
    panel.refresh(&mut ui, &world, &cat, &vfs, now);
    assert_eq!(shown(&ui), (false, None), "an ordinary refresh leaves it");
    panel.clock.force = true;
    assert!(panel.clock.due(1, false, PERIOD));
    panel.refresh(&mut ui, &world, &cat, &vfs, now);
    assert_eq!(shown(&ui), first, "a forced one sets it again");
    ui.set_icon(FOLLOW, None);
    assert!(panel.clock.due(PERIOD, true, PERIOD));
    panel.refresh(&mut ui, &world, &cat, &vfs, now);
    assert_eq!(shown(&ui), (false, None), "a periodic one does not");
}
