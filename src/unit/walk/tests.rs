//! The random walk on synthetic maps: the destination by class, the fallbacks and the draws.

use super::*;
use crate::entity::{Catalogue, EntityType};
use crate::unit::Staff;
use crate::unit::testing::{centre, entity, world, world_with};
use crate::zoo::{Entity, HabitatInfo};

fn rng(seed: u32) -> Rng {
    let mut rng = Rng::default();
    rng.seed(seed);
    rng
}

/// Lets unit `id` take a random walk with a generator seeded with `seed`; returns the answer and
/// the generator.
fn walk(w: &mut World, id: u32, seed: u32) -> (bool, Rng) {
    walk_in(w, &Catalogue::default(), id, seed)
}

fn walk_in(w: &mut World, cat: &Catalogue, id: u32, seed: u32) -> (bool, Rng) {
    let mut rng = rng(seed);
    let at = w.find(id).unwrap();
    let mut unit = std::mem::replace(w.units.get_mut(id).unwrap(), Unit::new(id, Kind::Animal));
    let ok = random_walk(&mut unit, w, cat, &mut rng, at);
    *w.units.get_mut(id).unwrap() = unit;
    (ok, rng)
}

fn tile_of(w: &World, (x, y): (i32, i32)) -> usize {
    w.tile_index(x, y).unwrap()
}

fn goes_to(w: &World, id: u32) -> Option<usize> {
    w.units.get(id).unwrap().path.last().copied()
}

fn destination(w: &World, id: u32) -> Option<(i32, i32, i32)> {
    w.units.get(id).unwrap().destination
}

/// Fences round the block of tiles from `(x0, y0)` to `(x1, y1)`, whose sides on the map get the
/// pieces of `edge`.
fn fences(e: &mut Vec<Entity>, (x0, y0): (i32, i32), (x1, y1): (i32, i32)) {
    for x in x0..=x1 {
        for y in y0..=y1 {
            for (d, outside) in [(0, y == y0), (2, x == x1), (4, y == y1), (6, x == x0)] {
                if outside {
                    e.push(entity(
                        "fences",
                        "exhibit",
                        100 + e.len() as u32,
                        centre((x, y)),
                        d,
                    ));
                }
            }
        }
    }
}

fn paths(e: &mut Vec<Entity>, tiles: impl IntoIterator<Item = (i32, i32)>) {
    e.extend(
        tiles
            .into_iter()
            .map(|t| entity("paths", "", 0, centre(t), 0)),
    );
}

#[test]
fn a_guest_strolls_to_a_path_tile_on_the_ring_with_three_draws() {
    let mut w = world((40, 30), |_, e| {
        paths(e, (0..40).map(|x| (x, 20)));
        e.push(entity("guests", "", 1, centre((20, 20)), 2));
    });
    let (ok, after) = walk(&mut w, 1, 7);
    // The reference: rx, ry, then the pick among the two path tiles of the border, column by
    // column from the west.
    let mut r = rng(7);
    let rx = r.pick(5) as i32 + 2;
    r.pick(5);
    let to = (20 + [-rx, rx][r.pick(2) as usize], 20);
    assert!(ok);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, to)));
    assert_eq!(w.units.get(1).unwrap().path[0], tile_of(&w, (20, 20)));
    assert_eq!(destination(&w, 1), Some((to.0 * 64 + 32, 20 * 64 + 32, 0)));
}

#[test]
fn no_path_tile_on_the_ring_sends_the_unit_to_its_far_corner() {
    // Issue 304: the last tile looked at, whether it is a path tile or not.
    let mut w = world((40, 30), |_, e| {
        paths(e, [(20, 20)]);
        e.push(entity("guests", "", 1, centre((20, 20)), 2));
    });
    let (ok, after) = walk(&mut w, 1, 7);
    let mut r = rng(7);
    let rx = r.pick(5) as i32 + 2;
    let ry = r.pick(5) as i32 + 2;
    assert!(ok);
    assert_eq!(after, r, "two draws");
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (20 + rx, 20 + ry))));
}

