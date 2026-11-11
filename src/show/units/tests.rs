//! The trick lists, the default script and the show unit lists (shows 19, 20).

use super::*;
use crate::config::Config;
use crate::exhibit::rating;
use crate::show::trick::{FOOD, default_items, tricks};
use crate::unit::testing::{centre, entity, shape, show_world};
use crate::zoo::{Show, ShowInfo};
use std::sync::Arc;

/// The dolphin's file: the default subtype `m` lists a hoop (needs its toy, 4 deep), a walk, a ball
/// (not available until researched), a spin (invisible), the same walk again, a trick with no
/// section and the treat; the young list only empties.
const DOLPHIN: &str = "[Global]\nDefaultSubtype=m\n\
    [m/tricks]\ntrick=empty\ntrick=hoop\ntrick=tail\ntrick=ball\ntrick=spin\ntrick=tail\n\
    trick=ghost\ntrick=food\n[y/tricks]\ntrick=empty\n\
    [m/tricks/food]\nid=11299\ntype=3\nname=foodtrick\ncomplexity=0\nsatisfaction=0\n\
    satisfactiondelta=0\nminimumdepth=3\ndefaultavailable=1\nkeeperposttrick=bTrickFeed\nvisible=0\n\
    [m/tricks/hoop]\nid=11208\ntype=1\nname=hoopjump\ncomplexity=5\nsatisfaction=4\n\
    satisfactiondelta=3\nminimumdepth=4\nbuilding=6503\ndefaultavailable=1\n\
    keeperpretrick=bTrickStart\nnormalHelpID=11308\ngrayedHelpID=11408\n\
    normalIcon=objects/hoop\ngrayedIcon=objects/hoop2\n\
    [m/tricks/tail]\nid=11209\ntype=0\nname=Tricktailwalk\ncomplexity=2\nsatisfaction=3\n\
    satisfactiondelta=3\nminimumdepth=2\ndefaultavailable=1\nkeeperpretrick=bTrickStart\n\
    [m/tricks/ball]\nid=11210\ntype=1\nname=ballplay\ncomplexity=3\nsatisfaction=4\n\
    minimumdepth=4\nbuilding=6501\ndefaultavailable=0\n\
    [m/tricks/spin]\nid=11214\ntype=0\ncomplexity=2\ndefaultavailable=1\nvisible=0\n";

fn animal(name: &str, sub: &str, id: i32, text: &str) -> EntityType {
    EntityType {
        class: "animals",
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: id,
        footprint: [2, 2, 1],
        height: 0,
    }
}

fn toy(name: &str, id: i32) -> EntityType {
    EntityType {
        class: "building",
        subtype: Vec::new(),
        ..animal(name, "", id, "[Satisfies]\nbuilding\nshowtrick\n")
    }
}

fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(animal("dolphin", "", 5402, DOLPHIN));
    cat.push(animal("dolphin", "y", 5402, DOLPHIN));
    cat.push(animal("fish", "", 100, "[Global]\n"));
    cat.push(toy("hoop", 6503));
    cat.push(toy("ball", 6501));
    cat
}

fn info(id: u16) -> ShowInfo {
    ShowInfo {
        frequency: 90,
        index: 0,
        schedule: Vec::new(),
        records: Vec::new(),
        events: Vec::new(),
        next: 0,
        id,
        upkeep: [0; 3],
        receipts: [0; 3],
        attendance: [0; 3],
        show: Show::default(),
    }
}

/// The show-neighbour map: the plain tank `T` and the show tank `S` (show 1001), 4 deep and full,
/// with the animals `(id, type, tile)` standing at the start and no news for the show lists.
fn world(animals: &[(u32, &str, (i32, i32))]) -> (World, Catalogue) {
    let mut w = show_world(animals);
    w.zoo.habitat_tiles = vec![(6, 2)];
    w.zoo.shows.infos = vec![Some(info(1001))];
    w.take_moves();
    (w, types())
}

