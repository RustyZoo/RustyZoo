//! The goal record of a loaded unit (behaviours 4.7).

use crate::goal::{Record, active_goal};
use crate::world::World;
use crate::zoo::Target;

/// The goal record of unit `id` as saved (`block`, zoo format 6.14): task and job with their targets
/// and done flags, start and duration, and the 3D target `name` the unit's move calls compare.
/// From version 72 the goal and job objects the file holds are built and not initialised again
/// ([`crate::goal::Goals::built`], behaviours 4.7). A task that has no goal object for the kind of
/// unit is not restored (the save writes none for it, save games S7): the unit keeps the goal it
/// has.
pub(super) fn goal_back(
    world: &mut World,
    id: u32,
    now: u32,
    block: &Target,
    version: u32,
    name: &str,
) {
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
