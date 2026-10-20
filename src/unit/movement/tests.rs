//! The move on synthetic maps: the worked examples of the spec.

use super::*;
use crate::sim::Sim;
use crate::unit::testing::{Walker, centre, entity, world, world_with};
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

/// Starts a walk along `path` at `speed`, as a move call does; a path of fewer than two tiles is a
/// unit already in its last tile, which stops at once.
fn start(unit: &mut Unit, path: Vec<usize>, speed: u32, anim_speed: u16) -> bool {
    if path.len() < 2 {
        stop(unit);
        return false;
    }
    (unit.path, unit.speed, unit.anim_speed) = (path, speed, anim_speed);
    (unit.moving, unit.walking) = (true, true);
    true
}

/// Sends unit `id` along `path` (tiles) at `speed`, to the last tile.
fn send(w: &mut World, id: u32, path: &[(usize, usize)], speed: u32) {
    let end = path.last().unwrap();
    let unit = w.units.get_mut(id).unwrap();
    unit.destination = Some((64 * end.0 as i32 + 32, 64 * end.1 as i32 + 32, 0));
    let tiles = path.iter().map(|&(x, y)| tile(x, y)).collect();
    assert!(start(unit, tiles, speed, 100));
    Walker::new("fWalk(0,0)").start(w, id);
}

