//! The helicopter on a 20 by 20 map: its base at tile (10, 10), rotation 0, so the slot point is at
//! (704, 640, 80) and the exit point 80 higher, a heron of a species it recovers loose at (14, 10).

use super::*;
use crate::animal::Animal;
use crate::building::birth::births;
use crate::entity::EntityType;
use crate::fixtures;
use crate::goal::{CAPTURE, ENTER_BUILDING, ENTRANCE, EXIT_BUILDING, USE};
use crate::unit::{Ctx, Part, Standard, testing};

const BASE: u32 = 1;
const HERON: u32 = 50;
const OTHER: u32 = 51;
/// The slot point and the exit point.
const SLOT: (i32, i32, i32) = (704, 640, 80);
const EXIT: (i32, i32, i32) = (704, 640, 160);

fn ty(class: &'static str, name: &str, id: i32, text: &str) -> EntityType {
    EntityType {
        name_id: id,
        ..fixtures::ty(class, name, "m", text)
    }
}

fn types() -> Catalogue {
    let slot = "[Characteristics/Integers]\ncCapacity=1000\ncUserStaysOutside=1\n[slots]\nname=slot0\n\
         [slot0]\nslotpos=-64\nslotpos=0\nslotposz=80\nexitpos=-64\nexitpos=0\nexitposz=160\n\
         capacity=1\nfacing=7\nbirthUser=9550\n";
    let mut cat = Catalogue::default();
    cat.push(ty("building", "helibase", 8031, slot));
    cat.push(ty("helicopter", "helicopter", 9550, ""));
    cat.push(ty(
        "animals",
        "heron",
        5200,
        "[Characteristics/Integers]\ncHeliRecovery=1\ncMaxEnergy=80\n",
    ));
    cat.push(ty("animals", "zebra", 5100, ""));
    cat
}

/// The text of the helicopter's type for the sets and the animations.
struct Flier(Sets);

impl Flier {
    fn new() -> Flier {
        Flier(Sets::new(&[
            ("bIdleGround", &["fPlay(idle)"]),
            ("bIdleAir", &["fPlay(level)"]),
            ("bFly", &["fFly(fly,150)"]),
            ("bAscend", &["fFly(level,150)"]),
            ("bDescend", &["fFly(level,150)"]),
            ("bChaseAnimal", &["fFly(fly,250)"]),
            (
                "bFireWeapon",
                &["fPlayWithSound(fire,fire_weapon)", "fPlay(level)"],
            ),
            ("bSpinup", &["fPlay(warmup)"]),
            ("bSpindown", &["fPlayReverse(warmup)"]),
        ]))
    }
}

