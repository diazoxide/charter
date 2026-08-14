# Success is checked; divergence is named

ADR 0009 governs what charter may say when something *failed*: it may name a cause it
recognised, never one it merely inferred. This is the same discipline aimed at the rest of
the output — what charter says when something **worked**, and what it says when two records
it maintains **disagree**.

Two rules, both descriptive of code that already exists:

1. **A success line reports what charter wrote, not what it was asked to write.** A state
   that was not read back is not confirmed, and must not be described as though it were.
2. **A divergence charter can see, charter names.** Where two records it maintains
   disagree, it surfaces the disagreement rather than resolving it silently by preferring
   one.

## Why it is written down

Because it keeps being derived from scratch, one site at a time, in comments — which is the
signal that it is a rule and not a series of coincidences:

- `_terminal_id` — *"an id that is wrong in the sharing direction is worse than no id"*,
  written after `WINDOWID` let every session in one window share a pointer and a workspace
  moved under a session that had chosen nothing.
- `check_plugin_skew` — *"it must not claim agreement it hasn't checked"*, written after
  `v0.1.0 matches the installed CLI` was printed against a CLI twelve releases ahead.
- `_scope_note` — no comment, because nobody had derived it there yet. It promised
  *"kept across closing/reopening Claude"* for a selection it had written nowhere
  persistent, and was fixed only after a user went looking for a bug in the status line.

Each was repaired locally and correctly. None of them made the next one easier to see.

## The four it explains

| Site | What it claimed | What was true |
| --- | --- | --- |
| `_scope_note` | the selection survives a restart | no terminal pointer had been written |
| `vault add --share` | *"registered ✓"*, echoing the item name | the local half still shadowed it, so the vault resolved to nothing |
| `check_plugin_skew` | *"matches the installed CLI"* | it had compared nothing |
| `workspace.resolve` | the active workspace, silently `default` | a selection existed and had become unreachable |

The first three are rule 1. The fourth is rule 2: charter *had* both records — a session
pointer that no longer matched and a fallback — and chose one without saying so.

## What this is not

**Not a mandate to warn.** charter declines to act all over the place, deliberately, and
that stays: the guard denies a command and says so; `skew_message` speaks only when the
plugin is *newer*, because only that direction breaks a hook; `_terminal_id` returns
`None` rather than a wrong key. Declining to act is a decision charter is entitled to make.
Misreporting what it did is not.

**Not "surface every difference".** A divergence here means two records *charter itself
maintains* — the two halves of a vault registry, a pin and an install, a pointer and the
state it points at. It is not a licence to compare everything to everything and report the
diff.

**Not new law.** Nothing here forbids anything that was permitted before. It exists so that
the next reviewer can cite one line instead of re-deriving the argument, and so that
deleting a confident, helpful-sounding success message reads as the fix it is rather than
as a regression.

## Consequences

**WARN is not a surface.** `check_plugin_skew` records the finding already: `doctor` exits
non-zero only on FAIL, and that exit code is what makes the SessionStart wrapper print
anything at all — *"at WARN the message reached nobody through either surface."* So a
divergence worth naming under rule 2 is worth FAIL. A divergence not worth FAIL was not
worth detecting; leaving it at WARN buys the appearance of diligence and none of the
effect.

**Pre-existing states start failing.** A machine already in a divergent state gets a red
preflight the first time a check like this ships, for a condition it had yesterday too.
That is the intent, not a migration problem — but the check owes the reader the command
that resolves it, per ADR 0009.

**Reading back costs a call.** Confirming what was written sometimes means re-reading a
file that was just written. That cost is accepted: the alternative is a success line whose
only guarantee is that no exception was raised.
