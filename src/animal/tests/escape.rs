//! The escape check and the escaped cases of the checks (animals 11, animals-needs 5.3, 6, 7.2, 9).

use super::life::Life;
use super::*;
use crate::animal::life::Sets;

/// The checks with the counter `counter` at 0, on a fresh generator: the animal, the output and
/// the generator.
fn tick(mut a: Animal, counter: usize, k: &Keys, env: &Env) -> (Animal, Out, Rng) {
    a.counters[counter] = 0;
    let mut rng = Rng::default();
    let out = crate::animal::run(&mut a, k, &Class::default(), env, &mut rng);
    (a, out, rng)
}

fn grounds() -> Env {
    Env {
        exhibit: false,
        grounds: true,
        ..in_exhibit()
    }
}

#[test]
fn the_animal_on_the_grounds_escapes_when_its_counter_fires() {
    let k = keys();
    let (a, out, _) = tick(animal(), ESCAPE, &k, &grounds());
    assert!(a.escaped && out.log == ["escape"]);
    // Inside an exhibit, or before the counter fires, nothing happens.
    assert!(!tick(animal(), ESCAPE, &k, &in_exhibit()).0.escaped);
    let mut waiting = animal();
    waiting.counters[ESCAPE] = 1;
    let out = crate::animal::run(
        &mut waiting,
        &k,
        &Class::default(),
        &grounds(),
        &mut Rng::default(),
    );
    assert!(!waiting.escaped && out.log.is_empty());
    // An egg and an animal in a show run no escape check.
    for (egg, in_show) in [(true, false), (false, true)] {
        let a = Animal {
            egg,
            in_show,
            ..animal()
        };
        assert!(
            !tick(a, ESCAPE, &k, &grounds()).0.escaped,
            "{egg} {in_show}"
        );
    }
    // It escapes once.
    let a = Animal {
        escaped: true,
        ..animal()
    };
    assert!(tick(a, ESCAPE, &k, &grounds()).1.log.is_empty());
    // On the grounds it takes the land mode and drops its thirst; in an exhibit it is caught.
    let a = Animal {
        terrain: 1,
        thirsty: true,
        drinking: true,
        ..animal()
    };
    let a = tick(a, ESCAPE, &k, &grounds()).0;
    assert_eq!((a.terrain, a.thirsty, a.drinking), (0, false, false));
    let back = tick(a, ESCAPE, &k, &in_exhibit()).0;
    assert!(!back.escaped && back.terrain == 0);
}

#[test]
fn an_escaped_animal_on_dry_land_that_stands_starts_its_escaped_set() {
    let data = Life::new().with("bEscaped", "fPlay(idle)");
    let set = |prepare: fn(&mut Unit)| {
        let world = pens();
        let at = world.find(500).unwrap();
        let mut unit = world.units.get(500).unwrap().clone();
        if let Part::Animal(a) = &mut unit.part {
            a.escaped = true;
        }
        prepare(&mut unit);
        let mut rng = Rng::default();
        crate::animal::escape::escaped_set(&mut unit, &world, &data, &mut rng, at);
        let r = &unit.runner;
        (
            r.running().then(|| r.top()).flatten().map(str::to_string),
            r.queued().map(str::to_string),
        )
    };
    let escaped = Some("bEscaped".to_string());
    assert_eq!(set(|_| {}), (escaped.clone(), None));
    // A pending escaped set goes.
    assert_eq!(
        set(|u| _ = u.runner.queue("bEscaped", true, true)),
        (escaped, None)
    );
    // Moving, rampaging or not escaped, it starts nothing.
    assert_eq!(set(|u| u.moving = true), (None, None));
    fn change(u: &mut Unit, f: fn(&mut Animal)) {
        if let Part::Animal(a) = &mut u.part {
            f(a);
        }
    }
    assert_eq!(set(|u| change(u, |a| a.rampaging = true)), (None, None));
    assert_eq!(set(|u| change(u, |a| a.escaped = false)), (None, None));
}

