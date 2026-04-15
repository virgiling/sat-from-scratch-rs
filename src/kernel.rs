use crate::{
    api::Search,
    common::{ActivityTable, Clause, Literal, Phases, Variable},
    constants::SATResult,
};

pub struct InnerSolver {
    pub assignment: Vec<Variable>,
    pub clauses: Vec<Clause>,
    pub trail: Vec<Literal>,
    pub vsids: ActivityTable,
    pub phases: Phases,

    pub unsat: bool,

    pub propagated: usize,
    pub assigned: usize,
}

impl InnerSolver {
    pub fn satisfied(&self) -> bool {
        self.assigned == self.assignment.len()
    }
}
