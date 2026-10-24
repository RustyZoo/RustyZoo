//! A guest uses a hot dog stand and a restroom on a synthetic map.

use super::*;
use crate::building::Prices;
use crate::entity::{Catalogue, EntityType};
use crate::goal::guest::tests::walks;
use crate::guest::{BATHROOM, HAPPINESS, HUNGER, THIRST};
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
const BOOTH: &str = "[Characteristics/Integers]\ncCapacity=1\ncTimeInside=2\ncHideUser=1\n\
    cUserStaysOutside=0\ncUserUsesExit=1\n[Satisfies]\nbuilding\neat\n[slots]\nname=slot\n\
    [slot]\nslotpos=-16\nslotpos=16\nentrpos=-16\nentrpos=48\nexitpos=-16\nexitpos=48\n\
    capacity=1\nfacing=7\n";
const CAN: &str = "[Characteristics/Integers]\ncCapacity=1000\ncUserStaysOutside=1\n\
    cUsedThought=10215\n[Removes]\ntrash\n[Satisfies]\nbuilding\ntrash\n[slots]\nname=slot\n\
    [slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1000\n";
/// A drink kiosk whose user stays outside: its slot is on its own tile, its entrance one tile
/// below.
const KIOSK: &str = "[Characteristics/Integers]\ncCapacity=2\ncTimeInside=3\n[Satisfies]\n\
    building\ndrink\n[slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\n\
    exitpos=0\nexitpos=48\ncapacity=2\n";
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
    cat.push(ty("building", "booth", BOOTH));
    cat.push(ty("building", "can", CAN));
    cat.push(ty("building", "kiosk", KIOSK));
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
    zoo_of(table.then_some("table"))
}

