//! The call to a show, the arrival and the release of an animal (shows 18.2, 21.1, 24).

use super::cycle::{SEAL, info_mut, info_of_show, item, staged, started, tick};
use super::*;
use crate::show::performer::{read_events, release};
use crate::sim::{Event, Events, UNIT_CHANNEL};
use crate::zoo::ItemState;

fn state(animal: u32) -> ItemState {
    ItemState {
        script: 2000,
        animal,
        item: -1,
        finished: false,
        aborted: false,
        cue_sent: false,
        ready: false,
        started: false,
        done: false,
    }
}

/// An event for `target` that the next read finds.
fn heard(events: &mut Events, kind: u16, target: u32) {
    events.post(Event {
        sender: 9,
        target,
        data: 0,
        extra: 0,
        kind,
        delay: 1,
        sender_channel: UNIT_CHANNEL,
        channel: UNIT_CHANNEL,
    });
    events.age();
}

#[test]
fn the_start_calls_every_animal_and_the_call_raises_its_flag_and_lowers_the_return() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().back = true;
    // The show posts the call at its first run (shows 5.4) and reads it in the next step.
    tick(&mut w, &cat, &mut events, 91);
    assert!(!w.units.get(21).unwrap().called);
    tick(&mut w, &cat, &mut events, 92);
    for id in [21, 22] {
        let u = w.units.get(id).unwrap();
        assert!(u.called && !u.back, "{id}");
    }
}

#[test]
fn only_the_events_for_the_animal_are_read() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut st = state(21);
    let mut events = Events::default();
    heard(&mut events, crate::show::event::CALL, 22);
    heard(&mut events, crate::show::event::CALL, 0);
    read_events(&mut w, &cat, &events, 21, &mut st);
    assert!(!w.units.get(21).unwrap().called);
    heard(&mut events, crate::show::event::CALL, 21);
    read_events(&mut w, &cat, &events, 21, &mut st);
    assert!(w.units.get(21).unwrap().called);
}

#[test]
fn the_arrival_resets_the_goal_and_sets_the_item_finished_and_ready() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().goal.record.task = crate::goal::GO_TO_SHOW;
    let mut st = state(21);
    let mut events = Events::default();
    heard(&mut events, crate::show::event::ARRIVED, 21);
    read_events(&mut w, &cat, &events, 21, &mut st);
    assert!(st.finished && st.ready && !st.aborted);
    assert_eq!(w.units.get(21).unwrap().goal.record.task, crate::goal::NONE);
}

#[test]
fn a_show_that_stops_resets_its_animals_and_sends_them_home() {
    let (mut w, cat, _) = started(vec![item(0, 3, 5)]);
    for id in [21, 22] {
        let u = w.units.get_mut(id).unwrap();
        (u.called, u.goal.record.task) = (true, crate::goal::GO_TO_SHOW);
    }
    crate::show::abort_show(&mut w, &cat, 1001);
    for id in [21, 22] {
        let u = w.units.get(id).unwrap();
        assert!(
            u.back && !u.called && u.goal.record.task == crate::goal::NONE,
            "{id}"
        );
    }
    assert!(info_of_show(&w).show.items.is_empty());
}

#[test]
fn releasing_an_animal_sends_it_home_and_forgets_its_call() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().called = true;
    release(&mut w, &cat, 21);
    let u = w.units.get(21).unwrap();
    assert!(u.back && !u.called);
}

#[test]
fn an_animal_is_in_a_show_when_it_performs_and_is_on_its_way_home_or_held_by_the_show() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let is = |w: &World, id| in_show_now(w, &cat, w.units.get(id).unwrap(), w.find(id).unwrap());
    // Started in the show, which holds no state for it yet: no.
    w.units.get_mut(21).unwrap().show = 1001;
    assert!(!is(&w, 21));
    info_mut(&mut w).show.items.push(state(21));
    assert!(is(&w, 21));
    // On its way home it is one with no show at all.
    w.units.get_mut(22).unwrap().back = true;
    assert!(is(&w, 22));
    // The specialist is no performer, whatever his flags.
    let keeper = w.units.get_mut(30).unwrap();
    (keeper.show, keeper.back) = (1001, true);
    assert!(!is(&w, 30));
}

#[test]
fn in_show_reads_the_type_at_the_place_it_is_given_and_only_for_an_animal() {
    // Shows 32.5 point 3: no search of the entity list by id.
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().back = true;
    let (unit, own) = (w.units.get(21).unwrap(), w.find(21).unwrap());
    assert!(in_show_now(&w, &cat, unit, own));
    // The place of the specialist (30): a type with no tricks.
    assert!(!in_show_now(&w, &cat, unit, w.find(30).unwrap()));
    // The specialist is no animal: not even its own place makes it a performer.
    let keeper = w.units.get_mut(30).unwrap();
    keeper.back = true;
    assert!(!in_show_now(&w, &cat, w.units.get(30).unwrap(), own));
}

#[test]
fn the_checks_of_an_animal_read_whether_it_is_in_a_show_as_it_is_now() {
    use crate::behaviour::NoData;
    use crate::unit::{Ctx, Hooks, Part, Standard};
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let (events, mut removals, mut rng) =
        (Events::default(), Vec::new(), crate::sim::Rng::default());
    for back in [true, false] {
        w.units.get_mut(21).unwrap().back = back;
        let mut unit = w.units.get(21).unwrap().clone();
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &cat,
        };
        let at = w.find(21).unwrap();
        Standard { data: &NoData }.checks(&mut unit, &mut w, &mut ctx, at);
        let Part::Animal(a) = &unit.part else {
            panic!("not an animal")
        };
        assert_eq!(a.in_show, back);
    }
}

#[test]
fn an_animal_in_a_show_or_a_land_species_in_a_tank_cannot_act_here() {
    // Animals-needs 2: not in a show, and an underwater species acts anywhere, else on a dry
    // tile, in land mode, outside a tank.
    use crate::animal::life::can_act;
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut under = Catalogue::default();
    under.push(EntityType {
        name_id: SEAL as i32,
        ..kind(
            "animals",
            "seal",
            "[tricks]\ntrick=a\n[Characteristics/Integers]\ncUnderwater=1\n",
        )
    });
    let acts =
        |w: &World, cat: &Catalogue| can_act(w.units.get(21).unwrap(), w, cat, w.find(21).unwrap());
    // In the tank: a land species cannot, an underwater one can.
    assert!(!acts(&w, &cat) && acts(&w, &under));
    // On its way home to a show it is in one: neither can.
    w.units.get_mut(21).unwrap().back = true;
    assert!(!acts(&w, &cat) && !acts(&w, &under));
    w.units.get_mut(21).unwrap().back = false;
    // An egg cannot; on the dry grounds a land species can, in land mode only.
    let at = w.find(21).unwrap();
    w.place_unit(at, (10 * 64 + 32, 6 * 64 + 32, 0), 0);
    assert!(acts(&w, &cat));
    if let Part::Animal(a) = &mut w.units.get_mut(21).unwrap().part {
        a.terrain = 1;
    }
    assert!(!acts(&w, &cat) && acts(&w, &under));
    if let Part::Animal(a) = &mut w.units.get_mut(21).unwrap().part {
        (a.terrain, a.egg) = (0, true);
    }
    assert!(!acts(&w, &cat) && !acts(&w, &under));
}
