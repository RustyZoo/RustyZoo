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
const COMPOST: u32 = 14;
const COMPOST_TILE: (i32, i32) = (18, 2);
const CAN_TILE: (i32, i32) = (3, 9);
const LITTER_TILE: (i32, i32) = (8, 9);
const FENCE_TILE: (i32, i32) = (14, 9);
/// The pen of the emptied exhibit: its tiles and its gate column.
const PEN: (i32, i32, i32, i32, i32) = (3, 2, 7, 5, 5);

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
        ty(
            "building",
            "compost",
            "",
            "[Characteristics/Integers]\ncCommerce=1\n[Characteristics/Floats]\ncDefaultCost=50\n\
            [Satisfies]\ncompost\n",
        ),
        EntityType {
            footprint: [1, 1, 1],
            ..ty(
                "objects",
                "other",
                "poo",
                "[Characteristics/Integers]\ncStink=1\n",
            )
        },
        EntityType {
            footprint: [1, 1, 1],
            ..ty(
                "objects",
                "other",
                "dinopoo",
                "[Characteristics/Integers]\ncStink=1\n",
            )
        },
        EntityType {
            footprint: [1, 1, 1],
            ..ty("food", "meat", "", "[Characteristics/Integers]\ncFood=1\n")
        },
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
    let play = ["bEmpty", "bSweep", "bFix", "bBag", "bBagDino"].map(|n| (n, call("fPlay(a)")));
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

/// The second fence piece (13) on the east edge of the fence's tile.
fn side_piece() -> Entity {
    let mut piece = entity("fences", "iron", 13, centre(FENCE_TILE), 2);
    piece.subtype = "f".into();
    piece
}

/// A pen of exhibit fences over the tiles from (x0, y0) to (x1, y1), its gate on the south side
/// at column `gate`: an exhibit of its own with no animal in it.
fn pen((x0, y0, x1, y1, gate): (i32, i32, i32, i32, i32)) -> Vec<Entity> {
    let mut found = Vec::new();
    for (x, y) in (x0..=x1).flat_map(|x| (y0..=y1).map(move |y| (x, y))) {
        for (d, outside) in [(0, y == y0), (2, x == x1), (4, y == y1), (6, x == x0)] {
            if outside {
                let id = 200 + found.len() as u32;
                let mut piece = entity("fences", "exhibit", id, centre((x, y)), d);
                if (x, y, d) == (gate, y1, 4) {
                    piece.subtype = "g".into();
                }
                found.push(piece);
            }
        }
    }
    found
}

/// The characteristics of a worker type with the given keys.
fn worker(keys: &str) -> String {
    format!("[m/Characteristics/Integers]\n{keys}\n")
}

impl Game {
    /// A 22 by 12 map with the worker at (1, 9), a trash can at (3, 9) and a fence piece that wears
    /// out in 20 s of its own at (14, 9). Fixed seed.
    fn new(keys: &str) -> Game {
        Game::with(keys, &[(WORKER, (1, 9))], &[])
    }

