---
version: unreleased
headline: The vault guard now reads the whole command, every spelling of a path, and the file `secret cp` wrote
---

Fourteen ways past the secret-leak guard, all found by the same question: not *can the rule be
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

**A segment boundary is an operator the shell *interprets*, and the tokenizer had thrown
that away.** The group forms above put `(`, `)`, `{` and `}` on the operator list, which is
a list of *strings* — and posix `shlex` hands back the identical one-character token `)` for
a literal `\)`, a quoted `')'` and a real subshell close. So `cat \) .charter/vaults/x.json`
segmented into `cat` and `.charter/vaults/x.json`: the reader lost its operand, the operand
lost its reader, and the hook allowed a command that prints a vault. `cat '(' …`,
`cat "{" …`, `cd .charter/vaults && cat \) x.json` and `charter secret get v k \) --reveal`
were the same one word. This is the wrapper bug one layer down — matching an operator's
*text* the way the old code matched a program by *name* — so the fix is the same shape:
`_ShellLexer` keeps the one thing the tokenizer knows and used to discard, which token was
quoted, and only an unquoted token can be a boundary. The multi-character operators needed
the same answer a second time: the routine that breaks a glued run like `);` back into
separate tokens was splitting quoted runs too, so `cat '&&' <vault>` and `cat '();' <vault>`
walked past a first draft of this very fix. Only an uninterpreted run is broken up now.

Asking what else the tokenizer disagreed with a shell about found two more, both live on
`main` as well. **A newline is a command separator** and `shlex` counts it as whitespace, so
a multi-line Bash call — most of them — collapsed into one segment and every command after
the first line was invisible to every guard here; the operator list had always *contained*
`"\n"` and never received the token. And **`#` begins a comment only where a word begins**,
while `shlex` honoured it mid-word and discarded the rest of the line, so `echo hi#; cat
<vault>` — which runs the `cat` in bash — arrived as a lone `echo hi`. Both are fixed at the
lexer, and `bash <<'EOF'` bodies are now read as the scripts they are.

**A wrapper usually does not change what the program is — `xargs -a` does.** `xargs` was
added to the wrapper list so `xargs cat <vault>` would be seen, and the entry for its own
flags listed `-a`/`--arg-file` among the values to skip. But that value is a file `xargs`
itself opens: `xargs -a .charter/vaults/x.json echo` prints the vault, and the only program
named on the line is `echo`. The same table listed `-e`/`--eof`, whose value in GNU `xargs`
is *attached* and optional, so `xargs -e cat <vault>` handed `cat` to the flag and made the
vault path the program. A wrapper's own file-reading flag is now checked as a read whatever
the wrapper wraps, and `-e` no longer eats the program.

**A redirection is the shell's own file plumbing — not the program, not an operand, and not
a command boundary.** Two more went through there. The `&` in `>&` was being read as the
control operator `&`, because a glued punctuation run was cut into operator *characters*
rather than into the tokens a shell would read, so `cat 2>&1 .charter/vaults/x.json` split
at that `&` and the vault path landed in a segment of its own — a command `main` denies and
this branch briefly allowed. And a redirection may sit in *front* of the command: `<
.charter/vaults/x.json cat` prints the vault while token 0 is `<`, so nothing was named as
the program and the path was nobody's operand. The target of an input redirection is now a
read wherever it appears and whatever follows it — the shell performs that open before the
program is execed, which is why `tee < <vault>`, whose program is in no reader list, leaked
too. Both of these are live on `main`.

**And the read guard swallowed its own refusal.** `pretooluse_read` wrapped its whole body
in `except Exception: return 0`, including the `_deny` call — so a `BrokenPipeError` out of
the denial's own `print` returned an allow. Its Bash sibling has no such wrapper, which left
the two vault guards failing in opposite directions. The parsing may fail open; the verdict
may not. Only the parse is inside the handler now.

Nothing to adopt: upgrading is the whole of it. If you have used `secret cp`, the first
`cp` after upgrading starts the ledger — files materialized before then are not in it, and
re-running the `cp` records them.

Found in the 2026-08-24 security audit (#429, #430, #431, #423, #438); the rest in three
rounds of adversarial review of the fix for them, each round finding the previous round's
answer written one spelling wider.
