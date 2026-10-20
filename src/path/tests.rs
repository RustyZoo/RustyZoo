//! The finder on synthetic maps: the worked examples of the spec.

use super::*;
use crate::render::order::Kind as Class;
use crate::world::{Shape, TankKind, World};
use crate::zoo::{Common, Entity, Fence, Tile, View, Zoo};

const N: i32 = 10;

fn entity(class: &str, kind: &str, subtype: &str, (tx, ty): (i32, i32), rotation: u32) -> Entity {
    entity_at(class, kind, subtype, (tx * 64 + 32, ty * 64 + 32), rotation)
}

fn entity_at(class: &str, kind: &str, subtype: &str, (x, y): (i32, i32), rotation: u32) -> Entity {
    let fence = (class == "fences").then_some(Fence {
        strength: 5,
        life: 5,
        timers: [0; 2],
        portal: false,
    });
    Entity {
        class: class.into(),
        type_name: kind.into(),
        subtype: subtype.into(),
        body_size: None,
        common: Some(Common {
            x,
            y,
            z: 0,
            rotation,
            id: 0,
            name: String::new(),
            colors: Vec::new(),
            fence,
            ..Default::default()
        }),
    }
}

/// `fences` of type `low` are low fences, the others exhibit fences; `bush` is a one-quarter
/// object and `gate` the zoo entrance.
fn shape(e: &Entity) -> Option<Shape> {
    let thing = |kind, footprint, fence, gate| Shape {
        kind,
        footprint,
        fence,
        wall: false,
        gate,
        gated: fence,
        tank: TankKind::NONE,
        walkable: (gate, gate),
    };
    match (e.class.as_str(), e.type_name.as_str()) {
        ("fences", "low") => Some(thing(Class::Edge, (1, 1), false, false)),
        ("fences", _) => Some(thing(Class::Edge, (1, 1), true, false)),
        ("paths", _) => Some(thing(Class::Path, (2, 2), false, false)),
        ("bush", _) | ("food", _) | ("building", _) => {
            Some(thing(Class::Fixed, (1, 1), false, false))
        }
        // Show walls of a tank (`show`) and the others; units that let others through (`soft`).
        ("tankwall", kind) => Some(Shape {
            kind: Class::Wall,
            tank: TankKind {
                wall: true,
                show: kind == "show",
                ..TankKind::NONE
            },
            ..thing(Class::Wall, (1, 1), true, false)
        }),
        ("animals" | "guests", kind) => Some(Shape {
            walkable: (kind == "soft", kind == "soft"),
            ..thing(Class::Unit, (1, 1), false, false)
        }),
        ("gate", _) => Some(thing(Class::Fixed, (2, 2), false, true)),
        _ => None,
    }
}

/// An open `N` by `N` map with the entrance off its south-east corner, then `edit`.
pub(super) fn world(edit: impl FnOnce(&mut Vec<Tile>, &mut Vec<Entity>)) -> World {
    sized((N, N), edit)
}

/// The same for a map of `w` by `h` tiles.
fn sized((w, h): (i32, i32), edit: impl FnOnce(&mut Vec<Tile>, &mut Vec<Entity>)) -> World {
    let mut tiles = vec![
        Tile {
            elevation: 0,
            shape: 0,
            terrain: 0,
            edges: 0
        };
        (w * h) as usize
    ];
    let mut entities = vec![entity("gate", "", "", (w - 1, h - 1), 2)];
    edit(&mut tiles, &mut entities);
    let zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: w as u32,
        height: h as u32,
        entrance: (w as u32 - 1, h as u32 - 1),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        clocks: Default::default(),
        names: Default::default(),
        tiles,
        entities,
        view: View::default(),
    };
    World::new(zoo, shape)
}

mod live;

fn t((x, y): (i32, i32)) -> usize {
    (y * N + x) as usize
}

fn xy(tile: usize) -> (i32, i32) {
    (tile as i32 % N, tile as i32 / N)
}

