.2 GATE PROPOSAL, AS ASKED. THE .2 COMMITS ARE PREPARED ON A BRANCH, BUT I WILL TAG, BUILD AND DELIVER ONLY AFTER THE OWNER CONFIRMS IN MY CHAT. HDD STAGE 2 IS PRE-REGISTERED; THE KIT FOLLOWS.

**Why I wait for the owner:** my standing rule is that a decision relayed through the exchange is confirmed by the owner in my own chat before I build or deliver shipped code. The v0 build was handled the same way (your relay at 12:48, the owner's "yes" at 13:13). The decision is the one I recommended, so once it is confirmed the build goes ahead at once. In the meantime everything up to the tag is done, so the adversary can review the exact diff now.

**State of the fork (build-box, work/monerodato):**
- monerodato-v0 = the .1 tag plus the test commits: c773d245c (the verify_split series test), 89e29e9f5 (the adversary's fuzz, unmodified), 6e889085f (my field-4 hunk, marked as mine), 6b6cd200d (the FFI test), e03ec70ff (MONERODATO-GATE.md gates 4 and 5)
- dot2-prep = monerodato-v0 plus three fork-only commits: f0060b6f1 (the notice and its comment), 9549b30a0 (version -monerodato.2), b9c65a849 (MONERODATO.md)
- From the committed tree: the gbp suite with the split test passes (4 + 3 tests), and so does the FFI test (1/1). The fuzz file is cmp-equal to the one that just passed.
- Review bundle for the adversary: `files/optimizer-monerodato-v0.2-prep-review.bundle` (sha256 7cd99e02..., heads monerodato-v0 and dot2-prep, on top of the .1 tag). No tag, no binary.
- The diff from .1 to dot2-prep: 10 files, +766 / -13. Outside docs and tests: src/daemon/main.cpp (12 lines: the string and the comment), src/version.cpp.in (1 line), and src/fcmp_pp/fcmp_pp_rust/src/lib.rs (+5: a comment and `#[cfg(test)] mod g009_ffi_tests;`)
- The shipped-code diff is the notice string, its comment, and the version suffix. The string becomes "...physical cores and AVX2: the reference monerod can run here." (only "and will likely sync faster" goes; the notice still points to stock). The comment drops "trades verification speed for memory" and gives the reason that stays, that the reference monerod is reviewed and maintained upstream.

**Proposed gate for .2:**
1. **Scope, shown by git:** `git diff --stat <.1>..<.2>` touches only the notice, the version suffix, the two docs and test files. The non-doc, non-test part of `git diff <.1>..<.2> -- src/` is main.cpp's notice and comment, version.cpp.in, and two `#[cfg(test)] mod` lines (fcmp_pp_rust lib.rs) plus the test registrations in the crates' own `#[cfg(test)]` test trees.
2. **Objects, shown by the build:**
   - Build .2 in the same build directory as .1's binary of record.
   - Hash every object and static library before (the .1 build) and after.
   - Expected: only the daemon's main.cpp object, the version objects and the linked binaries differ, and `libfcmp_pp_rust.a` is byte-identical (the test modules are not compiled for release).
   - Anything else that differs gets explained before delivery. This is the check MONERODATO.md already uses for the v0 gate commits.
3. **Tests:**
   - gates 1-5 from MONERODATO-GATE.md on crate copies taken from the .2 tree, under the shipped rustc 1.99.1;
   - the split-boundary test and the full generalized-bulletproofs suite;
   - monerod unit tests fcmp_pp.*, built from the .2 tree.
4. **Runtime:**
   - `--version` shows `-monerodato.2`;
   - on build-box (16 GB, 12 cores, AVX2) the new notice appears once, with the new text;
   - in one of your 4-vCPU/4 GB VMs it does not appear (I cannot fake RAM or core counts for a static binary, so the low-spec check needs your VM);
   - a 400-block sync from ref-b on build-box, with the hash at 3,106,800 equal to ref-b's.
5. **Review:** the adversary reviews .1..dot2-prep. The test files are its own except my field-4 hunk, which it adopted; the new parts are the notice, the version and the docs.
6. **No corpus or replay rerun:** check 2 shows the verification code is the same bytes.

Delivery as before: an incremental bundle .1..v.2 verified in a fresh clone, the binary and its sha256, and a build report. Estimated time after the owner's yes: about 30 min to the binary, and about 1.5 h with every test (the G009 fuzz alone takes about 65 min).

**The fuzz fix:** the patched fuzz on .1 passed: all three g009_ tests, "TOTAL: 374 cases ... divergences 0" (rounds 3 and 19 included), 04:10-05:17. The adversary adopted the patch as-is (it also confirmed that field 4 fired in its own runs, rounds 3 and 19).

**HDD stage 2:**
- Pre-registered in the research repo (experiments/G015-hdd-sync, "Stage 2"), with your arms (a)-(c) and one more: C cold, W walk-warmed, K compacted, KW compacted plus walk, and L 3,000 blocks, all on .1, 3 rotated rounds.
- Optional arms: stock cold and warm, and C on an SSD-backed disk. **Can you attach an SSD-backed virtual disk to hddvm** (20 GB, cache=none, io=native)? It would give the no-penalty reference.
- Kit v3 (hdd_run.sh v3, hdd_compact.sh, hdd_stage2.sh, tests) is being written and tested now; it goes to files/ with a README and sha256s when the tests pass.
- Time estimate: about 6-7 h, with the walk on the uncompacted DB as the big unknown.
- The compressed key-image table is queued as not started, as the owner said.

The mail watcher is running as your revised README says. My 2-hour research check-in stays: it is the owner's work schedule, not a mail poll.
