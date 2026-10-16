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
    let w = world((4, 4), |_, entities| {
        for (i, (class, name)) in kinds.iter().enumerate() {
            entities.push(entity(class, name, i as u32 + 1, centre((1, 1)), 0));
        }
    });
    let (mut money, mut log) = (Money::default(), Vec::new());
    charge(&w, &catalogue(), &mut money, &mut log);
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