fn path(w: &World, m: &Mover, to: (i32, i32)) -> Vec<(i32, i32)> {
    search(w, m, &Settings::default(), m.at.unwrap(), t(to), true)
        .path
        .into_iter()
        .map(xy)
        .collect()
}

/// The step cost between two neighbouring tiles.
fn cost(w: &World, m: &Mover, a: (i32, i32), b: (i32, i32)) -> u32 {
    let d = search::direction(w, t(a), t(b));
    cost::step(w, m, t(a), t(b), d, Some(t(b)))
}

fn unit(kind: Kind, at: (i32, i32)) -> Mover {
    Mover::new(kind, Some(t(at)))
}

/// A path piece on each of these tiles.
fn paths(entities: &mut Vec<Entity>, tiles: &[(i32, i32)]) {
    entities.extend(tiles.iter().map(|&p| entity("paths", "", "", p, 0)));
}

#[test]
fn a_keeper_crosses_open_grass_in_a_straight_line() {
    let w = world(|_, _| {});
    let m = unit(Kind::Keeper, (5, 5));
    assert_eq!(path(&w, &m, (8, 5)), [(5, 5), (6, 5), (7, 5), (8, 5)]);
}

#[test]
fn step_costs_by_unit_kind() {
    let w = world(|_, e| paths(e, &[(1, 1), (2, 1), (3, 1)]));
    let (keeper, worker, animal) = (Kind::Keeper, Kind::Worker, Kind::Animal);
    assert_eq!(cost(&w, &unit(keeper, (5, 5)), (5, 5), (6, 5)), 3);
    assert_eq!(cost(&w, &unit(worker, (5, 5)), (5, 5), (6, 5)), 3);
    assert_eq!(cost(&w, &unit(animal, (5, 5)), (5, 5), (6, 5)), 2);
    let m = unit(keeper, (5, 5));
    let diagonal = path_cost(&w, &m, &[t((5, 5)), t((6, 4))]);
    assert!((diagonal - 3.0 * std::f64::consts::SQRT_2).abs() < 1e-9);
    assert_eq!(cost(&w, &unit(keeper, (1, 1)), (1, 1), (2, 1)), 2);
    assert_eq!(cost(&w, &unit(animal, (1, 1)), (1, 1), (2, 1)), 17);
    // A guest on a path pays the non-path cost for grass, and keeps to the paths when going to one.
    let guest = unit(Kind::Guest, (1, 1));
    assert_eq!(cost(&w, &guest, (3, 1), (3, 2)), 8);
    assert_eq!(terrain(&w, &guest, t((3, 2)), Some(t((2, 1)))), MAX);
    let leaving = Mover {
        leaving: true,
        ..guest.clone()
    };
    assert_eq!(terrain(&w, &leaving, t((3, 2)), Some(t((2, 1)))), 7);
    let cheap = Mover {
        non_path_cost: 5,
        ..guest
    };
    assert_eq!(cost(&w, &cheap, (3, 1), (3, 2)), 6);
}

#[test]
fn a_path_corner_costs_the_worst_edge_and_13() {
    let w = world(|_, e| paths(e, &[(5, 5), (6, 5), (6, 4)]));
    let m = unit(Kind::Keeper, (5, 5));
    assert_eq!(cost::edge(&w, &m, t((5, 5)), 1), 14);
    assert_eq!(cost(&w, &m, (5, 5), (6, 4)), 15);
    assert_eq!(path(&w, &m, (6, 4)), [(5, 5), (6, 5), (6, 4)]);
}

#[test]
fn steep_ground_costs_16_and_is_avoided_for_a_gap() {
    let w = world(|tiles, _| (0..7).for_each(|y| tiles[t((5, y))].shape = 0x01));
    let m = unit(Kind::Keeper, (2, 3));
    assert_eq!(cost(&w, &m, (4, 3), (5, 3)), 17);
    assert!(path(&w, &m, (8, 3)).iter().all(|&(x, y)| x != 5 || y >= 7));
    assert_eq!(
        terrain(&w, &unit(Kind::Animal, (4, 3)), t((5, 3)), None),
        16
    );
}

