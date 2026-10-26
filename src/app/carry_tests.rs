//! Picking a staff member up and putting it down through the log, on the map of the bulldozer
//! tests: a pen of one tile at (20, 20) and two zookeepers on the grounds.

use super::bulldozer_tests::{Session, with_game};
use super::*;

/// The lines of the pass about selecting, carrying and dropping.
fn told(lines: Vec<String>) -> Vec<String> {
    let words = [
        "tool select",
        "world carry",
        "world drop",
        "world putback",
        "tool message",
    ];
    let kept = |l: &String| words.iter().any(|w| l.starts_with(w));
    lines.into_iter().filter(kept).collect()
}

fn unit(s: &Session, id: u32) -> crate::unit::Unit {
    s.app
        .game
        .as_ref()
        .unwrap()
        .world
        .units
        .get(id)
        .unwrap()
        .clone()
}

fn tile(s: &Session, id: u32) -> (i32, i32) {
    let world = &s.app.game.as_ref().unwrap().world;
    let c = world.zoo.entities[world.find(id).unwrap()]
        .common
        .clone()
        .unwrap();
    (c.x.div_euclid(64), c.y.div_euclid(64))
}

#[test]
fn a_unit_is_selected_picked_up_refused_where_another_stands_and_put_down() {
    with_game("carry", "", |s| {
        for side in [0, 2, 4, 6] {
            s.fence(1, 20, 20, side);
        }
        let (a, b) = (
            s.add(12, 21 * 64 + 32, 20 * 64 + 32, 0),
            s.add(12, 19 * 64 + 32, 20 * 64 + 32, 0),
        );
        s.pass(&[]);
        let (here, other, pen) = (
            s.at(21, 20, 32, 32),
            s.at(19, 20, 32, 32),
            s.at(20, 20, 32, 32),
        );
        // The first click selects, the second picks the unit up: it is off the map and stopped.
        assert_eq!(told(s.click_log(here)), [format!("tool select {a}")]);
        // The staged picture carries the marker over the selected unit, and not any more over
        // the one that is picked up.
        assert_eq!(s.app.game.as_ref().unwrap().extra.selected, Some(a));
        assert_eq!(told(s.click_log(here)), [format!("world carry {a}")]);
        assert_eq!(s.app.game.as_ref().unwrap().extra.selected, None);
        let game = s.app.game.as_ref().unwrap();
        assert_eq!(
            (game.carry.as_ref().map(|c| c.id), game.selected),
            (Some(a), None)
        );
        assert!(unit(s, a).carried && game.world.units_on(20 * 40 + 21).count() == 0);
        // Over the other unit it is refused and stays carried; its pending cost is 0.
        assert_eq!(told(s.click_log(other)), ["tool message 10555"]);
        assert!(unit(s, a).carried);
        assert_eq!(s.cost(), Some(0));
        // In the pen it is put down, selected again, and a click on it picks it up at once.
        assert_eq!(told(s.click_log(pen)), [format!("world drop {a} 20,20")]);
        assert_eq!((unit(s, a).carried, tile(s, a)), (false, (20, 20)));
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(a));
        assert_eq!(told(s.click_log(pen)), [format!("world carry {a}")]);
        // Escape puts it back where it stood, without a check, and selects it again.
        s.pass(&[other]);
        let escape = Command::Key(Key::Escape, false, false);
        assert_eq!(told(s.log(&[escape])), [format!("world putback {a} 20,20")]);
        assert_eq!((unit(s, a).carried, tile(s, a)), (false, (20, 20)));
        assert_eq!(tile(s, b), (19, 20));
        assert_eq!(s.app.game.as_ref().unwrap().selected, Some(a));
        // The right button does the same, and a click on bare ground clears the selection.
        s.click(pen);
        assert!(unit(s, a).carried);
        s.pass(&[
            Command::Mouse(Mouse::Move(1, 1)),
            Command::Mouse(Mouse::Right),
        ]);
        assert!(!unit(s, a).carried);
        assert_eq!(
            told(s.click_log(s.at(20, 21, 32, 32))),
            Vec::<String>::new()
        );
        assert_eq!(s.app.game.as_ref().unwrap().selected, None);
    });
}

