// Same license as the rest of this repository (see LICENSE).
//
// See bad_tx_in_block.h.
// Chain: genesis (hf1); 17 miner + 60 rewind blocks (hf2 — v1 coinbases so the
// outputs carry real amounts in the per-amount index; the spend amount is
// chosen at runtime from what those coinbases actually contain); one hf16
// spacer block; then the tested blocks at hf16. The n_txes txs are submitted
// as events with relay_method::block (kept-by-block). In this fork a bad BP+
// proof, a torsion key image and a broken balance all fail NON-input
// consensus at pool admission (rct semantics / KI domain checks live there),
// so the tx never becomes pool-available and the block is refused over the
// unavailable tx — end-to-end refusal, with the pool as the gate. A broken
// RING SIGNATURE passes non-input consensus, fails check_tx_inputs, and is
// stored in the pool UNVALIDATED by the kept-by-block path: the bad block's
// verification is then the FIRST to actually verify it — the pool-vs-block
// divergence scenario. The bad block must be refused whole (height unchanged,
// defective tx never in the DB); the same block with the defective tx swapped
// for a fresh valid spend of the same output must extend the chain.

#include "bad_tx_in_block.h"

#include "chaingen.h"
#include "cryptonote_core/cryptonote_core.h"
#include "cryptonote_basic/cryptonote_format_utils.h"
#include "ringct/rctSigs.h"
#include "ringct/rctOps.h"

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

gen_bad_tx_in_block::gen_bad_tx_in_block(size_t n_txes, size_t defect, size_t bad_idx,
    bool serialize_blocks, bool bitflip, size_t flip_tx, size_t flip_byte)
  : m_n_txes(n_txes), m_defect(defect), m_bad_idx(bad_idx),
    m_serialize_blocks(serialize_blocks), m_bitflip(bitflip),
    m_flip_tx(flip_tx), m_flip_byte(flip_byte)
{
  REGISTER_CALLBACK_METHOD(gen_bad_tx_in_block, mark_invalid_tx);
  REGISTER_CALLBACK_METHOD(gen_bad_tx_in_block, mark_invalid_block);
  REGISTER_CALLBACK_METHOD(gen_bad_tx_in_block, check_final);
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
  case 3:
    { // broken ring signature (input-side: passes non-input consensus, is
      // stored unvalidated by the kept-by-block pool, and the BLOCK path is
      // the first to actually verify it)
      if (tx.rct_signatures.p.CLSAGs.empty() || tx.rct_signatures.p.CLSAGs[0].s.empty())
        return false;
      tx.rct_signatures.p.CLSAGs[0].s[0] = rct::skGen();
      return true;
    }
  case 4:
    { // swapped BP+ L/R vectors (proof permutation: passes non-input
      // consensus, fails check_tx_inputs)
      auto &bp = tx.rct_signatures.p.bulletproofs_plus[0];
      if (bp.L.empty() || bp.L.size() != bp.R.size())
        return false;
      std::swap(bp.L, bp.R);
      return true;
    }
  }
  return false;
}

namespace
{
  // Flip the low bit of byte `byte_idx` of `tx_hash` inside the serialized block blob.
  // The block's tx_hashes vector holds fixed-size 32-byte entries, so every byte of the
  // entry sits past all varint/fixed prefixes of the blob: the corrupted blob still
  // parses; only block verification (the hash is now a merkle leaf that matches no
  // pooled tx / changes the computed merkle root) can reject it.
  static bool apply_bitflip(blobdata &blob, const crypto::hash &tx_hash, size_t byte_idx)
  {
    CHECK_AND_ASSERT_MES(byte_idx < sizeof(tx_hash.data), false, "bitflip byte index out of range");
    size_t hits = 0, pos = 0;
    for (size_t i = 0; i + sizeof(tx_hash.data) <= blob.size(); ++i)
    {
      if (memcmp(blob.data() + i, tx_hash.data, sizeof(tx_hash.data)) == 0)
      {
        ++hits;
        pos = i;
      }
    }
    CHECK_AND_ASSERT_MES(hits == 1, false, "tx hash found " << hits << " times in the block blob");
    blob[pos + byte_idx] ^= 0x01;
    return true;
  }

