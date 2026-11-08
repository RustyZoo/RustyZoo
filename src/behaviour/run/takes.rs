//! The calls of a set that the unit acts on after the step: each is raised once and taken once.

use super::Runner;
use crate::behaviour::{Gait, Number, Target};

/// The move of `fTargetBuilding`, `fMove(0, 0, fast, cFastRate, 100)` (animals 11.11 point 4).
pub(super) fn charge() -> (Gait, Target, bool) {
    let own = Gait::Own {
        anim: "run".into(),
        speed: Number::Characteristic("cFastRate".into()),
        percent: Number::Int(100),
        sound: None,
    };
    (own, Target::Keep, false)
}

impl Runner {
    /// Whether the move call taken is an `fTargetBuilding`, once.
    pub(crate) fn take_rampage(&mut self) -> bool {
        std::mem::take(&mut self.rampage)
    }

    /// Whether an `fDie` call ran since the last time, once.
    pub(crate) fn take_die(&mut self) -> bool {
        std::mem::take(&mut self.die)
    }

    /// Whether an `fHatch` call ran since the last time, once.
    pub(crate) fn take_hatch(&mut self) -> bool {
        std::mem::take(&mut self.hatch)
    }

    /// Whether an `fEndTrick` call ran since the last time, once.
    pub(crate) fn take_end_trick(&mut self) -> bool {
        std::mem::take(&mut self.end_trick)
    }

    /// Whether an `fKillPrey` (`kill`) or an `fCaughtPrey` call ran since the last time, once.
    pub(crate) fn take_meal(&mut self, kill: bool) -> bool {
        std::mem::take(&mut self.meal[usize::from(kill)])
    }

    /// Whether an `fEnterBuilding` call ran since the last time, once.
    pub(crate) fn take_enter(&mut self) -> bool {
        std::mem::take(&mut self.enter)
    }

    /// Whether an `fExitBuilding` call ran since the last time, once.
    pub(crate) fn take_exit(&mut self) -> bool {
        std::mem::take(&mut self.exit)
    }
}
