//! The declarations of a building type, the price thresholds and the placing of slots.

use super::*;
use crate::config::Config;
use crate::unit::Part;
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
fn an_outside_user_faces_the_building_only_for_a_set_user_facing_of_1() {
    // Missing: on; any other value than 1 (0, negative, huge): off.
    for (text, on) in [
        ("", true),
        ("0", false),
        ("-1", false),
        ("2", false),
        ("99999999999", false),
        ("1", true),
    ] {
        let file = if text.is_empty() {
            String::new()
        } else {
            format!("[Characteristics/Integers]\ncSetUserFacing={text}\n")
        };
        assert_eq!(Kind::of(&ty(&file), 3.0).faces_user, on, "{text:?}");
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
             exitpos=0\nexitpos=48\ncapacity=2\nfacing=3\n[b]\nexitonland=0\nexitonwater=1\n",
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
    // An exit may be on land and not on water unless the slot says otherwise.
    assert_eq!((a.exit_land, a.exit_water), (true, false));
    assert_eq!((k.slots[1].exit_land, k.slots[1].exit_water), (false, true));
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
    assert!(
        can_add(&world, &b, &k, Some(&live), 0, None) && has_open_slot(&world, &b, &k, None, None)
    );
    // The slot holds one; the entrance of a slot that leads off the map is no exit.
    live.users.push((1, 0, 0));
    assert!(
        !can_add(&world, &b, &k, Some(&live), 0, None) && !can_add(&world, &b, &k, None, 1, None)
    );
    b.common.as_mut().unwrap().y = -500;
    assert!(!can_add(&world, &b, &k, None, 0, None));
}

#[test]
fn a_slot_flag_is_on_only_for_the_value_one() {
    let slot = |keys: &str| {
        let text = format!("[slots]\nname=a\n[a]\ncapacity=1\n{keys}");
        let k = Kind::of(&ty(&text), 3.0);
        (k.slots[0].exit_land, k.slots[0].exit_water)
    };
    // Missing: land on, water off.
    assert_eq!(slot(""), (true, false));
    for (value, on) in [
        ("0", false),
        ("1", true),
        ("2", false),
        ("-1", false),
        ("99999999999", false),
        ("4294967297", true),
        ("yes", false),
    ] {
        let keys = format!("exitonland={value}\nexitonwater={value}\n");
        assert_eq!(slot(&keys), (on, on), "{value}");
    }
}

#[test]
fn a_user_is_put_inside_only_for_the_value_one() {
    let teleports = |keys: &str| {
        let text = format!("[Characteristics/Integers]\n{keys}");
        Kind::of(&ty(&text), 3.0).teleports
    };
    assert!(!teleports(""));
    for (value, on) in [
        ("0", false),
        ("1", true),
        ("-1", false),
        ("99999999999", false),
    ] {
        assert_eq!(
            teleports(&format!("cUserTeleportsInside={value}\n")),
            on,
            "{value}"
        );
    }
}

/// A stand at the centre of tile (1, 1), rotation 4: its entrances are on tiles (2, 1) and
/// (1, 2), its exit on (0, 1); `slot` adds keys to its slot.
fn exits(uses_exit: bool, slot: &str) -> (Kind, Entity) {
    let text = format!(
        "[Characteristics/Integers]\ncUserUsesExit={}\n[slots]\nname=a\n[a]\n\
         entrpos=64\nentrpos=0\nentrpos=0\nentrpos=64\nexitpos=-64\nexitpos=0\ncapacity=1\n{slot}",
        u8::from(uses_exit)
    );
    let k = Kind::of(&ty(&text), 3.0);
    (k, entity("building", "stand", 7, centre((1, 1)), 4))
}

/// A 4 by 4 map with a bush of type `bush` on each quarter of each tile listed.
fn blocked(tiles: &[(i32, i32)], bush: &str) -> World {
    world_with((4, 4), |zoo| {
        for &(tx, ty) in tiles {
            for (x, y) in [(16, 16), (48, 16), (16, 48), (48, 48)] {
                let at = (tx * 64 + x, ty * 64 + y);
                zoo.entities.push(entity("bush", bush, 0, at, 0));
            }
        }
    })
}

#[test]
fn a_user_leaves_by_the_first_entrance_in_the_zoo_with_a_free_or_walkable_quarter() {
    let (k, b) = exits(false, "");
    let at = |w: &World| exit_of(w, &b, &k, 0, None);
    assert_eq!(at(&blocked(&[], "bush")), Some((160, 96)));
    // All four quarters held: the next entrance; both held, no exit and no use of the slot.
    assert_eq!(at(&blocked(&[(2, 1)], "bush")), Some((96, 160)));
    let shut = blocked(&[(2, 1), (1, 2)], "bush");
    assert_eq!(at(&shut), None);
    assert!(!can_add(&shut, &b, &k, None, 0, None) && !has_open_slot(&shut, &b, &k, None, None));
    // Objects a guest walks through block nothing.
    assert_eq!(at(&blocked(&[(2, 1), (1, 2)], "grass")), Some((160, 96)));
    // An entrance on water is a way out: only an exit is tested for land and water.
    let mut wet = world_with((4, 4), |zoo| zoo.tiles[4 + 2].terrain = 1);
    wet.set_water(vec![false, true]);
    assert_eq!(at(&wet), Some((160, 96)));
    // A map whose every tile is outside the zoo wall: an entrance building with no wall around.
    let outside = world_with((4, 4), |zoo| {
        zoo.entrance = (3, 3);
        zoo.entities
            .push(entity("bush", "bush", 0, centre((3, 3)), 6));
    });
    assert!(!outside.in_zoo(outside.tile_index(2, 1).unwrap()));
    assert_eq!(at(&outside), None);
}

#[test]
fn an_exit_must_be_on_the_terrain_its_slot_allows() {
    let at = |w: &World, slot: &str| {
        let (k, b) = exits(true, slot);
        exit_of(w, &b, &k, 0, None)
    };
    let land = blocked(&[], "bush");
    let mut wet = world_with((4, 4), |zoo| zoo.tiles[4].terrain = 1);
    wet.set_water(vec![false, true]);
    assert_eq!(at(&land, ""), Some((32, 96)));
    assert_eq!(at(&land, "exitonland=0\n"), None);
    assert_eq!(at(&wet, ""), None);
    assert_eq!(at(&wet, "exitonwater=1\n"), Some((32, 96)));
    // Only the exit counts for such a type, whatever its entrances; a held exit is none.
    assert_eq!(at(&blocked(&[(0, 1)], "bush"), ""), None);
    // A value that wraps to another than 1 is off.
    assert_eq!(at(&land, "exitonland=99999999999\n"), None);
}

/// An animal of the shelter's species standing on the shelter's tile 5 (1, 1): `swims`, and
/// `large` for a footprint above 3.
fn animal(swims: bool, large: bool) -> Species {
    Species {
        animal: true,
        id: 0,
        subtype: "f".into(),
        swims,
        large,
        tile: Some((1, 1)),
    }
}

#[test]
fn an_animal_leaves_only_into_the_exhibit_of_its_shelter() {
    // The stand is on tile 5 (1, 1) and its exit on tile 4 (0, 1).
    let (k, b) = exits(true, "");
    let pen = |tiles: Vec<usize>| crate::exhibit::Part {
        tiles,
        grounds: false,
    };
    let with = |parts: Vec<crate::exhibit::Part>| {
        let mut w = blocked(&[], "bush");
        for part in parts {
            w.exhibits_mut().add(&part, String::new());
        }
        w
    };
    let walker = animal(true, false);
    // The exit on the grounds, and on another exhibit than the stand's: both refused.
    assert_eq!(
        exit_of(&with(vec![pen(vec![5])]), &b, &k, 0, Some(&walker)),
        None
    );
    let apart = with(vec![pen(vec![5]), pen(vec![4])]);
    assert_eq!(exit_of(&apart, &b, &k, 0, Some(&walker)), None);
    // The same exhibit: accepted; a guest is not held to it.
    let own = with(vec![pen(vec![4, 5])]);
    assert_eq!(exit_of(&own, &b, &k, 0, Some(&walker)), Some((32, 96)));
    assert_eq!(exit_of(&apart, &b, &k, 0, None), Some((32, 96)));
}

#[test]
fn an_animal_leaves_into_the_exhibit_it_stands_in() {
    // The stand is on tile 5 (1, 1) in the exhibit {5}, its exit on tile 4 (0, 1) in {4}.
    let (k, b) = exits(true, "");
    let mut w = blocked(&[], "bush");
    for tiles in [vec![5], vec![4]] {
        let part = crate::exhibit::Part {
            tiles,
            grounds: false,
        };
        w.exhibits_mut().add(&part, String::new());
    }
    let standing = |tile| Species {
        animal: true,
        tile: Some(tile),
        ..animal(true, false)
    };
    // An animal standing in the exit's exhibit leaves; one standing in the stand's is refused.
    assert_eq!(
        exit_of(&w, &b, &k, 0, Some(&standing((0, 1)))),
        Some((32, 96))
    );
    assert_eq!(exit_of(&w, &b, &k, 0, Some(&standing((1, 1)))), None);
}

#[test]
fn an_egg_is_no_swimmer_at_a_shelter_exit() {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "animals",
        type_name: b"duck".to_vec(),
        ..ty("[Characteristics/Integers]\ncSwims=1\n")
    });
    let duck =
        |egg| Species::of(&cat, &entity("animals", "duck", 0, centre((1, 1)), 0), egg).unwrap();
    let (k, b) = exits(true, "exitonwater=1\n");
    let mut wet = world_with((4, 4), |zoo| {
        zoo.tiles[4].terrain = 1;
        zoo.tiles[5].terrain = 1;
    });
    wet.set_water(vec![false, true]);
    let part = crate::exhibit::Part {
        tiles: vec![4, 5],
        grounds: false,
    };
    wet.exhibits_mut().add(&part, String::new());
    // The type swims; the animal leaves onto water only when it is no egg.
    assert_eq!(exit_of(&wet, &b, &k, 0, Some(&duck(true))), None);
    assert_eq!(exit_of(&wet, &b, &k, 0, Some(&duck(false))), Some((32, 96)));
}

