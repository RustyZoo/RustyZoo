//! The breath, the murky water and the water checks of the animals that live in tanks.

use super::*;
use crate::exhibit::Tank;

/// The checks on a fresh generator with the counter `counter` at 0 so that it fires.
fn tick(a: &mut Animal, counter: usize, k: &Keys, env: &Env) -> Out {
    a.counters[counter] = 0;
    crate::animal::run(a, k, &Class::default(), env, &mut Rng::default())
}

/// A tank of level 6 and purity `purity`, fresh or salt.
fn tank(purity: u32, salt: bool) -> Tank {
    Tank {
        height: 7,
        level: 6,
        purity,
        salt,
        filled: true,
        ..Tank::default()
    }
}

fn swimmer(tank_mode: u8, tank: Option<Tank>) -> Env {
    Env {
        tank_mode,
        tank,
        ..in_exhibit()
    }
}

#[test]
fn the_breath_rises_by_its_increment_under_water_up_to_100() {
    let k = Keys {
        breath_increment: 40,
        ..keys()
    };
    let class = Class::default();
    let mut a = animal();
    assert_eq!(tick(&mut a, BREATH, &k, &swimmer(4, None)).terrain, None);
    assert_eq!(
        (a.breath, a.counters[BREATH]),
        (40, class.periods[BREATH] - 1)
    );
    // Between run-outs nothing happens.
    run(&mut a, &k, &class, &swimmer(4, None));
    assert_eq!(a.breath, 40);
    for want in [80, 100, 100] {
        tick(&mut a, BREATH, &k, &swimmer(4, None));
        assert_eq!(a.breath, want);
    }
    // A species without an increment gains nothing, and a negative one cannot go below 0.
    let mut a = animal();
    tick(&mut a, BREATH, &keys(), &swimmer(4, None));
    assert_eq!(a.breath, 0);
    let k = Keys {
        breath_increment: -30,
        ..keys()
    };
    tick(&mut a, BREATH, &k, &swimmer(4, None));
    assert_eq!(a.breath, 0);
}

#[test]
fn a_breath_counter_that_runs_out_above_the_water_never_runs_again() {
    let k = Keys {
        breath_increment: 10,
        ..keys()
    };
    let mut a = animal();
    // At the surface (mode 3), on land (0), on the platform (2): the counter goes to -1 and falls.
    for mode in [3, 0, 2] {
        a.counters[BREATH] = 0;
        run(&mut a, &k, &Class::default(), &swimmer(mode, None));
        assert_eq!((a.breath, a.counters[BREATH]), (0, -1));
        for _ in 0..40 {
            run(&mut a, &k, &Class::default(), &swimmer(4, None));
        }
        assert_eq!((a.breath, a.counters[BREATH]), (0, -41));
        a.counters[BREATH] = 0;
    }
}

#[test]
fn murky_water_costs_happiness_and_health_by_the_band_of_the_purity() {
    let k = keys();
    let cost = |purity| {
        let mut a = animal();
        tick(&mut a, MURKY, &k, &swimmer(4, Some(tank(purity, false))));
        (a.happiness_change, a.health_change, a.counters[MURKY])
    };
    // The class period is 5: the counter restarts at 4.
    assert_eq!(cost(100), (0, 0, 4));
    assert_eq!(cost(60), (0, 0, 4));
    assert_eq!(cost(59), (-5, -5, 4));
    assert_eq!(cost(20), (-5, -5, 4));
    assert_eq!(cost(19), (-10, -10, 4));
    assert_eq!(cost(1), (-10, -10, 4));
    assert_eq!(cost(0), (-15, -15, 4));
    // The species' own bands.
    let k = Keys {
        murky: 30,
        very_murky: 10,
        extremely_murky: 5,
        murky_change: -1,
        very_murky_change: -2,
        extremely_murky_change: -3,
        murky_health: -4,
        very_murky_health: -5,
        extremely_murky_health: -6,
        ..keys()
    };
    let mut a = animal();
    tick(&mut a, MURKY, &k, &swimmer(4, Some(tank(7, false))));
    assert_eq!((a.happiness_change, a.health_change), (-2, -5));
    // Out of a tank, or off the counter, nothing.
    let mut a = animal();
    tick(&mut a, MURKY, &keys(), &swimmer(0, None));
    assert_eq!((a.happiness_change, a.health_change), (0, 0));
    run(
        &mut a,
        &keys(),
        &Class::default(),
        &swimmer(4, Some(tank(0, false))),
    );
    assert_eq!((a.happiness_change, a.health_change), (0, 0));
}

