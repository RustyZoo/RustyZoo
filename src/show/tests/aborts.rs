//! The aborts of a show and of an item (shows 30.1, 30.5), on the
//! staged show: two animals (21, 22) and a specialist (30) in the show 1001, idle at AI tick 0.

use super::cycle::{heard, info_mut, info_of_show, item, staged, started, tick};
use super::*;
use crate::behaviour::NoData;
use crate::sim::{KEEPER_CHANNEL, Rng, UNIT_CHANNEL, WORLD_CHANNEL};

/// The kinds of the events posted outside the show manager's step, in order.
fn posted(w: &World) -> Vec<u16> {
    w.units.posts.iter().map(|e| e.kind).collect()
}

#[test]
fn a_started_show_aborts_when_its_specialist_is_picked_up_and_the_figures_stay() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5), item(0, 3, 5)]);
    w.units.get_mut(30).unwrap().show = 1001;
    for st in &mut info_mut(&mut w).show.items {
        st.item = 1;
    }
    w.units.ai_ticks = 200;
    events.clear();
    w.units.take_messages();
    crate::goal::picked_up(&mut w, &cat, &mut Rng::default(), &NoData, 30);
    // 0x2717 then 0x2715, from the show, for the next step; one message of the end.
    assert_eq!(posted(&w), [event::ABORTED, event::ENDED]);
    assert!(w.units.posts.iter().all(|e| e.sender == 1001));
    assert_eq!(
        w.units
            .take_messages()
            .iter()
            .filter(|m| m.0 == 11492)
            .count(),
        1
    );
    let info = info_of_show(&w);
    assert!(!info.show.running && info.show.items.is_empty());
    assert_eq!((info.next, info.receipts), (290, [0; 3]));
    for id in [21, 22] {
        let u = w.units.get(id).unwrap();
        assert!(u.back && !u.called && u.goal.record.task == 0, "{id}");
    }
}

#[test]
fn an_idle_show_is_aborted_too_and_its_next_time_is_pushed_back() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().show = 1001;
    w.units.get_mut(21).unwrap().goal.record.task = 5;
    w.units.ai_ticks = 70;
    crate::show::abort_show(&mut w, &cat, 1001);
    assert_eq!(posted(&w), [event::ABORTED, event::ENDED]);
    assert!(w.units.take_messages().is_empty());
    assert_eq!(info_of_show(&w).next, 160);
    // The animals of the species that carry the show id swim home all the same? Not listed: only
    // the current species' list is walked, and an idle show has none.
    assert!(!w.units.get(21).unwrap().back);
}

/// The staged show running, both orcas at item 0 with the cue sent and ready, the second item
/// the same; the orcas skip every trick.
fn skippers() -> (World, Catalogue, Events) {
    let (mut w, mut cat, mut events) = started(vec![item(0, 3, 5), item(0, 3, 5)]);
    cat.push(EntityType {
        name_id: crate::show::tests::cycle::SEAL as i32,
        ..kind(
            "animals",
            "orca",
            "[Characteristics/Integers]\ncSkipTrickHappiness=50\ncSkipTrickChance=100\n[tricks]\ntrick=a\n",
        )
    });
    for e in &mut w.zoo.entities {
        if e.type_name == "seal" {
            e.type_name = "orca".into();
        }
    }
    for st in &mut info_mut(&mut w).show.items {
        (st.item, st.cue_sent, st.ready) = (0, true, true);
    }
    w.units.get_mut(21).unwrap().goal.record.task = 5;
    events.clear();
    (w, cat, events)
}

#[test]
fn an_item_the_animal_refuses_is_aborted_and_skipped_without_a_reaction() {
    let (mut w, cat, mut events) = skippers();
    tick(&mut w, &cat, &mut events, 91);
    // 0x2723 to the world manager, then to the animal.
    let told: Vec<(u16, u32, u8)> = heard(&events)
        .iter()
        .filter(|e| e.0 == event::ITEM_ABORTED)
        .map(|e| (e.0, e.2, e.3))
        .collect();
    assert_eq!(
        told[..2],
        [(0x2723, 0, WORLD_CHANNEL), (0x2723, 21, UNIT_CHANNEL)]
    );
    let st = &info_of_show(&w).show.items[0];
    assert!(st.finished && st.aborted && st.item == 0);
    // The next pass: no reaction, no figure, the item advanced, the cue posted, the goal reset.
    tick(&mut w, &cat, &mut events, 92);
    let sent = heard(&events);
    let reactions = [event::BAD, event::NEUTRAL, event::GOOD, event::GREAT];
    assert!(sent.iter().all(|e| !reactions.contains(&e.0)));
    assert!(
        sent.iter()
            .any(|e| e.0 == event::NEXT_ITEM && e.3 == KEEPER_CHANNEL)
    );
    let show = &info_of_show(&w).show;
    assert_eq!((show.possible, show.done), (0, 0));
    assert!(
        show.items
            .iter()
            .all(|s| s.item == 1 && !s.aborted && !s.finished)
    );
    assert_eq!(w.units.get(21).unwrap().goal.record.task, 0);
}
