//! A guest uses a hot dog stand and a restroom on a synthetic map.

use super::*;
use crate::building::Prices;
use crate::entity::{Catalogue, EntityType};
use crate::goal::guest::tests::{sim, walks};
use crate::guest::{BATHROOM, HAPPINESS, HUNGER};
use crate::money::{self, Money};
use crate::sim::{Events, Rng, Sim};
use crate::unit::testing::{centre, entity, world_with};
use crate::unit::{Ctx, Unit};
use crate::world::World;
use std::sync::Arc;

const STAND: &str = "[Characteristics/Integers]\ncCommerce=1\ncCapacity=2\ncTimeInside=3\n\
    [Characteristics/Floats]\ncDefaultCost=2\ncPriceFactor=3\n[Sells]\nhotdog\n\
    [Satisfies]\nbuilding\nfood\n[slots]\nname=slot\n[slot]\nslotpos=-16\nslotpos=48\n\
    entrpos=-16\nentrpos=48\nentrpos=16\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=2\n";
const RESTROOM: &str = "[Characteristics/Integers]\ncCapacity=2\ncTimeInside=5\ncHideUser=1\n\
    cUserStaysOutside=0\ncBathroomChange=-100\n[Satisfies]\nbuilding\nbathroom\n\
    [slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\n\
    exitpos=0\nexitpos=48\ncapacity=2\n";
const TABLE: &str = "[Characteristics/Integers]\ncCapacity=1\ncTimeInside=2\n\
    cUserStaysOutside=0\ncUserUsesExit=1\n[Satisfies]\nbuilding\neat\n[slots]\nname=slot\n\
    [slot]\nslotpos=-16\nslotpos=16\nentrpos=-16\nentrpos=48\nexitpos=-16\nexitpos=48\n\
    capacity=1\nfacing=7\n";
const CAN: &str = "[Characteristics/Integers]\ncCapacity=1000\ncUserStaysOutside=1\n\
    cUsedThought=10215\n[Removes]\ntrash\n[Satisfies]\nbuilding\ntrash\n[slots]\nname=slot\n\
    [slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1000\n";
const MAN: &str = "[Global]\nDefaultSubtype=man\n[man/Characteristics/Integers]\n\
    cPriceHappy1Change=5\ncPriceAngry1Change=-5\n";

fn ty(class: &'static str, name: &str, text: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: Vec::new(),
        file: None,
        config: Some(Arc::new(
            crate::config::Config::parse(text.as_bytes()).unwrap(),
        )),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    }
}

fn types() -> Catalogue {
    let mut cat = Catalogue::default();
    cat.push(ty("guests", "man", MAN));
    cat.push(ty("building", "stand", STAND));
    cat.push(ty("building", "restroom", RESTROOM));
    cat.push(ty("building", "table", TABLE));
    cat.push(ty("building", "can", CAN));
    let trash = EntityType {
        subtype: b"trash".to_vec(),
        ..ty("objects", "other", "")
    };
    cat.push(trash);
    let litter = Item {
        id: LITTER,
        now: true,
        ..Default::default()
    };
    cat.items.insert("trash".into(), litter);
    let item = Item {
        id: 6303,
        record: [50, 50, -75, 15, 0, 0],
        now: false,
        litter: true,
        remove: true,
        thought: 10659,
        ..Default::default()
    };
    cat.items.insert("hotdog".into(), item);
    cat
}

/// Guest 1 at (2, 6); a stand 7 at (5, 5), whose entrances are on row 6, and a restroom 8 at
/// (5, 8), whose entrance is on row 9.
fn zoo() -> World {
    zoo_with(false)
}

/// The same, with a table 9 at (5, 3) whose entrance is on row 4.
fn zoo_with(table: bool) -> World {
    world_with((10, 10), |zoo| {
        if table {
            zoo.entities
                .push(entity("building", "table", 9, centre((5, 3)), 4));
        }
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 6)), 0));
        zoo.entities
            .push(entity("building", "stand", 7, centre((5, 5)), 4));
        zoo.entities
            .push(entity("building", "restroom", 8, centre((5, 8)), 4));
    })
}

