use core::{
  ops::{Add, AddAssign, Sub, SubAssign, Neg, Mul, MulAssign},
  iter::{Sum, Product},
};

use zeroize::Zeroize;
use rand_core::RngCore;

use subtle::{
  Choice, CtOption, ConstantTimeEq, ConstantTimeLess, ConditionallyNegatable,
  ConditionallySelectable,
};

use crypto_bigint::{Encoding, U256};
#[cfg(test)]
use crypto_bigint::U512;

use group::ff::{Field, PrimeField, FieldBits, PrimeFieldBits, FromUniformBytes};

use crate::{u8_from_bool, constant_time, math_op, math};

// 2 ** 255 - 19
// Uses saturating_sub because checked_sub isn't available at compile time
const MODULUS: U256 = U256::from_u8(1).shl_vartime(255).saturating_sub(&U256::from_u8(19));
#[cfg(test)]
const WIDE_MODULUS: U512 = U256::ZERO.concat(&MODULUS);

/// An element of the field as four saturated 64-bit limbs, little-endian: any value below
/// `2**256`, taken modulo `p = 2**255 - 19`. Reduction uses `2**256 = 38 (mod p)`.
///
/// The type stays 32 bytes with the alignment of `u64`, which the C FFI of monerod's
/// `fcmp_pp_rust` (`SeleneScalar`, three per `HeliosPoint`) relies on. Every bit pattern is a valid
/// element, so the representation is not unique: comparison, encoding and parity go through the
/// canonical value.
#[derive(Clone, Copy, Default, Zeroize)]
#[repr(transparent)]
struct Limbs([u64; 4]);

/// `a + b + c`, as (low word, carry). The carry is at most 2 when `c` is at most 1, so callers may
/// also pass small integers as `c`.
#[inline(always)]
const fn adc(a: u64, b: u64, c: u64) -> (u64, u64) {
  let t = (a as u128) + (b as u128) + (c as u128);
  #[allow(clippy::cast_possible_truncation)]
  (t as u64, (t >> 64) as u64)
}

/// `a - b - borrow`, as (low word, borrow out of 0 or 1), for `borrow` of 0 or 1.
#[inline(always)]
const fn sbb(a: u64, b: u64, borrow: u64) -> (u64, u64) {
  let (d, b1) = a.overflowing_sub(b);
  let (d, b2) = d.overflowing_sub(borrow);
  (d, (b1 | b2) as u64)
}

/// `a + b * c + carry`, as (low word, high word). At most `2**128 - 1`, so it cannot overflow.
#[inline(always)]
const fn mac(a: u64, b: u64, c: u64, carry: u64) -> (u64, u64) {
  let t = (a as u128) + ((b as u128) * (c as u128)) + (carry as u128);
  #[allow(clippy::cast_possible_truncation)]
  (t as u64, (t >> 64) as u64)
}

impl Limbs {
  const ZERO: Self = Limbs([0; 4]);
  const ONE: Self = Limbs([1, 0, 0, 0]);

  /// Interpret a U256 as an element of the field. Every 256-bit value is a valid representation.
  const fn from_u256(x: &U256) -> Self {
    let l = x.as_limbs();
    #[cfg(target_pointer_width = "64")]
    #[allow(clippy::unnecessary_cast)]
    let w = [l[0].0 as u64, l[1].0 as u64, l[2].0 as u64, l[3].0 as u64];
    #[cfg(target_pointer_width = "32")]
    let w = [
      (l[0].0 as u64) | ((l[1].0 as u64) << 32),
      (l[2].0 as u64) | ((l[3].0 as u64) << 32),
      (l[4].0 as u64) | ((l[5].0 as u64) << 32),
      (l[6].0 as u64) | ((l[7].0 as u64) << 32),
    ];
    Limbs(w)
  }

