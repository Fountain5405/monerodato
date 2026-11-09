// G009 structured fuzz: BatchVerifier::merge + Generators::verify_split against
// per-batch verification (the pre-G009 semantics), per the four claims in the kit.
//
// Added by the second reviewer 2026-11-08 for the G009 review.
// Rounds run on 4 scoped threads (each round independent, seeded by round number,
// so every round is deterministic regardless of scheduling). Per-round progress
// is printed.
// Scenarios:
//   A. k batches x mixed shapes, none/one/multiple malleated proofs (parseable
//      byte mutations): merged verdict (split 1..8) must equal the AND of
//      per-batch verdicts, and merged order must not matter.
//   B. field-level corruption of a single batch's verifier (every field kind,
//      random index): the merged verdict must fail for every split.
//   C. merge with a fresh empty verifier (padding edge): verdict unchanged.
//   D. split sanity on real merged state (separate test below).
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rand_core::{OsRng, RngCore};

use ciphersuite::{group::ff::Field as _, Ciphersuite};
use helioselene::{Selene, Helios};

use crate::{Fcmp, FcmpParams, Input, TreeRoot};

use super::{random_params, random_proof, DeterministicRng};

type V1 = generalized_bulletproofs::BatchVerifier<Selene>;
type V2 = generalized_bulletproofs::BatchVerifier<Helios>;
type ProofStmt = (
  Fcmp<super::MoneroCurves>,
  TreeRoot<Selene, Helios>,
  usize,
  Vec<Input<<Selene as Ciphersuite>::F>>,
);
type PtG = <dalek_ff_group::Ed25519 as Ciphersuite>::G;
type Gtuv = (PtG, PtG, PtG, PtG);

fn run_batch(params: &FcmpParams<super::MoneroCurves>, batch: &[ProofStmt]) -> Option<(V1, V2)> {
  let mut v1 = generalized_bulletproofs::Generators::batch_verifier();
  let mut v2 = generalized_bulletproofs::Generators::batch_verifier();
  for (proof, root, layers, inputs) in batch {
    proof
      .verify(&mut OsRng, &mut v1, &mut v2, params, *root, *layers, inputs)
      .ok()?;
  }
  Some((v1, v2))
}

fn verdict(params: &FcmpParams<super::MoneroCurves>, merged: &(V1, V2), split: usize) -> bool {
  if split > 1 {
    params.curve_1_generators.verify_split(merged.0.clone(), split)
      && params.curve_2_generators.verify_split(merged.1.clone(), split)
  } else {
    params.curve_1_generators.verify(merged.0.clone())
      && params.curve_2_generators.verify(merged.1.clone())
  }
}

fn merge_all(states: &mut Vec<(V1, V2)>, order: &[usize]) -> (V1, V2) {
  let mut merged = states[order[0]].clone();
  for &i in &order[1..] {
    merged.0.merge(states[i].0.clone());
    merged.1.merge(states[i].1.clone());
  }
  merged
}

const SHAPES: &[(usize, usize)] = &[(1, 2), (2, 3), (3, 2), (4, 3), (8, 2), (8, 3), (16, 2)];
const ROUNDS: usize = 40;
const WORKERS: usize = 4;

