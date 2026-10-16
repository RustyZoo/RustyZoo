//! The goal record, the objects, the ladder and the wander on synthetic maps.

use super::*;
use crate::behaviour::{Call, Data, Env, Heard, NoData, Note, Number, Row, Timing, parse_call};
use crate::entity::Catalogue;
use crate::sim::{Events, Rng, Sim};
use crate::unit::testing::{centre, entity, rates, world};
use crate::zoo::Clocks;
use std::collections::HashMap;

/// A 10 by 10 map with one unit of this class and id 1 at tile (5, 5).
fn map(class: &str) -> World {
    world((10, 10), |_, e| {
        e.push(entity(class, "", 1, centre((5, 5)), 0))
    })
}

fn sim() -> Sim {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(100, Clocks::default());
    sim
}

fn tile(x: usize, y: usize) -> usize {
    y * 10 + x
}

fn unit(w: &World) -> &Unit {
    w.units.get(1).unwrap()
}

/// A unit type made of text: the sets as lists of calls.
struct Sets(HashMap<String, Vec<Call>>, Vec<Row>);

impl Sets {
    fn new(sets: &[(&str, &[&str])]) -> Sets {
        let call = |c: &&str| parse_call(c.as_bytes()).unwrap().0;
        let list = sets
            .iter()
            .map(|(n, c)| (n.to_string(), c.iter().map(call).collect()));
        Sets(list.collect(), Vec::new())
    }

    /// The same, and an animal's ambient table starts `bWalk`.
    fn strolling() -> Sets {
        let mut sets = Sets::walking();
        sets.1.push(Row {
            lo: i32::MIN,
            hi: i32::MAX,
            set: "bWalk".into(),
        });
        sets
    }

    /// The sets of a unit that walks: `bWalk` ends at once, `bHold` never does.
    fn walking() -> Sets {
        Sets::new(&[
            ("bWalk", &["fWalk(0,0)"]),
            ("bHold", &["fKeepControl()", "fKeepControl()"]),
        ])
    }
}

impl Data for Sets {
    fn set(&self, _: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        self.0.get(name).map(Vec::as_slice)
    }
    /// An animal walks through its ambient table.
    fn ambient(&self, unit: &crate::zoo::Entity) -> &[Row] {
        if unit.class == "animals" {
            &self.1
        } else {
            &[]
        }
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let count = match name {
            "a" => 2,
            "idle" | "walk" | "trot" | "run" => 1,
            _ => return None,
        };
        Some(Timing { count, delay: 100 })
    }
    fn number(&self, _: &crate::zoo::Entity, number: &Number) -> i32 {
        rates(number)
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<Heard> {
        None
    }
}

/// Steps the whole game `n` times and returns the `unit` lines.
fn run(w: &mut World, s: &mut Sim, n: usize) -> Vec<String> {
    run_with(w, s, &Sets::strolling(), n)
}

fn run_with(w: &mut World, s: &mut Sim, data: &dyn Data, n: usize) -> Vec<String> {
    let mut log = Vec::new();
    for _ in 0..n {
        s.step_with(data, &Catalogue::default(), w, 67, |_, _, _| {});
        log.extend(w.units.take_lines());
    }
    log
}

/// Runs `f` on a goal update context of unit 1, whose state is put back.
fn with_cx<R>(w: &mut World, rng: &mut Rng, f: impl FnOnce(&mut Cx) -> R) -> R {
    with_data(w, rng, &NoData, f)
}

fn with_data<R>(w: &mut World, rng: &mut Rng, data: &dyn Data, f: impl FnOnce(&mut Cx) -> R) -> R {
    let mut u = unit(w).clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &Catalogue::default(),
    };
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at: 0,
        data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(1).unwrap() = u;
    r
}

#[test]
fn the_tasks_and_the_jobs_have_the_names_of_the_original() {
    let tasks = [
        (0, "none"),
        (1, "random"),
        (100, "site"),
        (101, "habitat"),
        (112, "gawk"),
    ];
    let more = [
        (116, "chase"),
        (122, "drinkwater"),
        (134, "returnfromshow"),
        (135, "unknown"),
    ];
    for (task, name) in tasks.into_iter().chain(more) {
        assert_eq!(task_name(task), name);
    }
    let jobs = [
        (0, "none"),
        (1, "PutFood"),
        (29, "ClimbDownInsideLadder"),
        (30, "unknown"),
    ];
    for (job, name) in jobs {
        assert_eq!(job_name(job), name);
    }
}

