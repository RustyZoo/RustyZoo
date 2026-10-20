//! The rating of a tank and the land exhibit beside it, linked as amphibious neighbours (habitat
//! rating 4, 5.3, 9.2, 9.4): the tank of `amphibious_world` from (2, 2) and the land exhibit from
//! (6, 2), each 4 by 3 tiles, with polar bears (amphibious) and a shark (only under water).

use std::sync::Arc;

use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating::{self, input, rate};
use crate::render::order::Kind;
use crate::sim::Sim;
use crate::unit::testing::{amphibious_world, centre, entity};
use crate::world::{Shape, TankKind, World};
use crate::zoo::Clocks;

const WIDE: usize = 12;

fn kind(class: &'static str, name: &str, name_id: i32, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.into(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// The polar bear wants grass only, likes the rock (name id 7) at 20 and dislikes the goat (name
/// id 5003) at -10; the shark and the goat have no likes.
fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let polar = "[Characteristics/Integers]\ncUnderwater=1\ncOnlyUnderwater=0\n\
                 [cSuitableObjects]\nv=7\nv=20\n[cCompatibleTerrain]\nv=0\nv=100\n\
                 [cCompatibleAnimals]\nv=5003\nv=-10\n";
    cat.push(kind("animals", "polar", 5001, polar));
    cat.push(kind("animals", "goat", 5003, ""));
    let shark = "[Characteristics/Integers]\ncUnderwater=1\ncOnlyUnderwater=1\n";
    cat.push(kind("animals", "shark", 5002, shark));
    cat.push(kind(
        "bush",
        "rock",
        7,
        "[Characteristics/Integers]\ncHabitat=9400\n",
    ));
    cat
}

/// Polar bears 1 in the tank and 3 on the land, shark 2 in the tank.
fn linked() -> World {
    amphibious_world(
        2,
        &[
            (1, "polar", (3, 3)),
            (2, "shark", (4, 3)),
            (3, "polar", (7, 3)),
        ],
    )
}

/// The number of the tank and of the land exhibit.
fn pair(w: &World) -> (u32, u32) {
    let region = |t: usize| w.exhibits().region(t).unwrap();
    (region(2 * WIDE + 2), region(2 * WIDE + 6))
}

fn rock(at: (i32, i32)) -> (crate::zoo::Entity, Option<Shape>) {
    let shape = Shape {
        kind: Kind::Fixed,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: TankKind::NONE,
        walkable: (false, false),
    };
    (entity("bush", "rock", 0, centre(at), 0), Some(shape))
}

fn clean(w: &mut World) {
    for e in w.exhibits_mut().list_mut() {
        e.rating.dirty = false;
    }
}

fn dirty(w: &World, x: u32) -> bool {
    w.exhibits().get(x).unwrap().rating.dirty
}

#[test]
fn the_land_reads_the_types_and_the_water_of_its_tank_and_rates_them_too() {
    let (w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let of_land = input(&w, &cat, land);
    let keys = |list: &[rating::Likes]| list.iter().map(|l| l.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(&of_land.species), ["polar"]);
    let mut near = keys(&of_land.near);
    near.sort();
    assert_eq!(near, ["polar", "shark"]);
    assert!(of_land.linked && of_land.near.iter().any(|l| l.amphibious));
    let water = w.exhibits().get(tank).unwrap().tank.unwrap();
    assert_eq!(of_land.neighbours.len(), 1);
    assert_eq!(of_land.neighbours[0].tiles, 12);
    assert_eq!(
        of_land.neighbours[0].water.unwrap().purity,
        water.purity as i32
    );
    let records = rate(&of_land);
    let polar = records.iter().find(|r| r.key == "polar").unwrap();
    assert_eq!(
        polar.animals, 1,
        "the bear in the tank is not counted on the land"
    );
    assert!(records.iter().any(|r| r.key == "shark"));
}

#[test]
fn marking_one_marks_its_neighbour_up_to_the_marked_ones() {
    let mut w = linked();
    let (tank, land) = pair(&w);
    clean(&mut w);
    w.exhibits_mut().mark(tank);
    assert!(dirty(&w, tank) && dirty(&w, land));
    // The grounds and an exhibit that is none mark nothing.
    clean(&mut w);
    w.exhibits_mut().mark(crate::exhibit::GROUNDS);
    w.exhibits_mut().mark(u32::MAX);
    assert!(!dirty(&w, tank) && !dirty(&w, land));
    // An object or a tile changed in an exhibit marks it; a guest does not.
    w.unrecorded(|w| w.add(entity("guests", "man", 0, centre((7, 2)), 0), None));
    assert!(!dirty(&w, land));
    let (r, shape) = rock((8, 3));
    w.unrecorded(|w| w.add(r, shape));
    assert!(dirty(&w, land) && dirty(&w, tank));
    clean(&mut w);
    let tile = w.zoo.tiles[3 * WIDE + 8];
    w.set_tile(8, 3, tile.elevation, tile.shape, 5);
    assert!(dirty(&w, land));
}

#[test]
fn the_bears_on_the_land_and_in_the_tank_like_a_rock_put_on_the_land_and_dislike_it_going() {
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let tile = 3 * WIDE + 8;
    // Each exhibit once, the grounds never.
    let before = rating::before(&w, &cat, &[tile, 0, tile]);
    assert_eq!(before.iter().map(|b| b.0).collect::<Vec<_>>(), [land]);
    let (r, shape) = rock((8, 3));
    let id = w.unrecorded(|w| w.add(r, shape)).unwrap();
    rating::react(&mut w, &cat, before, Some((9400, 7, 1)), &mut || 0);
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["likes 1", "likes 3"], "the shark is not amphibious");
    assert!(!dirty(&w, land) && !dirty(&w, tank), "rated again");
    // The reactions last 1250 ms.
    w.units.fade(1249);
    assert_eq!(w.units.reactions.len(), 2);
    w.units.fade(1);
    assert!(w.units.reactions.is_empty());
    let before = rating::before(&w, &cat, &[tile]);
    w.unrecorded(|w| w.remove(id));
    rating::react(&mut w, &cat, before, Some((9400, 7, -1)), &mut || 0);
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["dislikes 1", "dislikes 3"]);
    // Nothing changed: an object the species does not value leaves it alone.
    let before = rating::before(&w, &cat, &[tile]);
    rating::react(&mut w, &cat, before, Some((9400, 99, 1)), &mut || 0);
    assert!(w.units.take_animal_lines().is_empty());
}

