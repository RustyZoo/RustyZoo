//! The predator search on a synthetic map: a 20 by 20 zoo with a pen round (2, 2) to (4, 4).

use super::*;
use crate::animal::Animal;
use crate::fixtures::unit_ty;
use crate::unit::testing::{centre, entity, world_with};

const GUEST: u32 = 1;
const KEEPER: u32 = 2;

/// A lion eats men, a trex keepers too, a zebra nobody; `radius` is `cPredatorRadius`.
fn cat(radius: Option<i32>) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(unit_ty(
        "animals",
        "lion",
        "m",
        5006,
        "cPrey=5001\ncPrey=9503",
    ));
    cat.push(unit_ty("animals", "trex", "m", 7001, "cPrey=9500"));
    cat.push(unit_ty("animals", "trex", "y", 7001, "cPrey=9500"));
    cat.push(unit_ty("animals", "zebra", "m", 5001, ""));
    cat.push(unit_ty("keeper", "keeper", "m", 9500, ""));
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
    babies.push(unit_ty(
        "animals",
        "trex",
        "y",
        7001,
        "cPrey=9500\ncBabiesAttack=1",
    ));
    babies.push(unit_ty("keeper", "keeper", "m", 9500, ""));
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
}

#[test]
fn the_predator_radius_is_32_bit_arithmetic() {
    // r * r wraps: from 46341 it is negative, so no new threat is found.
    let w = zoo((10, 10), &[(50, "lion", "m", (10, 11), true)]);
    assert_eq!(look(&w, &cat(Some(46340)), GUEST, 0), 50);
    assert_eq!(look(&w, &cat(Some(46341)), GUEST, 0), 0);
    // i32::MAX squares to 1: only a threat on the guest's own tile; i32::MIN squares to 0.
    assert_eq!(look(&w, &cat(Some(i32::MAX)), GUEST, 0), 0);
    assert_eq!(look(&w, &cat(Some(i32::MIN)), GUEST, 0), 0);
    let own = zoo((10, 10), &[(50, "lion", "m", (10, 10), true)]);
    assert_eq!(look(&own, &cat(Some(i32::MAX)), GUEST, 0), 50);
    assert_eq!(look(&own, &cat(Some(i32::MIN)), GUEST, 0), 0);
    // Inside an exhibit the same: nothing is found at the huge radius.
    let pen = zoo((2, 2), &[(50, "lion", "m", (4, 4), false)]);
    assert_eq!(look(&pen, &cat(Some(3)), GUEST, 0), 50);
    assert_eq!(look(&pen, &cat(Some(i32::MAX)), GUEST, 0), 0);
}

#[test]
fn an_animal_in_an_exhibit_fears_an_adult_that_preys_on_its_type_darted_or_not() {
    // Shows 30.2 point 4.6: the zebra in the pen sees the lion there; nothing but the type and
    // the age counts for an animal (the grounds and the dart are a staff member's tests).
    let cat = cat(None);
    let mut w = zoo(
        (10, 10),
        &[
            (50, "lion", "m", (4, 4), false),
            (51, "zebra", "m", (3, 3), false),
        ],
    );
    assert_eq!(look(&w, &cat, 51, 0), 50);
    assert_eq!(look(&w, &cat, 50, 0), 0);
    if let Some(Part::Animal(a)) = w.units.get_mut(50).map(|u| &mut u.part) {
        a.tranq = true;
    }
    assert_eq!(look(&w, &cat, 51, 0), 50);
    // The young do not hunt; the playmate search finds none (no animal chases yet).
    let w = zoo(
        (10, 10),
        &[
            (50, "lion", "y", (4, 4), false),
            (51, "zebra", "m", (3, 3), false),
        ],
    );
    assert_eq!(look(&w, &cat, 51, 0), 0);
    let w = zoo(
        (10, 10),
        &[
            (50, "lion", "m", (4, 4), false),
            (51, "zebra", "m", (3, 3), false),
        ],
    );
    let unit = w.units.get(51).unwrap().clone();
    assert_eq!(find(&w, &cat, &[], &unit, w.find(51).unwrap(), 1), 0);
}