/// The same, with a building 9 of type `name` at (5, 3) when given.
fn zoo_of(name: Option<&str>) -> World {
    world_with((10, 10), |zoo| {
        if let Some(name) = name {
            zoo.entities
                .push(entity("building", name, 9, centre((5, 3)), 4));
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
    game_at(0)
}

/// The same, with the AI clock at `ticks` as a saved zoo restores it.
fn game_at(ticks: u32) -> Sim {
    let mut s = Sim::new(b"");
    s.set_seed(1);
    s.start_game(
        100,
        crate::zoo::Clocks {
            ai_ticks: ticks,
            ..Default::default()
        },
    );
    s.status.settings.prices = Prices {
        expensive: 2.0,
        cheap: 0.5,
        default: 3.0,
        ..Prices::default()
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
        // Walking hidden, it stays on the list of the tile it is on and is not painted.
        let at = w.find(1).unwrap();
        let c = w.zoo.entities[at].common.as_ref().unwrap();
        let t = w
            .tile_index(c.x.div_euclid(64), c.y.div_euclid(64))
            .unwrap();
        assert_eq!(hidden, w.units_on(t).any(|e| e == at) && !w.is_drawn(at));
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
    w.place_unit(at, (336, 240, 0), 6);
    for _ in 0..400 {
        run(&mut s, &cat, &mut w, &mut money, 1, "never");
        if w.units.buildings[&9].users.is_empty() {
            break;
        }
    }
    // It leaves by the exit, below the table, facing as it sat (turned to 6 above).
    let c = w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!(
        (c.x.div_euclid(64), c.y.div_euclid(64), c.rotation),
        (5, 4, 6)
    );
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

/// Deletes building `id` with the bulldozer.
fn delete(w: &mut World, cat: &Catalogue, id: u32) {
    let mut money = Money::default();
    let removed = crate::place::bulldozer::remove(w, &mut money, cat, id, &mut Rng::default());
    assert!(removed.is_some());
}

fn spot(w: &World) -> (i32, i32, u32) {
    let c = w.zoo.entities[w.find(1).unwrap()].common.as_ref().unwrap();
    (c.x, c.y, c.rotation)
}

#[test]
fn a_deleted_stand_pushes_its_customer_out_to_an_entrance_after_it_has_been_paid() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    hungry(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    let log = run(&mut s, &cat, &mut w, &mut money, 600, "buy");
    assert_eq!(log, ["use 7 1", "buy 7 hotdog 2"]);
    assert_eq!(task(&w), crate::goal::USE);
    w.units.concessions.clear();
    delete(&mut w, &cat, 7);
    // 3 x 2.00, booked at once; the guest is at the first entrance, with no goal and no target.
    assert_eq!(w.units.concessions, [6.0]);
    assert_eq!(w.units.take_building_lines(), ["push 7 1"]);
    assert_eq!((spot(&w).0, spot(&w).1), (336, 400));
    assert_eq!((task(&w), guest(&w).building, guest(&w).need), (0, 0, None));
    // The building and its state are gone with it.
    assert!(w.find(7).is_none() && !w.units.buildings.contains_key(&7));
    // The guest goes on with its life.
    run(&mut s, &cat, &mut w, &mut money, 50, "never");
}

#[test]
fn a_deleted_restroom_shows_its_hidden_user_at_the_entrance_with_the_change_record() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.values[BATHROOM], g.countdowns, g.saw_animal) = (80, [1000; 11], 1000);
    }
    with_cx(&cat, &mut w, |cx| cx.seek("bathroom"));
    run(&mut s, &cat, &mut w, &mut money, 800, "use");
    assert!(w.units.get(1).unwrap().hidden);
    let before = guest(&w).pending[BATHROOM];
    delete(&mut w, &cat, 8);
    assert!(!w.units.get(1).unwrap().hidden);
    assert_eq!(guest(&w).pending[BATHROOM], before - 100);
    // Nothing is booked for a restroom; the guest stands at the entrance (0, 48) of the slot.
    assert!(w.units.concessions.is_empty());
    assert_eq!((spot(&w).0, spot(&w).1), (352, 592));
    assert!(w.units.get(1).unwrap().goal.record.task == 0);
}

#[test]
fn a_deleted_table_puts_its_visible_user_at_the_exit_with_its_own_facing() {
    let (cat, mut w, mut money) = (types(), zoo_with(true), Money::default());
    holding(&mut w);
    run(&mut game(), &cat, &mut w, &mut money, 1500, "use");
    assert_eq!(spot(&w), (336, 240, 3));
    // Turned in its seat: the exit keeps the facing, the slot's is not given again.
    let at = w.find(1).unwrap();
    w.place_unit(at, (336, 240, 0), 6);
    delete(&mut w, &cat, 9);
    assert_eq!(spot(&w), (336, 272, 6));
    assert_eq!(task(&w), 0);
}

#[test]
fn a_deleted_trash_can_pushes_out_its_user_and_spills_its_pieces() {
    let (cat, mut w, mut money) = (types(), littered(true), Money::default());
    with_cx(&cat, &mut w, |cx| cx.seek_trash());
    run(&mut game(), &cat, &mut w, &mut money, 800, "use");
    assert_eq!(w.units.buildings[&11].fill, 1);
    delete(&mut w, &cat, 11);
    assert_eq!((pieces(&w), task(&w)), (1, 0));
    // The can stands at (5, 6); the user was outside at its entrance and is put at the first.
    assert_eq!((spot(&w).0, spot(&w).1), (352, 464));
}

#[test]
fn a_user_of_a_building_with_no_exit_on_the_map_is_taken_off_it_and_stays_hidden() {
    let (cat, mut w) = (types(), zoo());
    // The stand at the last row has its entrances below the map: nobody could enter it.
    w.units.buildings.insert(
        7,
        Live {
            users: vec![(1, 0, 0)],
            ..Default::default()
        },
    );
    let at = w.find(7).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().y = 9 * 64 + 32;
    w.units.get_mut(1).unwrap().hidden = true;
    let (own, at) = (w.tile_index(2, 6).unwrap(), w.find(1).unwrap());
    assert!(w.units_on(own).any(|e| e == at));
    crate::building::leave::push_out(&mut w, &cat, 7);
    assert_eq!(w.units.take_building_lines(), ["push 7 1"]);
    assert!(w.units.get(1).unwrap().hidden && !w.units_on(own).any(|e| e == at));
    assert_eq!(task(&w), 0);
}

/// Moves building `id` to the last row, where the exit of its slot is off the map.
fn lose_exit(w: &mut World, id: u32) {
    let at = w.find(id).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().y = 9 * 64 + 32;
}

#[test]
fn a_user_with_no_exit_is_only_off_its_tile_list_and_is_shown_when_the_leave_goal_completes() {
    let (cat, mut w, mut money) = (types(), zoo_of(Some("booth")), Money::default());
    let mut s = game();
    holding(&mut w);
    run(&mut s, &cat, &mut w, &mut money, 1500, "use");
    let (at, before) = (w.find(1).unwrap(), spot(&w));
    let own = w.tile_index(before.0 / 64, before.1 / 64).unwrap();
    // Hidden, not painted, and still on its tile's list.
    assert!(w.units.get(1).unwrap().hidden && w.units_on(own).any(|e| e == at));
    assert!(!w.is_drawn(at));
    lose_exit(&mut w, 9);
    for _ in 0..3000 {
        s.step_game(&walks(), &cat, &mut w, &mut money, 67, |_, _, _| {});
        if w.units.buildings[&9].users.is_empty() {
            break;
        }
    }
    // Where it was, shown, its goal gone, in the entity list and on no tile list.
    assert!(w.units.buildings[&9].users.is_empty());
    assert_eq!(spot(&w), before);
    assert!(!w.units.get(1).unwrap().hidden && w.is_drawn(at));
    assert_eq!(task(&w), 0);
    assert!(!w.units_on(own).any(|e| e == at));
    // It joins a list again at its first change of tile (it holds nothing to drop, which would
    // edit the map).
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        g.items = Default::default();
    }
    for _ in 0..600 {
        s.step_game(&walks(), &cat, &mut w, &mut money, 67, |_, _, _| {});
        let (x, y, _) = spot(&w);
        if (x / 64, y / 64) != (before.0 / 64, before.1 / 64) {
            let t = w.tile_index(x / 64, y / 64).unwrap();
            assert!(w.units_on(t).any(|e| e == at));
            return;
        }
        assert!(!w.units_on(own).any(|e| e == at));
    }
    panic!("the guest never left its tile");
}

#[test]
fn a_visible_user_with_no_exit_stays_drawn_where_it_is_off_its_tile_list() {
    let (cat, mut w, mut money) = (types(), zoo_with(true), Money::default());
    let mut s = game();
    holding(&mut w);
    run(&mut s, &cat, &mut w, &mut money, 1500, "use");
    let (at, before) = (w.find(1).unwrap(), spot(&w));
    let own = w.tile_index(before.0 / 64, before.1 / 64).unwrap();
    lose_exit(&mut w, 9);
    crate::building::leave::push_out(&mut w, &cat, 9);
    assert_eq!(spot(&w), before);
    assert!(w.is_drawn(at) && !w.units_on(own).any(|e| e == at));
    assert_eq!(task(&w), 0);
}

#[test]
fn a_user_with_no_exit_stays_off_its_tile_list_across_a_map_edit_until_it_changes_tile() {
    let (cat, mut w, mut money) = (types(), zoo_with(true), Money::default());
    let mut s = game();
    holding(&mut w);
    run(&mut s, &cat, &mut w, &mut money, 1500, "use");
    let (at, before) = (w.find(1).unwrap(), spot(&w));
    let own = w.tile_index(before.0 / 64, before.1 / 64).unwrap();
    lose_exit(&mut w, 9);
    crate::building::leave::push_out(&mut w, &cat, 9);
    assert!(w.is_drawn(at) && !w.units_on(own).any(|e| e == at));
    // A map edit rebuilds the index from the entity list; the unit stays off its list.
    assert!(w.set_tile(0, 0, 1, 0, 0));
    assert!(w.is_drawn(at) && !w.units_on(own).any(|e| e == at));
    // Moving inside its tile does not list it; entering another tile does.
    w.place_unit(at, (before.0 + 1, before.1, 0), 0);
    assert!(!w.units_on(own).any(|e| e == at));
    let next = w.tile_index(before.0 / 64 + 1, before.1 / 64).unwrap();
    w.place_unit(at, (before.0 + 64, before.1, 0), 0);
    assert!(w.units_on(next).any(|e| e == at) && !w.units_on(own).any(|e| e == at));
}

#[test]
fn a_carried_unit_stays_off_the_map_across_a_map_edit() {
    let (cat, mut w) = (types(), zoo());
    let (at, own) = (w.find(1).unwrap(), w.tile_index(2, 6).unwrap());
    crate::place::unit::pick_up(&mut w, &cat, 1, 0).unwrap();
    assert!(!w.is_drawn(at) && !w.units_on(own).any(|e| e == at));
    // A map edit rebuilds the index from the entity list; the carried unit stays off the map.
    assert!(w.set_tile(0, 0, 1, 0, 0));
    assert!(!w.is_drawn(at) && !w.units_on(own).any(|e| e == at));
    // Put down, it is painted and listed again.
    assert!(crate::place::unit::put_down(
        &mut w,
        1,
        (2 * 64 + 32, 6 * 64 + 32, 0),
        0
    ));
    assert!(w.is_drawn(at) && w.units_on(own).any(|e| e == at));
}

#[test]
fn a_hidden_user_stays_hidden_across_a_map_edit() {
    let (cat, mut w, mut money) = (types(), zoo_of(Some("booth")), Money::default());
    let mut s = game();
    holding(&mut w);
    run(&mut s, &cat, &mut w, &mut money, 1500, "use");
    let (at, before) = (w.find(1).unwrap(), spot(&w));
    let own = w.tile_index(before.0 / 64, before.1 / 64).unwrap();
    assert!(w.units.get(1).unwrap().hidden && !w.is_drawn(at));
    // A map edit rebuilds the index; the hidden user is not painted again and stays on its list.
    assert!(w.set_tile(0, 0, 1, 0, 0));
    assert!(!w.is_drawn(at) && w.units_on(own).any(|e| e == at));
    // It is shown again only when its goal completes, at the exit.
    let mut shown = false;
    for _ in 0..3000 {
        s.step_game(&walks(), &cat, &mut w, &mut money, 67, |_, _, _| {});
        if w.units.get(1).unwrap().hidden {
            assert!(!w.is_drawn(at), "drawn while hidden");
        } else {
            shown = true;
            break;
        }
    }
    assert!(shown && w.is_drawn(at));
}

/// Unit 1 of class `class` at `(x, y)` (first, as the context wants it), a stand 7 at (5, 5) and a
/// hall 9 of a whole tile at `hall`.
fn footprint(class: &str, (x, y): (i32, i32), hall: (i32, i32)) -> World {
    world_with((10, 10), |zoo| {
        zoo.entities.push(entity(class, "man", 1, (x, y), 0));
        zoo.entities
            .push(entity("building", "stand", 7, centre((5, 5)), 4));
        zoo.entities.push(entity("building", "hall", 9, hall, 4));
    })
}

/// The catalogue with the hall as a type of the config `text`.
fn with_hall(text: &str) -> Catalogue {
    let mut cat = types();
    cat.push(ty("building", "hall", text));
    cat
}

/// Moves unit 1 to `(x, y, z)`.
fn put(w: &mut World, (x, y, z): (i32, i32, i32)) {
    let at = w.find(1).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y, c.z) = (x, y, z);
}