#[test]
fn the_depth_and_the_salinity_of_the_water_cost_happiness() {
    let (k, level) = (keys(), 6);
    let deep = Keys {
        depth_min: level + 1,
        depth_max: 29,
        depth_change: -7,
        ..keys()
    };
    let changes = |k: &Keys, tank: Tank, in_show: bool| {
        let mut a = Animal {
            in_show,
            ..animal()
        };
        tick(&mut a, MURKY, k, &swimmer(4, Some(tank)));
        (a.happiness_change, a.health_change)
    };
    // The level outside the species' depths costs happiness only, but not in a show tank, nor
    // for an animal in a show, nor when the two depths are the same.
    assert_eq!(changes(&deep, tank(100, false), false), (-7, 0));
    let show = Tank {
        show: true,
        ..tank(100, false)
    };
    assert_eq!(changes(&deep, show, false), (0, 0));
    assert_eq!(changes(&deep, tank(100, false), true), (0, 0));
    let flat = Keys {
        depth_max: deep.depth_min,
        ..deep.clone()
    };
    assert_eq!(changes(&flat, tank(100, false), false), (0, 0));
    let shallow = Keys {
        depth_min: 2,
        depth_max: level - 1,
        ..deep.clone()
    };
    assert_eq!(changes(&shallow, tank(100, false), false), (-7, 0));
    // The wrong kind of water: a salt-loving species in a fresh tank and the reverse.
    assert_eq!(changes(&k, tank(100, true), false), (-5, -5));
    assert_eq!(changes(&k, tank(100, false), false), (0, 0));
    let salt = Keys {
        salt: true,
        ..keys()
    };
    assert_eq!(changes(&salt, tank(100, false), false), (-5, -5));
    assert_eq!(changes(&salt, tank(100, true), false), (0, 0));
    // The bands add up.
    assert_eq!(
        changes(&deep, tank(10, true), false),
        (-10 - 7 - 5, -10 - 5)
    );
}

/// The water check of an animal in terrain mode `mode`: the answer and the number of draws it
/// made.
fn water(a: &mut Animal, k: &Keys, env: &Env, mode: u8) -> (Option<(u8, Option<usize>)>, usize) {
    a.terrain = mode;
    a.counters[WATER] = 0;
    let mut rng = Rng::default();
    let answer = super::super::tank::water(a, k, &Class::default(), env, &mut rng);
    let draws = (0..16).find(|&n| drew(&rng, n)).unwrap();
    (answer, draws)
}

fn tiles(land: usize, water: usize, tank: usize, stands: u8) -> Env {
    Env {
        kinds: [land, water, tank],
        stands,
        ..in_exhibit()
    }
}

#[test]
fn an_animal_on_land_goes_to_the_tank_mode_by_chance_when_there_are_enough_tank_tiles() {
    let tankish = Keys {
        underwater: true,
        enter_tank: 100,
        underwater_needed: 10,
        ..keys()
    };
    let mut a = animal();
    // 11 tank tiles are above 10: mode 2, and the animal on land is sent to the tile of the draw.
    let (answer, draws) = water(&mut a, &tankish, &tiles(50, 0, 11, 0), 0);
    assert_eq!(answer.map(|a| a.0), Some(2));
    assert_eq!(draws, 2, "the chance and the tile");
    assert!(answer.unwrap().1.is_some_and(|i| i < 11));
    // 10 are not: the chance is drawn all the same, and nothing follows.
    assert_eq!(water(&mut a, &tankish, &tiles(50, 0, 10, 0), 0), (None, 1));
    // No chance, no draw of a tile; a species that does not live in tanks draws nothing.
    let never = Keys {
        enter_tank: 0,
        ..tankish.clone()
    };
    assert_eq!(water(&mut a, &never, &tiles(50, 0, 11, 0), 0), (None, 1));
    assert_eq!(water(&mut a, &keys(), &tiles(50, 20, 11, 0), 0), (None, 0));
    // Already standing in a tank, or in no exhibit: no tile is drawn.
    let (answer, draws) = water(&mut a, &tankish, &tiles(50, 0, 11, 2), 0);
    assert_eq!((answer, draws), (Some((2, None)), 1));
    let grounds = Env {
        exhibit: false,
        ..tiles(50, 0, 11, 0)
    };
    assert_eq!(water(&mut a, &tankish, &grounds, 0), (Some((2, None)), 1));
}