#[test]
fn the_escaped_flag_and_the_escape_walk_are_saved_apart() {
    let (k, class) = (keys(), Class::default());
    let mut a = animal();
    assert_eq!(a.record(AnimalRecord::default()).escape, None);
    a.escaped = true;
    let r = a.record(AnimalRecord::default());
    assert!(r.escaped && r.escape.is_none());
    // A loaded walk record comes back with its flags read from their words.
    let saved = AnimalRecord {
        escape: Some(Escape {
            a: [1, 2],
            b: [3, 4],
            kind: 5,
            reached: 6,
            across: 1,
            broken: 8,
        }),
        ..record()
    };
    let mut b = animal();
    b.restore(&saved, &k, &class);
    assert!(!b.escaped);
    let walk = Escape {
        reached: 0,
        broken: 1,
        ..saved.escape.unwrap()
    };
    assert_eq!(b.walk, Some(walk));
    assert_eq!(b.record(saved.clone()).escape, Some(walk));
    b.restore(&record(), &k, &class);
    assert!(b.walk.is_none());
}

#[test]
fn an_escaped_animal_skips_the_tank_and_water_rules_and_the_land_draw() {
    let k = Keys {
        underwater: true,
        swims: true,
        enter_land: 100,
        enter_tank: 100,
        enter_water: 100,
        ..keys()
    };
    let kinds = |land, water, tank, stands| Env {
        kinds: [land, water, tank],
        stands,
        ..in_exhibit()
    };
    let check = |escaped, mode, env: &Env| {
        let mut a = Animal {
            escaped,
            terrain: mode,
            ..animal()
        };
        a.counters[WATER] = 0;
        let mut rng = Rng::default();
        let answer = super::super::tank::water(&mut a, &k, &Class::default(), env, &mut rng);
        let draws = (0..16).find(|&n| drew(&rng, n)).unwrap();
        (answer.map(|a| a.0), draws)
    };
    // On land the tank and the water rules do not run, with no draw.
    assert_eq!(check(false, 0, &kinds(50, 11, 11, 0)), (Some(2), 2));
    assert_eq!(check(true, 0, &kinds(50, 11, 11, 0)), (None, 0));
    // In the water the land rule draws no chance, the tank rule does not run.
    assert_eq!(check(false, 1, &kinds(2, 5, 11, 1)), (Some(0), 2));
    assert_eq!(check(true, 1, &kinds(2, 5, 11, 1)), (Some(0), 1));
    assert_eq!(check(true, 1, &kinds(1, 5, 11, 1)), (None, 0));
    // In a tank the same.
    assert_eq!(check(true, 2, &kinds(2, 5, 11, 2)), (Some(0), 1));
    assert_eq!(check(true, 2, &kinds(1, 5, 11, 2)), (None, 0));
}

#[test]
fn an_escaped_animal_stops_eating_gets_no_sleep_and_no_thirst_and_is_not_unwell() {
    let k = keys();
    // Eating: the bite is stopped.
    let food = Keys {
        bite: 4,
        ..k.clone()
    };
    let meal = crate::animal::food::Meal {
        left: 9,
        has_set: true,
        ..Default::default()
    };
    let mut m = meal.clone();
    let mut a = Animal {
        escaped: true,
        eating: true,
        ..animal()
    };
    crate::animal::food::eat(&mut a, &food, &mut m);
    assert!(!a.eating && m.stop && m.bites == 0 && m.left == 9);
    // Sleep: a tired escaped animal only tires.
    let sleepy = Env {
        sets: Sets {
            has_sleep: true,
            ..Sets::default()
        },
        ..grounds()
    };
    let tired = |escaped| Animal {
        escaped,
        energy: k.energy_threshold,
        ..animal()
    };
    let (a, out, _) = tick(tired(false), ENERGY, &k, &sleepy);
    assert!(a.sleeping && out.sets == ["bSleep"]);
    let (a, out, _) = tick(tired(true), ENERGY, &k, &sleepy);
    assert!(!a.sleeping && out.sets.is_empty() && a.energy_change == k.energy_increment);
    // Health: the low health of an escaped animal starts no set and costs no happiness.
    let unwell = Env {
        sets: Sets {
            has_sick: true,
            ..Sets::default()
        },
        ..in_exhibit()
    };
    let hurt = |escaped| Animal {
        escaped,
        health: 100,
        ..animal()
    };
    let (a, out, _) = tick(hurt(false), HEALTH, &k, &unwell);
    assert!(out.sets == ["bSick"] && a.happiness_change == -20);
    let (a, out, _) = tick(hurt(true), HEALTH, &k, &unwell);
    assert!(out.sets.is_empty() && a.happiness_change == 0 && a.advice == 0);
    // Thirst: the draw is made, the flag stays down.
    let thirst = Keys {
        drink: 10,
        chase: 0,
        ..k.clone()
    };
    let drink = |escaped| {
        let a = Animal {
            escaped,
            ..animal()
        };
        let (a, _, rng) = tick(a, BORED, &thirst, &in_exhibit());
        (a.thirsty, drew(&rng, 1))
    };
    assert_eq!(drink(false), (true, true));
    assert_eq!(drink(true), (false, true));
}

