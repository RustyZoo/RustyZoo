//! The pending animal or staff member, its purchase, names, sell price and crate on synthetic
//! maps.

use super::*;
use crate::behaviour::NoData;
use crate::config::Config;
use crate::fixtures;
use crate::sim::Rng;
use crate::world::TankKind;
use crate::zoo::{Common, Fence, Tile, Zoo};

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
/// A swimmer that keeps off the walls.
const RAY: usize = 12;
/// The young of a species that lays eggs, a swimmer, and an adult of it.
const YOUNG: usize = 13;
const ADULT: usize = 14;
const ADULT_F: usize = 15;
/// A species that lays eggs and has no young type.
const LONE: usize = 16;
/// A tall swimmer.
const TALL: usize = 17;

fn ty(subtype: &str, name: &str, ini: &str) -> EntityType {
    fixtures::ty("animals", name, subtype, ini)
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
    let ray = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncSwims=1\ncUnderwater=1\n\
               cAvoidEdges=1\n";
    cat.push(ty("m", "ray", ray));
    let trike = "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncPurchaseCost=500\n\
                 cLaysEggs=1\ncSwims=1\ncUnderwater=1\n";
    cat.push(ty("y", "trike", trike));
    cat.push(ty("m", "trike", trike));
    cat.push(ty("f", "trike", trike));
    cat.push(ty("m", "lone", trike));
    cat.push(ty(
        "m",
        "plesio",
        "[Global]\nDefaultSubtype=m\n[m/Characteristics/Integers]\ncSwims=1\ncUnderwater=1\ncTall=1\n",
    ));
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
        open: false,
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
            ..Default::default()
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let (kind, fence) = match e.class.as_str() {
        "animals" | "keeper" | "maint" | "tour" | "guests" => (Kind::Unit, false),
        "fences" => (Kind::Edge, true),
        "tankwall" => (Kind::Wall, true),
        "objects" if e.type_name == "sparkle" => (Kind::Sparkle, false),
        "objects" => (Kind::Fixed, false),
        _ => return None,
    };
    Some(Shape {
        kind,
        footprint: match (kind, e.type_name.as_str()) {
            (_, "pad") => (1, 1),
            (Kind::Fixed, _) => (2, 2),
            _ => (1, 1),
        },
        fence,
        wall: false,
        gate: false,
        gated: fence,
        // A rock: walkable by tall units only.
        walkable: (kind == Kind::Sparkle, kind == Kind::Fixed),
        forms: (e.type_name == "gnu").then_some([(3, 3), (4, 4)]),
        rubbleable: false,
        tank: TankKind {
            wall: e.class == "tankwall",
            // A pillar stands 5 levels high, a lily or a pad floats at the surface.
            height: if e.type_name == "pillar" { 5 } else { 0 },
            surface: matches!(e.type_name.as_str(), "lily" | "pad"),
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
        width: 8,
        height: 8,
        id_counter: 1000,
        tiles,
        entities,
        ..Default::default()
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
    // Every operation rounds to f32: 331.19998, where one rounding at the end gives 331.2.
    assert_eq!(sell_price(800.0, -93.0, 120.0, 120.0, false), 331.19998);
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
fn a_crate_in_the_way_of_a_new_animal_overlaps_with_its_crate_footprint() {
    // The units the new animal meets on its tile: the half sides of the gnu's rectangle.
    let sides = |egg: bool, crated: bool| {
        let gnu = entity_at("animals", "gnu", "m", 2 * 64 + 50, 2 * 64 + 16, 0);
        let mut w = world(vec![gnu]);
        let Some(crate::unit::Part::Animal(a)) = w.units.get_mut(1).map(|u| &mut u.part) else {
            panic!("not an animal");
        };
        (a.egg, a.crated) = (egg, crated);
        let found = units_on(&w, &[w.tile_index(2, 2).unwrap()]);
        found[0].3
    };
    assert_eq!(sides(false, false), (16, 16));
    assert_eq!(sides(false, true), (64, 64));
    assert_eq!(sides(true, false), (48, 48));
    assert_eq!(sides(true, true), (48, 48));
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
    let placed = place(
        &mut w,
        &mut money,
        zebra,
        "Test Zebra",
        &p,
        shape,
        (None, &NoData),
    );
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
            shape,
            (None, &NoData)
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
            shape,
            (None, &NoData)
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
            shape,
            (None, &NoData)
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
fn a_type_without_the_key_is_numbered_if_it_is_a_unit_and_not_otherwise() {
    let mut w = world(vec![]);
    let key = |class, ini: &str| EntityType {
        class,
        ..ty("m", "thing", ini)
    };
    assert_eq!(name(&mut w, &key("keeper", ""), "Thing"), "Thing 1");
    assert_eq!(name(&mut w, &key("objects", ""), "Thing"), "Thing");
    let on = "[Characteristics/Integers]\ncUseNumbersInName=1\n";
    // Scenery, fences, paths, walls, rubble and ambient animals keep the bare text, flag or not.
    for class in [
        "objects", "fences", "paths", "tankwall", "rubble", "ambient",
    ] {
        assert_eq!(name(&mut w, &key(class, on), "Thing"), "Thing", "{class}");
    }
    // A missing text gives no name, and no number is taken.
    assert_eq!(named(&mut w, &key("keeper", "")), "");
    assert_eq!(w.zoo.names["thing"], 1);
}

#[test]
fn food_keeps_the_bare_text_whatever_the_flag_says() {
    let mut w = world(vec![]);
    let food = |ini: &str| EntityType {
        class: "food",
        ..ty("m", "thing", ini)
    };
    assert_eq!(name(&mut w, &food(""), "Thing"), "Thing");
    assert_eq!(
        name(
            &mut w,
            &food("[Characteristics/Integers]\ncUseNumbersInName=1\n"),
            "Thing"
        ),
        "Thing"
    );
    assert_eq!(
        name(
            &mut w,
            &food("[Characteristics/Integers]\ncUseNumbersInName=0\n"),
            "Thing"
        ),
        "Thing"
    );
    assert!(!w.zoo.names.contains_key("thing"));
}

#[test]
fn a_building_or_a_tank_filter_is_numbered_under_its_subtype_and_the_flag_can_turn_it_off() {
    let mut w = world(vec![]);
    let of = |class: &'static str, subtype: &str, ini: &str| EntityType {
        class,
        ..ty(subtype, "building", ini)
    };
    assert_eq!(
        name(&mut w, &of("building", "vendfood", ""), "Test Stand"),
        "Test Stand 1"
    );
    assert_eq!(
        name(&mut w, &of("building", "vendfood", ""), "Test Stand"),
        "Test Stand 2"
    );
    assert_eq!(
        name(&mut w, &of("filter", "f", ""), "Test Filter"),
        "Test Filter 1"
    );
    // A flag of 0, -1, a huge value or a missing one: the bare text for 0, -1 and huge, and no counter taken.
    for ini in [
        "cUseNumbersInName=0",
        "cUseNumbersInName=-1",
        "cUseNumbersInName=2000000000",
    ] {
        let ini = format!("[Characteristics/Integers]\n{ini}\n");
        assert_eq!(
            name(&mut w, &of("building", "trshcan", &ini), "Test Bin"),
            "Test Bin"
        );
    }
    assert_eq!(w.zoo.names["vendfood"], 2);
    assert_eq!(w.zoo.names["f"], 1);
    assert!(!w.zoo.names.contains_key("trshcan") && !w.zoo.names.contains_key("building"));
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
        place(
            w,
            &mut money,
            &cat.types[ty],
            "Staff",
            &p,
            shape,
            (None, &NoData),
        )
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
            viewing: 0,
            serviced: 0,
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
fn only_a_tall_unit_collects_the_transients_of_a_taken_tile() {
    let guest = entity_at("guests", "guest", "m", 3 * 64 + 32, 2 * 64 + 32, 0);
    let sparkle = |tx| entity_at("objects", "sparkle", "", tx * 64 + 8, 2 * 64 + 8, 0);
    // A guest and a sparkle on tile (3, 2), a sparkle alone on (2, 2).
    let w = tank_world_with(3, vec![guest, sparkle(3), sparkle(2)]);
    let tank = Aim {
        zoom: 0,
        ..aim(0, behind_the_tank(3))
    };
    let (cat, wet) = (catalogue(), [false, true]);
    let held = |ty| {
        let p = pending_of(&w, &cat, &wet, ty, 0, 0, tank, 0.0, Form::Plain).unwrap();
        assert_eq!(
            (p.tile, p.message),
            ((3, 2), Some(super::super::IN_THE_WAY))
        );
        p.blocking
    };
    // The guest is entity 0 and the sparkle of the taken tile is 1; the lone sparkle never is.
    assert_eq!(held(TALL), [0, 1]);
    assert_eq!(held(HIPPO), [0]);
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
            crate::exhibit::members::animals_in(w, exhibit).len(),
        )
    };
    assert_eq!(counted(&w), (1, 1));
    pick_up(&mut w, &cat, id, 0).unwrap();
    assert_eq!(counted(&w), (0, 0));
    let (x, y) = (2 * 64 + 32, 2 * 64 + 32);
    assert!(put_down(
        &mut w,
        id,
        (x, y, 0),
        0,
        &crate::behaviour::NoData
    ));
    assert_eq!(counted(&w), (1, 1));
}

#[test]
fn the_pick_up_of_an_animal_clears_its_own_target_tile_and_position() {
    // Shows 32.2 point 4: the state clear writes them directly.
    let cat = catalogue();
    let mut pieces = square("fences", 2, 2);
    pieces.push(animal("m", (2, 2)));
    let mut w = world(pieces);
    let id = w.zoo.entities.last().unwrap().common.as_ref().unwrap().id;
    let mut u = w.units.get(id).unwrap().clone();
    u.aim(&w, w.tile_index(4, 4));
    assert_ne!(u.own_pos, [-1; 3]);
    *w.units.get_mut(id).unwrap() = u;
    pick_up(&mut w, &cat, id, 0).unwrap();
    let u = w.units.get(id).unwrap();
    assert_eq!((u.own_tile, u.own_pos), (None, [-1; 3]));
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
fn a_picked_up_staff_member_writes_no_slot_in_its_block() {
    let cat = catalogue();
    let mut w = tank_world(3);
    let mut keeper = entity_at("keeper", "marine", "m", 6 * 64 + 20, 6 * 64 + 20, 2);
    keeper.common.as_mut().unwrap().unit = Some(Default::default());
    let id = w.add(keeper.clone(), shape(&keeper)).unwrap();
    let slot = |w: &mut World, v| {
        let at = w.find(id).unwrap();
        let u = w.zoo.entities[at].common.as_mut().unwrap().unit.as_mut();
        std::mem::replace(&mut u.unwrap().slot, v)
    };
    slot(&mut w, 7);
    pick_up(&mut w, &cat, id, 0).unwrap();
    // The state clear of a pick-up writes 0xff for every kind (goals 11.3.1 point 2).
    assert_eq!(slot(&mut w, 7), 0xff);
}

#[test]
fn a_picked_up_keeper_closes_the_gate_it_remembers() {
    let cat = catalogue();
    let mut w = tank_world(3);
    let keeper = entity_at("keeper", "marine", "m", 6 * 64 + 20, 6 * 64 + 20, 2);
    let id = w.add(keeper.clone(), shape(&keeper)).unwrap();
    w.units.get_mut(id).unwrap().gate = Some(7);
    pick_up(&mut w, &cat, id, 0).unwrap();
    assert_eq!(
        w.units.gate_frames().map(|(g, _)| g).collect::<Vec<_>>(),
        [7]
    );
    assert_eq!(w.units.get(id).unwrap().gate, None);
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
        id,
        (p.x, p.y, p.z),
        p.facing,
        &crate::behaviour::NoData
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
        id,
        (p.x, p.y, p.z),
        0,
        &crate::behaviour::NoData
    ));
    assert!(!put_down(
        &mut w,
        99,
        (p.x, p.y, p.z),
        0,
        &crate::behaviour::NoData
    ));
    // A type that is not moveable is not picked up.
    let mut still = catalogue();
    still.types[MARINE].config = Some(std::sync::Arc::new(
        Config::parse(b"[m/Characteristics/Integers]\ncMoveable=0\n").unwrap(),
    ));
    assert_eq!(pick_up(&mut w, &still, id, 0), None);
}

#[test]
fn a_guest_put_down_has_lost_its_wish_to_leave() {
    let cat = catalogue();
    let mut w = world(vec![entity_at(
        "guests",
        "guest",
        "m",
        3 * 64 + 32,
        3 * 64 + 32,
        0,
    )]);
    let id = w.zoo.entities[0].common.as_ref().unwrap().id;
    let Some(crate::unit::Part::Guest(g)) = w.units.get_mut(id).map(|u| &mut u.part) else {
        panic!("a guest");
    };
    g.leave = true;
    pick_up(&mut w, &cat, id, 0).unwrap();
    assert!(put_down(
        &mut w,
        id,
        (4 * 64, 4 * 64, 0),
        0,
        &crate::behaviour::NoData
    ));
    let Some(crate::unit::Part::Guest(g)) = w.units.get(id).map(|u| &u.part) else {
        panic!("a guest");
    };
    assert!(!g.leave);
}

#[test]
fn a_guest_forgets_its_return_building_when_picked_up_and_when_put_down() {
    let cat = catalogue();
    let mut w = world(vec![entity_at(
        "guests",
        "guest",
        "m",
        3 * 64 + 32,
        3 * 64 + 32,
        0,
    )]);
    let id = w.zoo.entities[0].common.as_ref().unwrap().id;
    let set = |w: &mut World, v| {
        let Some(crate::unit::Part::Guest(g)) = w.units.get_mut(id).map(|u| &mut u.part) else {
            panic!("a guest");
        };
        std::mem::replace(&mut g.returning, v)
    };
    set(&mut w, Some(0x1f7b));
    let slot = |w: &mut World, v| match w.units.get_mut(id).map(|u| &mut u.part) {
        Some(crate::unit::Part::Guest(g)) => std::mem::replace(&mut g.slot, v),
        _ => panic!("a guest"),
    };
    slot(&mut w, 2);
    pick_up(&mut w, &cat, id, 0).unwrap();
    // The state clear of a pick-up writes "no slot" (goals 11.3.1 point 2).
    assert_eq!(slot(&mut w, 2), crate::guest::NO_SLOT, "pick-up slot");
    assert_eq!(set(&mut w, Some(5)), None, "pick-up");
    assert!(put_down(
        &mut w,
        id,
        (4 * 64, 4 * 64, 0),
        0,
        &crate::behaviour::NoData
    ));
    assert_eq!(set(&mut w, None), None, "drop");
}

#[test]
fn a_picked_up_escaped_animal_is_a_crate_that_may_stand_on_the_grounds() {
    let mut cat = catalogue();
    let crate_ini = "[m/Characteristics/Integers]\ncBoxFootprintX=4\ncBoxFootprintY=4\n";
    cat.push(ty("m", "zebra", crate_ini));
    let wide = cat.types.len() - 1;
    let mut w = world(square("fences", 2, 2));
    let zebra = animal("m", (4, 4));
    let id = w.add(zebra.clone(), shape(&zebra)).unwrap();
    let state = |w: &mut World, escaped: bool| {
        let Some(crate::unit::Part::Animal(a)) = w.units.get_mut(id).map(|u| &mut u.part) else {
            panic!("not an animal");
        };
        (a.escaped, a.thirsty, a.walk) = (escaped, true, Some(Default::default()));
    };
    let looks = |w: &World| match w.units.get(id).map(|u| &u.part) {
        Some(crate::unit::Part::Animal(a)) => (a.crated, a.escaped, a.thirsty, a.walk.is_some()),
        _ => panic!("not an animal"),
    };
    let judged = |w: &World, carry: &Carry, ty: usize, tile: (i64, i64)| {
        let carry = Carry {
            ty,
            ..carry.clone()
        };
        let point = (tile.0 * 64 + 32, tile.1 * 64 + 32, 0);
        let p = carried(w, &cat, &[false, true], &carry, 0, aim(0, point)).unwrap();
        (p.verdict, p.message)
    };
    // An animal that is not escaped is carried as itself: refused on the grounds.
    state(&mut w, false);
    let carry = pick_up(&mut w, &cat, id, 0).unwrap();
    assert_eq!(looks(&w), (false, false, true, false));
    let refused = (Verdict::Invalid, Some(IN_AN_EXHIBIT));
    assert_eq!(judged(&w, &carry, ZEBRA, (4, 4)), refused);
    assert!(put_down(
        &mut w,
        id,
        carry.from,
        0,
        &crate::behaviour::NoData
    ));
    // An escaped one is crated: no longer loose, its walk and its need flags gone.
    state(&mut w, true);
    let carry = pick_up(&mut w, &cat, id, 0).unwrap();
    assert_eq!(looks(&w), (true, false, false, false));
    // The crate may stand on the grounds, even for a species that lives only under water.
    let valid = (Verdict::Valid, None);
    assert_eq!(judged(&w, &carry, ZEBRA, (4, 4)), valid);
    assert_eq!(judged(&w, &carry, ZEBRA, (2, 2)), valid);
    assert_eq!(judged(&w, &carry, ORCA, (4, 4)), valid);
    // A crate takes its own footprint: 2 by 2 by default, so the long stegosaurus is not large;
    // a 4 by 4 crate is, and its quarters may not touch water (tile 5, 5).
    assert_eq!(judged(&w, &carry, BIG, (6, 2)), valid);
    assert_eq!(judged(&w, &carry, wide, (5, 2)), valid);
    assert_eq!(judged(&w, &carry, wide, (5, 4)).0, Verdict::Invalid);
}

#[test]
fn a_crated_fish_over_water_needs_a_tank() {
    let mut w = world(square("fences", 2, 2));
    let id = w
        .add(animal("m", (4, 4)), shape(&animal("m", (4, 4))))
        .unwrap();
    let Some(crate::unit::Part::Animal(a)) = w.units.get_mut(id).map(|u| &mut u.part) else {
        panic!("not an animal");
    };
    a.escaped = true;
    // A shark: lives only under water, does not swim.
    let mut cat = catalogue();
    let shark = "[m/Characteristics/Integers]\ncOnlyUnderwater=1\n";
    cat.push(ty("m", "shark", shark));
    let fish = cat.types.len() - 1;
    let carry = pick_up(&mut w, &cat, id, 0).unwrap();
    let judged = |ty: usize, tile: (i64, i64)| {
        let carry = Carry {
            ty,
            ..carry.clone()
        };
        let point = (tile.0 * 64 + 32, tile.1 * 64 + 32, 0);
        carried(&w, &cat, &[false, true], &carry, 0, aim(0, point))
            .unwrap()
            .message
    };
    // Over water outside a tank: the water message, for a fish and for any other crate.
    assert_eq!(judged(fish, (5, 5)), Some(NEEDS_WATER));
    assert_eq!(judged(ZEBRA, (5, 5)), Some(10569));
    // On dry land the crate of a fish is fine.
    assert_eq!(judged(fish, (2, 2)), None);
}

/// A keeper (job 3) holding the animal of a heal, which is held (`held` raised).
fn healing() -> (World, u32, u32) {
    let mut w = world(vec![animal("m", (3, 3))]);
    let a = w.zoo.entities.last().unwrap().common.as_ref().unwrap().id;
    let keeper = entity_at("keeper", "marine", "m", 6 * 64 + 20, 6 * 64 + 20, 2);
    let k = w.add(keeper.clone(), shape(&keeper)).unwrap();
    w.units.get_mut(a).unwrap().held = true;
    let r = &mut w.units.get_mut(k).unwrap().goal.record;
    (r.job, r.job_target.id) = (3, a);
    (w, k, a)
}

#[test]
fn a_keeper_reset_or_picked_up_leaves_the_animal_of_his_heal_held() {
    let cat = catalogue();
    let (mut w, k, a) = healing();
    pick_up(&mut w, &cat, k, 0).unwrap();
    assert!(w.units.get(a).unwrap().held);
}

#[test]
fn a_pick_up_keeps_an_ordinary_wait() {
    let cat = catalogue();
    let (mut w, _, a) = healing();
    w.units.get_mut(a).unwrap().wait = 77;
    pick_up(&mut w, &cat, a, 0).unwrap();
    assert_eq!(w.units.get(a).unwrap().wait, 77);
}

#[test]
fn an_animal_picked_up_is_no_longer_held() {
    let cat = catalogue();
    let (mut w, _, a) = healing();
    pick_up(&mut w, &cat, a, 0).unwrap();
    assert!(!w.units.get(a).unwrap().held);
}

/// The pending unit of the type for the point `p` (with its height) seen from view 0.
fn over(w: &World, ty: usize, p: (i64, i64, i32)) -> Option<Pending> {
    pending(w, &catalogue(), &[false, true], ty, 0, aim(0, p), false)
}

#[test]
fn the_tank_point_looks_48_plus_the_tank_limit_steps() {
    // The water stands 60 levels high, so the tank is 60 steps from the ground point.
    let tile = |limit: Option<u32>| {
        let mut w = tank_world(3);
        if let Some(limit) = limit {
            let config = crate::exhibit::TankConfig::default();
            w.set_tank_config(crate::exhibit::TankConfig { limit, ..config });
        }
        let id = w.tank_at(2 * 8 + 2).unwrap();
        w.exhibits_mut()
            .get_mut(id)
            .unwrap()
            .tank
            .as_mut()
            .unwrap()
            .level = 60;
        over(&w, HIPPO, behind_the_tank(60)).map(|p| p.tile)
    };
    // No tanks.cfg: the program's 30 gives 78 steps. The limit 13 gives 61, the last step being
    // 60; 12 gives 60 steps, which stop one short; 0 leaves the 48.
    assert_eq!(tile(None), Some((3, 2)));
    assert_eq!(tile(Some(13)), Some((3, 2)));
    assert_ne!(tile(Some(12)), Some((3, 2)));
    assert_ne!(tile(Some(0)), Some((3, 2)));
    // The largest limit a file can give (255) takes no overflow.
    assert_eq!(tile(Some(255)), Some((3, 2)));
    assert_eq!(tile(Some(u32::MAX)), Some((3, 2)));
}

#[test]
fn the_ladder_row_also_refuses_a_unit_that_covers_the_middle_of_the_gate_tile() {
    let (cat, w) = (catalogue(), tank_world(3));
    let id = w.tank_at(2 * 8 + 2).unwrap();
    // The gate inside tile is (2, 2): its middle is (160, 160). A unit on tile (3, 2) whose
    // rectangle reaches it is refused, the bounds being included; a keeper may stand there.
    let row = |ty: usize, half: i32| {
        let rect = (3 * 64, 2 * 64 + 32, half, half);
        tank_message(&w, id, 19, &cat.types[ty], rect, Form::Plain)
    };
    assert_eq!(
        row(MAINT, 32),
        Some(Some(LADDER)),
        "32 apart, bounds included"
    );
    assert_ne!(row(MAINT, 31), Some(Some(LADDER)));
    assert_ne!(row(KEEPER, 32), Some(Some(LADDER)));
    for half in [0, -5, i32::MAX] {
        row(MAINT, half);
    }
}

/// The tank world with an object of the type `name` at (`x`, `y`) of tile (3, 2).
fn with_object(name: &str, level: u32, (x, y): (i32, i32)) -> World {
    let thing = entity_at("objects", name, "", 3 * 64 + x, 2 * 64 + y, 0);
    tank_world_with(level, vec![thing])
}

#[test]
fn an_animal_is_too_deep_for_the_height_of_the_objects_that_block_its_quarters() {
    let cat = catalogue();
    let row = |w: &World| {
        let id = w.tank_at(2 * 8 + 2).unwrap();
        let rect = (3 * 64 + 16, 2 * 64 + 16, 16, 16);
        tank_message(w, id, 19, &cat.types[HIPPO], rect, Form::Plain)
    };
    // A pillar of 5 levels in a tank of 3: 10560. A tank of 5 or more takes it.
    assert_eq!(row(&with_object("pillar", 3, (32, 32))), Some(Some(DEEPER)));
    assert_eq!(row(&with_object("pillar", 5, (32, 32))), None);
    // Nothing blocks the quarters of the tile: the type's own depth (0) is all there is.
    assert_eq!(row(&tank_world(1)), None);
}

#[test]
fn a_swimmer_that_keeps_off_the_walls_and_a_floating_object_refuse_without_a_message() {
    let w = tank_world(3);
    let swim = |w: &World, ty| over(w, ty, behind_the_tank(3)).unwrap();
    let (hippo, ray) = (swim(&w, HIPPO), swim(&w, RAY));
    assert_eq!((hippo.verdict, hippo.message), (Verdict::Valid, None));
    // Both tiles of the tank touch a wall: the ray (`cAvoidEdges` 1) is refused, with no message.
    assert_eq!((ray.verdict, ray.message), (Verdict::Invalid, None));
    // A floating pad in the fourth quarter refuses; in the first quarter it refuses nothing.
    let first = with_object("pad", 3, (16, 16));
    assert_eq!(swim(&first, HIPPO).message, None);
    let fourth = with_object("pad", 3, (48, 48));
    let id = fourth.tank_at(2 * 8 + 2).unwrap();
    let rect = (3 * 64 + 16, 2 * 64 + 16, 16, 16);
    let hippo = &catalogue().types[HIPPO];
    assert_eq!(
        tank_message(&fourth, id, 19, hippo, rect, Form::Plain),
        Some(None)
    );
    let id = first.tank_at(2 * 8 + 2).unwrap();
    assert_eq!(tank_message(&first, id, 19, hippo, rect, Form::Plain), None);
}

#[test]
fn an_egg_is_the_young_of_a_species_that_lays_eggs_and_does_not_swim() {
    let cat = catalogue();
    assert!(is_egg(&cat.types[YOUNG]));
    assert!(!is_egg(&cat.types[ADULT]) && !is_egg(&cat.types[ZEBRA]));
    // The water of tile (5, 5), in a pen: the adult swims there, the egg is told to go on land.
    let w = world(square("fences", 5, 5));
    let adult = at(&w, ADULT, (5, 5));
    assert_eq!((adult.verdict, adult.message), (Verdict::Valid, None));
    let egg = at(&w, YOUNG, (5, 5));
    assert_eq!((egg.verdict, egg.message), (Verdict::Invalid, Some(10570)));
    // The tank holds no egg either (10570), whatever the water.
    let tank = tank_world(3);
    let id = tank.tank_at(2 * 8 + 2).unwrap();
    let rect = (3 * 64 + 16, 2 * 64 + 16, 16, 16);
    assert_eq!(
        tank_message(&tank, id, 19, &cat.types[YOUNG], rect, Form::Egg),
        Some(Some(10570))
    );
}

#[test]
fn an_egg_takes_the_slope_rule_by_its_egg_footprint() {
    // An egg is 2 by 2 quarters: a sloped tile that is no path shape refuses it, as it refuses a
    // large animal, where the adult of 1 by 1 may stand (unit placing 6.2 item 2).
    let mut pieces = square("fences", 2, 2);
    pieces.extend(square("fences", 3, 2));
    let w = world_with(pieces, |z| z.tiles[2 * 8 + 2].shape = 0x12);
    assert_eq!(at(&w, ADULT, (2, 2)).verdict, Verdict::Valid);
    assert_eq!(at(&w, YOUNG, (2, 2)).verdict, Verdict::Invalid);
    assert_eq!(at(&w, YOUNG, (3, 2)).verdict, Verdict::Valid);
}

#[test]
fn a_pending_animal_draws_its_creation_and_an_egg_layer_is_an_egg_of_the_sex_of_the_button() {
    let (cat, w) = (catalogue(), world(Vec::new()));
    // One draw for each counter that has a period.
    let counters = w.units.class.periods.iter().filter(|&&p| p != 0).count() as u32;
    let draws = |n: u32| {
        let mut rng = Rng::default();
        (0..n + counters).for_each(|_| _ = rng.rand());
        rng
    };
    // The young of an egg layer draws its sex, which the button then overwrites, and is an egg.
    for (button, female) in [(ADULT, false), (ADULT_F, true)] {
        let mut rng = Rng::default();
        let (ty, a) = crate::animal::breed::pending(&w, &cat, button, &mut rng).unwrap();
        assert_eq!((ty, a.egg, a.female), (YOUNG, true, female));
        assert_eq!(rng, draws(1));
    }
    // An adult that does not lay eggs is made as it is chosen, and draws nothing here.
    let mut rng = Rng::default();
    let (ty, a) = crate::animal::breed::pending(&w, &cat, ZEBRA, &mut rng).unwrap();
    assert_eq!((ty, a.egg, a.female), (ZEBRA, false, false));
    assert_eq!(rng, draws(0));
    // No young type, nothing is made, nothing is drawn; a type that is not there neither.
    for ty in [LONE, 999] {
        assert!(crate::animal::breed::pending(&w, &cat, ty, &mut rng).is_none());
    }
    assert_eq!(rng, draws(0));
}

#[test]
fn the_pending_type_is_the_made_one_and_an_animal_with_none_made_has_no_pending_unit() {
    let cat = catalogue();
    // An egg layer chosen as an adult is pending as its young; a species with no young: nothing.
    assert_eq!(made_type(&cat, Some(YOUNG), ADULT), Some(YOUNG));
    assert_eq!(made_type(&cat, Some(ZEBRA), ZEBRA), Some(ZEBRA));
    assert_eq!(made_type(&cat, None, ADULT), None);
    assert_eq!(made_type(&cat, Some(YOUNG), ZEBRA), None, "another species");
    assert_eq!(made_type(&cat, Some(999), ZEBRA), None);
    assert_eq!(made_type(&cat, None, 999), None);
    // A staff member is pending without one, as a building is.
    assert_eq!(made_type(&cat, None, KEEPER), Some(KEEPER));
}

#[test]
fn a_placed_unit_keeps_the_id_and_the_state_it_was_made_with_and_is_in_tank_mode_at_once() {
    let cat = catalogue();
    let mut w = tank_world(3);
    let mut money = Money::default();
    money.set_cash(1000.0);
    let id = w.take_id().unwrap();
    assert_eq!(id, 1001, "the counter moved at the creation");
    // The creation state: an animal with a happiness no other path gives.
    let made = crate::animal::Animal {
        happiness: -77,
        ..Default::default()
    };
    let made = Made {
        ty: HIPPO,
        id,
        part: Part::Animal(made),
    };
    // Something else is added meanwhile: the unit still has the id it took.
    let other = animal("m", (6, 6));
    let other = w.add(other.clone(), shape(&other)).unwrap();
    assert_eq!(other, 1002);
    let p = over(&w, HIPPO, behind_the_tank(3)).unwrap();
    assert_eq!(p.verdict, Verdict::Valid);
    let placed = place(
        &mut w,
        &mut money,
        &cat.types[HIPPO],
        "Hippo",
        &p,
        shape,
        (Some(made), &NoData),
    );
    assert_eq!(placed, Outcome::Placed(id));
    let unit = w.units.get(id).unwrap();
    assert!(unit.started && unit.typed);
    assert!(matches!(&unit.part, Part::Animal(a) if a.happiness == -77));
    // It is at the surface of the water from the click, with the move kind of a swimmer.
    assert_eq!(unit.tank, 3);
    let at = w.find(id).unwrap();
    assert_eq!(w.zoo.entities[at].common.as_ref().unwrap().movement_mode, 3);
    assert_eq!(w.zoo.id_counter, 1002, "no id was taken at the add");
}

#[test]
fn a_hired_staff_member_marks_its_exhibit_for_a_new_rating_and_an_animal_does_too() {
    let cat = catalogue();
    for ty in [KEEPER, ZEBRA] {
        let mut w = world(square("fences", 2, 2));
        let pen = w.exhibits().region(2 * 8 + 2).unwrap();
        let mut money = Money::default();
        money.set_cash(5000.0);
        w.exhibits_mut().get_mut(pen).unwrap().rating.dirty = false;
        let p = at(&w, ty, (2, 2));
        let done = place(
            &mut w,
            &mut money,
            &cat.types[ty],
            "x",
            &p,
            shape,
            (None, &NoData),
        );
        assert!(matches!(done, Outcome::Placed(_)), "{done:?}");
        assert!(w.exhibits().get(pen).unwrap().rating.dirty, "{ty}");
    }
}

#[test]
fn an_animal_refused_by_a_tank_is_crated_and_moved_and_an_egg_never_crated() {
    let cat = catalogue();
    let mut w = tank_world(3);
    let swimmer = animal("m", (3, 2));
    let id = w.add(swimmer.clone(), shape(&swimmer)).unwrap();
    let egg = animal("y", (2, 2));
    let egg = w.add(egg.clone(), shape(&egg)).unwrap();
    for e in [id, egg] {
        if let Some(Part::Animal(a)) = w.units.get_mut(e).map(|u| &mut u.part) {
            a.escaped = e == id;
            a.egg = e == egg;
            a.hatch = if e == egg { 5 } else { 0 };
        }
    }
    let width = 8;
    let moved = |w: &World, e: u32| {
        let c = w.zoo.entities[w.find(e).unwrap()].common.clone().unwrap();
        (
            c.x.div_euclid(64),
            c.y.div_euclid(64),
            c.x.rem_euclid(64),
            c.y.rem_euclid(64),
        )
    };
    assert!(refuse(&mut w, &cat, id, 0, &NoData));
    let (x, y, lx, ly) = moved(&w, id);
    // At the middle of a tile of the grounds, three or more tiles from the start (tile 0, 0).
    assert_eq!((lx, ly), (32, 32));
    assert!(x + y >= 3 && x < width && y < width);
    let Some(Part::Animal(a)) = w.units.get(id).map(|u| &u.part) else {
        panic!("not an animal");
    };
    assert!(a.crated && !a.escaped);
    assert!(refuse(&mut w, &cat, egg, 0, &NoData));
    let Some(Part::Animal(a)) = w.units.get(egg).map(|u| &u.part) else {
        panic!("not an animal");
    };
    assert!(a.egg && !a.crated, "an egg is never crated");
    assert_ne!(moved(&w, egg), (2, 2, 32, 32), "but it is moved");
    // The countdown of a spent egg is kept and its health recomputed from it.
    // No tile within reach: the animal stays where it is for the caller to sell.
    let mut full = world(
        (0..8)
            .flat_map(|y| (0..8).map(move |x| animal("m", (x, y))))
            .collect(),
    );
    let last = full.zoo.entities[0].common.as_ref().unwrap().id;
    assert!(!refuse(&mut full, &cat, last, 0, &NoData));
    let crated = matches!(full.units.get(last).map(|u| &u.part), Some(Part::Animal(a)) if a.crated);
    assert!(crated, "crated where it stood");
}

struct TimedData;

impl crate::behaviour::Data for TimedData {
    fn set(&self, _: &Entity, _: &str) -> Option<&[crate::behaviour::Call]> {
        None
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Timing> {
        Some(crate::behaviour::Timing {
            count: 1,
            delay: 100,
        })
    }
    fn number(&self, _: &Entity, _: &crate::behaviour::Number) -> i32 {
        0
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Heard> {
        None
    }
}

#[test]
fn an_egg_put_down_in_a_dry_tank_keeps_its_egg_picture() {
    // Behaviours 5.10.1 points 2.3 and 4: a pick-up keeps the base of an egg, and a drop where the
    // tank mode stays 0 changes nothing.
    let cat = catalogue();
    let mut w = tank_world(0);
    let egg = animal("m", (6, 6));
    let id = w.add(egg.clone(), shape(&egg)).unwrap();
    let Some(crate::unit::Part::Animal(a)) = w.units.get_mut(id).map(|u| &mut u.part) else {
        panic!("not an animal");
    };
    a.egg = true;
    let at = w.find(id).unwrap();
    let mut unit = std::mem::replace(
        w.units.get_mut(id).unwrap(),
        crate::unit::Unit::new(id, crate::unit::Kind::Animal),
    );
    crate::unit::swim::idle_reset(&mut unit, &w, &TimedData, at);
    *w.units.get_mut(id).unwrap() = unit;
    let carry = pick_up(&mut w, &cat, id, 0).unwrap();
    assert!(put_down(
        &mut w,
        id,
        (3 * 64 + 32, 2 * 64 + 32, 0),
        carry.facing,
        &TimedData
    ));
    let unit = w.units.get_mut(id).unwrap();
    unit.animation.advance(100, &|_| {
        Some(crate::behaviour::Timing {
            count: 1,
            delay: 100,
        })
    });
    assert_eq!(unit.animation.name, "egg_idle");
}
