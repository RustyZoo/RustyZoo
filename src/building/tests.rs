//! The declarations of a building type, the price thresholds and the placing of slots.

use super::*;
use crate::config::Config;
use crate::unit::testing::{centre, entity, world_with};
use std::sync::Arc;

fn ty(text: &str) -> EntityType {
    EntityType {
        class: "building",
        type_name: b"stand".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    }
}

#[test]
fn a_type_declares_its_slots_prices_items_and_needs_with_the_defaults_of_the_program() {
    let k = Kind::of(
        &ty(
            "[Characteristics/Integers]\ncCommerce=1\ncCapacity=2\ncUserStaysOutside=0\n\
             cHideUser=2\ncAdultChange=8\n[Characteristics/Floats]\ncPriceFactor=3\n\
             [Sells]\nHotDog\n[Satisfies]\nbuilding\nfood\n[slots]\nname=a\nname=b\n\
             [a]\nslotpos=-16\nslotpos=48\nentrpos=-16\nentrpos=48\nentrpos=16\nentrpos=48\n\
             exitpos=0\nexitpos=48\ncapacity=2\nfacing=3\n[b]\n",
        ),
        3.0,
    );
    assert_eq!((k.capacity, k.time_inside, k.commerce), (2, 0, true));
    // The default price is the economy's; a flag is on only for 1; the type stays outside by
    // default.
    assert_eq!(
        (k.price, k.factor, k.hide, k.outside),
        (3.0, 3.0, false, false)
    );
    assert_eq!(
        (k.change[0], k.sells, k.satisfies.len()),
        (8, vec!["hotdog".to_string()], 2)
    );
    assert_eq!(k.slots.len(), 2);
    let a = &k.slots[0];
    assert_eq!(
        (a.at, &a.entrances[..], a.exit, a.capacity, a.facing),
        ((-16, 48), &[(-16, 48), (16, 48)][..], Some((0, 48)), 2, 3)
    );
    assert_eq!((k.slots[1].capacity, k.slots[1].exit), (0, None));
    // A file without the keys: one user, outside; a type without a file: any number.
    let bare = Kind::of(&ty("[Satisfies]\nbuilding\n"), 3.0);
    assert_eq!(
        (bare.capacity, bare.outside, bare.commerce),
        (1, true, false)
    );
    let mut none = ty("");
    none.config = None;
    assert_eq!(Kind::of(&none, 3.0).capacity, i32::MAX);
}

#[test]
fn a_price_is_expensive_from_twice_the_default_and_cheap_to_half_of_it() {
    let p = Prices {
        expensive: 2.0,
        cheap: 0.5,
        default: 3.0,
        ..Prices::default()
    };
    for (price, mood) in [
        (4.0, Mood::Angry),
        (100.0, Mood::Angry),
        (3.99, Mood::Fair),
        (1.01, Mood::Fair),
        (1.0, Mood::Happy),
        (0.0, Mood::Happy),
    ] {
        assert_eq!(p.mood(price, 2.0), mood, "{price}");
    }
    // A negative or missing default makes everything expensive or cheap, and never panics.
    assert_eq!(p.mood(1.0, -1.0), Mood::Angry);
    assert_eq!(p.mood(f32::NAN, 2.0), Mood::Fair);
}

#[test]
fn a_user_needs_room_in_the_building_and_the_slot() {
    let k = Kind::of(
        &ty(
            "[Characteristics/Integers]\ncCapacity=2\n[slots]\nname=a\n[a]\nslotpos=0\n\
             slotpos=0\nentrpos=16\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1\n",
        ),
        3.0,
    );
    let world = world_with((4, 4), |_| {});
    let mut b = entity("building", "stand", 7, centre((1, 1)), 0);
    let mut live = Live::new(&k);
    assert!(can_add(&world, &b, &k, Some(&live), 0) && has_open_slot(&world, &b, &k, None));
    // The slot holds one; the entrance of a slot that leads off the map is no exit.
    live.users.push((1, 0, 0));
    assert!(!can_add(&world, &b, &k, Some(&live), 0) && !can_add(&world, &b, &k, None, 1));
    b.common.as_mut().unwrap().y = -500;
    assert!(!can_add(&world, &b, &k, None, 0));
}

