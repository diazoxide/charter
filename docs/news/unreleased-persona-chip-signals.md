---
version: unreleased
headline: The persona chips go quiet when nothing is wrong, and say who is running
---

Two changes to the persona column, both applying a rule the status line already lives by:
**a mark that renders every turn is furniture, and a real fault inside a row of furniture
reads like a zero.**

**Vault state is silent when it is fine.** The chip used to carry one of four marks on
every persona on every render, and on a healthy roster three of them meant "nothing to do
here". A healthy vault now renders nothing at all. What remains is the state worth a
character:

* `◦` dim — the persona *declares* a vault that cannot be used here: this machine has no
  vault by that name, or it has one whose file does not exist yet;
* `!` yellow — registered, and unhealthy.

The first of those was previously invisible. A persona declaring `vault: forge` on a
machine where no `forge` vault is registered rendered as the same dim `·` as a persona that
needs no vault at all — so "required and missing", the one vault fact that changes what you
should do next, was the one the line could not say. The two unusable cases share a glyph on
purpose: their fixes differ (`charter vault add` versus `charter secret set`), a chip can
carry neither, and `charter persona list` already prints both in words.

**A running persona says so on its own row.** `charter` has always known which personas
have sub-agents in flight, and spent it on one aggregate at the bottom of the screen —
`⚡in flight 2 · devops, devops` — where reading it meant matching a name against a roster
ten rows further up. The count now sits next to what it counts:

```
▸ devops ⚡2 12m
▫ forge ✎3 ⚡ 4m
```

The count appears only above one (a lone `⚡1` is the same non-fact as `todo 0`), and the
age — of the *oldest* dispatch, since the newest answers nothing — is always there, because
"has this been stuck for twelve minutes?" is the actual question and only the age answers
it. It is coarse (`4m`, `2h`) for the same reason every other age on this line is: at a
ten-second refresh, a seconds figure would be a number nobody could trust.

The session strip keeps a bare `⚡ 3`. The persona column caps at fourteen rows and
disappears entirely on a narrow pane, so the aggregate is what survives cropping.
