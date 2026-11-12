//! The hover with no tool (unit placing 8.8): the tint and the name label, on the map of the
//! bulldozer tests.

use super::bulldozer_tests::{Session, catalogue, ty, with_game_of};

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
    with_game_of(name, "", (cat, super::bulldozer_tests::ROOT, &[]), |s| {
        // The label stays due for the test to read.
        let ui = s.app.interface().unwrap();
        ui.set_settings(crate::ui::interface::Settings {
            tooltip_delay: 1_000_000,
            ..ui.settings()
        });
        body(s);
    });
}

/// The id, the tint it is drawn with and the label due, with the pointer at tile (`tx`, `ty`).
fn hover(s: &mut Session, tx: i32, ty: i32, ly: i32) -> (Vec<usize>, Option<String>) {
    s.pass(&[s.at(tx, ty, 32, ly)]);
    s.pass(&[]);
    let game = s.app.game.as_ref().unwrap();
    let tinted = game.extra.tinted.iter().map(|t| t.0).collect();
    (tinted, s.app.interface().unwrap().tip_due())
}

/// Gives the entity its own name, as the game does when it is made.
fn named(s: &mut Session, id: u32, text: &str) -> u32 {
    let world = &mut s.app.game.as_mut().unwrap().world;
    let at = world.find(id).unwrap();
    world.rename_entity(at, text.as_bytes());
    id
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
        named(s, keeper, "Mia");
        let statue = s.add(STATUE, 23 * 64 + 32, 20 * 64 + 32, 0);
        named(s, statue, "Venus");
        let want = |s: &Session, id| s.app.game.as_ref().unwrap().world.find(id).unwrap();
        let label = name(s, keeper);
        assert!(!label.is_empty());
        let (tinted, tip) = hover(s, 21, 20, 32);
        assert_eq!((tinted, tip), (vec![want(s, keeper)], Some(label)));
        let (tinted, tip) = hover(s, 23, 20, 32);
        assert_eq!((tinted, tip), (vec![want(s, statue)], Some(name(s, statue))));
        let tint = s.app.game.as_ref().unwrap().placing.tints.highlight;
        assert_eq!(tint, [225, 225, 0]);
    });
}

#[test]
fn an_object_that_is_not_selectable_gets_the_label_without_the_tint() {
    with_hover("hover-label", |s| {
        let tree = s.add(7, 21 * 64 + 32, 20 * 64 + 32, 0);
        named(s, tree, "Jeep 2");
        let (tinted, tip) = hover(s, 21, 20, 32);
        assert_eq!((tinted, tip), (vec![], Some(name(s, tree))));
    });
}

#[test]
fn a_fence_piece_a_path_and_bare_ground_get_neither_and_a_gate_gets_the_label() {
    with_hover("hover-neither", |s| {
        s.fence(1, 20, 20, 0);
        s.add(0, 22 * 64 + 32, 20 * 64 + 32, 0);
        let gate = s.fence(4, 24, 20, 0);
        named(s, gate, "Gate 1");
        let piece = s.fence(1, 26, 20, 0);
        named(s, piece, "Fence 1");
        let none = (vec![], None);
        assert_eq!(hover(s, 20, 19, 32), none, "above the pen");
        assert_eq!(hover(s, 20, 20, 4), none, "fence");
        assert_eq!(hover(s, 22, 20, 32), none, "path");
        assert_eq!(hover(s, 26, 20, 4), none, "named fence piece");
        assert_eq!(hover(s, 30, 30, 32), none, "ground");
        let (tinted, tip) = hover(s, 24, 20, 4);
        assert_eq!((tinted, tip), (vec![], Some(name(s, gate))));
    });
}

#[test]
fn nothing_is_hovered_with_a_tool_and_the_label_goes_with_the_entity() {
    with_hover("hover-tool", |s| {
        let statue = s.add(STATUE, 21 * 64 + 32, 20 * 64 + 32, 0);
        named(s, statue, "Venus");
        assert!(hover(s, 21, 20, 32).1.is_some());
        assert_eq!(hover(s, 25, 25, 32), (vec![], None), "off the entity");
        s.pass(&[super::bulldozer_tests::DELETE]);
        assert_eq!(hover(s, 21, 20, 32).0.len(), 1, "the bulldozer's own tint");
        assert!(s.app.game.as_ref().unwrap().placing.hovered.is_none());
        assert_eq!(s.app.game.as_ref().unwrap().dozer.at.len(), 1);
    });
}
