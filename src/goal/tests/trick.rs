//! An animal's trick: the start, the three kinds and the events it posts (shows 23.2 to 23.5).

use super::*;
use crate::entity::EntityType;
use crate::fixtures;
use crate::show::event;
use crate::unit::testing::{Walker, world_with};
use crate::zoo::{Item, ItemState, Script, ShowInfo};

const SEAL: u32 = 5400;
const TOY: u32 = 9100;
const SHOW: u16 = 1001;
const WAVE: u16 = 11200;

fn ty(class: &'static str, name: &str, subtype: &str, id: u32, text: &str) -> EntityType {
    EntityType {
        name_id: id as i32,
        ..fixtures::ty(class, name, subtype, text)
    }
}

/// A seal that skips tricks at or below `happy` with `chance` percent, and a toy for one seal.
fn types(skip: &str) -> Catalogue {
    types_with(skip, "")
}

/// [`types`] with `extra` ini text appended to the seal's.
fn types_with(skip: &str, extra: &str) -> Catalogue {
    let toy = "[Characteristics/Integers]\ncCapacity=1\ncTimeInside=9\ncUserStaysOutside=0\n\
        [Satisfies]\nbuilding\nshowtrick\n[slots]\nname=slot\n[slot]\nslotpos=0\nslotpos=0\n\
        entrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1\n";
    let mut cat = Catalogue::default();
    let seal = format!("[Characteristics/Integers]\n{skip}\n{extra}");
    cat.push(ty("animals", "seal", "m", SEAL, &seal));
    cat.push(ty("building", "building", "sealwav", TOY, toy));
    cat
}

/// A seal, unit 1, at (2, 2) of a map with a toy 9 at (5, 3); the show 1001 holds a script of the
/// item `item` and a state for the seal at item 0.
fn zoo(item: Item) -> World {
    let mut w = world_with((10, 10), |z| {
        let mut seal = entity("animals", "seal", 1, centre((2, 2)), 0);
        seal.subtype = "m".into();
        z.entities.push(seal);
        let mut toy = entity("building", "building", 9, centre((5, 3)), 4);
        toy.subtype = "sealwav".into();
        z.entities.push(toy);
    });
    w.zoo.shows.scripts = vec![Script {
        id: 5,
        marker: 0,
        kind: SEAL,
        items: vec![item],
    }];
    let mut info = ShowInfo {
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
        show: Default::default(),
    };
    info.show.script = 5;
    info.show.items = vec![ItemState {
        script: 5,
        animal: 1,
        item: 0,
        finished: false,
        aborted: false,
        cue_sent: false,
        ready: false,
        started: false,
        done: false,
    }];
    w.zoo.shows.infos = vec![Some(info)];
    w
}

fn trick(marker: u32, name: &str) -> Item {
    Item {
        available: 1,
        visible: 1,
        id: WAVE,
        kind: SEAL,
        marker,
        names: [name.into(), String::new(), String::new(), String::new()],
        building: TOY,
        complexity: 1,
        returns: 0,
        satisfaction: 3,
        delta: 1,
        current: 3,
        depth: 1,
        help: [0; 2],
        icons: Default::default(),
    }
}

/// Runs `f` on the goal update context of unit 1 with `data`.
fn on<R>(
    cat: &Catalogue,
    w: &mut World,
    rng: &mut Rng,
    data: &dyn Data,
    f: impl FnOnce(&mut Cx) -> R,
) -> R {
    let mut u = unit(w).clone();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat,
    };
    let at = w.find(1).unwrap();
    let r = f(&mut Cx {
        unit: &mut u,
        world: w,
        ctx: &mut ctx,
        at,
        data,
    });
    *w.units.get_mut(1).unwrap() = u;
    r
}

fn ask(w: &mut World, cat: &Catalogue, rng: &mut Rng, data: &dyn Data) -> i32 {
    let it = crate::show::trick_item(w, SHOW, WAVE).unwrap();
    start_trick(w, cat, rng, data, 1, (&it, SHOW))
}

