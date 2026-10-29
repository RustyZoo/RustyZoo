//! What the research panel reads from the tree and what its buttons change (research 5.6, 8.3,
//! 8.5).

use super::{Branch, Category, Program, Tree};

/// The cost of a level a day, as a float: 1/30 of a month's (5.6).
const PER_DAY: f32 = 0.033_333_335;

impl Branch {
    /// The category and program the branch works on.
    pub fn program(&self) -> Option<(&Category, &Program)> {
        let (c, p) = self.current?;
        let category = self.categories.get(c)?;
        Some((category, category.programs.get(p)?))
    }

    /// The work of the funding level, `None` when it is not above 0.
    fn work(&self) -> Option<f32> {
        let work = self.funding.get(self.level)?.work;
        (work > 0.0).then_some(work)
    }

    /// The days left to finish the current program, `None` with no program or no work (5.6).
    pub fn days_remaining(&self) -> Option<f32> {
        let (_, p) = self.program()?;
        Some((p.cost - p.progress) * 30.0 / self.work()?)
    }

    /// The share of the current program still to do, to the nearest percent (5.6).
    pub fn percent_remaining(&self) -> Option<i32> {
        let (_, p) = self.program()?;
        self.work()?;
        Some(((p.cost - p.progress) * 100.0 / p.cost).round() as i32)
    }

    /// The funding level's string and its cost a day in whole units; `None` for a level out of
    /// range.
    pub fn funding_text(&self) -> Option<(i32, i64)> {
        let level = self.funding.get(self.level)?;
        Some((level.name, (level.cost * PER_DAY).round() as i64))
    }

    /// The next level up, held at the last one (0 for no levels).
    pub fn increase(&self) -> usize {
        (self.level + 1).min(self.funding.len().saturating_sub(1))
    }

    /// The next level down, held at 0.
    pub fn decrease(&self) -> usize {
        self.level.saturating_sub(1)
    }
}

impl Tree {
    /// The categories of branch `b` the panel lists: those whose expansion is not off (8.3).
    pub fn listed(&self, b: usize) -> Vec<usize> {
        let categories = self.branches.get(b).map_or(&[][..], |b| &b.categories);
        let shown = categories.iter().enumerate();
        shown
            .filter(|(_, c)| !c.skipped(&self.off))
            .map(|(i, _)| i)
            .collect()
    }

    /// The row of a list is clicked: the tick of its category flips (8.3).
    pub fn toggle(&mut self, b: usize, row: usize) {
        let category = self.listed(b).get(row).copied();
        let category = category.and_then(|c| self.branches.get_mut(b)?.categories.get_mut(c));
        if let Some(c) = category {
            c.tick = !c.tick;
        }
    }

    /// The funding button of branch `b`: the level goes up or down (8.5).
    pub fn fund(&mut self, b: usize, up: bool) {
        if let Some(branch) = self.branches.get_mut(b) {
            branch.level = if up {
                branch.increase()
            } else {
                branch.decrease()
            };
        }
    }

    /// The branch that holds the program named `name`.
    pub fn branch_of(&self, name: i32) -> Option<&Branch> {
        let has =
            |b: &&Branch| (b.categories.iter()).any(|c| c.programs.iter().any(|p| p.name == name));
        self.branches.iter().find(has)
    }
}
