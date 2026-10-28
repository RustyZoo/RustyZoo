use super::*;

fn build(main: &str, extras: &[&str]) -> Scenario {
    let parse = |t: &str| Config::parse(t.as_bytes()).unwrap();
    let extras: Vec<Config> = extras.iter().map(|t| parse(t)).collect();
    Scenario::build("s.scn", &parse(main), &extras)
}

/// The value of a rule with no interface.
fn eval(rule: &[i32; 4], e: &Env) -> i32 {
    rules::eval(rule, 0, e, &[], &Gui::default())
}

/// The value of a rule with this interface.
fn with_gui(rule: &[i32; 4], value: i32, e: &Env, gui: &Gui) -> i32 {
    rules::eval(rule, value, e, &[], gui)
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
    for rule in [0, 1, 4, 8, 9, 10, 12, 13, 14, 15, 16, 100, -1] {
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

/// A scenario from `text` with the sections 1 to 3 of popups, and its pass.
fn effects(text: &str) -> Vec<Effect> {
    let mut s = build(text, &[]);
    s.start(&mut Vec::new());
    pass(&mut s, &env(1, 0), &mut Vec::new());
    s.gui().take_effects()
}

/// A scenario whose one start trigger is trigger 2 with these keys, and the sections `body`.
fn trigger(keys: &str, body: &str) -> Vec<Effect> {
    effects(&format!(
        "[start]\ntriggers=t\n[t]\ntrulea=2\n{keys}\n{body}"
    ))
}

const POPUPS: &str = "[9]\nlayout=45200\nlongText1=TXT\nimage1=pic\nshortText2=7\n\
    [default]\nlayout=45100\nbackground=bg\nshortText1=3\nlongText1=BASE\npause=1\n\
    persistentTextLayout=45400\npersistentShortText=-1\n";

#[test]
fn a_popup_is_its_section_over_the_default_one_with_its_defaults() {
    let cfg = |t: &str| Config::parse(t.as_bytes()).unwrap();
    let p = Popup::read(&cfg(POPUPS), 9).unwrap();
    assert_eq!((p.layout, p.short, p.modal), (45200, [3, 7], true));
    assert_eq!(p.long, ["TXT", ""]);
    assert_eq!(p.pictures, ["bg", "", "pic", ""]);
    assert_eq!((p.ok, p.cancel), (true, false));
    assert_eq!(p.hint.layout, 45400);
    assert_eq!((p.hint.short, p.hint.button), (-1, 45406));
    // No section of its own: the default one alone; no default section: all defaults.
    let q = Popup::read(&cfg(POPUPS), 4).unwrap();
    assert_eq!((q.layout, q.long[0].as_str()), (45100, "BASE"));
    let none = Popup::read(&cfg(""), 4).unwrap();
    assert_eq!(
        none,
        Popup {
            ok: true,
            hint: Hint {
                button: 45406,
                ..Hint::default()
            },
            ..Popup::default()
        }
    );
    // The id: 0 and below show nothing.
    for id in [0, -1, i32::MIN] {
        assert_eq!(Popup::read(&cfg(POPUPS), id), None);
    }
    // `modal` wins over `pause`; ok 0 with cancel 1 is a Cancel alone.
    let t = "[1]\npause=1\nmodal=0\nok=0\ncancel=1\n[2]\npause=0\nmodal=1\n";
    let (a, b) = (
        Popup::read(&cfg(t), 1).unwrap(),
        Popup::read(&cfg(t), 2).unwrap(),
    );
    assert_eq!(
        (a.modal, a.ok, a.cancel, b.modal),
        (false, false, true, true)
    );
}

#[test]
fn trigger_2_popups_and_presses_and_single_elements() {
    let e = |keys: &str| trigger(keys, POPUPS);
    // The hint of -1 is the text of the first goal in progress: none here.
    let popup = Popup::read(&Config::parse(POPUPS.as_bytes()).unwrap(), 9).map(|mut p| {
        p.hint.short = 0;
        p
    });
    let set = |op, always| Effect::Set {
        ops: vec![(1002, op)],
        popup: popup.clone(),
        always,
    };
    assert_eq!(
        e("truleb=0\ntarga=9"),
        [Effect::Popup(popup.clone().unwrap())]
    );
    for b in [1, 2] {
        assert_eq!(
            e(&format!("truleb={b}\ntarga=1002\ntargb=9")),
            [Effect::Press(1002, popup.clone())]
        );
    }
    let table = [
        (3, Op::Disable),
        (4, Op::Disable),
        (5, Op::Enable),
        (6, Op::Enable),
        (7, Op::Hide),
        (8, Op::Hide),
        (9, Op::Show),
        (10, Op::Show),
    ];
    for (b, op) in table {
        assert_eq!(
            e(&format!("truleb={b}\ntarga=1002\ntargb=9")),
            [set(op, false)],
            "truleb {b}"
        );
    }
    // No popup named, or one of 0 or below: the operation alone. Unknown truleb: nothing.
    let bare = Effect::Set {
        ops: vec![(1002, Op::Hide)],
        popup: None,
        always: false,
    };
    assert_eq!(e("truleb=7\ntarga=1002"), std::slice::from_ref(&bare));
    assert_eq!(e("truleb=7\ntarga=1002\ntargb=-5"), [bare]);
    assert!(e("truleb=17\ntarga=1").is_empty() && e("truleb=-1\ntarga=1").is_empty());
    assert!(e("truleb=0\ntarga=0").is_empty(), "popup 0 shows nothing");
}

#[test]
fn trigger_2_blocks_and_the_question() {
    let blocks = "[20]\nid=1000\nid=1001\nid=x\n[21]\n1000=enable\n1001=d\n1002=hide\n1003=s\n1004=x\n1005=\n";
    let e = |keys: &str| trigger(keys, &format!("{blocks}{POPUPS}"));
    for (b, op) in [
        (12, Op::Disable),
        (13, Op::Enable),
        (14, Op::Hide),
        (15, Op::Show),
    ] {
        let got = e(&format!("truleb={b}\ntarga=20\ntargb=9"));
        let [
            Effect::Set {
                ops,
                popup,
                always: false,
            },
        ] = &got[..]
        else {
            panic!("{got:?}")
        };
        assert_eq!(ops[..2], [(1000, op), (1001, op)]);
        assert_eq!(ops.len(), 3, "an id that is not a number is 0");
        assert!(popup.is_some());
    }
    // 16: letters or words in lower case; an entry with another value is not one; the popup shows anyway.
    let got = e("truleb=16\ntarga=21\ntargb=9");
    let [
        Effect::Set {
            ops, always: true, ..
        },
    ] = &got[..]
    else {
        panic!("{got:?}")
    };
    let want = [
        (1000, Op::Enable),
        (1001, Op::Disable),
        (1002, Op::Hide),
        (1003, Op::Show),
    ];
    assert_eq!(ops, &want);
    assert!(matches!(&e("truleb=12\ntarga=99")[..], [Effect::Set { ops, .. }] if ops.is_empty()));
    // 11: a question with the string `targa`, modal when `targb` is above 0.
    for (b, modal) in [(1, true), (0, false), (-3, false), (i32::MAX, true)] {
        let got = e(&format!("truleb=11\ntarga=77\ntargb={b}"));
        let [Effect::Popup(p)] = &got[..] else {
            panic!("{got:?}")
        };
        assert_eq!(
            (p.layout, p.short[0], p.modal, p.ok),
            (45000, 77, modal, true)
        );
    }
}

#[test]
fn trigger_16_acts_on_the_first_lower_case_letter_of_each_value() {
    let block = "[21]\n1=disable\n2=e\n3=hide\n4=show\n5=D\n6=Enable\n7=\n8=x\n9=xd\n";
    let got = trigger("truleb=16\ntarga=21\ntargb=9", &format!("{block}{POPUPS}"));
    let [Effect::Set { ops, always, .. }] = &got[..] else {
        panic!("{got:?}")
    };
    let want = [
        (1, Op::Disable),
        (2, Op::Enable),
        (3, Op::Hide),
        (4, Op::Show),
    ];
    assert_eq!((ops, *always), (&want.to_vec(), true));
}

#[test]
fn a_trigger_run_again_after_a_load_keeps_the_elements_and_drops_popups_and_presses() {
    let text = "[start]\ntriggers=a\ntriggers=b\ntriggers=c\ntriggers=d\n\
        [a]\ntrulea=2\ntruleb=0\ntarga=9\n[b]\ntrulea=2\ntruleb=1\ntarga=5\ntargb=9\n\
        [c]\ntrulea=2\ntruleb=3\ntarga=5\ntargb=9\n[d]\ntrulea=2\ntruleb=11\ntarga=5\n";
    let s = build(&format!("{text}{POPUPS}"), &[]);
    for (_, t) in &s.start {
        t.run(false, s.gui());
    }
    let got = s.gui().take_effects();
    assert_eq!(
        got,
        [Effect::Set {
            ops: vec![(5, Op::Disable)],
            popup: None,
            always: false
        }]
    );
}

#[test]
fn a_hint_of_minus_1_shows_the_text_of_the_first_goal_in_progress() {
    let text = format!(
        "[start]\ntriggers=t\n[goals]\ngoal=a\ngoal=b\ngoal=c\n\
         [t]\ntrulea=2\ntruleb=0\ntarga=9\n{POPUPS}\
         [a]\nrulea=99\narga=1\nvalue=1\ntext=11\n[b]\nrulea=99\narga=0\nvalue=1\ntext=22\n\
         [c]\nrulea=99\narga=0\nvalue=1\ntext=33\n"
    );
    let mut s = build(&text, &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    s.start(&mut Vec::new());
    let [Effect::Popup(p)] = &s.gui().take_effects()[..] else {
        panic!()
    };
    assert_eq!(p.hint.short, 22, "a is done, b is the first in progress");
    // Nothing in progress: no text.
    let mut s = build(&text.replace("arga=0", "arga=1"), &[]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    s.start(&mut Vec::new());
    let [Effect::Popup(p)] = &s.gui().take_effects()[..] else {
        panic!()
    };
    assert_eq!(p.hint.short, 0);
}

#[test]
fn a_goal_of_an_extra_file_reads_the_popup_of_its_own_file() {
    let main = "[goals]\ngoal=a\n[a]\nrulea=99\narga=1\nvalue=1\ntrulea=2\ntruleb=0\ntarga=1\n[1]\nlayout=1\n";
    let extra = main
        .replace("goal=a", "goal=b")
        .replace("[a]", "[b]")
        .replace("layout=1", "layout=2");
    let mut s = build(main, &[&extra]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    let layouts: Vec<i32> = s
        .gui()
        .take_effects()
        .iter()
        .map(|e| match e {
            Effect::Popup(p) => p.layout,
            _ => 0,
        })
        .collect();
    assert_eq!(layouts, [1, 2]);
}

#[test]
fn rule_10_reads_whether_an_element_is_shown_or_was_pressed_since_the_last_question() {
    let (gui, e) = (Gui::default(), env(1, 0));
    let shown = |gui: &Gui| with_gui(&[10, 0, 1000, 0], 5, &e, gui);
    let pressed = |gui: &Gui, id| with_gui(&[10, 1, id, 0], 5, &e, gui);
    // Nothing is known of an element until the application tells; asking registers it.
    assert_eq!(shown(&gui), 0);
    gui.sync(&|id| id == 1000, &|_| vec![]);
    assert_eq!(shown(&gui), 5);
    gui.sync(&|_| false, &|_| vec![]);
    assert_eq!(shown(&gui), 0);
    // A press before the first question is not seen; the first question makes the entry.
    gui.pressed(1001, None);
    assert_eq!(pressed(&gui, 1001), 0);
    gui.pressed(1001, None);
    assert_eq!(pressed(&gui, 1001), 5, "seen once");
    assert_eq!(pressed(&gui, 1001), 0, "the question consumed it");
    // Another ruleb or an element of no interest: 0.
    assert_eq!(with_gui(&[10, 2, 1000, 0], 5, &e, &gui), 0);
    assert_eq!(with_gui(&[10, -1, 1000, 0], 5, &e, &gui), 0);
    assert_eq!(with_gui(&[10, 0, i32::MIN, 0], 5, &e, &gui), 0);
}

#[test]
fn rule_11_reads_the_row_of_a_list_that_was_pressed() {
    let (gui, e) = (Gui::default(), env(1, 0));
    let by_row = |gui: &Gui| with_gui(&[11, 0, 22900, 0], 0, &e, gui);
    let by_text = |gui: &Gui| with_gui(&[11, 1, 22900, 22903], 0, &e, gui);
    assert_eq!(
        (by_row(&gui), by_text(&gui)),
        (-2, -2),
        "first question: not pressed"
    );
    gui.sync(&|_| false, &|id| {
        if id == 22903 {
            b"Lion".to_vec()
        } else {
            vec![]
        }
    });
    gui.pressed(22900, Some((3, b"Lion".to_vec())));
    assert_eq!(by_text(&gui), 1);
    assert_eq!(by_text(&gui), -2, "consumed");
    gui.pressed(22900, Some((3, b"Tiger".to_vec())));
    assert_eq!(by_text(&gui), -2, "another text");
    gui.pressed(22900, Some((3, b"Lion".to_vec())));
    assert_eq!(by_row(&gui), 3);
    // A press without a row (a button) and a huge row.
    gui.pressed(22900, None);
    assert_eq!(by_row(&gui), -2);
    gui.pressed(22900, Some((usize::MAX, vec![])));
    assert_eq!(by_row(&gui), i32::MAX);
}

#[test]
fn rule_10_reads_a_shown_element_in_the_first_pass() {
    let text = format!(
        "[goals]\ngoal=g\n[g]\nrulea=10\nruleb=0\narga=1000\nvalue=1\ntrulea=2\ntruleb=0\ntarga=9\n{POPUPS}"
    );
    let mut s = build(&text, &[]);
    // The first sync already tells the element, so the first pass sees it; a hidden one waits.
    s.gui().sync(&|id| id == 999, &|_| vec![]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert!(s.gui().take_effects().is_empty() && states(&s)[1] == 0);
    s.gui().sync(&|id| id == 1000, &|_| vec![]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!(s.gui().take_effects().len(), 1);
    assert_eq!(states(&s)[1], 1);
    // Also in a first pass of a fresh scenario, and inside a group.
    let text =
        "[goals]\ngoal=c\n[c]\nrtype=1\nchild=g\n[g]\nrulea=10\nruleb=0\narga=1000\nvalue=1\n";
    let mut s = build(text, &[]);
    s.gui().sync(&|id| id == 1000, &|_| vec![]);
    pass(&mut s, &env(1, 0), &mut Vec::new());
    assert_eq!(states(&s)[1], 1);
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
        assert_eq!(
            rules::eval(&[9, b, 0, 0], 0, &e, &[0, 1, 2, 3], &Gui::default()),
            0
        );
    }
    assert_eq!(
        rules::eval(&[9, 2, 0, 0], 0, &e, &[0, 1, 2, 3, 2], &Gui::default()),
        2
    );
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
    // Scenario 2 wants any expansion, scenario 19 the second one; the path may use either slash.
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

#[test]
fn the_tutorial_chain_follows_the_expansions_installed() {
    let next = |f: &str, x| next_tutorial(f, x);
    let file = |n: &str| Some(format!("scenario/{n}/{n}.scn"));
    // (file, expansion flags, next)
    let table = [
        ("scenario/scn01a/scn01a.scn", 0, file("scn01")),
        ("scenario/scn01/scn01.scn", 0, file("scn02")),
        ("scenario/scn02/scn02.scn", 0, None),
        ("scenario/scn02/scn02.scn", 1, file("scn19")),
        ("scenario/scn02/scn02.scn", 2, file("scn27")),
        ("scenario/scn02/scn02.scn", 3, file("scn19")),
        ("scenario/scn19/scn19.scn", 1, None),
        ("scenario/scn19/scn19.scn", 3, file("scn27")),
        ("scenario/scn27/scn27.scn", 0, file("scn28")),
        ("scenario/scn28/scn28.scn", 0, file("scn28b")),
        ("scenario/scn28b/scn28b.scn", 3, None),
        (r"Scenario\SCN01\scn01.SCN", 0, file("scn02")),
        ("scenario/scn03/scn03.scn", 3, None),
        ("", 3, None),
    ];
    for (f, x, want) in table {
        assert_eq!(next(f, x), want, "{f} {x}");
    }
    for (f, want) in [
        ("scenario/scn28b/scn28b.scn", true),
        (r"Scenario\SCN19\scn19.scn", true),
        ("scenario/scn03/scn03.scn", false),
        ("s/scn01.scn", false),
        ("", false),
    ] {
        assert_eq!(is_tutorial(f), want, "{f}");
    }
}

#[test]
fn triggers_4_and_7_leave_an_availability_change_and_a_bad_one_nothing() {
    let made = |keys: &str| {
        let s = build(
            &format!("[start]\ntriggers=t\n[t]\n{keys}\n[5]\nid=1\nid=3\n"),
            &[],
        );
        s.start(&mut Vec::new());
        s.gui().take_avail()
    };
    let one = |which, on| vec![Avail { which, on }];
    assert_eq!(
        made("trulea=4\ntruleb=0\ntarga=7"),
        one(Which::Name(7), true)
    );
    assert_eq!(
        made("trulea=4\ntruleb=3\ntarga=7"),
        one(Which::Name(7), false)
    );
    assert_eq!(made("trulea=4\ntruleb=4"), one(Which::Name(0), true));
    assert_eq!(made("trulea=4\ntruleb=-1\ntarga=-5"), vec![]);
    assert_eq!(made("trulea=4\ntruleb=6"), one(Which::Animals, true));
    assert_eq!(made("trulea=4\ntruleb=9"), one(Which::All, false));
    assert_eq!(made("trulea=4\ntruleb=10\ntarga=7"), vec![]);
    assert_eq!(made("trulea=4\ntruleb=99999999999"), vec![]);
    assert_eq!(
        made("trulea=7\ntruleb=1\ntarga=5"),
        one(Which::Names(vec![1, 3]), false)
    );
    assert_eq!(
        made("trulea=7\ntruleb=1\ntarga=999"),
        one(Which::Names(vec![]), false)
    );
    assert_eq!(made("trulea=7\ntruleb=9"), vec![]);
    assert_eq!(made("trulea=3\ntruleb=0\ntarga=7"), vec![]);
}
