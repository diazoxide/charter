---
version: unreleased
headline: the files under `.charter/` are 0600 because charter says so — a state directory charter did not create no longer leaks every one of them
security: true
---

**Affected: 0.53.0 and earlier. Fixed here.** Exposure needs a second account on the same
machine, or a backup or disk image somebody else can read — and one of the files at stake
is world-*writable* under a permissive umask, which makes this an integrity question and
not only a disclosure one.

0.53.0 settled the mode of every **directory** charter creates under `.charter/`: whichever
writer gets there first, each level comes out 0700, and the umask does not get a say
([#470](https://github.com/diazoxide/charter/issues/470)). The **files** inside them were
never asked. They were written at `0o777 & ~umask` — 0644 on the default `umask 022`, 0666
under `umask 000` — and that was harmless for exactly as long as the directory above them
was 0700.

Which is the one thing charter deliberately does **not** guarantee. `private_mkdir` leaves a
directory it did not create exactly as it is, on purpose: `$CHARTER_HOME` can point the
state directory at any path on the machine, so "chmod whatever we land in" is how charter
would come to tighten someone's home or a shared team directory unprompted
([#331](https://github.com/diazoxide/charter/issues/331)). `charter vault list` and `charter
doctor` report the loose one and print the `chmod` instead. So on a plane whose `.charter/`
predates charter, or was made by a `mkdir -p` under the umask default, doctor was correctly
telling you about a directory whose contents charter went on writing world-readable
([#505](https://github.com/diazoxide/charter/issues/505)).

Measured, in a plane with `.charter` at 0755 and `umask 022`:

```
-rw-r--r--  .charter/guard-seen.json
-rw-r--r--  .charter/persona-state/trace/<session>.jsonl
-rw-r--r--  .charter/persona-state/ephemeral/<session>/<persona>/<note>.md
```

The trace log carries which persona did what, when, and with what title — and `charter
persona remember`'s titles are the first line of the memory, so the text is in there. The
ephemeral store is session scratch memory in full. Under `umask 000` those are 0666, which
means another account could not merely read `guard-seen.json` but **rewrite** it, and decide
what charter treats as already consented.

**The property is "this file holds plane state", not "charter made the folder".** So the
answer is the one #470 gave the directories, asked at runtime about a file:
`config.write_for` / `open_for` / `touch_for` dispatch on where the path *is*, and every
state writer in the package goes through them — the guard sighting, the trace log, the
ephemeral and committed memory stores, the session/terminal/workspace pointers, the MCP
approvals, the commit gate, the route and ask markers, the update and status-line caches,
the tool ceiling, the push record, the reports. A path **outside** `.charter/` is written
with a plain `open` and left at your umask, because committed files are yours to mode:
`memstore` writes both quadrants through the same call and `personas/<n>/memory/` keeps its
0644.

**A file that already exists is tightened; a directory that already exists is not.** That
looks like two answers and is one — charter tightens what is its own and reports what is
not. A directory under `$CHARTER_HOME` may be a home or a team share charter merely landed
in, and has a life of its own. A file charter is putting its own bytes into is charter's
whatever its history, and leaving the old mode on it is
[#437](https://github.com/diazoxide/charter/issues/437) verbatim — the mode argument to
`os.open` is ignored for an inode that already exists, so the vault writers have settled the
mode on the **descriptor** since then. The rest of the state directory does the same now:
open without `O_TRUNC`, `fchmod` the descriptor, truncate, and only then write. There is no
window in which the new content is on disk at the old mode, and that is measured from inside
the write rather than asserted in a comment.

`.charter/` itself is untouched by this. If yours predates 0.53.0 it is still 0755, doctor
still says so, and `chmod 700 .charter` is still the fix — what changed is that the files in
it are no longer readable while you decide.

**The coverage half.** `tests/_statedirscan.py` reads the package and asks whether any
`mkdir` can still create a directory under `.charter/` without going through `config`. It
now asks the same two questions about `write_text` / `write_bytes` / `touch` / `open` /
`os.open` — the named half, where the writer spells a state path itself, and the handed
half, where one arrives as a parameter and no line in the callee mentions the state
directory at all. That second half is what caught `memstore` for the directories in #470,
and it is what caught it again here. An `os.open` clears the scan by settling the mode on
the descriptor rather than by being on a list, because the three writers that do it directly
— the vault file, the fingerprint key, the registry — each have a policy the dispatch does
not: two of them read the mode back and refuse to write rather than write into a file they
could not make private.

Nothing to adopt. Existing state files are tightened the next time charter writes them; a
file charter never writes again keeps the mode it has, so a plane that has been around a
while is worth one `chmod 600` sweep — or one `chmod 700 .charter`, which settles it for
everything at once.
