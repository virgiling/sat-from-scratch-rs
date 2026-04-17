use crate::{
    common::{ActivityTable, Clause, Literal, Phases, Variable, Watches},
    constants::SATResult,
};

pub struct Kernel {
    /// we use -1, 0, 1 to represent the assignment of the variable: -1 for false, 0 for unknown, 1 for true
    pub assignment: Vec<i8>,
    pub vars: Vec<Variable>,
    pub clauses: Vec<Clause>,
    pub watches: Vec<Vec<Watches>>,
    pub trail: Vec<Literal>,
    pub vsids: ActivityTable,
    pub phases: Phases,

    pub result: SATResult,

    pub level: usize,
    pub propagated: usize,
    pub assigned: usize,

    clause: Vec<Literal>,
}

impl Kernel {
    pub fn new(max_vars: usize) -> Self {
        Self {
            assignment: vec![0; max_vars],
            vars: vec![Variable::default(); max_vars],
            clauses: vec![],
            watches: vec![Vec::new(); max_vars * 2],
            trail: vec![],
            vsids: ActivityTable::new(max_vars),
            phases: Phases::new(max_vars),
            result: SATResult::UNKNOWN,
            level: 0,
            propagated: 0,
            assigned: 0,
            clause: Vec::new(),
        }
    }

    pub fn add(&mut self, lit: Option<Literal>) {
        if let Some(lit) = lit {
            self.clause.push(lit);
        } else {
            let literals = std::mem::take(&mut self.clause);
            let clause = Clause::new().with_literals(literals).with_lbd(0);
            let clause_id = self.clauses.len();
            self.clauses.push(clause);
            self.attach_clause_watchers(clause_id);
        }
    }

    #[inline]
    fn watcher_index(&self, lit: Literal) -> usize {
        assert_ne!(lit, 0);
        assert!(lit.unsigned_abs() <= self.assignment.len());
        let var = lit.unsigned_abs() as usize;
        ((var - 1) << 1) | usize::from(lit < 0)
    }

    #[inline]
    fn add_watch(&mut self, watched_lit: Literal, watch: Watches) {
        let idx = self.watcher_index(watched_lit);
        self.watches[idx].push(watch);
    }

    fn attach_clause_watchers(&mut self, clause_id: usize) {
        let Some(clause) = self.clauses.get(clause_id) else {
            return;
        };

        let literals = clause.literals();
        assert_ne!(literals.len(), 0);
        if literals.len() == 1 {
            let lit = literals[0];
            self.add_watch(
                -lit,
                Watches {
                    clause_id,
                    blocker: lit,
                },
            );
        } else {
            let lit0 = literals[0];
            let lit1 = literals[1];
            self.add_watch(
                -lit0,
                Watches {
                    clause_id,
                    blocker: lit1,
                },
            );
            self.add_watch(
                -lit1,
                Watches {
                    clause_id,
                    blocker: lit0,
                },
            );
        }
    }

    pub fn watches(&mut self, lit: Literal) -> Vec<Watches> {
        let idx = self.watcher_index(lit);
        std::mem::take(&mut self.watches[idx])
    }

    pub fn assign(&mut self, var_id: usize, lit: Literal, reason: Option<usize>) {
        let mut var = self.vars[var_id];
        self.trail.push(lit);
        var.level = self.level;
        var.trail_index = self.trail.len();
        var.reason = reason;
        self.vars[var_id] = var;
        self.assignment[var_id] = if lit > 0 { 1 } else { -1 };
        self.phases.save_phase_for_variable(var_id, lit > 0);
        self.assigned += 1;
    }

    pub fn satisfied(&self) -> bool {
        self.assigned == self.assignment.len()
    }
}
