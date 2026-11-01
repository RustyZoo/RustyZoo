//! The predator search on a synthetic map: a 20 by 20 zoo with a pen round (2, 2) to (4, 4).

use super::*;
use crate::animal::Animal;
use crate::config::Config;
use crate::unit::testing::{centre, entity, world_with};
use std::sync::Arc;

const GUEST: u32 = 1;
const KEEPER: u32 = 2;

fn ty(class: &'static str, name: &str, sub: &str, id: i32, keys: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(
            Config::parse(format!("[{sub}/Characteristics/Integers]\n{keys}\n").as_bytes())
                .unwrap(),
        )),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// A lion eats men, a trex keepers too, a zebra nobody; `radius` is `cPredatorRadius`.
fn cat(radius: Option<i32>) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(ty("animals", "lion", "m", 5006, "cPrey=5001\ncPrey=9503"));
    cat.push(ty("animals", "trex", "m", 7001, "cPrey=9500"));
    cat.push(ty("animals", "trex", "y", 7001, "cPrey=9500"));
    cat.push(ty("animals", "zebra", "m", 5001, ""));
    cat.push(ty("keeper", "keeper", "m", 9500, ""));
    cat.predator_radius = radius;
    cat
}

/// An animal: id, type, subtype, tile, escaped.
type Beast<'a> = (u32, &'a str, &'a str, (i32, i32), bool);

/// The guest at `guest`, a keeper at (11, 10) and the animals of `animals`; a pen of fences round
/// (2, 2) to (4, 4).
fn zoo(guest: (i32, i32), animals: &[Beast]) -> World {
    let mut w = world_with((20, 20), |zoo| {
        for i in 0..3 {
            for (tile, d) in [
                ((2 + i, 2), 0),
                ((2 + i, 4), 4),
                ((2, 2 + i), 6),
                ((4, 2 + i), 2),
            ] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(tile), d));
            }
        }
        zoo.entities
            .push(entity("guests", "man", GUEST, centre(guest), 0));
        let mut keeper = entity("keeper", "keeper", KEEPER, centre((11, 10)), 0);
        keeper.subtype = "m".into();
        zoo.entities.push(keeper);
        for &(id, kind, sub, tile, _) in animals {
            let mut a = entity("animals", kind, id, centre(tile), 0);
            a.subtype = sub.into();
            zoo.entities.push(a);
        }
    });
    for &(id, _, _, _, escaped) in animals {
        let unit = w.units.get_mut(id).unwrap();
        unit.part = Part::Animal(Animal {
            escaped,
            ..Animal::default()
        });
        if escaped {
            w.units.escapes.push(id);
        }
    }
    w
}

fn look(w: &World, cat: &Catalogue, id: u32, kept: u32) -> u32 {
    let mut unit = w.units.get(id).unwrap().clone();
    unit.predator = kept;
    find(w, cat, &[], &unit, w.find(id).unwrap(), 0)
}

