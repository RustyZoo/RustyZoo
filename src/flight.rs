//! The flying objects the date hooks launch (ambients 3 and 4): the sleigh, the biplane and the
//! witch are creatures of the class `ambient`. One starts at a random edge tile of the map, facing
//! inwards, and flies in a straight line at the height it started at until its tile is off the map;
//! the next step's removal pass then destroys it.
//!
//! The world keeps a spawn list ([`Spawns`], ambients 7): the launches, the scenario's ambients and
//! the permanent witch add entries, and the world manager makes a creature for each entry whose
//! type has a period that has passed.
//!
//! Every move of a creature plays the call its type names (`cSoundName`, ambients 6.4); a creature
//! added with no name gets its type's text alone (ambients 3.3).
//!
//! A launch runs inside the guest creation of the step, so the flight moves in that same step
//! (ambients 3.1). Not done yet: the shadow over the water of a tank.

use std::collections::BTreeMap;

use crate::behaviour::Data;
use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::render::order::{STEP, ground_height};
use crate::render::shape_of;
use crate::sim::Rng;
use crate::soundscape::Calls;
use crate::world::World;
use crate::zoo::{Common, Entity, Flight};

/// The facing of a flight for each side and each choice (ambients 3.2): all point into the map.
const FACING: [[u32; 3]; 4] = [[1, 0, 7], [3, 4, 5], [7, 6, 5], [1, 2, 3]];

/// The ambient type with the name id `id`, if the catalogue has one.
pub(crate) fn ambient(cat: &Catalogue, id: i32) -> Option<&EntityType> {
    cat.types
        .iter()
        .find(|t| t.class == "ambient" && t.name_id == id)
}

/// `cSpeed` of the type, 0 by default; a negative value is the huge number the original reads.
fn speed(ty: &EntityType) -> u32 {
    let section = ty.config.as_deref();
    let section = section.and_then(|c| c.section("Characteristics/Integers"));
    section
        .and_then(|s| s.get("cSpeed"))
        .and_then(to_int)
        .map_or(0, |n| n as u32)
}

/// `cFrequency` of the type: the seconds between two spawns, 0 (never) by default; read as the
/// signed number it is (ambients 2 point 1, 7 point 2).
fn frequency(ty: &EntityType) -> i32 {
    let section = ty.config.as_deref();
    let section = section.and_then(|c| c.section("Characteristics/Integers"));
    section
        .and_then(|s| s.get("cFrequency"))
        .and_then(to_int)
        .unwrap_or(0)
}

/// `cSoundName` of the type: the file of its call, empty (no call) by default.
fn sound_name(ty: &EntityType) -> String {
    let section = ty.config.as_deref();
    let section = section.and_then(|c| c.section("Characteristics/Strings"));
    section
        .and_then(|s| s.get("cSoundName"))
        .map_or_else(String::new, crate::vfs::name_from_bytes)
}

/// The spawn list of the world (ambients 7): for each entry the type's name id and the world second
/// of its last spawn. It is not saved and a game end empties it; the periods set by [`Spawns::period`]
/// belong to the types, so they outlive a game.
#[derive(Debug, Default)]
pub(crate) struct Spawns {
    list: Vec<(i32, u32)>,
    periods: BTreeMap<i32, i32>,
}

impl Spawns {
    /// A game ends: the list is empty.
    pub(crate) fn clear(&mut self) {
        self.list.clear();
    }

    /// Adds an entry for the type `id` with last spawn 0, even for a type already listed.
    pub(crate) fn add(&mut self, id: i32) {
        self.list.push((id, 0));
    }

    /// The name ids of the entries, in list order.
    #[cfg(test)]
    pub(crate) fn ids(&self) -> Vec<i32> {
        self.list.iter().map(|e| e.0).collect()
    }

    /// The type `id` spawns every `seconds` from now on, whatever its file says (the permanent
    /// witch, ambients 7 point 4).
    pub(crate) fn period(&mut self, id: i32, seconds: i32) {
        self.periods.insert(id, seconds);
    }

