//! Mates, births, the young growing up and eggs.

use super::life::Life;
use super::*;
use crate::animal::breed::{self, Mate};

/// The checks with the birth of `born` (which says whether the young are made) and how often it
/// was asked for.
fn go(a: &mut Animal, k: &Keys, env: &Env, rng: &mut Rng, born: bool) -> (Out, u32) {
    let mut asked = 0;
    let out = crate::animal::run_with(a, k, &Class::default(), env, rng, &mut |_, _| {
        asked += 1;
        born
    });
    (out, asked)
}

/// A male that may breed at its next reproduction check: happy, ready, with a mate.
fn father() -> (Animal, Keys, Env) {
    let k = Keys {
        reproduction_chance: 100,
        reproduction_interval: 10,
        happy_reproduce: 50,
        baby_born_change: 7,
        ..keys()
    };
    let mut a = Animal {
        happiness: 80,
        interval: 10,
        mate: 9,
        ..animal()
    };
    a.counters[REPRODUCTION] = 0;
    let env = Env {
        adult: true,
        male: true,
        ..in_exhibit()
    };
    (a, k, env)
}

#[test]
fn a_male_in_good_spirits_that_is_ready_and_has_a_mate_rolls_for_young() {
    let (mut a, k, env) = father();
    let mut rng = Rng::default();
    let (out, asked) = go(&mut a, &k, &env, &mut rng, true);
    // One draw for the roll; the birth resets the interval and gladdens the father.
    assert!(drew(&rng, 1) && asked == 1);
    assert_eq!((a.interval, a.happiness_change), (0, 7));
    assert_eq!((out.pair, out.unpair), (None, None));
    // A failed birth (no tile for the young) changes nothing.
    let (mut a, k, env) = father();
    let (_, asked) = go(&mut a, &k, &env, &mut Rng::default(), false);
    assert_eq!((asked, a.interval, a.happiness_change), (1, 10, 0));
}

#[test]
fn each_condition_of_the_birth_is_needed_and_the_roll_is_drawn_only_when_all_hold() {
    type Spoil = fn(&mut Animal, &mut Keys, &mut Env);
    let cases: [(&str, Spoil); 8] = [
        ("sad", |a, _, _| a.happiness = 49),
        ("female", |_, _, e| e.male = false),
        ("not ready", |a, _, _| a.interval = 9),
        ("young", |_, _, e| e.adult = false),
        ("in a show", |a, _, _| a.in_show = true),
        ("show tank", |_, _, e| e.show_tank = true),
        ("dying", |a, _, _| a.dying = true),
        ("not due", |a, _, _| a.counters[REPRODUCTION] = 5),
    ];
    for (what, spoil) in cases {
        let (mut a, mut k, mut env) = father();
        spoil(&mut a, &mut k, &mut env);
        let mut rng = Rng::default();
        let (_, asked) = go(&mut a, &k, &env, &mut rng, true);
        assert!(asked == 0 && drew(&rng, 0), "{what}");
    }
    // The chance 0 draws and never succeeds.
    let (mut a, mut k, env) = father();
    k.reproduction_chance = 0;
    let mut rng = Rng::default();
    let (_, asked) = go(&mut a, &k, &env, &mut rng, true);
    assert!(asked == 0 && drew(&rng, 1) && a.interval == 10);
    // Happiness exactly at the threshold is enough.
    let (mut a, k, env) = father();
    a.happiness = k.happy_reproduce;
    assert_eq!(go(&mut a, &k, &env, &mut Rng::default(), true).1, 1);
}

#[test]
fn the_interval_counts_up_at_every_tick_and_stops_at_its_key() {
    let (mut a, k, env) = father();
    a.counters[REPRODUCTION] = 100;
    a.interval = 0;
    for _ in 0..25 {
        go(&mut a, &k, &env, &mut Rng::default(), true);
    }
    assert_eq!(a.interval, 10);
    // A key of 0 or below is always ready; a huge one never reaches.
    for (key, ready) in [(0, true), (-5, true), (i32::MAX, false)] {
        let (mut a, mut k, env) = father();
        (k.reproduction_interval, a.interval) = (key, 3);
        a.counters[REPRODUCTION] = 0;
        let (_, asked) = go(&mut a, &k, &env, &mut Rng::default(), true);
        assert_eq!(asked == 1, ready, "{key}");
    }
}