/// The show tank and the plain tank.
fn tanks(w: &World) -> (u32, u32) {
    (
        w.tank_at(2 * 12 + 6).unwrap(),
        w.tank_at(2 * 12 + 2).unwrap(),
    )
}

fn record(w: &World, key: u32) -> Option<&TypeRecord> {
    w.zoo.shows.infos[0]
        .as_ref()?
        .records
        .iter()
        .find(|r| r.key == key)
}

fn units(w: &World) -> Vec<u32> {
    record(w, 5402).map_or_else(Vec::new, |r| r.units.clone())
}

/// Adopts an animal and returns its id.
fn adopt(w: &mut World, kind: &str, tile: (i32, i32)) -> u32 {
    let e = entity("animals", kind, 0, centre(tile), 0);
    let s = shape(&e);
    w.add(e, s).unwrap()
}

fn ids(items: &[Item]) -> Vec<u16> {
    items.iter().map(|i| i.id).collect()
}

#[test]
fn a_type_lists_its_tricks_in_order_without_a_repeat_a_missing_section_or_a_young_animal() {
    let cat = types();
    let adult = tricks(&cat.types[0]);
    // The default subtype's list serves the type with no own section; the same walk twice adds
    // nothing, the name with no section adds a fresh item.
    assert_eq!(ids(&adult), [11208, 11209, 11210, 11214, 0, 11299]);
    // The young list empties the default one.
    assert!(tricks(&cat.types[1]).is_empty());
    // A subtype with no list of its own reads the default subtype's (open point O13).
    let female = animal("dolphin", "f", 5402, DOLPHIN);
    assert_eq!(ids(&tricks(&female)), ids(&adult));
    assert!(tricks(&cat.types[2]).is_empty());
    assert!(!crate::show::trick::performs(&cat.types[2]));
    let hoop = &adult[0];
    assert_eq!(
        (hoop.marker, hoop.names[0].as_str(), hoop.names[2].as_str()),
        (1, "hoopjump", "bTrickStart")
    );
    assert_eq!((hoop.kind, hoop.building, hoop.complexity), (5402, 6503, 5));
    assert_eq!(
        (hoop.satisfaction, hoop.delta, hoop.current, hoop.depth),
        (4, 3, 4, 4)
    );
    assert_eq!(
        (hoop.help, hoop.icons[1].as_str()),
        ([11308, 11408], "objects/hoop2")
    );
    let food = &adult[5];
    assert_eq!(
        (food.marker, food.visible, food.names[3].as_str()),
        (3, 0, "bTrickFeed")
    );
}

#[test]
fn a_trick_section_with_no_key_is_a_fresh_item() {
    let text = "[Global]\n[x/tricks]\ntrick=a\n[x/tricks/a]\n";
    let ty = animal("seal", "x", 7, text);
    let [a] = &tricks(&ty)[..] else { panic!() };
    // Visible, 1 everywhere a number counts, not available.
    assert_eq!(
        (a.id, a.visible, a.available, a.marker),
        (0, 1, 0, u32::MAX)
    );
    assert_eq!(
        (a.complexity, a.satisfaction, a.delta, a.current, a.depth),
        (1, 1, 1, 1, 1)
    );
    // A key that is no number reads 0 as the original's number reader does, a huge one wraps.
    let text =
        "[x/tricks]\ntrick=a\n[x/tricks/a]\nid=zz\ncomplexity=-4\nminimumdepth=99999999999\n";
    let [a] = &tricks(&animal("seal", "x", 7, text))[..] else {
        panic!()
    };
    assert_eq!((a.id, a.depth), (0, 99999999999i64 as i32));
}

