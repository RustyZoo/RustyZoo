//! Fright and the avoid task on a 20 by 20 map: a path along y = 10 with the guest on it at
//! (10, 10), a keeper at (10, 12), a pen of fences round (2, 2) to (4, 4) and one animal.

use super::*;
use crate::animal::Animal;
use crate::config::Config;
use crate::entity::EntityType;
use crate::guest::Guest;
use crate::unit::Staff;
use std::sync::Arc;

const GUEST: u32 = 1;
const KEEPER: u32 = 2;
const BEAST: u32 = 50;

fn ty(class: &'static str, name: &str, sub: &str, id: i32, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// A lion eats men, a trex keepers; the man's keys take `man` in addition.
fn cat(man: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    let guest = format!("[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\n{man}\n");
    cat.push(ty("guests", "man", "", 9503, &guest));
    let lion = "[m/Characteristics/Integers]\ncPrey=9503\n";
    cat.push(ty("animals", "lion", "m", 5006, lion));
    let trex = "[m/Characteristics/Integers]\ncPrey=9500\n";
    cat.push(ty("animals", "trex", "m", 7001, trex));
    cat.push(ty(
        "keeper",
        "keeper",
        "m",
        9500,
        "[m/Characteristics/Integers]\n",
    ));
    cat
}

/// The map with the animal of type `kind` at `beast`, loose when `escaped`, and the guest at
/// `guest`; every countdown far from firing.
fn zoo(kind: &str, beast: (i32, i32), escaped: bool, guest: (i32, i32)) -> World {
    let mut w = world((20, 20), |_, e| {
        for i in 0..3 {
            for (tile, d) in [
                ((2 + i, 2), 0),
                ((2 + i, 4), 4),
                ((2, 2 + i), 6),
                ((4, 2 + i), 2),
            ] {
                let id = 100 + e.len() as u32;
                e.push(entity("fences", "", id, centre(tile), d));
            }
        }
        for x in 0..20 {
            e.push(entity("paths", "stone", 300 + x as u32, centre((x, 10)), 0));
        }
        e.push(entity("guests", "man", GUEST, centre(guest), 0));
        let mut keeper = entity("keeper", "keeper", KEEPER, centre((10, 12)), 0);
        keeper.subtype = "m".into();
        e.push(keeper);
        let mut a = entity("animals", kind, BEAST, centre(beast), 0);
        a.subtype = "m".into();
        e.push(a);
    });
    w.units.get_mut(BEAST).unwrap().part = Part::Animal(Animal {
        escaped,
        ..Animal::default()
    });
    if escaped {
        w.units.escapes.push(BEAST);
    }
    w.units.get_mut(GUEST).unwrap().part = Part::Guest(Guest {
        countdowns: [1000; 11],
        saw_animal: 1000,
        ..Guest::default()
    });
    w.units.get_mut(KEEPER).unwrap().part = Part::Staff(Staff {
        chase: 1000,
        ..Staff::default()
    });
    w
}

fn sets() -> Sets {
    Sets::new(&[
        ("bWalk", &["fWalk(0,0)"]),
        ("bRunFromPredator", &["fWalk(0,0)"]),
    ])
}

/// Runs `f` on the goal context of unit `id` with the types of `cat`; `removed` asked to go.
fn with<R>(
    w: &mut World,
    cat: &Catalogue,
    id: u32,
    removed: &[u32],
    f: impl FnOnce(&mut Cx) -> R,
) -> R {
    let mut u = w.units.get(id).unwrap().clone();
    let at = w.find(id).unwrap();
    let (mut rng, events, mut removals) = (Rng::default(), Events::default(), removed.to_vec());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    let data = sets();
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(id).unwrap() = u;
    r
}

/// The status checks of unit `id`.
fn checks(w: &mut World, cat: &Catalogue, id: u32) {
    with(w, cat, id, &[], |cx| {
        let (unit, world, at) = (&mut *cx.unit, &mut *cx.world, cx.at);
        crate::guest::checks(unit, world, cx.ctx, at, cx.data);
        crate::staff::checks(unit, world, cx.ctx, at, cx.data);
    });
}

fn guest(w: &mut World) -> &mut Guest {
    match &mut w.units.get_mut(GUEST).unwrap().part {
        Part::Guest(g) => g,
        _ => unreachable!(),
    }
}

/// The guest's chase countdown fires at its next status tick.
fn chase_due(w: &mut World) {
    guest(w).countdowns[6] = 0;
}

#[test]
fn a_guest_that_sees_a_loose_man_eater_is_afraid_and_thinks_of_it() {
    let cat = cat("");
    let mut w = zoo("lion", (13, 10), true, (10, 10));
    chase_due(&mut w);
    guest(&mut w).leave = true;
    checks(&mut w, &cat, GUEST);
    let u = w.units.get(GUEST).unwrap();
    assert!(u.afraid && u.frightened);
    // The leave flag goes, happiness falls by the built-in -20 and the saw-animal countdown restarts.
    let g = guest(&mut w).clone();
    assert_eq!((g.leave, g.pending[0], g.saw_animal), (false, -20, 19));
    let t = w.units.thoughts.of(GUEST)[0];
    assert_eq!((t.text, t.object), (10245, Some(BEAST)));
    // None in sight: the frightened flag goes down, the afraid one stays.
    let mut w2 = zoo("lion", (16, 10), true, (10, 10));
    w2.units.get_mut(GUEST).unwrap().frightened = true;
    chase_due(&mut w2);
    checks(&mut w2, &cat, GUEST);
    assert!(!w2.units.get(GUEST).unwrap().frightened);
    // An avoiding guest does not look.
    let mut w3 = zoo("lion", (13, 10), true, (10, 10));
    w3.units.get_mut(GUEST).unwrap().goal.record.task = AVOID;
    chase_due(&mut w3);
    checks(&mut w3, &cat, GUEST);
    assert!(!w3.units.get(GUEST).unwrap().frightened);
}

#[test]
fn in_an_exhibit_the_thought_is_about_no_animal_and_the_change_is_the_types() {
    for (key, change) in [
        ("", -20),
        ("cEscapedAnimalChange=0", 0),
        ("cEscapedAnimalChange=-5000", -5000),
        ("cEscapedAnimalChange=2147483647", i32::MAX),
    ] {
        let mut w = zoo("lion", (4, 4), false, (2, 2));
        chase_due(&mut w);
        guest(&mut w).pending[0] = 5;
        checks(&mut w, &cat(key), GUEST);
        assert_eq!(guest(&mut w).pending[0], change.saturating_add(5), "{key}");
        let t = w.units.thoughts.of(GUEST)[0];
        assert_eq!((t.text, t.object), (10248, None));
    }
}

#[test]
fn an_afraid_guest_calms_down_when_nothing_is_loose_and_walks_on() {
    let cat = cat("");
    // On the grounds any loose animal keeps it afraid, a harmless one too.
    let mut w = zoo("trex", (19, 19), true, (10, 10));
    w.units.get_mut(GUEST).unwrap().afraid = true;
    guest(&mut w).saw_animal = 0;
    checks(&mut w, &cat, GUEST);
    assert!(w.units.get(GUEST).unwrap().afraid);
    assert_eq!(guest(&mut w).saw_animal, 19);
    let mut w = zoo("lion", (13, 10), false, (10, 10));
    let u = w.units.get_mut(GUEST).unwrap();
    (u.afraid, u.moving, u.destination) = (true, true, Some((200, 672, 0)));
    guest(&mut w).saw_animal = 0;
    checks(&mut w, &cat, GUEST);
    let u = w.units.get(GUEST).unwrap();
    assert!(!u.afraid);
    assert_eq!(u.runner.top(), Some("bWalk"));
}

#[test]
fn a_frightened_guest_avoids_its_predator_and_runs_away_from_it() {
    let cat = cat("");
    let mut w = zoo("lion", (13, 10), true, (10, 10));
    w.units.get_mut(GUEST).unwrap().frightened = true;
    guest(&mut w).leave = true;
    with(&mut w, &cat, GUEST, &[], |cx| {
        assert!(cx.fright_ladder(NONE));
        let r = cx.record().clone();
        assert_eq!((r.task, r.mode, r.target.id), (AVOID, 3, BEAST));
        assert_eq!(cx.unit.predator, BEAST);
        assert!(cx.unit.mover.emergency);
        // Away from the lion, west: the path tile on the ring of 7 at (3, 10).
        cx.avoid_decide();
        assert_eq!(cx.record().target.tile, Some(10 * 20 + 3));
        assert_eq!(cx.unit.runner.top(), Some("bRunFromPredator"));
        // Still in sight: the avoid goes on.
        cx.avoid_check_done();
        assert_eq!(cx.record().task, AVOID);
    });
    assert!(!guest(&mut w).leave);
    // The lion gone, the avoid ends and the guest leaves the zoo.
    with(&mut w, &cat, GUEST, &[BEAST], |cx| {
        cx.avoid_check_done();
        assert_eq!(cx.record().task, NONE);
        assert!(!cx.unit.frightened && cx.unit.predator == 0 && !cx.unit.mover.emergency);
    });
    assert!(guest(&mut w).leave);
    // With nothing in sight the flag goes down and nothing else happens.
    let mut w = zoo("lion", (13, 10), false, (10, 10));
    w.units.get_mut(GUEST).unwrap().frightened = true;
    with(&mut w, &cat, GUEST, &[], |cx| {
        assert!(cx.fright_ladder(NONE));
        assert!(!cx.unit.frightened && cx.record().task == NONE);
    });
}

#[test]
fn a_guest_that_fled_into_an_exhibit_leaves_it_at_the_end_of_a_leg() {
    let cat = cat("");
    let mut w = zoo("lion", (4, 4), false, (3, 3));
    with(&mut w, &cat, GUEST, &[], |cx| {
        cx.set_task(AVOID, 3, BEAST, Some(3 * 20 + 3));
        cx.unit.predator = BEAST;
        cx.avoid_check_done();
        assert_eq!(cx.record().task, EXIT_TANK);
        assert_eq!(cx.world.units.thoughts.of(GUEST)[0].text, 10568);
    });
}

#[test]
fn the_flee_path_with_a_radius_of_zero_negative_or_huge() {
    for radius in [Some(0), Some(-7), Some(i32::MAX), None] {
        let mut cat = cat("");
        cat.predator_radius = radius;
        let mut w = zoo("lion", (13, 10), true, (10, 10));
        with(&mut w, &cat, GUEST, &[], |cx| {
            cx.set_task(AVOID, 3, BEAST, None);
            cx.avoid_decide();
            // Below a ring of 1 the guest walks at random; the walk always finds a place here.
            assert!(cx.record().target.tile.is_some(), "{radius:?}");
        });
    }
}

#[test]
fn a_keeper_sees_a_beast_that_preys_on_keepers_and_avoids_it() {
    let cat = cat("");
    let mut w = zoo("trex", (12, 12), true, (10, 10));
    let due = |w: &mut World| {
        if let Part::Staff(s) = &mut w.units.get_mut(KEEPER).unwrap().part {
            s.chase = 0;
        }
    };
    due(&mut w);
    checks(&mut w, &cat, KEEPER);
    assert!(w.units.get(KEEPER).unwrap().frightened);
    // The counter restarts at `cChaseCheck` - 1 (2 when the type has none).
    assert!(matches!(&w.units.get(KEEPER).unwrap().part, Part::Staff(s) if s.chase == 1));
    with(&mut w, &cat, KEEPER, &[], |cx| {
        cx.keeper_ladder(DUTIES, false);
        let r = cx.record().clone();
        assert_eq!(
            (r.task, r.mode, r.target.id, cx.unit.predator),
            (AVOID, 3, BEAST, BEAST)
        );
        assert!(!cx.unit.frightened);
    });
    // A lion does not frighten him.
    let mut w = zoo("lion", (12, 12), true, (10, 10));
    due(&mut w);
    checks(&mut w, &cat, KEEPER);
    assert!(!w.units.get(KEEPER).unwrap().frightened);
}

#[test]
fn the_staff_chase_counter_restarts_at_the_types_period() {
    for (key, next) in [
        ("", 1),
        ("cChaseCheck=0", -1),
        ("cChaseCheck=-5", -6),
        ("cChaseCheck=99999999999", i32::MAX - 1),
    ] {
        let mut cat = Catalogue::default();
        let keeper = format!("[m/Characteristics/Integers]\n{key}\n");
        cat.push(ty("keeper", "keeper", "m", 9500, &keeper));
        let mut w = zoo("lion", (12, 12), true, (10, 10));
        if let Part::Staff(s) = &mut w.units.get_mut(KEEPER).unwrap().part {
            s.chase = 0;
        }
        checks(&mut w, &cat, KEEPER);
        let chase = match &w.units.get(KEEPER).unwrap().part {
            Part::Staff(s) => s.chase,
            _ => unreachable!(),
        };
        assert_eq!(chase, next, "{key}");
    }
}

#[test]
fn a_frightened_guest_does_not_take_the_leave_task_and_an_afraid_one_runs_out() {
    let cat = cat("");
    let mut w = zoo("lion", (19, 19), false, (10, 10));
    let u = w.units.get_mut(GUEST).unwrap();
    (u.afraid, u.frightened) = (true, true);
    guest(&mut w).leave = true;
    with(&mut w, &cat, GUEST, &[], |cx| {
        assert!(!cx.leave_ladder());
        cx.unit.frightened = false;
        assert!(cx.leave_ladder());
        assert_eq!(cx.record().task, LEAVE);
        assert!(cx.leave_init());
        cx.unit.frightened = true;
        cx.leave_decide();
        assert!(!cx.unit.frightened && cx.unit.mover.emergency);
        assert_eq!(cx.unit.runner.top(), Some("bRunFromPredator"));
    });
    assert_eq!(w.units.thoughts.of(GUEST)[0].text, 10247);
}
