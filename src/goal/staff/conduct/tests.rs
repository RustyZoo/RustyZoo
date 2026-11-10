//! Job 19 and the duties that give it (shows 22.1, staff 7.5 point 3.1).

use super::*;
use crate::entity::{Catalogue, EntityType};
use crate::goal::{DUTIES, update};
use crate::sim::{Events, Rng};
use crate::unit::Ctx;
use crate::unit::testing::{Walker, show_tank};
use crate::world::World;
use std::sync::Arc;

const KEEPER: u32 = 7;
const MARINE: u32 = 9552;
const SPECIES: u32 = 5401;

fn marine() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(EntityType {
        class: "keeper",
        type_name: b"trainer".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(
            crate::config::Config::parse(b"[Characteristics/Integers]\ncUnderwater=1\n").unwrap(),
        )),
        name_id: MARINE as i32,
        footprint: [1, 1, 1],
        height: 0,
    });
    // The animal type of the scheduled species (the specialist's type is its keeper type).
    cat.push(EntityType {
        class: "animals",
        type_name: b"seal".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id: SPECIES as i32,
        footprint: [1, 1, 1],
        height: 0,
    });
    cat
}

/// Runs `f` on a goal update context of the specialist, whose state is put back.
fn with_cx<R>(w: &mut World, f: impl FnOnce(&mut Cx) -> R) -> R {
    let (cat, data, mut rng) = (marine(), Walker::new("fWalk(0,0)"), Rng::default());
    let mut u = w.units.get(KEEPER).unwrap().clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let at = w.find(KEEPER).unwrap();
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(KEEPER).unwrap() = u;
    r
}

fn list(w: &World) -> Vec<u32> {
    show::info_of(w, 1).unwrap().units(MARINE).to_vec()
}

fn show_mut(w: &mut World) -> &mut crate::zoo::Show {
    &mut w.zoo.shows.infos[0].as_mut().unwrap().show
}

fn ladder(w: &World) -> Ladder {
    Ladder::of(w, 1).unwrap()
}

/// The specialist's position and facing.
fn stands(w: &World) -> ((i32, i32, i32), u32) {
    let c = w.zoo.entities[w.find(KEEPER).unwrap()]
        .common
        .as_ref()
        .unwrap();
    ((c.x, c.y, c.z), c.rotation)
}

#[test]
fn the_init_puts_him_on_the_platform_facing_into_the_tank_and_in_the_show() {
    let mut w = show_tank();
    assert!(with_cx(&mut w, |cx| cx.conduct_init()));
    let l = ladder(&w);
    // The platform is above the gate tile, the gate being on the north side: he looks south.
    assert_eq!(stands(&w), (l.platform, l.toward_inside));
    assert_eq!(l.toward_inside, 4);
    assert_eq!(list(&w), [KEEPER]);
    assert_eq!(w.units.get(KEEPER).unwrap().show, 1001);
    // Again: once in the list.
    assert!(with_cx(&mut w, |cx| cx.conduct_init()));
    assert_eq!(list(&w), [KEEPER]);
}

#[test]
fn the_init_fails_on_the_grounds_and_in_a_tank_with_no_show_he_has_the_show_id_0() {
    let mut w = show_tank();
    let at = w.find(KEEPER).unwrap();
    w.place_unit(at, (9 * 64 + 32, 6 * 64 + 32, 0), 0);
    assert!(!with_cx(&mut w, |cx| cx.conduct_init()));
    assert!(list(&w).is_empty());
    let mut w = show_tank();
    w.zoo.shows.infos = vec![None];
    assert!(with_cx(&mut w, |cx| cx.conduct_init()));
    assert_eq!(w.units.get(KEEPER).unwrap().show, 0);
    assert_eq!(
        stands(&w).0,
        ladder(&w).platform,
        "he is on the platform all the same"
    );
}

#[test]
fn the_job_is_done_when_the_show_is_not_his_is_stopped_or_he_is_off_the_platform() {
    let mut w = show_tank();
    show_mut(&mut w).ready = true;
    with_cx(&mut w, |cx| {
        cx.unit.tank = 2;
        cx.conduct_init();
    });
    let done = |w: &mut World| {
        with_cx(w, |cx| {
            cx.record().job_done = false;
            cx.conduct_check_done();
            cx.record().job_done
        })
    };
    assert!(!done(&mut w), "announced, his show, on the platform");
    show_mut(&mut w).ready = false;
    assert!(done(&mut w), "stopped");
    show_mut(&mut w).ready = true;
    w.units.get_mut(KEEPER).unwrap().tank = 3;
    assert!(done(&mut w), "in the water");
    w.units.get_mut(KEEPER).unwrap().tank = 2;
    w.units.get_mut(KEEPER).unwrap().show = 1002;
    assert!(done(&mut w), "another show");
    // A running show of the scheduled script is not stopped.
    w.units.get_mut(KEEPER).unwrap().show = 1001;
    let info = w.zoo.shows.infos[0].as_mut().unwrap();
    info.schedule = vec![1];
    info.records.push(crate::zoo::TypeRecord::new(1, 3, 0));
    (info.show.ready, info.show.running, info.show.script) = (false, true, 3);
    assert!(!done(&mut w));
}

