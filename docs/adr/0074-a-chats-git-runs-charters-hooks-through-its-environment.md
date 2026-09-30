# A chat's git runs charter's hooks through its environment

**Accepted 2026-10-01** by the operator (ruling V26), drafted for program-map ticket SQ-16 (charter#592) from the
`/code-review` of charter#770. It adds a line to
[ADR 0051](0051-the-app-saves-the-plane-and-each-repo-by-a-mode-the-project-declares.md)'s checks
and relies on [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) for
what it cannot hold itself.

## Where charter is today

A plane save and a workspace repo's save scan what they stage (ADR 0051). Neither sees a commit
an agent makes itself, with `git commit` in its own shell. Most of an agent's commits are made
that way, in a workspace repo, in a piece, or in a clone it made outside any plane. SQ-16 asks
that such a commit be scanned before it is made, in any repository, and that a finding block the
commit and become a needs-you item.

## The decision

**Every chat the app starts on a harness has its git run charter's hooks, through one config
pair in the chat's environment.** git's documented `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>` and
`GIT_CONFIG_VALUE_<n>` set `core.hooksPath` to a directory the app writes at every launch,
`<app data>/git-hooks/` (`charter_core::githooks`).

- **Added after, never over.** The chat's environment is settled first, including a pair the
  operator passes through `[chat_env] pass` or a profile sets. charter's pair is the next index,
  and the count goes up by one. Each variable appears once in what the chat is started with.
- **Two hooks check.** `pre-commit` and `pre-merge-commit` run `charter git-hook <name>`, which
  reads the added lines of the staged diff (through the index git hands the hook, so `commit -a`
  and `commit <paths>` are scanned as what they commit). It uses `secretshape::leaks`: the repo
  save's token rules, gitleaks' prefix-anchored vendor rules, and personal data (email
  addresses, card numbers that pass the Luhn check, US social security numbers). A finding
  refuses the commit, names each hit by path, line and kind with the value masked, and puts the
  chat in the needs-you queue over the hook socket (`CommitRefused`, ADR 0068 §5). A file name
  git writes that cannot be read also refuses the commit, because a file nobody read is not a
  file that was scanned.
- **The repository's own hooks still run.** `core.hooksPath` replaces a repository's hooks
  directory, so charter writes a shim for each hook a repository is likely to have. Each shim
  runs the repository's own hook of that name after charter's check, with git's arguments,
  standard streams and exit status. The repository's own directory is git's answer
  (`git rev-parse --git-path hooks`) with charter's pair renamed out of the way for that one
  call, so a repository's `core.hooksPath` (husky, pre-commit) and `.git/hooks` (git-lfs) are
  both honoured. When git cannot answer, the shim says so and fails, rather than skipping the
  repository's hook without a word.
- **Three hooks get no shim:** `post-index-change`, `reference-transaction` and
  `fsmonitor-watchman`. The first two run on every index write and every ref update (seven of
  the eleven hook runs in one `git commit`), and each shim run costs a shell and a `git`.
  `fsmonitor-watchman` is named by `core.fsmonitor`'s path in practice, not looked up in the
  hooks directory. **A repository's own hook of one of these three names does not run in a
  chat.**
- **The Bash guard refuses the skips it can see** (`charter_core::commitguard`, ungated):
  `git commit --no-verify` and `-n` (alone or bundled), `git merge --no-verify`, `git -c
  core.hooksPath=…` and `--config-env=core.hooksPath=…`, and `GIT_CONFIG_*` set in front of a
  command or exported on the line. The pre-commit refusal itself says *"Do not use --no-verify;
  the operator has been told."*

## Why the environment

- **A hook in `.git/hooks`** is per repository. charter would have to write into every clone,
  and would still miss the clone an agent makes itself. It would also collide with husky and
  pre-commit, which own that directory or move it.
- **A `PreToolUse` guard reading `git commit` off a command line** runs before the command. In
  `git add -A && git commit`, nothing is staged yet when the guard runs, so it would have to guess
  what will be committed. It also sees only the harness's own tool calls, not a script's.
- **The environment** follows the chat into any repository and any process it starts, and is the
  same for every harness. It writes nothing into anyone's repository. git itself runs the hook at
  the moment of the commit, against the index being committed.

## What it holds, and what it does not

**A mistake guard, not a boundary.** An agent that means to can still commit unscanned: through
`sh -c`, a script, or an alias in the repository's config that the Bash guard cannot read. It
can also set a variable earlier in its session. The same holds for every charter guard. Holding
a chat to its hooks is the sandbox's work (ADR 0067). That is a follow-up there: for example,
denying a chat's writes to `core.hooksPath` and its environment. It is not something this
decision can promise.

**Not armed:** a harness the operator starts by hand in a shell tab (ADR 0062), a chat started
outside the app, and any chat off unix (the shims are POSIX shell scripts).

**Not scanned:** `git cherry-pick`, `git rebase` and `git am` do not run `pre-commit`, so a
commit they make is not scanned.

**Rule coverage:** of ID numbers, only US social security numbers; of gitleaks' rules, a curated
set anchored on vendor prefixes. International ID numbers, IBAN, and the rest of the anchored
gitleaks rules are charter#773. The allowlist is SQ-17's, below.

**The sandbox:** a Claude Code chat's compiled sandbox denies reads only of vault storage.
The app data directory is outside the chat's own directory, so the chat can read and run the
hooks and cannot rewrite them. A test pins that no denied path covers the directory.

## The allowlist (amended 2026-10-01, SQ-17)

A finding that is not a leak is let through by an entry. Entries come from two places:

- **charter's own, the same in every repository:** an email address in `Cargo.toml`,
  `package.json`, `.mailmap`, `AUTHORS*`, `CONTRIBUTORS*` or `CHANGELOG*`, anywhere in the
  tree. These files publish names and addresses on purpose. In a plane, an email address in a
  memory file is also let through, because the plane's own save scans memory by its own rules.
- **The repository's `.charter-scan-allow.toml`**, committed and reviewed like code. Each
  entry names a rule by its id and the paths it covers, or names one value by its SHA-256
  fingerprint, and always gives a reason. `docs/plane-format.md` has the format.
  `charter scan --explain` names a finding's rule and prints the entry that would let it
  through.

**An agent cannot allow its own finding (V16), and the operator is the one who approves.** The
file is read as it is at `HEAD`, never from the working tree or the index, so an entry a chat
has written and not committed allows nothing. A chat's `pre-commit` also refuses any commit
that changes the file. The operator's own terminal is not armed, so a change to the allowlist
is a commit the operator makes, which means reviewing the entry. An agent can write the entry
and ask; it takes effect only once the operator commits it. A merge may bring the file as it
was committed on the other side.

**Tier:** Plane when the repository is a plane. Elsewhere it is committed to the operator's
repository, which as a whole is tier None (ADR 0069, row 74).

**Audit:** when AU-5 lands, a change to the allowlist becomes an audit entry. Until then, the
repository's history is the record.

## What it costs

On a debug build, at a machine load average of 10, an armed `git add` and `git commit` took
0.28 s against 0.03 s unarmed. That was measured with every hook shimmed, before three were
dropped. Most of the cost is a shell and one `git` per hook. Only the two checking hooks start
`charter`. This is accepted under the priority order in `CLAUDE.md`: development experience,
then standard practice, then speed a person can notice. A commit is not something an operator
waits on keystroke by keystroke.

## What changes where

| Where | What |
|---|---|
| `charter_core::githooks` | the shims, and `GitHooks::arm` for a chat's environment |
| `charter_core::diffscan`, `charter git-hook` | the scan, and the refusal |
| `charter_core::secretshape::leaks` | the rules a commit is scanned with |
| `charter_core::commitguard`, toolgate A8 | the Bash guard's refusal of the skips |
| `hookwire::CommitRefused`, `state::Chat::commit_refused` | the needs-you item |
| the app's `Opening::git_hooks` | which chats are armed: every chat on a harness, no shell |
| `docs/plane-format.md` | `<app data>/git-hooks/`, Machine, device-bound, rebuildable |

## Ruled (V26, 2026-10-01)

The operator accepted both recommendations:

1. **V26a: a refused commit's needs-you item clears on the chat's next prompt,** the way a
   report back does. The audit chain (AU-1, ADR 0075) is the lasting record of a refusal, not
   the queue.
2. **V26b: `git cherry-pick`, `git rebase` and `git am` are not scanned for now.** This is a
   recorded gap: they mostly replay commits that were already scanned, and a check that could
   only warn after the fact adds noise without a guarantee.
