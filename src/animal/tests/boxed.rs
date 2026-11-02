//! The crate's own check (unit placing 9.2) and a crate's ambient choice (behaviours 6.4).

use super::life::Life;
use super::*;
use crate::exhibit::Tank;

/// Zebra 500 crated in the first pen and zebra 700 crated on the grounds after the first step,
/// with `edit` applied then, run for 20 s: the log from the crating on, and the world.
fn crates(data: &Life, edit: impl Fn(&mut World)) -> (Vec<String>, World) {
    let (cat, mut world) = (
        zebras(),
        pens_with(|n, x0| match n {
            0 => vec![
                entity("animals", "f", (x0, 2), 0, 500),
                entity("animals", "f", (4, 6), 0, 700),
            ],
            _ => Vec::new(),
        }),
    );
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let mut log = Vec::new();
    for n in 0..20_000 / 67 {
        sim.step_with(data, &cat, &mut world, 67, |_, _, _| {});
        log.extend(world.units.take_animal_lines());
        log.extend(world.units.take_lines());
        if n == 0 {
            log.clear();
            for id in [500, 700] {
                if let Part::Animal(a) = &mut world.units.get_mut(id).unwrap().part {
                    a.crated = true;
                }
            }
            edit(&mut world);
        }
    }
    (log, world)
}

fn crated(world: &World, id: u32) -> bool {
    matches!(&world.units.get(id).unwrap().part, Part::Animal(a) if a.crated)
}

#[test]
fn a_crate_opens_in_an_exhibit_and_rattles_on_the_grounds_by_chance() {
    let data = Life::new().with("bRattle", "fPlay(idle)");
    let rattles = |chance: i32| {
        let (log, world) = crates(&data, |w| w.units.class.rattle = chance);
        assert!(log.iter().any(|l| l == "open 500"), "{chance}");
        assert!(!log.iter().any(|l| l == "open 700"), "{chance}");
        assert!(!crated(&world, 500) && crated(&world, 700));
        log.iter().any(|l| l == "set 700 bRattle")
    };
    assert_eq!(
        [0, -5, i32::MIN, 100, i32::MAX].map(rattles),
        [false, false, false, true, true]
    );
}

#[test]
fn a_crate_opens_in_a_tank_only_with_water() {
    let opens = |filled: bool, level: u32| {
        let (log, _) = crates(&Life::new(), |w| {
            let x = w.exhibits().region(2 * SIZE as usize + 1).unwrap();
            let tank = Tank {
                show: false,
                height: 0,
                level,
                salt: false,
                target: false,
                purity: 100,
                filled,
                platform: (-1, -1),
            };
            w.exhibits_mut().get_mut(x).unwrap().tank = Some(tank);
        });
        log.iter().any(|l| l == "open 500")
    };
    assert_eq!(
        [opens(true, 1), opens(true, 0), opens(false, 3)],
        [true, false, false]
    );
}

#[test]
fn a_crate_chooses_no_ambient_set() {
    let (log, _) = crates(&Life::new().idling(), |_| {});
    assert!(!log.iter().any(|l| l == "set 700 bIdle"));
    // Once open, the animal in the pen chooses again.
    assert!(log.iter().any(|l| l == "set 500 bIdle"));
}

#[test]
fn the_rattle_chance_is_the_class_key_or_ten() {
    let rattle = |v: Option<i32>| {
        let int = |key: &str, fallback| match (key, v) {
            ("cRattleChance", Some(v)) => v,
            _ => fallback,
        };
        Class::read(&int).rattle
    };
    assert_eq!(
        [None, Some(0), Some(-5), Some(i32::MAX)].map(rattle),
        [10, 0, -5, i32::MAX]
    );
}