fn happy(w: &mut World, value: i32) {
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.happiness = value;
    }
}

#[test]
fn a_behaviour_trick_starts_its_set_and_keeps_its_ids_whatever_the_set() {
    // A set that does not exist still counts as started: it ends at its first run.
    let sets = Sets::new(&[("bwave", &["fKeepControl()"])]);
    for name in ["wave", "nothing"] {
        let (cat, mut w, mut rng) = (types(""), zoo(trick(0, name)), Rng::default());
        assert_eq!(ask(&mut w, &cat, &mut rng, &sets), 0);
        let u = unit(&w);
        assert_eq!((u.trick, u.show), (WAVE, SHOW));
        assert_eq!(u.runner.top(), Some(format!("b{name}").as_str()));
        assert_eq!(
            rng.state(),
            Rng::default().state(),
            "no draw above the threshold"
        );
    }
}

#[test]
fn a_trick_item_of_another_type_never_starts() {
    for marker in [2, 4, u32::MAX] {
        let (cat, mut w, mut rng) = (types(""), zoo(trick(marker, "wave")), Rng::default());
        assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 3, "type {marker}");
        assert_eq!(unit(&w).trick, 0);
    }
}

#[test]
fn an_animal_at_or_below_the_happiness_threshold_refuses_a_trick_with_its_chance() {
    // The threshold and the chance are read with the defaults -100 and 0 when missing; a draw is
    // made for every animal at or below the threshold, 0 and negative chances never refuse.
    let cases = [
        (
            "cSkipTrickHappiness=50\ncSkipTrickChance=100\n",
            50,
            8,
            true,
        ),
        (
            "cSkipTrickHappiness=50\ncSkipTrickChance=100\n",
            51,
            0,
            false,
        ),
        ("cSkipTrickHappiness=50\ncSkipTrickChance=0\n", 50, 0, true),
        (
            "cSkipTrickHappiness=50\ncSkipTrickChance=-5\n",
            -100,
            0,
            true,
        ),
        (
            "cSkipTrickHappiness=50\ncSkipTrickChance=2000000000\n",
            0,
            8,
            true,
        ),
        (
            "cSkipTrickHappiness=2000000000\ncSkipTrickChance=100\n",
            100,
            8,
            true,
        ),
        ("", -100, 0, true),
        ("", -99, 0, false),
    ];
    for (keys, happiness, result, drew) in cases {
        let (cat, mut w, mut rng) = (types(keys), zoo(trick(0, "wave")), Rng::default());
        happy(&mut w, happiness);
        assert_eq!(
            ask(&mut w, &cat, &mut rng, &NoData),
            result,
            "{keys:?} at {happiness}"
        );
        let mut one = Rng::default();
        one.rand();
        assert_eq!(rng.state() == one.state(), drew, "{keys:?} at {happiness}");
    }
}

#[test]
fn a_refused_trick_is_not_stored_and_a_food_trick_is_never_refused() {
    let keys = "cSkipTrickHappiness=50\ncSkipTrickChance=100\n";
    let (cat, mut w, mut rng) = (types(keys), zoo(trick(0, "wave")), Rng::default());
    happy(&mut w, 0);
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 8);
    assert_eq!(unit(&w).trick, 0);
    let (mut w, mut rng) = (zoo(trick(3, "foodtrick")), Rng::default());
    happy(&mut w, -100);
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    // Task 0x7d, mode 0, the ids stored, no draw.
    let u = unit(&w);
    assert_eq!((u.goal.record.task, u.goal.record.mode), (125, 0));
    assert_eq!((u.trick, u.show), (WAVE, SHOW));
    assert_eq!(rng.state(), Rng::default().state());
}