#[test]
fn the_factory_builds_the_base_goal_outside_the_table_and_nothing_for_tasks_not_built() {
    for task in [0, 1, 2, 3, 99, 135, 255] {
        assert_eq!(Goal::build(task, Kind::Animal), Some(Goal::Base));
    }
    assert_eq!(Goal::build(HABITAT, Kind::Guest), Some(Goal::Habitat));
    assert_eq!(
        Goal::build(HABITAT, Kind::Animal),
        None,
        "a keeper's, later"
    );
    assert_eq!(Goal::build(GAWK, Kind::Guest), None);
}

#[test]
fn a_duration_is_over_with_strictly_more_ticks_than_it_says() {
    let r = Record {
        start: 10,
        duration: 12,
        ..Record::default()
    };
    assert!(!r.duration_over(22) && r.duration_over(23));
    assert!(
        r.duration_over(5),
        "unsigned: a clock behind is a huge time"
    );
    let r = Record {
        start: u32::MAX,
        duration: 1,
        ..Record::default()
    };
    assert!(!r.duration_over(0) && r.duration_over(1), "the clock wraps");
}

#[test]
fn setting_a_task_clears_the_job_marks_the_object_and_logs_the_change() {
    let mut w = map("guests");
    let mut rng = Rng::default();
    w.units.ai_ticks = 7;
    let t = tile(2, 3);
    w.units.take_lines();
    with_cx(&mut w, &mut rng, |cx| {
        cx.set_job(16, 0, Some(t));
        assert_eq!(
            cx.record().job_target.pos,
            Some((128, 192, 0)),
            "the tile's corner"
        );
        cx.set_task(HABITAT, 4, 9, Some(t));
        cx.set_task(HABITAT, 0, 0, Some(t));
        let r = cx.record().clone();
        assert_eq!(
            (r.task, r.mode, r.start, r.duration, r.job),
            (101, 0, 7, 0, 0)
        );
        assert_eq!(r.target.tile, Some(t));
        assert!(!r.job_target.is_set() && !r.done && cx.unit.goal.stale);
        cx.unit
            .goal
            .record
            .target
            .set_pos(cx.world, Some((130, 200, 0)));
        assert_eq!(
            cx.record().target.tile,
            Some(t),
            "a position gives its tile"
        );
        cx.unit
            .goal
            .record
            .target
            .set_pos(cx.world, Some((-1, 0, 0)));
        assert_eq!(cx.record().target.tile, None, "off the map");
        cx.set_task(NONE, 0, 0, None);
    });
    assert_eq!(
        w.units.take_lines(),
        [
            "job 1 GoToFence",
            "goal 1 habitat 4 9 2,3",
            "job 1 none",
            "goal 1 none 0 - -"
        ]
    );
}

#[test]
fn a_task_change_replaces_the_object_at_the_next_update() {
    let mut w = map("guests");
    let mut s = sim();
    s.step(&mut w, 67, |_, _, _| {});
    assert_eq!(unit(&w).goal.object, Some(Goal::Base));
    let mut rng = Rng::default();
    with_cx(&mut w, &mut rng, |cx| {
        cx.set_task(HABITAT, 0, 0, Some(tile(5, 5)));
    });
    assert_eq!(
        unit(&w).goal.object,
        Some(Goal::Base),
        "not touched by the setter"
    );
    s.step(&mut w, 67, |_, _, _| {});
    assert_eq!(unit(&w).goal.object, Some(Goal::Habitat));
    // Habitat, already in the viewing tile: done, complete, and the next task has no object.
    assert_eq!(unit(&w).goal.record.task, GAWK);
    s.step(&mut w, 67, |_, _, _| {});
    assert_eq!(unit(&w).goal.object, None);
    assert_eq!(
        unit(&w).goal.record.task,
        GAWK,
        "nothing builds it yet: it stays"
    );
}

