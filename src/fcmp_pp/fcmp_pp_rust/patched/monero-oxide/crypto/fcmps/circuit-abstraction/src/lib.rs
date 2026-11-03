#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]
#![allow(non_snake_case)]

use std_shims::{vec, vec::Vec};

use zeroize::{Zeroize, ZeroizeOnDrop};

use ciphersuite::{
  group::ff::{Field as _, FromUniformBytes},
  Ciphersuite,
};

use generalized_bulletproofs::{
  PedersenCommitment, PedersenVectorCommitment, ProofGenerators,
  transcript::{Transcript as ProverTranscript, VerifierTranscript, Commitments},
  arithmetic_circuit_proof::{
    AcStatementError, ArithmeticCircuitStatement, ArithmeticCircuitWitness,
  },
};
pub use generalized_bulletproofs::arithmetic_circuit_proof::{Variable, LinComb};

mod gadgets;

/// A trait for the transcript, whether proving for verifying, as necessary for sampling
/// challenges.
pub trait Transcript {
  /// Sample a challenge from the transacript.
  ///
  /// It is the caller's responsibility to have properly transcripted all variables prior to
  /// sampling this challenge.
  fn challenge<C: Ciphersuite>(&mut self) -> C::F
  where
    C::F: FromUniformBytes<64>;

  /// Sample a challenge as a byte array.
  ///
  /// It is the caller's responsibility to have properly transcripted all variables prior to
  /// sampling this challenge.
  fn challenge_bytes(&mut self) -> [u8; 64];
}
impl Transcript for ProverTranscript {
  fn challenge<C: Ciphersuite>(&mut self) -> C::F
  where
    C::F: FromUniformBytes<64>,
  {
    self.challenge::<C>()
  }
  fn challenge_bytes(&mut self) -> [u8; 64] {
    self.challenge_bytes()
  }
}
impl Transcript for VerifierTranscript<'_> {
  fn challenge<C: Ciphersuite>(&mut self) -> C::F
  where
    C::F: FromUniformBytes<64>,
  {
    self.challenge::<C>()
  }
  fn challenge_bytes(&mut self) -> [u8; 64] {
    self.challenge_bytes()
  }
}

/// The witness for the satisfaction of this circuit.
#[derive(Clone, PartialEq, Eq, Debug, Zeroize, ZeroizeOnDrop)]
struct ProverData<C: Ciphersuite> {
  aL: Vec<C::F>,
  aR: Vec<C::F>,
  C: Vec<PedersenVectorCommitment<C>>,
  V: Vec<PedersenCommitment<C>>,
}

/// The constraints of a circuit, folded into a single constraint as they're produced.
///
/// Constraint `k` (zero-indexed) is weighted by `z^k`. `ArithmeticCircuitStatement::verify` weights
/// constraint `k` by `z^(k + 1)`, so a statement with solely the folded constraint is verified
/// against `z * sum(z^k constraint_k) = sum(z^(k + 1) constraint_k)`, the same weighted combination
/// it verifies a statement with every constraint against.
#[derive(Clone, PartialEq, Eq, Debug)]
struct FoldedConstraints<C: Ciphersuite> {
  z: C::F,
  next_weight: C::F,
  constraints: usize,
  WL: Vec<C::F>,
  WR: Vec<C::F>,
  WO: Vec<C::F>,
  WCG: Vec<Vec<C::F>>,
  WV: Vec<C::F>,
  c: C::F,
}

impl<C: Ciphersuite> FoldedConstraints<C> {
  fn new(z: C::F) -> Self {
    Self {
      z,
      next_weight: C::F::ONE,
      constraints: 0,
      WL: vec![],
      WR: vec![],
      WO: vec![],
      WCG: vec![],
      WV: vec![],
      c: C::F::ZERO,
    }
  }

  // `dense += sparse * weight`, growing `dense` as needed.
  fn accumulate(dense: &mut Vec<C::F>, sparse: &[(usize, C::F)], weight: C::F) {
    for (i, value) in sparse {
      if dense.len() <= *i {
        dense.resize(*i + 1, C::F::ZERO);
      }
      dense[*i] += *value * weight;
    }
  }

  fn fold(&mut self, constraint: &LinComb<C::F>) {
    let weight = self.next_weight;
    Self::accumulate(&mut self.WL, constraint.WL(), weight);
    Self::accumulate(&mut self.WR, constraint.WR(), weight);
    Self::accumulate(&mut self.WO, constraint.WO(), weight);
    for (i, values) in constraint.WCG() {
      if self.WCG.len() <= *i {
        self.WCG.resize(*i + 1, vec![]);
      }
      Self::accumulate(&mut self.WCG[*i], values, weight);
    }
    Self::accumulate(&mut self.WV, constraint.WV(), weight);
    self.c += constraint.c() * weight;
    self.next_weight *= self.z;
    self.constraints += 1;
  }

