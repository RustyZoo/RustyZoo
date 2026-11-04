//! The goal object of a task and the sub-goal object of a job: the factory, the task each
//! stands for and the one update.

use super::{
    AVOID, CAPTURE, CHASE, Cx, DO_SCENERY, DRINK, DROWN, DUTIES, EAT, EAT_STANDING,
    EATING_STANDING, ENTER_BUILDING, ENTER_TANK, ENTRANCE, EXIT_BUILDING, EXIT_TANK, GAWK,
    GO_BUILDING, GO_TO_SHOW, HABITAT, KEEPER_FOOD, LEAVE, LEG, MAINT, NONE, RANDOM, RECOVER,
    RETURN_FROM_SHOW, RETURN_TO_BUILDING, SCENERY, SICK, SITE, SURFACE, TALK, TRICK_FOOD, USE,
};
use crate::unit::Kind;

/// The goal object of a task. Each variant is a small state machine over the unit and its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Goal {
    /// Tasks 0 and 1: nothing and the wander (7).
    Base,
    /// 0x65 of a guest: walk to the viewing tile of an exhibit (11.2).
    Habitat,
    /// 0x65 of a keeper: walk to the exhibit that needs him (staff 7.4); 0x6e, his duties there
    /// (staff 7.5).
    Exhibit,
    Duties,
    /// 0x78 of a maintenance worker: take a task of the pool and do its jobs (staff 8.3).
    Maint,
    /// The capture task of a keeper: dart and crate an escaped animal (staff 7.11).
    Capture,
    /// 0x7e and 0x7f of a keeper: enter a tank and leave it by the ladders (tank staff 4).
    EnterTank,
    ExitTank,
    /// 0x72 of an animal: walk to a piece of keeper food and eat it (11.8).
    KeeperFood,
    /// 0x71 of a guest: walk to the zoo entrance and go (11.6).
    Leave,
    /// 0x7a of an animal: walk to the water and drink (11.9).
    DrinkWater,
    /// 0x7c of an animal: swim to the surface and breathe (tank fauna 10.2).
    Surface,
    /// 0x7d of an animal: go to the platform for a treat (shows 23.5).
    TrickFood,
    /// 100 of a guest: choose an exhibit to see (11.1).
    Site,
    /// 0x70 of a guest: view the animals from the viewing area (11.5).
    Gawk,
    /// 0x80 of a guest: walk to a scenery object; 0x81: gawk at it (11.12).
    Scenery,
    DoScenery,
    /// 0x76 of a guide: a leg to a viewing area, and 0x77 the talk there; 0x76 of a guest: the walk
    /// after the guide (11.7).
    Leg,
    Talk,
    Follow,
    /// 0x66 to 0x6a of a guest or an animal (to a shelter): the building chain (11.3), walking to
    /// the building, to its entrance and into its slot, using it and leaving (0x69 also for a unit
    /// of any kind sent out of a building).
    Go,
    Entrance,
    Enter,
    Exit,
    Use,
    /// 0x6b to 0x6d of a guest: find a place to eat, eat standing, the meal (11.4).
    Eat,
    EatStand,
    Eating,
    /// 0x79 of a guest: be sick on the spot (11.10).
    Sick,
    /// 0x75: run from a predator (11.11, animals 20.6).
    Avoid,
    /// 0x74 of an animal: hunt a prey or play the friendly chase (animals 20.2).
    Chase,
    /// 0x82 of a guest: the ride on the water after a return building (11.13).
    Ride,
    /// The recover task (131) of a guest: climb out of the water (tank fauna 13).
    Recover,
    /// The drown task (132): a man drowns, a woman or a child is put back on land (tank fauna 13.1).
    Drown,
    /// The tasks 133 and 134 of an animal: swim to the portal of the show tank, and back home
    /// (shows 21.3, 21.4).
    GoShow,
    Return,
    /// 0x6a to 0x67 of a helicopter: wait at its base, take off, return and land (staff 7.11.1).
    HeliBase,
    HeliExit,
    HeliReturn,
    HeliEnter,
}

