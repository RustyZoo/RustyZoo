//! A maintenance worker's round on a synthetic map: a full trash can, litter and a worn fence.

use crate::animal::dirt;
use crate::behaviour::{Call, Data, Heard, Number, Row, Timing, parse_call};
use crate::config::Config;
use crate::entity::{Catalogue, EntityType};
use crate::guest::litter::{CAN_FILLING, DROPPED};
use crate::money::Money;
use crate::sim::Sim;
use crate::unit::testing::{centre, entity, rates, world_with};
use crate::world::{Decay, World, piece_event};
use crate::zoo::{Clocks, Entity};
use std::collections::HashMap;
use std::sync::Arc;

const WORKER: u32 = 1;
const CAN: u32 = 11;
const FENCE: u32 = 12;
const CAN_TILE: (i32, i32) = (3, 9);
const LITTER_TILE: (i32, i32) = (8, 9);
const FENCE_TILE: (i32, i32) = (14, 9);

fn ty(class: &'static str, name: &str, sub: &str, keys: &str) -> EntityType {
    EntityType {
        class,
        type_name: name.as_bytes().to_vec(),
        subtype: sub.as_bytes().to_vec(),
        file: None,
        config: Some(Arc::new(Config::parse(keys.as_bytes()).unwrap())),
        name_id: 0,
        footprint: [2, 2, 1],
        height: 0,
    }
}

fn catalogue(worker: &str) -> Catalogue {
    let can = "[Characteristics/Integers]\ncCapacity=1000\ncUserStaysOutside=1\n\
        [Removes]\ntrash\n[Satisfies]\nbuilding\ntrash\n[slots]\nname=slot\n\
        [slot]\nslotpos=0\nslotpos=0\nentrpos=0\nentrpos=48\nexitpos=0\nexitpos=48\ncapacity=1000\n";
    let fence =
        "[Characteristics/Integers]\ncStrength=300\ncLife=3\ncDecayedLife=2\ncDecayDelta=100\n";
    let mut cat = Catalogue::default();
    for t in [
        ty("maint", "maint", "m", worker),
        ty("building", "can", "", can),
        EntityType {
            footprint: [1, 1, 1],
            ..ty("objects", "other", "trash", "")
        },
        ty("fences", "iron", "f", fence),
    ] {
        cat.push(t);
    }
    cat
}

/// The sets of the worker: a walk that ends on arrival, the work lasts a few frames.
struct Sets(HashMap<String, Vec<Call>>);

fn sets() -> Sets {
    let call = |c: &str| vec![parse_call(c.as_bytes()).unwrap().0];
    let walk = ["bWalk", "bRun"].map(|n| (n, call("fWalk(0,0)")));
    let play = ["bEmpty", "bSweep", "bFix"].map(|n| (n, call("fPlay(a)")));
    Sets(HashMap::from_iter(
        walk.into_iter().chain(play).map(|(n, c)| (n.into(), c)),
    ))
}

impl Data for Sets {
    fn set(&self, _: &Entity, name: &str) -> Option<&[Call]> {
        self.0.get(name).map(Vec::as_slice)
    }
    fn ambient(&self, _: &Entity, _: usize) -> &[Row] {
        &[]
    }
    fn timing(&self, _: &Entity, name: &str) -> Option<Timing> {
        let count = match name {
            "a" => 4,
            "idle" | "walk" | "trot" | "run" => 1,
            _ => return None,
        };
        Some(Timing { count, delay: 100 })
    }
    fn number(&self, _: &Entity, number: &Number) -> i32 {
        rates(number)
    }
    fn sound(&self, _: &Entity, _: &str) -> Option<Heard> {
        None
    }
}

struct Game {
    w: World,
    sim: Sim,
    money: Money,
    cat: Catalogue,
    data: Sets,
    log: Vec<String>,
    started: bool,
}

/// The characteristics of a worker type with the given keys.
fn worker(keys: &str) -> String {
    format!("[m/Characteristics/Integers]\n{keys}\n")
}

impl Game {
    /// A 22 by 12 map with the worker at (1, 9), a trash can at (3, 9) and a fence piece that wears
    /// out in 20 s of its own at (14, 9). Fixed seed.
    fn new(keys: &str) -> Game {
        Game::with(keys, &[(WORKER, (1, 9))], false)
    }

    /// The same with the workers `(id, tile)`, in this order of the entity list, and a second
    /// piece (13) on the east edge of the fence's tile.
    fn with(keys: &str, workers: &[(u32, (i32, i32))], side: bool) -> Game {
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(22 * 12, Clocks::default());
        let mut w = world_with((22, 12), |z| {
            for &(id, tile) in workers {
                let mut worker = entity("maint", "maint", id, centre(tile), 2);
                worker.subtype = "m".into();
                z.entities.push(worker);
            }
            z.entities
                .push(entity("building", "can", CAN, centre(CAN_TILE), 4));
            let mut fence = entity("fences", "iron", FENCE, centre(FENCE_TILE), 0);
            fence.subtype = "f".into();
            z.entities.push(fence);
            if side {
                let mut piece = entity("fences", "iron", 13, centre(FENCE_TILE), 2);
                piece.subtype = "f".into();
                z.entities.push(piece);
            }
        });
        let cat = catalogue(&worker(keys));
        w.set_decay(Decay {
            min: 20_000,
            max: 20_000,
        });
        for id in [FENCE, 13] {
            w.heal(&cat, id, &mut || 0);
        }
        Game {
            w,
            sim,
            money: Money::default(),
            cat,
            data: sets(),
            log: Vec::new(),
            started: false,
        }
    }