  /// Add `38 * k` for a small `k` (at most 2**57) to four limbs, folding a carry out of the top
  /// limb back in as another 38. The second fold cannot carry: it only happens when the sum
  /// wrapped, which leaves the bottom limb below `38 * k`.
  #[inline(always)]
  const fn fold_up(l: [u64; 4], k: u64) -> Self {
    let (l0, c) = adc(l[0], 38 * k, 0);
    let (l1, c) = adc(l[1], 0, c);
    let (l2, c) = adc(l[2], 0, c);
    let (l3, c) = adc(l[3], 0, c);
    Limbs([l0 + (38 * c), l1, l2, l3])
  }

  #[inline(always)]
  fn add(&self, o: &Self) -> Self {
    let (a, b) = (self.0, o.0);
    let (s0, c) = adc(a[0], b[0], 0);
    let (s1, c) = adc(a[1], b[1], c);
    let (s2, c) = adc(a[2], b[2], c);
    let (s3, c) = adc(a[3], b[3], c);
    // A carry out is 2**256, which is 38
    Self::fold_up([s0, s1, s2, s3], c)
  }

  #[inline(always)]
  fn sub(&self, o: &Self) -> Self {
    let (a, b) = (self.0, o.0);
    let (d0, br) = sbb(a[0], b[0], 0);
    let (d1, br) = sbb(a[1], b[1], br);
    let (d2, br) = sbb(a[2], b[2], br);
    let (d3, br) = sbb(a[3], b[3], br);
    // A borrow left 2**256 too much, which is 38 too much
    let (d0, br2) = sbb(d0, 38 * br, 0);
    let (d1, br2) = sbb(d1, 0, br2);
    let (d2, br2) = sbb(d2, 0, br2);
    let (d3, br2) = sbb(d3, 0, br2);
    // A second borrow only happens from a value below 38, leaving the bottom limb near 2**64
    Limbs([d0 - (38 * br2), d1, d2, d3])
  }

  #[inline(always)]
  fn neg(&self) -> Self {
    Self::ZERO.sub(self)
  }

  /// Reduce a 512-bit product to four limbs, with `2**256 = 38`.
  #[inline(always)]
  fn reduce_wide(r: [u64; 8]) -> Self {
    let (l0, c) = mac(r[0], r[4], 38, 0);
    let (l1, c) = mac(r[1], r[5], 38, c);
    let (l2, c) = mac(r[2], r[6], 38, c);
    let (l3, c) = mac(r[3], r[7], 38, c);
    // c is at most 38
    Self::fold_up([l0, l1, l2, l3], c)
  }

  #[inline(always)]
  fn mul(&self, o: &Self) -> Self {
    let (a, b) = (self.0, o.0);
    let (r0, c) = mac(0, a[0], b[0], 0);
    let (r1, c) = mac(0, a[0], b[1], c);
    let (r2, c) = mac(0, a[0], b[2], c);
    let (r3, r4) = mac(0, a[0], b[3], c);

    let (r1, c) = mac(r1, a[1], b[0], 0);
    let (r2, c) = mac(r2, a[1], b[1], c);
    let (r3, c) = mac(r3, a[1], b[2], c);
    let (r4, r5) = mac(r4, a[1], b[3], c);

    let (r2, c) = mac(r2, a[2], b[0], 0);
    let (r3, c) = mac(r3, a[2], b[1], c);
    let (r4, c) = mac(r4, a[2], b[2], c);
    let (r5, r6) = mac(r5, a[2], b[3], c);

    let (r3, c) = mac(r3, a[3], b[0], 0);
    let (r4, c) = mac(r4, a[3], b[1], c);
    let (r5, c) = mac(r5, a[3], b[2], c);
    let (r6, r7) = mac(r6, a[3], b[3], c);

    Self::reduce_wide([r0, r1, r2, r3, r4, r5, r6, r7])
  }

