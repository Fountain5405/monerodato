# Constant-time and variable-time code, and the multiexp choice

Background for the multiexp change described in [`MONERODATO.md`](MONERODATO.md#the-wallet-faster-fcmp-proving), written for the owner of this fork, who asked for it to be kept here.

**Constant-time code** runs the same way whatever its secret inputs are: the same time, and the same memory touched. **Variable-time code** may take shortcuts that depend on the data. That is faster, but it can leak secrets through timing.

## Why it matters
Suppose an operation on a secret takes slightly longer for some values than for others. Then someone who can measure the timing can work back to the secret. That could be:
- another program on the same machine;
- another VM on the same host;
- sometimes an observer over the network.

Such attacks have recovered real private keys. Two kinds of leak are classic:
- **Branches.** `if secret_bit == 1 { do_extra_work() }` takes longer when the bit is 1.
- **Memory access.** `table[secret_digit]` reads a different memory location for each digit. The CPU cache keeps the locations used recently, and another process can detect which they are by timing its own reads. That reveals the digit without any visible branch.

## How constant-time code avoids it
It does the work for every possibility and keeps the right answer with arithmetic, not a branch.
- **Instead of a branch:** always do the extra work, then use a mask to keep its result or discard it.
- **Instead of `table[secret_digit]`:** read every table entry and keep the matching one with a constant-time select (`conditional_select`).

The instructions and the memory locations are the same every time, so there is nothing to measure. The price is speed: with a 16-entry table, every lookup reads 16 entries instead of 1.

## Where each is used in Monero
- **The wallet's transaction building must be constant time.** It handles secrets:
  - the spend key;
  - the blinding values;
  - which output, among all outputs on the chain, is actually being spent. Hiding that is the point of FCMP++.
- **The daemon's proof verification can be variable time.** Everything it works on is public, so there is nothing to leak, and it may take every shortcut.

The `multiexp` crate has two entry points:
- `multiexp`, constant time, which the FCMP++ prover uses;
- `multiexp_vartime`, which the verifiers use.

## The multiexp choice
A multiexponentiation computes a sum of points times scalars. Two algorithms are in the crate.
- **Pippenger's algorithm** puts each point into a bucket chosen by a digit of its scalar.
  - The variable-time version just does `buckets[digit]`, which costs almost nothing. So larger windows, which mean more buckets and fewer digits, are a win as the input grows.
  - The constant-time version must read and write every bucket for every point and digit. So more buckets cost more, and larger windows do not pay.
- **Straus's algorithm** gives each point a small table of its multiples, and scans only that table: 16 entries with a 4-bit window.

`multiexp` 0.5.2 picks the algorithm and window from the number of points, with one table for both entry points: Straus below 50 points, then Pippenger with windows rising from 4 to 8.

**Measured** in the research experiment W009, on one core of a Ryzen 9 3900X:
- **Selene and Helios** (the curves of the FCMP++ membership proof): constant-time Straus with a 4-bit window was as fast as or faster than every other choice measured, at every length from 4 to 2,048 points. The crate's choice was up to 3.2 times slower.
- **ed25519:** constant-time Straus with a 3-bit window was fastest, and the crate's choice up to 7 times slower. A 4-bit window was 6-8% slower than 3. In this fork, the constant-time multiexp runs only on Selene and Helios points, so this does not apply.
- **Constant-time Pippenger:** its own best window was 3 or 4 at every length on all three curves. The crate uses up to 8.
- **The variable-time path:** the crate's choice was right, within 0-5% of the best on Selene.

**Why the crate chose these windows is not known.**
- Its comment says the thresholds came from benchmarking k256 and curve25519-dalek.
- Its own benchmark (`src/tests/mod.rs`, ignored by default) times the constant-time `straus` and `pippenger`. It raises the window within each algorithm only, and never compares Straus against Pippenger.
- These measurements disagree with its Pippenger windows on these curves. The cause could be different relative costs on k256 (the benchmark's curve), or a different Pippenger implementation when the thresholds were set. Neither can be checked here.

**The change keeps the guarantee.**
- The constant-time `multiexp` now uses Straus with a 4-bit window for two or more points.
- The choice depends only on the number of points, which is public.
- The function it switches to is the crate's existing constant-time Straus, unchanged.
- The variable-time path is unchanged.
- Adding the identity, which Straus does for every zero digit, takes the same steps as any other addition. On Selene and Helios the group addition is the complete straight-line formula add-2015-rcb-3, and doubling (dbl-2007-bl-2) handles the identity with a constant-time select (`helioselene/src/point.rs`). The second reviewer pointed this out. It was equally true for Straus below 50 points, and for Pippenger's bucket sums, before the change.

**Measured.** The second reviewer ran a dudect-style test of the constant-time `multiexp` on the code tagged as monerodato.3:
- Selene and Helios points at 4, 64 and 256 pairs;
- random scalars against all-zero scalars (the most identity additions), all-one scalars and a fixed random set;
- 3,000 interleaved samples per class, with Welch's t-test on the means and on the top decile.

All 18 comparisons were clean: |t| at most 2.31 on the means and 1.63 on the tails, against dudect's threshold of 5. As a check that the test can see a leak, `multiexp_vartime` with all-zero scalars against random ones (2,000 samples per class) gave t from 389 to 5,382. It skips zero digits, as it may: the daemon only ever gives it public data.
