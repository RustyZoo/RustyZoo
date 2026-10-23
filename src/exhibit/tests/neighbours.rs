//! The rating of a tank and the land exhibit beside it, linked as amphibious neighbours (habitat
//! rating 4, 5.3, 9.2, 9.4): the tank of `amphibious_world` from (2, 2) and the land exhibit from
//! (6, 2), each 4 by 3 tiles, with polar bears (amphibious) and a shark (only under water).

use std::sync::Arc;

use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::exhibit::rating::{self, input, rate};
use crate::render::order::Kind;
use crate::sim::Sim;
use crate::unit::testing::{amphibious_world, amphibious_world_between, centre, entity, gated};
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
                 [cSuitableObjects]\nv=7\nv=20\nv=8\nv=-30\nv=9\nv=-30\n[cCompatibleTerrain]\nv=0\nv=100\n\
                 [cCompatibleAnimals]\nv=5003\nv=-10\n";
    cat.push(kind("animals", "polar", 5001, polar));
    cat.push(kind("objects", "thorn", 8, ""));
    cat.push(kind("objects", "briar", 9, ""));
    cat.push(kind("animals", "goat", 5003, ""));
    cat.push(kind("fences", "iron", 0, ""));
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
    // Each exhibit once; the grounds of an entity's change are marked and flagged, with no
    // sums, those of a terrain change are not.
    let grounds = crate::exhibit::GROUNDS;
    let g = &mut w.exhibits_mut().grounds;
    (g.dirty, g.revise) = (false, false);
    let before = rating::before(&mut w, &cat, &[tile, 0], false, &mut || 0);
    assert_eq!(before.iter().map(|b| b.0).collect::<Vec<_>>(), [land]);
    let g = &w.exhibits().grounds;
    assert!(!g.dirty && !g.revise);
    let before = rating::before(&mut w, &cat, &[tile, 0, tile], true, &mut || 0);
    assert_eq!(
        before.iter().map(|b| b.0).collect::<Vec<_>>(),
        [land, grounds]
    );
    assert!(before[1].1.is_empty());
    let g = &mut w.exhibits_mut().grounds;
    assert!(g.dirty && g.revise);
    (g.dirty, g.revise) = (false, false);
    let (r, shape) = rock((8, 3));
    let id = w.unrecorded(|w| w.add(r, shape)).unwrap();
    rating::react(
        &mut w,
        &cat,
        before,
        Some(rating::Change::Object {
            habitat: 9400,
            name: 7,
            sign: 1,
        }),
        &mut || 0,
    );
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(
        lines,
        ["likes 1", "likes 3", "sound smile"],
        "the shark is not amphibious"
    );
    assert_eq!(w.units.take_cheers(), ["smile"], "once for the change");
    let g = &w.exhibits().grounds;
    assert!(g.dirty && g.revise, "the grounds are marked again after");
    assert!(!dirty(&w, land) && !dirty(&w, tank), "rated again");
    // The reactions last 1250 ms.
    w.units.fade(1249);
    assert_eq!(w.units.reactions.len(), 2);
    w.units.fade(1);
    assert!(w.units.reactions.is_empty());
    let before = rating::before(&mut w, &cat, &[tile], true, &mut || 0);
    w.unrecorded(|w| w.remove(id));
    rating::react(
        &mut w,
        &cat,
        before,
        Some(rating::Change::Object {
            habitat: 9400,
            name: 7,
            sign: -1,
        }),
        &mut || 0,
    );
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["dislikes 1", "dislikes 3", "sound frown"]);
    assert_eq!(w.units.take_cheers(), ["frown"]);
    // Nothing changed: an object the species does not value leaves it alone.
    let before = rating::before(&mut w, &cat, &[tile], true, &mut || 0);
    rating::react(
        &mut w,
        &cat,
        before,
        Some(rating::Change::Object {
            habitat: 9400,
            name: 99,
            sign: 1,
        }),
        &mut || 0,
    );
    assert!(w.units.take_animal_lines().is_empty());
    assert!(w.units.take_cheers().is_empty());
}

