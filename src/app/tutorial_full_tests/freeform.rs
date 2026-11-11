//! Freeform games of the full game on the same rig (awards 6.8, scenarios 11.7): the shipped
//! extra files give their awards and donations from the state of a played zoo, once, and a saved
//! game carries them.

use super::*;

/// The shipped map the games are played on.
const MAP: &str = "freeform/svalley.scn";

/// The adoption panel's animals tab, which the donation file disables (scenarios 11.3).
const ADOPT: i32 = 2075;

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

/// One animal on the grounds and every animal's happiness at `happiness` (-100 to 100): the zoo
/// status reads it at the start of the next step, so the animal rating is `(h + 100) / 2`.
fn mood(app: &mut App, happiness: i32) {
    let game = app.game.as_mut().unwrap();
    let ids: Vec<u32> = (game.world.zoo.entities.iter())
        .filter(|e| e.class == "animals")
        .filter_map(|e| e.common.as_ref().map(|c| c.id))
        .collect();
    for id in ids {
        if let Some(crate::unit::Part::Animal(a)) =
            game.world.units.get_mut(id).map(|u| &mut u.part)
        {
            a.happiness = happiness;
        }
    }
}

/// A zoo with one gazelle on the grounds, past the first pass.
fn grazing<'a>(rig: &'a Rig, name: &str) -> App<'a> {
    let mut app = game(rig, name);
    let cat = app.renderer.catalogue();
    let game = app.game.as_mut().unwrap();
    let rng = &mut app.sim.rng;
    let added = crate::sim::drive::add_unit(
        &mut game.world,
        cat,
        rng,
        "animals",
        Some("gazelle"),
        (22, 22),
        None,
    );
    assert!(added, "no gazelle in this install");
    app
}

/// A pass that answers a popup that holds the game.
fn answering(play: &mut Play) {
    let enter = [Command::Key(Key::Enter, false, false)];
    let modal = play.app.ui.as_ref().unwrap().modal_open();
    play.tick(STEP, if modal { &enter } else { &[] });
}

/// A pass at the end of the month: the calendar jumps, the month changes, the manager looks.
fn month(play: &mut Play) {
    play.tick(STEP, &[Command::Month]);
    for _ in 0..4 {
        answering(play);
    }
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn twenty_four_months_of_a_freeform_zoo_give_the_animal_care_ribbon_at_month_six_and_never_award_17()
 {
    let rig = rig("freeform-months");
    if !rig.measured("freeform months") {
        return;
    }
    let mut app = grazing(&rig, "played");
    let mut play = Play::new(&mut app, Vec::new());
    let total = |play: &Play| {
        let money = &play.app.game.as_ref().unwrap().money;
        (money.month + 12 * money.year) as i32
    };
    let mut first = None;
    for months in 1..=24 {
        mood(play.app, 100);
        month(&mut play);
        // The animal rating of a happy animal passes 98; its award is the only one of a zoo
        // with no guest and no exhibit, and comes with total month 6, not before (6.2).
        let held = awards(&play);
        assert_eq!(total(&play), months + 1);
        assert!(!held.contains(&17), "{held:?}");
        assert_eq!(
            held.contains(&8),
            total(&play) >= 6,
            "{held:?} at {}",
            total(&play)
        );
        if held.contains(&8) && first.is_none() {
            first = Some(total(&play));
        }
    }
    assert_eq!(first, Some(6));
    assert_eq!(awards(&play), [8]);
    assert_eq!(said(&play, "sim scenario cash 0 5000").len(), 1);
}

#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn the_adoption_element_is_disabled_below_an_animal_rating_of_25_and_enabled_again_from_45() {
    let rig = rig("freeform-adoption");
    if !rig.measured("freeform adoption") {
        return;
    }
    let mut app = grazing(&rig, "played");
    let mut play = Play::new(&mut app, Vec::new());
    // Settles at a happiness: the rating check is due every few passes.
    let settle = |play: &mut Play, happiness: i32| {
        for _ in 0..8 {
            mood(play.app, happiness);
            answering(play);
        }
        play.disabled(ADOPT)
    };
    // The rating is (happiness + 100) / 2: 100 gives 100 (enabled from the start), -52 gives 24,
    // -50 gives 25, -10 gives 45, -12 gives 44.
    assert!(!settle(&mut play, 100));
    assert!(said(&play, "sim scenario popup 45200").is_empty());
    assert!(!settle(&mut play, -50), "25 is not below 25");
    assert!(settle(&mut play, -52), "24 is");
    let popups = said(&play, "sim scenario popup").len();
    assert_eq!(popups, 1);
    assert!(settle(&mut play, -50), "25 does not enable it");
    assert!(settle(&mut play, -12), "44 does not enable it");
    assert!(!settle(&mut play, -10), "45 does");
    assert_eq!(said(&play, "sim scenario popup").len(), 2);
}
