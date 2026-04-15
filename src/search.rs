use crate::{
    api::{Pass, Search},
    constants::SATResult,
    kernel::InnerSolver,
};

pub struct Searcher;

impl Search for Searcher {
    fn propagate(&mut self, kernel: &mut InnerSolver) -> bool {
        todo!()
    }

    fn decide(&mut self, kernel: &mut InnerSolver) {
        todo!()
    }

    fn analyze(&mut self, kernel: &mut InnerSolver) {
        todo!()
    }

    fn backtrack(&mut self, kernel: &mut InnerSolver) {
        todo!()
    }

    fn search(
        &mut self,
        kernel: &mut InnerSolver,
        in_processor: &mut Vec<Box<dyn Pass>>,
    ) -> SATResult {
        let mut result = SATResult::UNKNOWN;

        while result == SATResult::UNKNOWN {
            if kernel.unsat {
                result = SATResult::UNSAT;
            } else if !self.propagate(kernel) {
                self.analyze(kernel);
            } else if kernel.satisfied() {
                result = SATResult::SAT;
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
        result
    }
}