#[test]
fn an_own_subtype_list_is_appended_to_the_default_subtype_list() {
    let text = "[Global]\nDefaultSubtype=m\n[m/tricks]\ntrick=a\ntrick=b\n\
        [m/tricks/a]\nid=1\n[m/tricks/b]\nid=2\n[x/tricks]\ntrick=c\ntrick=a\n\
        [x/tricks/c]\nid=3\n[x/tricks/a]\nid=1\n[z/tricks]\ntrick=empty\ntrick=c\n\
        [z/tricks/c]\nid=3\n";
    // Own list without `empty` adds to the default one; a repeated id adds nothing.
    assert_eq!(ids(&tricks(&animal("seal", "x", 7, text))), [1, 2, 3]);
    // `empty` clears what the default list made.
    assert_eq!(ids(&tricks(&animal("seal", "z", 7, text))), [3]);
    // The default subtype itself is read once; an own subtype with no list adds nothing.
    assert_eq!(ids(&tricks(&animal("seal", "m", 7, text))), [1, 2]);
    assert_eq!(ids(&tricks(&animal("seal", "f", 7, text))), [1, 2]);
}

#[test]
fn a_listed_trick_with_no_section_is_a_fresh_item_of_type_minus_one() {
    let text = "[Global]\n[tricks]\ntrick=ghost\ntrick=b\ntrick=c\n\
        [tricks/b]\nid=2\ndefaultavailable=7\nvisible=-3\nreturntokeeper=9\n\
        [tricks/c]\nid=3\nvisible=0\n";
    let ty = animal("seal", "", 7, text);
    let [g, b, c] = &tricks(&ty)[..] else {
        panic!()
    };
    // The missing section gives a fresh item; it performs.
    assert_eq!(
        (g.id, g.marker, g.visible, g.available),
        (0, u32::MAX, 1, 0)
    );
    assert!(crate::show::trick::performs(&ty));
    // The three flags are 1 when the number is not 0.
    assert_eq!((b.available, b.visible, b.returns), (1, 1, 1));
    assert_eq!((c.available, c.visible, c.returns), (0, 0, 0));
    // An id-0 trick that is listed already stops the fresh item.
    let text = "[tricks]\ntrick=a\ntrick=ghost\n[tricks/a]\nid=0\n";
    assert_eq!(tricks(&animal("seal", "", 7, text)).len(), 1);
}

#[test]
fn an_animal_that_grows_up_does_not_join_the_show_lists() {
    let (mut w, cat) = world(&[]);
    let young = adopt(&mut w, "dolphin", (7, 3));
    w.take_moves();
    let at = w.find(young).unwrap();
    w.retype(at, "", shape(&w.zoo.entities[at]).unwrap());
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
}

#[test]
fn a_default_script_adds_the_food_trick_after_every_run_that_reaches_the_threshold() {
    let (mut w, cat) = world(&[]);
    let (show, _) = tanks(&w);
    let ty = &cat.types[0];
    let params = crate::show::Params::default();
    // No toy yet: the walk alone (2) is under 5, the hoop and the ball need their toys.
    let items = default_items(&w, &cat, &params, show, ty).unwrap();
    assert_eq!(ids(&items), [11209]);
    // The hoop stands in the tank: hoop 5 reaches 5 at once (food), then the walk; the ball is not available.
    w.zoo
        .entities
        .push(entity("building", "hoop", 900, centre((7, 3)), 0));
    let items = default_items(&w, &cat, &params, show, ty).unwrap();
    assert_eq!(ids(&items), [11208, FOOD, 11209]);
    assert!(items.iter().all(|i| i.available == 1));
    // A threshold of 2 gives each walk its treat; research makes the ball available.
    let low = crate::show::Params {
        threshold: 2,
        ..params
    };
    let items = default_items(&w, &cat, &low, show, ty).unwrap();
    assert_eq!(ids(&items), [11208, FOOD, 11209, FOOD]);
    w.zoo
        .entities
        .push(entity("building", "ball", 901, centre((7, 3)), 0));
    w.research.unlock_trick(&cat, 5402, 11210);
    let items = default_items(&w, &cat, &params, show, ty).unwrap();
    assert_eq!(ids(&items), [11208, FOOD, 11209, 11210, FOOD]);
    // A type with no tricks has no script.
    assert!(default_items(&w, &cat, &params, show, &cat.types[2]).is_none());
}

