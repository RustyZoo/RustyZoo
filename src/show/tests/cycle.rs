//! The show cycle: announce, start, items, stop and abort.

use super::*;
use crate::behaviour::NoData;
use crate::exhibit::{Exhibit, Tank};
use crate::show::cycle::abort;
use crate::sim::{Event, KEEPER_CHANNEL, Rng, SHOW_CHANNEL, UNIT_CHANNEL, WORLD_CHANNEL};
use crate::zoo::{Item, ItemState, Script, TypeRecord};

pub(super) const SEAL: u32 = 5400;

/// An item for seals of trick type `marker` (0 behaviour, 3 food).
pub(super) fn item(marker: u32, satisfaction: i32, current: i32) -> Item {
    Item {
        available: 1,
        visible: 1,
        id: 11200,
        kind: SEAL,
        marker,
        names: Default::default(),
        building: 0,
        complexity: 1,
        returns: 0,
        satisfaction,
        delta: 1,
        current,
        depth: 1,
        help: [0; 2],
        icons: Default::default(),
    }
}

pub(super) fn record(key: u32, script: u16, units: &[u32]) -> TypeRecord {
    TypeRecord {
        key,
        script,
        pending: u16::MAX,
        admission: 0,
        receipts: [0; 3],
        attendance: [0; 3],
        date: 0,
        units: units.to_vec(),
    }
}

/// The crowded world with two seals (21, 22) and a specialist (30) in the show of the tank, a
/// script of `items` for seals, a schedule of the seals and the next show at AI tick 90.
pub(super) fn staged(items: Vec<Item>) -> (World, Catalogue) {
    let (mut w, mut cat) = crowded();
    cat.push(EntityType {
        name_id: SEAL as i32,
        ..kind("animals", "seal", "")
    });
    for (id, class, name) in [
        (21, "animals", "seal"),
        (22, "animals", "seal"),
        (30, "keeper", "zookeeper"),
    ] {
        w.zoo
            .entities
            .push(entity(class, name, id, centre((6, 1)), 0));
    }
    let all = w.zoo.entities.clone();
    w.units.sync(&all);
    w.zoo.shows.scripts = vec![Script {
        id: 2000,
        marker: u32::MAX,
        kind: SEAL,
        items,
    }];
    let info = w.zoo.shows.infos[0].as_mut().unwrap();
    info.schedule = vec![SEAL];
    info.next = 90;
    info.records = vec![record(SEAL, 2000, &[21, 22]), record(KEEPER, 0, &[30])];
    (w, cat)
}

/// The show tank.
pub(super) fn tank_mut(w: &mut World) -> &mut Exhibit {
    let id = tank_of(w, 0).unwrap();
    w.exhibits_mut().get_mut(id).unwrap()
}

pub(super) fn info_of_show(w: &World) -> &ShowInfo {
    w.zoo.shows.infos[0].as_ref().unwrap()
}

pub(super) fn info_mut(w: &mut World) -> &mut ShowInfo {
    w.zoo.shows.infos[0].as_mut().unwrap()
}

/// One step of the show manager at AI tick `now`, then the end of the AI manager's update, which
/// makes the events posted by it the ones read.
pub(super) fn tick(w: &mut World, cat: &Catalogue, events: &mut Events, now: u32) {
    update(w, cat, (events, &mut Rng::default(), &NoData), now);
    events.age();
}

/// What the buffer being read holds: (type, sender, target, channel, data, extra).
pub(super) fn heard(events: &Events) -> Vec<(u16, u32, u32, u8, u32, u16)> {
    (events.buffers()[0].iter())
        .map(|e| (e.kind, e.sender, e.target, e.channel, e.data, e.extra))
        .collect()
}

fn kinds(events: &Events) -> Vec<u16> {
    heard(events).iter().map(|e| e.0).collect()
}

/// Starts the staged show at its time and returns the events of the start step.
fn started(items: Vec<Item>) -> (World, Catalogue, Events) {
    let (mut w, cat) = staged(items);
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 90);
    assert!(info_of_show(&w).show.running);
    (w, cat, events)
}

