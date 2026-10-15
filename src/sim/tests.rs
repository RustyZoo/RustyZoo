//! The step driver, the generator and the event queue.

use super::*;
use crate::zoo::{Clocks, Common, Entity, Tile, View, Zoo};

fn world(entities: usize) -> World {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let entity = |id: u32| Entity {
        class: "animals".into(),
        type_name: String::new(),
        subtype: String::new(),
        body_size: None,
        common: Some(Common {
            x: 0,
            y: 0,
            z: 0,
            rotation: 0,
            id,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
            movement_mode: 0,
        }),
    };
    let zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: 4,
        height: 4,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; 16],
        entities: (1..=entities as u32).map(entity).collect(),
        view: View::default(),
        clocks: Default::default(),
    };
    World::new(zoo, |_| None)
}

fn sim(ini: &str) -> Sim {
    let mut sim = Sim::new(ini.as_bytes());
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    sim
}

fn run(sim: &mut Sim, world: &mut World, steps: u32, step: u32) {
    for _ in 0..steps {
        sim.step(world, step, |_, _, _| {});
    }
}

#[test]
fn the_generator_follows_the_c_runtime_after_seed_1() {
    let mut rng = Rng::default();
    rng.seed(1);
    assert_eq!([rng.rand(), rng.rand(), rng.rand()], [41, 18467, 6334]);
    assert_eq!([rng.rand(), rng.rand()], [26500, 19169]);
    // The state of a program that never seeded is the state after seed 1.
    let mut fresh = Rng::default();
    assert_eq!(fresh.rand(), 41);
    // 32-bit wrap: a state near the top stays inside the 15 bits.
    let mut rng = Rng::default();
    rng.seed(u32::MAX);
    assert!((0..1000).all(|_| rng.rand() <= 0x7fff));
}

#[test]
fn chance_and_pick_reduce_as_the_original_does() {
    let rng = || {
        let mut rng = Rng::default();
        rng.seed(1);
        rng
    };
    // The numbers are 41, 18467, 6334: modulo 100 they are 41, 67, 34.
    let mut r = rng();
    assert_eq!(
        [r.chance(42), r.chance(67), r.chance(35)],
        [true, false, true]
    );
    assert_eq!(r.state(), {
        let mut s = rng();
        (0..3).for_each(|_| _ = s.rand());
        s.state()
    });
    // Out-of-range percents still draw once.
    let mut r = rng();
    assert!(!r.chance(0) && !r.chance(-5) && r.chance(100) && r.chance(i32::MAX));
    assert_eq!(r.state(), {
        let mut s = rng();
        (0..4).for_each(|_| _ = s.rand());
        s.state()
    });
    let mut r = rng();
    assert_eq!([r.pick(15), r.pick(1), r.pick(7)], [41 % 15, 0, 6334 % 7]);
    // pick(0) gives 0 and draws nothing.
    let mut r = rng();
    assert_eq!(r.pick(0), 0);
    assert_eq!(r.rand(), 41);
    assert_eq!(rng().pick(u32::MAX), 41);
}

#[test]
fn the_seconds_are_the_seed_plus_the_count() {
    let mut s = Seconds::Test { seed: 7, n: 0 };
    assert_eq!([s.now(), s.now(), s.now()], [7, 8, 9]);
    let mut s = Seconds::Test {
        seed: u32::MAX,
        n: 0,
    };
    assert_eq!([s.now(), s.now()], [u32::MAX, 0]);
    assert!(Seconds::Clock.now() > 1_700_000_000);
}

#[test]
fn a_test_run_seeds_at_the_three_moments() {
    let mut s = Sim::new(b"");
    s.start_game(16, Clocks::default());
    s.set_seed(100);
    assert_eq!(s.take_lines(), ["seed 100", "seed 101"]);
    // The same map size does not seed again; another one does.
    s.start_game(16, Clocks::default());
    assert!(s.take_lines().is_empty());
    s.start_game(25, Clocks::default());
    assert_eq!(s.take_lines(), ["seed 102"]);
    // Without a game, the seed alone is the first moment.
    let mut s = Sim::new(b"");
    s.set_seed(5);
    s.start_game(16, Clocks::default());
    assert_eq!(s.take_lines(), ["seed 5", "seed 6"]);
}

