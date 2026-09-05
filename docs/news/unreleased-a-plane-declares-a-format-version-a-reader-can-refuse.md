---
version: unreleased
headline: A plane declares a format version a reader can refuse, and charter stops rather than guesses
---

charter is about to grow consumers that are not the frame. Today any of them would guess.

The research that led here (`docs/research/2026-09-05-interface-agnostic-cores.md`) went
looking for what git — charter's closest sibling — actually promises a program reading its
files directly, and found one sentence:

> An implementation of git which does not understand a particular version advertised by an
> on-disk repository **MUST NOT operate on that repository**.
> — `Documentation/technical/repository-version.adoc`

That is the whole contract. Not a schema and not a spec: a way to know you are out of your
depth and stop. `gitformat-index(5)` and `gitformat-pack(5)` carry no stability language at
all, and the loose-ref format is specified only in C — gitoxide shipped subtly wrong refs
for years because git silently `rtrim`s on read. A consumer with no way to refuse guesses.

## The number already existed. The refusal did not

`charter init` has stamped `schema = 1` into `charter.toml` since the beginning, and
`instance.load` has raised on a version from the future for just as long. Two things kept
that from being a refusal:

**`config.derive` caught the exception and carried on.** It recorded the message as
`CONFIG_ERROR`, set the config to `{}`, and handed every command a plane made of fallback
defaults — a fallback group, a fallback workspace, an empty forge set. So a plane declaring
`schema = 99` did not stop charter; it made charter quietly act on a plane it had already
been told it could not read. The guard was the sharpest case: `known_forges_report` opens
`charter.toml` through the same `load`, got the refusal, and returned nothing — a
PreToolUse guard that looks present and denies nothing.

`config.PLANE_REFUSAL` is now recorded apart from `CONFIG_ERROR`, and `cli.main` declines
the command. Four commands still run, and each is a question about *charter* rather than a
read of the plane's contents: `doctor` (where the refusal is reported), `version` and
`_version-check` ("which charter is this?"), and `update` — the only remedy, since nothing
but a newer charter can understand a newer plane. `--version` and `--help` need no
exemption: argparse's own actions exit before the gate.

`init` and `reinit` are deliberately not exempt. Writing into a layout you have been told
you do not understand is the guess at its most damaging.

**An absent stamp meant "whatever this charter is".** `cfg.get("schema", SCHEMA)` — and
that is the reading which makes the whole feature useless the first time it is needed. The
day `SCHEMA` moves to 2, every unstamped version-1 plane on disk would start claiming to be
a version-2 plane, and a charter that understands 2 would read a version-1 layout with
version-2 rules. The guess, arrived at through the number that exists to prevent it.

Absent now means `UNSTAMPED = 1` — the layout every plane had before planes declared one.
Definite, and true: there has only ever been one plane format, so every plane created before
this release is a version-1 plane and stays readable by every charter forever. The two
readings are indistinguishable today, because `SCHEMA` is 1; the test that pins it moves
`SCHEMA` before it looks, or it would be pinning nothing.

A `schema` charter cannot compare against at all — `"2"`, `1.5`, `true` — used to fall
straight through the `isinstance` check and read as understood. It is refused on the same
terms. `true` is the sharp one: `isinstance(True, int)` is `True` in Python, so without an
explicit exclusion a plane declaring `schema = true` compares as 1 and reads as current.

## Two numbers, and the rule that keeps them from disagreeing

`workspace.STRUCTURE_VERSION` moved 4 → 5 this week. Adding a second version number beside
a warm one is how two numbers come to contradict each other, so the relationship is stated
rather than left to be inferred. They are **nested, and only the outer one refuses**:

- **`instance.SCHEMA` is the refusal number.** May this charter operate on this plane at
  all? Yes or no, and nothing heals a no but a newer charter.
- **`workspace.STRUCTURE_VERSION` is the repair number.** Is this one workspace's interior
  current? A stale answer is flagged on the status line and healed by `charter workspace
  reinit`. A workspace an older charter can still *read* is exactly what makes that repair
  additive.

So a workspace-layout change an older charter survives is, by definition, not a change that
requires refusal, and it moves `STRUCTURE_VERSION` alone. Two numbers can only come to
disagree if something compares them, and nothing does — pinned in both directions: moving
`SCHEMA` must not change a workspace's staleness verdict, and moving `STRUCTURE_VERSION`
must not change the plane's.

## `.charter/frame/` is private, and says so in code

The issue offered two defensible answers — version the frame state too, or state plainly
that it carries no promise — and asked for one of them rather than the ambiguity.

It is private. Not because it is internal (everything in charter is internal until somebody
reads it) but because **a format version buys the ability to refuse, and refusal is worth
something only when the writer and the reader can be different charters.** A plane's files
are committed, shared with a team and outlive any install. Nothing under `.charter/frame/`
is: every path there is written by a frame launcher and read by the panels of that same
frame — one process tree, one machine, one charter — and any residue of another charter is
reaped rather than read, because liveness there is a pid or a tmux window. A version stamped
there could never fire, which is the definition of decoration.

`frame.state.NO_FORMAT_PROMISE` is the statement, as a value rather than a comment so a test
can hold it, and a tripwire keeps any module under `charter/frame/` from quietly gaining a
format version beside it. Two consequences, and they are the whole promise: charter may
change the shape of anything under `.charter/frame/` in a patch release with no bump and no
migration, and a program reading it has no contract to hold charter to. `charter frame` is
the interface.

## What this is not

Not a schema and not a spec. The shape of `gather.json`, of `workspace.json`, of anything
else charter writes is documented to nobody and is free to change. git's lesson, which this
rests on, is that the *format* is truth and the *commands* are interface. This buys exactly
one thing: a consumer's ability to know it must stop.

## Nothing to adopt

A plane created by an earlier charter has no `schema` line and needs none — absent means 1,
which is what it is. `charter init` goes on stamping the current version. The refusal fires
only on a plane written by a charter newer than the one reading it, and the way out of it is
`charter update`.
