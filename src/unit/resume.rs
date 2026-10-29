//! The units of a zoo just loaded go on with their saved behaviour set (behaviours 4.7): a mover,
//! a sleeper, a member of staff, whatever it was doing. A unit that was entering, using or leaving
//! a building that is gone is stopped instead.

use crate::goal::{ENTER_BUILDING, EXIT_BUILDING, USE, stop_use};
use crate::world::World;

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

/// Gives every loaded unit its saved set, or stops it when its saved task is in a building and
/// the stored tile of that building holds none (shown, goal reset, path emptied, set and pending
/// name cleared). A unit whose building is there is the building's: its users pair decides
/// ([`crate::building::users::restore`], which runs after this).
pub fn load(world: &mut World) {
    for at in 0..world.zoo.entities.len() {
        let Some(c) = world.zoo.entities[at].common.as_ref() else {
            continue;
        };
        let Some(block) = c.unit.as_deref() else {
            continue;
        };
        let (id, set, tile) = (c.id, block.behaviour.clone(), block.building);
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
        load(&mut w);
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
}
