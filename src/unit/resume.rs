//! The units of a zoo just loaded go on with their saved behaviour set (behaviours 4.7): a mover,
//! a sleeper, a member of staff, whatever it was doing. A unit that was entering, using or leaving
//! a building that is gone is stopped instead. The saved path and pending set name come back with
//! the set, so a restored move call walks on (behaviours 4.7).

use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, RANDOM, Record, USE, stop_use};
use crate::unit::Unit;
use crate::world::World;
use crate::zoo::UnitBlock;

/// Whether a building stands on tile `t`: one of its quarters holds it, or it is anchored there.
fn building_on(world: &World, t: usize) -> bool {
    let is = |e: usize| world.zoo.entities[e].class == "building";
    let anchored = (world.zoo.entities.iter()).any(|e| {
        e.class == "building"
            && e.common.as_ref().is_some_and(|c| {
                world.tile_index(c.x.div_euclid(64), c.y.div_euclid(64)) == Some(t)
            })
    });
    (0..4).any(|q| world.quadrant(t, q).is_some_and(is)) || anchored
}

/// A path, its levels and its destination.
type Way = (Vec<usize>, Vec<i32>, Option<(i32, i32, i32)>);

/// The way a unit record stores (zoo format 6.14), as saved: the path's tiles and, when a depth
/// search made it (from version 84), its levels, "none" being -1, the level 0 of a swimmer
/// included; with a path, also the destination point of the record. A node off the map makes the whole
/// path empty: nothing could walk it. A record with no path keeps no destination, so that the
/// move call finds the way the goals give it.
fn saved_way(world: &World, block: &UnitBlock) -> Way {
    let at = |n: u32| i32::try_from(n).ok();
    let tile = |r: &[u32; 3]| world.tile_index(at(r[0])?, at(r[1])?);
    let Some(path) = (block.records.iter().map(tile)).collect::<Option<Vec<_>>>() else {
        return Default::default();
    };
    let levels = if world.zoo.version >= 84 {
        block.records.iter().map(|r| r[2] as i32).collect()
    } else {
        Vec::new()
    };
    let [x, y, z] = block.p3;
    let there = (!path.is_empty() && x >= 0 && y >= 0).then_some((x, y, z));
    (path, levels, there)
}

/// The pending set name a unit record stores, as saved: the hook starts it when no set runs
/// (behaviours 4.6, 4.7). An empty name is none.
pub fn keep_pending(unit: &mut Unit, name: &str) {
    unit.runner.forget();
    if !name.is_empty() {
        unit.runner.request(name);
    }
}

/// The plain move goal of unit `id` as saved: its task, its target `aim` (a position, none when
/// below 0) and the 3D target `name` the unit's move calls compare.
fn walk_on(world: &mut World, id: u32, now: u32, aim: [i32; 3], name: &str) {
    let [x, y, z] = aim;
    let pos = (x >= 0 && y >= 0).then_some((x, y, z));
    let mut record = Record {
        task: RANDOM,
        mode: 1,
        start: now,
        ..Record::default()
    };
    record.target.set_pos(world, pos);
    let Some(unit) = world.units.get_mut(id) else {
        return;
    };
    unit.goal.record = record;
    unit.target = crate::behaviour::target(name);
}