#[test]
fn fences_close_or_charge_each_kind() {
    // On the west side of (5,5) an exhibit fence, of (5,6) a gate, of (5,7) a low fence, of (5,8)
    // a broken exhibit fence.
    let w = world(|_, e| {
        e.push(entity("fences", "", "f", (5, 5), 6));
        e.push(entity("fences", "", "g", (5, 6), 6));
        e.push(entity("fences", "low", "f", (5, 7), 6));
        e.push(entity("fences", "", "f", (5, 8), 6));
        e[4].common.as_mut().unwrap().fence.as_mut().unwrap().life = 0;
    });
    let costs = |kind| [5, 6, 7, 8].map(|y| cost(&w, &unit(kind, (4, y)), (4, y), (5, y)));
    assert_eq!(costs(Kind::Keeper), [MAX, 3, 18, 3]);
    assert_eq!(costs(Kind::Guest), [MAX, 8, MAX, MAX]);
    assert_eq!(costs(Kind::Animal), [MAX, MAX, 2, 9]);
}

#[test]
fn the_path_goes_round_a_fence_through_its_gate() {
    let w = world(|_, e| {
        for y in 0..9 {
            e.push(entity(
                "fences",
                "",
                ["f", "g"][usize::from(y == 6)],
                (5, y),
                6,
            ));
        }
    });
    let p = path(&w, &unit(Kind::Keeper, (2, 2)), (8, 2));
    assert_eq!(p.last(), Some(&(8, 2)));
    assert!(p.windows(2).any(|s| s[0] == (4, 6) && s[1] == (5, 6)));
}

#[test]
fn the_building_flag_passes_every_quarter_but_those_of_another_building() {
    // Buildings 7 and 8 hold the southern quarters of (5,5) and (7,5).
    let w = world(|_, e| {
        for (id, tx) in [(7, 5), (8, 7)] {
            for x in [16, 48] {
                let mut b = entity_at("building", "", "", (tx * 64 + x, 5 * 64 + 48), 0);
                b.common.as_mut().unwrap().id = id;
                e.push(b);
            }
        }
    });
    let open = |m: &Mover, from, to, flag| {
        let s = Settings::default();
        search::open(&w, m, &s, (t(from), t(to), 0), None, flag).is_some()
    };
    let target = Mover {
        target_building: Some(7),
        ..unit(Kind::Guest, (5, 6))
    };
    assert!(
        !open(&target, (5, 6), (5, 5), false),
        "closed without the flag"
    );
    assert!(
        open(&target, (5, 6), (5, 5), true),
        "the target's own quarters"
    );
    assert!(!open(&target, (7, 6), (7, 5), false));
    assert!(!open(&target, (7, 6), (7, 5), true), "another building's");
    // Without a building target the flag passes every quarter.
    assert!(open(&unit(Kind::Guest, (7, 6)), (7, 6), (7, 5), true));
}

#[test]
fn objects_hold_quarters_and_close_a_side() {
    // Two bushes on the southern quarters of (5,5).
    let w = world(|_, e| {
        e.push(entity_at("bush", "", "", (5 * 64 + 16, 5 * 64 + 48), 0));
        e.push(entity_at("bush", "", "", (5 * 64 + 48, 5 * 64 + 48), 0));
    });
    let m = unit(Kind::Keeper, (5, 6));
    let clear = |from, to, d| cost::clear(&w, &m, (t(from), t(to)), d, true, None);
    assert!(!clear((5, 6), (5, 5), 0), "in from the south");
    assert!(clear((5, 4), (5, 5), 4), "in from the north");
    assert!(clear((4, 5), (5, 5), 2), "in from the west");
    assert!(
        !clear((4, 6), (5, 5), 1),
        "the southern side closes the diagonal too"
    );
}

