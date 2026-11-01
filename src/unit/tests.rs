//! The unit table and the per-entity update.

use super::*;
use crate::behaviour::{Data, Heard, NoData};
use crate::sim::Sim;
use crate::zoo::{Clocks, Common, Tile, View, Zoo};

fn entity(class: &str, id: u32, (x, y): (i32, i32)) -> Entity {
    Entity {
        class: class.into(),
        type_name: String::new(),
        subtype: String::new(),
        body_size: None,
        common: Some(Common {
            x,
            y,
            z: 0,
            rotation: 0,
            id,
            name: String::new(),
            colors: Vec::new(),
            fence: None,
            ..Default::default()
        }),
    }
}

/// A 4 by 4 world, all of it inside the zoo, with these units (class, id) at the tile (1, 1).
fn world(units: &[(&str, u32)]) -> World {
    let tile = Tile {
        elevation: 0,
        shape: 0,
        terrain: 0,
        edges: 0,
    };
    let zoo = Zoo {
        awards: Vec::new(),
        kept: Default::default(),
        version: 0,
        locale: 0,
        expansion: 0,
        width: 4,
        height: 4,
        entrance: (0, 0),
        habitat_tiles: Vec::new(),
        habitat_gates: Vec::new(),
        habitat_info: Vec::new(),
        tanks: Vec::new(),
        id_counter: 1_000_000,
        tiles: vec![tile; 16],
        entities: (units.iter())
            .map(|&(class, id)| entity(class, id, (96, 96)))
            .collect(),
        view: View::default(),
        clocks: Clocks::default(),
        names: Default::default(),
    };
    let mut w = World::new(zoo, |_| None);
    crate::goal::quiet(&mut w);
    w
}

fn sim() -> Sim {
    let mut sim = Sim::new(b"");
    sim.set_seed(1);
    sim.start_game(16, Clocks::default());
    sim
}

fn status_lines(world: &mut World) -> Vec<String> {
    (world.units.take_lines().into_iter())
        .filter(|l| l.starts_with("status "))
        .collect()
}

#[test]
fn the_table_holds_the_units_of_the_list_in_list_order() {
    let mut w = world(&[
        ("animals", 5),
        ("objects", 6),
        ("guests", 7),
        ("keeper", 8),
        ("maint", 9),
        ("tour", 10),
        ("fences", 11),
        ("helicopter", 12),
    ]);
    let kinds: Vec<_> = w.units.iter().map(|u| (u.id, u.kind)).collect();
    assert_eq!(
        kinds,
        [
            (5, Kind::Animal),
            (7, Kind::Guest),
            (8, Kind::Keeper),
            (9, Kind::Worker),
            (10, Kind::Guide)
        ]
    );
    assert_eq!(w.units.len(), 5);
    assert_eq!(
        w.units.take_lines(),
        [
            "add 5 animal 1,1",
            "add 7 guest 1,1",
            "add 8 keeper 1,1",
            "add 9 worker 1,1",
            "add 10 guide 1,1"
        ]
    );
    assert!(w.units.get(6).is_none() && w.units.get(7).is_some());
}

#[test]
fn a_unit_added_or_removed_enters_or_leaves_the_table_and_the_others_keep_their_state() {
    let mut w = world(&[("animals", 1), ("guests", 2)]);
    w.units.take_lines();
    w.units.get_mut(2).unwrap().wait = 77;
    let id = w.add(entity("animals", 0, (64, 64)), None).unwrap();
    assert_eq!(w.units.take_lines(), [format!("add {id} animal 1,1")]);
    assert_eq!(w.units.get(2).unwrap().wait, 77);
    // The new unit's timer starts at its id modulo the period.
    assert_eq!(w.units.get(id).unwrap().status_timer, (id % 1000) as i32);
    assert!(w.remove(1));
    assert_eq!(w.units.take_lines(), ["remove 1"]);
    assert_eq!(w.units.len(), 2);
    assert_eq!(w.units.get(2).unwrap().wait, 77);
}

