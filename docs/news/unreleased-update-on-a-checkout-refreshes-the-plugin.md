---
version: unreleased
headline: On a charter checkout, `charter update` now does the half it safely can instead of refusing both
---

Found on the first real use of the dev channel, switching a maintainer's own plane over:

```
$ charter doctor
  !  plugin files   installed 18cdb73, marketplace 38c6671 — skills/browser/SKILL.md, …
        → this plane tracks the dev channel and its plugin does not. … Run: charter update

$ charter update
✗ this is a charter checkout — refusing to install over the tree you are editing.
```

`doctor` named the fix; the fix refused. There was no flag for the half that would have
worked, so the operator was left to reconstruct `marketplace update` → `uninstall` →
`install` by hand.

**The refusal is right and it stays.** Installing a wheel over the tree you are editing is
never what "let me try the update command" meant, and the failure is silent when it happens
— the news phase hands off to a binary that is not the tree you are working in and reports
on it as though it were.

**But `update` does two independent things, and only one of them was unsafe here.** It
moves the **CLI**, and it force-refreshes the **plugin**. The plugin is a separate artifact
under `~/.claude/plugins/`, entirely outside the checkout, refreshed from a marketplace
clone that is not the checkout either. On the dev channel, `charter update` in a charter
checkout now skips the CLI install, says so, and refreshes the plugin:

```
!  this is a charter checkout — the CLI here is the tree you are editing, so nothing was
   installed over it.
   the charter you run is this checkout, moved by git:  charter version
   the plugin lives outside this tree, so that half still runs:
✓  plugin: reinstalled charter@charter — it loads on the NEXT session.
```

No new flag, because `--plugin-only` would have been a second thing to know and would have
left `doctor`'s existing hint wrong. This makes the hint true where it is read.

**It bit exactly the wrong person, which is why it was worth fixing rather than
documenting.** A charter checkout is where a maintainer works, and a maintainer is the
person most likely to want the dev channel at all — so the guard blocked the remedy in
precisely the case the feature was built for. A remedy that refuses when followed is worse
than no remedy, because it costs the reader their trust in the next hint too.

Three things it does not do. It does not install the CLI — that is the guard, intact. It
does not move the harness artifact, because `_move_harness` writes into the plane root, and
on a charter checkout the plane root *is* the tree being protected. And it does not pretend:
with no `claude` on PATH there is no plugin to refresh either, and it says that instead of
reporting a refresh that did not happen.

Everything else still refuses outright, with the message and the exit code it always had:
a stable-channel checkout, where the released plugin is what pairs with the released CLI,
and `charter update --to X.Y.Z` anywhere in a checkout, because that names a published CLI
and installing it is the one thing that cannot happen here.

Nothing to adopt — upgrading is the whole of it.
