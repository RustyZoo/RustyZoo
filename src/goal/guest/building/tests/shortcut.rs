//! The sit and trash shortcut of the energy seek and the first trash search (goals 12.2.1 point 7).

use super::*;

/// Guest 1 at (2, 6), tired, on its way to bench 9 (served need `energy`), which is then deleted;
/// bench 10 at (8, 6) has an open slot; its entrance (8, 7) is walled in when `blocked`.
pub(super) fn benches(blocked: bool) -> (Catalogue, World) {
    let (cat, mut w) = (types(), bench());
    let mut other = entity("building", "building", 10, centre((8, 6)), 4);
    other.subtype = "bench".into();
    w.zoo.entities.push(other);
    unreachable(&mut w, blocked);
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.need, g.building) = (Some("energy"), 9);
    }
    w.units.get_mut(1).unwrap().goal.record.task = GO;
    delete(&mut w, &cat, 9);
    (cat, w)
}

/// Walls the entrance tile (8, 7) in: every tile round it has a step on all four edges.
pub(super) fn unreachable(w: &mut World, walled: bool) {
    for (x, y) in (7..=9).flat_map(|x| (6..=8).map(move |y| (x, y))) {
        let t = w.tile_index(x, y).unwrap();
        w.zoo.tiles[t].edges = 0xff * u8::from(walled);
    }
}

/// The seek of an energy check; the thoughts, the task, the wandered flag, the building target and
/// the draws made (the generator advanced that many times from its start).
fn seek(cat: &Catalogue, w: &mut World, need: &str) -> (Vec<i32>, u8, bool, u32, usize) {
    let mut rng = Rng::default();
    with_rng(cat, w, &walks(), &mut rng, |cx| match need {
        "trash" => cx.seek_trash(),
        _ => cx.seek("energy"),
    });
    let mut reference = Rng::default();
    let draws = (0..50).find(|_| {
        let same = reference == rng;
        reference.rand();
        same
    });
    let wandered = w.units.get(1).unwrap().goal.wandered;
    (
        thoughts(w),
        task(w),
        wandered,
        guest(w).building,
        draws.unwrap_or(99),
    )
}

#[test]
fn a_bench_deleted_on_the_way_does_not_stop_the_walk_or_the_energy_check() {
    let (cat, mut w) = benches(false);
    // The walker is untouched: the need stays, so the energy does not grow, and the goal goes on.
    assert_eq!(
        (guest(&w).need, guest(&w).building, task(&w)),
        (Some("energy"), 9, GO)
    );
    assert!(with_cx(&cat, &mut w, |cx| !cx.heads_for_need()));
    // (d) The other bench is reachable: a plain new trip to it, no second thought.
    let (thought, task, wandered, building, _) = seek(&cat, &mut w, "energy");
    assert_eq!(
        (thought, task, wandered, building),
        (vec![10092], GO, false, 10)
    );
    assert_eq!(guest(&w).need, Some("energy"));
}

#[test]
fn a_failed_new_trip_goes_into_the_sit_shortcut_and_the_caller_thinks_again() {
    // (a) Bench 10 has a slot but no path to its entrance: the shortcut's search draws once, its
    // update fails, and its thought comes before the caller's.
    let (cat, mut w) = benches(true);
    let (thought, task, wandered, building, draws) = seek(&cat, &mut w, "energy");
    assert_eq!(
        (thought, task, wandered, building),
        (vec![10093, 10093, 10092], 0, true, 0)
    );
    // The tie draw of the nearest search and the any search's.
    assert_eq!(draws, 2);
    // (b) The same state with no need being served: no shortcut, one thought, one draw.
    let (cat, mut w) = benches(true);
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        g.need = None;
    }
    let (thought, task, wandered, _, draws) = seek(&cat, &mut w, "energy");
    assert_eq!(
        (thought, task, wandered, draws),
        (vec![10093, 10092], 0, true, 1)
    );
}

#[test]
fn a_can_deleted_on_the_way_drops_the_litter_before_the_second_search() {
    // (c) The only other can has no path to its entrance: the shortcut's search takes it, its
    // update fails and the litter is put down.
    let (cat, mut w) = (types(), littered(true));
    w.zoo
        .entities
        .push(entity("building", "can", 12, centre((8, 6)), 4));
    unreachable(&mut w, true);
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.need, g.building) = (Some("trash"), 11);
    }
    w.units.get_mut(1).unwrap().goal.record.task = GO;
    delete(&mut w, &cat, 11);
    assert_eq!(pieces(&w), 0);
    let (_, task, wandered, building, draws) = seek(&cat, &mut w, "trash");
    assert_eq!((task, wandered, building), (0, true, 0));
    // The shortcut dropped it, and the trash check's second search drops again (the drop makes a
    // piece whether or not one is held, guests 10 point 3).
    assert_eq!((pieces(&w), guest(&w).items[4]), (2, None));
    assert_eq!(draws, 5);
}