#[test]
fn a_guest_on_the_grounds_sees_an_escaped_man_eater_strictly_inside_the_circle() {
    let cat = cat(None);
    // The radius is 5 when the file has none: (3, 4) away is 25, not below 25.
    let w = zoo((10, 10), &[(50, "lion", "m", (13, 14), true)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 0);
    let w = zoo((10, 10), &[(50, "lion", "m", (13, 13), true)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 50);
    // Not escaped, crated, an egg or a zebra: none.
    let w = zoo((10, 10), &[(50, "lion", "m", (13, 13), false)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 0);
    let w = zoo((10, 10), &[(51, "zebra", "m", (11, 11), true)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 0);
    let mut w = zoo((10, 10), &[(50, "lion", "m", (11, 11), true)]);
    if let Some(Part::Animal(a)) = w.units.get_mut(50).map(|u| &mut u.part) {
        a.crated = true;
    }
    assert_eq!(look(&w, &cat, GUEST, 0), 0);
}

#[test]
fn the_kept_predator_stays_one_inside_the_square() {
    let cat = cat(None);
    let w = zoo((10, 10), &[(50, "lion", "m", (15, 15), true)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 0);
    assert_eq!(look(&w, &cat, GUEST, 50), 50);
    let w = zoo((10, 10), &[(50, "lion", "m", (16, 10), true)]);
    assert_eq!(look(&w, &cat, GUEST, 50), 0);
    // A removed one is no longer kept.
    let mut unit = w.units.get(GUEST).unwrap().clone();
    unit.predator = 50;
    let w = zoo((10, 10), &[(50, "lion", "m", (12, 12), false)]);
    assert_eq!(find(&w, &cat, &[50], &unit, w.find(GUEST).unwrap(), 0), 0);
    // Inside the exhibit, the kept one counts whatever region it stands in.
    let w = zoo((3, 3), &[(50, "lion", "m", (6, 6), false)]);
    assert_eq!(look(&w, &cat, GUEST, 50), 50);
}

#[test]
fn inside_an_exhibit_only_the_units_of_that_exhibit_count_escaped_or_not() {
    let cat = cat(None);
    let w = zoo((2, 2), &[(50, "lion", "m", (4, 4), false)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 50);
    let w = zoo((2, 2), &[(50, "lion", "m", (5, 5), true)]);
    assert_eq!(look(&w, &cat, GUEST, 0), 0);
}

#[test]
fn the_nearest_wins_and_ties_keep_the_first_of_the_escaped_list() {
    let cat = cat(None);
    let mut w = zoo(
        (10, 10),
        &[
            (50, "lion", "m", (12, 10), true),
            (51, "lion", "m", (11, 10), true),
            (52, "lion", "m", (10, 11), true),
        ],
    );
    assert_eq!(look(&w, &cat, GUEST, 0), 51);
    // The list is in the order of escape; one no longer loose is not on it.
    w.units.escapes = vec![52, 51, 50];
    w.units.get_mut(50).unwrap().part = Part::Animal(Animal::default());
    assert_eq!(escaped(&w), [w.find(52).unwrap(), w.find(51).unwrap()]);
    assert_eq!(look(&w, &cat, GUEST, 0), 52);
}

#[test]
fn a_keeper_fears_an_adult_that_preys_on_keepers_only_on_the_grounds() {
    let w = zoo((10, 10), &[(50, "lion", "m", (12, 10), true)]);
    assert_eq!(look(&w, &cat(None), KEEPER, 0), 0);
    let w = zoo((10, 10), &[(50, "trex", "m", (12, 10), true)]);
    assert_eq!(look(&w, &cat(None), KEEPER, 0), 50);
    // The young trex only with `cBabiesAttack` 1.
    let w = zoo((10, 10), &[(50, "trex", "y", (12, 10), true)]);
    assert_eq!(look(&w, &cat(None), KEEPER, 0), 0);
    let mut babies = Catalogue::default();
    babies.push(ty(
        "animals",
        "trex",
        "y",
        7001,
        "cPrey=9500\ncBabiesAttack=1",
    ));
    babies.push(ty("keeper", "keeper", "m", 9500, ""));
    assert_eq!(look(&w, &babies, KEEPER, 0), 50);
    // The playmate search never finds one for staff.
    let w = zoo((10, 10), &[(50, "trex", "m", (12, 10), true)]);
    let unit = w.units.get(KEEPER).unwrap().clone();
    assert_eq!(
        find(&w, &cat(None), &[], &unit, w.find(KEEPER).unwrap(), 1),
        0
    );
}

#[test]
fn the_radius_at_zero_negative_and_huge() {
    let w = zoo((10, 10), &[(50, "lion", "m", (10, 11), true)]);
    assert_eq!(look(&w, &cat(Some(0)), GUEST, 0), 0);
    // The kept one on the same tile only.
    assert_eq!(look(&w, &cat(Some(0)), GUEST, 50), 0);
    // A negative radius scans nothing but its square still bounds the escaped list.
    assert_eq!(look(&w, &cat(Some(-2)), GUEST, 0), 50);
    let far = zoo((0, 0), &[(50, "lion", "m", (19, 19), true)]);
    assert_eq!(look(&far, &cat(Some(i32::MAX)), GUEST, 0), 50);
    assert_eq!(look(&far, &cat(Some(i32::MIN)), GUEST, 0), 50);
    let pen = zoo((2, 2), &[(50, "lion", "m", (4, 4), false)]);
    assert_eq!(look(&pen, &cat(Some(i32::MAX)), GUEST, 0), 50);
}