impl Data for Flier {
    fn set(&self, unit: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        self.0.set(unit, name)
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        let count = match name {
            "idle" | "level" | "fly" | "fire" => 2,
            "warmup" => 3,
            _ => return None,
        };
        Some(Timing { count, delay: 100 })
    }
    fn number(&self, _: &crate::zoo::Entity, number: &Number) -> i32 {
        match number {
            Number::Int(n) => *n,
            Number::Characteristic(k) => match k.as_str() {
                "cMinHeight" => 128,
                "cMaxHeight" => 256,
                _ => 0,
            },
        }
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<Heard> {
        None
    }
    fn loop_sound(&self, _: &crate::zoo::Entity) -> Option<(String, i32)> {
        Some(("sounds/helicopt.wav".into(), 1200))
    }
}

/// The base and its helicopter (made as a base placed in play makes it), and the herons
/// `(id, tile)` loose on the grounds.
fn zoo(herons: &[(u32, (i32, i32))]) -> (World, Catalogue, u32) {
    let cat = types();
    let mut w = testing::world((20, 20), |_, e| {
        let mut base = testing::entity("building", "helibase", BASE, (10 * 64, 10 * 64), 0);
        base.subtype = "m".into();
        base.common.as_mut().unwrap().building = Some(crate::zoo::fresh::building_new(0.0, 0));
        e.push(base);
        for &(id, tile) in herons {
            let mut a = testing::entity("animals", "heron", id, testing::centre(tile), 0);
            a.subtype = "m".into();
            e.push(a);
        }
    });
    for &(id, _) in herons {
        w.units.get_mut(id).unwrap().part = Part::Animal(Animal {
            escaped: true,
            ..Animal::default()
        });
        w.units.escapes.push(id);
    }
    let heli = births(&mut w, &cat, BASE, &mut Rng::default(), 9, testing::shape)[0];
    (w, cat, heli)
}

/// One step of `ms` milliseconds of the unit `id`: the move, the animation, the set and the goals.
/// The ids it asked to remove.
fn step(w: &mut World, cat: &Catalogue, rng: &mut Rng, id: u32, ms: u32) -> Vec<u32> {
    let (events, mut removals, data) = (Events::default(), Vec::new(), Flier::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: ms,
        cat,
    };
    let at = w.find(id).unwrap();
    crate::unit::update(&mut Standard { data: &data }, w, &mut ctx, at);
    removals
}

/// Steps of 67 milliseconds until `done`, at most `n` of them.
fn until(
    w: &mut World,
    cat: &Catalogue,
    rng: &mut Rng,
    id: u32,
    n: u32,
    mut done: impl FnMut(&World) -> bool,
) {
    for _ in 0..n {
        if done(w) {
            return;
        }
        step(w, cat, rng, id, 67);
    }
    assert!(done(w), "not reached in {n} steps");
}

fn heli(w: &World, id: u32) -> crate::staff::heli::Heli {
    match &w.units.get(id).unwrap().part {
        Part::Staff(s) => s.heli.clone(),
        _ => unreachable!(),
    }
}

fn point(w: &World, id: u32) -> (i32, i32, i32) {
    let c = w.zoo.entities[w.find(id).unwrap()].common.as_ref().unwrap();
    (c.x, c.y, c.z)
}

fn task(w: &World, id: u32) -> (u8, u8, u32) {
    let r = &w.units.get(id).unwrap().goal.record;
    (r.task, r.mode, r.target.id)
}

fn base(w: &World) -> crate::zoo::BuildingRecord {
    let at = w.find(BASE).unwrap();
    w.zoo.entities[at]
        .common
        .as_ref()
        .unwrap()
        .building
        .clone()
        .unwrap()
}

fn animal(w: &mut World, id: u32) -> &mut Animal {
    match &mut w.units.get_mut(id).unwrap().part {
        Part::Animal(a) => a,
        _ => unreachable!(),
    }
}

fn darted(w: &World, id: u32) -> bool {
    matches!(&w.units.get(id).unwrap().part, Part::Animal(a) if a.tranq)
}

fn top(w: &World, id: u32) -> Option<String> {
    w.units.get(id).unwrap().runner.top().map(String::from)
}

#[test]
fn a_helicopter_is_born_waiting_at_its_slot_point_on_the_ground() {
    let (w, _, h) = zoo(&[]);
    assert_eq!(task(&w, h), (USE, 0, 0));
    assert_eq!(point(&w, h), SLOT);
    let heli = heli(&w, h);
    assert_eq!((heli.base, heli.flying, heli.dir), (BASE, false, -1));
    assert_eq!((base(&w).users, base(&w).users_now), (vec![(h, 0)], 1));
}

/// The slot byte of the helicopter's unit block, replaced by `v` when given.
fn slot_byte(w: &mut World, id: u32, v: Option<u8>) -> u8 {
    let at = w.find(id).unwrap();
    let u = w.zoo.entities[at].common.as_mut().unwrap().unit.as_mut();
    let u = u.unwrap();
    let old = u.slot;
    u.slot = v.unwrap_or(old);
    old
}

#[test]
fn the_base_placement_writes_the_slot_index_in_the_helicopters_block_and_a_save_keeps_it() {
    let (mut w, cat, h) = zoo(&[]);
    assert_eq!(slot_byte(&mut w, h, None), 0);
    let mut saved = w.saved(|_| true);
    w.finish_save(&mut saved, &cat);
    let heli = saved.entities.iter().find(|e| e.class == "helicopter");
    let c = heli.unwrap().common.as_ref().unwrap();
    assert_eq!(c.unit.as_ref().unwrap().slot, 0);
}

#[test]
fn a_landed_helicopter_is_put_at_the_slot_point_of_its_stored_byte_or_at_the_base_without_one() {
    let (mut w, cat, h) = zoo(&[]);
    let at = w.find(h).unwrap();
    let base = ((10 * 64, 10 * 64, 0), SLOT);
    for (byte, to) in [(0, SLOT), (1, base.0), (255, base.0), (0, SLOT)] {
        slot_byte(&mut w, h, Some(byte));
        crate::staff::heli::land(&mut w, &cat, at);
        assert_eq!(point(&w, h), to, "byte {byte}");
    }
}

#[test]
fn a_helicopter_without_a_base_asks_for_its_removal_and_draws_nothing() {
    let (mut w, cat, h) = zoo(&[(HERON, (19, 10))]);
    let mut rng = Rng::default();
    // The id names no entity, then an entity that is no building.
    for gone in [99, HERON] {
        if let Part::Staff(s) = &mut w.units.get_mut(h).unwrap().part {
            s.heli.base = gone;
        }
        let before = rng.clone();
        assert_eq!(step(&mut w, &cat, &mut rng, h, 67), [h], "{gone}");
        assert_eq!(rng, before, "{gone}");
        // The ladder never ran: the animal is not claimed.
        assert_eq!(animal(&mut w, HERON).claim, 0, "{gone}");
    }
}

#[test]
fn a_helicopter_with_no_task_goes_after_an_escaped_animal_or_home() {
    let (mut w, cat, h) = zoo(&[(HERON, (19, 10))]);
    let mut rng = Rng::default();
    animal(&mut w, HERON).escaped = false;
    w.units.get_mut(h).unwrap().goal.record = Record::default();
    step(&mut w, &cat, &mut rng, h, 67);
    // Over its base's tile the return is done at once.
    assert_eq!(task(&w, h), (ENTER_BUILDING, 0, 0));
    animal(&mut w, HERON).escaped = true;
    w.units.get_mut(h).unwrap().goal.record = Record::default();
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(task(&w, h), (CAPTURE, 3, HERON));
    assert_eq!(animal(&mut w, HERON).claim, h);
    // A species the helicopter does not recover is left to the keepers.
    let (mut w, cat, h) = zoo(&[(OTHER, (19, 10))]);
    let other = w.find(OTHER).unwrap();
    w.zoo.entities[other].type_name = "zebra".into();
    w.units.get_mut(h).unwrap().goal.record = Record::default();
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(task(&w, h).0, ENTER_BUILDING);
}

#[test]
fn a_helicopter_waiting_at_its_base_takes_off_in_the_update_its_search_finds_an_animal() {
    let (mut w, cat, h) = zoo(&[(HERON, (19, 10))]);
    let mut rng = Rng::default();
    animal(&mut w, HERON).escaped = false;
    for _ in 0..10 {
        step(&mut w, &cat, &mut rng, h, 67);
    }
    assert_eq!(task(&w, h), (USE, 0, 0));
    animal(&mut w, HERON).escaped = true;
    step(&mut w, &cat, &mut rng, h, 67);
    // The done waiting task completed in the same update, and the claim is written.
    assert_eq!(task(&w, h), (EXIT_BUILDING, 0, 0));
    assert_eq!(animal(&mut w, HERON).claim, h);
}

#[test]
fn the_take_off_spins_up_then_climbs_straight_up_to_the_exit_point_and_leaves_the_base() {
    let (mut w, cat, h) = zoo(&[(HERON, (19, 10))]);
    let mut rng = Rng::default();
    until(&mut w, &cat, &mut rng, h, 5, |w| {
        task(w, h).0 == EXIT_BUILDING
    });
    let heading = w.zoo.entities[w.find(h).unwrap()]
        .common
        .as_ref()
        .unwrap()
        .rotation;
    // Spinning up: on the ground, and the base still has its user.
    step(&mut w, &cat, &mut rng, h, 67);
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(
        (top(&w, h).as_deref(), heli(&w, h).flying, point(&w, h)),
        (Some("bSpinup"), false, SLOT)
    );
    assert_eq!(base(&w).users_now, 1);
    let mut heights = Vec::new();
    until(&mut w, &cat, &mut rng, h, 60, |w| {
        heights.push(point(w, h).2);
        task(w, h).0 == CAPTURE
    });
    // Up at 10 units a step, the same spot, the same facing, the flight direction none.
    assert_eq!(point(&w, h), EXIT);
    assert!(
        heights
            .windows(2)
            .all(|p| p[1] >= p[0] && p[1] - p[0] <= 10)
    );
    assert_eq!(heights.iter().filter(|&&z| z > 80 && z < 160).count(), 7);
    let c = w.zoo.entities[w.find(h).unwrap()].common.as_ref().unwrap();
    assert_eq!((c.x, c.y, c.rotation), (SLOT.0, SLOT.1, heading));
    let h2 = heli(&w, h);
    assert_eq!((h2.flying, h2.dir), (true, -1));
    // It left the base, and its task is the capture of the animal, with no mode.
    assert_eq!((base(&w).users, base(&w).users_now), (vec![], 0));
    assert_eq!(task(&w, h), (CAPTURE, 0, HERON));
}

/// The helicopter at the exit point on the chase of the heron, stepped to the first move of the
/// chase.
fn chasing(herons: &[(u32, (i32, i32))]) -> (World, Catalogue, Rng, u32) {
    let (mut w, cat, h) = zoo(herons);
    let mut rng = Rng::default();
    until(&mut w, &cat, &mut rng, h, 80, |w| {
        top(w, h).as_deref() == Some("bChaseAnimal") && point(w, h) != EXIT
    });
    (w, cat, rng, h)
}

#[test]
fn the_chase_flies_at_250_towards_the_corner_of_the_animals_tile_with_no_search() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    let r = &w.units.get(h).unwrap().goal.record;
    assert_eq!((r.job, r.job_target.id), (13, HERON));
    assert_eq!(
        w.units.get(h).unwrap().destination,
        Some((19 * 64, 10 * 64, 0))
    );
    assert_eq!(r.job_target.pos, Some((19 * 64, 10 * 64, 0)));
    assert_eq!(heli(&w, h).dir, 2);
    assert_eq!(w.units.get(h).unwrap().speed, 250);
    // 16 units a step along the line, at the height it had.
    let (x, _, z) = point(&w, h);
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!((point(&w, h).0 - x, point(&w, h).2), (16, z));
}