#[test]
fn an_animal_walks_to_a_dry_tile_of_its_exhibit_with_one_draw() {
    let mut w = world((30, 30), |_, e| {
        fences(e, (10, 10), (12, 12));
        e.push(entity("animals", "", 1, centre((11, 11)), 2));
    });
    let id = w.exhibits().region(tile_of(&w, (11, 11))).unwrap();
    let order = w.tiles_of(id);
    assert_eq!(order.len(), 9);
    // A seed whose pick is not the tile the animal stands on.
    let seed = (1..).find(|&s| order[rng(s).pick(9) as usize] != tile_of(&w, (11, 11)));
    let seed = seed.unwrap();
    let (ok, after) = walk(&mut w, 1, seed);
    let mut r = rng(seed);
    let to = order[r.pick(9) as usize];
    assert!(ok);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(to));
    // A unit that already has a path does nothing and draws nothing.
    let (ok, after) = walk(&mut w, 1, seed);
    assert!(!ok);
    assert_eq!(after, rng(seed));
}

#[test]
fn an_animal_in_the_water_mode_walks_to_a_water_tile_and_without_one_goes_back_to_land() {
    let mut w = world_with((30, 30), |z| {
        fences(&mut z.entities, (10, 10), (12, 12));
        z.entities
            .push(entity("animals", "", 1, centre((11, 11)), 2));
        for t in [(12, 12), (10, 12)] {
            z.tiles[t.1 as usize * 30 + t.0 as usize].terrain = 9;
        }
    });
    // Terrain type 9 is water, and the animal swims.
    w.set_water((0..10).map(|t| t == 9).collect());
    w.units.get_mut(1).unwrap().mover.swims = true;
    let terrain = |w: &World| match &w.units.get(1).unwrap().part {
        Part::Animal(a) => a.terrain,
        _ => 9,
    };
    let set = |w: &mut World, mode: u8| {
        if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
            a.terrain = mode;
        }
    };
    // The water mode: one draw among the two water tiles of the exhibit.
    set(&mut w, 1);
    let (ok, after) = walk(&mut w, 1, 3);
    let wet = [tile_of(&w, (12, 12)), tile_of(&w, (10, 12))];
    let order: Vec<usize> = w
        .tiles_of(w.exhibits().region(tile_of(&w, (11, 11))).unwrap())
        .into_iter()
        .filter(|t| wet.contains(t))
        .collect();
    assert_eq!(order.len(), 2);
    assert!(ok && goes_to(&w, 1) == Some(order[rng(3).pick(2) as usize]));
    assert_eq!(after, {
        let mut r = rng(3);
        r.pick(2);
        r
    });
    assert_eq!(terrain(&w), 1);
    // The land mode never picks water.
    set(&mut w, 0);
    w.units.get_mut(1).unwrap().path.clear();
    for seed in 1..20 {
        walk(&mut w, 1, seed);
        assert!(!wet.contains(&goes_to(&w, 1).unwrap()), "seed {seed}");
        w.units.get_mut(1).unwrap().path.clear();
    }
    // No water tile in the exhibit: the animal goes to the land mode.
    for (x, y) in [(12, 12), (10, 12)] {
        w.set_tile(x, y, 0, 0, 0);
    }
    set(&mut w, 1);
    w.units.get_mut(1).unwrap().path.clear();
    walk(&mut w, 1, 3);
    assert_eq!(terrain(&w), 0);
}

#[test]
fn an_animal_on_the_grounds_picks_nothing_and_goes_to_the_ring_again() {
    let mut w = world((40, 30), |_, e| {
        e.push(entity("animals", "", 1, centre((20, 15)), 2));
    });
    let (ok, after) = walk(&mut w, 1, 5);
    let mut r = rng(5);
    let rx = r.pick(5) as i32 + 2;
    let ry = r.pick(5) as i32 + 2;
    let mut border = Vec::new();
    for ox in -rx..=rx {
        for oy in -ry..=ry {
            if ox.abs() == rx || oy.abs() == ry {
                border.push((20 + ox, 15 + oy));
            }
        }
    }
    let to = border[r.pick(border.len() as u32) as usize];
    assert!(ok);
    assert_eq!(after, r, "no draw for the pick, then rx, ry and the tile");
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, to)));
}