#[test]
fn a_swimmer_on_land_may_take_to_the_water_when_the_tank_rule_fails() {
    let swims = Keys {
        swims: true,
        enter_water: 100,
        ..keys()
    };
    let mut a = animal();
    let (answer, draws) = water(&mut a, &swims, &tiles(50, 11, 0, 0), 0);
    assert_eq!((answer.map(|a| a.0), draws), (Some(1), 2));
    assert_eq!(water(&mut a, &swims, &tiles(50, 10, 0, 0), 0), (None, 1));
    // A diver that fails the tank rule (here, too few tiles) tries the water rule next.
    let both = Keys {
        underwater: true,
        enter_tank: 100,
        ..swims.clone()
    };
    let (answer, draws) = water(&mut a, &both, &tiles(50, 11, 3, 0), 0);
    assert_eq!((answer.map(|a| a.0), draws), (Some(1), 3));
    // A sick animal does not go in, and draws nothing; one that stands on water is not sent.
    a.at_minimum_health = true;
    assert_eq!(water(&mut a, &both, &tiles(50, 11, 11, 0), 0), (None, 0));
    a.at_minimum_health = false;
    let (answer, draws) = water(&mut a, &swims, &tiles(50, 11, 0, 1), 0);
    assert_eq!((answer, draws), (Some((1, None)), 1));
}

#[test]
fn an_animal_in_the_water_or_a_tank_comes_back_to_land_by_chance() {
    let k = Keys {
        underwater: true,
        swims: true,
        enter_land: 100,
        enter_tank: 100,
        ..keys()
    };
    let mut a = animal();
    // Tank mode: the land rule first; more than `cLandNeeded` (1) land tiles.
    let (answer, draws) = water(&mut a, &k, &tiles(2, 0, 11, 2), 2);
    assert_eq!((answer.map(|a| a.0), draws), (Some(0), 2));
    assert_eq!(water(&mut a, &k, &tiles(1, 0, 11, 2), 2).0, None);
    // Water mode: back to land the same way, else the tank rule.
    assert_eq!(
        water(&mut a, &k, &tiles(2, 5, 11, 1), 1).0.map(|a| a.0),
        Some(0)
    );
    assert_eq!(
        water(&mut a, &k, &tiles(1, 5, 11, 1), 1).0.map(|a| a.0),
        Some(2)
    );
    // A sick animal skips the draw of the land rule.
    a.at_minimum_health = true;
    let (answer, draws) = water(&mut a, &k, &tiles(2, 0, 11, 2), 2);
    assert_eq!(
        (answer.map(|a| a.0), draws),
        (Some(0), 1),
        "the tile of the walk only"
    );
    a.at_minimum_health = false;
    // A species that lives only under water never leaves the tank mode; an egg and an animal in a
    // show take no part.
    let only = Keys {
        only_underwater: true,
        ..k.clone()
    };
    assert_eq!(water(&mut a, &only, &tiles(2, 0, 11, 2), 2), (None, 0));
    a.egg = true;
    assert_eq!(water(&mut a, &k, &tiles(2, 0, 11, 2), 2), (None, 0));
    a.egg = false;
    a.in_show = true;
    assert_eq!(water(&mut a, &k, &tiles(2, 0, 11, 2), 2), (None, 0));
}

#[test]
fn the_water_check_runs_between_the_bored_and_the_shelter_checks_on_its_own_counter() {
    let k = Keys {
        underwater: true,
        enter_tank: 100,
        ..keys()
    };
    let mut a = animal();
    let out = tick(&mut a, WATER, &k, &tiles(50, 0, 11, 0));
    assert_eq!(out.terrain.map(|t| t.0), Some(2));
    assert_eq!(a.counters[WATER], Class::default().periods[WATER] - 1);
    // Between run-outs it asks for nothing.
    let out = crate::animal::run(
        &mut a,
        &k,
        &Class::default(),
        &tiles(50, 0, 11, 0),
        &mut Rng::default(),
    );
    assert_eq!(out.terrain, None);
}

