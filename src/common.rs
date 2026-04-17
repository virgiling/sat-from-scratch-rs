/// This module contains the Clause struct and its associated methods.
use std::ops::{Index, IndexMut};

use keyed_priority_queue::KeyedPriorityQueue;
use ordered_float::OrderedFloat;

use crate::constants::{
    DEFAULT_VAR_DECAY, DEFAULT_VAR_DECAY_INC, DEFAULT_VAR_INC, INIT_PHASE, MAX_VAR_DECAY,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct Variable {
    pub level: usize,
    pub trail_index: usize,
    pub reason: Option<usize>,
}

pub type Literal = isize;

#[derive(Debug, Clone, Default)]
/// A clause is a disjunction of literals.
pub struct Clause {
    /// The LBD of the clause.
    lbd: u32,
    /// The literals of the clause.
    literals: Vec<Literal>,
    /// Deleted flag
    garbage: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Watches {
    pub clause_id: usize,
    pub blocker: Literal,
}

pub struct ActivityTable {
    pq: KeyedPriorityQueue<usize, OrderedFloat<f64>>,
    activities: Vec<f64>,
    var_inc: f64,
    var_decay: f64,
}

pub struct Phases {
    target_phase: Vec<bool>,
    forced_phases: Vec<bool>,
    saved_phases: Vec<bool>,
}

impl Clause {
    pub fn new() -> Self {
        Self {
            lbd: 0,
            literals: Vec::new(),
            garbage: false,
        }
    }

    pub fn with_literals(mut self, literals: Vec<Literal>) -> Self {
        self.literals = literals;
        self.literals
            .sort_by(|l1, l2| l1.unsigned_abs().cmp(&l2.unsigned_abs()));
        self
    }

    pub fn with_lbd(mut self, lbd: u32) -> Self {
        self.lbd = lbd;
        self
    }

    pub fn lbd(&self) -> u32 {
        self.lbd
    }

    pub fn literals(&self) -> &[Literal] {
        &self.literals
    }

    pub fn garbage(&self) -> bool {
        self.garbage
    }
}

impl IntoIterator for Clause {
    type Item = Literal;
    type IntoIter = std::vec::IntoIter<Literal>;

    fn into_iter(self) -> Self::IntoIter {
        self.literals.into_iter()
    }
}

impl<'a> IntoIterator for &'a Clause {
    type Item = &'a Literal;
    type IntoIter = std::slice::Iter<'a, Literal>;

    fn into_iter(self) -> Self::IntoIter {
        self.literals.iter()
    }
}

impl Index<usize> for Clause {
    type Output = Literal;

    fn index(&self, index: usize) -> &Self::Output {
        &self.literals[index]
    }
}

impl IndexMut<usize> for Clause {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.literals[index]
    }
}

impl ActivityTable {
    pub fn new(max_vars: usize) -> Self {
        let mut ret = Self {
            pq: KeyedPriorityQueue::new(),
            activities: vec![0.0; max_vars],
            var_inc: DEFAULT_VAR_INC,
            var_decay: DEFAULT_VAR_DECAY,
        };
        for var in 0..max_vars {
            ret.pq.push(var, OrderedFloat(0.0));
        }
        ret
    }

    pub fn bump_var_score(&mut self, var_id: usize) {
        self.activities[var_id] += self.var_inc;
        let p = OrderedFloat(self.activities[var_id]);
        match self.pq.entry(var_id) {
            keyed_priority_queue::Entry::Occupied(e) => {
                e.set_priority(p);
            }
            keyed_priority_queue::Entry::Vacant(e) => {
                e.set_priority(p);
            }
        }
    }

    #[inline]
    pub fn decay_inc(&mut self) {
        self.var_inc /= self.var_decay
    }

    #[inline]
    pub fn bump_decay_factor(&mut self) {
        self.var_decay += DEFAULT_VAR_DECAY_INC;
        if self.var_decay > MAX_VAR_DECAY {
            self.var_decay = MAX_VAR_DECAY;
        }
    }

    pub fn next_variable<F>(&mut self, mut has_assigned: F) -> Option<usize>
    where
        F: FnMut(usize) -> bool,
    {
        while let Some((var_id, _)) = self.pq.pop() {
            if !has_assigned(var_id) {
                return Some(var_id);
            }
        }
        None
    }
}

impl Phases {
    pub fn new(max_var: usize) -> Self {
        Self {
            target_phase: vec![INIT_PHASE; max_var],
            forced_phases: vec![INIT_PHASE; max_var],
            saved_phases: vec![INIT_PHASE; max_var],
        }
    }

    pub fn decide_phase(&self, var_id: usize, forced: bool, target: bool) -> Literal {
        if forced {
            return var_id as isize * (if self.forced_phases[var_id] { 1 } else { -1 });
        }
        if target {
            return var_id as isize * (if self.target_phase[var_id] { 1 } else { -1 });
        }
        var_id as isize * (if self.saved_phases[var_id] { 1 } else { -1 })
    }

    #[inline]
    pub fn save_phase_for_variable(&mut self, var_id: usize, phase: bool) {
        self.saved_phases[var_id] = phase;
    }
}