  #[inline(always)]
  fn square(&self) -> Self {
    let a = self.0;
    // Products a_i * a_j for i < j
    let (r1, c) = mac(0, a[0], a[1], 0);
    let (r2, c) = mac(0, a[0], a[2], c);
    let (r3, r4) = mac(0, a[0], a[3], c);
    let (r3, c) = mac(r3, a[1], a[2], 0);
    let (r4, r5) = mac(r4, a[1], a[3], c);
    let (r5, r6) = mac(r5, a[2], a[3], 0);
    // Doubled
    let r7 = r6 >> 63;
    let r6 = (r6 << 1) | (r5 >> 63);
    let r5 = (r5 << 1) | (r4 >> 63);
    let r4 = (r4 << 1) | (r3 >> 63);
    let r3 = (r3 << 1) | (r2 >> 63);
    let r2 = (r2 << 1) | (r1 >> 63);
    let r1 = r1 << 1;
    // Plus the squares a_i * a_i
    let (r0, h) = mac(0, a[0], a[0], 0);
    let (r1, c) = adc(r1, h, 0);
    let (lo, hi) = mac(0, a[1], a[1], 0);
    let (r2, c) = adc(r2, lo, c);
    let (r3, c) = adc(r3, hi, c);
    let (lo, hi) = mac(0, a[2], a[2], 0);
    let (r4, c) = adc(r4, lo, c);
    let (r5, c) = adc(r5, hi, c);
    let (lo, hi) = mac(0, a[3], a[3], 0);
    let (r6, c) = adc(r6, lo, c);
    let (r7, _) = adc(r7, hi, c);
    Self::reduce_wide([r0, r1, r2, r3, r4, r5, r6, r7])
  }

  /// The canonical limbs, of the value in `[0, p)`.
  #[inline(always)]
  fn canonical(self) -> [u64; 4] {
    const LOW_63: u64 = (1 << 63) - 1;
    let a = self.0;
    // Fold bit 255 (worth 19): the value is then below 2**255 + 19
    let top = a[3] >> 63;
    let (a0, c) = adc(a[0], 19 * top, 0);
    let (a1, c) = adc(a[1], 0, c);
    let (a2, c) = adc(a[2], 0, c);
    let a3 = (a[3] & LOW_63) + c;
    // q = 1 if value + 19 >= 2**255, i.e. value >= p
    let (_, c) = adc(a0, 19, 0);
    let (_, c) = adc(a1, 0, c);
    let (_, c) = adc(a2, 0, c);
    let q = (a3 + c) >> 63;
    // Subtract q * p by adding 19 * q and dropping bit 255
    let (a0, c) = adc(a0, 19 * q, 0);
    let (a1, c) = adc(a1, 0, c);
    let (a2, c) = adc(a2, 0, c);
    [a0, a1, a2, (a3 + c) & LOW_63]
  }

  /// The canonical little-endian encoding, in `[0, p)`.
  fn to_bytes(self) -> [u8; 32] {
    let l = self.canonical();
    let mut res = [0; 32];
    for (i, w) in l.iter().enumerate() {
      res[(i * 8) .. ((i + 1) * 8)].copy_from_slice(&w.to_le_bytes());
    }
    res
  }
}

impl ConstantTimeEq for Limbs {
  #[inline]
  fn ct_eq(&self, other: &Self) -> Choice {
    // Equal exactly when the difference is zero modulo p. Below 2**256 that is 0, p or 2p.
    const M: u64 = u64::MAX;
    const P: [u64; 4] = [M - 18, M, M, M >> 1];
    const P2: [u64; 4] = [M - 37, M, M, M];
    let d = self.sub(other).0;
    let is = |c: [u64; 4]| ((d[0] ^ c[0]) | (d[1] ^ c[1]) | (d[2] ^ c[2]) | (d[3] ^ c[3])).ct_eq(&0);
    is([0; 4]) | is(P) | is(P2)
  }
}

