//! The bulldozer on synthetic maps.

use super::*;
use crate::config::Config;
use crate::world::{Shape, TankKind};
use crate::zoo::{Common, Tile, View, Zoo};

fn ty(
    class: &'static str,
    name: &str,
    subtype: &str,
    ini: &str,
    footprint: [i32; 3],
) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: subtype.into(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint,
        height: 0,
    }
}

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let cost = |n: u32| format!("[Characteristics/Integers]\ncPurchaseCost={n}\n");
    cat.push(ty("paths", "dirt", "", &cost(10), [2, 2, 1]));
    cat.push(ty("objects", "tree", "", &cost(25), [1, 1, 1]));
    cat.push(ty(
        "objects",
        "dumpster",
        "",
        "[Characteristics/Integers]\ncPurchaseCost=25\ncDeletable=0\n",
        [1, 1, 1],
    ));
    cat.push(ty(
        "building",
        "shop",
        "",
        "[Characteristics/Integers]\ncPurchaseCost=350\ncNeedsConfirm=1\n",
        [4, 4, 1],
    ));
    let fence =
        "[Characteristics/Integers]\ncPurchaseCost=125\ncLife=10\ncDecayedLife=4\ncStrength=300\n";
    cat.push(ty("fences", "iron", "f", fence, [1, 1, 1]));
    cat.push(ty("fences", "iron", "g", fence, [1, 1, 1]));
    cat.push(ty("animals", "lion", "", &cost(500), [2, 2, 1]));
    let tank = "[TankFence]\nx=1\n[Characteristics/Integers]\ncPurchaseCost=100\ncLife=10\n";
    cat.push(ty("tankwall", "glass", "f", tank, [1, 1, 1]));
    cat.push(ty("tankwall", "show", "f", tank, [1, 1, 1]));
    cat.push(ty("tankwall", "glass", "g", tank, [1, 1, 1]));
    cat
}

fn entity(class: &str, name: &str, subtype: &str, x: i32, y: i32, rotation: u32) -> Entity {
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
        }),
    }
}

fn shape(e: &Entity) -> Option<Shape> {
    let (kind, footprint, fence) = match e.class.as_str() {
        "objects" => (Kind::Fixed, (1, 1), false),
        "building" => (Kind::Fixed, (4, 4), false),
        "paths" => (Kind::Path, (2, 2), false),
        "fences" => (Kind::Edge, (1, 1), true),
        "tankwall" => (Kind::Wall, (1, 1), true),
        "animals" => (Kind::Unit, (1, 1), false),
        _ => return None,
    };
    Some(Shape {
        kind,
        footprint,
        fence,
        wall: false,
        gate: false,
        gated: fence,
        walkable: (false, false),
        tank: TankKind {
            wall: e.class == "tankwall",
            show: e.type_name == "show",
            ..TankKind::NONE
        },
    })
}

/// An 8 by 8 map of flat tiles; the entrance (0, 0) is the zoo grounds; tiles with `x >= 7` are
/// outside the zoo when `outside` (a column that no entity marks, set by hand).
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
            tiles: vec![flat; 64],
            entities,
            view: View::default(),
            clocks: Default::default(),
        },
        shape,
    )
}

/// The piece on side `s` of tile (`tx`, `ty`).
fn piece(subtype: &str, tx: i32, ty: i32, s: u32) -> Entity {
    let (x, y) = [(32, 0), (63, 32), (32, 63), (0, 32)][s as usize / 2];
    entity("fences", "iron", subtype, tx * 64 + x, ty * 64 + y, s)
}

fn path(tx: i32, ty: i32) -> Entity {
    entity("paths", "dirt", "", tx * 64 + 32, ty * 64 + 32, 0)
}

/// The four sides of tile (`tx`, `ty`).
fn square(tx: i32, ty: i32) -> Vec<Entity> {
    [0, 2, 4, 6].map(|s| piece("f", tx, ty, s)).into()
}

fn at(w: &World, class: &str, name: &str) -> usize {
    w.zoo
        .entities
        .iter()
        .position(|e| e.class == class && e.type_name == name)
        .unwrap()
}

