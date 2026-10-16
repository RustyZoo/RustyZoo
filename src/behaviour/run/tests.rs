use super::*;
use crate::behaviour::parse_call;
use std::collections::HashMap;

/// A unit type made of text: the sets as lists of calls, the ambient rows and the animations
/// with their frames and delays.
#[derive(Default)]
struct Fake {
    sets: HashMap<String, Vec<Call>>,
    rows: Vec<Row>,
    timings: HashMap<String, Timing>,
    sounds: HashMap<String, String>,
}

impl Data for Fake {
    fn set(&self, _: &Entity, name: &str) -> Option<&[Call]> {
        self.sets
            .get(name)
            .filter(|c| !c.is_empty())
            .map(Vec::as_slice)
    }
    fn ambient(&self, _: &Entity) -> &[Row] {
        &self.rows
    }
    fn timing(&self, _: &Entity, name: &str) -> Option<Timing> {
        self.timings.get(name).copied()
    }
    fn number(&self, unit: &Entity, number: &Number) -> i32 {
        NoData.number(unit, number)
    }
    fn sound(&self, _: &Entity, key: &str) -> Option<Heard> {
        self.sounds.get(key).map(|file| Heard {
            file: file.clone(),
            base: 0,
            voice: key.into(),
        })
    }
}

/// A unit with the animations `a` (3 frames of 100 ms), `b` (2 of 500), `c` (1 of 100) and `d`
/// (2 of 100), and the ambient row `bAmbient` for every happiness.
struct Rig {
    data: Fake,
    anim: Animation,
    runner: Runner,
    rng: Rng,
    unit: Entity,
}

impl Rig {
    fn new(sets: &[(&str, &[&str])]) -> Rig {
        let mut data = Fake::default();
        for (name, calls) in sets {
            let calls = calls.iter().map(|c| parse_call(c.as_bytes()).unwrap().0);
            data.sets.insert(name.to_string(), calls.collect());
        }
        for (name, count, delay) in [
            ("idle", 1, 100),
            ("a", 3, 100),
            ("b", 2, 500),
            ("c", 1, 100),
            ("d", 2, 100),
        ] {
            data.timings.insert(name.into(), Timing { count, delay });
        }
        data.rows = vec![Row {
            lo: -100,
            hi: 100,
            set: "bAmbient".into(),
        }];
        Rig {
            data,
            anim: Animation::default(),
            runner: Runner::default(),
            rng: Rng::default(),
            unit: Entity {
                class: "animals".into(),
                type_name: String::new(),
                subtype: String::new(),
                body_size: None,
                common: None,
            },
        }
    }

    /// One step of the unit's turn: the animation advances, then the set runs.
    fn tick(&mut self, ms: u32) -> (String, u32) {
        let data = &self.data;
        self.anim.advance(ms, &|n| data.timing(&self.unit, n));
        let mut env = Env {
            data,
            unit: &self.unit,
            rng: &mut self.rng,
        };
        self.runner.step(&mut self.anim, &mut env, 0);
        (self.anim.name.clone(), self.anim.shown())
    }

    fn notes(&mut self) -> Vec<Note> {
        std::mem::take(&mut self.anim.notes)
    }
}

fn set(name: &str) -> Note {
    Note::Set(name.into())
}

fn anim(name: &str) -> Note {
    Note::Anim(name.into())
}

