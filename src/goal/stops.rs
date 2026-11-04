//! The stops of a unit's goal: a worker losing its task, a unit stopped with the goal reset, a
//! user pushed out of its building and a guest put back in its use.

use super::{ENTER_BUILDING, EXIT_BUILDING, Record, Target, USE};
use crate::unit::{Part, Unit};

/// A maintenance worker loses its task (staff 8.3, 8.9): the link, the target and the goal go, the
/// worker stands, and the ladder gives it the duties again at its next update.
pub(crate) fn lose_task(unit: &mut Unit) {
    if let Part::Staff(s) = &mut unit.part {
        (s.task, s.target) = (0, None);
    }
    unit.goal.record = Record::default();
    (unit.goal.stale, unit.goal.sub_stale) = (true, true);
    crate::unit::movement::stop(unit);
    unit.destination = None;
}

/// A unit stops everything with the goal reset (staff 7.11, unit placing 9.1): its record and its
/// need flags go, its walk and its set stop, and its hold is lowered.
pub(crate) fn stop_all(unit: &mut Unit) {
    unit.goal.record = Record::default();
    (unit.goal.stale, unit.goal.sub_stale) = (true, true);
    unit.goal.wants_to_decide = false;
    crate::unit::movement::stop(unit);
    unit.destination = None;
    unit.runner = Default::default();
    unit.mover.emergency = false;
    unit.wait = 0;
    if let Part::Animal(a) = &mut unit.part {
        (a.drinking, a.wants_food, a.thirsty, a.walk) = (false, false, false, None);
        a.sleeping = false;
    }
}

/// A user is pushed out of its building (guest buildings 6.7, 11.1): its walk, its set and its goal
/// stop and a guest forgets the building it came for; the ladder gives it a task at its next
/// update.
pub(crate) fn stop_use(unit: &mut Unit) {
    unit.goal.record = Record::default();
    (unit.goal.stale, unit.goal.sub_stale) = (true, true);
    unit.goal.wants_to_decide = false;
    crate::unit::movement::stop(unit);
    unit.destination = None;
    (unit.through_building, unit.aborted) = (false, false);
    unit.runner = Default::default();
    unit.use_set = None;
    unit.animation = Default::default();
    match &mut unit.part {
        Part::Guest(g) => (g.need, g.building, g.stand) = (None, 0, false),
        Part::Animal(a) => (a.building, a.toy) = (0, false),
        _ => {}
    }
}

/// A guest listed as a user of slot `slot` of building `id` in a zoo just loaded goes on with its
/// use (task 0x6a, guest buildings 4.1, 6.5), as if it had entered at AI tick `now`: the walk and
/// the set it ran stop and the building is its target (for an animal, its shelter). A unit saved
/// entering or leaving (`saved` 0x68 or 0x69) keeps that task; one saved in any other task (a
/// record without a unit block counts as using) ends its use at once (guest buildings 4.2).
pub(crate) fn resume_use(unit: &mut Unit, id: u32, slot: usize, now: u32, saved: Option<i32>) {
    stop_use(unit);
    let kept = saved.and_then(|t| u8::try_from(t).ok());
    let kept = kept.filter(|t| [ENTER_BUILDING, EXIT_BUILDING, USE].contains(t));
    unit.goal.record = Record {
        task: kept.unwrap_or(USE),
        mode: u8::from(kept == Some(EXIT_BUILDING)),
        done: saved.is_some() && kept.is_none(),
        target: Target {
            id,
            ..Target::default()
        },
        start: now,
        ..Record::default()
    };
    match &mut unit.part {
        Part::Guest(g) => (g.building, g.slot) = (id, slot),
        Part::Animal(a) => (a.building, a.slot) = (id, slot),
        _ => {}
    }
}