#[test]
fn a_show_announces_at_every_fifth_tick_of_the_last_minute() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    // 65 ticks before the start: too early. 60: the first announce. 59: not a multiple of 5.
    for (now, announces) in [(25, false), (30, true), (31, false), (35, true), (85, true)] {
        let mut events = Events::default();
        tick(&mut w, &cat, &mut events, now);
        let sent = heard(&events);
        assert_eq!(sent.iter().any(|e| e.0 == 0x2711), announces, "tick {now}");
        if announces {
            // From the show info, to everyone on the world's channel, extra 1, for the next step.
            assert_eq!(
                sent[0],
                (0x2711, 1001, 0, WORLD_CHANNEL, 0, 1),
                "tick {now}"
            );
            assert_eq!(events.buffers()[0][0].delay, 1);
        }
    }
    let show = &info_of_show(&w).show;
    assert!(show.ready && !show.running);
    assert_eq!((show.species, show.keeper), (SEAL, KEEPER));
}

#[test]
fn the_announce_window_is_the_frequency_when_it_is_below_a_minute() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let info = info_mut(&mut w);
    (info.frequency, info.next) = (20, 20);
    for (now, announces) in [(0, true), (5, true), (10, true), (3, false)] {
        let mut events = Events::default();
        tick(&mut w, &cat, &mut events, now);
        assert_eq!(kinds(&events).contains(&0x2711), announces, "tick {now}");
    }
    // 40 ticks before a due time, a show of 20 ticks is not announced yet.
    info_mut(&mut w).next = 60;
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 20);
    assert!(!kinds(&events).contains(&0x2711));
    tick(&mut w, &cat, &mut events, 40);
    assert!(kinds(&events).contains(&0x2711));
}

#[test]
fn the_about_to_start_message_is_asked_once_and_names_the_tank() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    tank_mut(&mut w).name = "Seals".into();
    let mut events = Events::default();
    for now in [30, 35, 40] {
        tick(&mut w, &cat, &mut events, now);
    }
    assert_eq!(
        w.units.take_messages(),
        [(11490, 0, vec!["Seals".to_string()])]
    );
}

#[test]
fn a_show_with_no_animals_or_script_is_not_announced_and_moves_on() {
    // No animal at the announce: the next species and time are scheduled at once.
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    info_mut(&mut w).records[0].units.clear();
    info_mut(&mut w).schedule = vec![SEAL, 5401];
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 30);
    let info = info_of_show(&w);
    assert!(!info.show.ready && heard(&events).is_empty());
    assert_eq!((info.index, info.next), (1, 120));
    // A script with no item is never announced.
    let (mut w, cat) = staged(Vec::new());
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 30);
    assert!(!info_of_show(&w).show.ready && heard(&events).is_empty());
}

#[test]
fn a_show_never_announces_or_starts_when_its_frequency_is_never() {
    for frequency in [-1] {
        let (mut w, cat) = staged(vec![item(0, 3, 5)]);
        let info = info_mut(&mut w);
        (info.frequency, info.next) = (frequency, -1);
        let mut events = Events::default();
        for now in [0, 30, 90, u32::MAX] {
            tick(&mut w, &cat, &mut events, now);
        }
        let show = &info_of_show(&w).show;
        assert!(!show.ready && !show.running, "{frequency}");
    }
}

#[test]
fn a_frequency_of_zero_or_a_huge_one_never_stops_the_cycle() {
    for frequency in [0, i32::MAX] {
        let (mut w, cat) = staged(vec![item(0, 3, 5)]);
        let info = info_mut(&mut w);
        (info.frequency, info.next) = (frequency, 0);
        let mut events = Events::default();
        for now in [0, 1, 1000, u32::MAX] {
            tick(&mut w, &cat, &mut events, now);
        }
    }
}

#[test]
fn a_show_is_updated_in_the_order_of_its_show_id() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut second = info_of_show(&w).clone();
    second.id = 1000;
    w.zoo.shows.infos.push(Some(second));
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 30);
    let senders: Vec<u32> = heard(&events).iter().map(|e| e.1).collect();
    assert_eq!(senders, [1000, 1001]);
}