#[test]
fn a_toy_outside_the_tank_or_water_too_shallow_leaves_a_trick_out() {
    let (mut w, cat) = world(&[]);
    let (show, plain) = tanks(&w);
    let params = crate::show::Params::default();
    let ty = &cat.types[0];
    // The toy is in the plain tank, not the show tank.
    w.zoo
        .entities
        .push(entity("building", "hoop", 900, centre((3, 3)), 0));
    assert_eq!(
        ids(&default_items(&w, &cat, &params, show, ty).unwrap()),
        [11209]
    );
    w.zoo
        .entities
        .push(entity("building", "hoop", 901, centre((7, 3)), 0));
    assert_eq!(
        ids(&default_items(&w, &cat, &params, show, ty).unwrap()),
        [11208, FOOD, 11209]
    );
    // The water is one step short of the hoop's 4 and of nothing else: walk (2), no treat.
    w.exhibits_mut()
        .get_mut(show)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .level = 3;
    assert_eq!(
        ids(&default_items(&w, &cat, &params, show, ty).unwrap()),
        [11209]
    );
    // Dry: every trick that needs 2 or more is out and the script is empty, but it exists.
    w.exhibits_mut()
        .get_mut(show)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .level = 0;
    assert!(
        default_items(&w, &cat, &params, show, ty)
            .unwrap()
            .is_empty()
    );
    let _ = plain;
}

#[test]
fn an_animal_put_in_the_show_tank_joins_its_species_and_the_species_gets_a_script() {
    let (mut w, cat) = world(&[]);
    w.units.show.admission = 5.5;
    w.units.now = 1_000_000;
    let first = adopt(&mut w, "dolphin", (7, 3));
    // Nothing is told to the lists before the step settles the move.
    assert!(units(&w).is_empty());
    settle(&mut w, &cat);
    assert_eq!(units(&w), [first]);
    let r = record(&w, 5402).unwrap();
    assert_eq!(
        (r.script, r.pending, r.admission),
        (2000, u16::MAX, 5.5f32.to_bits())
    );
    assert_eq!(r.date, crate::timing::file_time(1_000_000));
    // The script is the species', in the counter's order, and the species is on the schedule.
    let s = &w.zoo.shows;
    assert_eq!(
        (s.scripts.len(), s.next_script, s.scripts[0].kind),
        (1, 2001, 5402)
    );
    assert_eq!(s.scripts[0].marker, u32::MAX);
    assert_eq!(s.infos[0].as_ref().unwrap().schedule, [5402]);
    // A second animal joins the same record; it is not added twice; a fish joins nothing.
    let second = adopt(&mut w, "dolphin", (8, 3));
    adopt(&mut w, "fish", (8, 3));
    w.show_moved(w.find(first).unwrap(), true);
    settle(&mut w, &cat);
    assert_eq!(units(&w), [first, second]);
    assert_eq!(w.zoo.shows.scripts.len(), 1);
    assert_eq!(w.zoo.shows.infos[0].as_ref().unwrap().records.len(), 1);
}

#[test]
fn a_script_with_no_item_is_made_and_given_but_the_species_is_not_scheduled() {
    let (mut w, cat) = world(&[]);
    let (show, _) = tanks(&w);
    w.exhibits_mut()
        .get_mut(show)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .level = 0;
    let id = adopt(&mut w, "dolphin", (7, 3));
    settle(&mut w, &cat);
    let info = w.zoo.shows.infos[0].as_ref().unwrap();
    assert_eq!(
        (
            record(&w, 5402).unwrap().script,
            w.zoo.shows.scripts[0].items.len()
        ),
        (2000, 0)
    );
    assert!(info.schedule.is_empty());
    assert_eq!(units(&w), [id]);
}