#[test]
fn the_period_starts_every_timer_at_id_modulo_it() {
    let mut w = world(&[("animals", 113_954), ("guests", 121_718), ("keeper", 7)]);
    let timers = |w: &World| w.units.iter().map(|u| u.status_timer).collect::<Vec<_>>();
    assert_eq!(timers(&w), [954, 718, 7]);
    w.units.set_period(500);
    assert_eq!(timers(&w), [454, 218, 7]);
    for period in [0, -5] {
        w.units.set_period(period);
        assert_eq!(timers(&w), [0, 0, 0], "period {period}");
    }
}

#[test]
fn each_unit_has_one_status_tick_per_period_spread_by_its_id() {
    let mut w = world(&[("animals", 113_954), ("guests", 121_718), ("keeper", 7)]);
    let mut s = sim();
    w.units.take_lines();
    let mut first = Vec::new();
    let mut count = [0; 3];
    for step in 1..=3000 {
        s.step(&mut w, 67, |_, _, _| {});
        for line in status_lines(&mut w) {
            let id: u32 = line[7..].parse().unwrap();
            let k = [113_954, 121_718, 7].iter().position(|&i| i == id).unwrap();
            if count[k] == 0 {
                first.push((id, step));
            }
            count[k] += 1;
        }
    }
    // The worked example of the simulation spec: steps 1, 5 and 15, then 201 ticks in 201 s.
    first.sort_by_key(|&(_, step)| step);
    assert_eq!(first, [(113_954, 1), (121_718, 5), (7, 15)]);
    assert_eq!(count, [201, 201, 201]);
}

#[derive(Default)]
struct Recorder {
    calls: Vec<&'static str>,
    heard: Vec<usize>,
    /// The milliseconds the animation was asked to advance by.
    advances: Vec<u32>,
}

impl Hooks for Recorder {
    fn movement(&mut self, u: &mut Unit, _: &mut World, _: &mut Ctx, _: usize) {
        self.calls.push("move");
        self.heard.push(u.events.len());
    }
    fn animate(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _: usize, ms: u32) {
        self.advances.push(ms);
    }
    fn needs(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _: usize) {
        self.calls.push("needs");
    }
    fn checks(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _: usize) {
        self.calls.push("checks");
    }
    fn behaviour(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _: usize) {
        self.calls.push("behaviour");
    }
    fn goals(&mut self, _: &mut Unit, _: &mut World, _: &mut Ctx, _: usize) {
        self.calls.push("goals");
    }
}

/// One turn of the unit at position `at` with these hooks and events.
fn turn_at(w: &mut World, events: &Events, hooks: &mut Recorder, step: u32, at: usize) {
    let (mut rng, mut removals) = (Rng::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events,
        removals: &mut removals,
        step,
        cat: &crate::entity::Catalogue::default(),
    };
    update(hooks, w, &mut ctx, at);
}

fn turn_of(w: &mut World, events: &Events, hooks: &mut Recorder, step: u32) {
    turn_at(w, events, hooks, step, 0);
}

fn event(target: u32, channel: u8) -> Event {
    Event {
        sender: 9,
        target,
        data: 0,
        extra: 0,
        kind: 0x2711,
        delay: 0,
        sender_channel: UNIT_CHANNEL,
        channel,
    }
}

#[test]
fn a_turn_runs_in_the_originals_order() {
    let mut w = world(&[("animals", 1)]);
    let mut hooks = Recorder::default();
    // A status tick on this very turn: the timer is already at the period.
    w.units.get_mut(1).unwrap().status_timer = 1000;
    turn_of(&mut w, &Events::default(), &mut hooks, 67);
    assert_eq!(
        hooks.calls,
        ["move", "needs", "checks", "behaviour", "goals"]
    );
    // The remainder is kept: 1067 % 1000.
    assert_eq!(w.units.get(1).unwrap().status_timer, 67);
    turn_of(&mut w, &Events::default(), &mut hooks, 67);
    assert_eq!(hooks.calls.len(), 5 + 3, "no status tick, the rest runs");
}

