/// This module contains the Clause struct and its associated methods.
use std::ops::{Index, IndexMut};

pub type Literal = i32;

#[derive(Debug, Clone, Default)]
/// A clause is a disjunction of literals.
pub struct Clause {
    /// The LBD of the clause.
    lbd: u32,
    /// The literals of the clause.
    literals: Vec<Literal>,
    /// The reason for the clause.
    reason: Option<Literal>,
}

impl Clause {
    pub fn new() -> Self {
        Self {
            lbd: 0,
            literals: Vec::new(),
            reason: None,
        }
    }

    pub fn with_literals(mut self, literals: Vec<Literal>) -> Self {
        self.literals = literals;
        self
    }

    pub fn with_lbd(mut self, lbd: u32) -> Self {
        self.lbd = lbd;
        self
    }

    pub fn with_reason(mut self, reason: Literal) -> Self {
        self.reason = Some(reason);
        self
    }

    pub fn lbd(&self) -> u32 {
        self.lbd
    }

    pub fn literals(&self) -> &[Literal] {
        &self.literals
    }

    pub fn reason(&self) -> Option<Literal> {
        self.reason
    }
}

impl IntoIterator for Clause {
    type Item = Literal;
    type IntoIter = std::vec::IntoIter<Literal>;

    fn into_iter(self) -> Self::IntoIter {
        self.literals.into_iter()
    }
}

impl<'a> IntoIterator for &'a Clause {
    type Item = &'a Literal;
    type IntoIter = std::slice::Iter<'a, Literal>;

    fn into_iter(self) -> Self::IntoIter {
        self.literals.iter()
    }
}

impl Index<usize> for Clause {
    type Output = Literal;

    fn index(&self, index: usize) -> &Self::Output {
        &self.literals[index]
    }
}

impl IndexMut<usize> for Clause {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.literals[index]
    }
}
