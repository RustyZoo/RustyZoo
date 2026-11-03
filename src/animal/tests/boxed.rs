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

/// Where the entity `id` stands.
fn spot(world: &World, id: u32) -> (i32, i32) {
    let c = world.zoo.entities[world.find(id).unwrap()]
        .common
        .as_ref()
        .unwrap();
    (c.x, c.y)
}

#[test]
fn an_opened_crate_snaps_to_the_quarter_tile_centre() {
    let (_, world) = crates(&Life::new(), |w| {
        let at = w.find(500).unwrap();
        let c = w.zoo.entities[at].common.as_mut().unwrap();
        (c.x, c.y) = (c.x.div_euclid(64) * 64 + 41, c.y.div_euclid(64) * 64 + 9);
    });
    assert!(!crated(&world, 500));
    let (x, y) = spot(&world, 500);
    assert_eq!((x.rem_euclid(64), y.rem_euclid(64)), (48, 16));
}

/// Gives the animal at `at` the egg and crate footprints of its type, as the game's shapes do.
fn with_forms(world: &mut World, cat: &Catalogue, at: usize) {
    let mut shape = world.shape(at).unwrap();
    shape.forms = cat.find("animals", b"zebra", b"f").map(|t| t.forms());
    world.retype(at, "f", shape);
}

#[test]
fn snapping_takes_the_crate_footprint_while_crated() {
    let cat = zebras_with("cBoxFootprintX=4\ncBoxFootprintY=2", "");
    let mut world = pens_with(|n, x0| match n {
        0 => vec![entity("animals", "f", (x0, 2), 0, 500)],
        _ => Vec::new(),
    });
    let mut sim = Sim::new(b"");
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let at = world.find(500).unwrap();
    with_forms(&mut world, &cat, at);
    let mut snapped = |crated: bool| {
        let c = world.zoo.entities[at].common.as_mut().unwrap();
        (c.x, c.y) = (c.x.div_euclid(64) * 64 + 41, c.y.div_euclid(64) * 64 + 9);
        if let Part::Animal(a) = &mut world.units.get_mut(500).unwrap().part {
            a.crated = crated;
        }
        crate::animal::boxed::snap(&mut world, at);
        let (x, y) = spot(&world, 500);
        (x.rem_euclid(64), y.rem_euclid(64))
    };
    assert_eq!([snapped(true), snapped(false)], [(0, 32), (48, 16)]);
}

#[test]
fn an_opened_crate_snaps_with_its_footprint_turned_by_the_facing() {
    let cat = zebras_with(
        "cBoxFootprintX=4\ncBoxFootprintY=2\ncEggFootprintX=1\ncEggFootprintY=2",
        "",
    );
    let mut world = pens_with(|n, x0| match n {
        0 => vec![entity("animals", "f", (x0, 2), 0, 500)],
        _ => Vec::new(),
    });
    let mut sim = Sim::new(b"");
    sim.start_game(64, Clocks::default());
    sim.load(&mut world, &cat);
    let at = world.find(500).unwrap();
    with_forms(&mut world, &cat, at);
    let mut snapped = |crated: bool, rotation: u32| {
        let c = world.zoo.entities[at].common.as_mut().unwrap();
        (c.x, c.y) = (c.x.div_euclid(64) * 64 + 41, c.y.div_euclid(64) * 64 + 9);
        c.rotation = rotation;
        if let Part::Animal(a) = &mut world.units.get_mut(500).unwrap().part {
            (a.crated, a.egg) = (crated, !crated);
        }
        crate::animal::boxed::snap(&mut world, at);
        let (x, y) = spot(&world, 500);
        (x.rem_euclid(64), y.rem_euclid(64))
    };
    let egg = [0, 1, 2, 3, 4].map(|r| snapped(false, r));
    assert_eq!(egg, [(48, 32), (32, 16), (32, 16), (32, 16), (48, 32)]);
    assert_eq!([snapped(true, 0), snapped(true, 1)], [(0, 32), (32, 0)]);
}

#[test]
fn the_claim_and_the_dart_are_saved_with_the_animal_and_the_crate_drops_them() {
    let (k, class) = (keys(), Class::default());
    for (claim, tranq) in [(0, false), (7, true), (u32::MAX, false), (1, true)] {
        let saved = AnimalRecord {
            claim,
            tranq,
            ..record()
        };
        let mut a = animal();
        a.restore(&saved, &k, &class);
        assert_eq!((a.claim, a.tranq), (claim, tranq));
        let back = a.record(record());
        assert_eq!((back.claim, back.tranq), (claim, tranq));
    }
    let mut a = Animal {
        escaped: true,
        claim: 7,
        tranq: true,
        ..animal()
    };
    crate::animal::boxed::crate_up(&mut a);
    assert!(a.crated && !a.escaped && !a.tranq && a.claim == 0);
}

#[test]
fn a_darted_animal_chooses_no_ambient_set() {
    let (log, _) = crates(&Life::new().idling(), |w| {
        // The crate at 700 is open and darted instead.
        if let Part::Animal(a) = &mut w.units.get_mut(700).unwrap().part {
            (a.crated, a.tranq) = (false, true);
        }
    });
    assert!(!log.iter().any(|l| l == "set 700 bIdle"));
    assert!(log.iter().any(|l| l == "set 500 bIdle"));
}
