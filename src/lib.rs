pub mod api;
pub mod common;
pub mod constants;
pub mod kernel;
pub mod passes;
pub mod search;
pub mod utils;

/// 求解器使用 Type-State 模式表达状态机，并在编译期约束非法状态迁移。
///
/// 状态转移图：
/// `UNKNOWN -> SOLVING -> {SAT | UNSAT | UNKNOWN}`。
///
/// 典型使用方式：
/// 1. 用 [SolverBuilder] 从 DIMACS 构建内核；
/// 2. 调用 `build().solve()`；
/// 3. 通过 [SolveResult] 分支读取 SAT/UNSAT 结果与模型。
///
/// # 使用方式
/// ```rust
/// let cnf_path = "path/to/cnf/file.cnf";
/// let searcher = Searcher;
/// let solver = SolverBuilder::from_dimacs_file(searcher, &cnf_path)?.build();
/// let result = solver.solve();
/// match result {
///     SolveResult::SAT(solver) => {
///         assert!(solver.check_sat().is_ok());
///         println!("s SATISFIABLE");
///         solver.print_model();
///     }
///     SolveResult::UNSAT(solver) => {
///         println!("s UNSATISFIABLE");
///     }
///     SolveResult::UNKNOWN(solver) => {
///         println!("s UNKNOWN");
///     }
/// }
/// ```
use std::marker::PhantomData;
use std::{fmt, fs, path::Path};

use tracing::info;

use crate::utils::init_logger;
use crate::{
    api::{Pass, Search},
    constants::SATResult,
    kernel::Kernel,
};

/// 求解器外部可见的状态枚举。
///
/// ```mermaid
/// stateDiagram-v2
///     [*] --> UNKNOWN
///     UNKNOWN --> SOLVING: solve()
///     SOLVING --> SAT: 找到满足赋值
///     SOLVING --> UNSAT: 证明不可满足
///     SOLVING --> UNKNOWN: 保守返回/提前终止
/// ```
#[cfg_attr(doc, aquamarine::aquamarine)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverStatus {
    UNKNOWN,
    SOLVING,
    SAT,
    UNSAT,
}

/// Type-State 标记 trait。
pub trait SolverState {
    const STATUS: SolverStatus;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UNKNOWN;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SOLVING;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SAT;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UNSAT;

impl SolverState for UNKNOWN {
    const STATUS: SolverStatus = SolverStatus::UNKNOWN;
}
impl SolverState for SOLVING {
    const STATUS: SolverStatus = SolverStatus::SOLVING;
}
impl SolverState for SAT {
    const STATUS: SolverStatus = SolverStatus::SAT;
}
impl SolverState for UNSAT {
    const STATUS: SolverStatus = SolverStatus::UNSAT;
}

/// 求解结果（按最终状态分型返回）。
pub enum SolveResult<S>
where
    S: Search,
{
    SAT(Solver<S, SAT>),
    UNSAT(Solver<S, UNSAT>),
    UNKNOWN(Solver<S, UNKNOWN>),
}

impl<S> SolveResult<S>
where
    S: Search,
{
    /// 返回结果对应的状态枚举。
    pub const fn status(&self) -> SolverStatus {
        match self {
            Self::SAT(_) => SolverStatus::SAT,
            Self::UNSAT(_) => SolverStatus::UNSAT,
            Self::UNKNOWN(_) => SolverStatus::UNKNOWN,
        }
    }
}

/// 解析 DIMACS 文件时可能出现的错误。
#[derive(Debug)]
pub enum DimacsError {
    MissingHeader,
    ClauseBeforeHeader { line: usize },
    InvalidHeader { line: usize, content: String },
    InvalidLiteral { line: usize, token: String },
    LiteralOutOfRange { line: usize, lit: isize, max_vars: usize },
    UnterminatedClause,
    ClauseCountMismatch { expected: usize, parsed: usize },
    Io(std::io::Error),
}

impl fmt::Display for DimacsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => write!(f, "missing DIMACS header line 'p cnf <vars> <clauses>'"),
            Self::ClauseBeforeHeader { line } => {
                write!(f, "clause appears before header at line {line}")
            }
            Self::InvalidHeader { line, content } => {
                write!(f, "invalid DIMACS header at line {line}: '{content}'")
            }
            Self::InvalidLiteral { line, token } => {
                write!(f, "invalid literal '{token}' at line {line}")
            }
            Self::LiteralOutOfRange { line, lit, max_vars } => {
                write!(f, "literal {lit} out of range at line {line} (max var id {max_vars})")
            }
            Self::UnterminatedClause => write!(f, "unterminated clause: missing trailing 0"),
            Self::ClauseCountMismatch { expected, parsed } => {
                write!(f, "DIMACS clause count mismatch: expected {expected}, parsed {parsed}")
            }
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for DimacsError {}

impl From<std::io::Error> for DimacsError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// 解析 DIMACS CNF 文本并构造 [Kernel]。
///
/// 输入约定：
/// - 头部：`p cnf <vars> <clauses>`
/// - 子句：以 `0` 结束
/// - `c` 开头行为注释
///
/// 例：
/// `p cnf 3 2`
/// `1 -2 0`
/// `2 3 0`
fn parse_dimacs_kernel(input: &str) -> Result<Kernel, DimacsError> {
    let mut kernel: Option<Kernel> = None;
    let mut expected_clauses = 0usize;
    let mut parsed_clauses = 0usize;
    let mut open_clause = false;

    for (idx, raw_line) in input.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('c') {
            continue;
        }

        if line.starts_with('p') {
            if kernel.is_some() {
                return Err(DimacsError::InvalidHeader {
                    line: line_no,
                    content: line.to_string(),
                });
            }

            let mut parts = line.split_whitespace();
            let p = parts.next();
            let cnf = parts.next();
            let vars = parts.next();
            let clauses = parts.next();
            let extra = parts.next();
            if p != Some("p")
                || cnf != Some("cnf")
                || vars.is_none()
                || clauses.is_none()
                || extra.is_some()
            {
                return Err(DimacsError::InvalidHeader {
                    line: line_no,
                    content: line.to_string(),
                });
            }

            let max_vars = vars.and_then(|s| s.parse::<usize>().ok()).ok_or_else(|| {
                DimacsError::InvalidHeader { line: line_no, content: line.to_string() }
            })?;
            expected_clauses = clauses.and_then(|s| s.parse::<usize>().ok()).ok_or_else(|| {
                DimacsError::InvalidHeader { line: line_no, content: line.to_string() }
            })?;
            kernel = Some(Kernel::new(max_vars));
            continue;
        }

        let Some(k) = kernel.as_mut() else {
            return Err(DimacsError::ClauseBeforeHeader { line: line_no });
        };

        for token in line.split_whitespace() {
            let lit = token.parse::<isize>().map_err(|_| DimacsError::InvalidLiteral {
                line: line_no,
                token: token.to_string(),
            })?;
            if lit == 0 {
                k.add(None);
                parsed_clauses += 1;
                open_clause = false;
                continue;
            }

            let var_id = lit.unsigned_abs();
            let max_vars = k.assignment.len() - 1;
            if var_id == 0 || var_id > max_vars {
                return Err(DimacsError::LiteralOutOfRange { line: line_no, lit, max_vars });
            }
            k.add(Some(lit));
            open_clause = true;
        }
    }

