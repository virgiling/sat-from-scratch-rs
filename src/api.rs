use crate::{constants::SATResult, kernel::InnerSolver};

pub trait Pass {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn category(&self) -> &'static str;
    fn applying(&self, kernel: &InnerSolver) -> bool;
    fn apply(&mut self, kernel: &mut InnerSolver) -> SATResult;
}

pub trait Search {
    fn propagate(&mut self, kernel: &mut InnerSolver) -> bool;
    fn decide(&mut self, kernel: &mut InnerSolver);
    fn analyze(&mut self, kernel: &mut InnerSolver);
    fn backtrack(&mut self, kernel: &mut InnerSolver);
    fn search(
        &mut self,
        kernel: &mut InnerSolver,
        in_processor: &mut Vec<Box<dyn Pass>>,
    ) -> SATResult;
}
