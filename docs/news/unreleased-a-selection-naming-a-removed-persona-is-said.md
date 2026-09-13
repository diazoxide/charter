---
version: unreleased
headline: A session whose persona pointer names a removed persona is told so, and `charter persona clear` reads back the rung that decides
---

`charter persona remove forge` leaves every session and terminal pointer that named `forge`.
Those sessions resolve `forge` and get no role and no tool grants, and they never fall back
to the plane's default, because nobody chose the default for them. That stays. What was wrong
is that nothing said it: SessionStart injected nothing, `charter persona current` printed
`forge` / `resolved via session`, and `charter persona create forge` later made every one of
those pointers select the new persona without a word.

Now SessionStart tells the session that `forge` is selected (and through which rung) and
that no persona by that name exists, with the ways out. `charter persona current`, `list`
and `clear` say the same beside the name. `charter persona create forge` says how many
selections already named it and now select it, by rung. The ways out depend on the rung:
`charter persona use <persona>` or `charter persona clear` for a pointer, unsetting
`$CHARTER_PERSONA` for the variable. Inside a chat, `clear` is not offered for a pointer the
chat did not write.

Outside a chat, `charter persona clear` used to print `Resolves to the committed default …
(personas/.default)` even when `charter.toml`'s `[persona] default` outranked that file, and
`Active persona cleared.` while `$CHARTER_PERSONA` still decided. It now reads back what the
shell resolves to:

```
✓ Active persona cleared.
• This shell now resolves to 'steward' (via charter.toml).
```

and, when `$CHARTER_PERSONA` is set, says the variable still decides rather than calling the
persona cleared.

Nothing to adopt ([#1045](https://github.com/diazoxide/charter/issues/1045)).
