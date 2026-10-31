//! A tour guide's legs, talks and group on a synthetic map: a zebra pen with a path beside it.

use super::*;
use crate::behaviour::{Call, Data, Heard, Number, Row, Timing, parse_call};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::goal::Goal;
use crate::guest::Guest;
use crate::money::Money;
use crate::sim::{Events, Rng, Sim};
use crate::unit::testing::{centre, entity, rates, world_with};
use crate::unit::{Ctx, Staff};
use crate::world::World;
use crate::zoo::Clocks;
use std::collections::HashMap;
use std::sync::Arc;

const GUIDE: u32 = 2;

fn ty(class: &'static str, name: &str, sub: &str, id: i32, keys: &str) -> EntityType {
    let head = match sub.is_empty() {
        true => "[Characteristics/Integers]".to_string(),
        false => format!("[{sub}/Characteristics/Integers]"),
    };
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(
            Config::parse(format!("{head}\n{keys}\n").as_bytes()).unwrap(),
        )),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// The guide's keys: a gathering at every status tick of the tiles within two, everybody follows;
/// the lines of `over` replace the key they name.
fn catalogue(over: &str) -> Catalogue {
    let mut cat = Catalogue::default();
    let man = "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\ncHungerThreshold=70\n";
    let own: Vec<&str> = over.lines().collect();
    let key = |l: &&str| l.split('=').next().unwrap_or_default().to_string();
    let mut tour = String::new();
    for line in [
        "cTourGuideBonus=30",
        "cCrowdCheck=1",
        "cCrowdRadius=2",
        "cFollowChance=100",
    ] {
        if !own.iter().any(|o| key(o) == key(&line)) {
            tour = format!("{tour}{line}\n");
        }
    }
    let tour = format!("{tour}{over}");
    cat.push(ty("guests", "man", "", 9503, man));
    cat.push(ty("animals", "zebra", "", 5, ""));
    cat.push(ty("tour", "tour", "m", 9502, &tour));
    cat
}

/// A 10 by 10 map: a zebra pen round (2, 5), a path along x = 5 from y = 2 to 8, the guide on it at
/// (5, 2) and the guests with the ids of `at` standing still on the tiles of `at`.
fn zoo(at: &[(u32, (i32, i32))]) -> World {
    let mut w = world_with((10, 10), |zoo| {
        for i in 0..3 {
            for (tile, d) in [
                ((1 + i, 4), 0),
                ((1 + i, 6), 4),
                ((1, 4 + i), 6),
                ((3, 4 + i), 2),
            ] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(tile), d));
            }
        }
        for y in 2..9 {
            let path = entity("paths", "stone", 200 + y as u32, centre((5, y)), 0);
            zoo.entities.push(path);
        }
        zoo.entities
            .push(entity("animals", "zebra", 10, centre((2, 5)), 0));
        let mut guide = entity("tour", "tour", GUIDE, centre((5, 2)), 0);
        guide.subtype = "m".into();
        zoo.entities.push(guide);
        for &(id, tile) in at {
            let guest = entity("guests", "man", id, centre(tile), 0);
            zoo.entities.push(guest);
        }
    });
    let zebra = w.units.get_mut(10).unwrap();
    zebra.part = Part::Animal(crate::animal::Animal::default());
    (zebra.started, zebra.repositioning) = (true, true);
    for &(id, _) in at {
        let guest = w.units.get_mut(id).unwrap();
        guest.part = Part::Guest(Guest {
            countdowns: [1000; 11],
            saw_animal: 1000,
            ..Guest::default()
        });
        // A task without a goal: the guest stands.
        guest.goal.record.task = 134;
    }
    w
}

struct Sets(HashMap<String, Vec<Call>>, [Row; 1]);

fn sets() -> Sets {
    let call = |c: &str| vec![parse_call(c.as_bytes()).unwrap().0];
    let both = [call("fPlaySet(bSpeak1)"), call("fPlaySet(bSpeak2)")].concat();
    let list = [
        ("bWalk", call("fWalk(0,0)")),
        ("bGawk", call("fKeepControl()")),
        ("bZooRoutine", call("fKeepControl()")),
        ("bSpeak", both),
        ("bSpeak1", call("fPlayTime(speak,1)")),
        ("bSpeak2", call("fPlayTime(speak2,1)")),
    ];
    let routine = Row {
        lo: -100,
        hi: 100,
        set: "bZooRoutine".into(),
    };
    Sets(
        list.into_iter().map(|(n, c)| (n.into(), c)).collect(),
        [routine],
    )
}