    fn tile((x, y): (i32, i32)) -> usize {
        y as usize * 22 + x as usize
    }

    /// The can holds `n` pieces; the event a guest posts for the 13th piece is due at the first step.
    fn fill(&mut self, n: u32) {
        self.w.units.buildings.entry(CAN).or_default();
        crate::building::set_fill(&mut self.w, CAN, n);
        let tile = Game::tile(CAN_TILE) as u32;
        self.sim.events.post(piece_event(CAN, CAN_FILLING, tile));
    }

    /// A piece of litter on `tile`, with its event due at the first step when `told`.
    fn litter(&mut self, tile: (i32, i32), told: bool) {
        let ty = self.cat.find("objects", b"other", b"trash").unwrap();
        let names = ("objects", "other", "trash");
        assert!(dirt::put(&mut self.w, ty, names, tile, 0, 0));
        if told {
            let event = piece_event(CAN, DROPPED, Game::tile(tile) as u32);
            self.sim.events.post(event);
        }
    }

    /// Runs `steps` steps of 67 ms; the events posted before the first are due at it.
    fn run(&mut self, steps: usize) {
        if !std::mem::replace(&mut self.started, true) {
            self.sim.events.age();
        }
        for _ in 0..steps {
            let (w, money) = (&mut self.w, &mut self.money);
            self.sim
                .step_game(&self.data, &self.cat, w, money, 67, |_, _, _| {});
            let (units, staff) = (w.units.take_lines(), w.units.take_staff_lines());
            let world = w.units.take_world_lines();
            self.log
                .extend(units.into_iter().map(|l| format!("unit {l}")));
            self.log
                .extend(staff.into_iter().map(|l| format!("staff {l}")));
            self.log
                .extend(world.into_iter().map(|l| format!("world {l}")));
        }
    }

    /// The lines of the log that start with one of the words, in order.
    fn lines(&self, words: &[&str]) -> Vec<&str> {
        let wanted = |l: &&String| words.iter().any(|w| l.starts_with(w));
        self.log.iter().filter(wanted).map(String::as_str).collect()
    }

    fn jobs(&self) -> Vec<&str> {
        let jobs = self.lines(&["staff job"]);
        jobs.iter().map(|l| l.rsplit(' ').next().unwrap()).collect()
    }

    fn pieces(&self) -> usize {
        self.w
            .zoo
            .entities
            .iter()
            .filter(|e| e.subtype == "trash")
            .count()
    }

    fn piece(&mut self, id: u32) -> &mut crate::zoo::Fence {
        let at = self.w.find(id).unwrap();
        let c = self.w.zoo.entities[at].common.as_mut().unwrap();
        c.fence.as_mut().unwrap()
    }

    fn staff(&mut self, id: u32) -> &mut crate::unit::Staff {
        match &mut self.w.units.get_mut(id).unwrap().part {
            crate::unit::Part::Staff(s) => s,
            _ => panic!("not staff"),
        }
    }
}

#[test]
fn a_worker_empties_the_full_can_sweeps_the_litter_and_mends_the_worn_fence() {
    let mut g = Game::new("");
    g.fill(13);
    g.litter(LITTER_TILE, true);
    assert_eq!(g.pieces(), 1);
    // The fence wears at 20 s, while the worker is on his way to the litter; the repair is done
    // before it wears again.
    g.run(1000);
    assert_eq!(
        g.jobs(),
        [
            "TrashCan",
            "EmptyTrash",
            "none",
            "TrashPile",
            "SweepTrash",
            "none",
            "GoToFence",
            "FixFence",
            "none"
        ],
        "{:?}",
        g.lines(&["staff", "world"])
    );
    // The can is empty, its picture with it; the litter is gone; the fence has its full life back.
    let can = g.w.find(CAN).unwrap();
    assert_eq!(g.w.units.buildings[&CAN].fill, 0);
    assert_eq!(g.w.zoo.entities[can].common.as_ref().unwrap().food, 0);
    assert_eq!(g.pieces(), 0);
    let fence = *g.piece(FENCE);
    assert_eq!((fence.life, fence.strength), (3, 300));
    assert_eq!(g.lines(&["world fence"]), ["world fence 12 3"]);
    assert!(g.w.units.pool.tasks.is_empty());
}

