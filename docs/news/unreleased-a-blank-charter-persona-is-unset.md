---
version: unreleased
headline: A `$CHARTER_PERSONA` holding only whitespace counts as unset, and `charter persona current` says it was ignored
---

`export CHARTER_PERSONA=$(…)` over a command that printed only a space or a tab, or a stray
space in an rc file, leaves the variable set with nothing in it. charter used to take it as the
top rung anyway. The session resolved to an empty persona name, which hid the session and
terminal pointers, `.charter/active-persona` and `charter.toml`'s `[persona] default`. It got
no role and no persona tool grants, and nothing said why: the status line drew `◆ persona
none`, and `charter persona use forge` warned that `' '` takes precedence.

Now an empty or whitespace-only `$CHARTER_PERSONA` counts as unset and the rungs below it
decide. SessionStart, the status line and `charter persona use` read it the same way, and in a
frame launched with a blank `$CHARTER_PERSONA` the persona switcher's choice now shows, where
the panels used to keep resolving the blank. A name with whitespace around it (`" forge "`) still selects
`forge`, as before. `charter persona current` names the rung that decided and adds:

```
! $CHARTER_PERSONA is set but holds only whitespace, so charter ignored it and the rungs below it decided. Unset it, or set it to the persona you meant.
```

An empty `$CHARTER_PERSONA` is not reported, because a frame starts every chat with the
variable empty when the launch pinned no persona.

Nothing to adopt ([#1048](https://github.com/diazoxide/charter/issues/1048)).
