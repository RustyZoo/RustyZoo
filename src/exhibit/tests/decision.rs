//! The show decision of a tank made in play (shows 31.1, 31.2): the show info and its id, the wall
//! morph, the links to the plain tanks and the grandstand pass, on flat synthetic ground.

use super::tank::{put_wall, ring, tank_shape, tanks, wall};
use super::*;
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::show::{Move, info_of};
use crate::zoo::{Show, ShowInfo};
use std::sync::Arc;

/// Closes the square of tiles `x0..=x1` by `y0..=y1` with pieces of `class`, one by one.
fn build(world: &mut World, class: &'static str, (x0, y0, x1, y1): (i32, i32, i32, i32)) {
    world.begin();
    for (class, p) in ring(class, x0, y0, x1, y1) {
        put_wall(world, class, p);
    }
}

fn id_at(world: &World, x: usize, y: usize) -> u32 {
    world.tank_at(y * SIZE as usize + x).unwrap()
}

fn info(world: &World, x: usize, y: usize) -> Option<&ShowInfo> {
    info_of(world, id_at(world, x, y))
}

fn ids(world: &World) -> Vec<u16> {
    let mut ids: Vec<u16> = world
        .zoo
        .shows
        .infos
        .iter()
        .flatten()
        .map(|i| i.id)
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn a_show_tank_built_in_play_gets_a_new_show_info_with_the_next_id_even_when_the_build_is_declined()
{
    // 31.1 points 2 and 6, 31.2 points 1 and 6.
    let bench = [("bench", 3 * 64 + 16, 3 * 64 + 16)];
    let mut world = tanks(&[], &bench, &[]);
    world.units.ai_ticks = 500;
    build(&mut world, "show", (0, 0, 1, 1));
    let first = info(&world, 0, 0).unwrap();
    assert_eq!((first.id, first.frequency, first.next), (1001, 90, 590));
    assert_eq!(
        (first.index, first.schedule.len(), first.records.len()),
        (0, 0, 0)
    );
    assert!(first.events.is_empty() && first.show == Show::default());
    assert_eq!(
        (first.upkeep, first.receipts, first.attendance),
        ([0; 3], [0; 3], [0; 3])
    );
    build(&mut world, "show", (3, 0, 4, 1));
    assert_eq!(info(&world, 3, 0).unwrap().id, 1002);
    // The third has a bench in it: the question is open and the info is there already.
    build(&mut world, "show", (2, 3, 3, 4));
    assert_eq!(info(&world, 2, 3).unwrap().id, 1003);
    let declined = id_at(&world, 2, 3);
    world.decline_tank(declined);
    assert_eq!(
        ids(&world),
        [1001, 1002],
        "the declined tank takes its info away"
    );
    // Its number is not given back.
    build(&mut world, "show", (5, 5, 6, 6));
    assert_eq!(info(&world, 5, 5).unwrap().id, 1004);
    // A plain tank has none.
    build(&mut world, "tank", (0, 3, 1, 4));
    assert!(info(&world, 0, 3).is_none() && ids(&world).len() == 3);
}

#[test]
fn a_tank_that_already_has_a_show_info_keeps_it_through_a_decision_and_its_new_half_gets_its_own() {
    // 31.1 point 3 and 31.2 point 11: a show tank cut in two.
    let mut world = tanks(&[], &[], &[]);
    build(&mut world, "show", (2, 2, 5, 2));
    let n = crate::show::info_slot(&world, id_at(&world, 2, 2)).unwrap();
    world.zoo.shows.infos[n].as_mut().unwrap().frequency = 45;
    put_wall(&mut world, "show", (3, 2, 2));
    let (a, b) = (info(&world, 2, 2).unwrap(), info(&world, 5, 2).unwrap());
    let (kept, new) = if a.id == 1001 { (a, b) } else { (b, a) };
    assert_eq!(
        (kept.id, kept.frequency),
        (1001, 45),
        "the old info is kept"
    );
    assert_eq!((new.id, new.frequency), (1002, 90));
}

#[test]
fn a_tank_that_fails_the_show_test_loses_its_show_info_and_so_does_a_deleted_one() {
    // 31.1 point 5 and 31.5 point 1.
    let mut world = tanks(&[], &[], &[]);
    build(&mut world, "show", (2, 2, 3, 3));
    build(&mut world, "show", (5, 5, 6, 6));
    assert_eq!(ids(&world), [1001, 1002]);
    // A plain wall in the first: the lines no longer all pass.
    let piece = at(&world, 2, 2, 0);
    let plain = tank_shape(&Entity {
        class: "tank".into(),
        ..entity("tank", 2, 2, 0)
    });
    let mut wall = entity("tank", 2, 2, 0);
    wall.subtype = "f".into();
    world.swap(piece, wall, plain).unwrap();
    assert!(
        id_at(&world, 2, 2) > 0
            && !world
                .tank_at(2 * 8 + 2)
                .and_then(|t| world.tank(t))
                .unwrap()
                .show
    );
    assert_eq!(ids(&world), [1002]);
    // The second is opened to the grounds: it is gone, and its info with it.
    let line = at(&world, 5, 5, 0);
    assert!(world.remove(line));
    assert!(ids(&world).is_empty());
}

#[test]
fn the_plain_copy_of_a_wall_on_a_shared_line_becomes_a_show_wall_and_is_not_sold() {
    // 31.1 point 4.2: a show tank against a plain tank.
    let mut world = tanks(&[], &[], &[]);
    build(&mut world, "tank", (1, 2, 2, 3));
    world.begin();
    let mut pieces = ring("show", 3, 2, 4, 3);
    pieces.retain(|&(_, p)| !(p.0 == 3 && p.2 == 6));
    for (class, p) in pieces {
        put_wall(&mut world, class, p);
    }
    let show = id_at(&world, 3, 2);
    let class = |x: i32, y: i32, d: usize| {
        let t = world.tile_index(x, y).unwrap();
        world.zoo.entities[world.edge(t, d).unwrap()].class.clone()
    };
    assert_eq!(class(3, 2, 6), "show", "the copy is a show wall");
    assert_eq!(class(3, 3, 6), "show");
    assert_eq!(class(2, 2, 2), "tank", "the plain tank keeps its wall");
    assert!(world.refused(show).is_empty(), "nothing is sold");
    assert!(world.tank(show).unwrap().show);
}

fn dolphin() -> Catalogue {
    let text = "[Global]\nDefaultSubtype=m\n[m/tricks]\ntrick=tail\n[m/tricks/tail]\nid=11209\ntype=0\n\
        name=Tricktailwalk\ncomplexity=2\nsatisfaction=3\nsatisfactiondelta=3\nminimumdepth=0\n\
        defaultavailable=1\nkeeperpretrick=bTrickStart\n";
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "animals",
        type_name: b"dolphin".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 5402,
        footprint: [2, 2, 1],
        height: 0,
    });
    cat
}

