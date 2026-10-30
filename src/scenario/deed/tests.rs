use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::unit::testing::{centre, entity, keeper_shape, world_with};
use std::sync::Arc;

fn kind(class: &'static str, name: &str, sub: &str, name_id: i32, ini: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// Bears (name id 5001, family 1, genus 10), their young (5001 too), goats (5003, family 1,
/// genus 30), sharks (5002, family 2) and a nameless fence.
fn types() -> Catalogue {
    let ints = |f: i32, g: i32| format!("[Characteristics/Integers]\ncFamily={f}\ncGenus={g}\n");
    let mut cat = Catalogue::default();
    cat.push(kind("animals", "bear", "m", 5001, &ints(1, 10)));
    cat.push(kind("animals", "bear", "y", 5001, &ints(1, 10)));
    cat.push(kind("animals", "goat", "m", 5003, &ints(1, 30)));
    cat.push(kind("animals", "shark", "m", 5002, &ints(2, 20)));
    cat.push(kind("fences", "iron", "f", 0, ""));
    cat
}

/// A bear, a young bear, a goat, a shark and a fence on a 12 by 12 map.
fn world() -> World {
    world_with((12, 12), |zoo| {
        let list = [
            ("animals", "bear", "m"),
            ("animals", "bear", "y"),
            ("animals", "goat", "m"),
            ("animals", "shark", "m"),
            ("fences", "iron", "f"),
        ];
        for (i, (class, name, sub)) in list.into_iter().enumerate() {
            let mut e = entity(class, name, i as u32 + 1, centre((8, i as i32 + 4)), 0);
            e.subtype = sub.into();
            zoo.entities.push(e);
        }
    })
}

fn left(w: &World) -> Vec<u32> {
    let ids = w.zoo.entities.iter();
    ids.filter_map(|e| e.common.as_ref().map(|c| c.id))
        .collect()
}

#[test]
fn trigger_3_donates_adds_or_sets_and_any_other_mode_does_nothing() {
    let mut m = Money::default();
    cash(&mut m, 0, 500);
    let start = m.cash;
    assert_eq!(m.total[money::DONATIONS], 500.0);
    assert_eq!(start, Money::default().cash + 500.0);
    cash(&mut m, 1, 250);
    assert_eq!((m.cash, m.total[money::DONATIONS]), (start + 250.0, 500.0));
    cash(&mut m, 2, 1000);
    assert_eq!((m.cash, m.total[money::DONATIONS]), (1000.0, 500.0));
    cash(&mut m, 2, -40);
    assert_eq!(m.cash, -40.0);
    // A huge amount stays finite; other modes (negative, past the list, missing as 0 is a donation).
    cash(&mut m, 2, i32::MAX);
    assert!(m.cash.is_finite() && m.cash > 2.0e9);
    let kept = (m.cash, m.total[money::DONATIONS]);
    for how in [-1, 3, i32::MAX, i32::MIN] {
        cash(&mut m, how, 77);
        assert_eq!((m.cash, m.total[money::DONATIONS]), kept, "mode {how}");
    }
}

#[test]
fn trigger_8_removes_by_name_id_family_genus_or_species_and_nothing_else() {
    let cat = types();
    // (by, id, ids left): name id 5001 takes both bears; the nameless id 0 reaches the fence.
    let table: [(i32, i32, &[u32]); 12] = [
        (0, 5001, &[3, 4, 5]),
        (0, 0, &[1, 2, 3, 4]),
        (1, 1, &[4, 5]),
        (2, 10, &[3, 4, 5]),
        (2, 30, &[1, 2, 4, 5]),
        (3, 5001, &[3, 4, 5]),
        (3, 5002, &[1, 2, 3, 5]),
        // A family or a species reaches animals only, so the fence stays.
        (1, 0, &[1, 2, 3, 4, 5]),
        (3, 0, &[1, 2, 3, 4, 5]),
        // No match: a negative or huge id, a kind past the list, a negative kind.
        (0, -5, &[1, 2, 3, 4, 5]),
        (1, i32::MAX, &[1, 2, 3, 4, 5]),
        (4, 5001, &[1, 2, 3, 4, 5]),
    ];
    for (by, id, want) in table {
        let mut w = world();
        let n = remove(&mut w, &cat, by, id, |_| ());
        assert_eq!(left(&w), want, "by {by} id {id}");
        assert_eq!(n, 5 - want.len());
    }
    let mut w = world();
    assert_eq!(remove(&mut w, &cat, -1, 5001, |_| ()), 0);
    assert_eq!(left(&w).len(), 5);
}

#[test]
fn trigger_8_tells_each_entity_just_before_it_goes() {
    let cat = types();
    let mut w = world();
    let mut told = Vec::new();
    remove(&mut w, &cat, 0, 5001, |id| told.push(id));
    assert_eq!(told, [1, 2]);
    let mut none = Vec::new();
    remove(&mut w, &cat, 0, 5001, |id| none.push(id));
    assert!(none.is_empty());
}

#[test]
fn trigger_8_leaves_the_undo_list_and_the_cash_alone() {
    let cat = types();
    let mut w = world();
    remove(&mut w, &cat, 1, 1, |_| ());
    assert!(!w.undoes(1) && !w.undoes(3));
}

#[test]
fn trigger_5_boxes_the_animal_three_tiles_from_the_entrance_and_marks_it() {
    let cat = types();
    let mut w = world();
    let id = boxed(&mut w, &cat, 5003, b'm', |_| Some(keeper_shape())).expect("made");
    let at = w.find(id).unwrap();
    let e = &w.zoo.entities[at];
    assert_eq!(
        (e.class.as_str(), e.type_name.as_str()),
        ("animals", "goat")
    );
    let c = e.common.as_ref().unwrap();
    // The entrance is tile (0, 0); the first tile of distance 3 on the map is (0, 3).
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), (0, 3));
    assert!(w.units.get(id).unwrap().boxed);
    // The young one of the same name id is a type of its own.
    assert!(boxed(&mut w, &cat, 5001, b'y', |_| Some(keeper_shape())).is_some());
}

#[test]
fn trigger_5_does_nothing_without_the_type_the_subtype_or_a_place() {
    let cat = types();
    let mut w = world();
    let before = left(&w);
    for (name, sub) in [
        (5003, b'f'),
        (5003, b'y'),
        (0, b'm'),
        (-1, b'm'),
        (i32::MAX, b'm'),
        // The fence has a name id of 0 but is no animal.
        (0, b'f'),
    ] {
        assert!(boxed(&mut w, &cat, name, sub, |_| Some(keeper_shape())).is_none());
    }
    assert_eq!(left(&w), before);
    // A map with no tile three or more tiles from the entrance has no place for a crate.
    let mut tiny = world_with((2, 2), |_| ());
    assert!(boxed(&mut tiny, &cat, 5003, b'm', |_| Some(keeper_shape())).is_none());
    assert!(tiny.zoo.entities.is_empty());
}

#[test]
fn trigger_5_looks_up_to_20_tiles_from_the_entrance_and_no_further() {
    let cat = types();
    // A strip of 30 tiles with a unit on each of the tiles 3 to 20: tile 21 is out of reach.
    let mut strip = world_with((30, 1), |zoo| {
        for x in 3..=20 {
            let e = entity("animals", "goat", x as u32, centre((x, 0)), 0);
            zoo.entities.push(e);
        }
    });
    assert!(boxed(&mut strip, &cat, 5003, b'm', |_| Some(keeper_shape())).is_none());
    strip.remove(20);
    assert!(boxed(&mut strip, &cat, 5003, b'm', |_| Some(keeper_shape())).is_some());
}