#[test]
fn the_wait_stops_the_behaviour_and_the_goals_but_not_the_status_tick() {
    let mut w = world(&[("animals", 1)]);
    let mut hooks = Recorder::default();
    let unit = w.units.get_mut(1).unwrap();
    (unit.wait, unit.status_timer) = (100, 990);
    turn_of(&mut w, &Events::default(), &mut hooks, 67);
    assert_eq!(hooks.calls, ["move", "needs", "checks"]);
    assert_eq!(w.units.get(1).unwrap().wait, 33);
    hooks.calls.clear();
    // The step on which the wait reaches 0 or below still skips them.
    turn_of(&mut w, &Events::default(), &mut hooks, 67);
    assert_eq!(hooks.calls, ["move"]);
    assert_eq!(w.units.get(1).unwrap().wait, -34);
    hooks.calls.clear();
    turn_of(&mut w, &Events::default(), &mut hooks, 67);
    assert_eq!(hooks.calls, ["move", "behaviour", "goals"]);
    assert_eq!(w.units.get(1).unwrap().wait, -34, "never counted below 0");
}

#[test]
fn a_carried_or_repositioning_unit_skips_the_move_the_status_and_the_goals() {
    for flag in 0..2 {
        let mut w = world(&[("animals", 1)]);
        let mut hooks = Recorder::default();
        let unit = w.units.get_mut(1).unwrap();
        (unit.carried, unit.repositioning) = (flag == 0, flag == 1);
        (unit.status_timer, unit.wait, unit.moving, unit.anim_speed) = (999, 500, true, 50);
        let mut events = Events::default();
        events.post(event(1, UNIT_CHANNEL));
        events.age();
        turn_of(&mut w, &events, &mut hooks, 67);
        assert!(hooks.calls.is_empty(), "flag {flag}");
        let unit = w.units.get(1).unwrap();
        assert_eq!((unit.status_timer, unit.wait), (999, 500), "flag {flag}");
        // The animation still runs, and the events were still read and discarded.
        assert_eq!(hooks.advances, [33], "flag {flag}");
        assert!(unit.events.is_empty());
        assert!(status_lines(&mut w).is_empty());
    }
}

#[test]
fn the_animation_of_a_moving_unit_is_scaled_by_its_speed_percentage() {
    let mut w = world(&[("animals", 1)]);
    let mut hooks = Recorder::default();
    let turns = |w: &mut World, hooks: &mut Recorder| {
        turn_of(w, &Events::default(), hooks, 67);
        hooks.advances.iter().sum::<u32>()
    };
    assert_eq!(turns(&mut w, &mut hooks), 67, "not moving: the step");
    let unit = w.units.get_mut(1).unwrap();
    (unit.moving, unit.anim_speed) = (true, 150);
    assert_eq!(turns(&mut w, &mut hooks), 67 + 100);
    w.units.get_mut(1).unwrap().anim_speed = 0;
    assert_eq!(turns(&mut w, &mut hooks), 167);
    // A huge percentage and a huge step wrap, not panic.
    w.units.get_mut(1).unwrap().anim_speed = u16::MAX;
    turn_of(&mut w, &Events::default(), &mut hooks, u32::MAX);
}

#[test]
fn a_unit_hears_the_events_of_its_channel_for_the_length_of_its_turn() {
    let mut w = world(&[("animals", 1), ("keeper", 2)]);
    let mut events = Events::default();
    for target in [0, 1, 2, 3] {
        events.post(event(target, UNIT_CHANNEL));
        events.post(event(target, KEEPER_CHANNEL));
    }
    events.age();
    let mut heard = |at: usize| {
        let mut hooks = Recorder::default();
        turn_at(&mut w, &events, &mut hooks, 67, at);
        hooks.heard
    };
    assert_eq!(
        heard(0),
        [2],
        "the animal: for it and for all, channel 0x55"
    );
    assert_eq!(heard(1), [2], "the keeper: channel 0x4b");
    assert!(w.units.iter().all(|u| u.events.is_empty()));
    // The list grows: every matching event of the buffer is read.
    let mut many = Events::default();
    (0..250).for_each(|_| many.post(event(1, UNIT_CHANNEL)));
    many.age();
    let mut hooks = Recorder::default();
    turn_of(&mut w, &many, &mut hooks, 67);
    assert_eq!(hooks.heard, [250]);
}

