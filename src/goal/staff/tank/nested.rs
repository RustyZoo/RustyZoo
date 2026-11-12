//! A marine keeper in a tank inside a tank (tank staff 5.4): the outer tank X is 4 by 3 tiles from
//! (2, 2) with its gate on its north side, the inner tank Y is the two tiles (3, 3) and (4, 3)
//! walled off inside it, its gate on its north wall so that it opens into X's water.

use super::*;
use crate::behaviour::{Call, Data, Heard, Number, Row, Timing, parse_call};
use crate::entity::Catalogue;
use crate::entity::type_of;
use crate::fixtures::unit_ty;
use crate::money::Money;
use crate::sim::Sim;
use crate::staff::service;
use crate::unit::testing::{Walker, centre, entity, tank_world_with};
use crate::zoo::{Clocks, Entity};

const KEEPER: u32 = 7;
const GATE_Y: u32 = 3001;

/// The ring of tank walls round Y; the first piece is its gate.
fn ring() -> Vec<crate::zoo::Entity> {
    let wall = |id: u32, (x, y): (i32, i32), d: u32| {
        let side = [(32, 0), (63, 32), (32, 63), (0, 32)][d as usize / 2];
        entity(
            "tankwall",
            "iron",
            id,
            (x * 64 + side.0, y * 64 + side.1),
            d,
        )
    };
    let pieces = [
        ((3, 3), 0),
        ((4, 3), 0),
        ((3, 3), 4),
        ((4, 3), 4),
        ((3, 3), 6),
        ((4, 3), 2),
    ];
    (pieces.iter().enumerate())
        .map(|(n, &(t, d))| wall(GATE_Y + n as u32, t, d))
        .collect()
}

/// The keeper's sets: `bWalk` and `bWalksurf` walk, the climbs fly along the ladder.
struct Sets(Walker, Vec<Call>, Vec<Call>);

impl Sets {
    fn new() -> Sets {
        let call = |c: &str| vec![parse_call(c.as_bytes()).unwrap().0];
        Sets(
            Walker::new("fWalk(0,0)"),
            call("fFly(climb,33)"),
            call("fFlyReverse(climb,33)"),
        )
    }
}

impl Data for Sets {
    fn set(&self, e: &Entity, name: &str) -> Option<&[Call]> {
        match name {
            "bClimbUp" => Some(&self.1),
            "bClimbDown" => Some(&self.2),
            "bCleanTankSurf" | "bCleanTankSub" => self.0.set(e, "bWalk"),
            _ => self.0.set(e, name),
        }
    }
    fn ambient(&self, e: &Entity, n: usize) -> &[Row] {
        self.0.ambient(e, n)
    }
    fn timing(&self, e: &Entity, name: &str) -> Option<Timing> {
        self.0.timing(e, name)
    }
    fn number(&self, e: &Entity, number: &Number) -> i32 {
        match number {
            Number::Characteristic(key) if key == "cUnderwater" => 1,
            _ => self.0.number(e, number),
        }
    }
    fn sound(&self, e: &Entity, name: &str) -> Option<Heard> {
        self.0.sound(e, name)
    }
}

struct Game {
    w: World,
    sim: Sim,
    money: Money,
    cat: Catalogue,
    data: Sets,
    log: Vec<String>,
    said: Vec<i32>,
}

