//! Sleep, dung, health, old age and the drink goal.

use super::*;
use crate::animal::life::Sets;
use crate::behaviour::{Call, Data, Heard, Number, Row, Timing, parse_call};

fn go(a: &mut Animal, k: &Keys, env: &Env, rng: &mut Rng) -> Out {
    crate::animal::run(a, k, &Class::default(), env, rng)
}

/// The checks with the counter `counter` at 0 so that it fires, on a fresh generator.
fn fire_one(a: &mut Animal, counter: usize, k: &Keys, env: &Env) -> Out {
    a.counters[counter] = 0;
    go(a, k, env, &mut Rng::default())
}

/// A healthy animal with every counter far from firing.
fn well() -> Animal {
    Animal {
        health: 500,
        ..animal()
    }
}

fn sleepy(has_sleep: bool, sleep_now: bool) -> Env {
    let sets = Sets {
        has_sleep,
        sleep_now,
        ..Sets::default()
    };
    Env {
        sets,
        now: 7,
        ..in_exhibit()
    }
}

#[test]
fn a_tired_animal_sleeps_and_a_rested_one_wakes() {
    let k = keys();
    let tired = || Animal {
        energy: k.energy_threshold,
        ..animal()
    };
    // Rested: the tiredness rises by the increment.
    let mut a = Animal {
        energy: 1,
        ..animal()
    };
    let out = fire_one(&mut a, ENERGY, &k, &sleepy(true, false));
    assert_eq!(
        (a.energy_change, a.sleeping, &out.sets[..]),
        (15, false, &[][..])
    );
    // Tired: the set starts and the tiredness does not rise.
    let mut a = tired();
    let out = fire_one(&mut a, ENERGY, &k, &sleepy(true, false));
    assert_eq!(
        (a.energy_change, a.sleeping, out.sets),
        (0, true, vec!["bSleep"])
    );
    // A type without the set only tires; so does the water.
    for env in [
        sleepy(false, false),
        Env {
            wet: true,
            ..sleepy(true, false)
        },
    ] {
        let mut a = tired();
        let out = fire_one(&mut a, ENERGY, &k, &env);
        assert_eq!(
            (a.energy_change, a.sleeping, out.sets.len()),
            (15, false, 0)
        );
    }
    // Asleep it checks at every call and stays while the set runs, then wakes with 0.
    let mut a = Animal {
        sleeping: true,
        ..tired()
    };
    go(&mut a, &k, &sleepy(true, true), &mut Rng::default());
    assert!(a.sleeping && a.energy == k.energy_threshold);
    go(&mut a, &k, &sleepy(true, false), &mut Rng::default());
    assert_eq!((a.sleeping, a.energy, a.slept), (false, 0, 7));
}

#[test]
fn a_sleeping_animal_does_not_look_for_food_and_the_bored_check_calls_the_sleep_check() {
    let k = Keys {
        energy_threshold: 50,
        ..keys()
    };
    let hungry = |sleeping| Animal {
        hunger: 100,
        sleeping,
        ..animal()
    };
    let env = Env {
        food_total: 10,
        ..sleepy(true, true)
    };
    for (sleeping, wants) in [(true, false), (false, true)] {
        let mut a = hungry(sleeping);
        fire_one(&mut a, HUNGER, &k, &env);
        assert_eq!(a.wants_food, wants);
    }
    // A tired animal at its bored check: the sleep check runs once there, and once more at its own
    // place in the order, which sees the set the first one asked for.
    let mut a = Animal {
        energy: 50,
        ..animal()
    };
    a.counters[BORED] = 0;
    let out = fire_one(&mut a, ENERGY, &k, &sleepy(true, false));
    assert_eq!((a.sleeping, out.sets), (true, vec!["bSleep"]));
}

#[test]
fn the_dung_meter_is_tested_before_its_rise_at_each_hunger_check() {
    let k = keys();
    for (dung, due) in [(69, false), (70, true)] {
        let mut a = Animal { dung, ..animal() };
        let out = fire_one(&mut a, HUNGER, &k, &in_exhibit());
        assert_eq!((out.dung, a.dung_change, a.dung), (due, 15, dung));
    }
    let mut a = Animal {
        dung: 99,
        ..animal()
    };
    assert!(!go(&mut a, &k, &in_exhibit(), &mut Rng::default()).dung);
}