#[test]
fn an_animal_leaves_into_another_exhibit_from_the_any_exhibit_shelter() {
    // The stand of the test above, of the type whose users are not held to their exhibit.
    let (mut k, b) = exits(true, "");
    k.name_id = SHELTER_OF_ANY_EXHIBIT;
    let walker = animal(true, false);
    let pen = |tiles: Vec<usize>| crate::exhibit::Part {
        tiles,
        grounds: false,
    };
    // The exit on the grounds, and on another exhibit than the stand's: both accepted.
    let mut grounds = blocked(&[], "bush");
    grounds.exhibits_mut().add(&pen(vec![5]), String::new());
    assert_eq!(exit_of(&grounds, &b, &k, 0, Some(&walker)), Some((32, 96)));
    let mut apart = blocked(&[], "bush");
    for part in [pen(vec![5]), pen(vec![4])] {
        apart.exhibits_mut().add(&part, String::new());
    }
    assert_eq!(exit_of(&apart, &b, &k, 0, Some(&walker)), Some((32, 96)));
}

#[test]
fn a_large_animal_leaves_only_onto_a_flat_or_one_side_raised_tile() {
    let (k, b) = exits(true, "");
    let at = |shape: u8| {
        let mut w = world_with((4, 4), |zoo| zoo.tiles[4].shape = shape);
        let part = crate::exhibit::Part {
            tiles: vec![4, 5],
            grounds: false,
        };
        w.exhibits_mut().add(&part, String::new());
        exit_of(&w, &b, &k, 0, Some(&animal(true, true)))
    };
    assert_eq!(at(0x05), Some((32, 96)));
    assert_eq!(at(0x01), None);
}

