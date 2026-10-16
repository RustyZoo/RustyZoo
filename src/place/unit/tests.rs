//! The pending animal or staff member, its purchase, names, sell price and crate on synthetic
//! maps.

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
const KEEPER: usize = 6;
const MAINT: usize = 7;
const GUIDE: usize = 8;
const HIPPO: usize = 9;
const MARINE: usize = 10;
const GUEST: usize = 11;

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

fn staff(class: &'static str, name: &str, cost: u32) -> EntityType {
    let ini = format!("[m/Characteristics/Integers]\ncPurchaseCost={cost}\n");
    EntityType {
        class,
        ..ty("m", name, &ini)
    }
}

fn catalogue() -> Catalogue {
    let zebra = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=800\n\
                 cInitialHappiness=75\ncMaxHits=120\n";
    let dino = "[Global]\nDefaultSubtype=m\n[Characteristics/Integers]\ncExpansionID=1\n\
                [m/Characteristics/Integers]\ncPurchaseCost=1000\n";
    let orca = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=9\n\
                cSwims=1\ncUnderwater=1\ncOnlyUnderwater=1\n";
    let hippo = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=9\n\
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
    cat.push(staff("keeper", "keeper", 800));
    cat.push(staff("maint", "maint", 300));
    cat.push(staff("tour", "tour", 500));
    cat.push(ty("m", "hippo", hippo));
    cat.push(EntityType {
        class: "keeper",
        ..ty("m", "marine", hippo)
    });
    cat.push(EntityType {
        class: "guests",
        ..ty("m", "guest", "")
    });
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
        "animals" | "keeper" | "maint" | "tour" | "guests" => (Kind::Unit, false),
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
fn world(entities: Vec<Entity>) -> World {
    world_with(entities, |_| ())
}

/// The same, the zoo edited before the world is made.
fn world_with(mut entities: Vec<Entity>, edit: impl FnOnce(&mut Zoo)) -> World {
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
    let mut zoo = Zoo {
        version: 0,
        locale: 0,
        expansion: 0,
        width: 8,
        height: 8,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 1000,
        tiles,
        entities,
        view: View::default(),
        clocks: Default::default(),
        names: Default::default(),
    };
    edit(&mut zoo);
    World::new(zoo, shape)
}

/// The pointer over the ground point `p` (with its height) seen from `view` at normal zoom: its
/// own row is the row of the point.
fn aim(view: u8, p: (i64, i64, i32)) -> Aim {
    let to = |n: i64| i32::try_from(n).unwrap_or(i32::MAX);
    Aim {
        point: (p.0, p.1),
        row: i64::from(project(view, to(p.0), to(p.1), p.2).1),
        zoom: 0,
    }
}

/// The pending animal of the type for the middle of tile (`tx`, `ty`), October or not.
fn at(w: &World, ty: usize, (tx, tyy): (i64, i64)) -> Pending {
    let point = (tx * 64 + 32, tyy * 64 + 32);
    pending(
        w,
        &catalogue(),
        &[false, true],
        ty,
        0,
        aim(0, (point.0, point.1, 0)),
        false,
    )
    .unwrap()
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
    // An amphibious animal (`cUnderwater` only) is a land animal here: the exhibit message.
    assert_eq!(at(&w, HIPPO, (4, 4)).message, Some(IN_AN_EXHIBIT));
    assert_eq!(at(&w, HIPPO, (2, 2)).message, None);
    // Water is refused unless the animal swims (and then for being outside a tank).
    assert_eq!(at(&w, ZEBRA, (5, 5)).message, Some(10569));
    assert_eq!(at(&w, ORCA, (5, 5)).message, Some(NEEDS_WATER));
    // Off the map and an unknown type give nothing.
    let cat = catalogue();
    assert_eq!(
        pending(
            &w,
            &cat,
            &[false, true],
            ZEBRA,
            0,
            aim(0, (-1, 5, 0)),
            false
        ),
        None
    );
    assert_eq!(
        pending(&w, &cat, &[false, true], 99, 0, aim(0, (5, 5, 0)), false),
        None
    );
    assert_eq!(
        pending(
            &w,
            &cat,
            &[false, true],
            ZEBRA,
            0,
            aim(0, (i64::MAX, 5, 0)),
            false
        ),
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
    let free = pending(
        &w,
        &cat,
        &[false, true],
        BIG,
        0,
        aim(0, (point.0, point.1, 0)),
        false,
    )
    .unwrap();
    assert_eq!((free.verdict, free.message), (Verdict::Valid, None));
    let mut pieces = pen();
    let first = pieces.len();
    pieces.push(entity_at("objects", "rock", "m", free.x, free.y, 0));
    let w = world(pieces);
    let held = pending(
        &w,
        &cat,
        &[false, true],
        BIG,
        0,
        aim(0, (point.0, point.1, 0)),
        false,
    )
    .unwrap();
    assert_eq!(held.verdict, Verdict::Invalid);
    assert!(held.blocking.contains(&first));
}

#[test]
fn a_staff_member_stands_anywhere_a_unit_can_but_in_water_and_gives_no_message() {
    let mut pieces = square("fences", 2, 2);
    pieces.push(entity_at(
        "keeper",
        "keeper",
        "m",
        4 * 64 + 32,
        4 * 64 + 32,
        0,
    ));
    let w = world(pieces);
    // The grounds and an exhibit are both fine, and the price is `cPurchaseCost`.
    for tile in [(4, 3), (2, 2)] {
        let p = at(&w, KEEPER, tile);
        assert_eq!(
            (p.verdict, p.message, p.price),
            (Verdict::Valid, None, 800.0),
            "{tile:?}"
        );
    }
    assert_eq!(at(&w, MAINT, (4, 3)).price, 300.0);
    // A marine specialist (`cUnderwater` 1, no `cOnlyUnderwater`) is valid on land.
    let p = at(&w, MARINE, (4, 3));
    assert_eq!((p.verdict, p.message), (Verdict::Valid, None));
    // Water is refused without a word, and another unit gets in the way.
    let wet = at(&w, GUIDE, (5, 5));
    assert_eq!((wet.verdict, wet.message), (Verdict::Invalid, None));
    let over = at(&w, GUIDE, (4, 4));
    assert_eq!(
        (over.verdict, over.message),
        (Verdict::Invalid, Some(super::super::IN_THE_WAY))
    );
    // A staff member that does not swim is refused in a tank, with the staff message.
    let tank = world(square("tankwall", 2, 2));
    let p = at(&tank, KEEPER, (2, 2));
    assert_eq!((p.verdict, p.message), (Verdict::Invalid, Some(10563)));
}

#[test]
fn hiring_books_the_wages_of_the_class_names_the_member_and_empties_the_undo_list() {
    let cat = catalogue();
    let mut w = world(square("fences", 2, 2));
    let mut money = Money::default();
    money.set_cash(1700.0);
    let mut hire = |w: &mut World, ty: usize, tile| {
        let p = at(w, ty, tile);
        place(w, &mut money, &cat.types[ty], "Staff", &p, shape)
    };
    assert!(matches!(hire(&mut w, KEEPER, (4, 3)), Outcome::Placed(_)));
    assert!(matches!(hire(&mut w, GUIDE, (3, 3)), Outcome::Placed(_)));
    assert!(matches!(hire(&mut w, MAINT, (6, 6)), Outcome::Placed(_)));
    // 100 are left: a keeper is too dear.
    let dear = hire(&mut w, KEEPER, (3, 4));
    assert_eq!(dear, Outcome::Refused(Some(NO_FUNDS)));
    assert_eq!(money.bar().0, 1700 - 800 - 500 - 300);
    for (category, amount) in [(11, 800.0), (12, 500.0), (13, 300.0), (2, 0.0)] {
        assert_eq!(money.monthly[category][0], amount, "{category}");
    }
    let names: Vec<_> = (w.zoo.entities.iter())
        .filter_map(|e| e.common.as_ref())
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        &names[4..],
        ["Staff 1", "Staff 1", "Staff 1"],
        "one counter per type name"
    );
    assert_eq!(w.units.len(), 3);
    assert!(!w.can_undo(), "hiring clears the undo list");
}

/// A tank of the tiles (2, 2) and (3, 2) with a gate on the top side of the first, water to
/// `level`, filled.
fn tank_world(level: u32) -> World {
    tank_world_with(level, Vec::new())
}

/// The same with more entities in the world.
fn tank_world_with(level: u32, more: Vec<Entity>) -> World {
    let wall = |x: i32, y: i32, d: u32| entity_at("tankwall", "iron", "f", x, y, d);
    let mut pieces = more;
    for tx in [2, 3] {
        pieces.push(wall(tx * 64 + 32, 2 * 64, 0));
        pieces.push(wall(tx * 64 + 32, 3 * 64 - 1, 4));
    }
    pieces.push(wall(2 * 64, 2 * 64 + 32, 6));
    pieces.push(wall(4 * 64 - 1, 2 * 64 + 32, 2));
    let mut w = world_with(pieces, |z| {
        z.habitat_tiles = vec![(2, 2)];
        z.habitat_gates = vec![((2, 2), 0)];
        z.habitat_info = vec![crate::zoo::HabitatInfo {
            name: String::new(),
            stats: [0; 9],
            created: 0,
        }];
    });
    let id = w.tank_at(2 * 8 + 2).unwrap();
    let tank = w.exhibits_mut().get_mut(id).unwrap().tank.as_mut().unwrap();
    (tank.level, tank.filled) = (level, true);
    w
}

#[test]
fn the_first_row_of_the_tank_check_that_applies_gives_the_message() {
    // Any unit but a keeper on the tile inside the gate: the ladder row comes before 10563.
    let w = tank_world(3);
    assert_eq!(w.gate_slot(1).map(|(t, _)| t), Some(2 * 8 + 2));
    assert_eq!(at(&w, MAINT, (2, 2)).message, Some(LADDER));
    assert_eq!(at(&w, GUEST, (2, 2)).message, Some(LADDER));
    // A keeper may stand there, and then the next row applies; the marine specialist swims.
    assert_eq!(at(&w, KEEPER, (2, 2)).message, Some(NOT_FOR_STAFF));
    assert_eq!(at(&w, MARINE, (2, 2)).message, None);
    // Elsewhere in the tank: a land guest, an underwater animal in a dry tank.
    assert_eq!(at(&w, GUEST, (3, 2)).message, Some(NOT_IN_TANKS));
    assert_eq!(at(&tank_world(0), ORCA, (3, 2)).message, Some(NEEDS_WATER));
}

/// The ground point under the pointer for the middle of tile (3, 2) seen from view 0 with the
/// water at `level`: 32 world units further away from the viewer for each level (unit placing
/// 5.1).
fn behind_the_tank(level: i64) -> (i64, i64, i32) {
    (224 + 32 * level, 160 - 32 * level, 0)
}

#[test]
fn the_tank_point_lifts_a_swimmer_to_the_water_under_the_pointer() {
    let (cat, wet) = (catalogue(), [false, true]);
    let w = tank_world(3);
    let over = |ty, point| pending(&w, &cat, &wet, ty, 0, aim(0, point), false).unwrap();
    // The ground point is three tiles away, the surface is where the pointer is.
    let p = over(HIPPO, behind_the_tank(3));
    assert_eq!(
        (p.tile, p.verdict, p.message),
        ((3, 2), Verdict::Valid, None)
    );
    assert_eq!(p.z, 48, "three levels of 16");
    // A land animal there stays on the ground, outside every exhibit.
    let z = over(ZEBRA, behind_the_tank(3));
    assert_eq!((z.tile, z.message), ((5, 1), Some(IN_AN_EXHIBIT)));
    // The row of another depth does not meet the tank, and the point itself is the floor.
    assert_eq!(over(HIPPO, behind_the_tank(1)).tile, (4, 2));
    let floor = over(HIPPO, (224, 160, 0));
    assert_eq!(
        (floor.tile, floor.z, floor.verdict),
        ((3, 2), 0, Verdict::Invalid)
    );
    // A guest is lifted too, and told that it does not swim in tanks.
    let g = over(GUEST, behind_the_tank(3));
    assert_eq!((g.tile, g.message), ((3, 2), Some(NOT_IN_TANKS)));
    // A level that no tank has, or a point at the end of the map, give nothing odd and no panic.
    let deep = tank_world(u32::MAX);
    assert!(
        pending(
            &deep,
            &cat,
            &wet,
            HIPPO,
            0,
            aim(0, behind_the_tank(3)),
            false
        )
        .is_some()
    );
    for far in [(i64::from(i32::MAX), 0, i32::MAX), (-9, -9, i32::MIN)] {
        assert!(
            pending(&w, &cat, &wet, GUEST, 0, aim(0, far), false).is_none_or(|p| p.tile.0 >= -1)
        );
    }
    // Viewed from the other side the steps go the other way.
    let turned = pending(
        &w,
        &cat,
        &wet,
        HIPPO,
        2,
        aim(2, (224 - 96, 160 + 96, 0)),
        false,
    )
    .unwrap();
    assert_eq!((turned.tile, turned.verdict), ((3, 2), Verdict::Valid));
}

#[test]
fn the_tank_point_keeps_the_step_whose_floor_is_level_steps_below_the_pointer() {
    let (cat, wet) = (catalogue(), [false, true]);
    let w = tank_world(3);
    // The floor of step 3 is 48 rows below the ground point, which is the pointer's row.
    let flat = aim(0, behind_the_tank(3));
    let tile = |row: i64, zoom: i32| {
        let aim = Aim { row, zoom, ..flat };
        pending(&w, &cat, &wet, HIPPO, 0, aim, false).unwrap().tile
    };
    // A pointer up to 15 rows above the ground point still keeps the step (the pick can stop up to
    // 4 height units off the ground).
    for up in [0, 12, 15] {
        assert_eq!(tile(flat.row - up, 0), (3, 2), "{up} rows above");
    }
    // The band is one-sided: a row below the surface, or a whole step above it, gives 2 or 4.
    for off in [1, -16, 64] {
        assert_ne!(tile(flat.row + off, 0), (3, 2), "{off} rows");
    }
    // The zoom scales the rows and the step alike.
    assert_eq!(tile(flat.row * 2, 2), (3, 2));
    assert_eq!(tile(flat.row / 2, -2), (3, 2));
    for zoom in [i32::MIN, i32::MAX, -1, 1] {
        tile(flat.row, zoom);
    }
}

#[test]
fn a_unit_in_the_way_in_a_tank_gets_10555() {
    let guest = entity_at("guests", "guest", "m", 3 * 64 + 32, 2 * 64 + 32, 0);
    let w = tank_world_with(3, vec![guest]);
    // A guest over a tank tile where another unit's rectangle meets its own: the unit is in the
    // way and highlighted, not the tank refusal.
    let tank = Aim {
        zoom: 0,
        ..aim(0, behind_the_tank(3))
    };
    let (cat, wet) = (catalogue(), [false, true]);
    let p = pending(&w, &cat, &wet, GUEST, 0, tank, false).unwrap();
    assert_eq!(
        (p.tile, p.verdict, p.message, p.blocking.len()),
        ((3, 2), Verdict::Invalid, Some(super::super::IN_THE_WAY), 1)
    );
}

#[test]
fn a_carried_animal_counts_in_no_exhibit() {
    let cat = catalogue();
    let mut pieces = square("fences", 2, 2);
    pieces.push(animal("m", (2, 2)));
    let mut w = world(pieces);
    let id = w.zoo.entities.last().unwrap().common.as_ref().unwrap().id;
    let exhibit = w.exhibits().region(2 * 8 + 2).unwrap();
    let counted = |w: &World| {
        (
            crate::exhibit::rating::input(w, &cat, exhibit)
                .animals
                .len(),
            crate::panel::exhibit::animals_in(w, exhibit).len(),
        )
    };
    assert_eq!(counted(&w), (1, 1));
    pick_up(&mut w, &cat, id, 0).unwrap();
    assert_eq!(counted(&w), (0, 0));
    let (x, y) = (2 * 64 + 32, 2 * 64 + 32);
    assert!(put_down(&mut w, &cat.types[ZEBRA], id, (x, y, 0), 0));
    assert_eq!(counted(&w), (1, 1));
}

#[test]
fn a_guest_stands_on_any_empty_flat_tile_even_water_and_never_in_the_way() {
    let mut pieces = square("fences", 2, 2);
    pieces.push(entity_at(
        "objects",
        "rock",
        "m",
        4 * 64 + 16,
        4 * 64 + 16,
        0,
    ));
    pieces.push(animal("m", (3, 3)));
    let mut w = world(pieces);
    w.zoo.tiles[1].shape = 0x11;
    let guest = |w: &World, tile| at(w, GUEST, tile);
    for tile in [(0, 3), (2, 2), (5, 5)] {
        assert_eq!(guest(&w, tile).verdict, Verdict::Valid, "{tile:?}");
    }
    // A quarter held by an object and a unit on the tile are in the way; a steep tile refuses
    // quietly.
    for tile in [(4, 4), (3, 3)] {
        let p = guest(&w, tile);
        assert_eq!(
            (p.verdict, p.message),
            (Verdict::Invalid, Some(super::super::IN_THE_WAY))
        );
    }
    assert_eq!(guest(&w, (1, 0)).message, None);
    assert_eq!(guest(&w, (1, 0)).verdict, Verdict::Invalid);
}

#[test]
fn a_unit_is_picked_up_off_the_map_and_put_down_in_the_tank_mode_of_its_tile() {
    let cat = catalogue();
    let mut w = tank_world(3);
    let mut diver = entity_at("keeper", "marine", "m", 6 * 64 + 20, 6 * 64 + 20, 2);
    diver.common.as_mut().unwrap().movement_mode = 3;
    let id = w
        .add(diver, shape(&entity_at("keeper", "marine", "m", 0, 0, 0)))
        .unwrap();
    let wet = [false, true];
    let (tile, tank_tile) = (6 * 8 + 6, 2 * 8 + 3);
    assert_eq!(w.units_on(tile).count(), 1);
    let carry = pick_up(&mut w, &cat, id, 0).unwrap();
    assert_eq!(
        (carry.ty, carry.from, carry.facing, carry.view),
        (MARINE, (6 * 64 + 20, 6 * 64 + 20, 0), 2, 0)
    );
    // It is off the map, suspended, stopped, dry, and cannot be picked up twice.
    assert_eq!((w.units_on(tile).count(), w.unit_on(tile)), (0, false));
    let unit = w.units.get(id).unwrap();
    assert!(unit.carried && unit.suspended() && !unit.moving);
    let mode = |w: &World| {
        w.zoo
            .entities
            .last()
            .unwrap()
            .common
            .as_ref()
            .unwrap()
            .movement_mode
    };
    assert_eq!(mode(&w), 0);
    assert_eq!(pick_up(&mut w, &cat, id, 0), None);
    assert_eq!(pick_up(&mut w, &cat, 99, 0), None);
    // It keeps its screen facing when the view turns, costs nothing and does not block itself.
    let own = (6 * 64 + 20, 6 * 64 + 20, 0);
    let wild = Carry {
        facing: u32::MAX,
        ..carry.clone()
    };
    assert!(carried(&w, &cat, &wet, &wild, 3, aim(3, own)).is_some());
    let free = carried(&w, &cat, &wet, &carry, 1, aim(1, own)).unwrap();
    assert_eq!(
        (free.facing, free.price, free.verdict),
        (0, 0.0, Verdict::Valid)
    );
    // Over the tank it lands at the surface in tank mode 3; the tank is marked for a new rating.
    let p = carried(&w, &cat, &wet, &carry, 0, aim(0, behind_the_tank(3))).unwrap();
    assert_eq!((p.tile, p.verdict), ((3, 2), Verdict::Valid));
    let tank = w.tank_at(tank_tile).unwrap();
    w.exhibits_mut().get_mut(tank).unwrap().rating.dirty = false;
    assert!(put_down(
        &mut w,
        &cat.types[MARINE],
        id,
        (p.x, p.y, p.z),
        p.facing
    ));
    let c = w.zoo.entities.last().unwrap().common.as_ref().unwrap();
    assert_eq!(
        (c.x, c.y, c.z, c.rotation, c.movement_mode),
        (p.x, p.y, 48, 2, 3)
    );
    assert_eq!(w.units_on(tank_tile).count(), 1);
    assert!(!w.units.get(id).unwrap().carried);
    assert!(w.exhibits().get(tank).unwrap().rating.dirty);
    // Not carried, or gone: nothing to put down.
    assert!(!put_down(
        &mut w,
        &cat.types[MARINE],
        id,
        (p.x, p.y, p.z),
        0
    ));
    assert!(!put_down(
        &mut w,
        &cat.types[MARINE],
        99,
        (p.x, p.y, p.z),
        0
    ));
    // A type that is not moveable is not picked up.
    let mut still = catalogue();
    still.types[MARINE].config = Some(std::sync::Arc::new(
        Config::parse(b"[m/Characteristics/Integers]\ncMoveable=0\n").unwrap(),
    ));
    assert_eq!(pick_up(&mut w, &still, id, 0), None);
}
