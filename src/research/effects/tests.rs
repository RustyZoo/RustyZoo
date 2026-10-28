use super::*;
use crate::config::Config;
use crate::research::{Branch, Category, Done, Program, Tree};
use std::sync::Arc;

fn ty(class: &'static str, kind: &str, sub: &str, name_id: i32, expansion: i32) -> EntityType {
    let body = format!("[Characteristics/Integers]\ncExpansionID={expansion}\n");
    EntityType {
        class,
        type_name: kind.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(Config::parse(body.as_bytes()).unwrap())),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

const ZEBRA: [usize; 2] = [0, 1];
const REX: usize = 2;
const ZOOKEEPER: [usize; 2] = [3, 4];
const SCIENTIST: usize = 5;
const MARINE: usize = 6;
const WORKER: usize = 7;
const GUIDE: usize = 8;
const FILTER: usize = 9;
const CHOW: usize = 10;
const BONE: usize = 11;

fn cat() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.types = vec![
        ty("animals", "zebra", "m", 100, 0),
        ty("animals", "zebra", "f", 100, 0),
        ty("animals", "rex", "m", 200, 1),
        ty("keeper", "keeper", "m", 300, 0),
        ty("keeper", "keeper", "f", 300, 0),
        ty("keeper", "scientist", "m", 301, 1),
        ty("keeper", "trainer", "m", 9552, 2),
        ty("maint", "maint", "m", 400, 0),
        ty("tour", "guide", "m", 500, 0),
        ty("filter", "filter", "f", 600, 0),
        ty("food", "chow", "", 700, 0),
        ty("food", "bone", "", 701, 1),
    ];
    cat
}

fn int(over: &Overlay, cat: &Catalogue, at: usize, key: &str, base: i32) -> i32 {
    over.int(&cat.types[at], key, base)
}

fn mask(over: &Overlay, cat: &Catalogue, at: usize) -> u16 {
    over.training(&cat.types[at])
}

#[test]
fn zookeeper_training_1_raises_keeper_arrives_of_the_base_animals_and_trains_the_zookeepers() {
    let (cat, mut over) = (cat(), Overlay::default());
    over.apply(&cat, 4, 0, [0, 5, 0]);
    for at in ZEBRA {
        assert_eq!(int(&over, &cat, at, "cKeeperArrivesChange", 100), 105);
    }
    assert_eq!(int(&over, &cat, REX, "cKeeperArrivesChange", 100), 100);
    for at in ZOOKEEPER {
        assert_eq!(mask(&over, &cat, at), 1, "the first icon");
    }
    assert_eq!(
        (mask(&over, &cat, SCIENTIST), mask(&over, &cat, MARINE)),
        (0, 0)
    );
    // The other keys of the animals are not changed.
    assert_eq!(int(&over, &cat, 0, "cSickChance", 10), 10);
}

#[test]
fn a_staff_change_reaches_the_group_of_its_target_and_sets_the_bit_of_the_key_and_kind() {
    let (cat, mut over) = (cat(), Overlay::default());
    // Slow rate set to 40 for the zookeeper: both subtypes, not the scientist nor the marine one.
    over.apply(&cat, 2, 300, [0, 40, 1]);
    for at in ZOOKEEPER {
        assert_eq!(int(&over, &cat, at, "cSlowRate", 20), 40);
        assert_eq!(mask(&over, &cat, at), 2);
    }
    assert_eq!(int(&over, &cat, SCIENTIST, "cSlowRate", 20), 20);
    // The scientist's slow rate is its third icon, the marine specialist's its first.
    over.apply(&cat, 2, 301, [0, 40, 1]);
    over.apply(&cat, 2, 9552, [0, 40, 1]);
    assert_eq!(mask(&over, &cat, SCIENTIST), 4);
    assert_eq!(mask(&over, &cat, MARINE), 1);
    // Food per tile +25 %: only the scientist (expansion 1) takes it, as its second icon.
    over.apply(&cat, 2, 300, [1, 25, 2]);
    assert_eq!(int(&over, &cat, ZOOKEEPER[0], "cFoodPerTile", 100), 100);
    over.apply(&cat, 2, 301, [1, 25, 2]);
    assert_eq!(int(&over, &cat, SCIENTIST, "cFoodPerTile", 100), 125);
    assert_eq!(mask(&over, &cat, SCIENTIST), 6);
}

