---
version: unreleased
headline: A persona whose persona.md does not load is named as one, and pointed at persona lint instead of persona create
---

When a persona's directory was there but its `persona.md` did not load, `charter persona use
devops` said `no persona 'devops' (create it: charter persona create devops)`, and `persona
create devops` then refused because the persona already exists. A `persona.md` that could not
be read, or was not UTF-8, crashed the command with a traceback instead, and `charter persona
lint devops`, the command that says why a persona does not load, crashed the same way.

Now every command that takes a persona name says:

```
✗ persona 'devops' does not load from personas/devops/persona.md (see why: charter persona lint devops)
```

`charter handoff --persona` and `charter frame-switch --persona` print the same line inside
their own refusal, followed by the personas there are. `persona lint devops` names the reason,
for example `cannot be read (Permission denied)` or `is not utf-8 text`. A name with no
`persona.md` still gets the create hint.

Nothing to adopt ([#1061](https://github.com/diazoxide/charter/issues/1061)).
