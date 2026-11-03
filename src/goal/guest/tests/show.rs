//! A guest goes to a show and pays for its seat (shows 7.1 to 7.3, goals 10.2).

use super::*;
use crate::building;
use crate::config::Config;
use crate::entity::EntityType;
use crate::zoo::{Show, ShowInfo};
use std::sync::Arc;

/// A grandstand of 12 seats that sits its users and charges the default price.
const STAND: &str = "[Characteristics/Floats]\ncPriceFactor=2\n[Characteristics/Integers]\ncCommerce=1\n\
    cCapacity=12\ncUserStaysOutside=0\ncUserUsesExit=1\ncHoldsOntoUser=1\n[Satisfies]\nbuilding\n\
    grandstand\n[slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\n\
    exitpos=0\nexitpos=48\ncapacity=12\n";

fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    for (class, name, text) in [("building", "stand", STAND), ("guests", "man", "")] {
        cat.push(EntityType {
            class,
            type_name: name.as_bytes().to_vec(),
            subtype: Vec::new(),
            file: None,
            config: Some(Arc::new(Config::parse(text.as_bytes()).unwrap())),
            name_id: 9503,
            footprint: [1, 1, 1],
            height: 0,
        });
    }
    cat
}

/// Guest 1 at tile (2, 5) and grandstand 7 at (5, 5) belonging to show 1001, whose flag is
/// `soon`.
fn zoo(soon: bool) -> (World, Catalogue) {
    let cat = types();
    let mut w = crate::unit::testing::world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 5)), 0));
        zoo.entities
            .push(entity("building", "stand", 7, centre((5, 5)), 4));
        zoo.shows.infos = vec![Some(ShowInfo {
            frequency: 90,
            index: 0,
            schedule: Vec::new(),
            records: Vec::new(),
            events: Vec::new(),
            next: 0,
            id: 1001,
            upkeep: [0; 3],
            receipts: [0; 3],
            attendance: [0; 3],
            show: Show::default(),
        })];
    });
    building::built(&mut w, &cat.types[0], 7, 0);
    let live = w.units.buildings.get_mut(&7).unwrap();
    (live.fill, live.show_id) = (u32::from(soon), 1001);
    w.units.get_mut(1).unwrap().part = Part::Guest(Guest::default());
    (w, cat)
}

fn need(w: &World) -> (Option<&'static str>, u32) {
    match &w.units.get(1).unwrap().part {
        Part::Guest(g) => (g.need, g.building),
        _ => unreachable!(),
    }
}

#[test]
fn a_guest_without_a_task_goes_to_a_grandstand_with_a_show_soon_serving_the_need_grandstand() {
    let (mut w, cat) = zoo(true);
    let mut rng = Rng::default();
    let went = with_cx_in(&cat, &walks(), &mut w, &mut rng, |cx| {
        (cx.next_site(), cx.record().task, cx.record().target.id)
    });
    assert_eq!(went, (true, 102, 7));
    assert_eq!(need(&w), (Some("grandstand"), 7));
    assert_ne!(rng, Rng::default());
}

#[test]
fn a_grandstand_with_no_show_soon_is_not_gone_to() {
    let (mut w, cat) = zoo(false);
    let mut rng = Rng::default();
    let went = with_cx_in(&cat, &NoData, &mut w, &mut rng, |cx| {
        (cx.next_site(), cx.record().task)
    });
    assert_eq!(went, (false, 0));
    assert_eq!(need(&w), (None, 0));
}

#[test]
fn a_guest_that_came_for_a_show_is_refused_when_the_flag_is_down() {
    let (mut w, cat) = zoo(false);
    let mut rng = Rng::default();
    let mut enter = |w: &mut World, need| {
        with_cx_in(&cat, &NoData, w, &mut rng, |cx| {
            if let Part::Guest(g) = &mut cx.unit.part {
                g.need = need;
            }
            cx.add_user(7, 0)
        })
    };
    assert!(!enter(&mut w, Some("grandstand")));
    assert!(w.units.buildings[&7].users.is_empty());
    // Another need, and the same guest once the show is soon, are let in.
    assert!(enter(&mut w, Some("fun")));
    w.units.buildings.get_mut(&7).unwrap().users.clear();
    w.units.buildings.get_mut(&7).unwrap().fill = 1;
    assert!(enter(&mut w, Some("grandstand")));
}

