use super::*;

fn parse(text: &str) -> Result<Behaviours, BehaviourError> {
    Behaviours::parse(&Config::parse(text.as_bytes()).unwrap())
}

fn call(text: &str) -> (Call, Option<&'static str>) {
    parse_call(text.as_bytes()).unwrap_or_else(|| panic!("unknown: {text}"))
}

fn op(text: &str) -> Op {
    call(text).0.op
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_call_is_split_as_the_original_does() {
    let split = |t: &str| {
        let (name, args) = split_call(t.as_bytes());
        (name.map(|n| String::from_utf8_lossy(n).into_owned()), args)
    };
    assert_eq!(
        split("fPlay(stand)"),
        (Some("fPlay".into()), strings(&["stand"]))
    );
    // `f()` has one empty argument, `f(` none.
    assert_eq!(split("fDie()"), (Some("fDie".into()), strings(&[""])));
    assert_eq!(
        split("fMove(prey, 0, \"run\" ,66)"),
        (Some("fMove".into()), strings(&["prey", "0", "run", "66"]))
    );
    // A trailing comma adds nothing; an empty argument in the middle stays.
    assert_eq!(split("f(a,70,b,30,)").1, strings(&["a", "70", "b", "30"]));
    assert_eq!(split("f(a,,b)").1, strings(&["a", "", "b"]));
    assert_eq!(split("f(,)").1, strings(&[""]));
    // No closing parenthesis: no arguments. No name: only a `(` that is not last gives one.
    assert_eq!(
        split("fPlay(lie_idle"),
        (Some("fPlay".into()), strings(&[]))
    );
    assert_eq!(split("fDie(").0, None);
    assert_eq!(split("fDie").0, None);
    assert_eq!(split("(x)").0, Some(String::new()));
    // The name keeps a space before the parenthesis.
    assert_eq!(split("fPlay (x)").0, Some("fPlay ".into()));
}

#[test]
fn every_known_name_parses_and_the_others_do_not() {
    for name in NAMES {
        let text = format!("{name}()");
        assert_eq!(call(&text).0.name, name);
    }
    assert_eq!(NAMES.len(), KNOWN + 3);
    let distinct: std::collections::HashSet<_> = NAMES.iter().collect();
    assert_eq!(distinct.len(), NAMES.len());
    for text in [
        "fplay(stand)",
        "fPlay (x)",
        "fPlays(x)",
        "fDie",
        "fDie(",
        "x(",
        "(x)",
        "",
    ] {
        assert!(parse_call(text.as_bytes()).is_none(), "{text}");
    }
}

#[test]
fn the_names_the_original_does_not_know_fail_when_run() {
    for text in [
        "fFollow(keeper)",
        "fPlayWithSoud(a,b)",
        "fPlayWith(shake,shake)",
    ] {
        let (call, defect) = call(text);
        assert_eq!((call.op, defect), (Op::Fail, None), "{text}");
    }
}

#[test]
fn arguments_are_parsed_per_operation() {
    assert_eq!(
        op("fPlayPingPong(a,b)"),
        Op::Play(Play::PingPong, strings(&["a", "b"]))
    );
    assert_eq!(
        op("fPingPong(a)"),
        Op::Play(Play::PingPong, strings(&["a"]))
    );
    assert_eq!(
        op("fPlayWithSound(stand,grunt)"),
        Op::PlayWithSound {
            anim: "stand".into(),
            sound: "grunt".into()
        }
    );
    assert_eq!(
        op("fPlayWithSound(stand)"),
        Op::PlayWithSound {
            anim: "stand".into(),
            sound: "stand".into()
        }
    );
    assert_eq!(
        op("fPlayTime(stand, 5bSubdig)"),
        Op::PlayTime {
            anim: "stand".into(),
            time: Number::Int(5)
        }
    );
    assert_eq!(
        op("fPlayTime(talk, cInformGuestTime)"),
        Op::PlayTime {
            anim: "talk".into(),
            time: Number::Characteristic("cInformGuestTime".into())
        }
    );
    assert_eq!(
        op("fPlaySetProb(a,70,b,30)"),
        Op::PlaySetProb(vec![("a".into(), 70), ("b".into(), 30)])
    );
    assert_eq!(
        op("fPlaySetTerrain(l,w,s,u)"),
        Op::PlaySetTerrain {
            land: "l".into(),
            water: "w".into(),
            tank: Some(("s".into(), "u".into()))
        }
    );
    assert_eq!(
        op("fPlaySetTank(a,b)"),
        Op::PlaySetTank {
            land: "a".into(),
            tank: "b".into()
        }
    );
    let mv = |gait, reverse, target, time| Op::Move {
        gait,
        reverse,
        target,
        time: Number::Int(time),
    };
    assert_eq!(
        op("fWalk(keeperfood,0)"),
        mv(Gait::Slow, false, Target::None, 0)
    );
    assert_eq!(op("fRun(dive,2)"), mv(Gait::Fast, false, Target::Dive, 2));
    let own = |sound: Option<&str>, percent| Gait::Own {
        anim: "run".into(),
        speed: Number::Int(66),
        percent: Number::Int(percent),
        sound: sound.map(String::from),
    };
    assert_eq!(
        op("fMove(prey,0,\"run\",66)"),
        mv(own(None, 100), false, Target::None, 0)
    );
    assert_eq!(
        op("fMoveReverse(0,0,run,66,50)"),
        mv(own(None, 50), true, Target::Keep, 0)
    );
    assert_eq!(
        op("fMoveWithSound(level,0,run,66,snd)"),
        mv(own(Some("snd"), 100), false, Target::Level, 0)
    );
    assert_eq!(op("fKeeperRoutine()"), Op::Routine(Routine::Keeper));
    assert_eq!(op("fPlaySp(a,b)"), Op::Show(strings(&["a", "b"])));
    assert_eq!(op("fDie(ignored)"), Op::Die);
}

#[test]
fn numbers_of_calls_are_read_like_atol() {
    for (text, value) in [
        ("0x64", 0),
        ("-5x", -5),
        (" +7", 7),
        ("run", 0),
        ("", 0),
        ("12ab", 12),
    ] {
        assert_eq!(atol(text), value, "{text:?}");
    }
    assert_eq!(number("0x10"), Number::Int(0));
    // The weights 0x64 are 0: the sum is not 100.
    let (call, defect) = call("fPlaySetProb(bB,0x64)");
    assert_eq!((call.op, defect.is_some()), (Op::Fail, true));
    assert_eq!(
        op("fPlaySetProb(bA,60,bB,40)"),
        Op::PlaySetProb(vec![("bA".into(), 60), ("bB".into(), 40)])
    );
}

#[test]
fn bad_arguments_make_the_call_fail_with_a_reason() {
    for text in [
        "fPlay(lie_idle",
        "fPlayTime(stand)",
        "fPlayTime(a,1,2)",
        "fPlayWithBase(a)",
        "fPlaySet(a,b)",
        "fPlaySetTerrain(a)",
        "fPlaySetTank(a)",
        "fTrot(keeper)",
        "fMove(0,0,run)",
        "fMoveWithSound(0,0,run,66)",
        "fFaceDirection()",
        "fFly(0)",
        "fPlaySetProb(a,70)",
        "fPlaySetProb(a,70,b,30,c)",
        "fPlaySetProb(a,30,b,10,c,20,d,20)",
        "fPlaySetProb(a,2147483647,b,2147483647,c,6)",
    ] {
        let (call, defect) = call(text);
        assert_eq!(call.op, Op::Fail, "{text}");
        assert!(defect.is_some(), "{text}");
    }
}

#[test]
fn a_file_gives_sets_with_nested_sets_and_tables() {
    let found = parse(
        "[f\\BehaviorSet\\bCalm]\nf = fPlaySetProb(bIdle,60,bLook,40)\n\
         [f\\BehaviorSet\\bIdle]\nf = fPlay(stand)\nf = fPlaySet(bLook)\nname = ignored\n\
         [f\\BehaviorSet\\bLook]\nf = fHalt()\nf = fPlayTime(stand)\n\
         [BehaviorSet\\bEmpty]\nx = 1\n\
         [AmbientAnims]\na = 21\na = 100\nb = bCalm\na = -100\na = 20\nb = bIdle\n\
         [AmbientAnimsWater]\na = 1\nb = bWater\n",
    )
    .unwrap();
    assert_eq!(found.sets().count(), 4);
    let idle = found.find(&[b"f"], b"bIdle").unwrap();
    assert_eq!(idle.len(), 2);
    assert_eq!(idle[1].op, Op::PlaySet("bLook".into()));
    assert!(found.find(&[b"f", b""], b"bEmpty").is_none());
    // The call that fails is kept, in its place, and warned about.
    assert_eq!(found.find(&[b"f"], b"bLook").unwrap()[1].op, Op::Fail);
    assert_eq!(found.warnings.len(), 2);
    assert!(
        found.warnings[0].contains("[f\\BehaviorSet\\bLook], call 2"),
        "{:?}",
        found.warnings
    );
    assert_eq!(
        found.tables[0],
        [
            Row {
                lo: 21,
                hi: 100,
                set: "bCalm".into()
            },
            Row {
                lo: -100,
                hi: 20,
                set: "bIdle".into()
            }
        ]
    );
    // Twice as many bounds as sets is the rule: this table is ignored, with a warning.
    assert!(found.tables[1].is_empty());
    assert!(found.warnings[1].contains("AmbientAnimsWater"));
}

#[test]
fn an_unknown_name_is_an_error_with_its_section_call_and_line() {
    let text = "; comment\n[Global]\nf = fPlay(x)\n\n[f\\BehaviorSet\\bOne]\nf = fPlay(a)\n\
                [other]\nf = fNope(1)\n[f\\BehaviorSet\\bOne]\nx = 1\nf = fWait(2)\nf = fPlay(c)\n";
    let error = parse(text).unwrap_err();
    assert_eq!(
        (error.section.as_str(), error.call, error.text.as_str()),
        ("f\\BehaviorSet\\bOne", 2, "fWait(2)")
    );
    assert_eq!(error.line(text.as_bytes()), Some(11));
    assert_eq!(
        error.to_string(),
        "section [f\\BehaviorSet\\bOne], call 2: unknown operation in fWait(2)"
    );
    // A call with no `(` has no name.
    assert!(parse("[BehaviorSet\\x]\nf = fHalt\n").is_err());
}

#[test]
fn hostile_text_does_not_panic() {
    for text in [
        "f = ((((",
        "f = )",
        "f = ,,,,",
        "f = fPlaySetProb(,,,,)",
        "f = \"\"\"",
    ] {
        let _ = parse(&format!("[BehaviorSet\\x]\n{text}\n"));
    }
    for bytes in [&b"\xff\xfe(\x00)"[..], b"(", b")", b"()", b"a(b"] {
        let _ = parse_call(bytes);
    }
}

/// The numbers of the specification's survey of the full game.
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_survey() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let found = survey(&Vfs::open(std::path::Path::new(&dir)).unwrap()).unwrap();
    // The French install ships other files, so its totals differ.
    let french = crate::pe::installed_language(&dir) == 1036;
    let (counts, rest, play) = if french {
        ((261, 20189, 60252, 430), (274, 32), 25295)
    } else {
        ((263, 20299, 60727, 432), (276, 33), 25538)
    };
    assert_eq!((found.files, found.sets, found.calls, found.tables), counts);
    assert_eq!((found.unknown, found.defects), rest);
    assert_eq!(found.counts["fPlay"], play);
}