#[test]
fn the_refund_is_80_percent_and_a_fence_gets_less_as_it_decays() {
    let cat = catalogue();
    assert_eq!(refund(&cat, &path(1, 1)), 8.0);
    assert_eq!(
        refund(&cat, &entity("building", "shop", "", 0, 0, 0)),
        280.0
    );
    let mut fence = piece("f", 1, 1, 0);
    assert_eq!(refund(&cat, &fence), 100.0);
    let life = |e: &mut Entity, l: u32| e.common.as_mut().unwrap().fence.as_mut().unwrap().life = l;
    // Above the decayed life 4: 80 %; at it: 40 %; broken: nothing.
    life(&mut fence, 5);
    assert_eq!(refund(&cat, &fence), 100.0);
    life(&mut fence, 4);
    assert_eq!(refund(&cat, &fence), 50.0);
    life(&mut fence, 1);
    assert_eq!(refund(&cat, &fence), 50.0);
    life(&mut fence, 0);
    assert_eq!(refund(&cat, &fence), 0.0);
    // An unknown type refunds nothing.
    assert_eq!(refund(&cat, &entity("objects", "ghost", "", 0, 0, 0)), 0.0);
}

#[test]
fn only_paths_and_deletable_scenery_in_the_zoo_can_be_removed() {
    let cat = catalogue();
    let w = world(vec![
        path(1, 1),
        entity("objects", "tree", "", 2 * 64 + 16, 2 * 64 + 16, 0),
        entity("objects", "dumpster", "", 3 * 64 + 16, 3 * 64 + 16, 0),
        entity("animals", "lion", "", 4 * 64 + 5, 4 * 64 + 5, 0),
        piece("f", 5, 5, 2),
    ]);
    let can: Vec<bool> = (0..5).map(|e| removable(&w, &cat, e)).collect();
    assert_eq!(can, [true, true, false, false, true]);
    assert!(!removable(&w, &cat, 99));
}

#[test]
fn an_area_collects_each_object_once_by_the_tiles_it_covers() {
    let mut pieces: Vec<Entity> = (1..=7).map(|x| path(x % 8, 2)).collect();
    pieces.push(entity("building", "shop", "", 3 * 64, 4 * 64, 0));
    pieces.push(piece("f", 2, 2, 4));
    let w = world(pieces);
    // The row of paths and the fence on the south side of tile (2, 2).
    let row = area(&w, (1, 2), (7, 2));
    assert_eq!(row.len(), 8);
    assert!(row.contains(&8));
    // The shop covers four tiles and is collected once.
    let shop = area(&w, (2, 3), (4, 5));
    assert_eq!(shop.iter().filter(|&&e| e == 7).count(), 1);
    // The corners may come in either order; off the map tiles are left out.
    assert_eq!(area(&w, (7, 2), (1, 2)).len(), 8);
    assert_eq!(area(&w, (-4, -4), (20, 20)).len(), 9);
    assert!(area(&w, (50, 50), (60, 60)).is_empty());
    // A piece is collected by its own tile only: the north side of (2, 3) is the same line as
    // the south side of (2, 2), but another piece.
    assert!(!area(&w, (2, 3), (2, 3)).contains(&8));
}

#[test]
fn removing_a_piece_that_opens_an_exhibit_asks_and_a_closed_line_does_not() {
    let cat = catalogue();
    let w = world(square(3, 3));
    let top = 0;
    // The piece on side 0 of the exhibit tile: the other side is the zoo grounds.
    assert_eq!(
        check(&w, &cat, top, false, &[]),
        Check::Ask(Question::Escape(1))
    );
    // A piece on the other tile's slot of the same line keeps it closed: no question.
    let mut both = square(3, 3);
    both.push(piece("f", 3, 2, 4));
    let w2 = world(both);
    assert_eq!(check(&w2, &cat, 0, false, &[]), Check::Go);
    // A piece of no exhibit: no question.
    let lone = world(vec![piece("f", 5, 5, 0)]);
    assert_eq!(check(&lone, &cat, 0, false, &[]), Check::Go);
    // An asked pair is kept without a question.
    let pair = Question::Escape(1).pair().unwrap();
    assert_eq!(check(&w, &cat, top, false, &[pair]), Check::Keep);
}

#[test]
fn removing_the_line_between_two_exhibits_asks_to_merge_them() {
    let cat = catalogue();
    let mut v = vec![piece("f", 3, 3, 2)];
    v.extend([0, 4, 6].map(|s| piece("f", 3, 3, s)));
    v.extend([0, 2, 4].map(|s| piece("f", 4, 3, s)));
    // Tile (3, 3) and tile (4, 3) are two exhibits with one piece between them, on tile (3, 3).
    let w = world(v);
    assert_eq!(w.exhibits().list().len(), 2);
    assert_eq!(
        check(&w, &cat, 0, true, &[]),
        Check::Ask(Question::Merge(1, 2))
    );
    assert_eq!(Question::Merge(2, 1).pair(), Some((1, 2)));
    assert_eq!(Question::Escape(2).pair(), Some((0, 2)));
    assert_eq!(Question::Sell.pair(), None);
}

