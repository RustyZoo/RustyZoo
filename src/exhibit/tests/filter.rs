//! The purity of a tank's water over time and its filter (tank staff 7), and the points of its
//! ladders (tank staff 3).

use super::tank::tank;
use super::water::new_tank;
use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::money::BUILDING_UPKEEP;
use crate::staff::ladder::Ladder;
use crate::zoo::Filter;
use std::sync::Arc;

/// A tank (2..3, 2..3) with its gate, full to level 4 and purity 90, and a filter on (4, 2)
/// facing west into it: its health and timers, and the keys of its type.
fn filtered(health: u32, timers: [i32; 2], keys: &str) -> (World, Catalogue, usize) {
    let mut world = new_tank("tank", &[]);
    (0..4).for_each(|_| _ = world.step_water());
    let mut e = entity("filter", 4, 2, 6);
    (e.type_name, e.subtype) = ("filter1".into(), "f".into());
    e.common.as_mut().unwrap().filter = Some(Filter {
        health,
        timers: Some(timers),
    });
    let id = world.add(e, None).unwrap();
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "filter",
        type_name: b"filter1".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(
            Config::parse(format!("[Characteristics/Integers]\n{keys}\n").as_bytes()).unwrap(),
        )),
        name_id: 6471,
        footprint: [2, 2, 1],
        height: 0,
    });
    let at = world.find(id).unwrap();
    (world, cat, at)
}

fn purity(world: &World) -> u32 {
    tank(world, 2, 2).unwrap().purity
}

fn record(world: &World, at: usize) -> Filter {
    world.zoo.entities[at]
        .common
        .as_ref()
        .unwrap()
        .filter
        .unwrap()
}

const KEYS: &str = "cFilterDelay=50\ncFilterUpkeep=50";

#[test]
fn the_purity_is_cut_to_the_water_and_the_murky_message_comes_once() {
    let mut world = new_tank("tank", &[]);
    assert_eq!(purity(&world), 90);
    world.set_purity(1, 1000);
    assert_eq!(purity(&world), 90, "initialWaterPurity is the top");
    world.set_purity(1, 5);
    assert!(world.units.take_messages().is_empty());
    world.set_purity(1, -5);
    assert_eq!(purity(&world), 0);
    let told = world.units.take_messages();
    assert_eq!(told.len(), 1);
    assert_eq!((told[0].0, told[0].2.len()), (10549, 1));
    world.set_purity(1, 0);
    world.set_purity(1, i64::MIN);
    assert!(world.units.take_messages().is_empty());
    world.set_purity(99, 5);
    assert_eq!(purity(&world), 0, "no such tank");
}

#[test]
fn the_water_loses_a_point_each_decay_time_while_it_holds_water() {
    let mut world = new_tank("tank", &[]);
    // A tank without water does not decay.
    world.decay_water(1_000_000);
    assert_eq!(purity(&world), 90);
    (0..4).for_each(|_| _ = world.step_water());
    world.decay_water(29_999);
    assert_eq!(purity(&world), 90);
    world.decay_water(1);
    assert_eq!(purity(&world), 89);
    // A long step costs one point, as the timer restarts.
    world.decay_water(u32::MAX);
    assert_eq!(purity(&world), 88);
    world.set_purity(1, 0);
    world.decay_water(60_000);
    assert_eq!(purity(&world), 0);
}

#[test]
fn the_purity_is_reset_when_the_level_comes_back_to_0() {
    let mut world = new_tank("tank", &[]);
    (0..4).for_each(|_| _ = world.step_water());
    world.set_purity(1, 40);
    // Lowering the walls keeps water in the tank: nothing is reset.
    world.set_height(1, 3);
    assert_eq!((purity(&world), tank(&world, 2, 2).unwrap().level), (40, 2));
    // A drain keeps the purity until the last step, which brings the level to 0.
    world.drain(1);
    world.step_water();
    assert_eq!(purity(&world), 40);
    world.take_events();
    world.step_water();
    assert_eq!((purity(&world), tank(&world, 2, 2).unwrap().level), (90, 0));
    assert!(world.exhibits().get(1).unwrap().rating.dirty);
    assert!(!world.take_events().is_empty());
    // A tank with no water does not decay, and a refill is not a reset.
    world.set_purity(1, 50);
    world.fill(1);
    world.step_water();
    assert_eq!(purity(&world), 50);
    // Nothing to reset when the water is clean already: nothing is told.
    world.set_purity(1, 90);
    world.drain(1);
    world.step_water();
    world.take_events();
    assert!(!world.step_water());
    assert!(world.take_events().is_empty());
}