impl Goal {
    /// The factory (4.2): the base goal for every number outside the table of tasks, the goal of
    /// the task for the ones built so far, nothing for the others (they wait for their issue).
    pub(super) fn build(task: u8, kind: Kind) -> Option<Goal> {
        match (task, kind) {
            (..=99, _) | (135.., _) => Some(Goal::Base),
            (HABITAT, Kind::Guest) => Some(Goal::Habitat),
            (HABITAT, Kind::Keeper) => Some(Goal::Exhibit),
            (DUTIES, Kind::Keeper) => Some(Goal::Duties),
            (CAPTURE, Kind::Keeper | Kind::Heli) => Some(Goal::Capture),
            (USE, Kind::Heli) => Some(Goal::HeliBase),
            (EXIT_BUILDING, Kind::Heli) => Some(Goal::HeliExit),
            (ENTRANCE, Kind::Heli) => Some(Goal::HeliReturn),
            (ENTER_BUILDING, Kind::Heli) => Some(Goal::HeliEnter),
            (MAINT, Kind::Worker) => Some(Goal::Maint),
            (ENTER_TANK, Kind::Keeper) => Some(Goal::EnterTank),
            (EXIT_TANK, Kind::Keeper) => Some(Goal::ExitTank),
            (KEEPER_FOOD, Kind::Animal) => Some(Goal::KeeperFood),
            (LEAVE, Kind::Guest) => Some(Goal::Leave),
            (DRINK, Kind::Animal) => Some(Goal::DrinkWater),
            (SURFACE, Kind::Animal) => Some(Goal::Surface),
            (TRICK_FOOD, Kind::Animal) => Some(Goal::TrickFood),
            (SITE, Kind::Guest) => Some(Goal::Site),
            (GAWK, Kind::Guest) => Some(Goal::Gawk),
            (SCENERY, Kind::Guest) => Some(Goal::Scenery),
            (DO_SCENERY, Kind::Guest) => Some(Goal::DoScenery),
            (LEG, Kind::Guide) => Some(Goal::Leg),
            (TALK, Kind::Guide) => Some(Goal::Talk),
            (LEG, Kind::Guest) => Some(Goal::Follow),
            (GO_BUILDING, Kind::Guest | Kind::Animal) => Some(Goal::Go),
            (ENTRANCE, Kind::Guest | Kind::Animal) => Some(Goal::Entrance),
            (ENTER_BUILDING, Kind::Guest | Kind::Animal) => Some(Goal::Enter),
            // Any unit sent out of a building by the common part (6.2) leaves it.
            (EXIT_BUILDING, _) => Some(Goal::Exit),
            (USE, Kind::Guest | Kind::Animal) => Some(Goal::Use),
            (EAT, Kind::Guest) => Some(Goal::Eat),
            (EAT_STANDING, Kind::Guest) => Some(Goal::EatStand),
            (EATING_STANDING, Kind::Guest) => Some(Goal::Eating),
            (SICK, Kind::Guest) => Some(Goal::Sick),
            (RETURN_TO_BUILDING, Kind::Guest) => Some(Goal::Ride),
            (RECOVER, Kind::Guest) => Some(Goal::Recover),
            (DROWN, _) => Some(Goal::Drown),
            (GO_TO_SHOW, Kind::Animal) => Some(Goal::GoShow),
            (RETURN_FROM_SHOW, Kind::Animal) => Some(Goal::Return),
            (CHASE, Kind::Animal) => Some(Goal::Chase),
            (AVOID, _) => Some(Goal::Avoid),
            _ => None,
        }
    }

    fn task(self) -> u8 {
        match self {
            Goal::Base => RANDOM,
            Goal::Habitat | Goal::Exhibit => HABITAT,
            Goal::Duties => DUTIES,
            Goal::Capture => CAPTURE,
            Goal::Maint => MAINT,
            Goal::EnterTank => ENTER_TANK,
            Goal::ExitTank => EXIT_TANK,
            Goal::KeeperFood => KEEPER_FOOD,
            Goal::Leave => LEAVE,
            Goal::DrinkWater => DRINK,
            Goal::Surface => SURFACE,
            Goal::TrickFood => TRICK_FOOD,
            Goal::Site => SITE,
            Goal::Gawk => GAWK,
            Goal::Scenery => SCENERY,
            Goal::DoScenery => DO_SCENERY,
            Goal::Leg | Goal::Follow => LEG,
            Goal::Talk => TALK,
            Goal::Go => GO_BUILDING,
            Goal::Entrance => ENTRANCE,
            Goal::Enter => ENTER_BUILDING,
            Goal::Exit => EXIT_BUILDING,
            Goal::Use => USE,
            Goal::Eat => EAT,
            Goal::EatStand => EAT_STANDING,
            Goal::Eating => EATING_STANDING,
            Goal::Sick => SICK,
            Goal::Avoid => AVOID,
            Goal::Chase => CHASE,
            Goal::Ride => RETURN_TO_BUILDING,
            Goal::Recover => RECOVER,
            Goal::Drown => DROWN,
            Goal::GoShow => GO_TO_SHOW,
            Goal::Return => RETURN_FROM_SHOW,
            Goal::HeliBase => USE,
            Goal::HeliExit => EXIT_BUILDING,
            Goal::HeliReturn => ENTRANCE,
            Goal::HeliEnter => ENTER_BUILDING,
        }
    }

