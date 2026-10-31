//! The units in a tank on a synthetic tank: 4 by 3 tiles from (2, 2), the floor at level 0, the
//! walls 5 high and the water at level 4, so the surface is at height 64.

use super::*;
use crate::path::{Finder, Mover, Settings};
use crate::sim::Sim;
use crate::unit::testing::{Walker, tank_world};
use crate::zoo::Clocks;

const WIDE: i32 = 12;

fn tile((x, y): (i32, i32)) -> usize {
    (y * WIDE + x) as usize
}

fn height(w: &World, id: u32) -> i32 {
    w.zoo.entities[w.find(id).unwrap()]
        .common
        .as_ref()
        .unwrap()
        .z
}

/// A unit that lives in tanks, `depth` being its `cDepth`; typed, so its flags stay.
fn fish(w: &mut World, id: u32, depth: i32, edit: impl FnOnce(&mut Unit)) {
    let u = w.units.get_mut(id).unwrap();
    (u.typed, u.mover.underwater, u.mover.depth) = (true, true, depth);
    edit(u);
}

fn checked(w: &mut World, id: u32) -> (u8, u32) {
    let at = w.find(id).unwrap();
    let mut unit = std::mem::replace(w.units.get_mut(id).unwrap(), Unit::new(id, Kind::Animal));
    check(&mut unit, w, at);
    let mode = w.zoo.entities[at].common.as_ref().unwrap().movement_mode;
    let tank = unit.tank;
    *w.units.get_mut(id).unwrap() = unit;
    (tank, mode)
}

#[test]
fn the_tank_position_check_keeps_a_unit_between_the_floor_and_the_surface() {
    let mut w = tank_world(
        5,
        4,
        &[
            (1, (3, 3), 32),
            (2, (3, 3), 80),
            (3, (3, 3), 16),
            (4, (8, 6), 0),
        ],
    );
    (1..=4).for_each(|id| fish(&mut w, id, 1, |_| ()));
    w.units.get_mut(3).unwrap().mover.surface = true;
    // Under water the height is kept; above the surface, or for a surface species, it is the surface.
    assert_eq!((checked(&mut w, 1), height(&w, 1)), ((4, 3), 32));
    assert_eq!((checked(&mut w, 2), height(&w, 2)), ((3, 3), 64));
    assert_eq!((checked(&mut w, 3), height(&w, 3)), ((3, 3), 64));
    // Out of the tank: the ground (kind 2), dry; a species that lives only under water stays so.
    assert_eq!(checked(&mut w, 4), (0, 2));
    w.units.get_mut(4).unwrap().mover.only_underwater = true;
    assert_eq!(checked(&mut w, 4), (4, 2));
    // A tank with absurd numbers in it does not overflow.
    let mut wild = tank_world(5, 4, &[(1, (3, 3), i32::MAX)]);
    let id = wild.tank_at(tile((3, 3))).unwrap();
    let tank = wild
        .exhibits_mut()
        .get_mut(id)
        .unwrap()
        .tank
        .as_mut()
        .unwrap();
    (tank.height, tank.level) = (u32::MAX, u32::MAX);
    fish(&mut wild, 1, 1, |_| ());
    assert_eq!(checked(&mut wild, 1).1, 3);
    // A dry tank holds a unit on the floor, in mode 0.
    let mut dry = tank_world(5, 0, &[(1, (3, 3), 40)]);
    fish(&mut dry, 1, 1, |_| ());
    assert_eq!((checked(&mut dry, 1), height(&dry, 1)), ((0, 3), 0));
    // The mode change goes level and asks for the animation again.
    assert!(w.units.get(1).unwrap().flip);
}

