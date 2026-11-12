//! The animal's trips to and from a show tank (shows 21.2 to 21.5) on `show_world`: the plain tank
//! `T` (tiles 2 to 5, rows 2 to 4) and the show tank `S` (tiles 6 to 9) joined by a portal on the
//! tile (5, 3) of `T` and the tile (6, 3) of `S`; an orca stands in `T`, unit 1.

use super::*;
use crate::config::Config;
use crate::entity::EntityType;
use crate::show::event;
use crate::unit::testing::{gated, show_world};
use crate::zoo::{HabitatInfo, Show, ShowInfo};
use std::sync::Arc;

const SHOW: u16 = 1001;

fn kind(class: &'static str, name: &str, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
        name_id: 5400,
        footprint: [1, 1, 1],
        height: 0,
    }
}

/// The orca performs; the wall type has portal sounds.
fn cat() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(kind("animals", "orca", "[tricks]\ntrick=a\n"));
    cat.push(kind(
        "tankwall",
        "iron",
        "[PortalSounds]\ncPortalOpenSound=sounds/in.wav\ncPortalOpenSoundAtten=300\n\
         cPortalCloseSound=sounds/out.wav\ncPortalCloseSoundAtten=-200\n",
    ));
    cat
}

fn tile(x: i32, y: i32) -> usize {
    (y * 12 + x) as usize
}

/// The world with the orca in `at`, the portal and the show info of the show tank; the orca is
/// started in that show.
fn staged(at: (i32, i32), portal: bool) -> World {
    staged_with(&[(1, "orca", at)], portal)
}

/// [`staged`] with other animals in the world too; the first is the orca of the show.
pub(super) fn staged_with(animals: &[(u32, &str, (i32, i32))], portal: bool) -> World {
    let mut w = gated(show_world(animals));
    for (t, d) in [((5, 3), 2), ((6, 3), 6)] {
        let piece = w.edge(tile(t.0, t.1), d).unwrap();
        let fence = w.zoo.entities[piece]
            .common
            .as_mut()
            .unwrap()
            .fence
            .as_mut();
        fence.unwrap().portal = portal;
    }
    w.zoo.habitat_tiles = vec![(6, 2)];
    w.zoo.habitat_info = vec![HabitatInfo {
        name: String::new(),
        stats: [0; 9],
        created: 0,
        viewing: 0,
        serviced: 0,
    }];
    w.zoo.shows.infos = vec![Some(ShowInfo {
        frequency: 90,
        index: 0,
        schedule: Vec::new(),
        records: Vec::new(),
        events: Vec::new(),
        next: 0,
        id: SHOW,
        upkeep: [0; 3],
        receipts: [0; 3],
        attendance: [0; 3],
        show: Show::default(),
    })];
    let u = w.units.get_mut(1).unwrap();
    (u.typed, u.started, u.mover.underwater, u.mover.depth) = (true, true, true, 1);
    u.show = SHOW;
    w
}

/// The sets the animal runs: its run ends at once.
fn sets() -> Sets {
    Sets::new(&[("bRun", &["fWalk(0,0)"]), ("bWalk", &["fWalk(0,0)"])])
}

/// Runs `f` on the goal update context of the orca with the catalogue of the test.
fn with_orca<R>(w: &mut World, f: impl FnOnce(&mut Cx) -> R) -> R {
    let cat = cat();
    let mut u = unit(w).clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut rng = Rng::default();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let at = w.find(1).unwrap();
    let data = sets();
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(1).unwrap() = u;
    r
}

fn task(w: &World) -> u8 {
    unit(w).goal.record.task
}

/// The tile the orca stands on.
fn at(w: &World) -> usize {
    let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
    w.tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
        .unwrap()
}

