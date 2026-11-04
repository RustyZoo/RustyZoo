//! The dead state of a building and the forced leave it causes (guest buildings 11.1.1).

use super::*;
use crate::building::{Live, can_add};
use crate::config::Config;
use crate::exhibit::TankConfig;
use crate::unit::testing::{centre, entity, world_with};
use std::sync::Arc;

/// A shelter like the two underwater ones: dead on land, `cHeight` 6, not a surface object.
const CAVE: &str = "[Characteristics/Integers]\ncCapacity=3\ncHideUser=1\ncDeadOnLand=1\ncHeight=6\n\
    [Satisfies]\nbuilding\n[slots]\nname=a\n\
    [a]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=3\n";

const CONFIG: TankConfig = TankConfig {
    sink: [0, 0],
    height: [5, 9],
    level: 0,
    filled: false,
    salt: false,
    purity: 90,
    terrain: [0, 0],
    offset: -1,
    limit: 20,
    murky: 60,
    decay: 30_000,
    water: [1.0, 1.0],
    divisor: 5.0,
    ground: 8.0,
};

fn keys(on_land: bool, surface: bool, height: i32) -> Keys {
    Keys {
        on_land,
        surface,
        height,
        ..Keys::default()
    }
}

fn cave() -> EntityType {
    EntityType {
        class: "building",
        type_name: b"cave".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(CAVE.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    }
}

/// A tank of tiles (2, 2) to (4, 4) with a cave on (3, 3) and a guest 1 who uses it, filled to
/// the wall height 9 (level 8). Returns the world, the catalogue and the tank's id.
fn flooded() -> (World, Catalogue, u32) {
    let mut cat = Catalogue::default();
    cat.push(cave());
    let mut w = world_with((8, 8), |zoo| {
        let mut walls = Vec::new();
        for i in 2..=4 {
            walls.extend([(i, 2, 0), (i, 4, 4), (2, i, 6), (4, i, 2)]);
        }
        for (i, (x, y, d)) in walls.into_iter().enumerate() {
            let id = 100 + i as u32;
            zoo.entities
                .push(entity("tankwall", "", id, centre((x, y)), d));
        }
        zoo.entities
            .push(entity("building", "cave", 7, centre((3, 3)), 0));
        zoo.entities
            .push(entity("guests", "man", 1, centre((6, 6)), 0));
    });
    w.set_tank_config(CONFIG);
    w.set_paused(true);
    let id = w.tank_at(3 * 8 + 3).expect("a tank");
    w.set_height(id, 9);
    assert_eq!(w.fill(id), None);
    assert_eq!(w.tank(id).unwrap().level, 8);
    let ty = cat.types.last().unwrap();
    let live = Live {
        users: vec![(1, 0, 0)],
        ..Live::new(&Kind::of(ty, 0.0))
    };
    w.units.buildings.insert(7, live);
    w.units.get_mut(1).unwrap().hidden = true;
    w.take_wet();
    (w, cat, id)
}

#[test]
fn a_cave_is_dead_unless_the_water_covers_it() {
    let cave = keys(true, false, 6);
    let land = [false];
    assert!(is_dead(&cave, None, &land), "on land");
    assert!(is_dead(&cave, Some(0), &land), "in an empty tank");
    assert!(
        is_dead(&cave, Some(6), &land),
        "the water as high as the cave"
    );
    assert!(!is_dead(&cave, Some(7), &land));
    assert!(!is_dead(&cave, Some(u32::MAX), &land), "a huge level");
    assert!(
        is_dead(&keys(true, false, i32::MAX), Some(5), &land),
        "a huge cave"
    );
    assert!(
        !is_dead(&keys(true, false, -5), Some(0), &land),
        "a negative height"
    );
    assert!(!is_dead(&cave, None, &[]), "no tile at all");
    assert!(!is_dead(&cave, None, &[true, true]), "on water tiles");
    assert!(is_dead(&cave, None, &[true, false]), "one land tile");
}

#[test]
fn a_surface_object_is_dead_only_in_a_tank_with_no_water() {
    let toy = keys(true, true, 3);
    assert!(is_dead(&toy, Some(0), &[false]));
    assert!(!is_dead(&toy, Some(1), &[false]));
    assert!(is_dead(&toy, None, &[false]), "on land");
}

#[test]
fn the_other_two_keys_decide_by_the_ground_and_the_water() {
    let flat = Keys {
        on_flat_water: true,
        ..Keys::default()
    };
    assert!(is_dead(&flat, None, &[false, true]));
    assert!(!is_dead(&flat, None, &[false]));
    let under = Keys {
        underwater: true,
        ..Keys::default()
    };
    assert!(is_dead(&under, Some(1), &[false]));
    assert!(!is_dead(&under, Some(0), &[false]));
    assert!(!is_dead(&under, None, &[false]));
    assert!(
        !is_dead(&Keys::default(), Some(0), &[false, true]),
        "no key"
    );
}

#[test]
fn lowering_the_walls_puts_the_users_of_a_cave_out() {
    let (mut w, cat, tank) = flooded();
    decide_all(&mut w, &cat);
    assert!(!w.units.buildings[&7].dead, "covered by the water");
    // Enough to cover the cave, level 7 and above: nobody leaves.
    w.set_height(tank, 8);
    for id in w.take_wet() {
        tank_changed(&mut w, &cat, id);
    }
    assert_eq!(w.units.buildings[&7].users.len(), 1);
    w.units.posts.clear();
    w.set_height(tank, 7);
    assert_eq!(w.take_wet(), [tank]);
    tank_changed(&mut w, &cat, tank);
    let live = &w.units.buildings[&7];
    assert!(live.dead && live.users.is_empty());
    assert_eq!(w.units.posts.len(), 1, "one abort event for the user");
    assert!(!w.units.get(1).unwrap().hidden, "the user is shown again");
}

#[test]
fn a_dead_building_refuses_new_users() {
    let (mut w, cat, tank) = flooded();
    decide_all(&mut w, &cat);
    let (at, ty) = (w.find(7).unwrap(), cat.types.last().unwrap());
    let kind = Kind::of(ty, 0.0);
    let e = w.zoo.entities[at].clone();
    let open = |w: &World| can_add(w, &e, &kind, w.units.buildings.get(&7), 0, None);
    assert!(open(&w), "alive: room for two more users");
    w.set_height(tank, 3);
    tank_changed(&mut w, &cat, tank);
    assert!(!open(&w));
    // The water comes back: it accepts again.
    // The guest put out stands in the tank, so only the steps of the simulation raise it.
    w.set_height(tank, 9);
    w.set_paused(false);
    for _ in 0..8 {
        w.step_water();
    }
    assert_eq!(w.tank(tank).unwrap().level, 8);
    tank_changed(&mut w, &cat, tank);
    assert!(open(&w));
}

#[test]
fn a_water_step_below_the_cave_is_noticed() {
    let (mut w, cat, tank) = flooded();
    decide_all(&mut w, &cat);
    w.set_paused(false);
    assert!(w.drain(tank));
    assert!(w.step_water());
    assert_eq!(w.tank(tank).unwrap().level, 7);
    for id in w.take_wet() {
        tank_changed(&mut w, &cat, id);
    }
    assert!(!w.units.buildings[&7].dead);
    assert!(w.step_water());
    for id in w.take_wet() {
        tank_changed(&mut w, &cat, id);
    }
    assert!(w.units.buildings[&7].dead, "level 6 no longer covers it");
    assert!(w.units.buildings[&7].users.is_empty());
}

#[test]
fn a_loaded_dead_building_keeps_its_users_until_something_validates_it() {
    let (mut w, cat, tank) = flooded();
    w.set_height(tank, 3);
    w.take_wet();
    decide_all(&mut w, &cat);
    let live = &w.units.buildings[&7];
    assert!(live.dead, "found dead at the load");
    assert_eq!(live.users.len(), 1, "but nobody is put out");
    tank_changed(&mut w, &cat, tank);
    assert!(w.units.buildings[&7].users.is_empty());
}

#[test]
fn a_cave_on_the_ground_is_dead_when_it_is_placed() {
    let (mut w, cat, _) = flooded();
    let ty = cat.types.last().unwrap();
    let e = entity("building", "cave", 8, centre((6, 1)), 0);
    let shape = crate::unit::testing::shape(&e);
    let id = w.add(e, shape).expect("room for the cave");
    crate::building::built(&mut w, ty, id, 0);
    assert!(w.units.buildings[&id].dead);
}

#[test]
fn a_drained_tank_puts_the_users_of_a_cave_out_in_a_headless_run() {
    let (mut w, cat, tank) = flooded();
    decide_all(&mut w, &cat);
    w.set_paused(false);
    let mut sim = crate::sim::Sim::new(b"");
    assert!(w.drain(tank));
    for _ in 0..8 {
        sim.step_in(&mut w, &cat, 100, |_, _, _| {});
    }
    assert!(w.tank(tank).unwrap().level < 7);
    let live = &w.units.buildings[&7];
    assert!(live.dead && live.users.is_empty());
}

/// A change of tank `id`.
type Change = fn(&mut World, u32);

/// Guest 1 stands at tile `at`, then `change` moves the tank and the stirred tanks are told; the
/// guest's wait in milliseconds.
fn stirred(at: (i32, i32), change: Change) -> i32 {
    let (mut w, cat, id) = flooded();
    let index = w.find(1).unwrap();
    let c = w.zoo.entities[index].common.as_mut().unwrap();
    (c.x, c.y) = centre(at);
    // The guest uses no building, so that only the stir can reset it.
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    w.units.get_mut(1).unwrap().hidden = false;
    w.take_stirred();
    change(&mut w, id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    w.units.get(1).unwrap().wait
}

#[test]
fn a_change_of_the_water_the_walls_or_the_ground_resets_the_guests_in_the_tank() {
    // Guest buildings 12.2 point 22: the water level, the base level, the height, a terrain change.
    let changes: [(&str, Change); 5] = [
        ("level", |w, id| assert!(w.drain(id))),
        ("height", |w, id| w.set_height(id, 7)),
        ("base level", |w, id| w.flatten(id, 3, false)),
        ("terrain", |w, _| assert!(w.set_tile(3, 2, 0, 0, 1))),
        ("nothing", |_, _| ()),
    ];
    for (name, change) in changes {
        let waits = [stirred((2, 3), change), stirred((6, 6), change)];
        match name {
            "nothing" => assert_eq!(waits, [0, 0]),
            // The guest out of the tank is left alone.
            _ => assert!((1000..5000).contains(&waits[0]) && waits[1] == 0, "{name}"),
        }
    }
}