fn species(text: &str) -> Keys {
    let ty = crate::entity::EntityType {
        class: "animals",
        type_name: b"seal".to_vec(),
        subtype: b"m".to_vec(),
        file: None,
        config: Some(Arc::new(config(text))),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    Keys::of(&ty, &Class::default())
}

#[test]
fn the_tank_keys_default_as_the_program_does_and_the_species_may_prefer_salt() {
    let k = species("[m/Characteristics/Integers]\ncCaptivity=1\n");
    assert_eq!((k.breath_threshold, k.breath_increment), (100, 0));
    assert_eq!((k.murky, k.very_murky, k.extremely_murky), (60, 20, 1));
    assert_eq!(
        (
            k.murky_change,
            k.very_murky_change,
            k.extremely_murky_change
        ),
        (-5, -10, -15)
    );
    assert_eq!((k.depth_min, k.depth_max, k.depth_change), (0, 0, -5));
    assert_eq!((k.salinity_change, k.salinity_health), (-5, -5));
    assert_eq!((k.enter_water, k.enter_tank, k.enter_land), (40, 0, 40));
    assert_eq!(
        (k.water_needed, k.underwater_needed, k.land_needed),
        (10, 10, 1)
    );
    assert!(!k.swims && !k.underwater && !k.only_underwater);
    // No wish for either water: salt (0 >= 0). Fresh water wanted more than salt: fresh.
    assert!(k.salt);
    let fresh = species("[cCompatibleTerrain]\nv=9\nv=40\nv=10\nv=30\n");
    assert!(!fresh.salt);
    let salty = species("[cCompatibleTerrain]\nv=9\nv=30\nv=10\nv=30\nv=2\nv=90\n");
    assert!(salty.salt);
    // The last pair that names a terrain counts.
    assert!(!species("[cCompatibleTerrain]\nv=10\nv=50\nv=9\nv=10\nv=9\nv=60\n").salt);
    // The type keys and the tank keys, read like any other key.
    let k = species(
        "[Characteristics/Integers]\ncSwims=1\ncUnderwater=1\ncBreathThreshold=20\ncBreathIncrement=10\ncEnterTankChance=75\n",
    );
    assert!(k.swims && k.underwater && !k.only_underwater && k.amphibious);
    assert_eq!(
        (k.breath_threshold, k.breath_increment, k.enter_tank),
        (20, 10, 75)
    );
}

/// Moves animal 1 of `w` to the terrain mode `mode`, sent to the tile number `pick` of that kind.
fn send(w: &mut World, data: &dyn Data, mode: u8, pick: usize) {
    let (events, mut removals) = (crate::sim::Events::default(), Vec::new());
    let mut rng = Rng::default();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &Default::default(),
    };
    let at = w.find(1).unwrap();
    let mut unit = std::mem::replace(
        w.units.get_mut(1).unwrap(),
        Unit::new(1, crate::unit::Kind::Animal),
    );
    super::super::tank::switch(&mut unit, w, &mut ctx, at, data, (mode, Some(pick)));
    *w.units.get_mut(1).unwrap() = unit;
}

/// The exhibit animal 1 stands in.
fn region(w: &World) -> u32 {
    let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
    let tile = w
        .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
        .unwrap();
    w.exhibits().region(tile).unwrap()
}

