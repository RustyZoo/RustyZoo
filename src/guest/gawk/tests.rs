//! The gawk list, the unseen weight and the choice of scenery.

use super::*;
use crate::config::Config;
use crate::render::order::Kind;
use crate::unit::testing::{centre, entity};
use crate::world::{Shape, TankKind};
use crate::zoo::{Tile, View, Zoo};

fn kind(class: &'static str, name: &str, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// Statues of weight 60 and 30 that share a help id, an urn with another help id, a bench of no
/// weight and a stone of weight 20 that is a plain object.
fn catalogue() -> Catalogue {
    let item = |weight: i32, help: i32| {
        format!(
            "[Characteristics/Integers]\ncEstheticWeight={weight}\ncHelpID={help}\n\
             cUsedThought=10593\ncUsedThought=10594\n[EstheticBonus]\nv=9503\nv=8\nv=9504\nv=-4\n"
        )
    };
    let mut cat = Catalogue::default();
    cat.push(kind("building", "statue", &item(60, 70)));
    cat.push(kind("building", "twin", &item(30, 70)));
    cat.push(kind("building", "urn", &item(20, 71)));
    cat.push(kind("building", "bench", &item(0, 72)));
    cat.push(kind("objects", "stone", &item(20, 73)));
    cat
}

fn shape(e: &Entity) -> Option<Shape> {
    let fixed = Shape {
        kind: Kind::Fixed,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        walkable: (false, false),
        tank: TankKind::NONE,
    };
    match e.class.as_str() {
        "fences" => Some(Shape {
            kind: Kind::Edge,
            fence: true,
            wall: true,
            ..fixed
        }),
        "gate" => Some(Shape {
            footprint: (2, 2),
            gate: true,
            ..fixed
        }),
        "building" | "objects" => Some(fixed),
        _ => None,
    }
}

/// An 8 by 8 map whose entrance gate on (1, 4) faces west, with a zoo wall on the east edge of
/// column 4: only the columns 5 to 7 are inside. Statue 50 at (6, 1) and twin 51 at (6, 5) are
/// inside, urn 52 at (2, 1) outside, bench 53 at (7, 7) has no weight, stone 54 at (7, 2).
fn zoo_map() -> World {
    let mut zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: 8,
        height: 8,
        entrance: (1, 4),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 100,
        tiles: vec![
            Tile {
                elevation: 0,
                shape: 0,
                terrain: 0,
                edges: 0
            };
            64
        ],
        entities: vec![entity("gate", "", 1, (96, 288), 6)],
        view: View::default(),
        clocks: Default::default(),
        names: Default::default(),
    };
    zoo.entities.extend((0..8).map(|row| {
        entity(
            "fences",
            "",
            10 + row as u32,
            (4 * 64 + 63, row * 64 + 32),
            2,
        )
    }));
    for (class, name, id, at) in [
        ("building", "statue", 50, (6, 1)),
        ("building", "twin", 51, (6, 5)),
        ("building", "urn", 52, (2, 1)),
        ("building", "bench", 53, (7, 7)),
        ("objects", "stone", 54, (7, 2)),
    ] {
        zoo.entities.push(entity(class, name, id, centre(at), 0));
    }
    World::new(zoo, shape)
}

fn listed(w: &World) -> Vec<u32> {
    w.units.gawk.members().map(|j| j.id).collect()
}

#[test]
fn an_object_of_weight_inside_the_zoo_joins_the_list_when_the_map_changes() {
    let (cat, mut w) = (catalogue(), zoo_map());
    assert!(w.in_zoo(8 + 6) && !w.in_zoo(8 + 2));
    update(&mut w, &cat);
    // The urn stands outside, the bench has no weight.
    assert_eq!(listed(&w), [50, 51, 54]);
    // Another map change: a stone joins, and nothing is judged again.
    let ok = w.add(
        entity("objects", "stone", 0, centre((5, 6)), 0),
        shape(&entity("objects", "", 0, (0, 0), 0)),
    );
    assert!(ok.is_some());
    update(&mut w, &cat);
    assert_eq!(listed(&w).len(), 4);
    // A removed object leaves the list.
    w.remove(51);
    update(&mut w, &cat);
    assert_eq!(listed(&w).len(), 3);
    assert!(!listed(&w).contains(&51));
}

