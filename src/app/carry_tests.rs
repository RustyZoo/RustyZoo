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
        assert_eq!(told(s.click_log(here)), [format!("world carry {a}")]);
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
