# Any push to a PR branch cancels that PR in-progress CI deletion sweep: .

_2026-09-11 23:58 · persistent_

Any push to a PR branch cancels that PR in-progress CI deletion sweep: .github/workflows/sweep.yml sets concurrency group sweep-${{ github.workflow }}-${{ github.ref }} with cancel-in-progress: true. Measured on PR #978 2026-09-11: pushing e81251e cancelled the a36194d sweep 20 minutes in; every shard read cancelled, yet the verdict job still ran and published "no verdict: 2 of 7 shards did not report; 49 of 112 measured, 63 out of time". A cancelled run verdict line says nothing about the code. So land comment-only fixes BEFORE the sweep starts (its sizing job takes about 7 minutes), or accept a full restart (about 35 minutes).
