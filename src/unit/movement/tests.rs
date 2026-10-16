//! The move on synthetic maps: the worked examples of the spec.

use super::*;
use crate::sim::Sim;
use crate::unit::testing::{centre, entity, world, world_with};
use crate::zoo::Clocks;

/// A 40 by 30 map with these entities. The units are quiet: their goals leave the paths and the
/// moves of the tests alone.
fn map(entities: Vec<crate::zoo::Entity>) -> World {
    let mut w = world((40, 30), |_, e| e.extend(entities));
    crate::goal::quiet(&mut w);
    w
}

fn sim() -> Sim {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(1200, Clocks::default());
    sim
}

fn tile(x: usize, y: usize) -> usize {
    y * 40 + x
}

fn position(w: &World, id: u32) -> (i32, i32) {
    let c = w.zoo.entities[w.find(id).unwrap()].common.as_ref().unwrap();
    (c.x, c.y)
}

/// Sends unit `id` along `path` (tiles) at `speed`, to the last tile.
fn send(w: &mut World, id: u32, path: &[(usize, usize)], speed: u32) {
    let end = path.last().unwrap();
    let unit = w.units.get_mut(id).unwrap();
    unit.destination = Some((64 * end.0 as i32 + 32, 64 * end.1 as i32 + 32, 0));
    let tiles = path.iter().map(|&(x, y)| tile(x, y)).collect();
    assert!(start(unit, tiles, speed, 100));
}

fn step_all(w: &mut World, s: &mut Sim, steps: &[u32]) {
    for &ms in steps {
        s.step(w, ms, |_, _, _| {});
    }
}

#[test]
fn the_distance_of_a_step_is_whole_with_a_minimum_of_one_and_a_cap() {
    // Speed and step: the table of the spec.
    for (speed, ms, moved) in [
        (20, 67, 1),
        (33, 67, 2),
        (44, 67, 2),
        (56, 67, 3),
        (76, 67, 5),
        (44, 83, 3),
        (0, 67, 0),
        (5, 0, 1),
        (1, 1, 1),
        (1000, 100, 16),
        (u32::MAX, 67, 16),
    ] {
        let mut w = map(vec![entity("animals", "", 1, centre((5, 5)), 0)]);
        send(&mut w, 1, &[(5, 5), (6, 5), (7, 5)], speed);
        let before = position(&w, 1);
        let (mut u, at) = (w.units.get(1).unwrap().clone(), 0);
        step(&mut u, &mut w, ms, at);
        let after = position(&w, 1);
        let want = if speed == 0 { 0 } else { moved };
        assert_eq!(after.0 - before.0, want, "speed {speed} step {ms}");
    }
}

#[test]
fn a_guest_takes_the_rate_of_its_offset_and_its_energy() {
    // The worked examples and the observed guests.
    assert_eq!(guest_speed(33, 0, 3, true), 25);
    assert_eq!(guest_speed(33, 6, 9, true), 30);
    assert_eq!(guest_speed(33, 10, 27, true), 38);
    assert_eq!(guest_speed(44, 0, 50, true), 39);
    assert_eq!(guest_speed(44, 10, 50, true), 49);
    assert_eq!(guest_speed(44, 10, 75, true), 53);
    assert_eq!(guest_speed(44, 0, 19, true), 35);
    assert_eq!(guest_speed(44, 0, 20, true), 39);
    assert_eq!(guest_speed(44, 10, 74, true), 49);
    assert_eq!(guest_speed(44, 5, 0, false), 44);
    assert_eq!(guest_speed(0, 0, 100, true), 0);
    assert_eq!(guest_speed(3, 0, -5, true), 0);
}

