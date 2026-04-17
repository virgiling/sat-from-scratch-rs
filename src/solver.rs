use std::marker::PhantomData;

use crate::{
    api::{Pass, Search},
    constants::SATResult,
    kernel::Kernel,
};

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

/// This is the main solver struct. It contains the pre-processor, in-processor, search, kernel
/// We use type-state pattern to represent the solver's status and constrain the state transition. It will be checked at compile time, which is a zero cost abstraction.
/// The transition between states is as follows:
/// UNKNOWN -> SOLVING -> {SAT | UNSAT | UNKNOWN}
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
        let result = solving
            .search
            .search(&mut solving.kernel, &mut solving.in_processor);

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
        self.kernel
            .assignment
            .iter()
            .map(|&value| value == 1)
            .collect()
    }
}
