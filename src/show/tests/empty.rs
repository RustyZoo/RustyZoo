//! Emptying the grandstands (shows 30.3) and the drain (30.4 point 3), on the staged show: stand 7
//! faces the tank with guests 1 and 2, stand 8 is turned away with guest 3.

use super::cycle::{heard, info_of_show, item, staged, started, tick};
use super::*;
use crate::animal::Animal;
use crate::behaviour::NoData;
use crate::building::Seat;
use crate::rubble::{Kit, turn_to_rubble};
use crate::sim::{Event, Rng, UNIT_CHANNEL};
use crate::unit::{Ctx, Part};

/// The kinds of the events posted outside the show manager's step, in order.
fn posted(w: &World) -> Vec<u16> {
    w.units.posts.iter().map(|e| e.kind).collect()
}

/// The show events among them.
fn show_events(w: &World) -> Vec<u16> {
    let shown = [event::ABORTED, event::ENDED];
    posted(w)
        .into_iter()
        .filter(|k| shown.contains(k))
        .collect()
}

/// Seats guests 1 and 2 of stand 7 on draw places.
fn seat(w: &mut World) {
    let live = w.units.buildings.get_mut(&7).unwrap();
    live.seats = [(1, 5), (2, 3)]
        .map(|(unit, place)| Seat {
            unit,
            slot: 0,
            place,
        })
        .to_vec();
}

fn rubble(w: &mut World, cat: &Catalogue, id: u32) -> bool {
    let shape = |_: &crate::zoo::Entity| None;
    let mut kit = Kit {
        cat,
        rng: &mut Rng::default(),
        template: "piece-%s",
        shape: &shape,
    };
    turn_to_rubble(w, &mut kit, id)
}

#[test]
fn a_grandstand_turned_to_rubble_aborts_its_show_and_puts_its_audience_out() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    seat(&mut w);
    events.clear();
    w.units.ai_ticks = 200;
    assert!(rubble(&mut w, &cat, 7));
    // 0x2717 then 0x2715 for the next step, the show stopped and its schedule advanced.
    assert_eq!(show_events(&w), [event::ABORTED, event::ENDED]);
    let info = info_of_show(&w);
    assert!(!info.show.running && info.next == 290);
    assert!(w.find(7).is_none());
    // The audience is at the exit, whatever the grandstand held.
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x, c.y), (240, 160));
    // The other stand, turned away, keeps its guest.
    assert_eq!(w.units.buildings[&8].users.len(), 1);
}

#[test]
fn an_emptied_audience_gets_no_show_event() {
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    seat(&mut w);
    // Stand 7 stays; the cheat empties it from stand 8's side: both face the same show here.
    w.units.buildings.get_mut(&8).unwrap().show_id = 1001;
    crate::show::empty_grandstands_of(&mut w, &cat, 8);
    assert_eq!(show_events(&w), [event::ABORTED, event::ENDED]);
    // The next step: the world relays both to the stands of the show, which read them at once.
    w.units.posts.iter().for_each(|e| events.post(e.clone()));
    events.age();
    let mut rng = Rng::default();
    crate::show::listen_world(&mut w, &cat, &mut events, &mut rng);
    events.age();
    for at in 0..w.zoo.entities.len() {
        crate::show::listen_grandstand(&mut w, &cat, &mut events, &mut rng, at);
    }
    let to_guests =
        |e: &&Event| e.channel == UNIT_CHANNEL && [event::ABORTED, event::ENDED].contains(&e.kind);
    assert_eq!(
        events
            .buffers()
            .iter()
            .flat_map(|b| b.iter())
            .filter(to_guests)
            .count(),
        0
    );
    // The same without the emptying: the seated guests get both, each with its delay.
    let (mut w, cat, mut events) = started(vec![item(0, 3, 5)]);
    seat(&mut w);
    events.post(Event {
        sender: 1001,
        target: 0,
        data: 0,
        extra: 0,
        kind: event::ABORTED,
        delay: 0,
        sender_channel: crate::sim::SHOW_CHANNEL,
        channel: crate::sim::WORLD_CHANNEL,
    });
    events.age();
    crate::show::listen_world(&mut w, &cat, &mut events, &mut rng);
    events.age();
    for at in 0..w.zoo.entities.len() {
        crate::show::listen_grandstand(&mut w, &cat, &mut events, &mut rng, at);
    }
    assert_eq!(
        events
            .buffers()
            .iter()
            .flat_map(|b| b.iter())
            .filter(to_guests)
            .count(),
        2
    );
}

#[test]
fn a_grandstand_turned_to_rubble_with_no_show_aborts_nothing() {
    let (mut w, cat, _) = started(vec![item(0, 3, 5)]);
    w.units.buildings.get_mut(&7).unwrap().show_id = 0;
    assert!(rubble(&mut w, &cat, 7));
    assert!(show_events(&w).is_empty());
    assert!(info_of_show(&w).show.running);
    // The users still leave with their building.
    assert!(w.units.buildings.get(&7).is_none_or(|l| l.users.is_empty()));
}