#[test]
fn a_tile_with_all_its_quarters_held_is_closed_to_a_keeper() {
    let w = world(|_, e| {
        for (x, y) in [(16, 16), (48, 16), (16, 48), (48, 48)] {
            e.push(entity_at("bush", "", "", (5 * 64 + x, 5 * 64 + y), 0));
        }
    });
    let relaxed = Mover::keeper(false, true);
    assert_eq!(terrain(&w, &relaxed, t((5, 5)), None), MAX);
    assert_eq!(terrain(&w, &relaxed, t((4, 5)), None), 1);
    assert_eq!(terrain(&w, &unit(Kind::Worker, (1, 1)), t((5, 5)), None), 2);
}

#[test]
fn the_pair_cost_of_the_gate_choice_and_of_the_gate_tool() {
    let w = world(|tiles, e| {
        paths(e, &[(1, 1), (2, 1)]);
        tiles[t((6, 6))].shape = 0x01;
    });
    let (strict, relaxed) = (Mover::keeper(false, false), Mover::keeper(false, true));
    assert_eq!(pair_cost(&w, &strict, t((1, 1)), t((2, 1))), 2);
    assert_eq!(pair_cost(&w, &strict, t((3, 3)), t((4, 3))), 3);
    assert_eq!(pair_cost(&w, &strict, t((5, 6)), t((6, 6))), 17);
    assert_eq!(pair_cost(&w, &relaxed, t((5, 6)), t((6, 6))), 2);
}

#[test]
fn a_closed_exhibit_is_unreachable_and_the_list_counts_failures() {
    // A 3 by 3 exhibit (4..=6, 4..=6) closed with exhibit fences.
    let w = world(|_, e| {
        for i in 4..=6 {
            for (x, y, side) in [(i, 4, 0), (i, 6, 4), (4, i, 6), (6, i, 2)] {
                e.push(entity("fences", "", "f", (x, y), side));
            }
        }
    });
    let mut finder = Finder::default();
    let m = unit(Kind::Keeper, (1, 5));
    let to = t((5, 5));
    for n in 1..=4 {
        let r = finder.request(&w, &m, to, false);
        assert_eq!(
            (r.ok, r.dest, finder.failures(to)),
            (false, Dest::Clear, Some(n))
        );
    }
    finder.request(&w, &m, to, false);
    assert_eq!(finder.failures(to), Some(-5), "confirmed unreachable");
    for left in [-4, -3, -2, -1, 0] {
        let r = finder.request(&w, &m, to, false);
        assert_eq!((r.ok, r.path), (false, None), "refused without a search");
        assert_eq!(finder.failures(to), (left != 0).then_some(left));
    }
    finder.request(&w, &m, to, false);
    assert_eq!(finder.failures(to), Some(1), "searches again");
    // The clock empties the list at the first update, then every `ClearUnreachableFreq` ticks.
    finder.tick(0);
    assert_eq!(finder.failures(to), None);
    finder.request(&w, &m, to, false);
    finder.tick(4);
    assert_eq!(finder.failures(to), Some(1));
    finder.tick(5);
    assert_eq!(finder.failures(to), None);
}

#[test]
fn the_request_answers_the_easy_cases_without_a_search() {
    let w = world(|_, _| {});
    let mut finder = Finder::default();
    let m = unit(Kind::Keeper, (2, 2));
    let same = finder.request(&w, &m, t((2, 2)), false);
    assert_eq!((same.ok, same.path), (true, Some(vec![t((2, 2))])));
    let off = finder.request(&w, &m, 10_000, false);
    assert_eq!((off.ok, off.path), (false, None));
    let far = finder.request(&w, &m, t((8, 2)), false);
    assert_eq!(
        (far.ok, far.dest, far.path.map(|p| p.len())),
        (true, Dest::Set(t((8, 2))), Some(7))
    );
    assert!(
        !finder
            .request(&w, &Mover::new(Kind::Keeper, None), t((8, 2)), false)
            .ok
    );
}

