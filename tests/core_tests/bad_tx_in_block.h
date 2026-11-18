// Same license as the rest of this repository (see LICENSE).
//
// A block carrying ONE invalid transaction the pool has never seen must be refused end to
// end, above the batched key-image (>=8 txs) and parallel sum-check (>=16) thresholds.
// Defects: bad BP+ proof, torsion key image, broken balance. The same block with the
// bad tx replaced by a valid one must be accepted.

#pragma once

#include "chaingen.h"

struct gen_bad_tx_in_block : public test_chain_unit_base
{
  size_t m_n_txes;     // total txs in the bad block (9 or 17)
  size_t m_defect;     // 0 = bad BP+ proof, 1 = torsion key image, 2 = broken balance,
                       // 3 = broken ring signature (input-side; the only defect the
                       //     pool stores unvalidated, so the BLOCK path verifies it),
                       //     4 = BP+ L/R swap
  size_t m_bad_idx;    // position of the bad tx
  bool m_serialize_blocks = false; // push the tested blocks as serialized_block (bytes route:
                                   // handle_incoming_block(blob, NULL), core reparses) instead
                                   // of the object route
  bool m_bitflip = false;          // bitflip mode: no defective tx; the VALID block is
                                   // serialized, one bit is flipped inside a tx-hash entry
                                   // (a merkle-tree leaf) and pushed as serialized_block —
                                   // the blob still parses but fails block verification
  size_t m_flip_tx = 0;            // bitflip mode: which block tx hash entry to corrupt
  size_t m_flip_byte = 0;          // bitflip mode: which byte of that hash (bit 0x01 flips)

  size_t m_invalid_tx_index = 0;
  size_t m_invalid_block_index = 0;
  mutable crypto::hash m_bad_tx_hash = crypto::null_hash;

  gen_bad_tx_in_block(size_t n_txes, size_t defect, size_t bad_idx,
      bool serialize_blocks = false, bool bitflip = false, size_t flip_tx = 0, size_t flip_byte = 0);
  bool generate(std::vector<test_event_entry> &events) const;
  bool check_final(cryptonote::core &c, size_t ev_index, const std::vector<test_event_entry> &events);
  bool mark_invalid_tx(cryptonote::core &, size_t ev_index, const std::vector<test_event_entry> &)
  {
    m_invalid_tx_index = ev_index + 1;
    return true;
  }
  bool mark_invalid_block(cryptonote::core &, size_t ev_index, const std::vector<test_event_entry> &)
  {
    m_invalid_block_index = ev_index + 1;
    return true;
  }
  // the single defective tx is pool-submitted on its own event: a bad BP+
  // proof is refused outright by non-input consensus; input-side defects are
  // stored unvalidated (kept-by-block) — flagged either way
  bool check_tx_verification_context(const cryptonote::tx_verification_context &tvc, bool, size_t event_idx, const cryptonote::transaction &)
  {
    if (m_invalid_tx_index == event_idx)
      return tvc.m_verifivation_failed || tvc.m_verifivation_impossible;
    return !tvc.m_verifivation_failed && !tvc.m_verifivation_impossible;
  }
  bool check_block_verification_context(const cryptonote::block_verification_context &bvc, size_t event_idx, const cryptonote::block &)
  {
    if (m_invalid_block_index == event_idx)
      return bvc.m_verifivation_failed;
    return !bvc.m_verifivation_failed;
  }
};

