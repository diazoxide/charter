---
version: unreleased
headline: A value glued to a short flag is read as one value again — a one-word wrapper prefix walked past the vault guard, and is denied
security: true
---

**Affected: 0.53.0, and every earlier release carrying the vault guard. Fixed here.** A
single wrapper word placed in front of an ordinary file read walked past the guard that
exists to keep a credential out of a transcript, on a plane built by `charter init` — while
the same read written without that word was denied. Confirmed live before anything was
changed.

The assembled command is not printed in this note. There are no backport branches, so this
note reaches people who cannot yet be protected by it, and a runnable line would arm them
against themselves faster than it would inform them. What follows is the shape, which is
what you need in order to decide whether it reached you.

**The class.** `env` packs a whole command into one token two ways — a long
`--split-string=` form and a glued short form — and `hooks._split_env_chdir` applied the
long form's splitting rule first. A packed command carrying an `=` of its own was therefore
split at the wrong one: the wrong token was named as the program, the file-reading program
was never named at all, and no guard downstream had a reader to object to. getopt hands a
short option everything glued after it, `=` included; the rule for the long form was
answering about a token that was never a long form.

**Why probing this by hand would have missed it, and that is the part worth recording.**
Measured against the shipped hook, the neighbouring spellings of the same idea were **all**
denied the whole time — the separated long form, the quoted short form, the long form
carrying the same value, and an ordinary assignment prefix. Four of five neighbours held.
Only the one form combining a glued short flag with a value that itself contains an `=` got
through, and that is exactly the form a real `env -S` setting a variable takes. Anyone
walking the obvious spellings would have stopped two rows in and concluded the guard held.
All six are one table in `tests/test_guard_attached_option_values.py` now, driven by one
loop, because the five that already denied are what make the sixth's regression visible: a
later refactor that trades one spelling for another has to redden something.

**And its siblings, because a bug with a shape has them.** Every flag whose value may be
attached went through the same two rules in the same wrong order. That includes the
directory-changing flags, whose value is what makes a later relative operand resolve
somewhere else — so the same mis-ordering reached the vault directory by *relocating into
it* as well as by naming it, and that is the reach that matters, because the operand
afterwards looks like nothing at all. `env`, `sudo` and `xargs` are the same parse. The two
rules are disjoint now rather than merely ordered — a glued short form never starts with
`--`, and the `=` rule now requires it — so the ordering cannot quietly come back, and all
three wrappers are walked across all four spellings in the tests.

The same parser is what charter's own test harness uses to decide whether a test is about to
spawn charter against your live plane. It had been repairing this ordering on its way in
since the defect was filed; that repair is deleted with this fix, which makes its removal a
second regression test — the suite stays green on production's parse alone.

Two neighbours found while measuring this one are **not** fixed here, and were filed rather
than quietly absorbed: a value attached to a *bundled* short option, and `env` accepting
assignments to names that are not shell identifiers while the parser stopped at the first
token it could not read as one. Both were live, both were unchanged in either direction by
this fix, and both are a different question from the one #547 asked — a fix that widens its
own scope until it is answering three questions is how a guard change stops being
reviewable. *(Both are closed in this same release, by their own change: see "An option is
its letter, and an assignment is whatever will do the assigning" —
[#556](https://github.com/diazoxide/charter/issues/556),
[#555](https://github.com/diazoxide/charter/issues/555).)*

`SECURITY.md`'s position is unmoved: guard rails, not guarantees — four rounds of
adversarial review established that deciding what a shell will execute, without executing
it, is not winnable in a Python tokeniser. This was a mis-ordering with a correct answer,
and it has one now.

## Are you exposed, and what do you do

**Yes, if** you are on 0.53.0 or earlier *and* you rely on the plugin's `PreToolUse` guard
to keep an agent away from `.charter/vaults/`. This is one more spelling past a fence
`SECURITY.md` already describes as holding against mistakes rather than against an attacker
with shell access as your user.

**The fix is the upgrade.** There are no backport branches, and there is nothing to adopt
beyond moving: no config to change, no flag to set.

**What does not change with it.** If something you did not fully trust has had shell access
as your user on a plane holding real credentials, rotate them. That was true before this fix
and is true after it, because the guard was never the boundary — the boundary is that
charter itself never prints a value.

[#547](https://github.com/diazoxide/charter/issues/547), found by external review and
confirmed on a plane built by `charter init`.
