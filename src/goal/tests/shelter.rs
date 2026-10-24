//! An animal uses a shelter on a synthetic map: the search, the walk, the time hidden inside, the
//! changes of the building and the way out (animals 6, goals 10.1 and 11.3, guest buildings 6).

use super::*;
use crate::animal::{self, Animal, Census, Class, Env, Keys, Shelter};
use crate::building::{self, Live};
use crate::entity::EntityType;
use crate::unit::testing::world_with;
use std::sync::Arc;

/// A lion that always looks for a shelter; the tile of the walk to it needs no more.
const LION: &str = "[Characteristics/Integers]\ncBuildingUseChance=100\n";
/// A cave: two animals at a time for 3 AI ticks, hidden inside, with its changes.
const CAVE: &str = "[Characteristics/Integers]\ncCapacity=2\ncTimeInside=3\ncHideUser=1\n\
    cUserStaysOutside=0\ncAdultChange=7\ncChildChange=3\ncHungerChange=-11\ncEnergyChange=13\n\
    [Satisfies]\nbuilding\nanimalrest\n[slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\n\
    entrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=2\n";
/// A hot dog stand: no shelter.
const STAND: &str = "[Characteristics/Integers]\ncCapacity=2\n[Satisfies]\nbuilding\nfood\n\
    [slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\n\
    exitpos=48\ncapacity=2\n";

/// The species id of the lion.
const LION_ID: i32 = 5;

