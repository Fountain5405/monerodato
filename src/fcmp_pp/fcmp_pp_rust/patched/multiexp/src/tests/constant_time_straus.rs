// monerodato: `multiexp` (constant time) against the dispatch it had in 0.5.2, `multiexp_vartime`,
// constant-time Straus with a 4-bit window and the naive sum, at the lengths on either side of
// every threshold in both of 0.5.2's tables (with and without debug assertions), and above them.

use alloc::vec::Vec;

use rand_core::OsRng;

use zeroize::Zeroize;
use subtle::ConditionallySelectable;

use ff::{Field as _, PrimeFieldBits};
use group::Group;

use k256::ProjectivePoint;
use dalek_ff_group::EdwardsPoint;

use crate::{straus, pippenger, multiexp, multiexp_vartime};

// 0.5.2's `algorithm` for two or more pairs: (fewer pairs than, Straus or not, window)
const TABLE_RELEASE: &[(usize, bool, u8)] = &[
  (10, true, 3),
  (20, true, 4),
  (50, true, 5),
  (100, false, 4),
  (125, false, 5),
  (275, false, 6),
  (400, false, 7),
  (usize::MAX, false, 8),
];
const TABLE_DEBUG: &[(usize, bool, u8)] = &[
  (10, true, 3),
  (80, true, 4),
  (100, true, 5),
  (125, false, 4),
  (275, false, 5),
  (475, false, 6),
  (750, false, 7),
  (usize::MAX, false, 8),
];

// 0.5.2's constant-time `multiexp` for two or more pairs, with the given table
fn multiexp_0_5_2<
  G: Zeroize + ConditionallySelectable + Group<Scalar: Zeroize + PrimeFieldBits>,
>(
  table: &[(usize, bool, u8)],
  pairs: &[(G::Scalar, G)],
) -> G {
  let (_, use_straus, window) = *table.iter().find(|(below, _, _)| pairs.len() < *below).unwrap();
  if use_straus {
    straus(pairs, window)
  } else {
    pippenger(pairs, window)
  }
}

fn test_constant_time_straus<
  G: Zeroize + ConditionallySelectable + Group<Scalar: Zeroize + PrimeFieldBits>,
>() {
  let mut lengths = Vec::new();
  for table in [TABLE_RELEASE, TABLE_DEBUG] {
    for (below, _, _) in &table[.. table.len() - 1] {
      lengths.push(below - 1);
      lengths.push(*below);
    }
  }
  lengths.extend([2, 3, 4, 5, 1000, 1024]);
  lengths.sort_unstable();
  lengths.dedup();
  let max = *lengths.last().unwrap();

  // Edge scalars and points first, so that every length includes them, then random pairs
  let mut pairs = Vec::with_capacity(max);
  pairs.push((-G::Scalar::ONE, G::generator() * G::Scalar::random(&mut OsRng)));
  pairs.push((G::Scalar::ZERO, G::generator()));
  pairs.push((G::Scalar::ONE, G::identity()));
  pairs.push((-G::Scalar::ONE, G::generator()));
  pairs.push((G::Scalar::ONE, G::generator()));
  while pairs.len() < max {
    pairs.push((G::Scalar::random(&mut OsRng), G::generator() * G::Scalar::random(&mut OsRng)));
  }

  for len in lengths {
    let pairs = &pairs[.. len];
    let sum = pairs.iter().map(|(scalar, point)| *point * scalar).sum::<G>();
    let res = multiexp(pairs);
    assert_eq!(res, sum, "naive sum, {len} pairs");
    assert_eq!(res, multiexp_vartime(pairs), "multiexp_vartime, {len} pairs");
    assert_eq!(res, straus(pairs, 4), "straus(4), {len} pairs");
    assert_eq!(res, multiexp_0_5_2(TABLE_RELEASE, pairs), "0.5.2 release, {len} pairs");
    assert_eq!(res, multiexp_0_5_2(TABLE_DEBUG, pairs), "0.5.2 debug, {len} pairs");
  }
}

#[test]
fn test_constant_time_straus_secp256k1() {
  test_constant_time_straus::<ProjectivePoint>();
}

#[test]
fn test_constant_time_straus_ed25519() {
  test_constant_time_straus::<EdwardsPoint>();
}
