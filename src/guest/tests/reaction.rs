//! The guest's reactions to the events of a show (shows 6.4).

use super::*;
use crate::behaviour::{Data, NoData};
use crate::show::event;
use crate::sim::{BUILDING_CHANNEL, Event, Events, UNIT_CHANNEL};
use crate::unit::{Ctx, Unit};

fn heard(kind: u16, data: u32, extra: u16) -> Event {
    Event {
        sender: 7,
        target: 1,
        data,
        extra,
        kind,
        delay: 0,
        sender_channel: BUILDING_CHANNEL,
        channel: UNIT_CHANNEL,
    }
}

/// A guest with 50 happiness and 80 energy, running the set `top`, reads `events`.
fn read(top: Option<&str>, events: Vec<Event>) -> (World, Unit) {
    let mut w = crate::unit::testing::world_with((4, 4), |zoo| {
        let e = entity("guests", "man", 1, centre((1, 1)), 0);
        zoo.entities.push(e);
    });
    let mut cat = Catalogue::default();
    cat.push(man());
    let at = w.find(1).unwrap();
    let mut unit = w.units.get(1).unwrap().clone();
    unit.part = Part::Guest(Guest {
        values: [50, 0, 0, 80, 0],
        ..Guest::default()
    });
    if let Some(top) = top {
        let env = crate::behaviour::Env {
            data: &NoData,
            unit: &w.zoo.entities[at],
            rng: &mut Rng::default(),
        };
        unit.runner
            .start(&mut unit.animation, &env, top, false, true, true);
    }
    unit.events = events;
    let (queue, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &queue,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::guest::reaction::react(&mut unit, &mut w, &mut ctx, at, &NoData as &dyn Data);
    (w, unit)
}

fn values(unit: &Unit) -> ([i32; 5], [i32; 5]) {
    let Part::Guest(g) = &unit.part else { panic!() };
    (g.values, g.pending)
}

/// The sets the guest started, in order.
fn sets(unit: &Unit) -> Vec<&str> {
    (unit.animation.notes.iter())
        .filter_map(|n| match n {
            crate::behaviour::Note::Set(name) => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

fn thoughts(w: &mut World) -> Vec<String> {
    w.units.take_guest_lines()
}

#[test]
fn an_end_scores_the_four_bands_with_a_thought_and_a_set_each() {
    // The default thresholds are 25, 50 and 75 percent; the data is unsigned.
    for (percent, thought, set) in [
        (0, 10287, "bExitShowBad"),
        (25, 10287, "bExitShowBad"),
        (26, 10288, "bExitShow"),
        (49, 10288, "bExitShow"),
        (50, 10289, "bExitShowGood"),
        (74, 10289, "bExitShowGood"),
        (75, 10290, "bExitShowGreat"),
        (u32::MAX, 10290, "bExitShowGreat"),
    ] {
        let (mut w, unit) = read(None, vec![heard(event::ENDED, percent, 0)]);
        assert_eq!(
            thoughts(&mut w),
            [format!("thought 1 {thought}")],
            "{percent}"
        );
        assert_eq!(unit.runner.top(), Some(set), "{percent}");
    }
}

#[test]
fn an_end_takes_the_rest_bonus_from_the_energy_at_once() {
    let (_, unit) = read(None, vec![heard(event::ENDED, 60, 30)]);
    assert_eq!(values(&unit), ([50, 0, 0, 50, 0], [0; 5]));
    // The bonus is unsigned 16 bits and the energy stops at 0.
    let (_, unit) = read(None, vec![heard(event::ENDED, 60, 65535)]);
    assert_eq!(values(&unit).0[ENERGY], 0);
}

#[test]
fn a_trick_gives_happiness_at_once_then_a_thought_and_its_set() {
    for (kind, gain, thought, set) in [
        (event::BAD, -2, 10610, "bBadTrick"),
        (event::GOOD, 5, 10612, "bGoodTrick"),
        (event::GREAT, 9, 10613, "bGreatTrick"),
    ] {
        let (mut w, unit) = read(None, vec![heard(kind, gain as u32, 0)]);
        assert_eq!(values(&unit), ([50 + gain, 0, 0, 80, 0], [0; 5]));
        assert_eq!(thoughts(&mut w), [format!("thought 1 {thought}")]);
        assert_eq!(unit.runner.top(), Some(set));
    }
}

#[test]
fn a_neutral_trick_and_any_other_event_change_nothing() {
    for kind in [event::NEUTRAL, event::STARTED, 0x2722] {
        let (mut w, unit) = read(None, vec![heard(kind, 9, 9)]);
        assert_eq!(values(&unit), ([50, 0, 0, 80, 0], [0; 5]));
        assert!(thoughts(&mut w).is_empty() && unit.runner.top().is_none());
    }
}

#[test]
fn an_abort_sends_the_guest_out_and_the_next_events_only_change_its_mood() {
    let (mut w, unit) = read(
        None,
        vec![
            heard(event::GOOD, 4, 0),
            heard(event::ABORTED, 0, 0),
            heard(event::GOOD, 3, 0),
        ],
    );
    // The abort is known before the first trick: no trick set, but the gain and the thought.
    assert_eq!(unit.runner.top(), Some("bExitShowBad"));
    assert_eq!(sets(&unit), ["bExitShowBad"]);
    assert_eq!(values(&unit).0[HAPPINESS], 57);
    assert_eq!(thoughts(&mut w), ["thought 1 10612", "thought 1 10612"]);
}

#[test]
fn a_guest_already_leaving_badly_and_a_show_that_ended_start_no_trick_set() {
    let (_, unit) = read(Some("bExitShowBad"), vec![heard(event::GREAT, 6, 0)]);
    assert_eq!(unit.runner.top(), Some("bExitShowBad"));
    assert_eq!(values(&unit).0[HAPPINESS], 56);
    let (mut w, unit) = read(
        None,
        vec![heard(event::GREAT, 6, 0), heard(event::ENDED, 90, 0)],
    );
    assert_eq!(unit.runner.top(), Some("bExitShowGreat"));
    assert_eq!(sets(&unit), ["bExitShowGreat"]);
    assert_eq!(thoughts(&mut w), ["thought 1 10613", "thought 1 10290"]);
    // An end that follows an abort still thinks and tires, but starts no exit set of its own.
    let (mut w, unit) = read(
        None,
        vec![heard(event::ABORTED, 0, 0), heard(event::ENDED, 90, 20)],
    );
    assert_eq!(unit.runner.top(), Some("bExitShowBad"));
    assert_eq!(
        (thoughts(&mut w), values(&unit).0[ENERGY]),
        (vec!["thought 1 10290".into()], 60)
    );
}

#[test]
fn the_status_update_at_once_also_takes_what_an_earlier_change_left() {
    let mut w = crate::unit::testing::world_with((4, 4), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((1, 1)), 0));
    });
    let mut cat = Catalogue::default();
    cat.push(man());
    let at = w.find(1).unwrap();
    let mut unit = w.units.get(1).unwrap().clone();
    unit.part = Part::Guest(Guest {
        values: [50, 40, 0, 0, 0],
        pending: [0, 15, 0, 0, 0],
        ..Guest::default()
    });
    unit.events = vec![heard(event::GOOD, 5, 0)];
    let (queue, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &queue,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::guest::reaction::react(&mut unit, &mut w, &mut ctx, at, &NoData);
    assert_eq!(values(&unit), ([55, 55, 0, 0, 0], [0; 5]));
}

#[test]
fn a_unit_that_is_no_guest_reads_nothing() {
    let (mut w, mut unit) = read(None, vec![]);
    unit.part = Part::Animal(Default::default());
    unit.events = vec![heard(event::ENDED, 90, 0)];
    let (queue, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let cat = Catalogue::default();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &queue,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::guest::reaction::react(&mut unit, &mut w, &mut ctx, 0, &NoData);
    assert!(thoughts(&mut w).is_empty() && unit.runner.top().is_none());
}

#[test]
fn the_unit_turn_hands_the_guest_the_events_posted_to_it() {
    let mut w = crate::unit::testing::world_with((4, 4), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((1, 1)), 0));
    });
    let mut cat = Catalogue::default();
    cat.push(man());
    w.units.get_mut(1).unwrap().part = Part::Guest(Guest::default());
    let mut queue = Events::default();
    queue.post(heard(event::ENDED, 80, 0));
    queue.age();
    let (mut removals, mut rng) = (Vec::new(), Rng::default());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &queue,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let mut hooks = crate::unit::Standard { data: &NoData };
    crate::unit::update(&mut hooks, &mut w, &mut ctx, 0);
    assert_eq!(thoughts(&mut w), ["thought 1 10290"]);
}

#[test]
fn an_abort_leaves_the_pending_changes_for_the_status_tick() {
    let (mut w, mut unit) = read(None, vec![]);
    let at = w.find(1).unwrap();
    unit.part = Part::Guest(Guest {
        pending: [0, 15, 0, 0, 0],
        ..Guest::default()
    });
    unit.events = vec![heard(event::ABORTED, 0, 0)];
    let (queue, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let mut cat = Catalogue::default();
    cat.push(man());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &queue,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::guest::reaction::react(&mut unit, &mut w, &mut ctx, at, &NoData);
    assert_eq!(values(&unit).1, [0, 15, 0, 0, 0]);
    assert_eq!(unit.runner.top(), Some("bExitShowBad"));
}