#[test]
fn a_unit_with_no_tile_on_the_ring_tries_its_neighbours_from_a_random_one_on() {
    // A 3 by 3 map: the ring is off the map.
    let mut w = world((3, 3), |_, e| {
        e.push(entity("animals", "", 1, centre((1, 1)), 2));
    });
    let (ok, after) = walk(&mut w, 1, 9);
    let mut r = rng(9);
    r.pick(5);
    r.pick(5);
    let start = r.pick(4);
    assert!(ok);
    assert_eq!(after, r, "rx, ry and the start");
    let (dx, dy) = [(0, -1), (1, 0), (0, 1), (-1, 0)][start as usize];
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (1 + dx, 1 + dy))));
    assert_eq!(w.units.get(1).unwrap().path.len(), 2);
}

#[test]
fn a_keeper_in_an_exhibit_walks_to_a_tile_beside_its_gate() {
    let mut w = world_with((30, 30), |zoo| {
        let e = &mut zoo.entities;
        fences(e, (10, 10), (12, 12));
        // The gate is the piece on the north side of (11, 10); the record names it.
        let gate = e.iter_mut().find(|f| {
            let c = f.common.as_ref().unwrap();
            (c.x / 64, c.y / 64, c.rotation) == (11, 10, 0)
        });
        gate.unwrap().subtype = "g".into();
        e.push(entity("keeper", "", 1, centre((11, 11)), 2));
        zoo.habitat_tiles = vec![(10, 10)];
        zoo.habitat_gates = vec![((11, 10), 0)];
        zoo.habitat_info = vec![HabitatInfo {
            name: String::new(),
            stats: [0; 9],
            created: 0,
            viewing: 0,
            serviced: 0,
        }];
    });
    assert!(
        w.gate_slot(w.exhibits().region(tile_of(&w, (11, 11))).unwrap())
            .is_some()
    );
    // The tiles round the tile outside the gate that are on the grounds, column by column.
    let around = [(10, 8), (10, 9), (11, 8), (12, 8), (12, 9)];
    let seed = (1..)
        .find(|&s| around[rng(s).pick(5) as usize] != (11, 9))
        .unwrap();
    let (ok, after) = walk(&mut w, 1, seed);
    let mut r = rng(seed);
    let to = around[r.pick(5) as usize];
    assert!(ok);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, to)));
}

#[test]
fn the_ring_settings_come_from_the_ini_file_with_their_limits() {
    let ring = |text: &str| Ring::from_ini(&Config::parse(text.as_bytes()).unwrap());
    assert_eq!(ring(""), Ring::default());
    let r = ring("[AI]\nBaseX=3\nBaseY=0\nRandomX=7\nRandomY=1\n");
    assert_eq!((r.base, r.random), ((3, 0), (7, 1)));
    let r = ring("[AI]\nBaseX=-4\nBaseY=-1\nRandomX=0\nRandomY=-9\n");
    assert_eq!((r.base, r.random), ((0, 0), (1, 1)));
    let r = ring("[AI]\nbasex=99999999\nRANDOMX=2000000000\nRandomY=zz\n");
    assert_eq!((r.base.0, r.random), (1 << 20, (1 << 20, 5)));
    // A huge ring does not hang the walk: the border is cut to the map.
    let mut w = world((40, 30), |_, e| {
        e.push(entity("guests", "", 1, centre((20, 15)), 2))
    });
    w.units
        .set_ini(b"[AI]\nBaseX=99999999\nRandomX=2000000000\nBaseY=0\nRandomY=1\n");
    assert!(walk(&mut w, 1, 3).0);
    w.units.set_ini(b"");
    assert_eq!(w.units.ring, Ring::default());
}

