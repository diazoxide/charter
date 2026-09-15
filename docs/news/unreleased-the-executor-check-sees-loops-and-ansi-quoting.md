---
version: unreleased
security: true
headline: The vault guard sees an executor inside a loop and past ANSI-C quoting, closing two reads a heredoc body could hide
---

**Affected: every release with the vault guard, through 0.61.1.** A quoted heredoc fed to a
reader is treated as data — dropped, not read — but only when no executor stands in its pipeline;
an executor (a `sh`, a `bash`, an `eval`) means the body is a script the guard must read. Two ways
the guard missed an executor let a `cat` of a vault inside a heredoc body be dropped as data and go
unrefused. Both were measured against real bash and zsh with a fabricated vault.

- The guard split a pipeline on the `;` and other separators inside a `while … do … done` loop
  (and `for`, `until`, `if`, `case`), so an `eval` or `sh -c` in the loop BODY was attributed to a
  different command than the heredoc it consumes. `cat <<'EOF' | while read l; do eval "$l"; done`
  ran the body as commands, but the guard read it as data.
- ANSI-C `$'…'` quoting desynced the tokenizer: the apostrophe in `$'\''` was read as opening a
  plain quoted string that swallowed a following `| sh`, so the executor after it was never seen.

The guard now keeps a loop's or conditional's body together when it holds an executor — the same
answer a `{ … }` group already gets — and rewrites `$'…'` to an ordinary quoted token before
reading the line, so the pipe after it stays a pipe. A vault read in a body those constructs run is
refused.

This only brings two spellings in line with one the guard already refuses: a pipeline with a plain
`| sh` whose body names a vault has always been refused, on the conservative rule that any executor
downstream keeps the body visible. A loop whose body runs no executor
(`while read l; do echo "$l"; done`) keeps its body as data and stays allowed.

Nothing to adopt; the guard updates with the plugin
([#1086](https://github.com/diazoxide/charter/issues/1086)).
