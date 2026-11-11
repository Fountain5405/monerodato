# monerodato.2: build report

From optimizer, 2026-11-09. Built in the same tree, build directory, toolchain and recipe as monerodato.1 (`monerodato-v0.1-build.md`). This report covers what changed and how it was checked. The gate is the one proposed to the coordinator at 05:21 (an exchange message) and approved by the adversary (an exchange message).

**Why:** the owner chose to drop "and will likely sync faster" from the startup notice (relayed by the coordinator at 04:23). The owner confirmed it in my chat between 11:50 and 11:59 UTC: "yes, confirm .2: drop 'will likely sync faster'".

## Deliverables (in the lab exchange)

| file | sha256 | size |
|---|---|---|
| `monerodato-v0.2.bundle` | `0ab0624838de2cd823a833308bddce9763d275de21768814f875cb4bb6472d44` | 19,717 B |
| `monerodato-v0.2-monerod` | `200e07d0da4eb03274dd237f3415349ac2fd4481f1cac2076db9d3111a8567ee` | 42,497,864 B |
| `monerodato-v0.2-build.md` | this file; its sha256 is in the message | |

**The bundle:**
- **Contents:** branch `monerodato-v0` and the annotated tag `v0.19.0.0-beta.3.1-monerodato.2` (tag object `29338b3b18e32244c4351fdb2cb843446608c812`), both at commit `b9c65a8492dc117843d269871fb3b63494a50aed`.
- **Thin** against `e02f97bda`, the .1 tag's commit (the tip of `monerodato-v0.1.bundle`).
- **8 commits:** the 5 test and gate-doc commits that the adversary reviewed in the prep bundle, and 3 fork-only ones (the notice, the version, MONERODATO.md).

**How it was checked:** in a new repo holding only the base (fetched as tag v0.19.0.0-beta.3.1), I fetched `monerodato-v0.bundle`, `monerodato-v0-gate.bundle`, `monerodato-v0-review.bundle` and `monerodato-v0.1.bundle`, then this bundle. Each `git bundle verify` listed only the expected prerequisite (for this bundle, e02f97bda). There the branch is at `b9c65a849`, the tag object is `29338b3b1` and the tag's tree is `005362011c106b1a72c84a03eb48d095b1f633e4`, all the same as in the source repo. `git fsck` is clean.

To use it, in the repo that already holds the earlier bundles:

```sh
git bundle verify ../monerodato-v0.2.bundle
git fetch ../monerodato-v0.2.bundle 'refs/heads/monerodato-v0:refs/heads/monerodato-v0' \
  'refs/tags/v0.19.0.0-beta.3.1-monerodato.2:refs/tags/v0.19.0.0-beta.3.1-monerodato.2'
git checkout v0.19.0.0-beta.3.1-monerodato.2 && git submodule update --init --recursive
```

## The series (`git log --oneline --reverse v0.19.0.0-beta.3.1-monerodato.1..v0.19.0.0-beta.3.1-monerodato.2`)

```
c773d245c generalized-bulletproofs: test verify_split against verify at its chunk boundaries
89e29e9f5 [fork-only] fcmps: gate test, the adversary's G009 structured fuzz
6e889085f [fork-only] fcmps: g009_fuzz's field-4 case draws its h_sum index in range
6b6cd200d [fork-only] fcmp_pp_rust: gate test, the adversary's G009 FFI test
e03ec70ff [fork-only] MONERODATO-GATE.md: gates 4 and 5, the adversary's G009 fuzz and FFI tests
f0060b6f1 [fork-only] daemon: the startup notice no longer says the reference monerod will likely sync faster
9549b30a0 [fork-only] version: -monerodato.2
b9c65a849 [fork-only] MONERODATO.md: monerodato.2, the startup notice and the tests since monerodato.1
```

**Scope** (`git diff --stat .1 .2`): 10 files, +766 / -13.
- Shipped code: `src/daemon/main.cpp` (the notice string and its comment) and `src/version.cpp.in` (the suffix `-monerodato.2`).
- `src/fcmp_pp/fcmp_pp_rust/src/lib.rs` gains a comment and `#[cfg(test)] mod g009_ffi_tests;`, which is not compiled into the release library (shown below).
- Everything else is tests and docs: the verify_split series test, the adversary's G009 fuzz with my field-4 fix, the FFI test, MONERODATO.md and MONERODATO-GATE.md.

**The notice, before and after:**
- .1: "...physical cores and AVX2: the reference monerod can run here and will likely sync faster."
- .2: "...physical cores and AVX2: the reference monerod can run here."

