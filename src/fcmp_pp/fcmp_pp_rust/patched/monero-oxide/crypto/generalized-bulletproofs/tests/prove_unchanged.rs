//! The arithmetic circuit prover's output, byte for byte, for fixed seeds.
//!
//! The expected bytes in `tests/data/prove_unchanged.hex` were made by the prover as it was before monerodato.4's memory
//! changes to `ArithmeticCircuitStatement::prove` (monerodato.3, the same code as upstream there). Those changes only
//! move, shrink and free buffers, so the proofs must stay identical. Set `GBP_WRITE_PROVE_UNCHANGED=1` to rewrite the file
//! from the current code (only ever from a prover known to be unchanged).

#![allow(non_snake_case)]

use rand_core::{CryptoRng, RngCore};

use blake2::{Digest as _, Blake2b512};
use ciphersuite::{group::ff::Field as _, Ciphersuite, Ristretto};

use generalized_bulletproofs::{
  PedersenCommitment, PedersenVectorCommitment, Generators,
  transcript::*,
  arithmetic_circuit_proof::{Variable, LinComb, ArithmeticCircuitStatement, ArithmeticCircuitWitness},
  tests::insecure_test_generators,
};

/// A deterministic RNG for this test only: Blake2b-512 of a seed and a block counter. Not for any other use.
struct TestRng {
  seed: [u8; 32],
  counter: u64,
  buf: [u8; 64],
  pos: usize,
}

impl TestRng {
  fn new(seed: u8) -> Self {
    TestRng { seed: [seed; 32], counter: 0, buf: [0; 64], pos: 64 }
  }
}

impl RngCore for TestRng {
  fn next_u32(&mut self) -> u32 {
    let mut b = [0; 4];
    self.fill_bytes(&mut b);
    u32::from_le_bytes(b)
  }
  fn next_u64(&mut self) -> u64 {
    let mut b = [0; 8];
    self.fill_bytes(&mut b);
    u64::from_le_bytes(b)
  }
  fn fill_bytes(&mut self, dest: &mut [u8]) {
    for byte in dest {
      if self.pos == 64 {
        let mut h = Blake2b512::new();
        h.update(self.seed);
        h.update(self.counter.to_le_bytes());
        self.buf.copy_from_slice(&h.finalize());
        self.counter += 1;
        self.pos = 0;
      }
      *byte = self.buf[self.pos];
      self.pos += 1;
    }
  }
  fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
    self.fill_bytes(dest);
    Ok(())
  }
}
impl CryptoRng for TestRng {}

