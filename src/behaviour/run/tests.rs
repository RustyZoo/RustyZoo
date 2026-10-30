use super::*;
use crate::behaviour::parse_call;
use crate::zoo::Behaviour as Saved;
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
    fn ambient(&self, _: &Entity, _: usize) -> &[Row] {
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
        self.runner.step(&mut self.anim, &mut env, Some(0));
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

/// The animation the set a nesting call `call` picks plays, for a unit in tank mode `mode` on a
/// water tile or not: `a` for the first set (land), `b`, `c` and `d` for the others.
fn nested(call: &str, mode: u8, wet: bool) -> String {
    let mut rig = Rig::new(&[
        ("bAmbient", &[call]),
        ("bL", &["fPlay(a)"]),
        ("bW", &["fPlay(b)"]),
        ("bS", &["fPlay(c)"]),
        ("bU", &["fPlay(d)"]),
    ]);
    // Where the unit is for its ambient table does not count.
    (rig.runner.tank_mode, rig.runner.wet, rig.runner.place) = (mode, wet, Place::Land);
    rig.tick(100);
    rig.tick(100);
    rig.tick(100).0
}

#[test]
fn the_nesting_calls_choose_by_the_tank_mode_and_the_tile_only() {
    let four = "fPlaySetTerrain(bL,bW,bS,bU)";
    // Under water, at the surface, then by the tile alone: a unit that does not swim on water
    // (a guest in the water) takes the water set.
    for (mode, wet, shown) in [
        (4, false, "d"),
        (4, true, "d"),
        (3, true, "c"),
        (0, true, "b"),
        (1, true, "b"),
        (2, false, "a"),
        (0, false, "a"),
    ] {
        assert_eq!(nested(four, mode, wet), shown, "{mode} {wet}");
    }
    // Two sets: the tile alone, whatever the tank mode.
    let two = "fPlaySetTerrain(bL,bW)";
    assert_eq!(nested(two, 4, false), "a");
    assert_eq!(nested(two, 3, true), "b");
    // The tank call: the second set under water only, whatever the tile.
    let tank = "fPlaySetTank(bL,bW)";
    for (mode, wet, shown) in [
        (4, false, "b"),
        (3, true, "a"),
        (0, true, "a"),
        (u8::MAX, false, "a"),
    ] {
        assert_eq!(nested(tank, mode, wet), shown, "{mode} {wet}");
    }
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
fn a_new_base_waits_for_the_loop_in_progress_and_a_cut_ends_it() {
    let rig = Rig::new(&[]);
    let timing = |n: &str| rig.data.timing(&rig.unit, n);
    let mut anim = Animation::default();
    anim.advance(0, &timing);
    assert_eq!(anim.name, "idle");
    // The new base shows when the loop of idle (100 ms) ends.
    anim.base_to("a");
    anim.advance(50, &timing);
    assert_eq!(anim.name, "idle");
    anim.advance(50, &timing);
    assert_eq!(anim.name, "a");
    // A cut shows the base at the next advance, whatever is playing.
    anim.base_to("b");
    anim.cut();
    anim.advance(1, &timing);
    assert_eq!(anim.name, "b");
    // With the base showing and nothing queued the cut stays, and takes the next queued sequence.
    anim.cut();
    anim.advance(10, &timing);
    anim.queue(vec![Entry::new("c", false)]);
    anim.advance(10, &timing);
    assert_eq!(anim.name, "c");
}

#[test]
fn a_routine_call_shows_the_idle_animation_after_a_walk() {
    let mut rig = Rig::new(&[("bAmbient", &["fGuestRoutine()"])]);
    rig.unit.class = "guests".into();
    rig.data.timings.insert(
        "walk".into(),
        Timing {
            count: 2,
            delay: 100,
        },
    );
    let data = &rig.data;
    rig.anim.set_base("walk", &|n| data.timing(&rig.unit, n));
    assert_eq!(rig.anim.name, "walk");
    // The idle base shows when the walk's stop has left its cut, one step on.
    rig.anim.cut();
    assert_eq!(rig.tick(100).0, "walk");
    assert_eq!(rig.tick(100).0, "idle");
    assert_eq!(rig.tick(100).0, "idle");
}

#[test]
fn a_guide_routine_keeps_the_base_during_the_talk() {
    let mut rig = Rig::new(&[("bAmbient", &["fGuideRoutine()"])]);
    rig.unit.class = "tour".into();
    rig.data.timings.insert(
        "walk".into(),
        Timing {
            count: 2,
            delay: 100,
        },
    );
    let data = &rig.data;
    rig.anim.set_base("walk", &|n| data.timing(&rig.unit, n));
    rig.anim.cut();
    rig.runner.talking = true;
    for _ in 0..3 {
        assert_eq!(rig.tick(100).0, "walk");
    }
}

#[test]
fn a_move_call_waits_for_the_units_answer_and_the_set_goes_on_when_it_is_done() {
    let resume = |rig: &mut Rig| {
        let mut env = Env {
            data: &rig.data,
            unit: &rig.unit,
            rng: &mut rig.rng,
        };
        rig.runner.resume(&mut rig.anim, &mut env);
    };
    let mut rig = Rig::new(&[("bAmbient", &["fRun(0,0)", "fPlay(d)"])]);
    for _ in 0..3 {
        rig.tick(100);
        assert_eq!(rig.runner.take_move(), Some((Gait::Fast, Target::Keep)));
        assert_eq!(rig.runner.take_move(), None, "taken once");
    }
    // Still running: the next call waits. Done: it starts in the same step.
    rig.runner.answer(Flow::Running);
    resume(&mut rig);
    assert!(rig.anim.queue.is_empty());
    rig.runner.answer(Flow::Done);
    resume(&mut rig);
    assert_eq!(rig.anim.queue.len(), 1, "fPlay(d) queued");
    // A failed call ends the set.
    let mut rig = Rig::new(&[("bAmbient", &["fWalk(0,0)", "fPlay(d)"])]);
    rig.tick(100);
    rig.runner.take_move();
    rig.runner.answer(Flow::Failed);
    resume(&mut rig);
    assert!(!rig.runner.running());
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
        (anim.name.as_str(), anim.shown(), anim.shows_base()),
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
    rig.tick(100);
    assert!(rig.anim.shows_base());
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
    assert!(anim.shows_base() && anim.name == "idle");
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
    assert!(anim.shows_base());
}

/// A saved set: its name, its remaining calls as texts and its flags loaded, call done, running,
/// forced.
fn saved(name: &str, calls: &[&str], flags: [u8; 4]) -> Saved {
    let level = crate::zoo::Level {
        name: name.into(),
        list_b: calls.iter().map(|c| c.to_string()).collect(),
        tail: flags,
        ..Default::default()
    };
    Saved {
        levels: vec![level],
    }
}

/// The animations shown over `n` steps of 100 ms.
fn shown(rig: &mut Rig, n: usize) -> Vec<String> {
    (0..n).map(|_| rig.tick(100).0).collect()
}

#[test]
fn a_saved_set_runs_its_saved_calls_from_the_current_one_and_is_not_looked_up() {
    // The type's set of that name differs from the saved calls: the saved ones run.
    let mut rig = Rig::new(&[("bUse", &["fPlay(d)"])]);
    assert!(
        rig.runner
            .restore(&saved("bUse", &["fPlay(a)", "fPlay(c)"], [1, 0, 1, 0]))
    );
    assert_eq!(rig.runner.top(), Some("bUse"));
    assert_eq!(rig.runner.calls_left(&rig.data, &rig.unit), 2);
    let names = shown(&mut rig, 6);
    assert!(
        names.contains(&"a".into()) && names.contains(&"c".into()),
        "{names:?}"
    );
    assert!(!names.contains(&"d".into()), "{names:?}");
    // The set was not started: no start note.
    assert!(!rig.notes().contains(&set("bUse")));
    // A call played for a time plays all its turns again, then the rest follows.
    let mut rig = Rig::new(&[]);
    rig.runner.restore(&saved(
        "bUse",
        &["fPlayTime(b,2)", "fPlay(c)"],
        [1, 0, 1, 0],
    ));
    let names = shown(&mut rig, 4);
    assert_eq!(names[..2], ["b", "b"]);
    assert!((0..12).map(|_| rig.tick(500).0).any(|n| n == "c"));
}

#[test]
fn a_saved_current_call_starts_on_the_first_step_not_behind_the_base_loop() {
    // A long idle loop would hold the call back for its whole length.
    let mut rig = Rig::new(&[]);
    rig.data.timings.insert(
        "idle".into(),
        Timing {
            count: 50,
            delay: 100,
        },
    );
    rig.runner
        .restore(&saved("bUse", &["fPlay(a)", "fPlay(c)"], [1, 0, 1, 0]));
    assert_eq!(rig.tick(100), ("a".into(), 0));
    // A set started in play, not restored, still queues behind the loop.
    let mut rig = Rig::new(&[("bAmbient", &["fPlay(a)"])]);
    rig.data.timings.insert(
        "idle".into(),
        Timing {
            count: 50,
            delay: 100,
        },
    );
    assert_eq!(rig.tick(100).0, "idle");
}

#[test]
fn a_saved_call_that_was_done_is_dropped_not_run_again() {
    let mut rig = Rig::new(&[]);
    rig.runner
        .restore(&saved("bUse", &["fPlay(a)", "fPlay(c)"], [1, 1, 1, 0]));
    let names = shown(&mut rig, 4);
    assert!(
        names.contains(&"c".into()) && !names.contains(&"a".into()),
        "{names:?}"
    );
    // The last call done and nothing left: the set ends without failing the unit.
    let mut rig = Rig::new(&[]);
    rig.runner
        .restore(&saved("bUse", &["fPlay(a)"], [1, 1, 1, 0]));
    rig.tick(100);
    assert!(!rig.runner.running());
}

#[test]
fn a_saved_set_that_is_not_running_or_has_nothing_to_run_is_ended() {
    let mut rig = Rig::new(&[]);
    // Not running, no name, no levels: nothing is taken.
    assert!(
        !rig.runner
            .restore(&saved("bUse", &["fPlay(a)"], [1, 0, 0, 0]))
    );
    assert!(!rig.runner.restore(&saved("", &["fPlay(a)"], [1, 0, 1, 0])));
    assert!(!rig.runner.restore(&Saved { levels: Vec::new() }));
    assert_eq!(rig.runner.top(), None);
    // Running with no call left (a negative count reads as none): it ends at its first run.
    assert!(rig.runner.restore(&saved("bUse", &[], [1, 0, 1, 0])));
    assert!(rig.runner.running());
    rig.tick(100);
    assert!(!rig.runner.running());
    // A call that cannot run, a text that is no call and a set no longer loaded end the chain.
    for calls in [
        &["fNoSuchCall(a)"][..],
        &["fPlay(zzz"],
        &[""],
        &["fPlay(nothing)"],
    ] {
        let mut rig = Rig::new(&[]);
        rig.runner.restore(&saved("bUse", calls, [1, 0, 1, 0]));
        rig.tick(100);
        assert!(!rig.runner.running(), "{calls:?}");
    }
    let mut rig = Rig::new(&[]);
    rig.runner
        .restore(&saved("bUse", &["fPlay(a)"], [0, 0, 1, 0]));
    rig.tick(100);
    assert!(!rig.runner.running());
}

#[test]
fn a_saved_nested_set_resumes_inside_its_parent_and_the_depth_is_bounded() {
    let level = |name: &str, calls: &[&str], done: u8| crate::zoo::Level {
        name: name.into(),
        list_b: calls.iter().map(|c| c.to_string()).collect(),
        tail: [1, done, 1, 0],
        child: 1,
        ..Default::default()
    };
    let mut rig = Rig::new(&[]);
    let nested = Saved {
        levels: vec![
            level("bTop", &["fPlaySet(bIn)", "fPlay(c)"], 0),
            level("bIn", &["fPlay(d)"], 0),
        ],
    };
    assert!(rig.runner.restore(&nested));
    let names = shown(&mut rig, 6);
    assert!(
        names.contains(&"d".into()) && names.contains(&"c".into()),
        "{names:?}"
    );
    // A chain of a thousand levels is cut at the depth the runner allows.
    let mut rig = Rig::new(&[]);
    let deep = Saved {
        levels: (0..1000)
            .map(|_| level("bTop", &["fPlaySet(bTop)"], 0))
            .collect(),
    };
    assert!(rig.runner.restore(&deep));
    rig.tick(100);
    assert!(rig.runner.chain.len() <= DEPTH);
}

#[test]
fn a_saved_forced_set_cuts_the_animation_at_its_first_run_once() {
    let mut rig = Rig::new(&[]);
    rig.runner
        .restore(&saved("bUse", &["fPlay(a)", "fPlay(c)"], [1, 0, 1, 1]));
    assert!(!rig.anim.cut);
    rig.tick(0);
    assert!(rig.anim.cut);
    rig.anim.cut = false;
    rig.tick(0);
    assert!(!rig.anim.cut);
}

#[test]
fn a_running_set_is_saved_with_its_remaining_calls_and_restores_the_same() {
    let calls = ["fPlay(a)", "fPlayTime(b,2)", "fPlay(c)"];
    let mut rig = Rig::new(&[("bAmbient", &calls)]);
    let none = Saved { levels: Vec::new() };
    // No set run yet: nothing to write.
    assert!(rig.runner.saved(&none, |_| None).is_none());
    // The first call has run and is done, the second is current: the type's set is written
    // from the current call on, with the flags of the level.
    rig.tick(100);
    rig.tick(100);
    let got = rig.runner.saved(&none, |n| rig.data.set(&rig.unit, n));
    let got = got.unwrap();
    let top = &got.levels[0];
    assert_eq!((top.name.as_str(), top.flag, top.child), ("bAmbient", 1, 0));
    assert_eq!(top.list_a, ["bAmbient"]);
    assert_eq!(top.list_b, calls[1..]);
    assert_eq!(
        (top.text.as_str(), top.call.as_str()),
        (calls[1], "fPlayTime")
    );
    assert_eq!(top.args, ["b", "2"]);
    assert_eq!(top.tail, [1, 0, 1, 0]);
    // It loads back into a runner that writes the very same set.
    let mut other = Rig::new(&[]);
    assert!(other.runner.restore(&got));
    assert_eq!(other.runner.saved(&got, |_| None), Some(got.clone()));
    // A level that has ended is written as not running.
    rig.data.rows.clear();
    for _ in 0..20 {
        rig.tick(500);
    }
    assert!(!rig.runner.running());
    let ended = rig.runner.saved(&got, |n| rig.data.set(&rig.unit, n));
    assert_eq!(ended.unwrap().levels[0].tail[2], 0);
}

#[test]
fn a_nested_set_is_saved_inside_its_parent_with_the_chain_of_names() {
    let level = |name: &str, calls: &[&str]| crate::zoo::Level {
        name: name.into(),
        list_b: calls.iter().map(|c| c.to_string()).collect(),
        tail: [1, 0, 1, 0],
        child: 1,
        ..Default::default()
    };
    let mut rig = Rig::new(&[]);
    let nested = Saved {
        levels: vec![
            level("bTop", &["fPlaySet(bIn)", "fPlay(c)"]),
            level("bIn", &["fPlay(d)", "fPlay(a)"]),
        ],
    };
    rig.runner.restore(&nested);
    rig.tick(100);
    let got = rig.runner.saved(&nested, |_| None).unwrap();
    assert_eq!(got.levels.len(), 2);
    let names = |l: &crate::zoo::Level| (l.name.clone(), l.child, l.list_b.len());
    assert_eq!(names(&got.levels[0]), ("bTop".into(), 1, 2));
    assert_eq!(names(&got.levels[1]), ("bIn".into(), 0, 2));
    assert_eq!(got.levels[0].list_a, ["bTop", "bIn"]);
    assert_eq!(got.levels[1].text, "fPlay(d)");
    // A text that is no call is kept as written.
    let mut rig = Rig::new(&[]);
    let odd = saved("bUse", &["fNoSuchCall(a)"], [1, 0, 1, 0]);
    rig.runner.restore(&odd);
    assert_eq!(
        rig.runner.saved(&odd, |_| None).unwrap().levels[0].list_b,
        ["fNoSuchCall(a)"]
    );
}