#[test]
fn an_animal_is_large_above_a_footprint_of_three() {
    let large = |footprint: [i32; 3]| {
        let mut cat = Catalogue::default();
        cat.push(EntityType {
            class: "animals",
            type_name: b"lion".to_vec(),
            footprint,
            ..ty("")
        });
        Species::of(
            &cat,
            &entity("animals", "lion", 0, centre((0, 0)), 0),
            false,
        )
        .unwrap()
        .large
    };
    assert!(!large([1, 3, 1]));
    assert!(large([2, 2, 1]));
}

#[test]
fn a_non_swimmer_does_not_leave_onto_water() {
    // The exit may be on water here, so only the animal's own rule refuses it.
    let (k, b) = exits(true, "exitonwater=1\n");
    let mut wet = world_with((4, 4), |zoo| {
        zoo.tiles[4].terrain = 1;
        zoo.tiles[5].terrain = 1;
    });
    wet.set_water(vec![false, true]);
    let part = crate::exhibit::Part {
        tiles: vec![4, 5],
        grounds: false,
    };
    wet.exhibits_mut().add(&part, String::new());
    assert_eq!(exit_of(&wet, &b, &k, 0, Some(&animal(false, false))), None);
    assert_eq!(
        exit_of(&wet, &b, &k, 0, Some(&animal(true, false))),
        Some((32, 96))
    );
    assert_eq!(exit_of(&wet, &b, &k, 0, None), Some((32, 96)));
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
        upgrade: 0,
        price: bits(5.5),
        users: Vec::new(),
        users_now: 0,
        fill,
        built,
    }
}

fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    for (name, text) in [("stand", STAND), ("can", CAN), ("compost", COMPOST)] {
        cat.push(named(name, text));
    }
    cat
}

/// A zoo with the buildings `(type, id, record)` on tiles in a row, loaded with the types.
fn loaded(list: &[(&str, u32, Option<crate::zoo::BuildingRecord>)]) -> World {
    let cat = types();
    let mut w = world_with((12, 4), |zoo| {
        for (i, &(name, id, _)) in list.iter().enumerate() {
            let at = (i as i32, 1);
            zoo.entities
                .push(entity("building", name, id, centre(at), 0));
        }
    });
    for (i, (_, _, record)) in list.iter().enumerate() {
        w.zoo.entities[i].common.as_mut().unwrap().building = record.clone();
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
    // The price is the stored one, not the type's default, and nobody is listed by `load`.
    assert_eq!((live.price, live.users.len()), (5.5, 0));
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
        upgrade: 0,
        price: f32::NAN.to_bits(),
        users: vec![(u32::MAX, u32::MAX)],
        users_now: u32::MAX,
        fill: u32::MAX,
        built: u64::MAX,
    };
    let w = loaded(&[
        ("stand", 7, Some(wild.clone())),
        ("can", 8, Some(wild.clone())),
    ]);
    assert_eq!(w.units.buildings[&7].guests, [u32::MAX; 3]);
    // A built date past 30827 is refused: the calendar's start (zoo format 7.4).
    assert_eq!(w.units.buildings[&7].built, 0);
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
fn a_built_date_past_30827_is_the_start_of_the_calendar() {
    let start = crate::timing::Calendar::new().date().millis();
    let jan_1 = |year| {
        let date = crate::timing::Date {
            year,
            month: 1,
            day: 1,
            weekday: 0,
            hour: 0,
            minute: 0,
            second: 0,
            millisecond: 0,
        };
        crate::timing::file_time(date.millis() - start)
    };
    let w = loaded(&[
        ("stand", 7, Some(record(0, jan_1(30827)))),
        ("stand", 8, Some(record(0, jan_1(30828)))),
    ]);
    assert!(w.units.buildings[&7].built > 0);
    assert_eq!(w.units.buildings[&8].built, 0);
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
    // The listed unit is removed from the world by the caller, not released.
    assert_eq!(reset_all(&mut w, 5 * crate::timing::DAY_MS as i64), [3]);
    let want = |price| Live {
        price,
        built: 5 * crate::timing::DAY_MS as i64,
        ..Live::default()
    };
    assert_eq!(w.units.buildings[&7], want(4.5));
    assert_eq!(w.units.buildings[&8], want(5.5));
    // The can is empty and its picture follows; the call it posted at load is a separate event.
    assert_eq!(w.zoo.entities[1].common.as_ref().unwrap().food, 0);
    // No building, nothing to do.
    let mut none = world_with((2, 2), |_| {});
    assert!(reset_all(&mut none, i64::MAX).is_empty());
    assert!(none.units.buildings.is_empty());
}

#[test]
fn a_loaded_building_keeps_the_price_its_record_stores_whatever_it_is() {
    let price = |bits: u32| {
        let r = crate::zoo::BuildingRecord {
            price: bits,
            ..record(0, START)
        };
        loaded(&[("stand", 7, Some(r))]).units.buildings[&7].price
    };
    assert_eq!(price(5.5f32.to_bits()), 5.5);
    // The type's default is 29.0 here: a stored price that differs is not replaced by it.
    assert_eq!(price(0), 0.0);
    assert_eq!(price((-2.25f32).to_bits()), -2.25);
    assert_eq!(price(f32::MAX.to_bits()), f32::MAX);
    assert!(price(f32::NAN.to_bits()).is_nan());
    // No record: no live entry, so the type's default price.
    let w = loaded(&[("stand", 7, None)]);
    assert_eq!(w.units.buildings.get(&7), None);
}

const SHOP: &str = "[Characteristics/Integers]\ncCommerce=1\ncCapacity=3\ncTimeInside=3\ncUserStaysOutside=0\n\
    [Satisfies]\nbuilding\nfood\n[slots]\nname=a\nname=b\n\
    [a]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=2\n\
    [b]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=2\n";
const HIDER: &str = "[Characteristics/Integers]\ncCapacity=3\ncTimeInside=3\ncHideUser=1\n\
    [Satisfies]\nbuilding\nbathroom\n[slots]\nname=a\n\
    [a]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=3\n";

/// A zoo of a shop 7 and a hiding restroom 8, loaded with `pairs` as the users of each (and the
/// entity stamps `stamps` of the units 1 to 3, who are guests, and a keeper 4); the users are
/// put back at AI tick 9.
fn seated(pairs: [Vec<(u32, u32)>; 2], stamps: [Option<u32>; 3]) -> World {
    seated_in(106, pairs, stamps)
}

/// The same for a file of version `version`.
fn seated_in(version: u32, pairs: [Vec<(u32, u32)>; 2], stamps: [Option<u32>; 3]) -> World {
    let counts = [pairs[0].len() as u32, pairs[1].len() as u32];
    seated_with(version, pairs, counts, [None; 3], stamps).0
}

/// The same with the stored counts of users of the two buildings and the task each of the three
/// guests was saved in (none: no unit block). The catalogue comes back too.
fn seated_with(
    version: u32,
    pairs: [Vec<(u32, u32)>; 2],
    counts: [u32; 2],
    tasks: [Option<i32>; 3],
    stamps: [Option<u32>; 3],
) -> (World, Catalogue) {
    let mut cat = Catalogue::default();
    for (name, text) in [("shop", SHOP), ("hider", HIDER)] {
        cat.push(named(name, text));
    }
    let mut w = world_with((12, 4), |zoo| {
        for (i, (class, name, id)) in [
            ("building", "shop", 7),
            ("building", "hider", 8),
            ("guests", "man", 1),
            ("guests", "man", 2),
            ("guests", "man", 3),
            ("keeper", "keeper", 4),
            ("animals", "lion", 5),
        ]
        .into_iter()
        .enumerate()
        {
            zoo.entities
                .push(entity(class, name, id, centre((i as i32, 1)), 0));
        }
    });
    w.zoo.version = version;
    for (i, users) in pairs.into_iter().enumerate() {
        let r = crate::zoo::BuildingRecord {
            users,
            users_now: counts[i],
            ..record(0, START)
        };
        w.zoo.entities[i].common.as_mut().unwrap().building = Some(r);
    }
    for (i, stamp) in stamps.into_iter().enumerate() {
        w.zoo.entities[2 + i].common.as_mut().unwrap().stamp = stamp;
    }
    for (i, task) in tasks.into_iter().enumerate() {
        let mut block = crate::zoo::UnitBlock::default();
        block.target.active = task.unwrap_or(-1);
        block.pending = "bpending".into();
        block.behaviour.levels[0] = crate::zoo::Level {
            name: "bshopfood0".into(),
            list_b: vec!["fPlayTime(sit,30)".into(), "fExitBuilding()".into()],
            tail: [1, 0, 1, 0],
            ..Default::default()
        };
        let c = w.zoo.entities[2 + i].common.as_mut().unwrap();
        c.unit = task.map(|_| Box::new(block));
    }
    for at in 0..2 {
        load(&mut w, &cat, at);
    }
    users::restore(&mut w, &cat, 9);
    (w, cat)
}

#[test]
fn the_users_of_a_record_are_back_in_their_slots_with_the_time_they_entered_at() {
    let w = seated(
        [vec![(1, 0), (2, 1)], vec![(3, 0)]],
        [Some(5), None, Some(7)],
    );
    // In entering order, the stamp of the guest as the entry time and 0 (none) as the load's tick.
    assert_eq!(w.units.buildings[&7].users, [(1, 0, 5), (2, 1, 9)]);
    assert_eq!(w.units.buildings[&8].users, [(3, 0, 7)]);
    // Each takes up the building as its target; the guest of the type that hides its users goes on
    // with the use (task 0x6a) and is hidden, the others leave (they have no unit block).
    for (id, building, slot, hidden) in [(1, 7, 0, false), (2, 7, 1, false), (3, 8, 0, true)] {
        let u = w.units.get(id).unwrap();
        let Part::Guest(g) = &u.part else {
            panic!("not a guest")
        };
        assert_eq!(
            (g.building, g.slot, u.hidden),
            (building, slot, hidden),
            "{id}"
        );
        let r = &u.goal.record;
        let task = if hidden {
            crate::goal::USE
        } else {
            crate::goal::EXIT_BUILDING
        };
        assert_eq!((r.task, r.target.id, r.start), (task, building, 9));
    }
}

#[test]
fn an_animal_listed_as_a_shelter_user_is_seated_like_a_guest() {
    // The lion 5 is listed as the user of the hiding restroom 8 in slot 0.
    let w = seated([Vec::new(), vec![(5, 0)]], [None; 3]);
    assert_eq!(w.units.buildings[&8].users, [(5, 0, 9)]);
    let u = w.units.get(5).unwrap();
    let Part::Animal(a) = &u.part else {
        panic!("not an animal")
    };
    assert_eq!((a.building, a.slot, u.hidden), (8, 0, true));
    let r = &u.goal.record;
    assert_eq!((r.task, r.target.id, r.start), (crate::goal::USE, 8, 9));
}

#[test]
fn a_pair_whose_unit_is_missing_or_unfit_is_dropped() {
    let pairs = vec![
        (99, 0),       // no such unit
        (4, 0),        // a keeper, not a guest
        (7, 0),        // a building
        (1, 5),        // a slot the type does not have
        (1, u32::MAX), // the same, at the extreme
        (2, 1),        // fine
        (2, 0),        // the same unit again
        (0, 0),        // id 0
    ];
    let w = seated([pairs.clone(), vec![(2, 0), (3, 0)]], [None; 3]);
    // Unit 1 was dropped for its slot, 2 is the shop's, and the restroom does not take it again.
    assert_eq!(w.units.buildings[&7].users, [(2, 1, 9)]);
    assert_eq!(w.units.buildings[&8].users, [(3, 0, 9)]);
    assert!(!w.units.get(1).unwrap().hidden);
    assert_eq!(w.units.get(1).unwrap().goal.record.task, 0);
    // No pairs at all, and a huge count of them, do not panic.
    let w = seated([Vec::new(), vec![(3, 0); 10_000]], [Some(u32::MAX); 3]);
    assert!(w.units.buildings[&7].users.is_empty());
    assert_eq!(w.units.buildings[&8].users.len(), 1);
}

#[test]
fn a_listed_user_keeps_the_task_its_record_stores() {
    use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, USE};
    // Saved leaving and entering stay so; any other task, a task that is no task and a record without
    // a unit block: the saved tasks of the use stay, the others end the use at once, and a guest
    // without a unit block leaves. The set is taken back only with a task that stays.
    let cases = [
        (Some(106), (USE, false)),
        (Some(105), (EXIT_BUILDING, false)),
        (Some(104), (ENTER_BUILDING, false)),
        (Some(100), (USE, true)),
        (Some(0), (USE, true)),
        (Some(-1), (USE, true)),
        (Some(i32::MAX), (USE, true)),
        (Some(106 + 256), (USE, true)),
        (None, (EXIT_BUILDING, false)),
    ];
    for (saved, want) in cases {
        let (w, _) = seated_with(
            106,
            [vec![(1, 0)], Vec::new()],
            [1, 0],
            [saved, None, None],
            [None; 3],
        );
        let r = &w.units.get(1).unwrap().goal.record;
        assert_eq!((r.task, r.done), want, "{saved:?}");
        assert_eq!(
            (r.target.id, r.mode),
            (7, u8::from(r.task == EXIT_BUILDING))
        );
        assert_eq!(w.units.buildings[&7].users, [(1, 0, 9)], "{saved:?}");
        let kept = matches!(saved, Some(104..=106));
        assert_eq!(w.units.get(1).unwrap().runner.running(), kept, "{saved:?}");
    }
}

