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
    assert_eq!(k.raised(3.5, 30.0), 3.75);
    assert_eq!(k.lowered(3.5), 3.25);
    assert_eq!((k.raised(6.5, 30.0), k.lowered(1.0)), (6.5, 1.0));
    assert_eq!((k.raised(6.4, 30.0), k.lowered(1.1)), (6.5, 1.0));
    assert!(k.can_raise(6.25, 30.0) && !k.can_raise(6.5, 30.0));
    assert!(k.can_lower(1.25) && !k.can_lower(1.0));
    // The economy's ceiling comes first.
    assert_eq!(k.raised(3.5, 3.5), 3.5);
    assert!(!k.can_raise(3.5, 3.5));
    // A type with no floor still stops at 0.
    let free = kind(0.0, 6.5);
    assert_eq!(free.lowered(0.1), 0.0);
    assert!(free.can_lower(0.25) && !free.can_lower(0.0));
    // A type with no ceiling can be raised up to the economy's.
    assert!(kind(1.0, 0.0).can_raise(29.0, 30.0));
    // Odd prices and limits never panic: a negative ceiling, a price that is not a number, huge.
    for price in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, f32::MAX] {
        for max in [-1.0, 0.0, f32::MAX] {
            k.raised(price, max);
            k.can_raise(price, max);
        }
        k.lowered(price);
        k.can_lower(price);
    }
}

#[test]
fn a_price_set_in_the_panel_is_the_live_price_of_the_building() {
    let ty = ty(
        "[Characteristics/Integers]\ncCommerce = 1\n[Characteristics/Floats]\ncDefaultCost = 3.5\ncLowCost = 1\ncHighCost = 6.5\n",
    );
    let mut w = world_with((4, 4), |_| {});
    // A building that never had a guest starts at its default price and is changed from there.
    assert_eq!(
        set_price(&mut w, &ty, 7, |k, p| k.raised(p, 30.0)),
        Some(3.75)
    );
    assert_eq!(w.units.buildings[&7].price, 3.75);
    // The bound keeps the price, which is no change.
    assert_eq!(
        set_price(&mut w, &ty, 7, |k, _| k.raised(6.5, 30.0)),
        Some(6.5)
    );
    assert_eq!(set_price(&mut w, &ty, 7, |k, p| k.raised(p, 30.0)), None);
    assert_eq!(w.units.buildings[&7].price, 6.5);
}