#[test]
fn the_ai_keys_have_their_fallbacks() {
    let none = Ai::from_ini(b"");
    assert_eq!(
        (
            none.ai_tick,
            none.status_period,
            none.clear_unreachable_freq
        ),
        (1000, 1000, 5)
    );
    assert_eq!((none.min_unreachable_count, none.max_guests), (5, 1000));
    assert!(none.allow_diagonal_moves && none.animated_water);
    assert_eq!(
        (none.heuristic, none.map_size, none.node_max),
        (1.0, 20, 500)
    );
    assert_eq!(Ai::from_ini(b"[user]\nmAITick=7\n"), none, "other sections");
    let ai = Ai::from_ini(
        b"[ai]\nMAITICK=500\nmStatusPeriod=2000\nAnimalResetInterval=-3\nGuestResetInterval=9\n\
          AllowDiagonalMoves=2\nanimatedwater=0\nMinUnreachableCount=300\nHeuristic=1.5\n\
          maxGuests=abc\nMapSize= 30x\n",
    );
    assert_eq!((ai.ai_tick, ai.status_period), (500, 2000));
    assert_eq!((ai.animal_reset_interval, ai.guest_reset_interval), (0, 9));
    assert!(!ai.allow_diagonal_moves && !ai.animated_water);
    assert_eq!((ai.min_unreachable_count, ai.heuristic), (44, 1.5));
    assert_eq!((ai.max_guests, ai.map_size), (1000, 30));
    // 0 would divide by zero: refused. Negative reads as the huge unsigned number.
    assert_eq!(Ai::from_ini(b"[AI]\nmAITick=0\n").ai_tick, 1000);
    assert_eq!(Ai::from_ini(b"[AI]\nmAITick=-1\n").ai_tick, u32::MAX);
    assert_eq!(
        Ai::from_ini(b"[AI]\nmAITick=\n").ai_tick,
        1000,
        "unreadable"
    );
}

#[test]
fn the_ai_clock_keeps_whole_ticks_and_the_remainder() {
    let mut w = world(0);
    let mut s = sim("");
    run(&mut s, &mut w, 15, 67);
    assert_eq!(s.ai_clock(), (1, 5));
    run(&mut s, &mut w, 15, 67);
    assert_eq!(s.ai_clock(), (2, 10));
    assert_eq!(s.total_ms(), 30 * 67);
    let mut s = sim("[AI]\nmAITick=500\n");
    run(&mut s, &mut w, 15, 67);
    assert_eq!(s.ai_clock(), (2, 5));
    let mut s = sim("");
    run(&mut s, &mut w, 5, 200);
    assert_eq!(s.ai_clock(), (1, 0), "the cap: one tick every five steps");
    // A tick longer than the whole run never fires, a step of 0 changes nothing.
    let mut s = sim("[AI]\nmAITick=-1\n");
    run(&mut s, &mut w, 3, 67);
    assert_eq!(s.ai_clock(), (0, 201));
    run(&mut s, &mut w, 3, 0);
    assert_eq!(s.ai_clock(), (0, 201));
}

#[test]
fn a_new_game_starts_from_the_clocks_of_its_map() {
    let mut w = world(0);
    let mut s = sim("");
    let clocks = Clocks {
        ai_rem: 704,
        ai_ticks: 425,
        world_rem: 702,
        world_secs: 272,
    };
    s.start_game(16, clocks);
    assert_eq!((s.ai_clock(), s.world_clock()), ((425, 704), (272, 702)));
    // The first step goes on from there: 704 + 300 ms is one more tick, 702 + 300 one more second.
    s.step(&mut w, 300, |_, _, _| {});
    assert_eq!((s.ai_clock(), s.world_clock()), ((426, 4), (273, 2)));
    // The total time and the calendar start again, the clocks of another map replace these.
    assert_eq!(s.total_ms(), 300);
    s.start_game(16, Clocks::default());
    assert_eq!(
        (s.ai_clock(), s.world_clock(), s.total_ms()),
        ((0, 0), (0, 0), 0)
    );
}