#[test]
fn health_counts_the_starving_checks_and_takes_the_animal_away_when_no_keeper_works() {
    let k = Keys {
        sick_time: 3,
        ..keys()
    };
    let mut a = Animal {
        health: 0,
        hunger: 100,
        ..animal()
    };
    for n in 1..=2 {
        let out = fire_one(&mut a, HEALTH, &k, &in_exhibit());
        assert!(out.messages.contains(&VERY_SICK) && !out.remove, "{n}");
        assert_eq!(a.starving, n);
    }
    // A keeper at work: the animal stays, the count goes on.
    let kept = Env {
        keeper_working: true,
        ..in_exhibit()
    };
    let out = fire_one(&mut a.clone(), HEALTH, &k, &kept);
    assert!(!out.remove && !out.messages.contains(&TAKEN_AWAY));
    let out = fire_one(&mut a, HEALTH, &k, &in_exhibit());
    assert!(out.remove && !out.mourn && out.messages.contains(&TAKEN_AWAY));
    // Fed again: the count restarts.
    let mut a = Animal { hunger: 99, ..a };
    fire_one(&mut a, HEALTH, &k, &in_exhibit());
    assert_eq!(a.starving, 0);
}

#[test]
fn low_health_starts_the_sick_set_and_costs_happiness() {
    let k = keys();
    // 100 of 500 hit points are 20 percent.
    let env = |has_sick, sick_now| Env {
        sets: Sets {
            has_sick,
            sick_now,
            ..Sets::default()
        },
        ..in_exhibit()
    };
    for (env, set, change) in [
        (env(true, false), vec!["bSick"], -20),
        (env(true, true), vec![], -20),
        (env(false, false), vec![], 0),
    ] {
        let mut a = Animal {
            health: 100,
            ..animal()
        };
        let out = fire_one(&mut a, HEALTH, &k, &env);
        assert_eq!((out.sets, a.happiness_change), (set, change));
    }
    let mut a = Animal {
        health: 101,
        ..animal()
    };
    fire_one(&mut a, HEALTH, &k, &in_exhibit());
    assert_eq!((a.happiness_change, a.advice), (0, 0));
}

#[test]
fn a_sick_animal_in_the_exhibit_infects_by_two_draws_and_a_healthy_one_falls_ill_by_two() {
    let k = Keys {
        sick_chance: 100,
        sick_random: 100,
        ..keys()
    };
    let near = Env {
        sick_near: 1,
        ..in_exhibit()
    };
    // The company: the unhappiness and the bit; the first draw passes, the second (4 percent) is
    // searched for in the stream.
    let mut found = None;
    for seed in 0..200 {
        let (mut rng, mut a) = (Rng::default(), well());
        rng.seed(seed);
        a.counters[HEALTH] = 0;
        let out = go(&mut a, &k, &near, &mut rng);
        assert_eq!((a.happiness_change, a.advice & 0b100), (-10, 0b100));
        if a.at_minimum_health {
            found = Some((seed, out.messages, a.health));
            break;
        }
    }
    let (_, messages, health) = found.expect("one seed in 25 infects");
    assert_eq!((messages, health), (vec![CAUGHT_ILL], 0));
    // Alone, a sick animal draws nothing; a healthy one draws once or twice.
    let mut sick = Animal {
        health: 0,
        ..animal()
    };
    sick.counters[HEALTH] = 0;
    let mut rng = Rng::default();
    go(&mut sick, &k, &in_exhibit(), &mut rng);
    assert!(drew(&rng, 0));
    let mut a = well();
    a.counters[HEALTH] = 0;
    let mut rng = Rng::default();
    go(&mut a, &k, &in_exhibit(), &mut rng);
    assert!(drew(&rng, 1) || drew(&rng, 2));
    // An egg neither infects nor falls ill.
    let mut egg = Animal {
        egg: true,
        ..well()
    };
    egg.counters[HEALTH] = 0;
    let mut rng = Rng::default();
    go(&mut egg, &k, &near, &mut rng);
    assert!(drew(&rng, 0));
}

