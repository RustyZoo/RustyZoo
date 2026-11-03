//! The marine specialist's side of a show: who needs him, his cues, his rejoin after a load and
//! his removal (shows 22).

use super::*;
use crate::entity::EntityType;
use crate::show::CONDUCT;
use crate::sim::{KEEPER_CHANNEL, SHOW_CHANNEL};
use crate::unit::testing::show_tank;
use crate::unit::{Ctx, Kind};
use crate::zoo::{Item, Script};
use std::sync::Arc;

const MARINE: u32 = 9552;
const TANK: u32 = 1;

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
    cat
}

fn info(w: &mut World) -> &mut crate::zoo::ShowInfo {
    w.zoo.shows.infos[0].as_mut().unwrap()
}

/// The show of `show_tank` announced for the specialist.
fn announced() -> World {
    let mut w = show_tank();
    info(&mut w).show.ready = true;
    info(&mut w).show.keeper = MARINE;
    w
}

fn item(pre: &str, post: &str) -> Item {
    Item {
        available: 1,
        visible: 1,
        id: 0,
        kind: 0,
        marker: 0,
        names: ["t".into(), "a".into(), pre.into(), post.into()],
        building: 0,
        complexity: 0,
        returns: 0,
        satisfaction: 0,
        delta: 0,
        current: 0,
        depth: 0,
        help: [0; 2],
        icons: [String::new(), String::new()],
    }
}

/// Script 5 of the show: item 0 has both sets, item 1 only a pre-trick set, item 2 neither.
fn scripted() -> World {
    let mut w = show_tank();
    let items = vec![item("pre0", "post0"), item("pre1", ""), item("", "")];
    w.zoo.shows.scripts = vec![Script {
        id: 5,
        marker: 0,
        kind: 0,
        items,
    }];
    info(&mut w).show.script = 5;
    w
}

fn heard(sender: u32, kind: u16, extra: u16) -> Event {
    Event {
        sender,
        target: 0,
        data: 0,
        extra,
        kind,
        delay: 1,
        sender_channel: SHOW_CHANNEL,
        channel: KEEPER_CHANNEL,
    }
}

#[test]
fn a_ready_show_needs_a_keeper_of_its_type_only_while_none_is_in_its_list() {
    let mut w = show_tank();
    assert!(!needs_keeper(&w, TANK, MARINE), "not announced");
    info(&mut w).show.ready = true;
    assert!(!needs_keeper(&w, TANK, MARINE), "no keeper type yet");
    info(&mut w).show.keeper = MARINE;
    assert!(needs_keeper(&w, TANK, MARINE));
    assert!(!needs_keeper(&w, TANK, 9500), "a keeper of another type");
    assert!(!needs_keeper(&w, TANK, 0));
    assert!(!needs_keeper(&w, 99, MARINE), "no such exhibit");
    assert!(!needs_keeper(&w, 0, MARINE), "the grounds");
    units::join(&mut w, &marine(), TANK, 7, &marine().types[0]);
    assert!(!needs_keeper(&w, TANK, MARINE), "he is in the list");
    units::leave(&mut w, 1001, 7, MARINE);
    assert!(needs_keeper(&w, TANK, MARINE));
    info(&mut w).show.ready = false;
    assert!(!needs_keeper(&w, TANK, MARINE));
}

#[test]
fn an_exhibit_with_a_ready_show_calls_the_marine_keeper_and_no_other() {
    let cat = marine();
    let who = &cat.types[0];
    let mut w = announced();
    info(&mut w).show.ready = false;
    assert!(!crate::staff::service::needs(&w, &cat, TANK, who));
    info(&mut w).show.ready = true;
    assert!(crate::staff::service::needs(&w, &cat, TANK, who));
    let other = EntityType {
        class: "keeper",
        type_name: b"zookeeper".to_vec(),
        subtype: Vec::new(),
        file: None,
        config: None,
        name_id: 9500,
        footprint: [1, 1, 1],
        height: 0,
    };
    assert!(!crate::staff::service::needs(&w, &cat, TANK, &other));
}

