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
    // The water only tires; a type without the set, or with it already current, gets nothing.
    for (env, change) in [
        (sleepy(false, false), 0),
        (sleepy(true, true), 0),
        (
            Env {
                wet: true,
                ..sleepy(true, false)
            },
            15,
        ),
    ] {
        let mut a = tired();
        let out = fire_one(&mut a, ENERGY, &k, &env);
        assert_eq!(
            (a.energy_change, a.sleeping, out.sets.len()),
            (change, false, 0)
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
    let env = |has_sick, sick_refused| Env {
        sets: Sets {
            has_sick,
            sick_refused,
            ..Sets::default()
        },
        ..in_exhibit()
    };
    for (env, set, change) in [
        (env(true, false), vec!["bSick"], -20),
        (env(true, true), vec![], 0),
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
fn the_sick_reason_bits_are_always_important() {
    let k = keys();
    let sick = Env {
        sets: Sets {
            has_sick: true,
            ..Sets::default()
        },
        sick_near: 1,
        ..in_exhibit()
    };
    let mut a = Animal {
        health: 100,
        ..animal()
    };
    fire_one(&mut a, HEALTH, &k, &sick);
    assert_eq!(a.advice & 0b110, 0b110);
    assert_eq!(a.important & 0b110, 0b110);
    // A sick set already current counts as not started: no unhappiness and no "sick" bit.
    let mut a = Animal {
        health: 100,
        ..animal()
    };
    let current = Env {
        sets: Sets {
            sick_refused: true,
            ..sick.sets
        },
        ..sick
    };
    fire_one(&mut a, HEALTH, &k, &current);
    assert_eq!((a.advice & 0b110, a.important & 0b110), (0b100, 0b100));
}

#[test]
fn the_sleep_counter_restarts_at_every_call_while_asleep() {
    let k = keys();
    let period = Class::default().periods[ENERGY];
    let mut a = Animal {
        sleeping: true,
        energy: k.energy_threshold,
        ..animal()
    };
    a.counters[ENERGY] = 5;
    go(&mut a, &k, &sleepy(true, true), &mut Rng::default());
    assert_eq!(a.counters[ENERGY], period - 1);
    // Awake it counts down.
    let mut b = Animal {
        energy: 1,
        ..animal()
    };
    b.counters[ENERGY] = 5;
    go(&mut b, &k, &sleepy(true, false), &mut Rng::default());
    assert_eq!(b.counters[ENERGY], 4);
    // Waking, it starts a full period.
    go(&mut a, &k, &sleepy(true, false), &mut Rng::default());
    assert_eq!((a.sleeping, a.counters[ENERGY]), (false, period - 1));
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
pub(super) struct Life(std::collections::HashMap<&'static str, Vec<Call>>, Vec<Row>);

impl Life {
    pub(super) fn new() -> Life {
        let call = |c: &str| parse_call(c.as_bytes()).unwrap().0;
        let sets = [
            ("bDie", vec![call("fDie()")]),
            ("bFindOtherFood", vec![call("fWalk(0,0)")]),
            ("bEat", vec![call("fPlay(eat)")]),
            ("bIdle", vec![call("fPlay(idle)")]),
        ];
        Life(sets.into_iter().collect(), Vec::new())
    }

    /// The same with a set of the given calls added.
    pub(super) fn with(mut self, name: &'static str, call: &str) -> Life {
        let call = parse_call(call.as_bytes()).unwrap().0;
        self.0.insert(name, vec![call]);
        self
    }

    /// The same with an ambient row for every happiness that starts `bIdle`.
    pub(super) fn idling(mut self) -> Life {
        let (lo, hi, set) = (i32::MIN, i32::MAX, "bIdle".to_string());
        self.1.push(Row { lo, hi, set });
        self
    }
}

impl Data for Life {
    fn set(&self, _: &Entity, name: &str) -> Option<&[Call]> {
        self.0.get(name).map(Vec::as_slice)
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[Row] {
        &self.1
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
pub(super) fn lived_with(
    world: &mut World,
    cat: &Catalogue,
    ms: u32,
    prepare: fn(&mut World),
) -> Vec<String> {
    lived_data(world, cat, &Life::new(), ms, prepare)
}

/// The same with other sets.
fn lived_data(
    world: &mut World,
    cat: &Catalogue,
    data: &Life,
    ms: u32,
    prepare: fn(&mut World),
) -> Vec<String> {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(world, cat);
    let mut log = Vec::new();
    for n in 0..ms / 67 {
        sim.step_with(data, cat, world, 67, |_, _, _| {});
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

/// Runs until the first pile of dung is put down; whether a worker was called for it, with the
/// tile of the call.
fn dung_call(mut world: World) -> (bool, Option<u32>) {
    let cat = dirty("cDirtyThreshold=0\ncDirtyIncrement=1");
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    for _ in 0..3000 {
        sim.step_with(&Life::new(), &cat, &mut world, 67, |_, _, _| {});
        if (world.units.take_animal_lines().iter()).any(|l| l.starts_with("dung")) {
            let [now, next] = sim.events.buffers();
            let call = now.iter().chain(next).find(|e| e.kind == 10032);
            return (true, call.map(|e| e.data));
        }
    }
    (false, None)
}

#[test]
fn a_pile_on_the_zoo_grounds_calls_a_worker_and_one_in_a_pen_does_not() {
    let pen = dung_call(pens());
    assert_eq!(pen, (true, None));
    let ground = pens_with(|n, _| match n {
        0 => vec![entity("animals", "f", (4, 5), 0, 500)],
        _ => Vec::new(),
    });
    assert_eq!(dung_call(ground), (true, Some(5 * 8 + 4)));
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

#[test]
fn a_tired_idle_animal_chooses_no_ambient_row() {
    let cat = zebras_with("cEnergyThreshold=50\ncEnergyIncrement=0", "");
    let tire = |w: &mut World| {
        let Part::Animal(a) = &mut w.units.get_mut(500).unwrap().part else {
            panic!("not an animal");
        };
        a.energy = 50;
    };
    let rows = |log: &[String]| log.iter().filter(|l| *l == "set 500 bIdle").count();
    // Tired (from the second step on), without a sleep set: the sleep check takes the choice and
    // starts nothing.
    let mut world = pens();
    let log = lived_data(&mut world, &cat, &Life::new().idling(), 3000, tire);
    assert!(rows(&log) <= 1);
    assert!(!log.iter().any(|l| l == "set 500 bSleep"));
    // Rested, it chooses its row as ever.
    let mut world = pens();
    let log = lived_data(&mut world, &cat, &Life::new().idling(), 3000, |_| {});
    assert!(rows(&log) > 5);
    // Tired with the sleep set, it sleeps.
    let mut world = pens();
    let data = Life::new().idling().with("bSleep", "fPlay(sleep)");
    let log = lived_data(&mut world, &cat, &data, 3000, tire);
    assert!(log.iter().any(|l| l == "set 500 bSleep"));
}

#[test]
fn a_low_health_animal_falls_ill_only_when_its_running_set_has_ended() {
    // The health check queues `bSick` (behaviours 4.2.1): the idle set of 30 plays is not cut.
    let cat = zebras_with("cPctHits=50\ncMaxHits=500", "");
    let weaken = |w: &mut World| {
        let Part::Animal(a) = &mut w.units.get_mut(500).unwrap().part else {
            panic!("not an animal");
        };
        a.health = 1;
        a.at_minimum_health = false;
    };
    let mut data = Life::new().idling().with("bSick", "fPlay(sick)");
    let play = parse_call(b"fPlay(idle)").unwrap().0;
    data.0.insert("bIdle", vec![play; 30]);
    let sick = |ms| {
        let log = lived_data(&mut pens(), &cat, &data, ms, weaken);
        log.iter().any(|l| l == "set 500 bSick")
    };
    assert!(!sick(11_000));
    assert!(sick(16_000));
}

#[test]
fn a_tired_animal_sleeps_only_when_its_running_set_has_ended() {
    // The sleep check queues `bSleep` (behaviours 4.2): the idle set of 30 plays of 100 ms is
    // not cut.
    let cat = zebras_with("cEnergyThreshold=50\ncEnergyIncrement=0", "");
    let tire = |w: &mut World| {
        let Part::Animal(a) = &mut w.units.get_mut(500).unwrap().part else {
            panic!("not an animal");
        };
        a.energy = 50;
    };
    let mut data = Life::new().idling().with("bSleep", "fPlay(sleep)");
    let play = parse_call(b"fPlay(idle)").unwrap().0;
    data.0.insert("bIdle", vec![play; 30]);
    let slept = |ms| {
        let log = lived_data(&mut pens(), &cat, &data, ms, tire);
        log.iter().any(|l| l == "set 500 bSleep")
    };
    assert!(!slept(2000));
    assert!(slept(6000));
}

#[test]
fn the_drink_goal_tries_the_shore_tiles_in_order() {
    let cat = zebras_with("cDrinkWaterChance=0", "");
    // Water at (2, 3) and (3, 3) of the first pen. The nearest water to the animal at (1, 2) is
    // (2, 3); its shore tiles are (2, 2) and (1, 3), both at distance 1 from the animal, then
    // (3, 3), which is water (no step into it for a land animal), and (2, 4), behind a fence.
    let width = usize::try_from(SIZE).unwrap();
    let mut world = pens();
    for x in [2, 3] {
        world.zoo.tiles[3 * width + x].terrain = 3;
    }
    world.set_water(vec![false, false, false, true]);
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let mut destination = None;
    for n in 0..300 {
        sim.step_with(&Life::new(), &cat, &mut world, 67, |_, _, _| {});
        let log = world.units.take_lines();
        if n == 0 {
            let Part::Animal(a) = &mut world.units.get_mut(500).unwrap().part else {
                panic!("not an animal");
            };
            a.thirsty = true;
        }
        if log.iter().any(|l| l == "set 500 bFindOtherFood") {
            destination = world.units.get(500).unwrap().destination;
            break;
        }
    }
    // The later direction comes first on a tie: the west tile (1, 3), not the north one (2, 2).
    assert_eq!(destination, Some((64 + 32, 3 * 64 + 32, 0)));
}

/// Loads the first pen's animals with `saved`, and with `set` as the saved behaviour set and
/// `goal` as the saved active goal; steps `ms` and returns the log and the animal 500 with its task.
fn loaded_with(saved: AnimalRecord, set: &str, goal: i32, ms: u32) -> (Vec<String>, Animal, u8) {
    loaded_with_active(saved, set, (goal, goal), ms)
}

/// The same with the wanted goal and the active one apart.
fn loaded_with_active(
    saved: AnimalRecord,
    set: &str,
    goals: (i32, i32),
    ms: u32,
) -> (Vec<String>, Animal, u8) {
    let cat = zebras_with("cEnergyThreshold=500", "");
    let mut world = pens();
    for e in &mut world.zoo.entities {
        if e.class == "animals" {
            let mut block = crate::zoo::UnitBlock::default();
            block.behaviour.levels[0] = crate::zoo::Level {
                name: set.into(),
                list_b: vec!["fPlayTime(sleep,4)".into(), "fPlay(sleep)".into()],
                tail: [1, 0, 1, 0],
                ..Default::default()
            };
            (block.target.goal, block.target.active) = goals;
            let c = e.common.as_mut().unwrap();
            (c.animal, c.unit) = (Some(Box::new(saved.clone())), Some(Box::new(block)));
        }
    }
    let data = Life::new().idling().with("bSleep", "fPlay(sleep)");
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let mut log = Vec::new();
    for _ in 0..ms / 67 {
        sim.step_with(&data, &cat, &mut world, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
        log.extend(world.units.take_lines());
    }
    let unit = world.units.get(500).unwrap();
    let Part::Animal(a) = &unit.part else {
        panic!("not an animal");
    };
    (log, a.clone(), unit.goal.record.task)
}

#[test]
fn a_loaded_sleeper_goes_on_with_its_saved_sleep_set() {
    let saved = AnimalRecord {
        sleeping: true,
        energy: 100,
        ..record()
    };
    let at = |log: &[String], line: &str| log.iter().position(|l| l == line);
    // The saved set is the sleep set: it goes on with its saved calls (4 s of sleep, not the
    // type's one call) without being started again, and the animal wakes when they are over.
    let (log, a, _) = loaded_with(saved.clone(), "bSleep", 1, 3000);
    assert!(a.sleeping && at(&log, "wake 500").is_none() && at(&log, "set 500 bSleep").is_none());
    let (log, a, _) = loaded_with(saved.clone(), "bSleep", 1, 9000);
    assert!(!a.sleeping && at(&log, "wake 500").is_some() && at(&log, "set 500 bSleep").is_none());
    // Any other saved set: nothing holds the flag, the first sleep check wakes the animal.
    let (log, a, _) = loaded_with(saved.clone(), "bChasePrey", 116, 3000);
    assert!(at(&log, "set 500 bSleep").is_none() && at(&log, "wake 500").is_some());
    assert!(!a.sleeping && a.energy == 0);
    // Not saved asleep, with the sleep set: nothing starts.
    let (log, a, _) = loaded_with(
        AnimalRecord {
            sleeping: false,
            ..saved
        },
        "bSleep",
        1,
        3000,
    );
    assert!(at(&log, "set 500 bSleep").is_none() && !a.sleeping);
}

#[test]
fn a_loaded_drinker_reads_its_drink_task_from_the_wanted_goal() {
    let saved = AnimalRecord {
        drinking: true,
        eating: true,
        ..record()
    };
    // Saved in the drink goal: the flags come back and the goal starts.
    let (_, a, task) = loaded_with(saved.clone(), "bEat", 122, 0);
    assert!(a.drinking && a.eating && task == 122);
    // Saved in another goal: the goal comes back as saved and the flag stays up, with no drink task
    // from it (animals-needs 6.1); so for a goal of 0, none, or a number beyond a byte.
    for goal in [114, 0, -1, i32::MAX] {
        let (_, a, task) = loaded_with(saved.clone(), "bEat", goal, 0);
        assert!(a.drinking && a.eating && task != 122, "goal {goal}");
    }
    let (_, _, task) = loaded_with(saved.clone(), "bEat", 114, 0);
    assert_eq!(task, 114);
    // Only the wanted goal counts: a drink saved as the active goal alone is not one.
    let (_, a, task) = loaded_with_active(saved.clone(), "bEat", (114, 122), 0);
    assert!(a.drinking && task == 114);
    // Not saved drinking, in the drink goal: the goal comes back, the flag stays down.
    let (_, a, task) = loaded_with(
        AnimalRecord {
            drinking: false,
            ..saved
        },
        "bEat",
        122,
        0,
    );
    assert!(!a.drinking && task == 122);
}
