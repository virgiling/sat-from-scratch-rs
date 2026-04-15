use crate::common::{ActivityTable, Clause, Literal, Phases, Variable};

pub struct InnerSolver {
    pub assignment: Vec<Variable>,
    pub clauses: Vec<Clause>,
    pub trail: Vec<Literal>,
    pub vsids: ActivityTable,
    pub phases: Phases,
}