#[test]
fn a_set_of_four_calls_goes_through_its_states_and_ends_with_the_last_sequence_playing() {
    let mut rig = Rig::new(&[(
        "bAmbient",
        &["fPlay(a)", "fPlayTime(b,2)", "fPlayReverse(a)", "fPlay(c)"],
    )]);
    // The first step: the base animation shows, the ambient set starts and its first call queues
    // `a` behind the base loop.
    assert_eq!(rig.tick(100), ("idle".into(), 0));
    assert_eq!(rig.notes(), [anim("idle"), set("bAmbient")]);
    assert_eq!(rig.runner.top(), Some("bAmbient"));
    // The loop of the base ends and the player takes `a`, which is three frames of 100 ms. The
    // call is done in that step and the next one queues behind `a`.
    assert_eq!(rig.tick(100), ("a".into(), 0));
    assert_eq!(rig.notes(), [anim("a")]);
    assert_eq!(rig.tick(100), ("a".into(), 1));
    assert_eq!(rig.tick(100), ("a".into(), 2));
    // Two turns of `b` (2 s of 1 s loops), back to back with `a`.
    assert_eq!(rig.tick(100), ("b".into(), 0));
    assert_eq!(rig.tick(500), ("b".into(), 1));
    assert_eq!(rig.tick(500), ("b".into(), 0));
    assert_eq!(rig.tick(500), ("b".into(), 1));
    // Then `a` reversed: the first counted frame shows the last.
    assert_eq!(rig.tick(500), ("a".into(), 2));
    assert_eq!(rig.tick(100), ("a".into(), 1));
    assert_eq!(rig.tick(100), ("a".into(), 0));
    assert_eq!(rig.tick(100), ("c".into(), 0));
    // The last call is done as the player takes its sequence, and the set ends while it plays.
    assert!(!rig.runner.running());
    // `c` is one frame: the next step shows the base and chooses a set again.
    rig.notes();
    assert_eq!(rig.tick(100), ("idle".into(), 0));
    assert_eq!(rig.notes(), [anim("idle"), set("bAmbient")]);
    assert!(rig.runner.running());
}

#[test]
fn calls_that_are_done_at_once_chain_in_the_step_and_a_play_waits_for_the_next() {
    let mut rig = Rig::new(&[(
        "bAmbient",
        &[
            "fFaceTowardFood()",
            "fHalt()",
            "fPlay(d)",
            "fFaceTowardTarget()",
            "fPlay(c)",
        ],
    )]);
    // `d` queues behind the base loop; as the player takes it the call is done, the face is
    // done at once and `c` queues behind it, in the same step.
    assert_eq!(rig.tick(100).0, "idle");
    assert_eq!(rig.tick(100).0, "d");
    assert_eq!(rig.tick(100).0, "d");
    assert_eq!(rig.tick(100).0, "c");
}

#[test]
fn a_nested_set_runs_in_place_of_its_calls_and_a_failure_ends_the_chain() {
    let mut rig = Rig::new(&[
        ("bAmbient", &["fPlaySet(bChild)", "fPlay(c)"]),
        ("bChild", &["fPlay(d)", "fPlay(nope)", "fPlay(a)"]),
    ]);
    // The nested set is created by the call, and runs from the next step.
    assert_eq!(rig.tick(100).0, "idle");
    // `d` queues behind the base loop.
    assert_eq!(rig.tick(100).0, "idle");
    assert!(rig.runner.running());
    // The player takes `d`, which completes the call; then `nope` is not an animation of the
    // type: the play fails, the child ends, and so does the parent without its `fPlay(c)`.
    assert_eq!(rig.tick(100).0, "d");
    assert!(!rig.runner.running());
    let names: Vec<_> = rig
        .notes()
        .into_iter()
        .filter(|n| matches!(n, Note::Anim(_)))
        .collect();
    assert_eq!(names, [anim("idle"), anim("d")]);

    // A nested set that has no call fails its parent.
    let mut rig = Rig::new(&[("bAmbient", &["fPlaySet(bNone)", "fPlay(c)"])]);
    rig.tick(100);
    rig.tick(100);
    assert!(!rig.runner.running());
    assert_eq!(rig.anim.name, "idle");

    // A set that nests itself ends at the depth cap instead of growing.
    let mut rig = Rig::new(&[("bAmbient", &["fPlaySet(bAmbient)"])]);
    for _ in 0..40 {
        rig.tick(100);
    }
    assert!(rig.runner.chain.len() <= DEPTH);
}

#[test]
fn a_call_that_cannot_run_ends_its_set() {
    for call in [
        "fFollow(keeper)",
        "fPlayTime(a)",
        "fPlay(lie_idle",
        "fPlay()",
        "fPlay(zz)",
    ] {
        let mut rig = Rig::new(&[("bAmbient", &[call, "fPlay(a)"])]);
        for _ in 0..3 {
            assert_eq!(rig.tick(100).0, "idle", "{call}");
        }
    }
}

