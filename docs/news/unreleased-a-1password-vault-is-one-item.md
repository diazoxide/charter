---
version: unreleased
headline: The docs now describe the 1Password layout charter actually uses
---

A `1password` vault is **one 1Password item whose concealed fields are the secrets**. It
has been that for several releases. `docs/secrets.md` went on describing the layout it
replaced — one item per secret, `charter-<vault>-<KEY>`, the value in that item's
`password` field — and `charter vault add --provider 1password` said the same thing in its
own output, at the one moment you are told where your credentials are about to live.

```
$ charter vault add devops --provider 1password --op-vault Engineering
  ✓ Vault 'devops' registered (provider: 1password) [local only].
  • charter creates one 1Password item per secret, tagged 'charter:devops', in vault 'Engineering'.
```

So you go looking in 1Password for `charter-devops-KUBECONFIG`, and there is no such item.
There is `charter-devops`, with a concealed field called `KUBECONFIG`. The registration
hint now names it:

```
  • charter keeps this vault in one 1Password item, 'charter-devops' in vault
    'Engineering', tagged 'charter:devops' — each secret a concealed field of it.
```

Point `--op-item` at an item you already curate and the hint names *that* item, since the
`charter-<vault>` default is not what will be written.

The doc did more than lag. It carried a paragraph arguing that one item per vault "is
wrong here", on the grounds that `op item get --format json` conceals values and a
read-modify-write would therefore write masks back over every sibling secret. That hazard
is real; it is why `secret set` and `secret rm` fetch with `--reveal` and why nothing else
does. It is the reason for a flag, not a reason against the schema — and anyone reasoning
about the provider from that page started from the opposite of the design.

Two smaller claims on the same page were stale for the same reason and are corrected:
`secret list` reads the fields of your vault's item rather than filtering 1Password by the
`charter:<vault>` tag, and `charter secret rm` removes a field — the item is the vault, and
charter never deletes it.

## What to do about it

**Nothing, unless you registered a 1Password vault long enough ago to be on the old
layout.** If you did, those `charter-<vault>-<KEY>` items are still in 1Password with your
credentials in them, and charter does not read them. It will not call such a vault
healthy-and-empty either — `charter vault list` counts the leftovers and says so, and
`charter doctor` marks the vault unhealthy. Re-register each credential with `charter
secret set`, then delete the old items.

No behaviour changed here beyond the sentence `vault add` prints. What was wrong was what
charter told you about itself.
