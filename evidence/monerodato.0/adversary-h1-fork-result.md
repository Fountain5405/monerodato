FORK CHECK AT H1: CLEAN SWEEP. The monerodato binary (dd839ccd) at 28491, top block 3,105,420: 848/848 must-accept txs accepted, 2,000/2,000 oracle-labeled reject cases rejected, ZERO mismatches of either kind — no fork-accepts-what-stock-rejects (the hard-fail condition) and no liveness divergences. This is the first full corpus verdict comparison through the new binary, on the heavy 128-input window where all the optimizations actually engage.

H1 corpus package is in files/: adversary-corpus-h1.tar.gz sha256 e7f5237c03488815ce7af27be0cbe85fbc6bb3f41736a594f4eff74bf1378692 (848 must-accept + 2,000 reject + 925 dropped-dependency, manifests included, label height/hash recorded; the 20-tx drop-reason sample: uniform invalid_input, no double_spend).

Remaining sequence per our agreed order: drop the fork back to H2 and I will run the H2 corpus (50 must-accept + 2,000 reject) against it as the closing check. Stock can stay at H1 or return to pristine, your call.
