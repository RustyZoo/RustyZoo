use super::*;
use crate::unit::testing::{centre, entity, tank_world, world_with};

fn clock(count: u32, delay: u32) -> Clock {
    Clock::new(Some(Timing { count, delay }), None)
}

/// A loop drops the time left over at its end: 21 frames of 125 ms at 67 ms steps take
/// 2680 ms, not 2625.
#[test]
fn a_loop_drops_the_time_left_over_when_it_wraps() {
    let mut c = clock(21, 125);
    let mut passes = Vec::new();
    for _ in 0..2 {
        let mut steps = 0;
        loop {
            c.advance(67);
            steps += 1;
            if c.frame == 0 && steps > 1 {
                break;
            }
        }
        passes.push(steps * 67);
        assert_eq!(c.t, 0);
    }
    // The second pass starts from nothing again: same length.
    assert_eq!(passes, [2680, 2680]);
}

#[test]
fn a_still_picture_or_a_delay_of_zero_never_advances() {
    for (count, delay) in [(1, 125), (0, 125), (5, 0)] {
        let mut c = clock(count, delay);
        c.advance(10_000);
        assert_eq!(c.frame, 0, "{count} {delay}");
    }
    // A huge step still ends on a frame of the loop.
    let mut c = clock(3, 1);
    c.advance(u32::MAX);
    assert!(c.frame < 3);
}