#[test]
fn slot_places_and_facings_turn_with_the_building_as_the_original() {
    let mut b = entity("building", "stand", 7, (96, 96), 0);
    for (rotation, at) in [
        (0, (80, 48)),
        (2, (144, 80)),
        (4, (112, 144)),
        (6, (48, 112)),
        (1, (96, 96)),
        (7, (96, 96)),
    ] {
        b.common.as_mut().unwrap().rotation = rotation;
        assert_eq!(place(&b, (16, 48)), at, "{rotation}");
    }
    b.common.as_mut().unwrap().rotation = 4;
    assert_eq!(facing(&b, 7), 3);
}

fn kind(low: f32, high: f32) -> Kind {
    Kind {
        low,
        high,
        ..Kind::of(&ty("[Characteristics/Integers]\ncCommerce = 1\n"), 3.0)
    }
}

#[test]
fn the_price_steps_by_a_quarter_within_the_economy_and_the_type() {
    // The burger stand: 1.00 to 6.50 (guest buildings 7.2), the shipped ceiling 30.
    let k = kind(1.0, 6.5);
    let (up, down) = (
        |k: &Kind, p, max| k.pressed(p, max, true),
        |k: &Kind, p| k.pressed(p, 30.0, false),
    );
    assert_eq!(up(&k, 3.5, 30.0), 3.75);
    assert_eq!(down(&k, 3.5), 3.25);
    assert_eq!((up(&k, 6.5, 30.0), down(&k, 1.0)), (6.5, 1.0));
    assert_eq!((up(&k, 6.4, 30.0), down(&k, 1.1)), (6.5, 1.0));
    assert!(k.can_raise(6.25, 30.0) && !k.can_raise(6.5, 30.0));
    assert!(k.can_lower(1.25) && !k.can_lower(1.0));
    // The economy's ceiling comes first; at exactly the ceiling the button stays enabled and a
    // press changes nothing, above it the button is off.
    assert_eq!(up(&k, 3.5, 3.5), 3.5);
    assert!(k.can_raise(3.5, 3.5) && !k.can_raise(3.75, 3.5));
    // A type with no floor still stops at 0.
    let free = kind(0.0, 6.5);
    assert_eq!(free.lowered(0.1, 0.25), 0.0);
    assert!(free.can_lower(0.25) && !free.can_lower(0.0));
    // A type with no ceiling can be raised up to the economy's by the button; the step applies the
    // 0 literally, as the button does.
    assert!(kind(1.0, 0.0).can_raise(29.0, 30.0));
    assert_eq!(kind(1.0, 0.0).pressed(3.0, 30.0, true), 0.0);
    // Odd prices and limits never panic: a negative ceiling, a price that is not a number, huge.
    for price in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, f32::MAX] {
        for max in [-1.0, 0.0, f32::MAX] {
            k.raised(price, max, 0.25);
            k.can_raise(price, max);
            k.typed(price, max, 7.0);
        }
        k.lowered(price, 0.25);
        k.can_lower(price);
    }
}

#[test]
fn a_typed_price_is_clamped_like_a_button_press() {
    // The popcorn stand of the full game: 3.00 within 1.00 and 5.00.
    let k = kind(1.0, 5.0);
    let typed = |v: f64| k.typed(3.0, 30.0, v);
    assert_eq!([typed(9999.0), typed(0.5), typed(4.25)], [5.0, 1.0, 4.25]);
    // A number below 0, no number (0) and the same price.
    assert_eq!([typed(-3.0), typed(0.0), typed(3.0)], [1.0, 1.0, 3.0]);
    // Not numbers and huge values stay finite or clamped.
    assert_eq!(typed(f64::INFINITY), 5.0);
    assert_eq!(typed(f64::NEG_INFINITY), 1.0);
    assert_eq!(typed(f64::NAN), 3.0);
    // The economy's ceiling holds, and a type with a ceiling of 0 gives 0.
    assert_eq!(k.typed(3.0, 4.0, 4.5), 4.0);
    assert_eq!(kind(1.0, 0.0).typed(3.0, 30.0, 4.0), 0.0);
}