/// The common part with a draw of 0 (the 1 in 15); the task after it.
fn common_hit(cx: &mut Cx) -> u8 {
    while cx.ctx.rng.clone().pick(15) != 0 {
        cx.ctx.rng.rand();
    }
    cx.common();
    cx.record().task
}

#[test]
fn one_draw_in_fifteen_sends_a_guest_on_the_footprint_of_a_building_out_of_it() {
    let (cat, mut w) = (types(), footprint("guests", centre((2, 6)), centre((5, 3))));
    let mut task_at = |tile: (i32, i32)| {
        let (x, y) = centre(tile);
        put(&mut w, (x, y, 0));
        if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
            g.slot = 1;
        }
        with_cx(&cat, &mut w, common_hit)
    };
    // Off any building, and on the one-quarter stand, nothing changes; on the hall, a whole
    // tile, the guest leaves it, with the hall as its building and no slot.
    assert_eq!(task_at((2, 6)), 0);
    assert_eq!(task_at((5, 5)), 0);
    assert_eq!(task_at((5, 3)), EXIT_BUILDING);
    assert_eq!((guest(&w).building, guest(&w).slot), (9, 0));
    let r = &w.units.get(1).unwrap().goal.record;
    assert_eq!((r.mode, r.target.id, r.target.tile), (1, 9, None));
}