#[test]
fn an_idle_animal_wanders_to_a_tile_and_then_picks_another_walk() {
    let mut w = map("animals");
    let mut s = sim();
    let log = run(&mut w, &mut s, 1500);
    let goals: Vec<&str> = (log.iter())
        .filter(|l| l.starts_with("goal"))
        .map(String::as_str)
        .collect();
    // 15.1: task 0 becomes 1 at once; a walk that ends resets the goal, and the next step sets 1.
    assert_eq!(
        goals[..3],
        [
            "goal 1 random 1 - -",
            "goal 1 none 0 - -",
            "goal 1 random 1 - -"
        ]
    );
    assert!(goals.len() > 6, "{goals:?}");
    let steps = log.iter().filter(|l| l.starts_with("tile 1 ")).count();
    assert!(steps >= 3, "it walked: {steps} tiles");
    assert!(unit(&w).goal.record.task <= RANDOM);
}

#[test]
fn a_wander_starts_once_and_is_done_when_the_unit_arrives() {
    let mut w = map("animals");
    let mut s = sim();
    let data = Sets::strolling();
    run_with(&mut w, &mut s, &data, 2);
    let u = unit(&w);
    assert_eq!(u.goal.record.task, RANDOM);
    let to = u.destination.unwrap();
    assert_eq!(
        u.goal.record.target.pos,
        Some(to),
        "the target becomes the destination"
    );
    assert!(u.path.len() >= 2 && u.moving);
    let at = u.goal.record.target.tile.unwrap();
    let mut arrived = false;
    for _ in 0..2000 {
        let log = run_with(&mut w, &mut s, &data, 1);
        let u = unit(&w);
        // The goal is done in the step the unit enters the tile, which the move call also ends.
        arrived = log.contains(&"goal 1 none 0 - -".to_string());
        if arrived {
            assert!(!u.moving && u.path.is_empty() && u.speed == 0);
            break;
        }
        assert_ne!(u.goal.record.task, NONE, "it ends only on the target tile");
    }
    assert!(arrived);
    assert_eq!(
        w.zoo.entities[0]
            .common
            .as_ref()
            .map(|c| (c.x / 64, c.y / 64)),
        Some((at as i32 % 10, at as i32 / 10))
    );
}

#[test]
fn a_refused_step_makes_the_goal_give_up_and_clears_the_flag() {
    let mut w = map("guests");
    crate::goal::quiet(&mut w);
    let mut s = sim();
    let u = w.units.get_mut(1).unwrap();
    u.goal.record.target.tile = Some(tile(8, 5));
    u.path = vec![tile(5, 5), tile(6, 5)];
    run(&mut w, &mut s, 1);
    assert!(unit(&w).goal.record.target.is_set() || !unit(&w).path.is_empty());
    w.units.get_mut(1).unwrap().aborted = true;
    run(&mut w, &mut s, 1);
    let u = unit(&w);
    assert!(!u.aborted && u.path.is_empty() && !u.goal.record.target.is_set());
    assert_eq!(u.goal.record.task, GAWK, "the task stays");
}

#[test]
fn a_unit_with_no_place_to_go_keeps_trying_and_walks_nowhere() {
    let mut w = world((1, 1), |_, e| {
        e.push(entity("animals", "", 1, centre((0, 0)), 0))
    });
    let mut s = sim();
    let log = run(&mut w, &mut s, 50);
    let u = unit(&w);
    assert_eq!(u.goal.record.task, RANDOM);
    // Step 5 of the random walk: the path of its own tile, which the move call ends at once.
    assert!(u.path.len() < 2 && !u.moving);
    assert!(!log.iter().any(|l| l.starts_with("tile")), "{log:?}");
    assert!(
        log.iter().filter(|l| l.starts_with("goal")).count() > 1,
        "it chooses again"
    );
}

#[test]
fn going_to_an_unreachable_tile_or_without_leave_to_move_fails_and_leaves_no_target() {
    let mut w = map("animals");
    let mut rng = Rng::default();
    with_data(&mut w, &mut rng, &Sets::walking(), |cx| {
        assert!(!cx.go_to(1000, false, false, true), "off the map");
        assert!(cx.unit.path.is_empty() && cx.unit.destination.is_none());
        assert!(cx.go_to(tile(7, 5), false, false, true));
        assert_eq!(cx.unit.path.last(), Some(&tile(7, 5)));
        assert_eq!(cx.record().target.tile, Some(tile(7, 5)));
        // Carried: the walk set does not start; the path and the destination stay as asked.
        cx.record().target.clear();
        cx.unit.carried = true;
        assert!(!cx.go_to(tile(6, 5), false, false, true));
        assert_eq!(cx.unit.path.last(), Some(&tile(6, 5)));
        assert!(cx.unit.destination.is_some() && !cx.record().target.is_set());
    });
}