#[test]
fn a_due_show_starts_with_an_item_state_for_each_animal_and_tells_everyone() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5), item(0, 3, 5)]);
    let info = info_of_show(&w);
    let show = &info.show;
    assert_eq!(
        (show.ready, show.running, show.stopped, show.start),
        (false, true, false, 90)
    );
    assert_eq!(show.script, 2000);
    let states: Vec<(u32, i16)> = show.items.iter().map(|s| (s.animal, s.item)).collect();
    assert_eq!(states, [(21, -1), (22, -1)]);
    // The next show is one frequency on; the animals know their show.
    assert_eq!(info.next, 180);
    assert_eq!(
        (w.units.get(21).unwrap().show, w.units.get(22).unwrap().show),
        (1001, 1001)
    );
    assert_eq!(w.units.get(30).unwrap().show, 0);
    assert!(kinds(&events).starts_with(&[0x2713]));
    assert_eq!(
        w.units.take_messages(),
        [(11491, 0, vec!["Exhibit 1".to_string()])]
    );
    // The step after, each animal is called.
    events.clear();
    tick(&mut w, &cat, &mut events, 91);
    let calls: Vec<_> = heard(&events)
        .into_iter()
        .filter(|e| e.0 == 0x271a)
        .collect();
    assert_eq!(
        calls,
        [
            (0x271a, 1001, 21, UNIT_CHANNEL, 0, 0),
            (0x271a, 1001, 22, UNIT_CHANNEL, 0, 0)
        ]
    );
}

#[test]
fn a_started_show_goes_on_until_it_is_aborted() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    for now in 91..140 {
        events.clear();
        tick(&mut w, &cat, &mut events, now);
        assert!(info_of_show(&w).show.running, "tick {now}");
        // Each animal was cued once and nothing else is asked of it.
        let cues = kinds(&events).iter().filter(|&&k| k == 0x271a).count();
        assert_eq!(cues, if now == 91 { 2 } else { 0 }, "tick {now}");
    }
}

#[test]
fn a_due_show_without_a_specialist_or_a_script_waits_for_the_next_time() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    info_mut(&mut w).records[1].units.clear();
    info_mut(&mut w).schedule = vec![SEAL, 5401];
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 90);
    let info = info_of_show(&w);
    // No specialist: the same species, one frequency later.
    assert_eq!((info.show.running, info.index, info.next), (false, 0, 180));
    // An empty script: the next species.
    let (mut w, cat) = staged(Vec::new());
    info_mut(&mut w).schedule = vec![SEAL, 5401];
    tick(&mut w, &cat, &mut events, 90);
    let info = info_of_show(&w);
    assert_eq!((info.show.running, info.index, info.next), (false, 1, 180));
}

#[test]
fn a_show_whose_animals_are_not_of_the_scheduled_species_does_not_start() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    // Unit 1 is a guest, not a seal: the validation refuses the show and it ends at once.
    info_mut(&mut w).records[0].units = vec![21, 1];
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 90);
    let info = info_of_show(&w);
    assert!(!info.show.running);
    assert_eq!(info.next, 180);
    // The stop that follows a failed start tells the end.
    assert!(kinds(&events).contains(&0x2715));
}

#[test]
fn a_unit_that_is_not_there_refuses_the_start() {
    // Id 77 is no entity; entity 22 is removed from the world but still listed.
    for gone in [77, 22] {
        let (mut w, cat) = staged(vec![item(0, 3, 5)]);
        info_mut(&mut w).records[0].units = vec![21, gone];
        let at = w.find(22).unwrap();
        w.zoo.entities.remove(at);
        let all = w.zoo.entities.clone();
        w.units.sync(&all);
        tick(&mut w, &cat, &mut Events::default(), 90);
        assert!(!info_of_show(&w).show.running, "{gone}");
    }
}