#[test]
fn a_guest_crosses_a_tile_and_stops_in_the_destination_tile() {
    // 13.2: speed 44 is 2 per step; (1000, 1000) is tile (15, 15), sub-tile (40, 40).
    let mut w = map(vec![entity("guests", "", 1, (1000, 1000), 2)]);
    let mut s = sim();
    send(&mut w, 1, &[(15, 15), (16, 15), (17, 15)], 44);
    w.units.get_mut(1).unwrap().travel = 2;
    w.units.take_lines();
    step_all(&mut w, &mut s, &[67; 12]);
    assert_eq!(position(&w, 1), (1024, 1000));
    let tiles = |w: &mut World| {
        let lines = w.units.take_lines();
        lines
            .into_iter()
            .filter(|l| l.starts_with("tile"))
            .collect::<Vec<_>>()
    };
    assert_eq!(tiles(&mut w), ["tile 1 16,15"]);
    assert_eq!(w.units.get(1).unwrap().path, [tile(16, 15), tile(17, 15)]);
    step_all(&mut w, &mut s, &[67; 31]);
    assert_eq!(position(&w, 1), (1086, 1000));
    assert!(w.units.get(1).unwrap().moving);
    // The step that enters the tile: the move call ends the walk in the same step.
    step_all(&mut w, &mut s, &[67]);
    assert_eq!(position(&w, 1), (1088, 1000));
    assert_eq!(tiles(&mut w), ["tile 1 17,15"]);
    let u = w.units.get(1).unwrap();
    assert!(!u.moving && u.speed == 0 && u.path.is_empty());
    // It stays where it stopped.
    step_all(&mut w, &mut s, &[67; 3]);
    assert_eq!(position(&w, 1), (1088, 1000));
}

#[test]
fn units_follow_the_facing_and_the_index_of_the_map() {
    let mut w = map(vec![entity("keeper", "", 1, (5 * 64 + 60, 5 * 64 + 32), 0)]);
    let mut s = sim();
    send(&mut w, 1, &[(5, 5), (6, 5), (7, 5)], 33);
    assert_eq!(w.units_on(tile(5, 5)).count(), 1);
    w.units.take_lines();
    step_all(&mut w, &mut s, &[67; 3]);
    // 60 + 2 + 2 is in the next tile: the index and the facing follow.
    assert_eq!(w.units_on(tile(5, 5)).count(), 0);
    assert_eq!(w.units_on(tile(6, 5)).count(), 1);
    assert_eq!(
        w.units
            .take_lines()
            .iter()
            .filter(|l| l.starts_with("tile"))
            .count(),
        1
    );
    assert_eq!(w.zoo.entities[0].common.as_ref().unwrap().rotation, 2);
    assert!(w.take_dirty());
    assert_eq!(w.units.get(1).unwrap().travel, 2);
}

#[test]
fn a_corner_slides_diagonally_until_the_margin() {
    // 13.3: a keeper at sub-tile (40, 10) of tile (5, 9), travelling along y, turns towards +x.
    assert_eq!(turning(0, 2, (40, 10), 20), 3);
    assert_eq!(turning(0, 2, (40, 20), 20), 2);
    assert_eq!(turning(0, 2, (40, 44), 20), 2);
    assert_eq!(turning(0, 2, (40, 45), 20), 1);
    assert_eq!(turning(4, 6, (40, 10), 20), 5);
    assert_eq!(turning(4, 6, (40, 50), 20), 7);
    assert_eq!(turning(2, 0, (10, 30), 20), 1);
    assert_eq!(turning(6, 4, (50, 30), 20), 5);
    // Straight on, back and diagonal wishes are not changed; the small margin is 5.
    assert_eq!(turning(0, 0, (3, 3), 20), 0);
    assert_eq!(turning(0, 4, (3, 3), 20), 4);
    assert_eq!(turning(0, 3, (3, 3), 20), 3);
    assert_eq!(turning(0, -1, (3, 3), 20), -1);
    assert_eq!(turning(0, 2, (40, 6), 5), 2);
    let mut w = map(vec![entity("keeper", "", 1, (5 * 64 + 40, 9 * 64 + 10), 0)]);
    let mut s = sim();
    send(&mut w, 1, &[(5, 9), (6, 9)], 33);
    w.units.get_mut(1).unwrap().travel = 0;
    w.units.take_lines();
    let mut facing = Vec::new();
    for _ in 0..8 {
        step_all(&mut w, &mut s, &[67]);
        facing.push(w.zoo.entities[0].common.as_ref().unwrap().rotation);
    }
    // 5 diagonal steps of 2 take y from 10 to 20, then straight along x.
    assert_eq!(facing, [3, 3, 3, 3, 3, 2, 2, 2]);
    assert_eq!(position(&w, 1), (5 * 64 + 40 + 16, 9 * 64 + 20));
    let faces: Vec<_> = (w.units.take_lines().into_iter())
        .filter(|l| l.starts_with("face"))
        .collect();
    assert_eq!(faces, ["face 1 3", "face 1 2"]);
}