#[test]
fn an_adult_without_a_mate_picks_one_of_the_candidates_by_one_draw() {
    let (mut a, k, mut env) = father();
    a.mate = 0;
    a.advice = 1 << 7 | 1 << 15;
    env.male = false;
    env.candidates = vec![31, 32, 33];
    let mut rng = Rng::default();
    let want = Rng::default().pick(3) as usize;
    let (out, _) = go(&mut a, &k, &env, &mut rng, true);
    assert_eq!((a.mate, out.pair), (env.candidates[want], Some(a.mate)));
    assert!(drew(&rng, 1) && a.advice == 0, "the reason bits go");
    // No candidate: no draw, and only a monogamous animal says it has no mate.
    for (mating_type, bit) in [(0, 0), (1, 1 << 15)] {
        let (mut a, mut k, mut env) = father();
        (a.mate, env.male, k.mating_type) = (0, false, mating_type);
        let mut rng = Rng::default();
        go(&mut a, &k, &env, &mut rng, true);
        assert!(
            drew(&rng, 0) && a.mate == 0 && a.advice == bit,
            "{mating_type}"
        );
    }
    // On the grounds it looks for nothing.
    let (mut a, k, mut env) = father();
    (a.mate, env.exhibit, env.candidates) = (0, false, vec![5]);
    assert_eq!(go(&mut a, &k, &env, &mut Rng::default(), true).0.pair, None);
}

#[test]
fn a_mate_in_another_exhibit_is_given_up_and_one_that_is_gone_is_kept_as_lost() {
    // Away: both ids are cleared; a monogamous animal mourns, the others look for another.
    let (mut a, k, mut env) = father();
    (a.mate, env.mate, env.male) = (9, Mate::Away, false);
    env.candidates = vec![40];
    let out = go(&mut a, &k, &env, &mut Rng::default(), true).0;
    assert_eq!((a.mate, out.unpair, out.pair), (40, Some(9), Some(40)));
    // Gone: the id stays while it mourns.
    let k = Keys {
        mating_type: 1,
        no_mate_change: -9,
        ..k
    };
    let class = Class::default();
    let (full, delay) = (class.mate_pick_delay - 1, class.mate_pick_delay);
    let (mut a, _, mut env) = father();
    (a.mate, env.mate, env.male, a.mate_delay) = (9, Mate::Gone, false, full);
    env.candidates = vec![40];
    let mut rng = Rng::default();
    for check in 0..delay {
        a.counters[REPRODUCTION] = 0;
        let out = go(&mut a, &k, &env, &mut rng, true).0;
        let searched = check == delay - 1;
        assert_eq!(out.pair.is_some(), searched, "check {check}");
        // The first check after the loss pays once and shows the line.
        assert_eq!(
            a.happiness_change,
            if check == 0 { -9 } else { 0 },
            "{check}"
        );
        a.happiness_change = 0;
        assert!(a.advice & 1 << 7 != 0 || searched, "{check}");
    }
    assert_eq!((a.mate, a.mate_delay), (40, full));
    assert!(drew(&rng, 1) && a.advice & 1 << 7 == 0);
    // A mate that is not monogamous is replaced at once.
    let (mut a, k, mut env) = father();
    (a.mate, env.mate, env.male, env.candidates) = (9, Mate::Gone, false, vec![41]);
    assert_eq!(
        go(&mut a, &k, &env, &mut Rng::default(), true).0.pair,
        Some(41)
    );
    // With nobody to take, the lost id is cleared by the search all the same.
    env.candidates.clear();
    a.counters[REPRODUCTION] = 0;
    a.mate = 9;
    go(&mut a, &k, &env, &mut Rng::default(), true);
    assert_eq!(a.mate, 0);
}

#[test]
fn a_young_animal_grows_up_when_its_counter_ends() {
    let k = Keys {
        baby_to_adult: 3,
        ..keys()
    };
    let env = Env {
        baby: true,
        ..in_exhibit()
    };
    let mut a = Animal {
        baby: 2,
        ..animal()
    };
    let grown: Vec<bool> = (0..7)
        .map(|_| go(&mut a, &k, &env, &mut Rng::default(), false).0.grow)
        .collect();
    assert_eq!(grown, [false, false, true, false, false, true, false]);
    // An adult's counter runs and changes nothing; a key of 0 or below fires at every tick.
    let mut b = Animal {
        baby: 0,
        ..animal()
    };
    assert!(
        !go(&mut b, &k, &in_exhibit(), &mut Rng::default(), false)
            .0
            .grow
    );
    for key in [0, -3] {
        let k = Keys {
            baby_to_adult: key,
            ..k.clone()
        };
        let mut c = Animal {
            baby: 4,
            ..animal()
        };
        c.baby = 0;
        assert!(
            go(&mut c, &k, &env, &mut Rng::default(), false).0.grow,
            "{key}"
        );
    }
    // A dying young animal grows up too (the counter runs before the death test).
    let mut d = Animal {
        baby: 0,
        dying: true,
        ..animal()
    };
    assert!(go(&mut d, &k, &env, &mut Rng::default(), false).0.grow);
}