#[test]
fn an_animal_already_in_a_running_show_gets_no_item_state_in_another() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    // Seal 21 is in show 1000, which is running and holds a state for it.
    let mut other = info_of_show(&w).clone();
    other.id = 1000;
    other.schedule = vec![SEAL];
    other.records = vec![record(SEAL, 2000, &[21]), record(KEEPER, 0, &[30])];
    other.show.script = 2000;
    (other.show.species, other.show.keeper) = (SEAL, KEEPER);
    other.show.running = true;
    other.show.stopped = false;
    other.show.items = vec![ItemState {
        script: 2000,
        animal: 21,
        item: -1,
        finished: false,
        aborted: false,
        cue_sent: false,
        ready: false,
        started: false,
        done: false,
    }];
    w.zoo.shows.infos.push(Some(other));
    w.units.get_mut(21).unwrap().show = 1000;
    let mut events = Events::default();
    update(
        &mut w,
        &cat,
        (&mut events, &mut Rng::default(), &NoData),
        90,
    );
    let states: Vec<u32> = info_of_show(&w)
        .show
        .items
        .iter()
        .map(|s| s.animal)
        .collect();
    assert_eq!(states, [22]);
    assert_eq!(w.units.get(21).unwrap().show, 1000);
}

#[test]
fn an_animal_with_a_finished_item_scores_it_and_the_next_item_begins() {
    // Thresholds 0, 3, 6: a trick of 0 is bad, 2 neutral, 4 good, 7 great; 7 - 6 = 1.
    for (current, kind, extra) in [
        (0, 0x272a, 0),
        (-2, 0x272a, 2),
        (2, 0x272b, 0),
        (4, 0x272c, 1),
        (7, 0x272d, 1),
    ] {
        let (mut w, cat, mut events) = started(vec![item(0, 4, current), item(0, 4, 0)]);
        tick(&mut w, &cat, &mut events, 91);
        let state = &mut info_mut(&mut w).show.items[0];
        (state.item, state.finished, state.cue_sent, state.ready) = (0, true, true, true);
        events.clear();
        tick(&mut w, &cat, &mut events, 92);
        let sent = heard(&events);
        let reaction = sent.iter().find(|e| e.0 == kind).expect("a reaction");
        assert_eq!(
            (reaction.1, reaction.2, reaction.3, reaction.4, reaction.5),
            (1001, 0, WORLD_CHANNEL, current as u32, extra),
            "trick {current}"
        );
        let show = &info_of_show(&w).show;
        assert_eq!(
            (show.done, show.possible, show.achieved),
            (1, 4, current as u32),
            "trick {current}"
        );
        // The specialist is told of the finished trick, then of the next item; so is the animal.
        assert!(sent.contains(&(0x271f, 1001, 0, KEEPER_CHANNEL, 0, 0)));
        assert!(sent.contains(&(0x271d, 1001, 0, KEEPER_CHANNEL, 0, 1)));
        assert!(sent.contains(&(0x271d, 1001, 21, UNIT_CHANNEL, 0, 0)));
        let state = &show.items[0];
        assert_eq!(state.item, 1);
        assert!(!state.finished && !state.ready);
    }
}

#[test]
fn an_item_that_scores_nothing_or_was_aborted_is_not_scored() {
    // A food item (type 3) is not scored; an aborted one is passed over without a reaction.
    for (marker, aborted) in [(3, false), (0, true)] {
        let (mut w, cat, mut events) = started(vec![item(marker, 4, 5), item(0, 4, 5)]);
        tick(&mut w, &cat, &mut events, 91);
        let state = &mut info_mut(&mut w).show.items[0];
        (state.item, state.finished, state.aborted) = (0, true, aborted);
        events.clear();
        tick(&mut w, &cat, &mut events, 92);
        assert_eq!(info_of_show(&w).show.done, 0);
        assert!(
            kinds(&events)
                .iter()
                .all(|k| !(0x272a..=0x272d).contains(k))
        );
        assert_eq!(info_of_show(&w).show.items[0].item, 1);
    }
}