#[test]
fn a_diagonal_step_is_as_long_on_both_axes() {
    assert_eq!(displacement(3, 2, true, 20), (2, 2));
    assert_eq!(displacement(7, 5, true, 20), (-5, -5));
    assert_eq!(displacement(2, 5, false, 20), (5, 0));
    assert_eq!(displacement(-1, 5, true, 20), (0, 0));
    // A unit that may not go diagonally, on a long cornering step, does not move (issue 302).
    assert_eq!(displacement(3, 23, false, 20), (23, 23));
    assert_eq!(displacement(3, 24, false, 20), (0, 0));
    assert_eq!(displacement(3, 53, false, 5), (53, 53));
    assert_eq!(displacement(3, 54, false, 5), (0, 0));
    assert_eq!(displacement(3, 24, true, 20), (24, 24));
}

#[test]
fn the_pace_of_a_slow_a_medium_and_a_fast_unit_over_ten_seconds() {
    // Speeds 20, 44 and 76 are 1, 2 and 5 per step of 67 ms; 10 s are 149 steps.
    let mut w = map(vec![
        entity("animals", "", 1, centre((2, 3)), 2),
        entity("animals", "", 2, centre((2, 6)), 2),
        entity("animals", "", 3, centre((2, 9)), 2),
    ]);
    let mut s = sim();
    let row = |y: usize| (2..30).map(|x| (x, y)).collect::<Vec<_>>();
    for (id, y, speed) in [(1, 3, 20), (2, 6, 44), (3, 9, 76)] {
        send(&mut w, id, &row(y), speed);
    }
    step_all(&mut w, &mut s, &[67; 149]);
    assert_eq!(position(&w, 1).0 - 160, 149);
    assert_eq!(position(&w, 2).0 - 160, 298);
    assert_eq!(position(&w, 3).0 - 160, 745);
    // Steps of 83 and 67 alternate: 3 and 2 for the speed 44.
    let alternate: Vec<u32> = (0..100).map(|i| [83, 67][i % 2]).collect();
    let mut w = map(vec![entity("animals", "", 4, centre((2, 12)), 2)]);
    send(&mut w, 4, &row(12), 44);
    step_all(&mut w, &mut sim(), &alternate);
    assert_eq!(position(&w, 4).0 - 160, 50 * (3 + 2));
}

#[test]
fn two_runs_of_a_walk_have_the_same_hash() {
    let run = || {
        let mut w = map(vec![
            entity("guests", "", 1, centre((2, 3)), 2),
            entity("keeper", "", 2, centre((2, 5)), 2),
        ]);
        let mut s = sim();
        send(&mut w, 1, &[(2, 3), (3, 3), (4, 3), (5, 3)], 38);
        send(&mut w, 2, &[(2, 5), (3, 6), (4, 6), (5, 7)], 40);
        step_all(
            &mut w,
            &mut s,
            &[67, 83, 67, 67, 83, 67, 67, 67, 67, 67, 67].repeat(4),
        );
        (w.units.hash(), w.units.take_lines())
    };
    let (a, b) = (run(), run());
    assert_eq!(a, b);
    assert!(
        a.1.iter().any(|l| l.starts_with("tile 1")) && a.1.iter().any(|l| l.starts_with("tile 2"))
    );
}

#[test]
fn a_path_of_one_tile_does_not_go_and_the_last_tile_is_not_left() {
    let mut w = map(vec![entity(
        "animals",
        "",
        1,
        (5 * 64 + 63, 5 * 64 + 32),
        2,
    )]);
    let u = w.units.get_mut(1).unwrap();
    assert!(!start(u, vec![tile(5, 5)], 40, 100));
    assert!(!u.moving && u.speed == 0);
    // With one entry and another destination tile it does not move this step.
    let mut s = sim();
    let u = w.units.get_mut(1).unwrap();
    (u.path, u.moving, u.speed, u.travel) = (vec![tile(5, 5)], true, 40, 2);
    u.destination = Some((6 * 64 + 32, 5 * 64 + 32, 0));
    step_all(&mut w, &mut s, &[67]);
    assert_eq!(position(&w, 1), (5 * 64 + 63, 5 * 64 + 32));
    assert!(w.units.get(1).unwrap().moving);
    // In its destination tile, its step across the edge would leave the last entry: the move ends.
    w.units.get_mut(1).unwrap().destination = Some((5 * 64 + 32, 5 * 64 + 32, 0));
    step_all(&mut w, &mut s, &[67]);
    assert_eq!(position(&w, 1), (5 * 64 + 63, 5 * 64 + 32));
    let u = w.units.get(1).unwrap();
    assert!(u.path.is_empty() && !u.moving);
}

