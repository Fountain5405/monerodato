# Replay (c) on monerodato.1: report (2026-11-09)

**Setup:** corpus VM on ref-b (4 vCPU pinned, 6 GB), ref-b as the only peer, the same unit and data dir as v0's run.
- **Binary:** monerodato.1, 389c1bf9 (`v0.19.0.0-beta.3.1-monerodato.1-release`), on defaults with no sync-size flag.
- **Start:** the pristine snapshot (237e8949), restored and popped to S0, top block 3,105,400. Its hash, b70c9d6f5a4374fe…, equals ref-b's.
- **Run:** started 00:34:31Z and synchronized at 01:05:59Z, at top 3,127,632.

**C1: PASS.** The tip hash at 3,127,632, fb347be4826859cb…, equals ref-b's. There are 0 verification-failure or rejected-block lines in the log. The only two "ERROR" matches are the log-category string, as in v0's log.

| | v0 fork (monerodato.0), 2026-11-08 | .1 fork (monerodato.1), 2026-11-09 | stock, 2026-11-08 |
|---|---|---|---|
| elapsed to synchronized | 1,902 s | 1,889 s | 2,286 s (with `--block-sync-size 5`) |
| peak RssAnon | 931,456 kB | 947,808 kB | 2,167,448 kB |
| peak VmSwap | 0 | 0 | — |
| VmHWM (incl. file-backed) | — | 3,719,248 kB | — |
| defaults through the 128-input window | yes | yes | no (wedged; needed `--block-sync-size 5`) |

**Reading:**
- .1 syncs the same chain as v0 in the same time, -0.7%, with one run each. RssAnon is +1.8%.
- The replay is limited by peer and transfer on 4 vCPUs, so G009's CPU saving does not show in wall time here. The optimizer's G013, on 12 threads, found heavy blocks -20% wall and -26% CPU.
- The memory saving against stock still holds: -56%.

**Not rerun on .1:**
- C5 (DB swap): .1 changes verification only, not the DB format.
- C2-C4: not run for v0 either, for the same reasons.

**Conditions:** n450 stock sync from ref-b ran throughout. No other VM load (HDD stage 0 had finished). The corpus oracles were stopped by the unit's Conflicts= and restarted afterwards.
