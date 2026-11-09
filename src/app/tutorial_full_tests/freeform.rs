//! Freeform games of the full game on the same rig (awards 6.8, scenarios 11.7): the shipped
//! extra files give their awards and donations from the state of a played zoo, once, and a saved
//! game carries them.

use super::*;

/// The shipped map the games are played on.
const MAP: &str = "freeform/svalley.scn";

/// A freeform game just started.
fn game<'a>(rig: &'a Rig, name: &str) -> App<'a> {
    let mut app = rig.app(name);
    start(&mut app, MAP);
    app
}

/// `n` more guests at the entrance of the zoo, as a crowd that came in.
fn crowd(app: &mut App, n: usize) {
    let cat = app.renderer.catalogue();
    let game = app.game.as_mut().unwrap();
    let rng = &mut app.sim.rng;
    for k in 0..n {
        let at = (20 + (k % 5) as i32, 20 + (k / 5 % 5) as i32);
        assert!(crate::sim::drive::add_unit(
            &mut game.world,
            cat,
            rng,
            "guests",
            None,
            at,
            None
        ));
    }
}

/// The lines of the log that start with `what`, with the pass they came in.
fn said<'a>(play: &'a Play, what: &str) -> Vec<(u64, &'a str)> {
    let at = |l: &&(u64, String)| l.1.starts_with(what);
    (play.log.iter().filter(at))
        .map(|l| (l.0, l.1.as_str()))
        .collect()
}

fn awards(play: &Play) -> Vec<i32> {
    play.app.game.as_ref().unwrap().world.zoo.awards.clone()
}

fn cash(play: &Play) -> f32 {
    play.app.game.as_ref().unwrap().money.cash
}

/// Plays passes until the log has a line that starts with `what`, then one more.
fn until_said(play: &mut Play, what: &str) {
    assert!(
        play.until(10, |p| !said(p, what).is_empty()),
        "no {what}: {:?}",
        play.log.last()
    );
    play.tick(STEP, &[]);
}

/// A game saved and loaded again in a program of its own, with the passes that follow.
fn reloaded<'a>(rig: &'a Rig, play: &mut Play, name: &str) -> App<'a> {
    let file = rig.user.0.join(format!("{name}.zoo"));
    play.app.save_game(file.to_str().unwrap());
    let mut app = rig.app(name);
    assert!(app.load_saved(file.to_str().unwrap()));
    app
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn a_freeform_game_gives_the_awards_of_its_guests_once_and_a_saved_game_gives_none_again() {
    let rig = rig("freeform-guests");
    if !rig.measured("freeform guests") {
        return;
    }
    let mut app = game(&rig, "played");
    let mut play = Play::new(&mut app, Vec::new());
    play.tick(STEP, &[]);
    // The start lists the goals of the map and its extra files; the first pass makes the
    // default items available, so that the adoption panel has animals (11.7 points 1 and 2).
    let start = said(&play, "sim scenario start");
    let count = start[0]
        .1
        .rsplit(' ')
        .next()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(count >= 100, "{start:?}");
    assert!(!said(&play, "sim scenario trigger DEFAULT_ITEMS").is_empty());
    play.tick(100, &[Command::Press(1000)]);
    play.tick(100, &[]);
    assert!(!play.app.game.as_ref().unwrap().build.listed().is_empty());
    // 500 guests in the zoo: the award goal, then its popup goal, in one pass; the award is
    // held, one popup that does not stop the game, no cash.
    crowd(play.app, 520);
    until_said(&mut play, "sim scenario award");
    let (goal, popup) = (
        said(&play, "sim scenario trigger 500th_guest "),
        said(&play, "sim scenario trigger 500th_guest_popup"),
    );
    assert!(
        goal[0].0 == popup[0].0 && goal.len() == 1,
        "{goal:?} {popup:?}"
    );
    assert_eq!(awards(&play), [3]);
    assert_eq!(said(&play, "sim scenario popup 45200 open").len(), 1);
    assert!(said(&play, "sim scenario cash").is_empty());
    assert!(!play.app.ui.as_ref().unwrap().modal_open());
    // 1000 guests: award 4 and one donation of 25000, one more popup.
    let before = cash(&play);
    crowd(play.app, 500);
    until_said(&mut play, "sim scenario cash");
    assert_eq!(awards(&play), [4, 3]);
    assert_eq!(
        said(&play, "sim scenario cash"),
        [(play.pass - 1, "sim scenario cash 0 25000")]
    );
    assert_eq!(said(&play, "sim scenario popup 45200 open").len(), 2);
    assert!(cash(&play) >= before + 25000.0);
    // Saved and loaded: the same awards, and no popup, cash, or award again (6.6 point 5 and 6).
    let after = cash(&play);
    let mut again = reloaded(&rig, &mut play, "loaded");
    let mut play = Play::new(&mut again, Vec::new());
    for _ in 0..4 {
        play.tick(STEP, &[]);
    }
    assert_eq!(awards(&play), [4, 3]);
    for what in [
        "sim scenario popup",
        "sim scenario cash",
        "sim scenario award",
    ] {
        assert!(said(&play, what).is_empty(), "{what}");
    }
    assert!(cash(&play) <= after);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn fifteen_awards_make_the_chain_of_the_best_zoo_give_cash_and_a_crated_panda_once() {
    let rig = rig("freeform-chain");
    if !rig.measured("freeform chain") {
        return;
    }
    let mut app = game(&rig, "played");
    // A zoo that holds 15 awards.
    app.game.as_mut().unwrap().world.zoo.awards = (1..=15).rev().collect();
    let mut play = Play::new(&mut app, Vec::new());
    play.tick(STEP, &[]);
    // In the first pass the chain of award 25 gives it, 100000, its popup and a young giant
    // panda in a crate; the panda is a young one, so the next pass donates 50000 with a popup
    // that stops the game until it is answered (11.7 point 3).
    let first = said(&play, "sim scenario award");
    assert_eq!(first, [(1, "sim scenario award 25")]);
    assert_eq!(awards(&play)[0], 25);
    assert_eq!(said(&play, "sim scenario cash 0 100000").len(), 1);
    assert_eq!(said(&play, "sim scenario boxed").len(), 1);
    until_said(&mut play, "sim scenario cash 0 50000");
    assert_eq!(said(&play, "sim scenario popup 45200 modal").len(), 1);
    assert!(play.app.ui.as_ref().unwrap().modal_open());
    play.step();
    play.tick(STEP, &[]);
    assert!(!play.app.ui.as_ref().unwrap().modal_open());
    // Saved and loaded: no second panda, no cash, no popup.
    let (held, money) = (awards(&play), cash(&play));
    let mut again = reloaded(&rig, &mut play, "loaded");
    let mut play = Play::new(&mut again, Vec::new());
    for _ in 0..4 {
        play.tick(STEP, &[]);
    }
    assert_eq!((awards(&play), cash(&play)), (held, money));
    for what in [
        "sim scenario boxed",
        "sim scenario cash",
        "sim scenario popup",
    ] {
        assert!(said(&play, what).is_empty(), "{what}");
    }
}