fn ty(class: &'static str, name: &str, subtype: &str, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: subtype.as_bytes().to_vec(),
        file: None,
        config: Some(Arc::new(
            crate::config::Config::parse(text.as_bytes()).unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn types() -> Catalogue {
    types_with(CAVE)
}

fn types_with(cave: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    for subtype in ["m", "y", "p"] {
        let mut lion = ty("animals", "lion", subtype, LION);
        lion.name_id = LION_ID;
        cat.push(lion);
    }
    cat.push(ty("building", "cave", "", cave));
    cat.push(ty("building", "stand", "", STAND));
    cat
}

/// A lion `subtype` with id 1 at tile (2, 6); a cave 8 at (5, 8), whose entrance is on (5, 9), a
/// cave 9 at (8, 2) and a stand 7 at (3, 6), which is nearer than both and no shelter.
fn zoo(subtype: &str) -> World {
    let mut w = world_with((10, 10), |zoo| {
        let mut lion = entity("animals", "lion", 1, centre((2, 6)), 0);
        lion.subtype = subtype.into();
        zoo.entities.push(lion);
        for (kind, id, at) in [
            ("stand", 7, (3, 6)),
            ("cave", 8, (5, 8)),
            ("cave", 9, (8, 2)),
        ] {
            zoo.entities
                .push(entity("building", kind, id, centre(at), 4));
        }
    });
    let u = w.units.get_mut(1).unwrap();
    u.started = true;
    if let Part::Animal(a) = &mut u.part {
        // The checks stay far from firing; the shelter check is the test's to fire.
        (a.counters, a.hunger, a.energy) = ([1000; 16], 50, 50);
        (a.life, a.baby) = (5000, 5000);
        a.counters[animal::SHELTER] = 0;
    }
    w
}

fn animal(w: &World) -> &Animal {
    match &w.units.get(1).unwrap().part {
        Part::Animal(a) => a,
        _ => unreachable!(),
    }
}

fn users(w: &World, building: u32) -> usize {
    w.units
        .buildings
        .get(&building)
        .map_or(0, |l| l.users.len())
}

/// One step of the game, with the types above; the `unit` and `animal` lines.
fn step(w: &mut World, s: &mut Sim) -> Vec<String> {
    s.step_with(&Sets::walking(), &types(), w, 67, |_, _, _| {});
    let mut log = w.units.take_lines();
    log.extend(
        w.units
            .take_animal_lines()
            .into_iter()
            .map(|l| format!("animal {l}")),
    );
    log
}

/// Steps until `stop` holds; the lines.
fn until(w: &mut World, s: &mut Sim, n: usize, stop: impl Fn(&World) -> bool) -> Vec<String> {
    let mut log = Vec::new();
    for _ in 0..n {
        if stop(w) {
            return log;
        }
        log.extend(step(w, s));
    }
    assert!(stop(w), "not reached in {n} steps: {log:?}");
    log
}

/// Runs `f` on the goal update context of unit 1 with the types above.
fn with_types<R>(w: &mut World, rng: &mut Rng, f: impl FnOnce(&mut Cx) -> R) -> R {
    with_cat(w, rng, &types(), f)
}

/// The same with the types `cat`.
fn with_cat<R>(w: &mut World, rng: &mut Rng, cat: &Catalogue, f: impl FnOnce(&mut Cx) -> R) -> R {
    let data = Sets::walking();
    let mut u = unit(w).clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at: 0,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(1).unwrap() = u;
    r
}

#[test]
fn the_check_searches_the_nearest_shelter_and_keeps_one_that_still_serves() {
    let (cat, mut w) = (types(), zoo("m"));
    let here = Some(tile(2, 6));
    // The stand is nearer but no shelter; of the caves, 8 is nearer than 9.
    assert_eq!(
        animal::shelters(&w, &cat, 0, here, None),
        Shelter {
            held: false,
            nearest: vec![8]
        }
    );
    // A cave with no room is not found.
    w.units.buildings.insert(
        8,
        Live {
            users: vec![(100, 0, 0), (101, 0, 0)],
            ..Live::default()
        },
    );
    assert_eq!(animal::shelters(&w, &cat, 0, here, None).nearest, vec![9]);
    // An animal that aims at a building that serves the need keeps it, and searches no more; one
    // that aims at a stand, or at nothing that exists, searches.
    let held = animal::shelters(&w, &cat, 9, here, None);
    assert!(held.held && held.nearest.is_empty());
    for target in [7, 99] {
        let s = animal::shelters(&w, &cat, target, here, None);
        assert!(!s.held && s.nearest == [9], "{target}");
    }
    // The check itself, through the status tick: it takes the building and says so.
    let (mut w, mut s) = (zoo("m"), sim());
    let log = until(&mut w, &mut s, 100, |w| animal(w).building != 0);
    assert_eq!(animal(&w).building, 8);
    assert!(log.contains(&"animal shelter 1 8".to_string()), "{log:?}");
}

#[test]
fn an_animal_with_a_shelter_target_walks_to_its_entrance() {
    let (mut w, mut s) = (zoo("m"), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 8;
    }
    let log = until(&mut w, &mut s, 600, |w| {
        w.units.get(1).unwrap().goal.record.task == ENTER_BUILDING
    });
    // Task 0x66 towards the shelter, its entrance tile as the target, then the entrance (0x67).
    let line = |what: &str| log.iter().position(|l| l.starts_with(what));
    let walk = "goal 1 building 0 8 5,9";
    assert!(line(walk) < line("goal 1 gotoentrance"), "{log:?}");
    assert!(line("goal 1 gotoentrance") < line("goal 1 enterbuilding"));
    assert!(log.iter().any(|l| l == "set 1 bWalk"), "{log:?}");
    assert_eq!(animal(&w).slot, 0);
    // A target that serves nothing sends it nowhere: the walk of the base goal goes on.
    let (mut w, mut s) = (zoo("m"), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 7;
    }
    let log: Vec<_> = (0..5).flat_map(|_| step(&mut w, &mut s)).collect();
    assert!(
        !log.iter().any(|l| l.starts_with("goal 1 building")),
        "{log:?}"
    );
}

#[test]
fn the_animal_is_hidden_and_listed_for_the_time_inside() {
    let (mut w, mut s) = (zoo("m"), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 8;
    }
    let at = w.find(1).unwrap();
    assert!(w.is_drawn(at));
    until(&mut w, &mut s, 400, |w| users(w, 8) == 1);
    let u = w.units.get(1).unwrap();
    assert!(u.hidden && !w.is_drawn(at));
    assert_eq!(u.goal.record.task, USE);
    // It entered at the AI tick it is listed with and stays until 3 ticks have passed.
    let entered = w.units.buildings[&8].users[0].2;
    while w.units.ai_ticks < entered + 3 {
        assert!(w.units.get(1).unwrap().hidden && users(&w, 8) == 1);
        step(&mut w, &mut s);
        if w.units.ai_ticks < entered + 3 {
            assert_eq!(w.units.get(1).unwrap().goal.record.task, USE);
        }
    }
    // The guest figures count guests only.
    assert_eq!(w.units.buildings[&8].guests, [0; 3]);
}

/// The happiness, hunger and energy of the animal with what is pending.
fn values(w: &World) -> [i32; 3] {
    let a = animal(w);
    [
        a.happiness + a.happiness_change,
        a.hunger + a.hunger_change,
        a.energy + a.energy_change,
    ]
}

#[test]
fn the_change_record_of_the_building_reaches_an_adult_a_young_one_and_the_others() {
    // The happiness change of an adult, of a young one, of the other subtype; hunger and energy
    // change for all.
    for (subtype, happiness) in [("m", 7), ("y", 3), ("p", 0)] {
        let (mut w, mut s) = (zoo(subtype), sim());
        if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
            a.building = 8;
        }
        until(&mut w, &mut s, 400, |w| users(w, 8) == 1);
        // Nothing is given while it is inside.
        let inside = values(&w);
        let mut last = inside;
        for _ in 0..1000 {
            if users(&w, 8) == 0 {
                break;
            }
            last = values(&w);
            step(&mut w, &mut s);
        }
        assert_eq!(users(&w, 8), 0, "{subtype}");
        let after = values(&w);
        let given = [0, 1, 2].map(|i| after[i] - last[i]);
        assert_eq!(given, [happiness, -11, 13], "{subtype}");
        assert_eq!(inside, last, "{subtype}: nothing before it leaves");
    }
}

#[test]
fn it_walks_out_is_shown_again_and_forgets_the_shelter() {
    let (mut w, mut s) = (zoo("m"), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 8;
    }
    let at = w.find(1).unwrap();
    until(&mut w, &mut s, 400, |w| users(w, 8) == 1);
    let log = until(&mut w, &mut s, 600, |w| {
        w.units.get(1).unwrap().goal.record.task == EXIT_BUILDING
    });
    // The time is over: the leave task starts, the animal is still listed and hidden.
    assert!(
        users(&w, 8) == 1 && w.units.get(1).unwrap().hidden,
        "{log:?}"
    );
    let log = until(&mut w, &mut s, 600, |w| users(w, 8) == 0);
    let u = w.units.get(1).unwrap();
    assert!(!u.hidden && w.is_drawn(at), "{log:?}");
    // It stands on the entrance tile, with no goal and no shelter.
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), (5, 9));
    assert_eq!((u.goal.record.task, animal(&w).building), (0, 0));
    // It goes on: another check may send it again, and it wanders in between.
    until(&mut w, &mut s, 100, |w| {
        w.units.get(1).unwrap().goal.record.task != 0
    });
}

