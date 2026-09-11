---
version: unreleased
headline: A heredoc body is read by whoever runs it — a reader in front of a chained shell no longer hides that shell's `<<` from the vault guard
security: true
---

**Affected: every release carrying the heredoc pre-pass. Fixed here.** A file-reading
program at the *start* of a line let a shell later on the same line run a vault read the
guard never saw. `cat x && bash <<'EOF'`, `cat x | bash`, `cat x || bash`, `cat x & bash`
and `cat x; bash <<'EOF'` — each with `cat .charter/vaults/<v>.json` as the heredoc body —
were allowed through, because the pre-pass that strips a reader's heredoc asked only whether
the *line began* with a reader and then dropped the body of whatever `<<` it found. The
shell, not the reader, was opening that heredoc, and its body is a script.

**One shape, and it is the one this project keeps paying for: a guard matching where a token
*sits* instead of what the shell *does* with it.** The remedy is the same in kind as the
sibling fixes ([#474](https://github.com/diazoxide/charter/issues/474),
[#547](https://github.com/diazoxide/charter/issues/547),
[#556](https://github.com/diazoxide/charter/issues/556)): attribute the thing to the command
that owns it. Each heredoc is now attributed to the command that opens it — the program of
the segment holding the `<<`, and the pipeline that segment belongs to — and its body is
dropped only when a **quoted** heredoc feeds a **reader** whose pipeline runs **no executor**.
A body reaching a shell (`bash`, `sh`, `python3`, …), or a reader whose output pipes into one
(`cat <<'EOF' | bash`), stays visible, and the guard reads each of its lines as the command
it is. Bodies follow their `<<` in order across the whole line, so two heredocs going to
different programs are told apart (`cat <<'A' && bash <<'B'` drops A, keeps B).

The false positive [#258](https://github.com/diazoxide/charter/issues/258) removed stays
removed: a quoted `cat <<'X'` whose body merely names `.charter/` is stdin data and is not
read as a read of it. An *unquoted* `<<X` body now stays visible instead, because the shell
expands it first and a `$( … )` in it runs. Three spellings that the old regex mistook for a
heredoc and over-stripped — a here-string `<<<x`, a `<<` inside quotes, a `<<` in a comment —
now keep the following line in view as well.

The documented ceiling is unchanged: this reads argv and heredoc *structure*, never an
interpreter body or a shell string. `sh -c '…'`, `bash -c`, `eval`, and a body a command
turns into arguments rather than code (`… | xargs cat`) remain outside it, and a body written
to a file and run later (`cat <<'EOF' > x.sh && bash x.sh`) is beyond an argv guard by
construction — see `docs/hooks.md`, *Where the secret-leak guard stops*.

Nothing to adopt; the guard updates with the plugin
([#973](https://github.com/diazoxide/charter/issues/973)).