#[test]
fn an_animal_hunts_the_units_of_its_prey_list_in_its_exhibit() {
    let cat = cat(None);
    let w = zoo(
        (10, 10),
        &[
            (50, "lion", "m", (4, 4), false),
            (51, "zebra", "m", (3, 3), false),
        ],
    );
    let prey = |w: &World, id| hunt(w, &cat, id);
    assert_eq!((prey(&w, 50), prey(&w, 51)), (51, 0));
    // Another exhibit, or the grounds, holds no prey of the pen.
    let w = zoo(
        (10, 10),
        &[
            (50, "lion", "m", (4, 4), false),
            (51, "zebra", "m", (9, 9), true),
        ],
    );
    assert_eq!(prey(&w, 50), 0);
}

fn hunt(w: &World, cat: &Catalogue, id: u32) -> u32 {
    let unit = w.units.get(id).unwrap().clone();
    find_prey(w, cat, &unit, w.find(id).unwrap())
}

/// The catalogue of `cat` with `cPreyRadius` set.
fn prey_radius_cat(radius: Option<i32>) -> Catalogue {
    let mut cat = cat(None);
    cat.prey_radius = radius;
    cat
}

#[test]
fn the_prey_search_covers_the_square_of_cpreyradius_with_no_circle() {
    // A lion on the grounds at (10, 10); the zebra is prey, escaped or not.
    let at = |tile| {
        zoo(
            (0, 0),
            &[
                (50, "lion", "m", (10, 10), false),
                (51, "zebra", "m", tile, false),
            ],
        )
    };
    let cat = prey_radius_cat(None);
    // The radius is 5 (not `cPredatorRadius`): (5, 5) away counts, 6 on an axis does not.
    assert_eq!(hunt(&at((15, 15)), &cat, 50), 51);
    assert_eq!(hunt(&at((16, 10)), &cat, 50), 0);
    assert_eq!(hunt(&at((10, 4)), &cat, 50), 0);
    // `cPredatorRadius` 7 changes nothing.
    let mut wide = prey_radius_cat(None);
    wide.predator_radius = Some(7);
    assert_eq!(hunt(&at((16, 10)), &wide, 50), 0);
    // The radius at 0, negative, huge and missing.
    assert_eq!(hunt(&at((10, 11)), &prey_radius_cat(Some(0)), 50), 0);
    assert_eq!(hunt(&at((10, 11)), &prey_radius_cat(Some(1)), 50), 51);
    assert_eq!(hunt(&at((10, 11)), &prey_radius_cat(Some(-3)), 50), 0);
    assert_eq!(
        hunt(&at((19, 19)), &prey_radius_cat(Some(1_000_000)), 50),
        51
    );
    // A bound that wraps leaves the scan empty, as in the original.
    assert_eq!(hunt(&at((11, 10)), &prey_radius_cat(Some(i32::MAX)), 50), 0);
    assert_eq!(hunt(&at((19, 19)), &prey_radius_cat(Some(i32::MIN)), 50), 0);
    // The nearest wins, the first found of equal distances (the outer axis is x).
    let w = zoo(
        (0, 0),
        &[
            (50, "lion", "m", (10, 10), false),
            (51, "zebra", "m", (13, 10), false),
            (52, "zebra", "m", (11, 10), false),
            (53, "zebra", "m", (9, 10), false),
        ],
    );
    assert_eq!(hunt(&w, &cat, 50), 53);
}

fn set(w: &mut World, id: u32, edit: impl FnOnce(&mut Animal)) {
    if let Some(Part::Animal(a)) = w.units.get_mut(id).map(|u| &mut u.part) {
        edit(a);
    }
}

