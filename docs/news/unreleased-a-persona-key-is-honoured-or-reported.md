---
version: unreleased
headline: A persona frontmatter key is honoured or reported — and a miscased `Borrows:` no longer hands out the wide tool grant it was written to give up
security: true
---

**Affected: 0.53.0 and earlier. Fixed here.** This is the one entry in this release that
narrows a **live tool grant**: if a persona on your plane has a miscased key, tools are
being auto-approved today that its author wrote the line to give up. No attacker is
needed — a shift key is enough. The last section is how to check.

Two defects in the persona parser, and one property between them: **a key the author
declared is honoured or reported, never silently resolved.** One is "the key was not
recognised", the other is "the key was recognised twice", and both used to end in a value
chosen by accident with nothing said.

## `Borrows:` failed OPEN

charter's frontmatter parser matches keys exactly, so `Vault:` puts `Vault` in the dict and
the lookup for `vault` finds nothing. Almost every key fails that way toward *less*: a
miscased `Vault:` means no credentials, a miscased `Tools:` means no auto-approvals. The
persona declares a vault and has none, which is wrong but not dangerous.

`borrows:` is the exception, because it is answered by **absence**. `borrows_of` returns
`None` for an absent key on purpose, and `None` means "I have not opted in — keep the
legacy `uses:` grant", which is the *wider* one. So the author who wrote the word that
grants nothing got the grant they were opting out of:

```
uses: forge, release
Borrows: none            ← read by nothing
```

```
borrows_of(front):      None
effective_tools(front): ['Bash(gh:*)', 'Bash(git push:*)']
structural_errors:      []
```

Both borrowed personas' tools auto-approved at the tool gate, from a definition whose
author had written the opposite, with the lint that exists to say so reporting nothing at
error level. That is the grant `borrows:` was added to take back, reached through one shift
key.

**A `borrows:` charter cannot read now answers `[]` — borrow nothing — and never `None`.**
`None` means the author never mentioned borrowing, and nothing else. This is narrower than
the author asked for whenever they meant `Borrows: forge`, and narrower is the direction
this is allowed to be wrong in: the persona pays a permission prompt it did not expect and
the lint names the line to fix. The other direction is measured in a vault.

## The sibling was wider than the named bug

Three of the fields the generated sub-agent carries are enforced by being **present**, so
misspelling one does not narrow the enforcement — it deletes it:

* `Agent-tools:` — no `tools:` line is emitted, and no `tools:` line means the sub-agent
  inherits **every tool**. The allowlist does not shrink; it vanishes.
* `Disallowed-tools:` — the denylist vanishes the same way.
* `Draft:` — an unfinished charter becomes a sub-agent's system prompt, which is precisely
  what `sync-agents` refuses to let happen.

All three were reached through a run that printed `✓ Synced 1 persona sub-agent(s)`.
`sync-agents` now generates no agent from a definition charter could not read, removes any
stale one, and prints which key to fix — the same answer it already gives a draft, for the
same reason.

## A key written twice was resolved by line order

`vault: safe` above `vault: prod` handed out `prod`, because that is what building a dict
does. The file stated a contradiction and charter resolved it by which line was lower,
losing the first value before any consumer saw the dict — the same silence over `tools:`
twice, `extends:` twice, `borrows:` twice.

A key declared more than once is reported by name now. Charter does not pick.

## Refused, not case-folded

Folding the lookup would answer `Vault:` and leave `vualt:`, `borrow:` and
`delegate_when:` exactly as silent — a guard against one spelling rather than the property.
What catches every one of them is the vocabulary being **closed**, which charter already
had: `persona lint` has long warned about a key it neither reads nor emits.

What changed is which of those findings carries weight. A key charter simply does not read
stays a **warning** — charter has no claim about `modell:`, a harness's own field is a
legitimate thing to carry in a committed file, and an error there would break planes that
are correct. A key charter *does* read, spelled in another case or written twice, is an
**error**: charter can name the word the author was reaching for, and that is what makes it
actionable and what lets the grant act on it. Nothing is ever read out of the miscased key.

## If you have one committed today

Five shipped personas and all 109 shipped news entries are clean, so this is about your
plane, not charter's.

* A miscased or duplicated key goes from a lint **warning** to an **error**, and now shows
  on the status line rather than only in `charter persona lint`.
* Your tool grant changes in exactly one case: a miscased `Borrows:`, which today silently
  grants the borrowed personas' tools and after this grants none of them.
* `charter persona sync-agents` writes no agent for that persona until the key is fixed,
  and names it.
* Nothing changes for a definition whose keys are all spelled once and correctly —
  including the legacy `uses:` grant for a persona that declares no `borrows:` at all.

Fix is one edit: spell the key the way the error line spells it, or delete the duplicate.
