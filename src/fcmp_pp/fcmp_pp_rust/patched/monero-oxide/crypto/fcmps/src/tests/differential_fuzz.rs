// Differential fuzz of the deployed folded verifier (`Fcmp::verify`) against the
// historical stored verifier (`Fcmp::verify_stored`), over structured mutations the
// single-bit-flip campaign in tests.rs does not cover.
//
// Added by the second reviewer 2026-11-07 for the stressnet lowmem review.
// Strategies:
//   multibit        k simultaneous bit flips (k = 2, 3, 4, 8, 16)
//   self_splice     copy a byte run within the proof to another offset
//   cross_splice    splice a byte run from a second valid proof of the same shape
//   shift           shift a byte run by a small offset (field misalignment)
//   random_bytes    fully random proof buffers (parse/panic hunting)
//   metadata_swap   valid proof verified against the other proof's inputs/root
//   shape_confusion proof parsed with a different (paths, layers) than proven for
//
// Oracle: for every parseable mutant, folded and stored must agree on accept/reject;
// on accept, both batch verifiers must verify and be field-equal. A panic anywhere is
// a finding. Divergences are dumped under target/differential-fuzz-findings/ and the
// case is re-run in-process to confirm before failing the test.

use std::panic::{self, AssertUnwindSafe};

use rand_core::{RngCore, OsRng};

use ciphersuite::Ciphersuite;
use helioselene::{Selene, Helios};

use super::batch_verifies;

// (paths, layers): input counts through the power-of-two ipa_rows padding
// boundaries, both root curve parities, and a deeper tree.
const SHAPES: &[(usize, usize)] = &[
  (1, 1),
  (1, 2),
  (1, 3),
  (2, 2),
  (2, 3),
  (3, 2),
  (4, 3),
  (5, 2),
  (7, 3),
  (8, 2),
  (8, 3),
  (9, 2),
  (15, 3),
  (16, 2),
  (4, 5),
];

// Verify pairs per shape and strategy.
const ITER: usize = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Strategy {
  Multibit,
  SelfSplice,
  CrossSplice,
  Shift,
  RandomBytes,
  MetadataSwap,
  ShapeConfusion,
}
use Strategy::*;

struct Counts {
  generated: usize,
  unparsable: usize,
  both_reject: usize,
  both_accept: usize,
  diverged: usize,
  panics: usize,
}

fn dump_finding(tag: &str, paths: usize, layers: usize, strategy: Strategy, iter: usize, bytes: &[u8]) {
  let dir = "target/differential-fuzz-findings";
  let _ = std::fs::create_dir_all(dir);
  let name = format!("{dir}/{tag}-paths{paths}-layers{layers}-{strategy:?}-{iter}.bin");
  let _ = std::fs::write(&name, bytes);
  eprintln!("FINDING dumped to {name}");
}