/// The staged world with a loose lion at tile (3, 1) and the checks of guest 1 due, which goes to
/// stand 7 (its building target).
fn frightened(rampaging: bool) -> (World, Catalogue, Events) {
    let (mut w, mut cat, events) = started(vec![item(0, 3, 5)]);
    cat.push(EntityType {
        name_id: 9503,
        ..kind(
            "guests",
            "man",
            "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\n",
        )
    });
    cat.push(EntityType {
        name_id: 5006,
        subtype: b"m".to_vec(),
        ..kind(
            "animals",
            "lion",
            "[m/Characteristics/Integers]\ncPrey=9503\n",
        )
    });
    let mut lion = entity("animals", "lion", 50, centre((3, 1)), 0);
    lion.subtype = "m".into();
    w.zoo.entities.push(lion);
    let all = w.zoo.entities.clone();
    w.units.sync(&all);
    w.units.escapes.push(50);
    w.units.get_mut(50).unwrap().part = Part::Animal(Animal {
        escaped: true,
        rampaging,
        ..Animal::default()
    });
    w.units.get_mut(1).unwrap().part = Part::Guest(crate::guest::Guest {
        countdowns: [
            1000, 1000, 1000, 1000, 1000, 1000, 0, 1000, 1000, 1000, 1000,
        ],
        saw_animal: 1000,
        building: 7,
        ..Default::default()
    });
    seat(&mut w);
    (w, cat, events)
}

/// The status checks of guest 1.
fn checks(w: &mut World, cat: &Catalogue) {
    let mut unit = w.units.get(1).unwrap().clone();
    let at = w.find(1).unwrap();
    let (mut rng, events, mut removals) = (Rng::default(), Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    crate::guest::checks(&mut unit, w, &mut ctx, at, &NoData);
    *w.units.get_mut(1).unwrap() = unit;
}

#[test]
fn a_guest_that_sees_a_rampaging_animal_on_its_way_to_a_grandstand_empties_it() {
    let (mut w, cat, _) = frightened(true);
    checks(&mut w, &cat);
    assert_eq!(show_events(&w), [event::ABORTED, event::ENDED]);
    assert!(!info_of_show(&w).show.running);
    assert!(w.units.buildings[&7].users.is_empty());
    // The guest itself was in the audience: it was put out, and it is afraid all the same.
    let u = w.units.get(1).unwrap();
    assert!(u.afraid && u.frightened && !u.hidden);
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x, c.y), (240, 160));
}

#[test]
fn a_guest_that_sees_an_animal_that_does_not_rampage_empties_nothing() {
    let (mut w, cat, _) = frightened(false);
    checks(&mut w, &cat);
    assert!(show_events(&w).is_empty());
    assert!(info_of_show(&w).show.running);
    assert_eq!(w.units.buildings[&7].users.len(), 2);
    assert!(w.units.get(1).unwrap().frightened);
    // A rampaging one and a guest with no building target: nothing either.
    let (mut w, cat, _) = frightened(true);
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        g.building = 0;
    }
    checks(&mut w, &cat);
    assert!(show_events(&w).is_empty());
}

/// Guest 1, frightened and not yet in any task, takes its goal update (the ladder's fright
/// point). Whether it took the avoid task.
fn ladder(w: &mut World, cat: &Catalogue) -> bool {
    w.units.get_mut(1).unwrap().frightened = true;
    let mut unit = w.units.get(1).unwrap().clone();
    let at = w.find(1).unwrap();
    let (mut rng, events, mut removals) = (Rng::default(), Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    crate::goal::update(&mut unit, w, &mut ctx, at, &NoData);
    let avoids = unit.goal.record.task == crate::goal::AVOID;
    *w.units.get_mut(1).unwrap() = unit;
    avoids
}

#[test]
fn a_frightened_guest_bound_for_a_grandstand_empties_it_from_the_ladder() {
    let (mut w, cat, _) = frightened(true);
    ladder(&mut w, &cat);
    assert_eq!(show_events(&w), [event::ABORTED, event::ENDED]);
    assert!(!info_of_show(&w).show.running);
    assert!(w.units.buildings[&7].users.is_empty());
    // An animal that does not rampage empties nothing.
    let (mut w, cat, _) = frightened(false);
    ladder(&mut w, &cat);
    assert!(show_events(&w).is_empty());
    assert_eq!(w.units.buildings[&7].users.len(), 2);
}

#[test]
fn a_frightened_guest_bound_for_a_grandstand_does_not_avoid() {
    for rampaging in [true, false] {
        let (mut w, cat, _) = frightened(rampaging);
        assert!(!ladder(&mut w, &cat));
        assert!(w.units.get(1).unwrap().frightened);
    }
    // Bound for no grandstand, it avoids the animal.
    let (mut w, cat, _) = frightened(true);
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        g.building = 0;
    }
    assert!(ladder(&mut w, &cat));
}

