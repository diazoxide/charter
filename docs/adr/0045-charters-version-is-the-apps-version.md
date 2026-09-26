# charter's version is the app's version

[ADR 0030](0030-a-rust-charter-reports-the-charter-it-brought.md) had to answer a question
the rebuild raised: which number `charter version` should print for a binary that is not a
Python package. It chose the newest release in the compiled-in news corpus, `0.62.1`, and
called it the charter this build "brought". The app's own `0.1.0` came second, labelled as the
build. A plane's `[charter] version` pin was compared against the corpus number, because a pin
then named a `charter-cp` release.

That answer made sense while charter-app was a port that still leaned on the Python charter. It
does not make sense for a standalone app. On 2026-09-23 the operator ruled that charter-app
*"should be standalone application without any dependency from old charter"*. Asked what
`charter version` should print, he chose **"The app's version only"**: `charter version` prints
the app's version, and a plane's `[charter] version` pin means the app's version line from now
on. The Python charter's news stays compiled in as read-only history.

This record is that ruling and the four things that follow from it. It amends ADR 0030. The
parts of 0030 it does not mention still hold: the verdict is not the Python charter's sentence,
the exit status is the contract scripts read, and every surface asks one comparison rather than
making its own.

## 1. `charter version` prints the app's version

```
  charter    0.1.0
  pinned     — (this control plane pins no version)
```

There is one number for "which charter is this", and it is `CARGO_PKG_VERSION`, the version
every crate in the workspace shares (`adopt::app_version`). The `build` row is gone, because it
would print the same number twice. `charter --version` already printed this number.

The exit status is 0 when the plane pins nothing, 0 when the pin is met, 0 for a pin on the
Python charter's line (section 3), and 1 on drift.

## 2. `[charter] version` names a version of the app

A pin equal to the app's version is met: *"this charter is 0.1.0, which is what this control
plane pins."* A pin that names a version of the app this build is not, or that is not a version
at all, is drift: *"drift: this control plane pins 9.0.0, and this charter is 0.1.0."* It exits
1, the status line draws its alert row, and the window marks the pin. The remedy names the app,
not a package manager: set the pin to the number `charter version` prints, or update the app.
`charter version sync` and `charter version bump` still refuse by name, as ADR 0030 decided. Both
install a charter, and this binary cannot install itself.

## 3. A pin on the Python charter's line is an older line, not drift

Every pin a plane carries today was written by the Python charter's `charter version bump`. This
app writes none. Those pins name `charter-cp` releases, and the Python line ended at **0.62.1**.

A plane pinned to 0.62.1, or to 0.58.0, has not fallen behind this app. It names a different
program. Calling it drift would be false, and it would also be the most surprising thing the app
could do. Every plane the Python charter ever pinned would turn red the first time the app
opened it. Its status line would grow an alert row. A script's `charter version || conform`
would start conforming. The only remedy the app could offer would be to edit a committed file,
possibly under a teammate who is still on the Python charter and relies on that pin.

So the app names the pin for what it is and leaves the plane alone:

```
• this control plane pins 0.58.0, a release of the Python charter (charter-cp): an older charter
  line, not a version of this app, so it is not drift and there is nothing to compare.
•   to hold this plane to this app instead:  set `[charter] version = "0.1.0"` in charter.toml,
  or remove the pin.
```

It exits 0. The status line draws no row, and the window shows no drift. Nothing tells the
operator to install or run the Python charter.

**How the line is recognised.** A pin is on the Python line when it is a version, is not the
app's own version, and is no newer than `adopt::PYTHON_LINE_LAST` (`0.62.1`). That is a constant
rather than a value read from the news corpus. The two are the same string, and a test holds them
equal. But a boundary that moved whenever somebody added a file to `news/` would change what
every plane's pin means without anyone deciding it.

**What this costs, stated rather than hidden.** While the app's own version is at or below
0.62.1, the two lines share numbers. A pin written by hand for this app in that range, such as
`0.2.0` once the app is at `0.3.0`, reads as the Python line and is never called drift. Today no
such pin can come from charter itself, because the app writes none. The ambiguity ends when the
app's version passes 0.62.1, or when the app starts writing pins in a form the Python charter
never did. Whichever comes first reopens this section. The alternative was to jump the app's
version past 0.62.1 now. That would have contradicted the ruling's `0.1.0` to save a case no
plane is in.

A pin equal to the app's version is met even when the Python line also had that number. At
`0.1.0` that is a Python release nobody has pinned in months, and "met" is the answer the ruling
asks for.

## 4. The news corpus is frozen history

*Superseded by the amendment of 2026-09-26 below: the corpus is gone, and `charter news` prints
the app's CHANGELOG.md.*

`crates/charter-core/news/` holds the Python charter's notes about its own releases, 0.44.0 to
0.62.1, and the few it had staged when it stopped. None of them is news about this app, whose
release notes are `CHANGELOG.md`. Nothing is added to the corpus.