#[test]
fn a_species_of_eggs_only_likes_a_change_without_a_reaction_or_a_sound() {
    use crate::unit::Part;
    let (mut w, cat) = (linked(), catalogue());
    for id in [1, 3] {
        if let Some(Part::Animal(a)) = w.units.get_mut(id).map(|u| &mut u.part) {
            a.egg = true;
        }
    }
    let tile = 3 * WIDE + 8;
    let before = rating::before(&mut w, &cat, &[tile], true, &mut || 0);
    let (r, shape) = rock((8, 3));
    w.unrecorded(|w| w.add(r, shape)).unwrap();
    rating::react(
        &mut w,
        &cat,
        before,
        Some(rating::Change::Object {
            habitat: 9400,
            name: 7,
            sign: 1,
        }),
        &mut || 0,
    );
    assert!(w.units.take_animal_lines().is_empty());
    assert!(w.units.take_cheers().is_empty());
    assert!(w.units.reactions.is_empty());
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
    assert_eq!(lines, ["likes 1", "likes 3", "sound smile"]);
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
    assert!(!x.rating.escape && rating::tests() == 0);
    assert_eq!(x.rating.escapes, [("polar/".to_string(), target)]);
    // The bear searched with its test flag up: the finder's diagonal switch is left off.
    assert_eq!(w.units.finder.state()[2], 0);
    // With the flag clear the test stops at once 29 times; the 30th goes on.
    for n in 1..30 {
        rating::reach(&mut w, &cat, land, &mut rand);
        assert_eq!(rating::tests(), n);
    }
    rating::reach(&mut w, &cat, land, &mut rand);
    assert_eq!(rating::tests(), 0);
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

#[test]
fn the_escape_check_flag_is_set_by_its_changes() {
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let flag = |w: &World, x: u32| w.exhibits().get(x).unwrap().rating.escape;
    let clear = |w: &mut World| {
        for e in w.exhibits_mut().list_mut() {
            e.rating.escape = false;
        }
    };
    // A new exhibit has it; a recomputation keeps it unless the counts change.
    assert!(flag(&w, land));
    let mut state = rating::State::new();
    state.compute(&input(&w, &cat, land));
    state.escape = false;
    state.compute(&input(&w, &cat, land));
    assert!(!state.escape);
    state.compute(&rating::Input::default());
    assert!(state.escape, "no species and no animals any more");
    // A terrain change on its tiles.
    clear(&mut w);
    let tile = w.zoo.tiles[3 * WIDE + 8];
    w.set_tile(8, 3, tile.elevation, tile.shape, 5);
    assert!(flag(&w, land) && !flag(&w, tank));
    // A fence piece that breaks sets it on its side (the other is the grounds); one that decays
    // only when a species of the exhibit bashes.
    let piece = w.find(1000 + 9 * 10 + 3 + 2).unwrap();
    for (broken, bash, set) in [
        (true, false, true),
        (false, false, false),
        (false, true, true),
    ] {
        clear(&mut w);
        w.exhibits_mut().get_mut(land).unwrap().rating.bash = bash;
        w.fence_changed(piece, broken);
        assert_eq!(flag(&w, land), set, "{broken} {bash}");
    }
    // Making the piece healthy is such a change too, whatever called it.
    for (bash, set) in [(false, false), (true, true)] {
        clear(&mut w);
        w.exhibits_mut().get_mut(land).unwrap().rating.bash = bash;
        w.heal(&cat, 1000 + 9 * 10 + 3 + 2, &mut || 0).unwrap();
        assert_eq!(flag(&w, land), set, "heal {bash}");
    }
    // A species bashes with `cBashStrength` above 0; missing, 0 or negative, it does not.
    let bashes = |keys: &str| {
        let text = format!("[Characteristics/Integers]\n{keys}\n");
        rating::Likes::of(&kind("animals", "a", 1, &text)).bash
    };
    assert!(bashes("cBashStrength=1") && bashes("cBashStrength=2147483647"));
    for keys in ["", "cBashStrength=0", "cBashStrength=-1"] {
        assert!(!bashes(keys), "{keys}");
    }
}

#[test]
fn the_reachability_counter_belongs_to_the_program_not_to_the_world() {
    let cat = catalogue();
    let (mut w, mut rand) = (linked(), || 0);
    let (_, land) = pair(&w);
    for _ in 0..5 {
        w.exhibits_mut().get_mut(land).unwrap().rating.escape = false;
        rating::reach(&mut w, &cat, land, &mut rand);
    }
    assert_eq!(rating::tests(), 5);
    // A second zoo loaded in the same run goes on from the count the first left.
    let mut second = linked();
    let (_, land) = pair(&second);
    second.exhibits_mut().get_mut(land).unwrap().rating.escape = false;
    rating::reach(&mut second, &cat, land, &mut rand);
    assert_eq!(rating::tests(), 6);
}

/// Rates exhibit `x` and settles the bits of its animals, as the world manager does.
fn settle(w: &mut World, cat: &Catalogue, x: u32) {
    let input = input(w, cat, x);
    w.exhibits_mut().get_mut(x).unwrap().rating.compute(&input);
    crate::animal::home::settle(w, cat, x);
}

fn bits(w: &World, id: u32) -> (u64, u64) {
    match &w.units.get(id).unwrap().part {
        crate::unit::Part::Animal(a) => (a.advice, a.important),
        _ => panic!("not an animal"),
    }
}

fn home(w: &World, cat: &Catalogue, id: u32) -> Option<u32> {
    let at = w.find(id).unwrap();
    let likes = rating::Likes::of(crate::panel::exhibit::type_at(w, cat, at)?);
    crate::animal::home::home(
        w,
        cat,
        crate::panel::exhibit::animals_exhibit(w, at)?,
        &likes,
    )
}

fn give_gate(w: &mut World, x: u32) {
    w.exhibits_mut().get_mut(x).unwrap().gate = Some(u32::MAX);
}

#[test]
fn the_land_score_is_the_five_parts() {
    use crate::animal::home::land_score;
    let part = |f: fn(&mut rating::Record)| {
        let mut r = rating::Record::default();
        f(&mut r);
        land_score(&r)
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
    assert!(near(part(|r| r.terrain = 100.0), 30.0));
    assert!(near(part(|r| r.objects = 100.0), 20.0));
    assert!(near(part(|r| r.foliage = 100.0), 10.0));
    assert!(near(part(|r| r.rocks = 100.0), 10.0));
    assert!(near(part(|r| r.elevation = 100.0), 10.0));
    // The shelters and toys are not in it, and the rating has them.
    let mut r = rating::Record {
        shelters: 100.0,
        toys: -100.0,
        ..Default::default()
    };
    assert_eq!(land_score(&r), 0.0);
    r.shelters = -150.0;
    r.toys = 0.0;
    assert_eq!(land_score(&r), 0.0);
    assert!(r.rating() < 0.0);
    // All five at once.
    let all = rating::Record {
        terrain: 90.0,
        objects: 40.0,
        foliage: 50.0,
        rocks: 20.0,
        elevation: 70.0,
        ..Default::default()
    };
    assert!(near(land_score(&all), 27.0 + 8.0 + 5.0 + 2.0 + 7.0));
}

/// The tank and the land beside it on each side: the east one with three unused shelters (its
/// rating loses 15), the west one with a tile of other ground (its land score is lower). With the
/// east one the home, the better by rating is the west one.
fn between() -> (World, Catalogue, [u32; 3]) {
    let mut cat = catalogue();
    cat.push(kind(
        "building",
        "lean",
        8104,
        "[Characteristics/Integers]\ncShelter=1\ncCapacity=2\n",
    ));
    let mut w = gated(amphibious_world_between(
        2,
        &[(1, "polar", (3, 3)), (3, "polar", (7, 3))],
    ));
    for x in 6..9 {
        let shape = rock((x, 2)).1;
        w.unrecorded(|w| w.add(entity("building", "lean", 0, centre((x, 2)), 0), shape));
    }
    w.zoo.tiles[3 * WIDE].terrain = 3;
    let region = |t: usize| w.exhibits().region(t).unwrap();
    let ids = [region(2 * WIDE + 2), region(2 * WIDE + 6), region(2 * WIDE)];
    (w, cat, ids)
}

#[test]
fn the_panel_shows_the_best_neighbour_rating() {
    let (w, cat, [tank, east, west]) = between();
    let polar = |x| {
        let records = rate(&input(&w, &cat, x));
        records.into_iter().find(|r| r.key == "polar").unwrap()
    };
    let (e, o) = (polar(east), polar(west));
    assert!(crate::animal::home::land_score(&e) > crate::animal::home::land_score(&o));
    assert!(e.rating() < o.rating(), "{} {}", e.rating(), o.rating());
    assert_eq!(w.exhibits().get(tank).unwrap().amphibious.len(), 2);
    // The home is the better by the five parts, and the bar shows the better by the full rating.
    assert_eq!(home(&w, &cat, 1), Some(east));
    let at = w.find(1).unwrap();
    assert_eq!(
        crate::panel::animal::suitability(&w, &cat, at),
        o.shown() as i32
    );
    assert!(o.shown() as i32 > e.shown() as i32);
}

#[test]
fn the_plaque_lists_the_scenery_of_the_exhibit_under_the_animal() {
    let (mut w, cat) = (gated(linked()), catalogue());
    let (tank, land) = pair(&w);
    // The tank has a thorn the bears dislike, the land a briar and a patch of other ground.
    let mut put = |name: &str, tile: (i32, i32)| {
        let shape = rock(tile).1;
        w.unrecorded(|w| w.add(entity("objects", name, 0, centre(tile), 0), shape));
    };
    put("thorn", (3, 2));
    put("briar", (8, 3));
    w.zoo.tiles[2 * WIDE + 7].terrain = 3;
    let lines = |w: &World, id| crate::panel::exhibit::plaque_lines(w, &cat, w.find(id).unwrap());
    let objects =
        |l: &[crate::panel::exhibit::Line]| l.iter().filter_map(|l| l.2).collect::<Vec<_>>();
    assert_eq!(home(&w, &cat, 1), Some(land));
    assert_eq!(w.exhibits().region(2 * WIDE + 2), Some(tank));
    // The bear that swims lists the tank's thorn; the one on the land, its own briar.
    assert_eq!(objects(&lines(&w, 1)), [8]);
    assert_eq!(objects(&lines(&w, 3)), [9]);
    // The bits are the stored ones, written by the passes: the land bear has too much of the
    // other ground; the one that swims in the tank is not written by either pass.
    settle(&mut w, &cat, land);
    settle(&mut w, &cat, tank);
    let too_much = 9900 + 1 + rating::bit::terrain(3) as i32 + 1;
    assert!(lines(&w, 3).iter().any(|l| l.0 == too_much));
    assert!(!lines(&w, 1).iter().any(|l| l.0 == too_much));
}

#[test]
fn the_plaque_shows_the_stored_sets_without_a_rating() {
    let (mut w, cat) = (gated(linked()), catalogue());
    let lines = |w: &World, id| crate::panel::exhibit::plaque_lines(w, &cat, w.find(id).unwrap());
    let hungry = (9900 + 1, true, None);
    assert!(!lines(&w, 3).contains(&hungry));
    // A bit the hunger check sets shows at once, in the important group, with no exhibit change.
    if let crate::unit::Part::Animal(a) = &mut w.units.get_mut(3).unwrap().part {
        (a.advice, a.important) = (1, 1);
    }
    assert_eq!(lines(&w, 3)[0], hungry);
    // Another animal's plaque does not show it.
    assert!(!lines(&w, 1).contains(&hungry));
}

#[test]
fn the_plaque_of_an_egg_has_the_egg_line_and_no_scenery() {
    let (mut w, cat) = (gated(linked()), catalogue());
    let shape = rock((8, 3)).1;
    w.unrecorded(|w| w.add(entity("objects", "briar", 0, centre((8, 3)), 0), shape));
    let lines = |w: &World, id| crate::panel::exhibit::plaque_lines(w, &cat, w.find(id).unwrap());
    assert!(lines(&w, 3).iter().any(|l| l.2.is_some()));
    if let crate::unit::Part::Animal(a) = &mut w.units.get_mut(3).unwrap().part {
        a.egg = true;
    }
    assert_eq!(lines(&w, 3), [(9965, false, None)]);
}

#[test]
fn an_exhibit_without_a_gate_is_nobodys_home() {
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let stale = !(1u64 << 3);
    let set = |w: &mut World, id| {
        if let crate::unit::Part::Animal(a) = &mut w.units.get_mut(id).unwrap().part {
            (a.advice, a.important) = (stale, stale);
        }
    };
    let (bear, land_bear, shark) = (1, 3, 2);
    let same = |w: &World, id| bits(w, id) == (stale, stale);
    // No gate anywhere: nobody has a home, and the bits of the bear in the tank are kept; a
    // species that is not amphibious and a land exhibit are rewritten, as the rule says.
    for id in [bear, land_bear, shark] {
        assert_eq!(home(&w, &cat, id), None);
        set(&mut w, id);
    }
    settle(&mut w, &cat, tank);
    assert!(same(&w, bear) && !same(&w, shark));
    settle(&mut w, &cat, land);
    assert!(!same(&w, land_bear));
    // The land has a gate, the tank not: the bear in the tank has the land for its home, the
    // shark has none.
    give_gate(&mut w, land);
    assert_eq!(home(&w, &cat, bear), Some(land));
    assert_eq!(home(&w, &cat, land_bear), Some(land));
    assert_eq!(home(&w, &cat, shark), None);
    // The tank has one, the land not: the land neighbour is the candidate, and no other is tried.
    w.exhibits_mut().get_mut(land).unwrap().gate = None;
    give_gate(&mut w, tank);
    assert_eq!(home(&w, &cat, bear), None);
    assert_eq!(home(&w, &cat, shark), Some(tank));
    set(&mut w, bear);
    settle(&mut w, &cat, tank);
    assert!(same(&w, bear));
}

#[test]
fn the_home_is_read_when_asked() {
    // Nothing was rated: a loaded zoo has its homes right away.
    let (mut w, cat, [tank, east, west]) = between();
    assert_eq!(w.exhibits().region(2 * WIDE + 2), Some(tank));
    assert_eq!(home(&w, &cat, 1), Some(east));
    assert_eq!(home(&w, &cat, 3), Some(east));
    // The bear that moves onto the other land is at home there at once, and back in the tank
    // it has the better land again.
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre((0, 3));
    assert_eq!(home(&w, &cat, 1), Some(west));
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = centre((3, 3));
    assert_eq!(home(&w, &cat, 1), Some(east));
}

#[test]
fn an_amphibious_animal_in_a_linked_tank_keeps_the_bits_of_its_land_home() {
    let (mut w, cat) = (gated(linked()), catalogue());
    let (tank, land) = pair(&w);
    // The bear on the land is at home there and takes the bits of the land record, keeping the
    // bit of the animal checks.
    if let crate::unit::Part::Animal(a) = &mut w.units.get_mut(3).unwrap().part {
        (a.advice, a.important) = (1 << 3 | 1 << 20, 1 << 20);
    }
    settle(&mut w, &cat, land);
    let (advice, _) = bits(&w, 3);
    let advice = advice & !(1 << 3);
    assert_eq!(home(&w, &cat, 3), Some(land));
    let record = rate(&input(&w, &cat, land));
    assert_eq!(
        advice,
        record.iter().find(|r| r.key == "polar").unwrap().advice
    );
    // The bear that swims in the tank beside it has the land for its home, and a rating of the
    // tank leaves its bits alone: the animal checks' bit 3 and the pass's bit 20 stay.
    if let crate::unit::Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        (a.advice, a.important) = (1 << 3 | 1 << 20, 1 << 20);
    }
    settle(&mut w, &cat, tank);
    assert_eq!(home(&w, &cat, 1), Some(land));
    assert_eq!(bits(&w, 1), (1 << 3 | 1 << 20, 1 << 20));
    // The shark is not amphibious: the tank is its home and its bits are the tank's.
    let shark = rate(&input(&w, &cat, tank));
    let want = shark.iter().find(|r| r.key == "shark").unwrap();
    assert_eq!(bits(&w, 2), (want.advice, want.important));
    assert_eq!(home(&w, &cat, 2), Some(tank));
    // Without a land neighbour the bear is at home in the tank and takes its bits.
    w.exhibits_mut().get_mut(tank).unwrap().amphibious.clear();
    settle(&mut w, &cat, tank);
    assert_eq!(home(&w, &cat, 1), Some(tank));
    assert_ne!(bits(&w, 1).0 & 1 << 63, 0, "no land access");
}

/// The `catalogue` and an owl with an ambient sound, a goat-hating bear is the polar bear, and an
/// eel that wants water 6 to 9 deep.
fn zoo_catalogue() -> Catalogue {
    let mut cat = catalogue();
    cat.push(kind(
        "animals",
        "owl",
        5008,
        "[Characteristics/Strings]\ncAmbients=owl.wav\n",
    ));
    let eel = "[Characteristics/Integers]\ncUnderwater=1\ncOnlyUnderwater=1\n\
               cDepthMin=6\ncDepthMax=9\n";
    cat.push(kind("animals", "eel", 5009, eel));
    cat
}

fn draw_counter() -> (std::rc::Rc<std::cell::Cell<u32>>, impl FnMut() -> u32) {
    let n = std::rc::Rc::new(std::cell::Cell::new(0));
    let draws = n.clone();
    (n, move || {
        draws.set(draws.get() + 1);
        draws.get() * 1000 + 5
    })
}

#[test]
fn the_rating_that_follows_a_change_revises_the_species_list_first() {
    let cat = zoo_catalogue();
    let mut w = amphibious_world(2, &[(1, "polar", (7, 3)), (4, "owl", (8, 3))]);
    let (_, land) = pair(&w);
    let tile = 3 * WIDE + 8;
    assert!(w.exhibits().get(land).unwrap().rating.sounds.is_empty());
    // The sums before a change are read through a recomputation: the exhibit is marked and rated,
    // and the list is revised first: the first draw picks the tile of the owl's sound, the later
    // ones are the reachability test's.
    let (draws, mut rand) = draw_counter();
    let before = rating::before(&mut w, &cat, &[tile], true, &mut rand);
    let rating = &w.exhibits().get(land).unwrap().rating;
    assert!(!rating.dirty && !rating.revise);
    assert_eq!(rating.sounds, [("owl/".to_string(), 1005 % 12)]);
    assert_eq!(rating.types, ["polar/", "owl/"]);
    assert!(
        draws.get() >= 2,
        "the reachability test draws after the list"
    );
    assert_eq!(before[0].0, land);
    assert!(
        !before[0].1.is_empty(),
        "the sums of the recomputed records"
    );
    // A staff member placed: no reaction, but the exhibit is marked and rated again; the type is
    // known, so no sound tile is drawn.
    let sounds = &mut w.exhibits_mut().get_mut(land).unwrap().rating.sounds;
    *sounds = vec![("owl/".into(), 0)];
    rating::react(&mut w, &cat, before, None, &mut || 7);
    let rating = &w.exhibits().get(land).unwrap().rating;
    assert!(!rating.dirty && !rating.revise);
    assert_eq!(rating.sounds, [("owl/".to_string(), 0)]);
    assert!(w.units.take_animal_lines().is_empty() && w.units.take_cheers().is_empty());
}

#[test]
fn an_animal_placed_reacts_by_the_compatible_animals_of_the_species() {
    let (mut w, cat) = (linked(), catalogue());
    let tile = 3 * WIDE + 8;
    let before = rating::before(&mut w, &cat, &[tile], true, &mut || 0);
    let goat = w.unrecorded(|w| w.add(entity("animals", "goat", 5, centre((8, 3)), 0), None));
    assert!(goat.is_some());
    // The bears dislike the goat (-10): the bear of the land and, being amphibious, the one of
    // the tank; the goat has no likes of its own.
    let change = rating::Change::Animal {
        ids: [5003, 0, 0],
        sign: 1,
    };
    rating::react(&mut w, &cat, before, Some(change), &mut || 0);
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["dislikes 1", "dislikes 3", "sound frown"]);
    assert_eq!(w.units.take_cheers(), ["frown"]);
    // The goat removed again: the bears like it, the sign flipped.
    let before = rating::before(&mut w, &cat, &[tile], true, &mut || 0);
    w.unrecorded(|w| w.remove(goat.unwrap()));
    let change = rating::Change::Animal {
        ids: [5003, 0, 0],
        sign: -1,
    };
    rating::react(&mut w, &cat, before, Some(change), &mut || 0);
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["likes 1", "likes 3", "sound smile"]);
    w.units.take_cheers();
    // An animal nobody has an opinion about changes nothing.
    let before = rating::before(&mut w, &cat, &[tile], true, &mut || 0);
    rating::react(
        &mut w,
        &cat,
        before,
        Some(rating::Change::Animal {
            ids: [1, 2, 3],
            sign: 1,
        }),
        &mut || 0,
    );
    assert!(w.units.take_animal_lines().is_empty());
}

