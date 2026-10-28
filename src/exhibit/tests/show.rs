//! What a show neighbour adds to the rating (habitat rating 5.1 point 4, 6, 9.2, tank fauna 12.3)
//! on `show_world`: a shark and a polar bear in the plain tank `T`, an orca in the show tank `S`
//! and a goat on the land exhibit `L` (the bear and the orca are amphibious).

use super::neighbours::catalogue;
use crate::animal::home::home;
use crate::exhibit::rating::{self, Likes, Record, input, rate};
use crate::unit::testing::{gated, show_world, show_world2};
use crate::world::World;

fn world() -> World {
    show_world(&[
        (1, "shark", (3, 3)),
        (2, "polar", (4, 3)),
        (3, "orca", (7, 3)),
        (4, "goat", (0, 3)),
    ])
}

/// The numbers of `T`, `S` and `L`.
fn ids(w: &World) -> (u32, u32, u32) {
    let region = |x: usize| w.exhibits().region(2 * 12 + x).unwrap();
    (region(2), region(6), region(0))
}

/// The keys of the records of exhibit `x`, and those the pair rates.
fn lists(w: &World, x: u32) -> (Vec<String>, Vec<String>) {
    let records = rate(&input(w, &catalogue(), x));
    let keys = |all: bool| {
        (records.iter())
            .filter(|r| all || r.rated)
            .map(|r| r.key.clone())
            .collect()
    };
    (keys(true), keys(false))
}

/// Drains the plain tank after its two animals left.
fn empty_and_drain(w: &mut World, t: u32) {
    assert!(w.remove(1) && w.remove(2));
    w.drain(t);
    assert!(!w.exhibits().get(t).unwrap().tank.unwrap().filled);
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_plain_tank_holds_the_land_then_the_show_tank_and_rates_its_own_and_the_show_tanks() {
    let w = world();
    let (t, ..) = ids(&w);
    let (records, rated) = lists(&w, t);
    assert_eq!(records, strings(&["shark", "polar", "goat", "orca"]));
    assert_eq!(rated, strings(&["shark", "polar", "orca"]));
}

#[test]
fn the_show_tank_holds_a_record_for_each_species_of_the_plain_tank_beside_it() {
    let w = world();
    let (_, s, _) = ids(&w);
    let (records, rated) = lists(&w, s);
    assert_eq!(records, strings(&["orca", "shark", "polar"]));
    assert_eq!(rated, records);
    // None of its own: the records still hold the plain tank's species.
    let empty = show_world(&[(1, "shark", (3, 3))]);
    let (_, s, _) = ids(&empty);
    let input = input(&empty, &catalogue(), s);
    assert!(input.animals.is_empty());
    let records = rate(&input);
    assert_eq!(records.len(), 1);
    assert_eq!((records[0].key.as_str(), records[0].animals), ("shark", 0));
}

#[test]
fn the_land_exhibit_holds_the_tank_then_the_show_tank_but_rates_the_amphibious_species_of_the_tank_only()
 {
    let w = world();
    let (_, _, l) = ids(&w);
    let (records, rated) = lists(&w, l);
    assert_eq!(records, strings(&["goat", "shark", "polar", "orca"]));
    assert_eq!(rated, strings(&["goat", "polar"]));
}

#[test]
fn the_species_count_follows_the_records_and_no_animal_of_a_neighbour_is_counted() {
    let w = world();
    let (t, s, l) = ids(&w);
    for (x, count) in [(t, 4), (s, 3), (l, 4)] {
        let mut state = rating::State::default();
        state.compute(&input(&w, &catalogue(), x));
        assert_eq!(state.counts.0, count, "exhibit {x}");
    }
    let records = rate(&input(&w, &catalogue(), t));
    let animals = |k: &str| records.iter().find(|r| r.key == k).unwrap().animals;
    assert_eq!(
        (animals("shark"), animals("polar"), animals("orca")),
        (1, 1, 0)
    );
}

#[test]
fn a_drained_neighbour_takes_its_species_away() {
    let mut w = world();
    let (t, s, l) = ids(&w);
    assert_eq!(lists(&w, s).0.len(), 3);
    empty_and_drain(&mut w, t);
    assert_eq!(lists(&w, s).0, strings(&["orca"]));
    assert_eq!(lists(&w, t).0, strings(&["goat"]));
    assert_eq!(lists(&w, l).0, strings(&["goat"]));
}

#[test]
fn marking_an_exhibit_marks_its_show_neighbour() {
    let mut w = world();
    let (t, s, _) = ids(&w);
    w.exhibits_mut()
        .list_mut()
        .iter_mut()
        .for_each(|e| e.rating.dirty = false);
    w.exhibits_mut().mark(s);
    assert!(w.exhibits().get(t).unwrap().rating.dirty);
}

#[test]
fn a_show_tank_rates_an_amphibious_species_50_lower_only_without_a_show_neighbour() {
    let mut w = world();
    let (t, s, _) = ids(&w);
    let orca = |w: &World| -> Record {
        let records = rate(&input(w, &catalogue(), s));
        records.into_iter().find(|r| r.key == "orca").unwrap()
    };
    let with = orca(&w);
    assert!(with.show && with.show_linked);
    let without = orca(&{
        empty_and_drain(&mut w, t);
        w
    });
    assert!(!without.show_linked);
    assert_eq!(with.rating() - without.rating(), 50.0);
}

#[test]
fn a_plain_tank_rates_an_amphibious_species_50_lower_whatever_its_show_neighbours() {
    let w = world();
    let (t, _, _) = ids(&w);
    let records = rate(&input(&w, &catalogue(), t));
    let orca = records.iter().find(|r| r.key == "orca").unwrap();
    assert!(orca.tank && !orca.show && orca.amphibious && orca.show_linked);
    let plain = Record {
        amphibious: false,
        ..orca.clone()
    };
    assert_eq!(plain.rating() - orca.rating(), 50.0);
}

#[test]
fn the_home_of_an_animal_in_a_show_tank_is_the_plain_tank_beside_it() {
    let cat = catalogue();
    let mut w = gated(world());
    let (t, s, _) = ids(&w);
    let orca = |cat: &_| Likes::of(cat_type(cat, "orca"));
    assert_eq!(home(&w, &cat, s, &orca(&cat)), Some(t));
    empty_and_drain(&mut w, t);
    assert_eq!(
        home(&w, &cat, s, &orca(&cat)),
        None,
        "a show tank is no home"
    );
}

#[test]
fn an_amphibious_type_of_a_second_neighbour_is_listed_though_a_show_neighbour_of_the_first_holds_it()
 {
    // L has the plain tanks T (shark; its show tank S holds a polar bear) and T2 (polar bear).
    let w = show_world2(&[
        (1, "shark", (3, 3)),
        (3, "polar", (7, 3)),
        (4, "goat", (0, 3)),
        (5, "polar", (0, 6)),
    ]);
    let l = w.exhibits().region(2 * 12).unwrap();
    let t2 = w.exhibits().region(6 * 12).unwrap();
    assert_eq!(w.exhibits().get(l).unwrap().amphibious.len(), 2);
    assert!(w.exhibits().get(t2).is_some());
    let records = rate(&input(&w, &catalogue(), l));
    let bear = records.iter().find(|r| r.key.contains("polar")).unwrap();
    assert!(bear.rated);
}

fn cat_type<'a>(cat: &'a crate::entity::Catalogue, name: &str) -> &'a crate::entity::EntityType {
    cat.find("animals", name.as_bytes(), b"").unwrap()
}
