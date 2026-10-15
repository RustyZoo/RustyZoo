//! The status values and the checks on synthetic values, and four animals in two exhibits.

use std::sync::Arc;

use super::*;
use crate::render::order::Kind;
use crate::sim::Sim;
use crate::world::{Shape, TankKind};
use crate::zoo::{Clocks, Common, Tile, View, Zoo};

fn keys() -> Keys {
    Keys::read(&|_, fallback| fallback, &Class::default())
}

/// An animal with every counter far from firing.
fn animal() -> Animal {
    Animal {
        counters: [1000; COUNTERS],
        crowd_direction: -1,
        ..Animal::default()
    }
}

/// Runs the checks with one counter at 0 so that it fires.
fn fires(a: &mut Animal, counter: usize, k: &Keys, env: &Env) -> Vec<i32> {
    a.counters[counter] = 0;
    run(a, k, &Class::default(), env)
}

fn in_exhibit() -> Env {
    let census = Census {
        species: 3,
        adults: 3,
        all_adults: 3,
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
            animal: None,
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
        tanks: Vec::new(),
        id_counter: 10_000,
        tiles: (0..SIZE * SIZE).map(|t| tile(t % SIZE)).collect(),
        entities,
        view: View::default(),
        clocks: Clocks::default(),
    };
    World::new(zoo, shape)
}

fn zebras() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "animals",
        type_name: b"zebra".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(Arc::new(config(
            "[f/Characteristics/Integers]\ncInitialHappiness=0\ncHabitatPreference=60\n\
             cNumberAnimalsMin=2\ncHabitatSize=100\ncAnimalDensity=1\ncCaptivity=1\n\
             [cCompatibleTerrain]\nv=0\nv=100\n",
        ))),
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
