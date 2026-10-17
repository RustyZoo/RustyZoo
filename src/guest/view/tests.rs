//! The viewing area of a guest beside a pen of zebras, what it sees, the viewing check and the
//! donation.

use super::*;
use crate::animal::Animal;
use crate::unit::testing::{centre, entity, world_with};

fn kind(class: &'static str, name: &str, name_id: i32) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn catalogue_with_species() -> Catalogue {
    let mut cat = catalogue();
    for (n, name) in ["b", "c", "d"].into_iter().enumerate() {
        cat.push(kind("animals", name, 6 + n as i32));
    }
    cat
}

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(kind("animals", "zebra", 5));
    cat.push(kind("guests", "man", 9503));
    cat
}

/// A 10 by 10 map with the zoo entrance at (9, 5), a pen of 3 by 3 tiles round (2, 5) holding
/// one zebra for each happiness of `zebras`, and a path along x = 5 from y = 4 to 6. Guest 1
/// stands on the path at (5, 5); the zebras are 10, 11, ...
fn pen(zebras: &[i32]) -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (9, 5);
        zoo.entities
            .push(entity("guests", "man", 1, centre((5, 5)), 0));
        for i in 0..3 {
            for (at, d) in [
                ((1 + i, 4), 0),
                ((1 + i, 6), 4),
                ((1, 4 + i), 6),
                ((3, 4 + i), 2),
            ] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(at), d));
            }
            zoo.entities.push(entity(
                "paths",
                "stone",
                200 + i as u32,
                centre((5, 4 + i)),
                0,
            ));
        }
        for n in 0..zebras.len() {
            zoo.entities
                .push(entity("animals", "zebra", 10 + n as u32, centre((2, 5)), 0));
        }
    });
    for (n, &happiness) in zebras.iter().enumerate() {
        let part = Part::Animal(Animal {
            happiness,
            ..Animal::default()
        });
        w.units.get_mut(10 + n as u32).unwrap().part = part;
    }
    w
}

fn keys() -> Keys {
    Keys::of(&kind("guests", "man", 9503))
}

fn drew(before: &Rng, after: &Rng) -> usize {
    let mut r = before.clone();
    let mut n = 0;
    while r != *after && n < 20 {
        r.rand();
        n += 1;
    }
    n
}

#[test]
fn a_guest_on_a_path_beside_a_pen_finds_its_area_and_sees_the_animals() {
    let (mut w, cat) = (pen(&[40, 60]), catalogue());
    let tile = 5 * 10 + 5;
    let mut rng = Rng::default();
    let dir = lookup(&mut w, &cat, Some(tile), &mut rng).expect("an area");
    assert_eq!(
        rng.state(),
        {
            let mut r = Rng::default();
            r.rand();
            r.state()
        },
        "one draw"
    );
    let s = sight(&mut w, &cat, tile, dir).unwrap();
    assert_eq!((s.exhibit, s.total, s.animals), (1, 2, 2));
    assert_eq!((s.species, s.sick, s.average), (vec![5], 0, 50));
    // The guest alone on a 3-tile area: ten times one third, rounded.
    assert_eq!(s.crowd, 3);
    // Elsewhere there is nothing to look through, and the lookup draws all the same.
    let mut rng = Rng::default();
    assert_eq!(lookup(&mut w, &cat, Some(9 * 10 + 9), &mut rng), None);
    assert_eq!(lookup(&mut w, &cat, None, &mut rng), None);
    assert_eq!(drew(&Rng::default(), &rng), 2);
    // Inside the pen a guest has no area, and a tile of the exhibit is not a viewing tile.
    assert!(sight(&mut w, &cat, 5 * 10 + 2, dir).is_none());
    let areas = viewing_areas(&mut w, &cat, 1);
    assert!(
        !areas.is_empty() && areas.concat().iter().all(|t| t % 10 == 5 || t % 10 == 4),
        "{areas:?}"
    );
    // The areas are built again when the map changes.
    let stamp = w.units.views.edits;
    assert_eq!(stamp, Some(w.edits));
    assert!(w.set_tile(8, 8, 1, 0, 0));
    assert_ne!(w.units.views.edits, Some(w.edits));
    viewing_areas(&mut w, &cat, 1);
    assert_eq!(w.units.views.edits, Some(w.edits));
}

