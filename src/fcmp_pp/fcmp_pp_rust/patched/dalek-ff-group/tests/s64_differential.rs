// Gate test: differential of this (s64) Field25519 against the pristine
// dalek-ff-group 0.5.0 implementation, shipped as the sibling crate
// `dalek-ff-group-orig` (path dev-dependency; a registry dev-dependency would be
// redirected by the workspace [patch.crates-io] to this crate and compare
// nothing).
//
// Bar: 0 mismatches across acceptance, boundary pair ops, equality across
// representations, wide_reduce limb extremes, hard functions, and adversarially
// anchored chains. Run under overflow checks:
//   RUSTFLAGS="-C overflow-checks" cargo test --release --test s64_differential -- --nocapture
// Written by the second reviewer for the monerodato gate, 2026-11-08.
use std::sync::atomic::{AtomicUsize, Ordering};

use ff::{Field, PrimeField};
use rand_core::{OsRng, RngCore};
use subtle::ConstantTimeEq;

use dalek_ff_group_orig::FieldElement as O;
use dalek_ff_group::FieldElement as N;

static FAILURES: AtomicUsize = AtomicUsize::new(0);

fn fail(msg: String) {
  FAILURES.fetch_add(1, Ordering::Relaxed);
  eprintln!("MISMATCH: {msg}");
}

fn same(n: &N, o: &O) -> bool {
  n.to_repr() == o.to_repr()
}

fn limbs_bytes(l: [u64; 4]) -> [u8; 32] {
  let mut b = [0u8; 32];
  for i in 0..4 {
    b[i * 8..(i + 1) * 8].copy_from_slice(&l[i].to_le_bytes());
  }
  b
}

fn hex(b: &[u8; 32]) -> String {
  b.iter().rev().map(|x| format!("{x:02x}")).collect()
}

fn canonical_pool() -> Vec<([u64; 4], &'static str)> {
  let m = u64::MAX;
  let p: [u64; 4] = [m - 18, m, m, m >> 1];
  let mut pm1 = p;
  pm1[0] -= 1;
  let mut pp1 = p;
  pp1[0] += 1;
  let mut v = vec![
    ([0, 0, 0, 0], "0"),
    ([1, 0, 0, 0], "1"),
    ([2, 0, 0, 0], "2"),
    ([18, 0, 0, 0], "18"),
    ([19, 0, 0, 0], "19"),
    ([20, 0, 0, 0], "20"),
    ([37, 0, 0, 0], "37"),
    ([38, 0, 0, 0], "38"),
    ([39, 0, 0, 0], "39"),
    ([m, 0, 0, 0], "2^64-1"),
    ([0, m, 0, 0], "limb1max"),
    ([0, 0, m, 0], "limb2max"),
    (pm1, "p-1"),
    ([m - 2, m, m, m >> 1], "p-3"),
    ([m - 19, m, m, m >> 1], "p-38"),
    ([m - 38, m, m, m >> 1], "p-57"),
    ([m - 37, m, m, m >> 1], "p-56"),
    ([m, m, m, (m >> 1) - 1], "near-2^255-max"),
    (pp1, "p+1"),
    ([m, m, 0, 0], "l01max"),
    ([0, m, m, 0], "l12max"),
    ([0, 0, m, m], "l23max"),
    ([m, 0, 0, m >> 1], "l0+l3"),
    ([m, m, m, 0], "l012max"),
    ([1 << 63, 0, 0, 0], "l0-2^63"),
    ([0, 0, 0, 1], "2^192"),
  ];
  for (i, extra) in [
    ([m, m, m, m], "allmax"),
    ([m - 37, m, m, m], "2p"),
  ]
  .iter()
  .enumerate()
  {
    v.push((extra.0, extra.1));
  }
  v
}

fn canonical_only() -> Vec<([u64; 4], &'static str)> {
  let m = u64::MAX;
  let p: [u64; 4] = [m - 18, m, m, m >> 1];
  canonical_pool()
    .into_iter()
    .filter(|(l, _)| {
      let mut less = false;
      for i in (0..4).rev() {
        if l[i] < p[i] {
          less = true;
          break;
        }
        if l[i] > p[i] {
          break;
        }
      }
      less
    })
    .collect()
}

fn add4(a: [u64; 4], b: [u64; 4]) -> Option<[u64; 4]> {
  let mut r = [0u64; 4];
  let mut carry = 0u64;
  for i in 0..4 {
    let t = (a[i] as u128) + (b[i] as u128) + (carry as u128);
    r[i] = t as u64;
    carry = (t >> 64) as u64;
  }
  if carry != 0 {
    None
  } else {
    Some(r)
  }
}

