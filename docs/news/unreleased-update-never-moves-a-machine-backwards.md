---
version: unreleased
headline: `charter update` no longer installs a charter older than the one running unless you name that version
security: true
---

**Affected: 0.45.0 through 0.61.0. Fixed here.** On a plane with no `[charter] version` pin,
`charter update` could install an older charter than the one running, and exit 0.

The version it moved to came from charter's update cache, not from PyPI's answer to that
run. When PyPI did not answer, the cache still held whatever an earlier check had stored.
That can be older than the charter you run without anything going wrong: `uv tool upgrade`
and `pipx upgrade` move the binary and leave the cache alone. Measured with stubs: running
0.60.0, the cache holding 0.58.0, PyPI not answering, no pin. `charter update` installed
0.58.0. That can take away a security fix the machine already had.

`charter version bump` had the same pattern, and lost it in 0.61.0. Now `charter update`
works the same way:

- **The target is what PyPI answered this run.** A cached version is never used instead.
  When nothing came back, it says so, as it does when the cache is empty: with no pin it
  installs nothing and exits 1.
- **With no pin, it does not move a machine backwards.** If the newest release PyPI
  reports is older than the charter running, it installs nothing and exits 1:

```
✗ PyPI reported 0.58.0 as the newest release, which is older than the 0.60.0 this machine runs, so nothing was installed. charter update moves a machine to an older version only when that version is named: charter update --to X.Y.Z
```

It names no cause for the older answer, because charter checked none. A version equal to
the one running is not older: that machine is current, and the update succeeds as before.

Going back is still one command. `charter update --to X.Y.Z` installs the version you name,
older or not, and asks PyPI nothing. A pin works as it did: a machine behind its pin moves
up to it, and a machine ahead of its pin is left where it is, with the drift reported by
`charter version`. `charter version sync --cli` is what moves this machine's charter down to
its pin.