#[test]
fn when_every_animal_has_done_its_last_item_the_show_ends_with_its_figures() {
    // Six tricks of 5 are good, and 6 is the ideal length: no penalty, percent 100, bonus 100.
    let items = (0..6).map(|_| item(0, 5, 5)).collect();
    let (mut w, cat, mut events) = started(items);
    tick(&mut w, &cat, &mut events, 91);
    {
        let show = &mut info_mut(&mut w).show;
        (show.done, show.possible, show.achieved) = (6, 30, 30);
        for s in &mut show.items {
            (s.item, s.done) = (5, true);
        }
    }
    events.clear();
    tick(&mut w, &cat, &mut events, 92);
    let sent = heard(&events);
    assert_eq!(sent[0], (0x2715, 1001, 0, WORLD_CHANNEL, 100, 100));
    let info = info_of_show(&w);
    assert!(!info.show.running && info.show.stopped && info.show.items.is_empty());
    // The next show is a frequency from the stop.
    assert_eq!(info.next, 182);
    assert_eq!(
        w.units.take_messages().last(),
        Some(&(11492, 0, vec!["Exhibit 1".to_string()]))
    );
}

#[test]
fn the_end_figures_are_the_percent_of_the_satisfaction_less_the_length_penalty() {
    // (tricks done, possible, achieved, item count, ideal, percent, rest bonus)
    let table = [
        (6, 0, 0, 6, [6, 6], 0, 100),
        (6, 30, 0, 6, [6, 6], 0, 100),
        (6, 30, 15, 6, [6, 6], 50, 100),
        (6, 30, 30, 6, [6, 6], 100, 100),
        (6, 30, 40, 6, [6, 6], 100, 100),
        // Fewer tricks than the shortest ideal show, more than the longest.
        (4, 20, 20, 6, [6, 6], 98, 98),
        (0, 0, 0, 6, [5, 7], 0, 95),
        (9, 45, 45, 20, [5, 7], 98, 98),
        // More tricks than the script has items: no penalty.
        (9, 45, 45, 6, [5, 7], 100, 100),
        // A penalty of 100 or more.
        (0, 0, 0, 6, [150, 150], 0, 100),
        (3, 30, 30, 6, [-3, -1], 96, 96),
        (6, 8, 1, 6, [6, 6], 12, 100),
    ];
    for (done, possible, achieved, count, ideal, percent, bonus) in table {
        let items = (0..count).map(|_| item(0, 5, 5)).collect();
        let (mut w, cat, mut events) = started(items);
        w.units.show.ideal = ideal;
        let show = &mut info_mut(&mut w).show;
        (show.done, show.possible, show.achieved) = (done, possible, achieved);
        events.clear();
        abort(&mut w, &cat, &mut events, 1001);
        events.age();
        let ended = heard(&events).into_iter().find(|e| e.0 == 0x2715).unwrap();
        assert_eq!(
            (ended.4, ended.5),
            (percent, bonus),
            "{done} {possible} {achieved} {count}"
        );
    }
}

#[test]
fn the_end_percent_is_truncated_toward_zero_in_double_precision() {
    for (possible, achieved, percent) in [(8, 3, 37), (29, 19, 65), (100, 29, 28)] {
        let (mut w, cat, mut events) = started(vec![item(0, 5, 5)]);
        w.units.show.ideal = [6, 6];
        let show = &mut info_mut(&mut w).show;
        (show.done, show.possible, show.achieved) = (6, possible, achieved);
        events.clear();
        abort(&mut w, &cat, &mut events, 1001);
        events.age();
        let ended = heard(&events).into_iter().find(|e| e.0 == 0x2715).unwrap();
        assert_eq!(ended.4, percent, "{achieved} of {possible}");
    }
}

#[test]
fn a_new_item_state_holds_the_current_script_id() {
    let (w, _, _) = started(vec![item(0, 3, 5)]);
    let items = &info_of_show(&w).show.items;
    assert!(!items.is_empty());
    assert!(items.iter().all(|s| s.script == 2000));
}