#[test]
fn only_the_building_on_quarter_3_of_the_tile_sends_a_unit_out() {
    // The hall straddles four tiles: quarter 3 of (5, 3), quarter 2 of (6, 3).
    let cat = types();
    let mut w = footprint("guests", centre((2, 6)), (6 * 64, 4 * 64));
    // In quarter 0 of (5, 3), which is free, the unit is still sent out.
    put(&mut w, (5 * 64 + 10, 3 * 64 + 10, 0));
    assert_eq!(with_cx(&cat, &mut w, common_hit), EXIT_BUILDING);
    // On (6, 3), held by the hall in another quarter, nothing.
    with_cx(&cat, &mut w, |cx| cx.reset());
    put(&mut w, (6 * 64 + 48, 3 * 64 + 16, 0));
    assert_eq!(with_cx(&cat, &mut w, common_hit), 0);
}

#[test]
fn the_height_of_a_unit_on_a_building_counts_only_on_a_tank_tile() {
    // Ten levels above the hall, out of a tank: still on its footprint.
    let (cat, mut w) = (types(), footprint("guests", centre((2, 6)), centre((5, 3))));
    let (x, y) = centre((5, 3));
    put(&mut w, (x, y, 160));
    assert_eq!(with_cx(&cat, &mut w, common_hit), EXIT_BUILDING);
}

#[test]
fn a_unit_of_any_kind_is_sent_out_unless_it_moves_inside_a_tank() {
    let cat = with_hall(KIOSK);
    for class in ["animals", "keeper"] {
        let mut w = footprint(class, centre((5, 3)), centre((5, 3)));
        let at = w.find(1).unwrap();
        w.zoo.entities[at].common.as_mut().unwrap().movement_mode = 3;
        assert_eq!(with_cx(&cat, &mut w, common_hit), 0, "{class}");
        w.zoo.entities[at].common.as_mut().unwrap().movement_mode = 2;
        assert_eq!(with_cx(&cat, &mut w, common_hit), EXIT_BUILDING, "{class}");
        // The hall keeps its users outside: the goal is done where the unit stands at the next
        // update, and the unit has no task.
        let task = with_cx(&cat, &mut w, |cx| {
            cx.run_goal();
            assert_eq!(cx.record().target.tile, w_tile((5, 3)), "{class}");
            cx.run_goal();
            cx.record().task
        });
        assert_eq!(task, 0, "{class}");
    }
}

/// The index of tile `(x, y)` on the maps of these tests.
fn w_tile((x, y): (i32, i32)) -> Option<usize> {
    Some((y * 10 + x) as usize)
}

#[test]
fn the_draws_of_a_send_out_follow_the_goal_updates() {
    for (text, first) in [(KIOSK, 2), (RESTROOM, 3)] {
        let cat = with_hall(text);
        let mut w = footprint("guests", centre((5, 3)), centre((5, 3)));
        let (moved, task) = with_cx(&cat, &mut w, |cx| {
            let draws = |cx: &mut Cx, n: usize, f: fn(&mut Cx)| {
                let mut seen = cx.ctx.rng.clone();
                (0..n).for_each(|_| _ = seen.rand());
                f(cx);
                assert_eq!(*cx.ctx.rng, seen);
            };
            while cx.ctx.rng.clone().pick(15) != 0 {
                cx.ctx.rng.rand();
            }
            // Update n: the common part, the check done and, for a type whose users leave by an
            // entrance, the draw of the entrance.
            draws(cx, first, |cx| {
                cx.common();
                cx.run_goal();
            });
            assert_eq!(cx.record().task, EXIT_BUILDING);
            // Update n + 1: the common part and the check done.
            draws(cx, 2, |cx| {
                cx.common();
                cx.run_goal();
            });
            (cx.unit.through_building, cx.record().task)
        });
        match text {
            // Done on its own tile: no task, the wandered flag up.
            KIOSK => {
                assert_eq!((moved, task), (false, 0));
                assert!(w.units.get(1).unwrap().goal.wandered);
            }
            // On its way to the entrance of slot 0, (5, 4), with the building flag.
            _ => {
                assert_eq!((moved, task), (true, EXIT_BUILDING));
                let r = &w.units.get(1).unwrap().goal.record;
                assert_eq!(r.target.tile, w_tile((5, 4)));
            }
        }
        // The hall is not touched: no user, no money.
        assert!(!w.units.buildings.contains_key(&9));
    }
}