    /// Called once when the object is built; false clears the record.
    pub(super) fn init(self, cx: &mut Cx) -> bool {
        match self {
            Goal::Gawk => cx.gawk_init(),
            Goal::DoScenery => cx.do_scenery_init(),
            Goal::Talk => cx.talk_init(),
            Goal::Duties => {
                cx.duties_init();
                true
            }
            Goal::Capture => cx.capture_init(),
            Goal::Eating => cx.eating_init(),
            Goal::Sick => cx.sick_init(),
            Goal::Ride => cx.ride_init(),
            Goal::Recover => cx.recover_init(),
            Goal::Drown => cx.drown_init(),
            Goal::Surface => cx.surface_init(),
            Goal::EnterTank => cx.enter_init(),
            Goal::Leave => cx.leave_init(),
            Goal::Avoid => cx.avoid_init(),
            Goal::Chase => cx.hunt_init(),
            Goal::ExitTank => cx.exit_init(),
            Goal::HeliExit | Goal::HeliReturn | Goal::HeliEnter => cx.heli_init(self),
            _ => true,
        }
    }

    /// Called when a stale object is removed.
    pub(super) fn abort(self, cx: &mut Cx) {
        if self == Goal::Capture {
            cx.done_capture();
        }
    }

    /// 4.3: one update of the object.
    pub(super) fn update(self, cx: &mut Cx) {
        if self == Goal::Base {
            return cx.base();
        }
        if !cx.record().done {
            self.check_done(cx);
        }
        if cx.record().task != self.task() {
            cx.unit.goal.stale = true;
        } else if !cx.record().done {
            self.decide(cx);
        } else {
            self.complete(cx);
        }
    }

    pub(super) fn check_done(self, cx: &mut Cx) {
        match self {
            Goal::Leave => return cx.leave_check_done(),
            Goal::Avoid => return cx.avoid_check_done(),
            Goal::Chase => return cx.hunt_check_done(),
            Goal::Ride => return cx.ride_check_done(),
            Goal::Recover => return cx.recover_check_done(),
            Goal::Drown => return cx.drown_check_done(),
            Goal::GoShow | Goal::Return => return cx.show_check_done(),
            Goal::Gawk => return cx.gawk_check_done(),
            Goal::Scenery => return cx.scenery_check_done(),
            Goal::DoScenery => return cx.do_scenery_check_done(),
            Goal::Leg => return cx.leg_check_done(),
            Goal::Talk => return cx.talk_check_done(),
            Goal::Exhibit => return cx.exhibit_check_done(),
            Goal::Duties => return cx.duties_check_done(),
            Goal::Capture => return cx.capture_check_done(),
            Goal::HeliBase | Goal::HeliExit | Goal::HeliReturn | Goal::HeliEnter => {
                return cx.heli_check_done(self);
            }
            // It never completes by itself (staff 8.1).
            Goal::Maint => return,
            Goal::EnterTank | Goal::ExitTank => return cx.ladder_task_check_done(),
            Goal::Enter => return cx.enter_check_done(),
            Goal::Use => return cx.use_check_done(),
            Goal::Exit => return cx.exit_check_done(),
            Goal::Eat => return cx.eat_check_done(),
            Goal::Eating => {
                return cx.record().done = cx.unit.runner.top() != Some("bEatStanding");
            }
            Goal::Sick => return cx.record().done = cx.unit.runner.top() != Some("bSick"),
            _ => {}
        }
        if self != Goal::Base && cx.reached(false) {
            cx.record().done = true;
        }
    }

    pub(super) fn decide(self, cx: &mut Cx) {
        match self {
            Goal::Habitat | Goal::Scenery if cx.record().target.tile.is_none() => {
                cx.set_task(NONE, 0, 0, None)
            }
            Goal::KeeperFood => cx.keeper_food_decide(),
            Goal::Exhibit => cx.exhibit_decide(),
            Goal::Duties => cx.duties_decide(),
            Goal::Maint => cx.maint_decide(),
            Goal::Leave => cx.leave_decide(),
            Goal::Avoid => cx.avoid_decide(),
            Goal::Chase => cx.hunt_decide(),
            Goal::Ride => cx.ride_decide(),
            Goal::Recover => cx.recover_decide(),
            Goal::DrinkWater => cx.drink_decide(),
            Goal::Surface => cx.surface_decide(),
            Goal::TrickFood => cx.trick_food_decide(),
            Goal::GoShow => cx.go_show_decide(),
            Goal::Return => cx.return_decide(),
            Goal::Site => cx.site_decide(),
            Goal::Gawk => cx.gawk_decide(),
            Goal::Leg => cx.leg_decide(),
            Goal::Follow if cx.record().target.tile.is_none() => cx.clear_goal(),
            Goal::Enter if cx.record().target.tile.is_none() => cx.enter_decide(),
            Goal::Exit => cx.exit_decide(),
            Goal::HeliExit | Goal::HeliReturn => cx.heli_decide(self),
            Goal::Go if cx.record().target.tile.is_none() => cx.reset(),
            Goal::EatStand if cx.record().target.tile.is_none() => cx.wander(false),
            _ => {}
        }
    }

