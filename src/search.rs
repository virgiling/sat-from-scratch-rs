use std::mem::swap;

use crate::{
    api::{Pass, Search},
    common::Watches,
    constants::SATResult,
    kernel::Kernel,
};

pub struct Searcher;

impl Search for Searcher {
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
                if kernel.assignment[blocker.unsigned_abs() as usize] == 1 {
                    // Now, this clause is satisfied, we should remove it from the watch list
                    ws[j] = std::mem::take(&mut ws[i]);
                    i += 1;
                    j += 1;
                    continue;
                }
                let clause_id = ws[i].clause_id;
                let clause = &mut kernel.clauses[clause_id];
                if clause[0] == -lit {
                    // we should make sure the second literal is -lit
                    clause[0] = clause[1];
                    clause[1] = -lit;
                }
                let w: Watches = Watches {
                    clause_id,
                    blocker: clause[0],
                };
                i += 1;
                if kernel.assignment[clause[0].unsigned_abs() as usize] == 1 {
                    ws[j] = w;
                    j += 1;
                    continue;
                }
            }
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
        kernel.result.clone()
    }
}
