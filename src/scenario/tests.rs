use super::*;

fn build(main: &str, extras: &[&str]) -> Scenario {
    let parse = |t: &str| Config::parse(t.as_bytes()).unwrap();
    let extras: Vec<Config> = extras.iter().map(|t| parse(t)).collect();
    Scenario::build("s.scn", &parse(main), &extras)
}

fn eval(rule: &[i32; 4], e: &Env) -> i32 {
    rules::eval(rule, 0, e, &[])
}

fn env(month: i32, year: i32) -> Env<'static> {
    Env::new(month, year, None)
}

/// One pass of the manager.
fn pass(s: &mut Scenario, e: &Env, log: &mut Vec<String>) {
    s.update(PERIOD, e, log);
}

fn states(s: &Scenario) -> Vec<u8> {
    s.goals.iter().map(|g| g.state).collect()
}

/// A simple goal on rule 99: `arga` is the value read.
fn goal(name: &str, read: i32, kind: i32, value: i32, extra: &str) -> String {
    format!("[{name}]\nrulea=99\narga={read}\ntype={kind}\nvalue={value}\n{extra}\n")
}

#[test]
fn the_nine_types_compare_signed_and_other_types_leave_the_state() {
    let table = [
        // (type, rule value, state): at the value, below it, above it.
        (0, [1, 0, 1]),
        (1, [0, 1, 0]),
        (2, [1, 0, 0]),
        (3, [2, 0, 2]),
        (4, [0, 2, 0]),
        (5, [2, 0, 0]),
        (6, [1, 2, 1]),
        (7, [2, 1, 2]),
        (8, [1, 2, 2]),
    ];
    for (kind, want) in table {
        for (read, want) in [5, -5, 9].into_iter().zip(want) {
            let text = format!("[goals]\ngoal=a\n{}", goal("a", read, kind, 5, ""));
            let mut s = build(&text, &[]);
            pass(&mut s, &env(1, 0), &mut Vec::new());
            assert_eq!(s.goals[1].state, want, "type {kind} reads {read}");
        }
    }
    // Negative values compare as signed numbers; a type out of range or missing leaves 0.
    let text = format!(
        "[goals]\ngoal=a\ngoal=b\ngoal=c\n{}{}{}",
        goal("a", -3, 0, -4, ""),
        goal("b", 0, 9, 0, ""),
        goal("c", i32::MAX, 0, i32::MAX, ""),
    );
    let mut s = build(&text, &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    // The timer holds 1: no limit stated.
    assert_eq!(states(&s), [1, 1, 0, 1]);
}

#[test]
fn a_sticky_goal_keeps_its_state_and_fires_once_and_the_others_fire_at_each_change() {
    let text = format!(
        "[duration]\nnummonths=-1\n[goals]\ngoal=a\ngoal=b\n{}{}",
        goal("a", 1, 0, 1, "sticky=1\ntrulea=1\ntruleb=2"),
        // Done while the month is the second.
        "[b]\nrulea=7\nruleb=0\ntype=2\nvalue=2\ntrulea=3\n",
    );
    let mut s = build(&text, &[]);
    let mut log = Vec::new();
    for month in [1, 2, 2, 3, 2] {
        pass(&mut s, &env(month, 0), &mut log);
    }
    assert_eq!(
        log,
        [
            "scenario trigger a 1 1 2 0 0",
            "scenario trigger b 1 3 0 0 0",
            "scenario trigger b 1 3 0 0 0",
        ]
    );
}

#[test]
fn a_goal_with_no_section_or_no_trigger_is_done_and_silent() {
    let mut s = build("[goals]\ngoal=ghost\n", &[]);
    let mut log = Vec::new();
    pass(&mut s, &env(1, 0), &mut log);
    // Rule 0 reads 0, type 0, value 0: done at once, trigger 0 does nothing; the scenario ends.
    assert_eq!(s.goals[1].state, 1);
    assert_eq!(
        (s.run(), log),
        (Run::Won, vec!["scenario end won".to_string()])
    );
}

#[test]
fn the_pass_runs_from_1500_ms_and_gathers_what_is_left_over_to_nothing() {
    let mut s = build(
        "[duration]\nnummonths=5\n[goals]\ngoal=a\n[a]\nrulea=7\nruleb=2\ntype=0\nvalue=2\ntrulea=1\n",
        &[],
    );
    let mut log = Vec::new();
    s.update(1499, &env(2, 0), &mut log);
    assert!(log.is_empty());
    s.update(1, &env(2, 0), &mut log);
    assert_eq!(log.len(), 1);
    // The counter restarted at 0: the 3000 ms of one step are one pass, not two.
    s.update(3000, &env(1, 0), &mut log);
    s.update(1499, &env(2, 0), &mut log);
    assert_eq!(s.goals[1].state, 0);
    s.update(0, &env(2, 0), &mut log);
    assert_eq!(s.goals[1].state, 0);
    s.update(1, &env(2, 0), &mut log);
    assert_eq!(s.goals[1].state, 1);
}

#[test]
fn the_timer_wins_when_time_is_up_and_everything_is_done_and_loses_otherwise() {
    let text = |read| {
        format!(
            "[duration]\nnummonths=2\n[goals]\ngoal=a\n{}",
            goal("a", read, 0, 1, "")
        )
    };
    // Total months 2: the time is not over, the timer holds 0 and the scenario goes on.
    let mut s = build(&text(1), &[]);
    let mut log = Vec::new();
    pass(&mut s, &env(2, 0), &mut log);
    assert_eq!((s.goals[0].state, s.run()), (0, Run::On));
    // Total months 3: state 3, every goal done, won.
    pass(&mut s, &env(3, 0), &mut log);
    assert_eq!((s.goals[0].state, s.run()), (3, Run::Won));
    assert_eq!(log, ["scenario end won"]);
    // A goal not done: the timer becomes 2.
    let mut s = build(&text(0), &[]);
    pass(&mut s, &env(1, 1), &mut Vec::new());
    assert_eq!((s.goals[0].state, s.run()), (2, Run::Lost));
}

#[test]
fn the_timer_starts_done_without_a_limit_and_never_ends_with_minus_one() {
    for (months, state) in [("", 1), ("0", 1), ("-1", 0), ("-7", 0), ("x", 1)] {
        let s = build(&format!("[duration]\nnummonths={months}\n"), &[]);
        assert_eq!(s.goals[0].state, state, "nummonths {months:?}");
    }
    let mut s = build("[duration]\nnummonths=-1\n[goals]\ngoal=a\n", &[]);
    pass(&mut s, &env(12, 19), &mut Vec::new());
    assert_eq!(s.run(), Run::On);
    // A huge limit is just far away.
    let mut s = build("[duration]\nnummonths=2147483647\n", &[]);
    pass(&mut s, &env(12, 19), &mut Vec::new());
    assert_eq!(s.run(), Run::On);
}

#[test]
fn a_failed_goal_loses_at_once_and_an_optional_one_never_blocks() {
    let text = format!(
        "[duration]\nnummonths=-1\n[goals]\ngoal=a\ngoal=b\n{}{}",
        goal("a", 0, 6, 1, "optional=1\ntrulea=1"),
        goal("b", 1, 6, 1, ""),
    );
    let mut s = build(&text, &[]);
    let mut log = Vec::new();
    pass(&mut s, &env(1, 0), &mut log);
    // `a` failed but is optional: it fires its own trigger, reports 1; `b` is done; the timer
    // (-1 months) holds 0, so the scenario goes on.
    assert_eq!((states(&s), s.run()), (vec![0, 2, 1], Run::On));
    assert_eq!(log, ["scenario trigger a 2 1 0 0 0"]);
    let text = format!("[goals]\ngoal=a\n{}", goal("a", 0, 6, 1, ""));
    let mut s = build(&text, &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!(s.run(), Run::Lost);
}

#[test]
fn a_chain_stops_at_the_first_goal_not_done_and_a_complex_goal_updates_all() {
    let kids = format!(
        "{}{}{}",
        goal("x", 1, 0, 1, "trulea=1\ntruleb=1"),
        goal("y", 0, 0, 1, "trulea=1\ntruleb=2"),
        goal("z", 1, 0, 1, "trulea=1\ntruleb=3"),
    );
    for (rtype, fired, state) in [(2, 1, 0), (1, 2, 0)] {
        let text = format!(
            "[duration]\nnummonths=-1\n[goals]\ngoal=g\n[g]\nrtype={rtype}\nchild=x\nchild=y\nchild=z\n{kids}"
        );
        let mut s = build(&text, &[]);
        let mut log = Vec::new();
        pass(&mut s, &env(1, 0), &mut log);
        assert_eq!(log.len(), fired, "rtype {rtype}");
        assert_eq!(s.goals[1].state, state);
    }
    // All done: both are 1, and a failed child fails the group.
    let done = format!("{}{}", goal("x", 1, 0, 1, ""), goal("y", 1, 0, 1, ""));
    let text = |rtype| format!("[goals]\ngoal=g\n[g]\nrtype={rtype}\nchild=x\nchild=y\n{done}");
    for rtype in [1, 2] {
        let mut s = build(&text(rtype), &[]);
        pass(&mut s, &env(1, 0), &mut Vec::new());
        assert_eq!((s.goals[1].state, s.run()), (1, Run::Won));
    }
    let failed = format!("{}{}", goal("x", 1, 0, 1, ""), goal("y", 0, 6, 1, ""));
    let text = format!("[goals]\ngoal=g\n[g]\nrtype=1\nchild=x\nchild=y\n{failed}");
    let mut s = build(&text, &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!((s.goals[1].state, s.run()), (2, Run::Lost));
}

#[test]
fn goals_nest_and_sections_that_name_each_other_stop() {
    let text = "[goals]\ngoal=a\n[a]\nrtype=1\nchild=b\n[b]\nrtype=2\nchild=c\n[c]\nrulea=99\narga=1\nvalue=1\n";
    let mut s = build(text, &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!(s.goals[1].state, 1);
    // A section that is its own child, twice over: bounded by the depth, then by the count.
    let text = "[goals]\ngoal=a\n[a]\nrtype=1\nchild=a\nchild=a\n";
    let s = build(text, &[]);
    fn count(g: &Goal) -> usize {
        match &g.body {
            Body::Complex(c) | Body::Chain(c) => 1 + c.iter().map(count).sum::<usize>(),
            _ => 1,
        }
    }
    assert!(count(&s.goals[1]) <= MAX_GOALS as usize);
    assert!(count(&s.goals[1]) > 1);
    // A huge list of goals is cut at the cap.
    let many = "goal=ghost\n".repeat(MAX_GOALS as usize + 10);
    assert_eq!(
        build(&format!("[goals]\n{many}"), &[]).goals.len(),
        MAX_GOALS as usize + 1
    );
}

#[test]
fn the_list_is_the_timer_then_the_main_goals_then_each_extra_file_and_a_goal_reads_its_own_file() {
    let main = format!(
        "[goals]\ngoal=a\ngoal=b\n{}{}",
        goal("a", 0, 0, 1, ""),
        goal("b", 0, 0, 1, "")
    );
    let one = format!("[goals]\ngoal=a\n{}", goal("a", 1, 0, 1, ""));
    let two = "[goals]\ngoal=c\ngoal=missing\n[c]\nrulea=99\narga=1\nvalue=1\n";
    let s = build(&main, &[&one, two]);
    let names: Vec<&str> = s.goals.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["timer", "a", "b", "a", "c", "missing"]);
    let mut s = s;
    pass(&mut s, &env(1, 0), &mut Vec::new());
    // The `a` of the extra file is its own section: it reads 1, the main file's `a` reads 0.
    assert_eq!(states(&s)[1..4], [0, 0, 1]);
}

#[test]
fn start_triggers_fire_in_order_and_a_trigger_0_is_silent() {
    let text = "[start]\ntriggers=t2\ntriggers=none\ntriggers=t1\n\
        [t1]\ntrulea=4\ntruleb=9\n[t2]\ntrulea=2\ntruleb=3\ntarga=4107\n[none]\ntruleb=3\n";
    let s = build(text, &[]);
    let mut log = Vec::new();
    s.start(&mut log);
    assert_eq!(
        log,
        [
            "scenario start s.scn 1",
            "scenario trigger t2 start 2 3 4107 0",
            "scenario trigger t1 start 4 9 0 0",
        ]
    );
}

#[test]
fn rule_7_reads_the_date_and_rule_99_its_argument_and_the_stubs_read_0() {
    let e = env(5, 2);
    assert_eq!([0, 1, 2, 3].map(|b| eval(&[7, b, 0, 0], &e)), [5, 2, 29, 0]);
    assert_eq!(eval(&[99, 0, -4, 0], &e), -4);
    for rule in [8, 10, 11, 12, 13, 14, 15, 100, -1] {
        assert_eq!(eval(&[rule, 0, 3, 0], &e), 0, "rule {rule}");
    }
    assert_eq!(env(12, i32::MAX).total_months(), i32::MAX);
}

#[test]
fn a_scenario_is_read_from_the_install_and_an_unreadable_extra_file_is_skipped() {
    use crate::sprite::tests::Install;
    let main = "[start]\nextragoals=x/gone.scn\nextragoals=x/more.scn\n[goals]\ngoal=a\n";
    let install = Install::new(
        "scenario-prepare",
        &[
            ("x/main.scn", main.as_bytes().to_vec()),
            ("x/more.scn", b"[goals]\ngoal=b\n".to_vec()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let s = Scenario::prepare(&vfs, "x/main.scn").unwrap();
    assert_eq!(s.goals.len(), 3);
    assert!(Scenario::prepare(&vfs, "x/gone.scn").is_none());
}

#[test]
fn an_extra_value_names_every_file_its_prefix_and_suffix_match_and_no_other() {
    use crate::sprite::tests::Install;
    let main =
        "[start]\nextragoals=x/more.scn\nextragoals=x/more\nextragoals=x/more.\n[goals]\ngoal=a\n";
    let install = Install::new(
        "scenario-pattern",
        &[
            ("x/main.scn", main.as_bytes().to_vec()),
            ("x/more.scn", b"[goals]\ngoal=b\n".to_vec()),
            ("x/more02.scn", b"[goals]\ngoal=d\n".to_vec()),
            ("x/more01.scn", b"[goals]\ngoal=c\n".to_vec()),
            ("x/morex.txt", b"[goals]\ngoal=e\n".to_vec()),
            ("x/other.scn", b"[goals]\ngoal=f\n".to_vec()),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let s = Scenario::prepare(&vfs, "x/main.scn").unwrap();
    let names: Vec<&str> = s.goals.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["timer", "a", "b", "c", "d"]);
}

#[test]
fn rule_9_counts_the_goals_of_the_list_that_report_a_state_itself_included() {
    // The timer holds 1 (no limit). `b` counts the goals reporting 1; the optional ones report 1.
    let counts = |optional: &str, want: i32| {
        let text = format!(
            "[goals]\ngoal=a\ngoal=b\ngoal=c\n[a]\nrulea=99\narga=0\ntype=2\nvalue=1\n{optional}\n\
             [b]\nrulea=9\nruleb=1\ntype=0\nvalue={want}\n[c]\nrulea=99\narga=0\ntype=2\nvalue=1\n{optional}\n"
        );
        let mut s = build(&text, &[]);
        pass(&mut s, &env(1, 0), &mut Vec::new());
        s.goals[2].state
    };
    // Alone the timer reports 1; with two optional goals, three.
    assert_eq!([counts("", 1), counts("", 2)], [1, 0]);
    assert_eq!([counts("optional=1", 3), counts("optional=1", 4)], [1, 0]);
    // The goal itself is in the count: `b` is the only goal in progress.
    let text = "[goals]\ngoal=b\n[b]\nrulea=9\nruleb=0\ntype=2\nvalue=1\nsticky=1\n";
    let mut s = build(text, &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!(s.goals[1].state, 1);
    // Any other state counts nothing.
    let e = env(1, 0);
    for b in [4, -1, i32::MAX] {
        assert_eq!(rules::eval(&[9, b, 0, 0], 0, &e, &[0, 1, 2, 3]), 0);
    }
    assert_eq!(rules::eval(&[9, 2, 0, 0], 0, &e, &[0, 1, 2, 3, 2]), 2);
}

#[test]
fn the_sim_step_reads_the_cash_and_the_zoo_status_for_a_goal() {
    use crate::entity::Catalogue;
    use crate::money::Money;
    use crate::sim::Sim;
    use crate::unit::testing::world_with;
    use crate::zoo::Clocks;
    // Goal `rich` completes at 1000 in cash; `rated` when the zoo rating, as the status keeps it, is 50.
    let text = "[goals]\ngoal=rich\ngoal=rated\n\
        [rich]\nrulea=1\nruleb=7\ntype=0\nvalue=1000\ntrulea=4\n\
        [rated]\nrulea=1\nruleb=2\ntype=2\nvalue=50\ntrulea=5\n";
    let (cat, mut w) = (Catalogue::default(), world_with((4, 4), |_| {}));
    let (mut sim, mut money) = (Sim::new(b""), Money::default());
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    sim.start_scenario(build(text, &[]));
    let step = |sim: &mut Sim, money: &mut Money, w: &mut crate::world::World| {
        let data = &crate::behaviour::NoData;
        sim.step_game(data, &cat, w, money, 1500, |_, _, _| {});
        sim.take_lines()
            .into_iter()
            .filter(|l| l.starts_with("scenario trigger"))
            .collect::<Vec<_>>()
    };
    money.cash = 999.0;
    assert!(step(&mut sim, &mut money, &mut w).is_empty());
    money.cash = 1000.0;
    sim.status.rating = 50;
    let lines = step(&mut sim, &mut money, &mut w);
    assert_eq!(
        lines,
        [
            "scenario trigger rich 1 4 0 0 0",
            "scenario trigger rated 1 5 0 0 0"
        ]
    );
}
