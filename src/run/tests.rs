//! Tests of the command line and the scripts of `run`.

use super::*;

#[test]
fn out_dir_is_created_when_missing() {
    let dir = std::env::temp_dir().join(format!("rustyzoo-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let nested = dir.join("a").join("b");
    assert!(create_out_dir(&nested).is_ok());
    assert!(nested.is_dir());
    assert!(create_out_dir(&nested).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn loose_flags_ignore_the_words_that_are_not_flags() {
    let run = ["--headless", "--passes", "1", "--clock", "1"];
    let got = parse_flags(&[&run[..], &["extra.zoo"]].concat(), true);
    assert_eq!(got.unwrap_or_else(|_| panic!("refused")).passes, Some(1));
    let got = parse_flags(&[&["a.zoo", "b.zoo"][..], &run].concat(), true);
    assert!(got.is_ok());
    // A flag that is wrong stays wrong, and the strict form refuses any stray word.
    assert!(parse_flags(&[&run[..], &["--window"]].concat(), true).is_err());
    assert!(parse_flags(&[&run[..], &["--passes"]].concat(), true).is_err());
    assert!(parse_flags(&[&run[..], &["extra.zoo"]].concat(), false).is_err());
    assert!(parse_flags(&["x"], true).is_ok());
}

#[test]
fn flags_come_in_any_order_and_bad_ones_are_refused() {
    assert!(flags(&["--headless", "--replay", "log", "--png-every", "2"]).is_ok());
    assert!(matches!(
        flags(&[
            "--headless",
            "--passes",
            "1",
            "--clock",
            "1",
            "--size",
            "0x0"
        ]),
        Err(CliError::Failed(_))
    ));
    let got = flags(&[
        "--clock",
        "20",
        "--headless",
        "--passes",
        "5",
        "--size",
        "64x48",
    ]);
    let got = got.unwrap_or_else(|_| panic!("refused"));
    assert_eq!(
        (got.passes, got.clock, got.size),
        (Some(5), 20, Some((64, 48)))
    );
    for bad in [
        &["--passes", "5", "--clock", "20"][..],
        &["--headless", "--passes", "5"],
        &["--headless", "--passes", "x", "--clock", "1"],
        &[
            "--headless",
            "--passes",
            "1",
            "--clock",
            "1",
            "--png-every",
            "0",
        ],
        &[
            "--headless",
            "--passes",
            "1",
            "--clock",
            "1",
            "--size",
            "64",
        ],
        &["--headless", "--passes", "1", "--clock", "1", "--png-every"],
        &["--headless", "--passes", "1", "--clock", "1", "--window"],
        &[
            "--headless",
            "--replay",
            "log",
            "--passes",
            "1",
            "--clock",
            "1",
        ],
        &["--headless", "--replay", "log", "--size", "1x1"],
        &["--headless", "--replay", "log", "--script", "s"],
    ] {
        assert!(flags(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn rows_take_a_list_and_a_count_up_to_the_limit() {
    let rows = |words: &[&str]| {
        let mut all = vec!["--headless", "--passes", "1", "--clock", "1"];
        all.extend(words);
        flags(&all)
    };
    let got = rows(&["--rows", "11504", "30", "--rows", "3", "0"]).unwrap_or_else(|_| panic!());
    assert_eq!(got.fill.rows, [(11504, 30), (3, 0)]);
    assert!(matches!(
        rows(&["--rows", "1", "100001"]),
        Err(CliError::Failed(_))
    ));
    assert!(rows(&["--rows", "1", "100000"]).is_ok());
    for bad in [
        &["--rows", "1", "-1"][..],
        &["--rows", "x", "1"],
        &["--rows", "1"],
        &["--rows"],
    ] {
        assert!(matches!(rows(bad), Err(CliError::Usage)), "{bad:?}");
    }
    assert!(matches!(
        rows(&["--rows", "1", "99999999999999999999"]),
        Err(CliError::Usage)
    ));
    let got = rows(&["--text", "11510", "12 34"]).unwrap_or_else(|_| panic!());
    assert_eq!(got.fill.texts, [(11510, "12 34".to_string())]);
    assert!(matches!(rows(&["--text", "11510"]), Err(CliError::Usage)));
    assert!(matches!(rows(&["--text", "x", "a"]), Err(CliError::Usage)));
    let got = rows(&["--ask", "45000", "yes,no"]).unwrap_or_else(|_| panic!());
    assert_eq!(got.fill.ask, Some((45000, Buttons::YES_NO)));
    for bad in [
        &["--ask", "45000", "maybe"][..],
        &["--ask", "45000", ""],
        &["--ask", "x", "ok"],
        &["--ask", "45000"],
    ] {
        assert!(matches!(rows(bad), Err(CliError::Usage)), "{bad:?}");
    }
}

#[test]
fn bare_is_a_flag_of_both_kinds_of_run() {
    let got = flags(&["--bare", "--headless", "--passes", "1", "--clock", "1"]);
    assert!(got.unwrap_or_else(|_| panic!("refused")).bare);
    assert!(
        flags(&["--bare"])
            .unwrap_or_else(|_| panic!("refused"))
            .bare
    );
    assert!(!flags(&[]).unwrap_or_else(|_| panic!("refused")).bare);
    assert!(flags(&["--bare", "x"]).is_err());
}

#[test]
fn seed_is_a_whole_number_of_32_bits_and_the_flag_needs_one() {
    let seed = |words: &[&str]| {
        let mut all = vec!["--headless", "--passes", "1", "--clock", "1"];
        all.extend(words);
        flags(&all)
    };
    for (text, want) in [("0", 0), ("1", 1), ("4294967295", u32::MAX)] {
        let got = seed(&["--seed", text]).unwrap_or_else(|_| panic!("{text}"));
        assert_eq!(got.seed, Some(want), "{text}");
    }
    assert_eq!(seed(&[]).unwrap_or_else(|_| panic!()).seed, None);
    for bad in ["-1", "4294967296", "", "x", "1.5"] {
        assert!(
            matches!(seed(&["--seed", bad]), Err(CliError::Failed(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(seed(&["--seed"]), Err(CliError::Usage)));
}

#[test]
fn cash_is_any_finite_number_and_the_flag_needs_one() {
    let cash = |words: &[&str]| {
        let mut all = vec!["--headless", "--passes", "1", "--clock", "1"];
        all.extend(words);
        flags(&all)
    };
    for (text, want) in [
        ("0", 0.0),
        ("-250.5", -250.5),
        ("100000", 100_000.0),
        ("3e38", 3e38),
    ] {
        let got = cash(&["--cash", text]).unwrap_or_else(|_| panic!("{text}"));
        assert_eq!(got.cash, Some(want), "{text}");
    }
    assert_eq!(cash(&[]).unwrap_or_else(|_| panic!()).cash, None);
    for bad in ["1e39", "-1e39", "inf", "nan", "", "x", "1,5"] {
        assert!(
            matches!(cash(&["--cash", bad]), Err(CliError::Failed(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(cash(&["--cash"]), Err(CliError::Usage)));
    assert!(flags(&["--cash", "5"]).is_ok(), "a window takes it too");
}

#[test]
fn a_window_takes_its_own_flags() {
    let got = flags(&[
        "--size", "64x48", "--log", "l", "--passes", "3", "--script", "s",
    ]);
    let got = got.unwrap_or_else(|_| panic!("refused"));
    assert!(!got.headless);
    assert_eq!((got.log.as_deref(), got.passes), (Some("l"), Some(3)));
    let got = flags(&["--shot", "s.png"]);
    let got = got.unwrap_or_else(|_| panic!("refused"));
    assert_eq!(got.shot.as_deref(), Some("s.png"));
    assert!(flags(&[]).is_ok());
    for bad in [
        &["--clock", "1"][..],
        &["--replay", "log"],
        &["--png-every", "1"],
        &["--out", "d"],
        &[
            "--headless",
            "--passes",
            "1",
            "--clock",
            "1",
            "--shot",
            "s.png",
        ],
        &["--headless", "--passes", "1", "--clock", "1", "--log", "l"],
        &["--passes", "0"],
        &["--headless", "--passes", "0", "--clock", "1"],
    ] {
        assert!(flags(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_script_is_read_in_file_order_per_pass() {
    let text = "# comment\n\n3 pause\n0 scroll 1 0\n 3  rotate cw \n";
    assert_eq!(
        script(text).unwrap(),
        [
            (0, Command::Scroll(1, 0)),
            (3, Command::Pause),
            (3, Command::Rotate(true))
        ]
    );
    for bad in ["x pause", "3", "3 fly", "-1 pause"] {
        assert!(script(bad).is_err(), "{bad:?}");
    }
    assert!(script("4 fly").unwrap_err().starts_with("script line 1:"));
}

#[test]
fn a_log_is_read_back_as_passes_with_their_clock_and_commands() {
    let log = "size 64x48\n\
               pass 0 t=0 draw 0123456789abcdef view=1,2 rotation=0 zoom=0\n\
               pass 0 t=0 step 200 2001-01-01 00:24:00.000\n\
               pass 0 t=0 sim seed 1700000000\n\
               pass 0 t=0 sim seed 4294967295\n\
               pass 3 t=67 scroll -1 0\n\
               pass 3 t=67 pointer none\n\
               pass 3 t=67 interface 66\n\
               pass 3 t=67 music fade 33 sum -967 gain 0.999\n\
               pass 4 t=70 quit\n\
               pass 5 t=80 tool down 12,40\n\
               pass 5 t=80 tool up none\n\
               pass 5 t=80 world stage\n\
               pass 5 t=80 exhibit 2\n\
               pass 5 t=80 terrain paint 4\n\
               pass 5 t=80 staff job 1 PutFood\n\
               pass 5 t=80 dev speed 1.1\n\
               pass 5 t=80 building use 7 1\n\
               pass 5 t=80 building buy 7 hotdog 2\n";
    assert_eq!(
        replay(log).unwrap(),
        (
            (64, 48),
            (
                vec![
                    (0, 0, vec![]),
                    (3, 67, vec![Command::Scroll(-1, 0), Command::Pointer(None)]),
                    (4, 70, vec![Command::Quit]),
                    (5, 80, vec![]),
                ],
                vec![1_700_000_000, u32::MAX]
            )
        )
    );
    for bad in [
        "",
        "pass 0 t=0 draw -",
        "size 64\n",
        "size 64x48\npass 0 draw",
        "size 64x48\npass x t=0 pause",
        "size 64x48\npass 0 t=0 fly",
        "size 64x48\npass 0 t=0 sim seed -1",
        "size 64x48\npass 0 t=0 sim seed 4294967296",
    ] {
        assert!(replay(bad).is_err(), "{bad:?}");
    }
}