#[test]
fn a_random_walk_charges_the_turn_off_the_facing_of_the_unit() {
    // A keeper on open ground facing east (2): straight on 1, a quarter turn 2, a turn back 4,
    // each plus the 2 of open ground; outside a random walk every step is 1 plus 2.
    let w = world((12, 12), |_, e| {
        e.push(entity("keeper", "", 1, centre((5, 5)), 2))
    });
    let (at, here) = (w.find(1).unwrap(), 5 * 12 + 5);
    let unit = w.units.get(1).unwrap();
    let cost =
        |m: &Mover, d: usize| path::step(&w, m, here, w.neighbour(here, d).unwrap(), d, None);
    let walking = walker(unit, &w, at, here, 2);
    assert_eq!([2, 0, 6, 4].map(|d| cost(&walking, d)), [3, 4, 6, 4]);
    let plain = movement::mover(unit, &w, at, here);
    assert_eq!([2, 0, 6, 4].map(|d| cost(&plain, d)), [3, 3, 3, 3]);
    // A facing out of the 0 to 7 range wraps (here to 7, a diagonal one).
    let wrapped = walker(unit, &w, at, here, u32::MAX);
    assert_eq!([0, 2, 4, 6].map(|d| cost(&wrapped, d)), [3, 6, 6, 3]);
}

#[test]
fn a_terrain_mode_with_no_tile_changes_and_sends_the_animal_to_a_tile_of_its_new_kind() {
    use crate::unit::testing::amphibious_world;
    let terrain = |w: &World, id: u32| match &w.units.get(id).unwrap().part {
        Part::Animal(a) => a.terrain,
        _ => 9,
    };
    let set = |w: &mut World, id: u32, mode: u8, swims: bool, underwater: bool| {
        let u = w.units.get_mut(id).unwrap();
        (u.mover.swims, u.mover.underwater, u.path) = (swims, underwater, Vec::new());
        if let Part::Animal(a) = &mut u.part {
            a.terrain = mode;
        }
    };
    let sent = |w: &mut World, id: u32| w.units.get_mut(id).unwrap().sent.take();
    // The tank from (2, 2) and the land exhibit from (6, 2) beside it, linked; no water.
    let mut w = amphibious_world(2, &[(1, "fish", (3, 3)), (2, "fish", (7, 3))]);
    // In the water mode in the tank: no water tile, so the land mode; a send is made, which the
    // fallbacks of the walk replace (the next test).
    set(&mut w, 1, 1, true, true);
    walk(&mut w, 1, 7);
    assert_eq!(terrain(&w, 1), 0);
    // Already in the land mode: nothing changes, nothing is sent.
    set(&mut w, 1, 0, false, false);
    walk(&mut w, 1, 7);
    assert_eq!((terrain(&w, 1), sent(&mut w, 1)), (0, None));
    // Not linked (the land lower than the wall tops): the tank mode on land finds no tank tile;
    // the land mode, and the animal stands on land already: not sent.
    let mut w = amphibious_world(0, &[(1, "fish", (3, 3)), (2, "fish", (7, 3))]);
    set(&mut w, 2, 2, true, true);
    walk(&mut w, 2, 7);
    assert_eq!((terrain(&w, 2), sent(&mut w, 2)), (0, None));
    // A species that lives only under water stays in the tank mode.
    set(&mut w, 2, 2, true, true);
    w.units.get_mut(2).unwrap().mover.only_underwater = true;
    walk(&mut w, 2, 7);
    assert_eq!((terrain(&w, 2), sent(&mut w, 2)), (2, None));
    // The land mode in the tank, which has no land tile: a diver takes the tank mode, not sent;
    // a unit that neither swims nor dives keeps the land mode.
    set(&mut w, 1, 0, false, true);
    walk(&mut w, 1, 7);
    assert_eq!((terrain(&w, 1), sent(&mut w, 1)), (2, None));
    set(&mut w, 1, 0, false, false);
    walk(&mut w, 1, 7);
    assert_eq!((terrain(&w, 1), sent(&mut w, 1)), (0, None));
    // An exhibit of water only: a swimmer in the land mode on it takes the water mode.
    let mut w = world_with((30, 30), |z| {
        fences(&mut z.entities, (10, 10), (12, 12));
        z.entities
            .push(entity("animals", "", 1, centre((11, 11)), 2));
        (10..=12).for_each(|y| (10..=12).for_each(|x| z.tiles[y * 30 + x].terrain = 9));
    });
    w.set_water((0..10).map(|t| t == 9).collect());
    set(&mut w, 1, 0, true, false);
    walk(&mut w, 1, 7);
    assert_eq!((terrain(&w, 1), sent(&mut w, 1)), (1, None));
}

