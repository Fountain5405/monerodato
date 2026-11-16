// Same license as the rest of this repository (see LICENSE).
//
// See bad_miner_tx_in_block.h.
// Chain layout and the spend-tx construction are identical to bad_tx_in_block
// (17 hf2 miner blocks so the spend amount can be chosen at runtime from what
// those coinbases actually contain, 60 rewind blocks past the unlock window,
// one hf16 spacer, then the tested blocks). All txs in the tested block are
// VALID — the defect lives in the miner tx. The honest block is built first
// through the generator (construct_block_manually with bf_tx_fees), which
// yields the honest coinbase; the bad block is then built through the
// generator AGAIN on the same parent with bf_miner_tx, so the tampered
// coinbase is hashed and PoW-covered by the generator itself (tampering bytes
// after hashing would be a vacuous PoW refusal). The good block is the honest
// build, pushed after the bad one.

#include "bad_miner_tx_in_block.h"

#include "chaingen.h"
#include "cryptonote_core/cryptonote_core.h"
#include "cryptonote_basic/cryptonote_format_utils.h"

using namespace epee;
using namespace cryptonote;

static const size_t N_MINER_BLOCKS = 17;   // blocks h1..h17, one 4e9 coinbase output each
static const uint8_t EARLY_HF = 2;         // early chain: v1 coinbases so outputs are indexed by real amount
                                           // (the fork stores v2 miner-tx outputs as rct/amount-0)
static const uint8_t TEST_HF = 16;         // the hf of the tested blocks
static const size_t N_SPACERS = 1;         // one hf16 block so the chain reaches hf16 before the tested blocks

// The spendable amount is selected at runtime from the actual hf2 coinbase
// contents (no assumption about the fork's denomination scheme): any amount
// occurring >= RING_SIZE+1 times across h1..h17 that clears the hf16 fee floor.
static const uint64_t MIN_SPEND_AMOUNT = 10000000000ULL;  // ~6x the hf16 min fee for a 16-ring BP+ tx
static const size_t RING_SIZE = 16;                        // mixin 15 at hf16
static const uint64_t DUST_OUT = 5000;

gen_bad_miner_tx_in_block::gen_bad_miner_tx_in_block(size_t n_txes, size_t defect)
  : m_n_txes(n_txes), m_defect(defect)
{
  REGISTER_CALLBACK_METHOD(gen_bad_miner_tx_in_block, mark_invalid_block);
  REGISTER_CALLBACK_METHOD(gen_bad_miner_tx_in_block, check_final);
}

// Apply the coinbase defect. Returns false on a bad configuration.
static bool apply_coinbase_defect(transaction &miner_tx, size_t defect, uint64_t block_reward,
    size_t height, uint64_t already_generated_coins, const account_public_address &miner_address,
    uint64_t fees)
{
  switch (defect)
  {
  case 0:  // over-mint: one coinbase vout amount increased by the block reward
    if (miner_tx.vout.empty())
      return false;
    miner_tx.vout[0].amount += block_reward;
    return true;
  case 1:  // fee-claim: coinbase reward computed as base_reward + (fees + 1),
           // i.e. one unit more fee than the block's txs carry. The coinbase is
           // REBUILT by the framework helper, not patched. The helper APPENDS
           // (extra pubkey, vin, vout) without clearing, so it gets a fresh tx.
    {
      transaction rebuilt;
      if (!construct_miner_tx_manually(height, already_generated_coins, miner_address,
            rebuilt, fees + 1, TEST_HF))
        return false;
      miner_tx = rebuilt;
      return true;
    }
  }
  return false;
}

namespace
{
  struct out_ref
  {
    size_t block_idx;   // index into the miner-block array
    size_t vout_idx;    // index of the output within that coinbase
    crypto::public_key key;
  };