#[test]
fn the_level_of_a_destination_follows_the_3d_target_within_the_water() {
    let w = tank_world(5, 4, &[(1, (3, 3), 32)]);
    fish(&mut { w }, 1, 0, |_| ());
    let w = tank_world(5, 4, &[(1, (3, 3), 32)]);
    let mut rng = Rng::default();
    let mut unit = Unit::new(1, Kind::Animal);
    unit.mover.depth = 2;
    // From level 3 (here), `cDepth` 2, the surface 4.
    let mut seen = |target, surface: bool| {
        (unit.target, unit.mover.surface) = (target, surface);
        (0..200)
            .map(|_| level_of(&unit, &w, &mut rng, tile((3, 3)), 3))
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(seen(Target::Floor, false), [2].into());
    assert_eq!(seen(Target::Level, false), [3].into());
    assert_eq!(seen(Target::Surface, false), [4].into());
    assert_eq!(seen(Target::None, true), [4].into());
    // A dive goes below the level it is at, a rise above it, a plain one anywhere in the water.
    assert_eq!(seen(Target::Dive, false), [2].into());
    assert_eq!(seen(Target::Rise, false), [4].into());
    assert_eq!(seen(Target::None, false), [2, 3, 4].into());
    // Out of a tank it is the floor of the tile.
    assert_eq!(level_of(&unit, &w, &mut rng, tile((9, 6)), 3), 0);
    // Any `cDepth` and any level it is at gives a level without a panic.
    for depth in [i32::MIN, -5, 0, 1000, i32::MAX] {
        for target in [Target::Dive, Target::Rise, Target::Floor, Target::None] {
            (unit.mover.depth, unit.target) = (depth, target);
            for here in [i32::MIN, 0, 4, i32::MAX] {
                level_of(&unit, &w, &mut rng, tile((3, 3)), here);
            }
        }
    }
}

#[test]
fn a_depth_deeper_than_the_water_gives_no_level_in_it() {
    let w = tank_world(5, 4, &[(1, (3, 3), 32)]);
    let (mut unit, mut rng) = (Unit::new(1, Kind::Animal), Rng::default());
    let mut level = |depth, target| {
        (unit.mover.depth, unit.target) = (depth, target);
        level_of(&unit, &w, &mut rng, tile((3, 3)), 2)
    };
    // The depth is the type's as it is: 6 is beyond the surface at 4 for everything but the
    // surface target, a negative one puts the floor target under the floor.
    assert_eq!(level(6, Target::Floor), 6);
    assert_eq!(level(6, Target::Level), 6);
    assert_eq!(level(6, Target::None), 6);
    assert_eq!(level(6, Target::Surface), 4);
    assert_eq!(level(6, Target::Rise), 6);
    assert_eq!(level(6, Target::Dive), 6);
    assert_eq!(level(-2, Target::Floor), -2);
    // A path to such a level is refused, so the walk's fallbacks run.
    let mut m = Mover::new(crate::path::Kind::Animal, Some(tile((2, 2))));
    (m.underwater, m.deep, m.levels) = (true, true, (2, 6));
    let mut finder = Finder::new(Settings::default());
    assert!(!finder.request(&w, &m, tile((5, 4)), false).ok);
}

#[test]
fn a_3d_segment_is_as_long_as_its_truncated_true_length() {
    let mut w = tank_world(5, 4, &[(1, (3, 3), 32)]);
    fish(&mut w, 1, 1, |u| {
        // A flat diagonal step to tile (4, 4), then one that also climbs: 90 and 91 units.
        u.path = vec![tile((3, 3)), tile((4, 4)), tile((4, 3))];
        u.levels = vec![2, 2, 3];
    });
    let at = w.find(1).unwrap();
    let start = (3 * 64 + 10, 3 * 64 + 10);
    w.place_unit(at, (start.0, start.1, 32), 0);
    let mut unit = std::mem::replace(w.units.get_mut(1).unwrap(), Unit::new(1, Kind::Animal));
    let mut moved = Vec::new();
    for _ in 0..7 {
        assert!(!progress(&mut unit, &mut w, 4, at));
        let c = w.zoo.entities[at].common.as_ref().unwrap();
        moved.push((c.x - start.0, c.y - start.1));
    }
    // Both axes gain 64 * 4k / 90 truncated: 2, 5, 8, 11, 14, 17, 19.
    assert_eq!(
        moved.iter().map(|m| m.0).collect::<Vec<_>>(),
        [2, 5, 8, 11, 14, 17, 19]
    );
    assert!(moved.iter().all(|m| m.0 == m.1));
    assert_eq!(unit.segment.map(|s| s.length), Some(90));
}

#[test]
fn a_swimmer_at_the_edge_of_its_tank_always_finds_somewhere_to_go() {
    // Whatever the tile it is on, its facing and the draws: a destination of its own tile at its
    // own level goes to the fallbacks, which take tank tiles and give them a level (movement 10.2).
    for tile_xy in [(2, 2), (3, 2), (5, 4), (4, 3)] {
        for facing in 0..8 {
            for seed in 0..12 {
                let mut w = tank_world(5, 4, &[(1, tile_xy, 32)]);
                fish(&mut w, 1, 1, |u| {
                    if let Part::Animal(a) = &mut u.part {
                        a.terrain = 2;
                    }
                });
                let at = w.find(1).unwrap();
                w.place_unit(at, (tile_xy.0 * 64 + 32, tile_xy.1 * 64 + 32, 32), facing);
                checked(&mut w, 1);
                let mut rng = Rng::default();
                rng.seed(seed);
                let mut unit =
                    std::mem::replace(w.units.get_mut(1).unwrap(), Unit::new(1, Kind::Animal));
                assert!(super::super::walk::random_walk(
                    &mut unit,
                    &mut w,
                    &crate::entity::Catalogue::default(),
                    &mut rng,
                    at
                ));
                let to = unit.path.last().copied();
                assert!(unit.path.len() > 1, "{tile_xy:?} {facing} {seed}");
                assert!(w.tank_at(to.unwrap()).is_some());
            }
        }
    }
}

/// Two swimmers for a minute at the pace of the spec: they stay in the water and in their tank,
/// and a second run gives the same state.
fn minute() -> (u64, Vec<String>) {
    let mut w = tank_world(5, 4, &[(1, (3, 3), 32), (2, (4, 3), 48)]);
    crate::goal::quiet(&mut w);
    for id in [1, 2] {
        fish(&mut w, id, 1, |u| {
            u.mover.swims = true;
            u.goal.record.task = crate::goal::RANDOM;
            if let Part::Animal(a) = &mut u.part {
                a.terrain = 2;
            }
        });
    }
    let mut s = Sim::new(b"");
    s.set_seed(1);
    s.start_game(1200, Clocks::default());
    let walker = Walker::new("fSwim(level,0)");
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..900 {
        for id in [1, 2] {
            let u = w.units.get(id).unwrap();
            if !u.walking && !u.runner.running() {
                walker.start(&mut w, id);
            }
        }
        s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
        for id in [1, 2] {
            let c = w.zoo.entities[w.find(id).unwrap()].common.as_ref().unwrap();
            let t = w
                .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
                .unwrap();
            assert!(w.tank_at(t).is_some(), "left the tank at {:?}", (c.x, c.y));
            assert!((0..=64).contains(&c.z) && c.movement_mode == 3, "{}", c.z);
            seen.insert((id, t, c.z.div_euclid(16)));
        }
    }
    let tiles = seen
        .iter()
        .map(|k| k.1)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        tiles.len() > 4,
        "they swim around the tank: {}",
        tiles.len()
    );
    let depths = w
        .units
        .take_lines()
        .into_iter()
        .filter(|l| l.starts_with("depth "))
        .collect();
    (w.units.hash(), depths)
}