#[test]
fn a_terrain_change_that_changes_no_tile_marks_nothing() {
    let (mut w, cat) = (linked(), catalogue());
    let (_, land) = pair(&w);
    clean(&mut w);
    rating::terrain(&mut w, &cat, &mut || 0, |_| {});
    assert!(!dirty(&w, land));
    // Grass painted under the bear: it wants grass only, so its sum goes up.
    w.zoo.tiles[3 * WIDE + 7].terrain = 3;
    let tile = w.zoo.tiles[3 * WIDE + 7];
    rating::terrain(&mut w, &cat, &mut || 0, |w| {
        w.set_tile(7, 3, tile.elevation, tile.shape, 0)
    });
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["likes 1", "likes 3"]);
}

#[test]
fn each_exhibit_is_rated_at_its_first_update_then_at_its_timer_or_when_marked() {
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(w.zoo.tiles.len(), Clocks::default());
    let mut rated: Vec<(u32, u32)> = Vec::new();
    let step = |w: &mut World, sim: &mut Sim, n: u32, rated: &mut Vec<(u32, u32)>| {
        sim.step_in(w, &cat, 67, |_, _, _| {});
        for line in sim.take_lines() {
            if let Some(rest) = line.strip_prefix("rate ") {
                rated.push((n, rest.split(' ').next().unwrap().parse().unwrap()));
            }
        }
    };
    step(&mut w, &mut sim, 0, &mut rated);
    rated.sort();
    assert_eq!(rated, [(0, tank.min(land)), (0, tank.max(land))]);
    // The next ones after 7 s, give or take the 0 to 199 ms of the draw.
    rated.clear();
    for n in 1..=110 {
        step(&mut w, &mut sim, n, &mut rated);
    }
    assert_eq!(rated.len(), 2, "{rated:?}");
    assert!(
        rated.iter().all(|&(n, _)| (102..=105).contains(&n)),
        "{rated:?}"
    );
    // A rock on the land marks both: both are rated at the next step.
    rated.clear();
    let (r, shape) = rock((8, 3));
    w.unrecorded(|w| w.add(r, shape));
    step(&mut w, &mut sim, 111, &mut rated);
    assert_eq!(rated.len(), 2);
}

#[test]
fn amphibious_is_under_water_and_not_only_under_water() {
    let amphibious = |keys: &str| {
        let text = format!("[Characteristics/Integers]\n{keys}\n");
        rating::Likes::of(&kind("animals", "a", 1, &text)).amphibious
    };
    assert!(amphibious("cUnderwater=1"));
    assert!(amphibious("cUnderwater=1\ncOnlyUnderwater=0"));
    for keys in [
        "",
        "cUnderwater=0",
        "cUnderwater=-1",
        "cUnderwater=2147483647",
    ] {
        assert!(!amphibious(keys), "{keys}");
    }
    assert!(!amphibious("cUnderwater=1\ncOnlyUnderwater=1"));
}

