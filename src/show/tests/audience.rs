//! The search of a guest for a show and the price of a seat (shows 7.1, 7.3).

use super::*;
use crate::exhibit::Tank;
use crate::sim::Rng;
use crate::zoo::TypeRecord;

const REACH: i32 = 150;

/// Stands 7 at (1, 2) and 8 at (1, 4) both face the tank; the flags of `soon` go up, and the exhibit
/// is a show tank when `tank`.
fn audience(soon: &[u32], tank: bool) -> (World, Catalogue) {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2), (8, (1, 4), 2)]);
    associate(&mut w, &cat);
    for &id in soon {
        w.units.buildings.get_mut(&id).unwrap().fill = 1;
    }
    if tank {
        let x = w.exhibits().region(w.tile_index(5, 0).unwrap()).unwrap();
        w.exhibits_mut().get_mut(x).unwrap().tank = Some(Tank {
            show: true,
            height: 5,
            level: 0,
            salt: false,
            target: false,
            purity: 100,
            filled: true,
            platform: (-1, -1),
        });
    }
    (w, cat)
}

/// The draws a search makes: the generator after it is the one after `n` draws of 0 to 14.
fn drew(rng: &Rng, n: usize) -> bool {
    let mut other = Rng::default();
    (0..n).for_each(|_| _ = other.pick(15));
    *rng == other
}

fn tile(w: &World, x: i32, y: i32) -> usize {
    w.tile_index(x, y).unwrap()
}

#[test]
fn only_a_grandstand_with_a_show_soon_is_asked_and_draws_once() {
    for (soon, drawn, found) in [(&[][..], 0, None), (&[8], 1, Some(8))] {
        let (w, cat) = audience(soon, true);
        let mut rng = Rng::default();
        let got = find(&w, &cat, &mut rng, Some(tile(&w, 1, 2)), &[], REACH);
        assert!(drew(&rng, drawn), "{soon:?}");
        assert_eq!(got, found, "{soon:?}");
    }
    // Both up: two draws, in the order of the list.
    let (w, cat) = audience(&[7, 8], true);
    let mut rng = Rng::default();
    find(&w, &cat, &mut rng, Some(tile(&w, 1, 2)), &[], REACH);
    assert!(drew(&rng, 2));
}

#[test]
fn the_nearest_grandstand_wins_after_the_draw_is_added_to_the_distance() {
    let (w, cat) = audience(&[7, 8], true);
    // Stand 8 is 2 tiles from the guest at (1, 2), stand 7 none; the draws are those of the replay.
    let mut replay = Rng::default();
    let (r7, r8) = (replay.pick(15) as i32, replay.pick(15) as i32);
    let want = if r7 <= 2 + r8 { 7 } else { 8 };
    let got = find(
        &w,
        &cat,
        &mut Rng::default(),
        Some(tile(&w, 1, 2)),
        &[],
        REACH,
    );
    assert_eq!(got, Some(want));
}

#[test]
fn a_grandstand_farther_than_the_reach_is_not_chosen() {
    let (w, cat) = audience(&[7], true);
    let r = Rng::default().pick(15) as i32;
    let here = Some(tile(&w, 1, 2));
    assert_eq!(find(&w, &cat, &mut Rng::default(), here, &[], r), Some(7));
    assert_eq!(find(&w, &cat, &mut Rng::default(), here, &[], r - 1), None);
    // Negative and huge reaches.
    assert_eq!(find(&w, &cat, &mut Rng::default(), here, &[], -1), None);
    assert_eq!(
        find(&w, &cat, &mut Rng::default(), here, &[], i32::MAX),
        Some(7)
    );
}

#[test]
fn a_guest_that_has_seen_the_show_tank_does_not_go_but_the_draw_is_made() {
    let (w, cat) = audience(&[7], true);
    let mut rng = Rng::default();
    let seen = [tile(&w, 6, 3)];
    let here = Some(tile(&w, 1, 2));
    assert_eq!(find(&w, &cat, &mut rng, here, &seen, REACH), None);
    assert!(drew(&rng, 1));
    // A tile outside the tank does not count.
    let other = [tile(&w, 2, 3)];
    assert_eq!(
        find(&w, &cat, &mut Rng::default(), here, &other, REACH),
        Some(7)
    );
}

#[test]
fn an_exhibit_that_is_no_tank_has_no_show_to_have_seen() {
    let (w, cat) = audience(&[7], false);
    let seen = [tile(&w, 6, 3)];
    let got = find(
        &w,
        &cat,
        &mut Rng::default(),
        Some(tile(&w, 1, 2)),
        &seen,
        REACH,
    );
    assert_eq!(got, Some(7));
}

#[test]
fn a_guest_with_no_tile_sums_with_the_largest_number_and_wraps() {
    let (w, cat) = audience(&[7], true);
    let r = Rng::default().pick(15);
    let got = find(&w, &cat, &mut Rng::default(), None, &[], REACH);
    assert_eq!(got, (r > 0).then_some(7));
}

#[test]
fn a_seat_is_booked_in_the_type_record_and_the_info_this_month_and_in_all() {
    let (mut w, _) = audience(&[], true);
    w.zoo.shows.infos[0].as_mut().unwrap().show.species = 55;
    assert!(pay(&mut w, 1001, 2.5));
    assert!(pay(&mut w, 1001, 1.0));
    let info = w.zoo.shows.infos[0].as_ref().unwrap();
    let money = |bits: [u32; 3]| bits.map(f32::from_bits);
    let record = info.record(55).unwrap();
    assert_eq!(money(record.receipts), [3.5, 0.0, 3.5]);
    assert_eq!(record.attendance, [2, 0, 2]);
    assert_eq!(money(info.receipts), [3.5, 0.0, 3.5]);
    assert_eq!(info.attendance, [2, 0, 2]);
    // A show that is not there books nothing.
    assert!(!pay(&mut w, 4242, 1.0));
    assert!(!pay(&mut w, 0, 1.0));
}

#[test]
fn a_seat_for_a_species_with_no_record_makes_it_in_key_order_with_no_script_and_the_date() {
    for (species, keys) in [(5, [1, 5, 9]), (0, [0, 1, 9]), (12, [1, 9, 12])] {
        let (mut w, _) = audience(&[], true);
        w.units.now = 86_400_000;
        let info = w.zoo.shows.infos[0].as_mut().unwrap();
        info.show.species = species;
        info.records = vec![TypeRecord::new(1, 3, 7), TypeRecord::new(9, 4, 8)];
        assert!(pay(&mut w, 1001, 2.0));
        let info = w.zoo.shows.infos[0].as_ref().unwrap();
        assert_eq!(info.records.iter().map(|r| r.key).collect::<Vec<_>>(), keys);
        let made = info.record(species).unwrap();
        let date = crate::timing::file_time(86_400_000);
        assert_eq!(
            (made.script, made.pending, made.date),
            (u16::MAX, u16::MAX, date)
        );
        assert_eq!(made.attendance, [1, 0, 1]);
    }
}
