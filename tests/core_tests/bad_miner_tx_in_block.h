// Same license as the rest of this repository (see LICENSE).
//
// A MINED block whose COINBASE is
// dishonest must be refused end to end, and the same block with an honest
// coinbase (same parent, same txs) must be accepted. Defects: d0 over-mint
// (a coinbase vout amount increased by the block reward), d1 fee-claim
// (coinbase pays base_reward + fees + 1). d2 (replanted output commitment) is
// NOT instantiated: at the tested hf16 the coinbase is an amount-bearing
// v2-style tx — construct_miner_tx never populates rct_signatures.outPk
// (committed Carrot coinbases only start at HF_VERSION_CARROT = 17), so there
// is no commitment to replant.

#pragma once

#include "chaingen.h"

struct gen_bad_miner_tx_in_block : public test_chain_unit_base
{
  size_t m_n_txes;     // total txs in the tested block (1 or 17)
  size_t m_defect;     // 0 = coinbase over-mint, 1 = coinbase claims fees + 1

  size_t m_invalid_block_index = 0;

  gen_bad_miner_tx_in_block(size_t n_txes, size_t defect);
  bool generate(std::vector<test_event_entry> &events) const;
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
struct gen_bad_miner_tx_over_1  : gen_bad_miner_tx_in_block { gen_bad_miner_tx_over_1()  : gen_bad_miner_tx_in_block(1,  0) {} };
struct gen_bad_miner_tx_over_17 : gen_bad_miner_tx_in_block { gen_bad_miner_tx_over_17() : gen_bad_miner_tx_in_block(17, 0) {} };
struct gen_bad_miner_tx_fee_1   : gen_bad_miner_tx_in_block { gen_bad_miner_tx_fee_1()   : gen_bad_miner_tx_in_block(1,  1) {} };
struct gen_bad_miner_tx_fee_17  : gen_bad_miner_tx_in_block { gen_bad_miner_tx_fee_17()  : gen_bad_miner_tx_in_block(17, 1) {} };

// The replay derives the hardfork schedule from get_test_options<T> (chaingen.h),
// NOT from the block versions in the events; without a specialization the default
// is hf 1 only and the first v2 miner block is refused ("has old version").
// Chain: genesis (h0, hf1); 17 miner + 60 rewind blocks (h1..h77, hf2 — v1
// coinbases, so outputs carry real amounts and land in the per-amount index);
// one empty hf16 block (h78); tested blocks at h79+ (hf16).
struct get_bad_miner_tx_in_block_test_options
{
  const std::pair<uint8_t, uint64_t> hard_forks[4] = {std::make_pair(1, 0), std::make_pair(2, 1), std::make_pair((uint8_t)16, 78), std::make_pair(0, 0)};
  const cryptonote::test_options test_options = {
    hard_forks, 0
  };
};
template<> struct get_test_options<gen_bad_miner_tx_over_1>  : public get_bad_miner_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_miner_tx_over_17> : public get_bad_miner_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_miner_tx_fee_1>   : public get_bad_miner_tx_in_block_test_options {};
template<> struct get_test_options<gen_bad_miner_tx_fee_17>  : public get_bad_miner_tx_in_block_test_options {};
