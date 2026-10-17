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
    first_turn_of("keeper", ini)
}

fn first_turn_of(class: &'static str, ini: &str) -> (Staff, Rng) {
    let mut cat = Catalogue::default();
    cat.push(staff_type(class, ini));
    let mut w = world((4, 4), |_, entities| {
        let mut keeper = entity(class, "hand", 1, centre((1, 1)), 0);
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

#[test]
fn the_service_interval_is_an_unsigned_whole_number_and_90_without_one() {
    let read = |ini: &str| {
        let mut units = crate::unit::Units::default();
        units.set_ini(ini.as_bytes());
        units.service_interval
    };
    assert_eq!(read(""), 90);
    assert_eq!(read("[AI]\nexhibitServiceInterval=0\n"), 0);
    assert_eq!(
        read("[AI]\nEXHIBITSERVICEINTERVAL = 25\n"),
        25,
        "any case, white space"
    );
    assert_eq!(
        read("[AI]\nexhibitServiceInterval=-1\n"),
        u32::MAX,
        "compared unsigned"
    );
    assert_eq!(read("[AI]\nexhibitServiceInterval=x\n"), 90);
    read("[AI]\nexhibitServiceInterval=99999999999999999999\n");
}

#[test]
fn a_new_guide_draws_its_crowd_countdown_after_the_two_counters_and_nobody_else_does() {
    let section = |keys: &str| format!("[m/Characteristics/Integers]\n{keys}");
    let (staff, mut rng) = first_turn_of("tour", &section("cWorkCheck=20\ncCrowdCheck=5\n"));
    assert_eq!(
        (staff.work, staff.chase, staff.crowd),
        (rng.pick(20) as i32, rng.pick(2) as i32, rng.pick(5) as i32)
    );
    assert!(staff.members.is_empty() && !staff.leg);
    // A missing key reads 10, a key of 0 gives -1 without a draw.
    let (staff, mut rng) = first_turn_of("tour", "");
    assert_eq!(
        (staff.work, staff.chase, staff.crowd),
        (rng.pick(5) as i32, rng.pick(2) as i32, rng.pick(10) as i32)
    );
    let (staff, _) = first_turn_of("tour", &section("cCrowdCheck=0\n"));
    assert_eq!(staff.crowd, -1);
    assert_eq!(
        first_turn_of("maint", &section("cCrowdCheck=5\n")).0.crowd,
        0
    );
}

#[test]
fn the_keys_of_a_guides_group_read_the_programs_values_for_a_missing_key() {
    let read = |keys: &str| {
        let ty = staff_type("tour", &format!("[m/Characteristics/Integers]\n{keys}"));
        Crowd::of(Some(&ty))
    };
    let own = Crowd {
        bonus: 30,
        check: 5,
        radius: 1,
        chance: 60,
        max: 15,
    };
    let keys = "cTourGuideBonus=30\ncCrowdCheck=5\ncCrowdRadius=1\ncFollowChance=60\n\
                NOT IMPLEMENTED:\ncMaxGroupSize=15\n";
    assert_eq!(read(keys), own);
    let missing = Crowd {
        bonus: 5,
        check: 10,
        radius: 2,
        chance: 50,
        max: 12,
    };
    assert_eq!(read(""), missing);
    assert_eq!(Crowd::of(None), missing);
    // Zero, negative and huge numbers are kept as numbers.
    let odd = read("cCrowdCheck=-3\ncCrowdRadius=99999999999\ncMaxGroupSize=0\n");
    assert_eq!((odd.check, odd.radius, odd.max), (-3, i32::MAX, 0));
}
