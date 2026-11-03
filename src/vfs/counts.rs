//! Counts of what a `Vfs` indexed, for tests.

use std::collections::HashSet;

use super::{Source, Vfs};

impl Vfs {
    fn distinct_names(&self) -> HashSet<&String> {
        self.sources.iter().flat_map(Source::names).collect()
    }

    /// The number of archives (`*.ztd` and `*.zip`) accepted.
    pub(crate) fn archive_count(&self) -> usize {
        self.sources
            .iter()
            .filter(|s| matches!(s, Source::Archive { .. }))
            .count()
    }

    /// The number of loose files indexed, counted once for every search directory below which
    /// they lie (the install folder is usually one of them).
    pub(crate) fn loose_count(&self) -> usize {
        self.sources
            .iter()
            .map(|source| match source {
                Source::Directory(files) => files.len(),
                Source::Archive { .. } => 0,
            })
            .sum()
    }

    /// The number of distinct names, that is, of loose files and members that can be read.
    pub(crate) fn member_count(&self) -> usize {
        self.distinct_names().len()
    }

    /// The number of loose files and members hidden by a copy of the same name found earlier on
    /// the search path.
    pub(crate) fn shadowed_count(&self) -> usize {
        let all: usize = self.sources.iter().map(|s| s.names().len()).sum();
        all - self.member_count()
    }
}