#[test]
fn a_refused_walk_out_decides_again_with_a_new_draw_at_the_next_update() {
    let cat = types();
    // The guest is penned in on (2, 6) and leaves the restroom 8, whose entrance is on (5, 9).
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 6)), 0));
        zoo.entities
            .push(entity("building", "restroom", 8, centre((5, 8)), 4));
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
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 8;
        cx.set_task(EXIT_BUILDING, 1, 8, None);
        for _ in 0..2 {
            let mut seen = cx.ctx.rng.clone();
            seen.rand();
            cx.exit_decide();
            // No target and the flag still up; one draw each time.
            assert_eq!(*cx.ctx.rng, seen);
            assert_eq!(cx.record().target.tile, None);
            assert!(cx.unit.through_building);
        }
    });
}

#[test]
fn a_user_who_stays_outside_is_served_at_the_entrance_and_never_steps_on_the_footprint() {
    let (cat, mut money) = (types(), Money::default());
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((2, 6)), 0));
        zoo.entities
            .push(entity("building", "kiosk", 9, centre((5, 3)), 4));
    });
    let mut s = game();
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.values[THIRST], g.countdowns, g.saw_animal) = (90, [1000; 11], 1000);
    }
    with_cx(&cat, &mut w, |cx| cx.seek("drink"));
    assert_eq!(guest(&w).building, 9);
    let tile = |w: &World| {
        let (x, y, _) = spot(w);
        (x.div_euclid(64), y.div_euclid(64))
    };
    let mut served = false;
    for _ in 0..3000 {
        run(&mut s, &cat, &mut w, &mut money, 1, "never");
        // The kiosk is on (5, 3), its entrance on (5, 4).
        assert!(!(ENTRANCE..=USE).contains(&task(&w)) || tile(&w) != (5, 3));
        let users = &w.units.buildings.get(&9).map_or(0, |l| l.users.len());
        served |= *users == 1;
        if served && *users == 0 {
            break;
        }
        if *users == 1 {
            assert_eq!(tile(&w), (5, 4));
        }
    }
    assert!(served && w.units.buildings[&9].users.is_empty());
}

#[test]
fn a_picked_up_stand_pushes_its_customer_out_and_comes_back_with_its_figures_and_its_id() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    hungry(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    run(&mut s, &cat, &mut w, &mut money, 600, "buy");
    assert_eq!(task(&w), crate::goal::USE);
    let (before, count) = (w.can_undo(), w.zoo.entities.len());
    let mut live = w.units.buildings[&7].clone();
    live.price = 4.5;
    w.units.buildings.insert(7, live);
    let at = w.find(7).unwrap();
    let held = crate::building::lift(&mut w, &cat, 7, &mut Rng::default()).unwrap();
    // The customer paid and stands at the first entrance; the stand is off the map and no
    // undo records it; there is nothing to lift twice.
    assert_eq!(w.units.take_building_lines(), ["push 7 1"]);
    assert_eq!((spot(&w).0, spot(&w).1, task(&w)), (336, 400, 0));
    assert!(w.find(7).is_none() && !w.units.buildings.contains_key(&7));
    assert_eq!((w.can_undo(), w.zoo.entities.len()), (before, count - 1));
    assert!(crate::building::lift(&mut w, &cat, 7, &mut Rng::default()).is_none());
    // Set down elsewhere, facing another way: the same id, the same place in the list, the
    // price it had, and the figures the customer left.
    crate::building::set_down(&mut w, held, (352, 160, 0), 2);
    assert_eq!(w.find(7), Some(at));
    let c = w.zoo.entities[at].common.clone().unwrap();
    assert_eq!((c.x, c.y, c.rotation), (352, 160, 2));
    assert_eq!(w.units.buildings[&7].price, 4.5);
    assert!(w.units.buildings[&7].income[2] > 0.0 && w.units.buildings[&7].users.is_empty());
    assert_eq!((w.can_undo(), w.zoo.entities.len()), (before, count));
}

/// The zoo of `zoo()` loaded as a saved one: building `id` lists guest 1 as a user of its slot 0
/// and stores the price `price`; the guest's stamp is `stamp`; the AI clock reads 100.
fn saved(cat: &Catalogue, id: u32, price: f32, stamp: Option<u32>) -> World {
    let mut w = zoo();
    let at = w.find(id).unwrap();
    w.zoo.entities[at].common.as_mut().unwrap().building = Some(crate::zoo::BuildingRecord {
        guests: [0; 3],
        income: [0; 3],
        upkeep: None,
        price: price.to_bits(),
        users: vec![(1, 0)],
        fill: 0,
        built: 0,
    });
    let guest = w.find(1).unwrap();
    w.zoo.entities[guest].common.as_mut().unwrap().stamp = stamp;
    for at in 0..w.zoo.entities.len() {
        crate::building::load(&mut w, cat, at);
    }
    crate::building::users::restore(&mut w, cat, 100);
    w
}

/// Steps until the guest is no longer listed in building `id`; the AI tick it was gone at.
fn gone_at(s: &mut Sim, cat: &Catalogue, w: &mut World, money: &mut Money, id: u32) -> u32 {
    for _ in 0..3000 {
        if w.units.buildings[&id].users.is_empty() {
            return w.units.ai_ticks;
        }
        run(s, cat, w, money, 1, "never");
    }
    panic!("the guest never left");
}