impl Game {
    /// The two tanks full to level 4, X clean and Y at purity `y_purity`; the marine keeper on the
    /// grounds at (1, 0).
    fn new(y_purity: i64, y_height: u32) -> Game {
        let mut keeper = entity("keeper", "trainer", KEEPER, centre((1, 0)), 0);
        keeper.subtype = "m".into();
        let mut more = ring();
        more.push(keeper);
        let mut w = tank_world_with(5, 4, &[], more);
        let (x, y) = (
            w.tank_at(2 * 12 + 2).unwrap(),
            w.tank_at(3 * 12 + 3).unwrap(),
        );
        assert_ne!(x, y);
        w.exhibits_mut().get_mut(y).unwrap().gate = Some(GATE_Y);
        for (id, purity, height) in [(x, 100, 5), (y, y_purity, y_height)] {
            let tank = w.exhibits_mut().get_mut(id).unwrap().tank.as_mut().unwrap();
            (tank.height, tank.level, tank.filled) = (height, height.min(4), true);
            w.set_purity(id, purity);
        }
        crate::unit::testing::still_water(&mut w);
        let marine = "cUnderwater=1\ncCleanTankThreshold=100\ncCleanTankPct=10";
        let mut cat = Catalogue::default();
        cat.push(unit_ty("keeper", "trainer", "m", 9552, marine));
        let mut sim = Sim::new(b"");
        sim.set_seed(1);
        sim.start_game(12 * 8, Clocks::default());
        let mut g = Game {
            w,
            sim,
            money: Money::default(),
            cat,
            data: Sets::new(),
            log: Vec::new(),
            said: Vec::new(),
        };
        g.run(1);
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
            self.log.extend(units.chain(staff));
            self.said
                .extend(w.units.take_messages().into_iter().map(|m| m.0));
        }
    }

    /// The keeper's goals and jobs so far, as the log names them.
    fn round(&self) -> Vec<&str> {
        let kept = |l: &&String| l.contains("goal 7") || l.contains("job 7");
        self.log
            .iter()
            .filter(kept)
            .map(|l| &l[l.find("7 ").unwrap() + 2..])
            .collect()
    }

    /// The nested tank of `x` for the keeper standing in `here`.
    fn nested(&mut self, x: u32, here: Option<u32>) -> Option<u32> {
        let e = self.w.zoo.entities[self.w.find(KEEPER).unwrap()].clone();
        let who = type_of(&self.cat, &e).unwrap();
        service::nested(&mut self.w, &self.cat, who, x, here)
    }

    /// The keeper's record.
    fn record(&self) -> &crate::goal::Record {
        &self.w.units.get(KEEPER).unwrap().goal.record
    }

    /// The exhibit of the keeper's own target tile.
    fn target(&self) -> Option<u32> {
        let tile = self.w.units.get(KEEPER).unwrap().own_tile?;
        self.w.exhibits().region(tile)
    }

    /// The height of the keeper's body.
    fn z(&self) -> i32 {
        let at = self.w.find(KEEPER).unwrap();
        self.w.zoo.entities[at].common.as_ref().unwrap().z
    }

    /// The number of times the log names the goal or job.
    fn count(&self, what: &str) -> usize {
        self.round().iter().filter(|l| l.starts_with(what)).count()
    }

    /// Steps until `f` holds, at most 6000 steps.
    fn until(&mut self, f: impl Fn(&Game) -> bool) {
        for _ in 0..6000 {
            if f(self) {
                return;
            }
            self.run(1);
        }
        panic!("never: {:?}", self.round());
    }
}

/// The goals and jobs of the round, the cleaning and the ends of jobs left out.
fn steps(g: &Game) -> Vec<&str> {
    let mut v: Vec<&str> = g.round().into_iter().filter(|l| *l != "none").collect();
    v.retain(|l| !l.contains("Clean"));
    v
}

const X: u32 = 1;
const Y: u32 = 2;
const ENTER: u8 = crate::goal::ENTER_TANK;

#[test]
fn the_nested_lookup_names_the_next_tank_of_the_chain() {
    let mut g = Game::new(30, 5);
    let look = Game::nested;
    // The dirty inner tank is the one that needs him; in it, nothing is nested but itself.
    assert_eq!(look(&mut g, X, None), Some(Y));
    assert_eq!(look(&mut g, X, Some(X)), Some(Y));
    assert_eq!(look(&mut g, X, Some(Y)), None, "he stands in it");
    assert_eq!(look(&mut g, Y, Some(Y)), None);
    // A tank is no nested one of a land exhibit, and a gate that opens elsewhere is no way in.
    assert_eq!(look(&mut g, crate::exhibit::GROUNDS, None), None);
    g.w.exhibits_mut().get_mut(Y).unwrap().gate = None;
    assert_eq!(look(&mut g, X, None), None);
    // Clean water needs no one.
    let mut g = Game::new(100, 5);
    assert_eq!(look(&mut g, X, None), None);
}