#[test]
fn a_drain_while_paused_resets_the_purity_at_once() {
    let mut world = new_tank("tank", &[]);
    world.set_paused(true);
    world.fill(1);
    world.set_purity(1, 7);
    assert_eq!(purity(&world), 7);
    world.drain(1);
    assert_eq!((purity(&world), tank(&world, 2, 2).unwrap().level), (90, 0));
}

#[test]
fn a_filter_cleans_the_water_and_charges_its_upkeep_at_each_cycle() {
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    world.set_purity(1, 60);
    world.filter_step(&cat, at, 67);
    assert_eq!(purity(&world), 70);
    assert_eq!(world.units.spent, [(BUILDING_UPKEEP, 50.0)]);
    // The next cycle is 50 s later, not before.
    world.filter_step(&cat, at, 49_999);
    assert_eq!(purity(&world), 70);
    world.filter_step(&cat, at, 1);
    assert_eq!(purity(&world), 80);
    world.filter_step(&cat, at, 50_000);
    assert_eq!(purity(&world), 90, "cut to initialWaterPurity");
    assert_eq!(world.units.spent.len(), 3);
    let lines = world.units.take_world_lines();
    let lines: Vec<_> = lines.iter().filter(|l| l.starts_with("filter")).collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].ends_with(" 10 70"), "{lines:?}");
}

#[test]
fn a_decayed_filter_cleans_less_and_a_filter_with_no_water_does_nothing() {
    let (mut world, cat, at) = filtered(5, [200_000, 0], KEYS);
    world.set_purity(1, 60);
    world.filter_step(&cat, at, 1);
    assert_eq!(purity(&world), 65);
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    world.set_paused(true);
    world.drain(1);
    world.set_purity(1, 60);
    world.filter_step(&cat, at, 1_000_000);
    assert_eq!((purity(&world), world.units.spent.len()), (60, 0));
    // It still wears, off or on.
    assert_eq!(record(&world, at).health, 9);
}

#[test]
fn a_filter_wears_posts_its_events_and_stops_when_broken_until_it_is_serviced() {
    let (mut world, cat, at) = filtered(6, [1_000, 50_000], KEYS);
    world.set_purity(1, 60);
    assert!(world.filter_step(&cat, at, 999).0.is_empty());
    let (events, _) = world.filter_step(&cat, at, 1);
    assert_eq!(record(&world, at).health, 5);
    assert_eq!(events.len(), 1);
    let (kind, tile, channel) = (events[0].kind, events[0].data, events[0].channel);
    assert_eq!(
        (kind, tile, channel),
        (10035, 2 * 8 + 4, crate::sim::TASK_CHANNEL)
    );
    for health in (1..5).rev() {
        assert!(world.filter_step(&cat, at, 200_000).0.is_empty());
        assert_eq!(record(&world, at).health, health);
    }
    let (events, _) = world.filter_step(&cat, at, 200_000);
    assert_eq!((record(&world, at).health, events.len()), (0, 1));
    assert_eq!(events[0].kind, 10036);
    // Broken: no cycle, no more wear, however long it runs.
    let before = (purity(&world), world.units.spent.len());
    assert!(world.filter_step(&cat, at, 10_000_000).0.is_empty());
    assert_eq!((purity(&world), world.units.spent.len()), before);
    assert_eq!(record(&world, at).health, 0);
    // The service is free and gives back the health; the next update cleans at once.
    world.set_purity(1, 60);
    assert!(world.service_filter(&cat, world.zoo.entities[at].common.as_ref().unwrap().id));
    assert_eq!(record(&world, at).health, 10);
    world.filter_step(&cat, at, 1);
    assert_eq!(purity(&world), 70);
    assert!(!world.service_filter(&cat, 0));
}

/// Another filter of the tank's type on tile `(tx, ty)` facing `rotation`, as placed during play.
fn another(world: &mut World, (tx, ty): (i32, i32), rotation: u32) -> u32 {
    let mut e = entity("filter", tx, ty, rotation);
    (e.type_name, e.subtype) = ("filter1".into(), "f".into());
    e.common.as_mut().unwrap().filter = Some(Filter {
        health: 10,
        timers: Some([200_000, 0]),
    });
    world.add(e, None).unwrap()
}

