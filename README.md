<p align="center">
  <img src="docs/monerodato.jpg" width="100%" alt="A white robot in magnifying goggles scratches its head while soldering wires, RAM sticks and a fan into a potato labelled SPUD-CPU, which has googly eyes and a small Monero screen. Signs around the lab read: Do not use potato-based Monero in critical situations; Not suitable for critical deployments; Caution: agentic LLM generated optimizations active.">
</p>

# WARNING: LLM GENERATED OPTIMIZATIONS

## OPTIMIZATIONS NOT VERIFIED CORRECT BY HUMANS

Every check that has been run on the release, with its results and evidence, is listed in [VERIFICATION.md](VERIFICATION.md).

# monerodato

monerod for potatoes: an unofficial, low-spec build of the Monero daemon for the FCMP++ stressnet.

**Status: pre-alpha, for the FCMP++ stressnet only. Every program starts only on testnet. No binaries are published:
see [Building](#building).**

## Where the code is

This branch (`main`) holds only this page, [VERIFICATION.md](VERIFICATION.md) and the evidence. **The code is on the
[`monerodato-v0`](https://github.com/Fountain5405/monerodato/tree/monerodato-v0) branch.** There is one release, the
tag [`v0.19.0.0-beta.3.1-monerodato.7`](https://github.com/Fountain5405/monerodato/tree/v0.19.0.0-beta.3.1-monerodato.7).

- **To review the changes, start with
  [MONERODATO.md](https://github.com/Fountain5405/monerodato/blob/monerodato-v0/MONERODATO.md).** It sorts them
  into four series, one for each upstream project they would go to:
  - monero: 11 commits;
  - dalek-ff-group: 2;
  - multiexp: 1;
  - monero-oxide: 9.

  That is 23 commits, adding about 3,500 lines and removing about 200; [the list below](#the-23-commits) links each
  one. The file explains each change and gives the commands that extract each series as patches against its own
  upstream. Those patches are already made, and checked to apply cleanly, in [patches/](patches/).
- **The base and the full commit list:** the fork sits on seraphis-migration/monero `v0.19.0.0-beta.3.1`
  (`d80596ad1811`). [These 45 commits](https://github.com/Fountain5405/monerodato/compare/d80596ad1811...v0.19.0.0-beta.3.1-monerodato.7)
  on top of it are the whole fork. Besides the 23 changes:
  - 3 commits import the three Rust crates untouched;
  - 19 are marked `[fork-only]`: build plumbing, the testnet-only check, the startup notice, tests and docs.
- **Why the full diff is so large:** most of it is the generated `vendor/` directory and the untouched imports of
  the three Rust crates. The changes are the commits after each import.
- **GitHub's branch counts mean nothing here:** it shows `monerodato-v0` as thousands of commits ahead of `main`
  and a few behind. That is because `main` holds only the docs and shares no history with the code.
- **Earlier builds:** monerodato.0 to .6 were working builds that were not published; .7 contains all of their
  changes. Where a figure or a report names one of them, it was taken on that build.

## The 23 commits

Sixteen are optimisations: six cut memory, seven cut CPU time (one of the seven, monero 0005, only prepares the next)
and three are for slow disks. The other seven are a refactor, two bug fixes and four tests. Each commit message gives
the reasons and the measurements.

- **Patch:** the commit's number in its series in [patches/](patches/).

| Series | Patch | Commit | Kind | What it does |
|---|---|---|---|---|
| monero | 0001 | [`a8567e6ce`](https://github.com/Fountain5405/monerodato/commit/a8567e6ce) | Memory | caps FCMP++ batch verification at the physical core count, for proofs of 8 or more inputs |
| monero | 0002 | [`994918a78`](https://github.com/Fountain5405/monerodato/commit/994918a78) | Memory | returns freed verifier memory to the OS (`malloc_trim`), after batch verification and once at idle |
| monero | 0003 | [`18f49c423`](https://github.com/Fountain5405/monerodato/commit/18f49c423) | Fix | keeps block sync requests under levin's 100 MB packet limit; without it, a node can re-request the same span forever |
| monero | 0004 | [`412d6c9d3`](https://github.com/Fountain5405/monerodato/commit/412d6c9d3) | Fix | waits for queued batch tasks before an early exit; upstream has the same hazard |
| monero | 0005 | [`d95106256`](https://github.com/Fountain5405/monerodato/commit/d95106256) | CPU (groundwork) | splits batch verification into two FFI steps, so that 0006 can merge the batches |
| monero | 0006 | [`1d034e0c7`](https://github.com/Fountain5405/monerodato/commit/1d034e0c7) | CPU | one multiexp per curve per batch verification call |
| monero | 0007 | [`6bd706451`](https://github.com/Fountain5405/monerodato/commit/6bd706451) | Disk | adds `mdb_env_warm_dbis()` to the bundled LMDB: it reads database trees into the page cache in file order, with a stop function and a bounded read-ahead |
| monero | 0008 | [`b1d6fc6ef`](https://github.com/Fountain5405/monerodato/commit/b1d6fc6ef) | Test | tests it on an environment laid out as monerod's hash indexes |
| monero | 0009 | [`88b119bc3`](https://github.com/Fountain5405/monerodato/commit/88b119bc3) | Disk | `warm_indexes()`: reads the three hash indexes in a thread of its own, stopped before a close or a resize |
| monero | 0010 | [`e74c7b3be`](https://github.com/Fountain5405/monerodato/commit/e74c7b3be) | Disk | starts that warm-up at the end of `core::init` |
| monero | 0011 | [`710a32b70`](https://github.com/Fountain5405/monerodato/commit/710a32b70) | Test | tests the background warm-up against `close()` and a resize |
| dalek-ff-group | 0001 | [`70ce03ece`](https://github.com/Fountain5405/monerodato/commit/70ce03ece) | CPU | Field25519 as four saturated 64-bit limbs |
| dalek-ff-group | 0002 | [`a85f57b41`](https://github.com/Fountain5405/monerodato/commit/a85f57b41) | CPU | `wide_reduce` and `random` through the same fold |
| multiexp | 0001 | [`b8df9a72a`](https://github.com/Fountain5405/monerodato/commit/b8df9a72a) | CPU (wallet) | the constant-time multiexp uses Straus with a 4-bit window at every length |
| monero-oxide | 0001 | [`01f8dac15`](https://github.com/Fountain5405/monerodato/commit/01f8dac15) | Refactor | moves the verifier layout into its own functions, for 0002; no behaviour change |
| monero-oxide | 0002 | [`a7557102e`](https://github.com/Fountain5405/monerodato/commit/a7557102e) | Memory | verifies FCMP++ proofs with the constraints folded as they are produced, instead of storing them |
| monero-oxide | 0003 | [`fd2d30ce5`](https://github.com/Fountain5405/monerodato/commit/fd2d30ce5) | CPU | builds the circuits twice per proof instead of three times, cutting 0002's cost |
| monero-oxide | 0004 | [`0484d1a5a`](https://github.com/Fountain5405/monerodato/commit/0484d1a5a) | CPU | merges batch verifiers and splits the multiexp over threads |
| monero-oxide | 0005 | [`601562885`](https://github.com/Fountain5405/monerodato/commit/601562885) | Test | tests `verify_split` against `verify` at its chunk boundaries |
| monero-oxide | 0006 | [`1ba0eb513`](https://github.com/Fountain5405/monerodato/commit/1ba0eb513) | Test | tests that the prover's output is unchanged, byte for byte |
| monero-oxide | 0007 | [`ed01fbd43`](https://github.com/Fountain5405/monerodato/commit/ed01fbd43) | Memory (wallet) | frees the prover's coefficient vectors after evaluation, zeroized |
| monero-oxide | 0008 | [`6c31ede43`](https://github.com/Fountain5405/monerodato/commit/6c31ede43) | Memory (wallet) | shrinks each vector commitment's weight vector to the length it uses |
| monero-oxide | 0009 | [`2912fdd99`](https://github.com/Fountain5405/monerodato/commit/2912fdd99) | Memory (wallet) | moves the vector commitments' values instead of copying them |

## What this is

A fork of the FCMP++ stressnet monerod (seraphis-migration/monero) carrying optimisations for hardware that
cannot run the reference build: lower-memory proof verification, faster Field25519 arithmetic, merged batch
verifiers, and a startup read of the node's hash indexes for slow disks. It also changes the wallet's FCMP++ prover,
which builds transactions faster and with less memory. Same chain, same consensus rules as the reference build.
Every optimisation is compiled in and always on; there are no runtime switches, and the binary does not fall back
to the reference code on capable hardware. Instead, on a machine with at least 12 GB of RAM, 5 physical cores and
AVX2, it prints a notice at startup that the reference monerod can run there. If anything goes wrong, the fallback
is the stock binary on the same database.

## Testnet only

monerod, the wallets and the blockchain utilities start only on testnet, which is where the FCMP++ stressnet runs:
pass `--testnet`, or set `testnet=1` in the config file. On mainnet, stagenet or regtest they print one line and
exit before writing anything to disk. There is no flag, environment variable or config option that turns the check
off.

- **Why:** this is a build of a Monero beta, tested only on the stressnet. The reference beta defaults to mainnet,
  and on a mainnet data directory it would likely convert the database and then fail to follow the chain. That is
  from reading the code; it was not tried.
- **For mainnet or stagenet,** use the reference Monero software.
- **Details,** including the four small tools that have no check and what a test build is:
  [MONERODATO.md](https://github.com/Fountain5405/monerodato/blob/monerodato-v0/MONERODATO.md#testnet-only).

## Memory

These are measurements, not human-verified results. "Peak memory" means peak anonymous RSS. The comparison is always
with the reference build, `v0.19.0.0-beta.3.1`, on the heaviest stretch of the stressnet chain: blocks
3,105,401–3,105,444, which carry 128-input proofs.

**The working build monerodato.1 against the reference build.** Measured on a 12-core, 16 GB machine, over 3
alternating rounds. The verification code has not changed since that build:

| Threads | Peak memory, heavy blocks | Peak memory, small-transaction blocks |
|---|---|---|
| 24 | 5,313 → 1,451 MB (−72.7%) | −17.0% |
| 12 | 3,573 → 1,237 MB (−65.4%) | −12.9% |

**Where the saving comes from:** three of the changes. Measured cumulatively on a Ryzen 9 3900X (12 cores, 24 threads):

| Change | Peak memory | Smallest memory limit that gets through the heavy blocks |
|---|---|---|
| None (the reference build) | | 5,562 MB |
| Many-input proofs verified on at most one thread per physical core | −1.3% | |
| Plus: freed verifier memory returned to the OS (`malloc_trim`) | −47.7%, and −88.6% once idle | 3,000 MB |
| Plus: FCMP++ proofs verified with their constraints folded as they are produced, not stored | −70.4% | 1,765 MB |

- **The thread cap needs the trim:** on its own it saves little, because glibc keeps each thread's freed memory until it is trimmed.
- **One multiexp per curve per call** saved a further 10.1% of peak memory on heavy blocks.

**A replay of the chain:** from block 3,105,400 to the tip, in a 4-vCPU, 6 GB VM, with no swap:
- the reference build peaked at 2,167,448 kB;
- the working build monerodato.0 peaked at 931,456 kB (−57%).

**The cost:**
- With only the folded verification (working build monerodato.0), small-transaction blocks synced 7.9% slower than the reference build, because the folded verification rebuilds the circuits. With only the thread cap and the trim, the figure was +0.2%.
- With the merged batch verifiers as well (monerodato.1 and later), small-transaction blocks take 4.0% longer than the reference build at 12 threads, and 7.4% less time at 24 threads.

## The wallet

Measured, not human-verified. Both changes are in the wallet's FCMP++ prover; the proofs it produces are byte for
byte the same as before.

**Time to build a transaction** (wallet-rpc `sweep_all`, not relayed), before and after the multiexp change:

| Machine | Inputs | Before | After |
|---|---|---|---|
| 12-core Ryzen 9 3900X | 8 | 6.35 s | 3.11 s (−51%) |
| | 32 | 24.68 s | 12.08 s (−51%) |
| | 128 | 100.83 s | 52.09 s (−48%) |
| 4-core Core 2 Quad Q6600 | 8 | 37.14 s | 19.20 s (−48%) |
| | 32 | 183.54 s | 79.51 s (−57%) |

**Peak memory of the wallet while proving,** before and after the three memory commits: 169.1 MB at 8 inputs
(unchanged), 270.3 → 250.5 MB at 32 inputs (−7.3%), and 1,287.8 → 815.5 MB at 128 inputs (−36.7%). The time per
transaction did not change beyond the spread.

## Slow disks

Measured, not human-verified, in a test VM (4 vCPUs, 4 GB RAM) on a 5,400 rpm disk, with the stressnet chain.

Every block's verification looks up key images, transaction hashes and block hashes at random places in three
database tables. On a spinning disk each miss is a seek. At startup monerodato reads those three tables into the page
cache in file order, in a background thread, if they fit in 60% of the machine's memory.

- **A 400-block catch-up from a cold start** took 410 s without the warm-up and about 170–180 s with it.
- **Reading the tables** (0.83 GB) took about 60–80 s in file order. Read in key order, as a database cursor reads
  them, the same tables took 2,054 s.
- **RPC and P2P do not wait for it:** in the background, RPC answered about 30 s after launch on that disk.
- **Not settled:** the budget is 60% of memory with no absolute cap, and the tables' size on mainnet was not measured.

**Sources for all three sections:** the commit messages behind these figures are in [patches/](patches/), and
[MONERODATO.md](https://github.com/Fountain5405/monerodato/blob/monerodato-v0/MONERODATO.md) gives the full tables
with their ranges and methods. The reports are listed in [VERIFICATION.md](VERIFICATION.md).

## Who it is for

Only machines that cannot run the reference build. If your hardware runs the reference monerod, run that.

## Building

There are no binaries. To build the release on Ubuntu 24.04 (x86-64):

```sh
sudo apt update && sudo apt install -y build-essential cmake ninja-build pkg-config git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.99.1
. "$HOME/.cargo/env"
git clone https://github.com/Fountain5405/monerodato.git && cd monerodato
git checkout v0.19.0.0-beta.3.1-monerodato.7
git submodule update --init --recursive
make -C contrib/depends HOST=x86_64-linux-gnu -j"$(nproc)"
cmake -G Ninja -S . -B build/static \
  -DCMAKE_TOOLCHAIN_FILE="$PWD/contrib/depends/x86_64-linux-gnu/share/toolchain.cmake" \
  -DCMAKE_BUILD_TYPE=Release -DARCH=default -DSTATIC=ON -DBUILD_TESTS=OFF
ninja -C build/static daemon
build/static/bin/monerod --version
```

For the wallets, add their targets: `ninja -C build/static daemon simplewallet wallet_rpc_server`.

**Tested with monerodato.7** on 2026-11-11: these commands with the two wallet targets and `-j4`, in a fresh Ubuntu
24.04 container with 8 cores and 6 GiB of memory, on the tagged source. The three programs built and reported
`v0.19.0.0-beta.3.1-monerodato.7-release`.

**Memory and time,** measured on 2026-11-09 with the working build monerodato.2 and the daemon target only, in fresh
Ubuntu 24.04 containers on a Ryzen 9 9900X. Slower CPUs will take longer.

| Cores | Memory | `ninja` jobs | Result |
|---|---|---|---|
| 8 | 6 GiB, plus swap | default (10) | built in 4 min 51 s in all; it used up to 6.3 GB of RAM and 2.8 GB of swap |
| 2 | 4 GiB, no swap | default (3) | ran out of memory near the end of the main build (step 260 of 283) |
| 2 | 4 GiB, no swap | `-j2` | built in about 9 min in all (timed in two parts); peak memory 4.07 GB, just under the limit |

- **With 4 GB of memory:** use `ninja -C build/static -j2 daemon`, and close other programs or add swap. `-j1` should need
  less, but was not measured.
- **What was checked on the monerodato.7 build:** each of the three programs refused to start on mainnet and on
  stagenet, and monerod on regtest, with nothing written to disk; with `--testnet` they started. The release's own
  checks are in [VERIFICATION.md](VERIFICATION.md).
- **Running it on another machine:** the binary links only glibc dynamically, and needs glibc 2.38 or newer (Ubuntu
  24.04, Debian 13 and later). Built on Ubuntu 24.04, it will not run on Ubuntu 22.04 or Debian 12.
- **Hashes:** builds are repeatable, not bit-for-bit reproducible, so your binary's sha256 will not match anyone
  else's. The binary also holds the path of the directory it was built in.
- **Network:** the `contrib/depends` step downloads the C and C++ libraries' sources. The Rust code builds offline from
  the vendored crates.
- **Not tested:** other distributions, ARM, and building without `contrib/depends`.

## How it is made

Developed and maintained by coding agents under human direction, in the lab behind the
[stressnet monitor](https://github.com/Fountain5405/stressnetmonitor). The release had to pass a differential
testing gate against the reference binary: fuzzing of each changed primitive, a corpus of transactions that both
binaries must reject or accept identically, and a replay of the same chain through both binaries with identical
verdicts and tip hash. The checks and their evidence are listed in [VERIFICATION.md](VERIFICATION.md). A human
decided that the release is tagged, but no human has verified that the optimisations are correct.

## Scope

Stressnet only. FCMP++ is not on mainnet, and no mainnet binaries will be published from this base.

## Not affiliated

This is not a Monero project release and is not endorsed by the Monero project. "Monero" and "monerod" name the
software this is derived from. The code keeps Monero's BSD-3-Clause license; the LICENSE file is on the code branch.