/// Runs `f` on the goal update context of guest 1.
fn with_cx<R>(cat: &Catalogue, w: &mut World, f: impl FnOnce(&mut Cx) -> R) -> R {
    let mut u: Unit = w.units.get(1).unwrap().clone();
    let (events, mut removals, mut rng) = (Events::default(), Vec::new(), Rng::default());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    let data = walks();
    let mut cx = Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at: 0,
        data: &data,
    };
    let r = f(&mut cx);
    *w.units.get_mut(1).unwrap() = u;
    r
}

fn guest(w: &World) -> &Guest {
    match &w.units.get(1).unwrap().part {
        Part::Guest(g) => g,
        _ => unreachable!(),
    }
}

/// A game with the prices of the shipped economy.
fn game() -> Sim {
    let mut s = sim();
    s.status.settings.prices = Prices {
        expensive: 2.0,
        cheap: 0.5,
        default: 3.0,
    };
    s
}

/// Runs steps until `stop` finds a line or `n` steps are over; the `building` lines.
fn run(
    s: &mut Sim,
    cat: &Catalogue,
    w: &mut World,
    money: &mut Money,
    n: usize,
    stop: &str,
) -> Vec<String> {
    let mut log = Vec::new();
    for _ in 0..n {
        s.step_game(&walks(), cat, w, money, 67, |_, _, _| {});
        log.extend(w.units.take_building_lines());
        w.units.take_lines();
        w.units.take_guest_lines();
        if log.iter().any(|l| l.starts_with(stop)) {
            break;
        }
    }
    log
}

/// The guest is hungry and its checks are far from firing: it seeks when the test says so.
fn hungry(w: &mut World) {
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.values[HUNGER], g.countdowns, g.saw_animal) = (90, [1000; 11], 1000);
    }
}

#[test]
fn a_hungry_guest_walks_to_the_stand_buys_a_hot_dog_and_pays_when_it_leaves() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    hungry(&mut w);
    assert!(with_cx(&cat, &mut w, |cx| {
        cx.seek("food");
        cx.record().task == GO
    }));
    // The entrance nearer to the guest is the left one: row 6, column 5 at x 336.
    assert_eq!(guest(&w).entrance, (336, 400));
    let log = run(&mut s, &cat, &mut w, &mut money, 600, "buy");
    assert_eq!(log, ["use 7 1", "buy 7 hotdog 2"]);
    // The hot dog is held, not eaten; the stand books nothing before the guest leaves.
    assert_eq!(guest(&w).items[0], Some(6303));
    assert_eq!(money.total[money::CONCESSIONS], 0.0);
    run(&mut s, &cat, &mut w, &mut money, 600, "never");
    // 3 x 2.00 for the user who stayed 3 AI ticks; the guest then eats it standing.
    assert_eq!(money.total[money::CONCESSIONS], 6.0);
    let live = &w.units.buildings[&7];
    assert_eq!(
        (live.users.len(), live.guests, live.income),
        (0, [1, 0, 1], [6.0, 0.0, 6.0])
    );
    let g = guest(&w);
    assert!(g.items[0].is_none() && g.items[4] == Some(crate::building::items::LITTER));
    assert!(g.pending[HUNGER] + g.values[HUNGER] < 90, "{g:?}");
}

#[test]
fn an_expensive_stand_turns_the_guest_away_and_a_cheap_one_makes_it_happy() {
    let (cat, mut money) = (types(), Money::default());
    // The default price is 2.00: 4.00 and more is expensive, 1.00 and less cheap.
    for (price, change, buys) in [(4.0, -5, false), (3.0, 0, true), (1.0, 5, true)] {
        let mut w = zoo();
        hungry(&mut w);
        let live = crate::building::Live {
            price,
            ..Default::default()
        };
        w.units.buildings.insert(7, live);
        with_cx(&cat, &mut w, |cx| cx.seek("food"));
        let log = run(&mut game(), &cat, &mut w, &mut money, 600, "buy");
        let g = guest(&w);
        assert_eq!(log.iter().any(|l| l.starts_with("buy")), buys, "{log:?}");
        let happiness = g.values[HAPPINESS] + g.pending[HAPPINESS];
        match change {
            -5 => assert!(happiness <= -5, "{price}"),
            c => assert_eq!(happiness, c, "{price}"),
        }
        assert_eq!(log.iter().any(|l| l.starts_with("use")), buys);
        // The price is also a thought: 10073 too high, 10072 really low.
        let want = match change {
            -5 => Some(10073),
            5 => Some(10072),
            _ => None,
        };
        assert!(want.is_none_or(|t| thoughts(&w).contains(&t)), "{price}");
    }
}