// fixed-parameter instances
struct gen_bad_tx_in_block_bp_9  : gen_bad_tx_in_block { gen_bad_tx_in_block_bp_9()  : gen_bad_tx_in_block(9,  0, 4) {} };
struct gen_bad_tx_in_block_bp_17 : gen_bad_tx_in_block { gen_bad_tx_in_block_bp_17() : gen_bad_tx_in_block(17, 0, 8) {} };
struct gen_bad_tx_in_block_ki_9  : gen_bad_tx_in_block { gen_bad_tx_in_block_ki_9()  : gen_bad_tx_in_block(9,  1, 4) {} };
struct gen_bad_tx_in_block_ki_17 : gen_bad_tx_in_block { gen_bad_tx_in_block_ki_17() : gen_bad_tx_in_block(17, 1, 8) {} };
struct gen_bad_tx_in_block_bal_9  : gen_bad_tx_in_block { gen_bad_tx_in_block_bal_9()  : gen_bad_tx_in_block(9,  2, 4) {} };
struct gen_bad_tx_in_block_bal_17 : gen_bad_tx_in_block { gen_bad_tx_in_block_bal_17() : gen_bad_tx_in_block(17, 2, 8) {} };
struct gen_bad_tx_in_block_sig_9 : gen_bad_tx_in_block { gen_bad_tx_in_block_sig_9() : gen_bad_tx_in_block(9,  3, 4) {} };
struct gen_bad_tx_in_block_sig_17 : gen_bad_tx_in_block { gen_bad_tx_in_block_sig_17() : gen_bad_tx_in_block(17, 3, 8) {} };
struct gen_bad_tx_in_block_sig_8  : gen_bad_tx_in_block { gen_bad_tx_in_block_sig_8()  : gen_bad_tx_in_block(8,  3, 4) {} };
struct gen_bad_tx_in_block_sig_16 : gen_bad_tx_in_block { gen_bad_tx_in_block_sig_16() : gen_bad_tx_in_block(16, 3, 8) {} };
struct gen_bad_tx_in_block_perm_8  : gen_bad_tx_in_block { gen_bad_tx_in_block_perm_8()  : gen_bad_tx_in_block(8,  4, 4) {} };
struct gen_bad_tx_in_block_perm_16 : gen_bad_tx_in_block { gen_bad_tx_in_block_perm_16() : gen_bad_tx_in_block(16, 4, 8) {} };
// serialized route, bitflip mode: the valid 9-tx block with one bit flipped in tx-hash entry 0, 4 or 8
// (bytes 0, 15, 31), so that the blob still parses and only verification refuses it
struct gen_bad_ser_block_flip_0 : gen_bad_tx_in_block { gen_bad_ser_block_flip_0() : gen_bad_tx_in_block(9, 0, 4, true, true, 0, 0)  {} };
struct gen_bad_ser_block_flip_4 : gen_bad_tx_in_block { gen_bad_ser_block_flip_4() : gen_bad_tx_in_block(9, 0, 4, true, true, 4, 15) {} };
struct gen_bad_ser_block_flip_8 : gen_bad_tx_in_block { gen_bad_ser_block_flip_8() : gen_bad_tx_in_block(9, 0, 4, true, true, 8, 31) {} };

// The replay derives the hardfork schedule from get_test_options<T> (chaingen.h),
// NOT from the block versions in the events; without a specialization the default
// is hf 1 only and the first v2 miner block is refused ("has old version").
// Chain: genesis (h0, hf1); 17 miner + 60 rewind blocks (h1..h77, hf2 — v1
// coinbases, so outputs carry real amounts and land in the per-amount index;
// the fork stores hf8+ v2 miner-tx outputs as committed/amount-0, which the
// per-amount lookup cannot see); one empty hf16 block (h78); tested blocks at
// h79+ (hf16).
struct get_bad_tx_in_block_test_options
{
  const std::pair<uint8_t, uint64_t> hard_forks[4] = {std::make_pair(1, 0), std::make_pair(2, 1), std::make_pair((uint8_t)16, 78), std::make_pair(0, 0)};
  const cryptonote::test_options test_options = {
    hard_forks, 0
  };
};
template<> struct get_test_options<gen_bad_tx_in_block_bp_9> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_bp_17> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_ki_9> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_ki_17> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_bal_9> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_bal_17> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_sig_9> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_sig_17> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_sig_8> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_sig_16> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_perm_8> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_tx_in_block_perm_16> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_ser_block_flip_0> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_ser_block_flip_4> : public get_bad_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_ser_block_flip_8> : public get_bad_tx_in_block_test_options {};
