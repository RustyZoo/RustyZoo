//! The world manager's and the grandstands' reading of the show events.

use super::cycle::staged;
use super::*;
use crate::building::Seat;
use crate::sim::{
    BUILDING_CHANNEL, Event, Events, Rng, SHOW_CHANNEL, TANK_CHANNEL, UNIT_CHANNEL, WORLD_CHANNEL,
};

/// An event of show 1001 on the world's channel, as the next step reads it.
fn shown(kind: u16, sender: u32, data: u32, extra: u16) -> Event {
    Event {
        sender,
        target: 0,
        data,
        extra,
        kind,
        delay: 1,
        sender_channel: SHOW_CHANNEL,
        channel: WORLD_CHANNEL,
    }
}

/// An event for building `id` on the building channel.
fn for_stand(id: u32, kind: u16, data: u32, extra: u16) -> Event {
    Event {
        sender: 0,
        target: id,
        data,
        extra,
        kind,
        delay: 0,
        sender_channel: WORLD_CHANNEL,
        channel: BUILDING_CHANNEL,
    }
}

/// The queue holding `posted`, as the step after reads them.
fn queue(posted: Vec<Event>) -> Events {
    let mut events = Events::default();
    posted.into_iter().for_each(|e| events.post(e));
    events.age();
    events
}

/// What was posted since the queue was read: (type, target, channel, data, extra, delay, sender).
fn waiting(events: &Events) -> Vec<(u16, u32, u8, u32, u16, u16, u32)> {
    (events.buffers()[1].iter())
        .map(|e| {
            (
                e.kind, e.target, e.channel, e.data, e.extra, e.delay, e.sender,
            )
        })
        .collect()
}

/// The world with stand 7 in show 1001 and stand 8 in another show.
fn two_shows() -> (World, Catalogue) {
    let (mut w, cat) = staged(vec![super::cycle::item(0, 3, 5)]);
    w.units.buildings.get_mut(&8).unwrap().show_id = 1002;
    (w, cat)
}

fn relay(w: &mut World, cat: &Catalogue, events: &mut Events) -> Rng {
    let mut rng = Rng::default();
    rng.seed(1);
    listen_world(w, cat, events, &mut rng);
    rng
}

fn untouched() -> Rng {
    let mut rng = Rng::default();
    rng.seed(1);
    rng
}

fn drawn(n: usize) -> Rng {
    let mut rng = untouched();
    (0..n).for_each(|_| {
        rng.rand();
    });
    rng
}

#[test]
fn the_world_tells_the_tank_and_only_the_grandstands_of_the_show() {
    let (mut w, cat) = two_shows();
    let mut events = queue(vec![shown(0x2711, 1001, 0, 1)]);
    relay(&mut w, &cat, &mut events);
    assert_eq!(
        waiting(&events),
        [
            (0x2711, 1001, TANK_CHANNEL, 0, 1, 0, 0),
            (0x2711, 7, BUILDING_CHANNEL, 0, 1, 0, 0)
        ]
    );
}

#[test]
fn an_announce_of_a_show_without_a_specialist_or_an_info_is_not_relayed() {
    let (mut w, cat) = two_shows();
    super::cycle::info_mut(&mut w).records[1].units.clear();
    let mut events = queue(vec![shown(0x2711, 1001, 0, 1), shown(0x2711, 4242, 0, 1)]);
    relay(&mut w, &cat, &mut events);
    assert!(waiting(&events).is_empty());
}

#[test]
fn a_start_reaches_the_grandstands_as_the_start_note_and_the_tank_as_the_start() {
    let (mut w, cat) = two_shows();
    let mut events = queue(vec![shown(0x2713, 1001, 0, 0)]);
    relay(&mut w, &cat, &mut events);
    let sent = waiting(&events);
    assert_eq!(sent[0].0, 0x2713);
    assert_eq!(sent[1].0, 0x2712);
}

#[test]
fn the_relayed_data_word_is_passed_whole() {
    let (mut w, cat) = two_shows();
    let mut events = queue(vec![shown(0x2715, 1001, 0x1_0064, 95)]);
    relay(&mut w, &cat, &mut events);
    let sent = waiting(&events);
    assert_eq!((sent[0].3, sent[0].4), (0x1_0064, 95));
    assert_eq!((sent[1].3, sent[1].4), (0x1_0064, 95));
    // The cheer carries the percent in the half word it fits.
    assert_eq!(sent[2], (0x2735, 0, WORLD_CHANNEL, 1001, 100, 135, 0));
}

#[test]
fn events_that_are_not_shows_events_or_not_due_are_left_alone() {
    let (mut w, cat) = two_shows();
    let late = Event {
        delay: 2,
        ..shown(0x2715, 1001, 80, 100)
    };
    let mut events = queue(vec![
        shown(0x271d, 1001, 0, 0),
        shown(0x2719, 1001, 0, 0),
        late,
    ]);
    relay(&mut w, &cat, &mut events);
    assert!(waiting(&events).is_empty());
}