    /// The same with the workers `(id, tile)`, in this order of the entity list, and the extra
    /// entities (a pen, a second piece).
    fn with(keys: &str, workers: &[(u32, (i32, i32))], extra: &[Entity]) -> Game {
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
            z.entities.push(entity(
                "building",
                "compost",
                COMPOST,
                centre(COMPOST_TILE),
                4,
            ));
            z.entities.extend(extra.iter().cloned());
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
fn the_can_is_emptied_from_the_tile_the_worker_walked_to() {
    // The path from (1, 9) to the can at (3, 9) crosses (2, 9), within one tile of the can: the
    // check done of the job waits until he stands on the tile he walks to (staff 8.4).
    let mut g = Game::new("");
    g.fill(13);
    for _ in 0..1000 {
        g.run(1);
        if g.jobs().contains(&"EmptyTrash") {
            break;
        }
    }
    let at = g.w.find(WORKER).unwrap();
    let c = g.w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), CAN_TILE);
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
fn a_worn_fence_is_mended_from_the_worker_side_of_its_lowest_piece() {
    // The lowest piece (15) is on the east edge of the tile west of the task's tile (14, 9); that
    // tile is open and on the grounds, so the worker walks to (13, 9), not to the task's tile.
    let mut piece = entity("fences", "iron", 15, centre((13, 9)), 2);
    piece.subtype = "f".into();
    let mut g = Game::with("", &[(WORKER, (1, 9))], &[piece]);
    g.w.heal(&g.cat, 15, &mut || 0);
    (g.piece(15).strength, g.piece(15).life) = (100, 2);
    // The fence of the task's tile keeps its life: only the piece 15 wears.
    for id in [FENCE, 15] {
        g.piece(id).timers = [600_000; 2];
    }
    let tile = Game::tile(FENCE_TILE) as u32;
    g.sim.events.post(piece_event(FENCE, 10033, tile));
    for _ in 0..1000 {
        g.run(1);
        if !g.lines(&["world fence"]).is_empty() {
            break;
        }
    }
    assert_eq!(
        g.lines(&["world fence"]),
        ["world fence 15 3"],
        "{:?}",
        g.lines(&["staff"])
    );
    assert!(g.jobs().contains(&"FixFence"));
    let at = g.w.find(WORKER).unwrap();
    let c = g.w.zoo.entities[at].common.as_ref().unwrap();
    assert_eq!((c.x.div_euclid(64), c.y.div_euclid(64)), (13, 9));
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
    let mut g = Game::with("", &[(OTHER, (10, 9)), (WORKER, (4, 9))], &[]);
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
        let mut g = Game::with(keys, &[(WORKER, (1, 9))], &[side_piece()]);
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

#[test]
fn the_bear_cheat_makes_the_grizzly_bash_without_the_key() {
    use crate::unit::testing::amphibious_world;
    // The bear has no bash strength of its own; the flag gives one to the grizzly only.
    for (name_id, flag, bashes) in [
        (crate::cheat::GRIZZLY, true, true),
        (crate::cheat::GRIZZLY, false, false),
        (crate::cheat::GRIZZLY + 1, true, false),
        (0, true, false),
        (-crate::cheat::GRIZZLY, true, false),
    ] {
        let mut g = Game::new("");
        g.w = amphibious_world(2, &[(5, "bear", (3, 3))]);
        let mut bear = ty("animals", "bear", "", "[Characteristics/Integers]\n");
        bear.name_id = name_id;
        g.cat.push(bear);
        g.w.units.cheat.bears_bash = flag;
        let tank = g.w.exhibits().region(2 * 12 + 2).unwrap();
        g.run(2);
        let got = g.w.exhibits().get(tank).unwrap().rating.bash;
        assert_eq!(got, bashes, "{name_id} {flag}");
    }
}

#[test]
fn a_piece_made_healthy_by_a_worker_sets_the_escape_flag_decayed_or_not() {
    use crate::unit::testing::amphibious_world;
    // The amphibious world with a bear that bashes in the tank. The decayed piece on the north edge
    // of the land exhibit is mended by a worker on the grounds beside it (his side, staff 8.7) and,
    // with the modifier, the wall it shares with the tank, which borders the tank only: sound or
    // decayed, it sets the tank's flag.
    let mut g = Game::new("cFixFenceModifier=1");
    g.w = amphibious_world(2, &[(5, "bear", (3, 3))]);
    let bear = "[Characteristics/Integers]\ncBashStrength=1\n";
    g.cat.push(ty("animals", "bear", "", bear));
    let wall = "[Characteristics/Integers]\ncLife=5\n";
    g.cat.push(ty("tankwall", "iron", "f", wall));
    let fence =
        "[Characteristics/Integers]\ncStrength=300\ncLife=5\ncDecayedLife=2\ncDecayDelta=100\n";
    g.cat.push(ty("fences", "iron", "f", fence));
    g.w.set_decay(Decay {
        min: 600_000,
        max: 600_000,
    });
    let worker = entity("maint", "maint", 0, centre((6, 1)), 2);
    g.w.add(worker, None).unwrap();
    g.w.zoo
        .entities
        .iter_mut()
        .filter(|e| e.class == "maint")
        .for_each(|e| e.subtype = "m".into());
    let tank = g.w.exhibits().region(2 * 12 + 2).unwrap();
    g.run(2);
    assert!(g.w.exhibits().get(tank).unwrap().rating.bash);
    for life in [5, 2] {
        for id in [1062, 1054] {
            let at = g.w.find(id).unwrap();
            g.w.zoo.entities[at].subtype = "f".into();
            g.w.heal(&g.cat, id, &mut || 0);
            g.piece(id).life = if id == 1062 { 2 } else { life };
        }
        g.log.clear();
        g.sim.events.post(piece_event(1062, 10033, 2 * 12 + 6));
        let mut seen = false;
        for _ in 0..1000 {
            g.run(1);
            seen |= g.w.exhibits().get(tank).unwrap().rating.escape;
        }
        assert_eq!(g.lines(&["world fence"]).len(), 2, "life {life}");
        assert!(seen, "life {life}");
    }
}

/// A tank filter on (10, 5) facing the tile beside it with this health; its type and the shape of
/// a one-tile object. Its events are due at the first step.
fn filter(g: &mut Game, health: u32, urgent: bool) -> u32 {
    let mut e = entity("filter", "filter1", 0, centre((10, 5)), 6);
    e.subtype = "f".into();
    e.common.as_mut().unwrap().filter = Some(crate::zoo::Filter {
        health,
        timers: Some([1_000_000, 1_000_000]),
        ..Default::default()
    });
    let shape = crate::world::Shape {
        kind: crate::render::order::Kind::Fixed,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: crate::world::TankKind::NONE,
        walkable: (false, false),
    };
    let id = g.w.add(e, Some(shape)).unwrap();
    let keys = "[Characteristics/Integers]\ncStartingHealth=10\ncDecayedHealth=5\n";
    g.cat.push(ty("filter", "filter1", "f", keys));
    // The workers service filters where the marine data is installed (staff 8.1).
    g.cat.push(ty("keeper", "trainer", "m", ""));
    let kind = [10035, 10036][usize::from(urgent)];
    let event = piece_event(id, kind, Game::tile((10, 5)) as u32);
    g.sim.events.post(event);
    id
}

#[test]
fn a_worker_walks_beside_a_worn_filter_and_services_it() {
    let mut g = Game::new("");
    let id = filter(&mut g, 4, false);
    while !g.jobs().contains(&"FixFilter") && g.log.len() < 100_000 {
        g.run(1);
    }
    // He stands on a side of the filter's tile, not on it.
    let at = g.w.find(WORKER).unwrap();
    let c = g.w.zoo.entities[at].common.as_ref().unwrap();
    let (tile, filter) = ((c.x.div_euclid(64), c.y.div_euclid(64)), (10, 5));
    assert_eq!(
        (tile.0 - filter.0).abs() + (tile.1 - filter.1).abs(),
        1,
        "{tile:?}"
    );
    g.run(1000);
    // The fence of the map wears meanwhile and has its turn after.
    assert_eq!(
        g.jobs()[..3],
        ["GoToFilter", "FixFilter", "none"],
        "{:?}",
        g.lines(&["staff", "world", "unit set"])
    );
    // It has its health back, free of charge.
    let f = g.w.zoo.entities[g.w.find(id).unwrap()]
        .common
        .as_ref()
        .unwrap();
    assert_eq!(f.filter.unwrap().health, 10);
    assert!(
        g.lines(&["world filter"])
            .iter()
            .any(|l| l.ends_with("serviced"))
    );
    assert!(g.w.units.pool.tasks.iter().all(|t| t.job != 26));
}

#[test]
fn a_broken_filter_is_run_to_and_the_duty_switch_keeps_the_others_away() {
    let mut g = Game::new("");
    filter(&mut g, 0, true);
    g.run(1000);
    assert_eq!(g.jobs()[..2], ["GoToFilter", "FixFilter"]);
    // Kind 3 is the worker's fourth duty: switched off, the task waits in the pool.
    let mut g = Game::new("");
    g.staff(WORKER).off[3] = true;
    filter(&mut g, 4, false);
    g.run(400);
    assert!(!g.jobs().contains(&"GoToFilter"), "{:?}", g.jobs());
    assert_eq!(
        g.w.units.pool.tasks.iter().filter(|t| t.job == 26).count(),
        1
    );
}

impl Game {
    /// A pile of `name` (`poo`, `dinopoo`) or a piece of `food` on `tile`, with its event due at
    /// the first step when `told`.
    fn dung(&mut self, tile: (i32, i32), name: &str, told: bool) {
        let (class, kind, sub) = match name {
            "food" => ("food", "meat", ""),
            _ => ("objects", "other", name),
        };
        let ty = self
            .cat
            .find(class, kind.as_bytes(), sub.as_bytes())
            .unwrap();
        assert!(dirt::put(&mut self.w, ty, (class, kind, sub), tile, 0, 0));
        if told {
            let event = piece_event(1, 10032, Game::tile(tile) as u32);
            self.sim.events.post(event);
        }
    }

    /// The fence takes hours to wear, so that only the dung is on the worker's list.
    fn calm(&mut self) {
        let min = 1_000_000_000;
        self.w.set_decay(Decay { min, max: min });
        self.w.heal(&self.cat, FENCE, &mut || 0);
    }

    /// The cash and the construction refunds the money holds.
    fn refunds(&self) -> (f32, f32) {
        let paid = self.money.monthly[crate::money::CONSTRUCTION_REFUND].iter();
        (self.money.cash, paid.sum())
    }

    fn compost_income(&self) -> f32 {
        self.w
            .units
            .buildings
            .get(&COMPOST)
            .map_or(0.0, |l| l.income[2])
    }
}

#[test]
fn a_worker_takes_the_dung_task_bags_the_pile_and_the_compost_pays() {
    let mut g = Game::new("");
    g.calm();
    g.dung((6, 9), "poo", true);
    let (cash, _) = g.refunds();
    g.run(400);
    assert_eq!(
        g.jobs(),
        ["ZooDoo", "BagDoo", "none"],
        "{:?}",
        g.lines(&["staff", "unit set"])
    );
    assert_eq!(g.lines(&["unit set 1 bBag"]).len(), 1);
    // The pile is gone and the task with it; the compost has one use and the cash 50.
    assert!(!g.w.zoo.entities.iter().any(|e| e.subtype == "poo"));
    assert!(g.w.units.pool.tasks.is_empty());
    assert_eq!(g.refunds(), (cash + 50.0, 50.0));
    assert_eq!(g.compost_income(), 50.0);
}

#[test]
fn dinosaur_dung_is_bagged_with_its_own_set_and_pays_twice() {
    let mut g = Game::new("");
    g.calm();
    g.dung((6, 9), "dinopoo", true);
    let (cash, _) = g.refunds();
    g.run(400);
    assert_eq!(g.jobs(), ["ZooDoo", "BagDoo", "none"]);
    assert_eq!(g.lines(&["unit set 1 bBagDino"]).len(), 1);
    assert!(g.lines(&["unit set 1 bBag "]).is_empty());
    assert_eq!(g.refunds(), (cash + 100.0, 100.0));
    assert_eq!(g.compost_income(), 100.0);
}

#[test]
fn food_is_bagged_without_pay_while_dung_lies_somewhere() {
    let mut g = Game::new("");
    g.calm();
    g.dung((6, 9), "food", true);
    // Dung elsewhere keeps the walk going: with none in the world the job is cleared (O7).
    g.dung((12, 3), "poo", false);
    let (cash, _) = g.refunds();
    g.run(400);
    assert_eq!(g.jobs(), ["ZooDoo", "BagDoo", "none"]);
    assert!(!g.w.zoo.entities.iter().any(|e| e.class == "food"));
    assert!(g.w.zoo.entities.iter().any(|e| e.subtype == "poo"));
    assert_eq!(g.refunds(), (cash, 0.0));
    assert!(!g.w.units.buildings.contains_key(&COMPOST));
    // The same food with no dung anywhere is bagged too: food on the grounds keeps the walk going.
    let mut g = Game::new("");
    g.calm();
    g.dung((6, 9), "food", true);
    let (cash, _) = g.refunds();
    g.run(400);
    assert!(!g.w.zoo.entities.iter().any(|e| e.class == "food"));
    assert_eq!(g.refunds(), (cash, 0.0));
    assert!(!g.w.units.buildings.contains_key(&COMPOST));
}

#[test]
fn food_in_an_emptied_exhibit_is_bagged_with_no_dung_in_the_world() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &pen(PEN));
    g.calm();
    g.dung((4, 3), "food", true);
    let (cash, _) = g.refunds();
    g.run(1000);
    assert_eq!(
        g.jobs(),
        ["ZooDoo", "BagDoo", "none"],
        "{:?}",
        g.lines(&["staff", "unit set"])
    );
    assert!(!g.w.zoo.entities.iter().any(|e| e.class == "food"));
    assert!(g.w.units.pool.tasks.is_empty());
    assert_eq!(g.refunds(), (cash, 0.0));
    assert_eq!(g.compost_income(), 0.0);
}

