---
version: unreleased
headline: A `$CHARTER_WORKSPACE` holding only whitespace counts as unset, and `charter workspace current` says it was ignored
---

`export CHARTER_WORKSPACE=$(…)` over a command that printed only a space or a tab, or a stray
space in an rc file, leaves the variable set with nothing in it. charter used to take it as a
hard pin anyway. The tree you stood in, the session and terminal pointers and the nominated
default were all hidden, and the session fell to `default`. `charter workspace current` said
it resolved via `$CHARTER_WORKSPACE`, and `charter workspace use alpha` warned that `' '` takes
precedence. Neither the session briefing nor the launch picker asked which workspace to use,
because the variable counted as a pin.

Now an empty or whitespace-only `$CHARTER_WORKSPACE` counts as unset and the rungs below it
decide. Resolution, the status line, `charter workspace use`, the session briefing and the
launch picker all read it the same way, as `$CHARTER_PERSONA` has been read since #1048. A
name with whitespace around it (`" alpha "`) still selects `alpha`. `charter workspace
current` names the rung that decided and adds:

```
! $CHARTER_WORKSPACE is set but holds only whitespace, so charter ignored it and the rungs below it decided. Unset it, or set it to the workspace you meant.
```

An empty `$CHARTER_WORKSPACE` is not reported, because a frame starts every chat with the
variable empty when the launch pinned no workspace.

Two smaller fixes of the same kind. `--persona` holding only whitespace is refused by
`charter persona secret` and `charter recall` with `no persona ' ' (a persona name is never
only whitespace)`. Before, the first blamed a missing vault and the second searched a persona
named `' '`. The warnings from `charter workspace use` and `charter persona use` now print a
name taken from the environment on one line, so a newline in it no longer starts a line of its
own.

Nothing to adopt ([#1055](https://github.com/diazoxide/charter/issues/1055)).