#[test]
fn with_no_show_info_and_show_id_0_the_job_is_not_done() {
    let mut w = show_tank();
    w.zoo.shows.infos = vec![None];
    with_cx(&mut w, |cx| {
        cx.unit.tank = 2;
        assert!(cx.conduct_init());
        assert_eq!(cx.unit.show, 0);
        cx.record().job_done = false;
        cx.conduct_check_done();
        assert!(!cx.record().job_done);
    });
}

#[test]
fn with_no_exhibit_the_check_resets_his_goal() {
    let mut w = show_tank();
    let at = w.find(KEEPER).unwrap();
    w.place_unit(at, (9 * 64 + 32, 6 * 64 + 32, 0), 0);
    let r = &mut w.units.get_mut(KEEPER).unwrap().goal.record;
    (r.task, r.job) = (DUTIES, super::CONDUCT);
    with_cx(&mut w, |cx| cx.conduct_check_done());
    let r = &w.units.get(KEEPER).unwrap().goal.record;
    assert_eq!((r.task, r.job), (0, 0));
}

#[test]
fn dropping_the_job_takes_him_out_of_the_list_and_gives_him_no_show() {
    let mut w = show_tank();
    with_cx(&mut w, |cx| {
        cx.conduct_init();
        // The object of another job leaves the list alone.
        cx.unit.goal.sub = Some(Sub::Keeper(24));
        cx.sub_dropped();
        assert_eq!(cx.unit.show, 1001);
        cx.unit.goal.sub = Some(Sub::Keeper(CONDUCT));
        cx.sub_dropped();
        assert!(cx.unit.goal.sub.is_none());
        assert_eq!(cx.unit.show, 0);
    });
    assert!(list(&w).is_empty());
    // The show gone: the show id goes all the same.
    let mut w = show_tank();
    w.zoo.shows.infos = vec![None];
    with_cx(&mut w, |cx| {
        cx.unit.show = 1001;
        cx.unit.goal.sub = Some(Sub::Keeper(CONDUCT));
        cx.sub_dropped();
        assert_eq!(cx.unit.show, 0);
    });
}

#[test]
fn the_duties_give_job_19_on_the_platform_and_job_23_off_it() {
    let mut w = show_tank();
    show_mut(&mut w).ready = true;
    show_mut(&mut w).keeper = MARINE;
    w.zoo.shows.infos[0].as_mut().unwrap().schedule = vec![SPECIES];
    let job = |w: &mut World, tank: u8| {
        with_cx(w, |cx| {
            cx.unit.tank = tank;
            cx.set_job(0, 0, None);
            cx.set_task(DUTIES, 0, 0, None);
            cx.duties_decide();
            cx.record().job
        })
    };
    assert_eq!(job(&mut w, 2), 19);
    assert_eq!(job(&mut w, 3), 23);
    assert_eq!(job(&mut w, 4), 23);
    // In the list already, no show job: nothing to do ends the duties.
    super::show::join(&mut w, &marine(), 1, KEEPER, &marine().types[0]);
    assert_eq!(job(&mut w, 2), 0);
    assert!(with_cx(&mut w, |cx| cx.staff().unwrap().duties_done));
}

#[test]
fn the_show_job_runs_from_the_duties_to_the_stop_and_he_leaves_at_the_next_update() {
    let mut w = show_tank();
    show_mut(&mut w).ready = true;
    show_mut(&mut w).keeper = MARINE;
    w.zoo.shows.infos[0].as_mut().unwrap().schedule = vec![SPECIES];
    let (cat, data, mut rng) = (marine(), Walker::new("fWalk(0,0)"), Rng::default());
    let at = w.find(KEEPER).unwrap();
    let r = &mut w.units.get_mut(KEEPER).unwrap().goal.record;
    r.task = DUTIES;
    let mut step = |w: &mut World| {
        let mut u = w.units.get(KEEPER).unwrap().clone();
        u.tank = 2;
        let (events, mut removals) = (Events::default(), Vec::new());
        let mut ctx = Ctx {
            rng: &mut rng,
            events: &events,
            removals: &mut removals,
            step: 67,
            cat: &cat,
        };
        update(&mut u, w, &mut ctx, at, &data);
        *w.units.get_mut(KEEPER).unwrap() = u;
        (w.units.get(KEEPER).unwrap().goal.record.job, list(w))
    };
    assert_eq!(step(&mut w).0, 19, "the duties decide");
    assert_eq!(step(&mut w), (19, vec![KEEPER]), "the job's init");
    assert_eq!(step(&mut w), (19, vec![KEEPER]));
    show_mut(&mut w).ready = false;
    assert_eq!(
        step(&mut w).0,
        0,
        "the check done sees the stop and the job completes"
    );
    assert_eq!(
        step(&mut w),
        (0, vec![]),
        "its object is dropped at the next update"
    );
}

#[test]
fn completing_the_job_clears_the_job_alone_not_his_own_target_tile() {
    let mut w = show_tank();
    with_cx(&mut w, |cx| {
        cx.unit.own_tile = Some(30);
        cx.set_job(CONDUCT, 0, None);
        cx.job_complete(CONDUCT);
        assert_eq!(cx.record().job, 0);
        assert_eq!(cx.unit.own_tile, Some(30));
    });
}