#[test]
fn a_broken_fence_is_run_to_before_a_nearer_can() {
    let mut g = Game::new("");
    g.fill(13);
    g.piece(FENCE).life = 0;
    let tile = Game::tile(FENCE_TILE) as u32;
    g.sim.events.post(piece_event(FENCE, 10034, tile));
    g.run(1000);
    assert_eq!(
        g.jobs()[..4],
        ["GoToFence", "FixFence", "none", "TrashCan"],
        "{:?}",
        g.lines(&["staff", "unit set"])
    );
    assert_eq!(
        g.lines(&["unit set 1 bRun"]).len(),
        1,
        "run to the urgent one"
    );
    assert_eq!(g.lines(&["world fence"]), ["world fence 12 3"]);
}

#[test]
fn a_task_with_nothing_left_to_do_is_dropped() {
    let mut g = Game::new("");
    // The can is emptied before the worker comes; the fence was mended since its event.
    g.fill(13);
    crate::building::set_fill(&mut g.w, CAN, 0);
    let tile = Game::tile(FENCE_TILE) as u32;
    g.sim.events.post(piece_event(FENCE, 10033, tile));
    g.run(200);
    assert_eq!(g.jobs(), ["TrashCan", "none", "GoToFence", "none"]);
    assert!(g.w.units.pool.tasks.is_empty());
    assert!(g.lines(&["world fence"]).is_empty());
}

#[test]
fn a_worker_skips_the_duties_switched_off_and_the_tiles_he_could_not_reach() {
    // Duty kind 0 (the cans) switched off: the task waits in the pool.
    let mut g = Game::new("");
    g.fill(13);
    g.staff(WORKER).off[0] = true;
    g.run(200);
    assert!(g.jobs().is_empty() && g.w.units.pool.tasks.len() == 1);
    // A listed tile is skipped until the lists are emptied: at once for an interval of 0 or below,
    // after the interval that is set (10 s without one) and the wander he is in, never in the time
    // of a huge one.
    for (key, first) in [
        ("cClearInvalidListInterval=0", Some(1..=1)),
        ("cClearInvalidListInterval=-5", Some(1..=1)),
        ("cClearInvalidListInterval=1000000000", None),
        ("", Some(149..=400)),
        ("cClearInvalidListInterval=1000", Some(15..=400)),
    ] {
        let mut g = Game::new(key);
        g.fill(13);
        g.staff(WORKER).lost_tiles.push(Game::tile(CAN_TILE));
        let mut found = None;
        for step in 1..=400 {
            g.run(1);
            if found.is_none() && !g.jobs().is_empty() {
                found = Some(step);
            }
        }
        let right = match (found, first) {
            (None, None) => true,
            (Some(n), Some(range)) => range.contains(&n),
            _ => false,
        };
        assert!(right, "{key}: {found:?}");
    }
}

#[test]
fn a_nearer_worker_takes_the_task_of_a_worker_farther_away() {
    const OTHER: u32 = 2;
    let mut g = Game::with("", &[(OTHER, (10, 9)), (WORKER, (4, 9))], false);
    g.fill(13);
    g.run(2);
    let holder =
        g.w.units
            .pool
            .tasks
            .iter()
            .map(|t| t.worker)
            .collect::<Vec<_>>();
    assert_eq!(holder, [WORKER]);
    assert_eq!(g.staff(WORKER).task, 1);
    assert_eq!(g.staff(OTHER).task, 0);
    assert_eq!(g.w.units.get(OTHER).unwrap().goal.record.job, 0);
    // A worker that leaves gives the task back.
    crate::staff::leaves(&mut g.w, WORKER);
    assert_eq!(g.w.units.pool.tasks[0].worker, 0);
    assert_eq!(g.staff(WORKER).task, 0);
    assert_eq!(g.w.units.get(WORKER).unwrap().goal.record.task, 0);
}

#[test]
fn the_sweep_covers_the_sides_of_the_tile_and_with_a_radius_its_neighbours() {
    for (keys, left) in [
        ("", 1),
        ("cCleanTrashRadius=1", 0),
        ("cCleanTrashRadius=-1", 1),
    ] {
        let mut g = Game::new(keys);
        g.litter(LITTER_TILE, true);
        g.litter((8, 8), false);
        g.litter((9, 10), false);
        g.run(900);
        assert_eq!(g.pieces(), left, "{keys}: {:?}", g.jobs());
    }
}

#[test]
fn a_repair_with_a_modifier_mends_the_lines_beside_too() {
    for (keys, mended) in [
        ("", 1),
        ("cFixFenceModifier=1", 2),
        ("cFixFenceModifier=-1", 1),
    ] {
        let mut g = Game::with(keys, &[(WORKER, (1, 9))], true);
        // The pieces do not wear again in the time of the test.
        let slow = Decay {
            min: 600_000,
            max: 600_000,
        };
        g.w.set_decay(slow);
        for id in [FENCE, 13] {
            g.w.heal(&g.cat, id, &mut || 0);
        }
        g.piece(13).life = 2;
        g.piece(FENCE).life = 2;
        let tile = Game::tile(FENCE_TILE) as u32;
        g.sim.events.post(piece_event(FENCE, 10033, tile));
        g.run(1000);
        assert_eq!(g.lines(&["world fence"]).len(), mended, "{keys}");
    }
}