/// Gives every loaded unit its saved set, path, pending set name, and, for a unit walking its path
/// with the plain move goal, that goal with its target and the 3D target name; or stops it when its
/// saved task is in a building and the stored tile of that building holds none (shown, goal reset,
/// path emptied, set and pending name cleared). A unit whose building is there is the building's: its users pair decides
/// ([`crate::building::users::restore`], which runs after this).
pub fn load(world: &mut World, now: u32) {
    for at in 0..world.zoo.entities.len() {
        let Some(c) = world.zoo.entities[at].common.as_ref() else {
            continue;
        };
        let Some(block) = c.unit.as_deref() else {
            continue;
        };
        let (id, set, tile) = (c.id, block.behaviour.clone(), block.building);
        let (way, pending) = (saved_way(world, block), block.pending.clone());
        let (aim, name) = (block.target.target, block.text.clone());
        let task = u8::try_from(block.target.active).ok();
        let inside = task.is_some_and(|t| [ENTER_BUILDING, EXIT_BUILDING, USE].contains(&t));
        let here = tile
            .iter()
            .all(|&n| n >= 0)
            .then(|| world.tile_index(tile[0], tile[1]))
            .flatten();
        let present = here.is_some_and(|t| building_on(world, t));
        if inside && present {
            continue;
        }
        let Some(unit) = world.units.get_mut(id) else {
            continue;
        };
        if inside {
            stop_use(unit);
            unit.hidden = false;
        } else {
            unit.runner.restore(&set);
            (unit.path, unit.levels, unit.destination) = way;
            keep_pending(unit, &pending);
            if task == Some(RANDOM) && !unit.path.is_empty() {
                walk_on(world, id, now, aim, &name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::testing::{centre, entity, world_with};
    use crate::zoo::{Level, UnitBlock};

    /// A hall at tile (0, 1) and four units 1 to 4 (guest, keeper, animal, guest) that stand on
    /// tile (3, 1), each saved with the task `tasks[i]`, the building tile `tile` and the set
    /// `tail` (loaded, call done, running, forced); `resume::load` has run.
    fn loaded(tasks: [i32; 4], tile: [i32; 2], tail: [u8; 4]) -> World {
        let mut w = world_with((12, 4), |zoo| {
            zoo.entities
                .push(entity("building", "hall", 9, centre((0, 1)), 0));
            for (i, class) in ["guests", "keeper", "animals", "guests"].iter().enumerate() {
                let e = entity(class, "x", 1 + i as u32, centre((3, 1)), 0);
                zoo.entities.push(e);
            }
        });
        for (i, task) in tasks.into_iter().enumerate() {
            let mut block = UnitBlock::default();
            block.target.active = task;
            block.building = tile;
            block.behaviour.levels[0] = Level {
                name: "bWalk".into(),
                list_b: vec!["fWalk(0,0)".into(), "fPlay(a)".into()],
                tail,
                ..Default::default()
            };
            w.zoo.entities[1 + i].common.as_mut().unwrap().unit = Some(Box::new(block));
            let u = w.units.get_mut(1 + i as u32).unwrap();
            u.hidden = true;
            u.runner.request("bPending");
        }
        load(&mut w, 0);
        w
    }

    #[test]
    fn every_unit_goes_on_with_its_saved_set_whatever_its_kind() {
        // Walking (0x66 is a task of the walk), idle, a negative task: all resume the saved calls.
        let w = loaded([102, -1, 0, 5], [-1, -1], [1, 0, 1, 0]);
        for id in 1..=4 {
            let r = &w.units.get(id).unwrap().runner;
            assert_eq!(r.top(), Some("bWalk"), "{id}");
        }
    }

    #[test]
    fn a_set_that_was_not_running_is_not_taken() {
        let w = loaded([-1; 4], [-1, -1], [1, 0, 0, 0]);
        assert!((1..=4).all(|id| !w.units.get(id).unwrap().runner.running()));
    }

    #[test]
    fn a_unit_in_a_building_that_is_gone_is_stopped_and_shown() {
        // Saved entering, using and leaving, with no building on the stored tile (the tile of an
        // empty spot, none, a spot off the map): stopped, pending set cleared, shown.
        for tile in [[5, 2], [-1, -1], [900, 900], [-1, 3]] {
            let w = loaded([104, 105, 106, 106], tile, [1, 0, 1, 0]);
            for id in 1..=4 {
                let u = w.units.get(id).unwrap();
                assert!(!u.runner.running() && u.runner.queued().is_none(), "{id}");
                assert!(!u.hidden && u.path.is_empty(), "{id} {tile:?}");
            }
        }
    }

    #[test]
    fn a_unit_in_a_building_that_is_there_is_left_to_the_users() {
        // The tile of the hall, by its quarter or its anchor: nothing is taken here.
        let w = loaded([104, 105, 106, 106], [0, 1], [1, 0, 1, 0]);
        for id in 1..=4 {
            let u = w.units.get(id).unwrap();
            assert!(u.hidden && !u.runner.running(), "{id}");
        }
    }

    /// A keeper saved at tile (1, 1) in the set `bWalk` (one move call, running) with these path
    /// records and pending name, in a file of version `version`; `resume::load` has run.
    fn walking(version: u32, records: Vec<[u32; 3]>, pending: &str) -> World {
        saved_goal(version, records, pending, (-1, [-1; 3], ""))
    }

    /// The same, saved with the active goal `task`, the target `aim` and the 3D target name.
    fn saved_goal(
        version: u32,
        records: Vec<[u32; 3]>,
        pending: &str,
        (task, aim, name): (i32, [i32; 3], &str),
    ) -> World {
        let mut w = world_with((12, 4), |zoo| {
            zoo.version = version;
            zoo.entities
                .push(entity("keeper", "x", 1, centre((1, 1)), 0));
        });
        if task != 1 {
            crate::goal::quiet(&mut w);
        }
        let end = records.last().map_or([-1; 3], |r| {
            [r[0] as i32 * 64 + 32, r[1] as i32 * 64 + 32, 0]
        });
        let mut block = UnitBlock {
            p3: end,
            records,
            pending: pending.into(),
            text: name.into(),
            ..Default::default()
        };
        block.target.active = task;
        block.target.target = aim;
        block.behaviour.levels[0] = Level {
            name: "bWalk".into(),
            list_b: vec!["fWalk(0,0)".into()],
            tail: [1, 0, 1, 0],
            ..Default::default()
        };
        w.zoo.entities[0].common.as_mut().unwrap().unit = Some(Box::new(block));
        load(&mut w, 0);
        w
    }

    #[test]
    fn the_saved_path_and_pending_name_come_back_as_saved() {
        let w = walking(104, vec![[1, 1, 0], [2, 1, 0], [3, 1, 2]], "bEat");
        let u = w.units.get(1).unwrap();
        let tiles = [(1, 1), (2, 1), (3, 1)].map(|(x, y)| w.tile_index(x, y).unwrap());
        assert_eq!(
            (u.path.clone(), u.levels.clone()),
            (tiles.to_vec(), vec![0, 0, 2])
        );
        assert_eq!(u.runner.queued(), Some("bEat"));
        // An empty name is none; a file before version 84 has no levels; "none" is -1.
        let w = walking(104, vec![[1, 1, 0], [2, 1, 0]], "");
        let u = w.units.get(1).unwrap();
        assert!(u.runner.queued().is_none() && u.path.len() == 2);
        let w = walking(83, vec![[1, 1, 0], [2, 1, 0]], "");
        assert!(w.units.get(1).unwrap().levels.is_empty());
        let w = walking(104, vec![[1, 1, u32::MAX], [2, 1, 1]], "");
        assert_eq!(w.units.get(1).unwrap().levels, [-1, 1]);
    }

    #[test]
    fn a_depth_path_saved_at_level_0_keeps_its_levels() {
        // A swimmer at level 0 stores 0 on every node (behaviours 4.7): the levels stay as saved.
        let w = walking(104, vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]], "");
        assert_eq!(w.units.get(1).unwrap().levels, [0, 0, 0]);
        let w = walking(84, vec![[1, 1, 0], [2, 1, 0]], "");
        assert_eq!(w.units.get(1).unwrap().levels, [0, 0]);
    }

    #[test]
    fn a_path_off_the_map_is_not_taken() {
        for off in [[900, 1, 0], [1, u32::MAX, 0], [u32::MAX, u32::MAX, 0]] {
            let w = walking(104, vec![[1, 1, 0], off], "");
            let u = w.units.get(1).unwrap();
            assert!(u.path.is_empty() && u.destination.is_none(), "{off:?}");
        }
    }

    /// The tile of keeper 1 after `steps` steps of the walk.
    fn walked(mut w: World, steps: u32) -> (World, (i32, i32)) {
        let mut s = crate::sim::Sim::new(b"");
        s.set_seed(1);
        s.start_game(48, crate::zoo::Clocks::default());
        let walker = crate::unit::testing::Walker::new("fWalk(0,0)");
        for _ in 0..steps {
            s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
        }
        let c = w.zoo.entities[0].common.as_ref().unwrap();
        let tile = (c.x.div_euclid(64), c.y.div_euclid(64));
        (w, tile)
    }

    #[test]
    fn the_first_run_of_a_restored_move_call_walks_the_saved_path_on() {
        // Two nodes or more: no random walk, the unit goes to the last node and the call ends.
        let w = walking(104, vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]], "");
        let (w, tile) = walked(w, 3000);
        assert_eq!(tile, (3, 1));
        let u = w.units.get(1).unwrap();
        assert!(u.path.is_empty() && !u.walking && !u.runner.running());
    }

    #[test]
    fn the_first_run_of_a_restored_move_call_with_one_node_ends_at_once() {
        let w = walking(104, vec![[1, 1, 0]], "");
        let (w, tile) = walked(w, 3);
        assert_eq!(tile, (1, 1));
        let u = w.units.get(1).unwrap();
        assert!(!u.walking && !u.moving && u.path.is_empty() && !u.runner.running());
    }

    #[test]
    fn a_unit_saved_with_the_plain_move_goal_has_its_goal_and_3d_target_back() {
        let way = vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]];
        let w = saved_goal(104, way.clone(), "", (1, [3 * 64, 64, 0], "surface"));
        let u = w.units.get(1).unwrap();
        let r = &u.goal.record;
        assert_eq!((r.task, r.target.tile), (1, w.tile_index(3, 1)));
        assert_eq!(u.target, crate::behaviour::Target::Surface);
        // Another goal, or no path, or no target: the goal stays as it was.
        for (task, aim, records) in [
            (101, [3 * 64, 64, 0], way.clone()),
            (-1, [3 * 64, 64, 0], way.clone()),
            (1, [3 * 64, 64, 0], Vec::new()),
        ] {
            let w = saved_goal(104, records, "", (task, aim, "surface"));
            assert_ne!(w.units.get(1).unwrap().goal.record.task, 1, "{task}");
        }
        let w = saved_goal(104, way.clone(), "", (1, [-1; 3], ""));
        let r = &w.units.get(1).unwrap().goal.record;
        assert_eq!((r.task, r.target.tile), (1, None));
        // Huge and off-map targets are no tile.
        let w = saved_goal(104, way, "", (1, [i32::MAX, i32::MIN, 0], "x"));
        assert_eq!(w.units.get(1).unwrap().goal.record.target.tile, None);
    }

    #[test]
    fn a_unit_saved_with_the_plain_move_goal_walks_its_path_on() {
        // The goal's first update leaves the path (a target and a path): the unit goes on along
        // the row it was saved on instead of a random walk.
        let way = vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]];
        let w = saved_goal(104, way, "", (1, [3 * 64 + 32, 64 + 32, 0], ""));
        let (w, tile) = walked(w, 100);
        assert_eq!(tile.1, 1);
        assert!(w.units.get(1).unwrap().goal.record.task == 1);
    }
}
