//! The wages and the building upkeep of the month-end check.

use super::*;
use crate::config::Config;
use crate::money::KEEPER_WAGES;
use crate::sim::Sim;
use crate::unit::testing::{centre, entity, world};
use crate::zoo::Clocks;

fn ty(class: &'static str, name: &str, name_id: i32, ini: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

const SHOP: &str = "[Characteristics/Floats]\ncUpkeep=50.00\n[Satisfies]\nbuilding\nfood\n";

fn catalogue() -> Catalogue {
    let wage = |n: u32| format!("[Characteristics/Integers]\ncPurchaseCost={n}\n");
    let mut cat = Catalogue::default();
    cat.push(ty("keeper", "zookeeper", 0, &wage(800)));
    cat.push(ty("tour", "guide", 0, &wage(1000)));
    cat.push(ty("maint", "worker", 0, &wage(300)));
    cat.push(ty("helicopter", "heli", 0, &wage(2000)));
    cat.push(ty("animals", "zebra", 0, &wage(5000)));
    cat.push(ty("building", "shop", 0, SHOP));
    cat.push(ty("building", "restaurant", 0, &SHOP.replace("50", "500")));
    cat.push(ty(
        "building",
        "compost",
        0,
        "[Characteristics/Floats]\ncUpkeep=70\n[Satisfies]\ncompost\n",
    ));
    cat.push(ty("building", "heliport", 8031, SHOP));
    cat.push(ty("building", "bare", 0, "[Satisfies]\nbuilding\n"));
    cat.push(ty("objects", "tree", 0, SHOP));
    cat
}

fn month(kinds: &[(&'static str, &str)]) -> (Money, Vec<String>) {
    let mut w = world((4, 4), |_, entities| {
        for (i, (class, name)) in kinds.iter().enumerate() {
            entities.push(entity(class, name, i as u32 + 1, centre((1, 1)), 0));
        }
    });
    let (mut money, mut log) = (Money::default(), Vec::new());
    charge(&mut w, &catalogue(), &mut money, &mut log);
    (money, log)
}

#[test]
fn every_staff_member_is_paid_in_the_category_of_its_class_in_list_order() {
    let staff = [
        ("keeper", "zookeeper"),
        ("keeper", "zookeeper"),
        ("tour", "guide"),
        ("maint", "worker"),
        ("helicopter", "heli"),
    ];
    let (money, log) = month(&staff);
    let booked = |c: usize| money.monthly[c][0];
    assert_eq!(
        (booked(KEEPER_WAGES), booked(12), booked(13)),
        (3600.0, 1000.0, 300.0)
    );
    assert_eq!(money.cash, -4900.0, "taken whatever the cash");
    assert_eq!(booked(crate::money::NET), -4900.0);
    assert_eq!(
        log[..3],
        ["wages 1 800", "wages 2 800", "wages 3 1000"],
        "one booking each, in order"
    );
}

#[test]
fn only_the_guest_buildings_pay_upkeep_in_one_booking() {
    let (money, log) = month(&[
        ("building", "shop"),
        ("building", "restaurant"),
        ("building", "compost"),
        ("building", "heliport"),
        ("building", "bare"),
        ("objects", "tree"),
        ("animals", "zebra"),
        ("building", "unknown"),
    ]);
    assert_eq!(money.monthly[BUILDING_UPKEEP][0], 550.0);
    assert_eq!((money.cash, log), (-550.0, vec!["upkeep 550".to_string()]));
    let none = month(&[]).0;
    assert_eq!(
        none.cash, 0.0,
        "an empty zoo pays nothing, the booking is still made"
    );
}

#[test]
fn each_building_rolls_its_monthly_figures_and_adds_its_upkeep() {
    let mut w = world((4, 4), |_, entities| {
        entities.push(entity("building", "shop", 7, centre((1, 1)), 0));
        entities.push(entity("building", "compost", 8, centre((2, 1)), 0));
    });
    let live = |income| crate::building::Live {
        guests: [2, 5, 7],
        income: [income, 1.0, 1.0 + income],
        upkeep: 10.0,
        ..Default::default()
    };
    w.units.buildings.insert(7, live(6.0));
    w.units.buildings.insert(8, live(6.0));
    charge(&mut w, &catalogue(), &mut Money::default(), &mut Vec::new());
    let shop = &w.units.buildings[&7];
    assert_eq!(
        (shop.guests, shop.income, shop.upkeep),
        ([0, 2, 7], [0.0, 6.0, 7.0], 60.0)
    );
    // The compost is no guest building: it keeps its figures.
    assert_eq!(w.units.buildings[&8].guests[0], 2);
}

#[test]
fn a_building_that_never_had_a_guest_still_adds_its_upkeep() {
    let mut w = world((4, 4), |_, entities| {
        entities.push(entity("building", "shop", 7, centre((1, 1)), 0));
        entities.push(entity("building", "compost", 8, centre((2, 1)), 0));
    });
    charge(&mut w, &catalogue(), &mut Money::default(), &mut Vec::new());
    charge(&mut w, &catalogue(), &mut Money::default(), &mut Vec::new());
    // The shop was started at its default price; the compost pays nothing and has no entry.
    assert_eq!(w.units.buildings[&7].upkeep, 100.0);
    assert!(!w.units.buildings.contains_key(&8));
}

#[test]
fn a_hostile_upkeep_is_no_charge() {
    let mut t = ty(
        "building",
        "x",
        0,
        "[Characteristics/Floats]\ncUpkeep=-5\n[Satisfies]\nbuilding\n",
    );
    assert_eq!(
        upkeep(&t),
        -5.0,
        "a negative upkeep is read as the file says"
    );
    t.config = None;
    assert_eq!((upkeep(&t), is_guest_building(&t)), (0.0, false));
    let huge = ty(
        "building",
        "x",
        0,
        "[Characteristics/Floats]\ncUpkeep=1e99\n",
    );
    assert_eq!(upkeep(&huge), 0.0, "an infinite number reads as 0");
}

#[test]
fn a_month_of_steps_charges_once_and_the_charge_is_in_the_state_hash() {
    let cat = catalogue();
    let run = |staff: bool| {
        let mut w = world((4, 4), |_, entities| {
            entities.push(entity("building", "shop", 1, centre((2, 2)), 0));
            if staff {
                entities.push(entity("maint", "worker", 2, centre((1, 1)), 0));
            }
        });
        let (mut sim, mut money) = (Sim::new(b""), Money::default());
        sim.set_seed(1);
        sim.start_game(16, Clocks::default());
        let mut charged = 0;
        for _ in 0..400 {
            sim.step_game(
                &crate::behaviour::NoData,
                &cat,
                &mut w,
                &mut money,
                1000,
                |_, _, _| {},
            );
            charged += sim
                .take_lines()
                .iter()
                .filter(|l| l.starts_with("upkeep"))
                .count();
        }
        assert_eq!(charged, 1, "400 s hold one month end");
        let sum = |c: usize| money.monthly[c].iter().sum::<f32>();
        (
            crate::sim::hash::state(&sim, &w, &money),
            sum(BUILDING_UPKEEP),
            sum(13),
        )
    };
    let (with, upkeep, wages) = run(true);
    assert_eq!((upkeep, wages), (50.0, 300.0));
    let (without, upkeep, wages) = run(false);
    assert_eq!((upkeep, wages), (50.0, 0.0));
    assert_ne!(with, without);
}

#[test]
fn the_month_end_shifts_the_exhibits_figures_and_the_shift_is_in_the_state_hash() {
    let cat = catalogue();
    let mut w = world((4, 4), |_, _| {});
    let part = crate::exhibit::Part {
        tiles: vec![5],
        grounds: false,
    };
    let id = w.exhibits_mut().add(&part, String::new());
    let stats = &mut w.exhibits_mut().get_mut(id).unwrap().stats;
    stats.donations = [1.0, 12.5, 20.0];
    stats.upkeep = [2.0, 185.49, 300.0];
    let (mut sim, mut money) = (Sim::new(b""), Money::default());
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    let before = crate::sim::hash::state(&sim, &w, &money);
    for _ in 0..400 {
        sim.step_game(
            &crate::behaviour::NoData,
            &cat,
            &mut w,
            &mut money,
            1000,
            |_, _, _| {},
        );
    }
    let stats = &w.exhibits().get(id).unwrap().stats;
    assert_eq!(stats.donations, [12.5, 0.0, 20.0], "one month end in 400 s");
    assert_eq!(stats.upkeep, [185.49, 0.0, 300.0]);
    let after = crate::sim::hash::state(&sim, &w, &money);
    w.exhibits_mut().get_mut(id).unwrap().stats.donations[0] = 7.0;
    assert_ne!(after, crate::sim::hash::state(&sim, &w, &money));
    assert_ne!(before, after);
}

#[test]
fn a_replay_of_a_test_run_has_its_state_hash() {
    let cat = catalogue();
    let run = |replay: Option<Vec<u32>>| {
        let mut w = world((4, 4), |_, _| {});
        let (mut sim, mut money) = (Sim::new(b""), Money::default());
        match replay {
            Some(seeds) => sim.set_seeds(seeds),
            None => sim.set_seed(1),
        }
        sim.start_game(16, Clocks::default());
        let mut seeds = Vec::new();
        for _ in 0..400 {
            sim.step_game(
                &crate::behaviour::NoData,
                &cat,
                &mut w,
                &mut money,
                1000,
                |_, _, _| {},
            );
            let lines = sim.take_lines();
            seeds.extend(
                lines
                    .iter()
                    .filter_map(|l| l.strip_prefix("seed ")?.parse::<u32>().ok()),
            );
        }
        (crate::sim::hash::state(&sim, &w, &money), seeds)
    };
    let (hash, seeds) = run(None);
    assert_eq!(seeds, [1, 2, 3], "the start, the map and one month end");
    assert_eq!(run(Some(seeds)).0, hash);
}
