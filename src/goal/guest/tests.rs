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

pub(super) fn sim() -> Sim {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(100, Clocks::default());
    sim
}

/// A guest type whose sets `bWalk`, `bEatStanding`, and `bSick` end at once, the sets `bGawkObject1` to `6` end at once without moving and whose set `bGawk` never
/// does; `bbenchenergy0` and `btableeat0` enter the building, stay 5 seconds and leave it.
pub(super) struct Walks(Vec<Call>, Vec<Call>, Vec<Call>, Vec<Call>);

impl Data for Walks {
    fn set(&self, _: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        match name {
            "bWalk" | "bEatStanding" | "bSick" => Some(&self.0[..]),
            n if n.starts_with("bGawkObject") => Some(&self.2[..]),
            "bGawk" => Some(&self.1[..]),
            "bbenchenergy0" | "btableeat0" => Some(&self.3[..]),
            _ => None,
        }
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let walks = ["idle", "walk", "trot", "run", "sit"].contains(&name);
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

pub(super) fn walks() -> Walks {
    let call = |text: &[u8]| parse_call(text).unwrap().0;
    Walks(
        vec![call(b"fWalk(0,0)")],
        vec![call(b"fKeepControl()")],
        vec![call(b"fFaceTowardTarget()")],
        // A bench or a table: sit down, stay 5 seconds, get up.
        [
            "fEnterBuilding()",
            "fPlayTime(sit,5)",
            "fPlay(idle)",
            "fExitBuilding()",
        ]
        .map(|c| call(c.as_bytes()))
        .to_vec(),
    )
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
    with_cx_in(&Catalogue::default(), &NoData, w, rng, f)
}

fn with_cx_in<R>(
    cat: &Catalogue,
    data: &dyn Data,
    w: &mut World,
    rng: &mut Rng,
    f: impl FnOnce(&mut Cx) -> R,
) -> R {
    let mut u: Unit = w.units.get(1).unwrap().clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
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
    types_with(MAN)
}

fn types_with(man: &str) -> Catalogue {
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
    cat.push(ty("guests", "man", man, 9503));
    cat.push(ty("animals", "zebra", "", 5));
    cat
}

/// A zebra pen round (2, 5) with a path beside it along x = 5 and guest 1 at (8, 2), whose
/// countdowns are far from firing.
fn visit(happiness: i32) -> World {
    visit_with(happiness, &[])
}

/// The same with more entities, as (class, type, id, tile).
fn visit_with(happiness: i32, more: &[(&str, &str, u32, (i32, i32))]) -> World {
    let mut w = world_with((10, 10), |zoo| {
        for &(class, name, id, at) in more {
            zoo.entities.push(entity(class, name, id, centre(at), 0));
        }
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
    visit_run_in(&types(), w, n)
}

fn visit_run_in(cat: &Catalogue, w: &mut World, n: usize) -> Vec<String> {
    let (mut s, mut money) = (sim(), Money::default());
    let mut log = Vec::new();
    for _ in 0..n {
        s.step_game(&walks(), cat, w, &mut money, 67, |_, _, _| {});
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

/// The seconds of play between the start of viewing and the end of the goal.
fn lasted(cat: &Catalogue, happiness: i32) -> i32 {
    let mut w = visit(happiness);
    let log = visit_run_in(cat, &mut w, 1500);
    let from = log
        .iter()
        .position(|l| l.starts_with("guest view 1 1"))
        .unwrap();
    let to = from
        + log[from..]
            .iter()
            .position(|l| l.starts_with("goal 1 none"))
            .unwrap();
    log[from..to]
        .iter()
        .filter(|l| l.starts_with("guest status 1 "))
        .count() as i32
}

#[test]
fn viewing_lasts_by_the_average_happiness_of_the_exhibit() {
    // 25 to 30 seconds for an average of 50 (guests visit 5.3), 15 to 20 for -20; the status
    // lines counted are one or two fewer than the ticks.
    for (happiness, seconds) in [(50, 25..=30), (-20, 15..=20), (90, 45..=50), (-90, 10..=10)] {
        let n = lasted(&types(), happiness);
        assert!(
            n >= *seconds.start() - 1 && n <= seconds.end() + 1,
            "{happiness}: {n}"
        );
    }
    // Without the key the time is 10 to 13 seconds; a negative time and a huge one are kept in
    // range (the goal is over at once, or lasts for ever).
    let plain =
        "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\ncLeaveChanceDone=100\n";
    let n = lasted(&types_with(plain), 50);
    assert!((9..=14).contains(&n), "{n}");
    let negative =
        format!("{plain}cGawkTime=-9\ncGawkTime=-9\ncGawkTime=-9\ncGawkTime=-9\ncGawkTime=-9\n");
    assert!(lasted(&types_with(&negative), 50) <= 6);
    let huge = format!(
        "{plain}cGawkTime=2000000000\ncGawkTime=2000000000\ncGawkTime=2000000000\ncGawkTime=2000000000\ncGawkTime=2000000000\ncGawkAttrTime=2000000000\n"
    );
    let mut w = visit(50);
    let log = visit_run_in(&types_with(&huge), &mut w, 1500);
    assert!(log.iter().any(|l| l.starts_with("guest view 1 1")));
    assert!(
        !log.iter().any(|l| l.starts_with("goal 1 none")),
        "still viewing"
    );
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

#[test]
fn the_gawk_start_faces_the_viewing_direction_and_keeps_the_drawn_tile() {
    let (cat, walks) = (types(), walks());
    let mut w = visit(50);
    let (x, y) = centre((5, 5));
    w.place_unit(0, (x, y, 0), 0);
    let order = w.exhibits().list()[0].order.clone();
    let (mut rng, mut by_hand) = (Rng::default(), Rng::default());
    by_hand.rand();
    let drawn = order[by_hand.pick(order.len() as u32) as usize];
    let started = with_cx_in(&cat, &walks, &mut w, &mut rng, |cx| cx.gawk_init());
    assert!(started);
    // The lookup draw, then the draw of the tile; the set has no target.
    assert_eq!(rng.state(), {
        let mut r = Rng::default();
        r.rand();
        r.pick(order.len() as u32);
        // The duration of the gawk (5.3) follows; reading the attractiveness rates the exhibit
        // just made, whose reachability test draws its target (habitat rating 5.8).
        r.rand();
        r.rand();
        r.rand();
        r.state()
    });
    let Some(Part::Guest(g)) = w.units.get(1).map(|u| &u.part) else {
        panic!("a guest");
    };
    assert_eq!(g.seen, vec![drawn]);
    let (va, rotation) = (
        g.va.unwrap(),
        w.zoo.entities[0].common.as_ref().unwrap().rotation,
    );
    assert_ne!(va, 0, "the guest started facing 0");
    assert_eq!(rotation, u32::from(va));
}

#[test]
fn a_guest_that_feels_sick_draws_its_chance_is_sick_on_the_spot_and_wanders_on() {
    let sick = |chance: i32| {
        let man = format!(
            "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\ncSickChance={chance}\n"
        );
        let mut w = map(false);
        w.units.get_mut(1).unwrap().part = Part::Guest(Guest {
            sick: true,
            countdowns: [1000; 11],
            saw_animal: 1000,
            ..Guest::default()
        });
        let log = visit_run_in(&types_with(&man), &mut w, 60);
        let g = w.units.get(1).unwrap();
        (log, matches!(&g.part, Part::Guest(g) if g.sick))
    };
    let (log, still) = sick(100);
    for line in ["goal 1 sick 4 1 -", "set 1 bSick", "guest thought 1 10285"] {
        assert!(log.contains(&line.to_string()), "{line}: {log:?}");
    }
    // The flag stays up after the goal; only the surroundings lower it.
    assert!(still);
    let (log, _) = sick(0);
    assert!(!log.iter().any(|l| l.contains("sick")), "{log:?}");
}

/// The catalogue of `types()` with a statue of weight 60 whose bonus gives the man (9503) 8, and
/// two thoughts.
fn statued() -> Catalogue {
    statued_named(0)
}

/// `statued()` with the statue's name id `name_id`.
fn statued_named(name_id: i32) -> Catalogue {
    let man = "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\ncLeaveChanceDone=0\n";
    let mut cat = types_with(man);
    cat.push(crate::entity::EntityType {
        class: "building",
        type_name: b"statue".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(
                b"[Characteristics/Integers]\ncEstheticWeight=60\ncHelpID=70\n\
                  cUsedThought=10593\ncUsedThought=10594\n[EstheticBonus]\nv=9503\nv=8\n",
            )
            .unwrap(),
        )),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

/// Visit world with the zebra pen seen already (the guest's seen list holds its tile) and a statue
/// at (7, 7).
fn statue_visit() -> World {
    let mut w = visit_with(50, &[("building", "statue", 50, (7, 7))]);
    let pen = w.tile_index(2, 5).unwrap();
    if let Some(Part::Guest(g)) = w.units.get_mut(1).map(|u| &mut u.part) {
        g.seen = vec![pen];
    }
    w
}

#[test]
fn a_guest_walks_to_scenery_beside_it_gawks_at_it_thinks_and_has_seen_it() {
    let mut w = statue_visit();
    let log = visit_run_in(&statued(), &mut w, 500);
    let at = |text: &str| log.iter().position(|l| l.starts_with(text));
    let order: Vec<_> = [
        "goal 1 site 4",
        "goal 1 scenery 0 50 ",
        "goal 1 doscenery 0 50 -",
        "set 1 bGawkObject3",
        "guest thought 1 1059",
        "goal 1 none",
    ]
    .iter()
    .map(|t| at(t))
    .collect();
    assert!(
        order.iter().all(Option::is_some) && order.is_sorted(),
        "{order:?} {log:?}"
    );
    // It stands on a tile beside the statue, and the statue is in its seen list now.
    let line = &log[order[1].unwrap()];
    let place: Vec<i32> = (line.rsplit(' ').next().unwrap().split(','))
        .map(|n| n.parse().unwrap())
        .collect();
    let (dx, dy) = ((place[0] - 7).abs(), (place[1] - 7).abs());
    assert!(dx.max(dy) == 1, "{line}");
    assert!(
        log.contains(&format!("tile 1 {},{}", place[0], place[1])),
        "{log:?}"
    );
    let Some(Part::Guest(g)) = w.units.get(1).map(|u| &u.part) else {
        panic!("a guest");
    };
    assert!(g.seen.contains(&w.tile_index(7, 7).unwrap()));
    // One thought, and nothing is viewed at an exhibit.
    assert_eq!(
        log.iter()
            .filter(|l| l.starts_with("guest thought 1 1059"))
            .count(),
        1
    );
    assert!(!log.iter().any(|l| l.starts_with("guest view")), "{log:?}");
}

#[test]
fn the_choice_between_the_exhibit_and_the_scenery_is_one_draw_over_their_sites() {
    // One exhibit, the statue: weight 60 is one site. The guest has seen nothing.
    let (cat, walks) = (statued(), walks());
    let mut w = visit_with(50, &[("building", "statue", 50, (7, 7))]);
    crate::guest::gawk::update(&mut w, &cat);
    let mut rng = Rng::default();
    let mut by_hand = Rng::default();
    let r = by_hand.pick(2);
    with_cx_in(&cat, &walks, &mut w, &mut rng, |cx| cx.site_decide());
    let task = w.units.get(1).unwrap().goal.record.task;
    // r < H = 1 is the exhibit (task 0x65); else the scenery (0x80).
    assert_eq!(task, if r < 1 { 101 } else { 128 }, "{r}");
}

#[test]
fn user_scenery_is_gawked_at_and_thinks_nothing_but_the_draw_is_kept() {
    // Name 19000 is the first user-created name (localisation 8.2).
    let cat = statued_named(19000);
    let (mut w, mut rng, mut by_hand) = (statue_visit(), Rng::default(), Rng::default());
    w.units.get_mut(1).unwrap().goal.record.target.id = 50;
    by_hand.pick(2);
    assert!(with_cx_in(&cat, &walks(), &mut w, &mut rng, |cx| cx.do_scenery_init()));
    assert!(
        w.units
            .get(1)
            .unwrap()
            .runner
            .top()
            .is_some_and(|s| s.starts_with("bGawkObject"))
    );
    assert_eq!(rng.state(), by_hand.state());
    let lines = w.units.take_guest_lines();
    assert!(
        !lines.iter().any(|l| l.starts_with("thought ")),
        "{lines:?}"
    );
}

#[test]
fn scenery_that_is_gone_is_found_out_on_arrival_and_cannot_be_gawked_at() {
    let cat = statued();
    let mut w = statue_visit();
    let (mut s, mut money) = (sim(), Money::default());
    let mut log = Vec::new();
    let mut removed = false;
    for _ in 0..600 {
        // Removed as soon as the walk has started.
        if !removed
            && log
                .iter()
                .any(|l: &String| l.starts_with("goal 1 scenery 0 50 "))
        {
            removed = w.remove(50);
        }
        s.step_game(&walks(), &cat, &mut w, &mut money, 67, |_, _, _| {});
        log.extend(w.units.take_lines());
    }
    let at = |text: &str| log.iter().position(|l| l.starts_with(text));
    assert!(removed, "{log:?}");
    let line = &log[at("goal 1 scenery 0 50 ").unwrap()];
    let place = line.rsplit(' ').next().unwrap();
    // The goal ran on until the guest stood on the viewing tile; the gawk then failed its init.
    let (arrived, gawk, ended) = (
        at(&format!("tile 1 {place}")),
        at("goal 1 doscenery"),
        at("goal 1 none"),
    );
    assert!(
        arrived.is_some() && arrived < gawk && gawk < ended,
        "{log:?}"
    );
    assert!(at("set 1 bGawkObject").is_none(), "{log:?}");
    // The init of the gawk refuses an object that is gone, and no thought is given.
    let (mut w, mut rng) = (statue_visit(), Rng::default());
    w.units.get_mut(1).unwrap().goal.record.target.id = 999;
    assert!(!with_cx_in(&cat, &walks(), &mut w, &mut rng, |cx| cx.do_scenery_init()));
    assert_eq!(rng.state(), Rng::default().state());
    assert!(w.units.take_guest_lines().is_empty());
}

#[test]
fn the_guest_walks_to_the_viewing_tile_at_the_objects_offset_in_its_own_tile() {
    for (dx, dy) in [(0, 0), (8, 16), (-8, -24)] {
        let mut w = statue_visit();
        let at = w.find(50).unwrap();
        let c = w.zoo.entities[at].common.as_mut().unwrap();
        (c.x, c.y) = (c.x + dx, c.y + dy);
        let (x, y) = (c.x.rem_euclid(64), c.y.rem_euclid(64));
        let (mut rng, _) = stream_giving(8, 3);
        let (task, target) = scenery_choice(&mut w, &mut rng);
        let width = w.zoo.width as usize;
        let t = target.unwrap();
        assert_eq!(task, 128);
        let d = w.units.get(1).unwrap().destination.unwrap();
        assert_eq!(
            (d.0, d.1),
            ((t % width) as i32 * 64 + x, (t / width) as i32 * 64 + y)
        );
    }
}

/// A stream whose next `pick(n)` gives `v`, and the stream after that draw.
fn stream_giving(n: u32, v: u32) -> (Rng, Rng) {
    let mut rng = Rng::default();
    loop {
        let mut next = rng.clone();
        if next.pick(n) == v {
            return (rng, next);
        }
        rng.rand();
    }
}

/// Runs the scenery choice of guest 1 in `w` with `rng`; the task and the target tile it took.
fn scenery_choice(w: &mut World, rng: &mut Rng) -> (u8, Option<usize>) {
    let cat = statued();
    crate::guest::gawk::update(w, &cat);
    let list: Vec<_> = w.units.gawk.members().copied().collect();
    let here = w.tile_index(8, 2).unwrap();
    with_cx_in(&cat, &walks(), w, rng, |cx| cx.scenery_decide(&list, here));
    let r = &w.units.get(1).unwrap().goal.record;
    (r.task, r.target.tile)
}

#[test]
fn a_guest_draws_its_viewing_tile_among_the_ring_tiles_that_have_a_path() {
    let tile = |w: &World, x, y| w.tile_index(x, y).unwrap();
    // The statue at (7, 7); paths on (6, 7), (7, 8) and (8, 6) of its ring, listed x outer.
    let paths = [
        ("paths", "stone", 60, (6, 7)),
        ("paths", "stone", 61, (7, 8)),
    ];
    let more = [paths[0], paths[1], ("paths", "stone", 62, (8, 6))];
    let with = |extra: &[(&str, &str, u32, (i32, i32))]| {
        let mut all = vec![("building", "statue", 50, (7, 7))];
        all.extend_from_slice(extra);
        visit_with(50, &all)
    };
    let seen = |w: &mut World| {
        let pen = w.tile_index(2, 5).unwrap();
        if let Some(Part::Guest(g)) = w.units.get_mut(1).map(|u| &mut u.part) {
            g.seen = vec![pen];
        }
    };
    // The first and the last candidate of the list with a path, by the first and last draw; one
    // draw is spent each time, and the stream is then exactly one pick further.
    for (v, (x, y)) in [(0, (6, 7)), (1, (7, 8)), (2, (8, 6))] {
        let (mut rng, after) = stream_giving(3, v);
        let mut w = with(&more);
        seen(&mut w);
        let (task, target) = scenery_choice(&mut w, &mut rng);
        assert_eq!((task, target), (128, Some(tile(&w, x, y))), "draw {v}");
        assert_eq!(rng, after, "one draw");
    }
    // No path on the ring: the draw is over all eight tiles (the first (6, 6), the last (8, 8)).
    for (v, (x, y)) in [(0, (6, 6)), (7, (8, 8)), (4, (7, 8))] {
        let (mut rng, after) = stream_giving(8, v);
        let mut w = with(&[]);
        seen(&mut w);
        let (task, target) = scenery_choice(&mut w, &mut rng);
        assert_eq!((task, target), (128, Some(tile(&w, x, y))), "draw {v}");
        assert_eq!(rng, after);
    }
    // The drawn tile has no way to it (a pillar of rock): the draw is spent, nothing else
    // happens and no other tile is tried.
    let mut w = with(&[]);
    seen(&mut w);
    assert!(w.set_tile(6, 6, 30, 0, 0));
    let (mut rng, after) = stream_giving(8, 0);
    assert_eq!(scenery_choice(&mut w, &mut rng), (0, None));
    assert_eq!(rng, after);
}

/// The catalogue of `types()` with two kinds of attraction, rides of one help id (77) and a
/// carousel of another, which satisfy `fun` and have their entrance a tile above them.
fn rides() -> Catalogue {
    let text = |help| {
        format!(
            "[Characteristics/Integers]\ncCapacity=2\ncTimeInside=3\ncHelpID={help}\n\
             [Satisfies]\nbuilding\nfun\n[slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\n\
             entrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=2\n"
        )
    };
    let mut cat = types();
    for (name, help) in [("ride", 77), ("carousel", 78)] {
        cat.push(crate::entity::EntityType {
            class: "building",
            type_name: name.as_bytes().to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(text(help).as_bytes()).unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// Marks the zebra pen as seen by guest 1, with `count` sites counted.
fn saw_the_pen(w: &mut World, count: u32) {
    let pen = w.tile_index(2, 5).unwrap();
    if let Some(Part::Guest(g)) = w.units.get_mut(1).map(|u| &mut u.part) {
        (g.seen, g.seen_count) = (vec![pen], count);
    }
}

/// Runs the site decision of guest 1 with `rng`; the task and the building it took.
fn site_choice(w: &mut World, rng: &mut Rng) -> (u8, u32) {
    with_cx_in(&rides(), &walks(), w, rng, |cx| cx.site_decide());
    let r = &w.units.get(1).unwrap().goal.record;
    (r.task, r.target.id)
}

#[test]
fn the_site_draw_is_over_the_exhibits_and_the_attractions_alike() {
    // One exhibit, one ride: r = 0 is the exhibit (0x65), r = 1 the ride (0x66).
    for (v, want) in [(0, 101), (1, 102)] {
        let mut w = visit_with(50, &[("building", "ride", 50, (7, 3))]);
        let (mut rng, _) = stream_giving(2, v);
        assert_eq!(
            site_choice(&mut w, &mut rng),
            (want, if v == 1 { 50 } else { 0 })
        );
    }
}

#[test]
fn a_zoo_with_only_an_attraction_gives_the_guest_the_site_task() {
    let mut w = visit_with(50, &[("building", "ride", 50, (7, 3))]);
    w.remove(10);
    let mut rng = Rng::default();
    assert!(with_cx_in(&rides(), &walks(), &mut w, &mut rng, |cx| cx.next_site()));
    assert_eq!(w.units.get(1).unwrap().goal.record.task, 100);
}

#[test]
fn the_attractions_count_in_the_sites_a_guest_must_have_seen_before_it_forgets() {
    // The pen and a ride are two sites: one seen is not all of them, two are.
    for (count, forgets) in [(1, false), (2, true)] {
        let mut w = visit_with(50, &[("building", "ride", 50, (7, 3))]);
        saw_the_pen(&mut w, count);
        let (task, _) = site_choice(&mut w, &mut Rng::default());
        let Some(Part::Guest(g)) = w.units.get(1).map(|u| &u.part) else {
            panic!("a guest");
        };
        assert_eq!(
            (g.seen.is_empty(), task == 102),
            (forgets, !forgets),
            "{count}"
        );
    }
}

#[test]
fn a_ride_seen_counts_as_unseen_while_the_guest_holds_a_return_building() {
    // The ride is the only site left unseen; with a return building the pen is a site again.
    for (returning, n, v, want) in [(None, 1, 0, (101, 0)), (Some(7), 2, 1, (102, 50))] {
        let mut w = visit_with(50, &[("building", "ride", 50, (7, 3))]);
        let ride = w.tile_index(7, 3).unwrap();
        if let Some(Part::Guest(g)) = w.units.get_mut(1).map(|u| &mut u.part) {
            (g.seen, g.seen_count, g.returning) = (vec![ride], 1, returning);
        }
        let (mut rng, _) = stream_giving(n, v);
        assert_eq!(site_choice(&mut w, &mut rng), want, "{returning:?}");
    }
}

#[test]
fn an_attraction_is_the_nth_of_the_list_or_by_default_the_farthest_with_the_setting() {
    // Rides 50 at (7, 3) and 51 at (1, 9); the pen is seen, so r is the index in the list. The
    // guest stands at (8, 2): the second ride is the farther one.
    let more = [
        ("building", "ride", 50, (7, 3)),
        ("building", "ride", 51, (1, 9)),
    ];
    for (score, v, want) in [(false, 0, 50), (false, 1, 51), (true, 0, 51), (true, 1, 51)] {
        let mut w = visit_with(50, &more);
        saw_the_pen(&mut w, 1);
        w.units.attraction_score = score;
        let (mut rng, _) = stream_giving(2, v);
        assert_eq!(site_choice(&mut w, &mut rng), (102, want), "{score} {v}");
    }
}

#[test]
fn the_attraction_score_setting_is_on_for_any_value_but_0_and_off_when_missing() {
    for (value, on) in [
        ("", false),
        ("0", false),
        ("1", true),
        ("-1", true),
        ("99999999999", true),
    ] {
        let ini = format!("[AI]\nuseAttractionAttractiveness={value}\n");
        let (mut s, mut money) = (Sim::new(ini.as_bytes()), Money::default());
        s.set_seed(1);
        s.start_game(100, Clocks::default());
        let mut w = visit(50);
        s.step_game(&walks(), &types(), &mut w, &mut money, 67, |_, _, _| {});
        assert_eq!(w.units.attraction_score, on, "{value:?}");
    }
    let mut w = visit(50);
    let (mut s, mut money) = (Sim::new(b""), Money::default());
    s.step_game(&walks(), &types(), &mut w, &mut money, 67, |_, _, _| {});
    assert!(!w.units.attraction_score, "no ini");
}

#[test]
fn seeing_an_attraction_makes_every_one_of_its_help_id_seen() {
    let more = [
        ("building", "ride", 50, (7, 3)),
        ("building", "ride", 51, (7, 7)),
        ("building", "carousel", 52, (1, 9)),
    ];
    let (cat, mut w) = (rides(), visit_with(50, &more));
    let tile = w.tile_index(7, 3).unwrap();
    let mut guest = Guest::default();
    let ids = |w: &World, g: &Guest| {
        let (all, list) = crate::guest::view::attractions(w, &cat, g);
        (all, list.iter().map(|a| a.0).collect::<Vec<_>>())
    };
    assert_eq!(ids(&w, &guest), (3, vec![50, 51, 52]));
    guest.seen = vec![tile];
    assert_eq!(ids(&w, &guest), (3, vec![52]));
    w.remove(52);
    assert_eq!(ids(&w, &guest), (2, vec![]));
}

#[test]
fn a_thought_about_an_animal_is_about_its_exhibit_and_an_egg_is_the_egg_thought() {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entrance = (8, 5);
        for (id, at) in [(1, (2, 5)), (10, (2, 5)), (11, (2, 5)), (12, (7, 7))] {
            let class = if id == 1 { "guests" } else { "animals" };
            zoo.entities.push(entity(class, "man", id, centre(at), 0));
        }
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
    });
    w.units.get_mut(11).unwrap().part = Part::Animal(crate::animal::Animal {
        egg: true,
        ..Default::default()
    });
    let mut rng = Rng::default();
    // An animal in the pen: the pen is the exhibit; an egg changes the text; the grounds, an
    // object that is not an animal and no object leave the exhibit out.
    for (text, object) in [
        (10282, Some(10)),
        (10282, Some(11)),
        (10282, Some(12)),
        (10282, Some(77)),
        (10096, None),
    ] {
        with_cx(&mut w, &mut rng, |cx| cx.think(text, object));
    }
    let got: Vec<_> = w
        .units
        .thoughts
        .of(1)
        .iter()
        .map(|t| (t.text, t.exhibit))
        .collect();
    assert_eq!(
        got,
        [
            (10096, None),
            (10282, None),
            (10282, None),
            (10250, Some(1)),
            (10282, Some(1))
        ]
    );
    assert!(
        w.units.thoughts.of(1).iter().all(|t| t.gate.is_none()),
        "the pen has no gate"
    );
}