#[test]
fn a_synthetic_goal_runs_a_go_to_job_and_then_a_go_to_task_to_their_ends() {
    let mut w = map("guests");
    let mut s = sim();
    let mut rng = Rng::default();
    let (job_at, task_at) = (tile(6, 5), tile(8, 5));
    let data = Sets::walking();
    w.units.take_lines();
    with_data(&mut w, &mut rng, &data, |cx| {
        cx.set_task(HABITAT, 0, 0, Some(task_at));
        cx.set_job(16, 0, None);
        assert!(cx.go_to(job_at, false, true, false), "the job walks first");
    });
    assert_eq!(unit(&w).goal.record.job_target.tile, Some(job_at));
    let log = run_with(&mut w, &mut s, &data, 200);
    let u = unit(&w);
    assert!(
        u.goal.record.job_done && !u.goal.record.job_target.is_set(),
        "{log:?}"
    );
    assert_eq!(
        u.goal.record.task, HABITAT,
        "the task waits for its own walk"
    );
    assert_eq!(u.goal.record.target.tile, Some(task_at));
    with_data(&mut w, &mut rng, &data, |cx| {
        assert!(cx.go_to(task_at, false, false, false))
    });
    let log = run_with(&mut w, &mut s, &data, 400);
    assert_eq!(unit(&w).goal.record.task, GAWK, "{log:?}");
    assert!(log.contains(&"goal 1 gawk 0 - -".to_string()));
    assert!(!unit(&w).moving);
}

#[test]
fn a_halted_animal_runs_no_goal_update_and_draws_nothing() {
    for halt in 0..3 {
        let mut w = map("animals");
        let mut s = sim();
        run(&mut w, &mut s, 3);
        assert_eq!(unit(&w).goal.record.task, RANDOM);
        let u = w.units.get_mut(1).unwrap();
        u.goal.object = Some(Goal::Base);
        u.goal.stale = false;
        let Part::Animal(a) = &mut u.part else {
            panic!()
        };
        [&mut a.dying, &mut a.crated, &mut a.egg][halt].clone_from(&true);
        // One update: the record is cleared and stamped again, nothing else runs.
        w.units.ai_ticks = 9;
        let mut rng = Rng::default();
        let before = rng.state();
        let mut u = unit(&w).clone();
        let (events, mut removals) = (Events::default(), Vec::new());
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &Catalogue::default(),
        };
        update(&mut u, &mut w, &mut ctx, 0, &NoData);
        assert_eq!(rng.state(), before, "no draw");
        assert_eq!((u.goal.record.task, u.goal.record.start), (NONE, 9));
        assert!(
            u.goal.stale && u.goal.object == Some(Goal::Base),
            "no object update"
        );
        *w.units.get_mut(1).unwrap() = u;
        // At every update while the state is up, and no wander in between.
        let log = run(&mut w, &mut s, 100);
        assert_eq!(
            log.iter()
                .filter(|l| l.starts_with("goal"))
                .collect::<Vec<_>>(),
            ["goal 1 none 0 - -"]
        );
        assert_eq!(unit(&w).goal.record.task, NONE);
    }
}

#[test]
fn a_goal_update_draws_one_number_and_a_guest_standing_draws_one_more() {
    let draws = |class: &str, wandered: bool| {
        let mut w = map(class);
        w.units.get_mut(1).unwrap().goal.wandered = wandered;
        let mut rng = Rng::default();
        let mut at = rng.clone();
        let mut u = unit(&w).clone();
        let (events, mut removals) = (Events::default(), Vec::new());
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &Catalogue::default(),
        };
        update(&mut u, &mut w, &mut ctx, 0, &NoData);
        (1..=3).find(|_| {
            at.rand();
            at.state() == rng.state()
        })
    };
    assert_eq!(draws("animals", false), Some(1), "the common part");
    assert_eq!(draws("keeper", false), Some(1));
    assert_eq!(
        draws("guests", true),
        Some(2),
        "the ladder's draw and the common part"
    );
}