#[test]
fn an_entity_that_is_not_a_unit_or_not_in_the_list_is_left_alone() {
    let mut w = world(&[("objects", 1)]);
    let mut hooks = Recorder::default();
    turn_of(&mut w, &Events::default(), &mut hooks, 67);
    turn_at(&mut w, &Events::default(), &mut hooks, 67, 99);
    assert!(hooks.calls.is_empty());
}

#[test]
fn a_unit_off_the_zoo_is_removed_one_time_in_15_and_the_undo_list_forgets_it() {
    let mut w = world(&[("animals", 1)]);
    // One placed outside the map and one inside the zoo.
    let out = w.add(entity("guests", 0, (-64, 10)), None).unwrap();
    let inside = w.add(entity("guests", 0, (70, 70)), None).unwrap();
    assert!(w.can_undo());
    let mut s = sim();
    let mut gone_at = None;
    for step in 1..=300 {
        s.step(&mut w, 67, |_, _, _| {});
        if gone_at.is_none() && w.find(out).is_none() {
            gone_at = Some(step);
        }
    }
    // The flag is set in one step and the unit leaves in the next removal pass.
    assert!(gone_at.is_some_and(|n| n < 200), "{gone_at:?}");
    assert!(w.find(inside).is_some() && w.find(1).is_some());
    // The undo list keeps the placement of the unit that stayed, and the destroyed one is not
    // brought back nor removed again.
    let undone = w.undo();
    assert!(undone.blocked.is_none());
    assert_eq!(undone.steps.len(), 1, "{:?}", undone.steps);
    assert!(w.find(inside).is_none());
}

#[test]
fn the_simulation_draws_once_per_unit_that_runs_its_goals() {
    let mut w = world(&[("animals", 1), ("guests", 2), ("keeper", 3)]);
    let (mut a, mut b) = (sim(), sim());
    a.step(&mut w, 67, |_, _, _| {});
    // The animal (of no known subtype: a young one) draws its sex and its sixteen counters on
    // its first turn, the keeper its two colours and two counters, and the zoo grounds' rating
    // timer draws once (it starts dirty).
    (0..3 + 17 + 4 + 1).for_each(|_| _ = b.rng.rand());
    assert_eq!(a.rng.state(), b.rng.state());
    // A waiting unit draws nothing.
    w.units.get_mut(2).unwrap().wait = 1000;
    a.step(&mut w, 67, |_, _, _| {});
    (0..2).for_each(|_| _ = b.rng.rand());
    assert_eq!(a.rng.state(), b.rng.state());
}

#[test]
fn the_hash_covers_the_movement_of_the_units() {
    let mut w = world(&[("animals", 1), ("guests", 2)]);
    let base = w.units.hash();
    assert_eq!(base, world(&[("animals", 1), ("guests", 2)]).units.hash());
    let changes: [fn(&mut Unit); 18] = [
        |u| u.speed = 3,
        |u| u.moving = true,
        |u| u.anim_speed = 99,
        |u| u.destination = Some((1, 2, 3)),
        |u| u.animation.name = "walk".into(),
        |u| u.animation.frame = 1,
        |u| u.status_timer += 1,
        |u| u.wait = 1,
        |u| u.carried = true,
        |u| u.events.push(event(1, 0)),
        |u| u.walking = true,
        |u| u.band = movement::Band::Fast,
        |u| u.flip = true,
        |u| u.gate = Some(7),
        |u| u.levels.push(2),
        |u| u.tank = 4,
        |u| u.vertical = -1,
        |u| u.target = crate::behaviour::Target::Dive,
    ];
    for (i, change) in changes.iter().enumerate() {
        let mut other = world(&[("animals", 1), ("guests", 2)]);
        change(other.units.get_mut(2).unwrap());
        assert_ne!(other.units.hash(), base, "change {i}");
    }
    w.units.get_mut(1).unwrap().repositioning = true;
    assert_ne!(w.units.hash(), base);
    // The period and the list are in it too.
    let mut other = world(&[("animals", 1)]);
    assert_ne!(other.units.hash(), base);
    let before = other.units.hash();
    other.units.set_period(10);
    assert_ne!(other.units.hash(), before);
    // And the state of the path finder.
    let before = other.units.hash();
    other.units.finder.tick(7);
    assert_ne!(other.units.hash(), before);
}