#[test]
fn food_in_an_exhibit_is_left_to_the_keepers_while_its_amphibious_neighbour_holds_animals() {
    // A second exhibit beside the emptied one, with a zebra in it, the two linked as neighbours.
    let mut other = pen((10, 2, 12, 5, 11));
    for e in &mut other {
        e.common.as_mut().unwrap().id += 100;
    }
    let mut zebra = entity("animals", "zebra", 400, centre((11, 3)), 0);
    zebra.subtype = "m".into();
    let extra: Vec<Entity> = pen(PEN).into_iter().chain(other).chain([zebra]).collect();
    let mut g = Game::with("", &[(WORKER, (1, 9))], &extra);
    let ids: Vec<(usize, u32)> = (g.w.exhibits().list())
        .iter()
        .map(|e| (e.first, e.id))
        .collect();
    g.calm();
    g.dung((4, 3), "food", true);
    let id = |t: (i32, i32)| ids.iter().find(|(f, _)| *f == Game::tile(t)).unwrap().1;
    let (emptied, next) = (id((3, 2)), id((10, 2)));
    let list = g.w.exhibits_mut().list_mut();
    for e in list.iter_mut() {
        e.amphibious = match e.id {
            i if i == emptied => vec![next],
            i if i == next => vec![emptied],
            _ => Vec::new(),
        };
    }
    g.run(1000);
    // The keepers' work: the task is void, the food stays and no worker walks to it.
    assert!(!g.jobs().contains(&"BagDoo"), "{:?}", g.jobs());
    assert!(g.w.zoo.entities.iter().any(|e| e.class == "food"));
    assert!(g.w.units.pool.tasks.is_empty());
}