/// A plain tank at (1..2, 2..3) on ground `plain` high, holding a dolphin, and a show tank built
/// against its east side: the two tanks.
fn dolphin_tanks(plain: i32) -> (World, Catalogue, u32, u32) {
    let high: Vec<(usize, i32)> = [(1, 2), (2, 2), (1, 3), (2, 3)]
        .map(|(x, y)| (y * SIZE as usize + x, plain))
        .into();
    let mut world = tanks(&[], &[], &high);
    build(&mut world, "tank", (1, 2, 2, 3));
    let mut e = entity("animals", 1, 2, 0);
    e.type_name = "dolphin".into();
    let animal = world.add(e.clone(), tank_shape(&e)).unwrap();
    world.take_moves();
    world.begin();
    let mut pieces = ring("show", 3, 2, 4, 3);
    pieces.retain(|&(_, p)| !(p.0 == 3 && p.2 == 6));
    for (class, p) in pieces {
        put_wall(&mut world, class, p);
    }
    let show = id_at(&world, 3, 2);
    (world, dolphin(), animal, show)
}

#[test]
fn a_show_tank_built_against_a_filled_plain_tank_is_linked_and_takes_its_dolphins_in() {
    // 31.2 point 12 (the portal flag is another issue) and point 10.
    let (mut world, cat, animal, show) = dolphin_tanks(0);
    let plain = id_at(&world, 1, 2);
    assert_eq!(world.exhibits().get(show).unwrap().show_near, [plain]);
    assert_eq!(world.exhibits().get(plain).unwrap().show_near, [show]);
    // Nothing but the link made a record: the units of the plain tank join with the next step.
    let n = crate::show::info_slot(&world, show).unwrap();
    assert!(
        world.zoo.shows.infos[n]
            .as_ref()
            .unwrap()
            .records
            .is_empty()
    );
    crate::show::settle(&mut world, &cat);
    let info = world.zoo.shows.infos[n].as_ref().unwrap();
    assert_eq!(info.schedule, [5402]);
    let record = info.record(5402).unwrap();
    assert_eq!(record.units, [animal]);
    assert!(
        world
            .zoo
            .shows
            .scripts
            .iter()
            .any(|s| s.id == record.script && s.items.len() == 1)
    );
    // The plain tank on ground one step higher: its walls end one step above: no link.
    let (world, _, _, show) = dolphin_tanks(1);
    assert!(world.exhibits().get(show).unwrap().show_near.is_empty());
}

#[test]
fn the_grandstands_take_the_show_id_with_the_last_decision_and_not_before() {
    // 31.2 points 1 and 9: the pass is noted when the name is answered, not when the info is made.
    let bench = [("bench", 3 * 64 + 16, 3 * 64 + 16)];
    let mut world = tanks(&[], &bench, &[]);
    let mut pieces = ring("show", 2, 3, 3, 4);
    let last = pieces.pop().unwrap();
    world.begin();
    world.take_moves();
    for (class, p) in pieces {
        put_wall(&mut world, class, p);
    }
    let e = wall(last.0, last.1);
    world.add(e.clone(), tank_shape(&e)).unwrap();
    let asked = world.take_moves();
    assert!(
        !asked.contains(&Move::Associate),
        "the question is open: {asked:?}"
    );
    assert!(info(&world, 2, 3).is_some());
    let id = id_at(&world, 2, 3);
    world.accept_tank(id);
    assert!(
        !world.take_moves().contains(&Move::Associate),
        "the name is open"
    );
    world.give_gate(id);
    assert!(world.take_moves().contains(&Move::Associate));
}

#[test]
fn the_counter_of_a_loaded_zoo_moves_up_for_each_of_its_show_tanks() {
    // 31.6 point 5: the load made one fresh id for each, which the stored info replaced.
    let mut pieces = ring("show", 2, 2, 3, 3);
    pieces.extend(ring("show", 5, 5, 6, 6));
    pieces.extend(ring("tank", 0, 5, 1, 6));
    let mut world = tanks(&pieces, &[], &[]);
    assert_eq!(world.zoo.shows.next_show, 1002);
    assert!(
        ids(&world).is_empty(),
        "the stored infos come from the file"
    );
    build(&mut world, "show", (0, 0, 1, 1));
    assert_eq!(info(&world, 0, 0).unwrap().id, 1003);
}
