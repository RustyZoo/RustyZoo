//! The goal record, the objects, the ladder and the wander on synthetic maps.

use super::*;
use crate::behaviour::NoData;
use crate::entity::Catalogue;
use crate::sim::{Events, Rng, Sim};
use crate::unit::movement;
use crate::unit::testing::{centre, entity, world};
use crate::zoo::Clocks;

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

/// The move call of `bWalk` (the issue of the random walk does it): the walk starts when a path
/// of two tiles or more is held.
fn walk(w: &mut World) {
    let u = w.units.get_mut(1).unwrap();
    if !u.moving && u.path.len() >= 2 {
        let path = u.path.clone();
        movement::start(u, path, 40, 100);
    }
}

/// Steps the whole game `n` times and returns the `unit` lines.
fn run(w: &mut World, s: &mut Sim, n: usize) -> Vec<String> {
    let mut log = Vec::new();
    for _ in 0..n {
        s.step(w, 67, |_, _, _| {});
        walk(w);
        log.extend(w.units.take_lines());
    }
    log
}

/// Runs `f` on a goal update context of unit 1, whose state is put back.
fn with_cx<R>(w: &mut World, rng: &mut Rng, f: impl FnOnce(&mut Cx) -> R) -> R {
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
        data: &NoData,
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
    let log = run(&mut w, &mut s, 400);
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
    assert!(log.contains(&"set 1 bWalk".to_string()));
    let steps = log.iter().filter(|l| l.starts_with("tile 1 ")).count();
    assert!(steps >= 3, "it walked: {steps} tiles");
    assert!(unit(&w).goal.record.task <= RANDOM);
}

#[test]
fn a_wander_starts_once_and_is_done_when_the_unit_arrives() {
    let mut w = map("animals");
    let mut s = sim();
    run(&mut w, &mut s, 2);
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
    for _ in 0..100 {
        s.step(&mut w, 67, |_, _, _| {});
        let u = unit(&w);
        arrived |= u.goal.record.task == NONE;
        if arrived {
            assert!(!u.moving && u.path.len() < 2);
            break;
        }
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
fn a_unit_with_no_way_to_go_keeps_trying_and_holds_nothing() {
    let mut w = world((1, 1), |_, e| {
        e.push(entity("animals", "", 1, centre((0, 0)), 0))
    });
    let mut s = sim();
    let log = run(&mut w, &mut s, 50);
    let u = unit(&w);
    assert_eq!(u.goal.record.task, RANDOM);
    assert!(u.path.is_empty() && u.destination.is_none() && !u.moving);
    assert_eq!(
        log.iter().filter(|l| l.starts_with("goal")).count(),
        1,
        "{log:?}"
    );
}

#[test]
fn going_to_an_unreachable_tile_or_without_leave_to_move_fails_and_leaves_no_path() {
    let mut w = map("animals");
    let mut rng = Rng::default();
    with_cx(&mut w, &mut rng, |cx| {
        assert!(!cx.go_to(1000, false, false), "off the map");
        assert!(cx.unit.path.is_empty() && cx.unit.destination.is_none());
        assert!(cx.go_to(tile(7, 5), false, false));
        assert_eq!(cx.unit.path.last(), Some(&tile(7, 5)));
        assert_eq!(cx.record().target.tile, Some(tile(7, 5)));
        // Carried: the walk set does not start and the path is dropped.
        cx.unit.carried = true;
        assert!(!cx.go_to(tile(6, 5), false, false));
        assert!(cx.unit.path.is_empty());
    });
}

#[test]
fn a_synthetic_goal_runs_a_go_to_job_and_then_a_go_to_task_to_their_ends() {
    let mut w = map("guests");
    let mut s = sim();
    let mut rng = Rng::default();
    let (job_at, task_at) = (tile(6, 5), tile(8, 5));
    w.units.take_lines();
    with_cx(&mut w, &mut rng, |cx| {
        cx.set_task(HABITAT, 0, 0, Some(task_at));
        cx.set_job(16, 0, None);
        assert!(cx.go_to(job_at, false, true), "the job walks first");
    });
    assert_eq!(unit(&w).goal.record.job_target.tile, Some(job_at));
    let log = run(&mut w, &mut s, 200);
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
    with_cx(&mut w, &mut rng, |cx| {
        assert!(cx.go_to(task_at, false, false))
    });
    let log = run(&mut w, &mut s, 400);
    assert_eq!(unit(&w).goal.record.task, GAWK, "{log:?}");
    assert!(log.contains(&"goal 1 gawk 0 - -".to_string()));
    assert!(!unit(&w).moving);
}

#[test]
fn the_ladder_stops_the_goals_of_a_halted_animal_and_it_does_not_wander() {
    for halt in 0..3 {
        let mut w = map("animals");
        let mut s = sim();
        run(&mut w, &mut s, 3);
        assert_eq!(unit(&w).goal.record.task, RANDOM);
        let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part else {
            panic!()
        };
        [&mut a.dying, &mut a.crated, &mut a.egg][halt].clone_from(&true);
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
        if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
            a.crated = true;
        }
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
    let mut standing = 0;
    let mut walks = 0;
    for _ in 0..600 {
        let log = run(&mut w, &mut s, 1);
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