#[test]
fn an_abort_tells_the_abort_then_the_end_and_resets_the_show() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    events.clear();
    abort(&mut w, &cat, &mut events, 1001);
    events.age();
    // The abort first, then the end with percent 0 and the rest bonus of no trick (ideal 6).
    let sent = heard(&events);
    assert_eq!(sent[0], (0x2717, 1001, 0, WORLD_CHANNEL, 0, 0));
    assert_eq!(sent[1], (0x2715, 1001, 0, WORLD_CHANNEL, 0, 94));
    let info = info_of_show(&w);
    let show = &info.show;
    assert_eq!(
        (show.ready, show.running, show.stopped, show.species),
        (false, false, true, 0)
    );
    assert_eq!(
        (show.start, show.done, show.possible, show.achieved),
        (0, 0, 0, 0)
    );
    assert!(show.items.is_empty());
    assert_eq!(info.next, 90);
    assert_eq!(
        w.units.take_messages().last(),
        Some(&(11492, 0, vec!["Exhibit 1".to_string()]))
    );
    // The animals keep the show they were started in.
    assert_eq!(w.units.get(21).unwrap().show, 1001);
    // A show that is not there is not aborted.
    abort(&mut w, &cat, &mut events, 4242);
}

#[test]
fn an_abort_before_the_start_does_not_tell_the_end_message() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut events = Events::default();
    abort(&mut w, &cat, &mut events, 1001);
    assert!(w.units.take_messages().is_empty());
    assert_eq!(events.buffers()[1].len(), 2);
}

#[test]
fn a_running_show_is_aborted_when_the_tank_dries_the_animals_or_the_specialist_go() {
    for case in 0..3 {
        let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
        match case {
            0 => {
                let tank = Tank {
                    show: true,
                    height: 5,
                    level: 0,
                    salt: false,
                    target: false,
                    purity: 100,
                    filled: true,
                    platform: (-1, -1),
                };
                tank_mut(&mut w).tank = Some(tank);
            }
            1 => info_mut(&mut w).records[0].units.clear(),
            _ => info_mut(&mut w).records[1].units.clear(),
        }
        events.clear();
        tick(&mut w, &cat, &mut events, 91);
        assert!(!info_of_show(&w).show.running, "case {case}");
        assert_eq!(kinds(&events)[..2], [0x2717, 0x2715], "case {case}");
    }
}

#[test]
fn a_running_show_is_aborted_when_its_script_is_gone_or_a_unit_is_not_found() {
    for case in 0..2 {
        let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
        match case {
            0 => w.zoo.shows.scripts.clear(),
            _ => {
                // The entity is gone but its id is still listed.
                let at = w.find(22).unwrap();
                w.zoo.entities.remove(at);
            }
        }
        events.clear();
        tick(&mut w, &cat, &mut events, 91);
        assert!(!info_of_show(&w).show.running, "case {case}");
        assert_eq!(kinds(&events)[..2], [0x2717, 0x2715], "case {case}");
    }
}

#[test]
fn a_ready_show_whose_animals_are_gone_is_aborted_and_not_started() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut events = Events::default();
    tick(&mut w, &cat, &mut events, 30);
    assert!(info_of_show(&w).show.ready);
    info_mut(&mut w).records[0].units.clear();
    events.clear();
    tick(&mut w, &cat, &mut events, 31);
    assert!(!info_of_show(&w).show.ready);
    // The abort and the end are told even though the show never ran.
    assert_eq!(kinds(&events)[..2], [0x2717, 0x2715]);
}

#[test]
fn a_show_info_reads_its_stop_and_its_abort() {
    for (kind, told) in [(0x2714, vec![0x2715]), (0x2716, vec![0x2717, 0x2715])] {
        let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
        events.clear();
        // Posted to the show on its channel, read at the next step.
        for target in [1001, 0, 7] {
            let note = Event {
                sender: 0,
                target,
                data: 0,
                extra: 0,
                kind,
                delay: 1,
                sender_channel: UNIT_CHANNEL,
                channel: SHOW_CHANNEL,
            };
            events.post(note);
        }
        events.age();
        update(
            &mut w,
            &cat,
            (&mut events, &mut Rng::default(), &NoData),
            91,
        );
        events.age();
        // Target 7 is another show's: only the two for this one are read, so the end shows twice.
        let sent: Vec<u16> = kinds(&events)
            .into_iter()
            .filter(|k| k != &0x271a)
            .collect();
        assert_eq!(&sent[..told.len()], &told[..], "{kind:x}");
        assert!(!info_of_show(&w).show.running, "{kind:x}");
    }
}

