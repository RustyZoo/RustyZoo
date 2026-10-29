//! The loops of the tank filters through the pass, on the map of the bulldozer tests.

use super::bulldozer_tests::{Session, with_game};
use crate::zoo::Filter;

/// Gives the filter `id` this health.
fn health(s: &mut Session, id: u32, health: u32) {
    let world = &mut s.app.game.as_mut().unwrap().world;
    let at = world.find(id).unwrap();
    let c = world.zoo.entities[at].common.as_mut().unwrap();
    c.filter = Some(Filter {
        health,
        timers: None,
    });
}

/// What the last pass asked the shell to loop: filter, file, base term.
fn looped(s: &Session) -> Vec<(u32, String, i32)> {
    (s.app.loop_sounds().iter())
        .map(|l| (l.id, l.file.clone(), l.base))
        .collect()
}

#[test]
fn the_loops_of_the_filters_are_placed_in_the_view_and_follow_their_health_and_the_removal() {
    with_game("filter-loops", "", |s| {
        // 256 view pixels to the right and to the left of the middle of the view.
        let (right, left) = (s.add(18, 1536, 1536, 0), s.add(18, 1024, 1024, 0));
        s.add(0, 1280, 1280, 0);
        health(s, right, 10);
        health(s, left, 10);
        s.pass(&[]);
        let want = |id, file: &str, base| (id, file.to_string(), base);
        assert_eq!(
            looped(s),
            [want(right, "h.wav", 1700), want(left, "h.wav", 1700)]
        );
        let sounds = s.app.loop_sounds();
        assert_eq!(sounds[0].voice, format!("filter {right}"));
        assert_eq!(sounds[0].distance, sounds[1].distance);
        assert!(sounds[0].distance > 0);
        assert!(sounds[0].pan * sounds[1].pan < 0, "one on each side");
        // At or below the decayed health, the decayed loop; a broken filter is silent.
        health(s, left, 5);
        health(s, right, 0);
        s.pass(&[]);
        assert_eq!(looped(s), [want(left, "d.wav", 1800)]);
        health(s, right, 1);
        s.pass(&[]);
        assert_eq!(
            looped(s),
            [want(right, "d.wav", 1800), want(left, "d.wav", 1800)]
        );
        // Only a filter in the zoo plays: a removed one stops. (This map has no water, so by now
        // the other has been switched off by a step and is silent.)
        assert!(s.app.game.as_mut().unwrap().world.remove(right));
        s.pass(&[]);
        assert!(looped(s).iter().all(|l| l.0 != right));
    });
}
