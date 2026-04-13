use crate::structures::SATResult;

/// This is the CDCL trait
pub trait Search {
    fn propagate(&mut self);
    fn decide(&mut self);
    fn analyze(&mut self);
    fn backtrack(&mut self);
    fn search(&mut self) -> SATResult;
}