#[test]
fn the_worker_the_guide_and_the_filter_programs_set_their_keys_and_icons() {
    let (cat, mut over) = (cat(), Overlay::default());
    over.apply(&cat, 2, 400, [0, 1, 1]);
    assert_eq!(int(&over, &cat, WORKER, "cCleanTrashRadius", 0), 1);
    assert_eq!(mask(&over, &cat, WORKER), 1);
    over.apply(&cat, 2, 400, [1, 1, 1]);
    assert_eq!(int(&over, &cat, WORKER, "cFixFenceModifier", 0), 1);
    assert_eq!(mask(&over, &cat, WORKER), 3);
    over.apply(&cat, 2, 600, [0, 100, 0]);
    assert_eq!(int(&over, &cat, FILTER, "cDecayTime", 200), 300);
    assert_eq!(
        mask(&over, &cat, WORKER),
        7,
        "a tank filter trains the workers"
    );
    over.apply(&cat, 2, 500, [0, 40, 1]);
    assert_eq!(int(&over, &cat, GUIDE, "cTourGuideBonus", 5), 40);
    assert_eq!(mask(&over, &cat, GUIDE), 1);
}

#[test]
fn changes_add_put_or_take_a_percent_truncated_and_stack_in_order() {
    let (cat, mut over) = (cat(), Overlay::default());
    // Sick chance -5 %: 30 + (30 * -5 / 100 = -1).
    over.apply(&cat, 4, 0, [3, -5, 2]);
    assert_eq!(int(&over, &cat, 0, "cSickChance", 30), 29);
    // Then +2, then +50 %: ((29 + 2) * 50 / 100) added.
    over.apply(&cat, 4, 0, [3, 2, 0]);
    over.apply(&cat, 4, 0, [3, 50, 2]);
    assert_eq!(int(&over, &cat, 0, "cSickChance", 30), 31 + 15);
    // Put, then add: the put forgets what was before.
    over.apply(&cat, 4, 0, [5, 3, 1]);
    over.apply(&cat, 4, 0, [5, 1, 0]);
    assert_eq!(int(&over, &cat, 0, "cNumberAnimalsMax", 10), 4);
}

#[test]
fn a_food_price_is_changed_in_hundredths_and_only_for_the_foods_of_the_expansion() {
    let (cat, mut over) = (cat(), Overlay::default());
    over.apply(&cat, 5, 0, [0, -10, 2]);
    // 125 - 12 = 113 hundredths.
    assert_eq!(over.price(&cat.types[CHOW], 1.25), 1.13);
    assert_eq!(over.price(&cat.types[BONE], 1.25), 1.25);
    assert_eq!(Overlay::default().price(&cat.types[CHOW], 1.2345), 1.2345);
}

#[test]
fn the_life_counter_of_live_animals_takes_the_change_and_a_percent_only_above_0() {
    let (cat, mut over) = (cat(), Overlay::default());
    let life = over.apply(&cat, 4, 1, [4, 25, 2]);
    assert_eq!(life.len(), 1, "one type of the first expansion");
    assert_eq!(int(&over, &cat, REX, "cTimeDeath", 5001), 6251);
    assert_eq!(
        (life[0].of(100), life[0].of(0), life[0].of(-8)),
        (125, 0, -8)
    );
    let add = over.apply(&cat, 2, 100, [4, 10, 0]);
    assert_eq!((add.len(), add[0].of(0), add[0].of(-8)), (2, 10, 2));
}

#[test]
fn values_that_name_nothing_change_nothing_and_never_panic() {
    let (cat, mut over) = (cat(), Overlay::default());
    for how in [-1, 3, i32::MIN, i32::MAX] {
        assert!(over.apply(&cat, 4, 0, [4, 10, how]).is_empty());
    }
    for (effect, target, which) in [
        (2, -1, 0),
        (2, 0, 0),
        (4, 99, 0),
        (4, -5, 0),
        (4, 0, 6),
        (4, 0, -1),
        (5, 7, 0),
        (3, 0, 0),
        (2, 100, i32::MAX),
    ] {
        over.apply(&cat, effect, target, [which, 5, 0]);
    }
    over.apply(&cat, 4, i32::MAX, [0, 5, 0]);
    assert_eq!(over.changes.len(), 0);
    over.apply(&cat, 4, 0, [4, i32::MAX, 2]);
    over.apply(&cat, 4, 0, [4, i32::MIN, 2]);
    over.apply(&cat, 4, 0, [4, i32::MAX, 0]);
    int(&over, &cat, 0, "cTimeDeath", i32::MAX);
    for mark in [0, -1, 17, i32::MAX, i32::MIN] {
        over.train(&cat, 9552, mark);
    }
    assert_eq!(mask(&over, &cat, MARINE), 0);
    over.train(&cat, 9552, 16);
    over.train(&cat, 9552, 1);
    assert_eq!(mask(&over, &cat, MARINE), 0x8001);
    assert_eq!(Overlay::default().training(&cat.types[MARINE]), 0);
    // A family of 100 or more reaches the animals of that family only.
    let mut cat = cat;
    cat.types[0].config = Some(Arc::new(
        Config::parse(b"[m/Characteristics/Integers]\ncFamily=150\n").unwrap(),
    ));
    let mut over = Overlay::default();
    over.apply(&cat, 4, 150, [3, 1, 1]);
    assert_eq!(int(&over, &cat, 0, "cSickChance", 10), 1);
    assert_eq!(int(&over, &cat, 1, "cSickChance", 10), 10);
}