#[test]
fn the_flight_keeps_to_the_height_band_of_the_type() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    let at = w.find(h).unwrap();
    // A step is 16 units, half of it 8. Below the band it climbs by the half at most, above it
    // it sinks, inside it keeps its height.
    for (from, to) in [(100, 108), (127, 128), (300, 292), (260, 256), (200, 200)] {
        let (x, y, _) = point(&w, h);
        w.place_unit(at, (x, y, from), 2);
        step(&mut w, &cat, &mut rng, h, 67);
        assert_eq!(point(&w, h).2, to, "{from}");
    }
    // Under the ground it is lifted by the step and does not move on.
    let tile = w.tile_index(point(&w, h).0 / 64 + 1, 10).unwrap();
    w.zoo.tiles[tile].elevation = 20;
    let (x, y, _) = point(&w, h);
    w.place_unit(at, (x + 40, y, 100), 2);
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(point(&w, h), (x + 40, y, 116));
}

#[test]
fn the_destination_follows_the_animal_once_the_helicopter_is_over_the_tile_it_aimed_at() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    // The heron went elsewhere while it flew; the helicopter is over the old tile.
    let (heron, at) = (w.find(HERON).unwrap(), w.find(h).unwrap());
    w.place_unit(heron, (3 * 64 + 32, 3 * 64 + 32, 0), 0);
    w.place_unit(at, (19 * 64 + 10, 10 * 64, 160), 2);
    step(&mut w, &cat, &mut rng, h, 67);
    let r = &w.units.get(h).unwrap().goal.record;
    // The new destination, the new direction; the start of the chase is refused, so the job
    // has no target tile and asks again at every update.
    assert_eq!(
        w.units.get(h).unwrap().destination,
        Some((3 * 64, 3 * 64, 0))
    );
    assert_eq!((heli(&w, h).dir, r.job_target.tile, r.job), (7, None, 13));
    assert_eq!(top(&w, h).as_deref(), Some("bChaseAnimal"));
    let (x, y, _) = point(&w, h);
    step(&mut w, &cat, &mut rng, h, 67);
    assert!(point(&w, h).0 < x && point(&w, h).1 < y);
}

