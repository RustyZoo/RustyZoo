//! The show panel's admission and the grandstand prices (shows 33).

use super::*;
use crate::config::Config;
use crate::show::admission::{press_admission, type_admission, update_admission};

const SPECIES: u32 = 5401;

/// A world with a stand facing the show tank (7), one turned away (8) and one with a narrow price
/// range facing it (9); the tank's exhibit and the second exhibit, which has no show info.
fn staged() -> (World, Catalogue, u32, u32) {
    let (mut w, cat) = world(9, "glass", &[(7, (1, 2), 2), (8, (1, 3), 0)]);
    let narrow = cat.types.iter().find(|t| t.type_name == b"narrow").unwrap();
    w.zoo
        .entities
        .push(entity("building", "narrow", 9, centre((1, 1)), 2));
    crate::building::built(&mut w, narrow, 9, 0);
    associate(&mut w, &cat);
    let tank = exhibit_of(&w, 1001).unwrap();
    let other = w.exhibits().region(w.tile_index(7, 0).unwrap()).unwrap();
    (w, cat, tank, other)
}

fn price(w: &World, id: u32) -> f32 {
    w.units.buildings[&id].price
}

#[test]
fn the_buttons_move_the_admission_of_the_species_and_every_linked_grandstand_takes_it() {
    let (mut w, cat, tank, _) = staged();
    let before = price(&w, 8);
    assert_eq!(
        press_admission(&mut w, &cat, tank, Some(SPECIES), true),
        0.25
    );
    assert_eq!((price(&w, 7), price(&w, 8)), (0.25, before));
    // The narrow stand is held at its floor.
    assert_eq!(price(&w, 9), 2.0);
    let r = info_of(&w, tank).unwrap().record(SPECIES).unwrap();
    assert_eq!(f32::from_bits(r.admission), 0.25);
    press_admission(&mut w, &cat, tank, Some(SPECIES), false);
    assert_eq!(price(&w, 7), 0.0);
    // Down at the minimum stays there.
    press_admission(&mut w, &cat, tank, Some(SPECIES), false);
    assert_eq!(price(&w, 7), 0.0);
}

#[test]
fn a_typed_admission_is_clamped_to_the_panel_and_then_to_each_grandstand() {
    let (mut w, cat, tank, _) = staged();
    let mut typed = |v: f32| type_admission(&mut w, &cat, tank, Some(SPECIES), v);
    assert_eq!(typed(7.5), 7.5);
    assert_eq!(typed(1e30), 10.0);
    assert_eq!(typed(-3.0), 0.0);
    assert_eq!(typed(f32::NAN), 0.0);
    assert_eq!(typed(10.0), 10.0);
    // The stands: the first takes it as is, the narrow one is cut at its top.
    assert_eq!((price(&w, 7), price(&w, 9)), (10.0, 5.0));
    // A raised panel maximum is cut by the economy's, then by the stand's.
    w.units.show.max_admission = 1e9;
    w.units.prices.max = 20.0;
    type_admission(&mut w, &cat, tank, Some(SPECIES), 1e8);
    assert_eq!((price(&w, 7), price(&w, 9)), (20.0, 5.0));
    // Up from the top reaches the maximum.
    w.units.show.max_admission = 3.0;
    w.units.show.increment = 0.0;
    type_admission(&mut w, &cat, tank, Some(SPECIES), 1.0);
    assert_eq!(
        press_admission(&mut w, &cat, tank, Some(SPECIES), true),
        1.0
    );
    w.units.show.increment = 5.0;
    assert_eq!(
        press_admission(&mut w, &cat, tank, Some(SPECIES), true),
        3.0
    );
}

#[test]
fn a_show_tank_with_no_species_sets_the_price_to_the_floor() {
    let (mut w, cat, tank, _) = staged();
    type_admission(&mut w, &cat, tank, Some(SPECIES), 6.0);
    assert_eq!(price(&w, 7), 6.0);
    assert_eq!(update_admission(&mut w, &cat, tank, None), 0.0);
    assert_eq!((price(&w, 7), price(&w, 9)), (0.0, 2.0));
    // A button or a typed number with no species changes no record.
    assert_eq!(press_admission(&mut w, &cat, tank, None, true), 0.0);
    assert_eq!(type_admission(&mut w, &cat, tank, None, 4.0), 0.0);
    assert_eq!(price(&w, 7), 0.0);
}

#[test]
fn an_exhibit_with_no_show_info_or_a_missing_one_changes_no_price() {
    let (mut w, cat, tank, other) = staged();
    type_admission(&mut w, &cat, tank, Some(SPECIES), 6.0);
    assert_eq!(type_admission(&mut w, &cat, other, Some(SPECIES), 9.0), 0.0);
    assert_eq!(update_admission(&mut w, &cat, 123_456, Some(SPECIES)), 0.0);
    assert_eq!(price(&w, 7), 6.0);
}

#[test]
fn updating_makes_the_missing_record_with_admission_0() {
    let (mut w, cat, tank, _) = staged();
    type_admission(&mut w, &cat, tank, Some(SPECIES), 6.0);
    assert_eq!(update_admission(&mut w, &cat, tank, Some(77)), 0.0);
    assert_eq!(
        f32::from_bits(info_of(&w, tank).unwrap().record(77).unwrap().admission),
        0.0
    );
    assert_eq!(price(&w, 7), 0.0);
}

#[test]
fn a_grandstand_that_no_longer_faces_the_show_keeps_its_price() {
    let (mut w, cat, tank, _) = staged();
    type_admission(&mut w, &cat, tank, Some(SPECIES), 6.0);
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().rotation = 0;
    type_admission(&mut w, &cat, tank, Some(SPECIES), 3.0);
    assert_eq!(price(&w, 7), 6.0);
}

#[test]
fn shows_cfg_gives_the_admission_step_and_bounds() {
    let read = |text: &str| Params::read(&Config::parse(text.as_bytes()).unwrap());
    let p = read("");
    assert_eq!(
        (p.increment, p.max_admission, p.min_admission),
        (0.25, 10.0, 0.0)
    );
    let p = read("[cost]\nshowAdmissionIncrement=0.5\nmaxShowAdmission=12\nminShowAdmission=1\n");
    assert_eq!(
        (p.increment, p.max_admission, p.min_admission),
        (0.5, 12.0, 1.0)
    );
    // A negative, a huge and a word: nothing panics, a key that is missing keeps its number.
    let p = read("[cost]\nmaxShowAdmission=-4\nminShowAdmission=1e30\nshowAdmissionIncrement=x\n");
    assert_eq!(
        (p.max_admission, p.min_admission, p.increment),
        (-4.0, 1e30, 0.0)
    );
    assert_eq!(read("[cost]\nmaxShowAdmission=3\n").increment, 0.25);
}