#[test]
fn the_viewing_check_changes_happiness_by_the_average_of_the_exhibit() {
    let (cat, tile, k) = (catalogue(), Some(55), keys());
    for (happiness, change) in [
        (90, 10),
        (70, 5),
        (60, -5),
        (-60, -5),
        (-70, -10),
        (-90, -15),
    ] {
        let mut w = pen(&[happiness, happiness]);
        let (before, mut rng) = (Rng::default(), Rng::default());
        let mut g = Guest::default();
        check(&mut g, &k, &mut w, &cat, &mut rng, tile);
        assert_eq!(g.pending[HAPPINESS], change, "{happiness}");
        assert_eq!(
            drew(&before, &rng),
            2,
            "the lookup and the animal of the thought"
        );
    }
    // A sick animal in view costs more and one more draw; nothing in view changes nothing.
    let mut w = pen(&[90, 90]);
    let id = 10;
    let sick = Part::Animal(Animal {
        happiness: 90,
        dying: true,
        ..Animal::default()
    });
    w.units.get_mut(id).unwrap().part = sick;
    let (before, mut rng) = (Rng::default(), Rng::default());
    let mut g = Guest::default();
    check(&mut g, &k, &mut w, &cat, &mut rng, tile);
    assert_eq!(
        (g.pending[HAPPINESS], drew(&before, &rng)),
        (0, 3),
        "-10 and +10"
    );
    let mut w = pen(&[]);
    let (before, mut rng) = (Rng::default(), Rng::default());
    let mut g = Guest::default();
    check(&mut g, &k, &mut w, &cat, &mut rng, tile);
    assert_eq!((g.pending, drew(&before, &rng)), ([0; 5], 1));
}

#[test]
fn many_species_and_a_crowd_change_happiness_too() {
    let (cat, k) = (catalogue_with_species(), keys());
    let mut w = pen(&[90, 90, 90, 90]);
    // Four species in view: the variety adds +10 to the +10 of a happy exhibit.
    for (n, name) in ["b", "c", "d"].into_iter().enumerate() {
        let at = w.find(11 + n as u32).unwrap();
        w.zoo.entities[at].type_name = name.into();
    }
    let (before, mut rng) = (Rng::default(), Rng::default());
    let mut g = Guest::default();
    check(&mut g, &k, &mut w, &cat, &mut rng, Some(55));
    assert_eq!((g.pending[HAPPINESS], drew(&before, &rng)), (20, 2));
    // A crowd of 25 guests more on three tiles (87) costs 10 and another draw.
    for n in 0..25 {
        w.zoo
            .entities
            .push(entity("guests", "man", 300 + n, centre((5, 4)), 0));
    }
    let (before, mut rng) = (Rng::default(), Rng::default());
    let mut g = Guest::default();
    check(&mut g, &k, &mut w, &cat, &mut rng, Some(55));
    assert_eq!((g.pending[HAPPINESS], drew(&before, &rng)), (10, 3));
}