/// One proof from a circuit built as in `fuzz_test_arithmetic_circuit`, drawn from `rng`. It is also verified.
fn prove_one(rng: &mut TestRng, generators: &Generators<Ristretto>) -> Vec<u8> {
  let F = |rng: &mut TestRng| <Ristretto as Ciphersuite>::F::random(rng);

  let mut aL = vec![];
  let mut aR = vec![];
  while aL.len() < usize::try_from((rng.next_u64() % 8) + 1).unwrap() {
    aL.push(F(rng));
  }
  while aR.len() < aL.len() {
    aR.push(F(rng));
  }
  let aO = aL.iter().copied().zip(&aR).map(|(l, r)| l * r).collect::<Vec<_>>();

  // At least two vector commitments, so the commitment paths of the prover are always taken
  let mut C = vec![];
  while C.len() < usize::try_from((rng.next_u64() % 15) + 2).unwrap() {
    let mut g_values = vec![];
    while g_values.len() < usize::try_from((rng.next_u64() % 16) + 1).unwrap() {
      g_values.push(F(rng));
    }
    C.push(PedersenVectorCommitment { g_values, mask: F(rng) });
  }

  let mut constraints = vec![];
  let mut V = vec![];
  while V.len() < usize::try_from(rng.next_u64() % 4).unwrap() {
    V.push(PedersenCommitment { value: F(rng), mask: F(rng) });
    let constraint = LinComb::empty()
      .term(-<Ristretto as Ciphersuite>::F::ONE, Variable::V(V.len() - 1))
      .constant(V[V.len() - 1].value);
    constraints.push(constraint);
  }

  for _ in 0 .. (rng.next_u64() % 8) + 1 {
    let mut eval = <Ristretto as Ciphersuite>::F::ZERO;
    let mut constraint = LinComb::empty();
    for _ in 0 .. (rng.next_u64() % 4) {
      let index = usize::try_from(rng.next_u64() % u64::try_from(aL.len()).unwrap()).unwrap();
      let weight = F(rng);
      constraint = constraint.term(weight, Variable::aL(index));
      eval += weight * aL[index];
    }
    for _ in 0 .. (rng.next_u64() % 4) {
      let index = usize::try_from(rng.next_u64() % u64::try_from(aR.len()).unwrap()).unwrap();
      let weight = F(rng);
      constraint = constraint.term(weight, Variable::aR(index));
      eval += weight * aR[index];
    }
    for _ in 0 .. (rng.next_u64() % 4) {
      let index = usize::try_from(rng.next_u64() % u64::try_from(aO.len()).unwrap()).unwrap();
      let weight = F(rng);
      constraint = constraint.term(weight, Variable::aO(index));
      eval += weight * aO[index];
    }
    for (commitment, c) in C.iter().enumerate() {
      for _ in 0 .. (rng.next_u64() % 4) {
        let index = usize::try_from(rng.next_u64() % u64::try_from(c.g_values.len()).unwrap()).unwrap();
        let weight = F(rng);
        constraint = constraint.term(weight, Variable::CG { commitment, index });
        eval += weight * c.g_values[index];
      }
    }
    if !V.is_empty() {
      for _ in 0 .. (rng.next_u64() % 4) {
        let index = usize::try_from(rng.next_u64() % u64::try_from(V.len()).unwrap()).unwrap();
        let weight = F(rng);
        constraint = constraint.term(weight, Variable::V(index));
        eval += weight * V[index].value;
      }
    }
    constraints.push(constraint.constant(-eval));
  }

  let mut transcript = Transcript::new([0; 32]);
  let commitments = transcript.write_commitments(
    C.iter().map(|c| c.commit(generators.g_bold_slice(), generators.h()).unwrap()).collect(),
    V.iter().map(|v| v.commit(generators.g(), generators.h())).collect(),
  );
  let statement =
    ArithmeticCircuitStatement::<Ristretto>::new(generators.reduce(16).unwrap(), constraints, commitments).unwrap();
  let witness = ArithmeticCircuitWitness::<Ristretto>::new(aL, aR, C.clone(), V.clone()).unwrap();
  statement.clone().prove(rng, &mut transcript, witness).unwrap();
  let proof = transcript.complete();

  let mut verifier = Generators::batch_verifier();
  let mut vt = VerifierTranscript::new([0; 32], &proof);
  vt.read_commitments::<Ristretto>(C.len(), V.len()).unwrap();
  statement.verify(rng, &mut verifier, &mut vt).unwrap();
  assert!(generators.verify(verifier));
  proof
}

#[test]
fn prove_output_unchanged() {
  let mut all = vec![];
  for seed in 1 ..= 4u8 {
    let mut rng = TestRng::new(seed);
    let generators = insecure_test_generators(&mut rng, 32).unwrap();
    for _ in 0 .. 2 {
      let proof = prove_one(&mut rng, &generators);
      all.extend_from_slice(&u32::try_from(proof.len()).unwrap().to_le_bytes());
      all.extend_from_slice(&proof);
    }
  }
  let hex: String = all.iter().map(|b| format!("{b:02x}")).collect();
  let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/prove_unchanged.hex");
  if std::env::var("GBP_WRITE_PROVE_UNCHANGED").as_deref() == Ok("1") {
    std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data")).unwrap();
    std::fs::write(path, format!("{hex}\n")).unwrap();
    println!("wrote {} proof bytes to {path}", all.len());
    return;
  }
  let expected = std::fs::read_to_string(path).expect("tests/data/prove_unchanged.hex");
  assert_eq!(expected.trim().len(), hex.len(), "the proofs' total length changed");
  assert!(expected.trim() == hex, "the prover's output changed");
}