#[test]
fn a_restroom_hides_its_user_for_the_time_inside_and_takes_the_need_away() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.values[BATHROOM], g.countdowns, g.saw_animal) = (80, [1000; 11], 1000);
    }
    assert!(with_cx(&cat, &mut w, |cx| {
        cx.seek("bathroom");
        cx.guest().unwrap().building == 8
    }));
    run(&mut s, &cat, &mut w, &mut money, 800, "use");
    assert!(w.units.get(1).unwrap().hidden && w.units.buildings[&8].users.len() == 1);
    run(&mut s, &cat, &mut w, &mut money, 400, "never");
    assert!(!w.units.get(1).unwrap().hidden && w.units.buildings[&8].users.is_empty());
    // Nothing is booked for a restroom; the bathroom need is gone.
    assert_eq!(money.total[money::CONCESSIONS], 0.0);
    let g = guest(&w);
    assert!(g.values[BATHROOM] + g.pending[BATHROOM] <= 20, "{g:?}");
}

/// Steps one game step at a time until `stop` holds, at most `n` times.
fn until(
    s: &mut Sim,
    cat: &Catalogue,
    w: &mut World,
    money: &mut Money,
    n: usize,
    stop: impl Fn(&World) -> bool,
) {
    for _ in 0..n {
        if stop(w) {
            return;
        }
        run(s, cat, w, money, 1, "never");
    }
    assert!(stop(w), "not reached in {n} steps");
}

fn task(w: &World) -> u8 {
    w.units.get(1).unwrap().goal.record.task
}

#[test]
fn a_user_walks_out_to_an_entrance_of_its_slot_and_is_released_there() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.values[BATHROOM], g.countdowns, g.saw_animal) = (80, [1000; 11], 1000);
    }
    with_cx(&cat, &mut w, |cx| cx.seek("bathroom"));
    let bathroom = |w: &World| guest(w).values[BATHROOM] + guest(w).pending[BATHROOM];
    let target = |w: &World| w.units.get(1).unwrap().goal.record.target.tile;
    until(&mut s, &cat, &mut w, &mut money, 2000, |w| {
        task(w) == EXIT_BUILDING
    });
    until(&mut s, &cat, &mut w, &mut money, 5, |w| target(w).is_some());
    // The user is still hidden and listed on its way to the entrance tile (5, 9); the change of
    // the type is not given yet.
    let before = bathroom(&w);
    assert_eq!(target(&w), Some(9 * 10 + 5));
    assert!(w.units.get(1).unwrap().hidden && w.units.buildings[&8].users.len() == 1);
    until(&mut s, &cat, &mut w, &mut money, 400, |w| {
        let (listed, hidden) = (
            !w.units.buildings[&8].users.is_empty(),
            w.units.get(1).unwrap().hidden,
        );
        assert_eq!(listed, hidden);
        !listed
    });
    let at = w.find(1).unwrap();
    assert!(bathroom(&w) <= before - 99);
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), (5, 9));
}

#[test]
fn a_user_who_stays_outside_completes_the_walk_out_on_its_own_tile() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    hungry(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    let tile = |w: &World| {
        let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
        (c.x.div_euclid(64), c.y.div_euclid(64))
    };
    until(&mut s, &cat, &mut w, &mut money, 2000, |w| {
        task(w) == EXIT_BUILDING
    });
    let at = tile(&w);
    until(&mut s, &cat, &mut w, &mut money, 20, |w| {
        w.units.buildings[&7].users.is_empty()
    });
    assert_eq!(tile(&w), at);
}