#[test]
fn a_new_script_chosen_while_the_show_ran_takes_the_place_of_the_old_at_the_stop() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    w.zoo.shows.scripts.push(Script {
        id: 2001,
        marker: u32::MAX,
        kind: SEAL,
        items: vec![item(0, 3, 5)],
    });
    info_mut(&mut w).records[0].pending = 2001;
    abort(&mut w, &cat, &mut events, 1001);
    let info = info_of_show(&w);
    assert_eq!(
        (info.records[0].script, info.records[0].pending),
        (2001, u16::MAX)
    );
    assert_eq!(
        w.zoo.shows.scripts.iter().map(|s| s.id).collect::<Vec<_>>(),
        [2001]
    );
    assert_eq!(info.schedule, [SEAL]);
    // A pending script with no item takes the species off the schedule; the index falls to 0.
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    w.zoo.shows.scripts.push(Script {
        id: 2002,
        marker: u32::MAX,
        kind: SEAL,
        items: Vec::new(),
    });
    info_mut(&mut w).records[0].pending = 2002;
    abort(&mut w, &cat, &mut events, 1001);
    let info = info_of_show(&w);
    assert_eq!((info.schedule.len(), info.index), (0, 0));
    // The index goes back to 0 when the species left was the last of the schedule: the abort
    // moved it to the last place (2) of three, the species leaves and the list has two.
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    w.zoo.shows.scripts.push(Script {
        id: 2002,
        marker: u32::MAX,
        kind: SEAL,
        items: Vec::new(),
    });
    let info = info_mut(&mut w);
    (info.schedule, info.index, info.records[0].pending) = (vec![5401, 5402, SEAL], 1, 2002);
    abort(&mut w, &cat, &mut events, 1001);
    let info = info_of_show(&w);
    assert_eq!((info.schedule.clone(), info.index), (vec![5401, 5402], 0));
}

#[test]
fn the_upkeep_of_the_grandstands_that_face_the_tank_is_refreshed_every_step() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let mut events = Events::default();
    let figure = |w: &World| {
        let up = info_of_show(w).upkeep;
        (f32::from_bits(up[0]), f32::from_bits(up[2]))
    };
    // The total holds 100 from earlier months. Stand 7 faces the tank; stand 8 is turned away.
    info_mut(&mut w).upkeep = [0, 0, 100.0f32.to_bits()];
    tick(&mut w, &cat, &mut events, 1);
    assert_eq!(figure(&w), (15.0, 115.0));
    tick(&mut w, &cat, &mut events, 2);
    assert_eq!(figure(&w), (15.0, 115.0));
    // A second stand facing it adds its own; the total follows.
    let mut stand = w.zoo.entities[w.find(7).unwrap()].clone();
    stand.common.as_mut().unwrap().id = 9;
    stand.common.as_mut().unwrap().y += 64;
    w.zoo.entities.push(stand);
    let ty = cat.types.iter().find(|t| t.type_name == b"stand").unwrap();
    crate::building::built(&mut w, ty, 9, 0);
    tick(&mut w, &cat, &mut events, 3);
    assert_eq!(figure(&w), (30.0, 130.0));
}

#[test]
fn emptying_a_grandstand_aborts_the_running_show_it_belongs_to() {
    for stand in [7, 8] {
        let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
        events.clear();
        empty_grandstands(&mut w, &cat, &mut events, stand);
        events.age();
        assert_eq!(kinds(&events), [0x2717, 0x2715], "stand {stand}");
        assert!(!info_of_show(&w).show.running, "stand {stand}");
        assert!(w.units.buildings[&7].users.is_empty(), "stand {stand}");
    }
    // A grandstand with no show aborts nothing.
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    w.units.buildings.get_mut(&7).unwrap().show_id = 0;
    events.clear();
    empty_grandstands(&mut w, &cat, &mut events, 7);
    assert!(info_of_show(&w).show.running && events.buffers()[1].is_empty());
}