#[test]
fn a_guest_inside_a_stand_at_the_save_leaves_after_its_time_left_and_pays_the_stored_price() {
    let (cat, mut money) = (types(), Money::default());
    // The stand's time inside is 3 ticks. A guest stamped 98 has 1 tick left at the load, one with
    // no stamp (0) is taken to enter now and has all 3.
    let mut ticks = Vec::new();
    for stamp in [Some(98), None] {
        let mut w = saved(&cat, 7, 3.0, stamp);
        let live = &w.units.buildings[&7];
        assert_eq!((live.price, live.users.len()), (3.0, 1));
        assert_eq!(task(&w), crate::goal::USE);
        let mut s = game_at(100);
        let at = gone_at(&mut s, &cat, &mut w, &mut money, 7);
        ticks.push(at);
        // 3 x 3.00 for the stored price, booked when it left: not the default 2.00.
        run(&mut s, &cat, &mut w, &mut money, 1, "never");
    }
    assert_eq!(money.total[money::CONCESSIONS], 18.0);
    assert_eq!(ticks[1] - ticks[0], 2, "{ticks:?}");
}

#[test]
fn a_reset_start_removes_the_users_put_back_at_the_load() {
    let (cat, mut money) = (types(), Money::default());
    let mut w = saved(&cat, 8, 0.0, Some(100));
    assert!(w.units.get(1).unwrap().hidden);
    let removed = crate::building::reset_all(&mut w, 0);
    assert_eq!(removed, [1]);
    assert!(w.units.buildings[&8].users.is_empty());
    // The caller asks the removal; the unit is gone at the next removal pass and pays nothing.
    let mut s = game_at(100);
    s.remove(1);
    run(&mut s, &cat, &mut w, &mut money, 3, "never");
    assert!(w.units.get(1).is_none() && w.find(1).is_none());
    assert_eq!(money.total[money::CONCESSIONS], 0.0);
}

#[test]
fn a_customer_picked_up_leaves_the_stand_paying_and_with_no_goal() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    hungry(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    run(&mut s, &cat, &mut w, &mut money, 600, "buy");
    assert_eq!(task(&w), crate::goal::USE);
    w.units.concessions.clear();
    let carry = crate::place::unit::pick_up(&mut w, &cat, 1, 0).unwrap();
    assert_eq!(w.units.concessions, [6.0]);
    assert!(w.units.buildings[&7].users.is_empty());
    assert_eq!((task(&w), guest(&w).building, guest(&w).need), (0, 0, None));
    assert!(w.units.get(1).unwrap().carried);
    assert_eq!(carry.from.0, spot(&w).0);
    // Put down, the guest goes on with its life and the stand is free for the next one.
    assert!(crate::place::unit::put_down(
        &mut w,
        1,
        (2 * 64 + 32, 6 * 64 + 32, 0),
        0
    ));
    run(&mut s, &cat, &mut w, &mut money, 50, "never");
    assert_eq!(w.units.buildings[&7].income[2], 6.0);
}

#[test]
fn a_hidden_restroom_user_picked_up_is_shown_and_not_counted_any_more() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.values[BATHROOM], g.countdowns, g.saw_animal) = (80, [1000; 11], 1000);
    }
    with_cx(&cat, &mut w, |cx| cx.seek("bathroom"));
    run(&mut s, &cat, &mut w, &mut money, 800, "use");
    assert!(w.units.get(1).unwrap().hidden);
    let before = guest(&w).pending[BATHROOM];
    crate::place::unit::pick_up(&mut w, &cat, 1, 0).unwrap();
    assert!(!w.units.get(1).unwrap().hidden);
    assert!(w.units.buildings[&8].users.is_empty());
    assert_eq!(guest(&w).pending[BATHROOM], before - 100);
}

#[test]
fn a_guest_picked_up_on_its_way_in_is_taken_out_unserved() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    hungry(&mut w);
    with_cx(&cat, &mut w, |cx| cx.seek("food"));
    for _ in 0..600 {
        run(&mut s, &cat, &mut w, &mut money, 1, "use");
        if task(&w) == crate::goal::ENTER_BUILDING {
            break;
        }
    }
    assert_eq!(task(&w), crate::goal::ENTER_BUILDING);
    w.units.concessions.clear();
    crate::place::unit::pick_up(&mut w, &cat, 1, 0).unwrap();
    assert!(w.units.concessions.is_empty());
    assert!(w.units.buildings.get(&7).is_none_or(|l| l.users.is_empty()));
    assert_eq!((task(&w), guest(&w).building), (0, 0));
    // Put down, it does not walk into the stand on the old goal.
    crate::place::unit::put_down(&mut w, 1, (2 * 64 + 32, 6 * 64 + 32, 0), 0);
    assert!(run(&mut s, &cat, &mut w, &mut money, 5, "use").is_empty());
}

#[test]
fn a_guest_with_no_building_visit_picked_up_keeps_its_task() {
    let (cat, mut w, mut money) = (types(), zoo(), Money::default());
    let mut s = game();
    run(&mut s, &cat, &mut w, &mut money, 3, "never");
    let before = task(&w);
    crate::place::unit::pick_up(&mut w, &cat, 1, 0).unwrap();
    assert_eq!(task(&w), before);
}

