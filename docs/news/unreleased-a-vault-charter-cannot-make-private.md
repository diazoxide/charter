---
version: unreleased
headline: A plain-file vault is 0600 before the plaintext goes in, not after
---

`charter secret set` on a plain-file vault opened it `O_CREAT|O_TRUNC, 0o600` and chmod-ed
it to 0600 once the write returned. The comment above that line said the plaintext "is
never briefly world-readable", and it was wrong about its own mechanism: the mode argument
to `open(2)` applies **only when the call creates the inode**. For a vault someone had
hand-authored — or restored from a backup, or copied off another machine, all of which
land at the umask default — it was ignored outright. Measured on a 0644 vault: mode while
the plaintext was on disk, 0644. Mode afterwards, 0600. Same inode throughout.

The window is milliseconds and it takes an already-loose file to open it, so this is a
small bug. The confident sentence explaining why it could not happen is the larger one.

The order is now inverted, and the mode is settled on the **descriptor** rather than the
path. charter opens the file without truncating it, `fchmod`s that descriptor, `fstat`s
the same descriptor to read the mode back, and only then truncates and writes. Reading it
back matters: a chmod that returns successfully is not evidence the bits moved — a mount
with fixed permissions, exFAT or many network shares, accepts the call and reports the old
mode. Working on the descriptor rather than the name matters for the usual reason: it is
the object being written, not whatever the path happens to point at by the time the write
lands.

**If the file still cannot be made private, charter writes nothing and says so.** The
previous contents survive, and the error names the file and why. The alternative — warn
and write anyway — leaves the credential world-readable with the warning scrolled off the
top of somebody's log, which is how this class of thing survives in the first place. A
plain-file vault on a filesystem that cannot hold a mode was never storing a private file;
now it says so instead of pretending.

**Every directory the vault writers create** on the way to a vault file is 0700 rather
than the umask default — not just the last one. The first cut of this said
`mkdir(parents=True, mode=0o700)` and that is the same bug one level up: `pathlib` applies
`mode` to the **leaf only**, and creates the missing parents at `0o777 & ~umask`. Measured
on a plane where charter created every level itself, for a vault at
`.charter/vaults/team/prod.json`: `.charter` 0755, `.charter/vaults` 0755,
`.charter/vaults/team` 0700. The leaf was private and the directory the fix was *about* —
the one holding the vault names — was world-listable. Each level is now created and
chmod-ed individually.

**Known remaining case: `.charter/` itself, when something else creates it first.** The
0700 walk lives in the secrets writers, and on the default flow they are not what creates
the state directory — `charter vault add` writes the local registry first, and that write
makes `.charter/` with no mode of its own. So under `umask 022` the state directory is
0755 and under `umask 077` it is 0700: the umask decides that one level, and the fix above
decides every level below it. `.charter/vaults/`, the directory that lists vault names, is
0700 either way, and `charter vault list` names the loose `.charter` on the health line
like any other. Filed as [#470](https://github.com/diazoxide/charter/issues/470) rather
than fixed here, because moving state-directory creation onto this walk touches the
registry, persona and workspace writers and none of those are this entry's subject.

**A directory that was already there keeps its mode, and charter says so instead of
fixing it.** A `.charter/vaults/` that predates this, or one made by `mkdir -p` at the
umask default, is 0755 before `secret set` and 0755 after. That is deliberate: a vault's
`file` can name any path on the machine, so "tighten whatever directory we land in" is how
charter would come to chmod a home directory or a shared team directory unprompted, with
nobody watching — the defect #331 was filed about. What charter does instead is name it,
on the vault's health line — which is the `STATUS` column of `charter vault list`:
`listed by other accounts: .charter/vaults 755 (want 700)`. The remedy is one `chmod 700`,
run by you.

That line reaches `charter vault list` and nothing else. `charter doctor` asks each vault
whether it is *reachable* and drops the rest of the health detail, so a loose directory
appears in neither `doctor` nor `doctor --json`. Getting it onto doctor's green line is
[#471](https://github.com/diazoxide/charter/issues/471); this entry no longer claims it is
already there.

The rotation sidecar goes through the same writer.

None of this is about the plaintext. A plain-file vault stores values in the clear, that
is documented in four places, and it remains the accepted trade-off for the provider whose
entire job is "a JSON file you can also edit by hand". The fix is to the mode, and to the
docstring that was sure the mode was already fine.