#[test]
fn s64_gate_differential() {
  let pool = canonical_only();
  let mut cases = 0usize;

  // from_repr acceptance and round-trip on every pool entry
  for (l, name) in &canonical_pool() {
    let b = limbs_bytes(*l);
    let (cn, co) = (N::from_repr(b), O::from_repr(b));
    if bool::from(cn.is_some()) != bool::from(co.is_some()) {
      fail(format!("acceptance {name}"));
    }
    if let (Some(x), Some(y)) = (Option::<N>::from(cn), Option::<O>::from(co)) {
      if !same(&x, &y) {
        fail(format!("round-trip {name}"));
      }
    }
    cases += 1;
  }

  // pair ops
  for (la, na) in &pool {
    for (lb, nb) in &pool {
      let (ba, bb) = (limbs_bytes(*la), limbs_bytes(*lb));
      let (a, b) = (N::from_repr(ba).unwrap(), O::from_repr(ba).unwrap());
      let (c, d) = (N::from_repr(bb).unwrap(), O::from_repr(bb).unwrap());
      for (op, rn, ro) in [
        ("add", a + c, b + d),
        ("sub", a - c, b - d),
        ("sub-rev", c - a, d - b),
        ("mul", a * c, b * d),
        ("sq", a.square(), b.square()),
        ("neg", -a, -b),
        ("double", a.double(), b.double()),
      ] {
        if !same(&rn, &ro) {
          fail(format!("op {op} {na} x {nb}: {} vs {}", hex(&rn.to_repr()), hex(&ro.to_repr())));
        }
        cases += 1;
      }
      let ct_n = bool::from(a.ct_eq(&c));
      if ct_n != (a == c) || ct_n != bool::from(b.ct_eq(&d)) {
        fail(format!("eq {na} x {nb}"));
      }
    }
  }

  // equality across representations: a vs a+p vs a+2p; p and 2p are zero
  let m = u64::MAX;
  let p: [u64; 4] = [m - 18, m, m, m >> 1];
  let p2: [u64; 4] = [m - 37, m, m, m];
  for (l, name) in &pool {
    let mut ksum = *l;
    for (k, kn) in [(1u64, "+p"), (2u64, "+2p")] {
      let addend = if k == 1 { p } else { p2 };
      ksum = match add4(ksum, addend) {
        Some(x) => x,
        None => break,
      };
      let mut bytes = [0u8; 64];
      bytes[..32].copy_from_slice(&limbs_bytes(ksum));
      let (n_val, o_val) = (N::wide_reduce(bytes), O::wide_reduce(bytes));
      let n_a = N::from_repr(limbs_bytes(*l)).unwrap();
      let o_a = O::from_repr(limbs_bytes(*l)).unwrap();
      if !bool::from(n_val.ct_eq(&n_a)) || !bool::from(o_val.ct_eq(&o_a)) {
        fail(format!("eq-across-rep {name}{kn}"));
      }
      cases += 1;
    }
  }
  for (pat, pn) in [(p, "p"), (p2, "2p")] {
    let mut bytes = [0u8; 64];
    bytes[..32].copy_from_slice(&limbs_bytes(pat));
    let (n_val, o_val) = (N::wide_reduce(bytes), O::wide_reduce(bytes));
    if !bool::from(n_val.is_zero()) || !bool::from(o_val.is_zero()) {
      fail(format!("{pn} != 0"));
    }
  }

  // wide_reduce limb extremes + chained ops on the results
  let limb_choices = [
    0u64,
    1,
    19,
    38,
    37,
    (1 << 63) - 1,
    1 << 63,
    (1 << 63) + 1,
    m - 39,
    m - 38,
    m - 37,
    m - 19,
    m - 1,
    m,
  ];
  let mut rng = OsRng;
  for i in 0..1500 {
    let mut w = [0u64; 8];
    if i < limb_choices.len() {
      w = [limb_choices[i]; 8];
    } else {
      for limb in w.iter_mut() {
        *limb = limb_choices[(rng.next_u64() % limb_choices.len() as u64) as usize];
      }
    }
    let mut bytes = [0u8; 64];
    for j in 0..8 {
      bytes[j * 8..(j + 1) * 8].copy_from_slice(&w[j].to_le_bytes());
    }
    let (mut rn, mut ro) = (N::wide_reduce(bytes), O::wide_reduce(bytes));
    if !same(&rn, &ro) {
      fail(format!("wide_reduce case {i}"));
    }
    let lo = limbs_bytes([w[0], w[1], w[2], w[3]]);
    let (cn, co) = (N::from_repr(lo), O::from_repr(lo));
    if bool::from(cn.is_some()) {
      rn = rn * cn.unwrap() + rn;
      ro = ro * co.unwrap() + ro;
      if !same(&rn, &ro) {
        fail(format!("wide-chain case {i}"));
      }
    }
    cases += 1;
  }

  // hard functions on squares, non-squares, zero
  let squares: Vec<N> = pool
    .iter()
    .filter_map(|(l, _)| Option::<N>::from(N::from_repr(limbs_bytes(*l))))
    .map(|n| n.square())
    .collect();
  for (l, _) in &pool {
    let b = limbs_bytes(*l);
    let (cn, co) = (N::from_repr(b), O::from_repr(b));
    if !bool::from(cn.is_some()) {
      continue;
    }
    let (n, o) = (cn.unwrap(), co.unwrap());
    let (in_n, in_o) = (n.invert(), o.invert());
    if bool::from(in_n.is_some()) != bool::from(in_o.is_some())
      || (bool::from(in_n.is_some()) && !same(&in_n.unwrap(), &in_o.unwrap()))
    {
      fail(format!("invert {}", hex(&n.to_repr())));
    }
    let pn = n.pow(n);
    let po = o.pow(o);
    if !same(&pn, &po) {
      fail("pow self".into());
    }
    let pn2 = n.pow(N::from(12345u64));
    let po2 = o.pow(O::from(12345u64));
    if !same(&pn2, &po2) {
      fail("pow const".into());
    }
    for sq in &squares {
      let osq = O::from_repr(sq.to_repr()).unwrap();
      let (sn, so) = (sq.sqrt(), osq.sqrt());
      if bool::from(sn.is_some()) != bool::from(so.is_some())
        || (bool::from(sn.is_some()) && !same(&sn.unwrap(), &so.unwrap()))
      {
        fail("sqrt square".into());
      }
      let (cn2, xn) = N::sqrt_ratio_i(*sq, n);
      let (co2, xo) = O::sqrt_ratio_i(osq, o);
      if bool::from(cn2) != bool::from(co2) || !same(&xn, &xo) {
        fail("sqrt_ratio_i".into());
      }
      cases += 1;
    }
  }
  // sqrt on random values: orig's verdict is the oracle
  for _ in 0..500 {
    let n = N::random(&mut rng);
    let o = O::from_repr(n.to_repr()).unwrap();
    let (sn, so) = (n.sqrt(), o.sqrt());
    if bool::from(sn.is_some()) != bool::from(so.is_some())
      || (bool::from(sn.is_some()) && !same(&sn.unwrap(), &so.unwrap()))
    {
      fail("sqrt random".into());
    }
  }

  // adversarially anchored random chains
  for _round in 0..50 {
    let (mut rn, mut ro) = (N::ZERO, O::ZERO);
    for step in 0..200 {
      // Accumulate: rn/ro carry from the previous step; re-anchor only every
      // 20th step (review catch: the prior version re-derived both values
      // each step, testing 10,000 single ops rather than 50 chains of 200).
      let mut w = [0u8; 64];
      if step % 20 == 0 {
        rng.fill_bytes(&mut w);
      }
      let wv = N::random(&mut rng);
      let wn = wv;
      let wo = O::from_repr(wv.to_repr()).unwrap();
      if step % 20 == 0 {
        rn = N::wide_reduce(w);
        ro = O::wide_reduce(w);
      }
      let (new_r, new_o) = match rng.next_u64() % 5 {
        0 => (rn + wn, ro + wo),
        1 => (rn - wn, ro - wo),
        2 => (wn - rn, wo - ro),
        3 => (rn * wn, ro * wo),
        _ => (rn.double() - wn, ro.double() - wo),
      };
      if !same(&new_r, &new_o) {
        fail(format!("chain round {_round} step {step}"));
        break;
      }
      rn = new_r;
      ro = new_o;
    }
  }

  // layout invariants
  assert_eq!(core::mem::size_of::<N>(), 32);
  assert_eq!(core::mem::align_of::<N>(), core::mem::align_of::<u64>());

  let f = FAILURES.load(Ordering::Relaxed);
  eprintln!("s64 gate differential: {cases}+ cases, failures {f}");
  assert_eq!(f, 0, "s64 differential failures");
}