#[test]
fn the_calls_of_the_other_issues_are_done_at_once_or_never() {
    for (call, finishes) in [
        ("fWalk(0,0)", true),
        ("fDie()", true),
        ("fPlaySpecial(x)", true),
        ("fKeepControl()", false),
        ("fGuestRoutine()", false),
        ("fTargetBuilding()", false),
    ] {
        let mut rig = Rig::new(&[("bAmbient", &[call, "fPlay(d)"])]);
        rig.tick(100);
        assert_eq!(rig.tick(100).0 == "d", finishes, "{call}");
    }
}

#[test]
fn a_routine_call_is_done_for_its_own_class_and_the_keepers_fails_for_others() {
    // (class, call, done, still running): the other classes' calls do nothing and stay.
    for (class, call, done, running) in [
        ("guests", "fGuestRoutine()", true, false),
        ("keeper", "fKeeperRoutine()", true, false),
        ("tour", "fGuideRoutine()", true, false),
        ("maint", "fMaintRoutine()", true, false),
        ("animals", "fGuestRoutine()", false, true),
        ("guests", "fMaintRoutine()", false, true),
        ("guests", "fKeeperRoutine()", false, false),
    ] {
        let mut rig = Rig::new(&[("bAmbient", &[call, "fPlay(d)"])]);
        rig.unit.class = class.into();
        rig.tick(100);
        assert_eq!(rig.tick(100).0 == "d", done, "{class} {call}");
        assert_eq!(rig.runner.running(), running, "{class} {call}");
    }
}

#[test]
fn a_weighted_set_is_drawn_as_the_original_draws_it() {
    let sets = [
        ("bAmbient", &["fPlaySetProb(bX,20,bY,50,bZ,30)"][..]),
        ("bX", &["fPlay(a)"]),
        ("bY", &["fPlay(c)"]),
        ("bZ", &["fPlay(d)"]),
    ];
    let mut seen = [0; 3];
    for seed in 0..200 {
        let mut rig = Rig::new(&sets);
        rig.rng.seed(seed);
        // What the original computes from the same generator.
        let mut by_hand = rig.rng.clone();
        let mut draw = by_hand.rand() % 100 + 1;
        let mut want = None;
        let mut left = 100;
        for (i, w) in [20, 50, 30].into_iter().enumerate() {
            if draw <= w {
                want = Some(i);
                break;
            }
            left -= w;
            draw = by_hand.rand() % left + 1;
        }
        rig.tick(100);
        rig.tick(100);
        rig.tick(100);
        let name = rig.anim.name.clone();
        let got = ["a", "c", "d"].iter().position(|n| *n == name).unwrap();
        assert_eq!(Some(got), want, "seed {seed}");
        seen[got] += 1;
    }
    assert!(seen.iter().all(|&n| n > 20), "{seen:?}");
}

#[test]
fn extreme_weights_do_not_overflow() {
    let weights = "fPlaySetProb(bX,-2147483648,bY,2147483647,bZ,101)";
    let mut rig = Rig::new(&[
        ("bAmbient", &[weights]),
        ("bY", &["fPlay(a)"]),
        ("bZ", &["fPlay(c)"]),
    ]);
    for _ in 0..4 {
        rig.tick(100);
    }
}

#[test]
fn a_time_covers_the_animation_loops_it_needs() {
    // fPlayTime(a, 5): 5 s of a loop of 300 ms is 17 plays; 1 s of 500 ms loops is 2.
    for (call, ms, plays) in [
        ("fPlayTime(a,5)", 300, 17),
        ("fPlayTime(b,1)", 1000, 1),
        ("fPlayTime(b,0)", 1000, 1),
        ("fPlayTime(b,-3)", 1000, 1),
        ("fPlayTime(d,1)", 200, 5),
    ] {
        let mut rig = Rig::new(&[("bAmbient", &[call, "fPlay(c)"])]);
        let mut played = 0;
        rig.tick(0);
        while rig.anim.name != "c" {
            played += 1;
            rig.tick(ms / 2);
            assert!(played < 1000);
        }
        // Two steps a loop, and one more for the base loop the sequence waited for.
        assert_eq!(played, 2 * plays + 1, "{call}");
    }
}