#[test]
fn a_loose_animal_is_found_on_the_grounds_and_the_pick_up_catches_it() {
    let cat = zebras();
    let mut world = pens_with(|n, _| match n {
        0 => vec![entity("animals", "f", (4, 6), 0, 700)],
        _ => vec![entity("animals", "f", (6, 2), 0, 710)],
    });
    let mut saved = record();
    saved.counters[ESCAPE] = Some(0);
    for e in &mut world.zoo.entities {
        if e.class == "animals" {
            e.common.as_mut().unwrap().animal = Some(Box::new(saved.clone()));
        }
    }
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    assert!(
        crate::guest::occupied(&world, &cat),
        "the pen holds an animal"
    );
    let mut log = Vec::new();
    for _ in 0..30 {
        sim.step_with(&Life::new(), &cat, &mut world, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
    }
    assert_eq!(log.iter().filter(|l| *l == "escape 700").count(), 1);
    assert!(!crate::guest::occupied(&world, &cat), "no guest arrives");
    let sums = crate::status::zoo::Sums::of(&world, &cat, 0.0, 0);
    assert_eq!(sums.loose, 1);
    // The saved record follows the flag.
    let mut zoo = world.saved(|_| true);
    world.finish_save(&mut zoo, &cat);
    let stored = zoo
        .entities
        .iter()
        .find_map(|e| e.common.as_ref().filter(|c| c.id == 700))
        .and_then(|c| c.animal.as_ref())
        .map(|a| (a.escaped, a.escape.is_some()));
    assert_eq!(stored, Some((true, false)));
    // Picking it up makes it a carried animal again, no longer loose.
    crate::place::unit::pick_up(&mut world, &cat, 700, 0).unwrap();
    assert!(crate::guest::occupied(&world, &cat));
    assert_eq!(crate::status::zoo::Sums::of(&world, &cat, 0.0, 0).loose, 0);
}

/// The thirst of animal 500, and with `escaped` its escape too, which its escape check in the pen
/// does not lower in time.
fn thirsty(w: &mut World, escaped: bool) {
    let Part::Animal(a) = &mut w.units.get_mut(500).unwrap().part else {
        panic!("not an animal");
    };
    (a.thirsty, a.escaped, a.counters[ESCAPE]) = (true, escaped, i32::MAX);
}

#[test]
fn an_escaped_animal_at_the_water_does_not_drink() {
    let drink = |prepare: fn(&mut World)| {
        let cat = zebras_with("cDrinkWaterChance=0", "");
        let mut world = pens();
        let t = 3 * usize::try_from(SIZE).unwrap() + 3;
        world.zoo.tiles[t].terrain = 3;
        world.set_water(vec![false, false, false, true]);
        let log = super::life::lived_with(&mut world, &cat, 60_000, prepare);
        let Part::Animal(a) = &world.units.get(500).unwrap().part else {
            panic!("not an animal");
        };
        (
            log.iter().any(|l| l == "set 500 bEat"),
            a.thirsty,
            a.drinking,
        )
    };
    assert_eq!(drink(|w| thirsty(w, false)), (true, false, false));
    assert_eq!(drink(|w| thirsty(w, true)), (false, false, false));
}