#[test]
fn a_price_set_in_the_panel_is_the_live_price_of_the_building() {
    let ty = ty(
        "[Characteristics/Integers]\ncCommerce = 1\n[Characteristics/Floats]\ncDefaultCost = 3.5\ncLowCost = 1\ncHighCost = 6.5\n",
    );
    let mut w = world_with((4, 4), |_| {});
    // A building that never had a guest starts at its default price and is changed from there.
    assert_eq!(
        set_price(&mut w, &ty, 7, |k, p| k.pressed(p, 30.0, true)),
        Some(3.75)
    );
    assert_eq!(w.units.buildings[&7].price, 3.75);
    // The bound keeps the price, which is no change.
    assert_eq!(
        set_price(&mut w, &ty, 7, |k, _| k.pressed(6.5, 30.0, true)),
        Some(6.5)
    );
    assert_eq!(
        set_price(&mut w, &ty, 7, |k, p| k.pressed(p, 30.0, true)),
        None
    );
    assert_eq!(w.units.buildings[&7].price, 6.5);
}

const STAND: &str = "[Characteristics/Integers]\ncCommerce=1\n[Satisfies]\nbuilding\nfood\n";
const CAN: &str = "[Removes]\ntrash\n[Satisfies]\nbuilding\ntrash\n";
const COMPOST: &str = "[Satisfies]\ncompost\n";
const START: u64 = 126_227_808_000_000_000; // 1 January 2001 as a file time

fn named(name: &str, text: &str) -> EntityType {
    EntityType {
        type_name: name.as_bytes().to_vec(),
        ..ty(text)
    }
}

fn record(fill: u32, built: u64) -> crate::zoo::BuildingRecord {
    let bits = |v: f32| v.to_bits();
    crate::zoo::BuildingRecord {
        guests: [3, 2, 9],
        income: [bits(10.5), bits(20.0), bits(30.5)],
        upkeep: Some(bits(75.0)),
        fill,
        built,
    }
}

/// A zoo with the buildings `(type, id, record)` on tiles in a row, loaded with the types.
fn loaded(list: &[(&str, u32, Option<crate::zoo::BuildingRecord>)]) -> World {
    let mut cat = Catalogue::default();
    for (name, text) in [("stand", STAND), ("can", CAN), ("compost", COMPOST)] {
        cat.push(named(name, text));
    }
    let mut w = world_with((12, 4), |zoo| {
        for (i, &(name, id, _)) in list.iter().enumerate() {
            let at = (i as i32, 1);
            zoo.entities
                .push(entity("building", name, id, centre(at), 0));
        }
    });
    for (i, &(_, _, record)) in list.iter().enumerate() {
        w.zoo.entities[i].common.as_mut().unwrap().building = record;
    }
    for at in 0..list.len() {
        load(&mut w, &cat, at);
    }
    w
}

#[test]
fn a_loaded_building_takes_the_figures_its_record_stores() {
    let day = crate::timing::DAY_MS as i64;
    let w = loaded(&[
        (
            "stand",
            7,
            Some(record(0, START + 10 * day as u64 * 10_000)),
        ),
        // A date before the calendar starts is a negative time, not 0.
        ("stand", 8, Some(record(0, START - 5 * day as u64 * 10_000))),
    ]);
    let live = &w.units.buildings[&7];
    assert_eq!(live.guests, [3, 2, 9]);
    assert_eq!(live.income, [10.5, 20.0, 30.5]);
    assert_eq!(live.upkeep, 75.0);
    assert_eq!(live.built, 10 * day);
    assert_eq!(w.units.buildings[&8].built, -5 * day);
    // The price is the type's default and nobody is inside.
    assert_eq!((live.price, live.users.len()), (29.0, 0));
    // A stand holds no litter whatever the record says.
    let w = loaded(&[("stand", 7, Some(record(20, START)))]);
    assert_eq!(w.units.buildings[&7].fill, 0);
    assert_eq!(w.zoo.entities[0].common.as_ref().unwrap().food, 0);
}

