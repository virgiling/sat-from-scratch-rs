/// 求解器常量定义。
///
/// `SATResult` 数值遵循 SAT Competition 习惯：
/// - `SAT = 10`
/// - `UNSAT = 20`
/// - `UNKNOWN = 0`
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum SATResult {
    SAT = 10,
    UNSAT = 20,
    UNKNOWN = 0,
}

/// VSIDS/EVSIDS 默认参数。
pub const DEFAULT_VAR_INC: f64 = 1.0;
pub const DEFAULT_VAR_DECAY: f64 = 0.8;
pub const DEFAULT_VAR_DECAY_INC: f64 = 0.01;
pub const MAX_VAR_DECAY: f64 = 0.95;

/// 决策初始相位默认值。
pub const INIT_PHASE: bool = false;