#[test]
fn a_click_on_the_gate_of_an_exhibit_selects_it_and_the_ground_clears_the_selection() {
    with_game("gate-marker", "", |s| {
        let pieces: Vec<u32> = [0, 2, 4, 6].map(|side| s.fence(1, 20, 20, side)).to_vec();
        s.app.exhibits_told();
        s.app.game.as_mut().unwrap().world.set_gate(1, pieces[0]);
        s.pass(&[]);
        let selected = |s: &Session| {
            let game = s.app.game.as_ref().unwrap();
            (game.selected, game.extra.selected)
        };
        assert_eq!(selected(s), (None, None));
        // The gate is on the north edge of the tile: the exhibit panel opens and the gate carries
        // the marker.
        s.click(s.near(20 * 64 + 32, 20 * 64, (0, 0)));
        s.pass(&[]);
        assert_eq!(selected(s), (Some(pieces[0]), Some(pieces[0])));
        assert!(
            s.app
                .ui
                .as_ref()
                .unwrap()
                .shown(crate::panel::exhibit::LAYOUT)
        );
        s.click(s.at(20, 22, 32, 32));
        s.pass(&[]);
        assert_eq!(selected(s), (None, None));
    });
}

#[test]
fn one_selection_gives_the_panel_and_the_marker_to_the_unit_clicked_or_listed() {
    with_game("panels", "", |s| {
        for side in [0, 2, 4, 6] {
            s.fence(1, 20, 20, side);
        }
        let (zebra, keeper) = (
            s.add(13, 21 * 64 + 32, 20 * 64 + 32, 0),
            s.add(12, 19 * 64 + 32, 20 * 64 + 32, 0),
        );
        // Two animals in the pen at (20, 20), the exhibit panel on it.
        let (a, b) = (
            s.add(13, 20 * 64 + 16, 20 * 64 + 32, 0),
            s.add(13, 20 * 64 + 48, 20 * 64 + 32, 0),
        );
        s.pass(&[]);
        // The panels shown and the unit marked.
        let shown = |s: &Session| {
            let ui = s.app.ui.as_ref().unwrap();
            let game = s.app.game.as_ref().unwrap();
            (
                (game.selected, game.extra.selected),
                ui.shown(crate::panel::animal::LAYOUT),
                ui.shown(crate::panel::staff::LAYOUT),
            )
        };
        assert_eq!(shown(s), ((None, None), false, false));
        s.click(s.at(21, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(shown(s), ((Some(zebra), Some(zebra)), true, false));
        assert_eq!(s.app.game.as_ref().unwrap().animal.selected, Some(zebra));
        // An egg or a crate makes the marker take its own footprint, the crate first.
        for ((egg, crated), form) in [
            ((true, false), Form::Egg),
            ((true, true), Form::Crate),
            ((false, true), Form::Crate),
            ((false, false), Form::Plain),
        ] {
            let unit = s.app.game.as_mut().unwrap().world.units.get_mut(zebra);
            if let Some(crate::unit::Part::Animal(a)) = unit.map(|u| &mut u.part) {
                (a.egg, a.crated) = (egg, crated);
            }
            s.pass(&[]);
            assert_eq!(s.app.game.as_ref().unwrap().extra.form, form);
        }
        // The panel's close button removes the selection and the marker.
        let ui = s.app.ui.as_mut().unwrap();
        assert!(ui.press_button(3100));
        // The panels follow after the picture of the pass, so the marker leaves with the next one.
        s.pass(&[]);
        s.pass(&[]);
        assert_eq!(shown(s), ((None, None), false, false));
        // A click on the keeper shows the staff panel instead; the ground removes both.
        s.click(s.at(19, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(shown(s), ((Some(keeper), Some(keeper)), false, true));
        s.click(s.at(20, 22, 32, 32));
        s.pass(&[]);
        assert_eq!(shown(s), ((None, None), false, false));
        // A row of the exhibit panel's animal list moves both to that animal.
        let exhibit = s.app.game.as_ref().unwrap().world.exhibits().list()[0].id;
        s.app.open_exhibit(exhibit);
        s.pass(&[]);
        for (row, id) in [(0, a), (1, b)] {
            s.app.panel_animal(row);
            s.pass(&[]);
            assert_eq!(shown(s), ((Some(id), Some(id)), true, false));
        }
    });
}

#[test]
fn a_click_on_a_guest_shows_the_guest_panel_alone_and_its_close_button_clears_it() {
    with_game("guest-panel", "", |s| {
        let (guest, zebra) = (
            s.add(14, 20 * 64 + 32, 20 * 64 + 32, 0),
            s.add(13, 22 * 64 + 32, 20 * 64 + 32, 0),
        );
        s.pass(&[]);
        let shown = |s: &Session| {
            let ui = s.app.ui.as_ref().unwrap();
            let game = s.app.game.as_ref().unwrap();
            (
                game.selected,
                ui.shown(crate::panel::guest::LAYOUT),
                ui.shown(crate::panel::animal::LAYOUT),
            )
        };
        assert_eq!(shown(s), (None, false, false));
        s.click(s.at(20, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(shown(s), (Some(guest), true, false));
        assert_eq!(s.app.game.as_ref().unwrap().guest.selected, Some(guest));
        // Another unit takes the panel; the close button clears the selection.
        s.click(s.at(22, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(shown(s), (Some(zebra), false, true));
        assert_eq!(s.app.game.as_ref().unwrap().guest.selected, None);
        s.click(s.at(20, 20, 32, 32));
        s.pass(&[]);
        assert_eq!(shown(s), (Some(guest), true, false));
        assert!(s.app.ui.as_mut().unwrap().press_button(3413));
        s.pass(&[]);
        s.pass(&[]);
        assert_eq!(shown(s), (None, false, false));
        assert_eq!(s.app.game.as_ref().unwrap().guest.selected, None);
    });
}

#[test]
fn the_selected_moveable_building_is_picked_up_free_put_down_or_put_back_and_the_shop_stays() {
    with_game("carry-building", "", |s| {
        let (cart, shop) = (
            s.add(15, 20 * 64 + 32, 20 * 64 + 32, 4),
            s.add(2, 22 * 64 + 32, 20 * 64 + 32, 4),
        );
        s.pass(&[]);
        let (here, there, other) = (
            s.at(20, 20, 32, 32),
            s.at(18, 20, 32, 32),
            s.at(22, 20, 32, 32),
        );
        let cash = s.cash();
        let place = |s: &Session| {
            let world = &s.app.game.as_ref().unwrap().world;
            let at = world.find(cart).map(|at| &world.zoo.entities[at]);
            at.and_then(|e| e.common.as_ref())
                .map(|c| (c.x, c.y, c.rotation))
        };
        // A building that is not moveable is never picked up, by the button or a click.
        assert!(!s.app.lift_building(shop));
        assert_eq!(told(s.click_log(other)), Vec::<String>::new());
        assert_eq!(told(s.click_log(other)), Vec::<String>::new());
        assert!(s.app.game.as_ref().unwrap().carry.is_none());
        // The first click selects the cart, the second picks it up: it is off the map and the
        // world, not in the undo list, with no cost and no selection.
        assert_eq!(told(s.click_log(here)), [format!("tool select {cart}")]);
        let (count, undo) = (s.count(), s.app.game.as_ref().unwrap().world.can_undo());
        assert_eq!(told(s.click_log(here)), [format!("world carry {cart}")]);
        let game = s.app.game.as_ref().unwrap();
        assert_eq!(
            (game.carry.as_ref().map(|c| c.id), game.selected),
            (Some(cart), None)
        );
        assert_eq!((place(s), s.count()), (None, count - 1));
        assert_eq!(game.world.can_undo(), undo);
        // It follows the pointer as a pending building at no cost, and a place that is not valid
        // (on the shop) refuses it with the message of a new one and keeps it carried.
        s.pass(&[there]);
        assert_eq!(s.cost(), Some(0));
        s.pass(&[other]);
        assert!(
            told(s.click_log(other))
                .iter()
                .any(|l| l.starts_with("tool message"))
        );
        assert!(place(s).is_none());
        // Put down on free ground: the same id, facing as it did, selected again, nothing paid.
        assert_eq!(
            told(s.click_log(there)),
            [format!("world drop {cart} 18,20")]
        );
        let (x, y, facing) = place(s).unwrap();
        assert_eq!((x.div_euclid(64), y.div_euclid(64), facing), (18, 20, 4));
        let game = s.app.game.as_ref().unwrap();
        assert_eq!(
            (game.carry.is_none(), game.held.is_none(), game.selected),
            (true, true, Some(cart))
        );
        assert_eq!((s.cash(), s.count()), (cash, count));
        // Picked up again, Escape puts it back where it stood, without a check.
        assert_eq!(told(s.click_log(there)), [format!("world carry {cart}")]);
        s.pass(&[other]);
        let escape = Command::Key(Key::Escape, false, false);
        assert_eq!(
            told(s.log(&[escape])),
            [format!("world putback {cart} 18,20")]
        );
        assert_eq!(place(s).map(|p| p.2), Some(4));
    });
}

#[test]
fn an_undo_while_a_building_is_held_puts_it_back_and_removes_it_with_its_whole_price() {
    with_game("carry-undo", "", |s| {
        let cart = s.add(15, 20 * 64 + 32, 20 * 64 + 32, 4);
        s.app.game.as_mut().unwrap().world.clear_undo();
        let shop = s.add(2, 22 * 64 + 32, 20 * 64 + 32, 4);
        s.pass(&[]);
        let here = s.at(20, 20, 32, 32);
        s.click(here);
        assert_eq!(told(s.click_log(here)), [format!("world carry {cart}")]);
        // The list holds another placement only: it is undone and the cart stays held.
        s.app.undo();
        let game = s.app.game.as_ref().unwrap();
        assert!(game.held.is_some() && game.carry.is_some());
        assert!(game.world.find(shop).is_none() && !game.world.can_undo());
    });
    with_game("carry-undo-placed", "", |s| {
        let cart = s.add(15, 20 * 64 + 32, 20 * 64 + 32, 4);
        s.app.game.as_mut().unwrap().world.priced(60.0);
        s.pass(&[]);
        let (here, cash) = (s.at(20, 20, 32, 32), s.cash());
        s.click(here);
        assert_eq!(told(s.click_log(here)), [format!("world carry {cart}")]);
        // The held cart is the last operation's placement: the move is cancelled, the cart is
        // removed, the price is given back and nothing is held.
        let told = s.log(&[Command::Key(Key::Backspace, false, false)]);
        assert!(
            told.iter()
                .any(|l| l.starts_with("world remove building/cart/")),
            "{told:?}"
        );
        let game = s.app.game.as_ref().unwrap();
        assert!(game.held.is_none() && game.carry.is_none() && game.selected.is_none());
        assert!(game.world.find(cart).is_none());
        assert_eq!(s.cash(), cash + 60.0);
    });
}

#[test]
fn a_selection_and_a_pick_up_play_the_sound_of_a_type_that_has_one() {
    with_game("sounds", "", |s| {
        let (zebra, keeper, cart) = (
            s.add(13, 21 * 64 + 32, 20 * 64 + 32, 0),
            s.add(12, 19 * 64 + 32, 20 * 64 + 32, 0),
            s.add(15, 25 * 64 + 32, 20 * 64 + 32, 4),
        );
        s.pass(&[]);
        // What the pass played: the sounds of types, and the plain `place` of the interface.
        let heard = |s: &Session| {
            let sounds = s.app.unit_sounds().iter();
            let all = sounds.map(|u| (u.id, u.file.clone(), u.base, u.distance, u.pan));
            (all.collect::<Vec<_>>(), s.app.sounds().contains(&"place"))
        };
        let escape = Command::Key(Key::Escape, false, false);
        // The zebra's own sounds, one voice for each, plain (no distance, no pan).
        let own = |id, file: &str, base| (id, file.to_string(), base, 0, 0);
        s.click(s.at(21, 20, 32, 32));
        let want = own(zebra, "animals/zebra/zplace.wav", 1500);
        assert_eq!(heard(s), (vec![want], false));
        s.click(s.at(21, 20, 32, 32));
        let want = own(zebra, "animals/zebra/zpick.wav", 0);
        assert_eq!(heard(s), (vec![want], false));
        s.pass(&[escape]);
        // A keeper, which has no [Sounds], and a cart are silent when selected and picked up.
        for (id, point) in [(keeper, s.at(19, 20, 32, 32)), (cart, s.at(25, 20, 32, 32))] {
            for _ in 0..2 {
                s.click(point);
                assert_eq!(heard(s), (vec![], false), "{id}");
            }
            s.pass(&[escape]);
        }
    });
}