#[test]
fn a_deleted_grandstand_aborts_nothing() {
    let (mut w, cat, _) = started(vec![item(0, 3, 5)]);
    crate::building::lift(&mut w, &cat, 7, &mut Rng::default());
    assert!(show_events(&w).is_empty());
    assert!(info_of_show(&w).show.running);
}

#[test]
fn deleting_a_grandstand_with_the_bulldozer_aborts_nothing() {
    let (mut w, cat, _) = started(vec![item(0, 3, 5)]);
    seat(&mut w);
    let mut money = crate::money::Money::default();
    let removed = crate::place::bulldozer::remove(&mut w, &mut money, &cat, 7, &mut Rng::default());
    assert!(removed.is_some());
    assert!(w.find(7).is_none());
    assert!(show_events(&w).is_empty());
    assert!(info_of_show(&w).show.running);
}

#[test]
fn draining_the_tank_of_a_show_aborts_it_at_the_next_step() {
    let (mut w, cat) = staged(vec![item(0, 3, 5)]);
    let tank = super::cycle::tank_mut(&mut w).id;
    assert!(w.drain(tank));
    assert_eq!(posted(&w), [event::ABORT]);
    let mut events = Events::default();
    w.units.posts.drain(..).for_each(|e| events.post(e));
    events.age();
    tick(&mut w, &cat, &mut events, 10);
    // Both events, the schedule moved on: the next show is pushed back to now + 90.
    let sent: Vec<u16> = heard(&events).iter().map(|e| e.0).collect();
    assert_eq!(sent, [event::ABORTED, event::ENDED]);
    assert_eq!(info_of_show(&w).next, 10 + 90);
    // A tank with no show info posts nothing.
    let (mut w, ..) = staged(vec![item(0, 3, 5)]);
    let other = w
        .exhibits()
        .list()
        .iter()
        .map(|x| x.id)
        .find(|&x| x != tank)
        .unwrap();
    w.drain(other);
    assert!(posted(&w).is_empty());
}

#[test]
fn draining_a_tank_with_an_animal_in_it_posts_nothing() {
    let (mut w, _) = staged(vec![item(0, 3, 5)]);
    let tank = super::cycle::tank_mut(&mut w).id;
    let seal = entity("animals", "seal", 99, centre((6, 1)), 0);
    let shape = crate::world::Shape {
        kind: crate::render::order::Kind::Unit,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: crate::world::TankKind::NONE,
        walkable: (false, false),
        forms: None,
        rubbleable: false,
    };
    w.add_rated(seal, Some(shape), false);
    assert!(!w.drain(tank));
    assert!(w.tank(tank).unwrap().filled);
    assert!(posted(&w).is_empty());
}

#[test]
fn changing_the_salinity_of_a_show_tank_aborts_its_show() {
    let (mut w, _) = staged(vec![item(0, 3, 5)]);
    let tank = super::cycle::tank_mut(&mut w).id;
    // The same salinity returns before the drain.
    assert!(w.set_salinity(tank, false));
    assert!(posted(&w).is_empty());
    assert!(w.set_salinity(tank, true));
    assert_eq!(posted(&w), [event::ABORT]);
}

#[test]
fn a_grandstand_trampled_by_a_rampaging_animal_aborts_its_show_as_the_cheat_does() {
    let (mut w, cat, _) = started(vec![item(0, 3, 5)]);
    let shape = |_: &crate::zoo::Entity| None;
    let mut kit = Kit {
        cat: &cat,
        rng: &mut Rng::default(),
        template: "piece-%s",
        shape: &shape,
    };
    assert!(crate::rubble::leave_rubble(&mut w, &mut kit, 7));
    assert_eq!(show_events(&w), [event::ABORTED, event::ENDED]);
    assert!(!info_of_show(&w).show.running);
    assert!(w.units.buildings[&7].users.is_empty());
}

#[test]
fn an_abort_leaves_a_performer_listed_as_a_user_of_its_building() {
    // Shows 30.7: the reset of the abort takes the animal out of no building.
    let (mut w, cat, _) = started(vec![item(0, 3, 5)]);
    w.units.get_mut(21).unwrap().show = 1001;
    w.units
        .buildings
        .get_mut(&8)
        .unwrap()
        .users
        .push((21, 0, 0));
    crate::show::abort_show(&mut w, &cat, 1001);
    assert!(w.units.get(21).unwrap().back);
    assert!(w.units.buildings[&8].users.iter().any(|u| u.0 == 21));
}
