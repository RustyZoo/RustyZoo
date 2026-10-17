//! A keeper's round on two pens: the choice, the food, the heal, the dung and the way in.

use super::*;
use crate::animal::Animal;
use crate::behaviour::{Call, Data, Heard, Number, Row, Timing, parse_call};
use crate::config::Config;
use crate::entity::Catalogue;
use crate::money::Money;
use crate::sim::Sim;
use crate::unit::testing::{centre, entity, rates, world_with};
use crate::world::World;
use crate::zoo::{Clocks, Entity, HabitatInfo};
use std::collections::HashMap;
use std::sync::Arc;

const KEEPER: u32 = 1;
const ZEBRA_A: u32 = 10;
const ZEBRA_B: u32 = 11;
/// The pens: corners (left, top, right, bottom) and the column of the gate on the south side.
const PEN_A: (i32, i32, i32, i32, i32) = (3, 2, 7, 5, 5);
const PEN_B: (i32, i32, i32, i32, i32) = (12, 2, 16, 5, 14);

fn fences(e: &mut Vec<Entity>, (x0, y0, x1, y1, gate): (i32, i32, i32, i32, i32)) {
    for (x, y) in (x0..=x1).flat_map(|x| (y0..=y1).map(move |y| (x, y))) {
        for (d, outside) in [(0, y == y0), (2, x == x1), (4, y == y1), (6, x == x0)] {
            if outside {
                let mut piece =
                    entity("fences", "exhibit", 100 + e.len() as u32, centre((x, y)), d);
                if (x, y, d) == (gate, y1, 4) {
                    piece.subtype = "g".into();
                }
                e.push(piece);
            }
        }
    }
}

fn animal(id: u32, tile: (i32, i32)) -> Entity {
    let mut zebra = entity("animals", "zebra", id, centre(tile), 0);
    zebra.subtype = "f".into();
    zebra
}

/// A 22 by 12 map: the pens of `pens`, a zebra in each and the zookeeper on the grounds at (1, 9).
fn zoo(pens: &[(i32, i32, i32, i32, i32)]) -> World {
    world_with((22, 12), |z| {
        for (n, &pen) in pens.iter().enumerate() {
            fences(&mut z.entities, pen);
            z.entities.push(animal(ZEBRA_A + n as u32, (pen.0, pen.1)));
            z.habitat_tiles.push((pen.0, pen.1));
            z.habitat_gates.push(((pen.4, pen.3), 4));
            z.habitat_info.push(HabitatInfo {
                name: format!("Pen {n}"),
                stats: [0; 9],
                created: 0,
            });
        }
        let mut keeper = entity("keeper", "keeper", KEEPER, centre((1, 9)), 2);
        keeper.subtype = "m".into();
        z.entities.push(keeper);
    })
}

/// The zebras stand still and neither age, eat, dung nor fall ill by themselves: called after the
/// first step, which creates their state.
fn calm(w: &mut World) {
    for id in [ZEBRA_A, ZEBRA_B] {
        if let Some(u) = w.units.get_mut(id) {
            u.goal.record.task = 134;
            u.part = crate::unit::Part::Animal(Animal {
                counters: [1000; 16],
                crowd_direction: -1,
                life: 5000,
                health: 500,
                ..Animal::default()
            });
        }
    }
}

fn zebra(w: &mut World, id: u32) -> &mut Animal {
    match w.units.get_mut(id).map(|u| &mut u.part) {
        Some(crate::unit::Part::Animal(a)) => a,
        _ => panic!("no zebra {id}"),
    }
}

fn ty(class: &'static str, name: &str, sub: &str, id: i32, keys: &str) -> EntityType {
    let unit = matches!(class, "animals" | "keeper");
    let head = if unit {
        format!("[{sub}/Characteristics/Integers]")
    } else {
        "[Characteristics/Integers]".into()
    };
    EntityType {
        class,
        type_name: name.into(),
        subtype: sub.into(),
        file: None,
        config: Some(Arc::new(
            Config::parse(format!("{head}\n{keys}\n").as_bytes()).unwrap(),
        )),
        name_id: id,
        footprint: [1, 1, 1],
        height: 0,
    }
}