#[test]
fn a_send_by_the_random_walk_gives_way_to_its_fallbacks() {
    use crate::unit::testing::amphibious_world;
    // The tank and the land exhibit beside it, linked; no water.
    let mut w = amphibious_world(2, &[(1, "fish", (3, 3)), (2, "fish", (7, 3))]);
    let here = tile_of(&w, (3, 3));
    let set = |w: &mut World| {
        let u = w.units.get_mut(1).unwrap();
        (u.mover.swims, u.mover.underwater, u.path) = (true, true, Vec::new());
        if let Part::Animal(a) = &mut u.part {
            a.terrain = 1;
        }
    };
    // The water mode with no water tile: the pick draws a land tile to send the animal to; the
    // first successful search of the fallbacks sets the path instead, and nothing is sent.
    set(&mut w);
    let (ok, _) = walk(&mut w, 1, 7);
    let u = w.units.get(1).unwrap();
    assert!(ok && u.sent.is_none());
    // In the water mode the lenient tile test refuses the tank's tiles and the land beside it:
    // the walk ends on the last fallback, from here to here.
    assert_eq!(u.path, vec![here]);
}

fn kind_of(class: &'static str, name_id: i32) -> EntityType {
    EntityType {
        class,
        type_name: Vec::new(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(b"[Characteristics/Integers]\ncCleanTankThreshold=100\n")
                .unwrap(),
        )),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn staff_catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    for t in [kind_of("keeper", 9500), kind_of("animals", 5)] {
        cat.push(t);
    }
    cat
}

/// Two pens, A round (10..12, 10..12) and B round (20..22, 10..12), with a gate on the north side
/// of the middle tile each (outside tiles (11, 9) and (21, 9)); a zebra in each pen of `zebras`,
/// and the unit `class` with id 1 standing at `at`.
fn pens(zebras: &[char], class: &str, at: (i32, i32)) -> World {
    let mut w = world_with((40, 30), |zoo| {
        for (n, x) in [10, 20].into_iter().enumerate() {
            fences(&mut zoo.entities, (x, 10), (x + 2, 12));
            let gate = zoo.entities.iter_mut().find(|f| {
                let c = f.common.as_ref().unwrap();
                (c.x / 64, c.y / 64, c.rotation) == (x + 1, 10, 0) && f.class == "fences"
            });
            gate.unwrap().subtype = "g".into();
            zoo.habitat_tiles.push((x, 10));
            zoo.habitat_gates.push(((x + 1, 10), 0));
            zoo.habitat_info.push(HabitatInfo {
                name: format!("Pen {n}"),
                stats: [0; 9],
                created: 0,
                viewing: 0,
                serviced: 0,
            });
        }
        for (n, &z) in zebras.iter().enumerate() {
            let x = if z == 'a' { 11 } else { 21 };
            zoo.entities
                .push(entity("animals", "", 10 + n as u32, centre((x, 11)), 0));
        }
        if class == "tour" {
            // A guide keeps to the paths.
            paths(
                &mut zoo.entities,
                (9..=20).map(|y| (5, y)).chain((6..=22).map(|x| (x, 9))),
            );
        }
        zoo.entities.push(entity(class, "", 1, centre(at), 2));
    });
    for n in 0..zebras.len() as u32 {
        w.units.get_mut(10 + n).unwrap().part = Part::Animal(crate::animal::Animal::default());
    }
    w
}

fn gates(w: &World) -> Vec<usize> {
    let ids: Vec<u32> = w.exhibits().list().iter().map(|x| x.id).collect();
    ids.iter()
        .map(|&x| crate::staff::service::gate_tiles(w, x).unwrap().1)
        .collect()
}