#[test]
fn a_doubled_or_flooded_filter_is_removed_at_its_update_and_does_nothing_more() {
    let (mut world, cat, first) = filtered(10, [200_000, 0], KEYS);
    let first_id = world.zoo.entities[first].common.as_ref().unwrap().id;
    assert_eq!(world.exhibits().get(1).unwrap().filter, Some(first_id));
    world.set_purity(1, 60);
    // A second filter facing the same tank: the first serves it, so the second is to go, and
    // neither cleans nor charges nor wears.
    let second = another(&mut world, (4, 3), 6);
    assert_eq!(world.exhibits().get(1).unwrap().filter, Some(first_id));
    let at = world.find(second).unwrap();
    assert_eq!(
        world.filter_step(&cat, at, 300_000),
        (Vec::new(), Some(second))
    );
    assert_eq!((purity(&world), world.units.spent.len()), (60, 0));
    assert_eq!(record(&world, at).health, 10);
    assert_eq!(world.filter_step(&cat, first, 67).1, None);
    assert_eq!(purity(&world), 70);
    // Once the first is gone the second serves the tank: it is no longer doubled.
    assert!(world.remove(first_id));
    let at = world.find(second).unwrap();
    assert_eq!(world.filter_step(&cat, at, 67).1, None);
    assert_eq!(world.exhibits().get(1).unwrap().filter, Some(second));
    assert_eq!(purity(&world), 80);
    // A filter whose own tile is a tank tile is flooded, whatever it faces.
    for (tile, facing) in [((3, 3), 6), ((2, 2), 0)] {
        let wet = another(&mut world, tile, facing);
        let at = world.find(wet).unwrap();
        assert_eq!(world.filter_step(&cat, at, 67), (Vec::new(), Some(wet)));
    }
    assert_eq!(purity(&world), 80);
    let lines = world.units.take_world_lines();
    let words: Vec<_> = (lines.iter())
        .filter_map(|l| l.strip_prefix("filter "))
        .filter_map(|l| l.split(' ').nth(3))
        .collect();
    assert_eq!(words, ["doubled", "flooded", "flooded"], "{lines:?}");
}

