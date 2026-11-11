# monerodato

monerodato.7 is a fork of [seraphis-migration/monero](https://github.com/seraphis-migration/monero) v0.19.0.0-beta.3.1 (`d80596ad1811`) for the FCMP++ stressnet. It changes four things:
- **monerod on machines with little memory and few cores:** cheaper FCMP++ verification, and memory returned to the OS.
- **Startup on a spinning disk:** the node reads its hash indexes into memory in the background as it starts.
- **The wallet:** faster FCMP++ proving with less memory.
- **Safety:** every program starts only on testnet.

Every change is compiled in and always on. There are no runtime switches. On capable hardware, run the reference monerod. If anything goes wrong, the fallback is the stock binary on the same database.

The fork is laid out so that each change can go to its real upstream as a small, separate series. Everything else is generated or fork plumbing, and is marked `[fork-only]` in the commit subject.

**Labels:** G0xx, W0xx and E0xx in this file, [`MONERODATO-GATE.md`](MONERODATO-GATE.md) and [`MONERODATO-CONSTANT-TIME.md`](MONERODATO-CONSTANT-TIME.md) are the lab's experiment numbers. They name where a measurement comes from.

**Earlier builds:** monerodato.0 to .6 were working builds that were not published; .7 contains all of their changes. Where a measurement names one of them, it was taken on that build.

## Testnet only
monerodato is an unofficial build of a Monero beta, tested only on the FCMP++ stressnet. The beta's mainnet and stagenet hard-fork tables carry mock heights, and its database code migrates older databases. Pointed at a mainnet data dir, it would very likely convert the database and then fail to follow the chain; that is from reading the code, not tested.

So every program that opens a chain database or a wallet refuses to start on any network but testnet:

| program | mainnet (default) | `--testnet` | `testnet=1` in its config file | `--stagenet` (or in config) | `--regtest` (or in config) |
|---|---|---|---|---|---|
| monerod | refused | starts | starts | refused | refused |
| monero-wallet-cli, monero-wallet-rpc, monero-gen-trusted-multisig | refused | starts | starts | refused | no such flag |
| monero-blockchain-ancestry, -depth, -export, -import, -prune, -prune-known-spent-data, -stats, -usage | refused | starts | no config file | refused | refused |
| monero-gen-ssl-cert, monero-utils-deserialize, -dns-checks, -object-sizes | no check | | | | |

- **The check uses the effective network type,** after the command line and the config file are both parsed. So `testnet=1` in a config file counts as `--testnet`.
- **A refusal** is one message on stderr and exit status 1. It comes before the data dir, the log file, the database or any wallet file is created or opened. `--help` and `--version` work everywhere.
- **The four programs without a check** open no chain database, no wallet and no data dir:
  - gen-ssl-cert writes only the key and certificate it is told to;
  - deserialize parses the hex it is given;
  - dns-checks makes DNS queries and prints them;
  - object-sizes prints type sizes.
- **No flag, environment variable or config option turns the check off.** It is set in one place, `src/common/monerodato_guards.h`.
- **It is a safeguard, not a security boundary:** it stops a start on mainnet by accident, but anyone who builds from source can change it.
- **A test build** sets `TEST_BUILD` there to true. It also allows regtest, and wallets with the mainnet network type, which the Python functional tests use against regtest daemons. Never distribute such a build. unit_tests and core_tests need no test build: they make their cores and wallets inside the test process, never through the programs' `main`.
- **Not guarded:** libwallet_api. No program in this release uses it.

## The series

| upstream destination | commits | what |
|---|---|---|
| monero (`src/`, `external/db_drivers/liblmdb`, `tests/unit_tests`) | 11 | FCMP++ batch verification capped at physical cores for calls with proofs of 8+ inputs; `malloc_trim` after batch verification and once at idle; block sync requests kept under the levin packet limit, in both branches of `core::get_block_sync_size`; the batch tasks waited for before an early exit; a two-step batch verification FFI; one multiexp per curve per batch verification call; the hash indexes read into the page cache in the background at startup (an LMDB function with a stop function and a 64-page read-ahead window, its unit tests, `BlockchainDB::warm_indexes` in a thread of its own, the call at the end of `core::init`, and tests of the thread against `close()` and a resize) |
| dalek-ff-group (`src/fcmp_pp/fcmp_pp_rust/patched/dalek-ff-group`) | import + 2 | Field25519 as four saturated 64-bit limbs; `wide_reduce` / `random` through the same fold |
| multiexp (`src/fcmp_pp/fcmp_pp_rust/patched/multiexp`; upstream serai-dex/serai `crypto/multiexp`) | import + 1 | the constant-time `multiexp` uses Straus with a 4-bit window at every length, with a test |
| monero-oxide (`src/fcmp_pp/fcmp_pp_rust/patched/monero-oxide`) | import + 9 | FCMP++ verification with the constraints folded as they are produced (fcmps, generalized-bulletproofs, circuit-abstraction), with tests; merging batch verifiers and a multiexp split over threads (generalized-bulletproofs), with tests; a test of `verify_split` at its chunk boundaries; less memory in the arithmetic circuit prover (three commits), with a test of its output byte for byte |
| none (`[fork-only]`) | 19 | the Cargo `[patch]` lines, `scripts/`, the generated `vendor/`, the build hook for the revision check; the startup notice; the second reviewer's gate tests and the pristine crate they compare against; this file, [`MONERODATO-GATE.md`](MONERODATO-GATE.md), [`MONERODATO-CONSTANT-TIME.md`](MONERODATO-CONSTANT-TIME.md) and the README header; the testnet-only check (three commits); the `-monerodato.7` version suffix |

Each import commit adds the untouched upstream tree, so the commits after it are exactly the changes. The commit messages carry the reasons, the measurements and, for the Rust crypto commits, notes on bounds and constant-time behaviour. Where the two dalek-ff-group commit messages say "branch-free", that was inspected with overflow checks off; for the shipped build read the refined claim under [Constant time](#constant-time-under-the-shipped-flags) below.

To list or extract one series:

```sh
git log --oneline d80596ad1811..HEAD -- src/ringct src/cryptonote_core src/blockchain_db external/db_drivers/liblmdb   # monero
git log --oneline -- src/fcmp_pp/fcmp_pp_rust/patched/dalek-ff-group       # dalek-ff-group
git log --oneline -- src/fcmp_pp/fcmp_pp_rust/patched/monero-oxide         # monero-oxide
git log --oneline -- src/fcmp_pp/fcmp_pp_rust/patched/multiexp             # multiexp

# a crate series as patches against its own upstream (skip the import and the fork-only test commits):
P=src/fcmp_pp/fcmp_pp_rust/patched/monero-oxide
I=$(git log --format=%h --diff-filter=A -- $P/Cargo.toml)    # the import commit
git format-patch --relative=$P --invert-grep --grep='^\[fork-only\]' $I..HEAD -- $P
```

Review diffs:
- for monero: `git diff d80596ad1811 HEAD -- src/ringct src/cryptonote_core src/blockchain_db src/fcmp_pp/prove.* src/fcmp_pp/fcmp_pp_rust/src src/fcmp_pp/fcmp_pp_rust/fcmp++.h external/db_drivers/liblmdb tests/unit_tests/fcmp_pp.cpp tests/unit_tests/lmdb_warm.cpp tests/unit_tests/CMakeLists.txt`. The `[fork-only]` FFI gate test (`g009_ffi_tests.rs` and its lines in `lib.rs`) falls under that path list too.
- for each crate: `git diff <import> HEAD -- <patched tree>`. The dalek-ff-group and monero-oxide trees also hold the `[fork-only]` gate tests.

## FCMP++ verification in monerod
- **Batch verification is capped at the physical cores** for calls with proofs of 8 or more inputs, and the verifier's freed memory goes back to the OS (`malloc_trim` after batch verification and once at idle). Block sync requests are kept under the levin packet limit.
- **The membership proof's constraints are folded as they are produced** (fcmps, generalized-bulletproofs, circuit-abstraction), instead of being stored and evaluated afterwards.
- **dalek-ff-group's Field25519 is four saturated 64-bit limbs,** with `wide_reduce` and `random` through the same fold. It is the field of the Helios coordinates and Selene scalars.
- **One multiexp per curve per verification call.**
  - A node verifies a block's FCMP++ proofs as one batch per pool thread. If each batch ended in its own multiexp per curve, every one of those multiexps would carry the full generator vectors, so with 12 pool threads the generator terms the batches share would be computed 12 times per block.
  - Instead the batches' verifier states are merged, and one multiexp per curve runs for the whole call, split over the pool threads. Four series commits do it:
    - **generalized-bulletproofs** (monero-oxide series): `BatchVerifier::merge` and `Generators::verify_split`, with crate tests; `verify` is unchanged.
    - **rctSigs:** the batch tasks are waited for before an early exit. This fixes a lifetime hazard that upstream's code already has (tasks writing `results` after an early return), so it is its own commit.
    - **fcmp_pp:** the two-step FFI, `fcmp_pp_verify_prepare` per batch and `fcmp_pp_verify_finish` once per call; `fcmp_pp_verify` is unchanged.
    - **rctSigs:** `batchVerifyFcmpPpProofs` uses them.

**Why the merged result is the same.** Every batch uses the same generators and every proof its own random weight, so the merged multiexp is the sum of the per-batch ones. The call returns the same verdict, except with negligible probability, as for any batch verification.

**Measured** (G009 and G009c), the merge against the same build without it. Static builds on a 6-core Ryzen 5 5600G:
- **12 threads:** small-transaction blocks CPU -36.1%, wall -5.6%; heavy (128-input) blocks CPU -6.3%, wall -7.2%.
- **4 threads:** small-transaction blocks CPU -9.2%, wall 0.0%; heavy blocks CPU -6.8%, wall -5.5%.
- **Peak memory:** within +3.6%.

The gain grows with the thread count: a call's proofs form one batch per pool thread, and the merge removes the repeated generator terms of every batch but one. The comment-stripped code is identical to the code measured there, except that the split is always on (the measured arm, M2).

**Checks and review:**
- the unit test `batch_verify_rejects_one_bad_proof_among_valid`: proofs that only the merged multiexp can reject, placed first, middle and last among valid proofs;
- `batch_verify_early_exit_waits_for_tasks`;
- the crate tests `merged_batch_verifiers` and `merged_batches_reject_malleated_proofs`;
- a negative differential through the batch path, in test builds;
- the second reviewer's review: three fuzz passes, an FFI-level test, and the early-exit guard.

**The startup notice.** On a machine with at least 12 GB of RAM, more than four physical cores and AVX2, monerod prints one notice pointing to the reference monerod, which is reviewed and maintained upstream. It makes no claim that the reference monerod would sync faster. In G014 (stock beta.3.1 against monerodato.1 on a 12-core, 16 GB machine), monerodato.1 was faster in three cases of four; stock was faster by 4% in one, small-transaction blocks at 12 threads.

## The hash indexes, read into the page cache at startup
**What it does.**
- `mdb_env_warm_dbis()` is a new read-only function in the vendored LMDB (`external/db_drivers/liblmdb`).
  - It reads the B-trees of the given databases one level at a time across all of them, sub-databases included.
  - Each level's pages are sorted by page number and requested from the OS before they are read. Requests run at most 64 pages ahead of the page being read.
  - The leaves come last, and only if all of them fit in a byte budget.
  - A stop function, asked before each request and each page, ends it early; a stop is not an error.
- `BlockchainDB::warm_indexes()` does nothing by default. BlockchainLMDB starts a thread that reads spent_keys, tx_indices and block_heights with it.
  - The leaves are read only if they fit in 60% of the memory the process may use: the physical memory, lowered by a cgroup limit.
  - The thread holds a read transaction, and a map resize would wait for it while holding up every new transaction. So `close()` and a map resize stop the thread first, and wait for it.
  - A resize ends the warm-up for good.
- `core::init` starts it as its last step, after its own reads. P2P and RPC do not wait for it.
- One log line when it starts, and one at the end: what was read and how long it took, why the leaves were skipped, or what it had read when it was stopped.

**Why.** Every block's sync looks up key images, transaction hashes and block hashes in these three tables, at random places. On a spinning disk each miss is a seek. G015 measured it on a test VM (4 vCPUs, 4 GB RAM) on a 5,400 rpm disk, with the stressnet chain:
- with these three tables in the cache, a 400-block sync took 83 s instead of 360 s cold (stage 2);
- read level by level in file order, the tables (0.83 GB) took 61 s; read in key order, as a cursor walk reads them, 2,054 s (stage 4a);
- built into monerod, run before P2P and RPC started (monerodato.5): from launch to the top of the same 400 blocks, 166 s instead of 410 s, of which the warm-up took 60 s, as fast as the same function run outside monerod (stage 4b, 3 runs each; these times include the startup, stage 2's do not). Over 3,000 blocks the node kept 5 blocks/s and read nothing from the disk over the last 1,000.

**Why in the background.** Run before P2P and RPC, as in monerodato.5, the warm-up delayed every start: by about 3 s on a SATA SSD, and by about 60 s on the 5,400 rpm disk. A node restarted at the tip gains nothing from it, and was unreachable meanwhile. A Core 2 Quad Q6600 node on an SSD answered RPC after 28 s instead of 1 s.

**Two designs that did not work (both measured):**
- **Starting the thread right after the database opens.** The rest of `core::init` reads other tables, and those reads queued behind the warm-up's, so RPC came no earlier: 57 s at 15 MB/s, the same as the synchronous warm-up.
- **Requesting a whole level at once.** On the 5,400 rpm disk (G015 stage 4c):
  - the disk's queue filled, at about 250 requests;
  - after a cold start, monerod's own reads (its code, its P2P state) waited behind them until the warm-up ended;
  - RPC answered after 87 s, against 80 s with the synchronous warm-up.

  The 64-page window fixes this.

**Measured:**
- **On the 5,400 rpm disk** (G015 stage 4d). The page cache was dropped before each start; 400 blocks, 3 runs each, medians [range]:

| | RPC answers after launch | top of the 400 blocks after launch | warm-up |
|---|---|---|---|
| synchronous (monerodato.5) | 84.4 s [79.2-84.4] | 168 s [163-174] | 61.7 s, before RPC |
| background | 30.1 s [24.7-31.2] | 179 s [178-188] | 75.8 s [72.0-80.0], after RPC |

- **The gain there:** RPC answers 54 s sooner. The 30 s left are the cold start's own reads.
- **The cost there:**
  - The warm-up takes 23% longer: it shares the disk with the sync, and keeps at most 64 requests outstanding.
  - A catching-up node reaches the top 6% later. Each of the three background runs finished after each synchronous run.
- **On a 12-core Ryzen 9 3900X with 16 GB RAM and a SATA SSD** (the stressnet DB out of the page cache, the node offline). The 15 MB/s rows use a cgroup read limit as a stand-in for a slow disk, with the binary out of the cache too:

| | RPC answers after launch | warm-up |
|---|---|---|
| synchronous, SSD | 4.7-5.0 s | 3.3 s, before RPC |
| background, SSD | 1.5-1.7 s | 3.0 s, after RPC |
| synchronous, 15 MB/s | 58.3 s | 55.2 s, before RPC |
| background, 15 MB/s | 3.5 s | 55.5 s, after RPC |

- **Stopping:**
  - `stop_daemon` or SIGINT during the warm-up: the node exits about 1 s later.
  - A resize or `close()` during the warm-up returns in 5-10 ms.
- **The read transaction** keeps LMDB from reusing pages that the sync frees meanwhile. After a 400-block catch-up on the 5,400 rpm disk, the file had grown by 108 pages, from that and the sync together.

**What it costs:**
- While the warm-up reads, other reads of the same disk wait behind at most 64 of its requests.
- The page cache holds the tables, which a syncing node reads most anyway. It is reclaimable.
- A failure is logged as a warning, and the node goes on without the warm-up.
- 32-bit builds (MDB_VL32) skip it. Windows reads the pages in order without asking the OS for them first.
- **A fresh sync resizes the map often, and each resize ends the warm-up.** Such a node in effect never warms. That is harmless: a fresh database has nothing worth warming.
- **Not handled:** if another process grows the same database's map while the warm-up runs, LMDB's resize handler waits for the warm-up to end.

**Checked:**
- **Unit tests** on a generated three-table environment:
  - the same resident pages as a walk through every item with the public cursor API;
  - a budget one byte short of the leaves reads only the upper levels; exactly the leaves reads all;
  - argument checks;
  - the stop function;
  - the 64-page window, whose test fails on the code without the window;
  - the thread against `close()` and a resize, repeated 20-50 times. One of these is opt-in: it runs on a real database with slowed reads.
- **Bugs:** three single-line bugs in the function, and three in the background changes, are each caught by these tests. One of them crashes the race test.
- **On the stressnet DB (SSD):**
  - exactly the cursor walk's 202,506 resident pages, in 3.2 s against the walk's 28.5 s;
  - with a budget of 0, only the 3,864 upper pages.
- **monerod:** under a 1 GiB cgroup limit it reads only the upper levels, and under 2 GiB all of it.
- **The second reviewer** reviewed both the synchronous and the background versions.

## The wallet: faster FCMP++ proving
**Scope.** These changes were made for the wallet. The ones above were made for monerod, though the wallet runs some of that code too: dalek-ff-group's field is also Selene's, which the wallet's prover uses. The changes in this section are in code that only the wallet runs. They change how the wallet builds an FCMP++ transaction, and nothing in the daemon.

**What changed.** The `multiexp` crate's constant-time `multiexp` now uses Straus's algorithm with a 4-bit window for every input of two or more points. Upstream takes its algorithm and window from the same length table as `multiexp_vartime`: Straus below 50 points, then Pippenger with windows from 4 to 8. Those choices suit the variable-time function, but not the constant-time one:
- To hide which bucket each digit selects, constant-time Pippenger reads and writes every bucket for every point and digit. So larger windows cost more.
- On Selene and Helios points, constant-time Straus with a 4-bit window was as fast as or faster than every other choice measured, from 4 to 2,048 points. Upstream's choice was up to 3.2 times slower.

[`MONERODATO-CONSTANT-TIME.md`](MONERODATO-CONSTANT-TIME.md) explains constant time and the measurements. The commit message has the full table.

**Where it runs.** In fcmp_pp_rust, the constant-time `multiexp` is used only by the FCMP++ membership prover, on Selene and Helios points:
- generalized-bulletproofs `ArithmeticCircuitStatement::prove`;
- full-chain-membership-proofs `Fcmp::prove` and its vector-commitment tape.

**monerod does not reach it.** In the release monerod:
- the provers are its only callers;
- they are reached only from `fcmp_pp::prove_membership`;
- nothing in monerod calls that function or loads its address. Its one caller is `src/wallet/tx_builder.cpp`.

Verification, the curve-tree hashing and the sync all use `multiexp_vartime`, which is unchanged.

**Why the proofs are the same.** A multiexponentiation has one correct result whichever algorithm computes it, and the change draws no randomness. So, for the same random inputs, the prover makes the same proof, byte for byte. Checked:
- the new crate test, `src/tests/constant_time_straus.rs`: the new `multiexp` against the 0.5.2 dispatch, `multiexp_vartime` and the naive sum, at lengths on either side of every old threshold;
- a fixed-seed differential of 258 proofs, 1 to 128 inputs: every proof byte-identical to the one made with multiexp 0.5.2, and accepted by the 0.5.2 build's verifier;
- the generalized-bulletproofs, full-chain-membership-proofs and monero-fcmp-plus-plus crate tests;
- the fork's `fcmp_pp`, `wallet_tx_builder`, `carrot_fcmp`, `curve_trees` and `bulletproofs_plus` unit tests;
- transactions built by the wallet: weights and fees identical to those of the build without the change.

**Constant time.** No constant-time code is new. The change calls the crate's existing constant-time `straus` at more lengths, and the choice still depends only on the number of points, which is public. `straus` and `pippenger` are unchanged. Adding the identity for a zero digit costs the same as any other addition, because the Selene and Helios formulas are complete and straight-line (see [`MONERODATO-CONSTANT-TIME.md`](MONERODATO-CONSTANT-TIME.md)). A dudect-style test by the second reviewer found no timing dependence: Selene and Helios points at 4, 64 and 256 pairs; random scalars against all-zero, all-one and fixed random ones; 3,000 interleaved samples per class; Welch's t on the means and on the upper tails; |t| at most 2.31, against a threshold of 5. Under the same setup the variable-time `multiexp_vartime` shows t of 389 and more.

**Measured** (monero-wallet-rpc `sweep_all`, not relayed, from a copy of a stressnet wallet; 3 rounds alternating the two builds; median wall time per transaction):

| machine | inputs | without the change | with it | change |
|---|---|---|---|---|
| 12-core Ryzen 9 3900X; native builds, offline daemon | 8 | 6.35 s | 3.11 s | -51% |
| | 32 | 24.68 s | 12.08 s | -51% |
| | 128 | 100.83 s | 52.09 s | -48% |
| 4-core Core 2 Quad Q6600; static generic builds, local daemon | 8 | 37.14 s | 19.20 s | -48% |
| | 32 | 183.54 s | 79.51 s | -57% |

- On the Q6600 the times without the change varied from run to run: 35.04-46.77 s at 8 inputs, and 140.97-183.89 s at 32. Every run with the change was faster than every run without it.
- Fees and weights were identical between the builds on both machines.
- Peak memory is unchanged: 1.31 GB for the wallet-rpc process with either build, on the 3900X.

**Build check.** `scripts/check-vendor-rev.sh` fails the build unless every multiexp in `Cargo.lock` is the patched path package at its base version, with no vendored copy. A second, registry multiexp, which a monero-oxide update could pull in, would otherwise build without the change.

## The wallet: less memory for FCMP++ proofs
**What changed.** Three commits in generalized-bulletproofs' `ArithmeticCircuitStatement::prove`, the prover for the Selene and Helios circuits of each FCMP++ membership proof:
- the `l` and `r` coefficient vectors are wiped and freed right after they are evaluated, not at the end of the function;
- each vector commitment's weight vector is shrunk to the length it uses;
- each vector commitment's values are moved into `l`, not copied.

These change memory only: no arithmetic, no transcript order and no random draw changes. Like the multiexp change above, this runs only in the wallet; monerod never calls the prover.

**Measured** (wallet-rpc `sweep_all`, not relayed, from a copy of a stressnet wallet against an offline daemon, on a 12-core Ryzen 9 3900X with 16 GB RAM, 3 rotated rounds, median peak memory (VmHWM)):

| inputs | without the change | with it |
|---|---|---|
| 8 | 169.1 MB | 169.1 MB |
| 32 | 270.3 MB | 250.5 MB (-7.3%) |
| 128 | 1,287.8 MB | 815.5 MB (-36.7%) |

- **Where the memory was.** At 128 inputs about 270 vector commitments were each held three times at the peak: in the witness, in a full-length copy in `l`, and in a full-length weight vector, at about 1 MB per copy.
- **Time per transaction** is unchanged within the spread: 52.1 s and 51.5 s at 128 inputs. That is indicative only.

**Why the proofs are the same:**
- `prove_output_unchanged`, a test in generalized-bulletproofs: 8 fixed-seed proofs, compared byte for byte with the bytes the prover made without this change. A mutation that changes the proofs but keeps them valid fails it.
- The W009 proof differential: 258 cases, 1 to 128 inputs, 1 to 8 layers. 258 of 258 proofs are byte-identical to those made without the change, and this change's verifier accepted all 258 of those. The other direction was not run separately: the bytes are the same, and in an earlier review the unmodified verifier accepted those proofs, 258 of 258.
- The crate suites.

**Hygiene.** Stock never wiped `l` and `r`, which hold witness values. Its copy of the commitment values in `l` was also left in freed memory. Now none of them is left unwiped.

**Constant time.** No branch on secret data is added. The lengths involved (the row count, and the highest index a constraint uses) are public, and the zeroize is a pass over memory of public length.

## Rust dependencies: vendored, offline
- `src/fcmp_pp/fcmp_pp_rust/vendor/`: every package `Cargo.lock` pins (crates.io and git), unmodified, written by `scripts/vendor.sh` (`cargo vendor --locked --versioned-dirs`, cargo 1.99.1). Reviewers don't read it; they regenerate it in a clean checkout and check that `git status --porcelain --ignored src/fcmp_pp/fcmp_pp_rust` prints nothing.
- `src/fcmp_pp/fcmp_pp_rust/.cargo/config.toml`, generated by the same script, replaces every source with `vendor/` and sets `[net] offline = true`, so cargo builds without the network. With the existing `--locked`, every build uses exactly the committed sources.
- `src/fcmp_pp/fcmp_pp_rust/patched/` holds the three changed crates. Nothing in `vendor/` is edited, so cargo's checksums stay valid. `patched/bases.txt` records each tree's upstream base.
- `scripts/check-vendor-rev.sh` runs before every cargo build, from `src/fcmp_pp/fcmp_pp_rust/CMakeLists.txt`. It fails the build unless:
  - `Cargo.lock` pins monero-oxide at `patched/monero-oxide`'s base;
  - every vendored monero-oxide crate is file-for-file identical to its copy in `patched/monero-oxide`;
  - dalek-ff-group resolves to `patched/` at the base version;
  - every multiexp in `Cargo.lock` resolves to `patched/` at the base version, and `vendor/` holds no multiexp.

  It catches an update that moves one side only.

**Two dalek-ff-group versions appear in the graph.** 0.4.6 is a dependency of `ciphersuite` 0.4.2 only (`cargo tree -i dalek-ff-group@0.4.6` lists no other dependent). The Field25519 that FCMP++ verification spends its time in is 0.5.0, the patched one: it is the field of the Helios coordinates and Selene scalars in helioselene, and ec-divisors, monero-fcmp-plus-plus, monero-primitives and fcmp_pp_rust use it too. Profiles of the patched build no longer show crypto-bigint's `mul_wide<4>` and `montgomery_reduction<4>` (12% and 21% of CPU without the field change), so those were all Field25519's.

**Not changed, on purpose:** the verifier code that stores constraints (`Fcmp::verify_stored`, `Circuit::verify`, `ConstraintHandling::Store`) is already `#[cfg(test)]` in the monero-oxide series. It is not in the shipped code and is kept so the tests can compare the folded verifier against it.

## Build
Use the [Building](https://github.com/Fountain5405/monerodato/blob/main/README.md#building) section of `main`'s README. It is tested on clean machines for each release and covers:
- the Ubuntu 24.04 packages, including `pkg-config`;
- the Rust toolchain (rustc and cargo 1.99.1 through rustup);
- memory: with 4 GB, build with `-j2`;
- glibc 2.38 or newer to run the binary on another machine.

Two things hold for every release:
- **Network:** the `contrib/depends` step downloads the C and C++ libraries' sources. The Rust code builds offline from the vendored crates (see above).
- **Reproducibility:** the depends libraries are built with the host compiler, so a different gcc gives a different binary. The build is repeatable, not bit-for-bit reproducible; `contrib/guix` would be the route to that.

## Gate tests (from the second reviewer)
These are `[fork-only]` test commits; none of them is in the shipped code. When they were added, the Rust static library built with them had the same instructions as without them: 371 of its 372 members were byte-identical, and the last differed only in two embedded paths under the build directory (the generator tables monero-fcmp-plus-plus generates at build time) and the LLVM symbol hashes derived from them. [`MONERODATO-GATE.md`](MONERODATO-GATE.md) has the invocations and bars, and its ORACLE TRAP warning.

- **s64 field differential:** `patched/dalek-ff-group/tests/s64_differential.rs` compares the s64 `FieldElement` against `patched/dalek-ff-group-orig`, which is crates.io dalek-ff-group 0.5.0 with only its package and lib names changed (`src/` byte-identical; checked against the `.crate`).
  - The oracle is a **path** dev-dependency on purpose: a registry dependency would be redirected to the patched crate by the `[patch.crates-io]` line in `fcmp_pp_rust/Cargo.toml`, and the differential would compare s64 with itself.
  - `oracle_is_the_pristine_path_crate`, in the same file, fails if that ever happens: the two `FieldElement` types must differ, and the oracle's must come from `dalek_ff_group_orig`.
  - The "anchored chains" part runs 50 real chains of 200 operations: values carry from step to step and are re-anchored every 20th step.
- **Folded-vs-stored verifier differential:** `patched/monero-oxide/crypto/fcmps/src/tests/differential_fuzz.rs`. Structured mutants are judged by `Fcmp::verify` and the test-only `Fcmp::verify_stored`, with batch-verifier state and verdict comparison.
  - It runs under monero-oxide's pinned rustc 1.97.1 (`rust-toolchain.toml`) unless run with `cargo +stable`; the shipped binary is built with rustc 1.99.1.
  - For the first release it passed under both: `TOTAL: generated 900 unparsable 27 both_reject 873 both_accept 0 diverged 0 panics 0`. Under 1.99.1 the whole fcmps suite passed with it, 10 tests.
- **G009 merge and split (gates 4 and 5):** `patched/monero-oxide/crypto/fcmps/src/tests/g009_fuzz.rs` compares merged batch verification, at splits 1 to 8, with per-batch verification.
  - It covers malleated multi-batch mixes in both merge orders, corruption of each verifier field, padding with an empty verifier, and a merged state of about 12,600 terms.
  - `src/fcmp_pp/fcmp_pp_rust/src/g009_ffi_tests.rs` drives the two-step FFI on the tests/data vectors.
  - One hunk of the fuzz is by the fork's author and was adopted by the reviewer: its field-4 case draws its h_sum index in range, as Notes (c) in MONERODATO-GATE.md requires.
- **Constant-time structure:** `scripts/ct-check.sh` counts branches in a binary built with the second reviewer's force-emitted `ct_inspect` wrappers. As delivered it does not run against monerod: the wrapper source is not in this repo, and the script's GOT slot offsets are those of the reviewer's binary. The shipped library was inspected directly instead; see below.
- **How to run them here:**
  - cargo in `src/fcmp_pp/fcmp_pp_rust` is offline and vendored for fcmp_pp_rust's own dependencies only, and the crates' dev-dependencies are not vendored.
  - So run the gates on copies of `patched/dalek-ff-group`, `patched/dalek-ff-group-orig` and `patched/monero-oxide`, kept side by side outside the tree.
  - For the monero-oxide copy, add `[patch.crates-io] dalek-ff-group = { path = "../dalek-ff-group" }` to its workspace `Cargo.toml`, so it uses the s64 field as the fork does.
- **dalek-ff-group-orig** is a third copy of dalek-ff-group in the tree. It is test scaffolding only: cargo does not resolve the dev-dependencies of fcmp_pp_rust's path dependencies, so it is in neither `Cargo.lock` nor `vendor/`.

## Constant time under the shipped flags
**The claim:** in the s64 field, secret-dependent branches exist only as never-taken overflow guards on provably in-range arithmetic (overflow-checks on). Re-inspect after any toolchain or flag change.

- **Flags.** They come from `[profile.release]` in `fcmp_pp_rust/Cargo.toml`, with no RUSTFLAGS and no `CARGO_OPTIONS`. As `cargo build -v` shows them for dalek_ff_group: `-C opt-level=3 -C codegen-units=1 -C panic=abort -C overflow-checks=on -C debug-assertions=off -C strip=debuginfo -C linker-plugin-lto -C embed-bitcode=no --target x86_64-unknown-linux-gnu`, with no `target-cpu` (baseline x86-64), rustc 1.99.1.
- **Where the machine code is.** The dependencies are compiled to LLVM bitcode, and machine code is produced once, by the thin LTO (`-C lto=thin`) of the `fcmp_pp_rust` staticlib. So the code to inspect is in `libfcmp_pp_rust.a`, or in monerod, not in the dependency rlibs.
- **The second reviewer's re-inspection** under this profile found one conditional branch in each field operation: the overflow panic guard on the final limb arithmetic of `fold_up`. Its bounds proof shows that guard is never taken.
- **The first release's library**, `libfcmp_pp_rust.a` (sha256 `131ae878f047db43...`), was also disassembled with `objdump -d -r`. Every out-of-line `FieldElement` body was counted, and each conditional jump classed by what its target does:
  - **One conditional jump each, an overflow guard (a call to `core::panicking::panic_const_add_overflow` or `panic_const_sub_overflow`):** add_assign, sub, sub_assign, neg, mul, mul_assign, square, wide_reduce, from_uniform_bytes, ct_eq. The mul and mul_assign copies in other crates' objects have the same shape.
  - **No conditional jumps:** to_repr, from_repr, conditional_select, is_odd, to_le_bits, zeroize, `From<u64>`.
  - **Composite operations:**
    - pow22501: 18 guards plus 7 loop back-edges, with constant counters 5, 10, 20, 10, 50, 100, 50, its addition chain;
    - invert: pow22501, then 2 guards and one loop with constant counter 5;
    - sqrt: pow22501, then 6 guards and calls to ct_eq.
  - **Not on secret data:** `Debug::fmt`, `random` and `from_elem` have other branches, on formatting, on the error returns of getrandom and malloc, and on allocation.
  - `Add::add` and `double` have no out-of-line body: they are inlined at every use.
  - To repeat it: `objdump -d -r -C -w --no-show-raw-insn libfcmp_pp_rust.a | python3 scripts/ct-scan.py`.

## Open
- **The warm-up's budget has no absolute cap.** It is 60% of the memory the process may use, total rather than free. On mainnet the three tables are probably several GB (not measured). The warm-up runs in the background, so P2P and RPC do not wait for it, but whether to cap it is open before any mainnet use. monerodato itself runs on testnet only.
- **The build depends on its directory.** The two embedded paths above make the staticlib differ between build directories. Bit-identical builds (guix) would need `--remap-path-prefix`.