  // Build one spend of a pooled coinbase output: ring of RING_SIZE consecutive
  // pooled outputs containing the real one, two dust outputs, fee = remainder.
  // Returns the fee. (Verbatim from bad_tx_in_block.cpp.)
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

bool gen_bad_miner_tx_in_block::generate(std::vector<test_event_entry> &events) const
{
  uint64_t ts_start = 1338224400;

  GENERATE_ACCOUNT(miner_account);
  MAKE_GENESIS_BLOCK(events, blk_0, miner_account, ts_start);

  // the tested block's txs are submitted as events with relay_method::block
  // (kept-by-block), so they land in the pool for the block to pick up
  SET_EVENT_VISITOR_SETT(events, event_visitor_settings::set_txs_keeped_by_block);

  const uint64_t ts_step = DIFFICULTY_BLOCKS_ESTIMATE_TIMESPAN * 2;

  // 17 miner blocks + rewind, all mined by the same account
  block blocks[N_MINER_BLOCKS];
  const block *prev_block = &blk_0;
  for (size_t n = 0; n < N_MINER_BLOCKS; ++n)
  {
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blocks[n], *prev_block, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp | test_generator::bf_hf_version,
        EARLY_HF, EARLY_HF, prev_block->timestamp + ts_step,
        crypto::hash(), 0, transaction(), std::vector<crypto::hash>(), 0, 0, EARLY_HF),
        false, "Failed to generate miner block");
    events.push_back(blocks[n]);
    prev_block = blocks + n;
  }

  // rewind past the coinbase unlock window
  block blk_last = blocks[N_MINER_BLOCKS - 1];
  for (size_t i = 0; i < CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW; ++i)
  {
    block blk_r;
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_r, blk_last, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp | test_generator::bf_hf_version,
        EARLY_HF, EARLY_HF, blk_last.timestamp + ts_step,
        crypto::hash(), 0, transaction(), std::vector<crypto::hash>(), 0, 0, EARLY_HF),
        false, "Failed to generate rewind block");
    events.push_back(blk_r);
    blk_last = blk_r;
  }

  // collect the coinbase outputs of blocks h1..h17 by amount and pick one that
  // can fund a full ring: any amount with >= RING_SIZE+1 occurrences and
  // >= MIN_SPEND_AMOUNT (fee floor headroom)
  std::map<uint64_t, std::vector<out_ref>> by_amount;
  for (size_t b = 0; b < N_MINER_BLOCKS; ++b)
    for (size_t i = 0; i < blocks[b].miner_tx.vout.size(); ++i)
    {
      if (blocks[b].miner_tx.vout[i].amount < MIN_SPEND_AMOUNT)
        continue;
      out_ref ref;
      ref.block_idx = b;
      ref.vout_idx = i;
      CHECK_AND_ASSERT_MES(get_output_public_key(blocks[b].miner_tx.vout[i], ref.key), false, "no output key");
      by_amount[blocks[b].miner_tx.vout[i].amount].push_back(ref);
    }
  std::vector<out_ref> pool;
  uint64_t spend_amount = 0;
  for (const auto &e : by_amount)
    if (e.second.size() >= RING_SIZE + 1 && e.second.size() > pool.size())
      { pool = e.second; spend_amount = e.first; }
  if (pool.empty())
  {
    LOG_PRINT_L0("no hf2 coinbase amount occurs >= " << RING_SIZE + 1
        << " times with amount >= " << MIN_SPEND_AMOUNT << "; hf2 coinbase amounts:");
    for (const auto &e : by_amount)
      LOG_PRINT_L0("  amount " << e.first << " x" << e.second.size());
    return false;
  }
  LOG_PRINT_L0("spend amount " << spend_amount << " pool size " << pool.size());

  // one hf16 block so the tested (v16) blocks that follow are legal
  block blk_hf;
  CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_hf, blk_last, miner_account,
      test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp | test_generator::bf_hf_version,
      TEST_HF, TEST_HF, blk_last.timestamp + ts_step,
      crypto::hash(), 0, transaction(), std::vector<crypto::hash>(), 0, 0, TEST_HF),
      false, "Failed to generate the hf16 block");
  events.push_back(blk_hf);
  blk_last = blk_hf;

  // the tested txs; tx t spends pooled output t (all valid — the defect is in
  // the miner tx, so the bad block and the good block carry the same txs)
  std::unordered_map<crypto::public_key, cryptonote::subaddress_index> subaddresses;
  subaddresses[miner_account.get_keys().m_account_address.m_spend_public_key] = {0, 0};
  std::vector<transaction> txes;
  uint64_t fees = 0;
  for (size_t t = 0; t < m_n_txes; ++t)
  {
    txes.resize(txes.size() + 1);
    fees += build_spend(miner_account.get_keys(), pool, spend_amount, pool[t],
        get_tx_pub_key_from_extra(blocks[pool[t].block_idx].miner_tx), txes.back());
  }
  CHECK_AND_ASSERT_MES(fees == m_n_txes * (spend_amount - 2 * DUST_OUT), false, "unexpected fee sum");
  std::vector<crypto::hash> tx_hashes;
  for (const auto &tx : txes)
    tx_hashes.push_back(get_transaction_hash(tx));
  events.push_back(txes);

  // the honest block (h79, hf16): built first through the generator so the
  // honest coinbase and the block reward are known (this very block is pushed
  // below as the accepted replacement)
  const size_t honest_height = get_block_height(blk_last) + 1;
  const uint64_t already_generated_coins = generator.get_already_generated_coins(blk_last);
  block blk_good;
  CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_good, blk_last, miner_account,
      test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
      test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
      TEST_HF, TEST_HF, blk_last.timestamp + ts_step,
      crypto::hash(), 0, transaction(), tx_hashes, 0, 6, TEST_HF, fees),
      false, "Failed to generate the honest block");
  const uint64_t block_reward = get_outs_money_amount(blk_good.miner_tx);

  // the bad block (h79, hf16): the tampered coinbase is built INTO the block by
  // the generator (bf_miner_tx), so the block hash and PoW cover it
  transaction bad_miner_tx = blk_good.miner_tx;
  CHECK_AND_ASSERT_MES(apply_coinbase_defect(bad_miner_tx, m_defect, block_reward,
        honest_height, already_generated_coins, miner_account.get_keys().m_account_address, fees),
      false, "defect application failed");
  bad_miner_tx.invalidate_hashes();

  block blk_bad;
  CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_bad, blk_last, miner_account,
      test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
      test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs |
      test_generator::bf_tx_fees | test_generator::bf_miner_tx,
      TEST_HF, TEST_HF, blk_last.timestamp + ts_step,
      crypto::hash(), 0, bad_miner_tx, tx_hashes, 0, 6, TEST_HF, fees),
      false, "Failed to generate the bad block");
  DO_CALLBACK(events, "mark_invalid_block");
  events.push_back(blk_bad);

  // the good block: same parent, same txs, honest coinbase — must be accepted
  events.push_back(blk_good);

  DO_CALLBACK(events, "check_final");
  return true;
}