#[test]
fn the_nearest_entrance_is_taken_unless_the_guest_cannot_reach_it() {
    let (cat, mut w) = (types(), zoo());
    // A pen round the left entrance tile (5, 6) cuts it off; the right one is the same tile, so
    // the pen goes round the guest's side: fences on the guest's side of column 5.
    let at = w.find(1).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().x = 5 * 64 + 32;
    let open = with_cx(&cat, &mut w, |cx| cx.open_slot(7));
    assert_eq!(open, Some((0, (336, 400))));
    // Two entrances at different distances are sorted: the guest at x = 336 is nearer the left.
    let (e, kind) = with_cx(&cat, &mut w, |cx| cx.building(7).unwrap());
    let list = building::entrances(&w, &e, &kind, None, (100, 400));
    assert_eq!(
        list.iter().map(|l| l.2).collect::<Vec<_>>(),
        [(336, 400), (368, 400)]
    );
    // A full stand has no entrance.
    let full = Live {
        users: vec![(2, 0, 0), (3, 0, 0)],
        ..Live::new(&kind)
    };
    assert!(building::entrances(&w, &e, &kind, Some(&full), (0, 0)).is_empty());
}

/// The guest holds a hot dog and its checks are far from firing.
fn holding(w: &mut World) {
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.items[0], g.countdowns, g.saw_animal) = (Some(6303), [1000; 11], 1000);
    }
}

#[test]
fn a_guest_with_a_hot_dog_eats_it_at_a_table_which_puts_it_down_at_its_exit() {
    let (cat, mut w, mut money) = (types(), zoo_with(true), Money::default());
    let mut s = game();
    holding(&mut w);
    let log = run(&mut s, &cat, &mut w, &mut money, 1500, "use");
    assert_eq!(log, ["use 9 1"]);
    // The table sells nothing: the guest eats the hot dog it holds, in its seat, in view.
    let (u, at) = (w.units.get(1).unwrap(), w.find(1).unwrap());
    assert!(!u.hidden && guest(&w).items[0].is_none());
    assert_eq!(guest(&w).items[4], Some(crate::building::items::LITTER));
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    // Facing 7 at the table's rotation 4 turns to 3 (guest buildings 3.2).
    assert_eq!((c.x, c.y, c.rotation), (336, 240, 3));
    for _ in 0..400 {
        run(&mut s, &cat, &mut w, &mut money, 1, "never");
        if w.units.buildings[&9].users.is_empty() {
            break;
        }
    }
    // It leaves by the exit, below the table.
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), (5, 4));
    assert!(w.units.buildings[&9].users.is_empty());
    assert_eq!(money.total[money::CONCESSIONS], 0.0);
}

#[test]
fn a_guest_with_a_hot_dog_and_no_table_eats_it_standing() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    holding(&mut w);
    run(&mut s, &cat, &mut w, &mut money, 200, "never");
    let g = guest(&w);
    assert!(
        g.items[0].is_none() && !g.stand && g.items[4].is_some(),
        "{g:?}"
    );
    assert!(g.values[HUNGER] + g.pending[HUNGER] <= 0, "{g:?}");
    assert!(w.units.buildings.is_empty());
}

#[test]
fn a_removed_building_loses_its_state_and_a_removed_guest_its_place_among_the_users() {
    let mut w = zoo();
    let live = |users| Live {
        users,
        ..Default::default()
    };
    w.units
        .buildings
        .insert(7, live(vec![(1, 0, 0), (5, 0, 0)]));
    w.units.buildings.insert(8, live(vec![(1, 0, 0)]));
    assert!(w.remove(8));
    assert_eq!(w.units.buildings.keys().collect::<Vec<_>>(), [&7]);
    // Unit 5 is no unit of the table.
    assert!(w.remove(1));
    assert!(w.units.buildings[&7].users.is_empty());
}

/// Guest 1 at (2, 6), who holds litter, and a trash can 11 at (5, 6), whose entrance is on row 7.
fn littered(can: bool) -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 6)), 0));
        if can {
            zoo.entities
                .push(entity("building", "can", 11, centre((5, 6)), 4));
        }
    });
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.items[4], g.countdowns, g.saw_animal) = (Some(LITTER), [1000; 11], 1000);
    }
    w
}

