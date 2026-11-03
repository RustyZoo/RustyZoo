//! The animal's part of the cycle: the events it reads and the trick the show asks it to start
//! (shows 23.1, 23.2, 24).

use super::cycle::{heard, info_mut, item, staged, tick};
use super::*;
use crate::show::performer::read_events;
use crate::sim::{Event, SHOW_CHANNEL, UNIT_CHANNEL};
use crate::zoo::ItemState;

fn state() -> ItemState {
    ItemState {
        script: 2000,
        animal: 21,
        item: 0,
        finished: false,
        aborted: false,
        cue_sent: true,
        ready: false,
        started: false,
        done: false,
    }
}

fn post(events: &mut Events, kind: u16, target: u32, channel: u8) {
    events.post(Event {
        sender: 1001,
        target,
        data: 0,
        extra: 0,
        kind,
        delay: 1,
        sender_channel: SHOW_CHANNEL,
        channel,
    });
}

/// What the animal 21 makes of the events `sent` to it: its flags (finished, aborted, ready) and
/// whether its goal was reset.
fn read(sent: &[u16]) -> ((bool, bool, bool), bool) {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().goal.record.task = 5;
    let mut events = Events::default();
    sent.iter()
        .for_each(|&k| post(&mut events, k, 21, UNIT_CHANNEL));
    events.age();
    let mut st = state();
    read_events(&mut w, &cat, &events, 21, &mut st);
    let reset = w.units.get(21).unwrap().goal.record.task == 0;
    ((st.finished, st.aborted, st.ready), reset)
}

#[test]
fn the_animal_reads_the_cue_its_trick_done_its_failure_and_its_item_aborted() {
    // (events, finished, aborted, ready, goal reset)
    let rows = [
        (vec![event::CUE], (false, false, true), false),
        (vec![event::FINISHED], (true, false, false), true),
        (vec![event::FAILED], (true, true, false), false),
        (vec![event::ITEM_ABORTED], (false, false, false), true),
        (
            vec![event::NEXT_ITEM, event::PREPARE],
            (false, false, false),
            false,
        ),
        (vec![event::CUE, event::FINISHED], (true, false, true), true),
        (vec![], (false, false, false), false),
    ];
    for (sent, flags, reset) in rows {
        assert_eq!(read(&sent), (flags, reset), "{sent:?}");
    }
}

#[test]
fn the_animal_reads_only_the_events_aimed_at_it_on_its_channel() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut events = Events::default();
    post(&mut events, event::CUE, 22, UNIT_CHANNEL);
    post(&mut events, event::CUE, 0, UNIT_CHANNEL);
    post(&mut events, event::CUE, 21, SHOW_CHANNEL);
    events.age();
    let mut st = state();
    read_events(&mut w, &cat, &events, 21, &mut st);
    assert!(!st.ready);
}

/// The staged show running, both seals at item `n` with their cue sent and ready.
fn cued(items: Vec<crate::zoo::Item>, n: i16) -> (World, Catalogue, Events) {
    let (mut w, cat) = staged(items);
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 90);
    for st in &mut info_mut(&mut w).show.items {
        (st.item, st.cue_sent, st.ready) = (n, true, true);
    }
    (w, cat, events)
}

#[test]
fn the_show_asks_the_animal_to_start_the_trick_of_its_item_by_the_item_s_id() {
    let mut second = item(0, 3, 5);
    second.id = 11201;
    second.names[0] = "second".into();
    let (mut w, cat, mut events) = cued(vec![item(0, 3, 5), second], 1);
    tick(&mut w, &cat, &mut events, 91);
    let u = w.units.get(21).unwrap();
    assert_eq!(
        (u.trick, u.show, u.runner.top()),
        (11201, 1001, Some("bsecond"))
    );
    assert!(
        info_mut(&mut w)
            .show
            .items
            .iter()
            .all(|s| s.started && !s.aborted)
    );
}

#[test]
fn two_items_of_one_trick_id_start_the_first_one() {
    let (mut first, mut second) = (item(0, 3, 5), item(0, 3, 5));
    (first.names[0], second.names[0]) = ("first".into(), "second".into());
    let (mut w, cat, mut events) = cued(vec![first, second], 1);
    tick(&mut w, &cat, &mut events, 91);
    assert_eq!(w.units.get(21).unwrap().runner.top(), Some("bfirst"));
}

#[test]
fn an_item_whose_trick_cannot_start_aborts_for_that_animal() {
    // A trick of a type no trick has: the start answers 3 and the item is aborted (shows 5.4).
    let (mut w, cat, mut events) = cued(vec![item(4, 3, 5)], 0);
    tick(&mut w, &cat, &mut events, 91);
    let st = &info_mut(&mut w).show.items[0];
    assert!(st.finished && st.aborted && !st.started);
    assert_eq!(w.units.get(21).unwrap().trick, 0);
    assert!(
        heard(&events)
            .iter()
            .any(|e| e.0 == event::ITEM_ABORTED && e.2 == 21)
    );
}