#[test]
fn a_new_wall_height_of_a_filled_tank_with_a_gate_runs_the_terrain_pair() {
    let cat = zoo_catalogue();
    let new = || {
        let mut w = amphibious_world(2, &[(1, "polar", (3, 3)), (2, "eel", (4, 3))]);
        let tank = pair(&w).0;
        w.exhibits_mut().get_mut(tank).unwrap().gate = Some(1000);
        w.take_events();
        (w, tank)
    };
    let (mut w, tank) = new();
    w.set_height(tank, 3);
    let depth = w.take_events().into_iter().find_map(|e| match e {
        crate::world::Event::Depth { id, from, to } => Some((id, from, to)),
        _ => None,
    });
    assert_eq!(depth, Some((tank, 2, 3)));
    let level = w.tank(tank).unwrap().level;
    rating::depth(&mut w, &cat, &mut || 0, tank, 2, 3);
    // The eel wants deeper water: it likes the new depth; the bear has no wish.
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["likes 2", "sound smile"]);
    assert_eq!(
        w.tank(tank).unwrap().level,
        level,
        "the water level is put back"
    );
    assert!(dirty(&w, tank), "the tank is marked again");
    // Lowering the walls is disliked.
    w.set_height(tank, 2);
    rating::depth(&mut w, &cat, &mut || 0, tank, 3, 2);
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["dislikes 2", "sound frown"]);
    // No gate, not filled, or the same height: no event.
    let (mut w, tank) = new();
    w.exhibits_mut().get_mut(tank).unwrap().gate = None;
    w.set_height(tank, 3);
    assert!(
        w.take_events()
            .iter()
            .all(|e| !matches!(e, crate::world::Event::Depth { .. }))
    );
    let (mut w, tank) = new();
    w.set_height(tank, 2);
    assert!(w.take_events().is_empty());
    w.exhibits_mut()
        .get_mut(tank)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .filled = false;
    w.set_height(tank, 4);
    assert!(
        w.take_events()
            .iter()
            .all(|e| !matches!(e, crate::world::Event::Depth { .. }))
    );
    // A tank that is gone, and levels at the limits, do not panic.
    rating::depth(&mut w, &cat, &mut || 0, 99, 0, u32::MAX);
    rating::depth(&mut w, &cat, &mut || 0, tank, u32::MAX, 0);
    rating::depth(&mut w, &cat, &mut || 0, tank, 0, u32::MAX);
}

