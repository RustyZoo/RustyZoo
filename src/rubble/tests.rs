//! Turning to rubble (cheats 4.6) on synthetic worlds.

use super::*;
use crate::config::Config;
use crate::unit::testing::{centre, entity, world_with};

fn ty(
    class: &'static str,
    kind: &str,
    sub: &str,
    name_id: i32,
    fp: [i32; 2],
    keys: &str,
) -> EntityType {
    let text = format!("[Characteristics/Integers]\n{keys}\n");
    EntityType {
        class,
        type_name: kind.into(),
        subtype: sub.into(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id,
        footprint: [fp[0], fp[1], 1],
        height: 0,
    }
}

/// The rubble types, then a car (2 x 2), a tree (1 x 1, tree rubble), a restroom (1 x 1), a hall
/// (3 x 2), a hut (1 x 2) and a hut that cannot be rubbled.
fn cat(rubble: &[&str]) -> Catalogue {
    let mut cat = Catalogue::default();
    for sub in rubble {
        cat.push(ty("rubble", "other", sub, 0, [1, 1], ""));
    }
    cat.push(ty("objects", "car", "", 6138, [2, 2], ""));
    cat.push(ty("objects", "tree", "", 1, [1, 1], "cUsesTreeRubble=1"));
    cat.push(ty("building", "wc", "", RESTROOM, [1, 1], ""));
    cat.push(ty("building", "hall", "", 77, [3, 2], ""));
    cat.push(ty("building", "hut", "", 78, [1, 2], ""));
    cat.push(ty("building", "bunker", "", 79, [2, 2], "cRubbleable=0"));
    cat
}

const ALL: [&str; 7] = [
    "brubls1", "bstump1", "bathrubl", "brubble1", "brubble2", "brubble3", "brubble4",
];

fn world(item: (&str, &str), at: (i32, i32)) -> World {
    world_with((10, 10), |zoo| {
        let mut e = entity(item.0, item.1, 5, at, 0);
        e.common.as_mut().unwrap().name = "Thing".into();
        e.common.as_mut().unwrap().z = 35;
        zoo.entities.push(e);
    })
}

/// Turns entity 5 to rubble with the generator `rng`, and tells the pieces: their type, subtype,
/// position and rotation.
fn run(w: &mut World, cat: &Catalogue, rng: &mut Rng) -> Vec<(String, i32, i32, i32, u32)> {
    let shape = |_: &Entity| None;
    let mut kit = Kit {
        cat,
        rng,
        template: "piece-%s",
        shape: &shape,
    };
    assert!(turn_to_rubble(w, &mut kit, 5));
    let pieces = w.zoo.entities.iter().filter(|e| e.class == "rubble");
    pieces
        .map(|e| {
            let c = e.common.as_ref().unwrap();
            (e.subtype.clone(), c.x, c.y, c.z, c.rotation)
        })
        .collect()
}

#[test]
fn a_car_leaves_a_piece_per_quadrant_with_a_skip_draw_and_a_rotation_draw() {
    let cat = cat(&ALL);
    let mut w = world(("objects", "car"), (64, 64));
    let mut rng = Rng::default();
    let mut mirror = rng.clone();
    let pieces = run(&mut w, &cat, &mut rng);
    let mut want = Vec::new();
    for (i, j) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        let (x, y) = (64 - 32 + 16 + 32 * i, 64 - 32 + 16 + 32 * j);
        if mirror.pick(100) != 0 {
            want.push(("brubls1".to_string(), x, y, 0, mirror.rand() & 7));
        }
    }
    assert!(want.len() >= 3);
    assert_eq!(pieces, want);
    assert_eq!(rng, mirror);
    // The car is gone; the pieces are named, grouped and keep its name id.
    assert!(w.zoo.entities.iter().all(|e| e.class == "rubble"));
    let ids: Vec<u32> = w
        .zoo
        .entities
        .iter()
        .map(|e| e.common.as_ref().unwrap().id)
        .collect();
    for e in &w.zoo.entities {
        let c = e.common.as_ref().unwrap();
        assert_eq!(c.name, "piece-Thing");
        assert_eq!(
            c.rubble,
            Some(Rubble {
                group: ids.clone(),
                name_id: 6138
            })
        );
    }
}

#[test]
fn one_cell_in_a_hundred_is_skipped_and_draws_no_rotation() {
    let cat = cat(&ALL);
    // A seed whose first draw is 0 mod 100.
    let seed = (1..)
        .find(|&s| {
            let mut r = Rng::default();
            r.seed(s);
            r.pick(100) == 0
        })
        .unwrap();
    let mut w = world(("objects", "car"), (64, 64));
    let mut rng = Rng::default();
    rng.seed(seed);
    let mut mirror = rng.clone();
    let pieces = run(&mut w, &cat, &mut rng);
    mirror.pick(100);
    let mut left = 0;
    for _ in 0..3 {
        if mirror.pick(100) != 0 {
            mirror.rand();
            left += 1;
        }
    }
    assert_eq!(pieces.len(), left);
    assert_eq!(rng, mirror);
}