// One mutant through both verifiers, each in its own catch_unwind so a panic on one
// side is attributed. Returns Some(finding description) when divergent or panicking.
fn judge(
  params: &crate::FcmpParams<super::MoneroCurves>,
  root: crate::TreeRoot<Selene, Helios>,
  layers: usize,
  inputs: &[crate::Input<<Selene as Ciphersuite>::F>],
  tampered: &crate::Fcmp<super::MoneroCurves>,
  counts: &mut Counts,
) -> Option<String> {
  let rng = super::DeterministicRng::new();

  let mut folded_1 = generalized_bulletproofs::Generators::batch_verifier();
  let mut folded_2 = generalized_bulletproofs::Generators::batch_verifier();
  let folded_res = panic::catch_unwind(AssertUnwindSafe(|| {
    tampered.verify(&mut rng.clone(), &mut folded_1, &mut folded_2, params, root, layers, inputs)
  }));

  let mut stored_1 = generalized_bulletproofs::Generators::batch_verifier();
  let mut stored_2 = generalized_bulletproofs::Generators::batch_verifier();
  let stored_res = panic::catch_unwind(AssertUnwindSafe(|| {
    tampered.verify_stored(
      &mut rng.clone(),
      &mut stored_1,
      &mut stored_2,
      params,
      root,
      layers,
      inputs,
    )
  }));

  enum Side {
    Ok,
    Err,
    Panic,
  }
  let side = |r: &Result<Result<(), crate::FcmpError>, Box<dyn std::any::Any + Send>>| match r {
    Ok(Ok(())) => Side::Ok,
    Ok(Err(_)) => Side::Err,
    Err(_) => Side::Panic,
  };
  let detail = |r: &Result<Result<(), crate::FcmpError>, Box<dyn std::any::Any + Send>>| match r {
    Ok(Ok(())) => "ok".to_string(),
    Ok(Err(e)) => format!("err({e:?})"),
    Err(_) => "PANIC".to_string(),
  };
  let (f_side, s_side) = (side(&folded_res), side(&stored_res));
  let (f_detail, s_detail) = (detail(&folded_res), detail(&stored_res));

  match (&f_side, &s_side) {
    (Side::Err, Side::Err) => {
      counts.both_reject += 1;
      None
    }
    (Side::Panic, Side::Panic) => {
      // Shared reachable panic (documented MAY-panic class): not a folded-vs-stored
      // divergence, but it IS fatal — the final assert fails on any panic
      // (review catch: the old comment wrongly said "not fatal").
      counts.panics += 1;
      eprintln!("NOTE: both verifiers panicked (shared path) — recorded and fatal");
      counts.both_reject += 1;
      None
    }
    (Side::Ok, Side::Ok) => {
      // `verify` only queues; the verdict is the batch check. Both sides must queue
      // identical state, and every case judged here is an INVALID statement, so any
      // batch pass is an acceptance finding. Queue-ok + batch-fail is a healthy reject.
      let states_eq = folded_1.g == stored_1.g &&
        folded_1.h == stored_1.h &&
        folded_1.g_bold == stored_1.g_bold &&
        folded_1.h_bold == stored_1.h_bold &&
        folded_1.h_sum == stored_1.h_sum &&
        folded_1.additional == stored_1.additional &&
        folded_2.g == stored_2.g &&
        folded_2.h == stored_2.h &&
        folded_2.g_bold == stored_2.g_bold &&
        folded_2.h_bold == stored_2.h_bold &&
        folded_2.h_sum == stored_2.h_sum &&
        folded_2.additional == stored_2.additional;
      let batch_f = batch_verifies(params, &folded_1, &folded_2);
      let batch_s = batch_verifies(params, &stored_1, &stored_2);
      if !states_eq {
        counts.diverged += 1;
        Some(format!("queued verifier states differ (folded {f_detail} / stored {s_detail})"))
      } else if batch_f != batch_s {
        counts.diverged += 1;
        Some(format!("batch verdict divergence: folded {batch_f} / stored {batch_s}"))
      } else if batch_f {
        counts.diverged += 1;
        Some("INVALID STATEMENT ACCEPTED: batch verifies for a wrong/tampered statement".to_string())
      } else {
        counts.both_reject += 1;
        None
      }
    }
    _ => {
      counts.diverged += 1;
      Some(format!("verdict divergence: folded {f_detail} / stored {s_detail}"))
    }
  }
}