#[test]
fn a_called_animal_takes_the_go_to_show_task_unless_it_is_in_a_building_task() {
    for (called, task, ends, set) in [
        (true, 0, true, GO_TO_SHOW),
        (true, 104, false, 104),
        (true, 106, false, 106),
        (true, 107, true, GO_TO_SHOW),
        (false, 0, false, 0),
    ] {
        let mut w = staged((4, 3), true);
        let u = w.units.get_mut(1).unwrap();
        (u.called, u.goal.record.task) = (called, task);
        let cut = with_orca(&mut w, |cx| cx.animal_show_rung(task));
        assert_eq!((cut, self::task(&w)), (ends, set), "{called} {task}");
    }
}

#[test]
fn an_animal_going_to_or_from_a_show_starts_no_need_task() {
    for task in [GO_TO_SHOW, RETURN_FROM_SHOW] {
        let mut w = staged((4, 3), true);
        let u = w.units.get_mut(1).unwrap();
        u.goal.record.task = task;
        if let Part::Animal(a) = &mut u.part {
            (a.wants_food, a.thirsty) = (true, true);
        }
        with_orca(&mut w, |cx| cx.animal_ladder(task));
        assert_eq!(self::task(&w), task);
    }
}

#[test]
fn an_animal_sent_home_or_standing_in_a_show_tank_with_no_show_takes_the_return_task() {
    for (from, back, takes) in [
        ((4, 3), true, true),
        ((4, 3), false, false),
        ((7, 3), false, true),
        ((7, 3), true, true),
    ] {
        let mut w = staged(from, true);
        w.units.get_mut(1).unwrap().back = back;
        let cut = with_orca(&mut w, |cx| cx.animal_show_rung(0));
        assert_eq!(
            (cut, task(&w) == RETURN_FROM_SHOW),
            (takes, takes),
            "{from:?} {back}"
        );
    }
}

#[test]
fn an_animal_in_a_show_that_holds_a_state_for_it_stays_in_the_show_tank() {
    let mut w = staged((7, 3), true);
    let info = w.zoo.shows.infos[0].as_mut().unwrap();
    info.show.items.push(crate::zoo::ItemState {
        script: 0,
        animal: 1,
        item: -1,
        finished: false,
        aborted: false,
        cue_sent: false,
        ready: false,
        started: false,
        done: false,
    });
    assert!(!with_orca(&mut w, |cx| cx.animal_show_rung(0)));
}

#[test]
fn it_swims_to_the_tile_of_its_portal_and_aims_across_the_wall() {
    let mut w = staged((4, 3), true);
    w.units.get_mut(1).unwrap().called = true;
    with_orca(&mut w, |cx| {
        cx.set_task(GO_TO_SHOW, 1, 0, None);
        cx.go_show_decide();
    });
    let u = unit(&w);
    // The tile across is the animal's own target (shows 32.2 point 4, its centre the position);
    // the goal record's target is the walk's destination, the portal tile.
    assert_eq!(u.own_tile, Some(tile(6, 3)));
    let z = 16 * w.zoo.tiles[tile(6, 3)].elevation;
    assert_eq!(u.own_pos, [6 * 64 + 32, 3 * 64 + 32, z]);
    assert_eq!(u.goal.record.target.tile, Some(tile(5, 3)));
    assert_eq!(u.path.last(), Some(&tile(5, 3)));
    assert!(
        u.called && u.runner.top() == Some("bRun"),
        "it goes on running"
    );
}

#[test]
fn arriving_it_crosses_into_the_show_tank_reports_in_and_goes_on() {
    let mut w = staged((4, 3), true);
    w.units.get_mut(1).unwrap().called = true;
    with_orca(&mut w, |cx| {
        cx.set_task(GO_TO_SHOW, 1, 0, None);
        cx.go_show_decide();
    });
    // It reaches the portal tile.
    w.place_unit(w.find(1).unwrap(), (5 * 64 + 32, 3 * 64 + 32, 32), 2);
    w.units.get_mut(1).unwrap().destination = Some((5 * 64 + 32, 3 * 64 + 32, 32));
    with_orca(&mut w, |cx| {
        cx.show_check_done();
        assert!(cx.record().done);
        cx.go_show_complete();
    });
    let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
    // At the west edge of the tile across, at the level of the water less two, facing east.
    assert_eq!((c.x, c.y, c.z, c.rotation), (6 * 64, 3 * 64 + 32, 32, 2));
    let s = w.tank_at(tile(6, 3));
    assert_eq!(w.exhibit_of(w.find(1).unwrap()), s);
    let u = unit(&w);
    assert!(!u.called);
    assert_eq!(u.target, crate::behaviour::Target::Surface);
    let posts: Vec<_> = (w.units.posts.iter())
        .map(|e| (e.kind, e.sender, e.target, e.channel, e.delay))
        .collect();
    let arrived = event::ARRIVED;
    assert_eq!(
        posts,
        [
            (arrived, 1, 1, 0x55, 1),
            (arrived, 1, 0, 0x57, 1),
            (arrived, 1, 1001, 0x53, 1)
        ]
    );
}