#[test]
fn the_seen_test_goes_by_the_object_or_its_help_id() {
    let (cat, mut w) = (catalogue(), zoo_map());
    update(&mut w, &cat);
    let tile = |x, y| w.tile_index(x, y).unwrap();
    let mut g = Guest::default();
    let all = |g: &Guest| unseen(&w, &cat, g).iter().map(|j| j.id).collect::<Vec<_>>();
    assert_eq!(all(&g), [50, 51, 54]);
    // Seeing the statue (a tile of the list) makes its twin of the same help id seen too.
    g.seen = vec![tile(6, 1)];
    assert_eq!(all(&g), [54]);
    // A tile that holds nothing makes nothing seen.
    g.seen = vec![tile(0, 0)];
    assert_eq!(all(&g), [50, 51, 54]);
    g.seen = vec![tile(7, 2)];
    assert_eq!(all(&g), [50, 51]);
}

#[test]
fn unseen_scenery_weighs_a_hundredth_of_its_weight_and_at_least_one_site() {
    let j = |weight| Judged {
        id: 1,
        member: true,
        weight,
        help: None,
        tile: 0,
    };
    assert_eq!(sites(&[]), 0);
    assert_eq!(sites(&[j(0)]), 0);
    assert_eq!(sites(&[j(-50)]), 0);
    // Three statues of weight 20 weigh 60: one site; twelve weigh 240: two.
    assert_eq!(sites(&[j(20), j(20), j(20)]), 1);
    assert_eq!(sites(&vec![j(20); 12]), 2);
    assert_eq!(sites(&[j(100), j(100)]), 2);
    assert_eq!(sites(&[j(i32::MAX), j(i32::MAX)]), 42_949_672);
}

#[test]
fn the_nearest_unseen_object_within_reach_wins_and_the_first_wins_a_tie() {
    let at = |x: usize, y: usize| y * 64 + x;
    let j = |id, weight, tile| Judged {
        id,
        member: true,
        weight,
        help: None,
        tile,
    };
    // The guest on (0, 0): statues of weight 20 at squared distances 40, 400 and 1200.
    let list = [j(3, 20, at(0, 34)), j(2, 20, at(20, 0)), j(1, 20, at(6, 2))];
    assert_eq!(pick(&list, at(0, 0), 64).map(|j| j.id), Some(1));
    assert_eq!(pick(&list[..2], at(0, 0), 64).map(|j| j.id), Some(2));
    assert_eq!(pick(&list[..1], at(0, 0), 64), None);
    // Equal scores: the first. (1200 - 400) / 200 = 4 for a distance of 400, 5 for 40.
    let tie = [j(5, 20, at(1, 0)), j(6, 20, at(0, 1))];
    assert_eq!(pick(&tie, at(0, 0), 64).map(|j| j.id), Some(5));
    // A heavy object farther away can win: (1000 + 1000 - 441) / 200 = 7 beats 5.
    let heavy = [j(7, 20, at(6, 2)), j(8, 80, at(21, 0))];
    assert_eq!(pick(&heavy, at(0, 0), 64).map(|j| j.id), Some(8));
}

#[test]
fn the_set_comes_from_the_esthetic_bonus_for_the_guest_type() {
    let cat = catalogue();
    let ty = cat.find("building", b"statue", b"").unwrap();
    // The man (9503) has 8, the woman (9504) -4, the boy lists nothing.
    assert_eq!(gawk_set(ty, 9503), "bGawkObject3");
    assert_eq!(gawk_set(ty, 9504), "bGawkObject1");
    assert_eq!(gawk_set(ty, 9505), "bGawkObject2");
    assert_eq!(gawk_set(ty, 1), "bGawkObject2");
    let bonus = |v: i32| {
        kind(
            "building",
            "x",
            &format!("[EstheticBonus]\nv=9503\nv={v}\n"),
        )
    };
    for (v, set) in [
        (0, 2),
        (5, 2),
        (6, 3),
        (10, 3),
        (11, 4),
        (15, 4),
        (16, 5),
        (25, 5),
        (26, 6),
        (100, 6),
        (101, 2),
        (i32::MAX, 2),
        (i32::MIN, 1),
    ] {
        assert_eq!(
            gawk_set(&bonus(v), 9503),
            format!("bGawkObject{set}"),
            "{v}"
        );
    }
    assert_eq!(thoughts(ty), [10593, 10594]);
    assert!(thoughts(cat.find("building", b"bench", b"").unwrap()).len() == 2);
    assert!(thoughts(&kind("building", "x", "")).is_empty());
}
