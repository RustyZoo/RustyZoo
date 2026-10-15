//! The undo button and the Backspace key through the pass (placing 8), on the made-up
//! map and left bar of the bulldozer tests.

use super::bulldozer_tests::{DELETE, Session, with_game};
use super::*;
use crate::ui::layout::DISABLED;

const BACKSPACE: Command = Command::Key(Key::Backspace, false, false);
const UP: Command = Command::Mouse(Mouse::Up);
const DOWN: Command = Command::Mouse(Mouse::Down);

/// Whether the Undo button is enabled.
fn enabled(s: &mut Session) -> bool {
    let ui = s.app.interface().unwrap();
    ui.tree
        .find(1075)
        .is_some_and(|i| ui.tree.elements[i].state & DISABLED == 0)
}

/// The hash of the frame the last pass painted, with the pointer off the map so that no pending
/// object is drawn.
fn picture(s: &mut Session) -> u64 {
    s.pass(&[Command::Mouse(Mouse::Move(284, 224))]);
    s.pass(&[]);
    frame_hash(&s.app.frame)
}

/// Lays 8 dirt path pieces: (20, 20) to (24, 20), then (24, 21) to (24, 23).
fn lay_eight(s: &mut Session) -> Vec<String> {
    s.app.game.as_mut().unwrap().build.hold(0);
    s.pass(&[s.at(20, 20, 32, 32), DOWN, s.at(21, 20, 32, 32)]);
    s.pass(&[s.at(24, 23, 32, 32)]);
    s.log(&[UP])
}

fn world<'a>(s: &'a Session<'_>) -> &'a World {
    &s.app.game.as_ref().unwrap().world
}

#[test]
fn backspace_takes_back_a_drag_of_paths_with_the_full_price_and_greys_the_button() {
    with_game("path", "", |s| {
        assert!(!enabled(s), "nothing to undo yet");
        let before = picture(s);
        let told = lay_eight(s);
        assert_eq!(
            told.iter().filter(|l| l.starts_with("world add")).count(),
            8
        );
        assert!(told.contains(&"money cash 49920".to_string()), "{told:?}");
        assert!(enabled(s));
        let told = s.log(&[BACKSPACE]);
        let removes: Vec<&String> = told
            .iter()
            .filter(|l| l.starts_with("world remove"))
            .collect();
        assert_eq!(removes.len(), 8, "{told:?}");
        assert_eq!(removes[0], "world remove paths/dirt/ 24,23 id 8 undo");
        assert_eq!(removes[7], "world remove paths/dirt/ 20,20 id 1 undo");
        assert!(told.contains(&"tool undo 8".to_string()));
        assert!(told.contains(&"money cash 50000".to_string()), "{told:?}");
        let money = &s.app.game.as_ref().unwrap().money;
        assert_eq!(money.total[crate::money::CONSTRUCTION], 0.0);
        assert_eq!((s.count(), s.cash(), enabled(s)), (0, 50_000.0, false));
        assert_eq!(picture(s), before, "the picture of before the operation");
        // Once: a second press has nothing to take back.
        assert!(s.log(&[BACKSPACE]).iter().all(|l| !l.starts_with("world")));
    });
}

#[test]
fn a_press_that_records_nothing_keeps_the_list_and_the_button_works_like_the_key() {
    with_game("keep", "", |s| {
        lay_eight(s);
        // A bulldozer click on bare ground records nothing.
        s.pass(&[DELETE]);
        s.click(s.at(30, 30, 32, 32));
        assert_eq!((s.count(), enabled(s)), (8, true));
        // The button itself (at 284, 224 of the made-up bar).
        let told = s.log(&[Command::Mouse(Mouse::Move(284, 224)), DOWN, UP]);
        assert_eq!(told.iter().filter(|l| l.ends_with(" undo")).count(), 8);
        assert_eq!((s.count(), s.cash()), (0, 50_000.0));
    });
}

