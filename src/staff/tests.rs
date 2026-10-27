//! The staff types' keys and the counters a new staff member draws.

use super::*;
use crate::config::Config;
use crate::entity::Catalogue;
use crate::sim::{Rng, Sim};
use crate::unit::Staff;
use crate::unit::testing::{centre, entity, world, world_with};
use crate::zoo::{Clocks, StaffRecord};

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

/// The counters a keeper of the type draws at the load of a seeded run (the staff member is on the
/// map the zoo was loaded with) and keeps through its first turn, and the generator after its two
/// colour draws, before the draws that ought to give them.
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
    let mut drawn = sim.rng.clone();
    drawn.rand();
    drawn.rand();
    sim.load(&mut w, &cat);
    let counters = |w: &World| match &w.units.get(1).unwrap().part {
        Part::Staff(s) => (s.work, s.chase, s.crowd),
        _ => panic!("not staff"),
    };
    let made = counters(&w);
    sim.step_in(&mut w, &cat, 67, |_, _, _| {});
    assert_eq!(
        counters(&w),
        made,
        "drawn at the load, not at the first turn"
    );
    let Part::Staff(staff) = &w.units.get(1).unwrap().part else {
        panic!("not staff");
    };
    (staff.clone(), drawn)
}

#[test]
fn a_new_staff_member_draws_its_work_and_chase_counters_at_its_creation() {
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
fn a_staff_member_draws_nothing_at_its_first_turn() {
    let section = "[m/Characteristics/Integers]\ncWorkCheck=20\ncChaseCheck=3\ncCrowdCheck=7\n";
    for class in ["keeper", "maint", "tour"] {
        let mut cat = Catalogue::default();
        cat.push(staff_type(class, section));
        let mut w = world((4, 4), |_, entities| {
            let mut member = entity(class, "hand", 1, centre((1, 1)), 0);
            member.subtype = "m".into();
            entities.push(member);
        });
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(16, Clocks::default());
        let mut expect = sim.rng.clone();
        sim.load(&mut w, &cat);
        let counters = |w: &World| match &w.units.get(1).unwrap().part {
            Part::Staff(s) => (s.work, s.chase, s.crowd),
            _ => panic!("not staff"),
        };
        expect.rand();
        expect.rand();
        let (work, chase) = (expect.pick(20) as i32, expect.pick(3) as i32);
        let crowd = if class == "tour" {
            expect.pick(7) as i32
        } else {
            0
        };
        let made = (work, chase, crowd);
        assert!(w.units.get(1).unwrap().started, "{class}");
        assert_eq!(counters(&w), made, "{class}: drawn at the load");
        // The first turn would draw them again, from the generator the load left.
        sim.step_in(&mut w, &cat, 67, |_, _, _| {});
        assert_eq!(counters(&w), made, "{class}: kept at the first turn");
    }
}

#[test]
fn a_staff_member_draws_its_colours_before_its_counters_at_its_creation() {
    // Distinct ranges, so a draw out of place gives other numbers.
    let section =
        "[m/Characteristics/Integers]\ncWorkCheck=30011\ncChaseCheck=29989\ncCrowdCheck=29983\n";
    for kind in [Kind::Keeper, Kind::Worker, Kind::Guide] {
        let ty = staff_type("keeper", section);
        let mut rng = Rng::default();
        let mut expect = rng.clone();
        let made = create(Some(&ty), kind, &mut rng);
        // Hair and skin, then the work and chase counters, and a guide's crowd countdown last.
        expect.rand();
        expect.rand();
        let (work, chase) = (expect.pick(30011) as i32, expect.pick(29989) as i32);
        let crowd = match kind {
            Kind::Guide => expect.pick(29983) as i32,
            _ => 0,
        };
        assert_eq!((made.work, made.chase, made.crowd), (work, chase, crowd));
        assert_eq!(made.colours.len(), 2);
        assert_eq!(rng, expect, "{kind:?}: nothing else drawn");
    }
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

#[test]
fn the_pool_makes_a_task_of_each_event_of_its_channel_and_an_urgent_one_goes_first() {
    use crate::world::piece_event;
    let mut pool = pool::Pool::default();
    let at = |kind, tile| piece_event(1, kind, tile);
    let events = [
        at(10030, 5),
        at(10031, 6),
        at(10033, 7),
        at(10034, 8),
        at(10034, 9),
        at(10035, 1),
        at(10036, 2),
        at(10032, 1),
        // An event of no known number, and a tile off the map.
        at(10029, 1),
        at(10030, 10),
        at(10030, u32::MAX),
    ];
    pool.listen(events.iter(), 10);
    let seen: Vec<_> = pool
        .tasks
        .iter()
        .map(|t| (t.job, t.tile, t.urgent))
        .collect();
    assert_eq!(
        seen,
        [
            (26, 2, true),
            (16, 9, true),
            (16, 8, true),
            (9, 5, false),
            (7, 6, false),
            (16, 7, false),
            (26, 1, false),
            (11, 1, false)
        ]
    );
    // A can that refills makes a new task; none is merged. Ids are not reused.
    pool.listen([at(10030, 5)].iter(), 10);
    assert_eq!(pool.tasks.len(), 9);
    let mut ids: Vec<u32> = pool.tasks.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 9);
    // A task can be held, given back and finished.
    let id = pool.tasks[2].id;
    pool.tasks[2].worker = 4;
    pool.unlink(4);
    assert_eq!(pool.get(id).map(|t| t.worker), Some(0));
    pool.finish(id);
    assert!(pool.get(id).is_none() && pool.tasks.len() == 8);
}

#[test]
fn a_worker_starts_the_countdown_of_its_unreachable_lists_and_nobody_else_does() {
    let section = |keys: &str| format!("[m/Characteristics/Integers]\n{keys}");
    // The first update has run: it lowered the countdown by the step of 67 ms, and a countdown
    // that reached 0 is set to the interval again.
    for (keys, left) in [
        ("", 10_000 - 67),
        ("cClearInvalidListInterval=500", 500 - 67),
        ("cClearInvalidListInterval=0", 0),
        ("cClearInvalidListInterval=-5", -5),
        ("cClearInvalidListInterval=99999999999", i32::MAX - 67),
    ] {
        let (staff, _) = first_turn_of("maint", &section(keys));
        assert_eq!(staff.clear, left, "{keys}");
    }
    let (staff, _) = first_turn(&section("cClearInvalidListInterval=500"));
    assert_eq!(staff.clear, 0, "a keeper has none");
}

/// A 3 by 3 pen from (1, 1) in a 12 by 8 map with its gate on the south side at column 2 (its
/// inside tile is (2, 3)), the keeper 1, the worker 2 and the guide 3 on the grounds, a guest 4,
/// and a second pen from (6, 1) with no gate. `records` go to the three staff members.
fn pens(records: [Option<StaffRecord>; 3]) -> World {
    world_with((12, 8), |zoo| {
        let mut id = 100;
        for x0 in [1, 6] {
            for (x, y) in (x0..x0 + 3).flat_map(|x| (1..4).map(move |y| (x, y))) {
                for (d, edge) in [(0, y == 1), (2, x == x0 + 2), (4, y == 3), (6, x == x0)] {
                    if edge {
                        id += 1;
                        zoo.entities
                            .push(entity("fences", "exhibit", id, centre((x, y)), d));
                    }
                }
            }
        }
        let classes = ["keeper", "maint", "tour", "guests"];
        for (n, class) in classes.into_iter().enumerate() {
            let mut member = entity(class, "hand", 1 + n as u32, centre((10, n as i32)), 0);
            member.subtype = "m".into();
            let record = records.get(n).cloned().flatten();
            member.common.as_mut().unwrap().staff = record.map(Box::new);
            zoo.entities.push(member);
        }
        zoo.habitat_tiles = vec![(1, 1), (6, 1)];
        zoo.habitat_gates = vec![((2, 3), 4), ((-1, -1), -1)];
        let info = crate::zoo::HabitatInfo {
            name: "Pen".into(),
            stats: [0; 9],
            created: 0,
            serviced: 0,
        };
        zoo.habitat_info = vec![info.clone(), info];
    })
}

fn staff_types() -> Catalogue {
    let mut cat = Catalogue::default();
    for class in ["keeper", "maint", "tour"] {
        cat.push(staff_type(class, ""));
    }
    cat
}

fn staff_of(w: &World, id: u32) -> Staff {
    match &w.units.get(id).unwrap().part {
        Part::Staff(s) => s.clone(),
        _ => panic!("not staff"),
    }
}

fn record() -> StaffRecord {
    StaffRecord {
        work: 7,
        chase: -1,
        tiles: vec![[2, 3], [9, 9], [-1, -1], [7, 2], [2, 3]],
        crowd: Some(5),
        members: vec![4, 99, 1],
        leg: true,
        duties: vec![true, false, true, false],
        captured: 0,
        own: Vec::new(),
    }
}

#[test]
fn a_loaded_staff_member_takes_the_counters_lists_and_switches_of_its_record() {
    let cat = staff_types();
    let mut w = pens([Some(record()), Some(record()), Some(record())]);
    let pen = w.exhibits().region(3 * 12 + 2).unwrap();
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    sim.load(&mut w, &cat);
    for id in 1..=3 {
        let s = staff_of(&w, id);
        assert_eq!((s.work, s.chase), (7, -1), "{id}");
        // The tile inside a gate gives its exhibit, once; a tile off the map and the grounds none.
        let other = w.exhibits().region(2 * 12 + 7).unwrap();
        assert_eq!(s.assigned, [pen, other], "{id}");
        assert_eq!(s.off, [false, true, false, true], "{id}");
        // The group keeps the guests of the world, in the record's order.
        assert_eq!((s.members, s.leg), (vec![4], true), "{id}");
    }
    assert_eq!(staff_of(&w, 3).crowd, 5);
}

#[test]
fn the_draws_of_a_loaded_staff_member_do_not_depend_on_its_record() {
    let cat = staff_types();
    let next = |records| {
        let mut w = pens(records);
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(16, Clocks::default());
        sim.load(&mut w, &cat);
        (sim.rng.clone(), staff_of(&w, 3))
    };
    let (with, a) = next([Some(record()), Some(record()), Some(record())]);
    let (without, b) = next([None, None, None]);
    assert_eq!(with, without);
    // A record of a version below 54 has no crowd countdown: the draw stays.
    let old = StaffRecord {
        crowd: None,
        ..record()
    };
    let (_, c) = next([None, None, Some(old)]);
    assert_eq!(c.crowd, b.crowd);
    assert_ne!(a.crowd, b.crowd);
    assert_eq!((c.work, c.chase), (7, -1));
}

#[test]
fn a_saved_staff_member_writes_its_live_state_over_the_record_it_was_read_with() {
    let cat = staff_types();
    let mut w = pens([Some(record()), None, None]);
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    sim.load(&mut w, &cat);
    let (pen, other) = (
        w.exhibits().region(3 * 12 + 2).unwrap(),
        w.exhibits().region(2 * 12 + 7).unwrap(),
    );
    let saved = |w: &World, id: u32| {
        let mut zoo = w.saved(|_| true);
        w.finish_save(&mut zoo, &cat);
        let at = w.find(id).unwrap();
        *zoo.entities[at]
            .common
            .as_ref()
            .unwrap()
            .staff
            .clone()
            .unwrap()
    };
    // The assignment is written as the tile inside the gate, in the order of the list and once;
    // an exhibit that lost its gate is not written.
    let r = saved(&w, 1);
    assert_eq!(r.tiles, [[2, 3], [8, 3]]);
    w.exhibits_mut().get_mut(other).unwrap().gate = None;
    assert_eq!(saved(&w, 1).tiles, [[2, 3]]);
    assert_eq!((r.work, r.chase, r.crowd), (7, -1, Some(5)));
    assert_eq!(r.duties, [true, false, true, false]);
    assert_eq!(r.members, [4]);
    assert_ne!(pen, other);
    // The live state wins.
    if let Some(Part::Staff(s)) = w.units.get_mut(1).map(|u| &mut u.part) {
        s.assigned.clear();
        (s.work, s.chase) = (i32::MIN, i32::MAX);
        s.off = [true, false, false, true];
        s.members = vec![4, 4];
    }
    let r = saved(&w, 1);
    assert_eq!((r.tiles.len(), r.work, r.chase), (0, i32::MIN, i32::MAX));
    assert_eq!(r.duties, [false, true, true, false]);
    assert_eq!(r.members, [4, 4]);
    // A member made in play, never read, gets a full list of duties, all on.
    let r = saved(&w, 2);
    assert_eq!(r.duties, [true, true, true, false]);
    assert_eq!((r.tiles.len(), r.members.len(), r.leg), (0, 0, false));
}

#[test]
fn a_duty_kind_the_data_lacks_is_off_and_not_counted_when_saved() {
    let mut s = Staff::default();
    let saved =
        |s: &Staff, kinds: usize| s.record(StaffRecord::default(), Vec::new(), kinds).duties;
    assert_eq!(saved(&s, 3), [true, true, true, false]);
    assert_eq!(saved(&s, 4), [true; 4]);
    assert_eq!(saved(&s, 0), [false; 4]);
    assert_eq!(saved(&s, usize::MAX), [true; 4]);
    s.off[1] = true;
    assert_eq!(saved(&s, 4), [true, false, true, true]);
    assert_eq!(saved(&s, 3), [true, false, true, false]);
}
