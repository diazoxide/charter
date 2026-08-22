---
version: unreleased
headline: A 1Password vault charter could not read is no longer reported as empty
---

A vault that could not be read used to look exactly like a vault with nothing in it. It no
longer does.

`op item get` exits non-zero for *there is no item yet* and for every way a read can
fail — a rate limit, an expired session, a token that cannot see the vault, the wrong
`op-vault` name — and charter treated all of it as "no secrets here". So the failure
arrived as a benign state:

```
$ charter secret list devops
  • Vault 'devops' has no secrets.
$ echo $?
0
```

The vault was populated throughout. Absence is now **proven** rather than assumed: charter
asks the vault's own identity to list the vault, and only a listing that succeeds *without*
the item proves there is no item. A listing that fails is itself the answer, and it is
reported.

**The same swallow ran the write path, which is the expensive half.** `charter secret set`
is a read-modify-write, so a masked read meant the template piped back to 1Password held
only the key being written — every sibling secret in that item dropped, from a read that
had merely been rate-limited. Not mis-reported: gone from the current version of the item.
The write path had two more of these, so a failure landing mid-write could also renumber
the fields of an item you curated by hand, or create a second item with the same title and
leave the vault ambiguous to `op item get`. All of them now stop and report.

**`charter vault list` was already telling you the truth about these same vaults**, because
it reached a code path that raises while `secret list` reached one that did not. If the two
have ever disagreed for you — `vault list` unhappy about a vault `secret list` called
empty — that disagreement was this bug, and it is gone.

## What to do about it

**If charter now reports a read failure where it used to print "no secrets", nothing has
broken.** You are seeing a failure that was always happening. The message names the cause
where charter recognises it — a rate limit says so explicitly, and says the contents are
unknown. Wait and retry rather than re-provisioning credentials that are almost certainly
still there. That re-provisioning is the expensive mistake this prevents: it cost the
operator who reported it an hour and a rotated token that had been fine all along.

**Scripts that check an exit code will notice.** A masked read exited 0; a reported failure
exits non-zero. That is the intended change — a command that could not answer no longer
claims it did — but if you have automation branching on `charter secret list`, this is the
release where a broken 1Password read starts failing it instead of silently returning
nothing.

**If you ran `charter secret set` against a 1Password vault while its reads were failing,
check that item's history.** 1Password keeps previous versions, and a write made through a
masked read may have replaced the item with one holding only the key you set. The values
are recoverable there; charter cannot recover them for you.

Vaults on the `plain-file` and `op://` reference providers are unaffected — this was in the
native 1Password provider, the one where charter owns the item.
