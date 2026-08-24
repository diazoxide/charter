---
version: unreleased
headline: Declaring `[update] channel = "dev"` no longer turns charter's own suite red — and the staleness nudge says which channel you are on
---

`charter update` on a charter checkout only refreshes the plugin when the plane is on the
dev channel, so anyone working on charter has to put `[update] channel = "dev"` in their own
`charter.toml`. That file is committed, and `charter save` commits it — so the moment the
line landed, six tests went red for everyone. The feature charter shipped in 0.52.0 was
unusable by the people it was shipped for.

Nothing was wrong with the channel. Two test classes had never isolated `config.UPDATE`:
`test_statusline_brand.UpdateIndicator` redirected one config attribute by hand and
`test_version_lock.AutoSync` assigned `config.ROOT` directly, and neither re-derives the
rest. For as long as `[update]` has existed, `{"channel": "stable"}` was true on every
machine, so the missing isolation was invisible — a fixture nobody wrote, silently agreeing
with what every assertion assumed.

**Fixed as a class, not as two files.** `[update] channel` is a fact about the person
running the suite, not about charter, so reading it off the real plane is now refused
outright — the same move 0.52.0 made for *writes* into your `.charter/`, and for the same
reason: a mistake nothing makes visible arrives again in a new file. A test that reads the
developer's own channel fails on the line that read it, with its own name in the message
and both ways out of it (`PersonaIso`, or `pin_update_channel`) named there. Turning it on
found eight more sites the same afternoon, in `doctor`, `statusline` and `charter update`
tests that nobody had suspected.

The tripwire arms itself per derivation rather than once at import, which matters more than
it sounds: a test that isolates and then restores has to come back armed, or the guard
protects only the first half of a suite run.

**And the reason the six failures were hard to believe.** They did not reproduce in a full
suite run — only when the two modules were run alone. `test_secret_exec.SecretExecMode`
defined a `_restore` method of its own, which replaced the identically-named cleanup it
inherited: the harness's config restore never ran, and `config.ROOT` stayed pointed at a
deleted temp directory for all 1193 tests that ran after it, alphabetically. Everything
downstream was reading a plane that did not exist, which happens to look exactly like the
stable channel. That cleanup is name-mangled now, so a subclass cannot shadow it by
accident — one other fixture had avoided the collision by hand, with a comment warning the
next person.

**Separately, the staleness nudge grew a channel chip.** `charter report send` warns you
when a newer charter is out; on the dev channel the number it names is `main`'s head
commit, not a release, and "0.52.0 is out" could not tell you which. It now reads `you are
on charter 0.52.0 dev; … is out`, through the same `_dev_chip()` the status line and the
frame's top slot already use — so the property test that walks the package for this idiom
now finds no site that skips it.

None of this reaches you unless you run charter's own test suite, except the report nudge.
Nothing to adopt: upgrading is the whole of it.