    let Some(kernel) = kernel else {
        return Err(DimacsError::MissingHeader);
    };
    if open_clause {
        return Err(DimacsError::UnterminatedClause);
    }
    if parsed_clauses != expected_clauses {
        return Err(DimacsError::ClauseCountMismatch {
            expected: expected_clauses,
            parsed: parsed_clauses,
        });
    }
    Ok(kernel)
}

/// `Solver` 构建器：负责准备搜索器与内核初始状态。
pub struct SolverBuilder<S>
where
    S: Search,
{
    search: S,
    kernel: Kernel,
}

impl<S> SolverBuilder<S>
where
    S: Search,
{
    /// 用变量上限创建空公式求解器构建器。
    pub fn with_max_vars(search: S, max_vars: usize) -> Self {
        Self { search, kernel: Kernel::new(max_vars) }
    }

    /// 从 DIMACS 字符串创建构建器。
    pub fn from_dimacs_str(search: S, dimacs: &str) -> Result<Self, DimacsError> {
        let kernel = parse_dimacs_kernel(dimacs)?;
        Ok(Self { search, kernel })
    }

    /// 从 DIMACS 文件创建构建器。
    pub fn from_dimacs_file(search: S, path: impl AsRef<Path>) -> Result<Self, DimacsError> {
        let input = fs::read_to_string(path)?;
        Self::from_dimacs_str(search, &input)
    }

    /// 只读访问内核（用于调试或查询）。
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    /// 可变访问内核（用于自定义注入子句/参数）。
    pub fn kernel_mut(&mut self) -> &mut Kernel {
        &mut self.kernel
    }

    /// 构建求解器，初始状态为 `UNKNOWN`。
    pub fn build(self) -> Solver<S, UNKNOWN> {
        Solver::new(self.search, self.kernel)
    }
}

/// 主求解器对象。
///
/// 组件说明：
/// - `pre_processor`：搜索前执行；
/// - `in_processor`：搜索循环中周期执行；
/// - `search`：CDCL 核心；
/// - `kernel`：统一状态与数据。
pub struct Solver<S, St = UNKNOWN>
where
    S: Search,
    St: SolverState,
{
    pre_processor: Vec<Box<dyn Pass>>,
    in_processor: Vec<Box<dyn Pass>>,
    search: S,
    kernel: Kernel,
    _state: PhantomData<St>,
}

