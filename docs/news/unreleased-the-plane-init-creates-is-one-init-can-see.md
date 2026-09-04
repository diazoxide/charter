---
version: unreleased
headline: '`charter init` re-derives config where the marker lands, so the plane it just built is one it can see'
---

`charter.config` resolves the control plane **once**, at import, through
`root.find_root_or_cwd`. That is right for every command except the one whose own first act
changes the answer: `cmd_init` wrote `charter.toml` and then ran to completion with
`config.HAS_CONTROL_PLANE` still `False`, in a directory that was by then a plane (#858).

Nineteen other derived settings were stale with it. `GROUP` is the plainest: `--owner` is
written into `charter.toml` by that very command, and the parsed copy beside it never saw
it.

## Why nothing was visibly broken

Every helper `init` calls takes *root* as an argument and is handed the right directory —
`_create_baseline_dirs(root)`, `_wire_harnesses(root)`, and `_ensure_front_door`, whose own
docstring says it writes "through explicit paths under *root* rather than
`config.PERSONAS_DIR`". So the stale globals were never *read* on the path that mattered.
That made it a trap rather than a fault: correct until somebody adds a code path that asks
"am I in a plane?".

Somebody did. Gating `hooks.context_block` on `HAS_CONTROL_PLANE` (#852/#857) broke a fresh
`charter init` — the gate asked during `init`, the answer was `False`, and the generated
opencode context file came out empty. That half was reverted; the rest of #857 shipped,
gating every handler in `hooks._HANDLERS`. And with this fix in, the gate still stays out —
it no longer breaks a fresh `init`, so what it would add is a branch nothing can reach.
`hooks.context_block` carries both halves of that.

## What init does differently

One statement, where the marker lands:

```python
toml_path.write_text(_render_charter_toml(forge_kind, owner, host))
created.append(_root.MARKER)
config.use(root)
```

Everything `init` runs afterwards — baseline directories, `.gitignore`, the status line,
every harness's wiring, the front door, the guard hook, the first-clone offer — now sees a
config that agrees with the directory it is standing in.

**`config.use(root)` and not a fresh resolution**, which is the tempting spelling and is
wrong. `root._outermost` hops outward through an enclosing plane's `workspaces/`, so a
plane scaffolded inside one — `workspaces/<ws>/charter`, which is what charter's own
dogfooding produces — would re-derive to the plane *above* it, and `init` would spend the
rest of its run reporting on a plane it did not create. `init` acts on the root it chose;
the re-derivation follows that root.

**Once, here, rather than making `HAS_CONTROL_PLANE` a call.** The larger change was
considered and refused: the value changes at exactly one point in charter's life and this
is it, so a call at each of its ~25 read sites would pay a stat per read to model an event
with one cause — and would let the answer move mid-command for reasons nobody chose.
`root._outermost`'s "loop until the answer stops moving" is not a precedent for that: it
iterates inside **one** resolution and then the answer is fixed, which is what this is too.

## What is asserted

`tests/test_the_plane_init_creates_is_one_init_can_see.py`. The claim pinned is not
"`HAS_CONTROL_PLANE` is True afterwards" — that is one name, and the next reader will reach
for a different one — but that **nothing derived from the root is left stale**: every name
in `config.DERIVED` equals what `config.derive` produces for the directory `init` wrote
into. A setting added to `derive` tomorrow is covered the day it is added.

Two cases beyond that: a harness being wired is told there is a plane (the #857 symptom,
asserted through the registry rather than through one harness), and a plane scaffolded
inside another plane's `workspaces/` is the one `init` built rather than the one above it
— the case that tells `config.use(root)` and a re-resolution apart.

## One thing this moved in the suite

Four `init` fixtures isolated themselves with `mock.patch.object(config, "ROOT", root)`.
That looked like isolation and was a coincidence — the command's helpers take *root* as an
argument, so the other twenty settings were simply never read while still pointing at the
developer's real plane. It does not survive a command that re-derives: measured on
`tests/test_init.py`, the patcher put `ROOT` back and left **nineteen** names in a temp
directory `addCleanup` had already deleted, for every test that ran afterwards — the worse
half of #402's shape, `config` reporting the real plane's root beside a `PERSONAS_DIR` that
no longer exists.

They now use `tests._isolation.point_config_at`, which snapshots and restores the same set
`config.use` writes.

Nothing to adopt.
