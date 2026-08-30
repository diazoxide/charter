---
version: unreleased
headline: A component charter did not write is refused a size policy it cannot have, instead of being given a different one
---

A component declares how big it wants to be — `Fixed(n)`, `Content()` for "as tall as my
own content", `Fill()` for "whatever is left". For a component that arrives from an
installed distribution, only the first was ever real. The other two were accepted, kept on
the object and read by nothing: the pane's height is the whole number of cells your
`[[frame.component]]` table gives it, resolved without importing anything. So a provider
that copied charter's own worked example — `repos` is `Content()`, and it is the component
the docs show you — got a frame that drew, at a height it never asked for, with nothing
anywhere saying so.

**Charter now refuses it and says why.** The pane names the component and the policy it
declared and tells you to declare `Fixed(n)`; the rest of the frame draws around it, in the
same rectangle your table asked for, so a machine with the wrong provider installed draws
the geometry a machine without it does.

Refusing rather than honouring, because honouring is not available and the two reasons are
different. `Content()` would mean charter importing your module and calling your `render`
to measure your content — on every command that resolves a config, `charter --version`
included, which is the one thing binding a component by *name* exists to prevent. `Fill()`
would mean a second pane taking the remainder, and the frame has exactly one by
construction: the repo table is left unasserted when sizes are re-applied so tmux's
`resize-pane -y`, which moves one boundary, has one remainder to give the rows to. A second
claimant was measured once already — registered as a placed `Content()`, the sidebar's
changes section was handed the repo table's height.

`Fill()` is still the right policy — and the required one — for exactly one part of a
composite. A part is drawn inside its parent's pane rather than split for, so it has a
parent with a remainder to give it. It is *placing* that needs a number, and that is the
line the refusal is drawn on.

**Written down alongside it: your provider distribution depends on `charter-cp`.**
Discovery does not — charter reads your entry point out of your distribution's metadata and
imports nothing, so a provider you have installed and not placed costs a frame nothing.
Construction does: charter accepts a `Component`, and the only way to hand it one is to
import the class. That has been true since the seam shipped and was nowhere stated, which
made it a coupling to rediscover rather than a trade to weigh. The trade, now recorded: a
structural protocol would let you build a component without importing charter, and would
move every refusal `Component` performs — the id alphabet that reaches tmux, the edge, the
size policy, `events` without `on_event` — from construction, where the message names a
fixable thing before a pane exists, to the moment your pane draws. `API_VERSION` is what
makes the coupling safe: one integer on both sides, refused rather than negotiated.
