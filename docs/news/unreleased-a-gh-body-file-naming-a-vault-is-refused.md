---
version: unreleased
security: true
headline: `gh pr create -F <vault path>` is now refused — it read the vault and uploaded it to the forge
---

**Affected: every release with the vault guard, through 0.61.1.** The Bash vault guard refuses
a command that reads a vault file directly, but only for programs it knows print files (`cat`,
`less`, `grep` and a handful more). `gh` is not one of them, so a `gh` command that reads a
file and sends it to the forge slipped through. `gh pr create -F .charter/vaults/x.json` read
the vault and published its contents as a pull request body. That is worse than printing the
value into the transcript, because it leaves the credential on the forge.

The guard now checks the file operand of gh's body, notes and template flags the same way it
checks a reader's operand: `-F`/`--body-file`, `--notes-file`, `-T`/`--template` across
`gh pr`, `gh issue` and `gh release`, and `gh api --input <path>` and `--field key=@<path>`. A
vault path there is refused with the vault-read reason; the remedy the message names —
`charter … secret exec` — is the way to hand a credential to a command without it entering the
transcript or the forge.

Stdin stays data: `-F -` (and `gh api`'s `@-`) is the body a heredoc feeds gh, which 0.61.1
already reads as text. An ordinary body file (`-F notes.md`) is untouched, and so is
`gh api -f/--raw-field key=@path`, whose `@path` is a literal string gh does not open.

One related exfiltration is not covered here and is left as a separate finding: a release ASSET
named positionally, `gh release create v1 <path>`, is also uploaded. The flags above are the
ones this change reads.

Nothing to adopt; the guard updates with the plugin
([#1086](https://github.com/diazoxide/charter/issues/1086)).