impl ConditionallySelectable for Limbs {
  #[inline]
  fn conditional_select(a: &Self, b: &Self, choice: Choice) -> Self {
    Limbs([
      u64::conditional_select(&a.0[0], &b.0[0], choice),
      u64::conditional_select(&a.0[1], &b.0[1], choice),
      u64::conditional_select(&a.0[2], &b.0[2], choice),
      u64::conditional_select(&a.0[3], &b.0[3], choice),
    ])
  }
}

/// A constant-time implementation of the Ed25519 field.
#[derive(Clone, Copy, Default, Zeroize)]
#[repr(transparent)]
pub struct FieldElement(Limbs);

impl PartialEq for FieldElement {
  fn eq(&self, other: &Self) -> bool {
    bool::from(self.ct_eq(other))
  }
}
impl Eq for FieldElement {}

impl core::fmt::Debug for FieldElement {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_tuple("FieldElement").field(&self.to_repr()).finish()
  }
}

// Square root of -1.
// Formula from RFC-8032 (modp_sqrt_m1/sqrt8k5 z)
// 2 ** ((MODULUS - 1) // 4) % MODULUS
const SQRT_M1: FieldElement = FieldElement::from_u256(&U256::from_be_hex(
  "2b8324804fc1df0b2b4d00993dfbd7a72f431806ad2fe478c4ee1b274a0ea0b0",
));

// Constant useful in calculating square roots (RFC-8032 sqrt8k5's exponent used to calculate y)
// (MODULUS + 3) // 8. The field computes this power through an addition chain; kept to test it.
#[cfg(test)]
const MOD_3_8: FieldElement = FieldElement::from_u256(&U256::from_be_hex(
  "0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe",
));

// Constant useful in sqrt_ratio_i (sqrt(u / v))
// MOD_3_8 - 1. The field computes this power through an addition chain; kept to test it.
#[cfg(test)]
const MOD_5_8: FieldElement = FieldElement::from_u256(&U256::from_be_hex(
  "0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffd",
));

constant_time!(FieldElement, Limbs);

math!(
  FieldElement,
  FieldElement,
  |x: Limbs, y: Limbs| x.add(&y),
  |x: Limbs, y: Limbs| x.sub(&y),
  |x: Limbs, y: Limbs| x.mul(&y)
);

macro_rules! from_wrapper {
  ($uint: ident) => {
    impl From<$uint> for FieldElement {
      fn from(a: $uint) -> FieldElement {
        Self::from_u256(&U256::from(a))
      }
    }
  };
}

from_wrapper!(u8);
from_wrapper!(u16);
from_wrapper!(u32);
from_wrapper!(u64);
from_wrapper!(u128);

impl Neg for FieldElement {
  type Output = Self;
  #[inline]
  fn neg(self) -> Self::Output {
    Self(self.0.neg())
  }
}

impl Neg for &FieldElement {
  type Output = FieldElement;
  fn neg(self) -> Self::Output {
    (*self).neg()
  }
}

impl Field for FieldElement {
  const ZERO: Self = Self(Limbs::ZERO);
  const ONE: Self = Self(Limbs::ONE);

  fn random(mut rng: impl RngCore) -> Self {
    let mut bytes = [0; 64];
    rng.fill_bytes(&mut bytes);
    Self::wide_reduce(bytes)
  }

  #[inline]
  fn square(&self) -> Self {
    FieldElement(self.0.square())
  }
  #[inline]
  fn double(&self) -> Self {
    FieldElement(self.0.add(&self.0))
  }

  fn invert(&self) -> CtOption<Self> {
    // x ** (p - 2) = x ** (2**255 - 21) = (x ** (2**250 - 1)) ** (2**5) * x ** 11
    let (t19, t3) = self.pow22501();
    CtOption::new(t19.pow2k(5) * t3, !self.is_zero())
  }

