use crate::{
    common::{ActivityTable, Clause, Literal, Phases, Variable, Watches},
    constants::SATResult,
};

pub struct Kernel {
    /// The assignment of the variables, we use -1 for false, 0 for unknown, 1 for true.
    pub assignment: Vec<i8>,
    /// The variable table.
    pub vars: Vec<Variable>,
    /// The clause table
    pub clauses: Vec<Clause>,
    /// The watch list
    pub watches: Vec<Vec<Watches>>,
    /// The trail for literal assignment, only stores the true literals
    pub trail: Vec<Literal>,
    /// The VSIDS activity table
    pub vsids: ActivityTable,
    /// The phases for variable decision
    pub phases: Phases,
    /// The result of the solver
    pub result: SATResult,

    /// The decision level of the solver
    pub level: usize,
    /// The number of propagations in one decision level
    pub propagated: usize,
    /// The number of variables assigned in the current decision level
    pub assigned: usize,

    pub conflict: Option<usize>,

    /// Used to add clauses temporarily
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
            conflict: None,
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
    pub fn value(&self, lit: Literal) -> i8 {
        self.assignment[lit.unsigned_abs()]
    }

    /// Maps a non-zero literal to the corresponding watcher bucket index.
    ///
    /// Layout per variable:
    /// - positive literal bucket first
    /// - negative literal bucket second
    ///
    /// So variable $v$ uses buckets:
    /// - $2 \times (v - 1)$ for $v$
    /// - $2 \times (v - 1) + 1$ for $\neg v$
    #[inline]
    fn watcher_index(&self, lit: Literal) -> usize {
        assert_ne!(lit, 0);
        assert!(lit.unsigned_abs() <= self.assignment.len());
        let var = lit.unsigned_abs();
        ((var - 1) << 1) | usize::from(lit < 0)
    }

    /// Adds one watcher entry into the bucket indexed by `watched_lit`.
    ///
    /// > [Note]
    /// > in this solver, callers usually pass $\neg w$ (not $w$) where $w$ is
    /// > the watched literal in the clause. This stores a clause under the literal
    /// > assignment that would falsify $w$, enabling direct lookup during BCP.
    #[inline]
    pub fn add_watch(&mut self, watched_lit: Literal, watch: Watches) {
        let idx = self.watcher_index(watched_lit);
        self.watches[idx].push(watch);
    }

    /// Initializes watch entries for a newly added clause.
    ///
    /// Convention used here:
    /// - Unit clause ($l$) stores one watcher in $watch(\neg l)$.
    /// - Non-unit clause ($l_0 \lor l_1 \lor \ldots$) stores:
    ///   - watcher for $l_0$ in $watch(\neg l_0)$ with $blocker = l_1$
    ///   - watcher for $l_1$ in $watch(\neg l_1)$ with $blocker = l_0$
    ///
    /// This matches the propagation loop that pops a true literal $p$ from trail
    /// and directly processes $watch(p)$.
    fn attach_clause_watchers(&mut self, clause_id: usize) {
        let Some(clause) = self.clauses.get(clause_id) else {
            return;
        };

        let literals = clause.literals();
        assert_ne!(literals.len(), 0);
        if literals.len() == 1 {
            let lit = literals[0];
            self.add_watch(-lit, Watches { clause_id, blocker: lit });
        } else {
            let lit0 = literals[0];
            let lit1 = literals[1];
            self.add_watch(-lit0, Watches { clause_id, blocker: lit1 });
            self.add_watch(-lit1, Watches { clause_id, blocker: lit0 });
        }
    }

    /// Takes out and returns the full watcher bucket for `lit`.
    ///
    /// `propagate` uses this "take-and-rebuild" pattern to avoid aliasing while
    /// mutating clauses and potentially moving some watchers to other buckets.
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
