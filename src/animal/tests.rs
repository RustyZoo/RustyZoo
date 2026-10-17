//! The status values and the checks on synthetic values, and four animals in two exhibits.

use std::sync::Arc;

use super::*;
use crate::render::order::Kind;
use crate::sim::Sim;
use crate::world::{Shape, TankKind};
use crate::zoo::{Clocks, Common, Tile, View, Zoo};

mod life;

fn keys() -> Keys {
    Keys::read(&|_, fallback| fallback, &Class::default())
}

/// An animal with every counter far from firing.
fn animal() -> Animal {
    Animal {
        counters: [1000; COUNTERS],
        crowd_direction: -1,
        life: 5000,
        ..Animal::default()
    }
}

/// The checks on a fresh generator: the messages posted.
fn run(a: &mut Animal, k: &Keys, class: &Class, env: &Env) -> Vec<i32> {
    super::run(a, k, class, env, &mut Rng::default()).messages
}

/// Runs the checks with one counter at 0 so that it fires.
fn fires(a: &mut Animal, counter: usize, k: &Keys, env: &Env) -> Vec<i32> {
    a.counters[counter] = 0;
    run(a, k, &Class::default(), env)
}

/// Whether `rng` has drawn exactly `n` numbers since it was new.
fn drew(rng: &Rng, n: usize) -> bool {
    let mut check = Rng::default();
    (0..n).for_each(|_| _ = check.rand());
    *rng == check
}

fn in_exhibit() -> Env {
    let census = Census {
        species: 3,
        adults: 3,
        all_adults: 3,
        unhappy: 0,
        sick: 0,
    };
    Env {
        exhibit: true,
        size: 100,
        species: 3,
        rating: 70,
        census,
        ..Env::default()
    }
}

#[test]
fn creation_draws_one_number_per_counter_and_one_more_for_a_young_animal() {
    let (k, class) = (keys(), Class::default());
    for (subtype, draws) in [("f", 16), ("m", 16), ("p", 16), ("y", 17)] {
        let (mut rng, mut check) = (Rng::default(), Rng::default());
        let a = Animal::create(&k, &class, subtype, &mut rng);
        let drawn: Vec<u32> = (0..draws).map(|_| check.rand()).collect();
        assert_eq!(rng, check, "{subtype}");
        for (i, c) in a.counters.iter().enumerate() {
            assert_eq!(*c, drawn[draws - 16 + i] as i32 % class.periods[i]);
        }
        assert_eq!(
            a.female,
            subtype != "m" && (subtype != "y" || drawn[0] & 1 == 0)
        );
        assert_eq!((a.happiness, a.health, a.hunger), (60, 500, 0));
    }
    // A period of 0 starts at -1 and draws nothing.
    let mut class = Class::default();
    class.periods[HABITAT] = 0;
    let mut rng = Rng::default();
    assert_eq!(
        Animal::create(&k, &class, "f", &mut rng).counters[HABITAT],
        -1
    );
    let mut check = Rng::default();
    (0..15).for_each(|_| _ = check.rand());
    assert_eq!(rng, check);
}

#[test]
fn the_update_applies_each_pending_change_once_and_clamps_it() {
    let k = keys();
    let mut a = Animal {
        happiness: 90,
        happiness_change: 30,
        hunger: 5,
        hunger_change: -20,
        energy: 90,
        energy_change: 40,
        dung_change: 500,
        health: 100,
        health_change: -1000,
        ..animal()
    };
    a.update(&k);
    assert_eq!(
        (a.happiness, a.hunger, a.energy, a.dung, a.health),
        (100, 0, 100, 100, 0)
    );
    assert!(a.at_minimum_health && !a.very_unhappy);
    assert_eq!(
        a.happiness_change + a.hunger_change + a.energy_change + a.dung_change,
        0
    );
    a.happiness_change = -181;
    a.update(&k);
    assert!(a.very_unhappy, "-81 is below -80");
    a.happiness_change = i32::MIN;
    a.update(&k);
    assert_eq!(a.happiness, -100, "a huge change saturates");
    // An egg loses a hit point per tick and keeps the other pending changes.
    let mut egg = Animal {
        egg: true,
        health: 10,
        happiness_change: 7,
        ..animal()
    };
    egg.update(&k);
    assert_eq!((egg.health, egg.happiness, egg.happiness_change), (9, 0, 7));
    // A low limit above the high one wins.
    let mut a = animal();
    a.update(&Keys {
        min_dirty: 50,
        max_dirty: 20,
        ..keys()
    });
    assert_eq!(a.dung, 50);
}

#[test]
fn captivity_wears_down_every_period() {
    let mut a = animal();
    fires(&mut a, CAPTIVITY, &keys(), &in_exhibit());
    assert_eq!((a.happiness_change, a.counters[CAPTIVITY]), (-5, 4));
}

#[test]
fn the_habitat_check_follows_the_rating_in_three_steps() {
    let k = Keys {
        habitat_preference: 50,
        ..keys()
    };
    let check = |rating: i32, exhibit: bool| {
        let mut a = animal();
        let env = Env {
            rating,
            exhibit,
            grounds: !exhibit,
            ..in_exhibit()
        };
        let messages = fires(&mut a, HABITAT, &k, &env);
        (a.happiness_change, messages)
    };
    assert_eq!(check(50, true), (20, vec![]));
    assert_eq!(check(49, true), (-10, vec![]));
    assert_eq!(
        check(5, true),
        (-10, vec![]),
        "5 * 100 is not below 10 * 50"
    );
    assert_eq!(check(4, true), (-20, vec![DISLIKES_EXHIBIT]));
    assert_eq!(
        check(0, false),
        (-20, vec![]),
        "the grounds rate 0 and say nothing"
    );
    // What the species think of each other: a tenth, cut to -50 and 150 first.
    let mut a = animal();
    fires(
        &mut a,
        HABITAT,
        &k,
        &Env {
            other: -1000,
            ..in_exhibit()
        },
    );
    assert_eq!(
        (a.happiness_change, a.important >> INCOMPATIBLE & 1),
        (20 - 50, 1)
    );
}

#[test]
fn the_social_check_judges_the_number_the_room_and_the_guests() {
    let k = Keys {
        habitat_size: 100,
        number_min: 3,
        number_max: 2,
        ..keys()
    };
    let change = |env: &Env, pressure: [i32; 3]| {
        let mut a = Animal {
            pressure,
            ..animal()
        };
        fires(&mut a, SOCIAL, &k, env);
        (a.happiness_change, a.advice)
    };
    let bit = |b: u32| 1u64 << b;
    let few = Env {
        species: 2,
        ..in_exhibit()
    };
    assert_eq!(change(&few, [0; 3]), (-5, bit(FEW)));
    // Three adults in 100 tiles: one room of a hundred takes two. Three rooms take six.
    assert_eq!(change(&in_exhibit(), [0; 3]), (-5, bit(MANY)));
    let big = Env {
        size: 300,
        ..in_exhibit()
    };
    assert_eq!(change(&big, [0; 3]), (0, 0));
    // 100 tiles for 7 adults round to 14, below 30.
    let census = Census {
        all_adults: 7,
        adults: 1,
        ..in_exhibit().census
    };
    assert_eq!(
        change(
            &Env {
                census,
                ..in_exhibit()
            },
            [0; 3]
        ),
        (-10, bit(CRAMPED))
    );
    assert_eq!(change(&big, [41, 0, 0]), (-1, bit(CROWDED)));
    assert_eq!(change(&big, [40, 0, 0]), (0, 0));
    // The direction away from the guests; no direction keeps the old one.
    let mut a = Animal {
        pressure: [50, 3, -2],
        ..animal()
    };
    fires(&mut a, SOCIAL, &k, &big);
    assert_eq!(a.crowd_direction, 7);
    let mut a = Animal {
        pressure: [50, 0, 0],
        crowd_direction: 3,
        ..animal()
    };
    fires(&mut a, SOCIAL, &k, &big);
    assert_eq!(a.crowd_direction, 3);
    // The bits follow the animal at every tick, the change only when the counter fires.
    let mut a = animal();
    run(&mut a, &k, &Class::default(), &few);
    assert_eq!((a.happiness_change, a.advice), (0, bit(FEW)));
    run(&mut a, &k, &Class::default(), &big);
    assert_eq!(a.advice, 0);
    // A room size of 0 (which crashes the original) leaves room for any number, a negative one
    // makes one room of the exhibit, and a huge number of animals per room is never reached.
    for (size, max, bits) in [(0, 2, 0), (-5, 2, bit(MANY)), (100, i32::MAX, 0)] {
        let k = Keys {
            habitat_size: size,
            number_max: max,
            ..k.clone()
        };
        let mut a = animal();
        fires(&mut a, SOCIAL, &k, &in_exhibit());
        assert_eq!(a.advice, bits, "{size} {max}");
    }
    // An animal whose counter ends during a show never checks again (issue 317).
    let mut a = Animal {
        in_show: true,
        ..animal()
    };
    fires(&mut a, SOCIAL, &k, &few);
    a.in_show = false;
    (0..100).for_each(|_| _ = run(&mut a, &k, &Class::default(), &few));
    assert!(a.counters[SOCIAL] < 0 && a.happiness_change == 0);
}