#[test]
fn an_area_removal_is_put_back_and_its_refund_taken_back() {
    with_game("area", "", |s| {
        let ids: Vec<u32> = (18..25)
            .map(|x| s.add(0, x * 64 + 32, 20 * 64 + 32, 0))
            .collect();
        s.pass(&[DELETE]);
        s.pass(&[s.at(18, 20, 32, 32), DOWN, s.at(19, 20, 32, 32)]);
        s.pass(&[s.at(24, 20, 32, 32)]);
        s.pass(&[UP]);
        assert_eq!((s.count(), s.cash()), (0, 50_056.0));
        let told = s.log(&[BACKSPACE]);
        let adds: Vec<&String> = told.iter().filter(|l| l.starts_with("world add")).collect();
        assert_eq!(adds.len(), 7, "{told:?}");
        // The last removed comes back first.
        assert_eq!(
            adds[0],
            &format!("world add paths/dirt/ 24,20 id {} undo", ids[6])
        );
        assert!(told.contains(&"money cash 50000".to_string()), "{told:?}");
        let money = &s.app.game.as_ref().unwrap().money;
        assert_eq!(money.total[crate::money::CONSTRUCTION_REFUND], 0.0);
        // They are the same objects, in the same places of the list, with the same ids.
        let back: Vec<u32> = world(s)
            .zoo
            .entities
            .iter()
            .filter_map(|e| e.common.as_ref().map(|c| c.id))
            .collect();
        assert_eq!(back, ids);
    });
}

#[test]
fn a_path_of_another_type_and_a_fence_of_the_same_type_are_replaced_in_place_and_undone() {
    with_game("retype", "", |s| {
        let dirt = s.add(0, 20 * 64 + 32, 20 * 64 + 32, 0);
        let fence = s.fence(1, 20, 20, 0);
        // A decayed fence of the same type: replaced in place, healthy, for the full price.
        {
            let game = s.app.game.as_mut().unwrap();
            let at = game.world.find(fence).unwrap();
            let life = &mut game.world.zoo.entities[at].common.as_mut().unwrap().fence;
            life.as_mut().unwrap().life = 0;
        }
        s.app.game.as_mut().unwrap().build.hold(1);
        let told = s.click_log(s.at(20, 20, 32, 12));
        assert!(
            told.contains(&format!(
                "world retype fences/iron/f 20,20 side 0 id {fence}"
            )),
            "{told:?}"
        );
        assert!(told.contains(&"money cash 49875".to_string()));
        let life = |s: &Session| {
            let w = world(s);
            w.zoo.entities[w.find(fence).unwrap()]
                .common
                .as_ref()
                .unwrap()
                .fence
                .unwrap()
                .life
        };
        assert_eq!(life(s), 10);
        let told = s.log(&[BACKSPACE]);
        assert!(told.contains(&format!(
            "world retype fences/iron/f 20,20 side 0 id {fence} undo"
        )));
        assert!(told.contains(&"money cash 50000".to_string()));
        assert_eq!(life(s), 0, "the old life and strength come back");
        // A path of another type keeps the object and its place in the list.
        s.app.game.as_mut().unwrap().build.hold(7);
        let told = s.click_log(s.at(20, 20, 32, 32));
        assert!(
            told.contains(&format!("world retype paths/stone/ 20,20 id {dirt}")),
            "{told:?}"
        );
        assert_eq!(s.cash(), 49_980.0);
        let w = world(s);
        assert_eq!(w.zoo.entities[w.find(dirt).unwrap()].type_name, "stone");
        assert_eq!(w.find(dirt), Some(0), "the place in the entity list");
        s.log(&[BACKSPACE]);
        let w = world(s);
        assert_eq!(w.zoo.entities[0].type_name, "dirt");
        assert_eq!((w.zoo.entities.len(), s.cash()), (2, 50_000.0));
    });
}

