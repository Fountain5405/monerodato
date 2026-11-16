// Same license as the rest of this repository (see LICENSE).
//
// See bad_extra_tx_in_block.h.
// Chain: genesis (hf1); 17 miner + 60 rewind blocks (hf2 — v1 coinbases so the
// outputs carry real amounts in the per-amount index; the spend amounts are
// chosen at runtime from what those coinbases actually contain); one hf16
// spacer block; then the tested supplemented blocks at hf16. The n_txes txs are
// delivered ONLY as the pool supplement of the supplemented_block event (5-arg
// handle_incoming_block) — NO transaction events anywhere, the pool must never
// hold them. A bad BP+ proof, a torsion key image and a fee tamper all fail
// NON-input consensus, which block processing runs over the supplement
// (blockchain.cpp ver_non_input_consensus(extra_block_txs...)): the bad block
// must be refused whole (height unchanged, defective tx never in the DB, pool
// empty), and the same block with the defective tx swapped for a fresh valid
// spend of the same output must extend the chain.

#include "bad_extra_tx_in_block.h"

#include "chaingen.h"
#include "cryptonote_core/cryptonote_core.h"
#include "cryptonote_core/cryptonote_tx_utils.h"
#include "cryptonote_basic/cryptonote_format_utils.h"
#include "ringct/rctSigs.h"
#include "ringct/rctOps.h"

using namespace epee;
using namespace cryptonote;

static const size_t N_MINER_BLOCKS = 17;   // blocks h1..h17, one coinbase each
static const uint8_t EARLY_HF = 2;         // early chain: v1 coinbases so outputs are indexed by real amount
static const uint8_t TEST_HF = 16;         // the hf of the tested blocks
static const size_t N_SPACERS = 1;         // one hf16 block so the chain reaches hf16 before the tested blocks

// The hf2 coinbases are hand-built with SPEND_COINBASE_OUTS outputs of the same
// SPEND_AMOUNT each (far below the ~1.7e13 hf2 base reward), so the per-amount
// pool over h1..h17 has plenty of spendable outputs for mixin-15 rings even for
// the 33-tx instance. Only h1..h17 coinbases are spendable at the tested block:
// the fork spends at top + CRYPTONOTE_LOCKED_TX_ALLOWED_DELTA_BLOCKS (=1), so any
// later coinbase is still locked there.
static const size_t SPEND_COINBASE_OUTS = 3;
static const uint64_t SPEND_AMOUNT = 2000000000000ULL;   // 2e12, ~6x the hf16 min fee floor
static const size_t RING_SIZE = 16;                       // mixin 15 at hf16
static const uint64_t DUST_OUT = 5000;

gen_bad_extra_tx::gen_bad_extra_tx(size_t n_txes, size_t defect, size_t bad_idx)
  : m_n_txes(n_txes), m_defect(defect), m_bad_idx(bad_idx)
{
  REGISTER_CALLBACK_METHOD(gen_bad_extra_tx, mark_invalid_block);
  REGISTER_CALLBACK_METHOD(gen_bad_extra_tx, check_mid);
  REGISTER_CALLBACK_METHOD(gen_bad_extra_tx, check_final);
}

// Apply the defect to a constructed tx. Returns false on a bad configuration.
static bool apply_defect(transaction &tx, size_t defect)
{
  switch (defect)
  {
  case 0:  // bad BP+ proof
    if (tx.rct_signatures.p.bulletproofs_plus.empty())
      return false;
    tx.rct_signatures.p.bulletproofs_plus[0].r1 = rct::skGen();
    return true;
  case 1:
    { // torsion key image: valid point outside the prime-order subgroup
      if (tx.vin.empty())
        return false;
      txin_to_key &in = boost::get<txin_to_key>(tx.vin[0]);
      rct::key t = rct::identity();
      t.bytes[0] = 0xec;
      for (int b = 1; b < 31; ++b)
        t.bytes[b] = 0xff;
      t.bytes[31] = 0x7f;
      const rct::key cur = rct::ki2rct(in.k_image);
      const rct::key bad = rct::addKeys(cur, t);
      memcpy(&in.k_image, &bad, 32);
      return true;
    }
  case 2:  // broken balance (fee tamper)
    tx.rct_signatures.txnFee += 1;
    return true;
  }
  return false;
}