#[test]
fn the_other_checks_change_the_happiness_and_raise_the_flags() {
    let (k, env, class) = (keys(), in_exhibit(), Class::default());
    // A keeper's visit and mourning count once per tick.
    let mut a = Animal {
        keeper_visited: true,
        mourning: true,
        ..animal()
    };
    run(&mut a, &k, &class, &env);
    assert_eq!(a.happiness_change, 100 - 21);
    run(&mut a, &k, &class, &env);
    assert_eq!(a.happiness_change, 79);
    // The message below -60.
    for (happiness, said) in [(-61, vec![NOT_HAPPY]), (-60, vec![])] {
        let mut a = Animal {
            happiness,
            ..animal()
        };
        assert_eq!(fires(&mut a, MESSAGE, &k, &env), said);
    }
}

#[test]
fn a_crated_animal_and_an_egg_run_no_check_and_the_grounds_ask_for_an_exhibit() {
    let k = Keys {
        number_min: 3,
        ..keys()
    };
    let few = Env {
        species: 1,
        ..in_exhibit()
    };
    let grounds = Env {
        grounds: true,
        ..Env::default()
    };
    let mut crated = Animal {
        crated: true,
        advice: 0xff,
        ..animal()
    };
    fires(&mut crated, SOCIAL, &k, &few);
    assert_eq!((crated.advice, crated.happiness_change), (1 << 62, 0));
    let mut egg = Animal {
        egg: true,
        advice: 0xff,
        ..animal()
    };
    fires(&mut egg, SOCIAL, &k, &few);
    assert_eq!(egg.advice, 0);
    fires(&mut egg, SOCIAL, &k, &grounds);
    assert_eq!(egg.advice, 1 << 62);
    let mut a = Animal {
        advice: 1,
        ..animal()
    };
    run(&mut a, &k, &Class::default(), &grounds);
    assert_eq!(a.advice, 1 << 62, "on the grounds every other line goes");
    run(&mut a, &k, &Class::default(), &few);
    assert_eq!(a.advice >> 62, 0);
}

fn config(text: &str) -> Config {
    Config::parse(text.as_bytes()).unwrap()
}

#[test]
fn the_keys_come_from_the_subtype_then_the_default_then_the_plain_section_and_the_class_file() {
    let file = config(
        "[Characteristics/Integers]\ncCaptivityCheck=20\ncDeathChange=-20;\ncEnergyCheck=zebra\n",
    );
    let int = |key: &str, fallback| {
        let value = file.section("Characteristics/Integers").unwrap().get(key);
        value.and_then(to_int).unwrap_or(fallback)
    };
    let class = Class::read(&int);
    assert_eq!(
        (
            class.periods[CAPTIVITY],
            class.death_change,
            class.periods[ENERGY]
        ),
        (20, -20, 0)
    );
    let ty = EntityType {
        class: "animals",
        type_name: b"zebra".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(config(
            "[Global]\nDefaultSubtype=m\n[f/Characteristics/Integers]\ncCaptivity=9\n\
             [m/Characteristics/Integers]\ncCaptivity=1\ncCrowd=2\n\
             [Characteristics/Integers]\ncCrowd=3\ncMaxHits=120\n",
        ))),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let k = Keys::of(&ty, &class);
    assert_eq!((k.captivity, k.crowd, k.max_hits), (9, 2, 120));
    assert_eq!((k.keeper_arrives, k.number_max), (100, 10));
    assert_eq!(Class::default().periods[WATER], 30);
}

// Four zebras in two pens of 3 by 3 tiles: the ground of the first is grass, which the species
// likes, the ground of the other is of terrain type 2.
const SIZE: i32 = 8;