#[test]
fn a_cell_outside_the_map_still_draws_but_leaves_no_piece() {
    let cat = cat(&ALL);
    // The car's first column of cells is at x = -16, outside the map.
    let mut w = world(("objects", "car"), (0, 64));
    let mut rng = Rng::default();
    let mut mirror = rng.clone();
    let pieces = run(&mut w, &cat, &mut rng);
    let mut inside = 0;
    for i in 0..2 {
        for _ in 0..2 {
            if mirror.pick(100) != 0 && i == 1 {
                mirror.rand();
                inside += 1;
            }
        }
    }
    assert_eq!(pieces.len(), inside);
    assert!(pieces.iter().all(|p| p.1 == 16));
    assert_eq!(rng, mirror);
}

#[test]
fn foliage_leaves_a_stump_and_a_missing_type_leaves_nothing_and_draws_nothing() {
    let mut w = world(("objects", "tree"), (96, 96));
    run(&mut w, &cat(&ALL), &mut Rng::default());
    assert!(
        w.zoo
            .entities
            .iter()
            .all(|e| e.class == "rubble" && e.subtype == "bstump1")
    );
    assert_eq!(w.zoo.entities.len(), 1);
    // The tree rubble is not in the catalogue: the tree is gone all the same.
    let mut w = world(("objects", "tree"), (96, 96));
    let mut rng = Rng::default();
    assert!(run(&mut w, &cat(&["brubls1"]), &mut rng).is_empty());
    assert!(w.zoo.entities.is_empty());
    assert_eq!(rng, Rng::default());
}

#[test]
fn a_big_building_leaves_one_piece_per_tile_a_restroom_its_own_and_a_hut_per_quadrant() {
    // 3 x 2 quadrants: cells every 2 quadrants, so 2 x 1 tiles, at their centres.
    let mut w = world(("building", "hall"), centre((4, 4)));
    let mut rng = Rng::default();
    let mut mirror = rng.clone();
    let pieces = run(&mut w, &cat(&ALL), &mut rng);
    let kinds = ["brubble1", "brubble2", "brubble3", "brubble4"];
    let want: Vec<_> = [288, 352]
        .map(|x| {
            (
                kinds[mirror.pick(4) as usize].to_string(),
                x,
                288,
                0,
                mirror.rand() & 7,
            )
        })
        .into();
    assert_eq!(pieces, want);
    assert_eq!(rng, mirror);
    // The restroom, whatever its footprint: its own type, no 1 in 100 skip.
    let mut w = world(("building", "wc"), centre((4, 4)));
    let mut rng = Rng::default();
    let mut mirror = rng.clone();
    let pieces = run(&mut w, &cat(&ALL), &mut rng);
    mirror.pick(1);
    assert_eq!(
        pieces,
        [("bathrubl".to_string(), 288, 288, 0, mirror.rand() & 7)]
    );
    // One quadrant wide: a piece per quadrant, the pick draw `% 1`, no skip.
    let mut w = world(("building", "hut"), centre((4, 4)));
    let mut rng = Rng::default();
    let mut mirror = rng.clone();
    let pieces = run(&mut w, &cat(&ALL), &mut rng);
    for _ in 0..2 {
        mirror.pick(1);
        mirror.rand();
    }
    assert_eq!(pieces.len(), 2);
    assert!(pieces.iter().all(|p| p.0 == "brubls1"));
    assert_eq!(rng, mirror);
}

#[test]
fn nothing_is_made_for_an_entity_that_is_not_there() {
    let cat = cat(&ALL);
    let mut w = world(("objects", "car"), (64, 64));
    let shape = |_: &Entity| None;
    let mut rng = Rng::default();
    let mut kit = Kit {
        cat: &cat,
        rng: &mut rng,
        template: "",
        shape: &shape,
    };
    for id in [0, 4, u32::MAX] {
        assert!(!turn_to_rubble(&mut w, &mut kit, id));
    }
    assert_eq!(w.zoo.entities.len(), 1);
    assert_eq!(rng, Rng::default());
}

#[test]
fn a_rubble_piece_stands_on_the_ground_at_its_point() {
    let cat = cat(&ALL);
    // A car at z 35 on tiles at elevation 2: flat, then rising along x.
    for (shape, slope) in [(0u8, false), (0x50, true)] {
        let mut w = world(("objects", "car"), (64, 64));
        for t in [0, 1, 10, 11] {
            w.zoo.tiles[t].elevation = 2;
            w.zoo.tiles[t].shape = shape;
        }
        let pieces = run(&mut w, &cat, &mut Rng::default());
        assert!(pieces.len() >= 3);
        for p in pieces {
            assert_eq!(p.3, 32 + if slope { p.1 % 64 / 4 } else { 0 });
        }
    }
}