#[test]
fn a_species_that_has_a_record_keeps_its_script_and_a_running_show_only_holds_the_new_one() {
    let (mut w, cat) = world(&[]);
    // A record made empty earlier blocks the default script.
    let mut empty = TypeRecord::new(5402, u16::MAX, 7);
    empty.units.push(1);
    w.zoo.shows.infos[0].as_mut().unwrap().records.push(empty);
    let id = adopt(&mut w, "dolphin", (7, 3));
    settle(&mut w, &cat);
    assert!(w.zoo.shows.scripts.is_empty());
    assert_eq!(units(&w), [1, id]);
    // A show that runs the scheduled script: the species met now takes its script as pending.
    let (mut w, cat) = world(&[]);
    let i = w.zoo.shows.infos[0].as_mut().unwrap();
    let mut other = TypeRecord::new(5403, 2500, 7);
    other.units.push(2);
    i.records.push(other);
    (i.schedule, i.show.running, i.show.script) = (vec![5403], true, 2500);
    adopt(&mut w, "dolphin", (7, 3));
    settle(&mut w, &cat);
    let r = record(&w, 5402).unwrap();
    assert_eq!((r.script, r.pending), (u16::MAX, 2000));
    assert_eq!(w.zoo.shows.infos[0].as_ref().unwrap().schedule, [5403]);
}

#[test]
fn an_animal_of_the_plain_tank_joins_the_show_of_its_show_neighbour_and_leaves_with_the_link() {
    let (mut w, cat) = world(&[(40, "dolphin", (3, 3))]);
    let (show, plain) = tanks(&w);
    // Put down in the plain tank: it joins the neighbour's show, not a record of the plain tank.
    w.show_moved(w.find(40).unwrap(), true);
    settle(&mut w, &cat);
    assert_eq!(units(&w), [40]);
    // The link goes with the water of the show tank: the animal leaves the show.
    w.exhibits_mut()
        .get_mut(show)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .filled = false;
    w.set_height(show, 5);
    assert!(w.exhibits().get(plain).unwrap().show_near.is_empty());
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
    // The link made again: it joins again (the record keeps its script).
    w.exhibits_mut()
        .get_mut(show)
        .unwrap()
        .tank
        .as_mut()
        .unwrap()
        .filled = true;
    w.set_height(show, 4);
    settle(&mut w, &cat);
    assert_eq!(units(&w), [40]);
    assert_eq!(w.zoo.shows.scripts.len(), 1);
}

#[test]
fn the_first_rating_of_an_exhibit_adds_its_animals_once() {
    let (mut w, cat) = world(&[
        (40, "dolphin", (7, 3)),
        (41, "fish", (7, 3)),
        (42, "dolphin", (3, 3)),
    ]);
    let (show, _) = tanks(&w);
    assert!(units(&w).is_empty());
    // The rating of the show tank rates its plain neighbour first: its dolphin joins the same show.
    rating::finish(&mut w, &cat, show, &mut || 0);
    assert_eq!(units(&w), [40, 42]);
    // The flags are down: another rating adds no one.
    w.zoo.shows.infos[0].as_mut().unwrap().records[0]
        .units
        .clear();
    rating::finish(&mut w, &cat, show, &mut || 0);
    assert!(units(&w).is_empty());
}

#[test]
fn pick_up_put_down_and_death_tell_the_lists() {
    let (mut w, cat) = world(&[(40, "dolphin", (7, 3))]);
    let (_, plain) = tanks(&w);
    w.show_moved(w.find(40).unwrap(), true);
    settle(&mut w, &cat);
    assert_eq!(units(&w), [40]);
    // Picked up: it leaves; put down in the plain tank: it joins the neighbour's show again.
    assert!(crate::place::unit::pick_up(&mut w, &cat, 40, 0).is_some());
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
    let tile = (3 * 64 + 32, 3 * 64 + 32, 0);
    assert!(crate::place::unit::put_down(
        &mut w,
        40,
        tile,
        0,
        &crate::behaviour::NoData
    ));
    settle(&mut w, &cat);
    assert_eq!(units(&w), [40]);
    assert_eq!(w.exhibit_of(w.find(40).unwrap()), Some(plain));
    // Removed from the map: it leaves. A carried animal that is removed was told when lifted.
    assert!(w.remove(40));
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
    // An animal that grows up changes subtype in place: the adult does not join (shows 20).
    let young = adopt(&mut w, "dolphin", (7, 3));
    w.take_moves();
    let at = w.find(young).unwrap();
    w.retype(at, "", shape(&w.zoo.entities[at]).unwrap());
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
}

