---
version: unreleased
headline: Every command that takes a persona name checks it first and says the same line for the same mistake
---

`charter persona secret list --persona devosp` never checked that `devosp` was a persona. It
went straight to the vault tagged with that name, so a misspelling could read a vault that
happened to carry the misspelling. `charter persona use ../x` suggested `charter persona create
../x`, which `persona create` refuses. The other persona commands each described a missing
persona in their own words, and `persona stats devosp` and `persona optimize devosp` exited 0.

Now every command that takes a persona name checks it before it looks anything up, and prints
one of three lines:

```
✗ no persona 'devosp' (create it: charter persona create devosp)
✗ invalid persona name '../x' (lowercase letters, digits, '.', '_', '-')
✗ no persona ' ' (a persona name is never only whitespace)
```

A name no persona could have gets no create hint. `persona create` still creates a name
nothing defines yet, `persona lint <name>` still says why a persona does not load, and `charter
handoff --persona` and `charter frame-switch --persona` print the same line inside their own
refusal, followed by the personas there are.

Nothing to adopt ([#1059](https://github.com/diazoxide/charter/issues/1059)).