/// The timing of `idle`, and of the animations `named`.
pub(crate) struct Fixed(pub Option<Timing>, pub Vec<(&'static str, Timing)>);

/// A type with `idle` of 4 frames of 100 ms, `used` of 2 frames of 100 ms and `idledead` of
/// 3 frames of 100 ms.
pub(crate) struct Building;

impl Data for Fixed {
    fn set(&self, _: &crate::zoo::Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
        None
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let named = self.1.iter().find(|n| n.0 == name).map(|n| n.1);
        named.or(self.0.filter(|_| name == "idle"))
    }
    fn number(&self, _: &crate::zoo::Entity, _: &crate::behaviour::Number) -> i32 {
        0
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
}

impl Data for Building {
    fn set(&self, _: &crate::zoo::Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
        None
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let count = match name {
            "idle" => 4,
            "used" => 2,
            "idledead" => 3,
            _ => return None,
        };
        Some(Timing { count, delay: 100 })
    }
    fn number(&self, _: &crate::zoo::Entity, _: &crate::behaviour::Number) -> i32 {
        0
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
}

/// An entity present when a step starts advances by that step; one made during the step
/// (a ripple) first advances in the next. Units and objects with no idle animation have no
/// clock; a removed entity's clock goes.
#[test]
fn an_entity_first_seen_at_a_step_advances_by_it() {
    let mut world = world_with((4, 4), |zoo| {
        zoo.entities
            .push(entity("objects", "tree", 1, centre((0, 0)), 0));
        zoo.entities
            .push(entity("animals", "lion", 2, centre((1, 1)), 0));
    });
    let data = Fixed(
        Some(Timing {
            count: 4,
            delay: 100,
        }),
        vec![],
    );
    let mut idles = Idles::default();
    let step = |idles: &mut Idles, world: &World, ms: u32| {
        idles.register(world, &Catalogue::default(), &data, &mut Rng::default());
        idles.advance(ms);
    };
    // The first step of a loaded map counts: 200 ms of 100 per frame.
    step(&mut idles, &world, 200);
    assert_eq!(idles.frames().into_iter().collect::<Vec<_>>(), [(1, 2)]);
    // A second tree made in the middle of a step does not advance in it.
    world
        .zoo
        .entities
        .push(entity("objects", "bush", 4, centre((2, 2)), 0));
    idles.advance(70);
    assert_eq!(idles.frames().get(&4), None);
    step(&mut idles, &world, 70);
    assert_eq!((idles.frames()[&1], idles.frames()[&4]), (3, 0));
    step(&mut idles, &world, 70);
    assert_eq!(idles.frames()[&4], 1);
    world.zoo.entities.remove(0);
    step(&mut idles, &world, 70);
    assert!(!idles.frames().contains_key(&1));
    // Types with no `idle` animation show frame 0 and are not listed.
    let mut none = Idles::default();
    none.register(
        &world,
        &Catalogue::default(),
        &Fixed(None, vec![]),
        &mut Rng::default(),
    );
    none.advance(70);
    assert!(none.frames().is_empty());
}

/// A world of `n` fences of ids 1 to `n` and, when `gate`, a gate of id 100.
fn fences(n: u32, gate: bool) -> World {
    world_with((4, 4), |zoo| {
        for id in 1..=n {
            zoo.entities.push(entity("fences", "f", id, (32, 0), 0));
        }
        if gate {
            zoo.entities.push(entity("fences", "g", 100, (32, 0), 0));
            zoo.entities.last_mut().unwrap().subtype = "gate".into();
        }
    })
}

/// A fence that is not a gate starts at a random time of 0 to 1999 ms, one draw each.
#[test]
fn a_fence_starts_at_a_random_time() {
    let world = fences(40, true);
    // 20 frames of 100 ms: a loop of 2000 ms shows every start.
    let data = Fixed(
        Some(Timing {
            count: 20,
            delay: 100,
        }),
        vec![],
    );
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    idles.register(&world, &Catalogue::default(), &data, &mut rng);
    let mut expect = Rng::default();
    let starts: Vec<u32> = (0..40).map(|_| expect.pick(2000)).collect();
    assert_eq!(rng.state(), expect.state(), "40 draws, none for the gate");
    for (id, t) in (1..=40).zip(&starts) {
        assert_eq!(idles.fixed[&id].t, *t);
        assert!(*t < 2000);
    }
    // Neighbouring fences are not in step.
    idles.advance(67);
    let frames = idles.frames();
    assert!(frames.values().any(|&f| f != frames[&1]));
    // A fence already known draws nothing more.
    idles.register(&world, &Catalogue::default(), &data, &mut rng);
    assert_eq!(rng.state(), expect.state());
}

/// A fence whose idle is a still picture draws its start time all the same: one draw a piece,
/// none for a gate, and with no animation at all.
#[test]
fn a_still_fence_draws_its_start_time_too() {
    let world = fences(3, true);
    let mut expect = Rng::default();
    (0..3).for_each(|_| _ = expect.pick(2000));
    for data in [
        Fixed(
            Some(Timing {
                count: 1,
                delay: 100,
            }),
            vec![],
        ),
        Fixed(None, vec![]),
    ] {
        let mut rng = Rng::default();
        Idles::default().register(&world, &Catalogue::default(), &data, &mut rng);
        assert_eq!(rng.state(), expect.state());
    }
}

/// A catalogue with the fence type `iron`/`f` whose `cDecayedLife` is 2.
fn iron() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(crate::entity::EntityType {
        class: "fences",
        type_name: b"iron".to_vec(),
        subtype: b"f".to_vec(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(b"[f/Characteristics/Integers]\ncDecayedLife=2\n")
                .unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

/// A world with one `iron` fence (id 1) of the given life, on flat ground.
fn worn(life: i32) -> World {
    world_with((4, 4), |zoo| {
        let mut piece = entity("fences", "iron", 1, (32, 0), 0);
        piece.subtype = "f".into();
        piece.common.as_mut().unwrap().fence.as_mut().unwrap().life = life;
        zoo.entities.push(piece);
    })
}

/// `idle` of 20 frames of 100 ms, `det` of 10 frames of 50 ms, `broke` of 4 frames of 25 ms.
fn rows() -> Fixed {
    let t = |count, delay| Timing { count, delay };
    Fixed(
        Some(t(20, 100)),
        vec![("det", t(10, 50)), ("broke", t(4, 25))],
    )
}

/// A worn fence plays the animation its life chooses, not `idle`'s timing; when the picture
/// changes the clock starts again at a new random time, one draw (sprite drawing 3.6).
#[test]
fn a_worn_fence_follows_its_own_animation() {
    let (cat, data) = (iron(), rows());
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    let mut expect = Rng::default();
    let start = |idles: &mut Idles, life: i32, rng: &mut Rng| {
        idles.register(&worn(life), &cat, &data, rng);
        idles.fixed[&1].t
    };
    for (life, count) in [(5, 20), (2, 10), (0, 4)] {
        let t = start(&mut idles, life, &mut rng);
        assert_eq!(t, expect.pick(2000), "life {life}");
        assert_eq!(idles.fixed[&1].timing.count, count, "life {life}");
        // The same picture again: no new draw.
        start(&mut idles, life, &mut rng);
        assert_eq!(rng.state(), expect.state());
    }
    // A gate keeps `idle` whatever its life.
    let mut gate = worn(0);
    gate.zoo.entities[0].subtype = "g".into();
    let mut none = Idles::default();
    none.register(&gate, &cat, &data, &mut rng);
    assert!(none.frames().is_empty());
}

/// A wear step rewinds the clock in place: one draw, three for a break, the last one is the
/// start time.
#[test]
fn the_wear_step_of_a_fence_draws_again() {
    let (cat, data) = (iron(), rows());
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    idles.register(&worn(5), &cat, &data, &mut rng);
    for (life, draws, count) in [(2, 1, 10), (0, 3, 4)] {
        let before = rng.clone();
        idles.rewind_fence(&worn(life), &cat, &data, &mut rng, (0, draws));
        let mut expect = before;
        let starts: Vec<u32> = (0..draws).map(|_| expect.pick(2000)).collect();
        assert_eq!(rng.state(), expect.state(), "life {life}");
        assert_eq!(idles.fixed[&1].t, starts[draws as usize - 1]);
        assert_eq!(idles.fixed[&1].timing.count, count);
        // The register of the next step finds the picture as it is: no draw.
        idles.register(&worn(life), &cat, &data, &mut rng);
        assert_eq!(rng.state(), expect.state());
    }
}

/// A mended piece starts at the time its worker drew; the next register draws nothing.
#[test]
fn a_mended_fence_starts_where_it_drew() {
    let (cat, data) = (iron(), rows());
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    idles.register(&worn(0), &cat, &data, &mut rng);
    let before = rng.state();
    idles.start_fence(&worn(5), &cat, &data, (1, 1234));
    assert_eq!(
        (idles.fixed[&1].t, idles.fixed[&1].timing.count),
        (1234, 20)
    );
    idles.register(&worn(5), &cat, &data, &mut rng);
    assert_eq!(rng.state(), before);
}

/// A start time drawn outside the loop (a heal, a break) is taken at the next register, the
/// picture changed or not, with no draw; one for a piece that is gone is dropped.
#[test]
fn a_start_drawn_outside_the_loop_is_taken_at_the_next_register() {
    let (cat, data) = (iron(), rows());
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    idles.register(&worn(5), &cat, &data, &mut rng);
    let before = rng.state();
    idles.start_next(1, 777);
    idles.start_next(99, 5);
    idles.register(&worn(5), &cat, &data, &mut rng);
    assert_eq!((idles.fixed[&1].t, rng.state()), (777, before));
    assert!(idles.starts.is_empty());
    // Taken once: the next register leaves the clock alone.
    idles.advance(50);
    idles.register(&worn(5), &cat, &data, &mut rng);
    let k = &idles.fixed[&1];
    assert_eq!((k.frame, k.t, rng.state()), (8, 27, before));
}

/// A gate does not loop by itself: with an animated idle it stays on frame 0 after any number
/// of steps (its player runs only while it opens or closes).
#[test]
fn a_gate_stays_on_frame_zero() {
    let world = fences(1, true);
    let data = Fixed(
        Some(Timing {
            count: 20,
            delay: 100,
        }),
        vec![],
    );
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    for _ in 0..50 {
        idles.register(&world, &Catalogue::default(), &data, &mut rng);
        idles.advance(67);
    }
    assert!(!idles.frames().contains_key(&100));
    assert!(idles.frames().contains_key(&1), "a fence loops");
}

/// A guest coming into an empty building lets the pass of `idle` end, then `used` loops from
/// frame 0 and `idle` does not come back; a second guest changes nothing. A dead building
/// shows `idledead` at once, and alive again `idle`, from frame 0.
#[test]
fn a_building_keeps_used_as_its_base_loop() {
    let world = |users: u32, dead: bool| {
        let mut w = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("building", "hut", 1, centre((0, 0)), 0));
        });
        let live = crate::building::Live {
            extra: users,
            dead,
            ..Default::default()
        };
        w.units.buildings.insert(1, live);
        w
    };
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    let mut step = |w: &World, ms: u32| {
        idles.register(w, &Catalogue::default(), &Building, &mut rng);
        idles.advance(ms);
        (
            idles.looks().get(&1).copied(),
            idles.frames().get(&1).copied(),
        )
    };
    let (empty, one, two) = (world(0, false), world(1, false), world(2, false));
    assert_eq!(step(&empty, 150), (None, Some(1)));
    // A guest comes in: the pass of `idle` (4 frames of 100 ms) goes on to its end.
    assert_eq!(step(&one, 100), (None, Some(2)));
    assert_eq!(
        step(&two, 100),
        (None, Some(3)),
        "a second guest: no change"
    );
    // The pass ends: `used` (2 frames) from frame 0.
    assert_eq!(step(&two, 100), (Some("used"), Some(0)));
    assert_eq!(step(&one, 100), (Some("used"), Some(1)));
    // It loops, and it does not go back to `idle` when the guest leaves.
    assert_eq!(step(&empty, 100), (Some("used"), Some(0)));
    assert_eq!(step(&empty, 100), (Some("used"), Some(1)));
    // Another guest in an empty building changes nothing more.
    assert_eq!(step(&one, 100), (Some("used"), Some(0)));
    // Dead: `idledead` at once from frame 0, looping over its 3 frames.
    let dead = world(0, true);
    assert_eq!(step(&dead, 0), (Some("idledead"), Some(0)));
    assert_eq!(step(&dead, 100), (Some("idledead"), Some(1)));
    assert_eq!(step(&dead, 200), (Some("idledead"), Some(0)), "wrapped");
    // Alive again: `idle` at once from frame 0, and `used` is gone.
    assert_eq!(step(&empty, 100), (None, Some(1)));
    // A building first seen with guests inside is not a guest coming in.
    let mut seen = Idles::default();
    seen.register(&one, &Catalogue::default(), &Building, &mut Rng::default());
    seen.advance(1000);
    assert!(seen.looks().is_empty());
    // A type with no `used` keeps its loop, and a trash can its own pictures.
    let mut none = Idles::default();
    let fixed = Fixed(
        Some(Timing {
            count: 4,
            delay: 100,
        }),
        vec![],
    );
    none.register(&empty, &Catalogue::default(), &fixed, &mut rng);
    none.register(&one, &Catalogue::default(), &fixed, &mut rng);
    none.advance(250);
    assert_eq!((none.looks().len(), none.frames()[&1]), (0, 2));
    let mut can = world(1, false);
    can.zoo.entities[0].subtype = "trshcan".into();
    let mut idles = Idles::default();
    idles.register(&world(0, false), &Catalogue::default(), &Building, &mut rng);
    idles.register(&can, &Catalogue::default(), &Building, &mut rng);
    idles.advance(1000);
    assert!(idles.looks().is_empty());
}

/// A food object plays the animation its fill chooses (sprite drawing 3.6; the type's first
/// key, `empty`, is not it); a change of fill starts the other animation at frame 0; a state
/// whose animation the type lacks falls back to `idle`.
#[test]
fn a_food_object_loops_the_animation_its_state_chooses_and_restarts_when_it_changes() {
    let mut cat = Catalogue::default();
    let ini = "[Characteristics/Integers]\ncMaxFoodUnits=12\n";
    cat.push(crate::fixtures::ty("food", "food", "chum", ini));
    let mut world = world_with((4, 4), |zoo| {
        let mut e = entity("food", "food", 1, centre((0, 0)), 0);
        e.subtype = "chum".into();
        e.common.as_mut().unwrap().food = 12;
        zoo.entities.push(e);
    });
    let slow = Timing {
        count: 4,
        delay: 100,
    };
    let fast = Timing {
        count: 4,
        delay: 50,
    };
    let data = Fixed(None, vec![("full", slow), ("half", fast)]);
    let mut idles = Idles::default();
    let step = |idles: &mut Idles, world: &World, ms: u32| {
        idles.register(world, &cat, &data, &mut Rng::default());
        idles.advance(ms);
    };
    step(&mut idles, &world, 200);
    assert_eq!(idles.frames()[&1], 2, "full");
    step(&mut idles, &world, 70);
    assert_eq!(
        idles.frames()[&1],
        2,
        "70 ms are not a frame: still the same state"
    );
    // Half full: its own animation, from frame 0, with its own delay.
    world.zoo.entities[0].common.as_mut().unwrap().food = 6;
    step(&mut idles, &world, 70);
    assert_eq!(idles.frames()[&1], 1, "half");
    // Empty has no animation here and no `idle`: nothing to play.
    world.zoo.entities[0].common.as_mut().unwrap().food = 0;
    step(&mut idles, &world, 70);
    assert!(!idles.frames().contains_key(&1), "empty");
    // Nothing left of a type that has no fill (huge, negative): the state clamps.
    for food in [i32::MAX, i32::MIN, -1] {
        world.zoo.entities[0].common.as_mut().unwrap().food = food;
        step(&mut idles, &world, u32::MAX);
        assert!(idles.frames().values().all(|&f| f < 4), "{food}");
    }
}

/// A food object restarts its loop at frame 0 at every change of its amount, a bite that leaves
/// it `full` included (sprite drawing 3.6).
#[test]
fn a_food_object_restarts_its_loop_at_every_change_of_its_amount() {
    let mut cat = Catalogue::default();
    let ini = "[Characteristics/Integers]\ncMaxFoodUnits=12\n";
    cat.push(crate::fixtures::ty("food", "food", "chum", ini));
    let mut world = world_with((4, 4), |zoo| {
        let mut e = entity("food", "food", 1, centre((0, 0)), 0);
        e.subtype = "chum".into();
        e.common.as_mut().unwrap().food = 12;
        zoo.entities.push(e);
    });
    let full = Timing {
        count: 4,
        delay: 100,
    };
    let data = Fixed(None, vec![("full", full)]);
    let mut idles = Idles::default();
    let mut step = |world: &World, ms: u32| {
        idles.register(world, &cat, &data, &mut Rng::default());
        idles.advance(ms);
        idles.frames()[&1]
    };
    assert_eq!(step(&world, 200), 2);
    // Still `full` after the bite: frame 0 again, then 70 ms.
    world.zoo.entities[0].common.as_mut().unwrap().food = 11;
    assert_eq!(step(&world, 70), 0);
    // The same amount goes on.
    assert_eq!(step(&world, 70), 1);
}

/// The pieces whose `explode` is playing, in id order.
fn exploding(idles: &Idles) -> Vec<u32> {
    let mut ids: Vec<u32> = (idles.looks().into_iter())
        .filter(|(_, n)| *n == "explode")
        .map(|(id, _)| id)
        .collect();
    ids.sort_unstable();
    ids
}

/// A rubble piece made by turn-to-rubble plays `explode` once, from the end of its `idle`
/// loop (one delay), then `idle` again for good; a piece not queued, or whose type has no
/// `explode`, never does (sprite drawing 3.6).
#[test]
fn a_rubble_piece_plays_explode_once_after_its_idle_loop() {
    let world = world_with((4, 4), |zoo| {
        for id in 1..=3 {
            zoo.entities
                .push(entity("rubble", "other", id, centre((0, 0)), 0));
        }
    });
    let idle = Timing {
        count: 1,
        delay: 125,
    };
    let explode = Timing {
        count: 3,
        delay: 100,
    };
    let data = Fixed(Some(idle), vec![("explode", explode)]);
    let mut idles = Idles::default();
    idles.register(&world, &Catalogue::default(), &data, &mut Rng::default());
    // Piece 3 is not queued; an unknown id is ignored.
    idles.queue_explode(&world, &[1, 2, 99], &data);
    let step = |idles: &mut Idles, ms: u32| {
        idles.advance(ms);
        (exploding(idles), idles.frames())
    };
    let (on, _) = step(&mut idles, 124);
    assert!(on.is_empty(), "the idle delay has not ended");
    let (on, frames) = step(&mut idles, 1);
    assert_eq!(on, [1, 2]);
    assert_eq!((frames[&1], frames[&2]), (0, 0));
    assert!(!frames.contains_key(&3));
    let (_, frames) = step(&mut idles, 250);
    assert_eq!(frames[&1], 2);
    // The last frame ends: idle again, still, and no second explosion.
    let (on, frames) = step(&mut idles, 50);
    assert!(on.is_empty() && frames.is_empty());
    let (on, frames) = step(&mut idles, u32::MAX);
    assert!(on.is_empty() && frames.is_empty());
}

/// A type with no `explode`, a delay of 0 or a huge step never end a run or play twice.
#[test]
fn a_missing_explode_or_odd_timings_play_nothing_or_stay_on_a_frame() {
    let world = world_with((4, 4), |zoo| {
        zoo.entities
            .push(entity("rubble", "other", 1, centre((0, 0)), 0));
    });
    let idle = Timing {
        count: 1,
        delay: 125,
    };
    let none = Fixed(Some(idle), vec![]);
    let mut idles = Idles::default();
    idles.register(&world, &Catalogue::default(), &none, &mut Rng::default());
    idles.queue_explode(&world, &[1], &none);
    idles.advance(1000);
    assert!(exploding(&idles).is_empty());
    for explode in [
        Timing { count: 4, delay: 0 },
        Timing {
            count: 0,
            delay: 83,
        },
        Timing { count: 1, delay: 1 },
    ] {
        let data = Fixed(Some(idle), vec![("explode", explode)]);
        let mut idles = Idles::default();
        idles.register(&world, &Catalogue::default(), &data, &mut Rng::default());
        idles.queue_explode(&world, &[1], &data);
        idles.advance(125);
        idles.advance(u32::MAX);
        let frames = idles.frames();
        assert!(frames.values().all(|&f| f < 4));
        // A delay of 0 never ends; the others are back on `idle`.
        assert_eq!(exploding(&idles).len(), usize::from(explode.delay == 0));
    }
}

/// The use sound a random use carries starts with each queued `used` sequence when it becomes
/// the playing one, not at the check; a queue cut by the dead switch never plays it, and a use
/// with no sound starts none.
#[test]
fn a_random_use_sound_starts_with_the_used_sequence() {
    let world = |dead: bool| {
        let mut w = world_with((4, 4), |zoo| {
            zoo.entities
                .push(entity("building", "hut", 1, centre((0, 0)), 0));
        });
        let live = Live {
            dead,
            ..Default::default()
        };
        w.units.buildings.insert(1, live);
        w
    };
    let heard = || Heard {
        file: "use.wav".into(),
        base: 7,
        voice: "building/1/UseSound".into(),
    };
    // (step the sound began, `used` shown at that step) for a run of 14 steps of 100 ms.
    let run = |sound: Option<Heard>, times: u32, cut: bool| {
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        idles.register(&world(false), &Catalogue::default(), &Building, &mut rng);
        idles.queue_used(&world(false), 0, times, sound, &Building);
        let mut began = Vec::new();
        for step in 0..14 {
            if cut && step == 2 {
                idles.register(&world(true), &Catalogue::default(), &Building, &mut rng);
            }
            idles.advance(100);
            let shown = idles.looks().get(&1) == Some(&"used");
            began.extend(idles.take_heard().into_iter().map(|h| (step, shown, h)));
        }
        began
    };
    // The `idle` pass of 400 ms ends at step 3; each `used` pass of 200 ms starts a sound.
    let at = |step| (step, true, (1, heard()));
    assert_eq!(run(Some(heard()), 2, false), [at(3), at(5)]);
    assert_eq!(run(Some(heard()), 1, false), [at(3)]);
    assert_eq!(run(Some(heard()), 0, false), []);
    assert_eq!(run(None, 2, false), []);
    assert_eq!(run(Some(heard()), 2, true), [], "the dead switch drops it");
}

/// A flying creature plays the loop of its picture from the step after it appears, like a fixed
/// object (ambients 5.1).
#[test]
fn a_flying_creature_loops_its_frames() {
    let world = world_with((4, 4), |zoo| {
        zoo.entities
            .push(entity("ambient", "seagull", 7, centre((0, 0)), 4));
    });
    let data = Fixed(
        Some(Timing {
            count: 3,
            delay: 100,
        }),
        vec![],
    );
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    let mut frames = Vec::new();
    for _ in 0..5 {
        idles.register(&world, &Catalogue::default(), &data, &mut rng);
        idles.advance(100);
        frames.push(idles.frames().get(&7).copied());
    }
    assert_eq!(frames, [Some(1), Some(2), Some(0), Some(1), Some(2)]);
    // A one-picture creature has no clock to show.
    let still = Fixed(
        Some(Timing {
            count: 1,
            delay: 100,
        }),
        vec![],
    );
    let mut none = Idles::default();
    none.register(&world, &Catalogue::default(), &still, &mut rng);
    none.advance(500);
    assert!(none.frames().is_empty());
}

/// The tank of `tank_world` with its water at `level` and `purity`.
fn watered(level: u32, purity: u32) -> World {
    let mut world = tank_world(4, level, &[]);
    let id = world.exhibits().list()[0].id;
    let tank = world
        .exhibits_mut()
        .get_mut(id)
        .unwrap()
        .tank
        .as_mut()
        .unwrap();
    tank.purity = purity;
    world
}

/// The ripples against the walls of a tank with clean water start at random times of 0 to 1999 ms,
/// one draw a wall in id order, when the tank is set up; a tank already set up draws nothing more,
/// and one that loses its water or its clean purity loses them and sets them up again later.
#[test]
fn the_wall_ripples_start_at_random_times_when_the_tank_is_set_up() {
    let data = Fixed(
        Some(Timing {
            count: 20,
            delay: 100,
        }),
        vec![],
    );
    let (mut rng, mut idles) = (Rng::default(), Idles::default());
    let world = watered(3, 90);
    let walls = world.rippled_tanks()[0].1.clone();
    assert_eq!(
        walls.len(),
        14,
        "one for each wall held by a tile of the tank"
    );
    idles.register(&world, &Catalogue::default(), &data, &mut rng);
    let mut expect = Rng::default();
    let starts: Vec<u32> = walls.iter().map(|_| expect.pick(2000)).collect();
    assert_eq!(rng.state(), expect.state());
    let set = &idles.ripples[&world.exhibits().list()[0].id];
    assert_eq!(set.iter().map(|w| w.1.t).collect::<Vec<_>>(), starts);
    // Their frames are given under the key of the wall.
    idles.advance(67);
    let frames = idles.frames();
    // (The ids of the helper's walls are not all different.)
    assert_eq!(frames.len(), walls.iter().collect::<HashSet<_>>().len());
    assert!(walls.iter().all(|&w| frames.contains_key(&ripple_key(w))));
    assert!(frames.values().any(|&f| f != frames[&ripple_key(walls[0])]));
    idles.register(&world, &Catalogue::default(), &data, &mut rng);
    assert_eq!(rng.state(), expect.state());
    // No water, murky water, or a purity that is 0: none, and no draw.
    for (level, purity) in [(0, 90), (3, 0), (3, 59), (0, 0)] {
        let dry = watered(level, purity);
        idles.register(&dry, &Catalogue::default(), &data, &mut rng);
        assert!(
            idles.ripples.is_empty() && idles.frames().is_empty(),
            "{level} {purity}"
        );
        assert_eq!(rng.state(), expect.state());
    }
    // The water comes back with a level above 0 at any height, and sets them up again.
    idles.register(
        &watered(u32::MAX, u32::MAX),
        &Catalogue::default(),
        &data,
        &mut rng,
    );
    walls.iter().for_each(|_| _ = expect.pick(2000));
    assert_eq!(rng.state(), expect.state());
    assert_eq!(idles.ripples.len(), 1);
}

/// A ripple whose animation is missing or still has no frame to give, and a game starting anew
/// has no clock.
#[test]
fn a_wall_ripple_without_an_animation_shows_no_frame_and_a_new_game_clears_it() {
    let world = watered(3, 90);
    for data in [
        Fixed(None, vec![]),
        Fixed(
            Some(Timing {
                count: 1,
                delay: 100,
            }),
            vec![],
        ),
        Fixed(Some(Timing { count: 0, delay: 0 }), vec![]),
    ] {
        let (mut rng, mut idles) = (Rng::default(), Idles::default());
        idles.register(&world, &Catalogue::default(), &data, &mut rng);
        idles.advance(10_000);
        assert!(idles.frames().is_empty());
        assert_eq!(idles.ripples.len(), 1);
        idles.clear();
        assert!(idles.ripples.is_empty());
    }
}

/// A wall held by the tile outside a tank gets no ripple, though it stands on the tank's boundary
/// (map drawing 9.7.8 point 1): only the walls in the edge slots of the tank's tiles count.
#[test]
fn a_wall_held_by_the_tile_outside_gets_no_ripple() {
    use crate::unit::testing::tank_world_with;
    let outside = entity("tankwall", "iron", 7777, (6 * 64, 2 * 64 + 32), 6);
    let mut world = tank_world_with(4, 3, &[], vec![outside]);
    let id = world.exhibits().list()[0].id;
    world
        .exhibits_mut()
        .get_mut(id)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .purity = 90;
    let walls = &world.rippled_tanks()[0].1;
    assert_eq!(walls.len(), 14);
    assert!(!walls.contains(&7777));
}

/// A save takes the ripples of the tanks off and puts them back: every clean tank draws its start
/// times again (tank staff 9), unless `animatedwater` is off.
#[test]
fn a_save_draws_the_wall_ripples_again() {
    let data = Fixed(
        Some(Timing {
            count: 20,
            delay: 100,
        }),
        vec![],
    );
    for (on, again) in [(true, true), (false, false)] {
        let mut world = watered(3, 90);
        let mut sim = crate::sim::Sim::new(b"");
        sim.ai.animated_water = on;
        sim.set_seed(1);
        sim.start_game(0, crate::zoo::Clocks::default());
        let cat = Catalogue::default();
        sim.idles.register(&world, &cat, &data, &mut sim.rng);
        let before = sim.rng.state();
        sim.save_started(&mut world);
        sim.idles.register(&world, &cat, &data, &mut sim.rng);
        assert_eq!(sim.rng.state() != before, again, "animatedwater {on}");
    }
}

/// A tank that gets its first water in a step sets up its ripples in that step, at the change
/// (map drawing 9.7.8 points 3 and 4), not at the start of the next one; the next step draws
/// nothing more for it, and a tank whose water stays murky sets up none.
#[test]
fn the_ripples_of_a_filling_tank_are_set_up_in_the_step_of_the_fill() {
    use crate::behaviour::NoData;
    for (purity, set_up) in [(90, true), (59, false)] {
        let mut world = watered(0, purity);
        let id = world.exhibits().list()[0].id;
        world.fill(id);
        world.take_ripple_setups();
        let (mut sim, cat) = (crate::sim::Sim::new(b""), Catalogue::default());
        sim.set_seed(1);
        sim.start_game(0, crate::zoo::Clocks::default());
        let money = &mut crate::money::Money::default();
        let step = |sim: &mut crate::sim::Sim, world: &mut World, money: &mut _| {
            sim.step_game(&NoData, &cat, world, money, 100, |_, _, _| {});
        };
        step(&mut sim, &mut world, money);
        assert!(world.tank(id).unwrap().level > 0, "the first step fills");
        let walls = world.rippled_tanks().first().map(|t| t.1.len());
        let made = sim.idles.ripples.get(&id).map(Vec::len);
        assert_eq!(made, walls.filter(|_| set_up), "purity {purity}");
        // The next step draws no new start time for the ripples that stand (no timing here, so
        // the clocks stay where they were drawn).
        let start = |sim: &crate::sim::Sim| sim.idles.ripples.get(&id).map(|s| s[0].1.t);
        let before = start(&sim);
        step(&mut sim, &mut world, money);
        assert_eq!(before, start(&sim), "purity {purity}");
    }
}

/// The level setter of a tank that gets water resets the AI of the units in the tank first (one
/// draw each, a keeper's wait of 1000 to 4999 ms) and sets the ripples up after it (map drawing
/// 9.7.8 point 4): the generator gives the reset's draw, then the start times of the walls.
#[test]
fn a_filling_tank_resets_its_units_before_it_sets_up_its_ripples() {
    use crate::behaviour::NoData;
    use crate::unit::testing::tank_world_with;
    // The tank fills with the steps, or at once while paused (as an undo of the player does).
    for paused in [false, true] {
        // A keeper stands outside the gate of the tank, whose units are reset too.
        let keeper = entity("keeper", "", 7, centre((2, 1)), 0);
        let mut world = tank_world_with(4, 0, &[], vec![keeper]);
        let id = world.exhibits().list()[0].id;
        let tank = world.exhibits_mut().get_mut(id).unwrap().tank.as_mut();
        tank.unwrap().purity = 90;
        world.take_stirred();
        world.take_ripple_setups();
        world.set_paused(paused);
        assert_eq!(world.fill(id), None);
        let (mut sim, cat) = (crate::sim::Sim::new(b""), Catalogue::default());
        sim.set_seed(1);
        sim.start_game(0, crate::zoo::Clocks::default());
        let first = sim.rng.clone();
        let money = &mut crate::money::Money::default();
        sim.step_game(&NoData, &cat, &mut world, money, 100, |_, _, _| {});
        let wait = world.units.get(7).unwrap().wait;
        let starts: Vec<u32> = sim.idles.ripples[&id].iter().map(|w| w.1.t).collect();
        assert_eq!(starts.len(), 14);
        let found = (0..5000).any(|skip| {
            let mut rng = first.clone();
            (0..skip).for_each(|_| _ = rng.rand());
            let reset = 1000 + rng.pick(4000) as i32;
            reset == wait && starts.iter().all(|&s| rng.pick(2000) == s)
        });
        assert!(
            found,
            "paused {paused}: the reset draws, then the 14 start times"
        );
    }
}