#[test]
fn a_record_before_version_68_has_no_upkeep_and_the_extremes_do_not_panic() {
    let none = crate::zoo::BuildingRecord {
        upkeep: None,
        ..record(0, START)
    };
    let w = loaded(&[("stand", 7, Some(none))]);
    assert_eq!(w.units.buildings[&7].upkeep, 0.0);
    // The largest date, and money and counts that are not numbers.
    let wild = crate::zoo::BuildingRecord {
        guests: [u32::MAX; 3],
        income: [f32::NAN.to_bits(), f32::INFINITY.to_bits(), u32::MAX],
        upkeep: Some(u32::MAX),
        fill: u32::MAX,
        built: u64::MAX,
    };
    let w = loaded(&[("stand", 7, Some(wild)), ("can", 8, Some(wild))]);
    assert_eq!(w.units.buildings[&7].guests, [u32::MAX; 3]);
    assert!(w.units.buildings[&7].built > 0);
    assert_eq!(w.units.buildings[&8].fill, u32::MAX);
    assert_eq!(loaded_events(&w).len(), 1);
    let w = loaded(&[(
        "stand",
        7,
        Some(crate::zoo::BuildingRecord { built: 0, ..wild }),
    )]);
    assert!(w.units.buildings[&7].built < 0);
}

#[test]
fn a_building_without_a_record_or_a_guest_building_or_a_known_type_keeps_no_live_state() {
    let w = loaded(&[
        ("stand", 7, None),
        ("compost", 8, Some(record(0, START))),
        ("unknown", 9, Some(record(0, START))),
    ]);
    assert!(w.units.buildings.is_empty());
}

#[test]
fn a_loaded_trash_can_keeps_its_fill_and_the_picture_follows() {
    let w = loaded(&[("can", 7, Some(record(13, START)))]);
    assert_eq!(w.units.buildings[&7].fill, 13);
    // The counter of the entity is what the picture reads (half from 13 pieces).
    assert_eq!(w.zoo.entities[0].common.as_ref().unwrap().food, 13);
    let w = loaded(&[("can", 7, Some(record(12, START)))]);
    assert_eq!(w.zoo.entities[0].common.as_ref().unwrap().food, 12);
}

#[test]
fn a_loaded_can_above_12_pieces_calls_a_worker_with_its_tile() {
    use crate::guest::litter::CAN_FILLING;
    let w = loaded(&[
        ("can", 7, Some(record(12, START))),
        ("can", 8, Some(record(13, START))),
        ("stand", 9, Some(record(0, START))),
        ("can", 10, Some(record(25, START))),
        ("can", 11, Some(record(0, START))),
    ]);
    let events: Vec<_> = (loaded_events(&w).iter())
        .map(|e| (e.sender, e.kind, e.data, e.channel))
        .collect();
    let task = crate::sim::TASK_CHANNEL;
    // Entity list order; the tile is the can's (row 1 of a map 12 tiles wide).
    assert_eq!(
        events,
        [
            (8, CAN_FILLING, 12 + 1, task),
            (10, CAN_FILLING, 12 + 3, task)
        ]
    );
}

#[test]
fn a_reset_start_zeroes_every_building_and_keeps_its_price() {
    let mut w = loaded(&[
        ("stand", 7, Some(record(0, START))),
        ("can", 8, Some(record(20, START))),
    ]);
    w.units.buildings.get_mut(&7).unwrap().price = 4.5;
    w.units.buildings.get_mut(&7).unwrap().users.push((3, 0, 9));
    reset_all(&mut w, 5 * crate::timing::DAY_MS);
    let want = |price| Live {
        price,
        built: 5 * crate::timing::DAY_MS as i64,
        ..Live::default()
    };
    assert_eq!(w.units.buildings[&7], want(4.5));
    assert_eq!(w.units.buildings[&8], want(29.0));
    // The can is empty and its picture follows; the call it posted at load is a separate event.
    assert_eq!(w.zoo.entities[1].common.as_ref().unwrap().food, 0);
    // No building, nothing to do.
    let mut none = world_with((2, 2), |_| {});
    reset_all(&mut none, u64::MAX);
    assert!(none.units.buildings.is_empty());
}