#[test]
fn a_rock_put_in_the_tank_is_read_by_the_land_beside_it_too() {
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let tile = 3 * WIDE + 3;
    // The tank first, then its neighbour, which compares its amphibious species only.
    let before = rating::entity(&mut w, &cat, Some(tile), true, &mut || 0);
    let read: Vec<(u32, bool)> = before.iter().map(|b| (b.0, b.2)).collect();
    assert_eq!(read, [(tank, false), (land, true)]);
    let (r, shape) = rock((3, 3));
    w.unrecorded(|w| w.add(r, shape)).unwrap();
    let change = rating::Change::Object {
        habitat: 9400,
        name: 7,
        sign: 1,
    };
    rating::react(&mut w, &cat, before, Some(change), &mut || 0);
    let mut lines = w.units.take_animal_lines();
    lines.sort();
    assert_eq!(lines, ["likes 1", "likes 1", "likes 3", "sound smile"]);
    // A fence or a staff member reads the tank alone.
    let before = rating::entity(&mut w, &cat, Some(tile), false, &mut || 0);
    assert_eq!(before.iter().map(|b| b.0).collect::<Vec<_>>(), [tank]);
}

#[test]
fn a_terrain_change_marks_every_exhibit_before_it_reads_any() {
    let (mut w, cat) = (linked(), catalogue());
    let (tank, land) = pair(&w);
    let tiles = [3 * WIDE + 8, 3 * WIDE + 3];
    // A first change settles the species lists.
    let before = rating::before(&mut w, &cat, &tiles, false, &mut || 0);
    rating::react(&mut w, &cat, before, None, &mut || 0);
    for e in w.exhibits_mut().list_mut() {
        e.rating.escape = false;
    }
    // The land read first computes the tank, already flagged, which its own read finds clean:
    // two recomputations, not three.
    let tests = rating::tests();
    let before = rating::before(&mut w, &cat, &tiles, false, &mut || 0);
    assert_eq!(before.iter().map(|b| b.0).collect::<Vec<_>>(), [land, tank]);
    assert_eq!(rating::tests() - tests, 2);
}
