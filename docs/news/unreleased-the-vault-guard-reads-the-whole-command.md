---
version: unreleased
headline: The vault guard now reads the whole command, every spelling of a path, and the file `secret cp` wrote
---

Seven ways past the secret-leak guard, all found by the same question: not *can the rule be
outsmarted*, but *what does it actually match on, and what is the equivalent action it
misses*. Each one below was run against the shipped hook with a fabricated vault, and each
printed the value.

**A broken quote hid every command after the first.** `_segment_argv` could not tokenize
`echo $'it\'s fine' ; cat .charter/vaults/x.json` — valid bash, and `shlex` refuses it — and
its fallback returned the whole string as ONE whitespace-split segment. Every guard in
`hooks.py` reads token 0 as the program, so one segment means one program: the `echo` was
the entire command and the `cat` after the `;` did not exist. The same four characters
flipped `git clone git@…`, `git commit -S`, `GIT_SSH_COMMAND=… git fetch` and an unattended
`git tag` from deny to allow. The function's own docstring claimed this path kept the leak
guard *fail-closed*; it did the exact opposite, and the regression test that named the
property used a fixture with the offending program at token 0 — the one arrangement where
the collapse is harmless, so it passed green for as long as the property was false.

The fallback now segments on the operators too, including operators glued to their
neighbours (`echo 'x;cat …` is two commands to a shell and one token to `str.split`), and
the leak guard additionally scans the raw string when parsing failed. The plane-root branch
guard opts out in one explicit line and keeps failing open — it is the guard whose failure
mode is annoyance, and a phantom `git checkout` conjured out of a broken quote must not stop
a turn.

**One word in front of a reader made it invisible.** `env cat <vault>`, `command cat`,
`sudo cat`, `time`, `nohup`, `exec`, `xargs`, `{ cat …; }`, `( cat … )`, `echo $(cat …)`,
`if true; then cat …; fi` — all allowed, all printing the file, while the bare `cat` was
denied. `prog` came from token 0 and a wrapper is token 0. `_split_env` now strips the
wrapper run (and the wrapper's own flags, per wrapper, because `env -i` takes no value while
`xargs -I` does) before naming the program, so all four guards get the same answer from the
same place. That closed the sharper half of the same bug at once: `env GIT_SSH_COMMAND=/tmp/k
git push` and `/usr/bin/env git push git@github.com:o/r.git` walked past the one-credential
rule, and unlike `--reveal` nothing downstream re-checks an SSH transport override.

`sh -c '<string>'` is still out of scope and now says so in a test. A wrapper runs its own
argv; a shell runs a string, and re-parsing strings is a different guard.

**A substitution is both a command and a word, and reading it as only one is a bypass
either way.** The first pass at the group forms above made `(` and `)` plain segment
boundaries. That covered `echo $(cat <vault>)` — where the substitution is the reader — and
opened `cat $(echo <vault>)`, where the substitution is the *operand*: the reader lost its
argument, the argument lost its reader, and neither half named a guarded path.
`git push $(echo git@host:o/r.git)`, `head $(ls .charter/vaults/*.json)` and an unattended
`git tag $(cat VERSION)` all went from deny to allow with it. A `$( … )` run now yields an
additional INNER segment while the enclosing segment keeps accumulating, so both readings
exist at once. Backticks are normalised to `$( … )` before tokenizing — the same construct,
older punctuation — process substitution `<( … )` is handled the same way, and where a
substitution's output is spliced into a longer word (`cat $(echo .charter)/vaults/x.json`)
the neighbouring operands are re-joined before matching.

**Relocation was followed when it was spelled `cd`, and only then.** Teaching the leak guard
to follow `cd .charter/vaults && cat x.json` came in the same commit as a table of wrapper
flags whose values get skipped — including `env -C` and `sudo -D`, whose value *is* the new
directory. The flag was read purely in order to throw its value away, so
`env -C .charter/vaults cat x.json` named nothing guarded anywhere and was allowed, and so
was `pushd .charter/vaults && cat x.json`, since only `cd` was on the list. One reading of
the flag now answers both questions, `pushd` counts as `cd`, and a wrapper chdir applies to
its own program without moving the shell for the segments after it.

**`.charter//vaults/`, `.charter/./vaults/` and `.CHARTER/vaults/` are one file to the
filesystem, and were three to the guard.** The path pattern was a literal, case-sensitive
substring match. The first two were allowed through Bash; the third was allowed through
*both* guards, because macOS opens `.Charter/vaults/x.json` and the regex did not. One
predicate now answers for the Bash guard and the Read/Grep guard — the module has argued for
a while that those two must never disagree, while each open-coded the match. The program
name is folded the same way, since `CHARTER secret get --reveal` runs the same binary — in
*every* guard: the one-credential rule and the unattended release floor each kept their own
unfolded copy of "what program is this", so `GIT push git@host:o/r.git` and `GIT tag v1`
were still allowed after the fold landed everywhere else. There is one spelling of that
question now.

**`secret cp` wrote plaintext where no guard was looking, and the denial pointed at it.**
`charter secret cp v k /tmp/x && cat /tmp/x` was a two-command, fully in-policy read of any
vault value, and the `--reveal` refusal said *"Use `charter … secret exec`/`cp`"* — the
documented remedy was the bypass. Every destination `secret cp` writes is now recorded in
`.charter/materialized.json` (0600; paths and key names, never values), and both guards
refuse to read a file in it, by absolute path or relative to where the session stands.
Materializing a credential for a tool that insists on a file is still supported and still
the right move; reading it back into the transcript is not. The denial text no longer names
`cp` as an alternative to anything.

**And the read guard swallowed its own refusal.** `pretooluse_read` wrapped its whole body
in `except Exception: return 0`, including the `_deny` call — so a `BrokenPipeError` out of
the denial's own `print` returned an allow. Its Bash sibling has no such wrapper, which left
the two vault guards failing in opposite directions. The parsing may fail open; the verdict
may not. Only the parse is inside the handler now.

Nothing to adopt: upgrading is the whole of it. If you have used `secret cp`, the first
`cp` after upgrading starts the ledger — files materialized before then are not in it, and
re-running the `cp` records them.

Found in the 2026-08-24 security audit (#429, #430, #431, #423, #438), and the last
two in the adversarial review of the fix for them.