#[test]
fn old_age_counts_the_life_down_and_rolls_death_once_it_is_spent() {
    let k = Keys {
        death_chance: 100,
        ..keys()
    };
    let class = Class::default();
    let period = class.periods[REPRODUCTION];
    let mut a = Animal {
        life: period + 1,
        ..animal()
    };
    let mut rng = Rng::default();
    a.counters[REPRODUCTION] = 0;
    assert!(!go(&mut a, &k, &in_exhibit(), &mut rng).remove);
    assert_eq!((a.life, a.dying, drew(&rng, 0)), (1, false, true));
    // Spent: the roll comes at the next check. With a death set the animal starts it.
    a.counters[REPRODUCTION] = 0;
    let env = Env {
        sets: Sets {
            has_die: true,
            ..Sets::default()
        },
        ..in_exhibit()
    };
    let out = go(&mut a, &k, &env, &mut rng);
    assert_eq!(
        (a.dying, &out.sets[..], out.remove),
        (true, &["bDie"][..], false)
    );
    // While the set runs the animal stays; without it, it goes at once.
    let running = Env {
        sets: Sets {
            dying_now: true,
            ..env.sets
        },
        ..env.clone()
    };
    assert!(!go(&mut a, &k, &running, &mut rng).remove);
    assert!(go(&mut a, &k, &in_exhibit(), &mut rng).remove);
    let mut b = Animal {
        life: 0,
        ..animal()
    };
    b.counters[REPRODUCTION] = 0;
    let out = go(&mut b, &k, &in_exhibit(), &mut rng);
    assert!(out.remove && out.mourn && out.sets.is_empty());
    // A death chance of 0 never rolls death.
    let mut c = Animal {
        life: 0,
        ..animal()
    };
    c.counters[REPRODUCTION] = 0;
    let none = Keys {
        death_chance: 0,
        ..k
    };
    assert!(!go(&mut c, &none, &in_exhibit(), &mut rng).remove && !c.dying);
}

