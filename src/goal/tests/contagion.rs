//! The contagion cheats (cheats 4.2 E) on a synthetic map: unit 1 acts on the guests on its tile.

use super::*;
use crate::cheat::spread;

/// Unit 1 with these bits, and on its tile a man (2), a woman (3), a boy (4), a girl (5); a man (6)
/// stands one tile away. Every guest has the palette 0, 0, 0, 0.
fn crowd(bits: u8) -> World {
    let mut w = world((10, 10), |_, e| {
        let at = [(1, "man", (5, 5)), (2, "man", (5, 5)), (3, "woman", (5, 5))];
        let more = [(4, "boy", (5, 5)), (5, "girl", (5, 5)), (6, "man", (6, 5))];
        for (id, kind, tile) in at.into_iter().chain(more) {
            e.push(entity("guests", kind, id, centre(tile), 0));
        }
    });
    for e in &mut w.zoo.entities {
        e.common.as_mut().unwrap().colors = vec![0; 4];
    }
    w.units.get_mut(1).unwrap().spreads = bits;
    w
}

fn spread_once(w: &mut World) {
    with_cx(w, &mut Rng::default(), |cx| cx.spread());
}

fn guest(w: &World, id: u32) -> &crate::guest::Guest {
    match &w.units.get(id).unwrap().part {
        Part::Guest(g) => g,
        _ => panic!("not a guest"),
    }
}

fn shirt(w: &World, id: u32) -> u8 {
    let at = w.find(id).unwrap();
    w.zoo.entities[at].common.as_ref().unwrap().colors[0]
}

#[test]
fn sickness_stops_the_goals_of_the_guests_on_the_tile_and_makes_them_sick() {
    let mut w = crowd(spread::SICKNESS);
    for id in 2..=6 {
        w.units.get_mut(id).unwrap().goal.record.task = 100;
    }
    spread_once(&mut w);
    for id in 2..=5 {
        assert!(guest(&w, id).sick, "{id}");
        assert_eq!(w.units.get(id).unwrap().goal.record.task, 0, "{id}");
    }
    // Not the guest itself, nor one on another tile.
    assert!(!guest(&w, 1).sick && !guest(&w, 6).sick);
    assert_eq!(w.units.get(6).unwrap().goal.record.task, 100);
}

#[test]
fn a_busy_guest_is_spared_and_a_busy_spreader_does_nothing() {
    for task in [103, 104, 105, 106, 107] {
        let mut w = crowd(spread::SICKNESS | spread::SCARE_WOMEN | spread::YELLOW);
        w.units.get_mut(3).unwrap().goal.record.task = task;
        spread_once(&mut w);
        let busy = (104..=106).contains(&task);
        assert_eq!(guest(&w, 3).sick, !busy, "{task}");
        assert_eq!(guest(&w, 3).leave, !busy, "{task}");
        // The colour does not look at the state.
        assert_eq!(shirt(&w, 3), spread::YELLOW_SHIRT, "{task}");
        let mut w = crowd(spread::SICKNESS);
        w.units.get_mut(1).unwrap().goal.record.task = task;
        spread_once(&mut w);
        assert_eq!(guest(&w, 2).sick, !busy, "{task}");
    }
}

#[test]
fn the_scares_send_the_women_or_the_children_away() {
    let mut w = crowd(spread::SCARE_WOMEN);
    spread_once(&mut w);
    let left: Vec<bool> = (2..=6).map(|id| guest(&w, id).leave).collect();
    assert_eq!(left, [false, true, false, false, false]);
    let mut w = crowd(spread::SCARE_CHILDREN);
    spread_once(&mut w);
    let left: Vec<bool> = (2..=6).map(|id| guest(&w, id).leave).collect();
    assert_eq!(left, [false, false, true, true, false]);
}

#[test]
fn the_painters_paint_the_shirts_of_the_others_on_the_tile() {
    for (bit, colour) in [(spread::YELLOW, 9), (spread::RED, 1)] {
        let mut w = crowd(bit);
        spread_once(&mut w);
        let shirts: Vec<u8> = (1..=6).map(|id| shirt(&w, id)).collect();
        assert_eq!(shirts, [0, colour, colour, colour, colour, 0]);
    }
}

#[test]
fn no_bits_or_no_tile_changes_nothing() {
    let mut w = crowd(0);
    let before = w.zoo.entities.clone();
    spread_once(&mut w);
    assert_eq!(w.zoo.entities, before);
    assert!((2..=6).all(|id| !guest(&w, id).sick && !guest(&w, id).leave));
    // A unit off the map has no tile.
    let mut w = crowd(spread::SICKNESS);
    w.zoo.entities[0].common.as_mut().unwrap().x = -10_000;
    spread_once(&mut w);
    assert!((2..=6).all(|id| !guest(&w, id).sick));
}

#[test]
fn the_goal_update_of_the_spreader_runs_it() {
    let mut w = crowd(spread::YELLOW);
    run(&mut w, &mut sim(), 3);
    assert_eq!(shirt(&w, 2), spread::YELLOW_SHIRT);
    assert_eq!(shirt(&w, 6), 0);
}
