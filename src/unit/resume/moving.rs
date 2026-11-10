//! A unit saved moving (behaviours 5.10.2).

use super::tests::{saved_block, walking};
use super::*;
use crate::unit::testing::{centre, entity, world_with};
use crate::zoo::{Level, UnitBlock};

/// A type with the animations `names` (one frame each); the animation speeds of the bands are
/// 60, 80 and 90.
struct Moves(&'static [&'static str]);

impl crate::behaviour::Data for Moves {
    fn set(&self, _: &crate::zoo::Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
        None
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<crate::behaviour::Timing> {
        self.0.contains(&name).then_some(crate::behaviour::Timing {
            count: 4,
            delay: 100,
        })
    }
    fn number(&self, _: &crate::zoo::Entity, number: &crate::behaviour::Number) -> i32 {
        match number {
            crate::behaviour::Number::Characteristic(k) => match k.as_str() {
                "cSlowAnimSpeed" => 60,
                "cMediumAnimSpeed" => 80,
                "cFastAnimSpeed" => 90,
                _ => 0,
            },
            _ => 0,
        }
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
}

const MOVES: Moves = Moves(&["idle", "walk", "trot", "run"]);

/// A keeper saved in a file of version `version` running the call `call`, with the moving flag
/// `moving`, the speed 300 and the animation speed 150; loaded and settled with `names`.
fn moving_in(version: u32, call: &str, moving: u8, names: &Moves) -> World {
    let block = UnitBlock {
        b8: moving,
        w5: 300,
        short: 150,
        ..Default::default()
    };
    let mut w = world_with((12, 4), |zoo| {
        zoo.version = version;
        zoo.entities
            .push(entity("keeper", "x", 1, centre((1, 1)), 0));
    });
    let mut block = block;
    block.behaviour.levels[0] = Level {
        name: "bWalk".into(),
        list_b: vec![call.into()],
        tail: [1, 0, 1, 0],
        ..Default::default()
    };
    w.zoo.entities[0].common.as_mut().unwrap().unit = Some(Box::new(block));
    load(&mut w, 0);
    settle(&mut w, names);
    w
}

/// The name the unit shows after its player took the base.
fn animation_of(w: &mut World, names: &Moves) -> String {
    let entity = entity("keeper", "x", 1, (0, 0), 0);
    let unit = w.units.get_mut(1).unwrap();
    unit.animation
        .advance(0, &|n| crate::behaviour::Data::timing(names, &entity, n));
    unit.animation.name.clone()
}

#[test]
fn a_unit_saved_moving_loads_moving_and_shows_its_move_animation_before_any_step() {
    // Behaviours 5.10.2 points 1 to 3: the band comes from the call's name, so a restored
    // fTrot or fRun shows walk; the speed is the stored one, the animation speed the band's.
    for (call, name, percent) in [
        ("fMoveMedium()", "trot", 80),
        ("fMoveFast()", "run", 90),
        ("fMoveSlow()", "walk", 60),
        ("fTrot()", "walk", 60),
        ("fRun()", "walk", 60),
    ] {
        let mut w = moving_in(98, call, 1, &MOVES);
        let u = w.units.get(1).unwrap();
        assert_eq!((u.moving, u.speed, u.anim_speed), (true, 300, percent));
        assert_eq!(animation_of(&mut w, &MOVES), name, "{call}");
    }
    // Any non-zero byte counts as set.
    assert!(
        moving_in(98, "fMoveSlow()", 255, &MOVES)
            .units
            .get(1)
            .unwrap()
            .moving
    );
}

#[test]
fn a_unit_saved_standing_loads_standing_with_its_idle() {
    let mut w = moving_in(98, "fMoveMedium()", 0, &MOVES);
    let u = w.units.get(1).unwrap();
    assert_eq!((u.moving, u.speed, u.anim_speed), (false, 300, 150));
    assert_eq!(animation_of(&mut w, &MOVES), "idle");
}

#[test]
fn the_animation_speed_is_read_from_version_98_only() {
    // A unit that stands keeps what the load read: the stored percentage from 98, the
    // creation value 100 before.
    for (version, percent) in [(97, 100), (98, 150)] {
        let w = moving_in(version, "fMoveMedium()", 0, &MOVES);
        assert_eq!(w.units.get(1).unwrap().anim_speed, percent, "{version}");
    }
}

#[test]
fn a_type_without_the_move_animation_installs_nothing_but_takes_the_band_speed() {
    let none = Moves(&["idle"]);
    let mut w = moving_in(98, "fMoveMedium()", 1, &none);
    assert_eq!(w.units.get(1).unwrap().anim_speed, 80);
    assert_eq!(animation_of(&mut w, &none), "idle");
}

#[test]
fn a_save_writes_the_live_moving_flag_speed_and_animation_speed() {
    let mut w = walking(104, Vec::new(), "");
    let u = w.units.get_mut(1).unwrap();
    (u.started, u.moving, u.speed, u.anim_speed) = (true, true, 123, 77);
    let b = saved_block(&w);
    assert_eq!((b.b8, b.w5, b.short), (1, 123, 77));
}

#[test]
fn a_standing_unit_keeps_the_stored_speed_which_is_never_read() {
    let block = UnitBlock {
        w5: 25,
        ..Default::default()
    };
    let mut w = saved_in_started(block);
    let u = w.units.get_mut(1).unwrap();
    (u.moving, u.speed) = (false, 0);
    let b = saved_block(&w);
    assert_eq!((b.b8, b.w5), (0, 25));
}

#[test]
fn a_flag_byte_that_reads_as_the_live_value_is_written_as_it_was() {
    let block = UnitBlock {
        b8: 2,
        w5: 25,
        ..Default::default()
    };
    let mut w = saved_in_started(block);
    assert_eq!(saved_block(&w).b8, 2);
    w.units.get_mut(1).unwrap().moving = false;
    assert_eq!(saved_block(&w).b8, 0);
}

/// A keeper loaded from `block` and started.
fn saved_in_started(block: UnitBlock) -> World {
    let mut w = super::tests::saved_block_in(104, block);
    w.units.get_mut(1).unwrap().started = true;
    w
}