fn entity(class: &str, subtype: &str, (tx, ty): (i32, i32), rotation: u32, id: u32) -> Entity {
    Entity {
        class: class.to_string(),
        type_name: if class == "animals" { "zebra" } else { "" }.to_string(),
        subtype: subtype.to_string(),
        body_size: None,
        common: Some(Common {
            x: tx * 64 + 32,
            y: ty * 64 + 32,
            z: 0,
            rotation,
            id,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
            movement_mode: 0,
            food: 0,
            animal: None,
            guest: None,
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let gate = e.class == "gate";
    (gate || e.class == "fences").then_some(Shape {
        kind: if gate { Kind::Fixed } else { Kind::Edge },
        footprint: if gate { (2, 2) } else { (1, 1) },
        fence: !gate,
        wall: false,
        gate,
        gated: !gate,
        walkable: (gate, gate),
        tank: TankKind::NONE,
    })
}

fn pens() -> World {
    let mut entities = vec![entity("gate", "", (SIZE - 1, SIZE - 1), 2, 1)];
    for (n, x0) in [1, 5].into_iter().enumerate() {
        for i in 0..3 {
            for (at, d) in [
                ((x0 + i, 1), 0),
                ((x0 + i, 3), 4),
                ((x0, 1 + i), 6),
                ((x0 + 2, 1 + i), 2),
            ] {
                entities.push(entity("fences", "f", at, d, 100 + entities.len() as u32));
            }
        }
        for (i, dx) in [0, 1].into_iter().enumerate() {
            let id = 500 + 10 * n as u32 + i as u32;
            entities.push(entity("animals", "f", (x0 + dx, 2), 0, id));
        }
    }
    let tile = |x: i32| Tile {
        elevation: 0,
        shape: 0,
        terrain: if x >= 5 { 2 } else { 0 },
        edges: 0,
    };
    let zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: SIZE as u32,
        height: SIZE as u32,
        entrance: (SIZE as u32 - 1, SIZE as u32 - 1),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 10_000,
        tiles: (0..SIZE * SIZE).map(|t| tile(t % SIZE)).collect(),
        entities,
        view: View::default(),
        clocks: Clocks::default(),
        names: Default::default(),
    };
    World::new(zoo, shape)
}

fn zebras() -> Catalogue {
    // Neither starving nor falling ill: the pens are about the happiness.
    zebras_with("cHungryHealthChange=0\ncSickRandomChance=0", "")
}

/// The zebras with more keys (a key written twice keeps its first value) and more sections.
fn zebras_with(keys: &str, sections: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "animals",
        type_name: b"zebra".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(config(&format!(
            "[f/Characteristics/Integers]\n{keys}\ncInitialHappiness=0\ncHabitatPreference=60\n\
                 cNumberAnimalsMin=2\ncHabitatSize=100\ncAnimalDensity=1\ncCaptivity=1\n\
                 [cCompatibleTerrain]\nv=0\nv=100\n{sections}"
        )))),
        name_id: 1,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

/// Ten minutes of 67 ms steps: the animals' log lines and the world's units.
fn ten_minutes() -> (Vec<String>, World) {
    let (mut world, cat) = (pens(), zebras());
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    let mut log = Vec::new();
    for _ in 0..600_000 / 67 {
        sim.step_in(&mut world, &cat, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
    }
    (log, world)
}

#[test]
fn animals_in_a_good_and_a_bad_exhibit_go_up_and_down_and_two_runs_agree() {
    let (log, world) = ten_minutes();
    // The grass pen rates 80 against a preference of 60, the other 50: +20 every third tick
    // and -10; both pay the captivity of 1 every fifth tick.
    let series = |id: u32| -> Vec<i32> {
        let from = format!("status {id} ");
        (log.iter())
            .filter_map(|l| l.strip_prefix(&from)?.split(' ').next()?.parse().ok())
            .collect()
    };
    let (good, bad) = (series(500), series(510));
    assert_eq!(good.len(), 600, "one line per second");
    assert_eq!(good[..8], [0, 0, 19, 19, 19, 39, 39, 38]);
    assert_eq!(bad[..8], [0, 0, -11, -11, -11, -21, -21, -22]);
    assert_eq!((good[17], bad[29]), (100, -100));
    assert_eq!(
        (good[599], series(501)[599], series(511)[599]),
        (100, 100, -100)
    );
    let (again, other) = ten_minutes();
    assert!(log == again && world.units.hash() == other.units.hash());
}

/// The record of a loaded animal: the values of the example, a few counters, the rest not stored.
fn record() -> AnimalRecord {
    let mut counters = [None; COUNTERS];
    counters[CAPTIVITY] = Some(6);
    counters[HABITAT] = Some(15);
    AnimalRecord {
        happiness: 38,
        hunger: 7,
        health: 250,
        energy: 15,
        dung: 3,
        female: true,
        counters,
        at_minimum_health: false,
        very_unhappy: false,
        keeper_visited: false,
        mourning: false,
        dying: false,
        crated: false,
        egg: false,
        thirsty: false,
        rampaging: false,
        rampage: 0,
    }
}

#[test]
fn a_loaded_animal_keeps_its_record_and_draws_nothing_at_its_turn() {
    let cat = zebras();
    let start = |loaded: bool| {
        let mut world = pens();
        if loaded {
            for e in &mut world.zoo.entities {
                if e.class == "animals" {
                    e.common.as_mut().unwrap().animal = Some(Box::new(record()));
                }
            }
        }
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(64, Clocks::default());
        (sim, world)
    };
    let (mut sim, mut world) = start(true);
    let mut check = sim.rng.clone();
    sim.load(&mut world, &cat);
    // Four animals of a known sex: sixteen draws each, at the load, and they are started.
    (0..64).for_each(|_| _ = check.rand());
    assert_eq!(sim.rng, check);
    let Part::Animal(a) = &world.units.get(510).unwrap().part else {
        panic!("not an animal");
    };
    assert!(world.units.get(510).unwrap().started);
    let drawn = a.counters[SOCIAL];
    assert!((0..world.units.class.periods[SOCIAL]).contains(&drawn));
    assert_eq!(
        (a.happiness, a.hunger, a.energy, a.health, a.dung),
        (38, 7, 15, 250, 3)
    );
    assert_eq!((a.counters[CAPTIVITY], a.counters[HABITAT]), (6, 15));
    // The first step draws what it would draw without the animals' creation.
    sim.step_in(&mut world, &cat, 67, |_, _, _| {});
    let (mut other, mut fresh) = start(false);
    other.step_in(&mut fresh, &cat, 67, |_, _, _| {});
    assert_eq!(sim.rng, other.rng, "no draw at the first turn");
}

/// One run of the checks of an animal on a fresh generator, with the generator it leaves.
fn once(a: &mut Animal, k: &Keys, class: &Class, env: &Env) -> (Out, Rng) {
    let mut rng = Rng::default();
    (super::run(a, k, class, env, &mut rng), rng)
}

#[test]
fn a_negative_period_draws_a_remainder_and_never_fires() {
    let mut class = Class::default();
    class.periods[HABITAT] = -3;
    class.periods[BORED] = i32::MIN;
    let mut rng = Rng::default();
    let a = Animal::create(&keys(), &class, "f", &mut rng);
    assert!(drew(&rng, 16));
    assert!((0..3).contains(&a.counters[HABITAT]) && a.counters[BORED] >= 0);
    let mut c = 0;
    assert!(fire(&mut c, -3) && c == -4);
    assert!(!(0..9).any(|_| fire(&mut c, -3)));
}

#[test]
fn captivity_draws_twice_below_the_threshold_and_tramples_only_bare_grass() {
    let k = Keys {
        dirt_chance: 100,
        ..keys()
    };
    let class = Class {
        ui_threshold: 75,
        dirt_chance_chance: 100,
        ..Class::default()
    };
    let grass = Env {
        grass: true,
        ..in_exhibit()
    };
    let go = |happiness, k: &Keys, class: &Class, env: &Env| {
        let mut a = Animal {
            happiness,
            ..animal()
        };
        a.counters[CAPTIVITY] = 0;
        let (out, rng) = once(&mut a, k, class, env);
        (out.trample, rng)
    };
    // The threshold is 2 * 75 - 201 = -51.
    let (trample, rng) = go(-51, &k, &class, &grass);
    assert!(trample && drew(&rng, 2));
    let (trample, rng) = go(-50, &k, &class, &grass);
    assert!(!trample && drew(&rng, 0));
    // Not bare grass: the draws are made all the same.
    let (trample, rng) = go(-100, &k, &class, &in_exhibit());
    assert!(!trample && drew(&rng, 2));
    // No dirt chance in the species: no draw. A first draw that fails: one draw.
    let none = Keys {
        dirt_chance: 0,
        ..k.clone()
    };
    assert!(drew(&go(-100, &none, &class, &grass).1, 0));
    let shy = Class {
        dirt_chance_chance: 0,
        ..class.clone()
    };
    let (trample, rng) = go(-100, &k, &shy, &grass);
    assert!(!trample && drew(&rng, 1));
    // The first draw is compared with the class chance, the second with the species'.
    for seed in 0..40 {
        let (mut rng, mut check) = (Rng::default(), Rng::default());
        rng.seed(seed);
        check.seed(seed);
        let (d1, d2) = (check.rand() % 100, check.rand() % 100);
        let class = Class {
            dirt_chance_chance: 40,
            ..class.clone()
        };
        let k = Keys {
            dirt_chance: 30,
            ..k.clone()
        };
        let mut a = Animal {
            happiness: -100,
            ..animal()
        };
        a.counters[CAPTIVITY] = 0;
        let out = super::run(&mut a, &k, &class, &grass, &mut rng);
        assert_eq!(out.trample, d1 < 40 && d2 < 30, "seed {seed}");
        assert!(drew_since(&rng, seed, if d1 < 40 { 2 } else { 1 }));
    }
}

/// Whether `rng` has drawn exactly `n` numbers since it was seeded with `seed`.
fn drew_since(rng: &Rng, seed: u32, n: usize) -> bool {
    let mut check = Rng::default();
    check.seed(seed);
    (0..n).for_each(|_| _ = check.rand());
    *rng == check
}

#[test]
fn the_rampage_draws_are_made_at_or_below_the_threshold_and_not_in_a_show() {
    let k = Keys {
        rampage_threshold: -10,
        rampage_chance: 100,
        rampage_min: 2,
        rampage_max: 5,
        ..keys()
    };
    let class = Class::default();
    let go = |happiness, k: &Keys, show: bool| {
        let mut a = Animal {
            happiness,
            in_show: show,
            ..animal()
        };
        a.counters[ESCAPE] = 0;
        let (_, rng) = once(&mut a, k, &class, &in_exhibit());
        (a, rng)
    };
    let (a, rng) = go(-10, &k, false);
    assert!(drew(&rng, 2) && a.counters[ESCAPE] == class.periods[ESCAPE] - 1);
    assert!(a.rampaging && (0..=3).contains(&a.rampage));
    assert!(drew(&go(-9, &k, false).1, 0));
    // A missing chance or minimum time, or a minimum below 1, draws nothing at any happiness.
    for k in [
        Keys {
            rampage_chance: 0,
            ..k.clone()
        },
        Keys {
            rampage_chance: -4,
            ..k.clone()
        },
        Keys {
            rampage_min: 0,
            ..k.clone()
        },
        Keys {
            rampage_min: -3,
            ..k.clone()
        },
    ] {
        let (a, rng) = go(-100, &k, false);
        assert!(drew(&rng, 0) && !a.rampaging);
    }
    // The defaults draw nothing; so does a maximum below the minimum after the first draw.
    assert!(drew(&go(-100, &keys(), false).1, 0));
    let short = Keys {
        rampage_max: 1,
        ..k.clone()
    };
    let (a, rng) = go(-100, &short, false);
    assert!(drew(&rng, 1) && !a.rampaging && a.rampage == 0);
    let (a, rng) = go(-100, &k, true);
    assert!(
        drew(&rng, 0) && a.counters[ESCAPE] == 0,
        "nothing runs in a show"
    );
    // One draw against the percent, a second only on success.
    for seed in 0..40 {
        let k = Keys {
            rampage_chance: 50,
            ..k.clone()
        };
        let mut rng = Rng::default();
        rng.seed(seed);
        let mut a = Animal {
            happiness: -100,
            ..animal()
        };
        super::run(&mut a, &k, &class, &in_exhibit(), &mut rng);
        let started = first(seed) % 100 < 50;
        assert_eq!(a.rampaging, started);
        assert!(drew_since(&rng, seed, if started { 2 } else { 1 }));
    }
}

#[test]
fn a_rampage_draws_nothing_while_it_lasts() {
    let k = Keys {
        rampage_threshold: -10,
        rampage_chance: 100,
        rampage_min: 1,
        rampage_max: 3,
        ..keys()
    };
    let class = Class::default();
    let (env, mut rng) = (in_exhibit(), Rng::default());
    // A rampage of 0 to 2 more checks: the counter goes up by 1 at each check and the flag
    // drops (counter 0) when it is above the maximum of 3.
    let mut a = Animal {
        happiness: -50,
        rampaging: true,
        rampage: 1,
        ..animal()
    };
    let seen: Vec<(bool, i32)> = (0..4)
        .map(|_| {
            super::run(&mut a, &k, &class, &env, &mut rng);
            (a.rampaging, a.rampage)
        })
        .collect();
    assert_eq!(seen[..3], [(true, 2), (true, 3), (false, 0)]);
    assert!(
        drew(&rng, 2),
        "no draw until the flag dropped, then the two that start it again"
    );
    assert!(a.rampaging);
    // Happiness above the threshold ends it at the next check, still without a draw.
    let mut rng = Rng::default();
    let mut a = Animal {
        happiness: -9,
        rampaging: true,
        rampage: 1,
        ..animal()
    };
    super::run(&mut a, &k, &class, &env, &mut rng);
    assert!(drew(&rng, 0) && !a.rampaging && a.rampage == 0);
    // While it lasts the bored check returns before its draw.
    let k = Keys { drink: 10, ..k };
    let mut a = Animal {
        happiness: -50,
        rampaging: true,
        rampage: 0,
        ..animal()
    };
    a.counters[BORED] = 0;
    let mut rng = Rng::default();
    super::run(&mut a, &k, &class, &env, &mut rng);
    assert!(drew(&rng, 0) && !a.thirsty && a.counters[BORED] == class.periods[BORED] - 1);
}

#[test]
fn a_loaded_animal_keeps_its_thirst_and_its_rampage() {
    let cat = zebras_with(
        "cRampageChance=100\ncRampageTimeMin=1\ncRampageTimeMax=20\ncRampageThreshold=50",
        "",
    );
    let mut world = pens();
    for e in &mut world.zoo.entities {
        if e.class == "animals" {
            e.common.as_mut().unwrap().animal = Some(Box::new(AnimalRecord {
                thirsty: true,
                rampaging: true,
                rampage: 12,
                happiness: 20,
                ..record()
            }));
        }
    }
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let Part::Animal(a) = &world.units.get(500).unwrap().part else {
        panic!("not an animal");
    };
    assert_eq!((a.thirsty, a.rampaging, a.rampage), (true, true, 12));
    // Its first checks draw no rampage number: the generator ends where a pen of animals
    // that never rampage leaves it.
    let after = |cat: &Catalogue, saved: bool| {
        let mut world = pens();
        for e in &mut world.zoo.entities {
            if e.class == "animals" {
                e.common.as_mut().unwrap().animal = Some(Box::new(AnimalRecord {
                    thirsty: saved,
                    rampaging: saved,
                    rampage: 12,
                    happiness: 20,
                    ..record()
                }));
            }
        }
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(64, Clocks::default());
        sim.load(&mut world, cat);
        for _ in 0..40 {
            sim.step_in(&mut world, cat, 67, |_, _, _| {});
        }
        sim.rng.clone()
    };
    assert_eq!(after(&cat, true), after(&zebras(), true));
}

#[test]
fn a_reset_keeps_the_zap_counter() {
    let (k, env) = (keys(), in_exhibit());
    let class = Class {
        reset_interval: 1,
        ..Class::default()
    };
    let mut rng = Rng::default();
    let mut a = Animal::create(&k, &class, "f", &mut rng);
    a.zap = 3;
    a.rampaging = true;
    a.rampage = 4;
    super::run(&mut a, &k, &class, &env, &mut rng);
    assert_eq!(a.zap, 2, "the counter ran down across the reset");
    assert!(!a.rampaging && a.rampage == 0 && a.reset == 1);
}

/// The first number a generator seeded with `seed` gives.
fn first(seed: u32) -> u32 {
    let mut rng = Rng::default();
    rng.seed(seed);
    rng.rand()
}

#[test]
fn the_bored_check_draws_once_and_a_drink_raises_the_thirst_flag() {
    let k = Keys {
        drink: 10,
        chase: 10,
        ..keys()
    };
    let class = Class::default();
    let go = |a: &mut Animal, k: &Keys, env: &Env, seed| {
        a.counters[BORED] = 0;
        a.advice = 1 << STIMULATION;
        let mut rng = Rng::default();
        rng.seed(seed);
        super::run(a, k, &class, env, &mut rng);
        rng
    };
    for seed in 0..40 {
        let mut a = animal();
        let rng = go(&mut a, &k, &in_exhibit(), seed);
        let r = first(seed) as i32 % 20 + 1;
        assert!(
            drew_since(&rng, seed, 1) && a.thirsty == (r <= 10),
            "seed {seed}"
        );
        assert_eq!(a.advice, 0, "bit 6 is cleared when the counter fires");
        assert_eq!(a.counters[BORED], class.periods[BORED] - 1);
    }
    // Alone, the play chance is 0: the drink is the only choice, and nothing is drawn with none.
    let alone = Env {
        census: Census {
            species: 1,
            ..in_exhibit().census
        },
        ..in_exhibit()
    };
    let only_drink = Keys {
        drink: 5,
        ..k.clone()
    };
    let mut a = animal();
    assert!(drew_since(&go(&mut a, &only_drink, &alone, 7), 7, 1) && a.thirsty);
    let mut a = animal();
    assert!(drew_since(&go(&mut a, &k, &alone, 7), 7, 1) || !a.thirsty);
    let only_play = Keys {
        drink: 0,
        ..k.clone()
    };
    let mut a = animal();
    assert!(drew_since(&go(&mut a, &only_play, &alone, 7), 7, 0) && !a.thirsty);
    // Water, a full stomach, a tired, sick or busy animal: no draw, or no thirst.
    let wet = Env {
        wet: true,
        ..in_exhibit()
    };
    let mut a = animal();
    assert!(drew_since(&go(&mut a, &k, &wet, 3), 3, 1) && !a.thirsty);
    for tired in [
        Animal {
            energy: 70,
            ..animal()
        },
        Animal {
            hunger: 100,
            ..animal()
        },
        Animal {
            at_minimum_health: true,
            ..animal()
        },
        Animal {
            thirsty: true,
            ..animal()
        },
    ] {
        let mut a = tired;
        let rng = go(&mut a, &k, &in_exhibit(), 3);
        assert!(drew_since(&rng, 3, 0), "{a:?}");
        assert_eq!(a.advice, 0);
    }
    let mut a = Animal {
        energy: 69,
        hunger: 99,
        ..animal()
    };
    assert!(drew_since(&go(&mut a, &k, &in_exhibit(), 3), 3, 1));
    // An egg or an animal in a show: the counter waits.
    let mut a = Animal {
        in_show: true,
        ..animal()
    };
    go(&mut a, &k, &in_exhibit(), 3);
    assert_eq!(a.counters[BORED], 0);
}

#[test]
fn another_animal_that_is_very_unhappy_angers_the_animal_at_the_other_check() {
    let (k, class) = (keys(), Class::default());
    let upset = Env {
        census: Census {
            unhappy: 1,
            ..in_exhibit().census
        },
        ..in_exhibit()
    };
    let go = |k: &Keys, env: &Env| {
        let mut a = Animal {
            advice: 1 << ANGRY,
            ..animal()
        };
        run(&mut a, k, &class, env);
        a.counters[OTHER] = 0;
        run(&mut a, k, &class, env);
        (a.happiness_change, a.advice, a.important)
    };
    assert_eq!(go(&k, &upset), (-30, 1 << ANGRY, 1 << ANGRY));
    let mild = Keys {
        other_angry: -9,
        ..k.clone()
    };
    assert_eq!(go(&mild, &upset), (-9, 1 << ANGRY, 0));
    let calm = Keys {
        other_angry: 0,
        ..k.clone()
    };
    assert_eq!(go(&calm, &upset), (0, 0, 0));
    assert_eq!(go(&k, &in_exhibit()), (0, 0, 0));
    assert_eq!(
        go(
            &k,
            &Env {
                exhibit: false,
                ..upset
            }
        ),
        (0, 0, 0)
    );
}

#[test]
fn the_zap_counter_fires_below_1_and_draws_only_for_an_animal_with_a_chance_and_free() {
    let class = Class::default();
    let k = Keys {
        test_fence: 50,
        ..keys()
    };
    let (mut a, env) = (animal(), in_exhibit());
    let (_, rng) = once(&mut a, &k, &class, &env);
    assert!(
        drew(&rng, 1) && a.zap == class.zap - 1,
        "it acts at the first tick"
    );
    let mut rng = Rng::default();
    for _ in 0..class.zap {
        super::run(&mut a, &k, &class, &env, &mut rng);
    }
    assert!(drew(&rng, 1), "then every {} ticks", class.zap);
    // A negative counter fires every tick; a chance of 0 or a thirsty animal draws nothing.
    for (k, a) in [
        (keys(), animal()),
        (
            k.clone(),
            Animal {
                thirsty: true,
                ..animal()
            },
        ),
        (
            k.clone(),
            Animal {
                in_show: true,
                ..animal()
            },
        ),
    ] {
        let mut a = a;
        assert!(drew(&once(&mut a, &k, &class, &env).1, 0));
    }
    let mut a = Animal {
        zap: -7,
        ..animal()
    };
    assert!(drew(&once(&mut a, &k, &class, &env).1, 1));
    let off = Class { zap: 0, ..class };
    let mut a = animal();
    let mut rng = Rng::default();
    (0..3).for_each(|_| _ = super::run(&mut a, &k, &off, &env, &mut rng));
    assert!(drew(&rng, 3), "a period of 0 fires at every tick");
}

#[test]
fn the_checks_draw_in_the_original_order() {
    // Rampage (1 draw: the times are the wrong way round), captivity (2), bored (1), zap (1,
    // unless the bored check made it busy).
    let k = Keys {
        rampage_threshold: 0,
        rampage_chance: 100,
        rampage_min: 2,
        rampage_max: 1,
        dirt_chance: 100,
        drink: 10,
        chase: 10,
        test_fence: 100,
        ..keys()
    };
    let class = Class {
        ui_threshold: 75,
        dirt_chance_chance: 100,
        ..Class::default()
    };
    let env = Env {
        grass: true,
        ..in_exhibit()
    };
    for seed in 0..30 {
        let mut a = Animal {
            happiness: -60,
            ..animal()
        };
        a.counters[CAPTIVITY] = 0;
        a.counters[BORED] = 0;
        let mut rng = Rng::default();
        rng.seed(seed);
        let out = super::run(&mut a, &k, &class, &env, &mut rng);
        assert!(out.trample);
        // The bored check read the fourth number; a thirsty animal is busy and does not zap.
        let mut check = Rng::default();
        check.seed(seed);
        let fourth = (0..4).map(|_| check.rand()).last().unwrap() as i32;
        assert_eq!(a.thirsty, fourth % 20 < 10, "seed {seed}");
        assert!(
            drew_since(&rng, seed, if a.thirsty { 4 } else { 5 }),
            "seed {seed}"
        );
    }
}

#[test]
fn the_reset_interval_runs_the_creation_again_when_it_ends() {
    let (k, env) = (keys(), in_exhibit());
    let class = Class {
        reset_interval: 3,
        ..Class::default()
    };
    let mut rng = Rng::default();
    let mut a = Animal::create(&k, &class, "f", &mut rng);
    assert_eq!(a.reset, 3);
    let before = rng.clone();
    a.happiness = 10;
    a.thirsty = true;
    for left in [2, 1] {
        super::run(&mut a, &k, &class, &env, &mut rng);
        assert_eq!(
            (a.reset, a.happiness, rng.clone()),
            (left, 10, before.clone())
        );
    }
    super::run(&mut a, &k, &class, &env, &mut rng);
    assert_eq!((a.reset, a.happiness, a.thirsty), (3, 60, false));
    let mut check = before;
    (0..16).for_each(|_| _ = check.rand());
    assert_eq!(rng, check, "sixteen draws, none else");
    // A young animal draws its sex again.
    let young = Env {
        young: true,
        ..env.clone()
    };
    let mut a = Animal {
        reset: 1,
        ..animal()
    };
    let mut rng = Rng::default();
    super::run(&mut a, &k, &class, &young, &mut rng);
    // The new counters may fire at once and draw for their checks.
    assert!((17..20).any(|n| drew(&rng, n)));
    // 0 never resets, and a huge interval does not either; 1 resets at every tick.
    for (interval, resets) in [(0, 0), (i32::MAX, 0), (1, 5)] {
        // Checks that fire draw too: their periods are long.
        let class = Class {
            reset_interval: interval,
            periods: [30_000; COUNTERS],
            ..Class::default()
        };
        let mut rng = Rng::default();
        let mut a = Animal::create(&k, &class, "f", &mut rng);
        let start = rng.clone();
        (0..5).for_each(|_| _ = super::run(&mut a, &k, &class, &env, &mut rng));
        let mut check = start;
        (0..16 * resets).for_each(|_| _ = check.rand());
        assert_eq!(rng, check, "interval {interval}");
    }
}

#[test]
fn the_end_of_the_checks_asks_for_the_ambient_choice_unless_none_ran() {
    let (k, class, env) = (keys(), Class::default(), in_exhibit());
    let mut a = animal();
    run(&mut a, &k, &class, &env);
    assert!(a.ambient);
    for stopped in [
        Animal {
            crated: true,
            ..animal()
        },
        Animal {
            egg: true,
            ..animal()
        },
        Animal {
            dying: true,
            ..animal()
        },
    ] {
        let mut a = stopped;
        run(&mut a, &k, &class, &env);
        assert!(!a.ambient);
    }
}

#[test]
fn the_new_keys_read_their_defaults_and_the_dirt_chance_falls_back_to_the_class() {
    let class = Class::default();
    assert_eq!(
        (
            class.zap,
            class.ui_threshold,
            class.dirt_chance_chance,
            class.dirt_chance
        ),
        (5, 50, 50, 10)
    );
    let k = keys();
    assert_eq!(
        (
            k.energy_threshold,
            k.hunger_threshold,
            k.drink,
            k.chase,
            k.other_angry,
            k.dirt_chance
        ),
        (70, 100, 0, 0, -30, 10)
    );
    let file = config("[Characteristics/Integers]\ncUIThreshold=75\ncDirtChance=5\ncZapCheck=0\n");
    let int = |key: &str, fallback| {
        let value = file.section("Characteristics/Integers").unwrap().get(key);
        value.and_then(to_int).unwrap_or(fallback)
    };
    let class = Class::read(&int);
    assert_eq!(
        (class.ui_threshold, class.dirt_chance, class.zap),
        (75, 5, 0)
    );
    assert_eq!(Keys::read(&|_, f| f, &class).dirt_chance, 5);
    assert_eq!(
        Keys::read(&|k, f| if k == "cDirtChance" { 0 } else { f }, &class).dirt_chance,
        0
    );
}

/// Runs `world` for `ms` milliseconds of 67 ms steps from seed 1 (the class keys of its units
/// are used as they are); the log lines of the animals and the simulation.
fn steps(world: &mut World, cat: &Catalogue, ms: u32) -> (Vec<String>, Sim) {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(world, cat);
    let mut log = Vec::new();
    for _ in 0..ms / 67 {
        sim.step_in(world, cat, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
    }
    (log, sim)
}

fn advice(world: &World, id: u32) -> u64 {
    let Part::Animal(a) = &world.units.get(id).unwrap().part else {
        panic!("not an animal");
    };
    a.advice
}

#[test]
fn a_herd_below_the_threshold_tramples_the_grass_it_stands_on() {
    // The grass pen rates 80 against a preference of 100: the zebras lose 10 every third tick
    // and reach -51 within a minute; each captivity tick then tramples the tile under a zebra.
    let cat = zebras_with("cHabitatPreference=100\ncDirtChance=100", "");
    let run = || {
        let mut world = pens();
        world.units.class = Class {
            ui_threshold: 75,
            dirt_chance_chance: 100,
            ..Class::default()
        };
        let (log, sim) = steps(&mut world, &cat, 300_000);
        (log, world, sim)
    };
    let (log, world, sim) = run();
    let trampled: Vec<usize> = (0..world.zoo.tiles.len())
        .filter(|&t| world.zoo.tiles[t].terrain == 16)
        .collect();
    // The zebras 500 and 501 stand on tiles (1, 2) and (2, 2) of the grass pen.
    assert_eq!(trampled, [2 * 8 + 1, 2 * 8 + 2]);
    let lines: Vec<&String> = log.iter().filter(|l| l.starts_with("trample ")).collect();
    assert!(lines.iter().any(|l| l.starts_with("trample 500 1,2")) && lines.len() >= 2);
    assert!(
        lines.iter().all(|l| !l.starts_with("trample 51")),
        "the bare pen is not grass"
    );
    // The exhibit was rated again: the ground it has left is worth less.
    let pen = world.exhibits().region(2 * 8 + 1).unwrap();
    assert!(
        world
            .exhibits()
            .get(pen)
            .unwrap()
            .rating
            .records
            .iter()
            .all(|r| r.terrain < 100.0)
    );
    // A second run agrees, and so does the state hash.
    let (again, other, sim2) = run();
    let money = crate::money::Money::default();
    assert!(
        log == again
            && crate::sim::hash::state(&sim, &world, &money)
                == crate::sim::hash::state(&sim2, &other, &money)
    );
}

#[test]
fn a_species_that_does_not_trample_leaves_the_grass_alone() {
    let cat = zebras_with("cDirtChance=0\ncHabitatPreference=100", "");
    let mut world = pens();
    world.units.class.ui_threshold = 75;
    let (log, _) = steps(&mut world, &cat, 200_000);
    assert!(log.iter().all(|l| !l.starts_with("trample ")));
    assert!(world.zoo.tiles.iter().all(|t| t.terrain != 16));
}

#[test]
fn the_social_counts_take_in_the_amphibious_neighbours() {
    // Each pen holds two adult zebras; with a limit of 3 per room of 100 tiles a pen alone is
    // not too full, but linked to the other it counts four adults.
    let cat = zebras_with("cNumberAnimalsMax=3", "");
    let advice_of = |linked: bool| {
        let mut world = pens();
        let ids: Vec<u32> = world.exhibits().list().iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), 2);
        if linked {
            world.exhibits_mut().list_mut()[0].amphibious = vec![ids[1]];
        }
        steps(&mut world, &cat, 5_000);
        advice(&world, 500) >> MANY & 1
    };
    assert_eq!((advice_of(false), advice_of(true)), (0, 1));
}

#[test]
fn the_census_counts_species_and_unhappy_animals_in_the_exhibit_and_adults_around_it() {
    let mut world = pens();
    // 510 belongs to the other pen; make 500 very unhappy.
    let Part::Animal(a) = &mut world.units.get_mut(500).unwrap().part else {
        panic!("not an animal");
    };
    a.very_unhappy = true;
    let ids: Vec<u32> = world.exhibits().list().iter().map(|e| e.id).collect();
    let alone = census(&world, ids[0], &ids[..1], "zebra");
    // The subtype `f` is an adult.
    assert_eq!(
        alone,
        Census {
            species: 2,
            adults: 2,
            all_adults: 2,
            unhappy: 1,
            sick: 0
        }
    );
    let both = census(&world, ids[0], &ids, "zebra");
    assert_eq!(
        (both.species, both.adults, both.all_adults, both.unhappy),
        (2, 4, 4, 1)
    );
    assert_eq!(census(&world, ids[0], &ids, "lion").adults, 0);
}

#[test]
fn the_links_and_the_other_animals_sum_are_part_of_the_state_hash() {
    let hash = |world: &World| {
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        crate::sim::hash::state(&sim, world, &crate::money::Money::default())
    };
    let mut world = pens();
    let base = hash(&world);
    world.exhibits_mut().list_mut()[0].amphibious.push(2);
    let linked = hash(&world);
    assert_ne!(linked, base);
    world.exhibits_mut().list_mut()[0].rating.records = vec![Record::default()];
    let rated = hash(&world);
    world.exhibits_mut().list_mut()[0].rating.records[0].other = 1.0;
    assert_ne!(hash(&world), rated);
}

#[test]
fn an_incompatible_species_in_the_exhibit_sets_bit_10_and_lowers_the_happiness() {
    // The zebras dislike the type with name id 2 by 300: a tenth of it, -30, at each habitat tick.
    let mut cat = zebras_with("", "[cCompatibleAnimals]\nv=2\nv=-300\n");
    cat.push(EntityType {
        class: "animals",
        type_name: b"lion".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(config(
            "[f/Characteristics/Integers]\ncCaptivity=1\n",
        ))),
        name_id: 2,
        footprint: [1, 1, 1],
        height: 0,
    });
    let mut world = pens();
    world
        .zoo
        .entities
        .iter_mut()
        .find(|e| e.common.as_ref().is_some_and(|c| c.id == 501))
        .unwrap()
        .type_name = "lion".into();
    steps(&mut world, &cat, 20_000);
    assert_eq!(advice(&world, 500) >> INCOMPATIBLE & 1, 1);
    let Part::Animal(a) = &world.units.get(500).unwrap().part else {
        panic!("not an animal");
    };
    assert_eq!(a.important >> INCOMPATIBLE & 1, 1);
    // The lion takes the habitat change of a species that likes no other: no bit.
    assert_eq!(advice(&world, 501) >> INCOMPATIBLE & 1, 0);
}

#[test]
fn zero_negative_and_huge_values_of_the_new_keys_never_panic() {
    let sets = crate::animal::life::Sets {
        has_sleep: true,
        has_sick: true,
        has_die: true,
        ..crate::animal::life::Sets::default()
    };
    let env = Env {
        grass: true,
        sick_near: 3,
        sets,
        ..in_exhibit()
    };
    for v in [0, -1, i32::MIN, i32::MAX] {
        let k = Keys {
            energy_threshold: v,
            hunger_threshold: v,
            drink: v,
            chase: v,
            other_angry: v,
            rampage_threshold: v,
            rampage_chance: v,
            test_fence: v,
            dirt_chance: v,
            energy_increment: v,
            dirty_increment: v,
            dirty_threshold: v,
            pct_hits: v,
            sick_change: v,
            other_sick_change: v,
            sick_chance: v,
            sick_random: v,
            sick_time: v,
            time_death: v,
            death_chance: v,
            building_use: v,
            ..keys()
        };
        let class = Class {
            death_chance: v,
            zap: v,
            ui_threshold: v,
            dirt_chance_chance: v,
            reset_interval: v,
            ..Class::default()
        };
        let mut rng = Rng::default();
        let mut a = Animal::create(&k, &class, "f", &mut rng);
        a.happiness = -100;
        for _ in 0..100 {
            a.counters = [0; COUNTERS];
            super::run(&mut a, &k, &class, &env, &mut rng);
            a.update(&k);
        }
    }
    // A sum of two chances that wraps is negative: no draw. A negative drink chance draws.
    let go = |drink, chase| {
        let k = Keys {
            drink,
            chase,
            ..keys()
        };
        let mut a = animal();
        a.counters[BORED] = 0;
        let mut rng = Rng::default();
        super::run(&mut a, &k, &Class::default(), &in_exhibit(), &mut rng);
        (a.thirsty, drew(&rng, 1))
    };
    assert_eq!(go(i32::MAX, i32::MAX), (false, false));
    assert_eq!(go(i32::MAX, 0), (true, true));
    assert_eq!(go(-5, 10), (false, true));
}

// Hunger and eating (animals-needs 4, 5).

/// The zebra of animals-needs 16.1.
fn hungry_zebra() -> Keys {
    let int = |key: &str, fallback: i32| match key {
        "cHungerIncrement" => 3,
        "cHungerThreshold" => 50,
        "cNoFoodChange" => -50,
        "cHungryHealthChange" => 10,
        "cKeeperFoodUnitsEaten" => 5,
        "cMaxHits" => 120,
        _ => fallback,
    };
    Keys::read(&int, &Class::default())
}

/// One status tick of a hunger check: the pending changes first, then the checks.
fn hunger_check(a: &mut Animal, k: &Keys, env: &Env) -> Out {
    a.update(k);
    a.counters[HUNGER] = 0;
    super::run(a, k, &Class::default(), env, &mut Rng::default())
}

#[test]
fn a_zebra_without_food_notes_it_once_then_complains_and_pays_at_the_maximum() {
    let (k, env) = (hungry_zebra(), Env::default());
    let mut a = Animal {
        health: 120,
        ..animal()
    };
    let mut messages = Vec::new();
    for n in 1..=36 {
        let out = hunger_check(&mut a, &k, &env);
        messages.push(out.messages.contains(&NO_FOOD));
        // Hunger 3 (n - 1) is tested before the check's own rise.
        assert_eq!(a.hunger, (3 * (n - 1)).min(100), "check {n}");
        let paid = (a.happiness_change, a.health_change);
        // From the second check the "hungry" bit is shown and important.
        assert_eq!(
            (a.advice & 1, a.important & 1),
            (u64::from(n > 18), u64::from(n > 18))
        );
        match n {
            18 => assert!(
                a.no_food && !messages[17],
                "the first hungry check only notes it"
            ),
            19..=34 => assert_eq!(paid, (0, 0), "check {n}: no penalty before 100"),
            35 | 36 => assert_eq!(paid, (-50, -10), "check {n}"),
            _ => assert!(!a.no_food && messages[n as usize - 1..].iter().all(|m| !m)),
        }
    }
    assert_eq!(
        messages.iter().filter(|&&m| m).count(),
        18,
        "checks 19 to 36"
    );
}

#[test]
fn a_zebra_in_a_show_without_food_is_not_told_but_pays_and_shows_the_bit() {
    let (k, env) = (hungry_zebra(), Env::default());
    let mut a = Animal {
        health: 120,
        hunger: 100,
        in_show: true,
        no_food: true,
        ..animal()
    };
    let out = hunger_check(&mut a, &k, &env);
    assert!(out.messages.is_empty());
    assert_eq!((a.happiness_change, a.health_change), (-50, -10));
    assert_eq!((a.advice & 1, a.important & 1), (1, 1));
}

#[test]
fn a_hungry_animal_that_finds_food_wants_it_and_forgets_its_thirst() {
    let k = hungry_zebra();
    let env = Env {
        food_total: 1,
        ..Env::default()
    };
    let mut a = Animal {
        hunger: 50,
        thirsty: true,
        ..animal()
    };
    let out = hunger_check(&mut a, &k, &env);
    assert!(a.wants_food && !a.thirsty && !a.no_food && out.messages.is_empty());
    // Not hungry enough, already on its way, or on the keeper food task: nothing.
    for (hunger, wants, task) in [(49, false, false), (50, true, false), (50, false, true)] {
        let mut a = Animal {
            hunger,
            wants_food: wants,
            ..animal()
        };
        let env = Env {
            food_total: 1,
            food_task: task,
            ..Env::default()
        };
        hunger_check(&mut a, &k, &env);
        assert_eq!((a.wants_food, a.no_food), (wants, false));
    }
    // Food with nothing left in total is no food.
    let mut a = Animal {
        hunger: 60,
        ..animal()
    };
    hunger_check(&mut a, &k, &Env::default());
    assert!(!a.wants_food && a.no_food);
}

#[test]
fn eating_takes_a_bite_per_tick_and_two_on_a_hunger_check_tick() {
    let k = hungry_zebra();
    let meal = Meal {
        left: 60,
        in_set: true,
        has_set: true,
        ..Meal::default()
    };
    let env = Env {
        meal,
        ..Env::default()
    };
    // The tick's bite: the pending hunger is the bite's worth.
    let mut a = Animal {
        hunger: 60,
        eating: true,
        ..animal()
    };
    let out = super::run(&mut a, &k, &Class::default(), &env, &mut Rng::default());
    assert_eq!((out.meal.left, a.hunger_change), (55, -5));
    // On a hunger check tick the food loses two bites, and the hunger change is set once: the
    // increment is gone (original bug, issue 316).
    let mut a = Animal {
        hunger: 60,
        eating: true,
        ..animal()
    };
    let out = hunger_check(&mut a, &k, &env);
    assert_eq!(
        (out.meal.left, out.meal.bites, a.hunger_change),
        (50, 10, -5)
    );
    assert!(out.meal.queue && !out.meal.start && !out.meal.stop);
    // At hunger 0 the animal stops, and the date is kept.
    let mut a = Animal {
        eating: true,
        wants_food: true,
        ..animal()
    };
    let at = Env {
        now: 777,
        ..env.clone()
    };
    let out = super::run(&mut a, &k, &Class::default(), &at, &mut Rng::default());
    assert!(!a.eating && out.meal.stop && a.fed == 777 && out.meal.bites == 0);
}

#[test]
fn eating_starts_the_eat_set_and_stops_on_no_food_water_or_a_bite_of_zero() {
    let k = hungry_zebra();
    let case = |m: Meal| {
        let (mut a, mut m) = (animal(), m);
        food::eat(&mut a, &k, &mut m);
        (a, m)
    };
    let meal = Meal {
        left: 3,
        has_set: true,
        ..Meal::default()
    };
    // The first bite starts the set; the bite is what is left.
    let (a, m) = case(meal.clone());
    assert!(!a.eating && m.start && m.stop && !m.queue && (m.left, m.bites) == (0, 3));
    assert_eq!(a.hunger_change, -3);
    // A bite that leaves some keeps the eater eating, and clears the hungry bit.
    let (mut a, mut m) = (
        animal(),
        Meal {
            left: 8,
            ..meal.clone()
        },
    );
    a.mark(0, true);
    food::eat(&mut a, &k, &mut m);
    assert!(a.eating && m.queue && !m.stop && (m.left, a.advice & 1) == (3, 0));
    // A type without the set eats nothing and does not stop.
    let (a, m) = case(Meal {
        has_set: false,
        ..meal.clone()
    });
    assert!(!a.eating && !m.stop && m.bites == 0);
    // No food, a negative count, or water: stop.
    for bad in [(0, false), (-9, false), (3, true)] {
        let (a, m) = case(Meal {
            left: bad.0,
            wet: bad.1,
            ..meal.clone()
        });
        assert!(!a.eating && m.stop && m.bites == 0 && !a.no_food);
    }
    // A bite of 0 or below eats nothing; the worth of a huge food is capped.
    let with = |key: &'static str, v: i32| {
        Keys::read(&|k, f| if k == key { v } else { f }, &Class::default())
    };
    for bite in [0, -5] {
        let (mut a, mut m) = (animal(), meal.clone());
        food::eat(&mut a, &with("cKeeperFoodUnitsEaten", bite), &mut m);
        assert!(m.stop && m.bites == 0);
    }
    let (mut a, mut m) = (
        animal(),
        Meal {
            left: i32::MAX,
            ..meal
        },
    );
    food::eat(&mut a, &with("cFoodUnitValue", i32::MAX), &mut m);
    assert_eq!((m.bites, a.hunger_change), (20, -i32::MAX));
}