#[test]
fn an_egg_starts_with_the_health_and_time_of_the_species() {
    // The dinosaur egg of the specification: 500 hit points, 80 ticks, a keeper that calls
    // animals sickly at 90 percent.
    let k = Keys {
        max_hits: 500,
        time_to_hatch: 80,
        ..keys()
    };
    let mut a = animal();
    a.set_egg(&k, 10);
    assert_eq!((a.egg, a.hatch, a.health), (true, 80, 379));
    // The key of the least health, the maximum and the keeper's percentage cap it.
    for (min, sickly, health) in [
        (400, 10, 450),
        (0, 90, 50),
        (0, 150, 0),
        (0, -5, 379),
        (1000, 0, 500),
    ] {
        let k = Keys {
            min_hatch_hits: min,
            ..k.clone()
        };
        let mut a = animal();
        a.set_egg(&k, sickly);
        assert_eq!(a.health, health, "{min} {sickly}");
    }
    // Keys at the edge of the number range do not overflow.
    for big in [i32::MAX, i32::MIN] {
        let k = Keys {
            max_hits: big,
            time_to_hatch: big,
            min_hatch_hits: big,
            ..keys()
        };
        animal().set_egg(&k, i64::MAX);
    }
}

#[test]
fn an_egg_counts_down_while_it_keeps_its_health_and_hatches_inside_an_exhibit() {
    let k = Keys {
        max_hits: 500,
        time_to_hatch: 80,
        ..keys()
    };
    let sets = Sets {
        has_hatch: true,
        ..Sets::default()
    };
    let env = Env {
        sets,
        ..in_exhibit()
    };
    let tick = |a: &mut Animal, env: &Env| {
        a.update(&k);
        go(a, &k, env, &mut Rng::default(), false).0
    };
    // Untended it falls below 300 one tick before the time is out and never hatches.
    let mut egg = animal();
    egg.set_egg(&k, 10);
    let hatched = (0..200).any(|_| tick(&mut egg, &env).sets == ["bHatch"]);
    assert!(!hatched && egg.hatch == 1 && egg.health == 179);
    // Healed after tick 40, it hatches at tick 81: the countdown was 0 before that tick's step.
    let mut egg = animal();
    egg.set_egg(&k, 10);
    let mut at = 0;
    for n in 1..=100 {
        if n == 41 {
            egg.health = 500;
        }
        if tick(&mut egg, &env).sets == ["bHatch"] {
            at = n;
            break;
        }
    }
    assert_eq!((at, egg.health), (81, 459));
    // Not on the grounds, not while the set runs, not without the set; a dying egg is removed.
    for (what, env) in [
        (
            "grounds",
            Env {
                exhibit: false,
                ..env.clone()
            },
        ),
        (
            "running",
            Env {
                sets: Sets {
                    hatch_now: true,
                    ..sets
                },
                ..env.clone()
            },
        ),
        (
            "no set",
            Env {
                sets: Sets::default(),
                ..env.clone()
            },
        ),
    ] {
        let mut egg = Animal {
            egg: true,
            hatch: 0,
            health: 500,
            ..animal()
        };
        assert!(tick(&mut egg, &env).sets.is_empty(), "{what}");
    }
    let mut egg = Animal {
        egg: true,
        dying: true,
        health: 0,
        hatch: 0,
        ..animal()
    };
    let out = tick(&mut egg, &env);
    assert!(out.remove && out.sets.is_empty());
    // An egg that is crated still counts down (the egg check comes before the crate test).
    let mut egg = Animal {
        egg: true,
        crated: true,
        hatch: 5,
        health: 500,
        ..animal()
    };
    tick(&mut egg, &env);
    assert_eq!(egg.hatch, 4);
}