  // RFC-8032 sqrt8k5
  fn sqrt(&self) -> CtOption<Self> {
    // x ** MOD_3_8 = x ** (2**252 - 2) = ((x ** (2**250 - 1)) ** 2 * x) ** 2
    let tv1 = (self.pow22501().0.square() * self).square();
    let tv2 = tv1 * SQRT_M1;
    let candidate = Self::conditional_select(&tv2, &tv1, tv1.square().ct_eq(self));
    CtOption::new(candidate, candidate.square().ct_eq(self))
  }

  fn sqrt_ratio(u: &FieldElement, v: &FieldElement) -> (Choice, FieldElement) {
    let i = SQRT_M1;

    let u = *u;
    let v = *v;

    let v3 = v.square() * v;
    let v7 = v3.square() * v;
    let mut r = (u * v3) * (u * v7).pow_p58();

    let check = v * r.square();
    let correct_sign = check.ct_eq(&u);
    let flipped_sign = check.ct_eq(&(-u));
    let flipped_sign_i = check.ct_eq(&((-u) * i));

    r.conditional_assign(&(r * i), flipped_sign | flipped_sign_i);

    let r_is_negative = r.is_odd();
    r.conditional_negate(r_is_negative);

    (correct_sign | flipped_sign, r)
  }
}

impl PrimeField for FieldElement {
  type Repr = [u8; 32];

  // Big endian representation of the modulus
  const MODULUS: &'static str = "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffed";

  const NUM_BITS: u32 = 255;
  const CAPACITY: u32 = 254;

  // (MODULUS + 1) // 2
  const TWO_INV: Self = FieldElement::from_u256(&U256::from_be_hex(
    "3ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7",
  ));

  // This was calculated with the method from the ff crate docs
  // SageMath GF(modulus).primitive_element()
  const MULTIPLICATIVE_GENERATOR: Self = Self::from_u256(&U256::from_u8(2));
  // This was set per the specification in the ff crate docs
  // The number of leading zero bits in the little-endian bit representation of (modulus - 1)
  const S: u32 = 2;

  // This was calculated via the formula from the ff crate docs
  // Self::MULTIPLICATIVE_GENERATOR ** ((modulus - 1) >> Self::S)
  const ROOT_OF_UNITY: Self = FieldElement::from_u256(&U256::from_be_hex(
    "2b8324804fc1df0b2b4d00993dfbd7a72f431806ad2fe478c4ee1b274a0ea0b0",
  ));
  // Self::ROOT_OF_UNITY.invert(), which is -Self::ROOT_OF_UNITY as ROOT_OF_UNITY ** 2 = -1
  const ROOT_OF_UNITY_INV: Self = FieldElement::from_u256(&U256::from_be_hex(
    "547cdb7fb03e20f4d4b2ff66c2042858d0bce7f952d01b873b11e4d8b5f15f3d",
  ));

  // This was calculated via the formula from the ff crate docs
  // Self::MULTIPLICATIVE_GENERATOR ** (2 ** Self::S)
  const DELTA: Self = FieldElement::from_u256(&U256::from_be_hex(
    "0000000000000000000000000000000000000000000000000000000000000010",
  ));

  fn from_repr(bytes: [u8; 32]) -> CtOption<Self> {
    let res = U256::from_le_bytes(bytes);
    CtOption::new(Self::from_u256(&res), res.ct_lt(&MODULUS))
  }
  fn to_repr(&self) -> [u8; 32] {
    self.0.to_bytes()
  }

  fn is_odd(&self) -> Choice {
    Choice::from(self.0.to_bytes()[0] & 1)
  }

  fn from_u128(num: u128) -> Self {
    Self::from(num)
  }
}

impl PrimeFieldBits for FieldElement {
  type ReprBits = [u8; 32];

  fn to_le_bits(&self) -> FieldBits<Self::ReprBits> {
    self.to_repr().into()
  }

  fn char_le_bits() -> FieldBits<Self::ReprBits> {
    MODULUS.to_le_bytes().into()
  }
}