/// The pens of `pens`: A holds the animal of the keeper, B holds none.
#[test]
fn a_keeper_without_assignments_strolls_to_the_gate_of_an_exhibit_he_may_serve() {
    let cat = staff_catalogue();
    let mut w = pens(&['a'], "keeper", (5, 20));
    let (ok, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    r.pick(1);
    assert!(ok);
    assert_eq!(after, r, "one draw");
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (11, 9))));
    // Both hold an animal: one draw among the two, in the order of the exhibit list.
    let mut w = pens(&['a', 'b'], "keeper", (5, 20));
    let list = gates(&w);
    for seed in 1..6 {
        let mut w = pens(&['a', 'b'], "keeper", (5, 20));
        let (_, after) = walk_in(&mut w, &cat, 1, seed);
        let mut r = rng(seed);
        let to = list[r.pick(2) as usize];
        assert_eq!(after, r, "{seed}");
        assert_eq!(goes_to(&w, 1), Some(to), "{seed}");
    }
    // His own list, when it has an exhibit, comes first: B has no animal and is taken.
    let b = w.exhibits().region(tile_of(&w, (21, 11))).unwrap();
    w.units.get_mut(1).unwrap().part = Part::Staff(Staff {
        assigned: vec![b],
        ..Staff::default()
    });
    walk_in(&mut w, &cat, 1, 3);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (21, 9))));
}

#[test]
fn a_keeper_with_no_exhibit_to_serve_or_on_the_gate_takes_the_ring() {
    let cat = staff_catalogue();
    // No animal: no candidate, no draw for the pick; the ring finds no path tile and sends him
    // to the corner of its rectangle (two draws).
    let mut w = pens(&[], "keeper", (5, 20));
    let (_, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    let (rx, ry) = (r.pick(5) as i32 + 2, r.pick(5) as i32 + 2);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (5 + rx, 20 + ry))));
    // On the gate tile of the only candidate: the draw, then the ring.
    let mut w = pens(&['a'], "keeper", (11, 9));
    let (_, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    r.pick(1);
    let (rx, ry) = (r.pick(5) as i32 + 2, r.pick(5) as i32 + 2);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (11 + rx, 9 + ry))));
}

#[test]
fn a_guide_strolls_to_the_gate_of_an_assigned_exhibit_else_of_one_holding_an_animal() {
    let cat = staff_catalogue();
    let mut w = pens(&['a'], "tour", (5, 20));
    let (ok, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    r.pick(1);
    assert!(ok);
    assert_eq!(after, r, "one draw");
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (11, 9))));
    // Assigned: only that exhibit, animal or not.
    let b = w.exhibits().region(tile_of(&w, (21, 11))).unwrap();
    w.units.get_mut(1).unwrap().part = Part::Staff(Staff {
        assigned: vec![b],
        ..Staff::default()
    });
    w.units.get_mut(1).unwrap().path.clear();
    walk_in(&mut w, &cat, 1, 3);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (21, 9))));
    // Without an animal and an assignment: the ring (the corner, two draws).
    let mut w = pens(&[], "tour", (30, 20));
    let (_, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    let (rx, ry) = (r.pick(5) as i32 + 2, r.pick(5) as i32 + 2);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (30 + rx, 20 + ry))));
}

#[test]
fn a_worker_strolls_to_the_gate_of_an_assigned_exhibit_else_the_ring() {
    let cat = staff_catalogue();
    // No list, whatever the animals: no pick, the ring (the corner, two draws).
    let mut w = pens(&['a'], "maint", (5, 20));
    let (_, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    let (rx, ry) = (r.pick(5) as i32 + 2, r.pick(5) as i32 + 2);
    assert_eq!(after, r);
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (5 + rx, 20 + ry))));
    // Assigned to B (no animal): one draw, its gate.
    let b = w.exhibits().region(tile_of(&w, (21, 11))).unwrap();
    w.units.get_mut(1).unwrap().part = Part::Staff(Staff {
        assigned: vec![b],
        ..Staff::default()
    });
    w.units.get_mut(1).unwrap().path.clear();
    let (ok, after) = walk_in(&mut w, &cat, 1, 3);
    let mut r = rng(3);
    r.pick(1);
    assert!(ok);
    assert_eq!(after, r, "one draw");
    assert_eq!(goes_to(&w, 1), Some(tile_of(&w, (21, 9))));
}

/// A pen from (2, 2) to (4, 4) whose fence pieces (strength and life 5) have the type keys `keys`
/// makes, with animal 1 at `at`.
fn pen(at: (i32, i32), keys: impl Fn(&mut crate::world::TankKind)) -> World {
    let w = world((12, 12), |_, e| {
        fences(e, (2, 2), (4, 4));
        e.push(entity("animals", "", 1, centre(at), 2));
    });
    let shape = |e: &Entity| {
        let mut s = crate::unit::testing::shape(e)?;
        if s.fence {
            keys(&mut s.tank);
        }
        Some(s)
    };
    World::new(w.zoo, shape)
}