#[test]
fn a_type_that_needs_confirming_asks_when_the_switch_is_on() {
    let cat = catalogue();
    let w = world(vec![
        entity("building", "shop", "", 3 * 64, 3 * 64, 0),
        entity("objects", "tree", "", 16, 16, 0),
    ]);
    assert_eq!(check(&w, &cat, 0, true, &[]), Check::Ask(Question::Sell));
    assert_eq!(check(&w, &cat, 0, false, &[]), Check::Go);
    assert_eq!(check(&w, &cat, 1, true, &[]), Check::Go);
}

#[test]
fn removing_books_the_refund_and_the_gate_moves_to_the_piece_across() {
    let cat = catalogue();
    let mut money = Money {
        cash: 1000.0,
        ..Money::default()
    };
    // A gate on side 0 of the exhibit tile with a fence piece on the other tile's slot: the line
    // stays closed and that piece becomes the gate.
    let mut v = square(3, 3);
    v[0] = piece("g", 3, 3, 0);
    v.push(piece("f", 3, 2, 4));
    let mut w = world(v);
    let id = w.zoo.entities[0].common.as_ref().unwrap().id;
    w.set_gate(1, id);
    let removed = remove(&mut w, &mut money, &cat, id).unwrap();
    assert_eq!(removed.refund, 100.0);
    assert_eq!(removed.name, "fences/iron/g");
    assert_eq!((removed.tile, removed.side), ((3, 3), Some(0)));
    assert_eq!(money.cash, 1100.0);
    let across = w.edge(w.tile_index(3, 2).unwrap(), 4).unwrap();
    assert_eq!(w.zoo.entities[across].subtype, "g");
    // The exhibit is still closed and keeps its id; undo puts the piece back (the gate it gave
    // away is the undo issue's).
    assert_eq!(w.exhibits().list().len(), 1);
    assert_eq!(
        w.exhibits().get(1).and_then(|e| e.gate),
        w.zoo.entities[across].common.as_ref().map(|c| c.id)
    );
    assert!(!w.undo().is_empty());
    assert!(w.find(id).is_some());
    // An unknown id removes nothing and refunds nothing.
    assert!(remove(&mut w, &mut money, &cat, 9999).is_none());
    assert_eq!(money.cash, 1100.0);
}

#[test]
fn removing_the_gate_of_an_open_line_or_a_plain_piece_makes_no_gate() {
    let cat = catalogue();
    let mut money = Money::default();
    // Nothing across: the line opens and the exhibit goes; no piece to turn into a gate.
    let mut v = square(3, 3);
    v[0] = piece("g", 3, 3, 0);
    let mut w = world(v);
    let id = w.zoo.entities[0].common.as_ref().unwrap().id;
    let removed = remove(&mut w, &mut money, &cat, id).unwrap();
    assert_eq!(removed.id, id);
    assert!(w.exhibits().list().is_empty());
    // A plain piece with one across: the piece across stays a fence.
    let mut v = square(3, 3);
    v.push(piece("f", 3, 2, 4));
    let mut w = world(v);
    let id = w.zoo.entities[0].common.as_ref().unwrap().id;
    remove(&mut w, &mut money, &cat, id).unwrap();
    let across = at(&w, "fences", "iron");
    assert!(w.zoo.entities.iter().skip(across).all(|e| e.subtype == "f"));
}

#[test]
fn a_fence_is_found_through_the_slot_of_the_world() {
    let w = world(square(3, 3));
    let t = w.tile_index(3, 3).unwrap();
    for (e, d) in [(0, 0), (1, 2), (2, 4), (3, 6)] {
        assert_eq!(w.slot(e), Some((t, d)));
        assert_eq!(w.edge(t, d), Some(e));
    }
    assert_eq!(w.slot(99), None);
}

/// The piece of a tank wall of type `name` on side `s` of tile (`tx`, `ty`).
fn wall(name: &str, tx: i32, ty: i32, s: u32) -> Entity {
    let (x, y) = [(32, 0), (63, 32), (32, 63), (0, 32)][s as usize / 2];
    entity("tankwall", name, "f", tx * 64 + x, ty * 64 + y, s)
}