#[test]
fn the_node_cap_ends_the_search_with_a_partial_path_towards_the_goal() {
    let w = world(|_, _| {});
    let m = unit(Kind::Keeper, (0, 0));
    let capped = |node_max| Settings {
        node_max,
        ..Settings::default()
    };
    let go = |s: &Settings, partial| search(&w, &m, s, t((0, 0)), t((8, 8)), partial);
    let found = go(&capped(20), true);
    let end = xy(*found.path.last().unwrap());
    assert!(!found.reached && end.0 + end.1 > 2);
    assert!(go(&capped(20), false).path.is_empty());
    assert!(go(&capped(0), true).path.is_empty());
    let r = Finder::new(capped(20)).request(&w, &m, t((8, 8)), true);
    assert!(
        r.ok && r.dest != Dest::Set(t((8, 8))),
        "a partial path succeeds"
    );
}

#[test]
fn guests_search_in_four_directions_and_animals_in_eight() {
    let w = world(|_, _| {});
    let tiles = |kind, diagonal| {
        let s = Settings {
            diagonal,
            ..Settings::default()
        };
        search(&w, &unit(kind, (1, 1)), &s, t((1, 1)), t((4, 4)), false)
            .path
            .len()
    };
    assert_eq!(tiles(Kind::Guest, true), 7);
    assert_eq!(tiles(Kind::Animal, true), 4);
    assert_eq!(tiles(Kind::Animal, false), 7);
}

#[test]
fn settings_come_from_the_ai_section_of_the_ini_file() {
    let read = |text: &str| Settings::from_ini(&Config::parse(text.as_bytes()).unwrap());
    assert_eq!(read(""), Settings::default());
    let all = "[ai]\nnodemax = 40\nALLOWDIAGONALMOVES=0\nMinCost=3\nEstimateFunc=2\n\
               ClearUnreachableFreq=9\nMinUnreachableCount=2\n";
    let set = Settings {
        diagonal: false,
        node_max: 40,
        min_cost: 3,
        estimate: 2,
        min_unreachable: 2,
        clear_every: 9,
    };
    assert_eq!(read(all), set);
    assert_eq!(read("[AI]\nNodeMax=0").node_max, 0);
    assert_eq!(
        read("[AI]\nNodeMax=-5\nClearUnreachableFreq=-1").node_max,
        0
    );
    assert_eq!(read("[AI]\nNodeMax=2000000000").node_max, 2_000_000_000);
    assert_eq!(read("[AI]\nMinUnreachableCount=300").min_unreachable, 44);
    assert!(!read("[AI]\nAllowDiagonalMoves=2").diagonal);
    assert_eq!(read("[AI]\nNodeMax=").node_max, 500);
}

#[test]
fn the_request_checks_a_guest_without_a_goal_tile() {
    // Two path tiles apart on grass: with a goal tile the guest on a path would see the grass at
    // 20 in the leavable check and fail with nothing recorded; with none it sees 7, searches and
    // fails, which is recorded.
    let w = world(|_, e| paths(e, &[(1, 1), (5, 1)]));
    let mut finder = Finder::default();
    let to = t((5, 1));
    let r = finder.request(&w, &unit(Kind::Guest, (1, 1)), to, false);
    assert_eq!(
        (r.ok, r.dest, finder.failures(to)),
        (false, Dest::Clear, Some(1))
    );
}

#[test]
fn the_distance_threshold_keeps_the_first_width_and_measures_on_the_map() {
    // An exhibit closed with fences around the tile at the centre of a 3 by 3 block.
    let closed = |e: &mut Vec<Entity>, (cx, cy): (i32, i32)| {
        for i in -1..=1 {
            for (x, y, side) in [
                (cx + i, cy - 1, 0),
                (cx + i, cy + 1, 4),
                (cx - 1, cy + i, 6),
                (cx + 1, cy + i, 2),
            ] {
                e.push(entity("fences", "", "f", (x, y), side));
            }
        }
    };
    let mut finder = Finder::default();
    // The first request has the same tile on both ends and a map 10 wide: that is the threshold.
    let first = world(|_, _| {});
    assert!(
        finder
            .request(&first, &unit(Kind::Keeper, (2, 2)), t((2, 2)), false)
            .ok
    );
    let w = sized((20, 5), |_, e| {
        closed(e, (13, 2));
        closed(e, (5, 2));
    });
    let m = Mover::new(Kind::Keeper, Some(41));
    // 12 tiles away on this map (the old row length would have said 3), above the threshold 10.
    assert!(!finder.request(&w, &m, 2 * 20 + 13, false).ok);
    assert_eq!(finder.failures(2 * 20 + 13), None);
    assert!(!finder.request(&w, &m, 2 * 20 + 5, false).ok);
    assert_eq!(finder.failures(2 * 20 + 5), Some(1));
}

