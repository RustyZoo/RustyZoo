//! The hover with no tool (unit placing 8.8): the tint and the name label, on the map of the
//! bulldozer tests.

use super::bulldozer_tests::{Session, catalogue, ty, with_game_of};
use super::*;

const STATUE: usize = 20;

fn with_hover(name: &str, body: impl FnOnce(&mut Session)) {
    let mut cat = catalogue();
    cat.push(ty(
        "objects",
        b"statue",
        b"",
        "[Characteristics/Integers]\ncSelectable=1\n",
        [1, 1, 1],
    ));
    with_game_of(name, "", (cat, super::bulldozer_tests::ROOT, &[]), body);
}

/// The id, the tint it is drawn with and the label due, with the pointer at tile (`tx`, `ty`).
fn hover(s: &mut Session, tx: i32, ty: i32) -> (Vec<usize>, Option<String>) {
    s.pass(&[s.at(tx, ty, 32, 32)]);
    s.pass(&[]);
    let game = s.app.game.as_ref().unwrap();
    let tinted = game.extra.tinted.iter().map(|t| t.0).collect();
    (tinted, s.app.interface().unwrap().tip_due())
}

fn name(s: &Session, id: u32) -> String {
    let world = &s.app.game.as_ref().unwrap().world;
    let at = world.find(id).unwrap();
    world.zoo.entities[at].common.as_ref().unwrap().name.clone()
}

#[test]
fn a_selectable_entity_is_tinted_and_shows_its_own_name() {
    with_hover("hover-both", |s| {
        let keeper = s.add(12, 21 * 64 + 32, 20 * 64 + 32, 0);
        let statue = s.add(STATUE, 23 * 64 + 32, 20 * 64 + 32, 0);
        let want = |s: &Session, id| s.app.game.as_ref().unwrap().world.find(id).unwrap();
        let label = name(s, keeper);
        assert!(!label.is_empty());
        let (tinted, tip) = hover(s, 21, 20);
        assert_eq!((tinted, tip), (vec![want(s, keeper)], Some(label)));
        let (tinted, tip) = hover(s, 23, 20);
        assert_eq!((tinted, tip), (vec![want(s, statue)], Some(name(s, statue))));
        let tint = s.app.game.as_ref().unwrap().placing.tints.highlight;
        assert_eq!(tint, [225, 225, 0]);
    });
}

#[test]
fn an_object_that_is_not_selectable_gets_the_label_without_the_tint() {
    with_hover("hover-label", |s| {
        let tree = s.add(7, 21 * 64 + 32, 20 * 64 + 32, 0);
        let (tinted, tip) = hover(s, 21, 20);
        assert_eq!((tinted, tip), (vec![], Some(name(s, tree))));
        assert!(tip_is_set(s));
    });
}

fn tip_is_set(s: &mut Session) -> bool {
    s.app.interface().unwrap().tip_due().is_some()
}

#[test]
fn a_fence_piece_a_path_and_bare_ground_get_neither_and_a_gate_gets_the_label() {
    with_hover("hover-neither", |s| {
        s.fence(1, 20, 20, 0);
        s.add(0, 22 * 64 + 32, 20 * 64 + 32, 0);
        let gate = s.fence(4, 24, 20, 0);
        let none = (vec![], None);
        assert_eq!(hover(s, 20, 19), none, "above the pen");
        assert_eq!(hover(s, 20, 20), none, "fence");
        assert_eq!(hover(s, 22, 20), none, "path");
        assert_eq!(hover(s, 30, 30), none, "ground");
        let (tinted, tip) = hover(s, 24, 20);
        assert_eq!((tinted, tip), (vec![], Some(name(s, gate))));
    });
}

#[test]
fn nothing_is_hovered_with_a_tool_and_the_label_goes_with_the_entity() {
    with_hover("hover-tool", |s| {
        s.add(STATUE, 21 * 64 + 32, 20 * 64 + 32, 0);
        assert!(hover(s, 21, 20).1.is_some());
        assert_eq!(hover(s, 25, 25), (vec![], None), "off the entity");
        s.pass(&[super::bulldozer_tests::DELETE]);
        assert_eq!(hover(s, 21, 20).0.len(), 1, "the bulldozer's own tint");
        assert!(s.app.game.as_ref().unwrap().placing.hovered.is_none());
        assert_eq!(s.app.game.as_ref().unwrap().dozer.at.len(), 1);
    });
}