#[test]
fn a_guest_walks_once_then_stands_until_its_flag_comes_down_and_walks_again() {
    let mut w = map("guests");
    let mut s = sim();
    let data = Sets::walking();
    let mut standing = 0;
    let mut walks = 0;
    for _ in 0..600 {
        let log = run_with(&mut w, &mut s, &data, 1);
        walks += log.iter().filter(|l| *l == "goal 1 random 1 - -").count();
        let u = unit(&w);
        if u.goal.wandered {
            assert_eq!(u.goal.record.task, NONE, "it stands");
            standing += 1;
        }
    }
    assert!(walks >= 3, "{walks} walks");
    assert!(standing >= walks, "it stood between the walks: {standing}");
}

#[test]
fn aborting_and_resetting_a_goal_empty_the_path_and_the_targets() {
    let mut w = map("animals");
    let mut rng = Rng::default();
    with_cx(&mut w, &mut rng, |cx| {
        cx.set_task(HABITAT, 0, 3, Some(tile(7, 5)));
        cx.set_job(16, 4, Some(tile(7, 5)));
        assert!(cx.path_to(tile(7, 5), false));
        cx.abort();
        let r = cx.record().clone();
        assert_eq!((r.task, r.job), (HABITAT, 16), "the task stays");
        assert!(!r.target.is_set() && !r.job_target.is_set() && cx.unit.path.is_empty());
        assert!(cx.path_to(tile(7, 5), false));
        cx.unit.goal.wants_to_decide = true;
        cx.reset();
        assert_eq!((cx.record().task, cx.record().job), (NONE, 0));
        assert!(cx.unit.path.is_empty() && !cx.unit.goal.wants_to_decide);
    });
}

#[test]
fn the_hash_covers_the_goals() {
    let base = || map("guests");
    let h = |w: &World| w.units.hash();
    let b = h(&base());
    // A new field breaks these patterns until it has a change below.
    let Goals {
        record:
            Record {
                task: _,
                mode: _,
                target:
                    Target {
                        id: _,
                        tile: _,
                        pos: _,
                    },
                job: _,
                job_target: _,
                start: _,
                duration: _,
                done: _,
                job_done: _,
            },
        object: _,
        stale: _,
        sub: _,
        sub_stale: _,
        wants_to_decide: _,
        wandered: _,
    } = Goals::default();
    let changes: [fn(&mut Goals); 16] = [
        |g| g.record.task = 1,
        |g| g.record.mode = 1,
        |g| g.record.target.id = 1,
        |g| g.record.target.tile = Some(1),
        |g| g.record.target.pos = Some((1, 2, 3)),
        |g| g.record.job = 1,
        |g| g.record.job_target.id = 1,
        |g| g.record.job_target.tile = Some(1),
        |g| g.record.start = 1,
        |g| g.record.duration = 1,
        |g| g.record.done = true,
        |g| g.record.job_done = true,
        |g| g.object = Some(Goal::Base),
        |g| g.stale = true,
        |g| g.wants_to_decide = true,
        |g| g.wandered = true,
    ];
    for (i, change) in changes.iter().enumerate() {
        let mut w = base();
        change(&mut w.units.get_mut(1).unwrap().goal);
        assert_ne!(h(&w), b, "change {i}");
    }
    let mut w = base();
    w.units.get_mut(1).unwrap().goal.sub = Some(Sub::Base);
    assert_ne!(h(&w), b);
    w.units.get_mut(1).unwrap().goal.sub_stale = true;
    assert_ne!(h(&w), b);
}

#[test]
fn an_animals_wander_takes_a_destination_and_a_target_and_starts_no_set() {
    let data = Sets::walking();
    let mut w = world((10, 10), |_, e| {
        e.push(entity("animals", "", 1, centre((5, 5)), 0));
        e.push(entity("guests", "", 2, centre((2, 2)), 0));
    });
    let mut s = sim();
    let log = run_with(&mut w, &mut s, &data, 3);
    assert!(!log.contains(&"set 1 bWalk".to_string()), "{log:?}");
    assert!(log.contains(&"set 2 bWalk".to_string()), "{log:?}");
    let animal = unit(&w);
    assert!(animal.runner.top().is_none(), "no set");
    assert!(animal.goal.record.target.tile.is_some() && animal.destination.is_some());
    assert_eq!(animal.goal.record.target.pos, animal.destination);
}