#[test]
fn an_end_is_cheered_later_and_an_abort_or_an_aborted_end_is_booed_later() {
    let (mut w, cat) = two_shows();
    let cheer = (0x2735, 0, WORLD_CHANNEL, 1001, 80, 135, 0);
    let boo = (0x2736, 0, WORLD_CHANNEL, 1001, 0, 135, 0);
    // An end alone is cheered.
    let mut events = queue(vec![shown(0x2715, 1001, 80, 100)]);
    relay(&mut w, &cat, &mut events);
    assert_eq!(waiting(&events).last(), Some(&cheer));
    // An abort in the same step makes the end a boo too, and is itself booed.
    let mut events = queue(vec![shown(0x2717, 1001, 0, 0), shown(0x2715, 1001, 0, 94)]);
    relay(&mut w, &cat, &mut events);
    let sent = waiting(&events);
    assert_eq!(sent.iter().filter(|e| e.0 == 0x2736).count(), 2);
    assert!(sent.iter().all(|e| e.0 != 0x2735));
    assert_eq!(sent.last(), Some(&boo));
}

#[test]
fn only_the_first_abort_of_a_step_makes_its_show_a_boo() {
    let (mut w, cat) = two_shows();
    // The second show aborted too, but only the first abort is looked at: its end is cheered.
    let mut events = queue(vec![
        shown(0x2717, 1001, 0, 0),
        shown(0x2717, 1002, 0, 0),
        shown(0x2715, 1002, 80, 100),
    ]);
    relay(&mut w, &cat, &mut events);
    let sent = waiting(&events);
    assert!(sent.contains(&(0x2735, 0, WORLD_CHANNEL, 1002, 80, 135, 0)));
}

#[test]
fn a_reaction_draws_one_number_for_the_sound_when_a_grandstand_faces_the_show() {
    let (mut w, cat) = two_shows();
    // Bad, good and great draw; neutral, the announce and the start do not.
    for (kind, draws) in [
        (0x272a, 1),
        (0x272c, 1),
        (0x272d, 1),
        (0x272b, 0),
        (0x2713, 0),
    ] {
        let mut events = queue(vec![shown(kind, 1001, 5, 0)]);
        let rng = relay(&mut w, &cat, &mut events);
        assert_eq!(rng, drawn(draws), "{kind:x}");
    }
    // The cheer and the boo of an end, 135 steps on, draw as well.
    for kind in [0x2735, 0x2736] {
        let mut events = queue(vec![shown(kind, 0, 1001, 80)]);
        assert_eq!(relay(&mut w, &cat, &mut events), drawn(1), "{kind:x}");
        assert!(waiting(&events).is_empty());
    }
}

#[test]
fn no_number_is_drawn_for_a_show_no_grandstand_faces() {
    let (mut w, cat) = two_shows();
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().rotation = 6;
    let mut events = queue(vec![shown(0x272c, 1001, 5, 1), shown(0x2735, 0, 1001, 80)]);
    assert_eq!(relay(&mut w, &cat, &mut events), untouched());
    // The stored show id still sends the reaction to the grandstand.
    assert_eq!(waiting(&events).len(), 2);
}

/// Stand 7 with guests 1 and 2 seated, 2 on the lower place, so that the place order is not the
/// order they entered in.
fn seated() -> (World, Catalogue) {
    let (mut w, cat) = two_shows();
    let live = w.units.buildings.get_mut(&7).unwrap();
    live.users = vec![(2, 0, 0), (1, 0, 0)];
    live.seats = vec![
        Seat {
            unit: 1,
            slot: 0,
            place: 5,
        },
        Seat {
            unit: 2,
            slot: 0,
            place: 3,
        },
    ];
    live.fill = 0;
    (w, cat)
}

fn listen(w: &mut World, cat: &Catalogue, posted: Vec<Event>) -> (Events, Rng) {
    let mut events = queue(posted);
    let mut rng = untouched();
    let at = w.find(7).unwrap();
    listen_grandstand(w, cat, &mut events, &mut rng, at);
    (events, rng)
}

#[test]
fn an_announce_raises_the_flag_of_the_grandstand_and_draws_nothing() {
    let (mut w, cat) = seated();
    let (events, rng) = listen(&mut w, &cat, vec![for_stand(7, 0x2711, 0, 1)]);
    assert_eq!(w.units.buildings[&7].fill, 1);
    assert!(waiting(&events).is_empty());
    assert_eq!(rng, untouched());
}

#[test]
fn an_end_goes_to_each_seated_guest_in_place_order_after_its_own_delay() {
    let (mut w, cat) = seated();
    w.units.buildings.get_mut(&7).unwrap().fill = 1;
    let (events, rng) = listen(&mut w, &cat, vec![for_stand(7, 0x2715, 80, 100)]);
    // Guest 2 sits on place 3, guest 1 on place 5: each draws 0 to 5, then 90 more and 0 to 34.
    let mut want = untouched();
    let mut sent = Vec::new();
    for guest in [2, 1] {
        let delay = want.rand() % 6 + 90 + want.rand() % 35;
        sent.push((0x2715, guest, UNIT_CHANNEL, 80, 100, delay as u16, 7));
    }
    assert_eq!(waiting(&events), sent);
    assert_eq!(rng, want);
    assert!(sent.iter().all(|e| (90..=129).contains(&e.5)));
    // The end lowers the flag.
    assert_eq!(w.units.buildings[&7].fill, 0);
}