#[test]
fn the_hash_covers_the_needs_of_the_animals_and_nothing_else_of_them() {
    let base = world(&[("animals", 1), ("guests", 2)]).units.hash();
    let hash = |change: fn(&mut Animal)| {
        let mut other = world(&[("animals", 1), ("guests", 2)]);
        let Part::Animal(a) = &mut other.units.get_mut(1).unwrap().part else {
            panic!("not an animal");
        };
        change(a);
        other.units.hash()
    };
    let changes: [fn(&mut Animal); 10] = [
        |a| a.happiness = 1,
        |a| a.hunger = 1,
        |a| a.health = 1,
        |a| a.energy = 1,
        |a| a.dung = 1,
        |a| a.life = 1,
        |a| a.egg = true,
        |a| a.dying = true,
        |a| a.escaped = true,
        |a| a.rampaging = true,
    ];
    for (i, change) in changes.into_iter().enumerate() {
        assert_ne!(hash(change), base, "field {i}");
    }
    // A field left out of the projection does not move it.
    assert_eq!(hash(|a| a.counters[15] = 1), base);
    assert_eq!(hash(|a| a.crate_count = 1), base);
    assert_eq!(hash(|a| a.walk = Some(crate::zoo::Escape::default())), base);
    let mut other = world(&[("animals", 1), ("guests", 2)]);
    other.units.get_mut(1).unwrap().started = true;
    assert_ne!(other.units.hash(), base, "started");
    other.units.get_mut(1).unwrap().started = false;
    other.units.get_mut(1).unwrap().typed = true;
    assert_ne!(other.units.hash(), base, "typed");
}

#[test]
fn the_hash_of_the_units_follows_the_counters_of_a_staff_member() {
    let mut w = world(&[("keeper", 1)]);
    let before = w.units.hash();
    let Part::Staff(staff) = &mut w.units.get_mut(1).unwrap().part else {
        panic!("not staff");
    };
    staff.work = 3;
    let work = w.units.hash();
    assert_ne!(work, before);
    let Part::Staff(staff) = &mut w.units.get_mut(1).unwrap().part else {
        panic!("not staff");
    };
    staff.chase = 1;
    assert_ne!(w.units.hash(), work);
}

/// A type with one ambient set, `fPlayWithSound(a, purr)` then `fPlay(b)`, for every unit; `a`
/// has three frames of 100 ms and `b` one.
struct Script(Vec<crate::behaviour::Call>, Vec<crate::behaviour::Row>);

impl Script {
    fn new() -> Script {
        let calls = ["fPlayWithSound(a,purr)", "fPlay(b)"];
        let row = |set: &str| crate::behaviour::Row {
            lo: -100,
            hi: 100,
            set: set.into(),
        };
        Script(
            (calls.iter())
                .map(|c| crate::behaviour::parse_call(c.as_bytes()).unwrap().0)
                .collect(),
            vec![row("bAmbient")],
        )
    }
}