- It stays compiled in. `charter news` still reads it, and its range views still default
  `--until` to where the corpus ends, `news::history_ends`. That function was
  `news::shipped_version` and it is renamed, because it is no longer the version of anything
  this binary ships. Defaulting to the app's version would make every range empty.
- Each entry still links into `diazoxide/charter-plane` by its release tag, because that is
  where the note was published. The constant is `news::HISTORY_REPO`. It is a pointer to history, and
  nothing in this app is published from there.
- The pin dialog's news list is the corpus between the pin and the app's version, and only on
  drift. The corpus names no version of this app, so the list is empty. The field stays for the
  day the app's own notes feed it.
- The corpus was compared byte for byte with the Python charter's copy in the differential run,
  and `news/SOURCE` pinned it to the oracle's commit for that comparison. Both went when the
  oracle was retired ([ADR 0046](0046-the-python-oracle-is-frozen-into-recorded-fixtures.md)):
  the corpus is frozen, so nothing is left for it to drift from, and `news/SOURCE` now only says
  where the entries came from.

## What the differential says about it

Written while the Python charter still ran beside the app in CI. Its answers are recorded
fixtures now ([ADR 0046](0046-the-python-oracle-is-frozen-into-recorded-fixtures.md)), and the
four `version-*` scenarios below are replayed from them, the divergence included.

The words differ, as they already did under ADR 0030. The exit status is compared in four
states. Three agree:

| State | charter-cp | charter-app |
| --- | --- | --- |
| no pin | 0 | 0 |
| pin 0.62.1, the oracle's own | 0 (in sync) | 0 (the Python line) |
| pin 9.0.0 | 1 | 1 |

The fourth, a pin on an older Python release (0.44.0), is 1 for the Python charter and 0 here.
That is this decision, and the run declares it as a `Divergence` naming this record. It asserts
both exit statuses and charter-app's whole message, so it fails if either side stops doing what
the declaration says. The status line's drift row now carries the app's version as the running
number, and the one rewrite that declares its remedy covers that too. The row a Python charter
draws for an older Python pin, where this app draws none, is held by `alerts/tests.rs`.

## What this rules out

- Printing the news corpus's version as this charter's version, anywhere.
- Comparing a pin against anything but `adopt::pin_verdict`. `charter version`, the alert row
  and the window's pin dialog all ask it. `doctor`'s `version lock` row still only names the pin
  and sends the reader to `charter version`, and moving it onto the verdict is ADR 0030's
  follow-up, still open.
- Calling a Python-line pin drift, or telling the operator to install, run or return to the
  Python charter to meet one.
- Writing news entries about this app into the frozen corpus. (The corpus is gone since the
  2026-09-26 amendment; the app's notes are CHANGELOG.md.)

## Amendment, 2026-09-26: `charter news` is the app's changelog, and the corpus is gone

Issue #352. `charter news` with no flags told every plane *"no baseline recorded, so there is no
range to report"*. The range view needed the update baseline the Python charter's `charter
update` stamped, and nothing in this app writes one. The operator ruled: **retire the range
view; `charter news` shows the app's own CHANGELOG.md sections.**

- `charter news` prints every section of the `CHANGELOG.md` compiled into the binary, newest
  first, with `[Unreleased]` only when it has something in it. `charter news --for <version>`
  prints one section, the same notes the release page and About Charter show. All three read
  the file through the `changelog` crate.
- `--since`, `--until` and `--pending` are retired. Each is still accepted and refused by name,
  exit 1, with what to run instead, because an agent reading an older skill will type them.
  `--pending` had nothing left to probe: every `check:` in the corpus named a command this
  binary does not have, so every entry reported *unchecked*.
- Section 4 is void. `crates/charter-core/news/` (339 files), its `build.rs` inclusion,
  `news::HISTORY_REPO`, `news::history_ends`, the probe machinery (`PROBEABLE`,
  `CHARTER_NEWS_PROBE`) and the release-body renderer are deleted. `adopt::PYTHON_LINE_LAST`
  stays a constant, as section 3 argued, and no test ties it to a corpus any more.
- `charter update` reads no baseline. It says it installs nothing, names the channel, and
  points at `charter news`.
- The pin dialog's news list is gone. It was always empty, as section 4 said it would be.
- The recorded rows for `news` changed with it (ADR 0046). `news-for-a-version-that-quotes-a-
  headline-is-refused-before-it-publishes`, `news-range-lists-the-same-entries-in-the-same-order`
  and `news-with-no-baseline-reports-no-range` are removed. `news-for-a-version-nothing-shipped`
  is renamed `news-for-a-version-the-changelog-does-not-have`. The new rows are
  `news-for-a-released-version-prints-its-changelog-section`,
  `news-prints-the-changelog-newest-first-with-no-baseline`,
  `news-range-flags-are-retired-and-say-what-to-run` and
  `news-pending-is-retired-and-says-what-to-run`.