// A pen of zebras that breed: the types of the three subtypes share their keys.
fn breeders(keys: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    let text = format!(
        "[Characteristics/Integers]\n{keys}\ncInitialHappiness=100\ncHabitatPreference=60\n\
         cNumberAnimalsMin=1\ncNumberAnimalsMax=50\ncHabitatSize=100\ncAnimalDensity=1\ncCaptivity=0\n\
         cHungryHealthChange=0\ncSickRandomChance=0\ncHappyReproduceThreshold=-100\n\
         cReproductionChance=100\ncReproductionInterval=40\ncOffspring=2\ncBabyToAdult=100\n\
         cTimeDeath=100000\n[cCompatibleTerrain]\nv=0\nv=100\n"
    );
    let config = Arc::new(config(&text));
    for sub in ["f", "m", "y"] {
        cat.push(EntityType {
            class: "animals",
            type_name: b"zebra".to_vec(),
            subtype: sub.as_bytes().to_vec(),
            file: None,
            config: Some(config.clone()),
            name_id: 1,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// The first pen of [`pens`] with these animals in it, `(id, subtype)` each, on tile (2, 2).
fn pen(animals: &[(u32, &str)]) -> World {
    pens_with(|n, _| match n {
        0 => (animals.iter())
            .map(|&(id, sub)| entity("animals", sub, (2, 2), 0, id))
            .collect(),
        _ => Vec::new(),
    })
}

/// Runs `ms` milliseconds with the sets `data`; every animal line, and the unit lines.
fn lived(world: &mut World, cat: &Catalogue, data: &Life, ms: u32) -> Vec<String> {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(world, cat);
    let mut log = Vec::new();
    for _ in 0..ms / 67 {
        sim.step_with(data, cat, world, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
    }
    log
}

fn animals_of(world: &World) -> Vec<(u32, String, bool)> {
    let found = world.zoo.entities.iter().filter(|e| e.class == "animals");
    found
        .filter_map(|e| Some((e.common.as_ref()?.id, e.subtype.clone())))
        .map(|(id, sub)| match &world.units.get(id).unwrap().part {
            Part::Animal(a) => (id, sub, a.female),
            _ => unreachable!(),
        })
        .collect()
}

#[test]
fn a_pair_in_a_good_exhibit_has_young_that_grow_up_and_two_runs_agree() {
    let cat = breeders("");
    let run = || {
        let mut world = pen(&[(600, "m"), (601, "f")]);
        let log = lived(&mut world, &cat, &Life::new(), 400_000);
        (log, world)
    };
    let (log, world) = run();
    let line = |what: &str| {
        log.iter()
            .filter(|l| l.starts_with(what))
            .cloned()
            .collect::<Vec<_>>()
    };
    // They pair, the male gives birth to two young at a time, and the young pair up and grow.
    assert_eq!(line("mate 600 601").len() + line("mate 601 600").len(), 1);
    let births = line("birth 600 ");
    assert!(births.len() >= 2 && births[0].contains(" y "));
    assert!(log.iter().any(|l| l == "message 10032 600"));
    let grown = line("grow ");
    assert!(!grown.is_empty());
    // Every young animal stands inside the pen, with a name and its parents; the grown ones
    // are adults of their sex.
    let all = animals_of(&world);
    assert!(all.len() > 2);
    for (id, sub, female) in &all {
        let at = world.find(*id).unwrap();
        let c = world.zoo.entities[at].common.as_ref().unwrap();
        let tile = world
            .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
            .unwrap();
        assert!(
            world
                .exhibits()
                .region(tile)
                .is_some_and(|r| r != crate::exhibit::GROUNDS)
        );
        assert_ne!(sub, "", "{id}");
        if *id > 601 && sub != "y" {
            assert_eq!(sub == "f", *female, "{id} grew up as its sex");
        }
        assert!(*id < 602 || c.name.starts_with("Animal "), "{}", c.name);
    }
    assert!(all.iter().any(|(id, sub, _)| *id > 601 && sub != "y"));
    // A young animal has its parents' ids.
    let young = births[0].split(' ').nth(2).unwrap().parse::<u32>().unwrap();
    let Part::Animal(a) = &world.units.get(young).unwrap().part else {
        panic!("not an animal");
    };
    assert_eq!((a.father, a.mother), (600, 601));
    // The same seed, the same log.
    assert_eq!(log, run().0);
}

#[test]
fn no_birth_without_a_mate_or_with_a_sad_male() {
    // Two males never pair: a mate is of the other sex.
    let cat = breeders("");
    let mut world = pen(&[(600, "m"), (602, "m")]);
    let log = lived(&mut world, &cat, &Life::new(), 200_000);
    assert!(
        !log.iter()
            .any(|l| l.starts_with("mate ") || l.starts_with("birth "))
    );
    // A male that is never happy enough does not breed, though it has a mate.
    let cat = breeders("cHappyReproduceThreshold=101\n");
    let mut world = pen(&[(600, "m"), (601, "f")]);
    let log = lived(&mut world, &cat, &Life::new(), 200_000);
    assert!(
        log.iter().any(|l| l.starts_with("mate ")) && !log.iter().any(|l| l.starts_with("birth "))
    );
}

#[test]
fn the_young_fill_the_tiles_of_the_pen_four_to_a_tile_and_no_more() {
    let cat = breeders("cOffspring=1000\ncReproductionInterval=100000\n");
    let mut world = pen(&[(600, "m"), (601, "f")]);
    let pen = world
        .exhibits()
        .region(world.tile_index(2, 2).unwrap())
        .unwrap();
    let tiles = world.exhibits().get(pen).unwrap().tiles;
    let log = lived(&mut world, &cat, &Life::new(), 40_000);
    let born = log.iter().filter(|l| l.starts_with("birth ")).count();
    assert_eq!(born, 4 * tiles, "a unit slot each");
    assert_eq!(
        log.iter()
            .filter(|l| l.starts_with("message 10032 "))
            .count(),
        1
    );
}

#[test]
fn an_egg_laying_pair_lays_eggs_that_hatch_when_a_keeper_has_healed_them() {
    let keys =
        "cLaysEggs=1\ncTimeToHatch=10\ncMaxHits=100\ncOffspring=1\ncReproductionInterval=100000\n";
    let cat = breeders(keys);
    let data = Life::new().with("bHatch", "fHatch()");
    let run = || {
        let mut world = pen(&[(600, "m"), (601, "f")]);
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(64, Clocks::default());
        sim.load(&mut world, &cat);
        let mut log = Vec::new();
        let mut egg = None;
        for _ in 0..60_000 / 67 {
            sim.step_with(&data, &cat, &mut world, 67, |_, _, _| {});
            log.extend(world.units.take_animal_lines());
            // The keeper's heal, a minute after the egg was laid.
            let laid = log.iter().find_map(|l| l.strip_prefix("egg 600 "));
            if let (None, Some(l)) = (egg, laid) {
                egg = l.split(' ').next().and_then(|id| id.parse::<u32>().ok());
            }
        }
        (log, egg, world)
    };
    let (log, egg, world) = run();
    let egg = egg.expect("an egg was laid");
    let Part::Animal(a) = &world.units.get(egg).unwrap().part else {
        panic!("not an animal");
    };
    // One egg, never hatched: the pair breed only once, and an untended egg stays an egg.
    assert_eq!(log.iter().filter(|l| l.starts_with("egg ")).count(), 1);
    assert!(
        a.egg && !log.iter().any(|l| l.starts_with("hatch ")),
        "untended"
    );
    assert_eq!(world.units.get(egg).unwrap().animation.name, "egg_idle");
    // Healed at once, the same egg hatches: the animal is an adult-to-be with its picture.
    let mut world = pen(&[(600, "m"), (601, "f")]);
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let k = crate::animal::keys(&world, &cat, 0);
    let mut log = Vec::new();
    let mut hatched = None;
    for step in 0..60_000 / 67 {
        sim.step_with(&data, &cat, &mut world, 67, |_, _, _| {});
        let told = world.units.take_animal_lines();
        for l in &told {
            if let Some(id) = l.strip_prefix("hatch ") {
                hatched = Some((step, id.parse::<u32>().unwrap()));
            }
        }
        log.extend(told);
        for id in (log.iter())
            .filter_map(|l| l.strip_prefix("egg 600 "))
            .filter_map(|l| l.split(' ').next()?.parse::<u32>().ok())
        {
            if let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part)
                && a.egg
            {
                a.heal(&k);
            }
        }
    }
    let (_, id) = hatched.expect("the egg hatched");
    let Part::Animal(a) = &world.units.get(id).unwrap().part else {
        panic!("not an animal");
    };
    assert!(!a.egg);
    assert!(log.iter().any(|l| l == &format!("message 10322 {id}")));
    let unit = world.units.get(id).unwrap();
    assert_eq!(unit.animation.name, "idle");
}

#[test]
fn the_candidates_are_the_unmated_of_the_other_sex_that_are_not_family() {
    let mut world = pen(&[(600, "m"), (601, "f"), (602, "f"), (603, "f"), (604, "m")]);
    let cat = breeders("");
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    sim.step(&mut world, 67, |_, _, _| {});
    let x = world
        .exhibits()
        .region(world.tile_index(2, 2).unwrap())
        .unwrap();
    let set = |world: &mut World, id: u32, f: &dyn Fn(&mut Animal)| {
        if let Some(Part::Animal(a)) = world.units.get_mut(id).map(|u| &mut u.part) {
            f(a);
        }
    };
    let me = |world: &World| match &world.units.get(600).unwrap().part {
        Part::Animal(a) => a.clone(),
        _ => unreachable!(),
    };
    let at = world.find(600).unwrap();
    // Females are 601, 602, 603 in the order of the pen (the tiles, then the list).
    let mut a = me(&world);
    a.female = false;
    for id in [601, 602, 603, 604] {
        set(&mut world, id, &|b| b.female = id != 604);
    }
    assert_eq!(breed::candidates(&world, at, x, &a), [601, 602, 603]);
    // A taken female, a daughter and the mother are left out.
    set(&mut world, 601, &|b| b.mate = 77);
    set(&mut world, 602, &|b| b.father = 600);
    a.mother = 603;
    assert_eq!(breed::candidates(&world, at, x, &a), Vec::<u32>::new());
    // Another species is not a candidate, and nobody is a candidate of the grounds.
    assert_eq!(
        breed::candidates(&world, at, x + 100, &a),
        Vec::<u32>::new()
    );
}

#[test]
fn the_mate_of_an_animal_is_here_gone_or_away() {
    let mut world = pen(&[(600, "m"), (601, "f")]);
    let at = world.find(600).unwrap();
    assert_eq!(breed::mate_of(&world, at, 601), Mate::Here);
    assert_eq!(breed::mate_of(&world, at, 999), Mate::Gone);
    assert_eq!(breed::mate_of(&world, at, 0), Mate::Gone);
    // Out of the pen, on the grounds: still here; in the other pen: away.
    let other = world.find(601).unwrap();
    for (tile, mate) in [((0, 0), Mate::Here), ((6, 2), Mate::Away)] {
        let c = world.zoo.entities[other].common.as_mut().unwrap();
        (c.x, c.y) = (tile.0 * 64 + 32, tile.1 * 64 + 32);
        world.place_unit(other, (tile.0 * 64 + 32, tile.1 * 64 + 32, 0), 0);
        assert_eq!(breed::mate_of(&world, at, 601), mate, "{tile:?}");
    }
}

#[test]
fn the_reproduction_keys_default_to_the_program_and_survive_odd_values() {
    let k = keys();
    assert_eq!(
        (
            k.reproduction_chance,
            k.reproduction_interval,
            k.happy_reproduce,
            k.mating_type
        ),
        (10, 70, 0, 0)
    );
    assert_eq!(
        (
            k.no_mate_change,
            k.offspring,
            k.baby_born_change,
            k.baby_to_adult
        ),
        (-19, 1, 60, 100)
    );
    assert_eq!((k.lays_eggs, k.time_to_hatch, k.min_hatch_hits), (0, 0, 0));
    assert_eq!(Class::default().mate_pick_delay, 3);
    // The delay of the class file at 0, negative or huge never stops a mourning animal.
    for delay in [0, -5, i32::MAX, i32::MIN] {
        let class = Class {
            mate_pick_delay: delay,
            ..Class::default()
        };
        let k = Keys {
            mating_type: 1,
            ..keys()
        };
        let mut a = animal();
        let env = Env {
            adult: true,
            mate: Mate::Gone,
            candidates: vec![3],
            ..in_exhibit()
        };
        for _ in 0..5 {
            (a.mate, a.counters[REPRODUCTION]) = (9, 0);
            crate::animal::run(&mut a, &k, &class, &env, &mut Rng::default());
        }
    }
    // Zero, negative and huge values of every key never stop a run.
    for value in ["0", "-7", "2147483647", "-2147483648"] {
        let keys = [
            "cReproductionChance",
            "cReproductionInterval",
            "cHappyReproduceThreshold",
            "cMatingType",
            "cNoMateChange",
            "cOffspring",
            "cBabyBornChange",
            "cBabyToAdult",
            "cLaysEggs",
            "cTimeToHatch",
            "cMinHatchHits",
        ]
        .map(|key| format!("{key}={value}\n"))
        .concat();
        let cat = breeders(&format!("{keys}cMaxHits=100\n"));
        let mut world = pen(&[(600, "m"), (601, "f")]);
        lived(
            &mut world,
            &cat,
            &Life::new().with("bHatch", "fHatch()"),
            60_000,
        );
    }
}
