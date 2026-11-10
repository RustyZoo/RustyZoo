//! The research tree and the marketing campaigns the simulation keeps.

use super::*;
use crate::research::{Life, Overlay};

impl Sim {
    /// The research tree of the game just opened is built (research 7.4): the zoo status counts
    /// its programs from the next check on.
    pub(crate) fn set_research(&mut self, tree: Tree) {
        self.research = Some(tree);
    }

    /// The set-up of a scenario rebuilt the marketing (marketing 3.2).
    pub(crate) fn set_marketing(&mut self, marketing: Marketing) {
        self.marketing = marketing;
    }

    /// The marketing, for the panel and the file.
    pub(crate) fn marketing(&self) -> &Marketing {
        &self.marketing
    }

    /// The marketing the player's clicks and a load change.
    pub(crate) fn marketing_mut(&mut self) -> &mut Marketing {
        &mut self.marketing
    }

    /// The research tree, for the research panel.
    pub(crate) fn research(&self) -> Option<&Tree> {
        self.research.as_ref()
    }

    /// The research tree the player's clicks change.
    pub(crate) fn research_mut(&mut self) -> Option<&mut Tree> {
        self.research.as_mut()
    }

    /// The research section of a saved game for the writer; none when the game has no tree
    /// (research 7.5).
    pub(crate) fn research_section(&self) -> Option<Vec<u8>> {
        self.research.as_ref().map(Tree::save)
    }

    /// The programs completed since the last call (research 5.4).
    pub(crate) fn take_research_done(&mut self) -> Vec<Done> {
        std::mem::take(&mut self.research_done)
    }

    /// A program the research step completed, as the step reports it.
    #[cfg(test)]
    pub(crate) fn push_research_done(&mut self, done: Done) {
        self.research_done.push(done);
    }

    /// The building upgrade the completed program named `name` gives, with the name id of the
    /// building type it is for (research 6.3).
    pub(crate) fn research_upgrade(
        &self,
        name: i32,
    ) -> Option<(i32, crate::building::upgrade::Upgrade)> {
        self.research.as_ref()?.upgrade_of(name)
    }

    /// The characteristic effect of a program the step completed (research 6.4, 6.7), laid on
    /// `over`. The changes of the life counter come back for the live animals.
    pub(crate) fn research_effect(
        &mut self,
        cat: &Catalogue,
        over: &mut Overlay,
        d: &Done,
    ) -> Vec<Life> {
        let tree = self.research.as_mut();
        tree.map_or_else(Vec::new, |t| t.complete(cat, over, d))
    }

    /// Completes every research program of the game (research 6.8). The programs forced come back
    /// for the caller to apply their effects 0 and 1, with the changes of the life counter.
    pub(crate) fn force_research(
        &mut self,
        cat: &Catalogue,
        over: &mut Overlay,
        progress: bool,
    ) -> (Vec<Done>, Vec<Life>) {
        let Some(tree) = &mut self.research else {
            return Default::default();
        };
        let out = tree.force(cat, over, progress, &mut || self.rng.rand());
        self.lines.push(format!("research forced {}", out.0.len()));
        out
    }

    /// Completes every program of the research categories named `name` as a cheat does (research
    /// 6.8); the programs come back as for [`Sim::force_research`].
    pub(crate) fn complete_category(
        &mut self,
        cat: &Catalogue,
        over: &mut Overlay,
        name: i32,
    ) -> (Vec<Done>, Vec<Life>) {
        let Some(tree) = &mut self.research else {
            return Default::default();
        };
        let out = tree.complete_category(cat, over, name);
        self.lines
            .push(format!("research category {name} {}", out.0.len()));
        out
    }

    /// The research share of the zoo status: the tree's, 0 while a game has none.
    pub(crate) fn research_share(&self) -> i32 {
        self.research.as_ref().map_or(0, Tree::share)
    }
}