#[test]
fn an_object_in_a_tank_blocks_only_its_own_levels() {
    use crate::unit::testing::tank_world;
    // The tank of 4 by 3 tiles from (2, 2), walls 9 high, water to 8; 12 tiles wide.
    let mut w = tank_world(9, 8, &[]);
    let tile = |x: i32, y: i32| (y * 12 + x) as usize;
    // Four quarters of an object of `cDepth` `depth` and `cHeight` `height` at level `level` fill
    // tile `(x, y)`.
    let mut put = |(x, y): (i32, i32), level: i32, depth: i32, height: i32| {
        for (qx, qy) in [(16, 16), (48, 16), (16, 48), (48, 48)] {
            let mut e = entity_at("rock", "", "", (x * 64 + qx, y * 64 + qy), 0);
            e.common.as_mut().unwrap().z = 16 * level;
            let tank = TankKind {
                depth,
                height,
                ..TankKind::NONE
            };
            let shape = Shape {
                kind: Class::Fixed,
                footprint: (1, 1),
                fence: false,
                wall: false,
                gate: false,
                gated: false,
                tank,
                walkable: (false, false),
            };
            w.add(e, Some(shape)).unwrap();
        }
    };
    // A rock on the floor, 1 high; a buoy floating at level 6 that reaches 2 below it; a rock on
    // land.
    put((3, 3), 0, 0, 1);
    put((4, 3), 6, 2, 0);
    put((8, 6), 0, 0, 1);
    let m = Mover::new(Kind::Animal, Some(tile(2, 3)));
    let clear =
        |a: usize, b: usize, d: usize, z: i32| cost::clear(&w, &m, (a, b), d, true, Some((z, z)));
    let over_rock: Vec<i32> = (0..=8)
        .filter(|&z| clear(tile(2, 3), tile(3, 3), 2, z))
        .collect();
    assert_eq!(
        over_rock,
        [2, 3, 4, 5, 6, 7, 8],
        "a swimmer passes over the rock"
    );
    let under_buoy: Vec<i32> = (0..=8)
        .filter(|&z| clear(tile(4, 2), tile(4, 3), 4, z))
        .collect();
    assert_eq!(under_buoy, [0, 1, 2, 3, 7, 8], "and under the buoy");
    // Out of a tank an object blocks every level; without levels (a flat search) too.
    assert!((-5..=5).all(|z| !clear(tile(7, 6), tile(8, 6), 2, z)));
    assert!(!cost::clear(
        &w,
        &m,
        (tile(2, 3), tile(3, 3)),
        2,
        true,
        None
    ));
    // Food put in the water at level 4 blocks by its type's `cDepth` (1): levels 3 and 4.
    let food = crate::entity::EntityType {
        class: "food",
        type_name: b"fish".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(
            crate::config::Config::parse(b"[Characteristics/Integers]\ncDepth=1\n").unwrap(),
        )),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    };
    let names = ("food", "food", "fish");
    assert!(crate::animal::dirt::put_z(
        &mut w,
        &food,
        names,
        (5 * 64 + 32, 2 * 64 + 32, Some(64)),
        0,
        5
    ));
    let m = Mover::new(Kind::Animal, Some(tile(4, 2)));
    let clear = |z: i32| cost::clear(&w, &m, (tile(4, 2), tile(5, 2)), 2, true, Some((z, z)));
    let past_food: Vec<i32> = (0..=8).filter(|&z| clear(z)).collect();
    assert_eq!(past_food, [0, 1, 2, 5, 6, 7, 8]);
}
