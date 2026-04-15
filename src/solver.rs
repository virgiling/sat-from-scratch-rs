use std::collections::HashMap;

use crate::{
    api::{Pass, Search},
    common::Variable,
    constants::SATResult,
    kernel::InnerSolver,
};

pub struct Solver<S>
where
    S: Search,
{
    pub pre_processor: HashMap<String, Box<dyn Pass>>,
    pub in_processor: HashMap<String, Box<dyn Pass>>,
    pub search: S,
    kernel: InnerSolver,
}

impl<S> Solver<S>
where
    S: Search,
{
    pub fn add_preprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.pre_processor
            .insert(pass.name().to_string(), Box::new(pass));
    }

    pub fn add_inprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.in_processor
            .insert(pass.name().to_string(), Box::new(pass));
    }

    pub fn solve(&mut self) -> SATResult {
        // TODO: Should arrange the passes by priority
        self.search.search(&mut self.kernel)
    }

    pub fn model(&self) -> &[Variable] {
        &self.kernel.assignment
    }
}
