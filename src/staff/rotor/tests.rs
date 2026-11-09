use super::*;
use crate::staff::heli::Heli;

const POINT: (i32, i32, i32) = (640, 640, 80);

/// A listener `d` view-space pixels to the right of the helicopter at [`POINT`], with the global
/// term `global`.
fn hearing(d: i64, global: i32) -> Hearing {
    let (x, y) = project(0, POINT.0, POINT.1, POINT.2);
    Hearing {
        listener: (i64::from(x).saturating_add(d), i64::from(y)),
        global,
        ..Hearing::default()
    }
}

fn sound() -> Option<(String, i32)> {
    Some(("sounds/helicopt.wav".into(), 1200))
}

fn started(d: i64, global: i32) -> Rotor {
    Rotor::start(sound(), POINT, &hearing(d, global))
}

#[test]
fn the_start_loads_the_sound_at_level_3000_and_fades_in_from_4000() {
    let r = started(0, 0);
    assert_eq!(r.file.as_deref(), Some("sounds/helicopt.wav"));
    assert_eq!((r.level(), r.counter, r.rising, r.playing), (3000, 4000, true, true));
    // B + F and no distance, centred.
    assert_eq!((r.heard, r.pan), ((4200, 0), 0));
}

#[test]
fn the_start_is_refused_over_6000_and_the_object_stays() {
    // G 0, B 1200, F 3000: X may be 1800 (654 pixels give 1798, 655 give 1801).
    assert!(started(654, 0).playing);
    assert!(!started(655, 0).playing);
    // The global term counts: 6000 starts, 6001 does not.
    assert!(started(654, 2).playing);
    assert!(!started(654, 3).playing);
    // A huge global term, a huge distance: no start, and no panic.
    for (d, g) in [(0, i32::MAX), (i64::MAX, 0), (i64::MIN, i32::MIN), (0, -5000)] {
        let r = started(d, g);
        assert_eq!((r.counter, r.rising, r.level()), (4000, true, 3000), "{d} {g}");
    }
    assert!(started(0, -5000).playing, "a negative global term buys headroom");
}

#[test]
fn a_load_that_fails_keeps_an_object_that_does_nothing() {
    let mut r = Rotor::start(None, POINT, &hearing(0, 0));
    assert_eq!((r.file.clone(), r.playing, r.rising, r.counter), (None, false, false, 0));
    for flying in [true, false] {
        assert!(r.update(flying, 100_000, POINT, &hearing(0, 0)));
    }
    assert_eq!((r.playing, r.level()), (false, 0));
}

#[test]
fn an_update_refreshes_the_position_then_fades_and_leaves_the_distance_out_while_it_does() {
    let hear = hearing(100, 0);
    let mut r = Rotor::start(sound(), POINT, &hear);
    // 1000 ms of flight: the counter 3000, F = round(3000 * 0.00025 * 3000) = 2250, and the
    // volume refresh of the fade has no distance term; the pan is the position refresh's.
    assert!(r.update(true, 1000, POINT, &hear));
    assert_eq!((r.counter, r.level()), (3000, 2250));
    assert_eq!(r.heard, (1200 + 2250, 0));
    assert_eq!(r.pan, -600, "the listener is to the right: the sound is panned left");
    // Fully faded in: the fade flag is down, and the next update gives the distance back.
    assert!(r.update(true, 3000, POINT, &hear));
    assert_eq!((r.counter, r.level(), r.rising), (0, 0, false));
    assert_eq!(r.heard, (1200, 0));
    assert!(r.update(true, 10, POINT, &hear));
    assert_eq!(r.heard, (1200, 275));
    // Zero milliseconds, and a huge step, change nothing more.
    assert!(r.update(true, 0, POINT, &hear));
    assert!(r.update(true, u32::MAX, POINT, &hear));
    assert_eq!((r.counter, r.level()), (0, 0));
}