#[test]
fn a_lone_pile_in_an_emptied_exhibit_clears_the_job() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &pen(PEN));
    g.calm();
    g.dung((4, 3), "poo", true);
    g.run(400);
    assert_eq!(g.jobs(), ["ZooDoo", "none"], "{:?}", g.lines(&["staff"]));
    assert!(g.w.zoo.entities.iter().any(|e| e.subtype == "poo"));
    assert!(g.w.units.pool.tasks.is_empty());
    assert_eq!(g.refunds().1, 0.0);
}

#[test]
fn the_nearest_compost_by_manhattan_distance_is_paid_and_none_means_nothing() {
    let mut g = Game::new("");
    g.calm();
    g.dung((6, 9), "poo", false);
    let at = |g: &Game, id| g.w.find(id).unwrap();
    // A second compost nearer to the pile than the first.
    let mut near = entity("building", "compost", 15, centre((7, 9)), 4);
    near.subtype = String::new();
    g.w.zoo.entities.push(near);
    let tile = Game::tile((6, 9));
    crate::staff::chores::recycle(&mut g.w, &g.cat, tile, false);
    assert_eq!(g.w.units.buildings[&15].income[2], 50.0);
    assert!(!g.w.units.buildings.contains_key(&COMPOST));
    assert_eq!(g.w.units.recycled, [50.0]);
    let _ = at(&g, 15);
    // With no compost in the zoo nothing is paid.
    let mut g = Game::new("");
    g.w.zoo.entities.retain(|e| e.type_name != "compost");
    crate::staff::chores::recycle(&mut g.w, &g.cat, tile, true);
    assert!(g.w.units.recycled.is_empty() && g.w.units.buildings.is_empty());
}

