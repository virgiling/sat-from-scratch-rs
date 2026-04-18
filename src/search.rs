#![allow(unused_variables)]
use crate::{
    api::{Pass, Search},
    common::Watches,
    constants::SATResult,
    kernel::Kernel,
};

pub struct Searcher;

impl Search for Searcher {
    /// Runs Boolean Constraint Propagation (BCP) with two watched literals (2WL).
    ///
    /// # Invariants
    /// - `trail[..propagated]` has already been propagated.
    /// - `trail[propagated..]` are literals that became true but are not processed yet.
    /// - A watcher is stored under the literal that makes its watched literal false.
    ///   In other words, if this clause watches $w$, its entry is stored in $watch(\neg w)$.
    ///
    /// This indexing convention is why this function fetches [watches](Kernel::watches) with `lit`:
    ///
    /// When a literal $l$ is just assigned to true, all clauses that watch $\neg l$ must be revisited.
    ///
    /// # High-level flow
    /// 1. Pop one unpropagated true literal $l$ from `trail`.
    /// 2. Take (`mem::take`) the watch list of this literal $l$.
    /// 3. For each watcher:
    ///    - If `blocker` is true, keep the watcher (clause already satisfied).
    ///    - Otherwise inspect the clause and decide one of:
    ///      - keep watching (other watched literal is true),
    ///      - move the watch to another literal,
    ///      - or detect unit/conflict.
    /// 4. Write compacted watchers back to the same watch bucket.
    ///
    /// # Example
    /// Clause $C = (\neg x \lor y \lor z)$, initially watches $(\neg x, y)$.
    ///
    /// Watch table entries:
    /// - in watch($x$): watcher for $\neg x$ (because $x = - (\neg x)$)
    /// - in watch($\neg y$): watcher for $y$
    ///
    /// Assume decision sets $x = \top$, then $x$ is pushed to `trail`.
    /// - This makes $\neg x = \bot$, so $C$ becomes relevant now.
    /// - [propagate](Self::propagate) pops $lit = x$ and reads watch($x$).
    /// - It can then:
    ///   - move watch from $\neg x$ to $z$ (if $z$ is not false), or
    ///   - make $y$ unit, or
    ///   - report conflict (if all non-false candidates are gone).
    ///
    /// If we queried watch($\neg x$) here, we would process clauses watching $x$ instead,
    /// which is the opposite direction and misses necessary propagation work.
    fn propagate(&mut self, kernel: &mut Kernel) -> bool {
        while kernel.propagated < kernel.trail.len() {
            let lit = kernel.trail[kernel.propagated];
            kernel.propagated += 1;
            let mut ws = kernel.watches(lit);
            let mut i = 0usize;
            let mut j = 0usize;
            let size = ws.len();
            while i < size {
                let blocker = ws[i].blocker;
                if kernel.value(blocker) == 1 {
                    ws[j] = ws[i];
                    i += 1;
                    j += 1;
                    continue;
                }

                let clause_id = ws[i].clause_id;
                let (first_lit, clause_len) = {
                    let clause = &mut kernel.clauses[clause_id];
                    if clause[0] == -lit {
                        clause[0] = clause[1];
                        clause[1] = -lit;
                    }
                    (clause[0], clause.literals().len())
                };
                let w: Watches = Watches { clause_id, blocker: first_lit };
                i += 1;
                if kernel.value(first_lit) == 1 {
                    ws[j] = w;
                    j += 1;
                    continue;
                }
                let mut k = 0usize;
                while k < clause_len {
                    let lit_k = kernel.clauses[clause_id][k];
                    if kernel.value(lit_k) == 1 {
                        break;
                    }
                    k += 1;
                }
                if k < clause_len {
                    let moved_watch_lit = {
                        let clause = &mut kernel.clauses[clause_id];
                        clause[1] = clause[k];
                        clause[k] = -lit;
                        clause[1]
                    };
                    kernel.add_watch(moved_watch_lit, w);
                } else {
                    ws[j] = w;
                    j += 1;
                    if kernel.value(first_lit) == -1 {
                        while i < clause_len {
                            ws[j] = ws[i];
                            j += 1;
                            i += 1;
                        }
                        ws.truncate(j);
                        kernel.conflict = Some(clause_id);
                        return false;
                    }
                    kernel.assign(first_lit.unsigned_abs(), first_lit, Some(clause_id));
                }
            }
            ws.truncate(j);
        }
        true
    }

    fn decide(&mut self, kernel: &mut Kernel) {
        let var = kernel.vsids.next_variable(|v| kernel.assignment[v] == 0);
        if let Some(var_id) = var {
            let lit = kernel.phases.decide_phase(var_id, false, true);
            kernel.assign(var_id, lit, None);
        } else {
            kernel.result = SATResult::SAT;
        }
    }

    fn analyze(&mut self, kernel: &mut Kernel) {
        todo!()
    }

    fn backtrack(&mut self, kernel: &mut Kernel) {
        todo!()
    }

    fn search(&mut self, kernel: &mut Kernel, in_processor: &mut Vec<Box<dyn Pass>>) -> SATResult {
        while kernel.result == SATResult::UNKNOWN {
            if kernel.result == SATResult::UNSAT {
                return SATResult::UNSAT;
            } else if !self.propagate(kernel) {
                self.analyze(kernel);
            } else if kernel.satisfied() {
                return SATResult::SAT;
            } else {
                // Now we are doing inprocessing pass
                for pass in in_processor.iter_mut() {
                    if pass.applying(kernel) {
                        pass.apply(kernel);
                    }
                }
                // At last, we make a decision
                self.decide(kernel);
            }
        }
        kernel.result
    }
}