#[test]
fn the_zoo_status_chance_draws_nothing_at_0() {
    let state = |f: &dyn Fn(&mut Rng) -> bool| {
        let mut r = Rng::default();
        let answer = f(&mut r);
        (answer, r.state())
    };
    let fresh = Rng::default().state();
    assert_eq!(
        state(&|r| r.status_chance(0)),
        (false, fresh),
        "nothing drawn"
    );
    let mut drawn = Rng::default();
    drawn.rand();
    // Any other percent draws once; 41 % 100 is 41.
    assert_eq!(state(&|r| r.status_chance(42)), (true, drawn.state()));
    assert_eq!(state(&|r| r.status_chance(41)), (false, drawn.state()));
    assert_eq!(state(&|r| r.status_chance(100)), (true, drawn.state()));
    assert_eq!(state(&|r| r.status_chance(i32::MAX)), (true, drawn.state()));
    assert_eq!(
        state(&|r| r.status_chance(-5)),
        (false, drawn.state()),
        "draws, no"
    );
    assert_eq!(
        state(&|r| r.status_chance(i32::MIN)),
        (false, drawn.state())
    );
}

#[test]
fn the_world_clock_counts_seconds() {
    let mut w = world(0);
    let mut s = sim("");
    run(&mut s, &mut w, 15, 67);
    assert_eq!(s.world_clock(), (1, 5));
    run(&mut s, &mut w, 1, 200);
    assert_eq!(s.world_clock(), (1, 205));
}

#[test]
fn the_unreachable_list_is_cleared_every_freq_ticks() {
    let mut w = world(0);
    let mut s = sim("[AI]\nClearUnreachableFreq=2\n");
    // Tick 0 is already due on the first step (the session value starts at 0).
    let cleared: Vec<_> = (0..70)
        .map(|_| s.step(&mut w, 67, |_, _, _| {}).clear_unreachable)
        .collect();
    let at: Vec<_> = (0..70).filter(|&i| cleared[i]).collect();
    // Ticks 0, 2 and 4 are reached at steps 0, 29 and 59 (a tick every 14.9 steps).
    assert_eq!(at, [0, 29, 59]);
    // A negative or huge frequency wraps and does not panic.
    for freq in ["-1", "2147483647"] {
        let mut s = sim(&format!("[AI]\nClearUnreachableFreq={freq}\n"));
        run(&mut s, &mut w, 40, 67);
    }
}

#[test]
fn the_month_flag_is_acted_on_one_step_late() {
    let mut w = world(0);
    let mut s = sim("");
    // January is 31 days, 6 minutes 12 seconds of play: 372 s.
    let step = 1000;
    let mut month_end = Vec::new();
    for i in 0..400 {
        let report = s.step(&mut w, step, |_, _, _| {});
        if report.month_end {
            month_end.push((i, s.calendar().date().month));
        }
    }
    // The step at 372 s changes the month (index 371), the next one acts on it.
    assert_eq!(s.calendar().date().month, 2);
    assert_eq!(month_end, [(372, 2)]);
    // The generator was seeded again at that step, and the month told.
    let lines = s.take_lines();
    let at = lines.iter().position(|l| l == "month 2 2001").unwrap();
    assert_eq!(lines[at - 1], "seed 3");
    assert_eq!(lines[at - 2], "step 1000");
    assert_eq!(lines.iter().filter(|l| l.starts_with("seed ")).count(), 3);
}

