use super::*;

#[test]
fn a_file_time_reads_back_as_the_game_date() {
    for ms in [0, 1, 123_456_789, -86_400_000 * 400] {
        assert_eq!(Date::from_file_time(file_time(ms)).game_ms(), ms);
    }
    assert_eq!(file_time(i64::MIN), 0);
    assert_eq!(file_time(i64::MAX), u64::MAX);
}

fn periods(ini: &str) -> (u32, u32, u32) {
    let t = Timers::from_config(&Config::parse(ini.as_bytes()).unwrap());
    (t.update_period(), t.draw_period(), t.cap())
}

#[test]
fn ini_values_ignore_case_and_come_in_file_order() {
    let ini = Config::parse(b"[Some]\nKey = a\nKey = b\nEmpty\n[SOME]\nother = c\n").unwrap();
    let all = |s: &str, k: &str| ini_values(&ini, s, k).to_vec();
    assert_eq!(all("some", "KEY"), [b"a".to_vec(), b"b".to_vec()]);
    assert_eq!(ini_value(&ini, "SOME", "Key"), Some(&b"a"[..]));
    for (s, k) in [
        ("some", "nokey"),
        ("nosuch", "key"),
        ("some", "empty"),
        ("", ""),
    ] {
        assert!(
            all(s, k).is_empty() && ini_value(&ini, s, k).is_none(),
            "{s} {k}"
        );
    }
}

