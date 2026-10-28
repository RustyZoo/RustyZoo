//! The step driver, the generator and the event queue.

use super::*;
use crate::animal::Class;
use crate::money::Money;
use crate::zoo::{Clocks, Common, Entity, Tile, View, Zoo};
use drive::Run;

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
            ..Default::default()
        }),
    };
    let zoo = Zoo {
        awards: Vec::new(),
        kept: Default::default(),
        version: 0,
        locale: 0,
        expansion: 0,
        width: 4,
        height: 4,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![tile; 16],
        entities: (1..=entities as u32).map(entity).collect(),
        view: View::default(),
        clocks: Default::default(),
        names: Default::default(),
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
fn a_replay_seeds_with_the_seconds_of_the_log_and_then_the_clock() {
    let mut first = Sim::new(b"");
    first.start_game(16, Clocks::default());
    let told = first.take_lines();
    let seeds: Vec<u32> = told
        .iter()
        .map(|l| l.strip_prefix("seed ").unwrap().parse().unwrap())
        .collect();
    assert_eq!(seeds.len(), 2);
    let mut again = Sim::new(b"");
    again.set_seeds(seeds.clone());
    again.start_game(16, Clocks::default());
    assert_eq!(again.take_lines(), told);
    assert_eq!(again.rng.state(), first.rng.state());
    // No seconds left: the clock again, and an empty list draws nothing wrong.
    let mut none = Sim::new(b"");
    none.set_seeds(Vec::new());
    assert_eq!(none.take_lines().len(), 1);
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
    assert_eq!(none.variable_unit_speeds, 1);
    for (text, want) in [("0", 0), ("-1", -1), ("99999999999", 1_215_752_191)] {
        let ini = format!("[AI]\nVariableUnitSpeeds={text}\n");
        assert_eq!(
            Ai::from_ini(ini.as_bytes()).variable_unit_speeds,
            want,
            "{text}"
        );
    }
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
fn the_play_time_counts_the_steps_from_the_loaded_value() {
    let mut w = world(0);
    let mut s = sim("");
    // A fresh game and a map without the word start at 0; a step adds its milliseconds.
    s.start_game(16, Clocks::default());
    s.step(&mut w, 300, |_, _, _| {});
    s.step(&mut w, 0, |_, _, _| {});
    assert_eq!(s.clocks().play_time, 300);
    // A loaded value goes on from there, and a huge one stops at the end of the counter.
    for (loaded, step, want) in [(1000, 250, 1250), (u32::MAX - 1, 300, u32::MAX)] {
        s.start_game(
            16,
            Clocks {
                play_time: loaded,
                ..Clocks::default()
            },
        );
        s.step(&mut w, step, |_, _, _| {});
        assert_eq!(s.clocks().play_time, want);
    }
    // No step ran (a pause), no time passed.
    s.start_game(
        16,
        Clocks {
            play_time: 9,
            ..Clocks::default()
        },
    );
    assert_eq!(s.clocks().play_time, 9);
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
        ..Clocks::default()
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
fn the_path_finder_is_ticked_with_the_ai_clock() {
    let mut w = world(0);
    let mut s = sim("");
    run(&mut s, &mut w, 1, 67);
    assert_eq!(w.units.finder.state()[0], 5, "tick 0 is due at once");
    run(&mut s, &mut w, 80, 67);
    assert_eq!((s.ai_clock().0, w.units.finder.state()[0]), (5, 10));
}

#[test]
fn a_new_game_starts_with_the_event_queue_of_its_map() {
    let ev = |target, data| crate::zoo::Event {
        data,
        ..event(target, 0, UNIT_CHANNEL)
    };
    for next_is_first in [true, false] {
        let queue = crate::zoo::Queue {
            first: vec![ev(5, 1)],
            second: vec![ev(5, 2), ev(0, 3)],
            next_is_first,
        };
        let mut s = sim("");
        s.start_game(
            16,
            Clocks {
                queue,
                ..Clocks::default()
            },
        );
        // The buffer that is not "next" is read at the first step; the "next" one after it.
        let heard = |s: &Sim| {
            s.events
                .listen(5, UNIT_CHANNEL)
                .map(|e| e.data)
                .collect::<Vec<_>>()
        };
        let (now, later) = match next_is_first {
            true => (vec![2, 3], vec![1]),
            false => (vec![1], vec![2, 3]),
        };
        assert_eq!(heard(&s), now);
        s.events.age();
        assert_eq!(heard(&s), later);
    }
    // Another game replaces them.
    let mut s = sim("");
    s.start_game(16, Clocks::default());
    assert!(s.events.listen(5, UNIT_CHANNEL).next().is_none());
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
    // The generator was seeded again at that step, after the rating check the month flag forces,
    // and the month told.
    let lines = s.take_lines();
    let at = lines.iter().position(|l| l == "month 2 2001").unwrap();
    assert_eq!(lines[at - 1], "seed 3");
    assert!(lines[at - 2].starts_with("rating "), "{:?}", lines[at - 2]);
    assert_eq!(lines[at - 3], "step 1000");
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

fn own() -> (Catalogue, Class) {
    (Catalogue::default(), Class::default())
}

fn game(entities: usize) -> Run {
    Run::new((b"", b""), own(), world(entities), Money::default(), 1)
}

#[test]
fn the_hash_follows_fnv_1a_and_each_part_of_the_state() {
    // The published test vectors of 64-bit FNV-1a.
    let of = |bytes: &[u8]| {
        let mut h = hash::Fnv::default();
        h.put(bytes);
        h.finish()
    };
    assert_eq!(of(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(of(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(of(b"foobar"), 0x8594_4171_f739_67e8);

    let base = game(3).hash();
    assert_eq!(game(3).hash(), base, "the same state, the same hash");
    for what in [
        "tile",
        "terrain",
        "entity",
        "name",
        "unit",
        "goal",
        "cash",
        "generator",
        "clock",
        "grounds",
        "hire",
        "names",
        "timer",
        "rating",
        "sums",
        "given",
        "price",
        "column",
        "carried",
        "tank mode",
        "food",
        "bulldozed",
    ] {
        let mut run = game(3);
        match what {
            "tile" => assert!(run.world.set_tile(1, 1, 1, 0, 0)),
            "terrain" => assert!(run.world.set_tile(1, 1, 0, 0, 1)),
            "entity" => run.world.zoo.entities[1].common.as_mut().unwrap().x += 1,
            "name" => run.world.zoo.entities[2].common.as_mut().unwrap().name = "a".into(),
            "unit" => run.world.units.set_period(500),
            "goal" => run.world.units.get_mut(1).unwrap().goal.record.task = crate::goal::RANDOM,
            "cash" => run.money.cash += 0.01,
            "generator" => drop(run.sim.rng.rand()),
            "grounds" => run.world.exhibits_mut().grounds.rating_ms = 1,
            "hire" => run.sim.hire_male = !run.sim.hire_male,
            "names" => drop(run.world.zoo.names.insert("keeper".into(), 1)),
            "timer" => run.sim.status.timers[2] += 1,
            "rating" => run.sim.status.rating += 1,
            "sums" => run.sim.status.sums.sick += 1,
            "given" => run.sim.status.given += 1.0,
            "price" => run.sim.status.price += 0.25,
            "column" => run.sim.status.column += 1,
            "carried" => run.world.units.get_mut(1).unwrap().carried = true,
            "food" => run.world.zoo.entities[0].common.as_mut().unwrap().food = 1,
            "bulldozed" => run.sim.bulldozed.push(1),
            "tank mode" => {
                run.world.zoo.entities[0]
                    .common
                    .as_mut()
                    .unwrap()
                    .movement_mode = 3
            }
            _ => run.sim.ai_rem += 1,
        }
        assert_ne!(run.hash(), base, "{what}");
    }
    // Every field of a guest.
    let guest = |change: &dyn Fn(&mut crate::guest::Guest)| {
        let mut run = game(3);
        let mut g = crate::guest::Guest::default();
        change(&mut g);
        run.world.units.get_mut(1).unwrap().part = crate::unit::Part::Guest(g);
        run.hash()
    };
    let base = guest(&|_| {});
    let date = crate::timing::Calendar::new().date();
    let changes: [&dyn Fn(&mut crate::guest::Guest); 27] = [
        &|g| g.guide = 9,
        &|g| g.listening = true,
        &|g| g.values[3] = 1,
        &|g| g.values[0] = 1,
        &|g| g.pending[4] = 1,
        &|g| g.countdowns[10] = 1,
        &|g| g.saw_animal = 1,
        &|g| g.reset = 1,
        &|g| g.favourite = Some(0),
        &|g| g.female = true,
        &|g| g.child = true,
        &|g| g.arrived = Some(date),
        &|g| g.speed_offset = 1,
        &|g| g.sick = true,
        &|g| g.seen = vec![3],
        &|g| g.seen_count = 1,
        &|g| g.recent = vec![5003],
        &|g| g.va = Some(0),
        &|g| g.viewed_at = 1,
        &|g| g.items[0] = Some(6303),
        &|g| g.need = Some("food"),
        &|g| g.building = 7,
        &|g| g.slot = 1,
        &|g| g.entrance = (1, 2),
        &|g| g.stand = true,
        &|g| g.gift = true,
        &|g| g.littered = true,
    ];
    let hashes: Vec<u64> = changes.iter().map(|c| guest(c)).collect();
    assert!(hashes.iter().all(|&h| h != base), "{hashes:?}");
    let mut sorted = hashes.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), hashes.len(), "each field has its own place");
    // The leave flag of a guest.
    let leave = |on: bool| {
        let mut run = game(3);
        let g = crate::guest::Guest {
            leave: on,
            ..Default::default()
        };
        run.world.units.get_mut(1).unwrap().part = crate::unit::Part::Guest(g);
        run.hash()
    };
    assert_ne!(leave(true), leave(false));
    // Fixed-width fields: a name does not run into the next field.
    let mut a = game(2);
    a.world.zoo.entities[0].type_name = "ab".into();
    let mut b = game(2);
    b.world.zoo.entities[0].class = "animalsab".into();
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn the_hash_follows_the_live_state_of_a_building_and_a_hidden_user() {
    use crate::building::Live;
    let hash = |change: &dyn Fn(&mut Live)| {
        let mut run = game(3);
        let live = run.world.units.buildings.entry(7).or_default();
        change(live);
        run.hash()
    };
    let base = game(3).hash();
    let changes: [&dyn Fn(&mut Live); 10] = [
        &|l| l.users.push((1, 0, 5)),
        &|l| l.users.push((1, 1, 5)),
        &|l| l.users.push((1, 0, 6)),
        &|l| l.price = 2.0,
        &|l| l.guests[0] = 1,
        &|l| l.guests[2] = 1,
        &|l| l.income[1] = 1.0,
        &|l| l.upkeep = 1.0,
        &|l| l.fill = 13,
        &|l| l.built = 1,
    ];
    let mut hashes: Vec<u64> = changes.iter().map(|c| hash(c)).collect();
    assert!(hashes.iter().all(|&h| h != base));
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(hashes.len(), changes.len());
    let mut run = game(3);
    run.world.units.get_mut(1).unwrap().hidden = true;
    assert_ne!(run.hash(), base);
}

#[test]
fn the_hash_follows_the_thoughts_and_the_megatiles() {
    let hash = |change: &dyn Fn(&mut Run)| {
        let mut run = game(3);
        change(&mut run);
        run.hash()
    };
    let base = hash(&|_| {});
    let think = |text, object| move |r: &mut Run| r.world.units.think(1, text, object, None);
    let changes: [&dyn Fn(&mut Run); 6] = [
        &think(10079, None),
        &think(10082, None),
        &think(10243, Some(3)),
        &|r| {
            think(10079, None)(r);
            think(10082, None)(r);
        },
        &|r| crate::guest::mega::update(&mut r.world, &Catalogue::default(), 67),
        &|r| crate::guest::mega::update(&mut r.world, &Catalogue::default(), 7600),
    ];
    let mut hashes: Vec<u64> = changes.iter().map(|c| hash(c)).collect();
    assert!(hashes.iter().all(|&h| h != base));
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(
        hashes.len(),
        5,
        "both counts leave the clock at 0 and the same cells"
    );
}

#[test]
fn the_hash_follows_the_gawk_list() {
    let hash = |weight: i32| {
        let text = format!("[Characteristics/Integers]\ncEstheticWeight={weight}\n");
        let mut cat = Catalogue::default();
        cat.push(crate::entity::EntityType {
            class: "building",
            type_name: b"statue".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(std::sync::Arc::new(
                crate::config::Config::parse(text.as_bytes()).unwrap(),
            )),
            name_id: 0,
            footprint: [1, 1, 1],
            height: 0,
        });
        let mut run = game(3);
        let statue = crate::unit::testing::entity("building", "statue", 0, (96, 96), 0);
        run.world
            .add(statue, Some(crate::unit::testing::keeper_shape()));
        crate::guest::gawk::update(&mut run.world, &cat);
        run.hash()
    };
    // The same statue joins the list or does not by its weight.
    assert_ne!(hash(20), hash(0));
    assert_eq!(hash(20), hash(20));
}

#[test]
fn the_sex_of_the_first_hire_is_drawn_at_start_up_after_the_first_seeding_only() {
    // Bit 16 of the state after one draw from the seed: seed 2 gives a man, seed 3 a woman.
    let first = |seed: u32| {
        let mut rng = Rng::default();
        rng.seed(seed);
        rng.rand();
        (rng.state() >> 16) & 1 == 1
    };
    let sexes: Vec<_> = [2, 3].map(first).to_vec();
    assert_eq!(sexes, [true, false]);
    for seed in [2, 3] {
        let mut s = Sim::new(b"");
        s.set_seed(seed);
        assert_eq!(s.hire_male, first(seed), "{seed}");
        // Opening a game seeds the generator again and draws no sex.
        let before = s.hire_male;
        s.start_game(16, Clocks::default());
        assert_eq!(s.hire_male, before);
        let mut map = Rng::default();
        map.seed(seed + 1);
        assert_eq!(s.rng, map, "the map's seeding is the generator's state");
    }
    // A hire draws one more, bit 16 of the state set.
    let mut s = sim("");
    let mut check = s.rng.clone();
    s.draw_hire();
    let number = check.rand();
    assert_eq!(s.hire_male, (check.state() >> 16) & 1 == 1);
    assert_eq!(s.hire_male, number & 1 == 1);
    assert_eq!(s.rng, check, "one draw");
    // The seeds give both sexes.
    let all: Vec<_> = (0..40).map(first).collect();
    assert!(all.iter().any(|&d| d) && all.iter().any(|&d| !d), "{all:?}");
}

#[test]
fn a_run_gives_the_same_hash_twice_and_another_with_another_seed() {
    let hash = |seed| {
        let mut run = Run::new((b"", b""), own(), world(3), Money::default(), seed);
        (0..400).for_each(|_| drop(run.step(1000)));
        (run.hash(), run.money.month)
    };
    assert_eq!(hash(1), hash(1));
    assert_ne!(hash(1).0, hash(2).0);
    assert_eq!(
        hash(1).1,
        2,
        "the first month ended and the accounts moved on"
    );
}

#[test]
fn the_guide_command_puts_a_tour_guide_on_a_tile() {
    for bad in ["guide", "guide 3", "guide x,y", "guide 3,4 5", "guide 3,"] {
        assert!(bad.parse::<drive::Command>().is_err(), "{bad}");
    }
    assert_eq!("guide 3,4".parse(), Ok(drive::Command::Guide(3, 4)));
    let mut run = game(0);
    assert!(!run.apply(&drive::Command::Guide(1, 1)), "no such type");
    let ty = |class: &'static str| crate::entity::EntityType {
        class,
        type_name: b"tour".to_vec(),
        subtype: b"m".to_vec(),
        file: None,
        config: None,
        name_id: 9502,
        footprint: [1, 1, 1],
        height: 0,
    };
    run.cat.push(ty("tour"));
    assert!(!run.apply(&drive::Command::Guide(-1, 99)), "off the map");
    assert!(run.apply(&drive::Command::Guide(1, 1)));
    run.step(67);
    let unit = run.world.units.iter().next().unwrap();
    assert_eq!(unit.kind, crate::unit::Kind::Guide);
    let name = &run.world.zoo.entities[0].common.as_ref().unwrap().name;
    assert_eq!(name, "Guide 1");
}

#[test]
fn the_staff_animal_and_filter_commands_put_them_on_a_tile() {
    use drive::Command::{Animal, Filter, Guide, Keeper, Mood, Worker};
    for bad in [
        "keeper",
        "keeper 3",
        "worker x,y",
        "keeper 3,4 5",
        "animal",
        "animal mantaray",
        "animal 3,4",
        "animal mantaray 3,",
        "filter",
        "filter 3,4",
        "filter 3,4 8",
        "filter 3,4 -1",
        "filter 3,4 x",
        "filter 3,4 99999999999",
        "filter 3,4 2 1",
    ] {
        assert!(bad.parse::<drive::Command>().is_err(), "{bad}");
    }
    assert_eq!("keeper 3,4".parse(), Ok(Keeper(3, 4)));
    assert_eq!("worker -3,0".parse(), Ok(Worker(-3, 0)));
    assert_eq!(
        "animal mantaray 3,4".parse(),
        Ok(Animal("mantaray".into(), 3, 4, None))
    );
    assert_eq!("filter 3,4 7".parse(), Ok(Filter(3, 4, 7)));
    for (text, sex) in [("male", Some(false)), ("female", Some(true))] {
        let command = format!("animal mantaray 3,4 {text}");
        assert_eq!(command.parse(), Ok(Animal("mantaray".into(), 3, 4, sex)));
    }
    for bad in [
        "animal mantaray 3,4 x",
        "animal mantaray 3,4 Male",
        "animal mantaray 3,4 male 1",
        "mood",
        "mood 5",
        "mood 5 101",
        "mood 5 -101",
        "mood 5 x",
        "mood -5 1",
        "mood 5 99999999999",
        "mood 5 1 1",
    ] {
        assert!(bad.parse::<drive::Command>().is_err(), "{bad}");
    }
    assert_eq!("mood 5 -100".parse(), Ok(Mood(5, -100)));
    assert_eq!("mood 5 0".parse(), Ok(Mood(5, 0)));
    let mut run = game(0);
    // No such types: nothing changes.
    for c in [
        Keeper(1, 1),
        Worker(1, 1),
        Animal("mantaray".into(), 1, 1, None),
        Filter(1, 1, 0),
    ] {
        assert!(!run.apply(&c), "{c:?}");
    }
    let ty = |class: &'static str, name: &str, sub: &str| crate::entity::EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: sub.as_bytes().to_vec(),
        file: None,
        config: None,
        name_id: 1,
        footprint: [2, 2, 1],
        height: 0,
    };
    for t in [
        ty("keeper", "keeper", "m"),
        ty("keeper", "trainer", "m"),
        ty("maint", "maint", "m"),
        ty("tour", "guide", "m"),
        ty("animals", "mantaray", "m"),
        ty("filter", "filter1", "f"),
    ] {
        run.cat.push(t);
    }
    assert!(
        !run.apply(&Keeper(-1, 99)) && !run.apply(&Filter(-1, 99, 0)),
        "off the map"
    );
    assert!(
        !run.apply(&Animal("orca".into(), 1, 1, None)),
        "no such animal"
    );
    for c in [
        Guide(3, 2),
        Keeper(1, 1),
        Worker(2, 1),
        Animal("mantaray".into(), 3, 1, None),
        Filter(2, 2, 6),
    ] {
        assert!(run.apply(&c), "{c:?}");
    }
    run.step(67);
    let kinds: Vec<_> = run.world.units.iter().map(|u| u.kind).collect();
    use crate::unit::Kind;
    assert!(
        kinds.ends_with(&[Kind::Keeper, Kind::Worker, Kind::Animal]),
        "{kinds:?}"
    );
    // The keeper is the marine specialist, the filter has its health and a timer of 0.
    let entities = &run.world.zoo.entities;
    let marine = entities
        .iter()
        .find(|e| e.class == "keeper" && e.type_name == "trainer");
    assert!(marine.is_some_and(|e| e.common.as_ref().is_some_and(|c| c.name == "Keeper 1")));
    // The guide, the keeper and the worker keep the two colours their creation drew.
    for e in entities
        .iter()
        .filter(|e| matches!(e.class.as_str(), "tour" | "keeper" | "maint"))
    {
        assert_eq!(e.common.as_ref().map(|c| c.colors.len()), Some(2), "{e:?}");
    }
    let filter = entities
        .iter()
        .find(|e| e.class == "filter")
        .and_then(|e| e.common.as_ref());
    assert_eq!(filter.map(|c| c.rotation), Some(6));
    assert_eq!(filter.and_then(|c| c.filter).map(|f| f.health), Some(10));
}

#[test]
fn a_script_gives_an_animal_its_sex_and_its_mood() {
    use drive::Command::{Animal, Mood};
    let ty = |name: &str, sub: &str| crate::entity::EntityType {
        class: "animals",
        type_name: name.as_bytes().to_vec(),
        subtype: sub.as_bytes().to_vec(),
        file: None,
        config: None,
        name_id: 1,
        footprint: [2, 2, 1],
        height: 0,
    };
    let mut run = game(0);
    // A species with both sexes gives the subtype of each; one with only `m` gives a female
    // that subtype.
    for t in [ty("zebra", "f"), ty("zebra", "m"), ty("yak", "m")] {
        run.cat.push(t);
    }
    let asked = [
        ("zebra", Some(true)),
        ("zebra", Some(false)),
        ("yak", Some(true)),
    ];
    for (name, sex) in asked {
        assert!(run.apply(&Animal(name.into(), 1, 1, sex)), "{name}");
    }
    run.step(67);
    let animals: Vec<_> = (run.world.zoo.entities.iter())
        .filter(|e| e.class == "animals")
        .filter_map(|e| Some((e.common.as_ref()?.id, e.subtype.clone())))
        .collect();
    let sexes: Vec<_> = animals
        .iter()
        .map(|(id, sub)| match &run.world.units.get(*id).unwrap().part {
            crate::unit::Part::Animal(a) => (sub.as_str(), a.female),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(sexes, [("f", true), ("m", false), ("m", true)]);
    // The mood sets the happiness; a number outside -100 to 100 or a unit that is no animal
    // changes nothing.
    let (id, _) = animals[0].clone();
    assert!(run.apply(&Mood(id, -100)) && run.apply(&Mood(id, 100)));
    let crate::unit::Part::Animal(a) = &run.world.units.get(id).unwrap().part else {
        panic!("not an animal");
    };
    assert_eq!(a.happiness, 100);
    assert!(!run.apply(&Mood(9999, 5)), "no such unit");
}

#[test]
fn a_script_applies_its_commands_to_the_run() {
    let commands = drive::parse("# one\n\n3 cash -5.5\n0 tile 1 2 3 0 1\n3 remove 2\n").unwrap();
    let steps: Vec<u64> = commands.iter().map(|c| c.0).collect();
    assert_eq!(steps, [0, 3, 3], "sorted by step, the order of a step kept");
    let mut run = game(3);
    let t = run.world.tile_index(1, 2).unwrap();
    for (_, c) in &commands {
        assert!(run.apply(c));
    }
    assert_eq!(run.world.zoo.tiles[t].elevation, 3);
    assert_eq!(run.money.cash, -5.5);
    run.step(67);
    assert_eq!(run.world.zoo.entities.len(), 2, "removed by the next pass");
    assert!(
        !run.apply(&drive::Command::Tile(99, 0, 0, 0, 0)),
        "off the map"
    );
    assert!(!run.apply(&drive::Command::Fill(7)), "no tank");
    // A heal restores an animal's health; there is no animal 9.
    let sick = |run: &mut drive::Run| {
        let crate::unit::Part::Animal(a) = &mut run.world.units.get_mut(1).unwrap().part else {
            panic!("not an animal");
        };
        (a.health, a.at_minimum_health) = (0, true);
    };
    sick(&mut run);
    assert!(run.apply(&"heal 1".parse().unwrap()) && !run.apply(&drive::Command::Heal(9)));
    let crate::unit::Part::Animal(a) = &run.world.units.get(1).unwrap().part else {
        panic!("not an animal");
    };
    assert_eq!((a.health, a.at_minimum_health), (500, false));
    // The bulldozer deletes at once; there is no entity 99.
    assert_eq!("bulldoze 1".parse(), Ok(drive::Command::Bulldoze(1)));
    assert!(!run.apply(&drive::Command::Bulldoze(99)) && !run.apply(&drive::Command::Bulldoze(0)));
    assert!(run.apply(&drive::Command::Bulldoze(1)));
    assert_eq!(run.world.zoo.entities.len(), 1);
}

#[test]
fn a_bad_script_is_an_error() {
    for bad in [
        "x remove 1",
        "-1 remove 1",
        "1",
        "1 jump 1",
        "1 remove",
        "1 remove -1",
        "1 remove 4294967296",
        "1 bulldoze",
        "1 bulldoze -1",
        "1 bulldoze 1 2",
        "1 bulldoze 4294967296",
        "1 cash nan",
        "1 cash inf",
        "1 cash",
        "1 tile 0 0 256 0 0",
        "1 tile 0 0 -1 0 0",
        "1 tile 0 0 0 256 0",
        "1 tile 0 0 0 0 256",
        "1 tile 0 99999999999 0 0 0",
        "1 tile 0 0 0 0",
        "1 fill 1 2",
        "1 heal",
        "1 heal -1",
        "1 heal 1 2",
        "1 food 1,1",
        "1 food 1 herbchow",
        "1 food a,b herbchow",
        "1 food 1,1 herbchow -1",
        "1 food 1,1 herbchow 99999999999",
        "1 food 1,1 herbchow 5 6",
        "1 leave",
        "1 leave -1",
        "1 leave 4294967296",
        "1 leave 1 2",
    ] {
        assert!(drive::parse(bad).is_err(), "{bad}");
    }
    for good in [
        "0 tile -3 4 255 255 255",
        "0 food -3,4 herbchow",
        "0 food 1,1 herbchow 0",
    ] {
        assert!(
            drive::parse(good).is_ok(),
            "{good}: off the map is a warning"
        );
    }
}

#[test]
fn the_grounds_update_before_the_exhibits() {
    let mut w = world(0);
    let part = crate::exhibit::Part {
        tiles: vec![5],
        grounds: false,
    };
    let id = w.exhibits_mut().add(&part, String::new());
    let mut s = sim("");
    let mut check = s.rng.clone();
    s.step(&mut w, 1000, |_, _, _| {});
    // Both start dirty: each restarts its rating timer with one draw, the grounds' first.
    let draws = [check.rand() % 200, check.rand() % 200];
    assert_eq!(w.exhibits().grounds.rating_ms, draws[0]);
    assert_eq!(w.exhibits().get(id).unwrap().rating.rating_ms, draws[1]);
    assert_eq!(s.rng.state(), check.state());
    // At the eighth second both timers of each pass their limits: two draws each, again the
    // grounds' first (the species timer, then the rating timer).
    (0..7).for_each(|_| _ = s.step(&mut w, 1000, |_, _, _| {}));
    let draws = [(); 4].map(|()| check.rand() % 200);
    assert_eq!(w.exhibits().grounds.species_ms, draws[0]);
    assert_eq!(w.exhibits().grounds.rating_ms, draws[1]);
    assert_eq!(w.exhibits().get(id).unwrap().rating.species_ms, draws[2]);
    assert_eq!(w.exhibits().get(id).unwrap().rating.rating_ms, draws[3]);
    assert_eq!(s.rng.state(), check.state());
}

#[test]
fn the_month_end_rating_check_belongs_to_the_old_month_and_the_accounts_then_move_on() {
    let (mut w, mut s, mut money) = (world(0), sim(""), Money::default());
    let mut step = || {
        s.step_game(
            &NoData,
            &Catalogue::default(),
            &mut w,
            &mut money,
            1000,
            |_, _, _| {},
        )
    };
    let ended = (0..373).filter(|_| step().month_end).count();
    assert_eq!((ended, money.month), (1, 2));
    // The forced check wrote 24 (12 and 12 for the empty zoo) into January, not February.
    assert_eq!(money.monthly[crate::money::RATING][..2], [24.0, 0.0]);
}

#[test]
fn the_endowment_is_drawn_and_booked_at_the_month_end_in_the_old_month() {
    let (mut w, mut s, mut money) = (world(0), sim(""), Money::default());
    money.total[crate::money::MEMBERS] = 3.0;
    let mut step = || {
        s.step_game(
            &NoData,
            &Catalogue::default(),
            &mut w,
            &mut money,
            1000,
            |_, _, _| {},
        )
    };
    (0..373).for_each(|_| _ = step());
    let lines = s.take_lines();
    let at = lines.iter().position(|l| l.starts_with("month 2")).unwrap();
    let after: Vec<&str> = lines[at + 1..].iter().map(String::as_str).collect();
    let members: Vec<&String> = lines.iter().filter(|l| l.starts_with("member")).collect();
    // The rating of the empty zoo is 24: every draw above 34 leaves.
    assert_eq!(members.len(), 3, "{lines:?}");
    assert!(
        lines.iter().any(|l| l.starts_with("endowment ")),
        "{lines:?}"
    );
    // They come after the seeding and the month line, and before the first booking of the new month.
    assert!(lines.iter().position(|l| l.starts_with("member")) > Some(at));
    assert!(after.iter().any(|l| l.starts_with("endowment ")));
    let left = members.iter().filter(|l| l.ends_with("leaves")).count() as f32;
    use crate::money::{ENDOWMENT, MEMBERS, MEMBERS_LEFT};
    assert_eq!(money.monthly[MEMBERS_LEFT][0], left);
    assert_eq!(money.monthly[MEMBERS][0], -left);
    assert!(money.monthly[ENDOWMENT][0] > 0.0 || left == 3.0);
    assert_eq!(
        money.monthly[ENDOWMENT][1], 0.0,
        "booked in the month that ended"
    );
    assert_eq!(money.total[MEMBERS], 3.0 - left);
}

#[test]
fn a_game_takes_the_escape_keys_of_the_ini_and_a_scenario_economy_limits_the_price() {
    let s = sim("[AI]\ncEscapedAnimalChange=33\ncEscapedAnimalTime=4\n");
    let settings = &s.status.settings;
    assert_eq!((settings.escaped_change, settings.escaped_time), (33, 4));
    // The set-up check keeps them.
    let mut run = game(0);
    run.sim.status.settings.escaped_change = 33;
    run.sim
        .new_game(&mut run.world, &run.cat, &mut run.money, false);
    assert_eq!(run.sim.status.settings.escaped_change, 33);
    // The scenario's file comes over the settings and the price follows its range.
    let mut s = sim("");
    s.status.price = 70.0;
    s.scenario_economy(b"[characteristics]\ncMaxAdultAdmissionPrice=25\ncDonationChance=7\n");
    assert_eq!(
        (s.status.price, s.status.settings.donation_chance),
        (25.0, 7)
    );
    // A minimum above the maximum swaps the two.
    s.scenario_economy(b"[characteristics]\ncMinAdultAdmissionPrice=30\n");
    assert_eq!(
        (s.status.settings.min_price, s.status.settings.max_price),
        (25.0, 30.0)
    );
}

#[test]
fn a_new_game_starts_the_timers_again_and_runs_the_set_up_check() {
    let mut run = game(0);
    let first = run.sim.take_lines();
    // The set-up check ran with the built-in settings, no cash and the research done.
    assert_eq!(first.last().unwrap(), "rating 29");
    assert_eq!(run.sim.status.rating, 29);
    let mut again = game(0);
    again.sim.status.timers = [9, 9, 9];
    again.sim.status.given = 2.0;
    again
        .sim
        .new_game(&mut again.world, &again.cat, &mut again.money, false);
    assert_eq!(
        (again.sim.status.timers, again.sim.status.given),
        ([0; 3], 0.0)
    );
}

#[test]
fn a_load_ends_with_a_rating_check_that_restarts_only_the_rating_timer() {
    let mut run = game(0);
    run.sim.status.rating = 34;
    run.sim.status.timers = [3000, 7, 9];
    run.money.cash = 5000.0;
    run.sim.loaded(&run.world, &run.cat, &mut run.money);
    // 12 + 12 for the empty zoo and 1 for 5000 of cash; the research counts 0 points.
    assert_eq!(run.sim.status.rating, 25);
    assert_eq!(run.sim.status.timers, [0, 7, 9]);
    assert_eq!(run.money.total[crate::money::RATING], 25.0);
}

#[test]
fn the_research_share_of_the_checks_is_the_trees_and_a_game_without_one_counts_0() {
    use crate::research::{Branch, Category, Program, Tree};
    let program = |cost| Program {
        name: 1,
        cost,
        progress: 0.0,
        target: 0,
        effect: 0,
        order: 0,
    };
    let tree = |programs| Tree {
        branches: vec![Branch {
            name: 1,
            funding: vec![],
            level: 0,
            current: None,
            categories: vec![Category {
                name: 1,
                expansion: 0,
                tick: true,
                programs,
            }],
        }],
        ..Tree::default()
    };
    let mut run = game(0);
    run.sim.loaded(&run.world, &run.cat, &mut run.money);
    assert_eq!(run.sim.status.sums.research, 0);
    // One of four programs is free: 25, and an empty tree is 100.
    run.sim.set_research(tree(vec![
        program(0.0),
        program(1.0),
        program(1.0),
        program(9.0),
    ]));
    run.sim.loaded(&run.world, &run.cat, &mut run.money);
    assert_eq!(run.sim.status.sums.research, 25);
    run.sim.set_research(tree(vec![]));
    run.sim.loaded(&run.world, &run.cat, &mut run.money);
    assert_eq!(run.sim.status.sums.research, 100);
    // The next game starts without a tree.
    let clocks = run.world.zoo.clocks.clone();
    run.sim.start_game(run.world.zoo.tiles.len(), clocks);
    assert_eq!(run.sim.research_share(), 0);
}

#[test]
fn the_research_manager_pays_every_3_seconds_and_hands_over_the_programs_it_completes() {
    use crate::research::{Branch, Category, Done, Level, Program, Tree};
    let program = Program {
        name: 7,
        cost: 0.5,
        progress: 0.0,
        target: 5,
        effect: 0,
        order: 1,
    };
    let tree = Tree {
        branches: vec![Branch {
            name: 1,
            funding: vec![Level {
                name: 1,
                cost: 120.0,
                work: 100.0,
            }],
            level: 0,
            current: Some((0, 0)),
            categories: vec![Category {
                name: 1,
                expansion: 0,
                tick: true,
                programs: vec![program],
            }],
        }],
        ..Tree::default()
    };
    let mut run = game(0);
    run.money.set_cash(1000.0);
    run.sim.set_research(tree);
    run.step(2999);
    assert_eq!(run.money.total[crate::money::RESEARCH], 0.0);
    let lines = run.step(1);
    assert!(lines.contains(&"sim research done 7".into()));
    assert_eq!(
        run.money.total[crate::money::RESEARCH],
        360.0_f32 * 120.0 * 2.314_814_8e-5
    );
    let done = Done {
        name: 7,
        target: 5,
        effect: 0,
    };
    assert_eq!(run.sim.take_research_done(), [done]);
    assert!(run.sim.take_research_done().is_empty());
}

#[test]
fn an_exhibit_whose_animal_count_falls_to_0_loses_its_viewing_time() {
    let mut w = world(0);
    let part = |tile| crate::exhibit::Part {
        tiles: vec![tile],
        grounds: false,
    };
    let (emptied, kept) = (
        w.exhibits_mut().add(&part(5), String::new()),
        w.exhibits_mut().add(&part(10), String::new()),
    );
    for (id, counts) in [(emptied, (1, 1)), (kept, (1, 0))] {
        let x = w.exhibits_mut().get_mut(id).unwrap();
        x.stats.viewing = 7;
        x.rating.counts = counts;
    }
    let mut s = sim("");
    s.step(&mut w, 1000, |_, _, _| {});
    // The count fell from 1 to 0: its time is cleared. The other had no animal: it keeps its time.
    assert_eq!(w.exhibits().get(emptied).unwrap().stats.viewing, 0);
    assert_eq!(w.exhibits().get(kept).unwrap().stats.viewing, 7);
}

#[test]
fn the_hash_follows_the_statistics_of_an_exhibit() {
    let hash = |change: &dyn Fn(&mut crate::exhibit::Stats)| {
        let mut w = world(0);
        let part = crate::exhibit::Part {
            tiles: vec![5],
            grounds: false,
        };
        let id = w.exhibits_mut().add(&part, String::new());
        change(&mut w.exhibits_mut().get_mut(id).unwrap().stats);
        let run = Run::new((b"", b""), own(), w, Money::default(), 1);
        run.hash()
    };
    let base = hash(&|_| {});
    let changes: [&dyn Fn(&mut crate::exhibit::Stats); 5] = [
        &|s| s.popularity = 1,
        &|s| s.viewing = 1,
        &|s| s.donations[1] = 1.0,
        &|s| s.donations[2] = 1.0,
        &|s| s.upkeep[0] = 1.0,
    ];
    let mut hashes: Vec<u64> = changes.iter().map(|c| hash(c)).collect();
    assert!(hashes.iter().all(|&h| h != base));
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(hashes.len(), changes.len());
}

#[test]
fn the_hash_follows_what_a_keeper_and_his_exhibit_keep() {
    let hash = |change: &dyn Fn(&mut Run)| {
        let mut w = world(0);
        let shape = crate::world::Shape {
            kind: crate::render::order::Kind::Unit,
            footprint: (1, 1),
            fence: false,
            wall: false,
            gate: false,
            gated: false,
            tank: crate::world::TankKind::NONE,
            walkable: (false, false),
        };
        let keeper = crate::unit::testing::entity("keeper", "", 7, (32, 32), 0);
        w.add(keeper, Some(shape)).unwrap();
        let part = crate::exhibit::Part {
            tiles: vec![5],
            grounds: false,
        };
        w.exhibits_mut().add(&part, String::new());
        let mut run = Run::new((b"", b""), own(), w, Money::default(), 1);
        change(&mut run);
        run.hash()
    };
    fn staff(run: &mut Run) -> &mut crate::unit::Staff {
        let unit = run.world.units.iter().map(|u| u.id).next().unwrap();
        match &mut run.world.units.get_mut(unit).unwrap().part {
            crate::unit::Part::Staff(s) => s,
            _ => panic!("not staff"),
        }
    }
    let base = hash(&|_| {});
    let changes: [&dyn Fn(&mut Run); 10] = [
        &|r| staff(r).members.push(3),
        &|r| staff(r).crowd = 4,
        &|r| staff(r).leg = true,
        &|r| staff(r).lost_tiles.push(3),
        &|r| staff(r).lost_animals.push(3),
        &|r| staff(r).food_kind = 1,
        &|r| staff(r).assigned.push(1),
        &|r| r.world.exhibits_mut().list_mut()[0].rating.coming = 1,
        &|r| r.world.exhibits_mut().list_mut()[0].rating.serviced = 1,
        &|r| r.world.units.service_interval = 7,
    ];
    let mut hashes: Vec<u64> = changes.iter().map(|c| hash(c)).collect();
    assert!(hashes.iter().all(|&h| h != base));
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(hashes.len(), changes.len());
}

/// A game saved and opened again as Continue has the tiles, the entities and the units it had.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn a_saved_game_opens_as_it_was_saved() {
    use crate::{exhibit::TankConfig, place, terrain, vfs::Vfs, world::Decay};
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = Vfs::open(std::path::Path::new(&dir)).unwrap();
    let cat = Catalogue::load(&vfs).unwrap();
    let start = |bytes: &[u8], saved: bool| {
        let cat = Catalogue::load(&vfs).unwrap();
        let (zoo, money) = crate::zoo::Zoo::parse_money(bytes)
            .and_then(|(zoo, money)| Ok((zoo.checked(&cat)?, money)))
            .unwrap();
        let renderer = crate::render::MapRenderer::new(&vfs, &cat);
        let mut world = World::new(zoo, |e| renderer.shape(e));
        world.set_water(place::water(&terrain::types(&vfs)));
        world.set_tank_config(TankConfig::load(&vfs));
        world.set_decay(Decay::load(&vfs));
        let files = (vfs.ini(), &[][..]);
        let types = (cat, Class::load(&vfs));
        match saved {
            false => Run::start(files, types, world, money, 1, false),
            true => Run::resume(files, types, world, money, 1),
        }
    };
    // The size of a record is a version's, not the game's: the saved file is of version 106.
    let same = |a: &[crate::zoo::Entity], b: &[crate::zoo::Entity]| {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| {
                x.body_size.is_none() == y.body_size.is_none() && x.common == y.common
            })
    };
    for member in [
        "maps/default.zoo",
        "maps/airport.zoo",
        "maps/med_aqua.zoo",
        "maps/scn28.zoo",
        "saved games/test.zoo",
    ] {
        let Ok(bytes) = vfs.read(member) else {
            continue; // the saved game is in the English install only
        };
        let run = start(&bytes, false);
        let saved = run.save().unwrap_or_else(|e| panic!("{member}: {e}"));
        let back = start(&saved, true);
        let (a, b) = (&run.world.zoo, &back.world.zoo);
        assert!(a.tiles == b.tiles, "{member}: tiles");
        // The entities as the save writes them: a load clamps the life and the mate delay
        // (animals 3.5), so the loaded map's raw records are not the saved ones.
        let mut written = run.world.saved(|_| true);
        run.world.finish_save(&mut written, &cat);
        assert!(same(&written.entities, &b.entities), "{member}: entities");
        // A counter at 0 is not saved (it means the same as none).
        let named = |z: &crate::zoo::Zoo| {
            (z.names.iter())
                .filter(|&(_, &n)| n > 0)
                .map(|(k, &n)| (k.clone(), n))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            (a.id_counter, named(a)),
            (b.id_counter, named(b)),
            "{member}"
        );
        assert_eq!(
            run.world.units.hash(),
            back.world.units.hash(),
            "{member}: units"
        );
        assert_eq!(
            run.world.units.thoughts, back.world.units.thoughts,
            "{member}: thoughts"
        );
        assert!(
            run.world.exhibits() == back.world.exhibits(),
            "{member}: exhibits"
        );
        // Saving the opened game again changes none of that.
        let twice = start(&back.save().unwrap(), true);
        assert!(
            same(&twice.world.zoo.entities, &b.entities),
            "{member}: second save"
        );
        assert_eq!(
            back.world.units.hash(),
            twice.world.units.hash(),
            "{member}"
        );
    }
}

#[test]
fn award_add_gives_a_signed_id_to_the_zoo() {
    let ok = |text: &str| text.parse::<drive::Command>();
    assert_eq!(ok("award add 8"), Ok(drive::Command::Award(8)));
    assert_eq!(ok("award add 0"), Ok(drive::Command::Award(0)));
    assert_eq!(ok("award add -1"), Ok(drive::Command::Award(-1)));
    assert_eq!(
        ok("award add 2147483647"),
        Ok(drive::Command::Award(i32::MAX))
    );
    for bad in [
        "award",
        "award add",
        "award add x",
        "award add 2147483648",
        "award add 1 2",
        "award 8",
        "award remove 8",
    ] {
        assert!(bad.parse::<drive::Command>().is_err(), "{bad:?}");
    }
    let mut run = game(3);
    for id in [1, 8, 1] {
        assert!(run.apply(&drive::Command::Award(id)));
    }
    assert_eq!(run.world.zoo.awards, [8, 1], "no duplicate, largest first");
}

#[test]
fn save_and_load_take_a_file_name_to_the_end_of_the_line() {
    let ok = |text: &str| drive::parse(text).unwrap().remove(0).1;
    assert_eq!(ok("0 save out.zoo"), drive::Command::Save("out.zoo".into()));
    assert_eq!(
        ok("3 load  my zoo.zoo "),
        drive::Command::Load("my zoo.zoo".into())
    );
    for bad in ["0 save", "0 load   ", "0 saved x", "0 loader"] {
        assert!(drive::parse(bad).is_err(), "{bad:?}");
    }
}

/// A zoo finished for writing: units and buildings made in play take the values of a new one from
/// their type, a record of a transient type keeps no body, and a record that was read is left alone.
#[test]
fn a_finished_zoo_has_new_values_and_no_transient_body() {
    use crate::entity::EntityType;
    let ty = |class: &'static str, name: &str, ini: &str| EntityType {
        class,
        type_name: name.into(),
        subtype: "m".into(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(ini.as_bytes()).unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let mut run = game(0);
    let ints = "[Characteristics/Integers]\n";
    run.cat.push(ty(
        "animals",
        "zebra",
        &format!("{ints}cInitialHappiness=75\ncMaxHits=120\n"),
    ));
    run.cat.push(ty(
        "animals",
        "odd",
        &format!("{ints}cInitialHappiness=-5\ncMaxHits=2000000000\n"),
    ));
    run.cat.push(ty("animals", "bare", ""));
    run.cat.push(ty(
        "guests",
        "man",
        &format!("{ints}cInitialHappiness=50\ncMaxEnergy=100\n"),
    ));
    run.cat.push(ty("guests", "bare", ""));
    run.cat.push(ty(
        "building",
        "shop",
        "[Characteristics/Floats]\ncDefaultCost=2.5\n",
    ));
    run.cat.push(ty("building", "bare", ""));
    run.cat
        .push(ty("objects", "ripple", &format!("{ints}cIsTransient=1\n")));
    run.cat
        .push(ty("fences", "wall", &format!("{ints}cIsTransient=1\n")));
    let made = |class: &str, name: &str| Entity {
        class: class.into(),
        type_name: name.into(),
        subtype: "m".into(),
        body_size: None,
        common: Some(Common::default()),
    };
    let names = [
        ("animals", "zebra"),
        ("animals", "odd"),
        ("animals", "bare"),
        ("guests", "man"),
        ("guests", "bare"),
        ("building", "shop"),
        ("building", "bare"),
        ("objects", "ripple"),
        ("fences", "wall"),
    ];
    for (class, name) in names {
        assert!(run.world.add(made(class, name), None).is_some());
    }
    let mut zoo = run.world.saved(|_| true);
    run.world.finish_save(&mut zoo, &run.cat);
    let c = |i: usize| zoo.entities[i].common.as_ref();
    let animal = |i: usize| {
        c(i).and_then(|c| c.animal.as_deref())
            .map(|a| (a.happiness, a.hunger, a.health, a.energy))
    };
    assert_eq!(animal(0), Some((75, 0, 120, 0)));
    assert_eq!(animal(1), Some((-5, 0, 2_000_000_000, 0)));
    assert_eq!(
        animal(2),
        Some((60, 0, 500, 0)),
        "the defaults of a type that names none"
    );
    let guest = |i: usize| c(i).and_then(|c| c.guest.as_deref()).map(|g| g.values);
    assert_eq!(guest(3), Some([50, 0, 0, 0, 0]));
    assert_eq!(guest(4), Some([40, 0, 0, 0, 0]));
    let price = |i: usize| {
        c(i).and_then(|c| c.building.as_ref())
            .map(|b| f32::from_bits(b.price))
    };
    assert_eq!(price(5), Some(2.5));
    assert_eq!(price(6), Some(run.world.units.prices.default));
    assert!(c(7).is_none(), "a transient type keeps no body");
    assert!(c(8).is_some(), "a fence keeps its figures");
}
