//! What an animal's check reads of its exhibit when another animal changed just before it: the
//! counters of the last rating pass (animals 5.2, 9 point 5).

use super::*;
use crate::animal::food::Meal;

/// Runs the animal `a` through the checks of one tick, `fire` being the counter due; the exhibit
/// `x` is read for it as the game does. Returns what the checks read.
fn tick(
    world: &mut World,
    cat: &Catalogue,
    (x, at): (u32, usize),
    a: &mut Animal,
    fire: usize,
) -> Env {
    a.counters[fire] = 0;
    let mut env = in_exhibit();
    let read = &mut |r: Read, a: &Animal, meal: &Meal, env: &mut Env, rng: &mut Rng| {
        read_exhibit(world, cat, (r, x, at), (a, meal, "zebra"), env, rng);
    };
    let (k, class, mut rng) = (keys(), Class::default(), Rng::default());
    crate::animal::run_read(a, &k, &class, &mut env, &mut rng, &mut |_, _| false, read);
    env
}

/// The animal 501 of a rated pen, replaced by `a`.
fn put(world: &mut World, a: Animal) {
    world.units.get_mut(501).unwrap().part = Part::Animal(a);
}

#[test]
fn an_animal_that_fell_ill_is_unseen_by_the_next_health_check_until_the_rating_pass() {
    let (mut world, cat, x, at) = rated(|_| ());
    // 501 falls ill in its own health check; the exhibit is not marked.
    let mut sick = Animal {
        health: 0,
        ..animal()
    };
    tick(&mut world, &cat, (x, at), &mut sick, HEALTH);
    assert!(sick.at_minimum_health);
    put(&mut world, sick);
    // 500's health check right after counts none; the rating pass that follows counts one.
    let mut well = Animal {
        health: 500,
        ..animal()
    };
    assert_eq!(
        tick(&mut world, &cat, (x, at), &mut well, HEALTH).sick_near,
        0
    );
    world.exhibits_mut().get_mut(x).unwrap().rating.dirty = true;
    assert_eq!(
        tick(&mut world, &cat, (x, at), &mut well, HEALTH).sick_near,
        1
    );
}

#[test]
fn an_animal_that_became_very_unhappy_is_unseen_by_the_next_other_check_until_the_rating_pass() {
    let (mut world, cat, x, at) = rated(|_| ());
    let mut upset = Animal {
        happiness_change: -200,
        ..animal()
    };
    upset.update(&keys());
    assert!(upset.very_unhappy);
    put(&mut world, upset);
    let mut calm = animal();
    assert_eq!(
        tick(&mut world, &cat, (x, at), &mut calm, OTHER)
            .census
            .unhappy,
        0
    );
    world.exhibits_mut().get_mut(x).unwrap().rating.dirty = true;
    assert_eq!(
        tick(&mut world, &cat, (x, at), &mut calm, OTHER)
            .census
            .unhappy,
        1
    );
}

#[test]
fn a_bite_of_another_animal_is_seen_at_once_by_the_next_hunger_check() {
    use crate::sim::drive::{Command, Run};
    let mut run = Run::new(
        (b"", b""),
        (feeding(), Class::default()),
        pens(),
        Default::default(),
        1,
    );
    assert!(run.apply(&Command::Food(3, 2, "herbchow".into(), Some(45))));
    let (x, at) = (
        run.world.exhibits().list()[0].id,
        run.world.find(500).unwrap(),
    );
    let pile = (run.world.zoo.entities.iter())
        .find(|e| e.class == "food")
        .and_then(|e| e.common.as_ref())
        .unwrap()
        .id;
    let food = |run: &mut Run, a: &Animal, meal: &Meal| {
        let mut env = in_exhibit();
        let mut rng = Rng::default();
        let args = (a, meal, "zebra");
        read_exhibit(
            &mut run.world,
            &run.cat,
            (Read::Food, x, at),
            args,
            &mut env,
            &mut rng,
        );
        env.food_total
    };
    // 501 aims at the pile and has taken 5 units this tick: the pile is marked where it bites.
    let eater = Animal {
        food: pile,
        ..animal()
    };
    let bite = Meal {
        left: 40,
        bites: 5,
        ..Meal::default()
    };
    assert_eq!(food(&mut run, &animal(), &Meal::default()), 45);
    assert_eq!(food(&mut run, &eater, &bite), 40);
    assert_eq!(food(&mut run, &animal(), &Meal::default()), 40);
}