/// Whether an animal at (4, 3) with its escape flag up and the values `m` gives crosses the east
/// fence of the pen whose pieces `keys` makes, whether it crosses it back from the grounds, and
/// the crossing it chooses (animals 11.3).
fn cross(keys: impl Fn(&mut crate::world::TankKind), m: impl Fn(&mut Mover)) -> (bool, bool, u32) {
    let w = pen((3, 3), keys);
    let (a, b) = (tile_of(&w, (4, 3)), tile_of(&w, (5, 3)));
    let mut mover = Mover {
        test: true,
        ..Mover::new(path::Kind::Animal, Some(a))
    };
    m(&mut mover);
    let out = path::step(&w, &mover, a, b, 2, None) < MAX;
    mover.at = Some(b);
    let back = path::step(&w, &mover, b, a, 6, None) < MAX;
    let kind = crate::animal::escape::kind(&w, &mover, false, (w.edge(a, 2), w.edge(b, 6)));
    (out, back, kind)
}

#[test]
fn an_escaping_animal_gets_over_the_fence_it_jumps_climbs_bashes_or_crushes() {
    let plain = |_: &mut crate::world::TankKind| {};
    let jumpable = |k: &mut crate::world::TankKind| k.jumpable = true;
    assert_eq!(cross(plain, |_| {}), (false, false, 0));
    assert_eq!(cross(jumpable, |m| m.jumper = true), (true, false, 1));
    // The flag down, or a species that does not jump: the fence holds.
    let down = |m: &mut Mover| (m.jumper, m.test) = (true, false);
    assert_eq!(cross(jumpable, down), (false, false, 1));
    assert_eq!(cross(jumpable, |_| {}), (false, false, 0));
    let climbable = |k: &mut crate::world::TankKind| k.climbable = true;
    assert_eq!(cross(climbable, |m| m.climber = true), (true, false, 2));
    // A powered electric fence holds; at its decayed life it does not.
    let live = |k: &mut crate::world::TankKind| (k.jumpable, k.electrified) = (true, true);
    assert_eq!(cross(live, |m| m.jumper = true), (false, false, 1));
    let decayed =
        |k: &mut crate::world::TankKind| (k.jumpable, k.electrified, k.decayed) = (true, true, 5);
    assert_eq!(cross(decayed, |m| m.jumper = true), (true, false, 1));
    // The bash beats the strength 5; the crush, when not 0, the height 0 of the type.
    for (bash, open) in [
        (6, true),
        (i32::MAX, true),
        (5, false),
        (0, false),
        (-1, false),
    ] {
        let kind = if open { 3 } else { 0 };
        assert_eq!(
            cross(plain, |m| m.bash = bash),
            (open, false, kind),
            "{bash}"
        );
    }
    for (crush, open) in [(1, true), (i32::MAX, true), (0, false), (-1, false)] {
        let kind = if open { 3 } else { 0 };
        assert_eq!(
            cross(plain, |m| m.crush = crush),
            (open, false, kind),
            "{crush}"
        );
    }
}

/// The escape walk of animal 1 in `w`.
fn walk_of(w: &World) -> Option<crate::zoo::Escape> {
    match &w.units.get(1).unwrap().part {
        Part::Animal(a) => a.walk,
        _ => None,
    }
}