bool gen_bad_miner_tx_in_block::check_final(cryptonote::core &c, size_t ev_index, const std::vector<test_event_entry> &events)
{
  DEFINE_TESTS_ERROR_CONTEXT("gen_bad_miner_tx_in_block::check_final");
  const size_t expected_height = 1 /*genesis*/ + N_MINER_BLOCKS + CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW
      + N_SPACERS + 1 /*good block*/;
  CHECK_EQ(expected_height, c.get_current_blockchain_height());
  // NOTE: the replay builds a FRESH validator object, so nothing set during
  // generate() is visible here — the expected blocks are taken from the events.
  // The last two block events are the bad block and the honest replacement.
  const cryptonote::block *good_blk = nullptr;
  const cryptonote::block *bad_blk = nullptr;
  for (auto it = events.rbegin(); it != events.rend(); ++it)
    if (const cryptonote::block *b = boost::get<cryptonote::block>(&*it))
    {
      if (good_blk == nullptr)
        good_blk = b;
      else if (bad_blk == nullptr)
        bad_blk = b;
      else
        break;
    }
  CHECK_TEST_CONDITION(good_blk != nullptr && bad_blk != nullptr);
  // the tip is the honest replacement block; the dishonest coinbase (a
  // different miner tx on the same parent) must never have been written
  const crypto::hash tip_id = c.get_block_id_by_height(c.get_current_blockchain_height() - 1);
  CHECK_TEST_CONDITION(tip_id == get_block_hash(*good_blk));
  CHECK_TEST_CONDITION(tip_id != get_block_hash(*bad_blk));
  CHECK_TEST_CONDITION(get_transaction_hash(good_blk->miner_tx) != get_transaction_hash(bad_blk->miner_tx));
  size_t n_pool = 0;
  c.get_pool_transactions_count(n_pool);
  MDEBUG("final pool count: " << n_pool);
  return true;
}