impl FieldElement {
  /// `self ** (2**k)`.
  #[inline]
  fn pow2k(&self, k: u32) -> Self {
    let mut res = *self;
    for _ in 0 .. k {
      res = res.square();
    }
    res
  }

  /// `(self ** (2**250 - 1), self ** 11)`, the addition chain curve25519-dalek uses for inversion
  /// and square roots: 254 squarings and 11 multiplications, the same sequence for every input.
  fn pow22501(&self) -> (Self, Self) {
    let t0 = self.square(); // 2
    let t1 = t0.pow2k(2); // 8
    let t2 = *self * t1; // 9
    let t3 = t0 * t2; // 11
    let t4 = t3.square(); // 22
    let t5 = t2 * t4; // 31 = 2**5 - 1
    let t7 = t5.pow2k(5) * t5; // 2**10 - 1
    let t9 = t7.pow2k(10) * t7; // 2**20 - 1
    let t11 = t9.pow2k(20) * t9; // 2**40 - 1
    let t13 = t11.pow2k(10) * t7; // 2**50 - 1
    let t15 = t13.pow2k(50) * t13; // 2**100 - 1
    let t17 = t15.pow2k(100) * t15; // 2**200 - 1
    let t19 = t17.pow2k(50) * t13; // 2**250 - 1
    (t19, t3)
  }

  /// `self ** ((p - 5) / 8)` = `self ** (2**252 - 3)` = `(self ** (2**250 - 1)) ** 4 * self`.
  fn pow_p58(&self) -> Self {
    self.pow22501().0.pow2k(2) * self
  }

  /// Create a FieldElement from a `crypto_bigint::U256`.
  ///
  /// This will reduce the `U256` by the modulus, into a member of the field.
  pub const fn from_u256(u256: &U256) -> Self {
    FieldElement(Limbs::from_u256(u256))
  }

  /// Create a `FieldElement` from the reduction of a 512-bit number.
  ///
  /// The bytes are interpreted in little-endian format.
  pub fn wide_reduce(value: [u8; 64]) -> Self {
    // Eight 64-bit words, folded with 2**256 = 38 as a product would be
    let mut words = [0; 8];
    for (i, word) in words.iter_mut().enumerate() {
      let mut le = [0; 8];
      le.copy_from_slice(&value[(i * 8) .. ((i + 1) * 8)]);
      *word = u64::from_le_bytes(le);
    }
    FieldElement(Limbs::reduce_wide(words))
  }

  /// Perform an exponentiation.
  pub fn pow(&self, other: FieldElement) -> FieldElement {
    let mut table = [FieldElement::ONE; 16];
    table[1] = *self;
    for i in 2 .. 16 {
      table[i] = table[i - 1] * self;
    }

    let mut res = FieldElement::ONE;
    let mut bits = 0;
    for (i, mut bit) in other.to_le_bits().iter_mut().rev().enumerate() {
      bits <<= 1;
      let mut bit = u8_from_bool(&mut bit);
      bits |= bit;
      bit.zeroize();

      if ((i + 1) % 4) == 0 {
        if i != 3 {
          for _ in 0 .. 4 {
            res = res.square();
          }
        }

        let mut scale_by = FieldElement::ONE;
        #[allow(clippy::needless_range_loop)]
        for i in 0 .. 16 {
          #[allow(clippy::cast_possible_truncation)] // Safe since 0 .. 16
          {
            scale_by = <_>::conditional_select(&scale_by, &table[i], bits.ct_eq(&(i as u8)));
          }
        }
        res *= scale_by;
        bits = 0;
      }
    }
    res
  }

