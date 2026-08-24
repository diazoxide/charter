---
version: unreleased
headline: The tool-gate stopped smoothing five things a `tools:` line never meant to grant
---

`tools:` in a persona pre-approves a **binary**, and every argument rides along with it.
That is the feature — an operator who writes `tools: gh` means `gh` — and it is also where
five holes lived, each one arriving under a name that made the declaration look narrower
than it was. All five are now cases where the gate declines to smooth and you get the
normal permission prompt. Nothing here denies anything; the gate still cannot block work,
only fail to remove a prompt.

**`charter secret` and `charter vault` no longer auto-approve under `tools: charter`.**
`_DANGEROUS` already carved `exec` out of `kubectl`; `charter secret exec` is the same verb
doing something strictly more sensitive, and `charter secret cp` and `charter secret get
--reveal` sit beside it. A persona declaring `tools: charter` — a shape `docs/personas.md`
teaches — ran all three with no prompt at all. Everything else charter does is untouched:
`charter persona list`, `charter workspace status`, `charter trace` still run without a
prompt for a persona that declared the binary, because that is what it was declared for.

**An interpreter or a wrapper is a declaration of every command, and is never smoothed.**
`bash`, `sh`, `python3` (and `python3.12`), `node`, `perl`, `ruby`, `env`, `xargs`, `sudo`,
`timeout`, `npx` and their relatives. `tools: python3` reads as *this persona writes
Python*. What it granted was `python3 -c "print(open('.charter/vaults/devops.json').read())"`
with an affirmative `allow`, which is worse than silence: it removed the human prompt that
was the last remaining control. If you want that, it is one prompt away, on purpose.

**A command whose arguments reach a vault is never smoothed, whatever the binary is.** The
Bash leak guard asks *is this program a reader?* — answerable for `cat`, hopeless for `curl
--data-binary @…`. This asks the other question, about the argv.

It asks it about the **file**, not about the text. Each argument is resolved and compared by
inode with the directory `charter` actually keeps its state in. So all of these are one
answer:
`@".charter"/vaults/x.json`, `.chart\er/vaults/x.json`, `.charte?/vaults/x.json`,
`--data-binary=@…`, a symlink pointing into the state directory, `.Charter/…` on a
case-insensitive filesystem, the bare directory `.charter` (which `tar` and `cp -R` are
happy to take), and a directory that merely contains it — `tar -cf /tmp/o.tar .` in the
plane root archives every vault without naming one. Because the question is put to
`charter.config` instead of to a hardcoded name, this now also holds on a plane with
`$CHARTER_HOME` set, on a plane still using the legacy `.edm/` directory, and for a vault
whose `file` the registry points outside the plane — three planes where the first version of
this check matched nothing at all. Charter's own state and the persona definitions carrying
`tools:` are in the same rule, for the reason below.

**And it only asks about the file if the argument it examines is the one the program gets.**
That turned out to be the whole game. `shlex` splits words and removes quotes; the shell
does brace expansion, tilde expansion, parameter and command substitution, ANSI-C quoting
and globbing *first*. So `cat .charte{r..r}/vaults/devops.json` arrived at every rule above
as the literal string `.charte{r..r}/…`, matched nothing, was answered `allow`, and bash
opened the vault. `$'\x2echarter/…'` did the same. So did `charter secre{t..t} get v
API_TOKEN --revea{l..l}`, which prints an unredacted credential into the transcript and was
an affirmative `allow` — worse than where this started.

The rule is now stated the other way round, as the characters the shell hands over
unchanged: **letters, digits, `_ - . / : , = + @ %`, quotes, a backslash, spaces and tabs,
and any non-ASCII character.** A command containing anything else is not smoothed. Two
earlier versions of this were lists of *dangerous* characters, and each was defeated by a
character that had never been on the list; an allowlist puts the burden on the spelling
nobody thought of rather than on the reviewer. That `shlex` and bash really do agree over
that alphabet is asserted against a real bash in the test suite, not asserted here.