#[test]
fn removing_an_animal_that_never_joined_makes_an_empty_record_and_a_land_animal_nothing() {
    let (mut w, cat) = world(&[(40, "dolphin", (7, 3))]);
    assert!(w.remove(40));
    settle(&mut w, &cat);
    let r = record(&w, 5402).unwrap();
    assert!((r.units.is_empty(), r.script, r.admission) == (true, u16::MAX, 0));
    assert!(w.zoo.shows.scripts.is_empty());
    // On the zoo grounds there is no exhibit: no record, no panic.
    let (mut w, cat) = world(&[(40, "dolphin", (11, 7))]);
    assert!(w.remove(40));
    adopt(&mut w, "dolphin", (11, 7));
    settle(&mut w, &cat);
    assert!(record(&w, 5402).is_none());
}

#[test]
fn the_animals_of_a_loaded_zoo_join_the_show_in_file_order_with_the_first_step() {
    let (w, cat) = world(&[(41, "dolphin", (7, 3)), (40, "dolphin", (6, 3))]);
    // The load places the entities in file order; the tiles would give 40 first.
    let mut loaded = World::new(w.zoo.clone(), shape);
    settle(&mut loaded, &cat);
    assert_eq!(units(&loaded), [41, 40]);
    // A species with no saved record gets its default script as it joins.
    assert_eq!(loaded.zoo.shows.scripts.len(), 1);
}

#[test]
fn a_loaded_world_starts_with_no_news_and_the_new_numbers_read_from_the_file() {
    let (w, _) = world(&[(40, "dolphin", (7, 3))]);
    assert!(w.units.show.admission == 0.0 && w.units.show.threshold == 5);
    let read = |t: &str| crate::show::Params::read(&Config::parse(t.as_bytes()).unwrap());
    let p = read("[behavior]\nreturnToKeeperThreshold=7\n[cost]\ndefaultShowAdmission=5.50\n");
    assert_eq!((p.threshold, p.admission), (7, 5.5));
    // Missing, negative, huge and a word.
    assert_eq!(read("").threshold, 5);
    assert_eq!(
        read("[behavior]\nreturnToKeeperThreshold=-1\n").threshold,
        -1
    );
    assert_eq!(read("[behavior]\nreturnToKeeperThreshold=x\n").threshold, 0);
    assert!(read("[cost]\ndefaultShowAdmission=1e9\n").admission == 1e9);
    assert!(read("[cost]\ndefaultShowAdmission=-2\n").admission == -2.0);
}

#[test]
fn the_scripts_and_records_the_lists_made_are_saved_and_read_back() {
    let (mut w, cat) = world(&[]);
    w.units.show.admission = 5.5;
    w.zoo
        .entities
        .push(entity("building", "hoop", 900, centre((7, 3)), 0));
    let id = adopt(&mut w, "dolphin", (7, 3));
    settle(&mut w, &cat);
    let back = w.zoo.shows.saved_and_read();
    assert_eq!(back.scripts, w.zoo.shows.scripts);
    assert_eq!(back.next_script, 2001);
    let (a, b) = (
        back.infos[0].as_ref().unwrap(),
        w.zoo.shows.infos[0].as_ref().unwrap(),
    );
    assert_eq!((&a.schedule, a.records.len()), (&b.schedule, 1));
    assert_eq!(
        (a.records[0].script, a.records[0].admission),
        (2000, 5.5f32.to_bits())
    );
    // The unit list is not saved: the animals rebuild it.
    assert!(a.records[0].units.is_empty() && b.records[0].units == [id]);
}