#[test]
fn an_amphibious_animal_sent_to_land_crosses_the_connector_and_comes_back() {
    use crate::unit::testing::{Walker, amphibious_world};
    let mut w = amphibious_world(2, &[(1, "polar", (4, 3))]);
    let u = w.units.get_mut(1).unwrap();
    (
        u.typed,
        u.started,
        u.mover.swims,
        u.mover.underwater,
        u.mover.depth,
    ) = (true, true, true, true, 1);
    u.goal.record.task = crate::goal::RANDOM;
    if let Part::Animal(a) = &mut u.part {
        (a.terrain, a.counters) = (2, [1000; COUNTERS]);
    }
    let tank = region(&w);
    let walker = Walker::new("fWalk(0,0)");
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(1200, crate::zoo::Clocks::default());
    // Sent to a tile of the land mode, the animal walks over the connector into the land exhibit;
    // sent back to a tile of the tank mode, it swims into the tank again.
    for mode in [0, 2] {
        send(&mut w, &walker, mode, 0);
        for _ in 0..1500 {
            sim.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
            if (region(&w) == tank) == (mode == 2) && w.units.get(1).unwrap().path.is_empty() {
                break;
            }
        }
        assert_eq!(region(&w) == tank, mode == 2, "mode {mode}");
        let Part::Animal(a) = &w.units.get(1).unwrap().part else {
            panic!("not an animal");
        };
        assert_eq!(a.terrain, mode);
    }
}

#[test]
fn the_tank_keys_at_their_edges() {
    // The breath threshold: missing is 100, 0 and below are always reached, a huge one never.
    let at = |threshold: Option<i32>, breath, mode| {
        let k = Keys {
            breath_threshold: threshold.unwrap_or(keys().breath_threshold),
            ..keys()
        };
        super::super::tank::needs_air(breath, &k, mode)
    };
    assert!(at(None, 100, 4) && !at(None, 99, 4) && !at(None, 100, 3));
    assert!(at(Some(0), 0, 4) && at(Some(-5), 0, 4) && !at(Some(0), 0, 0));
    assert!(!at(Some(i32::MAX), 100, 4));
    // A huge increment stops at 100, as a negative one does at 0.
    for (increment, want) in [(i32::MAX, 100), (i32::MIN, 0), (0, 0)] {
        let mut a = animal();
        let k = Keys {
            breath_increment: increment,
            ..keys()
        };
        tick(&mut a, BREATH, &k, &swimmer(4, None));
        assert_eq!(a.breath, want, "increment {increment}");
    }
    // The bands: a threshold of 0 or below is never reached, a huge one always; a huge change adds
    // up without overflowing.
    let cost = |threshold: i32, change: i32| {
        let k = Keys {
            murky: threshold,
            murky_change: change,
            murky_health: change,
            ..keys()
        };
        let mut a = animal();
        tick(&mut a, MURKY, &k, &swimmer(4, Some(tank(50, false))));
        (a.happiness_change, a.health_change)
    };
    assert_eq!(cost(0, -5), (0, 0));
    assert_eq!(cost(i32::MIN, -5), (0, 0));
    assert_eq!(cost(i32::MAX, -5), (-5, -5));
    assert_eq!(cost(i32::MAX, i32::MIN), (i32::MIN, i32::MIN));
    assert_eq!(cost(i32::MAX, 0), (0, 0));
    // The depths: a negative one is below any level, a huge maximum above all.
    let depth = |min: i32, max: i32| {
        let k = Keys {
            depth_min: min,
            depth_max: max,
            ..keys()
        };
        let mut a = animal();
        tick(&mut a, MURKY, &k, &swimmer(4, Some(tank(100, false))));
        a.happiness_change
    };
    assert_eq!(depth(-9, -1), -5, "the level 6 is above");
    assert_eq!(depth(i32::MAX, i32::MAX), 0, "equal depths ask for nothing");
    assert_eq!(depth(7, i32::MAX), -5);
    assert_eq!(depth(0, i32::MAX), 0);
    // The chances and the counts of the water check: 0 or below never, 100 or more always, a count
    // needs to be above the number, whatever it is.
    let goes = |chance: i32, needed: i32, count: usize| {
        let k = Keys {
            underwater: true,
            enter_tank: chance,
            underwater_needed: needed,
            ..keys()
        };
        let mut a = animal();
        water(&mut a, &k, &tiles(50, 0, count, 0), 0).0.is_some()
    };
    assert!(!goes(0, 0, 5) && !goes(-3, 0, 5) && !goes(i32::MIN, 0, 5));
    assert!(goes(100, 0, 5) && goes(i32::MAX, 0, 5));
    assert!(!goes(100, 5, 5) && goes(100, 4, 5) && goes(100, -1, 0) && !goes(100, i32::MAX, 5));
}