## Build
- `ninja -C build/static -j10 daemon` in .1's build directory, the depends toolchain, Release, `ARCH=default`, `STATIC=ON`. gcc 13.3.0, rustc 1.99.1.
- `check-vendor-rev: ok` (monero-oxide 31c26d96: 6 vendored crates match patched/monero-oxide; dalek-ff-group 0.5.0 from patched/).

**Object identity (the gate's check 2):**
- **Method:** every `.o`, `.a` and `bin/monerod` under the build directory was hashed before (.1's build of record, binary 389c1bf9) and after the .2 build: 282 files.
- **Rebuilt:** after a CMake re-run, ninja ran 5 steps: version.cpp, libversion.a, the fcmp++ rust lib, main.cpp, and the link.
- **4 files differ:**
  - `bin/monerod`;
  - `src/CMakeFiles/obj_version.dir/__/version.cpp.o`;
  - `src/libversion.a`;
  - `src/daemon/CMakeFiles/daemon.dir/main.cpp.o`.
- **`libfcmp_pp_rust.a` is byte-identical to .1's.** Cargo rebuilt it after the lib.rs change and produced the same bytes, so the test modules are not in the release library.
- **So the verification code in the binary is the same bytes as in .1.** For that reason the corpus and replay checks were not re-run (gate item 6 of the proposal).

## Checks
**Tests:** crate copies are taken from the .2 tag's `patched/` (git shows no untracked or ignored files there), under the shipped rustc 1.99.1, with the gate's own commands (MONERODATO-GATE.md).

| check | result |
|---|---|
| gate 1, s64 field differential | 2 passed; "5783+ cases, failures 0" (as .1) |
| generalized-bulletproofs suite, with verify_split | 4 + 3 passed |
| gate 2, differential_fuzz | 1 passed; TOTAL: generated 900, unparsable 27, both_reject 873, both_accept 0, diverged 0, panics 0 (as v0 and .1) |
| rest of the fcmps suite | 11 passed (including `merged_batch_verifiers`, `merged_batches_reject_malleated_proofs`, `test_malleated_proofs`, `test_folded_verify_matches_stored_on_tampered_proofs`); `prove_benchmark` skipped as before, and differential_fuzz and the g009_ tests run as their own gates |
| gate 4, G009 fuzz | 3 passed (g009_structured_fuzz, g009_split_engagement, g009_hsum_corruption), 4,025 s; "g009 fuzz TOTAL: 363 cases, 0 malleations never parsed, ... divergences 0"; no SURVIVED line. The case count varies with the fuzz's draws (374 in the fixed run on .1) |
| gate 5, FFI | 1 passed (g009_ffi_valid_merges_and_rejections), in a clean clone checked out at the .2 tag |
| monerod unit tests `fcmp_pp.*`, built from the .2 tree | 16 passed, 1 skipped (`fcmp_pp.force_init_gen_u_v`, skipped as in .1), 0 failed |
| gate 3, constant-time census (`ct-scan.py` on `libfcmp_pp_rust.a`) | the census of .2's library equals .1's line for line (160 lines); the library itself is byte-identical |

**Runtime checks:**
- **`--version`:** `Monero 'Fluorine Fermi' (v0.19.0.0-beta.3.1-monerodato.2-release)`.
- **The notice on a 16 GB, 12-core AVX2 machine:** appears once, in stdout and in the log, with the new text. "sync faster" appears 0 times.
- **The notice on a low-spec machine:** not checked here; I cannot fake RAM or core counts for a static binary. Please run it in a 4-vCPU / 4 GB VM: `grep -c 'monerodato is built for'` on stdout and the log should be 0.
- **A 400-block sync from ref-b:**
  - the DB was rewound to height 3,106,401;
  - .2 synced 424 blocks from ref-b alone in 66 s (not a timing run);
  - the hash at 3,106,800 is `74db18cf...b59c`, equal to ref-b's and to ref-b's;
  - 0 oversize packets, 0 verification failures, 0 ERROR lines.

## Notes
1. **Not re-measured at the node.** Nothing in the verification path changed, so .1's figures stand.
2. **Not in .2:** the `verify_split` doc-comment nit (the last chunk can be shorter than 1,024 terms). It is held for a later tag, to keep .2's shipped diff to the notice.
3. **No access beyond the optimizer's own boxes:** blocks came from ref-b by P2P, and tip hashes from ref-b by RPC reads, as before.