#[test]
fn the_fade_term_rounds_the_counter_in_floating_point() {
    let hear = hearing(0, 0);
    for (ms, f) in [(0, 3000), (1, 2999), (3, 2998), (3999, 1), (4000, 0)] {
        let mut r = Rotor::start(sound(), POINT, &hear);
        r.update(true, ms, POINT, &hear);
        assert_eq!(r.level(), f, "{ms} ms");
    }
}

#[test]
fn landing_raises_the_fade_and_the_end_of_it_stops_a_playing_sound_and_frees_the_object() {
    let hear = hearing(0, 0);
    let mut heli = Heli::default();
    assert!(heli.set_flying(true), "a take-off needs a sound");
    heli.rotor = Some(Rotor::start(sound(), POINT, &hear));
    let r = heli.rotor.as_mut().unwrap();
    r.update(true, 4000, POINT, &hear);
    assert!(!r.rising);
    // Setting what the word is does nothing; the take-off with a sound asks for none.
    assert!(!heli.set_flying(true));
    assert!(!heli.set_flying(false));
    let r = heli.rotor.as_mut().unwrap();
    assert!(r.rising, "landing raises the flag");
    assert!(r.update(false, 3999, POINT, &hear));
    assert_eq!(r.level(), 2999);
    assert!(!r.update(false, 1, POINT, &hear), "stopped and freed");
    // A landed helicopter loaded has none; one loaded in the air has a pending one.
    assert!(Heli::from_own(&[7, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0]).rotor.is_none());
    assert!(Heli::from_own(&[7, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]).rotor.is_some());
}

#[test]
fn a_loop_that_does_not_play_is_never_started_again_and_the_object_stays() {
    // Taken off out of earshot: the object stays and holds the fade flag up.
    let far = hearing(5000, 0);
    let mut heli = Heli::default();
    assert!(heli.set_flying(true));
    heli.rotor = Some(Rotor::start(sound(), POINT, &far));
    let r = heli.rotor.as_mut().unwrap();
    assert!(!r.playing);
    // Fading in and out, nothing frees it: it is not playing.
    assert!(r.update(true, 4000, POINT, &hearing(0, 0)));
    assert!(!r.playing);
    assert!(!heli.set_flying(false));
    let r = heli.rotor.as_mut().unwrap();
    assert!(r.update(false, 10_000, POINT, &hearing(0, 0)));
    assert!(r.rising, "the flag stays up");
    // A later take-off finds the object and starts nothing.
    assert!(!heli.set_flying(true));
    assert!(!heli.rotor.as_mut().unwrap().playing);
}

#[test]
fn a_playing_loop_is_stopped_for_good_over_9000_at_a_position_refresh() {
    let mut r = started(0, 0);
    r.update(true, 4000, POINT, &hearing(0, 0));
    // F 0, B 1200: X may be 7800 (2836 pixels give 7799, 2837 give 7801).
    assert!(r.update(true, 0, POINT, &hearing(2836, 0)));
    assert!(r.playing);
    assert!(r.update(true, 0, POINT, &hearing(2837, 0)));
    assert!(!r.playing);
    // It comes back into earshot, and nothing starts it again.
    r.update(true, 0, POINT, &hearing(0, 0));
    assert!(!r.playing);
    // Landing does not free an object that is not playing.
    r.raise();
    assert!(r.update(false, 4000, POINT, &hearing(0, 0)));
}

#[test]
fn a_helicopter_loaded_in_the_air_starts_at_level_3000_then_sets_it_to_0() {
    let hear = hearing(0, 0);
    let mut r = Rotor::start(sound(), POINT, &hear);
    r.settle();
    assert_eq!((r.level(), r.rising, r.counter, r.playing), (0, false, 0, true));
    // No volume refresh: the first position refresh applies the full level.
    assert_eq!(r.heard, (4200, 0));
    assert!(r.update(true, 40, POINT, &hear));
    assert_eq!(r.heard, (1200, 0));
    // The 6000 test is the start's, at level 3000: out of earshot there it stays silent.
    let mut far = started(655, 0);
    far.settle();
    far.update(true, 40, POINT, &hearing(0, 0));
    assert!(!far.playing);
    assert!(Rotor::loaded_in_air().pending());
}