/// The tiles the events of a zoo just loaded name, in order.
fn loaded_tiles(g: &Game) -> Vec<u32> {
    let events = crate::staff::chores::loaded_leftovers(&g.w, &g.cat);
    assert!(events.iter().all(|e| e.kind == 10032));
    events.iter().map(|e| e.data).collect()
}

#[test]
fn a_loaded_zoo_calls_the_workers_for_the_dung_on_the_grounds_and_the_food_of_an_empty_exhibit() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &pen(PEN));
    g.dung((4, 3), "food", false);
    g.dung((6, 9), "poo", false);
    let tiles = [(4, 3), (6, 9)].map(|t| Game::tile(t) as u32);
    assert_eq!(loaded_tiles(&g), tiles);
}

/// A zoo wall down the east side of column 1, the entrance on tile (0, 0) facing east: columns 0
/// and 1 are beyond the wall, the tiles of the grounds (staff 8.2).
fn wall_beyond_column_one() -> Vec<Entity> {
    let mut extra: Vec<Entity> = (0..12)
        .map(|y: i32| entity("zoowall", "", 300 + y as u32, centre((1, y)), 2))
        .collect();
    extra.push(entity("building", "", 320, centre((0, 0)), 2));
    extra
}

#[test]
fn a_loaded_zoo_calls_the_workers_for_the_dung_outside_the_zoo_wall() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &wall_beyond_column_one());
    g.dung((0, 5), "poo", false);
    g.dung((1, 2), "food", false);
    let tiles = [(0, 5), (1, 2)].map(|t| Game::tile(t) as u32);
    assert_eq!(loaded_tiles(&g), tiles);
}

