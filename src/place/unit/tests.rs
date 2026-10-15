//! The pending animal, its purchase, names, sell price and crate on synthetic maps.

use super::*;
use crate::config::Config;
use crate::world::TankKind;
use crate::zoo::{Common, Fence, Tile, View, Zoo};

const ZEBRA: usize = 0;
const FEMALE: usize = 1;
const DINO: usize = 2;
const ORCA: usize = 3;
const BARE: usize = 4;
const BIG: usize = 5;

fn ty(subtype: &str, name: &str, ini: &str) -> EntityType {
    EntityType {
        class: "animals",
        type_name: name.into(),
        subtype: subtype.into(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn catalogue() -> Catalogue {
    let zebra = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=800\n\
                 cInitialHappiness=75\ncMaxHits=120\n";
    let dino = "[Global]\nDefaultSubtype=m\n[Characteristics/Integers]\ncExpansionID=1\n\
                [m/Characteristics/Integers]\ncPurchaseCost=1000\n";
    let orca = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=9\n\
                cSwims=1\ncUnderwater=1\n";
    let mut cat = Catalogue::default();
    cat.push(ty("m", "zebra", zebra));
    cat.push(ty("f", "zebra", zebra));
    cat.push(ty("m", "trex", dino));
    cat.push(ty("m", "orca", orca));
    cat.push(ty(
        "m",
        "mule",
        "[Characteristics/Integers]\ncUseNumbersInName=0\n",
    ));
    let mut big = ty(
        "m",
        "stego",
        "[Characteristics/Integers]\ncPurchaseCost=1\n",
    );
    big.footprint = [2, 8, 1];
    cat.push(big);
    cat
}

fn animal(subtype: &str, tile: (i32, i32)) -> Entity {
    entity_at(
        "animals",
        "zebra",
        subtype,
        tile.0 * 64 + 32,
        tile.1 * 64 + 32,
        0,
    )
}

fn entity_at(class: &str, name: &str, subtype: &str, x: i32, y: i32, rotation: u32) -> Entity {
    let fence = matches!(class, "fences" | "tankwall").then_some(Fence {
        strength: 300,
        life: 10,
        timers: [0; 2],
        portal: false,
    });
    Entity {
        class: class.into(),
        type_name: name.into(),
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
            movement_mode: 0,
            animal: None,
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let (kind, fence) = match e.class.as_str() {
        "animals" => (Kind::Unit, false),
        "fences" => (Kind::Edge, true),
        "tankwall" => (Kind::Wall, true),
        "objects" => (Kind::Fixed, false),
        _ => return None,
    };
    Some(Shape {
        kind,
        footprint: if kind == Kind::Fixed { (2, 2) } else { (1, 1) },
        fence,
        wall: false,
        gate: false,
        gated: fence,
        // A rock: walkable by tall units only.
        walkable: (false, kind == Kind::Fixed),
        tank: TankKind {
            wall: e.class == "tankwall",
            ..TankKind::NONE
        },
    })
}

/// The four sides of tile (`tx`, `ty`), of `class`.
fn square(class: &str, tx: i32, ty: i32) -> Vec<Entity> {
    [(32, 0), (63, 32), (32, 63), (0, 32)]
        .iter()
        .enumerate()
        .map(|(s, &(x, y))| entity_at(class, "iron", "f", tx * 64 + x, ty * 64 + y, 2 * s as u32))
        .collect()
}

/// An 8 by 8 map of flat tiles, tile (5, 5) of water.
fn world(mut entities: Vec<Entity>) -> World {
    for (i, e) in entities.iter_mut().enumerate() {
        e.common.as_mut().unwrap().id = i as u32 + 1;
    }
    let flat = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let mut tiles = vec![flat; 64];
    tiles[5 * 8 + 5].terrain = 1;
    World::new(
        Zoo {
            version: 0,
            locale: 0,
            expansion: 0,
            width: 8,
            height: 8,
            entrance: (0, 0),
            habitat_tiles: Vec::new(),
            habitat_gates: Vec::new(),
            tanks: Vec::new(),
            id_counter: 1000,
            tiles,
            entities,
            view: View::default(),
            clocks: Default::default(),
            names: Default::default(),
        },
        shape,
    )
}

/// The pending animal of the type for the middle of tile (`tx`, `ty`), October or not.
fn at(w: &World, ty: usize, (tx, tyy): (i64, i64)) -> Pending {
    let point = (tx * 64 + 32, tyy * 64 + 32);
    pending(w, &catalogue(), &[false, true], ty, 0, point, false).unwrap()
}

#[test]
fn an_animal_costs_its_price_and_half_in_october_when_it_is_from_dinosaur_digs() {
    let cat = catalogue();
    assert_eq!(price(&cat.types[ZEBRA], true), 800.0);
    assert_eq!(
        price(&cat.types[FEMALE], false),
        800.0,
        "the default subtype's cost"
    );
    assert_eq!(price(&cat.types[DINO], false), 1000.0);
    assert_eq!(price(&cat.types[DINO], true), 500.0);
    assert_eq!(price(&ty("m", "x", ""), true), 0.0, "no cost, no price");
}

#[test]
fn the_sell_price_follows_the_worked_examples() {
    // Cost 800, 120 hit points: unit placing 12, example 3.
    assert_eq!(sell_price(800.0, 75.0, 120.0, 120.0, false), 600.0);
    assert_eq!(sell_price(800.0, 30.0, 120.0, 120.0, false), 528.0);
    assert_eq!(sell_price(800.0, 0.0, 60.0, 120.0, false), 320.0);
    assert_eq!(sell_price(800.0, -100.0, 0.0, 120.0, true), 0.0);
    assert_eq!(sell_price(800.0, -100.0, 120.0, 120.0, true), 160.0);
    // A new zebra is as happy as its type says and has all its hit points.
    assert_eq!(new_sell_price(&catalogue().types[ZEBRA]), 600.0);
    // A type with neither key: 60 happiness and 500 hit points.
    assert_eq!(
        new_sell_price(&ty(
            "m",
            "x",
            "[Characteristics/Integers]\ncPurchaseCost=100\n"
        )),
        72.0
    );
}

#[test]
fn an_animal_is_valid_in_an_exhibit_and_refused_on_the_grounds() {
    let w = world(square("fences", 2, 2));
    let inside = at(&w, ZEBRA, (2, 2));
    assert_eq!(
        (inside.verdict, inside.message, inside.price),
        (Verdict::Valid, None, 800.0)
    );
    assert_eq!(inside.facing, facing(0, 0));
    assert_eq!(inside.amount(), -800);
    let grounds = at(&w, ZEBRA, (4, 4));
    assert_eq!(
        (grounds.verdict, grounds.message),
        (Verdict::Invalid, Some(IN_AN_EXHIBIT))
    );
    assert_eq!(grounds.amount(), 0);
    // An underwater species asks for a tank wherever it is.
    assert_eq!(at(&w, ORCA, (4, 4)).message, Some(NEEDS_WATER));
    assert_eq!(at(&w, ORCA, (2, 2)).message, Some(NEEDS_WATER));
    // Water is refused unless the animal swims (and then for being outside a tank).
    assert_eq!(at(&w, ZEBRA, (5, 5)).message, Some(10569));
    assert_eq!(at(&w, ORCA, (5, 5)).message, Some(NEEDS_WATER));
    // Off the map and an unknown type give nothing.
    let cat = catalogue();
    assert_eq!(
        pending(&w, &cat, &[false, true], ZEBRA, 0, (-1, 5), false),
        None
    );
    assert_eq!(
        pending(&w, &cat, &[false, true], 99, 0, (5, 5), false),
        None
    );
    assert_eq!(
        pending(&w, &cat, &[false, true], ZEBRA, 0, (i64::MAX, 5), false),
        None
    );
}

#[test]
fn another_unit_on_the_tile_is_in_the_way_and_a_neighbour_is_not() {
    let mut pieces = square("fences", 2, 2);
    pieces.push(animal("m", (2, 2)));
    let w = world(pieces);
    let over = at(&w, ZEBRA, (2, 2));
    assert_eq!(
        (over.verdict, over.message),
        (Verdict::Invalid, Some(super::super::IN_THE_WAY))
    );
    assert_eq!(over.blocking, [4]);
}

#[test]
fn a_tank_tile_says_why_the_animal_cannot_go_there() {
    let w = world(square("tankwall", 2, 2));
    assert!(w.tank_at(2 * 8 + 2).is_some());
    // A land animal does not swim in tanks; an underwater one needs water in it.
    let land = at(&w, ZEBRA, (2, 2));
    assert_eq!(
        (land.verdict, land.message),
        (Verdict::Invalid, Some(10553))
    );
    assert_eq!(at(&w, ORCA, (2, 2)).message, Some(NEEDS_WATER));
}

#[test]
fn an_animal_is_bought_booked_numbered_and_one_that_is_refused_costs_nothing() {
    let cat = catalogue();
    let mut w = world(square("fences", 2, 2));
    let mut money = Money::default();
    money.set_cash(1000.0);
    let zebra = &cat.types[ZEBRA];
    let p = at(&w, ZEBRA, (2, 2));
    let placed = place(&mut w, &mut money, zebra, "Test Zebra", &p, shape);
    let Outcome::Placed(id) = placed else {
        panic!("{placed:?}")
    };
    assert_eq!(money.bar().0, 200);
    assert_eq!(money.monthly[ANIMAL_PURCHASE][0], 800.0);
    let name = |w: &World, id| {
        w.zoo.entities[w.find(id).unwrap()]
            .common
            .as_ref()
            .unwrap()
            .name
            .clone()
    };
    assert_eq!(name(&w, id), "Test Zebra 1");
    assert_eq!(w.units.len(), 1, "it joined the unit table");
    // The cash does not cover a second one; the grounds refuse with their message.
    let p = at(&w, FEMALE, (2, 2));
    assert_eq!(
        place(
            &mut w,
            &mut money,
            &cat.types[FEMALE],
            "Test Zebra",
            &p,
            shape
        ),
        Outcome::Refused(Some(10555))
    );
    let mut p = at(&w, FEMALE, (4, 4));
    assert_eq!(
        place(
            &mut w,
            &mut money,
            &cat.types[FEMALE],
            "Test Zebra",
            &p,
            shape
        ),
        Outcome::Refused(Some(IN_AN_EXHIBIT))
    );
    p.verdict = Verdict::Valid;
    assert_eq!(
        place(
            &mut w,
            &mut money,
            &cat.types[FEMALE],
            "Test Zebra",
            &p,
            shape
        ),
        Outcome::Refused(Some(NO_FUNDS))
    );
    assert_eq!((money.bar().0, w.units.len()), (200, 1));
    // An undo takes the animal away and gives its price back.
    let done = w.undo();
    assert_eq!(done.steps.len(), 1);
    assert_eq!(w.units.len(), 0);
}

#[test]
fn the_counter_is_per_type_name_shared_by_both_sexes_and_kept_with_the_zoo() {
    let cat = catalogue();
    let mut w = world(vec![]);
    w.zoo.names.insert("zebra".into(), 4);
    assert_eq!(
        name(&mut w, &cat.types[ZEBRA], "Test Zebra"),
        "Test Zebra 5"
    );
    assert_eq!(
        name(&mut w, &cat.types[FEMALE], "Test Zebra"),
        "Test Zebra 6"
    );
    assert_eq!(name(&mut w, &cat.types[ORCA], "Orca"), "Orca 1");
    assert_eq!(w.zoo.names["zebra"], 6);
    assert_eq!(w.zoo.names["orca"], 1);
    // A type that does not number its names keeps the plain one.
    assert_eq!(name(&mut w, &cat.types[BARE], "Mule"), "Mule");
    assert!(!w.zoo.names.contains_key("Mule"));
    w.zoo.names.insert("zebra".into(), u32::MAX);
    name(&mut w, &cat.types[ZEBRA], "Test Zebra");
}

#[test]
fn the_crate_goes_to_the_best_free_tile_of_the_grounds_three_or_more_tiles_away() {
    let w = world(vec![]);
    let width = 8;
    // From the corner tile (0, 0) the first ring is at distance 3 and the first tile on the map
    // (dx = 0, dy = 3) scores 3.
    assert_eq!(crate_spot(&w, 0), Some(3 * width));
    // A unit on that tile pushes the choice on to the next of the ring, (1, 2).
    let w = world(vec![animal("m", (0, 3))]);
    assert_eq!(crate_spot(&w, 0), Some(2 * width + 1));
    // Nothing within reach: a map of tiles that all hold something.
    let tiny = world(
        (0..8)
            .flat_map(|y| (0..8).map(move |x| animal("m", (x, y))))
            .collect(),
    );
    assert_eq!(crate_spot(&tiny, 0), None);
}

#[test]
fn only_walkable_lets_a_tile_through_not_walkable_by_tall() {
    let mut pieces = square("fences", 2, 2);
    pieces.push(entity_at(
        "objects",
        "rock",
        "m",
        2 * 64 + 32,
        2 * 64 + 32,
        0,
    ));
    let w = world(pieces);
    let over = at(&w, ZEBRA, (2, 2));
    assert_eq!(
        (over.verdict, over.message),
        (Verdict::Invalid, Some(super::super::IN_THE_WAY))
    );
    assert_eq!(over.blocking, [4]);
}

#[test]
fn a_large_animal_runs_the_footprint_check_and_needs_room_inside_the_fences() {
    let w = world(square("fences", 2, 2));
    let big = at(&w, BIG, (2, 2));
    assert_eq!(big.verdict, Verdict::Invalid, "its body crosses the fences");
    // Inside a pen of 4 x 4 tiles it fits, and a rock under its body is in the way.
    let pen = || -> Vec<Entity> {
        (1..=4)
            .flat_map(|i| {
                [
                    entity_at("fences", "iron", "f", i * 64 + 32, 64, 0),
                    entity_at("fences", "iron", "f", 5 * 64 - 1, i * 64 + 32, 2),
                    entity_at("fences", "iron", "f", i * 64 + 32, 5 * 64 - 1, 4),
                    entity_at("fences", "iron", "f", 64, i * 64 + 32, 6),
                ]
            })
            .collect()
    };
    let w = world(pen());
    let cat = catalogue();
    let point = (2 * 64 + 32, 2 * 64 + 32);
    let free = pending(&w, &cat, &[false, true], BIG, 0, point, false).unwrap();
    assert_eq!((free.verdict, free.message), (Verdict::Valid, None));
    let mut pieces = pen();
    let first = pieces.len();
    pieces.push(entity_at("objects", "rock", "m", free.x, free.y, 0));
    let w = world(pieces);
    let held = pending(&w, &cat, &[false, true], BIG, 0, point, false).unwrap();
    assert_eq!(held.verdict, Verdict::Invalid);
    assert!(held.blocking.contains(&first));
}