  // The folded constraint.
  //
  // Every index referenced by any constraint is present, even if its folded weight is zero, so the
  // statement bounds-checks the same highest indices it would have for the individual constraints.
  fn into_lincomb(self) -> LinComb<C::F> {
    let mut res = LinComb::empty().constant(self.c);
    for (i, weight) in self.WL.into_iter().enumerate() {
      res = res.term(weight, Variable::aL(i));
    }
    for (i, weight) in self.WR.into_iter().enumerate() {
      res = res.term(weight, Variable::aR(i));
    }
    for (i, weight) in self.WO.into_iter().enumerate() {
      res = res.term(weight, Variable::aO(i));
    }
    for (commitment, weights) in self.WCG.into_iter().enumerate() {
      for (index, weight) in weights.into_iter().enumerate() {
        res = res.term(weight, Variable::CG { commitment, index });
      }
    }
    for (i, weight) in self.WV.into_iter().enumerate() {
      res = res.term(weight, Variable::V(i));
    }
    res
  }
}

/// How a circuit handles the constraints it's given.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Constraints<C: Ciphersuite> {
  /// Every constraint is stored.
  Stored(Vec<LinComb<C::F>>),
  /// The constraints are counted, not stored.
  Discarded(usize),
  /// The constraints are folded into one as they're produced.
  Folded(FoldedConstraints<C>),
}

/// An error incurred when obtaining the statement for a circuit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CircuitStatementError {
  /// The circuit discarded its constraints, so no statement can be formed from it.
  ConstraintsDiscarded,
  /// The circuit folded its constraints yet was given Pedersen commitments, which folding doesn't
  /// support (the statement's check that each is individually constrained needs the individual
  /// constraints).
  FoldedWithPedersenCommitments,
  /// An error from constructing the arithmetic circuit statement.
  AcStatementError(AcStatementError),
}
impl From<AcStatementError> for CircuitStatementError {
  fn from(err: AcStatementError) -> Self {
    CircuitStatementError::AcStatementError(err)
  }
}

/// A struct representing a circuit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Circuit<C: Ciphersuite> {
  muls: usize,
  // A series of linear combinations which must evaluate to 0.
  constraints: Constraints<C>,
  prover: Option<ProverData<C>>,
}

impl<C: Ciphersuite> Circuit<C> {
  /// Returns the amount of multiplications used by this circuit.
  pub fn muls(&self) -> usize {
    self.muls
  }

  /// Returns the amount of constraints given to this circuit.
  pub fn constraints(&self) -> usize {
    match &self.constraints {
      Constraints::Stored(constraints) => constraints.len(),
      Constraints::Discarded(constraints) => *constraints,
      Constraints::Folded(folded) => folded.constraints,
    }
  }

  /// Create an instance to prove satisfaction of a circuit with.
  #[allow(clippy::type_complexity)]
  pub fn prove(
    vector_commitments: Vec<PedersenVectorCommitment<C>>,
    commitments: Vec<PedersenCommitment<C>>,
  ) -> Self {
    Self {
      muls: 0,
      constraints: Constraints::Stored(vec![]),
      prover: Some(ProverData { aL: vec![], aR: vec![], C: vector_commitments, V: commitments }),
    }
  }

  /// Create an instance to verify a proof with.
  pub fn verify() -> Self {
    Self { muls: 0, constraints: Constraints::Stored(vec![]), prover: None }
  }

  /// Create an instance to verify a proof with which counts its constraints without storing them.
  ///
  /// No statement can be obtained from it. This allows advancing a transcript through the
  /// construction of a circuit without the memory to store its constraints, as needed before
  /// `Circuit::verify_folding` can be used.
  pub fn verify_discarding() -> Self {
    Self { muls: 0, constraints: Constraints::Discarded(0), prover: None }
  }

  /// Create an instance to verify a proof with which folds its constraints into one as they're
  /// produced, weighting constraint `k` (zero-indexed) by `z^k`.
  ///
  /// `z` MUST be the challenge `ArithmeticCircuitStatement::verify` will sample for this proof
  /// (see `generalized_bulletproofs::arithmetic_circuit_proof::peek_z`). The statement obtained
  /// then has that single constraint, which `verify` weights by `z`, verifying against the same
  /// weighted combination of constraints as it would for the individual constraints, while only
  /// the folded constraint is ever held in memory.
  ///
  /// Pedersen commitments (`V`) aren't supported in this mode.
  pub fn verify_folding(z: C::F) -> Self {
    Self { muls: 0, constraints: Constraints::Folded(FoldedConstraints::new(z)), prover: None }
  }

