/// This module contains the Clause struct and its associated methods.
use std::ops::{Index, IndexMut};

use keyed_priority_queue::KeyedPriorityQueue;
use ordered_float::OrderedFloat;

use crate::constants::{
    DEFAULT_VAR_DECAY, DEFAULT_VAR_DECAY_INC, DEFAULT_VAR_INC, INIT_PHASE, MAX_VAR_DECAY,
};

/// This is the variable struct, but do not associate with the real variable in the problem.
/// It only records the trail information and the reason for the assignment.
#[derive(Debug, Clone, Copy, Default)]
pub struct Variable {
    /// The decision level of the variable when it is assigned.
    pub level: usize,
    /// The trail index of the variable when it is assigned.
    pub trail_index: usize,
    /// The reason for the assignment, if the variable is assigned by decision, the reason is `None`. Otherwise, the reason is the clause index that the variable is assigned by.
    pub reason: Option<usize>,
}

/// This is the literal type, it is a signed integer.
/// It is used to represent the literal in the clause.
pub type Literal = isize;

#[derive(Debug, Clone, Default)]
/// This is the clause struct, it is used to represent the clause in the problem.
/// A clause is a disjunction of literals.
/// We use `lbd` to distinguish the (ir)redundant clause.
pub struct Clause {
    /// The LBD of the clause.
    lbd: u32,
    /// The literals of the clause.
    literals: Vec<Literal>,
    /// Deleted flag
    garbage: bool,
}

/// This is the foundation of the two-watched-literals (2WL) technique.
/// The 2WL is a table, basically it will map the positive and negative literals to its associated clause.
/// Then, when we do propagation, we can use the 2WL to quickly find the associated clause for a literal (which is assigned to `FALSE`).
/// We implement the `Blocking Literal` to optimize the propagation:
/// - The `blocker` is often chosen as the other watched literal in this clause, which index is `clause_id`.
/// - When we really check into clauses, we should check the `blocker` first, if it is true, we can skip the clause.
#[derive(Debug, Clone, Default, Copy)]
pub struct Watches {
    pub clause_id: usize,
    pub blocker: Literal,
}

/// The Variable State Independent Decaying Sum (VSIDS) Heap based on the activity score of the variables.
///
/// # Details
///
/// It is used to select the next variable to assign.
/// The activity score is metric for how frequently the variable is conflicted.
/// The VSIDS heap is a priority queue, the key is the variable id, the value is the activity score.
///
/// # Update Rule
///
/// The activity score is updated by the following formula:
/// - `activity_score[var_id] = activity_score[var_id] * var_decay + var_inc`
/// - `var_inc` is the increment of the activity score, it is a constant.
/// - `var_decay` is the decay factor of the activity score, it is a constant.
pub struct ActivityTable {
    pq: KeyedPriorityQueue<usize, OrderedFloat<f64>>,
    activities: Vec<f64>,
    var_inc: f64,
    var_decay: f64,
}

/// This is the phases for the variables.
/// It is used to store the phases for the variables in the solver.
/// The phases are used to decide the phase for the variable when it is assigned.
/// The phases are stored in a vector, the index is the variable id, the value is the phase.
/// The phase is a boolean value, true for positive phase, false for negative phase.
pub struct Phases {
    /// The `target` phase is the longest conflict-free assigned sequence of variables assignment.
    target_phase: Vec<bool>,
    /// The `forced` phase is the phase that the variable is assigned to by the external force.
    forced_phases: Vec<bool>,
    /// The `saved` phase is the phase to restore the history message during the search.
    saved_phases: Vec<bool>,
}

impl Clause {
    pub fn new() -> Self {
        Self { lbd: 0, literals: Vec::new(), garbage: false }
    }

    pub fn with_literals(mut self, literals: Vec<Literal>) -> Self {
        self.literals = literals;
        self.literals.sort_by_key(|l1| l1.unsigned_abs());
        self
    }

    pub fn with_ordered_literals(mut self, literals: Vec<Literal>) -> Self {
        self.literals = literals;
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
            activities: vec![0.0; max_vars + 1],
            var_inc: DEFAULT_VAR_INC,
            var_decay: DEFAULT_VAR_DECAY,
        };
        for var in 1..=max_vars {
            ret.pq.push(var, OrderedFloat(0.0));
        }
        ret
    }

    pub fn bump_var_score(&mut self, var_id: usize) {
        match self.pq.entry(var_id) {
            keyed_priority_queue::Entry::Occupied(e) => {
                self.activities[var_id] += self.var_inc;
                let p = OrderedFloat(self.activities[var_id]);
                e.set_priority(p);
            }
            keyed_priority_queue::Entry::Vacant(e) => {
                let p = OrderedFloat(self.activities[var_id]);
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

    pub fn next_variable<F>(&mut self, not_assigned: &F) -> Option<usize>
    where
        F: Fn(usize) -> bool,
    {
        while let Some((var_id, _)) = self.pq.pop() {
            if not_assigned(var_id) {
                return Some(var_id);
            }
        }
        None
    }
}

impl Phases {
    pub fn new(max_var: usize) -> Self {
        Self {
            target_phase: vec![INIT_PHASE; max_var + 1],
            forced_phases: vec![INIT_PHASE; max_var + 1],
            saved_phases: vec![INIT_PHASE; max_var + 1],
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