#[test]
fn the_end_of_a_behaviour_set_tells_the_unit_its_trick_is_done() {
    let (cat, mut w, mut rng) = (types(""), zoo(trick(0, "wave")), Rng::default());
    let sets = Sets::new(&[("bwave", &["fEndTrick()"])]);
    assert_eq!(ask(&mut w, &cat, &mut rng, &sets), 0);
    let at = w.find(1).unwrap();
    let (events, mut removals) = (Events::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &events,
        removals: &mut removals,
        step: 67,
        cat: &cat,
    };
    crate::unit::update(
        &mut crate::unit::Standard { data: &sets },
        &mut w,
        &mut ctx,
        at,
    );
    let posts: Vec<_> = (w.units.posts.iter())
        .map(|e| (e.kind, e.sender, e.target, e.channel, e.delay))
        .collect();
    assert_eq!(posts, [(event::FINISHED, 1, 1, 0x55, 1)]);
}

#[test]
fn a_toy_trick_aims_at_a_free_toy_and_serves_the_trick_need() {
    let (cat, mut w, mut rng) = (types(""), zoo(trick(1, "wave")), Rng::default());
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    let u = unit(&w);
    let Part::Animal(a) = &u.part else { panic!() };
    assert_eq!((a.building, a.toy, u.trick), (9, true, WAVE));
    // The only toy of the kind stands in the same area; one draw among one.
    assert_ne!(rng.state(), Rng::default().state());
    // No toy of that kind: the trick fails and nothing is aimed at.
    let mut it = trick(1, "wave");
    it.building = TOY + 1;
    let mut w = zoo(it);
    assert_eq!(ask(&mut w, &cat, &mut Rng::default(), &NoData), 3);
    assert_eq!(unit(&w).trick, 0);
}

#[test]
fn a_toy_that_cannot_take_the_animal_aborts_the_trick_and_leaving_one_ends_it() {
    let (cat, mut w, mut rng) = (types(""), zoo(trick(1, "wave")), Rng::default());
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    let kinds = |w: &World| w.units.posts.iter().map(|e| e.kind).collect::<Vec<_>>();
    // In its use the unit enters at once: the toy is full of another user, so it is refused.
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.set_task(USE, 0, 9, None);
        cx.world
            .units
            .buildings
            .entry(9)
            .or_default()
            .users
            .push((77, 0, 0));
        cx.set_enters();
    });
    assert_eq!(kinds(&w), [event::FAILED]);
    assert!(matches!(&unit(&w).part, Part::Animal(a) if a.building == 0 && !a.toy));
    // Leaving: the trick is done.
    w.units.posts.clear();
    let mut w = zoo(trick(1, "wave"));
    assert_eq!(ask(&mut w, &cat, &mut Rng::default(), &NoData), 0);
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.set_task(USE, 0, 9, None);
        cx.set_exits();
    });
    assert_eq!(kinds(&w), [event::FINISHED]);
}

/// The seal has a trick list, so it performs.
const PERFORMS: &str = "[tricks]\ntrick=a\n";

#[test]
fn a_trick_animal_whose_toy_is_full_at_arrival_aborts_the_item() {
    let (cat, mut w, mut rng) = (
        types_with("", PERFORMS),
        zoo(trick(1, "wave")),
        Rng::default(),
    );
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    // The toy takes one user and has another: no open slot when the animal arrives (task 0x66).
    w.units
        .buildings
        .entry(9)
        .or_default()
        .users
        .push((77, 0, 0));
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.building_arrived()
    });
    let kinds: Vec<u16> = w.units.posts.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [event::FAILED]);
    assert!(matches!(&unit(&w).part, Part::Animal(a) if a.building == 0 && !a.toy));
}

#[test]
fn a_trick_animal_whose_toy_is_gone_at_entry_aborts_the_item() {
    let (cat, mut w, mut rng) = (
        types_with("", PERFORMS),
        zoo(trick(1, "wave")),
        Rng::default(),
    );
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    w.zoo.entities.pop();
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.entrance_arrived()
    });
    let kinds: Vec<u16> = w.units.posts.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [event::FAILED]);
    assert!(matches!(&unit(&w).part, Part::Animal(a) if a.building == 0 && !a.toy));
}