#[test]
fn a_reaction_goes_to_each_seated_guest_after_zero_to_five_steps_and_keeps_the_flag() {
    let (mut w, cat) = seated();
    w.units.buildings.get_mut(&7).unwrap().fill = 1;
    let (events, rng) = listen(&mut w, &cat, vec![for_stand(7, 0x272c, 5, 1)]);
    let mut want = untouched();
    let sent: Vec<_> = [2, 1]
        .map(|g| (0x272c, g, UNIT_CHANNEL, 5, 1, (want.rand() % 6) as u16, 7))
        .into();
    assert_eq!(waiting(&events), sent);
    assert_eq!(rng, want);
    assert_eq!(w.units.buildings[&7].fill, 1);
}

#[test]
fn a_seat_whose_guest_left_gets_nothing() {
    let (mut w, cat) = seated();
    w.units.buildings.get_mut(&7).unwrap().users = vec![(1, 0, 0)];
    let (events, rng) = listen(&mut w, &cat, vec![for_stand(7, 0x2717, 0, 0)]);
    assert_eq!(waiting(&events).len(), 1);
    assert_eq!(rng, drawn(2));
}

#[test]
fn a_grandstand_that_does_not_face_its_show_turns_every_event_into_an_abort() {
    for own in [0, 1002] {
        let (mut w, cat) = seated();
        w.units.buildings.get_mut(&7).unwrap().show_id = own;
        w.units.buildings.get_mut(&7).unwrap().fill = 1;
        let (events, _) = listen(&mut w, &cat, vec![for_stand(7, 0x272c, 5, 1)]);
        let kinds: Vec<u16> = waiting(&events).iter().map(|e| e.0).collect();
        assert_eq!(kinds, [0x2717, 0x2717], "{own}");
        assert!(waiting(&events).iter().all(|e| e.5 >= 90), "{own}");
        assert_eq!(w.units.buildings[&7].fill, 0, "{own}");
    }
    // The stand that faces no tank and belongs to no show does the same.
    let (mut w, cat) = seated();
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().rotation = 6;
    w.units.buildings.get_mut(&7).unwrap().show_id = 0;
    let (events, _) = listen(&mut w, &cat, vec![for_stand(7, 0x272c, 5, 1)]);
    assert!(waiting(&events).iter().all(|e| e.0 == 0x2717));
    // The stand that faces away from the tank entirely.
    let (mut w, cat) = seated();
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().rotation = 6;
    let (events, _) = listen(&mut w, &cat, vec![for_stand(7, 0x2711, 0, 1)]);
    assert_eq!(w.units.buildings[&7].fill, 0);
    assert!(waiting(&events).iter().all(|e| e.0 == 0x2717));
}

#[test]
fn a_grandstand_ignores_events_that_are_for_another_building_or_not_due() {
    let (mut w, cat) = seated();
    let late = Event {
        delay: 2,
        ..for_stand(7, 0x2715, 0, 0)
    };
    let (events, rng) = listen(&mut w, &cat, vec![for_stand(8, 0x2715, 0, 0), late]);
    assert!(waiting(&events).is_empty());
    assert_eq!(rng, untouched());
}

#[test]
fn the_start_note_does_nothing_at_the_grandstand_and_the_flag_stays_up() {
    let (mut w, cat) = seated();
    w.units.buildings.get_mut(&7).unwrap().fill = 1;
    let (events, rng) = listen(&mut w, &cat, vec![for_stand(7, 0x2712, 0, 0)]);
    assert!(waiting(&events).is_empty());
    assert_eq!((w.units.buildings[&7].fill, rng), (1, untouched()));
}

#[test]
fn a_show_event_reaches_the_guests_two_steps_after_the_step_it_was_posted_in() {
    let (mut w, cat) = seated();
    w.units.buildings.get_mut(&7).unwrap().fill = 1;
    let mut sim = crate::sim::Sim::new(b"");
    sim.set_seed(1);
    sim.events.post(shown(0x2715, 1001, 80, 100));
    let guest_events = |sim: &crate::sim::Sim| {
        let both = sim.events.buffers();
        both.iter()
            .flat_map(|b| b.iter())
            .filter(|e| e.channel == UNIT_CHANNEL && e.kind == 0x2715)
            .count()
    };
    let mut seen = [(0, 0); 3];
    for at in &mut seen {
        sim.step_in(&mut w, &cat, 100, |_, _, _| {});
        *at = (guest_events(&sim), w.units.buildings[&7].fill);
    }
    // Step 1: the world relays to the stand. Step 3: the stand tells the guests and lowers its flag.
    assert_eq!(seen.map(|s| s.0), [0, 0, 2]);
    assert_eq!(seen.map(|s| s.1), [1, 1, 0]);
}
