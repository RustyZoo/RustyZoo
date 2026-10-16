//! The ladder rung that sends a guest out, and the leave goal on a synthetic map.

use super::*;
use crate::behaviour::{Call, Data, Heard, NoData, Number, Row, Timing, parse_call};
use crate::entity::Catalogue;
use crate::goal::Goal;
use crate::guest::{Guest, HAPPINESS};
use crate::money::{self, Money};
use crate::sim::{Events, Rng, Sim};
use crate::unit::testing::{centre, entity, world_with};
use crate::unit::{Ctx, Unit};
use crate::world::World;
use crate::zoo::Clocks;

/// A 10 by 10 map with the zoo entrance at (8, 5) and guest 1 at tile (2, 5), who wants to leave.
/// With `pen`, a pen of 3 by 3 tiles round tile (2, 5) as well.
fn map(pen: bool) -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (8, 5);
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 5)), 0));
        if pen {
            for i in 0..3 {
                for (at, d) in [
                    ((1 + i, 4), 0),
                    ((1 + i, 6), 4),
                    ((1, 4 + i), 6),
                    ((3, 4 + i), 2),
                ] {
                    let id = 100 + zoo.entities.len() as u32;
                    zoo.entities.push(entity("fences", "", id, centre(at), d));
                }
            }
        }
    });
    w.units.get_mut(1).unwrap().part = Part::Guest(Guest {
        leave: true,
        ..Guest::default()
    });
    w
}

fn sim() -> Sim {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(100, Clocks::default());
    sim
}

/// A guest type whose set `bWalk` ends at once and whose set `bGawk` never does.
struct Walks(Vec<Call>, Vec<Call>);