#[test]
fn a_user_goes_on_with_its_saved_use_set_hidden_or_not() {
    // The shop shows its users inside: guests 1 and 2 saved in the use run the rest of the set,
    // the current call (the 30 s) again, then the exit. The restroom hides its user, which
    // keeps its set too.
    let (w, _) = seated_with(
        106,
        [vec![(1, 0), (2, 1)], vec![(3, 0)]],
        [2, 1],
        [Some(106); 3],
        [None; 3],
    );
    for id in [1, 2] {
        let u = w.units.get(id).unwrap();
        assert_eq!(u.runner.top(), Some("bshopfood0"), "{id}");
        assert_eq!(
            u.runner
                .calls_left(&crate::behaviour::NoData, &w.zoo.entities[0]),
            2
        );
        assert_eq!(
            (u.goal.record.task, u.hidden),
            (crate::goal::USE, false),
            "{id}"
        );
    }
    let u = w.units.get(3).unwrap();
    assert!(u.runner.running() && u.hidden);
    // The pending set name is kept as saved.
    assert!((1..=3).all(|id| w.units.get(id).unwrap().runner.queued() == Some("bpending")));
}

#[test]
fn a_user_saved_in_another_task_keeps_no_pending_set() {
    // The building's stop clears the pending name; only a user that keeps its task keeps it.
    for (saved, want) in [(106, Some("bpending")), (100, None), (0, None), (-1, None)] {
        let (w, _) = seated_with(
            106,
            [vec![(1, 0)], Vec::new()],
            [1, 0],
            [Some(saved), None, None],
            [None; 3],
        );
        assert_eq!(w.units.get(1).unwrap().runner.queued(), want, "{saved}");
    }
}