#[test]
fn within_range_the_helicopter_hovers_darts_the_animal_and_counts_the_dart_on_its_base() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    until(&mut w, &cat, &mut rng, h, 80, |w| {
        top(w, h).as_deref() == Some("bFireWeapon")
    });
    // Within 5 tiles it stops where it is, at its height, facing the animal, job 14.
    let hover = point(&w, h);
    assert_eq!((hover.0 / 64, hover.2), (14, 160));
    assert_eq!(
        (w.units.get(h).unwrap().goal.record.job, base(&w).fill),
        (14, 0)
    );
    let facing = w.zoo.entities[w.find(h).unwrap()]
        .common
        .as_ref()
        .unwrap()
        .rotation;
    assert_eq!(facing, 2);
    until(&mut w, &cat, &mut rng, h, 40, |w| darted(w, HERON));
    assert_eq!(base(&w).fill, 1);
    assert_eq!(point(&w, h), hover);
    // The dart count is saved with the base.
    let mut saved = w.saved(|_| true);
    w.finish_save(&mut saved, &cat);
    let at = saved
        .entities
        .iter()
        .position(|e| e.class == "building")
        .unwrap();
    assert_eq!(
        saved.entities[at]
            .common
            .as_ref()
            .unwrap()
            .building
            .as_ref()
            .unwrap()
            .fill,
        1
    );
    // The capture test fails for it now: the task ends, the claim is released.
    until(&mut w, &cat, &mut rng, h, 5, |w| task(w, h).0 != CAPTURE);
    assert_eq!(animal(&mut w, HERON).claim, 0);
    // Hovering, it idles in the air.
    assert_eq!(top(&w, h).as_deref(), Some("bIdleAir"));
    let Part::Staff(s) = &w.units.get(h).unwrap().part else {
        unreachable!()
    };
    assert_eq!(s.captured, 0);
}