namespace
{
  // Hand-build a v1 coinbase with n_outs equal outputs of `amount` to `addr`
  // (modeled on construct_miner_tx: fresh txkey in extra, per-output derivations).
  bool make_coinbase(size_t height, size_t n_outs, uint64_t amount,
      const account_public_address &addr, transaction &tx)
  {
    tx.version = 1;
    tx.unlock_time = height + CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW;
    txin_gen in;
    in.height = height;
    tx.vin.push_back(in);
    keypair txkey = keypair::generate(hw::get_device("default"));
    CHECK_AND_ASSERT_MES(add_tx_pub_key_to_extra(tx, txkey.pub), false, "failed to add tx pub key to extra");
    crypto::key_derivation derivation = AUTO_VAL_INIT(derivation);
    CHECK_AND_ASSERT_MES(crypto::generate_key_derivation(addr.m_view_public_key, txkey.sec, derivation),
        false, "failed to generate key derivation");
    for (size_t i = 0; i < n_outs; ++i)
    {
      crypto::public_key out_eph_public_key = AUTO_VAL_INIT(out_eph_public_key);
      CHECK_AND_ASSERT_MES(crypto::derive_public_key(derivation, i, addr.m_spend_public_key, out_eph_public_key),
          false, "failed to derive public key");
      txout_target_v target = out_eph_public_key;
      tx.vout.push_back(tx_out{amount, target});
    }
    tx.invalidate_hashes();
    return true;
  }

  struct out_ref
  {
    size_t block_idx;   // index into the all-blocks array
    size_t vout_idx;    // index of the output within that coinbase
    crypto::public_key key;
  };

  // Build one spend of a pooled coinbase output: ring of RING_SIZE consecutive
  // pooled outputs containing the real one, two dust outputs, fee = remainder.
  // Returns the fee.
  uint64_t build_spend(const account_keys &sender,
      const std::vector<out_ref> &pool, uint64_t amount, const out_ref &real,
      const crypto::public_key &real_tx_pub_key, transaction &tx)
  {
    tx_source_entry src;
    src.amount = amount;
    // push_output's first argument is the output's per-amount global index,
    // which here equals the pool position (pool holds this amount's outputs in
    // chain order from the first block, so nothing shifts the numbering). The
    // ring is a window of RING_SIZE consecutive indices containing the real
    // output, pushed in ascending order: key_offsets are relative deltas of the
    // sorted indices, and the signature columns must line up with the keys the
    // core resolves from those offsets.
    CHECK_AND_ASSERT_MES(pool.size() >= RING_SIZE, 0, "pool too small for a ring");
    size_t real_pos_pool = (size_t)-1;
    for (size_t k = 0; k < pool.size(); ++k)
      if (pool[k].block_idx == real.block_idx && pool[k].vout_idx == real.vout_idx)
        { real_pos_pool = k; break; }
    CHECK_AND_ASSERT_MES(real_pos_pool != (size_t)-1, 0, "real output not in pool");
    const size_t window_start = real_pos_pool >= RING_SIZE - 1 ? real_pos_pool - (RING_SIZE - 1) : 0;
    CHECK_AND_ASSERT_MES(window_start + RING_SIZE <= pool.size(), 0, "ring window overruns pool");
    size_t real_pos = RING_SIZE;
    for (size_t m = 0; m < RING_SIZE; ++m)
    {
      const out_ref &oref = pool[window_start + m];
      src.push_output(window_start + m, oref.key, amount);
      if (oref.block_idx == real.block_idx && oref.vout_idx == real.vout_idx)
        real_pos = m;
    }
    CHECK_AND_ASSERT_MES(real_pos < RING_SIZE, 0, "real output missing from ring");
    src.real_output = real_pos;
    src.real_output_in_tx_index = real.vout_idx;
    src.real_out_tx_key = real_tx_pub_key;
    src.mask = rct::identity();
    src.rct = false;

    tx_destination_entry td;
    td.addr = sender.m_account_address;
    td.amount = DUST_OUT;
    std::vector<tx_destination_entry> destinations;
    destinations.push_back(td);
    destinations.push_back(td);

    std::vector<tx_source_entry> sources{src};
    crypto::secret_key tx_key;
    std::vector<crypto::secret_key> additional_tx_keys;
    std::unordered_map<crypto::public_key, cryptonote::subaddress_index> subaddresses;
    subaddresses[sender.m_account_address.m_spend_public_key] = {0, 0};
    const rct::RCTConfig rct_config = { rct::RangeProofPaddedBulletproof, 4 };
    const bool r = construct_tx_and_get_tx_key(sender, subaddresses, sources,
        destinations, boost::none, std::vector<uint8_t>(), tx, tx_key, additional_tx_keys,
        true, rct_config, true /* use_view_tags: hf16 requires tagged outputs */);
    CHECK_AND_ASSERT_MES(r, 0, "failed to construct transaction");
    return amount - 2 * DUST_OUT;
  }
}