#[test]
fn an_animal_whose_exhibit_lets_it_out_walks_one_tile_past_the_fence_with_no_draw() {
    let escaping = |at: (i32, i32), keys: fn(&mut crate::world::TankKind), m: fn(&mut Mover)| {
        let mut w = pen(at, keys);
        let x = w.exhibits().region(tile_of(&w, (3, 3))).unwrap();
        let target = tile_of(&w, (8, 3));
        w.exhibits_mut().get_mut(x).unwrap().rating.escapes = vec![("/".into(), target)];
        m(&mut w.units.get_mut(1).unwrap().mover);
        let (ok, after) = walk(&mut w, 1, 1);
        assert!(ok);
        (w, after)
    };
    let (w, after) = escaping((3, 3), |_| {}, |m| m.bash = 6);
    assert_eq!(after, rng(1), "no draw");
    let tiles = [(3, 3), (4, 3), (5, 3), (6, 3)].map(|p| tile_of(&w, p));
    assert_eq!(w.units.get(1).unwrap().path, tiles);
    assert_eq!(
        destination(&w, 1),
        Some(centre((6, 3))).map(|(x, y)| (x, y, 0))
    );
    let walk = crate::zoo::Escape {
        a: [4, 3],
        b: [6, 3],
        kind: 3,
        ..Default::default()
    };
    assert_eq!(walk_of(&w), Some(walk));
    // On the crossing tile already, a jumper starts its jump.
    let (w, _) = escaping((4, 3), |k| k.jumpable = true, |m| m.jumper = true);
    assert_eq!(walk_of(&w).map(|e| (e.kind, e.reached)), Some((1, 0)));
    let force = match &w.units.get(1).unwrap().part {
        Part::Animal(a) => a.force,
        _ => None,
    };
    assert_eq!(force, Some("jump_high"));
    // An animal the fence holds, or one whose type the table lacks, picks a tile of its pen.
    let (w, after) = escaping((3, 3), |_| {}, |_| {});
    assert_eq!(walk_of(&w), None);
    assert_ne!(after, rng(1));
    let x = w.exhibits().region(tile_of(&w, (3, 3)));
    assert_eq!(w.exhibits().region(goes_to(&w, 1).unwrap()), x);
}

#[test]
fn a_bash_breaks_the_fence_once_the_animal_leaves_the_crossing_tile_and_the_walk_ends_on_its_last_tile()
 {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "fences",
        type_name: b"exhibit".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(b"[Characteristics/Integers]\ncStrength=5\n").unwrap(),
        )),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    });
    let steps = |kind: u32, bash: i32| {
        let mut w = pen((3, 3), |_| {});
        let mut unit = w.units.get(1).unwrap().clone();
        unit.mover.bash = bash;
        if let Part::Animal(a) = &mut unit.part {
            a.walk = Some(crate::zoo::Escape {
                a: [4, 3],
                b: [6, 3],
                kind,
                ..Default::default()
            });
        }
        let piece = w.edge(tile_of(&w, (4, 3)), 2).unwrap();
        let mut seen = Vec::new();
        for p in [(4, 3), (5, 3), (6, 3)] {
            let t = tile_of(&w, p);
            crate::animal::escape::entered(&mut unit, &mut w, &cat, t);
            let life = w.zoo.entities[piece]
                .common
                .as_ref()
                .unwrap()
                .fence
                .unwrap()
                .life;
            let Part::Animal(a) = &mut unit.part else {
                unreachable!()
            };
            let walk = a.walk.unwrap();
            seen.push((a.force.take(), walk.reached, walk.broken, life));
        }
        // The walk ends on its last tile only.
        let ends =
            |unit: &mut Unit, p| crate::animal::escape::walk_done(unit, &w, Some(tile_of(&w, p)));
        assert!(ends(&mut unit, (5, 3)) && ends(&mut unit, (6, 3)));
        assert!(!ends(&mut unit, (6, 3)));
        (seen, w.units.posts.len())
    };
    let (seen, posts) = steps(3, 6);
    assert_eq!(
        seen,
        [(None, 1, 0, 5), (None, 1, 1, 0), (None, 1, 1, 0)],
        "broken on the tile across, once"
    );
    // Broken, and at its decayed life 0 too.
    assert_eq!(posts, 2, "the broken piece's events");
    // A jumper that does not bash leaves the fence whole and runs on.
    let (seen, posts) = steps(1, 0);
    let jump = [
        (Some("jump_high"), 1, 0, 5),
        (None, 1, 0, 5),
        (Some("run"), 1, 0, 5),
    ];
    assert_eq!((seen, posts), (jump.to_vec(), 0));
    // One that bashes breaks it on the way too; a climber climbs.
    assert_eq!(steps(1, 1).0[1], (None, 1, 1, 0));
    assert_eq!(steps(2, 0).0[0].0, Some("climb_up"));
}