/// Every item of every script of the shipped maps and saved game is the trick of the same id in the
/// list of the animal type the script is for, in what the file gives (not the flags and numbers a
/// player's editing or research changes, nor the help ids and icons that files before 103 lack).
#[test]
#[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
fn full_game_scripts_are_made_of_the_trick_lists() {
    let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
    let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
    let cat = Catalogue::load(&vfs).unwrap();
    let mut seen = 0;
    for member in vfs.list(None).iter().filter(|m| m.ends_with(".zoo")) {
        let Ok((zoo, _)) = crate::zoo::Zoo::parse_money(&vfs.read(member).unwrap()) else {
            continue;
        };
        for item in zoo.shows.scripts.iter().flat_map(|s| &s.items) {
            let own = (cat.types.iter())
                .filter(|t| t.class == "animals" && t.name_id as u32 == item.kind)
                .flat_map(tricks);
            let t = own.into_iter().find(|t| t.id == item.id);
            // Two maps were saved when the ball, the belly flop and the crowd wave were other tricks.
            let old = [11210, 11211, 11308].contains(&item.id);
            let Some(t) = t.or_else(|| old.then(|| item.clone())) else {
                panic!("{member}: no trick {} of {}", item.id, item.kind)
            };
            let same = |i: &Item| {
                (
                    i.kind,
                    i.marker,
                    &i.names,
                    i.building,
                    i.complexity,
                    i.returns,
                ) == (
                    t.kind,
                    t.marker,
                    &t.names,
                    t.building,
                    t.complexity,
                    t.returns,
                ) && (i.satisfaction, i.depth, i.visible) == (t.satisfaction, t.depth, t.visible)
            };
            assert!(same(item) || old, "{member}: {item:?}");
            seen += 1;
        }
    }
    // The scripts are in the maps of the expansions.
    assert!(
        seen > 50 || !crate::fixtures::has_expansions(&dir),
        "{seen} items"
    );
}

/// The land exhibit `L` beside the plain tank (the dolphin stands at `(1, 3)` there).
fn land(w: &World) -> u32 {
    w.exhibit_of(w.find(40).unwrap()).unwrap()
}

#[test]
fn an_animal_of_a_land_exhibit_joins_the_show_of_the_tank_next_to_its_tank_and_leaves() {
    let (mut w, cat) = world(&[(40, "dolphin", (1, 3))]);
    let (_, plain) = tanks(&w);
    assert!(
        w.exhibits()
            .get(land(&w))
            .unwrap()
            .amphibious
            .contains(&plain)
    );
    w.show_moved(w.find(40).unwrap(), true);
    settle(&mut w, &cat);
    assert_eq!(units(&w), [40]);
    assert_eq!(w.zoo.shows.scripts.len(), 1);
    assert!(crate::place::unit::pick_up(&mut w, &cat, 40, 0).is_some());
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
}

#[test]
fn the_link_takes_the_animals_of_the_land_exhibit_after_those_of_the_plain_tank() {
    let (mut w, cat) = world(&[(41, "dolphin", (1, 3)), (40, "dolphin", (3, 3))]);
    let (show, _) = tanks(&w);
    let set = |w: &mut World, filled: bool| {
        w.exhibits_mut()
            .get_mut(show)
            .unwrap()
            .tank
            .as_mut()
            .unwrap()
            .filled = filled;
    };
    // The link is made again after the world was built: both join, the tank's animal first.
    set(&mut w, false);
    w.set_height(show, 5);
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
    set(&mut w, true);
    w.set_height(show, 4);
    settle(&mut w, &cat);
    assert_eq!(units(&w), [40, 41]);
    // Drained: the link is dropped and both leave.
    set(&mut w, false);
    w.set_height(show, 5);
    settle(&mut w, &cat);
    assert!(units(&w).is_empty());
}