#[test]
fn with_no_other_escape_it_returns_lands_on_the_slot_point_and_waits_again() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    until(&mut w, &cat, &mut rng, h, 200, |w| {
        task(w, h).0 == ENTER_BUILDING
    });
    // Back over the base's tile, still at the exit height; the goal's init lists it at the next update.
    assert_eq!((point(&w, h).0 / 64, point(&w, h).1 / 64), (11, 10));
    assert_eq!((base(&w).users_now, point(&w, h).2), (0, 160));
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(base(&w).users, [(h, 0)]);
    until(&mut w, &cat, &mut rng, h, 200, |w| {
        top(w, h).as_deref() == Some("bSpindown")
    });
    assert_eq!(point(&w, h), SLOT);
    assert!(heli(&w, h).flying);
    until(&mut w, &cat, &mut rng, h, 80, |w| task(w, h).0 == USE);
    assert!(!heli(&w, h).flying);
    assert_eq!(base(&w).users, [(h, 0)]);
    // The spin down is over and flying is 0: the idle set of the ground follows.
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(top(&w, h).as_deref(), Some("bIdleGround"));
}

#[test]
fn a_helicopter_flying_home_turns_to_a_new_escape_at_once() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10)), (OTHER, (2, 2))]);
    animal(&mut w, OTHER).escaped = false;
    until(&mut w, &cat, &mut rng, h, 200, |w| task(w, h).0 == ENTRANCE);
    animal(&mut w, OTHER).escaped = true;
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(task(&w, h), (CAPTURE, 0, OTHER));
    // Landing waits: an escape seen then is for the next wait.
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10)), (OTHER, (2, 2))]);
    animal(&mut w, OTHER).escaped = false;
    until(&mut w, &cat, &mut rng, h, 200, |w| {
        task(w, h).0 == ENTER_BUILDING
    });
    animal(&mut w, OTHER).escaped = true;
    step(&mut w, &cat, &mut rng, h, 67);
    assert_eq!(task(&w, h).0, ENTER_BUILDING);
}

