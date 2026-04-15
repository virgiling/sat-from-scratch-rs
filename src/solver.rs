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
    pub pre_processor: Vec<Box<dyn Pass>>,
    pub in_processor: Vec<Box<dyn Pass>>,
    pub search: S,
    kernel: InnerSolver,
}

impl<S> Solver<S>
where
    S: Search,
{
    pub fn add_preprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.pre_processor.push(Box::new(pass));
    }

    pub fn arrange_preprocess_passes(&mut self, ordered: &str) {
        let mut remaining = std::mem::take(&mut self.pre_processor);
        let mut ordered_passes: Vec<Box<dyn Pass>> = Vec::with_capacity(remaining.len());

        for short_name in ordered.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(idx) = remaining.iter().position(|p| p.name() == short_name) {
                ordered_passes.push(remaining.swap_remove(idx));
            }
        }
        // NOTE: we do not extend the remaining passes
        self.pre_processor = ordered_passes;
    }

    pub fn add_inprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.in_processor.push(Box::new(pass));
    }

    pub fn arrange_inprocess_passes(&mut self, ordered: &str) {
        let mut remaining = std::mem::take(&mut self.in_processor);
        let mut ordered_passes: Vec<Box<dyn Pass>> = Vec::with_capacity(remaining.len());

        for short_name in ordered.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(idx) = remaining.iter().position(|p| p.name() == short_name) {
                ordered_passes.push(remaining.swap_remove(idx));
            }
        }
        // NOTE: we do not extend the remaining passes
        self.in_processor = ordered_passes;
    }

    pub fn solve(&mut self) -> SATResult {
        // TODO: Should arrange the passes by priority
        self.search.search(&mut self.kernel, &mut self.in_processor)
    }

    pub fn model(&self) -> &[Variable] {
        &self.kernel.assignment
    }
}