impl<S, St> Solver<S, St>
where
    S: Search,
    St: SolverState,
{
    /// 返回编译期状态对应的运行时枚举。
    pub const fn status(&self) -> SolverStatus {
        St::STATUS
    }

    /// 只读访问内核。
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    fn into_state<Next>(self) -> Solver<S, Next>
    where
        Next: SolverState,
    {
        Solver {
            pre_processor: self.pre_processor,
            in_processor: self.in_processor,
            search: self.search,
            kernel: self.kernel,
            _state: PhantomData,
        }
    }
}

impl<S> Solver<S, UNKNOWN>
where
    S: Search,
{
    /// 创建处于 `UNKNOWN` 状态的求解器。
    pub fn new(search: S, kernel: Kernel) -> Self {
        init_logger();
        Self {
            pre_processor: Vec::new(),
            in_processor: Vec::new(),
            search,
            kernel,
            _state: PhantomData,
        }
    }

    /// 注册一个预处理 Pass。
    pub fn add_preprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.pre_processor.push(Box::new(pass));
    }

    /// 按逗号分隔短名重排预处理 Pass。
    pub fn arrange_preprocess_passes(&mut self, ordered: &str) {
        Self::arrange_passes(&mut self.pre_processor, ordered);
    }

    /// 注册一个搜索中 Pass。
    pub fn add_inprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.in_processor.push(Box::new(pass));
    }

    /// 按逗号分隔短名重排搜索中 Pass。
    pub fn arrange_inprocess_passes(&mut self, ordered: &str) {
        Self::arrange_passes(&mut self.in_processor, ordered);
    }

    /// 执行完整求解流程并返回分型结果。
    ///
    /// 主要顺序：
    /// 1. 先跑预处理，若可直接判定则提前返回；
    /// 2. 进入 `SOLVING` 状态，执行 CDCL 搜索；
    /// 3. 按最终结果转移到 `SAT`/`UNSAT`/`UNKNOWN` 状态。
    pub fn solve(mut self) -> SolveResult<S> {
        let mut pre_result = SATResult::UNKNOWN;
        for pass in &mut self.pre_processor {
            if pass.applying(&self.kernel) {
                pre_result = pass.apply(&mut self.kernel);
                if pre_result != SATResult::UNKNOWN {
                    break;
                }
            }
        }

        if pre_result == SATResult::SAT {
            return SolveResult::SAT(self.into_state::<SAT>());
        }
        if pre_result == SATResult::UNSAT {
            return SolveResult::UNSAT(self.into_state::<UNSAT>());
        }

        let mut solving = self.into_state::<SOLVING>();
        let result = solving.search.search(&mut solving.kernel, &mut solving.in_processor);

        info!("c solver finished with result: {:?}", result);
        match result {
            SATResult::SAT => SolveResult::SAT(solving.into_state::<SAT>()),
            SATResult::UNSAT => SolveResult::UNSAT(solving.into_state::<UNSAT>()),
            SATResult::UNKNOWN => SolveResult::UNKNOWN(solving.into_state::<UNKNOWN>()),
        }
    }

    /// 根据短名称重排 Pass，未出现在 `ordered` 中的 Pass 会被丢弃。
    fn arrange_passes(passes: &mut Vec<Box<dyn Pass>>, ordered: &str) {
        let mut remaining = std::mem::take(passes);
        let mut ordered_passes: Vec<Box<dyn Pass>> = Vec::with_capacity(remaining.len());

        for short_name in ordered.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            if let Some(idx) = remaining.iter().position(|p| p.name() == short_name) {
                ordered_passes.push(remaining.swap_remove(idx));
            }
        }

        *passes = ordered_passes;
    }
}

impl<S> Solver<S, SAT>
where
    S: Search,
{
    /// 导出模型（索引 0 保留，不对应实际变量）。
    pub fn model(&self) -> Vec<bool> {
        self.kernel.assignment.iter().map(|&value| value == 1).collect()
    }

    /// 用当前模型逐子句校验 SAT 结果。
    pub fn check_sat(&self) -> Result<(), String> {
        let model = self.model();
        for clause in &self.kernel.clauses {
            let mut satisfied = false;
            for &lit in clause.literals() {
                let var = lit.unsigned_abs();
                let val = model[var];
                if (lit > 0 && val) || (lit < 0 && !val) {
                    satisfied = true;
                    break;
                }
            }
            if !satisfied {
                return Err(format!("clause {:?} is not satisfied", clause));
            }
        }
        Ok(())
    }

    /// 按 DIMACS 竞赛风格打印模型。
    pub fn print_model(&self) {
        let model = self.model();
        print!("v ");
        for (v, val) in model.iter().enumerate().skip(1) {
            if *val {
                print!("{} ", v);
            } else {
                print!("-{} ", v);
            }
            if v % 10 == 0 {
                print!("\nv ");
            }
        }
        println!("0");
    }
}