impl Data for Walks {
    fn set(&self, _: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        match name {
            "bWalk" => Some(&self.0[..]),
            "bGawk" => Some(&self.1[..]),
            _ => None,
        }
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let walks = ["idle", "walk", "trot", "run"].contains(&name);
        walks.then_some(Timing {
            count: 1,
            delay: 100,
        })
    }
    fn number(&self, _: &crate::zoo::Entity, number: &Number) -> i32 {
        crate::unit::testing::rates(number)
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<Heard> {
        None
    }
}

fn walks() -> Walks {
    let call = |text: &[u8]| parse_call(text).unwrap().0;
    Walks(vec![call(b"fWalk(0,0)")], vec![call(b"fKeepControl()")])
}

/// Runs `n` steps with the cash `money`; the `unit` and `guest` lines.
fn run(w: &mut World, s: &mut Sim, money: &mut Money, n: usize) -> Vec<String> {
    let mut log = Vec::new();
    for _ in 0..n {
        let update = |_: &mut World, _, _| {};
        s.step_game(&walks(), &Catalogue::default(), w, money, 67, update);
        log.extend(w.units.take_lines());
        log.extend((w.units.take_guest_lines().into_iter()).map(|l| format!("guest {l}")));
    }
    log
}

#[test]
fn a_guest_told_to_leave_walks_to_the_entrance_and_is_removed_there() {
    let mut w = map(false);
    let (mut s, mut money) = (sim(), Money::default());
    let log = run(&mut w, &mut s, &mut money, 600);
    assert!(log.contains(&"goal 1 leave 1 - -".to_string()), "{log:?}");
    assert_eq!(log.iter().filter(|l| *l == "guest leave 1").count(), 1);
    let tiles: Vec<_> = log.iter().filter(|l| l.starts_with("tile 1 ")).collect();
    assert_eq!(
        tiles.last().map(|l| l.as_str()),
        Some("tile 1 8,5"),
        "{tiles:?}"
    );
    assert!(log.contains(&"remove 1".to_string()));
    assert!(w.units.get(1).is_none() && w.find(1).is_none());
    // It counts as gone once, and nobody joined (the happiness 0 is below the threshold).
    assert_eq!(money.total[money::GUESTS_GONE], 1.0);
    assert_eq!(money.total[money::MEMBERS], 0.0);
    // The booking is made at the step, then not again.
    assert_eq!(w.units.guests_gone, 0);
}

#[test]
fn a_guest_on_the_entrance_starts_its_walk_like_any_other_and_leaves() {
    let mut w = map(false);
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre((8, 5));
    let (mut s, mut money) = (sim(), Money::default());
    let log = run(&mut w, &mut s, &mut money, 4);
    // It asks the finder like any other guest and starts its walk set.
    assert!(log.contains(&"set 1 bWalk".to_string()), "{log:?}");
    assert!(log.contains(&"guest leave 1".to_string()), "{log:?}");
    assert_eq!(money.total[money::GUESTS_GONE], 1.0);
}

/// Runs `f` on the goal update context of guest 1.
fn with_cx<R>(w: &mut World, rng: &mut Rng, f: impl FnOnce(&mut Cx) -> R) -> R {
    let mut u: Unit = w.units.get(1).unwrap().clone();
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
fn only_a_free_guest_that_is_out_in_the_open_takes_the_leave_task() {
    let mut rng = Rng::default();
    let mut try_with = |w: &mut World, task: u8| {
        w.units.get_mut(1).unwrap().goal.record.task = task;
        with_cx(w, &mut rng, |cx| (cx.leave_ladder(), cx.record().task))
    };
    let mut w = map(false);
    assert_eq!(try_with(&mut w, 1), (true, LEAVE));
    // Not leaving again, not avoiding or exiting, not in a building task; the others are fine.
    for (task, leaves) in [
        (LEAVE, false),
        (117, false),
        (127, false),
        (104, false),
        (105, false),
        (106, false),
        (102, true),
        (103, true),
        (107, true),
        (101, true),
        (0, true),
    ] {
        assert_eq!(try_with(&mut w, task).0, leaves, "task {task}");
    }
    // Without the flag nobody leaves.
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        g.leave = false;
    }
    assert_eq!(try_with(&mut w, 1), (false, 1));
    // In a tank, on water, in an exhibit.
    let mut tank = map(false);
    let at = tank.find(1).unwrap();
    tank.zoo.entities[at].common.as_mut().unwrap().movement_mode = 3;
    assert!(!try_with(&mut tank, 1).0);
    let mut wet = map(false);
    wet.set_water(vec![false, true]);
    assert!(wet.set_tile(2, 5, 0, 0, 1));
    assert!(!try_with(&mut wet, 1).0);
    let mut pen = map(true);
    assert_eq!(pen.exhibits().list().len(), 1);
    assert!(!try_with(&mut pen, 1).0, "inside a pen");
}

#[test]
fn a_happy_guest_may_join_the_endowment_with_one_draw_and_a_content_one_draws_nothing() {
    for (happiness, chance, joined, draws) in [
        (90, 100, 1, 1),
        (90, 0, 0, 1),
        (81, 100, 1, 1),
        (80, 100, 0, 0),
        (-5, 100, 0, 0),
    ] {
        let mut w = map(false);
        w.units.guest_join = (80, chance);
        if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
            g.values[HAPPINESS] = happiness;
        }
        let mut rng = Rng::default();
        let before = rng.clone();
        with_cx(&mut w, &mut rng, |cx| Goal::Leave.complete(cx));
        let mut check = before;
        (0..draws).for_each(|_| {
            check.rand();
        });
        assert_eq!(
            (rng, w.units.members_joined, w.units.guests_gone),
            (check, joined, 1),
            "{happiness} {chance}"
        );
    }
    // The step books the member and the guest gone.
    let mut w = map(false);
    let (mut s, mut money) = (sim(), Money::default());
    w.units.guest_join = (-100, 100);
    s.status.settings.join_happiness = -100;
    s.status.settings.join_chance = 100;
    run(&mut w, &mut s, &mut money, 600);
    assert_eq!(
        [money::GUESTS_GONE, money::MEMBERS, money::MEMBERS_JOINED].map(|c| money.total[c]),
        [1.0, 1.0, 1.0]
    );
    assert_eq!(
        money.total[money::NET],
        0.0,
        "counts do not touch the net line"
    );
}

fn flag(w: &World) -> bool {
    matches!(&w.units.get(1).unwrap().part, Part::Guest(g) if g.leave)
}

