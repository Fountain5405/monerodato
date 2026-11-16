// Same license as the rest of this repository (see LICENSE).
//
// The sync route: a block carrying n transactions that are
// delivered ONLY via the 5-arg core overload handle_incoming_block(blob, block, bvc,
// pool_supplement&) — the txs NEVER touch the mempool — so verification runs
// ver_non_input_consensus over the pool_supplement during block processing. Three
// defects (bad BP+ proof, torsion key image, fee tamper) must refuse the bad block
// with height unchanged and the pool empty; the same block with the defective tx
// swapped for a fresh valid spend of the same output must be accepted. Two valid
// controls (17 and 33 txs) prove the route delivers verifiable txs.

#pragma once

#include "chaingen.h"

struct gen_bad_extra_tx : public test_chain_unit_base
{
  static const size_t DEFECT_NONE = (size_t)-1;
  size_t m_n_txes;     // total txs in the supplemented block (17 or 33)
  size_t m_defect;     // 0 = bad BP+ proof, 1 = torsion key image, 2 = fee tamper,
                       // DEFECT_NONE = valid control
  size_t m_bad_idx;    // position of the bad tx

  size_t m_invalid_block_index = (size_t)-1;
  mutable crypto::hash m_bad_tx_hash = crypto::null_hash;

  gen_bad_extra_tx(size_t n_txes, size_t defect = DEFECT_NONE, size_t bad_idx = 0);
  bool generate(std::vector<test_event_entry> &events) const;
  bool check_mid(cryptonote::core &c, size_t ev_index, const std::vector<test_event_entry> &)
  {
    // between the refused bad block and the replacement: height UNCHANGED, pool empty
    DEFINE_TESTS_ERROR_CONTEXT("gen_bad_extra_tx::check_mid");
    CHECK_EQ(79u, c.get_current_blockchain_height());
    size_t n_pool = 0;
    c.get_pool_transactions_count(n_pool);
    CHECK_EQ(0u, (unsigned)n_pool);
    return true;
  }
  bool check_final(cryptonote::core &c, size_t ev_index, const std::vector<test_event_entry> &events);
  bool mark_invalid_block(cryptonote::core &, size_t ev_index, const std::vector<test_event_entry> &)
  {
    m_invalid_block_index = ev_index + 1;
    return true;
  }
  bool check_block_verification_context(const cryptonote::block_verification_context &bvc, size_t event_idx, const cryptonote::block &)
  {
    if (m_invalid_block_index == event_idx)
      return bvc.m_verifivation_failed;
    return !bvc.m_verifivation_failed;
  }
};

// fixed-parameter instances
struct gen_bad_extra_tx_bp_first  : gen_bad_extra_tx { gen_bad_extra_tx_bp_first()  : gen_bad_extra_tx(17, 0, 0)  {} };
struct gen_bad_extra_tx_ki_mid    : gen_bad_extra_tx { gen_bad_extra_tx_ki_mid()    : gen_bad_extra_tx(17, 1, 8)  {} };
struct gen_bad_extra_tx_sum_last  : gen_bad_extra_tx { gen_bad_extra_tx_sum_last()  : gen_bad_extra_tx(17, 2, 16) {} };
struct gen_bad_extra_tx_bp_mid_33 : gen_bad_extra_tx { gen_bad_extra_tx_bp_mid_33() : gen_bad_extra_tx(33, 0, 16) {} };
struct gen_bad_extra_tx_valid_17  : gen_bad_extra_tx { gen_bad_extra_tx_valid_17()  : gen_bad_extra_tx(17)        {} };
struct gen_bad_extra_tx_valid_33  : gen_bad_extra_tx { gen_bad_extra_tx_valid_33()  : gen_bad_extra_tx(33)        {} };

// The replay derives the hardfork schedule from get_test_options<T> (chaingen.h),
// NOT from the block versions in the events; without a specialization the default
// is hf 1 only. Chain: genesis (h0, hf1); 17 miner + 60 rewind blocks (h1..h77,
// hf2 — v1 coinbases, so outputs carry real amounts and land in the per-amount
// index); one empty hf16 block (h78); the tested supplemented blocks at h79 (hf16).
struct get_bad_extra_tx_test_options
{
  const std::pair<uint8_t, uint64_t> hard_forks[4] = {std::make_pair(1, 0), std::make_pair(2, 1), std::make_pair((uint8_t)16, 78), std::make_pair(0, 0)};
  const cryptonote::test_options test_options = {
    hard_forks, 0
  };
};
template<> struct get_test_options<gen_bad_extra_tx_bp_first>  : public get_bad_extra_tx_test_options {};
template<> struct get_test_options<gen_bad_extra_tx_ki_mid>    : public get_bad_extra_tx_test_options {};
template<> struct get_test_options<gen_bad_extra_tx_sum_last>  : public get_bad_extra_tx_test_options {};
template<> struct get_test_options<gen_bad_extra_tx_bp_mid_33> : public get_bad_extra_tx_test_options {};
template<> struct get_test_options<gen_bad_extra_tx_valid_17>  : public get_bad_extra_tx_test_options {};
template<> struct get_test_options<gen_bad_extra_tx_valid_33>  : public get_bad_extra_tx_test_options {};