#[test]
fn the_draws_of_the_path_follow_the_check_and_the_goal_updates() {
    // The check: a due counter draws the chance, and a shelter found the pick among the nearest;
    // a target that serves, no shelter in reach, a failed chance and a species with no chance
    // draw less, and a counter that is not due nothing.
    let k = |chance| Keys {
        building_use: chance,
        ..Keys::read(&|_, fallback| fallback, &Class::default())
    };
    let census = Census::default();
    let env = |held, nearest: &[u32]| Env {
        shelter: Shelter {
            held,
            nearest: nearest.to_vec(),
        },
        census,
        ..Env::default()
    };
    let quiet = || Animal {
        counters: [1000; 16],
        life: 5000,
        crowd_direction: -1,
        ..Animal::default()
    };
    // The numbers 41, 18467, 6334 with the seed 1: the chance is 41, the pick 18467 % 2 = 1.
    let check = |chance, due: bool, env: &Env| {
        let mut a = quiet();
        a.counters[animal::SHELTER] = if due { 0 } else { 5 };
        let mut rng = Rng::default();
        rng.seed(1);
        animal::run(&mut a, &k(chance), &Class::default(), env, &mut rng);
        let mut seen = Rng::default();
        seen.seed(1);
        let drawn = (0..8).find(|&n| {
            let mut r = seen.clone();
            (0..n).for_each(|_| _ = r.rand());
            r == rng
        });
        (drawn.unwrap(), a.building)
    };
    assert_eq!(check(100, true, &env(false, &[4, 5])), (2, 5));
    assert_eq!(check(100, true, &env(false, &[4])), (2, 4));
    assert_eq!(check(100, true, &env(false, &[])), (1, 0));
    assert_eq!(check(100, true, &env(true, &[])), (1, 0));
    assert_eq!(check(41, true, &env(false, &[4, 5])), (1, 0));
    assert_eq!(check(0, true, &env(false, &[4, 5])), (0, 0));
    assert_eq!(check(100, false, &env(false, &[4, 5])), (0, 0));
    // The goal updates: one draw for the common part of each, 0x68 adds the draw of its check
    // done, 0x69 the check done and the entrance it walks to. 0x66 and 0x6a draw nothing more.
    let draws = |cx: &mut Cx, n: usize, f: &dyn Fn(&mut Cx)| {
        let mut seen = cx.ctx.rng.clone();
        (0..n).for_each(|_| _ = seen.rand());
        f(cx);
        assert_eq!(*cx.ctx.rng, seen, "{n} draws");
    };
    let update = |cx: &mut Cx| {
        let task = cx.record().task;
        cx.animal_ladder(task);
        cx.common();
        cx.run_goal();
    };
    let (mut w, mut rng) = (zoo("m"), Rng::default());
    rng.seed(3);
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 8;
    }
    with_types(&mut w, &mut rng, |cx| {
        // Sets 0x66: the tiles are recomputed by the ladder.
        draws(cx, 1, &update);
        assert_eq!(cx.record().task, GO_BUILDING);
        cx.set_task(ENTER_BUILDING, 0, 8, None);
        draws(cx, 2, &update);
        cx.set_task(USE, 0, 8, None);
        cx.world.units.buildings.insert(8, Live::default());
        cx.world
            .units
            .buildings
            .get_mut(&8)
            .unwrap()
            .users
            .push((1, 0, 0));
        draws(cx, 1, &update);
        cx.set_task(EXIT_BUILDING, 1, 8, None);
        draws(cx, 3, &update);
    });
}