#[test]
fn a_fence_of_another_type_takes_the_place_of_a_gate_as_a_gate_and_the_undo_gives_the_gate_back() {
    with_game("gate", "", |s| {
        // A tile closed by steel fences; the west piece is its gate.
        let pieces: Vec<u32> = [0, 2, 4, 6].map(|d| s.fence(8, 20, 20, d)).to_vec();
        s.app.exhibits_told();
        let game = s.app.game.as_mut().unwrap();
        game.world.set_gate(1, pieces[3]);
        let at = game.world.find(pieces[3]).unwrap();
        game.build.hold(1);
        let told = s.click_log(s.at(20, 20, 12, 32));
        let w = world(s);
        let new = w.exhibits().get(1).unwrap().gate.unwrap();
        assert_ne!(new, pieces[3]);
        assert!(told.contains(&"money cash 49875".to_string()), "{told:?}");
        let e = &w.zoo.entities[at];
        assert_eq!(
            (
                e.type_name.as_str(),
                e.subtype.as_str(),
                e.common.as_ref().unwrap().id
            ),
            ("iron", "g", new),
            "a new entity at the old one's place, still the gate"
        );
        assert_eq!(w.exhibits().list().len(), 1, "the exhibit stays closed");
        s.log(&[BACKSPACE]);
        let w = world(s);
        let e = &w.zoo.entities[at];
        assert_eq!((e.type_name.as_str(), e.subtype.as_str()), ("steel", "g"));
        assert_eq!(w.exhibits().get(1).unwrap().gate, Some(pieces[3]));
        assert_eq!(s.cash(), 50_000.0);
    });
}

#[test]
fn bulldozing_the_piece_that_closed_an_exhibit_is_undone_with_its_name_and_gate() {
    with_game("exhibit", "[UI]\nconfirmSell=0\n", |s| {
        let pieces: Vec<u32> = [0, 2, 4, 6].map(|d| s.fence(1, 20, 20, d)).to_vec();
        s.app.exhibits_told();
        let game = s.app.game.as_mut().unwrap();
        game.world.rename(1, "Lions".into());
        game.world.set_gate(1, pieces[1]);
        // Bulldoze the north piece; the question about the animals is answered Yes.
        s.pass(&[DELETE]);
        s.click(s.at(20, 20, 32, 4));
        assert!(s.asking());
        let told = s.log(&[Command::Key(Key::Y, false, false)]);
        assert!(
            told.iter().any(|l| l.starts_with("world remove")),
            "{told:?}"
        );
        assert!(world(s).exhibits().list().is_empty());
        assert_eq!(s.cash(), 50_100.0);
        let told = s.log(&[BACKSPACE]);
        assert!(told.contains(&format!(
            "world add fences/iron/f 20,20 side 0 id {} undo",
            pieces[0]
        )));
        assert!(
            told.iter().any(|l| l.starts_with("exhibit new")),
            "{told:?}"
        );
        let w = world(s);
        let e = w.exhibits().list();
        assert_eq!(e.len(), 1);
        assert_eq!((e[0].name.as_str(), e[0].gate), ("Lions", Some(pieces[1])));
        assert_eq!(s.cash(), 50_000.0);
        // No name is asked and the counter did not move.
        assert_eq!(w.exhibits().counter, 2);
        assert!(!s.asking());
    });
}

#[test]
fn the_gate_of_a_line_that_another_piece_keeps_closed_goes_back_to_its_piece() {
    with_game("held", "[UI]\nconfirmSell=0\n", |s| {
        let pieces: Vec<u32> = [0, 2, 4, 6].map(|d| s.fence(1, 20, 20, d)).to_vec();
        // A second piece on the north line, from the other tile.
        let twin = s.fence(1, 20, 19, 4);
        s.app.exhibits_told();
        let game = s.app.game.as_mut().unwrap();
        game.world.rename(1, "Lions".into());
        game.world.set_gate(1, pieces[0]);
        s.pass(&[DELETE]);
        s.click(s.at(20, 20, 32, 4));
        let w = world(s);
        assert_eq!(w.exhibits().list().len(), 1, "the line stays closed");
        assert_eq!(
            w.exhibits().get(1).unwrap().gate,
            Some(twin),
            "the other piece is the gate"
        );
        s.log(&[BACKSPACE]);
        let w = world(s);
        assert_eq!(w.exhibits().get(1).unwrap().gate, Some(pieces[0]));
        let at = w.find(twin).unwrap();
        assert_eq!(
            w.zoo.entities[at].subtype, "f",
            "the other piece is a fence again"
        );
        assert_eq!(w.exhibits().get(1).unwrap().name, "Lions");
    });
}