  /// The square root of u/v, as used for Ed25519 point decoding (RFC 8032 5.1.3) and within
  /// Ristretto (5.1 Extracting an Inverse Square Root).
  ///
  /// The result is only a valid square root if the Choice is true.
  /// RFC 8032 simply fails if there isn't a square root, leaving any return value undefined.
  /// Ristretto explicitly returns 0 or sqrt((SQRT_M1 * u) / v).
  pub fn sqrt_ratio_i(u: FieldElement, v: FieldElement) -> (Choice, FieldElement) {
    let i = SQRT_M1;

    let v3 = v.square() * v;
    let v7 = v3.square() * v;
    // Candidate root
    let mut r = (u * v3) * (u * v7).pow_p58();

    // 8032 3.1
    let check = v * r.square();
    let correct_sign = check.ct_eq(&u);
    // 8032 3.2 conditional
    let neg_u = -u;
    let flipped_sign = check.ct_eq(&neg_u);
    // Ristretto Step 5
    let flipped_sign_i = check.ct_eq(&(neg_u * i));

    // 3.2 set
    r.conditional_assign(&(r * i), flipped_sign | flipped_sign_i);

    // Always return the even root, per Ristretto
    // This doesn't break Ed25519 point decoding as that doesn't expect these steps to return a
    // specific root
    // Ed25519 points include a dedicated sign bit to determine which root to use, so at worst
    // this is a pointless inefficiency
    r.conditional_negate(r.is_odd());

    (correct_sign | flipped_sign, r)
  }
}

impl FromUniformBytes<64> for FieldElement {
  fn from_uniform_bytes(bytes: &[u8; 64]) -> Self {
    Self::wide_reduce(*bytes)
  }
}

impl Sum<FieldElement> for FieldElement {
  fn sum<I: Iterator<Item = FieldElement>>(iter: I) -> FieldElement {
    let mut res = FieldElement::ZERO;
    for item in iter {
      res += item;
    }
    res
  }
}

impl<'a> Sum<&'a FieldElement> for FieldElement {
  fn sum<I: Iterator<Item = &'a FieldElement>>(iter: I) -> FieldElement {
    iter.copied().sum()
  }
}

impl Product<FieldElement> for FieldElement {
  fn product<I: Iterator<Item = FieldElement>>(iter: I) -> FieldElement {
    let mut res = FieldElement::ONE;
    for item in iter {
      res *= item;
    }
    res
  }
}

impl<'a> Product<&'a FieldElement> for FieldElement {
  fn product<I: Iterator<Item = &'a FieldElement>>(iter: I) -> FieldElement {
    iter.copied().product()
  }
}

#[test]
fn test_wide_modulus() {
  let mut wide = [0; 64];
  wide[.. 32].copy_from_slice(&MODULUS.to_le_bytes());
  assert_eq!(wide, WIDE_MODULUS.to_le_bytes());
}

#[test]
fn test_sqrt_m1() {
  // Test equivalence against the known constant value
  const SQRT_M1_MAGIC: U256 =
    U256::from_be_hex("2b8324804fc1df0b2b4d00993dfbd7a72f431806ad2fe478c4ee1b274a0ea0b0");
  assert_eq!(SQRT_M1.to_repr(), SQRT_M1_MAGIC.to_le_bytes());

  // Also test equivalence against the result of the formula from RFC-8032 (modp_sqrt_m1/sqrt8k5 z)
  // 2 ** ((MODULUS - 1) // 4) % MODULUS
  assert_eq!(
    SQRT_M1,
    FieldElement::from(2u8).pow(FieldElement::from_u256(
      &MODULUS.saturating_sub(&U256::ONE).wrapping_div(&U256::from(4u8))
    ))
  );
}