/// A swimmer in the middle of the show tank (full to level 4), which has a gate, with the script
/// of a food trick.
fn tank() -> World {
    let mut w = crate::unit::testing::tank_world(5, 4, &[(1, (3, 3), 16)]);
    let u = w.units.get_mut(1).unwrap();
    (u.typed, u.started, u.mover.underwater, u.mover.depth) = (true, true, true, 1);
    if let Part::Animal(a) = &mut u.part {
        a.terrain = 2;
    }
    let at = w.find(1).unwrap();
    w.zoo.entities[at].subtype = "m".into();
    let mut shown = zoo(trick(3, "foodtrick"));
    w.zoo.shows = std::mem::take(&mut shown.zoo.shows);
    w
}

#[test]
fn the_food_trick_walks_to_the_platform_and_gives_the_treat_there() {
    let (cat, mut w, mut rng) = (types(""), tank(), Rng::default());
    assert_eq!(ask(&mut w, &cat, &mut rng, &Sets::walking()), 0);
    // Decide: a path to a tile round the gate's inside tile, one draw; the goal has a target.
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.trick_food_decide()
    });
    assert!(!unit(&w).path.is_empty());
    assert!(unit(&w).goal.record.target.tile.is_some());
    assert!(w.units.posts.is_empty());
    assert_ne!(rng.state(), Rng::default().state());
    // Complete: the set of the item starts, the specialist is told, the changes are added and the
    // goal is cleared.
    let sets = Sets::new(&[("bfoodtrick", &["fKeepControl()"])]);
    on(&cat, &mut w, &mut rng, &sets, |cx| cx.trick_food_complete());
    let u = unit(&w);
    let Part::Animal(a) = &u.part else { panic!() };
    assert_eq!(
        (a.happiness_change, a.hunger_change, a.energy_change),
        (10, -10, 0)
    );
    assert_eq!(
        (u.goal.record.task, u.runner.top()),
        (0, Some("bfoodtrick"))
    );
    let cue = w
        .units
        .posts
        .iter()
        .map(|e| (e.kind, e.sender, e.channel, e.extra))
        .collect::<Vec<_>>();
    assert_eq!(cue, [(event::TRICK_DONE, 1001, 0x4b, 0)]);
}

#[test]
fn the_food_trick_gives_no_cue_for_an_aborted_state_and_no_happiness_to_another_subtype() {
    let (cat, mut w, mut rng) = (types(""), tank(), Rng::default());
    w.zoo.shows.infos[0].as_mut().unwrap().show.items[0].aborted = true;
    let at = w.find(1).unwrap();
    w.zoo.entities[at].subtype = "p".into();
    assert_eq!(ask(&mut w, &cat, &mut rng, &Sets::walking()), 0);
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.trick_food_complete()
    });
    let Part::Animal(a) = &unit(&w).part else {
        panic!()
    };
    assert_eq!((a.happiness_change, a.hunger_change), (0, -10));
    assert!(w.units.posts.is_empty());
}

#[test]
fn the_food_trick_without_a_way_to_the_platform_clears_the_goal_and_aborts() {
    let (cat, mut w, mut rng) = (types(""), tank(), Rng::default());
    assert_eq!(ask(&mut w, &cat, &mut rng, &Sets::walking()), 0);
    // No gate: there is no tile inside it.
    w.exhibits_mut()
        .list_mut()
        .iter_mut()
        .for_each(|e| e.gate = None);
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.trick_food_decide()
    });
    assert_eq!(unit(&w).goal.record.task, 0);
    let kinds: Vec<_> = w.units.posts.iter().map(|e| (e.kind, e.target)).collect();
    assert_eq!(kinds, [(event::FAILED, 1)]);
}

