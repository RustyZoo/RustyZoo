//! The records that a lookup makes (shows 32.7): each site of point 1, on the staged show, which
//! holds a record for the seals and one for the specialist unless a test takes it away.

use super::cycle::{SEAL, info_mut, info_of_show, item, staged, tick};
use super::*;
use crate::sim::{Event, Rng, SHOW_CHANNEL, WORLD_CHANNEL};
use crate::timing::file_time;
use crate::zoo::TypeRecord;

const NOW: i64 = 86_400_000;

/// The staged world without the record of `key`, at game time `NOW`.
fn without(key: u32) -> (World, Catalogue) {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    w.units.now = NOW;
    info_mut(&mut w).records.retain(|r| r.key != key);
    (w, cat)
}

/// What a lookup makes (point 3): no unit, script and pending 0xffff, admission 0, the figures 0
/// and the date of the moment.
fn made(key: u32) -> TypeRecord {
    TypeRecord::new(key, u16::MAX, file_time(NOW))
}

#[test]
fn the_first_announce_makes_the_record_of_the_specialist_type() {
    let (mut w, cat) = without(KEEPER);
    tick(&mut w, &cat, &mut Events::default(), 30);
    let info = info_of_show(&w);
    assert!(info.show.ready);
    assert_eq!(info.record(KEEPER), Some(&made(KEEPER)));
}

#[test]
fn the_due_tick_makes_the_record_of_the_specialist_type() {
    let (mut w, cat) = without(KEEPER);
    tick(&mut w, &cat, &mut Events::default(), 90);
    assert_eq!(info_of_show(&w).record(KEEPER), Some(&made(KEEPER)));
}

#[test]
fn the_script_lookup_of_the_scheduled_species_makes_its_record() {
    let (mut w, cat) = without(5401);
    info_mut(&mut w).schedule = vec![5401, SEAL];
    tick(&mut w, &cat, &mut Events::default(), 30);
    assert_eq!(info_of_show(&w).record(5401), Some(&made(5401)));
}

#[test]
fn the_unit_lists_of_the_current_species_make_its_record_and_an_idle_stop_makes_type_0() {
    let (mut w, cat) = without(0);
    assert!(info_of_show(&w).record(0).is_none());
    crate::show::cycle::abort(&mut w, &cat, &mut Events::default(), 1001);
    let info = info_of_show(&w);
    assert_eq!(info.record(0), Some(&made(0)));
    // In key order, before the others.
    assert_eq!(info.records[0].key, 0);
}

#[test]
fn the_specialists_need_test_makes_his_record_whatever_the_ready_flag() {
    let tank = tank_of(&staged(vec![]).0, 0).unwrap();
    // The keeper type is the scheduled one: the seals' (an animal type) or none (type 0).
    for (ready, schedule, made_one) in [
        (false, vec![SEAL], true),
        (true, vec![SEAL], true),
        (true, vec![], false),
    ] {
        let (mut w, cat) = without(KEEPER);
        let info = info_mut(&mut w);
        (info.show.ready, info.schedule) = (ready, schedule.clone());
        info.show.keeper = 0;
        needs_keeper(&mut w, &cat, tank, KEEPER);
        let record = info_of_show(&w).record(KEEPER);
        assert_eq!(
            record,
            made_one.then(|| made(KEEPER)).as_ref(),
            "{ready} {schedule:?}"
        );
    }
}

#[test]
fn the_relay_of_an_announce_makes_the_record_of_the_specialist_type() {
    let (mut w, cat) = without(KEEPER);
    let mut events = Events::default();
    events.post(Event {
        sender: 1001,
        target: 0,
        data: 0,
        extra: 1,
        kind: event::ANNOUNCE,
        delay: 1,
        sender_channel: SHOW_CHANNEL,
        channel: WORLD_CHANNEL,
    });
    events.age();
    listen_world(&mut w, &cat, &mut events, &mut Rng::default());
    assert_eq!(info_of_show(&w).record(KEEPER), Some(&made(KEEPER)));
}

#[test]
fn the_validation_of_an_animal_in_another_tank_looks_up_that_shows_keeper_type() {
    // Seal 21 stands in the second tank (point 1.1): its show info, with an empty schedule, gets the
    // record of type 0, with a seal scheduled the one of the specialist.
    for (schedule, key) in [(vec![], 0), (vec![SEAL], KEEPER)] {
        let (mut w, cat) = without(5401);
        let (x, y) = crate::unit::testing::centre((7, 1));
        let at = w.find(21).unwrap();
        let c = w.zoo.entities[at].common.as_mut().unwrap();
        (c.x, c.y) = (x, y);
        let mut other = info_of_show(&w).clone();
        (other.id, other.schedule, other.records) = (1002, schedule.clone(), Vec::new());
        w.zoo.shows.infos.push(Some(other));
        tick(&mut w, &cat, &mut Events::default(), 90);
        let other = w.zoo.shows.infos[1].as_ref().unwrap();
        assert_eq!(other.record(key), Some(&made(key)), "{schedule:?}");
    }
}