#[test]
fn the_leave_flag_is_down_once_the_walk_to_the_entrance_has_started() {
    let mut w = map(false);
    let (mut s, mut money) = (sim(), Money::default());
    assert!(flag(&w));
    let log = run(&mut w, &mut s, &mut money, 1);
    assert!(log.contains(&"goal 1 leave 1 - -".to_string()), "{log:?}");
    assert!(
        !flag(&w),
        "the walk started and its destination is the entrance"
    );
    assert_eq!(w.units.get(1).unwrap().goal.record.task, LEAVE);
    assert!(log.contains(&"set 1 bWalk".to_string()), "{log:?}");
}

#[test]
fn without_an_entrance_the_guest_walks_at_random_and_keeps_its_flag() {
    let mut w = map(false);
    w.zoo.entrance = (u32::MAX, u32::MAX);
    let (mut s, mut money) = (sim(), Money::default());
    let log = run(&mut w, &mut s, &mut money, 40);
    assert!(log.contains(&"set 1 bWalk".to_string()), "{log:?}");
    assert!(flag(&w), "the walk is no walk to the entrance");
    assert!(!log.contains(&"guest leave 1".to_string()));
}

/// The keys of a guest that matter here, with a certain leave once everything is seen.
const MAN: &str = "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\n\
    cLeaveChanceDone=100\ncGawkTime=10\ncGawkTime=15\ncGawkTime=25\ncGawkTime=35\ncGawkTime=45\n";

fn types() -> Catalogue {
    let ty = |class, name: &str, text: &str, name_id| crate::entity::EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(text.as_bytes()).unwrap(),
        )),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    };
    let mut cat = Catalogue::default();
    cat.push(ty("guests", "man", MAN, 9503));
    cat.push(ty("animals", "zebra", "", 5));
    cat
}

/// A zebra pen round (2, 5) with a path beside it along x = 5 and guest 1 at (8, 2), whose
/// countdowns are far from firing.
fn visit(happiness: i32) -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (9, 5);
        zoo.entities
            .push(entity("guests", "man", 1, centre((8, 2)), 0));
        for i in 0..3 {
            for (at, d) in [
                ((1 + i, 4), 0),
                ((1 + i, 6), 4),
                ((1, 4 + i), 6),
                ((3, 4 + i), 2),
            ] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(at), d));
            }
            zoo.entities.push(entity(
                "paths",
                "stone",
                200 + i as u32,
                centre((5, 4 + i)),
                0,
            ));
        }
        zoo.entities
            .push(entity("animals", "zebra", 10, centre((2, 5)), 0));
    });
    let animal = crate::animal::Animal {
        happiness,
        ..Default::default()
    };
    let zebra = w.units.get_mut(10).unwrap();
    (zebra.part, zebra.started, zebra.repositioning) = (Part::Animal(animal), true, true);
    w.units.get_mut(1).unwrap().part = Part::Guest(Guest {
        countdowns: [1000; 11],
        saw_animal: 1000,
        ..Guest::default()
    });
    w
}

/// Runs `n` steps of 67 ms in a game of `types()`; the `unit` and `guest` lines.
fn visit_run(w: &mut World, n: usize) -> Vec<String> {
    let (mut s, mut money, cat) = (sim(), Money::default(), types());
    let mut log = Vec::new();
    for _ in 0..n {
        s.step_game(&walks(), &cat, w, &mut money, 67, |_, _, _| {});
        log.extend(w.units.take_lines());
        log.extend((w.units.take_guest_lines().into_iter()).map(|l| format!("guest {l}")));
    }
    log
}

#[test]
fn a_guest_goes_to_the_exhibit_it_has_not_seen_views_it_and_leaves_when_all_is_seen() {
    let mut w = visit(50);
    let log = visit_run(&mut w, 4000);
    let at = |text: &str| log.iter().position(|l| l.starts_with(text));
    let order: Vec<_> = [
        "goal 1 site 4",
        "goal 1 habitat 0",
        "goal 1 gawk 0",
        "guest view 1 1",
        "guest donate 1 1",
        "goal 1 leave 1",
        "guest leave 1",
    ]
    .iter()
    .map(|t| at(t))
    .collect();
    assert!(
        order.iter().all(Option::is_some) && order.is_sorted(),
        "{order:?} {log:?}"
    );
    // It walked to a tile of the path, and viewed once.
    let walked = log.iter().filter(|l| l.starts_with("tile 1 5,")).count();
    assert!(walked >= 1, "{log:?}");
    // The first guest only: the new guest check lets others in.
    assert_eq!(
        log.iter()
            .filter(|l| l.starts_with("guest view 1 "))
            .count(),
        1
    );
    assert!(w.units.get(1).is_none(), "removed at the entrance");
    // The exhibit got the time and the donation.
    let stats = &w.exhibits().list()[0].stats;
    assert!(stats.viewing > 0 && stats.donations[2] > 0.0, "{stats:?}");
}

