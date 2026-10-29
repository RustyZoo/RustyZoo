//! The purity of a tank's water over time and its filter (tank staff 7), and the points of its
//! ladders (tank staff 3).

use super::tank::tank;
use super::water::new_tank;
use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::money::BUILDING_UPKEEP;
use crate::staff::ladder::Ladder;
use crate::timing::{DAY_MS, file_time};
use crate::zoo::{FILTER_CHECK_MS, Filter};
use std::sync::Arc;

/// The figures of a filter built at game time 0, with nothing paid.
fn fresh_dates() -> Filter {
    Filter {
        built: file_time(0),
        serviced: file_time(0),
        months: 1,
        check: FILTER_CHECK_MS,
        ..Filter::default()
    }
}

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
        ..fresh_dates()
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
        ..fresh_dates()
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

const DAY: i64 = DAY_MS as i64;

#[test]
fn each_cycle_adds_its_upkeep_to_the_current_and_the_total_figure() {
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    world.filter_step(&cat, at, 1);
    assert_eq!(record(&world, at).upkeep, [0, 50, 50]);
    world.filter_step(&cat, at, 50_000);
    assert_eq!(record(&world, at).upkeep, [0, 100, 100]);
    // A broken filter, or one with no water to clean, charges and counts nothing.
    let (mut world, cat, at) = filtered(0, [200_000, 0], KEYS);
    world.filter_step(&cat, at, 1);
    assert_eq!(record(&world, at).upkeep, [0; 3]);
    // The program's 350 for a missing key, and a negative or huge one cut to whole money.
    for (keys, paid) in [
        ("", 350),
        ("cFilterUpkeep=-5", 0),
        ("cFilterUpkeep=99999999999", u32::MAX),
    ] {
        let (mut world, cat, at) = filtered(10, [200_000, 0], keys);
        world.filter_step(&cat, at, 1);
        assert_eq!(record(&world, at).upkeep, [0, paid, paid], "{keys}");
    }
}

#[test]
fn the_periods_roll_over_from_the_build_date_at_the_check_every_three_seconds() {
    let (mut world, cat, at) = filtered(10, [i32::MAX, 0], KEYS);
    let months = |w: &World| (record(w, at).months, record(w, at).upkeep);
    // Day 30 is still the first month: the check changes nothing.
    world.units.now = 30 * DAY;
    world.filter_step(&cat, at, 3000);
    assert_eq!(
        (months(&world), record(&world, at).check),
        ((1, [0, 50, 50]), 3000)
    );
    // Day 31 is the second: this period becomes the last one, and the next check is not early.
    world.units.now = 31 * DAY;
    world.filter_step(&cat, at, 2999);
    assert_eq!(months(&world), (1, [0, 50, 50]));
    world.filter_step(&cat, at, 1);
    assert_eq!(months(&world), (2, [50, 0, 50]));
    // A cycle in the new period counts there; the same month again changes nothing.
    world.filter_step(&cat, at, 50_000);
    world.units.now = 60 * DAY;
    world.filter_step(&cat, at, 3000);
    assert_eq!(months(&world), (2, [50, 50, 100]));
    world.units.now = 61 * DAY;
    world.filter_step(&cat, at, 3000);
    assert_eq!(months(&world), (3, [50, 0, 100]));
}

#[test]
fn a_build_date_in_the_future_or_extreme_times_never_panic_or_roll_the_periods_wrongly() {
    let months = |from: u64, now: i64| crate::world::filter_months_since(from, now);
    // The program's `trunc(d + 29.9) / 30`: a date 1 to 59 days ahead is 0, 60 to 89 days -1.
    assert_eq!(
        [-5, -29, -30, -31, -59, -60, -89, -90].map(|d| months(file_time(0), d * DAY)),
        [0, 0, 0, 0, 0, -1, -1, -2]
    );
    // 0 days is 0 months, 1 to 30 days the first, 31 to 60 the second.
    let days = |d: i64| months(file_time(0), d * DAY + DAY - 1);
    assert_eq!(
        [
            days(0),
            days(1),
            days(29),
            days(30),
            days(31),
            days(60),
            days(61)
        ],
        [0, 1, 1, 1, 2, 2, 3]
    );
    for (from, now) in [
        (0, i64::MAX),
        (u64::MAX, i64::MIN),
        (u64::MAX, i64::MAX),
        (0, i64::MIN),
    ] {
        months(from, now);
    }
    // A filter whose check finds a count of 0 keeps its figures.
    let (mut world, cat, at) = filtered(10, [i32::MAX, 0], KEYS);
    world.units.now = -5 * DAY;
    world.filter_step(&cat, at, 3000);
    assert_eq!(
        (record(&world, at).months, record(&world, at).upkeep),
        (1, [0, 50, 50])
    );
    world.units.now = i64::MAX;
    world.filter_step(&cat, at, 3000);
    assert_eq!(record(&world, at).upkeep, [50, 0, 50]);
}