#[test]
fn an_animal_that_finds_no_portal_aborts_the_show_and_says_it_has_trouble() {
    for held in [false, true] {
        let mut w = staged((4, 3), false);
        let u = w.units.get_mut(1).unwrap();
        (u.called, u.held) = (true, held);
        let info = w.zoo.shows.infos[0].as_mut().unwrap();
        (info.show.running, info.show.species) = (true, 5400);
        info.records.push(crate::zoo::TypeRecord {
            key: 5400,
            script: 0,
            pending: u16::MAX,
            admission: 0,
            receipts: [0; 3],
            attendance: [0; 3],
            date: 0,
            units: vec![1],
            default_flag: true,
        });
        with_orca(&mut w, |cx| {
            cx.set_task(GO_TO_SHOW, 1, 0, None);
            cx.go_show_decide();
        });
        let u = unit(&w);
        assert!(!u.called);
        assert_eq!(u.goal.record.task, NONE);
        // The end of the show is told too (shows 5.6).
        let said: Vec<i32> = w.units.take_messages().iter().map(|m| m.0).collect();
        assert_eq!(said, [if held { 11005 } else { 11004 }, 11492]);
        // The show is over, and its animals are on their way home.
        let show = &w.zoo.shows.infos[0].as_ref().unwrap().show;
        assert!(!show.running && show.stopped);
        assert!(unit(&w).back);
        let kinds: Vec<u16> = w.units.posts.iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&event::ABORTED), "{kinds:?}");
    }
}

#[test]
fn an_animal_sent_home_swims_to_the_portal_and_crosses_back() {
    let mut w = staged((7, 3), true);
    w.units.get_mut(1).unwrap().back = true;
    with_orca(&mut w, |cx| {
        cx.set_task(RETURN_FROM_SHOW, 1, 0, None);
        cx.return_decide();
    });
    let u = unit(&w);
    assert_eq!(u.own_tile, Some(tile(5, 3)));
    assert_eq!(u.goal.record.target.tile, Some(tile(6, 3)));
    assert_eq!(u.path.last(), Some(&tile(6, 3)));
    w.place_unit(w.find(1).unwrap(), (6 * 64 + 32, 3 * 64 + 32, 32), 6);
    w.units.get_mut(1).unwrap().destination = Some((6 * 64 + 32, 3 * 64 + 32, 32));
    with_orca(&mut w, |cx| {
        cx.show_check_done();
        cx.return_complete();
    });
    assert_eq!(w.tank_at(at(&w)), w.tank_at(tile(3, 3)));
    let u = unit(&w);
    assert!(!u.back && task(&w) == NONE);
    assert!(w.units.posts.is_empty(), "the return posts nothing");
}

#[test]
fn an_animal_with_no_way_home_gives_up_the_return_and_says_so() {
    let mut w = staged((7, 3), false);
    w.units.get_mut(1).unwrap().back = true;
    with_orca(&mut w, |cx| {
        cx.set_task(RETURN_FROM_SHOW, 1, 0, None);
        cx.return_decide();
    });
    assert!(!unit(&w).back && task(&w) == NONE);
    let said = w.units.take_messages();
    assert_eq!(said.iter().map(|m| m.0).collect::<Vec<_>>(), [11004]);
}