#[test]
fn leaving_the_area_gives_the_time_watched_and_a_donation() {
    let (mut w, cat) = (pen(&[40, 60]), catalogue());
    w.units.donation_factor = 4.1108e-13;
    let mut rng = Rng::default();
    let mut g = Guest {
        viewed_at: 1000,
        ..Guest::default()
    };
    g.va = lookup(&mut w, &cat, Some(55), &mut rng);
    w.units.now = 201_601_000;
    leave_area(&mut g, 1, &mut w, &cat, Some(55));
    // 28 s of play are 201,600 calendar seconds; the average happiness is 50.
    let x = w.exhibits().get(1).unwrap();
    assert_eq!(x.stats.viewing, 2_016_000_000_000);
    assert!(
        (x.stats.donations[1] - 0.4144).abs() < 1e-3,
        "{:?}",
        x.stats.donations
    );
    assert_eq!(x.stats.donations[1], x.stats.donations[2]);
    assert_eq!(w.units.donated, [x.stats.donations[1]]);
    assert_eq!(w.units.take_guest_lines().len(), 1);
    assert_eq!(g.va, None);
    // A long view is capped at 10; a sick animal in view, or a view of nothing, gives none.
    w.units.donated.clear();
    g.va = lookup(&mut w, &cat, Some(55), &mut rng);
    w.units.now = u64::MAX / 1000;
    leave_area(&mut g, 1, &mut w, &cat, Some(55));
    assert_eq!(w.units.donated, [10.0]);
    let mut sick = pen(&[40, 60]);
    sick.units.get_mut(10).unwrap().part = Part::Animal(Animal {
        dying: true,
        ..Animal::default()
    });
    sick.units.donation_factor = 4.1108e-13;
    sick.units.now = 201_601_000;
    g.va = lookup(&mut sick, &cat, Some(55), &mut rng);
    leave_area(&mut g, 1, &mut sick, &cat, Some(55));
    assert!(sick.units.donated.is_empty());
    assert!(
        sick.exhibits().get(1).unwrap().stats.viewing > 0,
        "the time still counts"
    );
    // A factor of 0, a negative one and one that is not a number give nothing, a huge one 10.
    for (factor, expected) in [
        (0.0, vec![]),
        (-1.0, vec![]),
        (f32::NAN, vec![]),
        (f32::INFINITY, vec![10.0]),
    ] {
        let mut w = pen(&[40]);
        (w.units.donation_factor, w.units.now) = (factor, 201_601_000);
        g.va = lookup(&mut w, &cat, Some(55), &mut rng);
        leave_area(&mut g, 1, &mut w, &cat, Some(55));
        assert_eq!(w.units.donated, expected, "{factor}");
    }
    // Negative happiness gives nothing, and a guest with no area changes nothing.
    let mut sad = pen(&[-50]);
    (sad.units.donation_factor, sad.units.now) = (4.1108e-13, 201_601_000);
    g.va = lookup(&mut sad, &cat, Some(55), &mut rng);
    leave_area(&mut g, 1, &mut sad, &cat, Some(55));
    assert!(sad.units.donated.is_empty());
    leave_area(&mut g, 1, &mut sad, &cat, None);
}

#[test]
fn the_seen_list_keeps_ten_and_the_unseen_exhibits_are_those_with_animals_not_seen() {
    let (w, cat) = (pen(&[40]), catalogue());
    let mut g = Guest::default();
    assert_eq!(sites(&w, &cat, &g), (1, vec![1]));
    // The exhibit's first tile counts as seeing it, and so does any tile of it.
    let first = w.exhibits().get(1).unwrap().first;
    see(&mut g, first);
    assert_eq!((g.seen_count, sites(&w, &cat, &g)), (1, (1, vec![])));
    for t in 0..11 {
        see(&mut g, t + 80);
    }
    assert_eq!((g.seen.len(), g.seen_count, g.seen[0]), (10, 12, 81));
    assert_eq!(
        sites(&w, &cat, &g).1,
        vec![1],
        "the oldest entries are forgotten"
    );
    // An empty exhibit is no site.
    assert_eq!(sites(&pen(&[]), &cat, &Guest::default()), (0, vec![]));
}

#[test]
fn the_site_draws_an_area_then_one_of_its_tiles() {
    let (mut w, cat) = (pen(&[40]), catalogue());
    let areas = viewing_areas(&mut w, &cat, 1);
    assert!(!areas.is_empty());
    let (mut rng, mut by_hand) = (Rng::default(), Rng::default());
    let tile = viewing_tile(&mut w, &cat, 1, &mut rng).expect("a tile");
    // One draw for the area, one for a tile of that area's own list.
    let area = &areas[by_hand.pick(areas.len() as u32) as usize];
    assert_eq!(tile, area[by_hand.pick(area.len() as u32) as usize]);
    assert_eq!(rng, by_hand);
    assert_eq!(drew(&Rng::default(), &rng), 2);
    // An exhibit that is none, or sees no tile, is skipped with no draw.
    let mut rng = Rng::default();
    assert_eq!(viewing_tile(&mut w, &cat, 99, &mut rng), None);
    assert_eq!(rng, Rng::default());
}
