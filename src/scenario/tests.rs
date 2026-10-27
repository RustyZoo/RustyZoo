use super::*;

fn build(main: &str, extras: &[&str]) -> Scenario {
    let parse = |t: &str| Config::parse(t.as_bytes()).unwrap();
    let extras: Vec<Config> = extras.iter().map(|t| parse(t)).collect();
    Scenario::build("s.scn", &parse(main), &extras)
}

fn env(month: i32, year: i32) -> Env {
    Env { month, year }
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
fn rule_7_reads_the_date_and_rule_99_its_argument_and_others_are_stubs_that_read_0() {
    let e = env(5, 2);
    assert_eq!([0, 1, 2, 3].map(|b| eval(&[7, b, 0, 0], &e)), [5, 2, 29, 0]);
    assert_eq!(eval(&[99, 0, -4, 0], &e), -4);
    for rule in [0, 1, 4, 9, 10, 16, 100, -1] {
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
fn the_end_dialog_text_follows_the_file_the_result_and_the_expansions() {
    let text = |file, won, x| end_text(file, won, x);
    let table = [
        ("scenario/scn01/scn01.scn", [16104, 16104]),
        ("scenario/scn01a/scn01a.scn", [16104, 16104]),
        ("scenario/scn27/scn27.scn", [16104, 16104]),
        ("scenario/scn28/scn28.scn", [16104, 16104]),
        ("scenario/scn28b/scn28b.scn", [16102, 16102]),
        ("scenario/scn03/scn03.scn", [16100, 16100]),
    ];
    for (file, [with, without]) in table {
        assert_eq!(
            (text(file, true, 3), text(file, true, 0)),
            (with, without),
            "{file}"
        );
        assert_eq!(text(file, false, 0), without + 1, "{file} lost");
    }
    // Scenario 2 wants any expansion, scenario 19 Marine Mania; the path may use either slash.
    let scn2 = "scenario\\SCN02\\scn02.scn";
    assert_eq!(
        [0, 1, 2, 3].map(|x| text(scn2, true, x)),
        [16102, 16104, 16104, 16104]
    );
    let scn19 = "scenario/scn19/scn19.scn";
    assert_eq!(
        [0, 1, 2, 3].map(|x| text(scn19, false, x)),
        [16103, 16103, 16105, 16105]
    );
    assert_eq!(text("", true, i32::MIN), 16100);
}

#[test]
fn the_objectives_list_holds_the_timer_and_the_goals_that_have_a_text_and_are_not_hidden() {
    let text = "[duration]\nnummonths=3\ntext=100\n[goals]\ngoal=a\ngoal=b\ngoal=c\ngoal=g\n\
        [a]\ntext=11\nvalue=5\narga=6\nargb=7\n[b]\nvalue=1\n[c]\ntext=13\nhidden=1\n\
        [g]\nrtype=2\nchild=d\nchild=h\n[d]\ntext=14\n[h]\nrtype=1\nchild=e\n[e]\ntext=15\n";
    let s = build(text, &[]);
    let line = |text, values, state| Line {
        text,
        values,
        state,
    };
    assert_eq!(
        s.lines(),
        [
            line(100, [3, 0, 0], 0),
            line(11, [5, 6, 7], 0),
            line(14, [0, 0, 0], 0),
            line(15, [0, 0, 0], 0),
        ]
    );
    // No text for the timer: no line for it; a goal's line takes its state.
    let mut s = build(
        "[goals]\ngoal=a\n[a]\ntext=11\nrulea=99\narga=1\nvalue=1\n",
        &[],
    );
    assert_eq!(s.lines().len(), 1);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!(s.lines()[0].state, 1);
}

#[test]
fn the_scenario_ends_once_and_a_win_brings_its_unlocks() {
    let text = "[completion]\nunlock=7\nunlock=-3\n[goals]\ngoal=a\n\
        [a]\nrulea=99\narga=1\nvalue=1\n";
    let mut won = build(text, &[]);
    assert_eq!(won.ended(), None, "still running");
    pass(&mut won, &env(1, 0), &mut Vec::new());
    let end = Ended {
        file: "s.scn".into(),
        won: true,
        unlocks: vec![7, -3],
    };
    assert_eq!(won.ended(), Some(end));
    assert_eq!(won.ended(), None, "told once");
    // A loss unlocks nothing.
    let lost = "[completion]\nunlock=7\n[goals]\ngoal=a\n[a]\nrulea=99\narga=1\nvalue=1\ntype=7\n";
    let mut lost = build(lost, &[]);
    pass(&mut lost, &env(1, 0), &mut Vec::new());
    assert_eq!(
        lost.ended().map(|e| (e.won, e.unlocks)),
        Some((false, vec![]))
    );
}

#[test]
fn the_title_and_the_icons_come_from_the_desc_section() {
    let s = build(
        "[desc]\nname=16001\nstory=STORY\niconp=a\niconc=b\niconf=c\n",
        &[],
    );
    assert_eq!(
        (s.title(), s.icons()),
        ((16001, &b"STORY"[..]), &["a", "b", "c"].map(String::from))
    );
    let s = build("", &[]);
    assert_eq!(
        (s.title(), s.icons()),
        ((0, &b""[..]), &[""; 3].map(String::from))
    );
}
