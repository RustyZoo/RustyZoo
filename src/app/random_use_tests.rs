//! The random use time of a building placed by a click (guest buildings 10.1 point 2.5).

use super::adopt_tests::{click, point, with_app};
use super::*;

/// The geyser of the pen map (the last type) and the time it draws at the AI clock 0.
const GEYSER: usize = 5;

fn time(rng: &mut crate::sim::Rng) -> u32 {
    20 + rng.pick(40)
}

fn stamp(app: &App) -> Option<u32> {
    let w = &app.game.as_ref().unwrap().world;
    w.zoo
        .entities
        .iter()
        .rev()
        .find(|e| e.class == "building")
        .and_then(|e| e.common.as_ref().and_then(|c| c.stamp))
}

/// A building placed keeps the time drawn when its pending object was made (the type chosen);
/// the click draws the next pending object's time only afterwards, after the draws of the
/// rating reaction.
#[test]
fn a_placed_building_keeps_the_time_of_its_pending_object() {
    with_app("app-random-use-keeps", |app| {
        app.game.as_mut().unwrap().paused = true;
        let game = app.game.as_mut().unwrap();
        game.build.hold(GEYSER);
        game.build.chose(GEYSER);
        let mut g = app.sim.rng.clone();
        app.pass(1, &[]);
        let first = time(&mut g);
        assert_eq!(app.sim.rng, g, "the choice drew the time once");
        let camera = app.camera().unwrap().clone();
        let at = |n: i32| point(&camera, (20 - n) * 64 + 32, 20 * 64 + 32);
        click(app, 10, at(0));
        assert_eq!(stamp(app), Some(first), "the building keeps the time");
        // The click makes the next pending object: one draw, none for the placing itself.
        let next = time(&mut g);
        assert_eq!(app.sim.rng, g);
        assert_eq!(
            app.game.as_ref().unwrap().placing.used,
            Some((GEYSER, next))
        );
        let told = click(app, 20, at(1));
        assert_eq!(told.len(), 1, "{told:?}");
        assert_eq!(stamp(app), Some(next), "and the next one keeps its own");
        // A click that places nothing makes no pending object and draws nothing.
        let state = app.sim.rng.clone();
        click(app, 30, at(1));
        assert_eq!(app.sim.rng, state);
    });
}

/// The click draws the next pending object's time after the rating reaction (placing 6.1): with
/// the reachability test due at the reaction, that draw comes first and the time is the second.
#[test]
fn the_placing_draw_comes_after_the_rating_reaction() {
    with_app("app-random-use-order", |app| {
        app.game.as_mut().unwrap().paused = true;
        let camera = app.camera().unwrap().clone();
        let game = app.game.as_mut().unwrap();
        game.build.hold(GEYSER);
        game.build.chose(GEYSER);
        app.pass(1, &[]);
        let mut g = app.sim.rng.clone();
        // The test before the placing is the 29th, the one of the reaction the 30th: it draws.
        crate::exhibit::rating::set_tests(28);
        click(app, 10, point(&camera, 20 * 64 + 32, 20 * 64 + 32));
        g.rand();
        let next = time(&mut g);
        assert_eq!(app.sim.rng, g);
        assert_eq!(
            app.game.as_ref().unwrap().placing.used,
            Some((GEYSER, next))
        );
    });
}