#[test]
fn a_unit_picked_up_aborts_the_show_of_its_show_id() {
    let mut w = staged((4, 3), true);
    w.zoo.shows.infos[0].as_mut().unwrap().show.running = true;
    super::super::picked_up(&mut w, &cat(), &mut Rng::default(), &NoData, 1);
    assert!(!w.zoo.shows.infos[0].as_ref().unwrap().show.running);
    let kinds: Vec<u16> = w.units.posts.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&event::ABORTED), "{kinds:?}");
    // A unit with no show id, or one whose show has no info, aborts nothing.
    for show in [0, 7] {
        let mut w = staged((4, 3), true);
        w.zoo.shows.infos[0].as_mut().unwrap().show.running = true;
        w.units.get_mut(1).unwrap().show = show;
        super::super::picked_up(&mut w, &cat(), &mut Rng::default(), &NoData, 1);
        assert!(
            w.zoo.shows.infos[0].as_ref().unwrap().show.running,
            "{show}"
        );
    }
}

#[test]
fn the_portal_opens_while_an_animal_crosses_and_closes_after_with_one_sound_each() {
    let mut w = staged((4, 3), true);
    let piece = |w: &World, t: (i32, i32), d| w.edge(tile(t.0, t.1), d).unwrap();
    let walls = [piece(&w, (5, 3), 2), piece(&w, (6, 3), 6)];
    let open = |w: &World| {
        walls.map(|p| {
            w.zoo.entities[p]
                .common
                .as_ref()
                .unwrap()
                .fence
                .unwrap()
                .open
        })
    };
    let (s, cat) = (w.tank_at(tile(6, 3)).unwrap(), cat());
    // Nobody crosses: nothing happens.
    crate::show::open_portals(&mut w, &cat, s);
    assert_eq!(open(&w), [false, false]);
    assert!(w.units.take_sounds().is_empty());
    // An animal of T on its way to S opens both walls, and one sound plays.
    let u = w.units.get_mut(1).unwrap();
    u.goal.record.task = GO_TO_SHOW;
    u.own_tile = Some(tile(6, 3));
    crate::show::open_portals(&mut w, &cat, s);
    assert_eq!(open(&w), [true, true]);
    let sounds = w.units.take_sounds();
    let heard: Vec<_> = sounds
        .iter()
        .map(|(_, h)| (h.file.as_str(), h.base))
        .collect();
    assert_eq!(heard.len(), 1, "{heard:?}");
    assert!(
        heard[0].0.ends_with("in.wav") && heard[0].1 == 300,
        "{heard:?}"
    );
    // Again: nothing changes, so nothing plays.
    crate::show::open_portals(&mut w, &cat, s);
    assert!(w.units.take_sounds().is_empty());
    // The task ends: both close, with the close sound.
    w.units.get_mut(1).unwrap().goal.record.task = NONE;
    crate::show::open_portals(&mut w, &cat, s);
    assert_eq!(open(&w), [false, false]);
    let sounds = w.units.take_sounds();
    let heard: Vec<_> = sounds
        .iter()
        .map(|(_, h)| (h.file.as_str(), h.base))
        .collect();
    assert!(
        heard.len() == 1 && heard[0].0.ends_with("out.wav") && heard[0].1 == -200,
        "{heard:?}"
    );
}

#[test]
fn an_animal_of_the_show_tank_on_its_way_home_opens_the_portal_too() {
    let mut w = staged((7, 3), true);
    let u = w.units.get_mut(1).unwrap();
    u.goal.record.task = RETURN_FROM_SHOW;
    u.own_tile = Some(tile(5, 3));
    let s = w.tank_at(tile(6, 3)).unwrap();
    crate::show::open_portals(&mut w, &cat(), s);
    let wall = w.edge(tile(5, 3), 2).unwrap();
    assert!(
        w.zoo.entities[wall]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap()
            .open
    );
}

