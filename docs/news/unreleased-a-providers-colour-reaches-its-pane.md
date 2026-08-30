---
version: unreleased
headline: A provider's `ctx.chrome` colour now reaches its pane instead of printing as text
---

Charter hands a component a mapping of the frame's own recipes — `ctx.chrome['heading']`,
`['ok']`, `['inset']` — so a pane written by somebody else can match the rest of the frame
without charter drawing over it. `docs/frame.md` has a worked example that writes them.
For a component that came from an installed distribution, none of it worked: the row
arrived in the pane as the literal text `\x1b[1mMetrics\x1b[0m`.

Both halves were right on their own. Charter contains what a foreign component returns,
because a row reaches a terminal where an escape is an instruction rather than a character
— a cursor move in one could draw over the pane beside it. The containment was
`contain.one_line` over the whole row, which escapes every escape, including the seven
charter had handed that same component one call earlier. So charter served a colour channel
and then stripped it, and told providers to use it.

**The containment stays and the vocabulary is cut out of it.** An SGR from charter's own
recipes costs zero columns, says nothing about position, and names only a plain attribute
or one of the sixteen slots in your palette — it cannot reach the pane beside it and it
cannot pick a colour charter did not pick. Everything else a row can do is still contained:
a cursor move, an erase, an OSC title string, a 24-bit triple, a background colour, a
newline. One parameter outside the vocabulary takes the whole escape with it, so `\x1b[1;41m`
is contained entire rather than half-kept. Under `NO_COLOR`, or a pane that is not a
terminal, nothing is exempt — the recipes are already empty there, and passing an escape
through on a provider's behalf would be charter emitting SGR from the frame after promising
not to.

**It is asked of the SGR parameters as numbers, not by matching the recipes' text.** A
string carries no provenance: `ctx.chrome['heading']` and a hard-coded `"\x1b[1m"` are the
same six characters by the time charter sees the row, so "did charter serve this" is
unanswerable. "Is this inside charter's vocabulary" is answerable and is the question worth
asking — and it gets `\x1b[01m`, `\x1b[m` and `\x1b[1;32m` right, which a match against the
recipes' spelling would have escaped for being spelled a way charter does not happen to
write.

**The test that was missing asserts the round trip.** One test pinned that the recipes are
served and another pinned that a foreign row is contained; nothing spanned them, which is
why the two halves could disagree from the day the recipes landed, with everything green.
The new one installs a real distribution whose renderer reads `ctx.chrome`, draws it
through the registry, and asserts the escape that comes out is the escape that went in —
and that the same renderer run as one of charter's own components produces the identical
row.