#[test]
fn a_dropped_pair_stays_counted_against_the_capacity() {
    // The shop takes 3 users. Unit 99 is gone, so one user is listed; the stored count decides how
    // many more the shop takes (and a count below the pairs is raised to them by the reader).
    let free = |count: u32| {
        let (w, cat) = seated_with(
            106,
            [vec![(1, 0), (99, 0)], Vec::new()],
            [count, 0],
            [None; 3],
            [None; 3],
        );
        let e = &w.zoo.entities[0];
        let kind = crate::building::kind_of(&cat, e, w.units.prices.default).unwrap();
        let live = w.units.buildings.get(&7);
        let open = crate::building::can_add(&w, e, &kind, live, 1, None);
        (live.unwrap().extra, open)
    };
    assert_eq!(free(0), (0, true));
    assert_eq!(free(2), (1, true));
    assert_eq!(free(3), (2, false));
    let (extra, open) = free(u32::MAX);
    assert_eq!((extra, open), (u32::MAX - 1, false));
}

#[test]
fn putting_the_users_back_twice_gives_the_same_state() {
    let mut cat = Catalogue::default();
    cat.push(named("shop", SHOP));
    let mut w = world_with((12, 4), |zoo| {
        zoo.entities
            .push(entity("building", "shop", 7, centre((0, 1)), 0));
        zoo.entities
            .push(entity("guests", "man", 1, centre((1, 1)), 0));
    });
    w.zoo.version = 106;
    w.zoo.entities[0].common.as_mut().unwrap().building = Some(crate::zoo::BuildingRecord {
        users: vec![(1, 0)],
        ..record(0, START)
    });
    let once = |w: &mut World| {
        load(w, &cat, 0);
        users::restore(w, &cat, 3);
        w.units.buildings[&7].clone()
    };
    let first = once(&mut w);
    assert_eq!(once(&mut w), first);
    assert_eq!(first.users, [(1, 0, 3)]);
}