#[test]
fn the_last_cue_of_each_kind_wins_and_the_post_trick_set_comes_before_the_pre_trick_one() {
    let w = scripted();
    let set = |cues: &[(u16, u16)]| {
        let heard: Vec<_> = cues.iter().map(|&(k, n)| heard(1001, k, n)).collect();
        cue_set(&w, 1001, &heard)
    };
    let (pre, post, end) = (event::NEXT_ITEM, event::TRICK_DONE, event::ENDED);
    assert_eq!(set(&[(pre, 0)]).as_deref(), Some("pre0"));
    assert_eq!(set(&[(pre, 0), (pre, 1)]).as_deref(), Some("pre1"));
    assert_eq!(set(&[(pre, 0), (post, 0)]).as_deref(), Some("post0"));
    assert_eq!(
        set(&[(post, 1), (post, 0), (pre, 1)]).as_deref(),
        Some("post0")
    );
    // A post-trick item with no set falls back to the pre-trick set of the same step.
    assert_eq!(set(&[(pre, 1), (post, 1)]).as_deref(), Some("pre1"));
    // The end changes a post cue that is there and adds none.
    assert_eq!(set(&[(post, 1), (end, 0)]).as_deref(), Some("post0"));
    assert_eq!(set(&[(end, 0)]), None);
    assert_eq!(set(&[(pre, 0), (end, 0)]).as_deref(), Some("pre0"));
    // Nothing heard, an item with no set, an item that is not there (-1 included).
    assert_eq!(set(&[]), None);
    assert_eq!(set(&[(pre, 2), (post, 2)]), None);
    assert_eq!(set(&[(pre, 3)]), None);
    assert_eq!(set(&[(pre, u16::MAX), (post, u16::MAX)]), None);
}

#[test]
fn only_the_cues_sent_by_his_show_count_and_a_show_with_no_script_gives_no_set() {
    let w = scripted();
    let ours = heard(1001, event::NEXT_ITEM, 0);
    assert_eq!(
        cue_set(&w, 1001, std::slice::from_ref(&ours)).as_deref(),
        Some("pre0")
    );
    let foreign = [
        heard(1002, event::NEXT_ITEM, 0),
        heard(0, event::NEXT_ITEM, 0),
    ];
    assert_eq!(cue_set(&w, 1001, &foreign), None);
    // The sender is his show id: a keeper with none (0) hears the world's events, which carry 0.
    assert_eq!(cue_set(&w, 0, &foreign).as_deref(), None);
    let mut none = scripted();
    info(&mut none).show.script = 6;
    assert_eq!(cue_set(&none, 1001, &[ours]), None);
    // No show info of that id.
    let gone = heard(4242, event::NEXT_ITEM, 0);
    assert_eq!(cue_set(&w, 4242, &[gone]), None);
}

#[test]
fn a_loaded_keeper_whose_job_is_19_rejoins_the_show_of_his_exhibit() {
    let cat = marine();
    let mut w = show_tank();
    rejoin(&mut w, &cat);
    assert!(info(&mut w).units(MARINE).is_empty(), "no job, no list");
    w.units.get_mut(7).unwrap().goal.record.job = 20;
    rejoin(&mut w, &cat);
    assert!(info(&mut w).units(MARINE).is_empty(), "another job");
    w.units.get_mut(7).unwrap().goal.record.job = CONDUCT;
    rejoin(&mut w, &cat);
    rejoin(&mut w, &cat);
    assert_eq!(info(&mut w).units(MARINE), [7], "once, not twice");
    // On the grounds he is in no exhibit.
    let mut w = show_tank();
    let at = w.find(7).unwrap();
    w.place_unit(at, (9 * 64, 6 * 64, 0), 0);
    w.units.get_mut(7).unwrap().goal.record.job = CONDUCT;
    rejoin(&mut w, &cat);
    assert!(info(&mut w).units(MARINE).is_empty());
}

#[test]
fn removing_the_specialist_takes_him_out_of_the_list_and_aborts_the_show() {
    let cat = marine();
    let mut w = announced();
    units::join(&mut w, &cat, TANK, 7, &cat.types[0]);
    w.units.get_mut(7).unwrap().show = 1001;
    removed(&mut w, &cat, 7);
    assert!(info(&mut w).units(MARINE).is_empty());
    assert!(!info(&mut w).show.ready, "the show is reset");
    let kinds: Vec<_> = w.units.posts.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [event::ABORTED, event::ENDED]);
}

#[test]
fn a_keeper_with_no_show_or_with_a_show_that_is_gone_aborts_nothing() {
    let cat = marine();
    let mut w = announced();
    removed(&mut w, &cat, 7);
    assert!(
        w.units.posts.is_empty() && info(&mut w).show.ready,
        "no show id"
    );
    w.units.get_mut(7).unwrap().show = 4242;
    removed(&mut w, &cat, 7);
    assert!(
        w.units.posts.is_empty() && info(&mut w).show.ready,
        "no such show"
    );
    removed(&mut w, &cat, 99);
    assert!(w.units.posts.is_empty(), "no such unit");
}

