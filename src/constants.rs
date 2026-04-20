/// This module contains the constants for the solver.
///
/// This is the result of the solver.
/// We follow the competition standard:
/// - SAT: 10
/// - UNSAT: 20
/// - UNKNOWN: 0
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum SATResult {
    SAT = 10,
    UNSAT = 20,
    UNKNOWN = 0,
}

/// These following value is the DEFAULT value for solver's parameters.
///
/// These are the DEFAULT value for VSIDS.
pub const DEFAULT_VAR_INC: f64 = 1.0;
pub const DEFAULT_VAR_DECAY: f64 = 0.8;
pub const DEFAULT_VAR_DECAY_INC: f64 = 0.01;
pub const MAX_VAR_DECAY: f64 = 0.95;

/// This is the DEFAULT value for the initial phase.
pub const INIT_PHASE: bool = false;