#[test]
fn closing_an_exhibit_is_undone_with_the_exhibit_and_a_new_press_forgets_the_old_one() {
    with_game("close", "", |s| {
        s.app.game.as_mut().unwrap().build.hold(1);
        for (lx, ly) in [(32, 12), (52, 32), (32, 52), (12, 32)] {
            s.click(s.at(20, 20, lx, ly));
        }
        assert_eq!(world(s).exhibits().list().len(), 1);
        assert_eq!(s.cash(), 50_000.0 - 500.0);
        // The last press closed it: the undo takes the piece and the exhibit away.
        let told = s.log(&[BACKSPACE]);
        assert!(
            told.iter().any(|l| l.starts_with("exhibit removed")),
            "{told:?}"
        );
        assert_eq!(
            (world(s).exhibits().list().len(), s.count(), s.cash()),
            (0, 3, 49_625.0)
        );
        // The earlier presses are not on the list any more.
        assert!(!enabled(s));
    });
}

#[test]
fn a_removed_object_that_cannot_be_put_back_stops_the_undo_with_a_message() {
    with_game("blocked", "", |s| {
        let path = s.add(0, 20 * 64 + 32, 20 * 64 + 32, 0);
        s.pass(&[DELETE]);
        s.click(s.at(20, 20, 32, 32));
        assert_eq!(s.count(), 0);
        // Something stands on the tile now.
        let (blocker, shape) = s.entity(0, 20 * 64 + 32, 20 * 64 + 32, 0);
        let other = s
            .app
            .game
            .as_mut()
            .unwrap()
            .world
            .unrecorded(|w| w.add(blocker, shape))
            .unwrap();
        let told = s.log(&[BACKSPACE]);
        assert!(
            told.contains(&format!("world blocked paths/dirt/ 20,20 id {path}")),
            "{told:?}"
        );
        assert!(
            told.iter()
                .any(|l| l.starts_with("menu message blocked: Shop")),
            "{told:?}"
        );
        assert!(s.asking(), "the dialog");
        assert_eq!((s.count(), s.cash()), (1, 50_008.0), "still sold");
        assert!(enabled(s), "the list is kept");
        // Later, when the way is free, the same button puts it back.
        s.pass(&[Command::Key(Key::Enter, false, false)]);
        s.app
            .game
            .as_mut()
            .unwrap()
            .world
            .unrecorded(|w| w.remove(other));
        let told = s.log(&[BACKSPACE]);
        assert!(
            told.contains(&format!("world add paths/dirt/ 20,20 id {path} undo")),
            "{told:?}"
        );
        assert_eq!((s.count(), s.cash(), enabled(s)), (1, 50_000.0, false));
    });
}

#[test]
fn among_several_removed_objects_the_one_that_cannot_come_back_is_passed_over_silently() {
    with_game("several", "", |s| {
        for x in [20, 21] {
            s.add(0, x * 64 + 32, 20 * 64 + 32, 0);
        }
        s.pass(&[DELETE]);
        s.pass(&[s.at(20, 20, 32, 32), DOWN, s.at(21, 20, 32, 32)]);
        s.pass(&[UP]);
        assert_eq!((s.count(), s.cash()), (0, 50_016.0));
        let (blocker, shape) = s.entity(0, 21 * 64 + 32, 20 * 64 + 32, 0);
        s.app
            .game
            .as_mut()
            .unwrap()
            .world
            .unrecorded(|w| w.add(blocker, shape));
        let told = s.log(&[BACKSPACE]);
        assert!(
            told.iter()
                .any(|l| l.starts_with("world skip paths/dirt/ 21,20")),
            "{told:?}"
        );
        assert!(
            told.iter()
                .any(|l| l.starts_with("world add paths/dirt/ 20,20"))
        );
        assert!(
            !told.iter().any(|l| l.starts_with("menu message")),
            "no dialog"
        );
        assert!(!s.asking());
        // The skipped one stays sold with its refund; the list is emptied as usual.
        assert_eq!((s.count(), s.cash(), enabled(s)), (2, 50_008.0, false));
    });
}