#[test]
fn a_refused_tile_step_aborts_the_goal_and_the_unit_stays() {
    // An exhibit fence on the east side of the guest's tile.
    let mut w = map(vec![
        entity("guests", "", 1, centre((5, 5)), 2),
        entity("fences", "exhibit", 2, centre((5, 5)), 2),
    ]);
    let mut s = sim();
    let guest = w.find(1).unwrap();
    send(&mut w, 1, &[(5, 5), (6, 5), (7, 5)], 40);
    w.units.take_lines();
    step_all(&mut w, &mut s, &[67; 4]);
    assert!(w.units.get(1).unwrap().aborted);
    assert_eq!(position(&w, 1), centre((5, 5)));
    assert_eq!(guest, 0);
    // Told once.
    assert_eq!(
        w.units
            .take_lines()
            .iter()
            .filter(|l| l.starts_with("abort"))
            .count(),
        1
    );
    // A unit going through a building looks at the edge cost only, which the fence makes 20 too.
    let u = w.units.get_mut(1).unwrap();
    (u.aborted, u.through_building) = (false, true);
    step_all(&mut w, &mut s, &[67]);
    assert!(w.units.get(1).unwrap().aborted);
    assert!(!w.units.get(1).unwrap().through_building, "the move ended");
}

#[test]
fn a_walker_stands_on_the_ground_and_a_carried_unit_does_not_move() {
    let mut w = world((40, 30), |tiles, e| {
        tiles[tile(5, 5)].elevation = 2;
        tiles[tile(6, 5)].elevation = 2;
        tiles[tile(6, 5)].shape = 0x50;
        e.push(entity("animals", "", 1, (5 * 64 + 40, 5 * 64 + 32), 2));
        e.push(entity("animals", "", 2, (5 * 64 + 40, 8 * 64 + 32), 2));
    });
    crate::goal::quiet(&mut w);
    let mut s = sim();
    // The first step puts a unit that stands too low on the ground (16 per elevation step).
    step_all(&mut w, &mut s, &[67]);
    assert_eq!(w.zoo.entities[0].common.as_ref().unwrap().z, 32);
    send(&mut w, 1, &[(5, 5), (6, 5), (7, 5)], 40);
    step_all(&mut w, &mut s, &[67; 12]);
    // In tile (6, 5), shape 0x50 rises with x: 32 + (x mod 64) / 4 / ... the cut of map drawing 3.4.
    let c = w.zoo.entities[0].common.as_ref().unwrap();
    assert_eq!(c.x, 5 * 64 + 40 + 24);
    assert_eq!(c.x / 64, 6);
    assert_eq!(c.z, 32 + (c.x % 64) / 4);
    send(&mut w, 2, &[(5, 8), (6, 8), (7, 8)], 40);
    w.units.get_mut(2).unwrap().carried = true;
    step_all(&mut w, &mut s, &[67; 5]);
    assert_eq!(position(&w, 2), (5 * 64 + 40, 8 * 64 + 32));
}