    /// The types to make a creature of at the end of a world step whose clock reads `clock` whole
    /// seconds, in list order: the entries whose period is not 0 and is at most the seconds since
    /// their last spawn (signed); their last spawn becomes `clock`. An entry whose type the
    /// catalogue lacks never spawns.
    pub(crate) fn due<'c>(&mut self, cat: &'c Catalogue, clock: u32) -> Vec<&'c EntityType> {
        let mut out = Vec::new();
        for (id, last) in &mut self.list {
            let Some(ty) = ambient(cat, *id) else {
                continue;
            };
            let every = self
                .periods
                .get(id)
                .copied()
                .unwrap_or_else(|| frequency(ty));
            if every != 0 && every <= clock.wrapping_sub(*last) as i32 {
                *last = clock;
                out.push(ty);
            }
        }
        out
    }
}

/// Creates one creature of the ambient type `ty` and adds it to the world (ambients 3.2, 3.3):
/// three draws, the side, the choice (always 1, the straight one, when the type's picture has four
/// files: `files`) and the tile along the side; it stands at the corner of that tile on the ground.
/// The id, or `None` for a map without tiles.
pub(crate) fn launch(
    world: &mut World,
    cat: &Catalogue,
    ty: &EntityType,
    files: usize,
    rng: &mut Rng,
) -> Option<u32> {
    let (w, h) = (world.zoo.width, world.zoo.height);
    let (wide, high) = (i32::try_from(w).ok()?, i32::try_from(h).ok()?);
    if wide == 0 || high == 0 {
        return None;
    }
    let side = (rng.rand() & 3) as usize;
    let drawn = rng.rand() % 3;
    let choice = if files == 4 { 1 } else { drawn as usize };
    let along = rng.pick(if side < 2 { w } else { h }) as i32;
    let (tx, ty_) = match side {
        0 => (along, high - 1),
        1 => (along, 0),
        2 => (wide - 1, along),
        _ => (0, along),
    };
    let tile = world.tile_index(tx, ty_)?;
    let text = |bytes: &[u8]| bytes.iter().map(|&b| char::from(b)).collect::<String>();
    let entity = Entity {
        class: "ambient".into(),
        type_name: text(&ty.type_name),
        subtype: text(&ty.subtype),
        body_size: None,
        common: Some(Common {
            x: tx * 64,
            y: ty_ * 64,
            z: ground_height(&world.zoo.tiles[tile], 0, 0),
            rotation: FACING[side][choice],
            name: crate::place::unit::named(world, ty),
            flight: Some(Flight {
                speed: speed(ty),
                acc: 0,
            }),
            ..Default::default()
        }),
    };
    let shape = shape_of(cat, &entity);
    world.unrecorded(|w| w.add(entity, shape))
}

/// The move of the creature at position `at` of the entity list in a step of `step` ms (ambients
/// 4.1): `cSpeed` units per second, the remainder kept; when its tile is off the map it asks for
/// its removal. Any other entity is left alone.
pub(crate) fn advance(world: &mut World, at: usize, step: u32, removals: &mut Vec<u32>) {
    let (w, h) = (world.zoo.width, world.zoo.height);
    let Some(c) = (world.zoo.entities.get_mut(at))
        .filter(|e| e.class == "ambient")
        .and_then(|e| e.common.as_mut())
    else {
        return;
    };
    let Some(flight) = c.flight.as_mut() else {
        return;
    };
    if flight.speed == 0 {
        flight.acc = 0;
        return;
    }
    let acc = flight.acc.wrapping_add(flight.speed.wrapping_mul(step));
    // Under 1,000 it still moves one unit and keeps the whole accumulator (the original's quirk).
    let n = match acc >= 1000 {
        true => {
            flight.acc = acc % 1000;
            acc / 1000
        }
        false => {
            flight.acc = acc;
            1
        }
    };
    let n = i32::try_from(n).unwrap_or(i32::MAX);
    // A facing outside 0..7 gives the step (0, 0): the creature stays.
    let (dx, dy) = STEP.get(c.rotation as usize).copied().unwrap_or((0, 0));
    let (x, y) = (c.x.saturating_add(dx * n), c.y.saturating_add(dy * n));
    let (id, z, rotation) = (c.id, c.z, c.rotation);
    // Cut towards zero: the creature is still on the map one tile beyond the low edges.
    let on_map = (0..i64::from(w)).contains(&i64::from(x / 64))
        && (0..i64::from(h)).contains(&i64::from(y / 64));
    world.place_unit(at, (x, y, z), rotation);
    if !on_map {
        removals.push(id);
    }
}