#[test]
fn a_pile_shows_empty_half_or_full_by_thirds_of_its_full_amount() {
    use food::fullness;
    // 16.2: 39 of 60 is half, below 20 is empty; a full amount of 0 counts as 1.
    let at = |left| fullness(left, 60);
    assert_eq!([at(61), at(41), at(40), at(39), at(21)], [2, 2, 1, 1, 1]);
    assert_eq!([at(20), at(19), at(0), at(-5)], [1, 0, 0, 0]);
    assert_eq!([fullness(0, 0), fullness(1, 0), fullness(1, -4)], [0, 2, 2]);
    assert_eq!(fullness(i32::MAX, 1), 2);
}

#[test]
fn the_facing_towards_food_is_the_nearest_of_the_eight_directions() {
    // The original's arc tangent with its shortened constants.
    #[allow(clippy::approx_constant)] // the program's own, shorter constant
    let formula = |dx: i32, dy: i32| {
        let a = f64::from(dy).atan2(f64::from(dx));
        let i = ((a + 3.1415926535) * 0.15915494309644432 * 8.0 + 0.5) as u32;
        (i + 6) % 8
    };
    assert_eq!(food::nearest_direction(0, 0), None);
    for dx in -9..=9 {
        for dy in -9..=9 {
            if (dx, dy) != (0, 0) {
                assert_eq!(
                    food::nearest_direction(dx, dy),
                    Some(formula(dx, dy)),
                    "{dx} {dy}"
                );
            }
        }
    }
    assert_eq!(food::nearest_direction(i32::MIN, i32::MAX), Some(5));
}

