//! The calls of a set that the unit acts on after the step: each is raised once and taken once.

use super::Runner;

impl Runner {
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
