use crate::{constants::SATResult, kernel::Kernel};

pub trait Pass {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn category(&self) -> &'static str;
    fn applying(&self, kernel: &Kernel) -> bool;
    fn apply(&mut self, kernel: &mut Kernel) -> SATResult;
}

pub trait Search {
    fn propagate(&mut self, kernel: &mut Kernel) -> bool;
    fn decide(&mut self, kernel: &mut Kernel);
    fn analyze(&mut self, kernel: &mut Kernel);
    fn backtrack(&mut self, kernel: &mut Kernel);
    fn search(&mut self, kernel: &mut Kernel, in_processor: &mut Vec<Box<dyn Pass>>) -> SATResult;
}