/// Sets for the walk to the food and the meal.
struct Eats(std::collections::HashMap<&'static str, Vec<crate::behaviour::Call>>);

impl Eats {
    fn new() -> Eats {
        let call = |c: &str| crate::behaviour::parse_call(c.as_bytes()).unwrap().0;
        let sets = [
            ("bFindKeeperFood", vec!["fWalk(0,0)", "fPlaySet(bEat)"]),
            ("bEat", vec!["fFaceTowardFood()", "fPlay(stand)"]),
        ];
        let sets = sets.map(|(name, calls)| (name, calls.into_iter().map(call).collect()));
        Eats(sets.into_iter().collect())
    }
}

impl crate::behaviour::Data for Eats {
    fn set(&self, _: &Entity, name: &str) -> Option<&[crate::behaviour::Call]> {
        self.0.get(name).map(Vec::as_slice)
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &Entity, name: &str) -> Option<crate::behaviour::Timing> {
        ["idle", "walk", "stand"]
            .contains(&name)
            .then_some(crate::behaviour::Timing {
                count: 1,
                delay: 100,
            })
    }
    fn number(&self, _: &Entity, number: &crate::behaviour::Number) -> i32 {
        crate::unit::testing::rates(number)
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
}

/// The zebras that hunger slowly, and a food type of 100 units.
fn feeding() -> Catalogue {
    let mut cat = zebras_with("cHungerIncrement=1\ncHungerThreshold=50", "");
    cat.push(EntityType {
        class: "food",
        type_name: b"food".to_vec(),
        subtype: b"herbchow".to_vec(),
        file: None,
        config: Some(Arc::new(config(
            "[Characteristics/Integers]\ncMaxFoodUnits=100\n",
        ))),
        name_id: 2,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

/// One step of 67 ms with the sets of `data`: the `unit` and `animal` log lines.
fn step_log(run: &mut crate::sim::drive::Run, data: &dyn crate::behaviour::Data) -> Vec<String> {
    run.sim.step_game(
        data,
        &run.cat,
        &mut run.world,
        &mut run.money,
        67,
        |_, _, _| {},
    );
    let units = run
        .world
        .units
        .take_lines()
        .into_iter()
        .map(|l| format!("unit {l}"));
    let animals = run
        .world
        .units
        .take_animal_lines()
        .into_iter()
        .map(|l| format!("animal {l}"));
    units.chain(animals).collect()
}

#[test]
fn a_hungry_zebra_walks_to_the_food_a_script_puts_down_eats_it_and_the_pile_goes() {
    use crate::sim::drive::{Command, Run};
    let mut run = Run::new(
        (b"", b""),
        (feeding(), Class::default()),
        pens(),
        Default::default(),
        1,
    );
    let data = Eats::new();
    let mut log = step_log(&mut run, &data);
    // The zebra 500 is hungry; the pile of 45 units lies on the other side of its pen.
    let Part::Animal(a) = &mut run.world.units.get_mut(500).unwrap().part else {
        panic!("not an animal");
    };
    (a.hunger, a.counters[HUNGER]) = (60, 0);
    assert!(run.apply(&Command::Food(3, 2, "herbchow".into(), Some(45))));
    assert!(
        !run.apply(&Command::Food(3, 2, "nothing".into(), None)),
        "no such type"
    );
    assert!(
        !run.apply(&Command::Food(-1, 2, "herbchow".into(), None)),
        "off the map"
    );
    let pile = run
        .world
        .zoo
        .entities
        .last()
        .unwrap()
        .common
        .as_ref()
        .unwrap()
        .id;
    for _ in 0..40 * 15 {
        log.extend(step_log(&mut run, &data));
    }
    let lines = |what: &str| -> Vec<&str> {
        (log.iter().map(String::as_str))
            .filter(|l| l.contains(what))
            .collect()
    };
    let walk = lines("unit goal 500 keeperfood");
    assert_eq!(walk.len(), 1, "{walk:?}");
    assert_eq!(lines("unit set 500 bFindKeeperFood").len(), 1);
    // A short set is queued again after each bite until the food is gone.
    assert!(!lines("unit set 500 bEat").is_empty());
    let eats: Vec<_> = lines("animal eat 500")
        .into_iter()
        .map(|l| l.rsplitn(3, ' ').collect::<Vec<_>>())
        .collect();
    let left: Vec<_> = eats.iter().map(|e| e[1]).collect();
    let state: Vec<_> = eats.iter().map(|e| e[0]).collect();
    assert_eq!(left, ["25", "5", "0"], "bites of 20, 20 and what is left");
    assert_eq!(state, ["empty"; 3]);
    // The pile goes at the next removal pass, and the zebra stops: its goal is reset.
    assert!(run.world.find(pile).is_none());
    assert_eq!(lines("unit goal 500 none").len(), 1);
    let Part::Animal(a) = &run.world.units.get(500).unwrap().part else {
        panic!("not an animal");
    };
    assert!(!a.eating && !a.wants_food && a.hunger < 60);
    // The other zebra was never hungry.
    assert!(lines("animal eat 501").is_empty());
}

#[test]
fn the_bite_that_empties_a_pile_stops_every_animal_that_aims_at_it_at_once() {
    use crate::sim::drive::{Command, Run};
    let mut run = Run::new(
        (b"", b""),
        (feeding(), Class::default()),
        pens(),
        Default::default(),
        1,
    );
    let data = Eats::new();
    step_log(&mut run, &data);
    for id in [500, 501] {
        let Part::Animal(a) = &mut run.world.units.get_mut(id).unwrap().part else {
            panic!("not an animal");
        };
        (a.hunger, a.counters[HUNGER]) = (60, 0);
    }
    assert!(run.apply(&Command::Food(3, 2, "herbchow".into(), Some(45))));
    let mut last = Vec::new();
    for _ in 0..40 * 15 {
        let step = step_log(&mut run, &data);
        if step.iter().any(|l| l.ends_with(" 0 empty")) {
            last = step;
            break;
        }
    }
    // In the step of the bite, both animals reset their goal and are no longer eating.
    for id in [500, 501] {
        assert!(
            (last.iter()).any(|l| l.starts_with(&format!("unit goal {id} none "))),
            "{id}: {last:?}"
        );
        let Part::Animal(a) = &run.world.units.get(id).unwrap().part else {
            panic!("not an animal");
        };
        assert!(!a.eating);
    }
}

#[test]
fn the_keeper_food_goal_draws_before_looking_and_picks_by_them() {
    use food::{Piece, choose};
    let piece = |id, left, tile| Piece {
        id,
        at: id as usize,
        tile,
        left,
    };
    // Distances 9, 25 and 1; units left 50, 10 and 30. A neighbour holds piece 3.
    let home = vec![piece(0, 50, 3), piece(1, 10, 5), piece(2, 30, 1)];
    let groups = [home.clone(), vec![piece(3, 1, 0)]];
    let distance = |tile: usize| (tile * tile) as i64;
    let mut seen = [false; 3];
    for seed in 0..120 {
        let (mut rng, mut check) = (Rng::default(), Rng::default());
        rng.seed(seed);
        check.seed(seed);
        // A first draw not 0: the nearest, one draw. A first 0 and a second not 0: the least left.
        // Two 0s: a random one by a third draw.
        let (kind, expected, draws) = match check.rand() & 3 {
            0 if check.rand() & 3 != 0 => (1, 1, 2),
            0 => (2, home[check.rand() as usize % 3].id, 3),
            _ => (0, 2, 1),
        };
        assert_eq!(
            choose(&groups, distance, &mut rng).map(|p| p.id),
            Some(expected)
        );
        let mut reference = Rng::default();
        reference.seed(seed);
        (0..draws).for_each(|_| _ = reference.rand());
        assert_eq!(rng, reference, "seed {seed}");
        seen[kind] = true;
        // With no piece the first two draws are made, and a third only with a piece to pick.
        let mut empty = Rng::default();
        empty.seed(seed);
        assert_eq!(choose(&[vec![]], distance, &mut empty), None);
        let mut reference = Rng::default();
        reference.seed(seed);
        (0..draws.min(2)).for_each(|_| _ = reference.rand());
        assert_eq!(empty, reference, "empty, seed {seed}");
        // The neighbour is searched when the home exhibit has none.
        let mut near = Rng::default();
        near.seed(seed);
        let only = [vec![], vec![piece(3, 1, 0)]];
        assert_eq!(choose(&only, distance, &mut near).map(|p| p.id), Some(3));
    }
    assert_eq!(seen, [true; 3], "every way of choosing came up");
    assert_eq!(choose(&[], distance, &mut Rng::default()), None);
}
