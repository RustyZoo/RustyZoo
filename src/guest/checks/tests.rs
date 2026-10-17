//! The status update and the checks on the guest's own state.

use super::*;

/// A guest whose countdowns are all far from firing, with the program's keys for a bare type.
fn guest() -> (Guest, Keys) {
    let ty = EntityType {
        class: "guests",
        type_name: b"man".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id: 9503,
        footprint: [1, 1, 1],
        height: 0,
    };
    let g = Guest {
        countdowns: [100; 11],
        saw_animal: 100,
        ..Guest::default()
    };
    (g, Keys::of(&ty))
}

fn drew(before: &Rng, after: &Rng) -> usize {
    let mut r = before.clone();
    let mut n = 0;
    while r != *after && n < 20 {
        r.rand();
        n += 1;
    }
    n
}

#[test]
fn the_update_adds_the_pending_changes_and_keeps_the_values_between_their_limits() {
    let (mut g, _) = guest();
    g.values = [90, 5, 100, 90, 50];
    g.pending = [30, -20, 5, 40, i32::MAX];
    update(&mut g, 100);
    assert_eq!(g.values, [100, 0, 100, 100, 100]);
    assert_eq!(g.pending, [0; 5]);
    // Energy has its own top, and a huge change saturates.
    g.pending = [i32::MIN, 0, 0, 60, 0];
    update(&mut g, 120);
    assert_eq!((g.values[HAPPINESS], g.values[ENERGY]), (-100, 120));
    // A top below the bottom wins for the bottom.
    g.pending = [0; 5];
    update(&mut g, -5);
    assert_eq!(g.values[ENERGY], 0);
}

#[test]
fn a_need_grows_when_its_countdown_runs_out_and_costs_happiness_at_the_top() {
    let (mut g, k) = guest();
    let mut rng = Rng::default();
    // The hunger countdown 0: increment 5 (the program's value), restart at period - 1.
    g.countdowns[0] = 0;
    run(&mut g, &k, false, false, &mut rng);
    assert_eq!((g.pending[HUNGER], g.countdowns[0]), (5, 4));
    assert_eq!(g.pending[HAPPINESS], 0);
    // At the top the angry change is added, to the pending one.
    g.values[THIRST] = 100;
    g.countdowns[1] = 0;
    run(&mut g, &k, false, false, &mut rng);
    assert_eq!((g.pending[THIRST], g.pending[HAPPINESS]), (10, -20));
    // The countdown goes down by one when it does not fire; a period of 0 never fires.
    run(&mut g, &k, false, false, &mut rng);
    assert_eq!(g.countdowns[2], 97, "three checks have run");
    g.countdowns[2] = -1;
    run(&mut g, &k, false, false, &mut rng);
    assert_eq!((g.countdowns[2], g.pending[BATHROOM]), (-2, 0));
    assert_eq!(rng, Rng::default(), "no draw");
}

#[test]
fn the_trash_check_costs_every_guest_and_the_gift_check_draws_by_happiness() {
    let (mut g, k) = guest();
    g.countdowns[7] = 0;
    run(&mut g, &k, false, false, &mut Rng::default());
    assert_eq!(g.pending[HAPPINESS], -10);
    for (happiness, draws) in [(100, 1), (71, 1), (70, 1), (31, 1), (30, 0), (-50, 0)] {
        let (before, mut rng) = (Rng::default(), Rng::default());
        g.values[HAPPINESS] = happiness;
        g.countdowns[4] = 0;
        run(&mut g, &k, false, false, &mut rng);
        assert_eq!(drew(&before, &rng), draws, "{happiness}");
    }
}