#[test]
fn a_tank_rates_the_species_of_its_land_neighbours() {
    let cat = catalogue();
    let w = amphibious_world(
        2,
        &[
            (1, "polar", (3, 3)),
            (2, "shark", (4, 3)),
            (3, "polar", (7, 3)),
            (4, "goat", (8, 3)),
        ],
    );
    let (tank, land) = pair(&w);
    // The tank reads the land's types and tiles (no water); the goat gets a record there.
    let of_tank = input(&w, &cat, tank);
    let keys = |list: &[rating::Likes]| list.iter().map(|l| l.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(&of_tank.near), ["polar", "goat"]);
    assert_eq!(of_tank.neighbours.len(), 1);
    assert_eq!(of_tank.neighbours[0].water, None);
    let records = rate(&of_tank);
    let keys: Vec<&str> = records.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["polar", "shark", "goat"]);
    assert_eq!((records[0].animals, records[2].animals), (1, 0));
    // The surrounding list stays the tank's own: its bear does not meet the goat, the land's does.
    assert_eq!(records[0].other, 0.0);
    let on_land = rate(&input(&w, &cat, land));
    assert_eq!(on_land[0].other, -10.0);
}

#[test]
fn a_wall_height_marks_only_a_filled_tank() {
    let mut w = linked();
    let (tank, land) = pair(&w);
    let set = |w: &mut World, filled: bool, gate: Option<u32>| {
        clean(w);
        let x = w.exhibits_mut().get_mut(tank).unwrap();
        (x.gate, x.rating.escape) = (gate, false);
        x.tank.as_mut().unwrap().filled = filled;
    };
    let escape = |w: &World| w.exhibits().get(tank).unwrap().rating.escape;
    // No gate, or drained: the escape check flag only.
    set(&mut w, true, None);
    w.set_height(tank, 3);
    assert!(!dirty(&w, tank) && !dirty(&w, land) && escape(&w));
    set(&mut w, false, Some(1));
    w.set_height(tank, 4);
    assert!(!dirty(&w, tank) && escape(&w));
    // The same height marks nothing and sets nothing.
    set(&mut w, true, Some(1));
    w.set_height(tank, 4);
    assert!(!dirty(&w, tank) && !escape(&w));
    // Filled with a gate: the tank, and the land its walls are level with again.
    w.set_height(tank, 2);
    assert!(dirty(&w, tank) && dirty(&w, land) && escape(&w));
    // The water level and the purity mark nothing.
    set(&mut w, false, Some(1));
    assert!(w.step_water());
    w.set_purity(tank, 50);
    w.set_purity(tank, 1000);
    assert!(!dirty(&w, tank) && !escape(&w));
}

#[test]
fn the_reachability_test_draws_its_target_and_raises_the_test_flag() {
    use crate::exhibit::viewing::{Plan, candidates};
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    // The land's east fence at (9, 3) is broken and the ground beyond it at the land's height.
    let piece = w.find(1000 + 9 * 10 + 3 + 2).unwrap();
    let common = w.zoo.entities[piece].common.as_mut().unwrap();
    common.fence.as_mut().unwrap().life = 0;
    w.set_tile(10, 3, 2, 0, 0);
    let target = 3 * WIDE + 10;
    // The candidates of 10.2 lose the tiles of the tank beside it.
    let all = candidates(&Plan::of(&w, land));
    let in_tank = |t: &usize| w.exhibits().region(*t) == Some(tank);
    assert!(all.iter().any(in_tank));
    let kept: Vec<usize> = all.iter().copied().filter(|t| !in_tank(t)).collect();
    let k = kept.iter().position(|&t| t == target).unwrap() as u32;
    let drawn = std::cell::Cell::new(0);
    let mut rand = || {
        drawn.set(drawn.get() + 1);
        k + kept.len() as u32
    };
    // A new exhibit has its flag set: the test goes on, draws once and clears it.
    assert_eq!(w.units.finder.state()[2], 2, "no search yet");
    rating::reach(&mut w, &cat, land, &mut rand);
    let x = w.exhibits().get(land).unwrap();
    assert!(!x.rating.escape && w.exhibits().tests == 0);
    assert_eq!(x.rating.escapes, [("polar/".to_string(), target)]);
    // The bear searched with its test flag up: the finder's diagonal switch is left off.
    assert_eq!(w.units.finder.state()[2], 0);
    // With the flag clear the test stops at once 29 times; the 30th goes on.
    for n in 1..30 {
        rating::reach(&mut w, &cat, land, &mut rand);
        assert_eq!(w.exhibits().tests, n);
    }
    rating::reach(&mut w, &cat, land, &mut rand);
    assert_eq!(w.exhibits().tests, 0);
    assert_eq!(drawn.get(), 2);
    // The fence mended, the bear stays in: the table is empty. A tank has a test of its own.
    w.zoo.entities[piece]
        .common
        .as_mut()
        .unwrap()
        .fence
        .as_mut()
        .unwrap()
        .life = 5;
    w.exhibits_mut().get_mut(land).unwrap().rating.escape = true;
    rating::reach(&mut w, &cat, land, &mut rand);
    assert!(w.exhibits().get(land).unwrap().rating.escapes.is_empty());
    rating::reach(&mut w, &cat, tank, &mut rand);
    assert_eq!(drawn.get(), 4);
}
