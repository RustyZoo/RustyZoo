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
