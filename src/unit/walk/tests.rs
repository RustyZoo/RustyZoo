//! The random walk on synthetic maps: the destination by class, the fallbacks and the draws.

use super::*;
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
    let mut rng = rng(seed);
    let at = w.find(id).unwrap();
    let mut unit = std::mem::replace(w.units.get_mut(id).unwrap(), Unit::new(id, Kind::Animal));
    let ok = random_walk(&mut unit, w, &mut rng, at);
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