fn program(
    name: i32,
    cost: f32,
    progress: f32,
    target: i32,
    effect: i32,
    vals: [i32; 3],
) -> Program {
    Program {
        name,
        cost,
        progress,
        target,
        effect,
        vals,
        order: 0,
    }
}

fn tree(programs: Vec<Program>) -> Tree {
    Tree {
        branches: vec![Branch {
            name: 1,
            funding: vec![],
            level: 0,
            current: None,
            categories: vec![Category {
                name: 1,
                expansion: 0,
                tick: true,
                programs,
            }],
        }],
        ..Tree::default()
    }
}

#[test]
fn a_completed_discount_makes_the_programs_of_its_effect_cheaper_and_marks_the_staff() {
    let cat = cat();
    let mut t = tree(vec![
        program(1, 20.0, 0.0, 5, 6, [0; 3]),
        program(2, 30.0, 30.0, 5, 6, [0; 3]),
        program(3, 40.0, 0.0, 5, 3, [0; 3]),
        program(4, 10.0, 10.0, 6, 7, [10, 9552, 2]),
    ]);
    let mut over = Overlay::default();
    t.apply(&cat, &mut over);
    let costs: Vec<f32> = t.branches[0].categories[0]
        .programs
        .iter()
        .map(|p| p.cost)
        .collect();
    assert_eq!(
        costs,
        [18.0, 27.0, 40.0, 10.0],
        "a complete one too, the other effects not"
    );
    assert_eq!(mask(&over, &cat, MARINE), 2);
    // A program not complete discounts nothing.
    let mut t = tree(vec![
        program(1, 20.0, 0.0, 5, 6, [0; 3]),
        program(4, 10.0, 9.0, 6, 7, [10, 9552, 2]),
    ]);
    t.apply(&cat, &mut over);
    assert_eq!(t.branches[0].categories[0].programs[0].cost, 20.0);
    assert_eq!(
        mask(&over, &cat, MARINE),
        0,
        "the overlay starts empty at each build"
    );
}

#[test]
fn a_build_lays_the_complete_programs_on_an_empty_overlay_and_does_not_stack_across_games() {
    let cat = cat();
    let mut t = tree(vec![
        program(1, 10.0, 10.0, 0, 4, [0, 5, 0]),
        program(2, 10.0, 3.0, 0, 4, [0, 100, 0]),
    ]);
    let mut over = Overlay::default();
    for _ in 0..2 {
        t.apply(&cat, &mut over);
        assert_eq!(int(&over, &cat, 0, "cKeeperArrivesChange", 100), 105);
    }
    // A load sets the progress and builds it again: the second program counts now.
    t.branches[0].categories[0].programs[1].progress = 10.0;
    t.apply(&cat, &mut over);
    assert_eq!(int(&over, &cat, 0, "cKeeperArrivesChange", 100), 205);
}

#[test]
fn a_program_before_the_discount_in_tree_order_is_judged_against_its_full_cost() {
    let cat = cat();
    let mut avail = crate::avail::Availability::default();
    let mut t = tree(vec![
        program(1, 20.0, 19.0, 100, 0, [0; 3]),
        program(2, 10.0, 10.0, 0, 7, [10, 0, 0]),
        program(3, 20.0, 19.0, 200, 0, [0; 3]),
    ]);
    for (target, on) in t.apply(&cat, &mut Overlay::default()) {
        avail.set(&cat, target, on);
    }
    assert!(!avail.is(&cat, 0), "before: 19 of 20");
    assert!(avail.is(&cat, REX), "after: 19 of 18");
}

#[test]
fn a_step_completion_applies_through_the_tree() {
    let cat = cat();
    let mut t = tree(vec![program(4, 10.0, 10.0, 6, 7, [10, 9552, 2])]);
    let mut over = Overlay::default();
    let done = Done {
        name: 4,
        target: 6,
        effect: 7,
        vals: [10, 9552, 2],
    };
    assert!(t.complete(&cat, &mut over, &done).is_empty());
    assert_eq!(mask(&over, &cat, MARINE), 2);
}