#[test]
fn a_toy_is_found_by_its_type_and_only_while_it_has_room_with_one_draw() {
    use crate::show::toy::find_toy;
    let (cat, mut w) = (types(""), zoo(trick(1, "wave")));
    let from = 2 * 10 + 2;
    let mut one = Rng::default();
    one.rand();
    // Any toy (-1) or the named type: found with one draw; another type or a full toy: none, no draw.
    for (kind, found) in [
        (-1, true),
        (i64::from(TOY), true),
        (0, false),
        (i64::from(TOY) + 1, false),
    ] {
        let mut rng = Rng::default();
        let toy = find_toy(&w, &cat, &mut rng, None, from, kind);
        assert_eq!(toy, found.then_some(9), "kind {kind}");
        assert_eq!(rng.state() == one.state(), found, "kind {kind}");
    }
    w.units
        .buildings
        .entry(9)
        .or_default()
        .users
        .push((77, 0, 0));
    let mut rng = Rng::default();
    assert_eq!(find_toy(&w, &cat, &mut rng, None, from, -1), None);
    assert_eq!(rng.state(), Rng::default().state());
}

#[test]
fn an_animal_sent_to_a_toy_walks_there_uses_it_and_tells_its_trick_done_when_it_leaves() {
    let (cat, mut w, mut rng) = (types(""), zoo(trick(1, "wave")), Rng::default());
    // The use set is named from the toy's subtype, the trick and the slot.
    let sets = Sets::new(&[
        ("bWalk", &["fWalk(0,0)"]),
        (
            "bsealwavwave0",
            &["fEnterBuilding()", "fExitBuilding()", "fKeepControl()"],
        ),
    ]);
    assert_eq!(ask(&mut w, &cat, &mut rng, &sets), 0);
    let u = w.units.get_mut(1).unwrap();
    (u.typed, u.started) = (true, true);
    if let Part::Animal(a) = &mut u.part {
        a.counters = [1000; 16];
    }
    let mut s = sim();
    let mut log = Vec::new();
    let done = |s: &Sim| {
        let q = s.clocks().queue;
        let all = [q.first, q.second].concat();
        all.iter()
            .any(|e| e.kind == event::FINISHED && e.target == 1)
    };
    for _ in 0..300 {
        s.step_with(&sets, &cat, &mut w, 67, |_, _, _| {});
        log.extend(w.units.take_lines());
        if done(&s) {
            break;
        }
    }
    let line = |what: &str| log.iter().position(|l| l.contains(what));
    assert!(
        line("goal 1 building") < line("goal 1 enterbuilding"),
        "{log:?}"
    );
    assert!(
        line("goal 1 enterbuilding") < line("set 1 bsealwavwave0"),
        "{log:?}"
    );
    assert!(done(&s), "{log:?}");
    assert!(w.units.buildings[&9].users.is_empty());
}

/// A swimmer's sets: the walker's, and `bfoodtrick`, which ends the trick.
struct Treat(Walker, Vec<Call>);

impl Data for Treat {
    fn set(&self, e: &crate::zoo::Entity, name: &str) -> Option<&[Call]> {
        match name {
            "bfoodtrick" => Some(&self.1),
            _ => self.0.set(e, name),
        }
    }
    fn ambient(&self, e: &crate::zoo::Entity, place: usize) -> &[Row] {
        self.0.ambient(e, place)
    }
    fn timing(&self, e: &crate::zoo::Entity, name: &str) -> Option<Timing> {
        self.0.timing(e, name)
    }
    fn number(&self, e: &crate::zoo::Entity, number: &Number) -> i32 {
        self.0.number(e, number)
    }
    fn sound(&self, e: &crate::zoo::Entity, key: &str) -> Option<Heard> {
        self.0.sound(e, key)
    }
}