#[test]
fn a_stored_date_counts_game_milliseconds_and_a_bad_one_is_the_start() {
    let at = |year, month, day, hour| Date {
        year,
        month,
        day,
        weekday: 0,
        hour,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    assert_eq!(at(2001, 1, 1, 0).game_ms(), 0);
    assert_eq!(at(2001, 1, 2, 1).game_ms(), (DAY_MS + 3_600_000) as i64);
    // The calendar stands at the date and goes on from it.
    let mut c = Calendar::at(at(2001, 1, 31, 23).game_ms());
    assert_eq!((c.date().month, c.date().day), (1, 31));
    c.advance(100_000);
    assert_eq!((c.date().month, c.date().day), (2, 9));
    for bad in [
        at(1600, 12, 31, 23),
        at(0, 1, 1, 0),
        at(2001, 0, 1, 0),
        at(2001, 13, 1, 0),
        at(2001, 1, 0, 0),
        at(2001, 1, 32, 0),
        at(2001, 1, 1, 24),
        at(2001, 1, 1, u32::MAX),
    ] {
        assert_eq!(bad.game_ms(), 0, "{bad:?}");
    }
    assert!(at(30827, 12, 31, 23).game_ms() > 0);
    assert_eq!(at(30828, 1, 1, 0).game_ms(), 0);
    assert_eq!(at(65535, 12, 31, 23).game_ms(), 0);
}

#[test]
fn a_date_before_2001_is_kept_and_the_calendar_counts_back_to_it() {
    let at = |year, month, day, hour| Date {
        year,
        month,
        day,
        weekday: 0,
        hour,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    assert_eq!(
        i128::from(at(2000, 12, 31, 0).game_ms()),
        -i128::from(DAY_MS)
    );
    assert_eq!(
        i128::from(at(2000, 2, 29, 0).game_ms()),
        -307 * i128::from(DAY_MS)
    );
    // 31 December 2000 at 23:00 is one hour before the start: a Sunday.
    let c = Calendar::at(at(2000, 12, 31, 23).game_ms());
    let d = c.date();
    assert_eq!(
        (d.year, d.month, d.day, d.hour, d.weekday),
        (2000, 12, 31, 23, 6)
    );
    // The timers count from the start, an hour before it included.
    assert_eq!(c.elapsed_ms(), -3_600_000);
    let mut c = c;
    c.advance(1_000);
    let d = c.date();
    assert_eq!((d.year, d.month, d.day, d.hour), (2001, 1, 1, 1));
    assert_eq!(c.elapsed_ms(), 3_600_000);
    for x in [at(1601, 1, 1, 0), at(1999, 6, 15, 12), at(2000, 2, 29, 6)] {
        let d = Calendar::at(x.game_ms()).date();
        assert_eq!(
            (d.year, d.month, d.day, d.hour),
            (x.year, x.month, x.day, x.hour),
            "{x:?}"
        );
    }
}

#[test]
fn a_refused_date_is_the_start_of_the_calendar() {
    let at = |year, month, day| Date {
        year,
        month,
        day,
        weekday: 0,
        hour: 0,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    for bad in [
        at(2001, 2, 29),
        at(2001, 4, 31),
        at(2000, 2, 30),
        at(1600, 12, 31),
        Date {
            minute: 60,
            ..at(2001, 1, 1)
        },
        Date {
            second: 60,
            ..at(2001, 1, 1)
        },
        Date {
            millisecond: 1000,
            ..at(2001, 1, 1)
        },
    ] {
        assert_eq!(bad.game_ms(), 0, "{bad:?}");
    }
    // The start is Monday 1 January 2001, 00:00.
    let d = Calendar::at(0).date();
    assert_eq!((d.year, d.month, d.day, d.weekday), (2001, 1, 1, 0));
    // February 29 of 2000 and 2001 February 28 are valid.
    assert_eq!(
        i128::from(at(2000, 2, 29).game_ms()),
        -307 * i128::from(DAY_MS)
    );
    assert_eq!(
        i128::from(at(2001, 2, 28).game_ms()),
        58 * i128::from(DAY_MS)
    );
}

#[test]
fn a_date_counts_milliseconds_from_1970() {
    let at = |t: u64| Date::from_file_time(t * 10_000 + 134_774 * DAY_MS * 10_000);
    for ms in [
        0,
        1,
        86_399_999,
        86_400_000,
        951_782_400_000,
        4_102_444_800_123,
    ] {
        assert_eq!(at(ms).millis(), ms as i64, "{ms}");
    }
    let mut c = Calendar::new();
    c.advance(1_000_000);
    assert_eq!(
        c.date().millis() - Calendar::new().date().millis(),
        7_200_000_000
    );
}

#[test]
fn the_ini_key_enables_the_developer_mode() {
    let enabled = |ini: &str| {
        let mut t = Timers::from_config(&Config::parse(ini.as_bytes()).unwrap());
        t.speed.toggle();
        t.speed.developer()
    };
    for (ini, want) in [
        ("[advanced]\ndevModeEnabled=1\n", true),
        ("[ADVANCED]\ndevmodeenabled=7\n", true),
        ("[advanced]\ndevModeEnabled=-1\n", true),
        ("[advanced]\ndevModeEnabled=99999999999\n", true),
        ("[advanced]\ndevModeEnabled=0\n", false),
        ("[advanced]\ndevModeEnabled=x\n", false),
        ("[advanced]\ndevModeEnabled=\n", false),
        ("[advanced]\nother=1\n", false),
        ("[user]\ndevModeEnabled=1\n", false),
        ("", false),
    ] {
        assert_eq!(enabled(ini), want, "{ini:?}");
    }
}

#[test]
fn the_speed_keys_step_by_tenths_and_stop_at_a_tenth() {
    let mut off = Speed::new(false);
    assert!(!off.toggle() && !off.developer() && !off.key(b'.'));
    assert_eq!(off.factor(), 1.0);
    let mut s = Speed::new(true);
    assert!(!s.key(b'.'), "not before the mode is on");
    assert!(s.toggle() && s.developer());
    assert!(!s.key(b'x') && !s.key(0));
    s.key(b'.');
    s.key(b'.');
    assert_eq!(s.factor(), 1.2000000476837158);
    s.key(b'/');
    assert_eq!(s.factor(), 1.0);
    let down: Vec<f64> = (0..11)
        .map(|_| {
            s.key(b',');
            s.factor()
        })
        .collect();
    assert_eq!(down[4], 0.49999991059303284);
    assert_eq!(down[7], 0.19999992847442627);
    assert_eq!(down[8], 0.09999992698431015, "below 0.1: the lowest value");
    assert_eq!(down[10], down[8], "the next press does nothing");
    s.key(b'/');
    let up: Vec<f64> = (0..10)
        .map(|_| {
            s.key(b'.');
            s.factor()
        })
        .collect();
    assert_eq!(up[0], 1.100000023841858);
    assert_eq!(up[4], 1.5000001192092896);
    assert_eq!(up[9], 2.000000238418579);
    for _ in 0..1000 {
        s.key(b'.');
    }
    assert!(s.factor() > 100.0, "no upper limit");
    assert!(s.toggle() && !s.developer());
}

#[test]
fn period_table() {
    let period = |rate: &str| period_of(rate.as_bytes(), 7);
    // Shipped rates, then the rest of the table; out of range gives the fallback (7 here).
    for (rate, expected) in [
        ("15", 66),
        ("60", 16),
        ("10", 100),
        ("30", 33),
        ("1", 1000),
        ("20", 50),
        ("100", 10),
        ("29.97", 33),
        ("1000", 1),
        ("144", 6),
        ("0.99", 7),
        ("0", 7),
        ("-5", 7),
        ("1001", 7),
        ("abc", 7),
        ("", 7),
    ] {
        assert_eq!(period(rate), expected, "rate {rate:?}");
    }
}

#[test]
fn settings_from_the_ini() {
    assert_eq!(
        periods("[user]\nUpdateRate=15\nDrawRate=60\n"),
        (66, 16, 200)
    );
    assert_eq!(periods("[user]\nfullscreen=1\n"), (100, 33, 200), "absent");
    assert_eq!(
        periods("[resource]\npath=.\n"),
        (100, 33, 200),
        "no section"
    );
    assert_eq!(
        periods("[USER]\nupdaterate=20\ndrawrate=100\n"),
        (50, 10, 200),
        "case"
    );
    assert_eq!(
        periods("[user]\nUpdateRate=0\nDrawRate=5000\n"),
        (100, 33, 200)
    );
    assert_eq!(periods("[user]\nUpdateRate=x\nDrawRate=\n"), (100, 33, 200));
    assert_eq!(periods("[user]\nMaxSimTick=500\n").2, 500);
    assert_eq!(periods("[user]\nMaxSimTick=0\n").2, 200, "0 is refused");
    assert_eq!(
        periods("[user]\nMaxSimTick=-1\n").2,
        u32::MAX,
        "unsigned: no cap"
    );
    assert_eq!(periods("[user]\nMaxSimTick=0x\n").2, 200, "0 is refused");
    assert_eq!(
        periods("[user]\nMaxSimTick=0x1F\n").2,
        200,
        "no 0x rule: reads 0"
    );
    assert_eq!(periods("[user]\nMaxSimTick=abc\n").2, 200, "unreadable");
    assert_eq!(periods("[user]\nMaxSimTick= +31x\n").2, 31);
}

/// Passes at every clock reading in `times`, collecting the firings.
fn run(timers: &mut Timers, times: impl IntoIterator<Item = u64>) -> Vec<(u64, Pass)> {
    times
        .into_iter()
        .map(|now| (now, timers.pass(now, 1.0)))
        .collect()
}

#[test]
fn first_pass_fires_everything() {
    let mut t = Timers::new(66, 16, 200);
    let p = t.pass(0, 1.0);
    assert_eq!(
        p,
        Pass {
            draw: true,
            interface: Some(66),
            step: Some(200)
        }
    );
}

#[test]
fn fast_machine_ten_seconds() {
    // Spec 11.2: a pass per millisecond, from a moment when all three fire.
    let mut t = Timers::new(66, 16, 200);
    t.pass(0, 1.0);
    let passes = run(&mut t, 1..=10_000);
    let draws = passes.iter().filter(|(_, p)| p.draw).count();
    let interfaces: Vec<_> = passes.iter().filter_map(|(_, p)| p.interface).collect();
    let steps: Vec<_> = passes.iter().filter_map(|(_, p)| p.step).collect();
    assert_eq!((draws, interfaces.len(), steps.len()), (625, 151, 149));
    assert!(interfaces.iter().all(|&n| n == 66));
    assert!(steps.iter().all(|&n| n == 67));
    let times = |f: fn(&Pass) -> bool| -> Vec<u64> {
        passes
            .iter()
            .filter(|(_, p)| f(p))
            .map(|(n, _)| *n)
            .take(3)
            .collect()
    };
    assert_eq!(times(|p| p.draw), [16, 32, 48]);
    assert_eq!(times(|p| p.interface.is_some()), [66, 132, 198]);
    assert_eq!(times(|p| p.step.is_some()), [67, 134, 201]);
}

#[test]
fn sixty_passes_a_second_alternate_83_and_67() {
    // Spec 11.3: the clock reads 16, 33, 50, 66, 83, 100, 116, 133, 150, ...
    let mut t = Timers::new(66, 16, 200);
    t.pass(0, 1.0);
    let passes = run(&mut t, (1..=120).map(|k| k * 1000 / 60));
    let steps: Vec<_> = passes.iter().filter_map(|(_, p)| p.step).collect();
    assert_eq!(&steps[..6], [83, 67, 83, 67, 83, 67]);
    assert!(steps.iter().all(|&n| n == 83 || n == 67));
    // The same total of simulated time: 2 s of real time, minus the part not yet stepped.
    let last = passes
        .iter()
        .rev()
        .find(|(_, p)| p.step.is_some())
        .unwrap()
        .0;
    assert_eq!(steps.iter().sum::<u32>() as u64, last);
}

#[test]
fn coarse_clock_steps_78() {
    // Spec 11.4: jumps of 15.625 ms; the step fires at the fifth jump.
    let mut t = Timers::new(66, 16, 200);
    t.pass(0, 1.0);
    let passes = run(&mut t, (1..=64).map(|k| (k as f64 * 15.625) as u64));
    let steps: Vec<_> = passes.iter().filter_map(|(_, p)| p.step).collect();
    assert_eq!(steps[0], 78);
    assert!(steps.iter().all(|&n| n == 78 || n == 79), "{steps:?}");
}

#[test]
fn step_for_elapsed_and_speed() {
    // Spec 11.5, cap 200, update period 66.
    let table: [(u64, [Option<u32>; 5]); 7] = [
        (16, [None; 5]),
        (66, [None; 5]),
        (67, [Some(33), Some(67), Some(100), Some(134), Some(200)]),
        (80, [Some(39), Some(80), Some(120), Some(160), Some(200)]),
        (100, [Some(49), Some(100), Some(150), Some(200), Some(200)]),
        (200, [Some(99), Some(200), Some(200), Some(200), Some(200)]),
        (500, [Some(200); 5]),
    ];
    for (elapsed, expected) in table {
        for (speed, expected) in [
            0.49999991059303284,
            1.0,
            1.5000001192092896,
            2.000000238418579,
            3.0,
        ]
        .into_iter()
        .zip(expected)
        {
            let mut t = Timers::new(66, 16, 200);
            t.pass(1000, 1.0);
            assert_eq!(
                t.pass(1000 + elapsed, speed).step,
                expected,
                "{elapsed} at {speed}"
            );
        }
    }
    // The product is rounded to 24 bits before it is truncated: 70 * 0.9 is 63, not 62.
    let mut t = Timers::new(66, 16, 200);
    t.pass(1000, 1.0);
    assert_eq!(t.pass(1070, 0.8999999761581421).step, Some(63));
    // The lowest ordinary developer speed.
    let mut t = Timers::new(66, 16, 200);
    t.pass(1000, 1.0);
    assert_eq!(t.pass(1067, 0.09999992698431015).step, Some(6));
    // Zero speed gives steps of 0; a negative or NaN product gives 0 too.
    let mut t = Timers::new(66, 16, 200);
    t.pass(1000, 1.0);
    assert_eq!(t.pass(1100, f64::NAN).step, Some(0));
}

#[test]
fn regular_passes() {
    // Spec 11.5: passes 16 ms apart, a step of 80 in the fifth and then every fifth.
    let mut t = Timers::new(66, 16, 200);
    t.pass(0, 1.0);
    let steps: Vec<_> = run(&mut t, (1..=15).map(|k| k * 16))
        .into_iter()
        .map(|(_, p)| p.step)
        .collect();
    let at = |i: usize| steps[i - 1];
    assert!((1..=4).all(|i| at(i).is_none()));
    assert_eq!((at(5), at(10), at(15)), (Some(80), Some(80), Some(80)));
    // Passes 67 ms apart: 67 every time. 500 ms apart: 200 every time.
    for (gap, expected) in [(67, 67), (500, 200)] {
        let mut t = Timers::new(66, 16, 200);
        t.pass(0, 1.0);
        assert!(
            run(&mut t, (1..=10).map(|k| k * gap))
                .iter()
                .all(|(_, p)| p.step == Some(expected)),
            "gap {gap}"
        );
    }
}

#[test]
fn a_load_makes_the_next_step_the_cap() {
    let mut t = Timers::new(66, 16, 200);
    t.pass(0, 1.0);
    t.pass(70, 1.0);
    t.forget_simulation();
    // Due at once, and as long as any gap: the cap, then ordinary steps again.
    assert_eq!(t.next_due(71), 0);
    assert_eq!(t.pass(71, 1.0).step, Some(200));
    assert_eq!(t.pass(100, 1.0).step, None);
    assert_eq!(t.pass(140, 1.0).step, Some(69));
}

#[test]
fn one_capped_step_after_a_stall() {
    // Spec 11.6: blocked for 5 s: one frame, one interface update of 66, one step of 200.
    let mut t = Timers::new(66, 16, 200);
    t.pass(0, 1.0);
    t.pass(70, 1.0);
    let p = t.pass(5070, 1.0);
    assert_eq!(
        p,
        Pass {
            draw: true,
            interface: Some(66),
            step: Some(200)
        }
    );
    // The pass after that is an ordinary one: nothing is due, and nothing catches up.
    assert_eq!(
        t.pass(5071, 1.0),
        Pass {
            draw: false,
            interface: None,
            step: None
        }
    );
    assert_eq!(t.pass(5087, 1.0).step, None);
    assert_eq!(t.pass(5137, 1.0).step, Some(67));
}

#[test]
fn clock_beyond_32_bits() {
    // 64-bit clock and subtraction form: a gap across 2^32 is an ordinary gap.
    let near = u64::from(u32::MAX) - 10;
    let mut t = Timers::new(66, 16, 200);
    t.pass(near, 1.0);
    let p = t.pass(near + 100, 1.0);
    assert_eq!(
        p,
        Pass {
            draw: true,
            interface: Some(66),
            step: Some(100)
        }
    );
}

#[test]
fn cap_is_unsigned() {
    let mut t = Timers::new(66, 16, u32::MAX);
    t.pass(0, 1.0);
    assert_eq!(t.pass(100_000, 1.0).step, Some(100_000));
}

#[test]
fn next_due_is_the_earliest_firing() {
    let mut t = Timers::new(66, 16, 200);
    assert_eq!(t.next_due(0), 0, "before the first pass");
    t.pass(1000, 1.0);
    assert_eq!(t.next_due(1000), 16);
    assert_eq!(t.next_due(1010), 6);
    assert_eq!(t.next_due(1016), 0);
    assert_eq!(t.next_due(5000), 0);
    // Sleeping until the due time reproduces the spinning loop's firings.
    let mut spinning = Timers::new(66, 16, 200);
    let mut sleeping = Timers::new(66, 16, 200);
    spinning.pass(0, 1.0);
    sleeping.pass(0, 1.0);
    let mut want = Vec::new();
    for now in 1..=1000 {
        let p = spinning.pass(now, 1.0);
        if p != (Pass {
            draw: false,
            interface: None,
            step: None,
        }) {
            want.push((now, p));
        }
    }
    let mut got = Vec::new();
    let mut now = 0;
    loop {
        now += sleeping.next_due(now).max(1);
        if now > 1000 {
            break;
        }
        got.push((now, sleeping.pass(now, 1.0)));
    }
    assert_eq!(got, want);
}

fn at(c: &Calendar) -> (u32, u32, u32, u32, u32, u32, u32) {
    let d = c.date();
    (
        d.year,
        d.month,
        d.day,
        d.hour,
        d.minute,
        d.second,
        d.millisecond,
    )
}

#[test]
fn a_timer_stamped_before_2001_ages_from_its_stamp() {
    // Stamped at 31 December 2000, 23:00, one hour before the start, and read at the start.
    let stamp = Calendar::at(-3_600_000);
    assert_eq!(stamp.date().year, 2000);
    assert_eq!(Calendar::at(0).elapsed_ms() - stamp.elapsed_ms(), 3_600_000);
}

#[test]
fn from_2001_on_the_timers_are_the_game_milliseconds() {
    assert_eq!(Calendar::at(0).elapsed_ms(), 0);
    assert_eq!(Calendar::at(3_600_000).elapsed_ms(), 3_600_000);
    assert_eq!(Calendar::at(34_560_000_000).elapsed_ms(), 34_560_000_000);
}

#[test]
fn calendar_starts_on_a_monday() {
    let c = Calendar::new();
    assert_eq!(at(&c), (2001, 1, 1, 0, 0, 0, 0));
    assert_eq!(c.date().weekday, 0);
}

fn after(ms: u32, step: u32) -> Calendar {
    let mut c = Calendar::new();
    for _ in 0..ms / step {
        c.advance(step);
    }
    c
}

#[test]
fn calendar_worked_example() {
    // Spec 11.7, steps of 67 ms.
    let mut c = Calendar::new();
    c.advance(67);
    assert_eq!(at(&c), (2001, 1, 1, 0, 8, 2, 400), "67 * 7.2 s");
    assert_eq!(at(&after(1005, 67)).3, 2, "1 s of simulated time: 02:00");
    assert_eq!(
        at(&after(12_000, 1)),
        (2001, 1, 2, 0, 0, 0, 0),
        "12 s: 2 January"
    );
    assert_eq!(
        at(&after(60_000, 1)),
        (2001, 1, 6, 0, 0, 0, 0),
        "60 s: 6 January"
    );
    assert_eq!(
        at(&after(372_000, 1)),
        (2001, 2, 1, 0, 0, 0, 0),
        "6 min 12 s: 1 February"
    );
    assert_eq!(
        at(&after(3_600_000, 1)),
        (2001, 10, 28, 0, 0, 0, 0),
        "one hour"
    );
    assert_eq!(after(12_000, 1).date().weekday, 1, "Tuesday");
}

#[test]
fn month_end_comes_one_step_late() {
    let mut c = Calendar::new();
    // 371.999 s: still January.
    for _ in 0..371_999 {
        assert!(!c.advance(1));
    }
    assert_eq!(c.date().month, 1);
    assert!(
        !c.advance(1),
        "the step that crosses midnight only raises the flag"
    );
    assert_eq!(c.date().month, 2);
    assert!(c.advance(1), "the next step runs the month end");
    assert!(!c.advance(1), "once");
}

#[test]
fn calendar_real_months_and_leap_years() {
    let day = 12_000; // ms of step per game day
    // February 2004 has 29 days.
    let c = after(day * (365 * 3 + 31 + 28), 1000);
    assert_eq!(at(&c), (2004, 2, 29, 0, 0, 0, 0));
    let c = after(day * (365 * 3 + 31 + 29), 1000);
    assert_eq!(at(&c), (2004, 3, 1, 0, 0, 0, 0));
    let c = after(day * (365 * 4 + 1), 1000);
    assert_eq!(at(&c), (2005, 1, 1, 0, 0, 0, 0), "a leap year has 366 days");
    // A year takes 73 minutes of simulated time.
    assert_eq!(at(&after(73 * 60_000, 1000)), (2002, 1, 1, 0, 0, 0, 0));
}