/// The call of the creature at position `at` of the entity list after its move (ambients 6.4): its
/// group plays at the creature's map position with the step `step`, whatever the move made of the
/// creature. A creature without speed, or any other entity, is silent.
pub(crate) fn call(
    calls: &mut Calls,
    world: &World,
    cat: &Catalogue,
    data: &dyn Data,
    at: usize,
    step: u32,
    rng: &mut Rng,
) {
    let e = &world.zoo.entities[at];
    let Some(c) = e.common.as_ref().filter(|_| e.class == "ambient") else {
        return;
    };
    if c.flight.is_none_or(|f| f.speed == 0) {
        return;
    }
    let Some(ty) = crate::place::bulldozer::type_of(cat, e) else {
        return;
    };
    calls.play(&sound_name(ty), &|n| data.read(n), (c.x, c.y), step, rng);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::sim::Sim;
    use crate::unit::testing::world;
    use std::sync::Arc;

    /// An ambient type whose file has `ini` in its integers, and a catalogue of it.
    fn kind(ini: &str) -> (Catalogue, EntityType) {
        let config = format!("[Characteristics/Integers]\ncNameID=5083\n{ini}");
        let ty = || EntityType {
            class: "ambient",
            type_name: b"sleigh".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(Arc::new(Config::parse(config.as_bytes()).unwrap())),
            name_id: 5083,
            footprint: [1, 1, 1],
            height: 0,
        };
        let mut cat = Catalogue::default();
        cat.push(ty());
        (cat, ty())
    }

    fn rng(seed: u32) -> Rng {
        let mut rng = Rng::default();
        rng.seed(seed);
        rng
    }

    /// The first creature of the world: position, facing and its speed and accumulator.
    fn state(w: &World) -> ((i32, i32, i32), u32, Flight) {
        let c = w.zoo.entities[0].common.as_ref().unwrap();
        ((c.x, c.y, c.z), c.rotation, c.flight.unwrap())
    }

    /// A world of 5 by 4 tiles with one creature at `(x, y)` facing `facing`, of speed `speed`.
    fn flying(at: (i32, i32), facing: u32, speed: u32) -> World {
        world((5, 4), |_, entities| {
            entities.push(Entity {
                class: "ambient".into(),
                type_name: "sleigh".into(),
                subtype: String::new(),
                body_size: None,
                common: Some(Common {
                    x: at.0,
                    y: at.1,
                    id: 7,
                    rotation: facing,
                    flight: Some(Flight { speed, acc: 0 }),
                    ..Default::default()
                }),
            })
        })
    }

    /// The type ids of a list of due types.
    fn ids(due: &[&EntityType]) -> Vec<i32> {
        due.iter().map(|t| t.name_id).collect()
    }

    #[test]
    fn an_entry_spawns_once_its_period_has_passed_and_again_a_period_later() {
        let (cat, _) = kind("cFrequency=20\n");
        let mut spawns = Spawns::default();
        spawns.add(5083);
        assert!(spawns.due(&cat, 19).is_empty());
        assert_eq!(ids(&spawns.due(&cat, 20)), [5083]);
        assert!(spawns.due(&cat, 20).is_empty(), "last spawn is now");
        assert!(spawns.due(&cat, 39).is_empty());
        assert_eq!(ids(&spawns.due(&cat, 40)), [5083]);
        // A step that jumps over several periods spawns once.
        assert_eq!(ids(&spawns.due(&cat, 500)), [5083]);
    }

    #[test]
    fn an_entry_listed_twice_spawns_twice() {
        let (cat, _) = kind("cFrequency=5\n");
        let mut spawns = Spawns::default();
        spawns.add(5083);
        spawns.add(5083);
        assert_eq!(ids(&spawns.due(&cat, 5)), [5083, 5083]);
    }

    #[test]
    fn a_period_of_zero_or_a_missing_key_never_spawns() {
        for ini in ["cFrequency=0\n", "", "cFrequency=x\n"] {
            let (cat, _) = kind(ini);
            let mut spawns = Spawns::default();
            spawns.add(5083);
            assert!(spawns.due(&cat, 100_000).is_empty(), "{ini:?}");
        }
    }

    #[test]
    fn a_negative_period_spawns_at_every_check_and_a_huge_one_never() {
        let (cat, _) = kind("cFrequency=-3\n");
        let mut spawns = Spawns::default();
        spawns.add(5083);
        for clock in [0, 1, 2] {
            assert_eq!(spawns.due(&cat, clock).len(), 1, "clock {clock}");
        }
        let (cat, _) = kind("cFrequency=2147483647\n");
        let mut spawns = Spawns::default();
        spawns.add(5083);
        assert!(spawns.due(&cat, i32::MAX as u32 - 1).is_empty());
        assert_eq!(spawns.due(&cat, i32::MAX as u32).len(), 1);
    }

    #[test]
    fn a_clock_behind_the_last_spawn_is_a_negative_age() {
        let (cat, _) = kind("cFrequency=5\n");
        let mut spawns = Spawns::default();
        spawns.add(5083);
        spawns.due(&cat, 100);
        // A clock that went back (a load) waits until the age is a period again.
        assert!(spawns.due(&cat, 50).is_empty());
        assert!(spawns.due(&cat, u32::MAX).is_empty());
    }

    #[test]
    fn an_entry_of_a_type_the_catalogue_lacks_never_spawns() {
        let (cat, _) = kind("cFrequency=1\n");
        let mut spawns = Spawns::default();
        spawns.add(9999);
        assert!(spawns.due(&cat, 100).is_empty());
    }

    #[test]
    fn a_set_period_replaces_the_files_and_outlives_a_game_end() {
        let (cat, _) = kind("cFrequency=0\n");
        let mut spawns = Spawns::default();
        spawns.period(5083, 30);
        spawns.add(5083);
        assert!(spawns.due(&cat, 29).is_empty());
        assert_eq!(spawns.due(&cat, 30).len(), 1);
        spawns.clear();
        assert!(spawns.due(&cat, 1000).is_empty(), "the list is empty");
        spawns.add(5083);
        assert_eq!(spawns.due(&cat, 1000).len(), 1, "the period stayed");
    }

    #[test]
    fn the_world_step_makes_a_creature_for_each_due_entry_with_three_draws() {
        let (cat, _) = kind("cFrequency=2\n");
        let step = |sim: &mut Sim, w: &mut World| _ = sim.step_in(w, &cat, 1000, |_, _, _| {});
        let (mut sim, mut plain) = (Sim::new(b""), Sim::new(b""));
        let (mut w, mut other) = (world((5, 4), |_, _| {}), world((5, 4), |_, _| {}));
        sim.set_seed(5);
        plain.set_seed(5);
        sim.spawns.add(5083);
        step(&mut sim, &mut w);
        step(&mut plain, &mut other);
        assert!(w.zoo.entities.is_empty(), "1 s");
        step(&mut sim, &mut w);
        step(&mut plain, &mut other);
        assert_eq!(w.zoo.entities.len(), 1, "2 s");
        assert!(other.zoo.entities.is_empty());
        // The creature's three draws come at the end of the step.
        (0..3).for_each(|_| _ = plain.rng.rand());
        assert_eq!(sim.rng, plain.rng);
        (0..4).for_each(|_| step(&mut sim, &mut w));
        assert_eq!(w.zoo.entities.len(), 3, "6 s: one more at 4 s and at 6 s");
    }

    #[test]
    fn a_game_start_empties_the_list() {
        let (cat, _) = kind("cFrequency=1\n");
        let mut sim = Sim::new(b"");
        sim.spawns.add(5083);
        sim.start_game(0, Default::default());
        let mut w = world((5, 4), |_, _| {});
        sim.step_in(&mut w, &cat, 5000, |_, _, _| {});
        assert!(w.zoo.entities.is_empty());
    }

    #[test]
    fn a_creature_starts_on_an_edge_facing_inwards_after_three_draws() {
        let (cat, ty) = kind("cSpeed=100\n");
        for seed in 1..80 {
            let mut w = world((5, 4), |_, _| {});
            let mut r = rng(seed);
            let id = launch(&mut w, &cat, &ty, 5, &mut r).expect("made");
            let ((x, y, z), facing, flight) = state(&w);
            let mut check = rng(seed);
            let (side, choice) = (check.rand() & 3, check.rand() % 3);
            let along = check.rand() as i32;
            assert_eq!(r, check, "three draws");
            // Side, tile and the three facings of the table, written out.
            let (tile, want) = match side {
                0 => ((along % 5, 3), [1, 0, 7]),
                1 => ((along % 5, 0), [3, 4, 5]),
                2 => ((4, along % 4), [7, 6, 5]),
                _ => ((0, along % 4), [1, 2, 3]),
            };
            assert_eq!(((x, y), z), ((tile.0 * 64, tile.1 * 64), 0), "seed {seed}");
            assert_eq!(facing, want[choice as usize], "seed {seed}");
            assert_eq!(flight, Flight { speed: 100, acc: 0 });
            assert_eq!(w.find(id), Some(0));
            // Every facing points away from its side.
            let (dx, dy) = STEP[facing as usize];
            let inward = [dy < 0, dy > 0, dx < 0, dx > 0];
            assert!(inward[side as usize], "seed {seed}");
        }
    }

    #[test]
    fn a_type_with_four_picture_files_always_flies_straight() {
        let (cat, ty) = kind("");
        for seed in 1..80 {
            let mut w = world((5, 4), |_, _| {});
            launch(&mut w, &cat, &ty, 4, &mut rng(seed)).unwrap();
            assert!(state(&w).1.is_multiple_of(2), "seed {seed}");
        }
    }

    #[test]
    fn a_creature_stands_at_the_height_of_the_corner_it_starts_on() {
        let (cat, ty) = kind("");
        let mut w = world((5, 4), |tiles, _| {
            tiles.iter_mut().for_each(|t| t.elevation = 3)
        });
        launch(&mut w, &cat, &ty, 5, &mut rng(1)).unwrap();
        assert_eq!(state(&w).0.2, 48);
    }

    #[test]
    fn a_map_without_tiles_makes_no_creature_and_draws_nothing() {
        let (cat, ty) = kind("");
        let mut w = world((0, 0), |_, _| {});
        let mut r = rng(1);
        assert_eq!(launch(&mut w, &cat, &ty, 5, &mut r), None);
        assert_eq!(r, rng(1));
        assert!(w.zoo.entities.is_empty());
    }

    #[test]
    fn the_speed_is_read_from_the_type_with_zero_for_a_missing_or_bad_key() {
        let speed = |ini| speed(&kind(ini).1);
        assert_eq!(speed("cSpeed=100\n"), 100);
        assert_eq!(speed(""), 0);
        assert_eq!(speed("cSpeed=x\n"), 0);
        assert_eq!(speed("cSpeed=-1\n"), u32::MAX);
        assert_eq!(speed("cSpeed=2147483647\n"), i32::MAX as u32);
    }

    #[test]
    fn a_creature_moves_its_speed_in_units_per_second_keeping_the_remainder() {
        let mut w = flying((0, 0), 4, 100);
        let mut removals = Vec::new();
        for k in 1..=35u32 {
            advance(&mut w, 0, 67, &mut removals);
            let ((x, y, _), _, flight) = state(&w);
            assert_eq!((x, y as u32), (0, 6700 * k / 1000), "step {k}");
            assert_eq!(flight.acc, 6700 * k % 1000);
        }
        assert!(removals.is_empty());
    }

    #[test]
    fn a_facing_outside_0_to_7_does_not_move_the_creature() {
        for facing in [8, 12, 1 << 20, u32::MAX] {
            let mut w = flying((100, 100), facing, 1000);
            let mut removals = Vec::new();
            advance(&mut w, 0, 50, &mut removals);
            assert_eq!(state(&w).0, (100, 100, 0), "facing {facing}");
            assert!(removals.is_empty());
        }
    }

    #[test]
    fn a_diagonal_creature_moves_on_both_axes() {
        let mut w = flying((0, 0), 3, 1000);
        advance(&mut w, 0, 5, &mut Vec::new());
        assert_eq!(state(&w).0, (5, 5, 0));
    }

    #[test]
    fn below_a_thousand_it_moves_one_unit_and_keeps_the_whole_accumulator() {
        let mut w = flying((0, 0), 4, 100);
        advance(&mut w, 0, 5, &mut Vec::new());
        assert_eq!((state(&w).0.1, state(&w).2.acc), (1, 500));
        advance(&mut w, 0, 5, &mut Vec::new());
        assert_eq!((state(&w).0.1, state(&w).2.acc), (2, 0));
    }

    #[test]
    fn a_creature_without_speed_stays_and_is_never_removed() {
        let mut w = flying((40, 40), 4, 0);
        w.zoo.entities[0].common.as_mut().unwrap().flight = Some(Flight { speed: 0, acc: 9 });
        let mut removals = Vec::new();
        advance(&mut w, 0, 1000, &mut removals);
        assert_eq!(state(&w), ((40, 40, 0), 4, Flight { speed: 0, acc: 0 }));
        assert!(removals.is_empty());
    }

    #[test]
    fn it_is_on_the_map_one_tile_beyond_the_low_edges_and_off_at_the_high_ones() {
        // Speed 1000: a step of s ms moves s units.
        let leave = |at, facing, step| {
            let mut w = flying(at, facing, 1000);
            let mut removals = Vec::new();
            advance(&mut w, 0, step, &mut removals);
            removals
        };
        assert!(leave((0, 0), 6, 63).is_empty(), "-63 is still tile 0");
        assert_eq!(leave((0, 0), 6, 64), [7]);
        assert!(leave((100, 0), 0, 63).is_empty());
        assert_eq!(leave((100, 0), 0, 64), [7]);
        assert!(leave((318, 0), 2, 1).is_empty());
        assert_eq!(leave((319, 0), 2, 1), [7], "x = 320 is tile 5 of 5");
        assert_eq!(leave((0, 255), 4, 1), [7], "y = 256 is tile 4 of 4");
        assert!(leave((0, 254), 4, 1).is_empty());
    }

    #[test]
    fn a_huge_speed_or_step_is_no_panic() {
        for speed in [u32::MAX, i32::MAX as u32, 1 << 31] {
            let mut w = flying((100, 100), 2, speed);
            for step in [u32::MAX, 1, 0, 67] {
                advance(&mut w, 0, step, &mut Vec::new());
            }
        }
    }

    #[test]
    fn other_entities_and_missing_positions_are_left_alone() {
        let mut w = world((5, 4), |_, entities| {
            entities.push(Entity {
                class: "keeper".into(),
                type_name: String::new(),
                subtype: String::new(),
                body_size: None,
                common: Some(Common::default()),
            });
            entities.push(Entity {
                class: "ambient".into(),
                type_name: "bird".into(),
                subtype: String::new(),
                body_size: None,
                common: Some(Common::default()),
            });
        });
        let mut removals = Vec::new();
        for at in [0, 1, 2] {
            advance(&mut w, at, 1000, &mut removals);
        }
        assert!(removals.is_empty());
    }

    /// A type of the ambient class with the given strings section and a name text, and its catalogue.
    fn typed(strings: &str) -> (Catalogue, EntityType) {
        let config = format!(
            "[Characteristics/Integers]\ncNameID=5083\ncSpeed=100\n[Characteristics/Strings]\n{strings}"
        );
        let ty = || EntityType {
            class: "ambient",
            type_name: b"sleigh".to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(Arc::new(Config::parse(config.as_bytes()).unwrap())),
            name_id: 5083,
            footprint: [1, 1, 1],
            height: 0,
        };
        let mut cat = Catalogue::default();
        cat.push(ty());
        (cat, ty())
    }

    /// The name of a creature made while the type text is `text` (none: the interface gave no text).
    fn launched_name(text: Option<&str>) -> (String, Rng) {
        let (cat, ty) = kind("");
        let mut w = world((5, 4), |_, _| {});
        w.units.names = text.map(|t| (5083, t.to_string())).into_iter().collect();
        let mut r = rng(3);
        launch(&mut w, &cat, &ty, 5, &mut r).unwrap();
        (w.zoo.entities[0].common.as_ref().unwrap().name.clone(), r)
    }

    #[test]
    fn a_new_creature_is_named_by_its_type_text_alone() {
        for (text, want) in [
            (Some("Sleigh"), "Sleigh"),
            (Some(" Sleigh "), " Sleigh "),
            (Some("Sleigh 7!"), "Sleigh 7!"),
            (Some("."), "."),
            (Some(""), ""),
            (Some(" \t"), ""),
            (None, ""),
        ] {
            let (name, r) = launched_name(text);
            assert_eq!(name, want, "{text:?}");
            // No draw for the name: the three of the start only.
            assert_eq!(r, after(3, 3));
        }
    }

    fn after(seed: u32, n: usize) -> Rng {
        let mut r = rng(seed);
        (0..n).for_each(|_| _ = r.rand());
        r
    }

    #[test]
    fn a_creature_takes_no_number_and_no_counter_whatever_its_flag_says() {
        let (cat, ty) = kind("cUseNumbersInName=1\n");
        let mut w = world((5, 4), |_, _| {});
        w.units.names = [(5083, "Sleigh".to_string())].into();
        for _ in 0..2 {
            launch(&mut w, &cat, &ty, 5, &mut rng(1)).unwrap();
        }
        let names: Vec<_> = (w.zoo.entities.iter())
            .map(|e| e.common.as_ref().unwrap().name.clone())
            .collect();
        assert_eq!(names, ["Sleigh", "Sleigh"]);
        assert!(w.zoo.names.is_empty());
    }

    /// What the data holds: the files of the call, by name.
    struct Files(&'static [(&'static str, &'static str)]);

    impl Data for Files {
        fn set(&self, _: &Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
            None
        }
        fn ambient(&self, _: &Entity, _: usize) -> &[crate::behaviour::Row] {
            &[]
        }
        fn timing(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Timing> {
            None
        }
        fn number(&self, _: &Entity, _: &crate::behaviour::Number) -> i32 {
            0
        }
        fn sound(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Heard> {
            None
        }
        fn read(&self, name: &str) -> Option<Vec<u8>> {
            let found = self.0.iter().find(|(n, _)| *n == name);
            found.map(|(_, text)| text.as_bytes().to_vec())
        }
    }

    const HAWK: Files = Files(&[(
        "hawk.cfg",
        "[ambientlevels]\ngroup = happy\nvalue = -100\nvalue = 100\n[happy]\nchance = 1\nsound = hawk1.wav\nprob = 100\n",
    )]);

    /// Moves the creature `steps` times by `step` ms, calling after each move.
    fn flown(
        w: &mut World,
        cat: &Catalogue,
        data: &Files,
        calls: &mut Calls,
        rng: &mut Rng,
        step: u32,
    ) {
        advance(w, 0, step, &mut Vec::new());
        call(calls, w, cat, data, 0, step, rng);
    }

    #[test]
    fn each_move_plays_the_group_of_the_type_at_the_map_position() {
        let (cat, _) = typed("cSoundName=hawk.cfg\n");
        let mut w = flying((100, 200), 4, 1000);
        w.zoo.entities[0].type_name = "sleigh".into();
        let (mut calls, mut r) = (Calls::default(), rng(5));
        // 1000 ms joins the clock: not above a second, no draw.
        flown(&mut w, &cat, &HAWK, &mut calls, &mut r, 1000);
        assert_eq!(r, rng(5));
        assert!(calls.take().is_empty());
        // 1 ms more: above it, one draw for the chance (1 in 1) and one for the sound.
        flown(&mut w, &cat, &HAWK, &mut calls, &mut r, 1);
        assert_eq!(r, after(5, 2));
        let shots = calls.take();
        assert_eq!(shots.len(), 1);
        // The place is the position after the move, in map units, not a view offset.
        assert_eq!(
            (shots[0].file.as_str(), shots[0].at),
            ("hawk1.wav", Some((100, 201 + 1000)))
        );
        assert_eq!(shots[0].voice, "ambient 0 hawk1.wav");
    }

    #[test]
    fn creatures_of_a_type_share_one_clock_and_one_voice() {
        let (cat, _) = typed("cSoundName=hawk.cfg\n");
        let mut a = flying((0, 0), 4, 1000);
        let mut b = flying((64, 64), 4, 1000);
        let (mut calls, mut r) = (Calls::default(), rng(5));
        // Two creatures of 600 ms each pass a second together: the second one's move rolls.
        flown(&mut a, &cat, &HAWK, &mut calls, &mut r, 600);
        assert!(calls.take().is_empty());
        flown(&mut b, &cat, &HAWK, &mut calls, &mut r, 600);
        let shots = calls.take();
        assert_eq!(shots.len(), 1);
        assert_eq!(shots[0].at, Some((64, 64 + 600)));
    }

    #[test]
    fn a_type_without_a_sound_name_or_a_creature_without_speed_is_silent_and_draws_nothing() {
        let (cat, _) = typed("");
        let mut w = flying((0, 0), 4, 1000);
        let (mut calls, mut r) = (Calls::default(), rng(5));
        for _ in 0..3 {
            flown(&mut w, &cat, &HAWK, &mut calls, &mut r, 5000);
        }
        assert_eq!(r, rng(5));
        let (cat, _) = typed("cSoundName=hawk.cfg\n");
        let mut w = flying((0, 0), 4, 0);
        flown(&mut w, &cat, &HAWK, &mut calls, &mut r, 5000);
        assert_eq!(r, rng(5));
        // A name whose file is missing has no group; a huge step wraps without a panic.
        let (cat, _) = typed("cSoundName=none.cfg\n");
        let mut w = flying((0, 0), 4, 1000);
        for step in [u32::MAX, 0, u32::MAX] {
            flown(&mut w, &cat, &HAWK, &mut calls, &mut r, step);
        }
        assert!(calls.take().is_empty());
    }

    #[test]
    fn another_entity_does_not_call() {
        let (cat, _) = typed("cSoundName=hawk.cfg\n");
        let mut w = flying((0, 0), 4, 1000);
        w.zoo.entities[0].class = "keeper".into();
        let (mut calls, mut r) = (Calls::default(), rng(5));
        call(&mut calls, &w, &cat, &HAWK, 0, 5000, &mut r);
        assert_eq!(r, rng(5));
    }

    #[test]
    fn the_world_step_calls_for_each_creature_and_the_draws_move_the_stream() {
        let run = |strings: &str| {
            let (cat, _) = typed(strings);
            let (mut sim, mut w) = (Sim::new(b""), flying((0, 0), 4, 100));
            sim.set_seed(9);
            for _ in 0..2 {
                sim.step_with(&HAWK, &cat, &mut w, 600, |_, _, _| {});
            }
            (sim.take_shots(), sim.rng)
        };
        let (plain, silent) = run("");
        let (shots, rng) = run("cSoundName=hawk.cfg\n");
        assert!(plain.is_empty());
        // The second move rolled: the chance, then the sound; the creature stands at (0, 120).
        assert_eq!(shots.len(), 1);
        assert_eq!(
            (shots[0].file.as_str(), shots[0].at),
            ("hawk1.wav", Some((0, 120)))
        );
        let mut moved = silent;
        (0..2).for_each(|_| _ = moved.rand());
        assert_eq!(rng, moved);
    }
}
