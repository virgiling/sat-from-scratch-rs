/// This module contains the Watch struct and its associated methods.
/// We use 2-watched-literals (2W) to represent the watch list.

#[derive(Debug, Clone, Default)]
pub struct Watch {
    clause_idx: usize,
    blocker: bool,
}

impl Watch {
    pub fn new() -> Self {
        Self {
            clause_idx: 0,
            blocker: false,
        }
    }

    pub fn with_clause_idx(mut self, clause_idx: usize) -> Self {
        self.clause_idx = clause_idx;
        self
    }

    pub fn with_blocker(mut self, blocker: bool) -> Self {
        self.blocker = blocker;
        self
    }

    pub fn clause_idx(&self) -> usize {
        self.clause_idx
    }

    pub fn satisfied(&self) -> bool {
        self.blocker
    }
}

#[derive(Debug, Clone, Default)]
pub struct WatchList {
    watches: Box<Vec<Watch>>,
}
