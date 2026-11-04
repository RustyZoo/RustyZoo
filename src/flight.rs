//! The flying objects the date hooks launch (ambients 3 and 4): the sleigh, the biplane and the
//! witch are creatures of the class `ambient`. One starts at a random edge tile of the map, facing
//! inwards, and flies in a straight line at the height it started at until its tile is off the map;
//! the next step's removal pass then destroys it.
//!
//! Not done yet: the spawn list (periodic birds, the permanent witch), the animation of the
//! frames, the group sound of the witch and the birds, and the shadow over the water of a tank.

use crate::config::to_int;
use crate::entity::{Catalogue, EntityType};
use crate::render::order::{STEP, ground_height};
use crate::render::shape_of;
use crate::sim::Rng;
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
    let (dx, dy) = STEP[(c.rotation % 8) as usize];
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
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
}