fn give_litter(w: &mut World) {
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        g.items[4] = Some(LITTER);
    }
}

/// The pieces of litter on the ground.
fn pieces(w: &World) -> usize {
    (w.zoo.entities.iter())
        .filter(|e| (e.class.as_str(), e.subtype.as_str()) == ("objects", "trash"))
        .count()
}

fn thoughts(w: &World) -> Vec<i32> {
    w.units.thoughts.of(1).iter().map(|t| t.text).collect()
}

#[test]
fn a_guest_with_litter_walks_to_a_trash_can_and_the_can_counts_the_piece() {
    let (cat, mut w, mut money) = (types(), littered(true), Money::default());
    assert!(with_cx(&cat, &mut w, |cx| {
        cx.seek_trash();
        cx.record().task == GO && cx.guest().unwrap().building == 11
    }));
    assert_eq!(guest(&w).need, Some("trash"));
    let log = run(&mut game(), &cat, &mut w, &mut money, 800, "use");
    assert_eq!(log, ["use 11 1"]);
    assert_eq!(w.units.buildings[&11].fill, 1);
    assert!(guest(&w).items[4].is_none() && pieces(&w) == 0);
    // Newest first: the can's thought, then the guest's own about its litter.
    assert_eq!(thoughts(&w), [10215, 10237]);
}

#[test]
fn a_guest_with_litter_and_no_can_drops_it_where_it_stands_and_calls_a_worker() {
    let (cat, mut w) = (types(), littered(false));
    with_cx(&cat, &mut w, |cx| cx.seek_trash());
    assert_eq!(pieces(&w), 1);
    let g = guest(&w);
    assert!(g.items[4].is_none() && !g.littered);
    assert!(w.units.get(1).unwrap().goal.wandered);
    let e = &w.units.posts[..];
    assert_eq!(
        (e.len(), e[0].kind, e[0].channel, e[0].data, e[0].sender),
        (1, 10031, 0x4d, 6 * 10 + 2, 1)
    );
    assert_eq!(
        w.units.take_guest_lines(),
        ["thought 1 10237", "litter 1 2,6"]
    );
    // Where it stands, in the quarter its position falls in: that quarter is taken now, so a
    // second piece fails and the guest keeps it.
    give_litter(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek_trash());
    assert_eq!(pieces(&w), 1);
    assert!(guest(&w).items[4].is_some() && guest(&w).littered);
}

#[test]
fn a_full_can_sends_the_litter_to_the_ground_and_the_thirteenth_piece_calls_a_worker() {
    let (cat, mut w) = (types(), littered(true));
    let fill = |w: &mut World, n| w.units.buildings.entry(11).or_default().fill = n;
    // 12 pieces: the guest's makes 13 and posts event 10030 with the can's tile.
    fill(&mut w, 12);
    with_cx(&cat, &mut w, |cx| cx.bin_litter(11, true));
    assert_eq!(w.units.buildings[&11].fill, 13);
    let e = &w.units.posts[..];
    assert_eq!(
        (e.len(), e[0].kind, e[0].data, e[0].sender),
        (1, 10030, 65, 11)
    );
    assert!(guest(&w).items[4].is_none());
    // 24 pieces take one more; above that the can is full.
    for (before, after, ground) in [(24, 25, 0), (25, 25, 1)] {
        fill(&mut w, before);
        give_litter(&mut w);
        with_cx(&cat, &mut w, |cx| cx.bin_litter(11, true));
        assert_eq!((w.units.buildings[&11].fill, pieces(&w)), (after, ground));
    }
    // A building that removes nothing takes nothing.
    give_litter(&mut w);
    with_cx(&cat, &mut w, |cx| cx.bin_litter(11, false));
    assert!(guest(&w).items[4].is_some());
}

