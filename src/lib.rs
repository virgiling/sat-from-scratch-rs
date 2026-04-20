pub mod api;
pub mod common;
pub mod constants;
pub mod kernel;
pub mod passes;
pub mod search;
pub mod utils;

/// The solver use the Type-State pattern to represent the solver's status and constrain the state transition. It will be checked at compile time, which is a zero cost abstraction.
/// The transition between states is as follows, which are defined in the [SolverStatus] enum:
/// UNKNOWN -> SOLVING -> {SAT | UNSAT | UNKNOWN}
use std::marker::PhantomData;
use std::{fmt, fs, path::Path};

use tracing::info;

use crate::utils::init_logger;
use crate::{
    api::{Pass, Search},
    constants::SATResult,
    kernel::Kernel,
};

/// This is the status of the solver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverStatus {
    UNKNOWN,
    SOLVING,
    SAT,
    UNSAT,
}

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
    pub const fn status(&self) -> SolverStatus {
        match self {
            Self::SAT(_) => SolverStatus::SAT,
            Self::UNSAT(_) => SolverStatus::UNSAT,
            Self::UNKNOWN(_) => SolverStatus::UNKNOWN,
        }
    }
}

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
    pub fn with_max_vars(search: S, max_vars: usize) -> Self {
        Self { search, kernel: Kernel::new(max_vars) }
    }

    pub fn from_dimacs_str(search: S, dimacs: &str) -> Result<Self, DimacsError> {
        let kernel = parse_dimacs_kernel(dimacs)?;
        Ok(Self { search, kernel })
    }

    pub fn from_dimacs_file(search: S, path: impl AsRef<Path>) -> Result<Self, DimacsError> {
        let input = fs::read_to_string(path)?;
        Self::from_dimacs_str(search, &input)
    }

    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    pub fn kernel_mut(&mut self) -> &mut Kernel {
        &mut self.kernel
    }

    pub fn build(self) -> Solver<S, UNKNOWN> {
        Solver::new(self.search, self.kernel)
    }
}

/// This is the main solver struct. It contains the pre-processor, in-processor, search, kernel
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
    pub const fn status(&self) -> SolverStatus {
        St::STATUS
    }

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

    pub fn add_preprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.pre_processor.push(Box::new(pass));
    }

    pub fn arrange_preprocess_passes(&mut self, ordered: &str) {
        Self::arrange_passes(&mut self.pre_processor, ordered);
    }

    pub fn add_inprocess_pass(&mut self, pass: impl Pass + 'static) {
        self.in_processor.push(Box::new(pass));
    }

    pub fn arrange_inprocess_passes(&mut self, ordered: &str) {
        Self::arrange_passes(&mut self.in_processor, ordered);
    }

    /// Type-state transition:
    /// Unknown -> Solving -> {Sat | Unsat | Unknown}
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
    pub fn model(&self) -> Vec<bool> {
        self.kernel.assignment.iter().map(|&value| value == 1).collect()
    }

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