#[test]
fn undoing_the_removal_of_a_flooded_filter_takes_back_its_sell_price_of_the_moment() {
    use crate::world::Step;
    // Cost 100: sold for 80 healthy, 40 decayed (5 or below), 0 broken.
    let keys = "cPurchaseCost=100\ncDecayedHealth=5";
    for (health, price) in [(10, 80.0), (5, 40.0), (0, 0.0)] {
        let (mut world, cat, at) = filtered(health, [200_000, 0], keys);
        let id = world.zoo.entities[at].common.as_ref().unwrap().id;
        assert!(world.remove_filter(&cat, id));
        let undone = world.undo();
        let prices: Vec<f32> = (undone.steps.iter())
            .filter_map(|s| match s {
                Step::Back(_, p) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(prices, [price], "{health}");
    }
    assert!(
        !filtered(10, [1, 1], keys)
            .0
            .remove_filter(&Catalogue::default(), 9999)
    );
}

#[test]
fn a_loaded_filter_that_is_decayed_or_broken_posts_its_event_again() {
    let kinds = |health| {
        let (world, cat, _) = filtered(health, [1, 1], KEYS);
        let events = world.loaded_filter_events(&cat);
        events.iter().map(|e| e.kind).collect::<Vec<_>>()
    };
    assert_eq!(kinds(10), Vec::<u16>::new());
    assert_eq!(kinds(6), Vec::<u16>::new());
    assert_eq!(kinds(5), [10035]);
    assert_eq!(kinds(1), [10035]);
    assert_eq!(kinds(0), [10036]);
}

#[test]
fn the_ladders_stand_on_the_gate_tiles_between_the_floor_and_the_platform() {
    let (world, ..) = filtered(10, [1, 1], KEYS);
    let l = Ladder::of(&world, 1).unwrap();
    let w = SIZE as usize;
    let (inside, outside) = (l.inside, l.outside);
    assert_eq!(world.tank_at(inside), Some(1));
    assert_eq!(world.tank_at(outside), None);
    assert_eq!(
        world.neighbour(outside, l.toward_inside as usize),
        Some(inside)
    );
    assert_eq!(
        world.neighbour(inside, l.toward_outside as usize),
        Some(outside)
    );
    // The wall is 5 steps high and the water 4 steps deep over the floor.
    assert_eq!(l.platform.2 - l.in_foot.2, 16 * 5);
    assert_eq!(l.in_surface.2 - l.in_foot.2, 16 * 4);
    assert_eq!(l.in_top.2, l.platform.2);
    assert_eq!(l.out_top.2, l.platform.2);
    assert!(l.out_foot.2 <= l.out_top.2 && l.out_foot.2 >= l.in_foot.2);
    assert_eq!(
        (l.platform.0 / 64, l.platform.1 / 64),
        ((inside % w) as i32, (inside / w) as i32)
    );
    assert_eq!(
        (l.out_foot.0 / 64, l.out_foot.1 / 64),
        ((outside % w) as i32, (outside / w) as i32)
    );
    assert_eq!((l.out_foot.0, l.out_foot.1), (l.out_top.0, l.out_top.1));
    assert_eq!(Ladder::of(&world, 99), None);
}

fn keeper(marine: bool) -> EntityType {
    let flag = if marine { "cUnderwater=1\n" } else { "" };
    EntityType {
        class: "keeper",
        type_name: b"keeper".to_vec(),
        subtype: b"m".to_vec(),
        file: None,
        config: Some(Arc::new(
            Config::parse(format!("[Characteristics/Integers]\n{flag}").as_bytes()).unwrap(),
        )),
        name_id: 9552,
        footprint: [1, 1, 1],
        height: 0,
    }
}

#[test]
fn only_a_marine_keeper_is_called_to_a_tank_and_by_water_below_the_murky_limit() {
    let (marine, land) = (keeper(true), keeper(false));
    let (mut world, cat, _) = filtered(10, [1, 1], KEYS);
    let needs = |w: &World, who| crate::staff::service::needs(w, &cat, 1, who);
    // Clean water and nobody to look after: no one is needed. A land keeper never is.
    assert!(!needs(&world, &marine) && !needs(&world, &land));
    world.set_purity(1, 60);
    assert!(!needs(&world, &marine), "the limit itself is clean enough");
    world.set_purity(1, 59);
    assert!(needs(&world, &marine));
    assert!(!needs(&world, &land));
    // A tank without water is not murky, and a marine keeper has no business on land.
    world.set_paused(true);
    world.drain(1);
    assert!(!needs(&world, &marine));
}

#[test]
fn a_filter_type_with_missing_zero_negative_or_huge_keys_still_runs() {
    // No keys: the program's values, 350 a cycle and 10 points each.
    let (mut world, cat, at) = filtered(10, [200_000, 0], "");
    world.set_purity(1, 60);
    world.filter_step(&cat, at, 1);
    assert_eq!(
        (purity(&world), world.units.spent.clone()),
        (70, vec![(BUILDING_UPKEEP, 350.0)])
    );
    // A delay and a decay time of 0 or below cycle and wear at every step; huge ones never do.
    for keys in [
        "cFilterDelay=0\ncDecayTime=0",
        "cFilterDelay=-7\ncDecayTime=-7",
    ] {
        let (mut world, cat, at) = filtered(3, [1, 1], keys);
        for _ in 0..3 {
            world.filter_step(&cat, at, 1);
        }
        assert_eq!(record(&world, at).health, 0, "{keys}");
    }
    let huge = "cFilterDelay=99999999999\ncDecayTime=99999999999\ncFilterCleanAmount=99999999999";
    let (mut world, cat, at) = filtered(3, [i32::MAX, i32::MAX], huge);
    world.set_purity(1, 60);
    world.filter_step(&cat, at, 1_000_000);
    assert_eq!((record(&world, at).health, purity(&world)), (3, 60));
    // A starting health of 0 or below services to a broken filter.
    let (mut world, cat, at) = filtered(0, [1, 1], "cStartingHealth=-4");
    let id = world.zoo.entities[at].common.as_ref().unwrap().id;
    assert!(world.service_filter(&cat, id));
    assert_eq!(record(&world, at).health, 0);
}

#[test]
fn the_sell_rate_of_a_filter_follows_its_decayed_health() {
    let rate = |keys: &str, health: u32| {
        let (_, cat, _) = filtered(10, [1, 1], keys);
        crate::world::filter_sell_rate(&cat.types[0], health)
    };
    // Missing: 5; 0, negative and huge as written; broken is always 0.
    for (keys, health, expected) in [
        ("", 6, 0.8),
        ("", 5, 0.4),
        ("cDecayedHealth=0", 1, 0.8),
        ("cDecayedHealth=-5", 1, 0.8),
        ("cDecayedHealth=99999999", 10, 0.4),
        ("cDecayedHealth=99999999", u32::MAX, 0.8),
        ("cDecayedHealth=-5", 0, 0.0),
    ] {
        assert_eq!(rate(keys, health), expected, "{keys} {health}");
    }
}