#[test]
fn a_guest_thinks_what_it_wants_and_what_it_finds_or_misses() {
    let (cat, mut w) = (types(), zoo());
    hungry(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    assert_eq!(thoughts(&w), [10079]);
    // No bench: it wants rest, finds none, and the goal is reset with the wandered flag up.
    with_cx(&cat, &mut w, |cx| {
        cx.clear_target();
        cx.seek("energy");
    });
    assert_eq!(thoughts(&w), [10093, 10092, 10079]);
    assert!(w.units.get(1).unwrap().goal.wandered);
    // A stand that has no room is too crowded, a thought about the building.
    let mut w = zoo();
    w.units.buildings.insert(
        7,
        Live {
            users: vec![(2, 0, 0), (3, 0, 0)],
            ..Default::default()
        },
    );
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    let seen = w.units.thoughts.of(1);
    assert_eq!((seen[0].text, seen[0].object), (10243, Some(7)));
    // Nothing satisfies the gift need: wanting and not finding.
    with_cx(&cat, &mut w, |cx| cx.seek("gift"));
    assert_eq!(thoughts(&w)[..2], [10091, 10090]);
}

#[test]
fn the_second_trash_search_draws_among_every_can_in_reach() {
    // Can 11 at 2 tiles holds 24 pieces and is the nearest; can 12 at 5 tiles has room. The first
    // search takes 11 (one tie draw); the second draws among both, and its draw is odd.
    let (cat, mut w) = (types(), littered(true));
    let at = w.find(11).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().x = centre((4, 6)).0;
    w.zoo
        .entities
        .push(entity("building", "can", 12, centre((7, 6)), 4));
    w.units.buildings.entry(11).or_default().fill = 24;
    with_cx(&cat, &mut w, |cx| cx.seek_trash());
    assert_eq!(guest(&w).building, 12);
    assert!(guest(&w).items[4].is_some() && pieces(&w) == 0);
}

#[test]
fn a_dropped_piece_of_litter_faces_a_drawn_direction() {
    let (cat, mut w) = (types(), littered(false));
    let (mut rng, mut seen) = (Rng::default(), Rng::default());
    let facing = seen.rand() & 7;
    assert_eq!(
        crate::guest::litter::drop(&mut w, &cat, 0, 1, &mut rng),
        Some(true)
    );
    let piece = w.zoo.entities.iter().find(|e| e.subtype == "trash");
    assert_eq!(piece.unwrap().common.as_ref().unwrap().rotation, facing);
    assert_eq!(rng, seen);
    // In an exhibit nothing happens: no draw, no piece, and the flag keeps its value.
    let mut pen = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 6)), 0));
        for i in 0..3 {
            for (at, d) in [
                ((1 + i, 5), 0),
                ((1 + i, 7), 4),
                ((1, 5 + i), 6),
                ((3, 5 + i), 2),
            ] {
                let id = 100 + zoo.entities.len() as u32;
                zoo.entities.push(entity("fences", "", id, centre(at), d));
            }
        }
    });
    assert_eq!(pen.exhibits().list().len(), 1);
    let before = rng.clone();
    assert_eq!(
        crate::guest::litter::drop(&mut pen, &cat, 0, 1, &mut rng),
        None
    );
    assert!(rng == before && pieces(&pen) == 0);
}

#[test]
fn a_price_thought_is_about_the_building() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    hungry(&mut w);
    let live = crate::building::Live {
        price: 4.0,
        ..Default::default()
    };
    w.units.buildings.insert(7, live);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    run(&mut game(), &cat, &mut w, &mut money, 600, "buy");
    let about = |t: &&crate::guest::thought::Thought| t.text == 10073;
    let found = w.units.thoughts.of(1).into_iter().find(about);
    assert_eq!(found.map(|t| t.object), Some(Some(7)));
}

#[test]
fn a_removed_building_takes_the_thoughts_about_it() {
    let mut w = zoo();
    w.units.think(1, 10243, Some(7), None);
    w.units.think(1, 10243, Some(8), None);
    w.units.think(1, 10237, None, None);
    assert!(w.remove(7));
    let all = w.units.thoughts.of(1);
    assert_eq!(
        all.iter().map(|t| t.object).collect::<Vec<_>>(),
        [None, Some(8)]
    );
}