#[test]
fn ping_pong_plays_forward_then_back() {
    let mut rig = Rig::new(&[("bAmbient", &["fPlayPingPong(d)", "fPlay(a)"])]);
    let frames: Vec<_> = (0..6).map(|_| rig.tick(100)).collect();
    // The last frame is shown twice: the end of the forward pass and the start of the way back.
    assert_eq!(
        frames,
        [
            ("idle".into(), 0),
            ("d".into(), 0),
            ("d".into(), 1),
            ("d".into(), 1),
            ("d".into(), 0),
            ("a".into(), 0)
        ]
    );
}

#[test]
fn a_stop_at_end_hold_is_never_left() {
    let mut rig = Rig::new(&[("bAmbient", &["fPlayStopAtEnd(a)", "fPlay(c)"])]);
    // The call sets the unit's flag and queues `a`; the base loop that plays holds its last
    // frame, and the held unit takes no queued sequence, so the call is never done.
    for _ in 0..30 {
        assert_eq!(rig.tick(100), ("idle".into(), 0));
    }
    assert!(rig.anim.stopped && rig.runner.running());
    // A sequence of the player is held the same way, however long it is advanced.
    let timing = |_: &str| {
        Some(Timing {
            count: 3,
            delay: 100,
        })
    };
    let mut held = Animation::default();
    held.advance(0, &timing);
    held.stop = true;
    held.queue(vec![Entry::new("a", false)]);
    held.advance(5000, &timing);
    held.advance(5000, &timing);
    assert_eq!((held.shown(), held.stopped), (2, true));
}

#[test]
fn set_base_lasts_until_the_next_play() {
    let timing = |_: &str| {
        Some(Timing {
            count: 2,
            delay: 100,
        })
    };
    let mut anim = Animation::default();
    anim.advance(0, &timing);
    anim.queue(vec![Entry::new("a", false)]);
    anim.advance(200, &timing);
    assert_eq!(anim.name, "a");
    // `fSetBase` cuts the sequence and shows the new base at once, from its first frame.
    anim.set_base("b", &timing);
    assert_eq!(
        (anim.name.as_str(), anim.shown(), anim.on_base()),
        ("b", 0, true)
    );
    for _ in 0..10 {
        anim.advance(200, &timing);
        assert_eq!(anim.name, "b");
    }
    // A play call makes the idle animation the base again.
    anim.queue(vec![Entry::new("c", false)]);
    anim.advance(200, &timing);
    assert_eq!(anim.name, "c");
    anim.advance(200, &timing);
    assert_eq!(anim.name, "idle");
}

#[test]
fn a_sound_starts_at_the_next_advance() {
    let timing = |_: &str| {
        Some(Timing {
            count: 3,
            delay: 100,
        })
    };
    let sounding = |name: &str| Entry {
        sound: Some("purr".into()),
        ..Entry::new(name, false)
    };
    let sounds = |anim: &mut Animation| {
        let notes = std::mem::take(&mut anim.notes);
        notes.iter().filter(|n| matches!(n, Note::Sound(_))).count()
    };
    let mut anim = Animation::default();
    anim.advance(0, &timing);
    anim.queue(vec![sounding("a"), Entry::new("b", false)]);
    // The entry becomes current at the end of this advance: no sound yet, and none for a step of 0.
    anim.advance(300, &timing);
    assert_eq!(anim.name, "a");
    assert_eq!(sounds(&mut anim), 0);
    anim.advance(0, &timing);
    assert_eq!(sounds(&mut anim), 0);
    // The next advance with time starts it, once.
    anim.advance(10, &timing);
    assert_eq!(sounds(&mut anim), 1);
    anim.advance(10, &timing);
    assert_eq!(sounds(&mut anim), 0);

    // An entry that a long step skips whole never sounds.
    let mut anim = Animation::default();
    anim.advance(0, &timing);
    anim.queue(vec![
        Entry::new("a", false),
        sounding("b"),
        Entry::new("c", false),
    ]);
    anim.advance(300, &timing);
    anim.advance(650, &timing);
    assert_eq!((anim.name.as_str(), anim.entry), ("c", 2));
    anim.advance(10, &timing);
    assert_eq!(sounds(&mut anim), 0);

    // In a set: the sound is told one step after the animation.
    let mut rig = Rig::new(&[("bAmbient", &["fPlayWithSound(d,purr)", "fPlay(c)"])]);
    rig.tick(100);
    assert_eq!(rig.notes(), [Note::Anim("idle".into()), set("bAmbient")]);
    rig.tick(100);
    assert_eq!(rig.notes(), [Note::Anim("d".into())]);
    rig.tick(100);
    assert_eq!(rig.notes(), [Note::Sound("purr".into())]);
}