#[test]
fn picked_up_is_carried_from_the_exit() {
    let (cat, mut w, mut money) = (types(), zoo_with(true), Money::default());
    holding(&mut w);
    run(&mut game(), &cat, &mut w, &mut money, 1500, "use");
    // Seated at the table, 32 above the exit tile.
    assert_eq!(spot(&w), (336, 240, 3));
    assert!(!w.units.buildings[&9].users.is_empty());
    let carry = crate::place::unit::pick_up(&mut w, &cat, 1, 0).unwrap();
    assert_eq!((carry.from.0, carry.from.1), (336, 272));
    assert!(w.units.buildings[&9].users.is_empty());
}

#[test]
fn a_hidden_user_stays_in_the_crowd_of_its_megatile() {
    let (cat, mut w, mut money) = (types(), zoo_of(Some("booth")), Money::default());
    let mut s = game();
    holding(&mut w);
    run(&mut s, &cat, &mut w, &mut money, 1500, "use");
    assert!(w.units.get(1).unwrap().hidden);
    crate::guest::mega::update(&mut w, &cat, 7500);
    let n: u32 = [0usize, 5, 50, 55]
        .iter()
        .map(|&t| w.units.mega.at(10, t).unwrap().guests)
        .sum();
    assert_eq!(n, 1);
}

/// A hall whose users neither stay outside nor use the exit, and whose slot has no entrance.
const SHUT: &str = "[Characteristics/Integers]\ncCapacity=1\ncUserStaysOutside=0\n\
    [slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\nexitpos=0\nexitpos=48\ncapacity=1\n";

/// Draws until the next `pick(15)` is 0.
fn hit(cx: &mut Cx) {
    while cx.ctx.rng.clone().pick(15) != 0 {
        cx.ctx.rng.rand();
    }
}

/// Runs `f` and checks that it made `n` draws.
fn draws(cx: &mut Cx, n: usize, f: impl FnOnce(&mut Cx)) {
    let mut seen = cx.ctx.rng.clone();
    (0..n).for_each(|_| _ = seen.rand());
    f(cx);
    assert_eq!(*cx.ctx.rng, seen);
}

#[test]
fn a_walk_out_with_no_entrance_or_no_building_sets_no_target_and_decides_again() {
    let cat = with_hall(SHUT);
    for building in [9, 0] {
        let mut w = footprint("guests", centre((5, 3)), centre((5, 3)));
        with_cx(&cat, &mut w, |cx| {
            cx.guest().unwrap().building = building;
            cx.set_task(EXIT_BUILDING, 1, building, None);
            for _ in 0..2 {
                draws(cx, 0, |cx| cx.exit_decide());
                assert_eq!(cx.record().target.tile, None, "{building}");
                assert!(cx.unit.through_building, "{building}");
            }
        });
    }
}

#[test]
fn the_walk_out_is_done_on_the_one_in_fifteen_once_off_a_building() {
    let cat = with_hall(SHUT);
    // On (2, 6), no building; on the hall's tile (5, 3), the hall: the target is far either way.
    for (at, done) in [((2, 6), true), ((5, 3), false)] {
        let mut w = footprint("guests", centre(at), centre((5, 3)));
        let done_now = with_cx(&cat, &mut w, |cx| {
            cx.guest().unwrap().building = 7;
            cx.set_task(EXIT_BUILDING, 1, 7, w_tile((9, 9)));
            hit(cx);
            cx.exit_check_done();
            cx.record().done
        });
        assert_eq!(done_now, done, "{at:?}");
    }
}

/// Guest 1 on the entrance tile (5, 9) of the restroom 8 at (5, 8), heading for it (0x67).
fn at_the_entrance() -> World {
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((5, 9)), 0));
        zoo.entities
            .push(entity("building", "restroom", 8, centre((5, 8)), 4));
    });
    if let Part::Guest(g) = &mut w.units.get_mut(1).unwrap().part {
        (g.need, g.building, g.entrance) = (Some("bathroom"), 8, centre((5, 9)));
    }
    w.units.get_mut(1).unwrap().goal.record.task = ENTRANCE;
    w
}

#[test]
fn the_walk_into_the_slot_starts_in_the_decide_one_update_after_the_entrance() {
    let (cat, mut w) = (types(), at_the_entrance());
    with_cx(&cat, &mut w, |cx| {
        cx.set_task(ENTRANCE, 0, 8, w_tile((5, 9)));
        // The entrance is reached: 0x68 with no target, no walk yet.
        cx.run_goal();
        assert_eq!(
            (cx.record().task, cx.record().target.tile),
            (ENTER_BUILDING, None)
        );
        assert!(!cx.unit.through_building && cx.unit.path.is_empty());
        // The next update walks into the slot on (5, 8), with the building flag.
        cx.run_goal();
        assert_eq!(cx.record().target.tile, w_tile((5, 8)));
        assert!(cx.unit.through_building && !cx.unit.path.is_empty());
    });
}

