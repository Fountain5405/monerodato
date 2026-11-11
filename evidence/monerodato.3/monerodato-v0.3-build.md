# monerodato.3: build report

From optimizer, 2026-11-10. Built in the same tree, build directory, toolchain and recipe as monerodato.2 (`monerodato-v0.2-build.md`). This report covers what changed and how it was checked.

**Why:**
- **The change:** W009 A, faster FCMP++ proving in the wallet. The owner approved it in my chat at about 01:33 UTC. coordinator relayed the owner's terms at 04:08: a fork-only patch, and the safety bar.
- **The adversary** approved dot3-prep (4ca30d2fc) with no findings at 11:56 UTC (20261110T121500Z, 71459cac). It reproduced the result independently, and at 12:10 its dudect run on the changed function came back clean.
- **The owner, in my chat:** "tag .3, have the adversary review it first" (about 11:35 UTC), and "yes, confirm both patches for .3" (about 12:20 UTC) for coordinator's README and Build-section patches.

## Deliverables (in <path>/files/)

| file | sha256 | size |
|---|---|---|
| `monerodato-v0.3.bundle` | `bb2fcb68723d656c7892aaabc888daa549551a122b690c17c55d26d9d4d35754` | 32,904 B |
| `monerodato-v0.3-monerod` | `5b75f25f863780bbd529dbce5e4cf24652e466f527edf6036b92edc5c225ca4c` | 42,489,680 B |
| `monerodato-v0.3-monero-wallet-rpc` | `3203fd46bfb57cef9f61b1b154040d1f09a594f551c253c8a88894c5ee298bdf` | 49,878,728 B |
| `monerodato-v0.3-monero-wallet-cli` | `28d40ddf3899015f978fad57ac614d5788105a50725c30052138bb6c7e8aa3c9` | 49,188,168 B |
| `monerodato-v0.3-build.md` | this file; its sha256 is in the message | |

**Why the wallets are new deliverables:** .3's change runs only in the wallet. Earlier releases shipped monerod alone. All three binaries are static: they link only libc and libm.

**The bundle:**
- **Contents:** branch `monerodato-v0` and the annotated tag `v0.19.0.0-beta.3.1-monerodato.3` (tag object `add3021aab9c49f50eb064d76f165fa65bdb6630`), both at commit `f29621fdc3101dea4f601d1ccbe5908ce1d77a81`. The tag's tree is `8646e3fd32e243b4d345cf710a4ed158514cc914`.
- **Thin** against `b9c65a849`, the .2 tag's commit.
- **9 commits.**
- **How it was checked:** in a new repo holding only the base, the v0, gate, review, .1 and .2 bundles were fetched, then this one. Every `git bundle verify` listed only its expected prerequisite. The branch, the tag object and the tag tree there equal the source repo's, and `git fsck` is clean.

To use it, in the repo that already holds the earlier bundles:

```sh
git bundle verify ../monerodato-v0.3.bundle
git fetch ../monerodato-v0.3.bundle 'refs/heads/monerodato-v0:refs/heads/monerodato-v0' \
  'refs/tags/v0.19.0.0-beta.3.1-monerodato.3:refs/tags/v0.19.0.0-beta.3.1-monerodato.3'
git checkout v0.19.0.0-beta.3.1-monerodato.3 && git submodule update --init --recursive
```

## The series (`git log --oneline --reverse .2..3`)

```
f2afea840 patched/multiexp: import multiexp 0.5.2 from crates.io, unmodified
3e1037de1 multiexp: constant-time multiexp uses Straus with a 4-bit window at every length
e6e9095fc [fork-only] fcmp_pp_rust: build against patched/multiexp
b7492d051 [fork-only] scripts: check-vendor-rev.sh fails unless the build uses patched/multiexp
21ab39cdc [fork-only] MONERODATO.md: the multiexp change; MONERODATO-CONSTANT-TIME.md
4ca30d2fc [fork-only] MONERODATO.md: "after monerodato.2", and the Q6600 timings
1ff3b3f50 [fork-only] README: warning header for the monerodato code branch
ac4985e58 [fork-only] MONERODATO.md: point the Build section to main's tested instructions
f29621fdc [fork-only] monerodato.3: version suffix, the docs say .3, two constant-time notes
```

- **The adversary reviewed** the first six (bundle 1c927263).
- **1ff3b3f50 and ac4985e58 are coordinator's patches**, applied with `git am --keep-non-patch`. `git format-patch` of each equals coordinator's file except for the hash line.
- **f29621fdc** sets the version suffix and turns "after monerodato.2" into "since monerodato.3". It adds two notes to the docs: why identity additions take constant time, and the adversary's dudect result.

**Scope, outside `vendor/`:** 22 files, +310 / -29.
- **Shipped code:**
  - `patched/multiexp/src/lib.rs`: the dispatch arm;
  - `patched/multiexp/src/pippenger.rs`: an `allow(dead_code)` attribute;
  - `src/version.cpp.in`: the suffix.
- **Build plumbing:** the `[patch]` line, `Cargo.lock`, `vendor/` without multiexp, and `check-vendor-rev.sh` check 4.
- **The rest is tests and docs:** `constant_time_straus.rs`, MONERODATO.md, MONERODATO-CONSTANT-TIME.md and README.md.