#[test]
fn a_unit_off_its_path_goes_back_to_it_and_an_edge_of_the_map_ends_the_move() {
    // Off path[0] an animal heads for it diagonally.
    let mut w = map(vec![entity("animals", "", 1, centre((4, 4)), 2)]);
    let mut s = sim();
    send(&mut w, 1, &[(5, 5), (6, 5), (7, 5)], 33);
    step_all(&mut w, &mut s, &[67]);
    assert_eq!(
        position(&w, 1),
        (centre((4, 4)).0 + 2, centre((4, 4)).1 + 2)
    );
    // A destination off the map empties the path and ends the move.
    let mut w = map(vec![entity("guests", "", 1, centre((5, 5)), 2)]);
    let mut s = sim();
    send(&mut w, 1, &[(5, 5), (6, 5)], 33);
    w.units.get_mut(1).unwrap().destination = None;
    step_all(&mut w, &mut s, &[67]);
    let u = w.units.get(1).unwrap();
    assert!(u.path.is_empty() && !u.moving);
    assert_eq!(position(&w, 1), centre((5, 5)));
}

#[test]
fn a_unit_entering_an_exhibit_through_its_gate_tells_the_exhibit() {
    let mut w = world_with((30, 30), |zoo| {
        let e = &mut zoo.entities;
        for x in 10..=12 {
            for y in 10..=12 {
                for (d, outside) in [(0, y == 10), (2, x == 12), (4, y == 12), (6, x == 10)] {
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
        let gate = e.iter_mut().find(|f| {
            let c = f.common.as_ref().unwrap();
            (c.x / 64, c.y / 64, c.rotation) == (11, 10, 0)
        });
        gate.unwrap().subtype = "g".into();
        e.push(entity("keeper", "", 1, centre((11, 9)), 4));
        zoo.habitat_tiles = vec![(10, 10)];
        zoo.habitat_gates = vec![((11, 10), 0)];
    });
    crate::goal::quiet(&mut w);
    let mut s = sim();
    let exhibit = w.exhibits().region(w.tile_index(11, 11).unwrap()).unwrap();
    let to = [(11, 9), (11, 10), (11, 11)];
    let end = to.map(|(x, y)| w.tile_index(x, y).unwrap());
    let u = w.units.get_mut(1).unwrap();
    u.destination = Some((11 * 64 + 32, 11 * 64 + 32, 0));
    assert!(start(u, end.to_vec(), 40, 100));
    w.units.take_lines();
    step_all(&mut w, &mut s, &[67; 60]);
    let lines = w.units.take_lines();
    let tiles: Vec<_> = lines.iter().filter(|l| l.starts_with("tile")).collect();
    assert_eq!(tiles, ["tile 1 11,10", "tile 1 11,11"]);
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.starts_with("enter"))
            .collect::<Vec<_>>(),
        [&format!("enter 1 {exhibit}")]
    );
}

#[test]
fn a_unit_of_more_than_four_cells_has_the_small_margin() {
    let w = map(vec![
        entity("animals", "", 1, centre((5, 5)), 0),
        entity("animals", "big", 2, centre((8, 5)), 0),
    ]);
    assert_eq!((margin(&w, 0), margin(&w, 1)), (20, 5));
}

#[test]
fn only_the_value_one_of_the_ini_allows_diagonal_moves() {
    for (ini, want) in [
        ("", true),
        ("[AI]\nAllowDiagonalMoves=1\n", true),
        ("[AI]\nallowdiagonalmoves=0\n", false),
        ("[AI]\nAllowDiagonalMoves=-3\n", false),
        ("[AI]\nAllowDiagonalMoves=2\n", false),
        ("[AI]\nAllowDiagonalMoves=99999999999\n", false),
        ("[AI]\nAllowDiagonalMoves=x\n", true),
    ] {
        let mut w = map(vec![]);
        w.units.set_ini(ini.as_bytes());
        assert_eq!(w.units.finder.settings.diagonal, want, "{ini:?}");
    }
}

#[test]
fn a_fast_guest_does_not_move_on_a_cornering_step_but_a_keeper_does() {
    // Speed 400 at 67 ms is 26 per step, over the limit of 24 (issue 302).
    for (class, kind, moved) in [("guests", "g", false), ("keeper", "k", true)] {
        let mut w = map(vec![entity(class, kind, 1, (5 * 64 + 40, 9 * 64 + 10), 0)]);
        let mut s = sim();
        send(&mut w, 1, &[(5, 9), (6, 9)], 400);
        w.units.get_mut(1).unwrap().travel = 0;
        step_all(&mut w, &mut s, &[67]);
        assert_eq!(
            position(&w, 1) != (5 * 64 + 40, 9 * 64 + 10),
            moved,
            "{class}"
        );
    }
}
