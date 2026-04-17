#[derive(Eq, PartialEq, Clone, Copy)]
pub enum SATResult {
    SAT = 10,
    UNSAT = 20,
    UNKNOWN = 0,
}

pub const DEFAULT_VAR_INC: f64 = 1.0;
pub const DEFAULT_VAR_DECAY: f64 = 0.8;
pub const DEFAULT_VAR_DECAY_INC: f64 = 0.01;
pub const MAX_VAR_DECAY: f64 = 0.95;

pub const INIT_PHASE: bool = false;