#[test]
fn the_library_reads_sets_tables_timings_numbers_and_sounds_of_a_type() {
    use crate::sprite::tests::{Install, sized, sprite_file};
    let mut frame = Vec::new();
    for field in [1u16, 1, 0, 0, 0] {
        frame.extend(field.to_le_bytes());
    }
    frame.extend([1, 0, 1, 1]);
    let definition = b"[Animations]\nidle = idle\nwalk = walk\nbroken = nofile\n\
        [BehaviorSet\\bZooRoutine]\nf = fGuestRoutine()\n\
        [AmbientAnims]\na = -100\na = 100\nb = bZooRoutine\n\
        [Characteristics/Integers]\ncInformGuestTime = 7\n\
        [Global]\nDefaultSubtype = man\n[man/Characteristics/Integers]\ncSlowRate = 33\n\
        [Sounds]\ngrunt = afrbuf1\ngrunt = 1500\nyell = Sounds/Yell.wav\nnoise = x.WAV\n"
        .to_vec();
    let ani =
        |dir: &str| format!("[animation]\ndir0 = a\ndir1 = {dir}\nanimation = N\n").into_bytes();
    let install = Install::new(
        "library",
        &[
            ("guests.cfg", b"[guests]\nman = man.ai\n".to_vec()),
            ("man.ai", definition),
            (
                "ztunit.ai",
                b"[Characteristics/Integers]\ncNonPathCost = 5\nCNonPathCostEmergency = x\n"
                    .to_vec(),
            ),
            ("guests/man/idle/idle.ani", ani("idle")),
            ("guests/man/walk/walk.ani", ani("walk")),
            ("guests/man/broken/broken.ani", ani("broken")),
            (
                "a/idle/N",
                sprite_file(None, 2, &[sized(&frame), sized(&frame)]),
            ),
            ("a/walk/N", sprite_file(None, 1, &[sized(&frame)])),
        ],
    );
    let vfs = Vfs::open(&install.0).unwrap();
    let catalogue = crate::entity::Catalogue::load(&vfs).unwrap();
    let timings = Timings::default();
    let library = Library {
        catalogue: &catalogue,
        vfs: &vfs,
        timings: &timings,
    };
    let guest = |type_name: &str| crate::zoo::Entity {
        class: "guests".into(),
        type_name: type_name.into(),
        subtype: String::new(),
        body_size: None,
        common: None,
    };
    let man = guest("man");
    assert_eq!(library.set(&man, "bZooRoutine").map(<[Call]>::len), Some(1));
    assert!(library.set(&man, "bNone").is_none());
    assert_eq!(library.ambient(&man, 0).len(), 1);
    // The frames and the delay come from the first sprite file; a name the type lacks, and an
    // animation whose files are missing, have none, and asking again gives the same answer.
    for _ in 0..2 {
        assert_eq!(
            library.timing(&man, "idle"),
            Some(Timing {
                count: 2,
                delay: 125
            })
        );
        assert_eq!(
            library.timing(&man, "walk"),
            Some(Timing {
                count: 1,
                delay: 125
            })
        );
        assert_eq!(library.timing(&man, "run"), None);
        assert_eq!(library.timing(&man, "broken"), None);
    }
    assert_eq!(library.number(&man, &Number::Int(-4)), -4);
    assert_eq!(
        library.number(&man, &Number::Characteristic("cInformGuestTime".into())),
        7
    );
    assert_eq!(
        library.number(&man, &Number::Characteristic("cWalkRandomlyTime1".into())),
        0
    );
    // A guest's rate is the default subtype's.
    let rate = Number::Characteristic("cSlowRate".into());
    assert_eq!(library.number(&man, &rate), 33);
    // A short name is a file of the type's folder; a value with `.wav` is the path as written.
    // The second value of the key is the base term, 0 when absent; each key has its own voice.
    let sound = |key| library.sound(&man, key).map(|h| (h.file, h.base, h.voice));
    let heard =
        |file: &str, base, key: &str| Some((file.into(), base, format!("guests/man/{key}")));
    assert_eq!(
        sound("grunt"),
        heard("guests/man/afrbuf1.wav", 1500, "grunt")
    );
    assert_eq!(sound("yell"), heard("sounds/yell.wav", 0, "yell"));
    assert_eq!(sound("noise"), heard("guests/man/x.wav.wav", 0, "noise"));
    assert_eq!(library.sound(&man, "roar"), None);
    // The unit class file: a whole number by key (case does not matter), nothing for a key that
    // is missing or has no number.
    let class = |key| library.unit_class(key);
    assert_eq!(class("cNonPathCost"), Some(5));
    assert_eq!(class("cnonpathcost"), Some(5));
    assert_eq!(
        (class("CNonPathCostEmergency"), class("cNone"), class("")),
        (None, None, None)
    );
    // A unit of a type the catalogue lacks has nothing.
    let other = guest("woman");
    assert!(library.set(&other, "bZooRoutine").is_none() && library.ambient(&other, 0).is_empty());
    assert_eq!(library.timing(&other, "idle"), None);
}
