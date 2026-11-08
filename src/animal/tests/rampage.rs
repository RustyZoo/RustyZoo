//! The rampage sets, their end, the escape's dating and send, and what a rampage does to scenery
//! (animals 11, behaviours 6.4).

use super::life::Life;
use super::*;
use crate::animal::escape::{ambient_sets, calm, habitat_rampage, trample};
use crate::config::Config;
use crate::unit::testing::{centre, entity as thing, world_with};

const HABITAT: &str = "bHabitatRampage";
const LOOSE: &str = "bEscapedRampage";

/// The data every set test runs on: both rampage sets and the escaped one are known.
fn data() -> Life {
    Life::new()
        .with(HABITAT, "fPlay(idle)")
        .with(LOOSE, "fPlay(idle)")
        .with("bEscaped", "fPlay(idle)")
}

/// The unit 500 of the pens, in the pen or (`pen` false) on the grounds between the two, with its
/// flags set by `prepare`; the world and the position.
fn unit_in(pen: bool, prepare: impl FnOnce(&mut Animal, &mut Unit)) -> (World, Unit, usize) {
    let mut world = pens();
    let at = world.find(500).unwrap();
    if !pen {
        world.zoo.entities[at].common.as_mut().unwrap().x = 4 * 64 + 32;
    }
    let mut unit = world.units.get(500).unwrap().clone();
    let mut a = Animal::default();
    if let Part::Animal(own) = &mut unit.part {
        std::mem::swap(own, &mut a);
    }
    prepare(&mut a, &mut unit);
    unit.part = Part::Animal(a);
    (world, unit, at)
}

/// The running set and the pending one after the sets of the ambient choice.
fn after(
    pen: bool,
    prepare: impl FnOnce(&mut Animal, &mut Unit),
) -> (Option<String>, Option<String>) {
    let (world, mut unit, at) = unit_in(pen, prepare);
    ambient_sets(&mut unit, &world, &data(), &mut Rng::default(), at);
    let r = &unit.runner;
    (
        r.running().then(|| r.top()).flatten().map(str::to_string),
        r.queued().map(str::to_string),
    )
}

fn named(name: &str) -> Option<String> {
    Some(name.to_string())
}

#[test]
fn a_rampaging_animal_in_an_exhibit_starts_its_habitat_rampage_even_while_moving() {
    let rampage = |a: &mut Animal, _: &mut Unit| a.rampaging = true;
    assert_eq!(after(true, rampage), (named(HABITAT), None));
    assert_eq!(
        after(true, |a, u| {
            rampage(a, u);
            u.moving = true;
        }),
        (named(HABITAT), None)
    );
    // Neither on the grounds without being loose, nor without a rampage.
    assert_eq!(after(false, rampage), (None, None));
    assert_eq!(after(true, |_, _| {}), (None, None));
}

#[test]
fn a_loose_rampaging_animal_runs_the_escaped_rampage_unless_it_moves() {
    let loose = |a: &mut Animal, _: &mut Unit| (a.rampaging, a.escaped) = (true, true);
    // In an exhibit as on the grounds: the escaped branch wins over the habitat one.
    assert_eq!(after(true, loose), (named(LOOSE), None));
    assert_eq!(after(false, loose), (named(LOOSE), None));
    assert_eq!(
        after(false, |a, u| {
            loose(a, u);
            u.moving = true;
        }),
        (None, None)
    );
    // It does not start `bEscaped` while it rampages.
    assert_ne!(after(false, loose).0, named("bEscaped"));
}

#[test]
fn a_rampage_set_that_runs_is_queued_behind_itself_once() {
    let (world, mut unit, at) = unit_in(true, |a, _| a.rampaging = true);
    let mut rng = Rng::default();
    for _ in 0..3 {
        ambient_sets(&mut unit, &world, &data(), &mut rng, at);
    }
    assert_eq!(unit.runner.top(), Some(HABITAT));
    assert_eq!(unit.runner.queued(), Some(HABITAT));
    assert!(!unit.runner.queued_flag(), "not interruptible");
    // A set that ends first (sleep, sickness, hatching) drops the rampage instead.
    unit.runner.queue("bSleep", true, true);
    ambient_sets(&mut unit, &world, &data(), &mut rng, at);
    let Part::Animal(a) = &unit.part else {
        panic!()
    };
    assert!(!a.rampaging);
}

