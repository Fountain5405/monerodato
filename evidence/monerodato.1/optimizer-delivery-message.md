# monerodato.1 delivered; the HDD kit for stage 0

From optimizer. Two deliveries, both in files/.

## monerodato.1 (the owner's request: G009 in the fork)

| file | sha256 |
|---|---|
| `monerodato-v0.1.bundle` | `ce1b0e595f5313ca2f3c5c8aa49b1a796e4f5c005e3dbef53de2ad9333f07649` |
| `monerodato-v0.1-monerod` | `389c1bf9fd085ff83540d139fbb5dd884f902566604752325990835ea9faee88` |
| `monerodato-v0.1-build.md` | `65337a9e85dfbc5eee7d50a5d6e3f313c526d47047536171e69959ec41a1a7ba` |

**What it is:**
- Tag `v0.19.0.0-beta.3.1-monerodato.1` at e02f97bda: 6 commits on top of 510c123ff, the review bundle's tip. The bundle is thin against that commit.
- Verified in a new repo: v0, gate, review and this bundle fetched in that order; same tree as the source; fsck clean.
- With comments stripped, the code equals the measured G009 code, with the split always on.

**Checks: all passed.**
- **Unit tests:** 1,505 of 1,508 passed and 3 skipped, the same 3 as v0. That includes the 2 new tests: early exit, and one bad proof among valid ones in three positions.
- **Crate suites, rustc 1.99.1:** fcmps 12/12, including differential_fuzz and two new merge tests. In those, 9,123 malleated proofs that passed the per-proof checks were rejected after merging, serially and split. The dalek-ff-group, gate 1 and generalized-bulletproofs suites also passed.
- **Vendor:** `vendor/` is unchanged, and regenerating it gives a clean status.
- **Instruction census:** identical to v0 and stock.
- **Smoke test,** `--block-sync-size 20` through the wedge window: tip equals ref-b's; 0 oversize packets; 0 disconnects.
- **negdiff at both heights: DIFF PASS,** for the shipped binary and for a test-only build whose switch sends the pool cases through the merged batch path. The switch's log line is present.

**Not done:**
- No node timing of this binary yet. That comes as the base arm of my G012 stage 2 run, followed by a monerodato.0 vs .1 A/B.
- No ASan on this build. ASan ran on the research build.

**For the adversary:** ready when you and the owner want it. A reading order:
1. MONERODATO.md, the section "monerodato.1: one multiexp per curve per verification call";
2. a1efc0ca5, the generalized-bulletproofs merge and split (57 lines);
3. 0ffafbc2d, the merged call;
4. 39a74234f, the early-exit guard.

## HDD kit for stage 0

| file | sha256 |
|---|---|
| `optimizer-hddkit-hdd_run.sh` | `f2d2da62b84391e8e29a74d91cb31093d413fbeafeee43a42b967d9c83a3cb53` |
| `optimizer-hddkit-README.md` | `7bb1b7b5768591b5263a2b5355d6bc8ef3f182ca506c63ed1eb677b0266f7435` |

**Changes since the plan message:**
- your exclusive-node flags (`--hide-my-port --out-peers 1 --in-peers 0 --p2p-bind-ip 127.0.0.1`);
- it refuses to run if the data dir's lmdb is the snapshot's own directory, so a slip cannot overwrite S;
- it checks for room before copying;
- `lsblk` with scheduler and read-ahead goes into the summary;
- the RESULT line has `cpu_per_wall`.

The script touches only `$OUT_BASE/<run>` and `$DATA_DIR`.

**Exact invocation,** as in the README, which calls the script by its short name `hdd_run.sh`:

```sh
export SNAP_MDB=/var/lib/hdd/s3106400/testnet/lmdb/data.mdb
export DATA_DIR=/var/lib/hdd/run          # any HDD dir outside s3106400, about 22 GB free
export PEER=<ref-b>:28080                 # the string your replays use
export OUT_BASE=$HOME/hddruns
bash optimizer-hddkit-hdd_run.sh R0a-monerodato0 /opt/monero/monerodato.0/monerod
bash optimizer-hddkit-hdd_run.sh R0b-stock       /opt/monero/v0.19.0.0-beta.3.1/monerod
```

Then please send a tar of the two run directories, and check the `hash_at_3106800` in each RESULT line against ref-b.

**Dry run on build-box** (SSD, so this checks the mechanics only; monerodato.1, 20 blocks from ref-b): the restore copied 18.9 GB in 216 s (88 MB/s). RESULT: top 3,106,423, wall 7 s, CPU 24.3 s. The hash at 3,106,420 equals ref-b's. All eight files were written, and monerod stopped cleanly over RPC. The details are at the end of the README.

**Note:** your stock binary is 805add63 and mine (static, same tag) is 3432fd20. Either is fine for stage 0; I'll record yours.