#[test]
fn a_file_before_version_80_restores_no_user_of_a_type_visible_inside() {
    // The shop shows its users inside, the restroom hides them. Guests 1 and 2 are kept pairs of the
    // shop, 99 is missing: two are counted off the figures [3, 2, 9] and nobody is listed. The
    // restroom takes its user as in any version.
    for version in [38, 79] {
        let w = seated_in(
            version,
            [vec![(1, 0), (99, 0), (2, 1)], vec![(3, 0)]],
            [None; 3],
        );
        assert_eq!(w.units.buildings[&7].guests, [1, 0, 7], "{version}");
        assert!(w.units.buildings[&7].users.is_empty(), "{version}");
        assert_eq!(w.units.get(1).unwrap().goal.record.task, 0);
        assert_eq!(w.units.buildings[&8].users.len(), 1, "{version}");
    }
    // The figures never go below 0, and version 80 keeps them and the users.
    let w = seated_in(
        70,
        [vec![(1, 0), (2, 0), (3, 0), (1, 1), (2, 1)], Vec::new()],
        [None; 3],
    );
    assert_eq!(w.units.buildings[&7].guests, [0, 0, 4]);
    let w = seated_in(80, [vec![(1, 0), (2, 1)], Vec::new()], [None; 3]);
    assert_eq!(w.units.buildings[&7].guests, [3, 2, 9]);
    assert_eq!(w.units.buildings[&7].users.len(), 2);
}

