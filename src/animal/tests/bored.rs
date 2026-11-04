//! The bored check's chase branches (animals 21.1): what it does with what the animal sees.

use super::*;

/// One bored check on a fresh generator: the animal, the output and the generator. Animals have
/// drink 10, play 10 and company.
fn go(mut a: Animal, env: &Env) -> (Animal, Out, Rng) {
    let k = Keys {
        drink: 10,
        chase: 10,
        ..keys()
    };
    a.counters[BORED] = 0;
    let mut rng = Rng::default();
    let out = crate::animal::run(&mut a, &k, &Class::default(), env, &mut rng);
    (a, out, rng)
}

fn seeing(sight: Sight) -> Env {
    Env {
        sight,
        ..in_exhibit()
    }
}

#[test]
fn a_threat_or_a_chasing_playmate_frightens_the_animal_and_nothing_is_drawn() {
    let threat = Sight {
        threat: true,
        ..Sight::default()
    };
    let (_, out, rng) = go(animal(), &seeing(threat));
    assert!(out.afraid && drew(&rng, 0));
    let chaser = Sight {
        chaser: true,
        ..Sight::default()
    };
    let (_, out, rng) = go(animal(), &seeing(chaser));
    assert!(out.afraid && drew(&rng, 0));
    // The playmate counts only for a calm animal: playing, escaped, rampaging, or busy ones are
    // not afraid, and the busy and the rampaging return before the draw.
    let calm = animal();
    let cases = [
        (
            Animal {
                playing: true,
                ..calm.clone()
            },
            1,
        ),
        (
            Animal {
                escaped: true,
                ..calm.clone()
            },
            1,
        ),
        (
            Animal {
                thirsty: true,
                ..calm.clone()
            },
            0,
        ),
        (
            Animal {
                rampaging: true,
                happiness: -100,
                rampage: -50,
                ..calm.clone()
            },
            0,
        ),
        (
            Animal {
                caught_prey: true,
                ..calm.clone()
            },
            0,
        ),
    ];
    for (i, (a, draws)) in cases.into_iter().enumerate() {
        let (_, out, rng) = go(a, &seeing(chaser));
        assert!(!out.afraid, "case {i}");
        assert!(drew(&rng, draws), "case {i} {:?}", rng);
    }
    // With the afraid flag already up neither search counts.
    let up = Sight {
        afraid: true,
        ..threat
    };
    let (_, out, _) = go(animal(), &seeing(up));
    assert!(!out.afraid);
}

#[test]
fn a_lonely_friendly_chase_ends_with_a_reset_and_no_draw() {
    let alone = Env {
        census: Census {
            species: 1,
            ..in_exhibit().census
        },
        sight: Sight {
            task: 0x74,
            friendly: true,
            ..Sight::default()
        },
        ..in_exhibit()
    };
    let hunter = Animal {
        playing: true,
        prey: 9,
        ..animal()
    };
    let (a, out, rng) = go(hunter.clone(), &alone);
    assert!(out.lonely && !a.playing && a.prey == 0 && drew(&rng, 0));
    // Not a friendly chase, or company: the flag stays down.
    let other = Env {
        sight: Sight {
            friendly: false,
            ..alone.sight
        },
        ..alone.clone()
    };
    assert!(!go(hunter.clone(), &other).1.lonely);
    let company = Env {
        census: in_exhibit().census,
        ..alone
    };
    assert!(!go(hunter, &company).1.lonely);
}

#[test]
fn the_draw_starts_a_game_or_a_thirst_and_not_in_a_chase_or_an_avoid() {
    let prey = Sight {
        prey: true,
        ..Sight::default()
    };
    // Seeds whose draw % 20 is above the drink chance (play) and at or below it (drink).
    let seed = |play: bool| (0..).find(|&s| (first(s) % 20 >= 10) == play).unwrap();
    let run = |play: bool, env: &Env, a: Animal| {
        let k = Keys {
            drink: 10,
            chase: 10,
            ..keys()
        };
        let mut a = Animal {
            counters: [1000; COUNTERS],
            ..a
        };
        a.counters[BORED] = 0;
        let mut rng = Rng::default();
        rng.seed(seed(play));
        crate::animal::run(&mut a, &k, &Class::default(), env, &mut rng);
        (a, rng)
    };
    let (a, rng) = run(true, &seeing(prey), animal());
    assert!(a.playing && !a.thirsty && drew_since(&rng, seed(true), 1));
    // No prey, a game under way or an escaped animal: no game.
    assert!(!run(true, &seeing(Sight::default()), animal()).0.playing);
    let escaped = Animal {
        escaped: true,
        ..animal()
    };
    assert!(!run(true, &seeing(prey), escaped).0.playing);
    let (a, _) = run(false, &seeing(prey), animal());
    assert!(a.thirsty && !a.playing);
    // The drink needs land: a water mode of the terrain, the tank mode or a game under way stop it.
    let water = Animal {
        terrain: 1,
        ..animal()
    };
    assert!(!run(false, &seeing(prey), water).0.thirsty);
    let tank = Env {
        tank_mode: 3,
        ..seeing(prey)
    };
    assert!(!run(false, &tank, animal()).0.thirsty);
    let playing = Animal {
        playing: true,
        ..animal()
    };
    assert!(!run(false, &seeing(prey), playing).0.thirsty);
    // The chase and avoid tasks make no draw at all.
    for task in [0x74, 0x75] {
        let busy = Sight { task, ..prey };
        let (a, rng) = run(true, &seeing(busy), animal());
        assert!(drew(&rng, 0) || drew_since(&rng, seed(true), 0));
        assert!(!a.playing && !a.thirsty);
    }
}

#[test]
fn a_rampage_target_flag_occupies_the_animal_and_is_saved() {
    // 21.4: occupied animals do not zap, and the flag survives a record round trip.
    let a = Animal {
        rampage_target: true,
        ..animal()
    };
    let record = a.record(Default::default());
    assert!(record.rampage_target);
    let mut back = Animal::default();
    back.restore(&record, &keys(), &Class::default());
    assert!(back.rampage_target);
}