## Build
- **Daemon:** `ninja -C build/static -j10 daemon` in .2's build directory (the depends toolchain, Release, `ARCH=default`, `STATIC=ON`; gcc 13.3.0, rustc 1.99.1). Then `ninja -C build/static -j10 wallet_rpc_server simplewallet`.
- **The Rust library was rebuilt on purpose.** `libfcmp_pp_rust.a` was removed before the build, and the gate required cargo's log to show `Compiling multiexp v0.5.2 (.../patched/multiexp)`. It does.
  - Why: ninja does not track the patched crates' sources (found on 2026-11-10 in a research build). Without this step a rebuild could keep .2's multiexp.
- **`check-vendor-rev: ok`:** monero-oxide 31c26d96; 6 vendored crates match patched/monero-oxide; dalek-ff-group 0.5.0 and multiexp 0.5.2 from patched/.

**Object identity:**
- **Method:** every `.o`, `.a` and `bin/monerod` under the build directory was hashed before (.2's build of record, monerod 200e07d0) and after the .3 daemon build. 283 files.
- **6 differ:**
  - `bin/monerod`;
  - the version object and `libversion.a`;
  - `libfcmp_pp_rust.a` and its copy in cargo's target directory;
  - one new hashed cargo artifact of that library.
- **That is exactly what the adversary predicted:** only the multiexp-derived library, the version objects and the binary.
- **No C++ object of the daemon changed,** and `main.cpp.o` is byte-identical to .2's. So monerod's own code is .2's, relinked with a library whose only change is in the wallet's prover path. That path is not reachable from monerod: shown from the source and an objdump call graph, and confirmed by the adversary.

## Checks
**Tests:**
- **Crate copies** are taken from the tag's `patched/`, under rustc 1.99.1.
  - The copies point `[patch]` at both the fork's s64 dalek-ff-group and its patched multiexp.
  - Their `Cargo.lock` was moved off registry multiexp 0.5.1. The gate checked that it holds one multiexp, version 0.5.2, from a path.

| check | result |
|---|---|
| gate 1, s64 field differential | 2 passed; "5783+ cases, failures 0" (as .2) |
| multiexp crate tests | 4 passed, including `test_constant_time_straus_ed25519` and `_secp256k1`; the benchmark ignored, as upstream |
| generalized-bulletproofs suite | 4 + 3 passed, including `verify_split_matches_verify_at_chunk_boundaries` |
| gate 2, differential_fuzz | 1 passed; TOTAL: generated 900, unparsable 27, both_reject 873, both_accept 0, diverged 0, panics 0 (as v0, .1 and .2) |
| rest of the fcmps suite | 11 passed (`prove_benchmark` skipped as before; differential_fuzz and the g009_ tests run as their own gates) |
| monero-fcmp-plus-plus | 2 passed |
| gate 4, G009 fuzz | 3 passed; "g009 fuzz TOTAL: 341 cases, 0 malleations never parsed, ... divergences 0"; no SURVIVED line |
| gate 5, FFI | 1 passed, in a fresh clean clone at the .3 tag |
| unit tests (fcmp_pp, wallet_tx_builder, carrot_fcmp, bulletproofs_plus, curve_trees), built from the .3 tree | 41 passed, 1 skipped (`fcmp_pp.force_init_gen_u_v`, as in .1 and .2), 0 failed |
| gate 3, constant-time census (`ct-scan.py` on `libfcmp_pp_rust.a`) | 160 lines for .2 and .3. They are identical once build-generated names (crate and codegen-unit hashes, anonymous label numbers) are normalised: the same functions, instruction counts, conditional jumps and panic guards |

- **The gate log** has one interruption. The FFI step first stopped because the copy used for .2 had been removed, and the clean-check had passed on the missing directory. A resume script ran gates 5 and 3 and the wallet check on a fresh clone. The precheck now requires the copy to exist.

**The wallet:**
- **The run:** the W009 kit, 8 inputs, 1 round, `do_not_relay`, on W006 copies against an offline node, with .2's static wallet-rpc (a24a198b) and .3's (3203fd46).
- **Fee 1,292,925 and weight 17,239** in both.
- **Time:** 3.16 s with .3 against 6.33 s with .2. One round, not a timing run; W009's 3-round figures are 0.49-0.52 of .2 on this box, and 0.52 and 0.43 on the Q6600.

**Runtime checks:**
- **`--version`, all three binaries:** `Monero 'Fluorine Fermi' (v0.19.0.0-beta.3.1-monerodato.3-release)`.
- **The notice on a 16 GB, 12-core AVX2 machine:** once, in stdout and in the log, unchanged from .2.
- **A 400-block sync from ref-b,** cleared by coordinator at 12:24:
  - the DB was rewound to height 3,106,401;
  - .3 synced 420 blocks from ref-b alone in 66 s (not a timing run);
  - the hash at 3,106,800 is `74db18cf...b59c`, equal to ref-b's and to the reference;
  - 0 oversize packets, 0 verification failures, 0 ERROR lines.
  - **One false start:** the first try stopped before any pop, because my W009 offline node held testnet ZMQ port 28082. It was rerun with `--no-zmq` for the rewind.

## Notes
1. **Not re-measured at the node,** because monerod's code is .2's (see Object identity). .1's and .2's node figures stand.
2. **The proof evidence for the change** is in the review bundle's material:
   - the 258-case differential (byte-identical, 258/258 cross-verified);
   - the adversary's 36-case reproduction;
   - its dudect (18/18 clean, with a variable-time control at t ≥ 389).
3. **No access beyond the optimizer's own boxes.** Blocks came from ref-b by P2P, and tip hashes from ref-b by RPC reads.