#[test]
fn an_unhappy_guest_draws_for_its_band_and_leaves_by_chance() {
    let (mut g, k) = guest();
    for (happiness, chance) in [
        (-20, 10),
        (-40, 10),
        (-41, 15),
        (-80, 15),
        (-81, 20),
        (-100, 20),
    ] {
        let (mut rng, mut check) = (Rng::default(), Rng::default());
        (g.values[HAPPINESS], g.countdowns[3], g.leave) = (happiness, 0, false);
        run(&mut g, &k, false, false, &mut rng);
        assert_eq!(g.leave, check.chance(chance), "{happiness}");
        assert_eq!(rng, check, "one draw");
        assert_eq!(g.countdowns[3], 19);
    }
    // Above -20 or below -100 there is no draw and the guest stays.
    for happiness in [-19, 0, 100] {
        let mut rng = Rng::default();
        (g.values[HAPPINESS], g.countdowns[3], g.leave) = (happiness, 0, false);
        run(&mut g, &k, false, false, &mut rng);
        assert!(!g.leave && rng == Rng::default(), "{happiness}");
    }
    // Some seed leaves and some stays at the lowest band.
    let results: Vec<bool> = (0..30)
        .map(|seed| {
            let mut rng = Rng::default();
            rng.seed(seed);
            (g.values[HAPPINESS], g.countdowns[3], g.leave) = (-100, 0, false);
            run(&mut g, &k, false, false, &mut rng);
            g.leave
        })
        .collect();
    assert!(results.contains(&true) && results.contains(&false));
}

#[test]
fn a_wandering_guest_skips_the_countdowns_and_a_guest_on_water_the_checks() {
    let (mut g, k) = guest();
    // Wandered: the need countdowns stay, nothing grows, the leave check draws and restarts.
    (g.values[HAPPINESS], g.countdowns[0]) = (-30, 3);
    let (before, mut rng) = (Rng::default(), Rng::default());
    let out = run(&mut g, &k, true, false, &mut rng);
    assert_eq!(
        (g.countdowns[0], g.pending[HUNGER], g.countdowns[3]),
        (3, 0, 19)
    );
    assert_eq!(g.countdowns[7], 100, "the trash countdown stays too");
    assert_eq!(drew(&before, &rng), 1);
    assert!(!out.seek[ENERGY_SEEK]);
    // On water, in a tank or leaving: no check at all, not even the countdowns; the chase, the
    // saw-animal and the viewing countdowns go on.
    let (mut g, k) = guest();
    (g.countdowns[0], g.countdowns[3], g.countdowns[9]) = (0, 0, 7);
    g.values[HAPPINESS] = -90;
    let mut rng = Rng::default();
    run(&mut g, &k, false, true, &mut rng);
    assert_eq!(
        (g.countdowns[0], g.countdowns[3], g.countdowns[9]),
        (0, 0, 6)
    );
    assert_eq!((g.countdowns[6], g.saw_animal), (99, 99));
    assert!(rng == Rng::default() && g.pending == [0; 5]);
}

#[test]
fn a_tired_guest_seeks_a_bench_and_the_viewing_countdown_is_reported() {
    let (mut g, k) = guest();
    g.values[ENERGY] = 89;
    g.countdowns[5] = 0;
    assert!(
        !run(&mut g, &k, false, false, &mut Rng::default()).seek[ENERGY_SEEK],
        "below the threshold"
    );
    g.values[ENERGY] = 90;
    g.countdowns[5] = 0;
    assert!(run(&mut g, &k, false, false, &mut Rng::default()).seek[ENERGY_SEEK]);
    // Not when the check does not fire, but always when the wandered flag is up.
    assert!(!run(&mut g, &k, false, false, &mut Rng::default()).seek[ENERGY_SEEK]);
    assert!(run(&mut g, &k, true, false, &mut Rng::default()).seek[ENERGY_SEEK]);
    // The saw-animal countdown restarts after 0, the viewing one is due after its period.
    g.saw_animal = 0;
    g.countdowns[9] = 0;
    let out = run(&mut g, &k, false, false, &mut Rng::default());
    assert!(out.view && g.countdowns[9] == k.periods[9] - 1);
    assert_eq!(g.saw_animal, k.saw_animal_reset - 1);
}