fn tank(name: &str, tx: i32, ty: i32, skip: Option<u32>) -> Vec<Entity> {
    [0, 2, 4, 6]
        .into_iter()
        .filter(|&s| Some(s) != skip)
        .map(|s| wall(name, tx, ty, s))
        .collect()
}

#[test]
fn removing_a_wall_that_opens_a_tank_asks_and_between_two_tanks_only_across_a_piece() {
    let cat = catalogue();
    let w = world(tank("glass", 3, 3, None));
    assert!(w.exhibits().get(1).unwrap().tank.is_some());
    assert_eq!(
        check(&w, &cat, 0, false, &[]),
        Check::Ask(Question::Escape(1))
    );
    // Two tanks side by side, one wall on the line (on the first tile's slot): no question; with a
    // wall of the second tank on the line too it asks, whichever is taken.
    let mut one = tank("glass", 3, 3, None);
    one.extend(tank("glass", 4, 3, Some(6)));
    let w = world(one.clone());
    assert_eq!(w.exhibits().list().len(), 2);
    let shared = w.edge(3 * 8 + 3, 2).unwrap();
    assert_eq!(check(&w, &cat, shared, false, &[]), Check::Go);
    one.push(wall("glass", 4, 3, 6));
    let w = world(one);
    let shared = w.edge(3 * 8 + 3, 2).unwrap();
    assert_eq!(
        check(&w, &cat, shared, false, &[]),
        Check::Ask(Question::Merge(1, 2))
    );
}

#[test]
fn a_wall_between_a_show_tank_and_another_tank_cannot_be_removed() {
    let cat = catalogue();
    let mut v = tank("glass", 3, 3, None);
    v.extend(tank("show", 4, 3, None));
    let w = world(v);
    let t = |x: i32| w.tank(w.tank_at(3 * 8 + x as usize).unwrap()).unwrap().show;
    assert!(!t(3) && t(4));
    // The line holds the wall of each tank; neither goes. The outer walls do.
    for (tile, side) in [(3, 2), (4, 6)] {
        let e = w.edge(3 * 8 + tile, side).unwrap();
        assert!(mixed_tanks(&w, e));
        assert!(!removable(&w, &cat, e));
    }
    let outer = w.edge(3 * 8 + 3, 0).unwrap();
    assert!(!mixed_tanks(&w, outer) && removable(&w, &cat, outer));
    // Two show tanks, or two normal ones: the wall goes.
    let mut v = tank("glass", 3, 3, None);
    v.extend(tank("glass", 4, 3, None));
    let w = world(v);
    assert!(removable(&w, &cat, w.edge(3 * 8 + 3, 2).unwrap()));
}

#[test]
fn a_tank_wall_is_tied_to_the_tank_wall_across_and_goes_with_it_in_the_order_of_the_heights() {
    // Two tanks side by side, each with the wall of its own tile on the line: each is tied to
    // the other, and a click takes the piece first.
    let mut v = tank("glass", 3, 3, None);
    v.extend(tank("glass", 4, 3, None));
    let w = world(v);
    let (p, q) = (w.edge(3 * 8 + 3, 2).unwrap(), w.edge(3 * 8 + 4, 6).unwrap());
    assert_eq!((tied(&w, p), tied(&w, q)), (Some(q), Some(p)));
    assert_eq!(
        (with_tied(&w, p), with_tied(&w, q)),
        (vec![p, q], vec![q, p])
    );
    let outer = w.edge(3 * 8 + 3, 0).unwrap();
    assert_eq!((tied(&w, outer), with_tied(&w, outer)), (None, vec![outer]));
    // The drag over one tile takes the outer walls and its own, not the other tank's.
    assert_eq!(area(&w, (3, 3), (3, 3)).len(), 4);

    // A tank beside the grounds, whose wall across is a tank wall on the grounds' tile (it has
    // no wall height): the tied wall goes first, and a drag collects it before the piece even
    // when its tile is outside the rectangle.
    let mut v = tank("glass", 3, 3, None);
    v.push(wall("glass", 4, 3, 6));
    let w = world(v);
    let (p, q) = (w.edge(3 * 8 + 3, 2).unwrap(), w.edge(3 * 8 + 4, 6).unwrap());
    assert_eq!((tied(&w, p), tied(&w, q)), (Some(q), None));
    assert_eq!(with_tied(&w, p), [q, p]);
    let found = area(&w, (3, 3), (3, 3));
    let place = |e| found.iter().position(|&x| x == e).unwrap();
    assert!(found.len() == 5 && place(q) + 1 == place(p), "{found:?}");
    // A fence across that is no tank wall is nobody's tied wall, and neither is a piece of the
    // same tank's other tile.
    let mut v = tank("glass", 3, 3, None);
    v.push(piece("f", 4, 3, 6));
    let w = world(v);
    assert_eq!(tied(&w, w.edge(3 * 8 + 3, 2).unwrap()), None);
}