#[test]
fn the_outermost_tank_is_the_last_one_a_gate_opens_into() {
    let mut g = Game::new(30, 5);
    assert_eq!(service::outermost(&g.w, Y), X);
    assert_eq!(service::outermost(&g.w, X), X);
    // A tank with no gate ends the chain (the original crashes there).
    g.w.exhibits_mut().get_mut(X).unwrap().gate = None;
    assert_eq!(service::outermost(&g.w, Y), X);
    g.w.exhibits_mut().get_mut(Y).unwrap().gate = None;
    assert_eq!(service::outermost(&g.w, Y), Y);
}

#[test]
fn the_walk_to_a_nested_tank_goes_to_the_gate_of_the_outermost_and_enters_that_one() {
    let mut g = Game::new(30, 5);
    g.until(|g| g.record().task == ENTER);
    // The keeper was sent to Y (it is the one that needs him); his target is a tile of X now,
    // and job 22 walks to the tile outside X's gate.
    assert_eq!(g.count("go"), 0);
    assert_eq!(g.target(), Some(X));
    g.until(|g| g.record().job == 22 && g.record().job_target.tile.is_some());
    assert_eq!(g.record().job_target.tile, Some(14));
}

#[test]
fn the_duties_of_the_outer_tank_end_in_the_nested_one_that_needs_him() {
    let mut g = Game::new(30, 5);
    g.until(|g| g.count("entertank") == 2);
    // He climbed down in X, found nothing to do, and the duties' complete sent him to Y.
    assert_eq!(
        &steps(&g)[..8],
        [
            "habitat 4 - -",
            "entertank 0 - -",
            "GoToOutsideLadderBase",
            "ClimbUpOutsideLadder",
            "habitatduties 0 - -",
            "ClimbDownInsideLadder",
            "none 0 - -",
            "entertank 0 - -",
        ]
    );
    assert_eq!(g.target(), Some(Y));
}

#[test]
fn job_22_swims_to_a_gate_in_the_water_at_the_surface() {
    let mut g = Game::new(30, 5);
    g.until(|g| {
        g.count("entertank") == 2 && g.record().job == 22 && g.record().job_target.pos.is_some()
    });
    // The tile outside Y's gate is in X, whose water is at level 4.
    assert_eq!(g.record().job_target.tile, Some(27));
    assert_eq!(g.record().job_target.pos.map(|p| p.2), Some(64));
}

#[test]
fn the_keeper_leaving_the_nested_tank_serves_the_outer_one_then_leaves_it() {
    let mut g = Game::new(30, 5);
    g.until(|g| g.count("exittank") == 2);
    g.run(200);
    let round = steps(&g);
    let at = round
        .iter()
        .position(|l| l.starts_with("exittank"))
        .unwrap();
    assert_eq!(
        &round[at..at + 10],
        [
            "exittank 0 - -",
            "GoToInsideLadderBase",
            "ClimbUpInsideLadder",
            "ClimbDownOutsideLadder",
            "habitatduties 0 - -",
            "none 0 - -",
            "exittank 0 - -",
            "GoToInsideLadderBase",
            "ClimbUpInsideLadder",
            "ClimbDownOutsideLadder",
        ]
    );
}

#[test]
fn the_climb_down_out_of_a_nested_tank_ends_at_the_surface_of_the_outer_one() {
    // Y's walls (3) are below X's water (4): the foot of the ladder is under the surface.
    let mut g = Game::new(30, 3);
    g.until(|g| g.count("habitatduties") == 3);
    assert_eq!(g.z(), 64);
}

#[test]
fn a_nested_tank_whose_outer_tank_has_no_gate_is_told_blocked_and_the_keeper_wanders() {
    let mut g = Game::new(30, 5);
    g.w.exhibits_mut().get_mut(X).unwrap().gate = None;
    g.run(1500);
    assert!(g.said.contains(&11001), "{:?}", g.said);
    assert_eq!(g.count("entertank"), 0);
    assert!(g.count("random") > 0);
}