#[test]
fn an_idle_animal_rampaging_in_an_exhibit_starts_the_habitat_rampage_at_the_end_of_the_choice() {
    let start = |pen: bool, rampaging: bool, busy: bool| {
        let (world, mut unit, at) = unit_in(pen, |a, _| a.rampaging = rampaging);
        if busy {
            unit.runner.queue("bWalk", true, true);
        }
        let started = habitat_rampage(&mut unit, &world, &data(), &mut Rng::default(), at);
        (started, unit.runner.top().map(str::to_string))
    };
    assert_eq!(start(true, true, false), (true, named(HABITAT)));
    assert_eq!(start(true, true, true), (false, None));
    assert_eq!(start(true, false, false), (false, None));
    assert_eq!(start(false, true, false), (false, None));
}

#[test]
fn the_end_of_a_rampage_clears_its_set_stops_the_unit_and_resets_the_status() {
    let run = |set: &str| {
        let (world, mut unit, at) = unit_in(true, |a, u| {
            (a.drinking, a.thirsty, a.walk) = (true, true, Some(Escape::default()));
            u.moving = true;
            u.path = vec![1];
        });
        let mut rng = Rng::default();
        let env = crate::behaviour::Env {
            data: &data(),
            unit: &world.zoo.entities[at],
            rng: &mut rng,
        };
        unit.runner
            .start(&mut unit.animation, &env, set, false, true, true);
        unit.runner.queue(set, false, true);
        calm(&mut unit, &world, &data(), at);
        let Part::Animal(a) = &unit.part else {
            panic!()
        };
        (
            unit.runner.top().map(str::to_string),
            unit.runner.queued().map(str::to_string),
            unit.moving,
            unit.path.len(),
            (a.drinking, a.thirsty, a.walk.is_some()),
        )
    };
    for set in [HABITAT, LOOSE] {
        assert_eq!(
            run(set),
            (None, None, false, 0, (false, false, false)),
            "{set}"
        );
    }
    // Another set is left alone.
    let (top, queued, moving, ..) = run("bEat");
    assert_eq!((top, queued, moving), (named("bEat"), named("bEat"), true));
}

#[test]
fn the_check_tells_when_the_rampage_drops() {
    let k = Keys {
        rampage_threshold: 10,
        rampage_max: 3,
        ..keys()
    };
    let (class, env) = (Class::default(), in_exhibit());
    let calm = |happiness: i32, rampage: i32| {
        let mut a = Animal {
            rampaging: true,
            happiness,
            rampage,
            ..animal()
        };
        let out = crate::animal::run(&mut a, &k, &class, &env, &mut Rng::default());
        (out.calm, a.rampaging)
    };
    assert_eq!(calm(5, 1), (false, true));
    // Happiness above the threshold, or the time spent.
    assert_eq!(calm(11, 1), (true, false));
    assert_eq!(calm(5, 3), (true, false));
    // No rampage, nothing to calm.
    let mut a = animal();
    assert!(!crate::animal::run(&mut a, &k, &class, &env, &mut Rng::default()).calm);
}

fn grounds() -> Env {
    Env {
        exhibit: false,
        grounds: true,
        ..in_exhibit()
    }
}

#[test]
fn every_firing_on_the_grounds_dates_the_escape_and_the_alert_comes_once() {
    let (k, class) = (keys(), Class::default());
    let mut a = Animal {
        counters: [1000; COUNTERS],
        ..animal()
    };
    let firing = |a: &mut Animal, env: &Env| {
        a.counters[ESCAPE] = 0;
        crate::animal::run(a, &k, &class, env, &mut Rng::default())
    };
    let first = firing(&mut a, &grounds());
    assert!(first.loose && first.log == ["escape"]);
    let again = firing(&mut a, &grounds());
    assert!(again.loose && again.log.is_empty());
    // In an exhibit it is not loose, and between firings nothing is dated.
    assert!(!firing(&mut a, &in_exhibit()).loose);
    a.counters[ESCAPE] = 5;
    assert!(!crate::animal::run(&mut a, &k, &class, &grounds(), &mut Rng::default()).loose);
}