fn one_round(
  round: usize,
  params: &FcmpParams<super::MoneroCurves>,
  gtuv: &Gtuv,
) -> (usize, usize, usize, usize) {
  let mut rng = DeterministicRng {
    seed: [round as u8, 0x47, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    counter: 0,
  };
  let mut total_cases = 0usize;
  let mut malleated_rejected = 0usize;
  let mut divergences = 0usize;
  let mut max_terms = 0usize;

  let n_batches = 1 + (rng.next_u64() as usize % 3);
  let mut batches: Vec<Vec<ProofStmt>> = vec![];
  for _ in 0..n_batches {
    let mut batch = vec![];
    let n_proofs = 1 + (rng.next_u64() as usize % 3);
    for _ in 0..n_proofs {
      let (paths, layers) = SHAPES[rng.next_u64() as usize % SHAPES.len()];
      let (proof, root, inputs) = random_proof(params, *gtuv, paths, layers);
      batch.push((proof, root, layers, inputs));
    }
    batches.push(batch);
  }

  let n_bad = (rng.next_u64() as usize % 3).min(n_batches);
  for _ in 0..n_bad {
    let b = rng.next_u64() as usize % n_batches;
    let p = rng.next_u64() as usize % batches[b].len();
    let paths = batches[b][p].3.len();
    let layers = batches[b][p].2;
    let mut bytes = vec![];
    batches[b][p].0.write(&mut bytes).unwrap();
    let mut ok = false;
    for _ in 0..64 {
      let mut m = bytes.clone();
      let flips = 1 + (rng.next_u64() as usize % 3);
      for _ in 0..flips {
        let off = rng.next_u64() as usize % m.len();
        let bit = rng.next_u32() % 8;
        m[off] ^= 1 << bit;
      }
      if let Ok(t) = Fcmp::<super::MoneroCurves>::read(&mut m.as_slice(), paths, layers) {
        batches[b][p].0 = t;
        ok = true;
        break;
      }
    }
    if !ok {
      malleated_rejected += 1;
    }
  }

  let mut states: Vec<(V1, V2)> = vec![];
  let mut old_all = true;
  let mut per_batch_detail: Vec<String> = vec![];
  for batch in &batches {
    match run_batch(params, batch) {
      Some((v1, v2)) => {
        let ok1 = params.curve_1_generators.verify(v1.clone());
        let ok2 = params.curve_2_generators.verify(v2.clone());
        per_batch_detail.push(format!(
          "ok c1={ok1} c2={ok2} len1=({},{},{},{})",
          v1.g_bold.len(),
          v1.h_bold.len(),
          v1.h_sum.len(),
          v1.additional.len()
        ));
        if !ok1 || !ok2 {
          old_all = false;
        }
        states.push((v1, v2));
      }
      None => {
        per_batch_detail.push("PERPROOF-FAIL".to_string());
        old_all = false;
      }
    }
  }

  if !states.is_empty() {
    // G009 semantics: any batch whose per-proof checks failed yields a null state,
    // and the C++ returns false before finish. Encode that: the new verdict is
    // false whenever any batch was dropped.
    let any_failed = states.len() != batches.len();
    let order: Vec<usize> = (0..states.len()).collect();
    let mut rev = order.clone();
    rev.reverse();
    for split in [1usize, 2, 3, 5, 8] {
      let m1 = merge_all(&mut states, &order);
      let m2 = merge_all(&mut states, &rev);
      max_terms = max_terms
        .max(m1.0.g_bold.len() + m1.0.h_bold.len() + m1.0.h_sum.len() + m1.0.additional.len());
      let new1 = !any_failed && verdict(params, &m1, split);
      let new2 = !any_failed && verdict(params, &m2, split);
      total_cases += 1;
      if new1 != old_all || new2 != old_all || new1 != new2 {
        divergences += 1;
        eprintln!(
          "DIVERGENCE round {round} split {split}: old {old_all} new {new1}/{new2} (any_failed {any_failed}) batches: {per_batch_detail:?}"
        );
      }
    }

    let victim = rng.next_u64() as usize % states.len();
    {
      let mut bad = states[victim].clone();
      let which = rng.next_u64() % 8;
      match which {
        0 => bad.0.g += <Selene as Ciphersuite>::F::from(1u64),
        1 => bad.0.h += <Selene as Ciphersuite>::F::from(1u64),
        2 => {
          let i = rng.next_u64() as usize % bad.0.g_bold.len().max(1);
          if i < bad.0.g_bold.len() {
            bad.0.g_bold[i] += <Selene as Ciphersuite>::F::from(1u64);
          }
        }
        3 => {
          let i = rng.next_u64() as usize % bad.0.h_bold.len().max(1);
          if i < bad.0.h_bold.len() {
            bad.0.h_bold[i] += <Selene as Ciphersuite>::F::from(1u64);
          }
        }
        4 => {
          // Oracle rule (c): `verify` zips h_sum with the generators' h_sum (one entry per power of two up to
          // the generator count) and drops the zero padding past it, so draw only up to the highest nonzero
          // entry, which is always in range. (2026-11-09: a uniform index over the padded length
          // lands in range 14 times in 2,048 on curve 1, and the case then "survives" at every split.)
          let top = bad.0.h_sum.iter().rposition(|x| !bool::from(x.is_zero())).unwrap_or(0);
          let i = rng.next_u64() as usize % (top + 1);
          if i < bad.0.h_sum.len() {
            bad.0.h_sum[i] += <Selene as Ciphersuite>::F::from(1u64);
          }
        }
        5 if !bad.0.additional.is_empty() => {
          let i = rng.next_u64() as usize % bad.0.additional.len();
          bad.0.additional[i].0 += <Selene as Ciphersuite>::F::from(1u64);
        }
        6 if !bad.1.additional.is_empty() => {
          let i = rng.next_u64() as usize % bad.1.additional.len();
          bad.1.additional[i].0 += <Helios as Ciphersuite>::F::from(1u64);
        }
        _ => bad.1.h += <Helios as Ciphersuite>::F::from(1u64),
      }
      let mut with_bad = states.clone();
      with_bad[victim] = bad;
      let ord: Vec<usize> = (0..with_bad.len()).collect();
      let merged = merge_all(&mut with_bad, &ord);
      for split in [1usize, 2, 3, 5, 8] {
        total_cases += 1;
        if verdict(params, &merged, split) {
          divergences += 1;
          eprintln!("CORRUPTION SURVIVED round {round} field {which} split {split}");
        }
      }
    }

    {
      let mut m = states[0].clone();
      let empty: (V1, V2) = (
        generalized_bulletproofs::Generators::batch_verifier(),
        generalized_bulletproofs::Generators::batch_verifier(),
      );
      m.0.merge(empty.0);
      m.1.merge(empty.1);
      total_cases += 1;
      let plain = params.curve_1_generators.verify(states[0].0.clone())
        && params.curve_2_generators.verify(states[0].1.clone());
      if verdict(params, &m, 1) != plain {
        divergences += 1;
        eprintln!("EMPTY-MERGE changed verdict round {round}");
      }
    }
  }

  (total_cases, malleated_rejected, divergences, max_terms)
}

#[test]
fn g009_structured_fuzz() {
  let (g, t, u, v, params) = random_params(16);
  let gtuv: Gtuv = (g, t, u, v);

  let next_round = AtomicUsize::new(0);
  let totals: Mutex<(usize, usize, usize, usize)> = Mutex::new((0, 0, 0, 0));

  std::thread::scope(|scope| {
    let mut handles = vec![];
    for _ in 0..WORKERS {
      handles.push(scope.spawn(|| loop {
        let round = next_round.fetch_add(1, Ordering::Relaxed);
        if round >= ROUNDS {
          break;
        }
        let res = one_round(round, &params, &gtuv);
        eprintln!(
          "round {round:2} done: cases {} skipped {} divergences {} max_terms {}",
          res.0, res.1, res.2, res.3
        );
        let mut tot = totals.lock().unwrap();
        tot.0 += res.0;
        tot.1 += res.1;
        tot.2 += res.2;
        tot.3 = tot.3.max(res.3);
      }));
    }
    for h in handles {
      h.join().expect("worker panicked");
    }
  });

  let (cases, skipped, divergences, max_terms) = *totals.lock().unwrap();
  eprintln!(
    "g009 fuzz TOTAL: {cases} cases, {skipped} malleations never parsed, max g-side terms ~{max_terms}, divergences {divergences}"
  );
  assert_eq!(divergences, 0, "G009 divergence");
}

// D. Split engagement on large shapes: merged term count large enough that split
// actually threads, verify equivalence, plus corruption must fail every split.
#[test]
fn g009_split_engagement() {
  let (g, t, u, v, params) = random_params(16);
  let mut states: Vec<(V1, V2)> = vec![];
  for &(paths, layers) in &[(8, 5), (16, 3)] {
    let (proof, root, inputs) = random_proof(&params, (g, t, u, v), paths, layers);
    let batch = vec![(proof, root, layers, inputs)];
    states.push(run_batch(&params, &batch).unwrap());
  }
  let mut merged = states[0].clone();
  for s in &states[1..] {
    merged.0.merge(s.0.clone());
    merged.1.merge(s.1.clone());
  }
  let terms = 2 + merged.0.g_bold.len() + merged.0.h_bold.len() + merged.0.h_sum.len()
    + merged.0.additional.len();
  eprintln!("g009 split engagement: c1 term count {terms}");
  let base = verdict(&params, &merged, 1);
  assert!(base, "valid merged batch must verify");
  for split in [2usize, 4, 8, 16] {
    assert_eq!(verdict(&params, &merged, split), base, "split {split}");
  }
  let mut bad = merged.clone();
  bad.0.g += <Selene as Ciphersuite>::F::from(1u64);
  for split in [1usize, 2, 4, 8, 16] {
    assert!(!verdict(&params, &bad, split), "corruption survived split {split}");
  }
}

// Temporary diagnostic: replay one round with detail on why old_all is false.
#[test]
fn g009_diag_round() {
  let round: usize = std::env::var("G009_DIAG_ROUND")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(36);
  let (g, t, u, v, params) = random_params(16);
  let gtuv = (g, t, u, v);
  let mut rng = DeterministicRng {
    seed: [round as u8, 0x47, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    counter: 0,
  };
  let n_batches = 1 + (rng.next_u64() as usize % 3);
  eprintln!("round {round}: {n_batches} batches");
  let mut batches: Vec<Vec<ProofStmt>> = vec![];
  for _ in 0..n_batches {
    let mut batch = vec![];
    let n_proofs = 1 + (rng.next_u64() as usize % 3);
    for _ in 0..n_proofs {
      let (paths, layers) = SHAPES[rng.next_u64() as usize % SHAPES.len()];
      let (proof, root, inputs) = random_proof(&params, gtuv, paths, layers);
      batch.push((proof, root, layers, inputs));
    }
    batches.push(batch);
  }
  let n_bad = (rng.next_u64() as usize % 3).min(n_batches);
  eprintln!("round {round}: {n_bad} malleation targets");
  for k in 0..n_bad {
    let b = rng.next_u64() as usize % n_batches;
    let p = rng.next_u64() as usize % batches[b].len();
    let paths = batches[b][p].3.len();
    let layers = batches[b][p].2;
    let mut bytes = vec![];
    batches[b][p].0.write(&mut bytes).unwrap();
    eprintln!("  malleating batch {b} proof {p} (paths {paths} layers {layers}, {} bytes)", bytes.len());
    let mut ok = false;
    for _ in 0..64 {
      let mut m = bytes.clone();
      let flips = 1 + (rng.next_u64() as usize % 3);
      for _ in 0..flips {
        let off = rng.next_u64() as usize % m.len();
        let bit = rng.next_u32() % 8;
        m[off] ^= 1 << bit;
      }
      if let Ok(t) = Fcmp::<super::MoneroCurves>::read(&mut m.as_slice(), paths, layers) {
        batches[b][p].0 = t;
        ok = true;
        eprintln!("  malleation {k}: accepted after parse");
        break;
      }
    }
    if !ok {
      eprintln!("  malleation {k}: never parsed");
    }
  }
  for (bi, batch) in batches.iter().enumerate() {
    match run_batch(&params, batch) {
      Some((v1, v2)) => {
        let ok1 = params.curve_1_generators.verify(v1);
        let ok2 = params.curve_2_generators.verify(v2);
        eprintln!("  batch {bi}: per-proof OK; batch verify c1={ok1} c2={ok2}");
      }
      None => eprintln!("  batch {bi}: PER-PROOF FAILURE (verify() errored)"),
    }
  }
}

// Targeted h_sum corruption: the random-index variant mostly corrupts beyond the
// generators' h_sum length (silently zipped out by verify), so corrupt the first
// entry — always in range — and confirm merged rejection at every split.
#[test]
fn g009_hsum_corruption() {
  let (g, t, u, v, params) = random_params(16);
  let mut states: Vec<(V1, V2)> = vec![];
  for &(paths, layers) in &[(2, 3), (4, 2), (2, 3)] {
    let (proof, root, inputs) = random_proof(&params, (g, t, u, v), paths, layers);
    let batch = vec![(proof, root, layers, inputs)];
    states.push(run_batch(&params, &batch).unwrap());
  }
  assert!(states.iter().all(|s| {
    params.curve_1_generators.verify(s.0.clone()) && params.curve_2_generators.verify(s.1.clone())
  }));
  let mut bad = states[1].clone();
  assert!(!bad.0.h_sum.is_empty(), "h_sum unexpectedly empty");
  bad.0.h_sum[0] += <Selene as Ciphersuite>::F::from(1u64);
  let mut with_bad = states.clone();
  with_bad[1] = bad;
  let ord: Vec<usize> = (0..with_bad.len()).collect();
  let merged = merge_all(&mut with_bad, &ord);
  for split in [1usize, 2, 3, 5, 8] {
    assert!(!verdict(&params, &merged, split), "h_sum[0] corruption survived split {split}");
  }
}