/// The sets of the animals of these tests.
struct Life(std::collections::HashMap<&'static str, Vec<Call>>);

impl Life {
    fn new() -> Life {
        let call = |c: &str| parse_call(c.as_bytes()).unwrap().0;
        let sets = [
            ("bDie", vec![call("fDie()")]),
            ("bFindOtherFood", vec![call("fWalk(0,0)")]),
            ("bEat", vec![call("fPlay(eat)")]),
        ];
        Life(sets.into_iter().collect())
    }
}

impl Data for Life {
    fn set(&self, _: &Entity, name: &str) -> Option<&[Call]> {
        self.0.get(name).map(Vec::as_slice)
    }
    fn ambient(&self, _: &Entity) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &Entity, _: &str) -> Option<Timing> {
        Some(Timing {
            count: 1,
            delay: 100,
        })
    }
    fn number(&self, _: &Entity, number: &Number) -> i32 {
        match number {
            Number::Characteristic(key) if key == "cSwims" => 0,
            _ => crate::unit::testing::rates(number),
        }
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<Heard> {
        None
    }
}

/// The pens of the zebras with the types of the dung.
fn dirty(keys: &str) -> Catalogue {
    let mut cat = zebras_with(keys, "");
    for name in ["poo", "dinopoo"] {
        cat.push(EntityType {
            class: "objects",
            type_name: b"other".to_vec(),
            subtype: name.as_bytes().to_vec(),
            file: None,
            config: None,
            name_id: 2,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// Runs `ms` milliseconds with the sets of [`Life`]; the animal and unit lines.
fn lived(world: &mut World, cat: &Catalogue, ms: u32) -> Vec<String> {
    lived_with(world, cat, ms, |_| {})
}

/// The same, with `prepare` run on the world after the first step, when the animals have started.
fn lived_with(world: &mut World, cat: &Catalogue, ms: u32, prepare: fn(&mut World)) -> Vec<String> {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(world, cat);
    let mut log = Vec::new();
    for n in 0..ms / 67 {
        sim.step_with(&Life::new(), cat, world, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
        log.extend(world.units.take_lines());
        if n == 0 {
            prepare(world);
        }
    }
    log
}

fn piles(world: &World) -> Vec<(String, i32)> {
    let dung = |e: &&Entity| e.class == "objects";
    let found = world.zoo.entities.iter().filter(dung);
    found
        .map(|e| (e.subtype.clone(), e.common.as_ref().unwrap().food))
        .collect()
}

#[test]
fn a_full_meter_puts_down_a_pile_of_the_size_of_the_species() {
    for (key, pile) in [
        ("", ("poo", 0)),
        ("cSmallZoodoo=1", ("poo", 1)),
        ("cGiantZoodoo=1\ncSmallZoodoo=1", ("poo", 2)),
        ("cDinoZoodoo=1\ncGiantZoodoo=1", ("dinopoo", 2)),
    ] {
        let cat = dirty(&format!("cDirtyThreshold=0\ncDirtyIncrement=1\n{key}"));
        let mut world = pens();
        let log = lived(&mut world, &cat, 200_000);
        assert!(log.iter().any(|l| l.starts_with("dung 500 1,2")), "{key}");
        let found = piles(&world);
        assert!(!found.is_empty() && found.iter().all(|p| *p == (pile.0.into(), pile.1)));
        // One pile to a tile: a second one waits for the quarter to be free.
        let tiles: Vec<_> = (world.zoo.entities.iter())
            .filter(|e| e.class == "objects")
            .map(|e| e.common.as_ref().map(|c| (c.x, c.y)))
            .collect();
        assert!(
            tiles
                .iter()
                .all(|t| tiles.iter().filter(|u| *u == t).count() == 1)
        );
    }
    // No type of dung in the data: nothing is put down and the meter stays full.
    let mut world = pens();
    lived(&mut world, &zebras_with("cDirtyThreshold=0", ""), 100_000);
    assert!(piles(&world).is_empty());
}

#[test]
fn a_starving_animal_with_no_keeper_is_taken_away_with_its_message() {
    let cat = zebras_with(
        "cHungerIncrement=100\ncHungerThreshold=0\ncHungryHealthChange=1000\ncSickTime=2",
        "",
    );
    let mut world = pens();
    let log = lived(&mut world, &cat, 600_000);
    assert!(log.iter().any(|l| l == "message 10010 500"));
    assert!(log.iter().any(|l| l == "taken 500") && log.iter().any(|l| l == "remove 500"));
    assert!(world.units.get(500).is_none());
}

#[test]
fn an_old_animal_dies_by_its_set_and_the_exhibit_mourns() {
    let cat = zebras_with("cTimeDeath=1\ncDeathChance=100\ncDeathChange=-30", "");
    let mut world = pens();
    world.units.class.death_change = -30;
    let log = lived(&mut world, &cat, 120_000);
    assert!(log.iter().any(|l| l == "dying 500"));
    assert!(log.iter().any(|l| l == "died 500"));
    assert!(world.units.get(500).is_none() && world.units.get(510).is_none());
}

#[test]
fn a_thirsty_animal_walks_to_the_water_of_its_exhibit_and_drinks() {
    let cat = zebras_with("cDrinkWaterChance=0", "");
    let mut world = pens();
    // The tile at (3, 3) of the grass pen is water; so is nothing else.
    let t = 3 * usize::try_from(SIZE).unwrap() + 3;
    world.zoo.tiles[t].terrain = 3;
    world.set_water(vec![false, false, false, true]);
    let thirsty = |w: &mut World| {
        let Part::Animal(a) = &mut w.units.get_mut(500).unwrap().part else {
            panic!("not an animal");
        };
        a.thirsty = true;
    };
    let log = lived_with(&mut world, &cat, 60_000, thirsty);
    let goals: Vec<&String> = log
        .iter()
        .filter(|l| l.starts_with("goal 500 drinkwater"))
        .collect();
    assert_eq!(goals.len(), 1);
    assert!(log.iter().any(|l| l == "set 500 bFindOtherFood"));
    assert!(log.iter().any(|l| l == "set 500 bEat"));
    let Part::Animal(a) = &world.units.get(500).unwrap().part else {
        panic!("not an animal");
    };
    assert!(!a.thirsty && !a.drinking, "the goal reset at the end");
}

#[test]
fn a_healed_animal_has_full_health_and_is_no_longer_sick() {
    let k = keys();
    let mut a = Animal {
        health: 0,
        at_minimum_health: true,
        ..animal()
    };
    a.mark(SICK, false);
    a.heal(&k);
    assert_eq!((a.health, a.at_minimum_health, a.advice), (500, false, 0));
}

#[test]
fn the_shelter_check_draws_once_when_the_species_has_a_chance() {
    for (chance, draws) in [(0, 0), (-5, 0), (i32::MAX, 1), (30, 1)] {
        let k = Keys {
            building_use: chance,
            ..keys()
        };
        let mut a = well();
        a.counters[SHELTER] = 0;
        let mut rng = Rng::default();
        go(&mut a, &k, &in_exhibit(), &mut rng);
        assert!(drew(&rng, draws), "{chance}");
    }
}

#[test]
fn zero_negative_and_huge_keys_never_stop_a_run() {
    for v in ["0", "-1", "2147483647", "-2147483648"] {
        let keys = [
            "cDirtyThreshold",
            "cDirtyIncrement",
            "cPooWaterImpact",
            "cSickTime",
        ]
        .into_iter()
        .chain(["cTimeDeath", "cDeathChance", "cEnergyIncrement", "cPctHits"])
        .map(|key| format!("{key}={v}\n"))
        .collect::<String>();
        let mut world = pens();
        lived(&mut world, &dirty(&keys), 90_000);
    }
}