impl Data for Script {
    fn set(&self, _: &Entity, name: &str) -> Option<&[crate::behaviour::Call]> {
        (name == "bAmbient").then_some(self.0.as_slice())
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[crate::behaviour::Row] {
        &self.1
    }
    fn timing(&self, _: &Entity, name: &str) -> Option<crate::behaviour::Timing> {
        let count = match name {
            "a" => 3,
            "b" => 1,
            _ => return None,
        };
        Some(crate::behaviour::Timing { count, delay: 100 })
    }
    fn number(&self, unit: &Entity, number: &crate::behaviour::Number) -> i32 {
        NoData.number(unit, number)
    }
    fn sound(&self, _: &Entity, key: &str) -> Option<Heard> {
        Some(Heard {
            file: format!("{key}.wav"),
            base: 0,
            voice: key.into(),
        })
    }
}

#[test]
fn a_unit_runs_its_set_after_the_animation_and_tells_the_log_and_the_app() {
    let mut w = world(&[("animals", 1), ("guests", 2), ("keeper", 3)]);
    let (mut s, data) = (sim(), Script::new());
    w.units.get_mut(3).unwrap().carried = true;
    w.units.take_lines();
    let mut frames = Vec::new();
    for _ in 0..5 {
        s.step_with(&data, &Catalogue::default(), &mut w, 100, |_, _, _| {});
        let a = &w.units.get(1).unwrap().animation;
        frames.push((a.name.clone(), a.shown()));
    }
    let want = |n: &str, f| (n.to_string(), f);
    assert_eq!(
        frames,
        [
            want("idle", 0),
            want("a", 0),
            want("a", 1),
            want("a", 2),
            want("b", 0)
        ]
    );
    let lines: Vec<_> = (w.units.take_lines().into_iter())
        .filter(|l| l.starts_with("set 1 ") || l.starts_with("anim 1 "))
        .collect();
    assert_eq!(
        lines,
        ["anim 1 idle", "set 1 bAmbient", "anim 1 a", "anim 1 b"]
    );
    // The sound of the call is asked once for each unit that runs the set; the unit that is
    // carried runs no set but its animation shows the base.
    let mut sounds = w.units.take_sounds();
    sounds.sort();
    let files: Vec<_> = sounds
        .iter()
        .map(|(id, h)| (*id, h.file.as_str()))
        .collect();
    assert_eq!(files, [(1, "purr.wav"), (2, "purr.wav")]);
    let carried = w.units.get(3).unwrap();
    assert_eq!(
        (carried.runner.top(), carried.animation.name.as_str()),
        (None, "idle")
    );
    // The state hash follows the frame.
    let before = w.units.hash();
    s.step_with(&data, &Catalogue::default(), &mut w, 100, |_, _, _| {});
    assert_ne!(w.units.hash(), before);
}

/// One set of one call each, by name.
struct Once(Vec<(&'static str, Vec<crate::behaviour::Call>)>);

impl Once {
    fn new(sets: &[(&'static str, &str)]) -> Once {
        let call = |c: &str| crate::behaviour::parse_call(c.as_bytes()).unwrap().0;
        Once(
            sets.iter()
                .map(|&(name, c)| (name, vec![call(c)]))
                .collect(),
        )
    }
}

impl Data for Once {
    fn set(&self, _: &Entity, name: &str) -> Option<&[crate::behaviour::Call]> {
        let (_, calls) = self.0.iter().find(|(n, _)| *n == name)?;
        Some(calls.as_slice())
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[crate::behaviour::Row] {
        &[]
    }
    fn timing(&self, _: &Entity, _: &str) -> Option<crate::behaviour::Timing> {
        None
    }
    fn number(&self, unit: &Entity, number: &crate::behaviour::Number) -> i32 {
        NoData.number(unit, number)
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<Heard> {
        None
    }
}

/// Runs the behaviour hook of unit `id` once with its set `name` requested.
fn behave(w: &mut World, data: &Once, id: u32, name: &str) {
    let at = w.find(id).unwrap();
    let kind = w.units.get(id).unwrap().kind;
    let mut unit = std::mem::replace(w.units.get_mut(id).unwrap(), Unit::new(id, kind));
    unit.runner.queue(name, true, true);
    let (mut rng, mut removals) = (Rng::default(), Vec::new());
    let mut ctx = Ctx {
        rng: &mut rng,
        events: &Events::default(),
        removals: &mut removals,
        step: 100,
        cat: &crate::entity::Catalogue::default(),
    };
    Standard { data }.behaviour(&mut unit, w, &mut ctx, at);
    *w.units.get_mut(id).unwrap() = unit;
}

/// Moves the entity `id` to the centre of tile `(tx, ty)`.
fn put(w: &mut World, id: u32, tile: (i32, i32)) {
    let at = w.find(id).unwrap();
    let c = w.zoo.entities[at].common.as_mut().unwrap();
    (c.x, c.y) = crate::unit::testing::centre(tile);
}

fn rotation(w: &World, id: u32) -> u32 {
    let at = w.find(id).unwrap();
    w.zoo.entities[at].common.as_ref().unwrap().rotation
}

#[test]
fn a_guest_or_staff_member_turns_at_once_to_its_own_target_tile() {
    // The keeper 3 and the guest 2 stand on tile (1, 1); the scenery 50 is the guest's target.
    let mut w = world(&[("guests", 2), ("keeper", 3), ("building", 50)]);
    let data = Once::new(&[("bFace", "fFaceTowardTarget()")]);
    // East (2), south (4) and west (6) of the keeper's own target tile.
    for (tile, facing) in [((2, 1), 2), ((1, 2), 4), ((0, 1), 6)] {
        let target = w.tile_index(tile.0, tile.1).unwrap();
        w.units.get_mut(3).unwrap().part = Part::Staff(Staff {
            target: Some(target),
            ..Staff::default()
        });
        behave(&mut w, &data, 3, "bFace");
        assert_eq!(rotation(&w, 3), facing, "keeper towards {tile:?}");
    }
    // The guest faces the scenery of its goal, its object's tile, the same way.
    for (tile, facing) in [((2, 1), 2), ((1, 2), 4)] {
        put(&mut w, 50, tile);
        let record = &mut w.units.get_mut(2).unwrap().goal.record;
        (record.task, record.target.id) = (crate::goal::SCENERY, 50);
        behave(&mut w, &data, 2, "bFace");
        assert_eq!(rotation(&w, 2), facing, "guest towards {tile:?}");
    }
    // A staff member with no target tile keeps its facing, and the call is done all the same.
    w.units.get_mut(3).unwrap().part = Part::Staff(Staff::default());
    behave(&mut w, &data, 3, "bFace");
    assert_eq!(rotation(&w, 3), 6);
}

#[test]
fn a_halt_clears_the_moving_flag_of_a_unit_and_keeps_its_path() {
    let mut w = world(&[("animals", 1)]);
    let path = vec![w.tile_index(1, 1).unwrap(), w.tile_index(2, 1).unwrap()];
    let unit = w.units.get_mut(1).unwrap();
    (unit.moving, unit.path) = (true, path.clone());
    let data = Once::new(&[("bHalt", "fHalt()")]);
    behave(&mut w, &data, 1, "bHalt");
    let unit = w.units.get(1).unwrap();
    assert!(!unit.moving);
    assert_eq!(unit.path, path);
}

#[test]
fn colours_are_drawn_per_group_modulo_its_palettes_and_always_the_same_number_of_draws() {
    let ini = "[m/colorrep]\nreplace=cr_hair\nreplace=cr_skin\nreplace=cr_none\n\
               [cr_hair]\npal=a\npal=b\npal=c\n[cr_skin]\npal=a\npal=b\n";
    let make = |subtype: &[u8]| EntityType {
        class: "keeper",
        type_name: b"keeper".to_vec(),
        subtype: subtype.to_vec(),
        file: None,
        config: Some(std::sync::Arc::new(Config::parse(ini.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [1, 1, 1],
        height: 0,
    };
    let (ty, other) = (make(b"m"), make(b"f"));
    assert_eq!(ty.palettes(), [3, 2, 0]);
    assert!(other.palettes().is_empty());
    // More groups than the type has, none, no type and a type with no such section all draw the
    // number asked for.
    for (ty, count) in [
        (Some(&ty), 3),
        (Some(&ty), 5),
        (Some(&ty), 0),
        (None, 2),
        (Some(&other), 2),
    ] {
        let (mut rng, mut expect) = (Rng::default(), Rng::default());
        let palettes = ty.map(EntityType::palettes).unwrap_or_default();
        let want: Vec<u8> = (0..count)
            .map(|g| {
                let r = expect.rand() as usize;
                palettes
                    .get(g)
                    .filter(|&&n| n > 0)
                    .map_or(0, |&n| (r % n) as u8)
            })
            .collect();
        assert_eq!(draw_colours(ty, count, &mut rng), want);
        assert_eq!(rng, expect);
    }
}

mod live_set;