fn catalogue() -> Catalogue {
    let mut cat = Catalogue::default();
    let keeper = "cFoodPerTile=1000\ncSicklyAnimalPct=15\ncDirt=6062\ncDirt=6093";
    for t in [
        ty("keeper", "keeper", "m", 9500, keeper),
        ty(
            "animals",
            "zebra",
            "f",
            1,
            "cNeededFood=100\ncPurchaseCost=1000",
        ),
        ty(
            "food",
            "food",
            "herbchow",
            6060,
            "cPurchaseCost=50\ncMaxFoodUnits=1000",
        ),
        ty("objects", "other", "poo", 6062, ""),
    ] {
        cat.push(t);
    }
    cat
}

/// The sets of the keeper: the walk ends on arrival, the others last a few frames.
struct Sets(HashMap<String, Vec<Call>>);

fn sets() -> Sets {
    let call = |c: &str| vec![parse_call(c.as_bytes()).unwrap().0];
    let walk = ("bWalk", call("fWalk(0,0)"));
    let play = ["bFeedHerb", "bHeal", "bClean"].map(|n| (n, call("fPlay(a)")));
    Sets(HashMap::from_iter(
        std::iter::once(walk)
            .chain(play)
            .map(|(n, c)| (n.into(), c)),
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
    said: Vec<(i32, Vec<String>)>,
}

impl Game {
    fn new(pens: &[(i32, i32, i32, i32, i32)]) -> Game {
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(22 * 12, Clocks::default());
        let (w, cat, data) = (zoo(pens), catalogue(), sets());
        let mut g = Game {
            w,
            sim,
            money: Money::default(),
            cat,
            data,
            log: Vec::new(),
            said: Vec::new(),
        };
        g.run(1);
        calm(&mut g.w);
        g
    }

    fn run(&mut self, steps: usize) {
        for _ in 0..steps {
            let (w, money) = (&mut self.w, &mut self.money);
            self.sim
                .step_game(&self.data, &self.cat, w, money, 67, |_, _, _| {});
            let units = w
                .units
                .take_lines()
                .into_iter()
                .map(|l| format!("unit {l}"));
            let staff = w
                .units
                .take_staff_lines()
                .into_iter()
                .map(|l| format!("staff {l}"));
            let animals = w
                .units
                .take_animal_lines()
                .into_iter()
                .map(|l| format!("animal {l}"));
            self.log.extend(units.chain(staff).chain(animals));
            self.said
                .extend(w.units.take_messages().into_iter().map(|(m, _, n)| (m, n)));
        }
    }

    /// The lines of the log that start with one of the words, in order.
    fn lines(&self, words: &[&str]) -> Vec<&str> {
        let wanted = |l: &&String| words.iter().any(|w| l.contains(w));
        self.log.iter().filter(wanted).map(String::as_str).collect()
    }

    fn exhibit(&self, tile: (i32, i32)) -> u32 {
        self.w
            .exhibits()
            .region(tile.1 as usize * 22 + tile.0 as usize)
            .unwrap()
    }

    fn put(&mut self, ty: &str, tile: (i32, i32), units: i32) -> bool {
        let (class, kind) = if ty == "poo" {
            ("objects", "other")
        } else {
            ("food", "food")
        };
        let found = self
            .cat
            .find(class, kind.as_bytes(), ty.as_bytes())
            .unwrap();
        dirt::put(&mut self.w, found, (class, kind, ty), tile, 0, units)
    }
}

#[test]
fn a_keeper_feeds_the_hungry_exhibit_pays_for_it_and_notes_the_service() {
    let mut g = Game::new(&[PEN_A]);
    zebra(&mut g.w, ZEBRA_A).hunger = 100;
    let pen = g.exhibit((4, 3));
    g.run(1500);
    let round = g.lines(&["staff go", "habitatduties", "staff job", "staff done"]);
    let kept: Vec<&str> = round.iter().map(|l| l.split_once(' ').unwrap().1).collect();
    assert_eq!(
        kept[..6],
        [
            format!("staff go 1 {pen}").as_str(),
            "unit goal 1 habitatduties 0 - -",
            "staff job 1 PutFood",
            "staff job 1 PuttingFood",
            "staff job 1 none",
            format!("staff done 1 {pen}").as_str(),
        ]
        .map(|l| l.split_once(' ').unwrap().1),
        "{round:?}"
    );
    // One pile of 1000 units in the pen, paid as animal food, the service noted.
    let piles = food::pieces(&g.w, &g.cat, &[pen], 0);
    assert_eq!(piles.iter().map(|p| p.left).collect::<Vec<_>>(), [1000]);
    assert_eq!(g.money.total[crate::money::ANIMAL_FOOD], 50.0);
    assert_eq!(g.money.cash, -50.0);
    let x = g.w.exhibits().get(pen).unwrap();
    assert_eq!((x.stats.upkeep[1], x.rating.coming), (50.0, 0));
    assert!(x.rating.serviced > 0);
    // The gate opened and closed round him.
    assert_eq!(g.lines(&["unit gate 1"]).len(), 4, "in and out");
}

#[test]
fn a_keeper_heals_a_sick_animal_then_rakes_the_dung_and_pays_the_medicine() {
    let mut g = Game::new(&[PEN_A]);
    let pen = g.exhibit((4, 3));
    // Food is there already; the zebra is sick, and a pile of dung lies in the pen.
    assert!(g.put("herbchow", (6, 4), 1000));
    assert!(g.put("poo", (6, 3), 0));
    zebra(&mut g.w, ZEBRA_A).health = 100;
    g.run(2500);
    let jobs: Vec<&str> = g
        .lines(&["staff job"])
        .iter()
        .map(|l| l.rsplit(' ').next().unwrap())
        .collect();
    assert_eq!(
        jobs,
        [
            "HealAnimal",
            "HealingAnimal",
            "none",
            "CleanDirt",
            "CleaningDirt",
            "none"
        ],
        "food first, then the heal, then the dung"
    );
    assert_eq!(zebra(&mut g.w, ZEBRA_A).health, 500);
    assert_eq!(g.money.total[crate::money::ANIMAL_HEALTH], 200.0);
    assert!(g.lines(&["animal heal 10"]).len() == 1);
    // The pile is gone and nothing is held.
    let dung = service::dung(
        &g.w,
        &g.cat,
        pen,
        type_of(&g.cat, &g.w.zoo.entities[g.w.find(KEEPER).unwrap()]).unwrap(),
    );
    assert!(dung.is_empty());
    assert_eq!(g.w.units.get(ZEBRA_A).unwrap().wait, 0);
}

#[test]
fn the_exhibit_with_the_most_trouble_comes_first_and_one_keeper_goes_to_a_place() {
    let mut g = Game::new(&[PEN_A, PEN_B]);
    let (a, b) = (g.exhibit((4, 3)), g.exhibit((13, 3)));
    let keeper = type_of(&g.cat, &g.w.zoo.entities[g.w.find(KEEPER).unwrap()]).unwrap();
    let choose = |g: &Game, here| service::choose(&g.w, &g.cat, keeper, &[], here, &[]);
    // The keeper that the first step sent to the first pen is not on his way here.
    g.w.exhibits_mut().get_mut(a).unwrap().rating.coming = 0;
    // Both are due (never serviced) and tie: the list order wins, the one he stands in does not.
    assert_eq!((choose(&g, None), choose(&g, Some(a))), (Some(a), Some(b)));
    // A hungry zebra with no food puts its exhibit first.
    zebra(&mut g.w, ZEBRA_B).hunger = 100;
    assert_eq!(choose(&g, None), Some(b));
    // A keeper on his way keeps the first pass away; the second needs a long wait.
    g.w.exhibits_mut().get_mut(b).unwrap().rating.coming = 1;
    assert_eq!(choose(&g, None), Some(a));
    g.w.exhibits_mut().get_mut(a).unwrap().rating.coming = 1;
    assert_eq!(choose(&g, None), None);
    g.w.units.ai_ticks = 5000;
    assert_eq!(choose(&g, None), Some(b));
    // Serviced a moment ago, with nothing wrong: no exhibit needs him.
    zebra(&mut g.w, ZEBRA_B).hunger = 0;
    for x in [a, b] {
        let e = g.w.exhibits_mut().get_mut(x).unwrap();
        (e.rating.coming, e.rating.serviced) = (0, 4990);
    }
    assert_eq!(choose(&g, None), None);
    g.w.units.service_interval = 5;
    assert_eq!(choose(&g, None), Some(a), "10 ticks are more than 5");
    // Assigned exhibits only.
    assert_eq!(
        service::choose(&g.w, &g.cat, keeper, &[b], None, &[]),
        Some(b)
    );
}

#[test]
fn a_blocked_entrance_posts_its_message_and_the_keeper_wanders_on_without_a_hold_on_the_gate() {
    // A pen of two tiles whose both tiles are full of bushes.
    let mut g = Game::new(&[(5, 2, 5, 3, 5)]);
    let pen = g.exhibit((5, 2));
    for tile in [(5, 2), (5, 3)] {
        for (dx, dy) in [(16, 16), (48, 16), (16, 48), (48, 48)] {
            let mut bush = entity("bush", "", 0, (tile.0 * 64 + dx, tile.1 * 64 + dy), 0);
            bush.subtype = String::new();
            g.w.add(bush, Some(bush_shape()));
        }
    }
    g.run(1500);
    let name = g.w.exhibits().get(pen).unwrap().name.clone();
    assert!(g.said.contains(&(11001, vec![name])), "{:?}", g.said);
    let keeper = g.w.units.get(KEEPER).unwrap();
    assert!(!keeper.mover.relaxed && keeper.gate.is_none());
    assert_eq!(g.w.exhibits().get(pen).unwrap().rating.coming, 0);
    // He wanders and tries again; every failure posts again (staff 10).
    assert!(g.said.iter().filter(|m| m.0 == 11001).count() > 1);
}

fn bush_shape() -> crate::world::Shape {
    crate::world::Shape {
        kind: crate::render::order::Kind::Fixed,
        footprint: (1, 1),
        fence: false,
        wall: false,
        gate: false,
        gated: false,
        tank: crate::world::TankKind::NONE,
        walkable: (false, false),
    }
}

#[test]
fn a_keeper_leaving_lowers_the_count_of_the_exhibit_he_was_going_to() {
    let mut g = Game::new(&[PEN_A]);
    let pen = g.exhibit((4, 3));
    g.run(60);
    assert_eq!(
        g.w.exhibits().get(pen).unwrap().rating.coming,
        1,
        "on his way"
    );
    assert!(
        g.w.units
            .get(KEEPER)
            .unwrap()
            .goal
            .record
            .target
            .tile
            .is_some()
    );
    crate::staff::leaves(&mut g.w, KEEPER);
    assert_eq!(g.w.exhibits().get(pen).unwrap().rating.coming, 0);
    crate::staff::leaves(&mut g.w, KEEPER);
    assert_eq!(
        g.w.exhibits().get(pen).unwrap().rating.coming,
        0,
        "not below 0"
    );
}

#[test]
fn an_assigned_keeper_strolls_to_the_outside_of_the_gate_of_his_exhibit() {
    let mut g = Game::new(&[PEN_A, PEN_B]);
    let b = g.exhibit((13, 3));
    if let Some(Part::Staff(s)) = g.w.units.get_mut(KEEPER).map(|u| &mut u.part) {
        s.assigned = vec![b];
    }
    let at = g.w.find(KEEPER).unwrap();
    let mut unit = g.w.units.get(KEEPER).unwrap().clone();
    unit.path.clear();
    let mut rng = crate::sim::Rng::default();
    assert!(crate::unit::walk::random_walk(
        &mut unit, &mut g.w, &mut rng, at
    ));
    let outside = 6 * 22 + 14;
    assert_eq!(unit.path.last(), Some(&outside));
}

#[test]
fn a_search_that_fails_is_asked_again_relaxed_and_a_partial_path_brings_him_nearer_each_time() {
    let mut g = Game::new(&[PEN_B]);
    // Far from the gate for a search that gives up after a few nodes.
    g.w.units.set_ini(b"[AI]\nNodeMax=15\n");
    let pen = g.exhibit((13, 3));
    g.run(3);
    let keeper = g.w.units.get(KEEPER).unwrap();
    assert!(!keeper.mover.relaxed, "the flag is cleared after the retry");
    assert!(
        keeper.walking || !keeper.path.is_empty(),
        "a partial path was taken"
    );
    assert_eq!(g.lines(&["staff go 1"]), [format!("staff go 1 {pen}")]);
    g.run(3000);
    assert!(
        !g.lines(&["unit enter 1"]).is_empty(),
        "he gets in by short stretches"
    );
    assert!(!g.w.units.get(KEEPER).unwrap().mover.relaxed);
}