#[test]
fn a_helicopter_draws_once_per_goal_update_and_nothing_else_from_take_off_to_landing() {
    let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    let (mut counted, mut steps) = (rng.clone(), 0);
    for stop in [ENTER_BUILDING, USE] {
        while task(&w, h).0 != stop && steps < 1000 {
            step(&mut w, &cat, &mut rng, h, 67);
            counted.pick(15);
            steps += 1;
        }
    }
    assert!(steps > 40);
    assert_eq!(rng, counted);
}

#[test]
fn the_slot_and_exit_points_ignore_the_heights_at_an_odd_rotation() {
    let (mut w, cat, _) = zoo(&[]);
    let at = w.find(BASE).unwrap();
    let pts = |w: &World| crate::staff::heli::points(w, &cat, at, 0).unwrap();
    assert_eq!(pts(&w), (SLOT, EXIT));
    w.zoo.entities[at].common.as_mut().unwrap().rotation = 1;
    assert_eq!(pts(&w), ((640, 640, 0), (640, 640, 0)));
}

#[test]
fn a_saved_helicopter_in_the_air_comes_back_with_its_words_goal_and_destination() {
    let (w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
    let mut zoo = w.saved(|_| true);
    w.finish_save(&mut zoo, &cat);
    let c = zoo
        .entities
        .iter()
        .find(|e| e.class == "helicopter")
        .unwrap()
        .common
        .clone()
        .unwrap();
    let own = c.staff.as_ref().unwrap().own.clone();
    assert_eq!(
        own,
        [BASE, 1, 2]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<u8>>()
    );
    // Its movement is the free one while it flies.
    assert_eq!(c.unit.as_ref().unwrap().move_kind, 1);
    let mut back = crate::world::World::new(zoo, testing::shape);
    for at in 0..back.zoo.entities.len() {
        crate::staff::make(&mut back, &cat, &mut rng, at);
    }
    crate::unit::resume::load(&mut back, 0);
    let (before, after) = (w.units.get(h).unwrap(), back.units.get(h).unwrap());
    assert_eq!(heli(&back, h).level(), Some(0));
    assert_eq!(
        (
            heli(&back, h).base,
            heli(&back, h).flying,
            heli(&back, h).dir
        ),
        (BASE, true, 2)
    );
    assert_eq!(after.destination, before.destination);
    assert_eq!(after.goal.record.task, before.goal.record.task);
    assert_eq!(after.goal.record.job, before.goal.record.job);
}

#[test]
fn a_landed_helicopter_is_put_at_its_slot_point_on_the_map_but_one_in_the_air_stays() {
    let (mut w, cat, h) = zoo(&[]);
    let at = w.find(h).unwrap();
    let mut rng = Rng::default();
    w.place_unit(at, (300, 300, 0), 3);
    crate::staff::make(&mut w, &cat, &mut rng, at);
    // The facing is not changed.
    assert_eq!(point(&w, h), SLOT);
    assert_eq!(w.zoo.entities[at].common.as_ref().unwrap().rotation, 3);
    let own = [BASE, 1, 0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    w.zoo.entities[at]
        .common
        .as_mut()
        .unwrap()
        .staff
        .as_mut()
        .unwrap()
        .own = own;
    w.place_unit(at, (300, 300, 0), 3);
    crate::staff::make(&mut w, &cat, &mut rng, at);
    assert_eq!(point(&w, h), (300, 300, 0));
}

#[test]
fn a_helicopter_of_a_loaded_zoo_draws_its_work_and_chase_counters_once() {
    // Two draws, work then chase, no colour (staff 4.2).
    let mut want = Rng::default();
    let counters = (want.pick(5) as i32, want.pick(2) as i32);
    let (w, cat, h) = zoo(&[]);
    let (mut back, mut rng) = (
        World::new(w.saved(|_| true), testing::shape),
        Rng::default(),
    );
    for at in 0..back.zoo.entities.len() {
        crate::staff::make(&mut back, &cat, &mut rng, at);
    }
    // The base draws nothing, and the helicopter's record overwrites its counters.
    assert_eq!(rng, want);
    let at = back.find(h).unwrap();
    let c = back.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!(
        (
            c.staff.as_ref().unwrap().work,
            c.staff.as_ref().unwrap().chase
        ),
        counters
    );
    // The birth drew them once, before the unit was made.
    let mut birth = Rng::default();
    births(&mut zoo(&[]).0, &cat, BASE, &mut birth, 9, testing::shape);
    assert_eq!(birth, want);
}

fn rotor(w: &World, id: u32) -> Option<crate::staff::rotor::Rotor> {
    heli(w, id).rotor
}

#[test]
fn the_rotor_starts_with_the_take_off_fades_in_and_is_freed_after_the_landing() {
    let (mut w, cat, h) = zoo(&[(HERON, (19, 10))]);
    let mut rng = Rng::default();
    // The listener is where the helicopter stands, so the sound plays.
    let (x, y) = crate::render::order::project(0, SLOT.0, SLOT.1, SLOT.2);
    w.units.hearing.listener = (i64::from(x), i64::from(y));
    assert!(rotor(&w, h).is_none(), "none on the ground");
    until(&mut w, &cat, &mut rng, h, 80, |w| heli(w, h).flying);
    let r = rotor(&w, h).unwrap();
    assert_eq!(
        (r.file.as_deref(), r.playing),
        (Some("sounds/helicopt.wav"), true)
    );
    // It fades in over 4 s, 67 ms a step whatever the pace of the animation (a moving unit's
    // animation runs at its speed percentage), and is not freed while it flies.
    let mut last = r.level();
    for _ in 0..20 {
        w.units.get_mut(h).unwrap().anim_speed = 300;
        step(&mut w, &cat, &mut rng, h, 67);
        let level = rotor(&w, h).unwrap().level();
        assert!((49..=51).contains(&(last - level)), "{last} then {level}");
        last = level;
    }
    until(&mut w, &cat, &mut rng, h, 90, |w| {
        rotor(w, h).is_some_and(|r| r.level() == 0)
    });
    let r = rotor(&w, h).unwrap();
    assert!(r.playing, "the chase stays in earshot");
    // The landing: flying goes to 0, the sound fades out in 4 s, then it is freed.
    until(&mut w, &cat, &mut rng, h, 2000, |w| !heli(w, h).flying);
    assert!(rotor(&w, h).is_some_and(|r| r.level() < 3000 || r.playing));
    until(&mut w, &cat, &mut rng, h, 100, |w| rotor(w, h).is_none());
}

#[test]
fn a_helicopter_loaded_in_the_air_starts_its_sound_at_its_first_update_and_a_landed_one_has_none() {
    let (mut w, cat, h) = zoo(&[]);
    let mut rng = Rng::default();
    let (x, y) = crate::render::order::project(0, SLOT.0, SLOT.1, SLOT.2);
    w.units.hearing.listener = (i64::from(x), i64::from(y));
    let own = |flying: u8| [BASE as u8, 0, 0, 0, flying, 0, 0, 0, 0xff, 0xff, 0xff, 0xff];
    let load = |w: &mut World, flying| {
        if let Part::Staff(s) = &mut w.units.get_mut(h).unwrap().part {
            s.heli = crate::staff::heli::Heli::from_own(&own(flying));
        }
    };
    load(&mut w, 0);
    step(&mut w, &cat, &mut rng, h, 67);
    assert!(rotor(&w, h).is_none(), "a landed one has no sound");
    load(&mut w, 1);
    assert!(
        rotor(&w, h).is_some_and(|r| r.file.is_none() && !r.playing),
        "not started yet"
    );
    step(&mut w, &cat, &mut rng, h, 67);
    let r = rotor(&w, h).unwrap();
    // Started at level 3000, then 0 with no fade: the full level at the first refresh.
    assert_eq!(
        (r.file.as_deref(), r.playing, r.level(), r.heard),
        (Some("sounds/helicopt.wav"), true, 0, (1200, 0))
    );
    // The same out of earshot: the start is refused and the helicopter stays silent for good.
    load(&mut w, 1);
    w.units.hearing.listener = (i64::from(x) + 5000, i64::from(y));
    step(&mut w, &cat, &mut rng, h, 67);
    w.units.hearing.listener = (i64::from(x), i64::from(y));
    for _ in 0..5 {
        step(&mut w, &cat, &mut rng, h, 67);
    }
    assert!(rotor(&w, h).is_some_and(|r| !r.playing));
}

#[test]
fn the_rotor_fade_has_moved_in_the_update_that_takes_off() {
    let (mut w, cat, h) = zoo(&[(HERON, (19, 10))]);
    let mut rng = Rng::default();
    let (x, y) = crate::render::order::project(0, SLOT.0, SLOT.1, SLOT.2);
    w.units.hearing.listener = (i64::from(x), i64::from(y));
    until(&mut w, &cat, &mut rng, h, 80, |w| heli(w, h).flying);
    // The counter has moved by the 67 ms step of that very update: 4000 - 67 = 3933.
    assert_eq!(rotor(&w, h).unwrap().level(), 2950);
}

#[test]
fn a_helicopter_loaded_in_the_air_starts_its_sound_at_the_stored_position() {
    // At the 6000 limit (1200 + 3000 + X <= 6000): X = 2.75 * 650 = 1787 at the stored position,
    // and the 16 units of the first movement take it over for some side of the listener.
    for (dx, dy) in [(650, 0), (-650, 0), (0, 650), (0, -650)] {
        let (mut w, cat, mut rng, h) = chasing(&[(HERON, (19, 10))]);
        if let Part::Staff(s) = &mut w.units.get_mut(h).unwrap().part {
            s.heli.rotor = Some(crate::staff::rotor::Rotor::loaded_in_air());
        }
        let (px, py, pz) = point(&w, h);
        let (x, y) = crate::render::order::project(0, px, py, pz);
        w.units.hearing.listener = (i64::from(x) + dx, i64::from(y) + dy);
        step(&mut w, &cat, &mut rng, h, 67);
        assert_eq!(point(&w, h).0 - px, 16, "it moved");
        assert!(rotor(&w, h).unwrap().playing, "({dx}, {dy})");
    }
}
