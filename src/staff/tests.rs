//! The staff types' keys and the counters a new staff member draws.

use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::sim::{Rng, Sim};
use crate::unit::Staff;
use crate::unit::testing::{centre, entity, world};
use crate::zoo::Clocks;

fn staff_type(class: &'static str, ini: &str) -> EntityType {
    EntityType {
        class,
        type_name: b"hand".to_vec(),
        subtype: b"m".to_vec(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

#[test]
fn a_staff_type_books_in_the_wages_of_its_class_and_a_helicopter_or_an_animal_is_no_hire() {
    let ini = "[Characteristics/Integers]\ncPurchaseCost=800\ncDutiesTextID=3614\n";
    for (class, category) in [("keeper", 11), ("tour", 12), ("maint", 13)] {
        let ty = staff_type(class, ini);
        assert_eq!(
            (is_staff(&ty), wages(&ty)),
            (true, Some(category)),
            "{class}"
        );
    }
    for class in ["animals", "helicopter", "guests", "objects"] {
        let ty = staff_type(class, ini);
        assert_eq!((is_staff(&ty), wages(&ty)), (false, None), "{class}");
    }
}

#[test]
fn the_monthly_cost_is_the_purchase_cost_unless_the_file_says_otherwise() {
    let plain = staff_type(
        "keeper",
        "[m/Characteristics/Integers]\ncPurchaseCost=800\n",
    );
    assert_eq!((monthly(&plain), duties(&plain)), (800.0, 0));
    let own = "[m/Characteristics/Integers]\ncPurchaseCost=800\ncMonthlyCost=250.5\n\
               cDutiesTextID=3614\n";
    let own = staff_type("keeper", own);
    assert_eq!((monthly(&own), duties(&own)), (250.5, 3614));
    assert_eq!(monthly(&staff_type("keeper", "")), 0.0, "no key at all");
}

/// The counters a keeper of the type draws at its first turn of a seeded run, and the draws that
/// ought to give them.
fn first_turn(ini: &str) -> (Staff, Rng) {
    let mut cat = Catalogue::default();
    cat.push(staff_type("keeper", ini));
    let mut w = world((4, 4), |_, entities| {
        let mut keeper = entity("keeper", "hand", 1, centre((1, 1)), 0);
        keeper.subtype = "m".into();
        entities.push(keeper);
    });
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    let drawn = sim.rng.clone();
    sim.step_in(&mut w, &cat, 67, |_, _, _| {});
    let Part::Staff(staff) = &w.units.get(1).unwrap().part else {
        panic!("not staff");
    };
    (staff.clone(), drawn)
}

#[test]
fn a_new_staff_member_draws_its_work_and_chase_counters_at_its_first_turn() {
    let section = |keys: &str| format!("[m/Characteristics/Integers]\n{keys}");
    let (staff, mut rng) = first_turn(&section("cWorkCheck=20\ncChaseCheck=3\n"));
    assert_eq!(staff.work, rng.pick(20) as i32);
    assert_eq!(staff.chase, rng.pick(3) as i32);
    // A missing key reads 5 and 2.
    let (staff, mut rng) = first_turn("");
    assert_eq!(
        (staff.work, staff.chase),
        (rng.pick(5) as i32, rng.pick(2) as i32)
    );
    // A key of 0 gives -1 and draws nothing for it.
    let (staff, _) = first_turn(&section("cWorkCheck=0\ncChaseCheck=0\n"));
    assert_eq!((staff.work, staff.chase), (-1, -1));
    let (staff, mut rng) = first_turn(&section("cWorkCheck=0\n"));
    assert_eq!((staff.work, staff.chase), (-1, rng.pick(2) as i32));
    // A negative key reduces as the C remainder does, a huge one stays a number.
    let (staff, mut rng) = first_turn(&section("cWorkCheck=-7\ncChaseCheck=99999999999\n"));
    assert_eq!(staff.work, rng.pick(7) as i32);
    assert_eq!(staff.chase, rng.pick(i32::MAX as u32) as i32);
}
