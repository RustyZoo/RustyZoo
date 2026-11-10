//! The units of a zoo just loaded go on with their saved behaviour set (behaviours 4.7): a mover,
//! a sleeper, a member of staff, whatever it was doing. A unit that was entering, using or leaving
//! a building that is gone is stopped instead. The saved path and pending set name come back with
//! the set, so a restored move call walks on, and so do the goal record (task, job, targets, done flags,
//! start, duration) and the 3D target name, whatever the task (behaviours 4.7), and the frightened
//! flag and the kept predator.

use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, Record, USE, active_goal, stop_use};
use crate::unit::Part;
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
/// included; and the destination point of the record, kept whatever the path (nothing at load
/// clears it, save games 15.8). A node off the map makes the whole way empty: nothing could walk it.
pub(crate) fn saved_way(world: &World, block: &UnitBlock) -> Way {
    let at = |n: u32| i32::try_from(n).ok();
    let tile = |r: &[u32; 3]| world.tile_index(at(r[0])?, at(r[1])?);
    let [x, y, z] = block.p3;
    let there = (x >= 0 && y >= 0).then_some((x, y, z));
    let Some(path) = (block.records.iter().map(tile)).collect::<Option<Vec<_>>>() else {
        return Default::default();
    };
    let levels = if world.zoo.version >= 84 {
        block.records.iter().map(|r| r[2] as i32).collect()
    } else {
        Vec::new()
    };
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
pub(crate) fn load(world: &mut World, now: u32) {
    for at in 0..world.zoo.entities.len() {
        let Some(c) = world.zoo.entities[at].common.as_ref() else {
            continue;
        };
        let Some(block) = c.unit.as_deref() else {
            continue;
        };
        let (id, set, tile) = (c.id, block.behaviour.clone(), block.building);
        let flier = world.zoo.entities[at].class == "helicopter";
        let (way, pending) = (saved_way(world, block), block.pending.clone());
        let (flag, held) = (block.pending_flag != 0, block.b7 != 0);
        let moving = (
            block.b8 != 0,
            block.w5,
            (world.zoo.version >= 98).then_some(block.short),
        );
        let [trick, show] = block.flags.map(|f| if f.0 == 1 { f.1 } else { 0 });
        let (goal, name) = (block.target.clone(), block.text.clone());
        let fright = (block.b1 != 0, block.w1, block.b2 != 0);
        let call = (block.b5, block.b6);
        let (cost, need, slot) = (block.w4, block.need, block.slot);
        let own = (block.p1, crate::unit::own::tile_under(world, block.p1));
        let task = u8::try_from(block.target.goal).ok();
        // A helicopter's building tasks are its own (staff 7.11.1): it is no user of the building here.
        let inside =
            !flier && task.is_some_and(|t| [ENTER_BUILDING, EXIT_BUILDING, USE].contains(&t));
        let here = tile
            .iter()
            .all(|&n| n >= 0)
            .then(|| world.tile_index(tile[0], tile[1]))
            .flatten();
        let present = here.is_some_and(|t| building_on(world, t));
        // The heal hold is the base block's first byte (staff 7.7, zoo format 6.14).
        if let Some(unit) = world.units.get_mut(id) {
            (unit.held, unit.show, unit.trick) = (held, show, trick);
            // The walk's live state is read as stored (behaviours 5.10.2 point 1); before version
            // 98 the animation speed keeps the creation value.
            (unit.moving, unit.speed) = (moving.0, moving.1);
            if let Some(percent) = moving.2 {
                unit.anim_speed = percent;
            }
            // The need being served comes back as it was saved (shows 32.1 point 3).
            if let Part::Animal(a) = &mut unit.part {
                (a.need, a.slot) = (need, usize::from(slot));
                // The position is read; the tile is the one under it (shows 32.2 point 6).
                (unit.own_pos, unit.own_tile) = (own.0, own.1);
            }
            (unit.called, unit.back) = (call.0 != 0, call.1 != 0);
            (unit.frightened, unit.predator, unit.caught) = fright;
            // A guest runs with the non-path cost it was saved with (pathfinding 6.5).
            if unit.kind == crate::unit::Kind::Guest {
                unit.mover.word = Some(cost);
            }
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

/// The idle animation of every loaded unit before any step (behaviours 5.10.1 point 3): the add of
/// each, then the tank check of the load, which resets a unit in a tank only when its mode changes;
/// then the move reset of a unit saved moving (5.10.2 point 3).
/// The tank mode is not saved, so every unit starts in mode 0.
pub(crate) fn settle(world: &mut World, data: &dyn crate::behaviour::Data) {
    for at in 0..world.zoo.entities.len() {
        let Some(id) = world.zoo.entities[at].common.as_ref().map(|c| c.id) else {
            continue;
        };
        // A unit that already shows something was settled by an earlier load (the animals are made
        // again when the seed is told, the other units stay).
        let Some(slot) = world
            .units
            .get_mut(id)
            .filter(|u| u.animation.name.is_empty())
        else {
            continue;
        };
        let mut unit = std::mem::replace(slot, crate::unit::Unit::new(id, slot.kind));
        crate::unit::movement::read_type(&mut unit, data, &world.zoo.entities[at]);
        crate::unit::swim::added(&mut unit, world, data, at);
        if unit.moving {
            crate::unit::movement::reset(&mut unit, world, data, at);
        }
        if let Some(slot) = world.units.get_mut(id) {
            *slot = unit;
        }
    }
}

#[cfg(test)]
mod tests;