#[test]
fn the_upgrade_index_is_loaded_kept_by_a_reset_start_and_saved() {
    let stored = crate::zoo::BuildingRecord {
        upgrade: 3,
        ..record(0, START)
    };
    let mut w = loaded(&[("stand", 7, Some(stored.clone())), ("stand", 8, None)]);
    assert_eq!(w.units.upgrade_index[&7], 3);
    reset_all(&mut w, 0);
    assert_eq!(w.units.upgrade_index[&7], 3, "a reset start keeps it");
    // The save takes the live index; a building without a live entry keeps the record's.
    w.units.upgrade_index.insert(7, 1);
    let mut zoo = w.saved(|_| true);
    w.finish_save(&mut zoo, &types());
    let index = |at: usize| zoo.entities[at].common.as_ref().unwrap().building.as_ref();
    assert_eq!(index(0).map(|b| b.upgrade), Some(1));
    assert_eq!(index(1).map(|b| b.upgrade), Some(0));
}

#[test]
fn the_current_upgrade_changes_the_adult_and_child_happiness_of_a_leaving_user() {
    use crate::guest::{HAPPINESS, HUNGER};
    use crate::unit::{Kind as UnitKind, Unit};
    let ty = named(
        "stand",
        "[Characteristics/Integers]\ncAdultChange=8\ncChildChange=2\ncHungerChange=-3\n",
    );
    let kind = Kind::of(&ty, 1.0);
    let cat = Catalogue::default();
    let mut w = world_with((4, 4), |zoo| {
        let mut zebra = entity("animals", "zebra", 5, centre((1, 1)), 0);
        zebra.subtype = "m".into();
        zoo.entities.push(zebra);
    });
    let up = |name, adult, child| upgrade::Upgrade {
        name,
        icon: String::new(),
        upkeep: 0,
        adult,
        child,
    };
    let leave = |w: &mut World, child: bool| {
        let mut u = Unit::new(1, UnitKind::Guest);
        if let Part::Guest(g) = &mut u.part {
            g.child = child;
        }
        leave::pay(w, &cat, 0, &mut u, 9, &kind);
        let Part::Guest(g) = &u.part else { panic!() };
        (g.pending[HAPPINESS], g.pending[HUNGER])
    };
    assert_eq!(
        (leave(&mut w, false), leave(&mut w, true)),
        ((8, -3), (2, -3))
    );
    upgrade::add(&mut w.units.upgrades, kind.name_id, up(1, 20, 30));
    upgrade::add(&mut w.units.upgrades, kind.name_id, up(2, -4, -1));
    // The building has no live entry: the first upgrade; the other changes are the type's.
    assert_eq!(
        (leave(&mut w, false), leave(&mut w, true)),
        ((20, -3), (30, -3))
    );
    assert!(upgrade::choose(&mut w.units, kind.name_id, 9, 1));
    assert_eq!(
        (leave(&mut w, false), leave(&mut w, true)),
        ((-4, -3), (-1, -3))
    );
    // An animal gets the same adult change.
    let mut a = Unit::new(5, UnitKind::Animal);
    leave::pay(&mut w, &cat, 0, &mut a, 9, &kind);
    let Part::Animal(a) = &a.part else { panic!() };
    assert_eq!((a.happiness_change, a.hunger_change), (-4, -3));
    // An index past the list is no upgrade: the type's values.
    w.units.upgrade_index.insert(9, u32::MAX);
    assert_eq!(leave(&mut w, false), (8, -3));
}
