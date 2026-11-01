//! The units of a zoo just loaded go on with their saved behaviour set (behaviours 4.7): a mover,
//! a sleeper, a member of staff, whatever it was doing. A unit that was entering, using or leaving
//! a building that is gone is stopped instead. The saved path and pending set name come back with
//! the set, so a restored move call walks on, and so do the goal record (task, job, targets, done flags,
//! start, duration) and the 3D target name, whatever the task (behaviours 4.7), and the frightened
//! flag and the kept predator.

use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, Record, USE, active_goal, stop_use};
use crate::world::World;
use crate::zoo::{Target, UnitBlock};

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

/// The goal record of unit `id` as saved (`block`, zoo format 6.14): task and job with their targets
/// and done flags, start and duration, and the 3D target `name` the unit's move calls compare.
/// From version 72 the goal and job objects the file holds are built and not initialised again
/// ([`crate::goal::Goals::built`], behaviours 4.7). A task that has no goal object for the kind of
/// unit is not restored (the save writes none for it, save games S7): the unit keeps the goal it
/// has.
fn goal_back(world: &mut World, id: u32, now: u32, block: &Target, version: u32, name: &str) {
    let Some(kind) = world.units.get(id).map(|u| u.kind) else {
        return;
    };
    let Ok(task) = u8::try_from(block.goal) else {
        return;
    };
    if active_goal(task, kind) < 0 {
        return;
    }
    let pos = |p: [i32; 3]| (p[0] >= 0 && p[1] >= 0).then_some((p[0], p[1], p[2]));
    let (start, duration) = if version >= 53 {
        (block.extra[0], block.extra[1])
    } else {
        (now, 0)
    };
    let mut record = Record {
        task,
        mode: 1,
        job: u8::try_from(block.sub_goal).unwrap_or(0),
        start,
        duration,
        done: block.bytes[0] != 0,
        job_done: block.bytes[1] != 0,
        ..Record::default()
    };
    record.target.id = block.words[0];
    record.target.set_pos(world, pos(block.target));
    record.job_target.id = block.words[1];
    record.job_target.set_pos(world, pos(block.sub_target));
    let Some(unit) = world.units.get_mut(id) else {
        return;
    };
    unit.goal.record = record;
    if version >= 72 {
        unit.goal.built(kind, block.active, block.active_sub);
    }
    unit.target = crate::behaviour::target(name);
}