#[test]
fn only_an_adult_or_a_baby_attacker_hunts_and_a_young_prey_counts() {
    let mut cat = cat(None);
    cat.push(unit_ty("animals", "lion", "y", 5006, "cPrey=5001"));
    cat.push(unit_ty("animals", "zebra", "y", 5001, ""));
    // A young lion hunts nothing, a young zebra is prey.
    let w = zoo(
        (0, 0),
        &[
            (50, "lion", "y", (10, 10), false),
            (51, "zebra", "m", (11, 10), false),
        ],
    );
    assert_eq!(hunt(&w, &cat, 50), 0);
    let w = zoo(
        (0, 0),
        &[
            (50, "lion", "m", (10, 10), false),
            (51, "zebra", "y", (11, 10), false),
        ],
    );
    assert_eq!(hunt(&w, &cat, 50), 51);
    // With `cBabiesAttack` 1 the young trex hunts.
    let mut babies = Catalogue::default();
    babies.push(unit_ty(
        "animals",
        "trex",
        "y",
        7001,
        "cPrey=5001\ncBabiesAttack=1",
    ));
    babies.push(unit_ty("animals", "zebra", "m", 5001, ""));
    let w = zoo(
        (0, 0),
        &[
            (50, "trex", "y", (10, 10), false),
            (51, "zebra", "m", (11, 10), false),
        ],
    );
    assert_eq!(hunt(&w, &babies, 50), 51);
    // A zebra is no prey of a zebra.
    let w = zoo(
        (0, 0),
        &[
            (50, "zebra", "m", (10, 10), false),
            (51, "zebra", "m", (11, 10), false),
        ],
    );
    assert_eq!(hunt(&w, &cat, 50), 0);
}

#[test]
fn a_crated_unit_is_no_prey_and_an_egg_only_to_an_egg_eater_of_another_species() {
    let mut cat = cat(None);
    cat.push(unit_ty("animals", "croc", "m", 5200, "cEatsEggs=1"));
    let mut w = zoo(
        (0, 0),
        &[
            (50, "lion", "m", (10, 10), false),
            (51, "zebra", "m", (11, 10), false),
            (52, "croc", "m", (9, 10), false),
        ],
    );
    set(&mut w, 51, |a| a.crated = true);
    assert_eq!(hunt(&w, &cat, 50), 0);
    // An egg of the zebra: the lion does not eat eggs, the crocodile does, a zebra's egg is not
    // a crocodile's own species, and a crocodile's egg is none to a crocodile.
    set(&mut w, 51, |a| (a.crated, a.egg) = (false, true));
    assert_eq!(hunt(&w, &cat, 50), 0);
    assert_eq!(hunt(&w, &cat, 52), 51);
    let mut w = zoo(
        (0, 0),
        &[
            (52, "croc", "m", (9, 10), false),
            (53, "croc", "m", (10, 10), false),
        ],
    );
    set(&mut w, 53, |a| a.egg = true);
    assert_eq!(hunt(&w, &cat, 52), 0);
}

#[test]
fn a_unit_in_the_building_chain_is_no_prey_unless_the_hunter_rampages() {
    let cat = cat(None);
    let mut w = zoo(
        (0, 0),
        &[
            (50, "lion", "m", (10, 10), false),
            (51, "zebra", "m", (11, 10), false),
            (52, "zebra", "m", (12, 10), false),
        ],
    );
    for task in [103, 104, 105, 106, 107] {
        w.units.get_mut(51).unwrap().goal.record.task = task;
        let chain = (104..=106).contains(&task);
        // The next unit of the search takes over; the one on the tile itself does not count.
        assert_eq!(
            hunt(&w, &cat, 50),
            if chain { 52 } else { 51 },
            "task {task}"
        );
    }
    w.units.get_mut(51).unwrap().goal.record.task = 105;
    set(&mut w, 50, |a| a.rampaging = true);
    assert_eq!(hunt(&w, &cat, 50), 51);
}
