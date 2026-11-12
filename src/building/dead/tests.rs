//! The dead state of a building and the forced leave it causes (guest buildings 11.1.1).

use super::*;
use crate::building::{Live, can_add};
use crate::config::Config;
use crate::exhibit::TankConfig;
use crate::unit::Part;
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
    typed("cave", CAVE)
}

fn typed(name: &str, config: &str) -> EntityType {
    EntityType {
        class: "building",
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(config.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    }
}

/// The tank walls of tiles (2, 2) to (4, 4), ids from 100.
fn walls(zoo: &mut crate::zoo::Zoo) {
    let mut walls = Vec::new();
    for i in 2..=4 {
        walls.extend([(i, 2, 0), (i, 4, 4), (2, i, 6), (4, i, 2)]);
    }
    for (i, (x, y, d)) in walls.into_iter().enumerate() {
        let id = 100 + i as u32;
        zoo.entities
            .push(entity("tankwall", "", id, centre((x, y)), d));
    }
}

/// A tank of tiles (2, 2) to (4, 4) with a cave on (3, 3), a guest 1 who uses it and a guest 2, filled to
/// the wall height 9 (level 8). Returns the world, the catalogue and the tank's id.
fn flooded() -> (World, Catalogue, u32) {
    flooded_with(Vec::new())
}

/// `flooded` with the entities `extra` after the guests.
fn flooded_with(extra: Vec<crate::zoo::Entity>) -> (World, Catalogue, u32) {
    let mut cat = Catalogue::default();
    cat.push(cave());
    let mut w = world_with((8, 8), |zoo| {
        walls(zoo);
        zoo.entities
            .push(entity("building", "cave", 7, centre((3, 3)), 0));
        // Guest 2 comes first in the entity list.
        zoo.entities
            .push(entity("guests", "man", 2, centre((6, 6)), 0));
        zoo.entities
            .push(entity("guests", "man", 1, centre((6, 6)), 0));
        zoo.entities.extend(extra);
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
    stand(&mut w, 1, at);
    // The guest uses no building, so that only the stir can reset it.
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    w.units.get_mut(1).unwrap().hidden = false;
    w.take_stirred();
    change(&mut w, id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    w.units.get(1).unwrap().wait
}

/// Unit `id` stands at the centre of tile `at`, in the map index too.
fn stand(w: &mut World, id: u32, at: (i32, i32)) {
    let index = w.find(id).unwrap();
    let (x, y) = centre(at);
    w.place_unit(index, (x, y, 0), 0);
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

/// Stirs tank `id` with guest 1 at `at` and guest 2 at `other`, neither busy; the waits of both.
fn stir_two(at: (i32, i32), other: (i32, i32), gate: bool) -> (i32, i32) {
    let (mut w, cat, id) = flooded();
    if gate {
        // The wall east of tile (3, 3): the gate, with tile (5, 3) outside.
        w.set_gate(id, 107);
    }
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    for (unit, to) in [(1, at), (2, other)] {
        stand(&mut w, unit, to);
        w.units.get_mut(unit).unwrap().hidden = false;
    }
    w.take_stirred();
    w.stir(id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    (w.units.get(1).unwrap().wait, w.units.get(2).unwrap().wait)
}

#[test]
fn the_stir_resets_the_tank_tiles_before_the_gate_tile() {
    // Point 22: the tank's tile list, then the outer gate tile: guest 2, first in the entity list
    // and on the gate tile, draws after guest 1 in the tank.
    let alone = stir_two((2, 3), (6, 6), true).0;
    assert!((1000..5000).contains(&alone));
    let (tank, gate) = stir_two((2, 3), (5, 3), true);
    assert_eq!(tank, alone, "the tank guest takes the first draw");
    assert!((1000..5000).contains(&gate) && gate != tank);
}

#[test]
fn a_guest_on_the_outer_gate_tile_is_reset() {
    let (a, b) = stir_two((6, 6), (5, 3), true);
    assert!(a == 0 && (1000..5000).contains(&b));
    // Without a gate there is no gate tile: the same guest is left alone.
    assert_eq!(stir_two((6, 6), (5, 3), false), (0, 0));
}

/// Stirs the flooded tank with `units` (class, id, tile, in list order) standing as given, none
/// busy, put in place after the tank is filled (ids 1 and 2 are the guests of the fixture); the waits in the order given.
fn stirred_units(units: &[(&str, u32, (i32, i32))]) -> (World, Vec<i32>) {
    let extra = (units.iter().filter(|u| u.1 > 2))
        .map(|&(class, id, _)| entity(class, "", id, centre((6, 6)), 0))
        .collect();
    let (mut w, cat, id) = flooded_with(extra);
    w.set_gate(id, 107);
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    w.units.get_mut(1).unwrap().hidden = false;
    stand(&mut w, 1, (6, 6));
    stand(&mut w, 2, (6, 6));
    for &(_, unit, at) in units {
        stand(&mut w, unit, at);
    }
    w.take_stirred();
    w.stir(id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let waits = units
        .iter()
        .map(|u| w.units.get(u.1).unwrap().wait)
        .collect();
    (w, waits)
}

#[test]
fn an_animal_a_worker_and_a_guide_in_the_tank_stop_and_wait() {
    // Point 22: the unit's reset, 1000 to 4999 ms, no goal.
    for class in ["animals", "maint", "tour"] {
        let (w, waits) = stirred_units(&[(class, 20, (2, 3))]);
        assert!((1000..5000).contains(&waits[0]), "{class}");
        assert_eq!(w.units.get(20).unwrap().goal.record.task, crate::goal::NONE);
        // Left of the tank, and a unit on the gate tile too.
        assert_eq!(stirred_units(&[(class, 20, (6, 6))]).1, [0], "{class}");
        assert!((1000..5000).contains(&stirred_units(&[(class, 20, (5, 3))]).1[0]));
    }
}

#[test]
fn a_sleeping_animal_in_the_tank_wakes_at_the_reset() {
    let (mut w, cat, id) = flooded_with(vec![entity("animals", "", 20, centre((6, 6)), 0)]);
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    stand(&mut w, 20, (2, 3));
    let Part::Animal(a) = &mut w.units.get_mut(20).unwrap().part else {
        panic!("an animal")
    };
    a.sleeping = true;
    w.take_stirred();
    w.stir(id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let Part::Animal(a) = &w.units.get(20).unwrap().part else {
        panic!("an animal")
    };
    assert!(!a.sleeping);
}

#[test]
fn a_non_guest_using_a_building_is_taken_out_of_it_by_the_reset() {
    // Point 22: the unit's reset takes a unit in a building task out of its building.
    let (mut w, cat, id) = flooded_with(vec![entity("animals", "", 20, centre((6, 6)), 0)]);
    w.units
        .buildings
        .get_mut(&7)
        .unwrap()
        .users
        .push((20, 1, 0));
    stand(&mut w, 20, (2, 3));
    let u = w.units.get_mut(20).unwrap();
    u.goal.record.task = crate::goal::USE;
    let Part::Animal(a) = &mut u.part else {
        panic!("an animal")
    };
    (a.building, a.slot) = (7, 1);
    w.take_stirred();
    w.stir(id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let users = &w.units.buildings[&7].users;
    assert!(users.iter().all(|u| u.0 != 20), "{users:?}");
    assert_eq!(w.units.get(20).unwrap().goal.record.task, crate::goal::NONE);
}

#[test]
fn a_helicopter_in_the_tank_is_left_alone() {
    // It keeps its goal and takes no draw: the guest after it takes the first one.
    let units = [("helicopter", 20, (2, 3)), ("guests", 1, (3, 2))];
    let (mut w, cat, id) = flooded_with(vec![entity("helicopter", "", 20, centre((6, 6)), 0)]);
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    w.units.get_mut(1).unwrap().hidden = false;
    for (_, unit, at) in units {
        stand(&mut w, unit, at);
    }
    w.units.get_mut(20).unwrap().goal.record.task = crate::goal::DUTIES;
    w.take_stirred();
    w.stir(id);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let heli = w.units.get(20).unwrap();
    assert_eq!((heli.goal.record.task, heli.wait), (crate::goal::DUTIES, 0));
    let first = 1000 + crate::sim::Rng::default().pick(4000) as i32;
    assert_eq!(w.units.get(1).unwrap().wait, first);
}

#[test]
fn a_keeper_in_the_tank_waits_and_starts_its_duties_at_once() {
    let (w, waits) = stirred_units(&[("keeper", 20, (2, 3))]);
    assert!((1000..5000).contains(&waits[0]));
    let record = &w.units.get(20).unwrap().goal.record;
    assert_eq!((record.task, record.mode), (crate::goal::DUTIES, 0));
}

#[test]
fn mixed_units_draw_in_the_order_of_the_tank_tiles_then_the_gate_tile() {
    // The list order is guests 2 and 1, the animal, the keeper; the draws go by tile: the keeper on
    // tile (3, 2), guest 1 on (2, 3), then the animal on the gate tile (5, 3). Guest 2 is outside.
    let units = [
        ("animals", 20, (5, 3)),
        ("keeper", 21, (3, 2)),
        ("guests", 1, (2, 3)),
    ];
    let (w, waits) = stirred_units(&units);
    let mut rng = crate::sim::Rng::default();
    let draws: Vec<i32> = (0..3).map(|_| 1000 + rng.pick(4000) as i32).collect();
    assert_eq!(waits, [draws[2], draws[0], draws[1]]);
    assert_eq!(w.units.get(2).unwrap().wait, 0);
}

#[test]
fn a_change_of_the_height_alone_resets_the_guests_in_the_tank() {
    let (mut w, cat, id) = flooded();
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    stand(&mut w, 1, (2, 3));
    w.units.get_mut(1).unwrap().hidden = false;
    // A lower tank holds less water than its walls: raising the walls keeps the level.
    w.set_height(id, 7);
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let (level, height) = (w.tank(id).unwrap().level, w.tank(id).unwrap().height);
    w.units.get_mut(1).unwrap().wait = 0;
    w.set_height(id, height + 1);
    assert_eq!(w.tank(id).unwrap().level, level, "only the height moved");
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    assert!((1000..5000).contains(&w.units.get(1).unwrap().wait));
}

#[test]
fn a_loaded_tank_stirs_no_guest() {
    // The tank record of a file sets the height and the level when the world is built.
    let mut w = world_with((8, 8), |zoo| {
        walls(zoo);
        zoo.tanks = vec![crate::zoo::Tank {
            tile: (2, 2),
            gate: (-1, -1),
            height: 9,
            level: 6,
            salt: false,
            purity: 90,
            filled: true,
            target: false,
        }];
    });
    assert_eq!(
        w.tank(w.tank_at(3 * 8 + 3).unwrap()).map(|t| t.height),
        Some(9)
    );
    assert_eq!(w.take_stirred(), Vec::<u32>::new());
}

/// A tank toy: not a guest building, dead on land, a surface object.
const TOY: &str = "[Characteristics/Integers]\ncCapacity=1\ncDeadOnLand=1\ncSurface=1\n\
    [Satisfies]\nshowtrick\n";

/// A type that is dead when it stands on a water tile.
const BUOY: &str = "[Characteristics/Integers]\ncCapacity=3\ncDeadOnFlatWater=1\n\
    [Satisfies]\nbuilding\n[slots]\nname=a\n\
    [a]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=3\n";

/// A tank of `flooded` with a building of type `ty` (it must be named "cave" to be taken by a
/// tank), id 7, and no figures yet.
fn flooded_toy(ty: EntityType) -> (World, Catalogue, u32) {
    let mut cat = Catalogue::default();
    cat.push(ty);
    let mut w = world_with((8, 8), |zoo| {
        walls(zoo);
        zoo.entities
            .push(entity("building", "cave", 7, centre((3, 3)), 0));
    });
    w.set_tank_config(CONFIG);
    w.set_paused(true);
    let id = w.tank_at(3 * 8 + 3).expect("a tank");
    w.set_height(id, 9);
    assert_eq!(w.fill(id), None);
    w.take_wet();
    (w, cat, id)
}

#[test]
fn a_removed_wall_takes_the_tank_away_and_the_cave_goes_dead() {
    let (mut w, cat, _) = flooded();
    decide_all(&mut w, &cat);
    assert!(!w.units.buildings[&7].dead);
    assert!(w.remove(100));
    assert_eq!(w.tank_at(3 * 8 + 3), None, "no tank any more");
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let live = &w.units.buildings[&7];
    assert!(live.dead && live.users.is_empty());
    assert!(!w.units.get(1).unwrap().hidden, "the user is put out");
}

#[test]
fn a_tank_made_validates_the_objects_again() {
    let (mut w, cat, _) = flooded();
    w.take_reshaped();
    let wall = w
        .zoo
        .entities
        .iter()
        .find(|e| e.common.as_ref().is_some_and(|c| c.id == 100));
    let wall = wall.unwrap().clone();
    assert!(w.remove(100));
    w.take_reshaped();
    let shape = crate::unit::testing::shape(&wall);
    w.add(wall, shape).expect("the wall is back");
    assert!(w.tank_at(3 * 8 + 3).is_some(), "a tank again");
    assert!(w.take_reshaped());
    assert!(!w.take_reshaped(), "told once");
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
}

#[test]
fn a_toy_is_dead_in_a_tank_with_no_water_and_keeps_no_figures_otherwise() {
    let (mut w, cat, tank) = flooded_toy(typed("cave", TOY));
    let ty = cat.types.last().unwrap();
    decide_all(&mut w, &cat);
    assert!(w.units.buildings.is_empty(), "alive: nothing stored");
    w.set_paused(false);
    assert!(w.drain(tank));
    let mut sim = crate::sim::Sim::new(b"");
    for _ in 0..12 {
        sim.step_in(&mut w, &cat, 100, |_, _, _| {});
    }
    assert_eq!(w.tank(tank).unwrap().level, 0);
    assert!(w.units.buildings[&7].dead, "drained");
    // Put down again where the water is: alive.
    w.set_paused(true);
    assert_eq!(w.fill(tank), None);
    w.take_wet();
    decide(&mut w, ty, 7);
    assert!(!w.units.buildings[&7].dead);
}

#[test]
fn a_toy_set_down_in_an_empty_tank_is_dead() {
    let (mut w, cat, tank) = flooded_toy(typed("cave", TOY));
    w.set_paused(true);
    assert!(w.drain(tank));
    assert_eq!(w.tank(tank).unwrap().level, 0);
    crate::building::built(&mut w, cat.types.last().unwrap(), 7, 0);
    assert!(w.units.buildings[&7].dead);
}

#[test]
fn a_tile_turning_water_validates_the_objects_on_it() {
    let (mut w, cat, _) = flooded_toy(typed("cave", BUOY));
    w.set_water(vec![false, true]);
    let tile = |w: &World| w.zoo.tiles[3 * 8 + 3];
    let live = Live {
        users: vec![(1, 0, 0)],
        ..Live::new(&Kind::of(cat.types.last().unwrap(), 0.0))
    };
    w.units.buildings.insert(7, live);
    w.take_reshaped();
    decide_all(&mut w, &cat);
    assert!(!w.units.buildings[&7].dead);
    let t = tile(&w);
    assert!(w.set_tile(3, 3, t.elevation, t.shape, 1));
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    let live = &w.units.buildings[&7];
    assert!(
        live.dead && live.users.is_empty(),
        "on water: dead, emptied"
    );
    assert!(w.set_tile(3, 3, t.elevation, t.shape, 0));
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    assert!(!w.units.buildings[&7].dead, "on land again");
    // The undo of a paint turns the tile the other way.
    assert!(w.set_tile(3, 3, t.elevation, t.shape, 1));
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    assert!(w.units.buildings[&7].dead);
    w.undo();
    wet_changed(&mut w, &cat, &mut crate::sim::Rng::default());
    assert!(!w.units.buildings[&7].dead, "the paint is undone");
}

#[test]
fn a_revalidation_puts_out_the_users_of_a_building_loaded_dead() {
    let (mut w, cat, tank) = flooded();
    w.set_height(tank, 3);
    w.take_wet();
    decide_all(&mut w, &cat);
    assert_eq!(w.units.buildings[&7].users.len(), 1, "kept at the load");
    revalidate(&mut w, &cat);
    assert!(w.units.buildings[&7].users.is_empty());
}

/// A toy that turns dead keeps its own building record in a save (zoo format 6.9): the dead
/// state is not saved (11.1.1), so its price and construction date must not become those of a
/// new building.
#[test]
fn a_dead_toy_keeps_its_saved_record() {
    let (mut w, cat, tank) = flooded_toy(typed("cave", TOY));
    let bits = |v: f32| v.to_bits();
    let record = crate::zoo::BuildingRecord {
        guests: [0; 3],
        income: [bits(0.0); 3],
        upkeep: Some(bits(0.0)),
        upgrade: 0,
        price: bits(5.5),
        users: Vec::new(),
        users_now: 0,
        members: Vec::new(),
        fill: 0,
        built: 126_227_808_000_000_000,
    };
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().building = Some(record.clone());
    w.set_paused(true);
    assert!(w.drain(tank));
    decide_all(&mut w, &cat);
    assert!(w.units.buildings.get(&7).is_some_and(|l| l.dead), "dead");
    let mut zoo = w.saved(|_| true);
    w.finish_save(&mut zoo, &cat);
    let at = (zoo.entities.iter())
        .position(|e| e.common.as_ref().is_some_and(|c| c.id == 7))
        .unwrap();
    let saved = zoo.entities[at]
        .common
        .as_ref()
        .unwrap()
        .building
        .clone()
        .unwrap();
    assert_eq!((saved.price, saved.built), (record.price, record.built));
}

/// A scenery type with a dead key is decided like a building and keeps its state through the
/// unit sync; the guest building lives of the same table are kept too.
#[test]
fn a_scenery_object_on_land_is_dead_and_stays_dead_after_a_sync() {
    let bush = EntityType {
        class: "objects",
        ..typed("bush", "[Characteristics/Integers]\ncDeadOnLand=1\n")
    };
    let mut cat = Catalogue::default();
    cat.push(bush);
    let mut w = world_with((8, 8), |zoo| {
        zoo.entities
            .push(entity("objects", "bush", 9, centre((6, 6)), 0));
    });
    decide_all(&mut w, &cat);
    assert!(w.units.buildings[&9].dead, "dead on land");
    let entities = w.zoo.entities.clone();
    w.units.sync(&entities);
    assert!(w.units.buildings[&9].dead, "kept by the sync");
}