#[test]
fn taking_off_one_piece_takes_the_whole_group_with_it() {
    let cat = cat(&ALL);
    let mut w = world(("objects", "car"), (64, 64));
    // A bystander that a saved group wrongly names stays.
    let mut other = entity("objects", "tree", 9, (640, 640), 0);
    other.common.as_mut().unwrap().name = "Bystander".into();
    let other = w.add(other, None).unwrap();
    run(&mut w, &cat, &mut Rng::default());
    let ids: Vec<u32> = w
        .zoo
        .entities
        .iter()
        .filter(|e| e.class == "rubble")
        .map(|e| e.common.as_ref().unwrap().id)
        .collect();
    assert!(ids.len() >= 3);
    for e in w.zoo.entities.iter_mut().filter(|e| e.class == "rubble") {
        e.common
            .as_mut()
            .unwrap()
            .rubble
            .as_mut()
            .unwrap()
            .group
            .extend([other, 999]);
    }
    // The last piece named, then a missing id, go the same way.
    assert!(w.remove(*ids.last().unwrap()));
    assert!(w.zoo.entities.iter().all(|e| e.class != "rubble"));
    assert_eq!(w.zoo.entities.len(), 1);
    assert!(!w.remove(ids[0]));
}

#[test]
fn a_piece_alone_in_its_group_and_a_plain_entity_go_alone() {
    let cat = cat(&ALL);
    let mut w = world(("objects", "car"), (64, 64));
    run(&mut w, &cat, &mut Rng::default());
    let first = w.zoo.entities[0].common.as_ref().unwrap().id;
    w.zoo.entities[0].common.as_mut().unwrap().rubble = Some(Rubble::default());
    let count = w.zoo.entities.len();
    assert!(w.remove(first));
    assert_eq!(w.zoo.entities.len(), count - 1);
    let rest = w.zoo.entities[0].common.as_ref().unwrap().id;
    w.zoo.entities[0].common.as_mut().unwrap().rubble = None;
    assert!(w.remove(rest));
    assert_eq!(w.zoo.entities.len(), count - 2);
}

#[test]
fn an_undo_brings_back_only_the_piece_removed_without_its_group() {
    let cat = cat(&ALL);
    let mut w = world(("objects", "car"), (64, 64));
    run(&mut w, &cat, &mut Rng::default());
    assert!(w.zoo.entities.len() > 1);
    let id = w.zoo.entities[1].common.as_ref().unwrap().id;
    w.begin();
    w.remove(id);
    w.priced(3.0);
    assert!(w.zoo.entities.is_empty());
    let undone = w.undo();
    assert_eq!(w.zoo.entities.len(), 1);
    let back = w.zoo.entities[0].common.as_ref().unwrap();
    assert_eq!(back.id, id);
    assert!(back.rubble.as_ref().unwrap().group.is_empty());
    let prices: Vec<f32> = undone
        .steps
        .iter()
        .filter_map(|s| match s {
            crate::world::Step::Back(_, p) => Some(*p),
            _ => None,
        })
        .collect();
    assert_eq!(prices, [3.0]);
}

/// The ids of the pieces whose first cell of their column made a piece, with their sound.
fn explosions(keys: &str, seed: Option<u32>) -> (Vec<u32>, Vec<(u32, crate::behaviour::Heard)>) {
    let mut cat = cat(&[]);
    cat.push(ty("rubble", "other", "brubls1", 0, [1, 1], keys));
    let mut w = world(("objects", "car"), (64, 64));
    let mut rng = Rng::default();
    seed.into_iter().for_each(|s| rng.seed(s));
    run(&mut w, &cat, &mut rng);
    let firsts = (w.zoo.entities.iter())
        .filter_map(|e| e.common.as_ref().filter(|c| c.y == 48))
        .map(|c| c.id)
        .collect();
    (firsts, w.units.take_sounds())
}

#[test]
fn the_first_piece_of_each_column_explodes_with_the_sound_of_its_type() {
    let keys = "cExplosionSound=sounds/rubble4.wav\ncExplosionSoundAtten=1100";
    let (firsts, sounds) = explosions(keys, None);
    assert_eq!(firsts.len(), 2);
    assert_eq!(sounds.iter().map(|s| s.0).collect::<Vec<_>>(), firsts);
    for (id, heard) in &sounds {
        assert_eq!(
            (heard.file.as_str(), heard.base),
            ("sounds/rubble4.wav", 1100)
        );
        assert_eq!(heard.voice, format!("rubble/other/{id}/cExplosionSound"));
    }
    // No attenuation key: 0.
    let (_, sounds) = explosions("cExplosionSound=sounds/rubble4.wav", None);
    assert!(sounds.iter().all(|s| s.1.base == 0));
    // No sound key, or an empty one: nothing, whatever the attenuation.
    assert!(explosions("cExplosionSoundAtten=1100", None).1.is_empty());
    assert!(explosions("cExplosionSound=", None).1.is_empty());
}

#[test]
fn a_column_whose_first_cell_was_skipped_plays_nothing() {
    let seed = (1..)
        .find(|&s| {
            let mut r = Rng::default();
            r.seed(s);
            r.pick(100) == 0
        })
        .unwrap();
    let (firsts, sounds) = explosions("cExplosionSound=sounds/rubble4.wav", Some(seed));
    assert!(firsts.len() < 2);
    assert_eq!(sounds.iter().map(|s| s.0).collect::<Vec<_>>(), firsts);
}
