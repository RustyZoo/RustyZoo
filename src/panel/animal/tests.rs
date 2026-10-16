use super::*;
use crate::entity::Catalogue;
use crate::vfs::Vfs;

fn keys() -> Keys {
    Keys::read(
        &|key, fallback| match key {
            "cMaxHits" => 500,
            "cHabitatPreference" => 50,
            "cPctHabitat" => 10,
            _ => fallback,
        },
        &Default::default(),
    )
}

fn animal() -> Animal {
    Animal {
        happiness: 98,
        hunger: 70,
        health: 250,
        ..Animal::default()
    }
}

#[test]
fn the_bars_follow_the_formulas_of_the_panel() {
    let b = bars(&animal(), &keys(), 91, false);
    // (98 + 100) * 100 / 200, 100 - 70, 250 * 100 / 500, the rating itself.
    assert_eq!(
        b,
        Bars {
            values: [99, 30, 50, 91],
            level: 2
        }
    );
    let b = bars(
        &Animal {
            happiness: -100,
            hunger: 100,
            health: 0,
            ..animal()
        },
        &keys(),
        0,
        false,
    );
    assert_eq!(b.values, [0, 0, 0, 0]);
    let full = Animal {
        happiness: 100,
        hunger: 0,
        health: 500,
        ..animal()
    };
    assert_eq!(
        bars(&full, &keys(), 100, false).values,
        [100, 100, 100, 100]
    );
}

#[test]
fn an_egg_shows_happiness_hunger_and_suitability_full() {
    let egg = Animal {
        egg: true,
        ..animal()
    };
    let b = bars(&egg, &keys(), 20, false);
    assert_eq!(
        b.values,
        [100, 100, 50, 100],
        "the health bar keeps its formula"
    );
}

#[test]
fn the_suitability_colour_follows_the_habitat_check() {
    let level = |a: &Animal, rating, carried| bars(a, &keys(), rating, carried).level;
    // Very unhappy below 10% of the preference (5), unhappy below 50, else green.
    assert_eq!(level(&animal(), 4, false), 0);
    assert_eq!(level(&animal(), 5, false), 1);
    assert_eq!(level(&animal(), 49, false), 1);
    assert_eq!(level(&animal(), 50, false), 2);
    // A carried animal is never unhappy, but it can still be very unhappy.
    let egg = Animal {
        egg: true,
        ..animal()
    };
    assert_eq!(level(&egg, 20, false), 2);
    assert_eq!(level(&animal(), 20, true), 2);
    assert_eq!(level(&animal(), 0, true), 0);
}

#[test]
fn an_egg_very_unhappy_shows_a_green_suitability_bar() {
    let egg = Animal {
        egg: true,
        ..animal()
    };
    let level = |rating| bars(&egg, &keys(), rating, false).level;
    // The egg shows 100, so the transitions (r + 1, r + 2) of a very unhappy egg stay above it.
    assert_eq!(level(0), 2);
    assert_eq!(level(4), 2);
    assert_eq!(level(-5), 2);
    assert_eq!(level(i32::MIN), 2);
}

#[test]
fn a_species_without_limits_or_with_huge_values_gives_numbers_not_panics() {
    let k = Keys::read(&|_, _| 0, &Default::default());
    let wild = Animal {
        happiness: i32::MAX,
        hunger: i32::MIN,
        health: i32::MAX,
        ..Animal::default()
    };
    let b = bars(&wild, &k, i32::MAX, false);
    assert_eq!(
        b.values[2],
        i32::MAX,
        "no health limit: the value saturates"
    );
    assert_eq!(percent(i32::MAX, i32::MIN, i32::MAX), 100);
    assert_eq!(
        percent(5, 7, 7),
        -200,
        "limits that meet do not divide by zero"
    );
}

#[test]
fn the_time_ago_has_three_bands_by_the_hours_of_calendar_time() {
    let at = |hours: u64| band(hours * HOUR, 0);
    assert_eq!(
        [at(0), at(1439), at(1440), at(8640), at(8641)],
        [0, 0, 1, 1, 2]
    );
    assert_eq!(band(0, u64::MAX), 0, "a date in the future is recent");
    assert_eq!(band(u64::MAX, 0), 2, "huge");
}

use crate::sprite::tests::Install;
use crate::ui::draw::Strings;
use crate::ui::layout::{DISABLED, HIDDEN, Tree};
use crate::unit::testing::{centre, entity, world};

struct Texts;

