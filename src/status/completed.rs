//! The completed research list of the zoo status window (zoo status 21.5): a row for each
//! finished program, the newest on top.

use crate::research::Tree;
use std::collections::HashSet;

/// The list box of the tab.
pub(crate) const LIST: i32 = 4164;

/// The finished programs: the name id and the icon of each, newest first, and the ids already
/// listed.
#[derive(Debug, Default)]
pub(crate) struct Completed {
    pub rows: Vec<(i32, String)>,
    seen: HashSet<i32>,
    /// The rows changed since the list was last written.
    pub changed: bool,
}

impl Completed {
    /// A program of `tree` named `name` is completed: nothing when its cost is 0 or its name is
    /// listed, else a row at the top.
    pub(crate) fn add(&mut self, tree: &Tree, name: i32) {
        let Some(p) = tree.programs().find(|p| p.name == name) else {
            return;
        };
        if p.cost != 0.0 && self.seen.insert(name) {
            self.rows.insert(0, (name, p.picture.clone()));
            self.changed = true;
        }
    }

    /// The list of a game just started or loaded: emptied, then every program the tree holds as
    /// done is added in tree order, so the last one read is on top.
    pub(crate) fn replay(tree: &Tree) -> Completed {
        let mut list = Completed {
            changed: true,
            ..Completed::default()
        };
        for p in tree.programs().filter(|p| p.done()) {
            list.add(tree, p.name);
        }
        list
    }
}

#[cfg(test)]
mod tests;