#[test]
fn a_loose_animal_on_water_is_sent_to_a_land_tile_by_one_draw() {
    let (k, class) = (keys(), Class::default());
    let send = |terrain: u8, stands: u8, land: usize| {
        let mut a = Animal {
            terrain,
            ..animal()
        };
        a.counters[ESCAPE] = 0;
        let env = Env {
            stands,
            kinds: [land, 4, 0],
            ..grounds()
        };
        let mut rng = Rng::default();
        let out = crate::animal::run(&mut a, &k, &class, &env, &mut rng);
        let drawn = (0..8).find(|&n| drew(&rng, n)).unwrap();
        (out.sent, drawn, a.terrain)
    };
    let mut mirror = Rng::default();
    let pick = mirror.pick(7) as usize;
    assert_eq!(send(1, 1, 7), (Some(pick), 1, 0));
    // On dry land, already in land mode, or with no land tile: the mode is stored, no draw.
    assert_eq!(send(1, 0, 7), (None, 0, 0));
    assert_eq!(send(0, 1, 7), (None, 0, 0));
    assert_eq!(send(1, 1, 0), (None, 0, 0));
}

fn trees() -> Catalogue {
    let ty = |class, kind: &str, sub: &str| EntityType {
        class,
        type_name: kind.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(
            Config::parse(b"[Characteristics/Integers]\n").unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let mut cat = Catalogue::default();
    cat.push(ty("rubble", "other", "brubls1"));
    for kind in ["tree", "wide", "stone"] {
        cat.push(ty("objects", kind, ""));
    }
    cat.push(ty("building", "shed", ""));
    cat
}

/// A 6 by 6 map with a rampaging animal 500 and scenery on tile (3,3): the quadrants hold the
/// `kinds`, in the order of the quadrants.
fn grove(kinds: &[&str]) -> (World, Unit) {
    let mut world = world_with((6, 6), |zoo| {
        zoo.entities
            .push(thing("animals", "zebra", 500, centre((2, 3)), 0));
        for (i, kind) in kinds.iter().enumerate() {
            let (x, y) = [(16, 16), (48, 16), (16, 48), (48, 48)][i % 4];
            let at = (3 * 64 + x, 3 * 64 + y);
            let class = if *kind == "shed" {
                "building"
            } else {
                "objects"
            };
            zoo.entities.push(thing(class, kind, 10 + i as u32, at, 0));
        }
    });
    world
        .units
        .names
        .insert(crate::rubble::PIECE_NAME, "piece-%s".into());
    let mut unit = world.units.get(500).unwrap().clone();
    unit.part = Part::Animal(Animal {
        rampaging: true,
        ..Animal::default()
    });
    (world, unit)
}

/// The ids queued for removal after a rampaging step onto tile (3,3), and the rubble pieces made.
fn trampled(world: &mut World, unit: &Unit) -> (Vec<u32>, usize) {
    let cat = trees();
    let (mut rng, events, mut removals) =
        (Rng::default(), crate::sim::Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let tile = world.tile_index(3, 3).unwrap();
    trample(unit, world, &mut ctx, tile);
    let pieces = world.zoo.entities.iter().filter(|e| e.class == "rubble");
    (removals, pieces.count())
}

#[test]
fn a_rampaging_animal_turns_each_rubbleable_item_of_the_tile_to_rubble_once() {
    let (mut w, unit) = grove(&["tree", "tree", "stone", "tree"]);
    let (removals, pieces) = trampled(&mut w, &unit);
    assert_eq!(removals, [10, 11, 13]);
    assert!(pieces >= 1);
    // An item that holds all four quadrants goes once.
    let (mut w, unit) = grove(&["wide"]);
    assert_eq!(trampled(&mut w, &unit).0, [10]);
}

#[test]
fn a_rampaging_animal_turns_a_rubbleable_building_to_rubble_too() {
    let (mut w, unit) = grove(&["shed", "stone"]);
    let (removals, pieces) = trampled(&mut w, &unit);
    assert_eq!(removals, [10]);
    assert!(pieces >= 1);
}

#[test]
fn only_a_rampaging_animal_off_an_escape_walk_tramples() {
    for (rampaging, walk) in [(false, false), (true, true)] {
        let (mut w, mut unit) = grove(&["tree"]);
        if let Part::Animal(a) = &mut unit.part {
            a.rampaging = rampaging;
            a.walk = walk.then(Escape::default);
        }
        assert_eq!(trampled(&mut w, &unit), (vec![], 0), "{rampaging} {walk}");
    }
}

#[test]
fn an_animal_that_walks_out_posts_the_alert_once_and_dates_the_escape_every_check() {
    let (mut world, cat) = (pens(), zebras());
    let at = world.find(500).unwrap();
    world.zoo.entities[at].common.as_mut().unwrap().x = 4 * 64 + 32;
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    let (mut alerts, mut dates) = (0, Vec::new());
    for _ in 0..40_000 / 67 {
        sim.step_in(&mut world, &cat, 67, |_, _, _| {});
        alerts += (world.units.take_messages().iter())
            .filter(|m| *m == &(crate::animal::ESCAPED, 500, vec![]))
            .count();
        dates.push(sim.status.escape);
    }
    assert_eq!((alerts, world.units.escapes.clone()), (1, vec![500]));
    // The date moves to the present at each check while it is loose: more than one value.
    dates.dedup();
    assert!(dates.len() > 2, "{dates:?}");
}

#[test]
fn a_loose_swimmer_on_the_grounds_is_sent_to_a_dry_tile_at_its_check() {
    let (mut world, cat) = (pens(), zebras());
    let at = world.find(500).unwrap();
    let lake = 2 * SIZE as usize + 4;
    world.zoo.entities[at].common.as_mut().unwrap().x = 4 * 64 + 32;
    world.zoo.tiles[lake].terrain = 1;
    world.set_water(vec![false, true]);
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.step_in(&mut world, &cat, 67, |_, _, _| {});
    // Waiting, the animal chooses no walk of its own: only the check can send it.
    let unit = world.units.get_mut(500).unwrap();
    unit.wait = 1_000_000;
    let Part::Animal(a) = &mut unit.part else {
        panic!()
    };
    (a.terrain, a.counters[ESCAPE]) = (1, 0);
    for _ in 0..1100 / 67 {
        sim.step_in(&mut world, &cat, 67, |_, _, _| {});
    }
    let unit = world.units.get(500).unwrap();
    let to = unit.destination.expect("it was sent");
    let tile = world
        .tile_index(to.0.div_euclid(64), to.1.div_euclid(64))
        .unwrap();
    assert!(tile != lake && !world.is_water(tile));
    let Part::Animal(a) = &unit.part else {
        panic!()
    };
    assert_eq!(a.terrain, 0);
}

#[test]
fn an_idle_animal_rampaging_in_its_pen_runs_the_habitat_rampage_in_the_game() {
    let (mut world, cat) = (
        pens(),
        zebras_with("cRampageThreshold=100\ncRampageTimeMax=100", ""),
    );
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.step_in(&mut world, &cat, 67, |_, _, _| {});
    let Part::Animal(a) = &mut world.units.get_mut(500).unwrap().part else {
        panic!()
    };
    a.rampaging = true;
    for _ in 0..300 / 67 {
        sim.step_in(&mut world, &cat, 67, |_, _, _| {});
    }
    assert_eq!(world.units.get(500).unwrap().runner.top(), Some(HABITAT));
}

#[test]
fn the_send_off_water_draw_walks_the_grounds_fill_list_in_its_order() {
    use crate::animal::tank;
    let mut world = pens();
    let at = world.find(500).unwrap();
    let lake = 2 * SIZE as usize + 4;
    world.zoo.entities[at].common.as_mut().unwrap().x = 4 * 64 + 32;
    world.zoo.tiles[lake].terrain = 1;
    world.set_water(vec![false, true]);
    // The list is the fill from the entrance, not the map order, and holds only grounds tiles.
    let list = tank::tiles(&world, crate::exhibit::GROUNDS);
    let (ex, ey) = world.zoo.entrance;
    let entrance = world.tile_index(ex as i32, ey as i32).unwrap();
    assert_eq!(list, crate::exhibit::fill(&world, entrance));
    assert!(list.windows(2).any(|w| w[0] > w[1]) && !list.contains(&(SIZE as usize + 2)));
    let dry: Vec<usize> = list
        .iter()
        .copied()
        .filter(|&t| tank::kind(&world, t) == 0)
        .collect();
    assert!(!dry.contains(&lake) && dry.len() > 3);
    for pick in [0, 1, dry.len() - 1] {
        let mut unit = world.units.get(500).unwrap().clone();
        let (cat, events, mut removals, mut rng) = (
            Catalogue::default(),
            crate::sim::Events::default(),
            Vec::new(),
            Rng::default(),
        );
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &cat,
        };
        tank::send(
            &mut unit,
            &mut world,
            &mut ctx,
            at,
            &data(),
            (0, Some(pick)),
        );
        let to = unit.destination.expect("it was sent");
        let tile = world.tile_index(to.0.div_euclid(64), to.1.div_euclid(64));
        assert_eq!(tile, Some(dry[pick]), "pick {pick}");
    }
}

#[test]
fn a_rampaging_animal_turns_the_quarters_of_a_tile_in_the_slot_order() {
    let (mut w, unit) = grove(&["tree", "tree", "tree", "tree"]);
    // Slot 1 is the lower-left quarter (id 12), slot 2 the upper-right one (id 11).
    assert_eq!(trampled(&mut w, &unit).0, [10, 12, 11, 13]);
}

#[test]
fn a_trampled_item_leaves_the_undo_list() {
    let (mut w, unit) = grove(&["tree"]);
    let at = w.find(10).unwrap();
    let (e, s) = (w.zoo.entities[at].clone(), w.shape(at));
    w.unrecorded(|w| w.remove(10));
    // The tree is placed again by hand: the undo list holds it.
    let id = w.add(e, s).unwrap();
    assert!(w.can_undo());
    assert_eq!(trampled(&mut w, &unit).0, [id]);
    assert!(!w.can_undo());
}

#[test]
fn a_loose_swimmer_in_the_zoo_exterior_is_sent_to_the_grounds_list() {
    use crate::animal::tank;
    // A zoo wall down the east side of column 1, the entrance on tile (2, 0) facing west: columns
    // 0 and 1 are the exterior.
    let mut world = world_with((6, 6), |zoo| {
        zoo.entrance = (2, 0);
        let wall = |y: i32| thing("zoowall", "", 300 + y as u32, centre((1, y)), 2);
        zoo.entities.extend((0..6).map(wall));
        zoo.entities
            .push(thing("building", "", 320, centre((2, 0)), 6));
        zoo.entities
            .push(thing("animals", "zebra", 500, centre((0, 3)), 0));
    });
    let tile = world.tile_index(0, 3).unwrap();
    assert_eq!(
        world.exhibits().region(tile),
        Some(crate::exhibit::EXTERIOR)
    );
    let mut unit = world.units.get(500).unwrap().clone();
    let (cat, events, mut removals, mut rng) = (
        Catalogue::default(),
        crate::sim::Events::default(),
        Vec::new(),
        Rng::default(),
    );
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    // The list holds the exterior with the grounds: pick its dry tile (0, 5). The tiles beyond the
    // wall are closed to the path search, so the send shows in the task it sets.
    let far = world.tile_index(0, 5).unwrap();
    let dry: Vec<usize> = (tank::tiles(&world, crate::exhibit::GROUNDS).into_iter())
        .filter(|&t| tank::kind(&world, t) == 0)
        .collect();
    let pick = dry.iter().position(|&t| t == far).expect("in the list");
    assert_eq!(unit.goal.record.task, 0);
    let at = world.find(500).unwrap();
    tank::send(
        &mut unit,
        &mut world,
        &mut ctx,
        at,
        &data(),
        (0, Some(pick)),
    );
    assert_eq!(unit.goal.record.task, crate::goal::RANDOM);
}

#[test]
fn the_end_of_a_rampage_clears_the_target_flag_in_the_escape_check() {
    let mut a = Animal {
        rampaging: true,
        rampage_target: true,
        happiness: 0,
        ..animal()
    };
    run(&mut a, &keys(), &Class::default(), &in_exhibit());
    assert!(!a.rampaging && !a.rampage_target);
}

#[test]
fn a_failed_rampage_start_clears_the_target_flag_in_the_escape_check() {
    // Chance 100, but the time max is below the min: no rampage, the sub-state down.
    let k = Keys {
        rampage_threshold: 0,
        rampage_chance: 100,
        rampage_min: 5,
        rampage_max: 1,
        ..keys()
    };
    let mut a = Animal {
        rampage_target: true,
        happiness: 0,
        ..animal()
    };
    run(&mut a, &k, &Class::default(), &in_exhibit());
    assert!(!a.rampaging && !a.rampage_target);
}

#[test]
fn the_ambient_choice_that_drops_a_rampage_clears_the_target_flag() {
    let (world, mut unit, at) = unit_in(true, |a, _| {
        (a.rampaging, a.rampage_target) = (true, true);
    });
    unit.runner.queue("bSleep", true, true);
    ambient_sets(&mut unit, &world, &data(), &mut Rng::default(), at);
    let Part::Animal(a) = &unit.part else {
        panic!()
    };
    assert!(!a.rampaging && !a.rampage_target);
}