#[test]
fn the_entity_count_is_taken_after_the_removal_pass_and_before_the_loop() {
    let mut w = world(4);
    let mut s = sim("");
    let id = |w: &World, at: usize| w.zoo.entities[at].common.as_ref().unwrap().id;
    s.remove(2);
    s.remove(2);
    s.remove(77);
    let mut seen = Vec::new();
    s.step(&mut w, 67, |w, at, step| {
        seen.push((id(w, at), step));
        // An entity added during the loop is first updated at the next step.
        if at == 0 {
            let mut e = w.zoo.entities[0].clone();
            e.common.as_mut().unwrap().id = 50;
            w.zoo.entities.push(e);
        }
    });
    assert_eq!(seen, [(1, 67), (3, 67), (4, 67)]);
    assert_eq!(w.zoo.entities.len(), 4);
    assert!(!w.can_undo(), "the removal is not an undoable action");
    seen.clear();
    s.step(&mut w, 67, |w, at, _| seen.push((id(w, at), 0)));
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[3].0, 50);
}

fn event(target: u32, delay: u16, channel: u8) -> Event {
    Event {
        sender: 9,
        target,
        data: 0,
        extra: 0,
        kind: 0x2711,
        delay,
        sender_channel: UNIT_CHANNEL,
        channel,
    }
}

#[test]
fn an_event_is_delivered_at_the_step_its_delay_says() {
    let mut q = Events::default();
    // Posted at step 100 with delays 0, 1 and 3.
    for delay in [0, 1, 3] {
        q.post(event(0, delay, UNIT_CHANNEL));
    }
    assert_eq!(q.listen(5, UNIT_CHANNEL).count(), 0, "not in the same step");
    q.age();
    let heard = |q: &Events| {
        q.listen(5, UNIT_CHANNEL)
            .map(|e| e.delay)
            .collect::<Vec<_>>()
    };
    assert_eq!(heard(&q), [0, 1], "step 101");
    q.age();
    assert!(heard(&q).is_empty(), "step 102");
    q.age();
    assert_eq!(heard(&q), [1], "step 103: the delay of 3 is now below 2");
    q.age();
    assert!(heard(&q).is_empty(), "dropped");
}

#[test]
fn events_go_to_the_matching_listeners_only_and_are_not_consumed() {
    let mut q = Events::default();
    q.post(event(5, 0, UNIT_CHANNEL));
    q.post(event(0, 0, UNIT_CHANNEL));
    q.post(event(6, 0, UNIT_CHANNEL));
    q.post(event(5, 0, 0x4b));
    q.age();
    assert_eq!(q.listen(5, UNIT_CHANNEL).count(), 2);
    assert_eq!(q.listen(5, UNIT_CHANNEL).count(), 2, "read twice");
    assert_eq!(q.listen(6, UNIT_CHANNEL).count(), 2);
    assert_eq!(q.listen(5, 0x4b).count(), 1);
    q.clear();
    assert_eq!(q.listen(5, UNIT_CHANNEL).count(), 0);
}

#[test]
fn an_event_posted_by_the_entity_loop_is_heard_at_the_next_step() {
    let mut w = world(1);
    let mut s = sim("");
    s.step(&mut w, 67, |_, _, _| {});
    // Posted by the world manager after the swap (here: between steps): heard two steps later.
    s.events.post(event(0, 1, UNIT_CHANNEL));
    let mut heard = Vec::new();
    for _ in 0..3 {
        let mut count = 0;
        let ev = &s.events;
        count += ev.listen(1, UNIT_CHANNEL).count();
        heard.push(count);
        s.step(&mut w, 67, |_, _, _| {});
    }
    assert_eq!(heard, [0, 1, 0]);
}

#[test]
fn two_runs_with_one_seed_print_the_same_lines() {
    let lines = |seed| {
        let mut w = world(3);
        let mut s = Sim::new(b"");
        s.set_seed(seed);
        s.start_game(16, Clocks::default());
        run(&mut s, &mut w, 20, 1000);
        s.take_lines()
    };
    assert_eq!(lines(1), lines(1));
    assert_eq!(lines(1)[..2], ["seed 1", "seed 2"]);
    assert_ne!(lines(1), lines(2));
}
