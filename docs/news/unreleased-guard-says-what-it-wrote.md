---
version: unreleased
headline: `charter guard` writes the rule you typed, and shows you all of them
---

Three defects in `charter guard`, all of one species: the command did something wider or
narrower than it said, and said it convincingly. A guard whose output cannot be trusted is
worse than no guard, because the tick is what stops you checking.

**An MCP rule is no longer wrapped into a rule that matches nothing.** `_RULE_TOOLS` lists
the tools whose rules charter writes verbatim and deliberately includes `mcp__`, but the
condition consulting it also demanded a parenthesis. Claude Code's MCP permission syntax is
the bare `mcp__<server>__<tool>` and never has one, so `charter guard ask 'mcp__slack__send'`
wrote `Bash(mcp__slack__send)` — a rule matching a *bash command* by that literal name,
which is to say nothing at all — printed a tick, and committed it. The operator was never
prompted. Bare whole-tool rules like `Read` and `Task` went the same way.

The obvious fix mirrors the bug. `str.startswith` matches raw prefixes, so
`Globalprotect --connect` starts with `Glob` and `Taskwarrior add x` starts with `Task`;
dropping the parenthesis requirement would have written both bare, matching nothing in the
other direction. That requirement was load-bearing by accident. So the test is now on the
shape a rule can actually take — `Tool(pattern)`, a bare tool name, or a bare MCP name — and
everything else is a command to be wrapped.

One shape is refused rather than guessed. `mcp__slack__send *` names the MCP syntax and then
asks for a wildcard it does not have, and both available guesses fail silently, so charter
writes nothing and says why. That refusal happens before any harness is touched, so a
pattern charter cannot express never lands under one harness and not another.

**`charter guard` shows both halves of what it wrote.** The listing read `permissions.ask`
alone, and bare `charter guard` defaults to the listing — so the command you reach for to
answer "what has charter put in my permissions" showed the conservative half and hid the
widening one. An ask rule narrows what happens without a human; an allow rule widens it,
which is why `guard allow` shouts `COMMITTED` when it writes one. That being the invisible
half was the wrong way round. Rules written with `--local` had no reader at all.

The output is grouped by file rather than labelled per row, because the file a rule lives in
*is* its blast radius: a label asks you to trust it, a heading makes it structural.
`permissions.deny` stays unlisted — charter never writes it, and it answers neither question
you open this command with.

**And the help now names every harness the command writes.** It said rules go into Claude
Code's `.claude/settings.json` while both verbs write every registered harness, so a plane
that only uses Claude Code gained a tracked `opencode.json` from a command whose help named
one file. The behaviour is right and did not change: gating on `Harness.detect()` would make
a rule's reach depend on which harness happened to type the command, and a teammate on
opencode would silently not get it — the drift ADR 0014 exists to remove. The names are read
from the registry rather than written into the help, so the sentence cannot go stale the day
a harness is added.

The same command has a second, smaller gap in the same direction. A malformed settings file
is refused per harness rather than per command, so a `✗` beside a `✓` means one invocation
left the rule in force under one harness and absent under another — while the `✗` reads as
"nothing was written". `charter guard` now says so at the moment it happens. Making the
write all-or-nothing is a two-phase apply across three file formats and is filed separately
rather than smuggled in here.

Two further fixes are invisible from the outside and worth recording anyway: the dispatch
nudge now records the ask as well as its approval, so those approvals stop counting against
a denominator nothing incremented; and the session-marker sweep now covers the directory
rather than a list of suffixes that had drifted three times, which puts a floor under the
`ask-pending` markers every declined nudge leaves behind.

Found during the third pass of an authority audit (#365, #366, #367, #368, #369).