#[test]
fn the_last_item_of_the_script_finishes_the_animal_and_then_the_show() {
    let (mut w, cat, mut events) = started(vec![item(0, 4, 5), item(0, 4, 5)]);
    tick(&mut w, &cat, &mut events, 91);
    for state in &mut info_mut(&mut w).show.items {
        (state.item, state.finished, state.cue_sent, state.ready) = (1, true, true, true);
    }
    events.clear();
    tick(&mut w, &cat, &mut events, 92);
    // Both are scored and done; no next item is cued.
    assert!(
        info_of_show(&w)
            .show
            .items
            .iter()
            .all(|s| s.done && s.item == 1)
    );
    assert_eq!(info_of_show(&w).show.done, 2);
    assert!(kinds(&events).iter().all(|k| *k != 0x271d));
    assert!(info_of_show(&w).show.running);
    events.clear();
    tick(&mut w, &cat, &mut events, 93);
    assert_eq!(kinds(&events), [0x2715]);
    assert!(!info_of_show(&w).show.running);
}

#[test]
fn the_events_a_show_info_holds_are_dropped_at_the_end_of_its_turn() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let held = Event {
        sender: 1,
        target: 2,
        data: 3,
        extra: 4,
        kind: 0x2711,
        delay: 1,
        sender_channel: UNIT_CHANNEL,
        channel: SHOW_CHANNEL,
    };
    info_mut(&mut w).events = vec![held];
    tick(&mut w, &cat, &mut Events::default(), 1);
    assert!(info_of_show(&w).events.is_empty());
}

#[test]
fn the_simulation_step_runs_the_show_manager_before_the_ai_clock_moves() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    // A tick of 100 ms: the first step is at tick 0, five ticks before the start time.
    let mut sim = crate::sim::Sim::new(b"[AI]\nmAITick=100\n");
    info_mut(&mut w).next = 5;
    sim.step_in(&mut w, &cat, 100, |_, _, _| {});
    assert!(info_of_show(&w).show.ready);
    assert_eq!(sim.ai_clock().0, 1);
}

#[test]
fn the_show_month_change_moves_the_figures_to_last_month_and_takes_the_upkeep_rate() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let info = info_mut(&mut w);
    let f = |x: f32| x.to_bits();
    (info.frequency, info.next) = (-1, -1);
    info.upkeep = [0, f(9.0), f(100.0)];
    info.receipts = [f(5.0), f(1.0), f(20.0)];
    info.attendance = [3, 2, 9];
    info.records[0].receipts = [f(4.0), f(1.5), f(8.0)];
    info.records[0].attendance = [2, 1, 7];
    let (mut sim, mut money) = (crate::sim::Sim::new(b""), crate::money::Money::default());
    sim.set_seed(1);
    sim.start_game(16, crate::zoo::Clocks::default());
    let mut ended = 0;
    for _ in 0..400 {
        let report = sim.step_game(
            &crate::behaviour::NoData,
            &cat,
            &mut w,
            &mut money,
            1000,
            |_, _, _| {},
        );
        ended += usize::from(report.month_end);
    }
    assert_eq!(ended, 1);
    // The stand facing the tank costs 15 a month: the step refreshed it, the change closed it.
    let info = info_of_show(&w);
    assert_eq!(info.upkeep, [f(15.0), f(15.0), f(130.0)]);
    assert_eq!(info.receipts, [0, f(5.0), f(20.0)]);
    assert_eq!(info.attendance, [0, 3, 9]);
    assert_eq!(info.records[0].receipts, [0, f(4.0), f(8.0)]);
    assert_eq!(info.records[0].attendance, [0, 2, 7]);
}