#[test]
fn a_slot_that_filled_up_leaves_the_walk_in_to_the_complete() {
    let cat = types();
    for (need, left) in [("bathroom", ENTER_BUILDING), ("food", 0)] {
        let mut w = at_the_entrance();
        let kind = building::kind_of(&cat, &w.zoo.entities[1], 3.0).unwrap();
        let live = w
            .units
            .buildings
            .entry(8)
            .or_insert_with(|| Live::new(&kind));
        live.users.extend([(100, 0, 0), (101, 0, 0)]);
        let task_now = with_cx(&cat, &mut w, |cx| {
            cx.guest().unwrap().need = Some(need);
            cx.set_task(ENTER_BUILDING, 0, 8, w_tile((5, 8)));
            hit(cx);
            cx.enter_check_done();
            cx.record().task
        });
        // The full slot does not end the goal; a building that does not serve the need does.
        assert_eq!(task_now, left, "{need}");
        if left == ENTER_BUILDING {
            // The complete refuses the user.
            with_cx(&cat, &mut w, |cx| cx.enter_arrived());
            assert_eq!(task(&w), 0);
        }
    }
}

#[test]
fn the_draws_of_the_changed_building_goals_follow_the_goal_updates() {
    let cat = with_hall(SHUT);
    let mut w = at_the_entrance();
    with_cx(&cat, &mut w, |cx| {
        cx.set_task(ENTRANCE, 0, 8, w_tile((5, 9)));
        // Update n: the common part; the 0x67 complete draws nothing.
        draws(cx, 1, |cx| {
            cx.common();
            cx.run_goal();
        });
        // Update n + 1: the common part, the 0x68 check done, then the walk in.
        draws(cx, 2, |cx| {
            cx.common();
            cx.run_goal();
        });
        assert_eq!(cx.record().target.tile, w_tile((5, 8)));
    });
    // A unit sent out of the hall, whose slot has no entrance: the common part and the check done
    // at every update, no entrance drawn, the goal kept.
    let mut w = footprint("guests", centre((5, 3)), centre((5, 3)));
    with_cx(&cat, &mut w, |cx| {
        hit(cx);
        for _ in 0..3 {
            draws(cx, 2, |cx| {
                cx.common();
                cx.run_goal();
            });
            assert_eq!(
                (cx.record().task, cx.record().target.tile),
                (EXIT_BUILDING, None)
            );
        }
    });
}

#[test]
fn an_entrance_or_slot_on_the_units_own_tile_still_raises_the_flag_and_walks() {
    let cat = types();
    // The restroom 8 at (5, 8): its slot is on (5, 8), its only entrance on (5, 9).
    let on = |at| {
        world_with((10, 10), |zoo| {
            zoo.entities.push(entity("guests", "man", 1, centre(at), 0));
            zoo.entities
                .push(entity("building", "restroom", 8, centre((5, 8)), 4));
        })
    };
    let mut w = on((5, 8));
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 8;
        cx.set_task(ENTER_BUILDING, 0, 8, None);
        cx.enter_decide();
        assert_eq!(cx.record().task, ENTER_BUILDING);
        assert_eq!(cx.record().target.tile, w_tile((5, 8)));
        assert!(cx.unit.through_building);
    });
    let mut w = on((5, 9));
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 8;
        cx.set_task(EXIT_BUILDING, 1, 8, None);
        draws(cx, 1, |cx| cx.exit_decide());
        assert_eq!(cx.record().target.tile, w_tile((5, 9)));
        assert!(cx.unit.through_building);
    });
    // A user that stays outside is done on its own tile, with the flag down.
    let mut w = world_with((10, 10), |zoo| {
        zoo.entities
            .push(entity("guests", "man", 1, centre((5, 4)), 0));
        zoo.entities
            .push(entity("building", "kiosk", 9, centre((5, 3)), 4));
    });
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 9;
        cx.set_task(EXIT_BUILDING, 1, 9, None);
        cx.exit_decide();
        assert_eq!(cx.record().target.tile, w_tile((5, 4)));
        assert!(!cx.unit.through_building);
    });
}

#[test]
fn the_enter_decide_clears_the_goal_when_the_building_is_gone() {
    let (cat, mut w) = (types(), at_the_entrance());
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 99;
        cx.set_task(ENTER_BUILDING, 0, 99, None);
        cx.enter_decide();
        assert_eq!(cx.record().task, 0);
    });
}

/// Guest 1 hidden where it stands, its building 9 gone: the hidden user of a gone building.
fn hidden_in_a_gone_building() -> World {
    let mut w = zoo();
    let at = w.find(1).unwrap();
    w.units.get_mut(1).unwrap().hidden = true;
    w.hide_unit(at);
    w
}

#[test]
fn a_hidden_user_of_a_gone_building_stays_hidden_after_the_use_check() {
    let (cat, mut w) = (types(), hidden_in_a_gone_building());
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 9;
        cx.set_task(USE, 0, 9, None);
        cx.use_check_done();
    });
    assert!(w.units.get(1).unwrap().hidden);
    assert_eq!(task(&w), 0);
    assert!(w.units.get(1).unwrap().goal.wandered);
}

#[test]
fn a_hidden_user_of_a_gone_building_stays_hidden_after_the_walk_out_check() {
    let (cat, mut w) = (types(), hidden_in_a_gone_building());
    with_cx(&cat, &mut w, |cx| {
        cx.guest().unwrap().building = 9;
        cx.set_task(EXIT_BUILDING, 1, 9, None);
        hit(cx);
        cx.exit_check_done();
    });
    assert!(w.units.get(1).unwrap().hidden);
    assert_eq!(task(&w), 0);
    assert!(!w.units.get(1).unwrap().goal.wandered);
}