impl Data for Sets {
    fn set(&self, _: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        self.0.get(name).map(Vec::as_slice)
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[Row] {
        &self.1
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let count = match name {
            "idle" | "walk" | "trot" | "run" => 1,
            "speak" | "speak2" => 4,
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

struct Game {
    w: World,
    sim: Sim,
    money: Money,
    cat: Catalogue,
    data: Sets,
    log: Vec<String>,
}

impl Game {
    fn new(keys: &str, at: &[(u32, (i32, i32))]) -> Game {
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(100, Clocks::default());
        Game {
            w: zoo(at),
            sim,
            money: Money::default(),
            cat: catalogue(keys),
            data: sets(),
            log: Vec::new(),
        }
    }

    fn run(&mut self, steps: usize) {
        for _ in 0..steps {
            let (w, money) = (&mut self.w, &mut self.money);
            let update = |_: &mut World, _, _| {};
            self.sim
                .step_game(&self.data, &self.cat, w, money, 67, update);
            let units = w
                .units
                .take_lines()
                .into_iter()
                .map(|l| format!("unit {l}"));
            let guests = w
                .units
                .take_guest_lines()
                .into_iter()
                .map(|l| format!("guest {l}"));
            let staff = w
                .units
                .take_staff_lines()
                .into_iter()
                .map(|l| format!("staff {l}"));
            self.log.extend(units.chain(guests).chain(staff));
        }
    }

    fn lines(&self, text: &str) -> Vec<&str> {
        let found = self.log.iter().filter(|l| l.contains(text));
        found.map(String::as_str).collect()
    }

    fn guide(&self) -> &Staff {
        match &self.w.units.get(GUIDE).unwrap().part {
            Part::Staff(s) => s,
            _ => panic!("not staff"),
        }
    }

    fn guest(&self, id: u32) -> &Guest {
        match &self.w.units.get(id).unwrap().part {
            Part::Guest(g) => g,
            _ => panic!("not a guest"),
        }
    }
}

#[test]
fn a_guide_leads_a_group_to_guests_on_a_viewing_area_talks_and_releases_it_at_the_next_leg() {
    let mut g = Game::new("", &[(1, (5, 5)), (3, (5, 6)), (4, (8, 8))]);
    g.run(3000);
    let at = |text: &str| g.log.iter().position(|l| l.starts_with(text));
    let order = [
        "staff job 2 va",
        "guest follow 1 2",
        "staff job 2 speak",
        "guest follow 1 0",
    ];
    let found: Vec<_> = order.iter().map(|t| at(t)).collect();
    assert!(
        found.iter().all(Option::is_some) && found.is_sorted(),
        "{found:?} {:?}",
        (g.log.iter())
            .filter(|l| l.contains("staff") || l.contains("follow"))
            .collect::<Vec<_>>()
    );
    // The guest far off the guide's tiles is never asked.
    assert!(g.lines("guest follow 4 ").is_empty());
    // Two runs agree.
    let hash = |g: &mut Game| {
        g.run(1);
        g.w.units.hash()
    };
    let mut again = Game::new("", &[(1, (5, 5)), (3, (5, 6)), (4, (8, 8))]);
    again.run(3000);
    assert_eq!(hash(&mut g), hash(&mut again));
}

#[test]
fn a_guide_with_nobody_on_a_viewing_area_walks_at_random() {
    let mut g = Game::new("", &[]);
    // The next leg starts at step 164.
    g.run(150);
    assert_eq!(
        g.lines("staff job 2"),
        ["staff job 2 va", "staff job 2 random"]
    );
    assert!(g.lines("guest follow").is_empty());
}

/// Runs `f` on the goal update context of unit `id`.
fn with_cx<R>(g: &mut Game, id: u32, f: impl FnOnce(&mut Cx) -> R) -> R {
    let at = g.w.find(id).unwrap();
    let mut u = g.w.units.get(id).unwrap().clone();
    let (events, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &g.cat,
    };
    let mut cx = Cx {
        unit: &mut u,
        world: &mut g.w,
        ctx: &mut ctx,
        at,
        data: &g.data,
    };
    let r = f(&mut cx);
    *g.w.units.get_mut(id).unwrap() = u;
    r
}

/// The guide at (5, 5) with its target tile set and a task, three guests round it.
fn crowd(keys: &str, task: u8) -> Game {
    let near = [(1, (4, 4)), (3, (5, 5)), (4, (6, 6)), (5, (9, 9))];
    let mut g = Game::new(keys, &near);
    let at = g.w.find(GUIDE).unwrap();
    let (x, y) = centre((5, 5));
    g.w.place_unit(at, (x, y, 0), 0);
    let tile = Some(5 * 10 + 5);
    let world = &g.w;
    let mut target = crate::goal::Target::default();
    target.set_tile(world, tile);
    let record = &mut g.w.units.get_mut(GUIDE).unwrap().goal.record;
    (record.task, record.target) = (task, target);
    g
}

#[test]
fn a_gathering_offers_the_guests_of_the_square_in_scan_order_up_to_the_group_size() {
    for (keys, members) in [
        ("", vec![1, 3, 4]),
        ("cMaxGroupSize=2", vec![1, 3]),
        ("cMaxGroupSize=0", vec![]),
        ("cMaxGroupSize=-4", vec![]),
        ("cFollowChance=0", vec![]),
        ("cCrowdRadius=0", vec![3]),
        ("cCrowdRadius=-1", vec![]),
        ("cCrowdRadius=2000000000", vec![1, 3, 4, 5]),
    ] {
        let mut g = crowd(keys, TALK);
        with_cx(&mut g, GUIDE, |cx| cx.gather());
        assert_eq!(g.guide().members, members, "{keys}");
        for m in &members {
            assert_eq!(g.guest(*m).guide, GUIDE);
        }
    }
}

#[test]
fn a_guest_that_is_busy_in_a_pressing_need_or_in_another_group_is_not_taken() {
    let mut g = crowd("", LEG);
    g.w.units.get_mut(1).unwrap().goal.record.task = 107;
    if let Part::Guest(guest) = &mut g.w.units.get_mut(3).unwrap().part {
        guest.values[1] = 70;
    }
    if let Part::Guest(guest) = &mut g.w.units.get_mut(4).unwrap().part {
        guest.guide = 77;
    }
    with_cx(&mut g, GUIDE, |cx| cx.gather());
    assert_eq!(g.guide().members, Vec::<u32>::new());
    // Without a target tile, or with another task, the guide offers nothing.
    let mut g = crowd("", 0);
    with_cx(&mut g, GUIDE, |cx| cx.gather());
    assert!(g.guide().members.is_empty());
}

#[test]
fn a_release_resets_every_follower_with_one_draw_each_and_empties_the_list() {
    let mut g = crowd("", TALK);
    with_cx(&mut g, GUIDE, |cx| cx.gather());
    assert_eq!(g.guide().members.len(), 3);
    g.w.units.take_guest_lines();
    let mut draws = Rng::default();
    let rng = with_cx(&mut g, GUIDE, |cx| {
        cx.release();
        cx.ctx.rng.state()
    });
    (0..3).for_each(|_| {
        draws.rand();
    });
    assert_eq!(rng, draws.state());
    assert!(g.guide().members.is_empty());
    for id in [1, 3, 4] {
        assert_eq!(
            (
                g.guest(id).guide,
                g.w.units.get(id).unwrap().goal.record.task
            ),
            (0, 0)
        );
    }
    let said = g.w.units.take_guest_lines();
    assert_eq!(said.len(), 3, "{said:?}");
    assert!(
        said.iter()
            .all(|l| l.starts_with("follow ") && l.ends_with(" 0"))
    );
}

#[test]
fn a_guest_that_arrives_while_the_guide_talks_gets_the_bonus_of_its_area_once() {
    let mut g = crowd("", TALK);
    // The guest on the path looks at the pen in some direction.
    let tile = 5 * 10 + 5;
    let cat = catalogue("");
    let dir = (0..8).find(|&k| crate::guest::view::sight(&mut g.w, &cat, tile, k).is_some());
    let guest = g.w.units.get_mut(3).unwrap();
    guest.goal.record.task = LEG;
    if let Part::Guest(guest) = &mut guest.part {
        (guest.guide, guest.va) = (GUIDE, dir);
    }
    g.w.units.get_mut(GUIDE).unwrap().goal.record.done = false;
    // The guide stands on the same tile: its bonus is the area's.
    with_cx(&mut g, 3, |cx| Goal::Follow.complete(cx));
    assert_eq!(
        (g.guest(3).pending[HAPPINESS], g.guest(3).listening),
        (30, true)
    );
    with_cx(&mut g, 3, |cx| Goal::Follow.complete(cx));
    assert_eq!(g.guest(3).pending[HAPPINESS], 30, "once");
    // A guide that does not talk gives nothing; one that is gone lets the guest go.
    for (task, guide, gone) in [(LEG, GUIDE, false), (TALK, 99, true)] {
        let mut g = crowd("", task);
        let guest = g.w.units.get_mut(3).unwrap();
        guest.goal.record.task = LEG;
        if let Part::Guest(guest) = &mut guest.part {
            (guest.guide, guest.va) = (guide, dir);
        }
        with_cx(&mut g, 3, |cx| Goal::Follow.complete(cx));
        assert_eq!(g.guest(3).pending[HAPPINESS], 0);
        assert_eq!(g.w.units.get(3).unwrap().goal.record.task == 0, gone);
    }
}

#[test]
fn the_view_of_an_area_under_a_guide_carries_its_bonus_into_the_donation() {
    let mut g = crowd("", TALK);
    let tile = 5 * 10 + 5;
    let sight = |g: &mut Game| {
        let see = |g: &mut Game, k| crate::guest::view::sight(&mut g.w, &g.cat, tile, k);
        let dir = (0..8).find(|&k| see(g, k).is_some());
        see(g, dir.unwrap()).unwrap()
    };
    assert_eq!(sight(&mut g).bonus, 30);
    // A guide with no bonus, or one picked up, gives none.
    let mut none = crowd("cTourGuideBonus=0", TALK);
    assert_eq!(sight(&mut none).bonus, 0);
    g.w.units.get_mut(GUIDE).unwrap().carried = true;
    assert_eq!(sight(&mut g).bonus, 0);
}

#[test]
fn the_countdown_of_the_gathering_runs_at_each_status_tick_and_a_leg_gathers_only_while_moving() {
    let mut g = crowd("cCrowdCheck=4", TALK);
    if let Part::Staff(s) = &mut g.w.units.get_mut(GUIDE).unwrap().part {
        s.crowd = 2;
    }
    let mut counts = Vec::new();
    for _ in 0..4 {
        with_cx(&mut g, GUIDE, |cx| cx.crowd_check());
        counts.push((g.guide().crowd, g.guide().members.len()));
    }
    // 2, 1, 0 count down; at 0 the next tick restarts at 3 and gathers.
    assert_eq!(counts, [(1, 0), (0, 0), (3, 3), (2, 3)]);
    // A leg gathers once the guide moves.
    for (moving, members) in [(false, 0), (true, 3)] {
        let mut g = crowd("", LEG);
        g.w.units.get_mut(GUIDE).unwrap().moving = moving;
        with_cx(&mut g, GUIDE, |cx| {
            cx.guide_state(|s| s.crowd = 0);
            cx.crowd_check();
        });
        assert_eq!(g.guide().members.len(), members, "moving {moving}");
    }
}

#[test]
fn a_guide_picked_up_lets_go_of_its_group_and_the_guests_notice() {
    let mut g = crowd("", TALK);
    with_cx(&mut g, GUIDE, |cx| cx.gather());
    assert_eq!(g.guide().members.len(), 3);
    let unit = g.w.units.get_mut(GUIDE).unwrap();
    (unit.carried, unit.goal.record.task) = (true, TALK);
    crate::staff::leaves(&mut g.w, GUIDE);
    assert!(g.guide().members.is_empty() && !g.guide().leg);
    g.w.units.get_mut(3).unwrap().goal.record.task = LEG;
    with_cx(&mut g, 3, |cx| Goal::Follow.complete(cx));
    assert_eq!(
        (g.guest(3).guide, g.w.units.get(3).unwrap().goal.record.task),
        (0, 0)
    );
}

#[test]
fn a_walk_set_started_again_during_a_walk_does_not_end_the_walk() {
    let mut g = Game::new("", &[]);
    // The walk set reaches its move call through a nested set: one step after it starts.
    let call = |c: &str| vec![parse_call(c.as_bytes()).unwrap().0];
    g.data.0.insert("bWalk".into(), call("fPlaySet(bWalkland)"));
    g.data.0.insert("bWalkland".into(), call("fWalk(0,0)"));
    let walking = |g: &Game| {
        let u = g.w.units.get(GUIDE).unwrap();
        u.walking && u.path.len() > 2
    };
    let mut steps = 0;
    while !walking(&g) && steps < 1000 {
        g.run(1);
        steps += 1;
    }
    assert!(walking(&g));
    assert!(with_cx(&mut g, GUIDE, |cx| cx.start_walk(false, false, true)));
    g.run(3);
    assert!(walking(&g));
}