#[test]
fn a_pile_beyond_the_zoo_wall_keeps_the_dung_check_of_the_job_going() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &wall_beyond_column_one());
    g.dung((0, 5), "poo", false);
    assert!(crate::staff::chores::poo_left(&g.w, WORKER));
}

#[test]
fn food_beyond_the_zoo_wall_does_not_keep_the_dung_check_going() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &wall_beyond_column_one());
    g.dung((0, 5), "food", false);
    assert!(!crate::staff::chores::poo_left(&g.w, WORKER));
}

#[test]
fn dung_beyond_the_zoo_wall_is_bagged_like_dung_on_the_grounds() {
    let mut g = Game::with("", &[(WORKER, (0, 5))], &wall_beyond_column_one());
    g.calm();
    g.dung((0, 5), "poo", true);
    g.run(400);
    assert_eq!(
        g.jobs(),
        ["ZooDoo", "BagDoo", "none"],
        "{:?}",
        g.lines(&["staff", "unit set"])
    );
    assert!(!g.w.zoo.entities.iter().any(|e| e.subtype == "poo"));
    assert!(g.w.units.pool.tasks.is_empty());
}

#[test]
fn a_worker_who_cannot_reach_a_pile_beyond_the_zoo_wall_keeps_the_task_in_the_pool() {
    // Beyond the wall the pile is the grounds' (staff 8.6): the worker does not take the task as
    // the keepers' void one, it finds no way there and the task is unlinked.
    let mut g = Game::with("", &[(WORKER, (0, 9))], &wall_beyond_column_one());
    g.calm();
    g.dung((0, 5), "poo", true);
    g.run(400);
    assert_eq!(g.jobs(), ["ZooDoo", "none"], "{:?}", g.lines(&["staff"]));
    assert_eq!(g.w.units.pool.tasks.len(), 1);
    assert!(g.w.zoo.entities.iter().any(|e| e.subtype == "poo"));
}

#[test]
fn a_leftover_is_food_by_its_type_flag_not_its_class() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &[]);
    let mut snack = ty(
        "objects",
        "other",
        "snack",
        "[Characteristics/Integers]\ncFood=1\n",
    );
    snack.footprint = [1, 1, 1];
    g.cat.push(snack);
    let ty = g.cat.find("objects", b"other", b"snack").unwrap();
    assert!(dirt::put(
        &mut g.w,
        ty,
        ("objects", "other", "snack"),
        (4, 3),
        0,
        0
    ));
    assert_eq!(loaded_tiles(&g), [Game::tile((4, 3)) as u32]);
}

#[test]
fn a_loaded_zoo_leaves_the_dung_of_an_exhibit_with_an_animal_alone() {
    let mut zebra = entity("animals", "zebra", 300, centre((5, 3)), 0);
    zebra.subtype = "z".into();
    let mut g = Game::with("", &[(WORKER, (1, 9))], &[pen(PEN), vec![zebra]].concat());
    g.dung((4, 3), "poo", false);
    assert_eq!(loaded_tiles(&g), Vec::<u32>::new());
}

#[test]
fn an_emptied_exhibit_calls_a_worker_for_the_centre_slot_of_a_tile_only() {
    let mut g = Game::with("", &[(WORKER, (1, 9))], &pen(PEN));
    g.dung((4, 3), "poo", false);
    let ty = g.cat.find("objects", b"other", b"poo").unwrap();
    // A pile in a corner quarter of the next tile is not looked at.
    let corner = (5 * 64 + 8, 3 * 64 + 8);
    assert!(dirt::put_at(
        &mut g.w,
        ty,
        ("objects", "other", "poo"),
        corner,
        0,
        0
    ));
    let x = g.w.exhibits().region(Game::tile((4, 3))).unwrap();
    let tiles = g.w.exhibits().get(x).unwrap().order.clone();
    let found = crate::staff::chores::leftovers(&g.w, &g.cat, &tiles);
    assert_eq!(
        found.iter().map(|f| f.1).collect::<Vec<_>>(),
        [Game::tile((4, 3))]
    );
}

#[test]
fn a_worker_bags_the_old_dung_of_a_loaded_zoo() {
    let mut g = Game::new("");
    g.calm();
    g.dung((6, 9), "poo", false);
    g.sim.fences_loaded(&g.w, &g.cat);
    g.run(400);
    assert_eq!(g.jobs(), ["ZooDoo", "BagDoo", "none"]);
    assert!(!g.w.zoo.entities.iter().any(|e| e.subtype == "poo"));
}