#[test]
fn test_constants() {
  // The constants written out in hex, against their definitions
  let p_minus_1 = MODULUS.saturating_sub(&U256::ONE);
  assert_eq!(
    MOD_3_8.to_repr(),
    MODULUS.saturating_add(&U256::from_u8(3)).wrapping_div(&U256::from_u8(8)).to_le_bytes()
  );
  assert_eq!(MOD_5_8, MOD_3_8 - FieldElement::ONE);
  assert_eq!(FieldElement::TWO_INV * FieldElement::from(2u8), FieldElement::ONE);
  assert_eq!(
    FieldElement::ROOT_OF_UNITY,
    FieldElement::MULTIPLICATIVE_GENERATOR.pow(FieldElement::from_u256(&p_minus_1.shr_vartime(2)))
  );
  assert_eq!(FieldElement::ROOT_OF_UNITY * FieldElement::ROOT_OF_UNITY_INV, FieldElement::ONE);
  assert_eq!(FieldElement::DELTA, FieldElement::from(16u8));
  // Bit 255 set, and the modulus itself, both reduce
  assert_eq!(FieldElement::from_u256(&MODULUS), FieldElement::ZERO);
  assert_eq!(FieldElement::from_u256(&U256::MAX).to_repr(), {
    // 2**256 - 1 = 2 * p + 37
    U256::from_u8(37).to_le_bytes()
  });
}

#[test]
fn test_limb_edges() {
  const M: u64 = u64::MAX;
  // p itself, p + 1, 2p, 2**256 - 1 = 2p + 37, and the largest values each operation can see
  let p = Limbs([M - 18, M, M, M >> 1]);
  assert_eq!(p.to_bytes(), [0; 32]);
  assert_eq!(Limbs([M - 17, M, M, M >> 1]).to_bytes(), FieldElement::ONE.to_repr());
  assert_eq!(Limbs([M - 37, M, M, M]).to_bytes(), [0; 32]);
  let max = FieldElement(Limbs([M; 4]));
  assert_eq!(max.to_repr(), U256::from_u8(37).to_le_bytes());
  let m37 = FieldElement::from(37u8);
  assert_eq!(max * max, m37 * m37);
  assert_eq!(max.square(), m37 * m37);
  assert_eq!(max + max, m37 + m37);
  assert_eq!(max - FieldElement(Limbs::ZERO), m37);
  assert_eq!(FieldElement(Limbs::ZERO) - max, -m37);
  assert_eq!(FieldElement(Limbs([37, 0, 0, 0])) - max, FieldElement::ZERO);
  // Subtractions that borrow twice: a small value minus a large one
  for small in 0 .. 40u64 {
    for big in [M, M - 1, M - 37, M - 38] {
      let s = FieldElement(Limbs([small, 0, 0, 0]));
      let b = FieldElement(Limbs([big, M, M, M]));
      let expected = FieldElement::from(small) - FieldElement::from_u256(&U256::from_words([big, M, M, M]).wrapping_rem(&MODULUS));
      assert_eq!(s - b, expected);
      assert_eq!((s - b) + b, s);
      assert_eq!(b + b - b, b);
    }
  }
}

#[test]
fn test_addition_chains() {
  use rand_core::OsRng;
  const NEG_2: FieldElement = FieldElement::from_u256(&U256::from_be_hex(
    "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffeb",
  ));
  let mut xs = [FieldElement::ZERO; 68];
  xs[1] = FieldElement::ONE;
  xs[2] = -FieldElement::ONE;
  xs[3] = SQRT_M1;
  for x in &mut xs[4 ..] {
    *x = FieldElement::random(&mut OsRng);
  }
  for x in xs {
    // Against the generic windowed exponentiation
    assert_eq!(x.pow_p58(), x.pow(MOD_5_8));
    assert_eq!((x.pow22501().0.square() * x).square(), x.pow(MOD_3_8));
    assert_eq!(x.invert().unwrap_or(FieldElement::ZERO), x.pow(NEG_2));
    if !bool::from(x.is_zero()) {
      assert_eq!(x * x.invert().unwrap(), FieldElement::ONE);
    }
    let sq = x.square();
    let r = sq.sqrt().unwrap();
    assert!((r == x) || (r == -x));
  }
}

#[test]
fn test_field() {
  ff_group_tests::prime_field::test_prime_field_bits::<_, FieldElement>(&mut rand_core::OsRng);
}