#[test]
fn the_wall_of_a_tank_asks_when_the_slot_across_is_empty_or_holds_a_tank_wall() {
    let cat = catalogue();
    let asks = Check::Ask(Question::Escape(1));
    // Across empty: asked. Across a tank wall: asked, and the tank wall on the grounds' side
    // is not (the tank's wall is still there).
    let w = world(tank("glass", 3, 3, None));
    assert_eq!(
        check(&w, &cat, w.edge(3 * 8 + 3, 2).unwrap(), false, &[]),
        asks
    );
    let mut v = tank("glass", 3, 3, None);
    v.push(wall("glass", 4, 3, 6));
    let w = world(v);
    let (p, q) = (w.edge(3 * 8 + 3, 2).unwrap(), w.edge(3 * 8 + 4, 6).unwrap());
    assert_eq!(check(&w, &cat, p, false, &[]), asks);
    assert_eq!(check(&w, &cat, q, false, &[]), Check::Go);
    // Another fence across: the land rule, closed, so no question.
    let mut v = tank("glass", 3, 3, None);
    v.push(piece("f", 4, 3, 6));
    let w = world(v);
    assert_eq!(
        check(&w, &cat, w.edge(3 * 8 + 3, 2).unwrap(), false, &[]),
        Check::Go
    );
}

#[test]
fn the_refusal_between_a_show_tank_and_another_tank_needs_a_tied_wall() {
    let cat = catalogue();
    let mut v = tank("glass", 3, 3, None);
    v.extend(tank("show", 4, 3, Some(6)));
    let w = world(v);
    let p = w.edge(3 * 8 + 3, 2).unwrap();
    assert!(w.tank_at(3 * 8 + 3).is_some() && w.tank_at(3 * 8 + 4).is_some());
    assert_eq!(tied(&w, p), None);
    assert!(!mixed_tanks(&w, p) && removable(&w, &cat, p));
}

#[test]
fn gate_tank_hides_the_cost_and_refuses_when_an_object_stands_on_the_tank_tile() {
    use crate::money::Money;
    use crate::place::Verdict;
    let cat = catalogue();
    let money = Money {
        cash: 1000.0,
        ..Money::default()
    };
    let over = |w: &World| {
        // The north piece of tile (3, 3).
        let north = w.edge(w.tile_index(3, 3).unwrap(), 0).unwrap();
        crate::place::gate::pending(w, &cat, &money, north).unwrap()
    };
    let tree = entity("objects", "tree", "", 3 * 64 + 10, 3 * 64 + 10, 0);
    // A tank with nothing in it: a valid gate, green for the marine specialist.
    let w = world(tank("glass", 3, 3, None));
    let g = over(&w);
    assert_eq!((g.pending.verdict, g.hidden), (Verdict::Valid, false));
    // An object in a quadrant of the tile: not valid, no cost text, and a click does nothing.
    let mut v = tank("glass", 3, 3, None);
    v.push(tree.clone());
    let mut w = world(v);
    let g = over(&w);
    assert_eq!(
        (g.pending.verdict, g.hidden, g.pending.message),
        (Verdict::Invalid, true, None)
    );
    let mut m = Money {
        cash: 1000.0,
        ..Money::default()
    };
    assert!(matches!(
        crate::place::gate::place(&mut w, &mut m, &cat, &g),
        crate::place::Outcome::Refused(None)
    ));
    assert_eq!(w.exhibits().get(1).unwrap().gate, None);
    // The same tile of a land exhibit: the object is no reason, the gate is valid and shown
    // (green: the zookeeper is stopped only by an object in quadrant 3, the tree is in 0).
    let mut v = square(3, 3);
    v.push(tree);
    let w = world(v);
    let g = over(&w);
    assert_eq!((g.pending.verdict, g.hidden), (Verdict::Valid, false));
}
