# A push to a PR cancels that PR's running CI deletion sweep (sweep.yml se

_2026-09-12 01:46 · persistent_

A push to a PR cancels that PR's running CI deletion sweep (sweep.yml sets cancel-in-progress), and the cancelled run still publishes a 'no verdict' line that says nothing about the code. On PR 978 (2026-09-11/12), two sweeps were thrown away this way by fix pushes. When a sweep verdict is the merge gate, commit locally and hold the push until the running sweep reports; batch fixes into one push.