    pub(super) fn complete(self, cx: &mut Cx) {
        match self {
            Goal::Habitat => {
                cx.cost_back();
                cx.set_task(GAWK, 0, 0, None)
            }
            Goal::KeeperFood => cx.keeper_food_complete(),
            Goal::Exhibit => cx.exhibit_complete(),
            Goal::Duties => cx.duties_complete(),
            Goal::Capture => cx.capture_complete(),
            Goal::EnterTank | Goal::ExitTank => cx.ladder_task_complete(),
            Goal::Leave => cx.leave_complete(),
            Goal::DrinkWater => cx.drink_complete(),
            Goal::Surface => cx.surface_complete(),
            Goal::TrickFood => cx.trick_food_complete(),
            Goal::Gawk => cx.gawk_complete(),
            Goal::Scenery => cx.scenery_arrived(),
            Goal::DoScenery => cx.reset(),
            Goal::Leg => cx.leg_complete(),
            Goal::Talk => cx.talk_complete(),
            Goal::Follow => cx.follow_complete(),
            Goal::Go => cx.building_arrived(),
            Goal::Entrance => cx.entrance_arrived(),
            Goal::Enter => cx.enter_arrived(),
            Goal::Use => cx.use_done(),
            Goal::Exit => cx.exit_done(),
            Goal::Eat => cx.eat_done(),
            Goal::EatStand => cx.set_task(EATING_STANDING, 1, 0, None),
            Goal::Eating => cx.reset(),
            Goal::Sick => cx.sick_done(),
            Goal::Avoid => cx.avoid_exit(),
            Goal::Chase => cx.hunt_complete(),
            Goal::Ride => cx.ride_complete(),
            Goal::Recover => cx.recover_complete(),
            Goal::Drown => cx.drown_complete(),
            Goal::GoShow => cx.go_show_complete(),
            Goal::Return => cx.return_complete(),
            Goal::HeliBase | Goal::HeliExit | Goal::HeliReturn | Goal::HeliEnter => {
                cx.heli_complete(self)
            }
            Goal::Base | Goal::Site | Goal::Maint => {}
        }
    }
}

/// The sub-goal object of a job: the keeper's jobs 1 to 6 and 13 to 15 (staff 7.6 to 7.8, 7.11), the worker's jobs 7 to
/// 10, 16, 17, 26 and 27 (staff 8.4 to 8.7, tank staff 8), the base one (9.2) for every other job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sub {
    Base,
    /// Job 18 of an animal: the preattack of a hunter and the defence of its prey (animals 20.4).
    Preattack,
    Keeper(u8),
    Worker(u8),
}

impl Sub {
    pub(super) fn build(job: u8, kind: Kind) -> Option<Sub> {
        match (kind, job) {
            (Kind::Keeper, 1..=6 | 13..=15 | 19..=25 | 28 | 29) => Some(Sub::Keeper(job)),
            (Kind::Worker, 7..=12 | 16 | 17 | 26 | 27) => Some(Sub::Worker(job)),
            (Kind::Heli, 13..=15) => Some(Sub::Keeper(job)),
            (Kind::Animal, super::chase::PREATTACK) => Some(Sub::Preattack),
            _ => Some(Sub::Base),
        }
    }

    /// 9.1 for a keeper's job: check done, then decide or complete while the job is the current one.
    pub(super) fn update(self, cx: &mut Cx) {
        if let Sub::Keeper(job) = self {
            if !cx.record().job_done {
                cx.job_check_done(job);
            }
            if cx.record().job != job {
                cx.unit.goal.sub_stale = true;
            } else if !cx.record().job_done {
                cx.job_decide(job);
            } else {
                cx.job_complete(job);
            }
            return;
        }
        if self == Sub::Preattack {
            if !cx.record().job_done {
                cx.preattack_check_done();
            }
            if cx.record().job != super::chase::PREATTACK {
                cx.unit.goal.sub_stale = true;
            } else if cx.record().job_done {
                cx.preattack_complete();
            }
            return;
        }
        if let Sub::Worker(job) = self {
            if !cx.record().job_done {
                cx.work_check_done(job);
            }
            if cx.record().job != job {
                cx.unit.goal.sub_stale = true;
            } else if !cx.record().job_done {
                cx.work_decide(job);
            } else {
                cx.work_complete(job);
            }
            return;
        }
        if cx.record().job == 0 || cx.record().job_done {
            return;
        }
        if cx.reached(true) {
            cx.record().job_target.clear();
            cx.record().job_done = true;
        }
        if !cx.record().job_done && !cx.record().job_target.is_set() {
            cx.wander(true);
        }
    }
}