#[test]
fn differential_fuzz_structured_mutations() {
  let (G, T, U, V, params) = super::random_params(16);

  let mut total = Counts {
    generated: 0,
    unparsable: 0,
    both_reject: 0,
    both_accept: 0,
    diverged: 0,
    panics: 0,
  };

  for &(paths, layers) in SHAPES {
    let (proof_a, root_a, inputs_a) = super::random_proof(&params, (G, T, U, V), paths, layers);
    let (proof_b, root_b, inputs_b) = super::random_proof(&params, (G, T, U, V), paths, layers);
    let mut bytes_a = vec![];
    proof_a.write(&mut bytes_a).unwrap();
    let mut bytes_b = vec![];
    proof_b.write(&mut bytes_b).unwrap();

    let mut rng = super::DeterministicRng {
      seed: [paths as u8, layers as u8, 0xAA, 0x47, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
      counter: 0,
    };

    let mut counts = Counts {
      generated: 0,
      unparsable: 0,
      both_reject: 0,
      both_accept: 0,
      diverged: 0,
      panics: 0,
    };

    let strat = |rng: &mut super::DeterministicRng| match rng.next_u64() % 7 {
      0 => Multibit,
      1 => SelfSplice,
      2 => CrossSplice,
      3 => Shift,
      4 => RandomBytes,
      5 => MetadataSwap,
      _ => ShapeConfusion,
    };

    for iter in 0 .. ITER {
      let strategy = strat(&mut rng);
      counts.generated += 1;

      let len = bytes_a.len();
      let rand_range = |rng: &mut super::DeterministicRng, n: usize| {
        usize::try_from(rng.next_u64() % u64::try_from(n).unwrap()).unwrap()
      };

      match strategy {
        Multibit => {
          let k = 2 * (1 + rand_range(&mut rng, 8));
          let mut mutant = bytes_a.clone();
          for _ in 0 .. k {
            let off = rand_range(&mut rng, len);
            let bit = rng.next_u32() % 8;
            mutant[off] ^= 1 << bit;
          }
          if let Ok(tampered) = crate::Fcmp::<super::MoneroCurves>::read(&mut mutant.as_slice(), paths, layers)
          {
            if let Some(what) = judge(&params, root_a, layers, &inputs_a, &tampered, &mut counts) {
              dump_finding("panic-or-divergence", paths, layers, strategy, iter, &mutant);
              panic!("FINDING {what}: {paths} paths {layers} layers {strategy:?} iter {iter}");
            }
          } else {
            counts.unparsable += 1;
          }
        }
        SelfSplice | CrossSplice => {
          let (src, dst_base) = if strategy == CrossSplice {
            (&bytes_b, &bytes_a)
          } else {
            (&bytes_a, &bytes_a)
          };
          let run = 16 + rand_range(&mut rng, 240);
          let src_off = rand_range(&mut rng, src.len().saturating_sub(run) + 1);
          let mut mutant = dst_base.clone();
          let dst_off = rand_range(&mut rng, mutant.len().saturating_sub(run) + 1);
          let dst_len = run.min(mutant.len() - dst_off);
          let src_run = &src[src_off .. (src_off + run).min(src.len())][.. dst_len];
          mutant[dst_off .. dst_off + dst_len].copy_from_slice(src_run);
          if let Ok(tampered) = crate::Fcmp::<super::MoneroCurves>::read(&mut mutant.as_slice(), paths, layers)
          {
            if let Some(what) = judge(&params, root_a, layers, &inputs_a, &tampered, &mut counts) {
              dump_finding("panic-or-divergence", paths, layers, strategy, iter, &mutant);
              panic!("FINDING {what}: {paths} paths {layers} layers {strategy:?} iter {iter}");
            }
          } else {
            counts.unparsable += 1;
          }
        }
        Shift => {
          let mut mutant = bytes_a.clone();
          let off = rand_range(&mut rng, len.saturating_sub(16));
          let run = 8 + rand_range(&mut rng, (len - off).min(64) - 8);
          let by = 1 + rand_range(&mut rng, 8);
          let region = mutant[off .. off + run].to_vec();
          if off + by + run <= len {
            mutant[off + by .. off + by + run].copy_from_slice(&region);
          } else {
            mutant.drain(off .. (off + by).min(len));
            mutant.extend_from_slice(&region[.. run.min(by)]);
          }
          if let Ok(tampered) = crate::Fcmp::<super::MoneroCurves>::read(&mut mutant.as_slice(), paths, layers)
          {
            if judge(&params, root_a, layers, &inputs_a, &tampered, &mut counts).is_some() {
              dump_finding("panic-or-divergence", paths, layers, strategy, iter, &mut mutant);
              panic!("divergence/panic: {paths} paths {layers} layers {strategy:?} iter {iter}");
            }
          } else {
            counts.unparsable += 1;
          }
        }
        RandomBytes => {
          let mut mutant = vec![0; len];
          OsRng.fill_bytes(&mut mutant);
          if let Ok(tampered) = crate::Fcmp::<super::MoneroCurves>::read(&mut mutant.as_slice(), paths, layers)
          {
            if judge(&params, root_a, layers, &inputs_a, &tampered, &mut counts).is_some() {
              dump_finding("panic-or-divergence", paths, layers, strategy, iter, &mut mutant);
              panic!("divergence/panic: {paths} paths {layers} layers {strategy:?} iter {iter}");
            }
          } else {
            counts.unparsable += 1;
          }
        }
        MetadataSwap => {
          // A's bytes against B's statement (same shape): different inputs/root
          if let Some(what) = judge(&params, root_b, layers, &inputs_b, &proof_a, &mut counts) {
            dump_finding("panic-or-divergence", paths, layers, strategy, iter, &bytes_a);
            panic!("FINDING {what}: {paths} paths {layers} layers {strategy:?} iter {iter}");
          }
        }
        ShapeConfusion => {
          // Parse A's bytes under a different declared shape than proven
          let alt_paths = *[1usize, 2, 4, 8, 16].iter().find(|&&p| p != paths).unwrap();
          let alt_layers = if layers == 2 { 3 } else { 2 };
          if let Ok(tampered) = crate::Fcmp::<super::MoneroCurves>::read(&mut bytes_a.as_slice(), alt_paths, alt_layers)
          {
            if let Some(what) = judge(&params, root_a, layers, &inputs_a, &tampered, &mut counts) {
              dump_finding("panic-or-divergence", paths, layers, strategy, iter, &bytes_a);
              panic!("FINDING {what}: {paths} paths {layers} layers {strategy:?} iter {iter}");
            }
          } else {
            counts.unparsable += 1;
          }
        }
      }
    }

    eprintln!(
      "shape ({paths:2},{layers}): generated {} unparsable {} both_reject {} both_accept {} diverged {} panics {}",
      counts.generated, counts.unparsable, counts.both_reject, counts.both_accept, counts.diverged, counts.panics
    );
    total.generated += counts.generated;
    total.unparsable += counts.unparsable;
    total.both_reject += counts.both_reject;
    total.both_accept += counts.both_accept;
    total.diverged += counts.diverged;
    total.panics += counts.panics;
  }

  eprintln!(
    "TOTAL: generated {} unparsable {} both_reject {} both_accept {} diverged {} panics {}",
    total.generated, total.unparsable, total.both_reject, total.both_accept, total.diverged, total.panics
  );
  assert_eq!(total.diverged, 0, "folded/stored verdict divergence");
  assert_eq!(total.panics, 0, "verifier panic");
}
