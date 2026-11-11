# monerodato v0 replay gate, option (c): corpus VM on ref-b, ref-b as sole peer (2026-11-08, lab coordinator)

Inputs: reference_sha256 805add6308818779... (`v0.19.0.0-beta.3.1-release`); fork_sha256 dd839ccd3d557924389669cb13d7f2624987278795c8b56025939ff1bc6d80b9 (`v0.19.0.0-beta.3.1-monerodato.0-release`); snapshot S0 = stock DB popped to top block 3,105,400 (hash b70c9d6f...), data.mdb sha256 72ee13d00720c1afd4f02c66fac67558f371ea9906591537b97aadb6605339a9, byte-identical copy for each binary; corpus_manifest: n/a (corpus not yet run). Host: KVM VM, 4 vCPU (Ryzen 9 5900XT threads, pinned), 6 GB RAM; this is NOT low-spec hardware, so timings and memory are relative, not budget figures. Peer: one honest unpruned stressnet node (ref-b), exclusive. Stretch: 3,105,401 to tip (~3,127,400), includes the 128-input window 3,105,424-440.

| condition | result |
|---|---|
| C1 base replay | PASS. Stock reached 3,127,400 (hash 589ae5ab... = ref-b), fork reached 3,127,418 (e4ce75c3... = ref-b), and ref-b's block 3,127,400 is stock's tip: identical chain, 0 rejected blocks either side. Only one honest peer (no silent/under-reporting fixture). |
| C1 finding | Stock with default settings WEDGES at the 128-input window: it requests 100-block spans, worst 10-block window there is 100 MB (20-block 156 MB, max block 10.03 MB), the peer cannot send over levin's 100 MB limit, drops, reconnect loop ~1/s, no progress. Stock needed --block-sync-size 5. The fork, on defaults, synced with no drops (its cap). |
| C5 DB swap, light | PASS both ways from 3,126,400: fork on stock-written DB 1,019 blocks; stock on fork-written DB 1,020 blocks; tips = ref-b; 0 DB/verify errors. |
| C5 DB swap, heavy | PASS both ways from 3,111,000 (cb756a1a...), through the heavy blocks to 3,113,363: stock on fork-written DB 16,423 blocks in 609 s (tip 3,127,423 = ref-b); fork on stock-written DB 16,429 blocks in 515 s (tip 3,127,429 = ref-b); 0 errors. |
| C6 memory (relative) | Peak RssAnon over the full stretch: stock 2,167,448 kB, fork 931,456 kB (-57%); VmSwap 0 both (5 s samples, 456 / 380). Budget check on real low-spec hardware still to do. |
| timing (not comparable) | stock 2,286 s with --block-sync-size 5; fork 1,902 s on defaults. |
| C2 injected bad blocks, C3 reorg, C4 sync-cap liveness with fixtures | NOT RUN: need the corpus REJECT set and the E-series peer fixtures. |

Verdict so far: no soundness or liveness failure; C2-C4 and a low-spec memory/timing run remain before release.