#[test]
fn the_wander_is_done_on_the_target_tile_with_fewer_than_two_calls_left() {
    let data = Sets::new(&[
        ("bOne", &["fKeepControl()"]),
        ("bTwo", &["fKeepControl()", "fKeepControl()"]),
    ]);
    // (set, done): no set counts 0; the path is not what counts.
    for (set, done) in [(None, true), (Some("bOne"), true), (Some("bTwo"), false)] {
        let mut w = map("animals");
        let mut rng = Rng::default();
        with_data(&mut w, &mut rng, &data, |cx| {
            cx.set_task(RANDOM, 1, 0, Some(tile(5, 5)));
            cx.unit.path = vec![tile(5, 5), tile(6, 5), tile(7, 5), tile(8, 5)];
            if let Some(set) = set {
                assert!(cx.start_set(set, false));
            }
            cx.check_done();
            assert_eq!(cx.record().done, done, "{set:?}");
            assert!(!cx.record().target.is_set(), "the target is cleared");
            if !done {
                // The task stays and the decide of the same update draws a new place.
                cx.unit.path.clear();
                cx.base();
                assert_eq!(cx.record().task, RANDOM);
                assert!(cx.record().target.is_set() && !cx.record().done);
            }
        });
    }
    // Not on the target tile: nothing happens, whatever the set.
    let mut w = map("animals");
    let mut rng = Rng::default();
    with_data(&mut w, &mut rng, &data, |cx| {
        cx.set_task(RANDOM, 1, 0, Some(tile(6, 5)));
        cx.check_done();
        assert!(!cx.record().done && cx.record().target.tile == Some(tile(6, 5)));
    });
}

#[test]
fn the_walk_start_restarts_the_set_forced_and_fails_without_its_calls() {
    let data = Sets::new(&[("bWalk", &["fKeepControl()"]), ("bPlay", &["fPlay(a)"])]);
    let mut w = map("animals");
    let mut rng = Rng::default();
    with_data(&mut w, &mut rng, &data, |cx| {
        assert!(cx.path_to(tile(7, 5), false));
        // A running bWalk starts again, and the sequence that plays goes on.
        assert!(cx.start_set("bPlay", false));
        let timing = |n: &str| data.timing(&cx.world.zoo.entities[0], n);
        {
            let mut rng = Rng::default();
            let mut env = Env {
                data: &data,
                unit: &cx.world.zoo.entities[0],
                rng: &mut rng,
            };
            cx.unit.runner.step(&mut cx.unit.animation, &mut env, 0);
        }
        cx.unit.animation.advance(0, &timing);
        assert!(!cx.unit.animation.on_base());
        cx.unit.animation.notes.clear();
        assert!(cx.start_walk(false, false, false));
        assert!(cx.start_walk(false, false, false));
        assert_eq!(
            cx.unit.animation.notes,
            [Note::Set("bWalk".into()), Note::Set("bWalk".into())]
        );
        assert!(!cx.unit.animation.on_base(), "the sequence is kept");
        assert_eq!(cx.record().target.tile, Some(tile(7, 5)));
        // A set that cannot be interrupted refuses a walk that is not forced.
        {
            let mut rng = Rng::default();
            let env = Env {
                data: &data,
                unit: &cx.world.zoo.entities[0],
                rng: &mut rng,
            };
            let (anim, runner) = (&mut cx.unit.animation, &mut cx.unit.runner);
            assert!(runner.start(anim, &env, "bWalk", false, false, true));
        }
        cx.record().target.clear();
        assert!(!cx.start_walk(false, false, false));
        assert!(!cx.record().target.is_set() && !cx.unit.path.is_empty());
        assert!(cx.start_walk(false, false, true), "forced");
        // The flying mode uses bFly, which the type lacks: the start fails, nothing is set and
        // the path and the destination stay.
        cx.record().target.clear();
        cx.world.zoo.entities[0]
            .common
            .as_mut()
            .unwrap()
            .movement_mode = 1;
        assert!(!cx.start_walk(false, false, true));
        assert!(!cx.record().target.is_set());
        assert!(!cx.unit.path.is_empty() && cx.unit.destination.is_some());
    });
    // No calls for the name: the start fails.
    let mut w = map("animals");
    with_data(&mut w, &mut rng, &Sets::new(&[]), |cx| {
        assert!(cx.path_to(tile(7, 5), false));
        assert!(!cx.start_walk(false, false, true));
        assert!(!cx.record().target.is_set() && !cx.unit.path.is_empty());
    });
}