#[test]
fn the_food_trick_goal_runs_in_the_update_and_ends_with_the_treat_set() {
    let (cat, mut w, mut rng) = (types(""), tank(), Rng::default());
    let end = parse_call(b"fEndTrick()").unwrap().0;
    let sets = Treat(Walker::new("fSwim(level,0)"), vec![end]);
    assert_eq!(ask(&mut w, &cat, &mut rng, &sets), 0);
    if let Part::Animal(a) = &mut w.units.get_mut(1).unwrap().part {
        a.counters = [1000; 16];
    }
    let mut s = sim();
    let mut log = Vec::new();
    for _ in 0..400 {
        s.step_with(&sets, &cat, &mut w, 67, |_, _, _| {});
        log.extend(w.units.take_lines());
        if log.iter().any(|l| l.contains("set 1 bfoodtrick")) {
            break;
        }
    }
    let line = |what: &str| log.iter().position(|l| l.contains(what));
    assert!(line("goal 1 trickfood").is_some(), "{log:?}");
    assert!(line("set 1 bWalk") < line("set 1 bfoodtrick"), "{log:?}");
    assert!(line("set 1 bfoodtrick").is_some(), "{log:?}");
}

#[test]
fn a_toy_deleted_while_the_animal_walks_in_drops_the_goal_without_aborting_the_item() {
    // Shows 30.6 point 1: the check done of 0x68 resets; only the arrival's complete clears.
    let (cat, mut w, mut rng) = (
        types_with("", PERFORMS),
        zoo(trick(1, "wave")),
        Rng::default(),
    );
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    w.zoo.entities.pop();
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.set_task(ENTER_BUILDING, 0, 9, None);
        // One draw in 15 looks at the building again.
        for _ in 0..500 {
            if cx.record().task != ENTER_BUILDING {
                break;
            }
            cx.enter_check_done();
        }
    });
    assert_eq!(unit(&w).goal.record.task, NONE);
    assert!(w.units.posts.is_empty());
    assert!(!w.zoo.shows.infos[0].as_ref().unwrap().show.items[0].aborted);
    // The same for the check done of the way out (0x69).
    let mut w = zoo(trick(1, "wave"));
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    w.zoo.entities.pop();
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.set_task(EXIT_BUILDING, 1, 9, None);
        for _ in 0..500 {
            if cx.record().task != EXIT_BUILDING {
                break;
            }
            cx.exit_check_done();
        }
    });
    assert_eq!(unit(&w).goal.record.task, NONE);
    assert!(w.units.posts.is_empty());
}

#[test]
fn a_toy_deleted_before_the_animal_is_inside_aborts_the_item_at_the_arrival() {
    // Shows 30.6 point 1, the arrival first: the complete of 0x68 clears the goal.
    let (cat, mut w, mut rng) = (
        types_with("", PERFORMS),
        zoo(trick(1, "wave")),
        Rng::default(),
    );
    assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
    w.zoo.entities.pop();
    on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
        cx.set_task(ENTER_BUILDING, 0, 9, None);
        cx.enter_arrived();
    });
    let kinds: Vec<u16> = w.units.posts.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [event::FAILED]);
    assert_eq!(unit(&w).goal.record.task, NONE);
}

#[test]
fn an_animal_is_never_turned_away_at_the_entrance_for_the_price() {
    // Shows 30.6 point 2: the test of the price is a guest's; the animal goes on to 0x68.
    let toy = "[Characteristics/Integers]\ncCapacity=1\ncTimeInside=9\ncUserStaysOutside=0\n\
        cCommerce=1\n[Satisfies]\nbuilding\nshowtrick\n[slots]\nname=slot\n[slot]\nslotpos=0\n\
        slotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1\n";
    let mut cat = Catalogue::default();
    cat.push(ty("animals", "seal", "m", SEAL, PERFORMS));
    cat.push(ty("building", "building", "sealwav", TOY, toy));
    for price in [1e9_f32, 0.0] {
        let (mut w, mut rng) = (zoo(trick(1, "wave")), Rng::default());
        assert_eq!(ask(&mut w, &cat, &mut rng, &NoData), 0);
        w.units.buildings.insert(
            9,
            crate::building::Live {
                price,
                ..Default::default()
            },
        );
        on(&cat, &mut w, &mut rng, &Sets::walking(), |cx| {
            cx.set_task(ENTRANCE, 0, 9, None);
            cx.entrance_arrived();
        });
        assert_eq!(unit(&w).goal.record.task, ENTER_BUILDING, "{price}");
        assert!(w.units.posts.is_empty());
    }
}