#[test]
fn a_hidden_animal_is_not_walked_off_by_its_ambient_choice() {
    let (mut w, mut s) = (zoo("m"), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 8;
    }
    let spot = |w: &World| {
        let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
        (c.x, c.y)
    };
    // The ambient table of this type walks at every choice; the checks make a choice every step.
    // The cave keeps its user long; the walk in ends within some steps of the entering, and no
    // ambient walk follows.
    let (cat, data) = (
        types_with(&CAVE.replace("cTimeInside=3", "cTimeInside=100")),
        Sets::strolling(),
    );
    let (mut inside, mut since) = (None, 0);
    for _ in 0..1500 {
        w.units.get_mut(1).unwrap().status_timer = i32::MAX / 2;
        s.step_with(&data, &cat, &mut w, 67, |_, _, _| {});
        w.units.take_lines();
        since += usize::from(users(&w, 8) == 1);
        match inside {
            None if since == 60 => inside = Some(spot(&w)),
            Some(at) => assert_eq!((users(&w, 8), spot(&w)), (1, at)),
            None => {}
        }
        if since == 120 {
            return;
        }
    }
    panic!("it never got in");
}

#[test]
fn a_hidden_animal_chooses_its_ambient_set_and_sleep_check_but_does_not_walk_off() {
    let (mut w, mut s) = (zoo("m"), sim());
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.building = 8;
    }
    let spot = |w: &World| {
        let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
        (c.x, c.y)
    };
    // The cave keeps its user long; the ambient table walks at every choice. Once inside, the
    // animal chooses its ambient set; then it is tired and its energy check is due.
    let (cat, data) = (
        types_with(&CAVE.replace("cTimeInside=3", "cTimeInside=100")),
        Sets::strolling(),
    );
    let (mut inside, mut since, mut log) = (None, 0, Vec::new());
    for _ in 0..400 {
        w.units.get_mut(1).unwrap().status_timer = i32::MAX / 2;
        if since == 80 {
            if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
                (a.energy, a.counters[animal::ENERGY]) = (100, 0);
            }
        }
        s.step_with(&data, &cat, &mut w, 67, |_, _, _| {});
        let lines = w.units.take_lines();
        let animal = w.units.take_animal_lines();
        since += usize::from(users(&w, 8) == 1);
        match inside {
            None if since == 60 => inside = Some(spot(&w)),
            Some(at) => {
                assert_eq!((users(&w, 8), spot(&w)), (1, at));
                log.extend(lines.into_iter().chain(animal));
            }
            None => {}
        }
    }
    assert!(inside.is_some(), "it never got in");
    assert!(log.contains(&"set 1 bWalk".to_string()), "no ambient set");
    assert!(log.contains(&"sleep 1".to_string()), "no sleep check");
}

