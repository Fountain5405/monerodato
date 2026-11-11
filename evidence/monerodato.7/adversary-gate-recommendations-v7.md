Re: .7 public release — gate recommendations. Good call asking early; the
two-part review you describe is the right shape, and the public-release
setting adds items I would not demand for an internal tag. Plainly, then:

**On the guard (part 1) — design points to settle before the review:**
1. Key the guard on the EFFECTIVE network type after arg + config parse,
   not on the literal --testnet argv flag. Otherwise --regtest, or
   testnet=1 in a config file, behave inconsistently with the stated
   intent ("refuse unless testnet"). Decide explicitly which nettypes
   satisfy it (testnet only? testnet + regtest?) and say so in the docs.
2. The refusal must precede EVERY initialization side effect — not just
   "no database": no log file, no data-dir creation, no p2pstate, nothing.
   The gate should prove it empirically: run each refused binary in a
   snapshot-clean HOME and diff the filesystem (and <path>/tmp) after exit.
3. Every shipped entry point, enumerated: monerod, wallet-cli, wallet-rpc,
   blockchain_import/export/usage/ancestry/dump-svc-keys, and anything
   else the release builds — each refused, each clean, each with a test.

**Additional gate items for a public release (my recommendation):**
A. **Binary strings scan.** The fork's own docs note the build embeds
   build-directory paths. For public binaries, `strings` over all three
   (and the utilities) for internal names, usernames, home paths, host
   names. This is the item most likely to actually fire.
B. **Tree-wide scrub grep**, not just the files they thought of: user and
   machine names (build-box, the VM names, the Q6600 box), home paths,
   non-loopback IPs, and a decision on internal experiment labels
   (W009/G015 etc. — keep or scrub, but decide once). Plus my standing
   check: the scrub must touch ONLY docs and comments — code files
   byte-identical except the guard and the version suffix, verified by a
   path-restricted diff, not eyeballing.
C. **Behavior parity of the allowed path**: .7 --testnet against .6's
   binaries — the unit suites, a cold start, and a wallet refresh-class
   check. Strangers run exactly that path; it should be gated, not
   assumed.
D. **Licenses** present and unmodified in the vendored and patched crates
   (they were at import; a public repo must not regress it).
E. **The build-directory "Open" item**: at minimum, state it in the public
   docs so builders know bit-identical output is not guaranteed;
   --remap-path-prefix if they want to close it for the release.

**On part 2 (the rewrite), my anchor:** the rebuilt history means none of
my bundle hashes transfer, so I will verify TREE-wise — the .7 tree must
equal the .6 tag tree (which I hold, verified) plus exactly the allowed
delta, hunk by hunk: guard, scrub, suffix, nothing else. If the public
repo keeps the series history, I will also spot-check that the rewritten
commit messages still describe the same diffs, but the tree equality is
the load-bearing check.

No action needed from you now — this is input for the optimizer's build
and my review checklist. Send the review request when it is ready.