/// Gives every loaded unit its saved set, path, pending set name, goal task with its target and
/// the 3D target name (whatever the task); or stops it when its saved task is in a building and
/// the stored tile of that building holds none (shown, goal reset, path emptied, set and pending
/// name cleared). A unit whose building is there is the building's: its users pair decides
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
        let flag = block.pending_flag != 0;
        let (goal, name) = (block.target.clone(), block.text.clone());
        let fright = (block.b1 != 0, block.w1);
        let task = u8::try_from(block.target.goal).ok();
        let inside = task.is_some_and(|t| [ENTER_BUILDING, EXIT_BUILDING, USE].contains(&t));
        let here = tile
            .iter()
            .all(|&n| n >= 0)
            .then(|| world.tile_index(tile[0], tile[1]))
            .flatten();
        let present = here.is_some_and(|t| building_on(world, t));
        if let Some(unit) = world.units.get_mut(id) {
            (unit.frightened, unit.predator) = fright;
        }
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
            // The pending call as saved: the hook starts it when no set runs (behaviours 4.6, 4.7).
            unit.runner.keep(&pending, flag);
            goal_back(world, id, now, &goal, world.zoo.version, &name);
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
        loaded_held(tasks, tasks, tile, tail)
    }

    /// The same, the units' active goal numbers being `held`.
    fn loaded_held(tasks: [i32; 4], held: [i32; 4], tile: [i32; 2], tail: [u8; 4]) -> World {
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
            (block.target.goal, block.target.active) = (task, held[i]);
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
            u.runner.queue("bPending", true, true);
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
    fn the_building_tasks_are_read_from_the_wanted_goal_not_the_active_number() {
        // Gone building: a wanted task of 104 to 106 stops the unit whatever goal object it held; a
        // held object of 104 to 106 with another wanted task changes nothing (behaviours 4.7).
        let w = loaded_held([106, 105, 0, 5], [0, -1, 106, 104], [5, 2], [1, 0, 1, 0]);
        for (id, stopped) in [(1, true), (2, true), (3, false), (4, false)] {
            let u = w.units.get(id).unwrap();
            assert_eq!(!u.runner.running(), stopped, "{id}");
            assert_eq!(u.hidden, !stopped, "{id}");
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
        (block.target.goal, block.target.active) = (task, task);
        block.target.target = aim;
        saved_block_in(version, block)
    }

    /// A keeper at tile (1, 1) saved with `block`, in a file of version `version`; `resume::load`
    /// has run.
    fn saved_block_in(version: u32, mut block: UnitBlock) -> World {
        let mut w = world_with((12, 4), |zoo| {
            zoo.version = version;
            zoo.entities
                .push(entity("keeper", "x", 1, centre((1, 1)), 0));
        });
        crate::goal::quiet(&mut w);
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
    fn a_unit_saved_with_a_goal_has_its_task_target_and_3d_target_back() {
        let way = vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]];
        let w = saved_goal(104, way.clone(), "", (1, [3 * 64, 64, 0], "surface"));
        let u = w.units.get(1).unwrap();
        let r = &u.goal.record;
        assert_eq!((r.task, r.target.tile), (1, w.tile_index(3, 1)));
        assert_eq!(u.target, crate::behaviour::Target::Surface);
        // Every task that has a goal for a keeper, with a path, with none, and the base one.
        for (task, records) in [(101, way.clone()), (110, way.clone()), (127, Vec::new())] {
            let w = saved_goal(104, records, "", (task, [3 * 64, 64, 0], "surface"));
            let u = w.units.get(1).unwrap();
            let r = &u.goal.record;
            assert_eq!(
                (i32::from(r.task), r.target.tile),
                (task, w.tile_index(3, 1))
            );
            assert_eq!(u.target, crate::behaviour::Target::Surface, "{task}");
        }
        let w = saved_goal(104, Vec::new(), "", (1, [3 * 64, 64, 0], ""));
        assert_eq!(w.units.get(1).unwrap().goal.record.task, 1);
        // No goal object for a keeper (a guide's leg, a negative or a huge number): the goal and
        // the 3D target stay as they were.
        for task in [118, -1, 1000, i32::MIN] {
            let w = saved_goal(104, way.clone(), "", (task, [3 * 64, 64, 0], "surface"));
            let u = w.units.get(1).unwrap();
            assert_eq!(u.goal.record.target.tile, None, "{task}");
            assert_ne!(u.target, crate::behaviour::Target::Surface, "{task}");
        }
        let w = saved_goal(104, way.clone(), "", (1, [-1; 3], ""));
        let r = &w.units.get(1).unwrap().goal.record;
        assert_eq!((r.task, r.target.tile), (1, None));
        // Huge and off-map targets are no tile.
        let w = saved_goal(104, way, "", (101, [i32::MAX, i32::MIN, 0], "x"));
        assert_eq!(w.units.get(1).unwrap().goal.record.target.tile, None);
    }

    /// The target block of a keeper saved in task `goal` and job `sub_goal`, with these active numbers.
    fn job_block(goal: i32, sub_goal: i32, active: (i32, i32)) -> UnitBlock {
        let mut block = UnitBlock::default();
        let t = &mut block.target;
        (t.goal, t.sub_goal, t.active, t.active_sub) = (goal, sub_goal, active.0, active.1);
        t.words = [7, 9];
        (t.target, t.sub_target) = ([3 * 64, 64, 16], [2 * 64, 2 * 64, 0]);
        (t.bytes, t.extra) = ([1, 1], [40, 12]);
        block
    }

    #[test]
    fn a_unit_saved_in_a_job_has_the_whole_record_back() {
        let w = saved_block_in(104, job_block(110, 5, (110, 5)));
        let r = &w.units.get(1).unwrap().goal.record;
        assert_eq!((r.task, r.job, r.done, r.job_done), (110, 5, true, true));
        assert_eq!(
            (r.target.id, r.job_target.id, r.start, r.duration),
            (7, 9, 40, 12)
        );
        assert_eq!(r.target.tile, w.tile_index(3, 1));
        assert_eq!(r.job_target.tile, w.tile_index(2, 2));
        // Done flags down and no target; a file before version 53 has no start and no duration.
        let mut block = job_block(110, 5, (110, 5));
        (block.target.bytes, block.target.target) = ([0, 0], [-1; 3]);
        let w = saved_block_in(52, block);
        let r = &w.units.get(1).unwrap().goal.record;
        assert_eq!((r.done, r.job_done, r.target.tile), (false, false, None));
        assert_eq!((r.start, r.duration), (0, 0));
    }

    #[test]
    fn a_job_number_that_is_no_job_is_no_job_and_a_task_with_no_goal_keeps_the_unit_as_it_is() {
        for job in [-1, 256, i32::MIN, i32::MAX] {
            let w = saved_block_in(104, job_block(110, job, (110, job)));
            let r = &w.units.get(1).unwrap().goal.record;
            assert_eq!((r.task, r.job), (110, 0), "{job}");
        }
        // The record is the wanted task: an active goal that differs does not change it.
        let w = saved_block_in(104, job_block(0, 0, (110, 5)));
        assert_eq!(w.units.get(1).unwrap().goal.record.task, 0);
        for task in [118, -1, 1000, i32::MIN] {
            let w = saved_block_in(104, job_block(task, 5, (task, 5)));
            assert_eq!(w.units.get(1).unwrap().goal.record.job, 0, "{task}");
        }
    }

    #[test]
    fn the_objects_are_built_from_version_72_when_the_active_numbers_match_the_record() {
        // Which of the two objects the load built: the goal's, the job's.
        let built = |version, active| {
            let w = saved_block_in(version, job_block(110, 2, active));
            w.units.get(1).unwrap().goal.built_pair()
        };
        assert_eq!(built(104, (110, 2)), (true, true));
        assert_eq!(built(72, (110, 2)), (true, true));
        assert_eq!(built(71, (110, 2)), (false, false));
        // Only the job object differs: the goal object stays built (the original's slip is not kept).
        assert_eq!(built(104, (110, 1)), (true, false));
        assert_eq!(built(104, (110, -1)), (true, false));
        assert_eq!(built(104, (109, 2)), (false, true));
        for active in [(-1, -1), (0, 0), (i32::MIN, i32::MAX), (110 + 256, 2 + 256)] {
            assert_eq!(built(104, active), (false, false), "{active:?}");
        }
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

    /// The units of the shipped games saved with a goal other than the plain move, a target and
    /// a path have that task and target back after a load, and the units saved in a job have it
    /// back (behaviours 4.7).
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn the_units_of_the_game_saved_in_another_task_have_their_goal_back() {
        use crate::{animal::Class, exhibit::TankConfig, place, sim::Sim, terrain};
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
        let cat = crate::entity::Catalogue::load(&vfs).unwrap();
        let renderer = crate::render::MapRenderer::new(&vfs, &cat);
        let (mut checked, mut jobs, mut kinds) = (0, 0, std::collections::BTreeSet::new());
        for member in vfs.list(None).iter().filter(|m| m.ends_with(".zoo")) {
            let Ok((zoo, _)) = crate::zoo::Zoo::parse_money(&vfs.read(member).unwrap())
                .and_then(|(zoo, money)| Ok((zoo.checked(&cat)?, money)))
            else {
                continue;
            };
            let saved: Vec<_> = (zoo.entities.iter())
                .filter_map(|e| {
                    let c = e.common.as_ref()?;
                    let b = c.unit.as_deref()?;
                    let task = u8::try_from(b.target.goal).ok()?;
                    let off =
                        [ENTER_BUILDING, EXIT_BUILDING, USE, crate::goal::RANDOM].contains(&task);
                    let [x, y, _] = b.target.target.map(|n| n.div_euclid(64));
                    let job = u8::try_from(b.target.sub_goal).ok()?;
                    let aimed = !off && !b.records.is_empty() && b.target.target[0] >= 0;
                    (aimed || job != 0).then_some((c.id, task, job, aimed.then_some((x, y))))
                })
                .collect();
            // A file whose tasks all came up already adds nothing (a load takes seconds).
            if saved
                .iter()
                .all(|(_, t, j, _)| kinds.contains(t) && *j == 0)
            {
                continue;
            }
            let mut world = World::new(zoo, |e| renderer.shape(e));
            world.set_water(place::water(&terrain::types(&vfs)));
            world.set_tank_config(TankConfig::load(&vfs));
            let mut sim = Sim::new(vfs.ini());
            world.units.set_period(sim.ai.status_period);
            world.units.class = Class::load(&vfs);
            sim.load(&mut world, &cat);
            for (id, task, job, aim) in saved {
                let Some(u) = world.units.get(id) else {
                    continue;
                };
                if active_goal(task, u.kind) < 0 {
                    continue;
                }
                assert_eq!(u.goal.record.task, task, "{member} unit {id}");
                assert_eq!(u.goal.record.job, job, "{member} unit {id}");
                jobs += usize::from(job != 0);
                if let Some((x, y)) = aim {
                    let tile = world.tile_index(x, y);
                    assert_eq!(u.goal.record.target.tile, tile, "{member} unit {id}");
                }
                checked += 1;
                kinds.insert(task);
            }
        }
        // The tasks that both the English and the French install hold (the French one lacks 122).
        let held = [101, 102, 103, 113, 114, 118, 124, 128];
        assert!(
            checked >= 100 && jobs >= 5 && held.iter().all(|t| kinds.contains(t)),
            "{checked} {jobs} {kinds:?}"
        );
    }

    /// The keeper's block in the zoo a save of the world of [`walking`] writes.
    fn saved_block(w: &World) -> UnitBlock {
        let mut cat = crate::entity::Catalogue::default();
        cat.push(crate::entity::EntityType {
            class: "keeper",
            type_name: b"x".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: None,
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        let mut zoo = w.saved(|_| true);
        w.finish_save(&mut zoo, &cat);
        *zoo.entities[0]
            .common
            .as_ref()
            .unwrap()
            .unit
            .clone()
            .unwrap()
    }

    #[test]
    fn a_save_writes_the_whole_record_and_a_load_gives_it_back() {
        let mut w = walking(104, Vec::new(), "");
        let u = w.units.get_mut(1).unwrap();
        u.typed = true;
        let (target, job) = (w.tile_index(3, 1), w.tile_index(2, 2));
        let u = w.units.get_mut(1).unwrap();
        let r = &mut u.goal.record;
        (r.task, r.job, r.done, r.job_done, r.start, r.duration) = (110, 5, true, false, 40, 12);
        (r.target.id, r.job_target.id) = (7, 9);
        u.goal.built(u.kind, 110, 5);
        let r = &mut u.goal.record;
        r.target.pos = target.map(|_| (3 * 64, 64, 16));
        r.job_target.pos = job.map(|_| (2 * 64, 2 * 64, 0));
        let before = r.clone();
        let b = saved_block(&w);
        let t = &b.target;
        assert_eq!(
            (t.goal, t.sub_goal, t.active, t.active_sub),
            (110, 5, 110, 5)
        );
        assert_eq!((t.words, t.bytes, t.extra), ([7, 9], [1, 0], [40, 12]));
        assert_eq!((t.target, t.sub_target), ([192, 64, 16], [128, 128, 0]));
        let back = saved_block_in(104, b);
        let r = &back.units.get(1).unwrap().goal.record;
        assert_eq!((r.task, r.job, r.done, r.job_done), (110, 5, true, false));
        assert_eq!(
            (r.start, r.duration, r.target.id, r.job_target.id),
            (40, 12, 7, 9)
        );
        assert_eq!((r.target.tile, r.job_target.tile), (target, job));
        assert_eq!(r.target.pos, before.target.pos);
    }

    /// A keeper built for task 110 and job 5, then set to task 101 and job 2 (the new objects are
    /// not built yet), saved with `typed` as it is after its first turn.
    fn keeper_that_changed_task(changed: bool) -> World {
        let mut w = walking(104, Vec::new(), "");
        let u = w.units.get_mut(1).unwrap();
        u.typed = true;
        (u.goal.record.task, u.goal.record.job) = (110, 5);
        u.goal.built(u.kind, 110, 5);
        if changed {
            (u.goal.record.task, u.goal.record.job) = (101, 2);
        }
        w
    }

    #[test]
    fn a_save_writes_the_numbers_of_the_objects_the_unit_holds() {
        let t = saved_block(&keeper_that_changed_task(true)).target;
        assert_eq!(
            (t.goal, t.sub_goal, t.active, t.active_sub),
            (101, 2, 110, 5),
            "the old objects while the new ones are not built"
        );
        let t = saved_block(&keeper_that_changed_task(false)).target;
        assert_eq!((t.active, t.active_sub), (110, 5));
        // No object: -1.
        let mut w = keeper_that_changed_task(false);
        w.units.get_mut(1).unwrap().goal = Default::default();
        let t = saved_block(&w).target;
        assert_eq!((t.active, t.active_sub), (-1, -1));
    }

    #[test]
    fn a_unit_saved_before_its_goal_object_is_built_is_initialised_after_a_load() {
        // The saved numbers differ from the record's: nothing is built at the load, the first update
        // builds both objects and runs their init (behaviours 4.7).
        let back = saved_block_in(104, saved_block(&keeper_that_changed_task(true)));
        assert_eq!(back.units.get(1).unwrap().goal.built_pair(), (false, false));
        // A unit that did not change task has its objects built, without init.
        let back = saved_block_in(104, saved_block(&keeper_that_changed_task(false)));
        assert_eq!(back.units.get(1).unwrap().goal.built_pair(), (true, true));
    }

    #[test]
    fn a_save_writes_the_live_path_destination_and_pending_name() {
        let mut w = walking(104, vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]], "bEat");
        let tile = |w: &World, x, y| w.tile_index(x, y).unwrap();
        let live = (vec![tile(&w, 4, 2), tile(&w, 5, 2)], vec![1, -1]);
        let u = w.units.get_mut(1).unwrap();
        u.started = true;
        (u.path, u.levels, u.destination) = (live.0, live.1, Some((5 * 64 + 32, 2 * 64 + 32, 0)));
        u.runner.queue("bDrink", true, true);
        let b = saved_block(&w);
        assert_eq!(b.records, [[4, 2, 1], [5, 2, u32::MAX]]);
        assert_eq!(
            (b.p3, b.pending.as_str()),
            ([5 * 64 + 32, 2 * 64 + 32, 0], "bDrink")
        );
        // No path, no destination, no pending set, no levels: none and -1.
        let one = vec![tile(&w, 4, 2)];
        let u = w.units.get_mut(1).unwrap();
        (u.path, u.levels, u.destination) = (one, Vec::new(), None);
        u.runner.forget();
        let b = saved_block(&w);
        assert_eq!(
            (b.records, b.p3, b.pending),
            (vec![[4, 2, u32::MAX]], [-1; 3], String::new())
        );
        let u = w.units.get_mut(1).unwrap();
        u.path.clear();
        assert!(saved_block(&w).records.is_empty());
        // A unit that has not started keeps the block it was read with.
        let mut w = walking(104, vec![[1, 1, 0], [2, 1, 0]], "bEat");
        w.units.get_mut(1).unwrap().path.clear();
        let b = saved_block(&w);
        assert_eq!((b.records.len(), b.pending.as_str()), (2, "bEat"));
    }

    /// The keeper of `w` as a unit read from a file that has had its first turn.
    fn read_unit(w: &mut World) {
        w.zoo.entities[0].common.as_mut().unwrap().raw = crate::zoo::Raw::read();
        let u = w.units.get_mut(1).unwrap();
        (u.started, u.typed) = (true, true);
    }

    #[test]
    fn a_loaded_unit_that_changed_goal_saves_it_and_a_second_round_trip_agrees() {
        // Stored: task 110, job 5, objects 110 and 5. After the load the unit takes task 101 and
        // job 2 with new targets, done flags, start and duration (objects not built yet).
        let mut stored = UnitBlock::default();
        stored.target.goal = 110;
        stored.target.sub_goal = 5;
        (stored.target.active, stored.target.active_sub) = (110, 5);
        stored.target.extra = [40, 12];
        let mut w = saved_block_in(104, stored.clone());
        read_unit(&mut w);
        let (target, job) = (w.tile_index(3, 1), w.tile_index(2, 2));
        let u = w.units.get_mut(1).unwrap();
        assert_eq!(u.goal.held(), (110, 5));
        let r = &mut u.goal.record;
        (r.task, r.job, r.done, r.job_done, r.start, r.duration) = (101, 2, false, true, 77, 9);
        (r.target.id, r.job_target.id) = (3, 4);
        r.target.pos = target.map(|_| (3 * 64, 64, 16));
        r.job_target.pos = job.map(|_| (2 * 64, 2 * 64, 0));
        let first = saved_block(&w).target;
        assert_eq!(
            (first.goal, first.sub_goal, first.active, first.active_sub),
            (101, 2, 110, 5)
        );
        assert_eq!(
            (first.words, first.bytes, first.extra),
            ([3, 4], [0, 1], [77, 9])
        );
        assert_eq!(
            (first.target, first.sub_target),
            ([192, 64, 16], [128, 128, 0])
        );
        // A load of that block gives the live record back, and saved again before its first turn
        // (the objects of the other numbers are not built until then) it writes the same block.
        let mut back = saved_block_in(104, saved_block(&w));
        let (a, b) = (
            &w.units.get(1).unwrap().goal,
            &back.units.get(1).unwrap().goal,
        );
        let key = |r: &Record| (r.task, r.job, r.done, r.job_done, r.start, r.duration);
        assert_eq!(key(&a.record), key(&b.record));
        assert_eq!(a.record.target.pos, b.record.target.pos);
        back.zoo.entities[0].common.as_mut().unwrap().raw = crate::zoo::Raw::read();
        assert_eq!(saved_block(&back).target, first);
        // A started unit with no goal object (no goal update since the load) keeps its stored record.
        let mut fresh = saved_block_in(104, UnitBlock::default());
        fresh.zoo.entities[0]
            .common
            .as_mut()
            .unwrap()
            .unit
            .as_mut()
            .unwrap()
            .target
            .goal = 116;
        read_unit(&mut fresh);
        fresh.units.get_mut(1).unwrap().goal = Default::default();
        assert_eq!(saved_block(&fresh).target.goal, 116);
        // A unit that has not started keeps the block it was read with.
        let mut idle = saved_block_in(104, stored);
        idle.zoo.entities[0].common.as_mut().unwrap().raw = crate::zoo::Raw::read();
        idle.units.get_mut(1).unwrap().goal.record.task = 101;
        assert_eq!(saved_block(&idle).target.goal, 110);
    }

    #[test]
    fn a_saved_moving_unit_loads_with_the_same_path() {
        let mut w = walking(104, vec![[1, 1, 0], [2, 1, 0], [3, 1, 0]], "");
        let tiles = [(2, 1), (3, 1), (4, 1)].map(|(x, y)| w.tile_index(x, y).unwrap());
        let u = w.units.get_mut(1).unwrap();
        u.started = true;
        (u.path, u.levels) = (tiles.to_vec(), vec![0, 0, 2]);
        u.destination = Some((4 * 64 + 32, 64 + 32, 0));
        u.runner.queue("bSleep", true, true);
        assert_eq!(saved_block(&w).pending_flag, 1);
        let u = w.units.get_mut(1).unwrap();
        u.runner.queue("bSleep", false, true);
        let block = saved_block(&w);
        assert_eq!(block.pending_flag, 0);
        let mut loaded = walking(104, Vec::new(), "");
        loaded.zoo.entities[0].common.as_mut().unwrap().unit = Some(Box::new(block));
        load(&mut loaded, 0);
        let (a, b) = (w.units.get(1).unwrap(), loaded.units.get(1).unwrap());
        assert_eq!(
            (&b.path, &b.levels, b.destination, b.runner.queued()),
            (&a.path, &a.levels, a.destination, Some("bSleep"))
        );
        // The pending call's own flag comes back with it.
        assert!(!b.runner.queued_flag());
    }
}