#[test]
fn a_seat_at_a_show_is_paid_to_the_show_whoever_sits_and_a_grandstand_with_no_show_books_nothing() {
    let (mut w, cat) = zoo(true);
    let mut rng = Rng::default();
    let price = w.units.buildings[&7].price;
    let mut enter = |w: &mut World| {
        with_cx_in(&cat, &NoData, w, &mut rng, |cx| {
            if let Part::Guest(g) = &mut cx.unit.part {
                g.need = Some("grandstand");
            }
            assert!(cx.add_user(7, 0));
        });
        w.units.buildings.get_mut(&7).unwrap().users.clear();
    };
    enter(&mut w);
    assert_eq!(w.units.show_income, [2.0 * price]);
    let info = w.zoo.shows.infos[0].as_ref().unwrap();
    assert_eq!(f32::from_bits(info.receipts[0]), 2.0 * price);
    assert_eq!(info.attendance, [1, 0, 1]);
    // A guest that returns pays too; a stand of no show does not.
    enter(&mut w);
    assert_eq!(w.units.show_income.len(), 2);
    w.units.buildings.get_mut(&7).unwrap().show_id = 0;
    enter(&mut w);
    assert_eq!(w.units.show_income.len(), 2);
    assert_eq!(w.zoo.shows.infos[0].as_ref().unwrap().attendance[0], 2);
}

#[test]
fn the_step_books_the_seats_as_show_income_and_reads_the_reach_from_the_ini() {
    for (ini, reach) in [
        ("", 150),
        ("[AI]\nmaxDistToGoToShow=7\n", 7),
        ("[AI]\nmaxDistToGoToShow=-5\n", -5),
    ] {
        let (mut w, _) = zoo(true);
        w.units.show_income.push(3.0);
        let mut s = Sim::new(ini.as_bytes());
        s.set_seed(1);
        s.start_game(100, Clocks::default());
        let mut money = Money::default();
        run(&mut w, &mut s, &mut money, 1);
        assert_eq!(w.units.show.distance, reach, "{ini:?}");
        assert_eq!(money.total[money::SHOW_INCOME], 3.0);
        assert!(w.units.show_income.is_empty());
    }
}

/// The goal (need, building, task), the wandered flag and the thoughts after a rung.
type After = ((Option<&'static str>, u32, u8), bool, Vec<i32>);

/// Runs the show rung for the guest of `zoo(true)` with a stand of `users` users; the rung's
/// answer, the goal, the wandered flag and the thoughts after it.
fn rung(users: usize, walk: &dyn Data) -> (bool, After) {
    let (mut w, cat) = zoo(true);
    w.units.buildings.get_mut(&7).unwrap().users = vec![(2, 0, 0); users];
    let mut rng = Rng::default();
    let went = with_cx_in(&cat, walk, &mut w, &mut rng, |cx| {
        (cx.next_site(), cx.record().task)
    });
    let (need, building) = need(&w);
    let u = w.units.get(1).unwrap();
    let thoughts = w.units.thoughts.of(1).iter().map(|t| t.text).collect();
    (
        went.0,
        ((need, building, went.1), u.goal.wandered, thoughts),
    )
}

#[test]
fn a_grandstand_with_no_open_slot_resets_the_goal_and_raises_the_wandered_flag() {
    // Twelve users of twelve seats, and a free seat that no path leads to: the ladder ends with
    // no task, no need, no building target, and the flag up.
    for (users, walk) in [(12, &walks() as &dyn Data), (0, &NoData)] {
        let (ended, (goal, wandered, _)) = rung(users, walk);
        assert_eq!(
            (ended, goal, wandered),
            (true, (None, 0, 0), true),
            "{users}"
        );
    }
    // A free seat with a path is gone to, and the flag stays down.
    let (ended, (goal, wandered, _)) = rung(11, &walks());
    assert_eq!(
        (ended, goal, wandered),
        (true, (Some("grandstand"), 7, 102), false)
    );
}

#[test]
fn a_full_grandstand_found_by_the_rung_makes_the_guest_think_crowded() {
    assert_eq!(rung(12, &walks()).1.2, [10243]);
    // With no path the guest thinks nothing, and with room it thinks nothing.
    assert!(rung(0, &NoData).1.2.is_empty());
    assert!(rung(11, &walks()).1.2.is_empty());
}

#[test]
fn leaving_a_grandstand_makes_the_price_reaction_and_the_status_tick_takes_it_later() {
    // The default price makes no reaction; an expensive or a cheap one thinks (guests 9.4).
    for (factor, thought) in [(1.0, None), (100.0, Some(10073)), (0.0, Some(10072))] {
        let (mut w, cat) = zoo(true);
        let at = w.find(7).unwrap();
        let kind = building::kind_of(&cat, &w.zoo.entities[at], w.units.prices.default).unwrap();
        w.units.buildings.get_mut(&7).unwrap().price = factor * kind.price;
        let mut unit = w.units.get(1).unwrap().clone();
        let guest = w.find(1).unwrap();
        building::leave::pay(&mut w, &cat, guest, &mut unit, 7, &kind);
        let thoughts: Vec<_> = w.units.thoughts.of(1).iter().map(|t| t.text).collect();
        assert_eq!(
            thoughts,
            thought.into_iter().collect::<Vec<_>>(),
            "{factor}"
        );
        // The values wait for the status tick.
        let Part::Guest(g) = &unit.part else { panic!() };
        assert_eq!(g.values, Guest::default().values);
        assert_eq!(g.pending[HAPPINESS] != 0, thought.is_some(), "{factor}");
    }
}