#[test]
fn a_slot_that_lists_other_species_or_filters_the_subtype_is_no_shelter_for_the_animal() {
    // The lion is species 5 and subtype m. A slot that lists another species or a filter that holds
    // the subtype refuses it; its own species, an empty list and a filter of other subtypes let it in.
    let caves = [
        ("id=6\n", false),
        ("id=6\nid=7\n", false),
        ("filter=m\n", false),
        ("filter=ym\n", false),
        ("id=5\nfilter=m\n", false),
        ("id=6\nid=5\n", true),
        ("id=5\n", true),
        ("filter=y\n", true),
        ("filter=yf\n", true),
        ("", true),
    ];
    for (slot, allowed) in caves {
        let cat = types_with(&format!("{CAVE}{slot}"));
        let mut w = zoo("m");
        let e = w.zoo.entities[w.find(1).unwrap()].clone();
        let who = building::Species::of(&cat, &e);
        assert_eq!(who.as_ref().map(|s| s.id), Some(LION_ID));
        // The search.
        let found = animal::shelters(&w, &cat, 0, Some(tile(2, 6)), who.as_ref());
        assert_eq!(found.nearest == [8], allowed, "search {slot:?}");
        assert_eq!(found.nearest.is_empty(), !allowed, "search {slot:?}");
        // The open slots of the walk.
        let b = w.zoo.entities[w.find(8).unwrap()].clone();
        let kind = building::kind_of(&cat, &b, 0.0).unwrap();
        let at = (e.common.as_ref().unwrap().x, e.common.as_ref().unwrap().y);
        let open = building::entrances(&w, &b, &kind, None, at, who.as_ref());
        assert_eq!(!open.is_empty(), allowed, "walk {slot:?}");
        // The entry.
        let mut rng = Rng::default();
        let entered = with_cat(&mut w, &mut rng, &cat, |cx| cx.add_user(8, 0));
        assert_eq!(entered, allowed, "entry {slot:?}");
        assert_eq!(users(&w, 8), usize::from(allowed), "entry {slot:?}");
    }
    // A guest is not tested: the slot lists species, no guest building does.
    let cat = types_with(&format!("{CAVE}id=6\n"));
    let w = zoo("m");
    let b = w.zoo.entities[w.find(8).unwrap()].clone();
    let kind = building::kind_of(&cat, &b, 0.0).unwrap();
    assert!(building::has_open_slot(&w, &b, &kind, None, None));
}
