use tracing::debug;

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
        // TODO Should re-write it more rusty
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
                    let clause_len = clause.literals().len();
                    if clause_len > 1 && clause[0] == -lit {
                        clause[0] = clause[1];
                        clause[1] = -lit;
                    }
                    (clause[0], clause_len)
                };
                let w: Watches = Watches { clause_id, blocker: first_lit };
                i += 1;
                if kernel.value(first_lit) == 1 {
                    ws[j] = w;
                    j += 1;
                    continue;
                }
                // The first two literals are watched literals.
                let mut k = 2usize;
                while k < clause_len {
                    let lit_k = kernel.clauses[clause_id][k];
                    if kernel.value(lit_k) != -1 {
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
                    kernel.add_watch(-moved_watch_lit, w);
                } else {
                    ws[j] = w;
                    j += 1;
                    if kernel.value(first_lit) == -1 {
                        while i < size {
                            ws[j] = ws[i];
                            j += 1;
                            i += 1;
                        }
                        ws.truncate(j);
                        kernel.set_watches(lit, ws);
                        kernel.conflict = Some((clause_id, first_lit));
                        return false;
                    }
                    kernel.assign(first_lit.unsigned_abs(), first_lit, Some(clause_id));
                }
            }
            ws.truncate(j);
            kernel.set_watches(lit, ws);
        }
        true
    }

    fn decide(&mut self, kernel: &mut Kernel) {
        let var = kernel.vsids.next_variable(&|v| kernel.assignment[v] == 0);
        if let Some(var_id) = var {
            let lit = kernel.phases.decide_phase(var_id, false, true);
            debug!("c deciding variable: {:?}, and assign literal: {:?}", var_id, lit);
            kernel.level += 1;
            kernel.assign(var_id, lit, None);
        } else {
            kernel.result = SATResult::SAT;
        }
    }

    fn analyze(&mut self, kernel: &mut Kernel) {
        let Some((mut conflict_idx, _conflict_lit)) = kernel.conflict else {
            panic!("c no conflict clause found, crashed in propagate");
        };

        kernel.statistics.conflicts += 1;
        let conflict_level = kernel.level;
        if conflict_level == 0 {
            kernel.backtrack_level = 0;
            kernel.result = SATResult::UNSAT;
            return;
        }

        kernel.lemma.clear();
        kernel.lemma.push(0);

        let var_stamp = kernel.next_mark_epoch();
        let mut bump_vars: Vec<usize> = Vec::new();
        let mut open = 0usize;
        let mut resolve_lit = 0isize;
        let mut trail_idx = kernel.trail.len();

        while open > 0 || resolve_lit == 0 {
            let clause_len = kernel.clauses[conflict_idx].literals().len();
            for i in 0..clause_len {
                let q = kernel.clauses[conflict_idx][i];
                if q == resolve_lit {
                    continue;
                }

                let var_id = q.unsigned_abs();
                let level = kernel.vars[var_id].level;
                if level == 0 || kernel.mark_at(var_id) == var_stamp {
                    continue;
                }

                kernel.set_mark_at(var_id, var_stamp);
                kernel.vsids.bump_var_score(var_id);
                bump_vars.push(var_id);
                if level == conflict_level {
                    open += 1;
                } else {
                    kernel.lemma.push(q);
                }
            }

            loop {
                trail_idx -= 1;
                let lit = kernel.trail[trail_idx];
                let var_id = lit.unsigned_abs();
                if kernel.mark_at(var_id) == var_stamp
                    && kernel.vars[var_id].level == conflict_level
                {
                    resolve_lit = lit;
                    break;
                }
            }

            let resolve_var = resolve_lit.unsigned_abs();
            kernel.set_mark_at(resolve_var, 0);
            open -= 1;
            if open == 0 {
                break;
            }
            conflict_idx = kernel.vars[resolve_var]
                .reason
                .unwrap_or_else(|| panic!("c missing reason for lit {resolve_lit}"));
        }

        kernel.lemma[0] = -resolve_lit;

        let level_stamp = kernel.next_mark_epoch();
        let mut lbd = 0u32;
        for i in 0..kernel.lemma.len() {
            let lit = kernel.lemma[i];
            let level = kernel.vars[lit.unsigned_abs()].level;
            if level > 0 && kernel.mark_at(level) != level_stamp {
                kernel.set_mark_at(level, level_stamp);
                lbd += 1;
            }
        }

        if kernel.lemma.len() == 1 {
            kernel.backtrack_level = 0;
        } else {
            let mut max_idx = 1usize;
            let mut max_level = kernel.vars[kernel.lemma[1].unsigned_abs()].level;
            for i in 2..kernel.lemma.len() {
                let level = kernel.vars[kernel.lemma[i].unsigned_abs()].level;
                if level > max_level {
                    max_level = level;
                    max_idx = i;
                }
            }
            if max_idx != 1 {
                kernel.lemma.swap(1, max_idx);
            }
            kernel.backtrack_level = max_level;
        }

        let threshold = kernel.backtrack_level.saturating_sub(1);
        for var_id in bump_vars {
            if kernel.vars[var_id].level >= threshold {
                kernel.vsids.bump_var_score(var_id);
            }
        }

        let lemma = std::mem::take(&mut kernel.lemma);
        debug!("c learned clause: {:?}", lemma);
        let first_lit = lemma[0];
        let clause_id = kernel.add_learned_clause_with_lbd(lemma, lbd);
        if kernel.clauses[clause_id].literals().len() == 1 {
            kernel.learnt = (first_lit, None);
        } else {
            kernel.learnt = (first_lit, Some(clause_id));
        }
        kernel.conflict = None;
        if kernel.statistics.conflicts % 5000 == 0 {
            kernel.vsids.bump_decay_factor();
        }
    }

    fn backtrack(&mut self, kernel: &mut Kernel) {
        debug!(
            "c backtracking to level: {}, and assign literal: {:?}",
            kernel.backtrack_level, kernel.learnt
        );
        while let Some(&lit) = kernel.trail.last() {
            let var_id = lit.unsigned_abs();
            if kernel.vars[var_id].level <= kernel.backtrack_level {
                break;
            }
            kernel.trail.pop();
            kernel.reset_value(var_id);
        }
        kernel.level = kernel.backtrack_level;
        kernel.propagated = kernel.propagated.min(kernel.trail.len());

        let (lit, clause_id) = kernel.learnt;
        kernel.assign(lit.unsigned_abs(), lit, clause_id);
    }

    fn search(&mut self, kernel: &mut Kernel, in_processor: &mut Vec<Box<dyn Pass>>) -> SATResult {
        while kernel.result == SATResult::UNKNOWN {
            if kernel.result == SATResult::UNSAT {
                return SATResult::UNSAT;
            } else if !self.propagate(kernel) {
                self.analyze(kernel);
                if kernel.result == SATResult::UNSAT {
                    return SATResult::UNSAT;
                }
                self.backtrack(kernel);
            } else if kernel.satisfied() {
                return SATResult::SAT;
            } else {
                for pass in in_processor.iter_mut() {
                    if pass.applying(kernel) {
                        pass.apply(kernel);
                    }
                }
                self.decide(kernel);
            }
        }
        kernel.result
    }
}