  /// Evaluate a linear combination.
  ///
  /// Yields WL aL + WR aR + WO aO + WCG CG + WV V + c.
  ///
  /// May panic if the linear combination references non-existent terms.
  ///
  /// Returns None if not a prover.
  pub fn eval(&self, lincomb: &LinComb<C::F>) -> Option<C::F> {
    self.prover.as_ref().map(|prover| {
      let mut res = lincomb.c();
      for (index, weight) in lincomb.WL() {
        res += prover.aL[*index] * weight;
      }
      for (index, weight) in lincomb.WR() {
        res += prover.aR[*index] * weight;
      }
      for (index, weight) in lincomb.WO() {
        res += prover.aL[*index] * prover.aR[*index] * weight;
      }
      for (i, C) in prover.C.iter().enumerate() {
        for (j, weight) in lincomb.WCG().get(&i).into_iter().flat_map(|values| values.iter()) {
          res += C.g_values[*j] * weight;
        }
      }
      for (index, weight) in lincomb.WV() {
        res += prover.V[*index].value * weight;
      }
      res
    })
  }

  /// Multiply two values, optionally constrained, returning the constrainable left/right/out
  /// terms.
  ///
  /// May panic if any linear combinations reference non-existent terms or if the witness isn't
  /// provided when proving/is provided when verifying.
  #[allow(clippy::many_single_char_names)]
  pub fn mul(
    &mut self,
    a: Option<LinComb<C::F>>,
    b: Option<LinComb<C::F>>,
    witness: Option<(C::F, C::F)>,
  ) -> (Variable, Variable, Variable) {
    let l = Variable::aL(self.muls);
    let r = Variable::aR(self.muls);
    let o = Variable::aO(self.muls);
    self.muls += 1;

    debug_assert_eq!(self.prover.is_some(), witness.is_some());
    if let Some(witness) = witness {
      let prover = self.prover.as_mut().unwrap();
      prover.aL.push(witness.0);
      prover.aR.push(witness.1);
    }

    if let Some(a) = a {
      self.constrain_equal_to_zero(a.term(-C::F::ONE, l));
    }
    if let Some(b) = b {
      self.constrain_equal_to_zero(b.term(-C::F::ONE, r));
    }

    (l, r, o)
  }

  /// Constrain a linear combination to be equal to 0.
  ///
  /// May panic if the linear combination references non-existent terms.
  pub fn constrain_equal_to_zero(&mut self, lincomb: LinComb<C::F>) {
    match &mut self.constraints {
      Constraints::Stored(constraints) => constraints.push(lincomb),
      Constraints::Discarded(constraints) => *constraints += 1,
      Constraints::Folded(folded) => folded.fold(&lincomb),
    }
  }
}

impl<C: Ciphersuite> Circuit<C>
where
  C::F: FromUniformBytes<64>,
{
  /// Obtain the statement for this circuit.
  ///
  /// If configured as the prover, the witness to use is also returned if one was successfully
  /// constructed.
  ///
  /// A circuit which folded its constraints yields a statement with the single, folded constraint.
  #[allow(clippy::type_complexity)]
  pub fn statement(
    self,
    generators: ProofGenerators<'_, C>,
    commitments: Commitments<C>,
  ) -> Result<
    (ArithmeticCircuitStatement<'_, C>, Option<ArithmeticCircuitWitness<C>>),
    CircuitStatementError,
  > {
    let constraints = match self.constraints {
      Constraints::Stored(constraints) => constraints,
      Constraints::Discarded(_) => return Err(CircuitStatementError::ConstraintsDiscarded),
      Constraints::Folded(folded) => {
        if !commitments.V().is_empty() {
          return Err(CircuitStatementError::FoldedWithPedersenCommitments);
        }
        vec![folded.into_lincomb()]
      }
    };
    let statement = ArithmeticCircuitStatement::new(generators, constraints, commitments)?;

    let witness = self.prover.and_then(|mut prover| {
      // We can't deconstruct the witness as it implements Drop (per ZeroizeOnDrop)
      // Accordingly, we take the values within it and move forward with those
      let mut aL = vec![];
      core::mem::swap(&mut prover.aL, &mut aL);
      let mut aR = vec![];
      core::mem::swap(&mut prover.aR, &mut aR);
      let mut C = vec![];
      core::mem::swap(&mut prover.C, &mut C);
      let mut V = vec![];
      core::mem::swap(&mut prover.V, &mut V);
      ArithmeticCircuitWitness::new(aL, aR, C, V)
    });

    Ok((statement, witness))
  }
}