fn step_all(w: &mut World, s: &mut Sim, steps: &[u32]) {
    for &ms in steps {
        s.step_with(
            &Walker::new("fWalk(0,0)"),
            &Default::default(),
            w,
            ms,
            |_, _, _| {},
        );
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
    // It stays where it stopped: the set is over.
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
    (u.path, u.moving, u.walking, u.speed, u.travel) = (vec![tile(5, 5)], true, true, 40, 2);
    u.destination = Some((6 * 64 + 32, 5 * 64 + 32, 0));
    Walker::new("fWalk(0,0)").start(&mut w, 1);
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
    // The goal gives up and clears the flag in the step of the refusal (4.7).
    assert!(!w.units.get(1).unwrap().aborted);
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
    // The move call fails and the walk is over.
    let u = w.units.get(1).unwrap();
    assert!(!u.walking && !u.moving && u.path.is_empty());
    // A unit going through a building looks at the edge cost only, which the fence makes 20 too.
    send(&mut w, 1, &[(5, 5), (6, 5), (7, 5)], 40);
    let u = w.units.get_mut(1).unwrap();
    (u.aborted, u.through_building) = (false, true);
    w.units.take_lines();
    step_all(&mut w, &mut s, &[67]);
    assert!((w.units.take_lines().iter()).any(|l| l.starts_with("abort")));
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

/// An exhibit of 3 by 3 tiles from (10, 10) with its gate on the north side of (11, 10), and
/// the keeper at tile (11, 9) outside it or (11, 11) inside.
fn gated(inside: bool) -> World {
    world_with((30, 30), |zoo| {
        let e = &mut zoo.entities;
        for x in 10..=12 {
            for y in 10..=12 {
                for (d, outside) in [(0, y == 10), (2, x == 12), (4, y == 12), (6, x == 10)] {
                    if outside {
                        let id = 100 + e.len() as u32;
                        e.push(entity("fences", "exhibit", id, centre((x, y)), d));
                    }
                }
            }
        }
        let gate = e.iter_mut().find(|f| {
            let c = f.common.as_ref().unwrap();
            (c.x / 64, c.y / 64, c.rotation) == (11, 10, 0)
        });
        gate.unwrap().subtype = "g".into();
        let at = centre((11, [9, 11][usize::from(inside)]));
        e.push(entity("keeper", "", 1, at, 4));
        zoo.habitat_tiles = vec![(10, 10)];
        zoo.habitat_gates = vec![((11, 10), 0)];
        zoo.habitat_info = vec![crate::zoo::HabitatInfo {
            name: String::new(),
            stats: [0; 9],
            created: 0,
            serviced: 0,
        }];
    })
}

/// Walks unit 1 of `w` through the gate, from (11, 9) to (11, 11) or back, and returns the log.
fn through_the_gate(mut w: World, inside: bool) -> Vec<String> {
    crate::goal::quiet(&mut w);
    let mut s = sim();
    let mut to = [(11, 9), (11, 10), (11, 11)];
    if inside {
        to.reverse();
    }
    let end = to.map(|(x, y)| w.tile_index(x, y).unwrap());
    let u = w.units.get_mut(1).unwrap();
    u.destination = Some((to[2].0 * 64 + 32, to[2].1 * 64 + 32, 0));
    assert!(start(u, end.to_vec(), 40, 100));
    Walker::new("fWalk(0,0)").start(&mut w, 1);
    w.units.take_lines();
    step_all(&mut w, &mut s, &[67; 60]);
    w.units.take_lines()
}

#[test]
fn a_unit_entering_an_exhibit_through_its_gate_tells_the_exhibit() {
    let w = gated(false);
    let exhibit = w.exhibits().region(w.tile_index(11, 11).unwrap()).unwrap();
    let lines = through_the_gate(w, false);
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
fn a_keeper_opens_the_gate_ahead_and_closes_it_behind_but_an_animal_does_not_open() {
    // Out of the exhibit: ahead on entering (11, 10), behind on entering (11, 9).
    let lines = through_the_gate(gated(true), true);
    let tail: Vec<_> = lines.iter().filter(|l| l.starts_with("gate")).collect();
    assert_eq!(tail.len(), 2);
    assert!(tail[0].starts_with("gate 1 open ") && tail[1].starts_with("gate 1 close "));
    assert_eq!(tail[0][12..], tail[1][13..], "the same gate");
    // An animal does not open it (the pathfinder keeps it from crossing: the entry is made by hand).
    for (kind, opens) in [(Kind::Keeper, true), (Kind::Animal, false)] {
        let mut w = gated(true);
        let t = |x, y| w.tile_index(x, y).unwrap();
        let (old, new, next) = (t(11, 11), t(11, 10), t(11, 9));
        let mut unit = Unit::new(1, kind);
        unit.path = vec![new, next];
        enter(&mut unit, &mut w, old, new);
        let lines = w.units.take_lines();
        assert_eq!(lines.iter().any(|l| l.starts_with("gate 1 open ")), opens);
        assert_eq!(unit.gate.is_some(), opens);
    }
}

/// A keeper enters (11, 10) of [`gated`] from (11, 11) with `path` (tile coordinates), and the
/// gate lines of the log are returned.
fn enters_the_gate_tile(path: &[(usize, usize)]) -> Vec<String> {
    let mut w = gated(true);
    let t = |x, y| w.tile_index(x as i32, y as i32).unwrap();
    let (old, new) = (t(11, 11), t(11, 10));
    let mut unit = Unit::new(1, Kind::Keeper);
    unit.path = path.iter().map(|&(x, y)| t(x, y)).collect();
    enter(&mut unit, &mut w, old, new);
    let lines = w.units.take_lines();
    lines
        .into_iter()
        .filter(|l| l.starts_with("gate"))
        .collect()
}

#[test]
fn a_gate_is_looked_up_towards_the_second_entry_of_the_path_and_across_a_corner() {
    // The second entry is the tile north of the gate tile: the gate is ahead. The first entry
    // does not matter, even when it is another tile (the unit is off its path).
    assert_eq!(enters_the_gate_tile(&[(11, 10), (11, 9)]).len(), 1);
    assert_eq!(enters_the_gate_tile(&[(12, 10), (11, 9)]).len(), 1);
    // A diagonal step to the north east reads the north slot of the tile, the north west one
    // does not reach the gate (it is the slot of the next edge round the corner).
    assert_eq!(enters_the_gate_tile(&[(11, 10), (12, 9)]).len(), 1);
    assert!(enters_the_gate_tile(&[(11, 10), (10, 9)]).is_empty());
    // Only the second entry counts.
    assert!(enters_the_gate_tile(&[(11, 9), (12, 10)]).is_empty());
}

#[test]
fn a_move_call_takes_the_rate_the_animation_and_the_animation_speed_of_its_band() {
    for (call, rate, anim, speed) in [
        ("fWalk(0,0)", 20, "walk", 50),
        ("fTrot(0,0)", 46, "trot", 100),
        ("fMoveFast(0,0)", 76, "run", 100),
        ("fMove(0,0,run,12,30)", 12, "run", 30),
        ("fMove(0,0,run,-5,70000)", 0, "run", 4464),
    ] {
        let mut w = map(vec![entity("keeper", "", 1, centre((5, 5)), 2)]);
        let (mut s, walker) = (sim(), Walker::new(call));
        // The unit shows its idle animation before the walk starts.
        s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
        let u = w.units.get_mut(1).unwrap();
        u.path = vec![tile(5, 5), tile(6, 5), tile(9, 5)];
        u.destination = Some((9 * 64 + 32, 5 * 64 + 32, 0));
        walker.start(&mut w, 1);
        let mut at = |w: &mut World| {
            s.step_with(&walker, &Default::default(), w, 67, |_, _, _| {});
            let u = w.units.get(1).unwrap();
            (u.speed, u.animation.name.clone(), u.anim_speed, u.moving)
        };
        // The call starts the walk and the cut of the forced start shows the move animation in
        // the step after; the unit moves from the step after that.
        assert_eq!(at(&mut w), (rate, "idle".into(), speed, false), "{call}");
        assert_eq!(position(&w, 1), centre((5, 5)));
        assert_eq!(at(&mut w), (rate, anim.into(), speed, true), "{call}");
        assert_eq!(position(&w, 1), centre((5, 5)));
        at(&mut w);
        assert_eq!(position(&w, 1) != centre((5, 5)), rate > 0);
    }
    // A type without the animation fails the call, which ends the set.
    let mut w = map(vec![entity("keeper", "", 1, centre((5, 5)), 2)]);
    let (mut s, walker) = (sim(), Walker::new("fMove(0,0,zz,12)"));
    w.units.get_mut(1).unwrap().path = vec![tile(5, 5), tile(6, 5)];
    walker.start(&mut w, 1);
    s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
    let u = w.units.get(1).unwrap();
    assert!(!u.walking && u.speed == 0 && !u.runner.running());
}

#[test]
fn a_move_call_without_a_path_walks_at_random_and_an_idle_guest_starts_one() {
    let mut w = world((40, 30), |_, e| {
        (0..40).for_each(|x| e.push(entity("paths", "", 0, centre((x, 20)), 0)));
        e.push(entity("guests", "", 1, centre((20, 20)), 2));
        e.push(entity("animals", "", 2, centre((20, 10)), 2));
    });
    let (mut s, walker) = (sim(), Walker::new("fWalk(0,0)"));
    // The guest stands in a routine that never ends and is sent out by its goals; the
    // animal's set has the move call itself, with no path and no destination.
    walker.start_set(&mut w, 1, "bZoo", false);
    walker.start(&mut w, 2);
    // Each unit walks and stands still when its walk ends: the guest in its destination tile
    // (the animal has no exhibit, and a partial path may end short of it).
    let mut arrived = [false; 2];
    for _ in 0..3000 {
        s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
        for (i, id) in [1, 2].into_iter().enumerate() {
            let u = w.units.get(id).unwrap();
            let Some((x, y, _)) = u.destination else {
                continue;
            };
            let here = position(&w, id);
            let over = !u.walking && !u.moving && u.path.is_empty() && !u.runner.running();
            let there = (here.0 / 64, here.1 / 64) == (x / 64, y / 64);
            if over && (there || id == 2) && (x, y) != centre((20, 20)) {
                arrived[i] = true;
            }
        }
    }
    assert_eq!(arrived, [true, true]);
}

#[test]
fn the_end_of_a_walk_keeps_the_move_animation_but_a_path_of_one_entry_returns_to_idle() {
    let mut w = map(vec![entity("keeper", "", 1, centre((5, 5)), 2)]);
    let (mut s, walker) = (sim(), Walker::new("fWalk(0,0)"));
    let u = w.units.get_mut(1).unwrap();
    u.path = vec![tile(5, 5), tile(6, 5)];
    u.destination = Some((6 * 64 + 32, 5 * 64 + 32, 0));
    walker.start(&mut w, 1);
    let mut step = |w: &mut World| s.step_with(&walker, &Default::default(), w, 67, |_, _, _| {});
    for _ in 0..200 {
        step(&mut w);
    }
    let u = w.units.get(1).unwrap();
    assert!(!u.walking && !u.moving && u.path.is_empty());
    assert_eq!(u.animation.name, "walk");
    // Already in its last tile: the call stops the unit at once and it shows the idle animation.
    w.units.get_mut(1).unwrap().path = vec![tile(6, 5)];
    walker.start(&mut w, 1);
    step(&mut w);
    step(&mut w);
    assert_eq!(w.units.get(1).unwrap().animation.name, "idle");
}

#[test]
fn an_aborted_walk_ends_its_move_call_as_done() {
    let mut w = map(vec![entity("keeper", "", 1, centre((5, 5)), 2)]);
    let (mut s, walker) = (sim(), Walker::new("fWalk(0,0)"));
    s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
    let u = w.units.get_mut(1).unwrap();
    u.path = vec![tile(5, 5), tile(6, 5), tile(9, 5)];
    u.destination = Some((9 * 64 + 32, 5 * 64 + 32, 0));
    walker.start(&mut w, 1);
    for _ in 0..3 {
        s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
    }
    let mut u = w.units.get(1).unwrap().clone();
    assert!(u.walking && u.moving);
    u.aborted = true;
    let at = w.find(1).unwrap();
    let flow = call(
        &mut u,
        &mut w,
        &walker,
        &mut Rng::default(),
        at,
        &(Gait::Slow, Target::None),
    );
    assert_eq!(flow, Flow::Done);
    assert!(!u.walking && !u.moving && u.path.is_empty());
}

#[test]
fn a_target_0_keeps_the_3d_target() {
    let mut w = map(vec![entity("keeper", "", 1, centre((5, 5)), 2)]);
    let mut u = w.units.get(1).unwrap().clone();
    u.goal.record.task = crate::goal::RANDOM;
    let at = w.find(1).unwrap();
    let (mut rng, walker) = (Rng::default(), Walker::new("fWalk(0,0)"));
    let mut run = |u: &mut Unit, w: &mut World, target| {
        u.walking = false;
        u.path = vec![tile(5, 5), tile(6, 5)];
        u.destination = Some((6 * 64 + 32, 5 * 64 + 32, 0));
        call(u, w, &walker, &mut rng, at, &(Gait::Slow, target));
    };
    let held = |u: &mut Unit| u.goal.record.target.id = 7;
    u.target = Target::Dive;
    held(&mut u);
    // `0` and the name it has change nothing.
    run(&mut u, &mut w, Target::Keep);
    run(&mut u, &mut w, Target::Dive);
    assert_eq!(u.target, Target::Dive);
    assert!(u.goal.record.target.is_set());
    // Another name clears the destination and is recorded, also one that is no name.
    run(&mut u, &mut w, Target::Surface);
    assert_eq!(u.target, Target::Surface);
    assert!(!u.goal.record.target.is_set());
    held(&mut u);
    run(&mut u, &mut w, Target::None);
    assert_eq!(u.target, Target::None);
    assert!(!u.goal.record.target.is_set());
    // Only while the goal is the plain wander.
    u.goal.record.task = crate::goal::NONE;
    run(&mut u, &mut w, Target::Rise);
    assert_eq!(u.target, Target::None);
}

#[test]
fn a_swimmer_entering_water_picks_the_animation_speed_of_the_water() {
    let mut w = world((40, 30), |tiles, e| {
        tiles[tile(6, 5)].terrain = 1;
        e.push(entity("animals", "", 1, (5 * 64 + 60, 5 * 64 + 32), 2));
    });
    w.set_water(vec![false, true]);
    crate::goal::quiet(&mut w);
    let (mut s, walker) = (sim(), Walker::new("fWalk(0,0)"));
    let u = w.units.get_mut(1).unwrap();
    u.path = vec![tile(5, 5), tile(6, 5), tile(7, 5)];
    u.destination = Some((7 * 64 + 32, 5 * 64 + 32, 0));
    walker.start(&mut w, 1);
    let mut run = |w: &mut World, n| {
        (0..n).for_each(|_| _ = s.step_with(&walker, &Default::default(), w, 67, |_, _, _| {}));
        w.units.get(1).unwrap().anim_speed
    };
    assert_eq!(run(&mut w, 3), 50, "on land");
    assert_eq!(w.units.get(1).unwrap().animation.name, "walk");
    assert_eq!(run(&mut w, 4), 80, "in the water");
    assert_eq!(w.units.get(1).unwrap().animation.name, "swim");
    assert!(!w.units.get(1).unwrap().flip);
}

#[test]
fn a_swimmer_starting_a_walk_in_the_water_swims_at_once_and_a_started_unit_reads_its_type() {
    let mut w = world((40, 30), |tiles, e| {
        tiles[tile(5, 5)].terrain = 1;
        tiles[tile(6, 5)].terrain = 1;
        e.push(entity("animals", "", 1, centre((5, 5)), 2));
        e.push(entity("guests", "", 2, centre((5, 5)), 2));
    });
    w.set_water(vec![false, true]);
    crate::goal::quiet(&mut w);
    let walker = Walker::new("fMove(0,0,run,12,30)");
    // An animal loaded with the map has had its first turn already.
    w.units.get_mut(1).unwrap().started = true;
    let mut s = sim();
    s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
    for id in [1, 2] {
        let u = w.units.get_mut(id).unwrap();
        u.path = vec![tile(5, 5), tile(6, 5)];
        u.destination = Some((6 * 64 + 32, 5 * 64 + 32, 0));
        walker.start(&mut w, id);
    }
    for _ in 0..2 {
        s.step_with(&walker, &Default::default(), &mut w, 67, |_, _, _| {});
    }
    let name = |id| w.units.get(id).unwrap().animation.name.clone();
    // The animal's own name is replaced on water, its own speed is kept; the guest keeps both
    // (a guest does not swim).
    assert_eq!(
        (name(1), w.units.get(1).unwrap().anim_speed),
        ("swim".into(), 30)
    );
    assert_eq!(
        (name(2), w.units.get(2).unwrap().anim_speed),
        ("run".into(), 30)
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

#[test]
fn a_corner_cut_before_the_destination_walks_back_into_it() {
    // 13.7: an animal at sub-tile (40, 10) of A, aiming at the tile diagonally behind it; the cut
    // leaves it on a tile off its path, and the way back into the tile it aims at is allowed.
    let mut w = map(vec![entity(
        "animals",
        "",
        1,
        (19 * 64 + 40, 20 * 64 + 10),
        5,
    )]);
    let mut s = sim();
    send(&mut w, 1, &[(19, 20), (18, 21)], 20);
    step_all(&mut w, &mut s, &[67; 120]);
    let (x, y) = position(&w, 1);
    assert_eq!((x.div_euclid(64), y.div_euclid(64)), (18, 21));
    assert!(w.units.get(1).unwrap().path.is_empty());
}

/// The `Mover` the path finder gets for unit `id`, as a goal's request builds it.
fn mover_of(w: &World, id: u32) -> path::Mover {
    let at = w.find(id).unwrap();
    let here = here(w, at).unwrap();
    mover(w.units.get(id).unwrap(), w, at, here)
}

fn set_task(w: &mut World, id: u32, task: u8, target: u32) {
    let record = &mut w.units.get_mut(id).unwrap().goal.record;
    (record.task, record.target.id) = (task, target);
}

#[test]
fn the_mover_follows_the_task_of_the_unit() {
    use crate::goal::{
        AVOID, CHASE, ENTER_BUILDING, GO_TO_SHOW, LEAVE, RECOVER, RETURN_FROM_SHOW,
        RETURN_TO_BUILDING,
    };
    let mut w = map(vec![
        entity("guests", "", 1, centre((3, 3)), 0),
        entity("animals", "", 2, centre((5, 5)), 0),
        entity("keeper", "", 3, centre((7, 7)), 0),
    ]);
    // (task, guest: leaving, avoiding, recovering, in water; animal: show, chasing).
    let guest = |m: &path::Mover| [m.leaving, m.avoiding, m.recovering, m.in_water];
    for (task, flags) in [
        (0, [false; 4]),
        (LEAVE, [true, false, false, false]),
        (AVOID, [false, true, false, false]),
        (RECOVER, [false, false, true, true]),
        (ENTER_BUILDING, [false, false, true, false]),
        (RETURN_TO_BUILDING, [false, false, false, true]),
        (CHASE, [false; 4]),
        (u8::MAX, [false; 4]),
    ] {
        set_task(&mut w, 1, task, 9);
        assert_eq!(guest(&mover_of(&w, 1)), flags, "guest, task {task}");
        // The animal's and the keeper's tasks never set the guest's states.
        for id in [2, 3] {
            set_task(&mut w, id, task, 9);
            assert_eq!(
                guest(&mover_of(&w, id)),
                [false; 4],
                "unit {id}, task {task}"
            );
        }
    }
    for (task, target, show, chasing) in [
        (GO_TO_SHOW, 9, true, None),
        (RETURN_FROM_SHOW, 9, true, None),
        (CHASE, 9, false, Some(9)),
        // No target, no chase.
        (CHASE, 0, false, None),
        (LEAVE, 9, false, None),
    ] {
        set_task(&mut w, 2, task, target);
        let m = mover_of(&w, 2);
        assert_eq!((m.show, m.chasing), (show, chasing), "animal, task {task}");
        // Only an animal has them.
        for id in [1, 3] {
            set_task(&mut w, id, task, target);
            let m = mover_of(&w, id);
            assert_eq!((m.show, m.chasing), (false, None), "unit {id}, task {task}");
        }
    }
}

#[test]
fn the_mover_has_the_destination_tile_the_building_flag_and_the_terrain_mode() {
    let mut w = map(vec![
        entity("animals", "", 1, centre((3, 3)), 0),
        entity("keeper", "", 2, centre((7, 7)), 0),
    ]);
    assert_eq!(mover_of(&w, 1).dest, None);
    for (dest, tile) in [
        (Some(centre((9, 4))), Some(tile(9, 4))),
        // Off the map, in either direction, and far off it.
        (Some((-1, 32)), None),
        (Some((32, -1)), None),
        (Some((i32::MAX, i32::MIN)), None),
        (None, None),
    ] {
        w.units.get_mut(1).unwrap().destination = dest.map(|(x, y)| (x, y, 0));
        assert_eq!(mover_of(&w, 1).dest, tile, "{dest:?}");
    }
    let m = mover_of(&w, 1);
    assert_eq!(
        (m.id, m.at, m.building, m.water_mode),
        (Some(1), Some(tile(3, 3)), false, false)
    );
    w.units.get_mut(1).unwrap().through_building = true;
    assert!(mover_of(&w, 1).building);
    for (terrain, water) in [(0, false), (1, true), (2, false), (u8::MAX, false)] {
        if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
            a.terrain = terrain;
        }
        assert_eq!(mover_of(&w, 1).water_mode, water, "mode {terrain}");
    }
    // A keeper has no terrain mode.
    assert!(!mover_of(&w, 2).water_mode);
}

#[test]
fn a_guest_leaving_crosses_the_grass_between_two_ends_of_a_path_it_would_walk_round() {
    // A path from (10, 10) up to row 6, across and down to (12, 10); the grass between its ends
    // is two steps.
    let mut tiles: Vec<(i32, i32)> = (6..=10).flat_map(|y| [(10, y), (12, y)]).collect();
    tiles.push((11, 6));
    let mut e: Vec<_> = tiles
        .iter()
        .map(|&t| entity("paths", "", 0, centre(t), 0))
        .collect();
    e.push(entity("guests", "", 1, centre((10, 10)), 0));
    let mut w = map(e);
    let walk = |w: &mut World, task| {
        set_task(w, 1, task, 0);
        let m = mover_of(w, 1);
        let reply = path::Finder::default().request(w, &m, tile(12, 10), false);
        reply.path.map_or(0, |p| p.len())
    };
    // Round by the path, 11 tiles; leaving, over the grass tile between.
    assert_eq!(walk(&mut w, 0), 11);
    assert_eq!(walk(&mut w, crate::goal::LEAVE), 3);
    // Avoiding a predator, the grass costs the emergency cost of 1.
    assert_eq!(walk(&mut w, crate::goal::AVOID), 3);
}

/// A class file with these keys.
struct Class(Vec<(&'static str, i32)>);

impl crate::behaviour::Data for Class {
    fn set(&self, _: &crate::zoo::Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
        None
    }
    fn ambient(&self, _: &crate::zoo::Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Timing> {
        None
    }
    fn number(&self, _: &crate::zoo::Entity, _: &crate::behaviour::Number) -> i32 {
        0
    }
    fn sound(&self, _: &crate::zoo::Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
    fn unit_class(&self, key: &str) -> Option<i32> {
        self.0.iter().find(|k| k.0 == key).map(|k| k.1)
    }
}

#[test]
fn a_guest_reads_its_costs_off_the_paths_from_the_class_file() {
    let w = map(vec![
        entity("guests", "", 1, centre((3, 3)), 0),
        entity("keeper", "", 2, centre((7, 7)), 0),
    ]);
    let costs = |id: u32, class: &Class| {
        let mut unit = w.units.get(id).unwrap().clone();
        read_type(&mut unit, class, &w.zoo.entities[w.find(id).unwrap()]);
        (unit.mover.non_path_cost, unit.mover.emergency_cost)
    };
    let class = |v: Vec<(&'static str, i32)>| Class(v);
    // The fallbacks are 7 and 1; the file's values replace them; none goes below 0.
    assert_eq!(costs(1, &class(vec![])), (7, 1));
    assert_eq!(
        costs(
            1,
            &class(vec![("cNonPathCost", 5), ("cNonPathCostEmergency", 2)])
        ),
        (5, 2)
    );
    assert_eq!(costs(1, &class(vec![("cNonPathCost", 0)])), (0, 1));
    assert_eq!(
        costs(
            1,
            &class(vec![
                ("cNonPathCost", -3),
                ("cNonPathCostEmergency", i32::MIN)
            ])
        ),
        (0, 0)
    );
    assert_eq!(
        costs(1, &class(vec![("cNonPathCost", i32::MAX)])),
        (i32::MAX as u32, 1)
    );
    // Only a guest has them.
    assert_eq!(costs(2, &class(vec![("cNonPathCost", 5)])), (7, 1));
}

#[test]
fn a_free_move_goes_straight_to_its_point_by_the_distance_of_a_step_and_stands_on_it() {
    let (x, y) = centre((5, 5));
    let mut w = map(vec![entity("keeper", "keeper", 1, (x, y), 0)]);
    let at = w.find(1).unwrap();
    let mut unit = w.units.get(1).unwrap().clone();
    let z = |w: &World| w.zoo.entities[at].common.as_ref().unwrap().z;
    // Straight up: two units a step, the last one onto the point, which is done.
    unit.destination = Some((x, y, 5));
    assert_eq!(
        [fly(&mut unit, &mut w, 2, at), fly(&mut unit, &mut w, 2, at)],
        [false, false]
    );
    assert_eq!(z(&w), 4);
    assert!(fly(&mut unit, &mut w, 2, at));
    assert_eq!((position(&w, 1), z(&w)), ((x, y), 5));
    // On the point, and with no destination, it is done at once.
    assert!(fly(&mut unit, &mut w, 2, at));
    unit.destination = None;
    assert!(fly(&mut unit, &mut w, 2, at));
    // Down, and along a line: the length is truncated, the end is the point.
    unit.destination = Some((x + 30, y, -35));
    let mut steps = 0;
    while !fly(&mut unit, &mut w, 16, at) {
        steps += 1;
        assert!(steps < 10);
    }
    assert_eq!((position(&w, 1), z(&w)), ((x + 30, y), -35));
    // From (x, y, 5): a length of 50 is three steps of 16 and the one that reaches it.
    assert_eq!(steps, 3);
}

#[test]
fn a_unit_in_the_air_is_not_put_on_the_ground_by_the_snap() {
    let (x, y) = centre((5, 5));
    let mut w = map(vec![entity("keeper", "keeper", 1, (x, y), 0)]);
    let at = w.find(1).unwrap();
    w.place_unit(at, (x, y, 50), 0);
    snap(&mut w, at);
    assert_eq!(w.zoo.entities[at].common.as_ref().unwrap().z, 0);
    w.place_unit(at, (x, y, 50), 0);
    w.zoo.entities[at].common.as_mut().unwrap().movement_mode = 1;
    snap(&mut w, at);
    assert_eq!(w.zoo.entities[at].common.as_ref().unwrap().z, 50);
}