#[test]
fn the_exhibit_update_of_a_step_opens_the_portal_for_a_crossing_animal() {
    let mut w = staged((4, 3), true);
    let u = w.units.get_mut(1).unwrap();
    u.goal.record.task = GO_TO_SHOW;
    u.own_tile = Some(tile(6, 3));
    let wall = w.edge(tile(5, 3), 2).unwrap();
    let open = |w: &World| {
        w.zoo.entities[wall]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap()
            .open
    };
    assert!(!open(&w));
    run_with(&mut w, &mut sim(), &sets(), 1);
    assert!(open(&w));
}

#[test]
fn the_animal_is_occupied_while_it_is_in_a_show_as_it_is_at_that_moment() {
    // Shows 32.3 point 3: the ladder's "doing something" does not read the status tick's copy.
    let mut w = staged((4, 3), true);
    let occupied = |w: &mut World| with_orca(w, |cx| cx.occupied());
    // Sent home since the last status tick: in a show, though the copy says no.
    w.units.get_mut(1).unwrap().back = true;
    assert!(occupied(&mut w));
    // Back home since: not in a show, though the copy says yes.
    let u = w.units.get_mut(1).unwrap();
    u.back = false;
    if let Part::Animal(a) = &mut u.part {
        a.in_show = true;
    }
    assert!(!occupied(&mut w));
}

#[test]
fn a_goal_reset_empties_the_own_target_tile_and_a_clear_of_the_record_alone_leaves_it() {
    // Shows 32.2 point 4: the reset writes none with flag 1; the record's own clear does not.
    let mut w = staged((4, 3), true);
    let aim = |w: &mut World| {
        let mut u = unit(w).clone();
        u.aim(w, Some(tile(6, 3)));
        *w.units.get_mut(1).unwrap() = u;
    };
    aim(&mut w);
    with_orca(&mut w, |cx| cx.reset());
    assert_eq!((unit(&w).own_tile, unit(&w).own_pos), (None, [-1; 3]));
    aim(&mut w);
    with_orca(&mut w, |cx| cx.record().target.clear());
    assert_eq!(unit(&w).own_tile, Some(tile(6, 3)));
}

#[test]
fn the_portal_reads_the_own_target_tile_and_not_the_goal_records() {
    // Shows 32.2 point 7, 21.5 point 1.
    let mut w = staged((4, 3), true);
    let u = w.units.get_mut(1).unwrap();
    u.goal.record.task = GO_TO_SHOW;
    u.goal.record.target.tile = Some(tile(6, 3));
    let s = w.tank_at(tile(6, 3)).unwrap();
    let wall = w.edge(tile(5, 3), 2).unwrap();
    let open = |w: &World| {
        w.zoo.entities[wall]
            .common
            .as_ref()
            .unwrap()
            .fence
            .unwrap()
            .open
    };
    crate::show::open_portals(&mut w, &cat(), s);
    assert!(!open(&w));
    w.units.get_mut(1).unwrap().own_tile = Some(tile(6, 3));
    crate::show::open_portals(&mut w, &cat(), s);
    assert!(open(&w));
}

#[test]
fn an_animal_on_the_land_exhibit_swims_to_the_portal_of_the_tank_next_to_it() {
    let mut w = staged_with(&[(1, "orca", (1, 3))], true);
    w.units.get_mut(1).unwrap().called = true;
    with_orca(&mut w, |cx| {
        cx.set_task(GO_TO_SHOW, 1, 0, None);
        cx.go_show_decide();
    });
    let u = unit(&w);
    assert_eq!(u.own_tile, Some(tile(6, 3)));
    assert_eq!(u.goal.record.target.tile, Some(tile(5, 3)));
    assert!(u.called);
}

#[test]
fn an_animal_on_the_land_exhibit_with_no_tank_linked_to_the_show_aborts_it() {
    let mut w = staged_with(&[(1, "orca", (1, 3))], false);
    w.units.get_mut(1).unwrap().called = true;
    with_orca(&mut w, |cx| {
        cx.set_task(GO_TO_SHOW, 1, 0, None);
        cx.go_show_decide();
    });
    let said: Vec<i32> = w.units.take_messages().iter().map(|m| m.0).collect();
    assert_eq!(said.first(), Some(&11004));
    assert!(!unit(&w).called);
}