#[test]
fn viewing_lasts_by_the_average_happiness_of_the_exhibit() {
    // 25 to 30 seconds for an average of 50 (guests visit 5.3), 15 to 20 for -20.
    for (happiness, seconds) in [(50, 25..=30), (-20, 15..=20), (90, 45..=50), (-90, 10..=10)] {
        let mut w = visit(happiness);
        let log = visit_run(&mut w, 1500);
        let from = log
            .iter()
            .position(|l| l.starts_with("guest view 1 1"))
            .unwrap();
        let to = from
            + log[from..]
                .iter()
                .position(|l| l.starts_with("goal 1 none"))
                .unwrap();
        let lasted = log[from..to]
            .iter()
            .filter(|l| l.starts_with("guest status 1 "))
            .count();
        assert!(
            lasted as i32 >= *seconds.start() && lasted as i32 <= seconds.end() + 1,
            "{happiness}: {lasted}"
        );
    }
}

#[test]
fn a_guest_in_an_exhibit_gets_the_exit_task_before_anything_else() {
    let mut w = map(true);
    let (mut s, mut money) = (sim(), Money::default());
    // It wants to leave, but the exit task comes first (ladder point 1 before point 2).
    let log = run(&mut w, &mut s, &mut money, 3);
    assert!(
        log.contains(&"goal 1 exittank 0 - -".to_string()),
        "{log:?}"
    );
    assert!(!log.iter().any(|l| l.contains("leave 1")), "{log:?}");
    assert!(flag(&w), "the wish to leave stays");
}

#[test]
fn a_guest_that_has_seen_everything_forgets_the_count_and_may_decide_to_leave() {
    let mut w = visit(50);
    let first = w.exhibits().list()[0].first;
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.seen, g.seen_count) = (vec![first], 3);
    }
    let log = visit_run(&mut w, 3);
    assert!(log.contains(&"goal 1 leave 1 - -".to_string()), "{log:?}");
    let Some(Part::Guest(g)) = w.units.get(1).map(|u| &u.part) else {
        panic!("a guest");
    };
    assert_eq!(
        (g.seen_count, g.seen.len()),
        (0, 1),
        "the count goes, the list stays"
    );
    assert!(!log.iter().any(|l| l.starts_with("goal 1 site")));
}

#[test]
fn an_exhibit_scores_by_attractiveness_the_favourite_species_and_a_far_tile() {
    // attractiveness + 500 for the favourite - (1000 - d) / 10 + 100, d at most 990.
    for (attractiveness, loved, d, score) in [
        (5, false, 0, 5),
        (5, false, 9, 6),
        (5, false, 990, 104),
        (5, true, 4000, 604),
    ] {
        let d = d.min(990);
        assert_eq!(
            super::sites::site_score(attractiveness, loved, d),
            score,
            "{attractiveness} {loved} {d}"
        );
    }
}

#[test]
fn the_site_goal_of_a_guest_that_has_seen_the_sites_of_the_zoo_forgets_them_and_may_leave() {
    let mut w = visit(50);
    let first = w.exhibits().list()[0].first;
    let u = w.units.get_mut(1).unwrap();
    u.goal.record.task = crate::goal::SITE;
    if let Part::Guest(g) = &mut u.part {
        (g.seen, g.seen_count) = (vec![first], 5);
    }
    let log = visit_run(&mut w, 1);
    let Some(Part::Guest(g)) = w.units.get(1).map(|u| &u.part) else {
        panic!("a guest");
    };
    assert_eq!(
        (g.seen_count, g.seen.len(), g.leave),
        (0, 0, true),
        "{log:?}"
    );
}
