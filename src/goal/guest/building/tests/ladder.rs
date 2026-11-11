//! The guest ladder after the sick rung (goals 10.2.1 points 1 to 8).

use super::shortcut::benches;
use super::*;
use crate::goal::guest::LEAVE;
use crate::goal::{EAT, RANDOM, SITE};

/// Runs the guest ladder on guest 1 with the task and done flag it has now.
fn ladder(cat: &Catalogue, w: &mut World, rng: &mut Rng) {
    with_rng(cat, w, &walks(), rng, |cx| {
        let r = cx.record();
        let (task, done) = (r.task, r.done);
        cx.guest_ladder(task, done)
    });
}

fn set(w: &mut World, task: u8, done: bool) {
    let r = &mut w.units.get_mut(1).unwrap().goal.record;
    (r.task, r.done) = (task, done);
}

fn edit(w: &mut World, f: impl FnOnce(&mut Guest)) {
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        f(g);
    }
}

fn wandered(w: &World) -> bool {
    w.units.get(1).unwrap().goal.wandered
}

#[test]
fn eat_standing_without_the_first_item_resets_and_ends_the_ladder() {
    let (cat, mut w) = (types(), zoo());
    // Only a drink in hand: the first entry is empty.
    edit(&mut w, |g| {
        g.stand = true;
        g.items[1] = Some(6304);
    });
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!((task(&w), guest(&w).stand), (0, false));
}

#[test]
fn a_done_task_other_than_the_site_is_kept() {
    let (cat, mut w) = (types(), zoo());
    set(&mut w, EAT, true);
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!(task(&w), EAT);
}

#[test]
fn a_site_task_still_there_becomes_a_random_walk() {
    let (cat, mut w) = (types(), zoo());
    set(&mut w, SITE, false);
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!(task(&w), RANDOM);
}

#[test]
fn a_guest_out_of_a_building_with_nothing_in_hand_stands() {
    let (cat, mut w) = (types(), zoo());
    set(&mut w, EXIT_BUILDING, true);
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!((task(&w), wandered(&w)), (EXIT_BUILDING, true));
    // With food it looks for a place to eat, with no reset.
    let mut w = zoo();
    set(&mut w, EXIT_BUILDING, true);
    edit(&mut w, |g| g.items[0] = Some(6303));
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!((task(&w), wandered(&w)), (EAT, false));
}

#[test]
fn a_walk_to_a_building_with_no_way_in_runs_the_building_step_and_the_guest_stands() {
    // (b) Task 0x66 with an empty path to bench 10, which has no path to its entrance.
    let (cat, mut w) = benches(true);
    edit(&mut w, |g| (g.need, g.building) = (Some("energy"), 10));
    set(&mut w, GO, false);
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!(task(&w), 0);
    // The reset raised the flag; the 1-in-15 draw lowers it and wants to decide.
    let u = w.units.get(1).unwrap();
    assert!(u.goal.wandered || u.goal.wants_to_decide);
    assert_eq!(guest(&w).building, 0);
    // (a) A leaving guest keeps its building target and its task.
    let (cat, mut w) = benches(true);
    edit(&mut w, |g| (g.need, g.building) = (Some("energy"), 10));
    set(&mut w, LEAVE, false);
    ladder(&cat, &mut w, &mut Rng::default());
    assert_eq!((task(&w), guest(&w).building), (LEAVE, 10));
}
