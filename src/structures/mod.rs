pub mod clause;
pub mod watch;

pub enum SATResult {
    SAT = 10,
    UNSAT = 20,
    UNKNOWN = 0,
}
