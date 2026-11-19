// C3 — Iron Fish Merkle-positioning class (adversary, 2026-11-13).
//
// Takes valid FCMP++ verify-input vectors (tests/data/fcmp_pp_verify_inputs_*in.bin),
// applies structured Merkle-position mutations that preserve the wire length where
// possible (so they cannot be caught by the fcmp_pp_proof_len sanity check), and
// proves each mutant is rejected. Unmodified controls must verify TRUE.
//
// Mutations:
//  m4 path_truncate: drop the last path entry's bytes AND decrement n_layers to match,
//    defeating the length check; the truncated path must fail to rebuild the root.
//  m5 root_transplant: verify under another vector file's tree root (cross-file).
//  m6 signable_transplant: verify under a foreign signable_tx_hash (transcript binding).
//  m1' leaf_tuple_swap: swap the per-input leaf tuple (O~, I~, R) + SAL proof blocks
//    between input positions 0 and 1, length-preserving. This substitutes for
//    m1/m2/m3 (leaf-vs-sibling swap, adjacent-sibling swap, path shift), which are
//    BLOCKED: the membership proof's path is not discretely addressable in the wire
//    format. Evidence:
//    - C++ side: src/fcmp_pp/fcmp_pp_types.h:82 `using FcmpPpProof = std::vector<uint8_t>`
//    - Rust side: crypto/fcmps/src/lib.rs:238 `struct Fcmp<C> { proof: Vec<u8>,
//      root_blind_pok: [u8; 64] }` — the membership proof is a single opaque
//      Bulletproofs+ style blob; path leaves/siblings are folded into BP circuit
//      commitments (Fcmp::read/write, same file, are pure byte-slice copies), so
//      there are no per-sibling byte offsets to mutate from C++.
//    Note m6 uses a fixed non-zero foreign signable_tx_hash rather than one taken
//    from another vector file.

#include "gtest/gtest.h"

#include "crypto/crypto.h"
#include "fcmp_pp/fcmp_pp_types.h"
#include "fcmp_pp/prove.h"
#include "misc_log_ex.h"
#include "unit_tests_utils.h"

#include <algorithm>
#include <vector>

namespace
{
struct VerifyInputs
{
  crypto::hash signable_tx_hash;
  fcmp_pp::FcmpPpProof fcmp_pp_proof;
  uint8_t n_layers;
  fcmp_pp::TreeRootShared tree_root;
  std::vector<crypto::ec_point> pseudo_outs;
  std::vector<crypto::key_image> key_images;
};

bool load(const std::size_t n_inputs, VerifyInputs &in)
{
  return unit_test::read_fcmp_pp_verify_input_from_file(
      n_inputs,
      in.signable_tx_hash,
      in.fcmp_pp_proof,
      in.n_layers,
      in.tree_root,
      in.pseudo_outs,
      in.key_images);
}

bool do_verify(const VerifyInputs &in)
{
  return fcmp_pp::verify(
      in.signable_tx_hash,
      in.fcmp_pp_proof,
      in.n_layers,
      in.tree_root,
      in.pseudo_outs,
      in.key_images);
}

// One per-input block in the FCMP++ proof wire: (O~, I~, R) tuple without C~, then
// the SAL proof. See fcmp_pp_proof_from_parts_v1 / fcmp_pp_parts_from_proof_v1.
constexpr std::size_t INPUT_BLOCK_SIZE_V1 = (3 * 32) + (12 * 32);

const std::vector<std::size_t> ALL_SIZES{1, 2, 4, 8, 128};
} // anonymous namespace

// Unmodified vectors must verify TRUE.
TEST(C3Merkle, controls_accept)
{
  for (const std::size_t n_inputs : ALL_SIZES)
  {
    VerifyInputs in;
    ASSERT_TRUE(load(n_inputs, in));
    EXPECT_TRUE(do_verify(in)) << "control failed to verify (n_inputs=" << n_inputs << ")";
  }
}

// m4 path_truncate: drop the last path entry's membership-proof bytes and decrement
// n_layers so the proof_len sanity check passes; the path no longer rebuilds the
// claimed root, so verify must reject.
TEST(C3Merkle, m4_path_truncate_rejected)
{
  for (const std::size_t n_inputs : ALL_SIZES)
  {
    VerifyInputs in;
    ASSERT_TRUE(load(n_inputs, in));

    // Decrement by 2, not 1: the tree_root is a Selene/Helios variant whose
    // parity must stay consistent with n_layers. Decrementing by 2 keeps the
    // root-curve parity consistent while still defeating the proof_len check.
    ASSERT_GT(in.n_layers, 1);
    const std::size_t truncated_len = fcmp_pp::fcmp_pp_proof_len(n_inputs, in.n_layers - 2);
    ASSERT_LT(truncated_len, in.fcmp_pp_proof.size());
    in.fcmp_pp_proof.resize(truncated_len);
    in.n_layers = in.n_layers - 2;
    EXPECT_FALSE(do_verify(in)) << "m4 accepted (n_inputs=" << n_inputs << ")";
  }
}

// m5 root_transplant: verify under a foreign vector file's tree root. The 128in
// vector was grown under a different tree than the 1/2/4/8in vectors, so pair each
// size with a cross-file donor root that differs.
TEST(C3Merkle, m5_root_transplant_rejected)
{
  for (const std::size_t n_inputs : ALL_SIZES)
  {
    VerifyInputs in;
    ASSERT_TRUE(load(n_inputs, in));

    const std::size_t donor_inputs = (n_inputs == 128) ? 1 : 128;
    VerifyInputs donor;
    ASSERT_TRUE(load(donor_inputs, donor));
    in.tree_root = donor.tree_root;
    EXPECT_FALSE(do_verify(in)) << "m5 accepted (n_inputs=" << n_inputs << ")";
  }
}

// m6 signable_transplant: verify under a foreign signable_tx_hash: a fixed
// non-zero value stands in for the foreign hash.
TEST(C3Merkle, m6_signable_transplant_rejected)
{
  crypto::hash foreign_hash = crypto::null_hash;
  static_assert(sizeof(foreign_hash) == 32, "unexpected hash size");
  memset(foreign_hash.data, 0xab, sizeof(foreign_hash));
  for (const std::size_t n_inputs : ALL_SIZES)
  {
    VerifyInputs in;
    ASSERT_TRUE(load(n_inputs, in));

    in.signable_tx_hash = foreign_hash;
    EXPECT_FALSE(do_verify(in)) << "m6 accepted (n_inputs=" << n_inputs << ")";
  }
}

// m1' leaf_tuple_swap (length-preserving substitute for m1/m2/m3): swap the whole
// per-input block (leaf tuple + SAL proof) between input positions 0 and 1. The
// pseudo-outs and key images stay in place, so position 0 now carries input 1's
// tuple against input 0's pseudo-out/key image. Must reject.
TEST(C3Merkle, m1_leaf_tuple_swap_rejected)
{
  for (const std::size_t n_inputs : {std::size_t{2}, std::size_t{4}, std::size_t{8}, std::size_t{128}})
  {
    VerifyInputs in;
    ASSERT_TRUE(load(n_inputs, in));

    ASSERT_GE(in.fcmp_pp_proof.size(), 2 * INPUT_BLOCK_SIZE_V1);
    std::swap_ranges(in.fcmp_pp_proof.begin(),
        in.fcmp_pp_proof.begin() + INPUT_BLOCK_SIZE_V1,
        in.fcmp_pp_proof.begin() + INPUT_BLOCK_SIZE_V1);
    EXPECT_FALSE(do_verify(in)) << "m1' accepted (n_inputs=" << n_inputs << ")";
  }
}