#[test]
fn a_filter_placed_or_serviced_takes_the_date_of_the_day() {
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    let id = world.zoo.entities[at].common.as_ref().unwrap().id;
    world.units.now = 10 * DAY;
    assert!(world.service_filter(&cat, id));
    let f = record(&world, at);
    assert_eq!((f.built, f.serviced), (file_time(0), file_time(10 * DAY)));
    // Placed during play: both dates are today, the count 1 and nothing paid.
    world.units.now = 20 * DAY;
    let tile = world.tile_index(4, 3).unwrap();
    assert!(world.add_filter(&cat.types[0], tile, 6));
    let last = world.zoo.entities.len() - 1;
    let f = record(&world, last);
    assert_eq!(
        (f.built, f.serviced, f.months),
        (file_time(20 * DAY), file_time(20 * DAY), 1)
    );
    assert_eq!((f.upkeep, f.check), ([0; 3], FILTER_CHECK_MS));
}

#[test]
fn a_start_with_reset_zeroes_the_figures_and_sets_the_dates() {
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    world.filter_step(&cat, at, 1);
    world.zoo.entities[at]
        .common
        .as_mut()
        .unwrap()
        .filter
        .as_mut()
        .unwrap()
        .months = 7;
    world.reset_filters(3 * DAY);
    let f = record(&world, at);
    assert_eq!(
        (f.upkeep, f.months, f.built, f.serviced, f.health),
        ([0; 3], 1, file_time(3 * DAY), file_time(3 * DAY), 10)
    );
}

fn looks(world: &World) -> Vec<(bool, bool)> {
    (world.filter_looks().iter())
        .map(|l| (l.off, l.bubbles.is_some()))
        .collect()
}

#[test]
fn a_cycle_shows_bubbles_for_3000_ms_and_a_filter_without_water_is_off_with_none() {
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    let id = world.zoo.entities[at].common.as_ref().unwrap().id;
    // Placed, it cycles at its first update: the bubbles come with the cycle.
    assert!(looks(&world).is_empty());
    world.filter_step(&cat, at, 67);
    assert_eq!(looks(&world), [(false, true)]);
    assert_eq!(world.filter_looks()[0].id, id);
    assert_eq!(
        world.filter_looks()[0].bubbles,
        Some(0),
        "begun with the cycle"
    );
    world.filter_step(&cat, at, 2_999);
    assert_eq!(looks(&world), [(false, true)]);
    assert_eq!(world.filter_looks()[0].bubbles, Some(2_999));
    world.filter_step(&cat, at, 1);
    assert!(looks(&world).is_empty(), "3000 ms after the cycle");
    // The next cycle is 50 s after the first and brings them again; a step of 0 changes nothing.
    world.filter_step(&cat, at, 0);
    world.filter_step(&cat, at, 46_999);
    assert!(looks(&world).is_empty());
    world.filter_step(&cat, at, 1);
    assert_eq!(looks(&world), [(false, true)]);
    // Without water it is off, and its bubbles are removed at once.
    world.set_paused(true);
    world.drain(1);
    world.filter_step(&cat, at, 1);
    assert_eq!(looks(&world), [(true, false)]);
    // The water comes back: on again, no bubbles until its next cycle.
    world.fill(1);
    (0..4).for_each(|_| _ = world.step_water());
    world.filter_step(&cat, at, 1);
    assert!(looks(&world).is_empty());
    // A removed filter keeps no look.
    world.set_paused(false);
    world.filter_step(&cat, at, 50_000);
    assert_eq!(looks(&world), [(false, true)]);
    assert!(world.remove(id));
    assert!(looks(&world).is_empty());
}

