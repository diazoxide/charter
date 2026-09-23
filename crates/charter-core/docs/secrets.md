# Secrets and the vault directory

**`charter vault` and `charter secret` are not in this version yet**, and neither is
`charter persona secret`. There is no command here that stores, lists, injects or prints a
secret, and `charter doctor`'s vaults row says it does not check them yet.

What this version does have is the part that keeps a credential **out of an agent's
transcript**: the guards that refuse to print a vault file into the conversation, and the
checks that refuse to let a credential be committed with a memory. Every message an agent
reads and every tool result it gets can end up in a transcript — saved, logged, reviewed, or
fed back into a later prompt — so the transcript is not a safe place for a value, whatever
holds it on disk.

## Where a vault lives

A plane keeps its state under `.charter/`, and `charter init` gitignores the whole of
`/.charter/`, which is what keeps a vault out of a commit. Inside it the guards treat these
entries as secret, because their **content** is the secret:

- `.charter/vaults/` — the vault directory and every file under it;
- `.charter/browser…`, `.charter/active-…` and `.charter/fingerprint…` — a browser profile,
  the active-persona marker and the fingerprint key;
- `.charter/` itself, named as a whole.

`.charter/vaults.json` — the registry, which holds provider settings and file paths, never a
value — is an ordinary file and is not refused.

A file in `.charter/vaults/` is plaintext on disk. The guards below are about the
conversation, not about encryption at rest: anyone who can read your account's files can
read that directory.

## What the guards refuse

All of these answer in `charter hook pretooluse`, which the app's plugin runs before each
tool call. [hooks.md](hooks.md) has the whole list and its limits.

- **A reader pointed at a vault path.** `cat`, `less`, `more`, `head`, `tail`, `bat`, `nl`,
  `tac`, `xxd`, `od`, `strings`, `grep`, `rg`, `ag`, `awk` and `sed`, with a guarded path
  as an operand or an input redirection (`< <vault> tee`), behind any wrapper (`env`,
  `sudo`, `{ …; }`, `if …; then …; fi`), after a relocation however it is spelled (`cd`,
  `pushd`, `env -C`, `sudo --chdir`), and on any line of a multi-line command. A wrapper that
  opens a file itself (`xargs -a <vault>`) is a read of that file.
- **The harness's own file tools.** `Read` or `Grep` on a guarded path is refused on the same
  predicate as the shell route — the two do not differ on any spelling.
- **A walk that reaches the vault directory.** `grep -rn TOKEN .` from the plane root names
  no vault file and prints every one of them. The guard resolves the operand against the
  shell's directory and asks whether the walk would reach `.charter/`'s guarded entries; it
  fires only when they exist and hold something. The denial names the fix —
  `grep -rn --exclude-dir=.charter …` or `rg --glob '!.charter' …`. A `Grep` with no path
  walks the directory it stands in and is judged the same way.
- **`--reveal`** on a charter invocation it can recognise.
- **A write into `.charter/`** with the `Write` or `Edit` tool, inside a plane — that
  directory decides which commands run without a prompt.

Path spellings are folded before they are compared: `.charter//vaults/`,
`.charter/./vaults/` and `.CHARTER/vaults/` are the same file to the guard, as they are to
the filesystem on macOS. `\` is not folded, because on POSIX it is an ordinary filename
character.

**A denial is the guard working, not a bug.** Every denial says so, and says there is no
switch that lifts it; see [hooks.md](hooks.md) → *When a guard is wrong*.

## Where the guard stops

It matches a **known program name** against a **path spelled in the command line, before
any shell runs**. Everything outside that sentence gets through, and it is stated here so
you do not have to discover it:

- *The name.* A program that is not on the reader list runs: an interpreter
  (`python3 -c "print(open('.charter/vaults/db.json').read())"`), `base64`, `cp`, `jq`,
  `cut`, `dd`, `git show`, and a shell string (`sh -c 'cat …'`), which reaches the guard as
  one opaque argument.
- *The path.* A different path holding the same bytes — a symlink, a copy, a vault file kept
  outside `.charter/` — is an ordinary file to every guard.
- *The walk.* Only programs known to walk directories are asked where they go, so
  `find . -type f -exec cat {} +`, `tar cf - .` and an interpreter reach the same files.
- *The shell.* Every expansion is a read the guard does not see: a glob inside the vault
  path (`cat .charter/vault?/db.json`), a variable (`V=…; cat $V`), a quoted
  `"$(cat …)"`, brace or tilde expansion. A glob only escapes when it falls inside the
  guarded part of the path, so `cat .charter/vaults/*.json` is still refused.

It is a guard against mistakes, not against someone deliberately spelling around it.

## A credential in committed text

A plane commits and pushes its memory, so a secret written into one is disclosed the moment
it lands. Two checks look for a credential's **shape** — a JWT, an AWS access key, a
`token: <value>` assignment — and name the kind, never the matched text:

- after a memory or ref file is written, the chat is told to remove it;
- `charter save` refuses to commit a staged memory or ref file that matches.

A vault **reference** such as `token: "vault:forge/gh"` names where a secret lives and is not
a credential; both checks let it through.

Separately, a forge command that publishes prose (`gh issue create --body "…"`) and charter's
own text-taking commands (`persona remember`, `workspace remember|note|todo|vision`) refuse a
live command substitution in their text, because inside double quotes a backtick or `$(…)`
runs and can carry your whole environment into a public page. Write such text with
`--body-file -` and a **quoted** heredoc (`<<'BODY'`). [hooks.md](hooks.md) has the scope.
