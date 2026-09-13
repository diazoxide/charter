---
version: unreleased
headline: `charter update` and `charter version bump` no longer move a machine, or a team's pin, to a charter older than the one running unless you name that version
security: true
---

**Affected: `charter update` from 0.45.0 through 0.61.0, and `charter version bump` through
0.61.0. Fixed here.** Both could install a charter older than the one running, and exit 0.
`version bump` also wrote that older version as the pin, so with `--push` every teammate
moved back to it on their next session. That can take away a security fix a machine
already had.

**`charter update`, on a plane with no `[charter] version` pin.** The version it moved to
came from charter's update cache, not from PyPI's answer to that run. When PyPI did not
answer, the cache still held whatever an earlier check had stored. That can be older than
the charter you run without anything going wrong: `uv tool upgrade` and `pipx upgrade` move
the binary and leave the cache alone. Measured with stubs: running 0.60.0, the cache holding
0.58.0, PyPI not answering, no pin. `charter update` installed 0.58.0.

`charter version bump` stopped reading the cache in 0.61.0, and `charter update` now works
the same way. The target is what PyPI answered this run, and a cached version is never used
instead. When nothing came back, it says so, as it does when the cache is empty: with no pin
it installs nothing and exits 1.

**Both commands, when PyPI's answer is older than the charter running.** Nothing compared
the two. Now neither command installs or writes anything, and each exits 1:

```
✗ PyPI reported 0.58.0 as the newest release, which is older than the 0.60.0 this machine runs, so nothing was installed. charter update moves a machine to an older version only when that version is named: charter update --to X.Y.Z
```

```
✗ PyPI reported 0.58.0 as the newest release, which is older than the 0.60.0 this machine runs, so nothing was installed or pinned. charter version bump pins an older version only when that version is named: charter version bump --to X.Y.Z
```

Neither names a cause for the older answer, because charter checked none. A version equal
to the one running is not older. `update` succeeds as before, and `bump` pins the team to
the charter this machine already runs.

Going back is still one command, and it asks PyPI nothing. `charter update --to X.Y.Z`
installs the version you name, older or not, and `charter version bump --to X.Y.Z` pins it.
A pin works as it did: `update` moves a machine behind its pin up to it, and leaves a machine
ahead of its pin where it is, with the drift reported by `charter version`. `charter version
sync --cli` is what moves this machine's charter down to its pin.