#[test]
fn a_broken_filter_makes_no_bubbles_and_odd_steps_do_not_panic() {
    let (mut world, cat, at) = filtered(0, [200_000, 0], KEYS);
    world.filter_step(&cat, at, 67);
    assert!(looks(&world).is_empty());
    let (mut world, cat, at) = filtered(10, [200_000, 0], KEYS);
    world.filter_step(&cat, at, u32::MAX);
    assert_eq!(looks(&world), [(false, true)]);
    // The bubbles of that cycle last 3000 ms from its step.
    world.filter_step(&cat, at, 1);
    assert_eq!(looks(&world), [(false, true)]);
    world.filter_step(&cat, at, 3_000);
    assert!(looks(&world).is_empty());
    // A type with a huge delay still shows the bubbles of the first cycle.
    let (mut world, cat, at) = filtered(10, [200_000, 0], "cFilterDelay=99999999999");
    world.filter_step(&cat, at, 1);
    assert_eq!(looks(&world), [(false, true)]);
}

#[test]
fn a_filter_plays_its_healthy_loop_above_the_decayed_health_its_decayed_one_below_and_none_broken_or_off()
 {
    let sounds = "\n[FilterSounds]\ncHealthySound=scenery/building/h.wav\ncHealthyAtten=1700\n\
                  cDecayedSound=d.wav\ncDecayedAtten=-20";
    let heard = |health: u32, keys: &str| {
        let (world, cat, _) = filtered(health, [1, 1], keys);
        (world.filter_loops(&cat).iter())
            .map(|l| (l.file.clone(), l.base))
            .collect::<Vec<_>>()
    };
    let healthy = ("scenery/building/h.wav".to_string(), 1700);
    let decayed = ("d.wav".to_string(), -20);
    assert_eq!(heard(10, sounds), vec![healthy.clone()]);
    assert_eq!(heard(6, sounds), vec![healthy]);
    for health in [5, 1] {
        assert_eq!(heard(health, sounds), vec![decayed.clone()], "{health}");
    }
    // A broken filter is silent, and so is a switched-off one whatever its health.
    assert!(heard(0, sounds).is_empty());
    // The threshold is the type's.
    let moved = format!("cDecayedHealth=8{sounds}");
    assert_eq!(heard(8, &moved)[0].1, -20);
    assert_eq!(heard(9, &moved)[0].1, 1700);
    // No section, no file or an empty one is silence; a missing or bad attenuation is 0.
    assert!(heard(10, "").is_empty());
    assert!(heard(10, "\n[FilterSounds]\ncHealthyAtten=5").is_empty());
    assert!(heard(10, "\n[FilterSounds]\ncHealthySound=").is_empty());
    for atten in ["", "cHealthyAtten=x", "cHealthyAtten=-"] {
        let text = format!("\n[FilterSounds]\ncHealthySound=h.wav\n{atten}");
        assert_eq!(heard(10, &text), [("h.wav".to_string(), 0)], "{atten}");
    }
}

#[test]
fn a_switched_off_filter_plays_no_loop_and_plays_again_with_water() {
    let sounds = "\n[FilterSounds]\ncHealthySound=h.wav\ncDecayedSound=d.wav";
    for health in [10, 3] {
        let (mut world, cat, at) = filtered(health, [200_000, 0], sounds);
        world.filter_step(&cat, at, 67);
        assert_eq!(world.filter_loops(&cat).len(), 1, "{health} with water");
        world.set_paused(true);
        world.drain(1);
        world.filter_step(&cat, at, 1);
        assert!(world.filter_loops(&cat).is_empty(), "{health} off");
        world.fill(1);
        (0..4).for_each(|_| _ = world.step_water());
        world.filter_step(&cat, at, 1);
        assert_eq!(world.filter_loops(&cat).len(), 1, "{health} water back");
    }
}