/// The set the specialist runs after the cue hook has read `heard`, his job being `job`, his show
/// `show` and his kind `kind`; the script's pre-trick set of item 0 is one the test data has.
fn cued(heard: Vec<Event>, job: u8, show: u16, kind: Kind) -> Option<String> {
    let mut w = scripted();
    w.zoo.shows.scripts[0].items[0].names[2] = "bWalk".into();
    let data = crate::unit::testing::Walker::new("fWalk(0,0)");
    let mut u = w.units.get(7).unwrap().clone();
    (u.kind, u.show, u.events) = (kind, show, heard);
    u.goal.record.job = job;
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut rng = crate::sim::Rng::default();
    let cat = marine();
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let at = w.find(7).unwrap();
    cue(&mut u, &mut w, &mut ctx, at, &data);
    u.runner.top().map(str::to_string)
}

#[test]
fn a_cue_starts_the_set_of_the_item_only_for_a_keeper_whose_job_is_19_and_whose_show_sent_it() {
    let cue = || vec![heard(1001, event::NEXT_ITEM, 0)];
    assert_eq!(
        cued(cue(), CONDUCT, 1001, Kind::Keeper).as_deref(),
        Some("bWalk")
    );
    assert_eq!(cued(cue(), 20, 1001, Kind::Keeper), None, "another job");
    assert_eq!(cued(cue(), 0, 1001, Kind::Keeper), None, "no job");
    assert_eq!(
        cued(cue(), CONDUCT, 1002, Kind::Keeper),
        None,
        "another show"
    );
    assert_eq!(
        cued(cue(), CONDUCT, 1001, Kind::Worker),
        None,
        "not a keeper"
    );
    assert_eq!(
        cued(Vec::new(), CONDUCT, 1001, Kind::Keeper),
        None,
        "no cue"
    );
}

/// The show of `announced` with the specialist in its list and his show id.
fn conducting() -> World {
    let mut w = announced();
    units::join(&mut w, &marine(), TANK, 7, &marine().types[0]);
    w.units.get_mut(7).unwrap().show = 1001;
    w
}

#[test]
fn the_unit_turn_reads_the_cue_before_anything_else() {
    let mut w = scripted();
    w.zoo.shows.scripts[0].items[0].names[2] = "bWalk".into();
    w.units.get_mut(7).unwrap().show = 1001;
    w.units.get_mut(7).unwrap().goal.record.job = CONDUCT;
    let mut events = Events::default();
    events.post(heard(1001, event::NEXT_ITEM, 0));
    events.age();
    let (mut removals, mut rng, cat) = (Vec::new(), crate::sim::Rng::default(), marine());
    let data = crate::unit::testing::Walker::new("fWalk(0,0)");
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    let at = w.find(7).unwrap();
    crate::unit::update(
        &mut crate::unit::Standard { data: &data },
        &mut w,
        &mut ctx,
        at,
    );
    assert_eq!(w.units.get(7).unwrap().runner.top(), Some("bWalk"));
}

#[test]
fn firing_the_specialist_aborts_his_show_before_the_entity_goes() {
    let cat = marine();
    let mut w = conducting();
    let fired = crate::place::bulldozer::remove(
        &mut w,
        &mut crate::money::Money::default(),
        &cat,
        7,
        &mut crate::sim::Rng::default(),
    );
    assert!(fired.is_some());
    assert!(info(&mut w).units(MARINE).is_empty() && !info(&mut w).show.ready);
    assert_eq!(w.units.posts.len(), 2);
}

#[test]
fn a_removal_of_the_next_step_leaves_the_list_and_aborts_the_show_too() {
    let cat = marine();
    let mut w = conducting();
    let mut sim = crate::sim::Sim::new(b"");
    sim.remove(7);
    sim.step_in(&mut w, &cat, 67, |_, _, _| {});
    assert!(w.find(7).is_none());
    assert!(info(&mut w).units(MARINE).is_empty() && !info(&mut w).show.ready);
}

#[test]
fn a_zoo_loaded_with_a_specialist_on_job_19_has_him_in_the_list_again() {
    let cat = marine();
    let mut w = announced();
    let mut block = crate::zoo::UnitBlock::default();
    (block.target.goal, block.target.sub_goal) =
        (i32::from(crate::goal::DUTIES), i32::from(CONDUCT));
    (block.target.active, block.target.active_sub) = (110, i32::from(CONDUCT));
    block.flags[1] = (1, 1001);
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().unit = Some(Box::new(block));
    crate::sim::Sim::new(b"").load(&mut w, &cat);
    assert_eq!(info(&mut w).units(MARINE), [7]);
    let u = w.units.get(7).unwrap();
    assert_eq!((u.goal.record.job, u.show), (CONDUCT, 1001));
}