#[test]
fn two_swimmers_stay_in_the_water_for_a_minute_and_two_runs_agree() {
    let (a, depths) = minute();
    assert!(depths.len() > 10, "{}", depths.len());
    assert_eq!(a, minute().0);
}

#[test]
fn the_depth_search_changes_the_level_by_at_most_one_and_stays_in_the_water() {
    let w = tank_world(5, 4, &[(1, (3, 3), 0)]);
    let mut m = Mover::new(crate::path::Kind::Animal, Some(tile((2, 2))));
    (m.underwater, m.deep, m.levels) = (true, true, (0, 4));
    let mut finder = Finder::new(Settings::default());
    let reply = finder.request(&w, &m, tile((5, 4)), false);
    let (path, levels) = (reply.path.unwrap(), reply.levels);
    assert!(reply.ok && path.len() == levels.len() && path.len() > 3);
    assert_eq!((levels[0], levels[levels.len() - 1]), (0, 4));
    assert!(levels.windows(2).all(|l| (l[1] - l[0]).abs() <= 1));
    assert!(
        path.windows(2).all(|p| p[0] != p[1]),
        "never a level change alone"
    );
    assert!(
        path.iter().all(|&t| w.tank_at(t).is_some()) && levels.iter().all(|l| (0..=4).contains(l))
    );
    // A level above the water is refused, and a surface species goes along the surface.
    m.levels = (0, 5);
    assert!(!finder.request(&w, &m, tile((5, 4)), false).ok);
    (m.surface, m.levels) = (true, (4, 4));
    let reply = finder.request(&w, &m, tile((5, 4)), false);
    assert!(reply.ok && reply.levels.iter().all(|&l| l == 4));
}