bool gen_bad_extra_tx::generate(std::vector<test_event_entry> &events) const
{
  uint64_t ts_start = 1338224400;

  GENERATE_ACCOUNT(miner_account);
  MAKE_GENESIS_BLOCK(events, blk_0, miner_account, ts_start);

  const uint64_t ts_step = DIFFICULTY_BLOCKS_ESTIMATE_TIMESPAN * 2;

  // 17 miner blocks with hand-built 3-output coinbases + 60 rewind blocks, all
  // mined by the same account
  std::vector<block> blocks_all;
  blocks_all.reserve(N_MINER_BLOCKS + CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW);
  const block *prev_block = &blk_0;
  for (size_t n = 0; n < N_MINER_BLOCKS + CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW; ++n)
  {
    blocks_all.resize(blocks_all.size() + 1);
    if (n < N_MINER_BLOCKS)
    {
      transaction miner_tx;
      CHECK_AND_ASSERT_MES(make_coinbase(n + 1, SPEND_COINBASE_OUTS, SPEND_AMOUNT,
            miner_account.get_keys().m_account_address, miner_tx),
          false, "Failed to construct the coinbase");
      CHECK_AND_ASSERT_MES(generator.construct_block_manually(blocks_all.back(), *prev_block, miner_account,
          test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
          test_generator::bf_miner_tx | test_generator::bf_hf_version,
          EARLY_HF, EARLY_HF, prev_block->timestamp + ts_step,
          crypto::hash(), 0, miner_tx, std::vector<crypto::hash>(), 0, 0, EARLY_HF),
          false, "Failed to generate miner block");
    }
    else
    {
      CHECK_AND_ASSERT_MES(generator.construct_block_manually(blocks_all.back(), *prev_block, miner_account,
          test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp | test_generator::bf_hf_version,
          EARLY_HF, EARLY_HF, prev_block->timestamp + ts_step,
          crypto::hash(), 0, transaction(), std::vector<crypto::hash>(), 0, 0, EARLY_HF),
          false, "Failed to generate rewind block");
    }
    events.push_back(blocks_all.back());
    prev_block = &blocks_all.back();
  }

  // collect the spendable coinbase outputs of blocks h1..h17 (their unlock
  // times are <= the tested block's spend height; later coinbases are locked
  // there): SPEND_COINBASE_OUTS * N_MINER_BLOCKS outputs of one amount
  std::vector<out_ref> pool;
  for (size_t b = 0; b < N_MINER_BLOCKS; ++b)
    for (size_t i = 0; i < blocks_all[b].miner_tx.vout.size(); ++i)
    {
      out_ref ref;
      ref.block_idx = b;
      ref.vout_idx = i;
      CHECK_AND_ASSERT_MES(get_output_public_key(blocks_all[b].miner_tx.vout[i], ref.key), false, "no output key");
      pool.push_back(ref);
    }
  CHECK_AND_ASSERT_MES(pool.size() >= RING_SIZE && pool.size() >= m_n_txes, false,
      "only " << pool.size() << " spendable outputs for " << m_n_txes << " txs");
  LOG_PRINT_L0("spendable outputs " << pool.size() << " (need " << m_n_txes << ") amount " << SPEND_AMOUNT);

  // one hf16 block so the tested (v16) blocks that follow are legal
  block blk_hf;
  CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_hf, *prev_block, miner_account,
      test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp | test_generator::bf_hf_version,
      TEST_HF, TEST_HF, prev_block->timestamp + ts_step,
      crypto::hash(), 0, transaction(), std::vector<crypto::hash>(), 0, 0, TEST_HF),
      false, "Failed to generate the hf16 block");
  events.push_back(blk_hf);

  // the tested txs; tx t spends pooled output t (one real output per tx, decoys
  // may be shared between txs)
  std::vector<transaction> txes;
  uint64_t fees = 0;
  for (size_t t = 0; t < m_n_txes; ++t)
  {
    txes.resize(txes.size() + 1);
    fees += build_spend(miner_account.get_keys(), pool, SPEND_AMOUNT, pool[t],
        get_tx_pub_key_from_extra(blocks_all[pool[t].block_idx].miner_tx), txes.back());
  }
  CHECK_AND_ASSERT_MES(fees == m_n_txes * (SPEND_AMOUNT - 2 * DUST_OUT), false, "unexpected fee sum");

  // apply the defect, if any, then hash (mirrors the bad_tx_in_block family:
  // block hash lists reference the defected tx)
  std::vector<crypto::hash> tx_hashes;
  if (m_defect != DEFECT_NONE)
  {
    transaction &bad_tx = txes[m_bad_idx];
    CHECK_AND_ASSERT_MES(apply_defect(bad_tx, m_defect), false, "defect application failed");
    bad_tx.invalidate_hashes();
    m_bad_tx_hash = get_transaction_hash(bad_tx);
  }
  for (const auto &tx : txes)
    tx_hashes.push_back(get_transaction_hash(tx));

  if (m_defect != DEFECT_NONE)
  {
    // the bad block (h79, hf16): txs ride the pool supplement only
    block blk_bad;
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_bad, blk_hf, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
        test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
        TEST_HF, TEST_HF, blk_hf.timestamp + ts_step,
        crypto::hash(), 0, transaction(), tx_hashes, 0, 6, TEST_HF, fees),
        false, "Failed to generate the bad block");
    DO_CALLBACK(events, "mark_invalid_block");
    events.push_back(supplemented_block(blk_bad, txes));

    DO_CALLBACK(events, "check_mid");

    // the good block: same txs, the defective one replaced by a fresh valid one
    // that spends the very output the rejected tx claimed (it was never spent:
    // the txs never entered the pool and the bad block was refused whole)
    transaction good_tx;
    CHECK_AND_ASSERT_MES(build_spend(miner_account.get_keys(), pool, SPEND_AMOUNT, pool[m_bad_idx],
          get_tx_pub_key_from_extra(blocks_all[pool[m_bad_idx].block_idx].miner_tx), good_tx) != 0,
        false, "failed to construct the replacement transaction");
    std::vector<transaction> good_txes(txes);
    good_txes[m_bad_idx] = good_tx;
    std::vector<crypto::hash> ok_hashes = tx_hashes;
    ok_hashes[m_bad_idx] = get_transaction_hash(good_tx);

    block blk_good;
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_good, blk_hf, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
        test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
        TEST_HF, TEST_HF, blk_hf.timestamp + ts_step,
        crypto::hash(), 0, transaction(), ok_hashes, 0, 6, TEST_HF, fees),
        false, "Failed to generate the good block");
    events.push_back(supplemented_block(blk_good, good_txes));
  }
  else
  {
    // valid control: a single supplemented block must be accepted
    block blk_valid;
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_valid, blk_hf, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
        test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
        TEST_HF, TEST_HF, blk_hf.timestamp + ts_step,
        crypto::hash(), 0, transaction(), tx_hashes, 0, 6, TEST_HF, fees),
        false, "Failed to generate the valid block");
    events.push_back(supplemented_block(blk_valid, txes));
  }

  DO_CALLBACK(events, "check_final");
  return true;
}

bool gen_bad_extra_tx::check_final(cryptonote::core &c, size_t ev_index, const std::vector<test_event_entry> &)
{
  DEFINE_TESTS_ERROR_CONTEXT("gen_bad_extra_tx::check_final");
  const size_t expected_height = 1 /*genesis*/ + N_MINER_BLOCKS + CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW
      + N_SPACERS + 1 /*the accepted supplemented block*/;
  CHECK_EQ(expected_height, c.get_current_blockchain_height());
  // the bad transaction must never have been written to the blockchain
  CHECK_TEST_CONDITION(!c.get_blockchain_storage().have_tx(m_bad_tx_hash));
  size_t n_pool = 0;
  c.get_pool_transactions_count(n_pool);
  CHECK_EQ(0u, (unsigned)n_pool);
  MDEBUG("final height " << c.get_current_blockchain_height() << ", final pool count: " << n_pool);
  return true;
}