The price, plainly: **`ls *`, `cat ~/notes.md`, `git commit -m "fix #12"` and
`kubectl get -o jsonpath={.items}` now take a normal prompt.** `ls *` in particular was
explicitly allowed before, on the reasoning that the shell never expands `*` onto a
dotfile — true, and beside the point: `charter secre*` becomes `charter secret` the moment
anything creates a file called `secret`, which an agent does with a plain Write.

`git clean` joined the destructive subcommands in the same pass. The state directory is
gitignored, so `git clean -xfd` deletes the session ceiling below while naming nothing —
and untracked work is unrecoverable, which is reason enough on its own.

**A tool added to `tools:` after the session started grants nothing until the next
session.** `persona.md` and the active-persona pointer are read from the working tree on
every hook call, and they are files the model can write — so one approved edit was
unprompted execution for the rest of the session, no restart and no commit. SessionStart
now records what every persona declares, and the gate answers within that set. Switching
persona mid-session still works: the snapshot holds the whole roster, not just the active
one. Narrowing a `tools:` line still takes effect immediately — both directions fail toward
fewer approvals.

Deleting or corrupting that record does not lift it. A snapshot that is present and
unparseable, or missing while the marker beside it remains, approves **nothing** for the
rest of that session — not even what was declared before it began, because charter no
longer knows what that was — and a new session takes a fresh snapshot in the ordinary way.
Removing the marker as well would put the session back at first use, and that is a command
reaching into the state directory, which is never smoothed: `rm -rf .charter`,
`rm -f .charter/sessions/*.tools` and `git clean -xfd` all take a prompt. The one place a
snapshot is still taken mid-life is a harness with no SessionStart hook at all (opencode),
on its first gated call and only if no snapshot was ever taken for that session.

The cost lands on the person who edits a `tools:` line by hand and watches it keep
prompting, so `charter persona use` now names it:

```
✓ Active persona set to 'devops' for this session and this pane.
  1 tool(s) declared since this session started: kubectl.
  Those still prompt HERE. The tool-gate answers within the set that existed at session
  start — `tools:` is read from a file this session can write, and freezing it is what
  stops an edit from becoming an unprompted command. A new session picks them up.
```

**Writing charter's own state with the Write/Edit tools is now denied.** `.charter/` holds
the active-persona pointer, the per-session pointers and that snapshot — three files that
decide what the gate will answer next — plus the vaults themselves. Reading them was already
denied; writing them was not, so the shape needed no Bash at all. Every charter command that
owns those files is unaffected: they write them directly, and this guard is on the agent's
tools. Editing `personas/<name>/persona.md` is *not* denied — asking for a persona charter
to be edited is ordinary work, and what made it dangerous was the re-reading, which is fixed
at the reading end.

**And one thing that was never enforced now says so.** `uses:`/`borrows:` reads as a
two-part grant — *read their vault, run their tools* — and only the tool half is code. Any
session can name any registered vault: `charter secret list <vault>` does not consult the
active persona, and nothing refuses it. That has not changed, because it cannot be honestly
enforced — every persona runs as the same user against the same files, and one of the paths
that would carry the check is how a credentialed MCP server is launched, with no tty and no
guarantee about which persona is active. So it is disclosed instead, in `docs/personas.md`
beside the `bin/` disclosure it reads like: vault reach is declared, not gated. The
generated sub-agent charter no longer claims *"You do NOT hold their credentials"* as a fact
about the world; it says not to, and says that it is a rule the agent keeps rather than a
wall charter holds.

Nothing to adopt: upgrading is the whole of it. If a persona in your plane declares an
interpreter, `charter`, or a shell, expect prompts where there were none — and expect them
too wherever a command carries a `*`, a `~`, a `{`, a `$` or a `#`, however harmless it
looks. That is the change, and `charter guard allow` is how you take a specific one back on
purpose.