#[test]
fn a_keeper_on_a_ladder_is_in_the_ladder_mode_and_kept_between_its_foot_and_its_top() {
    let mut w = tank_world(5, 4, &[]);
    let keeper = crate::unit::testing::entity("keeper", "keeper", 9, (0, 0), 0);
    let id = (w.add(keeper, Some(crate::unit::testing::keeper_shape()))).unwrap();
    let entities = w.zoo.entities.clone();
    w.units.sync(&entities);
    let place = move |w: &mut World, tile: (i32, i32), z: i32| {
        let at = w.find(id).unwrap();
        w.place_unit(at, (tile.0 * 64 + 32, tile.1 * 64 + 32, z), 0);
    };
    let job = move |w: &mut World, job: u8, target: Option<(i32, i32)>| {
        let u = w.units.get_mut(id).unwrap();
        u.goal.record.job = job;
        if let Part::Staff(s) = &mut u.part {
            s.target = target.map(tile);
        }
    };
    // The gate of the tank is on the north side of (2, 2): the tile outside is (2, 1). The
    // outside ladder (jobs 20 and 21, the target tile in the tank) holds the height between the
    // ground and the top of the wall, 80, and leaves the move kind the job gave (the air, 1).
    let air = move |w: &mut World| {
        let at = w.find(id).unwrap();
        w.zoo.entities[at].common.as_mut().unwrap().movement_mode = 1;
    };
    job(&mut w, 20, Some((3, 3)));
    air(&mut w);
    place(&mut w, (2, 1), 30);
    assert_eq!((checked(&mut w, id), height(&w, id)), ((1, 1), 30));
    place(&mut w, (2, 1), 500);
    assert_eq!((checked(&mut w, id), height(&w, id)), ((1, 1), 80));
    // Without the job the tile is plain ground.
    job(&mut w, 0, Some((3, 3)));
    assert_eq!(checked(&mut w, id), (0, 2));
    // The inside ladder (jobs 28 and 29) is on the tile inside the gate, from the floor up.
    job(&mut w, 29, None);
    air(&mut w);
    place(&mut w, (2, 2), -20);
    assert_eq!((checked(&mut w, id), height(&w, id)), ((1, 1), 0));
    place(&mut w, (2, 2), 70);
    assert_eq!((checked(&mut w, id), height(&w, id)), ((1, 1), 70));
    // With no job a unit at the height of the wall is on the platform; lower it is in the water.
    job(&mut w, 0, None);
    place(&mut w, (2, 2), 80);
    assert_eq!((checked(&mut w, id), height(&w, id)), ((2, 3), 80));
    place(&mut w, (2, 2), 40);
    w.units.get_mut(id).unwrap().tank = 0;
    assert_eq!((checked(&mut w, id), height(&w, id)), ((4, 3), 40));
}

#[test]
fn a_3d_move_call_goes_on_with_one_path_entry() {
    let mut w = tank_world(5, 4, &[(1, (3, 3), 32)]);
    crate::goal::quiet(&mut w);
    fish(&mut w, 1, 1, |u| {
        u.mover.swims = true;
        u.path = vec![tile((4, 3))];
        u.levels = vec![2];
        u.destination = Some((4 * 64 + 32, 3 * 64 + 32, 32));
    });
    let mut s = Sim::new(b"");
    s.set_seed(1);
    s.start_game(1200, Clocks::default());
    let walker = Walker::new("fSwim(level,0)");
    walker.start(&mut w, 1);
    s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
    let u = w.units.get(1).unwrap();
    assert!(u.walking && !u.path.is_empty());
}