impl Strings for Texts {
    fn string(&self, id: i32) -> String {
        match id {
            3104 => "Mood %d".into(),
            3116 | 3119 => "Recently".into(),
            3117 | 3120 => "A while".into(),
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

/// A panel with the elements the refresh fills, and a map with a zebra (1), a guest (2) and an
/// egg (3).
fn panel_in_game(name: &str) -> (Interface, Vfs, World) {
    let bar = |id: i32| {
        format!("[b{id}]\ntype=UIStatusImage\nid={id}\nx=0\ny=0\ndx=10\ndy=2\nimage=ui/g.tga\n")
    };
    let text = |id: i32| format!("[t{id}]\ntype=UIText\nid={id}\nx=0\ny=0\ndx=10\ndy=4\n");
    let button = |id: i32| format!("[u{id}]\ntype=UIButton\nid={id}\nx=0\ny=0\ndx=4\ndy=4\n");
    let layout = [3104, 3108, 3106, 3110].map(bar).concat()
        + &[3176, 3177, 3178, 3179].map(text).concat()
        + &[3111, 3115, 3197, 3114].map(button).concat()
        + "[s]\ntype=UIImageSet\nid=3172\nx=0\ny=0\nimage=ui/g.tga\nimage=ui/g.tga\n";
    let layout = format!("[LayoutInfo]\nid=6\nx=0\ny=0\ndx=40\ndy=40\n{layout}");
    let root = "[LayoutInfo]\nid=1\nx=0\ny=0\ndx=whole\ndy=whole\n\
        [a]\ntype=ZTMapView\nid=7000\ndx=whole\ndy=whole\n[b]\ntype=UILayout\nlayout=ui/p.lyt\n";
    let install = Install::new(
        name,
        &[
            ("ui/root.lyt", root.as_bytes().to_vec()),
            ("ui/p.lyt", layout.into_bytes()),
            ("ui/g.tga", tga()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let mut tree = Tree::load(&vfs, "ui/root.lyt", 0);
    tree.resolve(100, 80, |_| 12);
    let mut ui = Interface::new(tree, Box::new(Texts));
    ui.show(6);
    let world = world((4, 4), |_, e| {
        e.push(entity("animals", "zebra", 1, centre((1, 1)), 0));
        e.push(entity("guests", "guest", 2, centre((2, 1)), 0));
        e.push(entity("animals", "egg", 3, centre((1, 2)), 0));
    });
    (ui, vfs, world)
}

fn flags(ui: &Interface, id: i32) -> u32 {
    ui.tree.elements[ui.tree.find(id).unwrap()].state
}

#[test]
fn a_refresh_writes_the_bars_the_time_rows_and_the_buttons_and_tells_what_changed() {
    let (mut ui, vfs, mut world) = panel_in_game("animal-panel-refresh");
    let cat = Catalogue::default();
    let Part::Animal(a) = &mut world.units.get_mut(1).unwrap().part else {
        panic!("not an animal");
    };
    (a.happiness, a.hunger, a.health) = (98, 70, 250);
    let mut panel = Panel::default();
    // No selection, no such entity, and an entity that is not an animal: nothing.
    for id in [None, Some(99), Some(2)] {
        panel.selected = id;
        assert_eq!(panel.refresh(&mut ui, &world, &cat, &vfs, 0), None);
    }
    panel.select(1);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert_eq!(
        told,
        "animal 1 \"\" happiness 99 hunger 30 health 50 suitability 0 fed Recently slept Recently"
    );
    assert_eq!(ui.string(3104), "Mood 99", "the tooltip holds the value");
    assert_eq!(ui.text(3177).unwrap(), b"Recently");
    assert_eq!(flags(&ui, 3176) & HIDDEN, 0);
    assert_eq!(flags(&ui, 3111) & DISABLED, 0);
    assert_eq!(
        panel.refresh(&mut ui, &world, &cat, &vfs, 0),
        None,
        "unchanged"
    );
    // 1500 hours later the rows read "a while"; a carried animal cannot be sold.
    world.units.get_mut(1).unwrap().carried = true;
    let told = panel
        .refresh(&mut ui, &world, &cat, &vfs, 1500 * HOUR)
        .unwrap();
    assert!(told.ends_with("fed A while slept A while"), "{told}");
    for id in [3111, 3115, 3197] {
        assert_ne!(flags(&ui, id) & DISABLED, 0, "{id}");
    }
    // An egg hides the time rows and shows its bars full.
    let Part::Animal(egg) = &mut world.units.get_mut(3).unwrap().part else {
        panic!("not an animal");
    };
    egg.egg = true;
    panel.select(3);
    let told = panel.refresh(&mut ui, &world, &cat, &vfs, 0).unwrap();
    assert!(told.contains("happiness 100 hunger 100"), "{told}");
    for id in [3176, 3177, 3178, 3179] {
        assert_ne!(flags(&ui, id) & HIDDEN, 0, "{id}");
    }
}
