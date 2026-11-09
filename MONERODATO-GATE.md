# monerodato gate tooling — how to run

From the second reviewer, 2026-11-08. Contents of this kit:

- `0001-fcmps-gate-test-differential-fuzz-of-folded-vs-store.patch`
  Applies to monero-oxide e020 (bde81f43). Adds `tests/differential_fuzz.rs`
  under crypto/fcmps and its registration. Commit into the fork's
  monero-oxide series.
- `0001-gate-test-differential-of-s64-Field25519-vs-pristine.patch`
  Applies to the patched dalek-ff-group crate (s64, c9eb2d7). Adds
  `tests/s64_differential.rs`, the dev-dependency, and the `[[test]]` target.
- `dalek-ff-group-orig-pristine.tar.gz`
  The pristine crates.io 0.5.0 crate with package renamed to
  `dalek-ff-group-orig` and lib to `dalek_ff_group_orig` (two-line diff from
  upstream). Commit as its own directory (e.g. `reference/`) BEFORE applying
  the s64 gate-test patch; the dev-dependency references it by relative path
  `../dalek-ff-group-orig` — adjust the path in the patch if the layout
  differs.
- `ct-check.sh`
  Branch-structure check for constant-time claims (x86-64; see header for the
  aarch64 variant). Runs against a built binary with force-emitted wrappers.

## THE ORACLE TRAP

The s64 differential's oracle (pristine 0.5.0) MUST be a path dev-dependency.
The monerodato workspace carries `[patch.crates-io] dalek-ff-group = { path =
... }`, which redirects ANY registry dependency on that package name —
including a renamed dev-dependency — to the patched crate. A registry-based
oracle would compare s64 against itself and pass vacuously. The gate fails
closed if the oracle crate is missing or unrenamed.

## Gate invocations (bar: zero failures/divergences on each)

1. s64 field differential:
   `RUSTFLAGS="-C overflow-checks" cargo test --release --test s64_differential -- --nocapture`
2. Folded-vs-stored verifier differential (in the fork's monero-oxide):
   `cargo test -p full-chain-membership-proofs --release differential_fuzz -- --nocapture`
   (0 verdict divergences, 0 verifier-state divergences, 0 batch divergences,
   0 panics, 0 invalid-statement acceptances; the harness prints its TOTAL
   line — the gate reads that line, not just the exit code)
   Toolchain: this runs under monero-oxide's pinned rustc 1.97.1
   (`rust-toolchain.toml`), while the binary is built with rustc 1.99.1; add
   `+stable` to run it under the shipped toolchain. v0 passed under both
   (TOTAL: generated 900 unparsable 27 both_reject 873 both_accept 0
   diverged 0 panics 0), and under 1.99.1 the whole fcmps suite passed with it.
3. Constant-time structure (per compilation; re-run after any toolchain or
   flag change): build the ct-inspect wrappers against the release crate and
   run `ct-check.sh <binary>`. Nonzero branch counts in the hot paths fail
   the gate; the known-accepted exceptions (fixed-trip loops, public-counter
   branches in pow) are documented in the script header.
   The claim this gate supports (refined after the v0 review): secret-dependent
   branches exist only as never-taken overflow guards on provably in-range
   arithmetic (overflow-checks on); re-inspect after any toolchain or flag
   change. With overflow checks on, each field op's body has one such guard,
   an accepted exception; `scripts/ct-scan.py` counts them in the shipped
   `libfcmp_pp_rust.a` (see MONERODATO.md, "Constant time under the shipped
   flags").

4. G009 structured fuzz of BatchVerifier::merge and Generators::verify_split
   (in the fork's monero-oxide):
   `cargo test -p full-chain-membership-proofs --release g009_ -- --skip diag --nocapture --test-threads=1`
   (three tests: g009_structured_fuzz, g009_split_engagement and
   g009_hsum_corruption; the fuzz prints a TOTAL line and the bar is
   `divergences 0` there. g009_diag_round is a replay aid, gated on
   G009_DIAG_ROUND and skipped by `--skip diag`.)
5. G009 FFI: fcmp_pp_verify_prepare / _finish / destroy on the tests/data
   vectors (in src/fcmp_pp/fcmp_pp_rust):
   `cargo test --release g009_ffi -- --nocapture --test-threads=1`

   Gates 4 and 5 run under monero-oxide's pinned rustc 1.97.1 in the
   second reviewer's trees; on monerodato.1 they were run under the shipped 1.99.1
   (`+stable`), 2026-11-09: gate 4, 3 passed ("g009 fuzz TOTAL:
   374 cases ... divergences 0", with the field-4 fix); gate 5, 1 passed.

The verify_split chunk-boundary test is not a gate: it is a test of the
series' own code, in the generalized-bulletproofs suite
(`cargo test --release -p generalized-bulletproofs --features tests verify_split`).

## Notes

- The differential harnesses are seeded/deterministic where possible
  (shape-seeded RNGs); random draws (OsRng) are re-verified in-process before
  any failure is reported — keep that property in any modification.
- Test-build caveat: verify_stored exists only under cfg(test); the gate
  therefore depends on the test build compiling the same crate code. Record
  the test binary's source hash alongside release artifacts.
- Both harnesses encode oracle rules that cost me false positives during the
  G009/s64 reviews; do not remove them:
  (a) Fcmp::verify Ok only QUEUES — the batch check is the verdict;
  (b) any per-proof-failed batch forces the overall verdict false;
  (c) corrupt verifier fields only at indices inside the generators' lengths
  (verify zips and silently drops the rest).

The toolchain line under gate 2 and the refined claim under gate 3 were added
by the fork's author after the v0 review, at the coordinator's request. Gates 4
and 5 were written up by the fork's author from the second reviewer's delivery
message of 2026-11-09. The test files are the second reviewer's, except one
hunk in the fuzz by the fork's author (field 4 draws its h_sum index in range,
per Notes (c)), which the second reviewer adopted. The rest is the second
reviewer's text.