#[test]
fn sets_start_by_name_and_a_set_that_cannot_be_interrupted_refuses() {
    let mut rig = Rig::new(&[
        ("bAmbient", &["fPlay(a)", "fPlay(c)"]),
        ("bSick", &["fPlay(d)", "fPlay(d)"]),
    ]);
    rig.tick(100);
    let env = Env {
        data: &rig.data,
        unit: &rig.unit,
        rng: &mut rig.rng,
    };
    // The ambient set is interruptible; a started one that is not refuses a plain start.
    assert!(
        rig.runner
            .start(&mut rig.anim, &env, "bSick", false, false, true)
    );
    assert!(
        !rig.runner
            .start(&mut rig.anim, &env, "bAmbient", true, false, true)
    );
    assert_eq!(rig.runner.top(), Some("bSick"));
    // A pending set waits for the running one to end.
    rig.runner.request("bAmbient");
    let tops: Vec<_> = (0..12)
        .map(|_| {
            rig.tick(100);
            rig.runner.top().map(String::from)
        })
        .collect();
    assert_eq!(tops.first().unwrap().as_deref(), Some("bSick"));
    assert_eq!(tops.last().unwrap().as_deref(), Some("bAmbient"));
    // A start that is not kept drops the sequence that plays.
    let env = Env {
        data: &rig.data,
        unit: &rig.unit,
        rng: &mut rig.rng,
    };
    assert!(
        rig.runner
            .start(&mut rig.anim, &env, "bSick", true, true, false)
    );
    assert!(rig.anim.on_base());
}

#[test]
fn an_animation_a_sequence_names_but_the_type_lacks_ends_it() {
    let mut anim = Animation::default();
    let timing = |n: &str| {
        (n == "a").then_some(Timing {
            count: 2,
            delay: 100,
        })
    };
    anim.advance(0, &timing);
    anim.queue(vec![
        Entry::new("a", false),
        Entry::new("zz", false),
        Entry::new("a", false),
    ]);
    // The type has no base animation: the player takes the sequence at once.
    anim.advance(0, &timing);
    assert_eq!(anim.name, "a");
    anim.advance(200, &timing);
    // The unknown entry ends the whole sequence, not just itself.
    assert!(anim.on_base() && anim.name == "idle");
}

#[test]
fn a_huge_time_is_cut_and_a_huge_step_skips_whole_animations() {
    let mut rig = Rig::new(&[("bAmbient", &["fPlayTime(d,2000000000)"])]);
    rig.tick(100);
    assert!(rig.anim.queue.len() + rig.anim.seq.len() <= ENTRIES as usize);
    let timing = |n: &str| {
        (n == "d").then_some(Timing {
            count: 2,
            delay: 100,
        })
    };
    let mut anim = Animation::default();
    anim.advance(0, &timing);
    anim.queue((0..5).map(|_| Entry::new("d", false)).collect());
    anim.advance(0, &timing);
    // 5 entries of 200 ms; 450 ms skips two and lands 50 ms into the third.
    anim.advance(450, &timing);
    assert_eq!((anim.entry, anim.frame, anim.elapsed), (2, 0, 50));
    anim.advance(u32::MAX, &timing);
    assert!(anim.on_base());
}