  struct out_ref
  {
    size_t block_idx;   // index into the miner-block array
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

bool gen_bad_tx_in_block::generate(std::vector<test_event_entry> &events) const
{
  uint64_t ts_start = 1338224400;

  GENERATE_ACCOUNT(miner_account);
  MAKE_GENESIS_BLOCK(events, blk_0, miner_account, ts_start);

  // Submit tx events with relay_method::block (kept-by-block): txs failing
  // check_tx_inputs (a broken ring signature) are stored in the pool
  // UNVALIDATED instead of refused, so the bad block's verification path is
  // the first to really check them — the pool-vs-block divergence scenario.
  // Non-input consensus (bad BP+ proof, torsion KI, broken balance) still
  // refuses at admission in this fork; those blocks are refused over the
  // unavailable tx.
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

  // the tested txs; tx t spends pooled output t
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

  // the tested blocks go out as bytes (serialized_block) on the serialized route,
  // or as objects on the object route
  auto push_tested_block = [this](std::vector<test_event_entry> &events, const block &blk)
  {
    if (m_serialize_blocks)
      events.push_back(serialized_block(t_serializable_object_to_blob(blk)));
    else
      events.push_back(blk);
  };

  // the defective tx is submitted on its own event and must be refused
  std::vector<crypto::hash> tx_hashes;
  // the other txs are all valid (used by the good replacement block)
  std::vector<transaction> good_txes(txes);
  good_txes.erase(good_txes.begin() + m_bad_idx);
  std::vector<crypto::hash> good_hashes;
  for (const auto &tx : good_txes)
    good_hashes.push_back(get_transaction_hash(tx));

  block blk_bad;
  if (!m_bitflip)
  {
    transaction &bad_tx = txes[m_bad_idx];
    CHECK_AND_ASSERT_MES(apply_defect(bad_tx, m_defect), false, "defect application failed");
    bad_tx.invalidate_hashes();
    m_bad_tx_hash = get_transaction_hash(bad_tx);
    // block hash lists reference the defected tx (mirrors the control tests, which
    // hash after post_tx); the good block swaps in a valid replacement below
    for (const auto &tx : txes)
      tx_hashes.push_back(get_transaction_hash(tx));
    DO_CALLBACK(events, "mark_invalid_tx");
    events.push_back(bad_tx);
    events.push_back(good_txes);

    // the bad block (h79, hf16)
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_bad, blk_last, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
        test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
        TEST_HF, TEST_HF, blk_last.timestamp + ts_step,
        crypto::hash(), 0, transaction(), tx_hashes, 0, 6, TEST_HF, fees),
        false, "Failed to generate the bad block");
    DO_CALLBACK(events, "mark_invalid_block");
    push_tested_block(events, blk_bad);
  }
  else
  {
    // bitflip mode: the tested block is the fully VALID block with one bit flipped
    // inside tx_hashes[m_flip_tx] — a merkle-tree leaf of the block. The blob still
    // parses (fixed-size hash entries); the refusal must come from verification.
    for (const auto &tx : txes)
      tx_hashes.push_back(get_transaction_hash(tx));
    CHECK_AND_ASSERT_MES(m_flip_tx < tx_hashes.size(), false, "flip tx index out of range");
    DO_CALLBACK(events, "mark_invalid_block");
    CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_bad, blk_last, miner_account,
        test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
        test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
        TEST_HF, TEST_HF, blk_last.timestamp + ts_step,
        crypto::hash(), 0, transaction(), tx_hashes, 0, 6, TEST_HF, fees),
        false, "Failed to generate the bitflip block");
    blobdata blob = t_serializable_object_to_blob(blk_bad);
    CHECK_AND_ASSERT_MES(apply_bitflip(blob, tx_hashes[m_flip_tx], m_flip_byte), false,
        "failed to flip a bit in the block blob");
    block reparsed;
    CHECK_AND_ASSERT_MES(parse_and_validate_block_from_blob(blob, reparsed),
        false, "corrupted block blob must still parse");
    CHECK_AND_ASSERT_MES(reparsed.tx_hashes[m_flip_tx] != tx_hashes[m_flip_tx],
        false, "flip did not land in the tx hash entry");
    LOG_PRINT_L0("bitflip: corrupted tx_hashes[" << m_flip_tx << "] byte " << m_flip_byte
        << " in a " << blob.size() << "-byte block blob; blob still parses -> the refusal "
        << "below is a block VERIFICATION failure (unknown tx / merkle mismatch), not a parse failure");
    events.push_back(serialized_block(blob));
  }

  // bitflip mode: the chain stays at the spacer tip (the refused block is the
  // last event before the final callback)
  if (m_bitflip)
  {
    DO_CALLBACK(events, "check_final");
    return true;
  }

  // the good block: same txs, the defective one replaced by a fresh valid one that
  // spends the very output the rejected tx claimed (it was never spent: the bad tx
  // was refused at pool submission)
  transaction good_tx;
  CHECK_AND_ASSERT_MES(build_spend(miner_account.get_keys(), pool, spend_amount, pool[m_bad_idx],
        get_tx_pub_key_from_extra(blocks[pool[m_bad_idx].block_idx].miner_tx), good_tx) != 0,
      false, "failed to construct the replacement transaction");
  events.push_back(good_tx);
  std::vector<crypto::hash> ok_hashes = good_hashes;
  ok_hashes.insert(ok_hashes.begin() + m_bad_idx, get_transaction_hash(good_tx));

  block blk_good;
  CHECK_AND_ASSERT_MES(generator.construct_block_manually(blk_good, blk_last, miner_account,
      test_generator::bf_major_ver | test_generator::bf_minor_ver | test_generator::bf_timestamp |
      test_generator::bf_tx_hashes | test_generator::bf_hf_version | test_generator::bf_max_outs | test_generator::bf_tx_fees,
      TEST_HF, TEST_HF, blk_last.timestamp + ts_step,
      crypto::hash(), 0, transaction(), ok_hashes, 0, 6, TEST_HF, fees),
      false, "Failed to generate the good block");
  push_tested_block(events, blk_good);

  DO_CALLBACK(events, "check_final");
  return true;
}

bool gen_bad_tx_in_block::check_final(cryptonote::core &c, size_t ev_index, const std::vector<test_event_entry> &)
{
  DEFINE_TESTS_ERROR_CONTEXT("gen_bad_tx_in_block::check_final");
  const size_t expected_height = 1 /*genesis*/ + N_MINER_BLOCKS + CRYPTONOTE_MINED_MONEY_UNLOCK_WINDOW
      + N_SPACERS + (m_bitflip ? 0 : 1 /*good block*/);
  CHECK_EQ(expected_height, c.get_current_blockchain_height());
  // the bad transaction must never have been written to the blockchain
  CHECK_TEST_CONDITION(!c.get_blockchain_storage().have_tx(m_bad_tx_hash));
  size_t n_pool = 0;
  c.get_pool_transactions_count(n_pool);
  MDEBUG("final pool count: " << n_pool);
  return true;
}
