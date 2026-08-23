---
version: unreleased
headline: A 1Password write asks op for the item once instead of three times
---

`charter secret set` on a 1Password vault used to make **three** `op item get` calls, and
`charter secret rm` two. All of them fetched the same document. Traced on 0.50.1:

```
set() op calls, in order:
  0: item get --reveal      <- the values
  1: item get               <- does the item exist?
  2: item get               <- what are the field ids?
  3: item edit
  4: read                   <- the read-back
```

Call 0 already fetched everything the other two needed — it is the same `op item get`, plus
`--reveal`. They are now one read.

**Why it was worth doing.** Rate limiting is what #322 and #354 were both reported from, and
three calls where one would do is three chances to be limited on the path where being limited
costs the most: a write that stops halfway leaves the operator unsure whether the credential
landed.

**And the answers could disagree.** Three fetches are three separate snapshots, so the item
description charter piped back could pair one snapshot's values with another's field ids —
describing no instant that ever existed. Two ways that showed up, neither needing a failure
anywhere:

- Someone renames a field in the 1Password UI while a write is running. The ids come back
  keyed by the new label, the values by the old one, so charter finds no id for the field it
  is writing and mints a fresh one — silently renumbering a field on an item it does not own.
  That is the mutation #354 was filed for, reached with every `op` call exiting 0.
- Another writer creates the item between the read that proved it absent and the read that
  asked whether it exists. charter got *yes*, switched from `item create` to `item edit`, and
  an edit **replaces**: the template held only the key being written, so the other writer's
  secret was gone and `set` returned success. It now creates, which against a title that is
  now taken fails loudly on the read-back instead of destroying a credential quietly.

**What that second one trades away, said plainly.** One read cannot close that race, only
move where it lands. The write is now always `op item create`, so the outcome is a **duplicate
item** — two 1Password items sharing a title, after which `op item get <title>` is ambiguous
until a human deletes one. #354 called the duplicate item the worst outcome of its own set,
and this change routes more of the race into it on purpose: an ambiguous title is repaired by
hand, from history 1Password still keeps, whereas a replaced secret is simply gone and was
reported as success. What is still wrong is the sentence you get when it happens — the
read-back cannot resolve an ambiguous title, so `secret set` says *no secret 'KEY' in vault
'devops'* about a credential it has just written. That is issue #399, not fixed here.

Reads are unchanged. `charter secret get` still fetches one field through `op read` and never
the whole item, and `vault list` and `doctor` still never ask for a value — the shared read
takes `--reveal` only on the write path, so routine status cannot trigger a re-auth prompt.

Nothing to adopt: upgrading is the whole of it.
