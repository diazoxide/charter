# A chat's git runs purlis's hooks through its environment

**Accepted 2026-10-01** by the operator (ruling V26), drafted for program-map ticket SQ-16 (charter#592) from the
`/code-review` of charter#770. It adds a line to
[ADR 0051](0051-the-app-saves-the-plane-and-each-repo-by-a-mode-the-project-declares.md)'s checks
and relies on [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) for
what it cannot hold itself.

## Where purlis is today

A plane save and a workspace repo's save scan what they stage (ADR 0051). Neither sees a commit
an agent makes itself, with `git commit` in its own shell. Most of an agent's commits are made
that way, in a workspace repo, in a piece, or in a clone it made outside any plane. SQ-16 asks
that such a commit be scanned before it is made, in any repository, and that a finding block the
commit and become a needs-you item.

## The decision

**Every chat the app starts on a harness has its git run purlis's hooks, through one config
pair in the chat's environment.** git's documented `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_<n>` and
`GIT_CONFIG_VALUE_<n>` set `core.hooksPath` to a directory the app writes at every launch,
`<app data>/git-hooks/` (`charter_core::githooks`).

- **Added after, never over.** The chat's environment is settled first, including a pair the
  operator passes through `[chat_env] pass` or a profile sets. purlis's pair is the next index,
  and the count goes up by one. Each variable appears once in what the chat is started with.
- **Two hooks check.** `pre-commit` and `pre-merge-commit` run `purlis git-hook <name>`, which
  reads the added lines of the staged diff (through the index git hands the hook, so `commit -a`
  and `commit <paths>` are scanned as what they commit). It uses `secretshape::leaks`: the repo
  save's token rules, gitleaks' prefix-anchored vendor rules, and personal data (email
  addresses, card numbers that pass the Luhn check, US social security numbers). A finding
  refuses the commit, names each hit by path, line and kind with the value masked, and puts the
  chat in the needs-you queue over the hook socket (`CommitRefused`, ADR 0068 §5). A file name
  git writes that cannot be read also refuses the commit, because a file nobody read is not a
  file that was scanned.
- **The repository's own hooks still run.** `core.hooksPath` replaces a repository's hooks
  directory, so purlis writes a shim for each hook a repository is likely to have. Each shim
  runs the repository's own hook of that name after purlis's check, with git's arguments,
  standard streams and exit status. The repository's own directory is git's answer
  (`git rev-parse --git-path hooks`) with purlis's pair renamed out of the way for that one
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

- **A hook in `.git/hooks`** is per repository. purlis would have to write into every clone,
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
can also set a variable earlier in its session. The same holds for every purlis guard. Holding
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

- **purlis's own, the same in every repository:** an email address in a file named
  `Cargo.toml`, `package.json`, `.mailmap`, `AUTHORS`, `CONTRIBUTORS` or `CHANGELOG` (the last
  three also as `.md` and `.txt`), at the top or in any directory. These files publish names and
  addresses on purpose. Only those exact names are covered, and only the `email` rule. A
  repository turns them off with `[builtin] enabled = false` in its allowlist.
- **The repository's `.charter-scan-allow.toml`**, committed and reviewed like code. An entry
  either names a rule by its id together with the `paths` it covers, or names one key by the
  SHA-256 fingerprint of its value. Every entry gives a `reason`. `docs/plane-format.md` has
  the format.
  - Every rule entry must name its paths. A repository-wide entry writes `paths = ["**"]`, so a
    reviewer sees how wide it is.
  - A fingerprint entry never lets personal data through (`email`, `card-number`, `us-ssn`).
    Those values can be recovered from their hash by guessing, and the file is committed, so
    they are allowed by path only.
  - `purlis scan --explain` names a finding's rule and prints the entry that would let it
    through. For a key it also prints the fingerprint entry.

**Plane memory has no entry of its own.** Some planes are public, and whether a repository is a
plane is something an agent can change. The operator of a private plane commits an entry for
its memory.

**The operator is the one who approves an entry.** The file is read as it is at `HEAD`, never
from the working tree or the index, so an entry a chat has written and not committed allows
nothing. A chat's `pre-commit` refuses any commit that changes the file, including one that
moves it away. The Bash guard also refuses two commands, because they reach a changed file
without a `pre-commit` and the command line names them cheaply:

- a `git revert` of a commit that changed the file;
- a `git merge --ff-only` to a commit whose file differs from `HEAD`'s.

The operator's own terminal is not armed, so the ordinary way an entry takes effect is a commit
the operator makes. An agent can write the entry and ask. A merge commit may bring the file in
as it was committed on the other side.

**This is a mistake guard, not a boundary.** git moves `HEAD` without a `pre-commit` in more ways
than the guard can name:

- a fast-forward to a commit made elsewhere, spelled in ways the guard does not read (for
  example, through the stash);
- a `reset`, `checkout` or `switch` to a commit with a wider file;
- `commit-tree` with `update-ref`;
- `am`, which V26b already records as not scanned.

An agent that means to widen its own allowlist can do it through any of these. Holding a chat to
the allowlist its operator committed is the sandbox's work (ADR 0067), and charter#784 is the
follow-up there.

**Concept homes (ADR 0072 §2):** the commit scan, `purlis scan` and its report are part of
**Workspace**, as checks on a repo's save, beside a repo's own save mode. The allowlist is a
setting of a repo, in the repo itself.

**Tier:** Plane when the repository is a plane. Elsewhere it is committed to the operator's
repository, which as a whole is tier None (ADR 0069, row 80; row numbers renumbered 2026-10-01; see ADR 0069).

**Audit:** #593's acceptance asks that an allowlist change appear in the audit log once AU-5
lands. Until then, the repository's git history is the record. The note is filed on the audit
epic (charter#552).

## What it costs

On a debug build, at a machine load average of 10, an armed `git add` and `git commit` took
0.28 s against 0.03 s unarmed. That was measured with every hook shimmed, before three were
dropped. Most of the cost is a shell and one `git` per hook. Only the two checking hooks start
`purlis`. This is accepted under the priority order in `CLAUDE.md`: development experience,
then standard practice, then speed a person can notice. A commit is not something an operator
waits on keystroke by keystroke.

## What changes where

| Where | What |
|---|---|
| `charter_core::githooks` | the shims, and `GitHooks::arm` for a chat's environment |
| `charter_core::diffscan`, `purlis git-hook` | the scan, and the refusal |
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

## The message hook (amended 2026-10-03, V67, GL-8)

**A third hook starts `purlis`: `commit-msg`, which stamps an agent's own commit with its
provenance trailers.** Ruling V67 (#702) puts `Assisted-by`, `Charter-Chat`, `Charter-Persona`
and `Charter-Change` on every commit an agent run makes, in workspace repos and in the project,
and on none that a human makes by hand. Most of an agent's commits are its own `git commit`
(above), so the one place purlis can reach them is the hook this record already arms. The format
is in `docs/plane-format.md`, *Provenance trailers*.

- **`commit-msg` runs `purlis git-hook commit-msg <file>`, then the repository's own
  `commit-msg`**, which sees the stamped message. purlis reads the chat from
  `$PURLIS_SESSION_ID` and the app's record (ADR 0066), and the change from the repo's branch
  (ADR 0060). purlis appends the lines itself and changes nothing else in the message. It does
  not run `git interpret-trailers`, which would reformat the agent's own lines and read trailer
  configuration that can run a program. A line already there is not added twice.
- **It never refuses a commit.** It is not a check: a message purlis could not stamp, a chat
  the record does not hold, or a `purlis` that has gone commits the message as it was written.
  This is the opposite of the two checking hooks, which refuse when `purlis` has gone.
- **Only an armed chat's commits get trailers.** The operator's terminal is not armed, and a shell
  tab is not a chat on a harness (the app's `Opening::git_hooks`). An agent can skip the hook
  with `--no-verify`, as it can skip the scan, and the Bash guard refuses that spelling.
- **Only a commit made below the chat's harness gets trailers** (amended 2026-10-03, V82,
  #1018). The environment that arms a chat is inherited by whatever it starts: an editor or
  terminal opened from the chat keeps it, and the operator's commits there are theirs. So
  purlis stamps only when the committing process descends from the program the app started for
  that chat, whose pid the app's record keeps (`chats[].pid`). The walk reads `/proc` on Linux
  and asks the kernel in its own process on macOS (`proc_pidinfo`, amended 2026-10-07,
  D-1407-8; it was one `ps` run before), and every doubt answers "not the agent". The pid is recorded only
  while that program runs: the record is written again when it ends, and the record written at
  quit names no pid, so a pid the system may hand to another process is never one purlis
  vouches for. The same rule covers `purlis save` from inside a chat.

  Its limits, each of which fails closed (the commit is left as written, never stamped wrongly):
  - **A sandboxed chat on Linux gets no trailers yet** (operator ruling, 2026-10-03). Inside
    the sandbox ADR 0067 runs a chat's commands in, the walk cannot reach the harness: the
    sandbox gives commands their own pid namespace. Binding provenance to the sandbox purlis
    launched is a follow-up (#1021).

    **On macOS that ruling is lifted** (operator ruling D-1407-8b, 2026-10-07). It held there
    only because the sandbox does not let a chat's commands run `ps`. The walk now asks the
    kernel in its own process, which a sandbox allows for the processes inside it, and the
    harness is one of them: the walk stops on the recorded pid without reading anything
    above it. So a sandboxed macOS chat's commits get the trailers, as an unsandboxed chat's
    do. A sandbox that hides a process between the commit and the harness still ends the
    walk early, and that commit is left as written.
  - **Windows is never stamped.** purlis has no way to read another process's parent there
    yet, so the answer is always "not the agent".
  - **A process that left the tree is not stamped**: a detached editor, or a job that outlived
    its harness. For an editor that is the point.
- **What it costs:** one more `purlis` process per commit, reading the app's record and, in a
  workspace repo, the workspace's change records. This is accepted on the same grounds as above.

| Where | What |
|---|---|
| `charter_core::githooks::COMMIT_MSG` | the shim that runs `purlis` and never refuses |
| `charter_core::provenance` | the trailers, the chat and change they name, and `stamp` |
| `charter_core::process::descends_from` | whether the commit runs below the chat's harness |
| `purlis git-hook commit-msg` | the stamping |

## The push hook (amended 2026-10-03, SQ-7)

**`pre-push` runs the same scan over what a push would send.** Program-map ticket SQ-7 (#586,
gap G4a: the scan is the project's pre-commit *and* pre-push hook) adds a third checking hook to the
two above. A commit can reach a chat's push without passing its `pre-commit`: one the operator
made in their own terminal, which is not armed, or one `cherry-pick`, `rebase` or `am` made
(V26b). The push is the last point before it is published.

- **What is read.** git hands `pre-push` the remote's name and URL, and one line per ref it
  updates. Each ref is peeled to the commit it names; a ref that names a blob or a tree,
  directly or through a tag, has no history to read and refuses the push. purlis reads every
  commit reachable from what is pushed and from neither what the remote has for those refs nor,
  for a configured remote, its remote-tracking refs. Each commit's added lines are scanned as its
  own diff, root commits included whatever `log.showRoot` says, so a value one commit adds and a
  later one removes is still found: the history publishes it. Merge commits are not read; each
  side's commits are. A deleted ref sends nothing.
- **The same rules, and the remote's allowlist** (`secretshape::leaks`, `.charter-scan-allow.toml`).
  A pushed range that changes the allowlist refuses the push: the operator pushes an allowlist
  change themselves, as only the operator commits one. Otherwise findings go through the file as
  the remote already has it for each ref, never the checkout's: the remote's commit, or for a new
  ref the pushed tip, which the range then leaves unchanged. Refs whose files differ get
  purlis's own entries alone.
- **A finding refuses the push**, names each hit by path, line and kind with the value masked,
  says that the value is in history and a live credential needs revoking, and puts the chat in
  the needs-you queue with the same `CommitRefused` line, worded "push refused".
- **It refuses like the commit hooks.** A `purlis` that has gone, input purlis cannot read, or
  a git that does not answer within the network deadline refuses the push.
- **The repository's own `pre-push` still runs**, after purlis's check, with the same arguments
  and the same lines on its standard input: the shim reads them once and feeds both.
- **The Bash guard refuses `git push --no-verify`** and each prefix of it git accepts
  (`--no-veri` and longer; the same now holds for `git merge`), and `git send-pack`, the plumbing
  under `git push`, which runs no `pre-push`.
- **What it does not cover.** This is a mistake guard, as above, not a boundary:
  - a push the app or the operator's terminal makes is not armed, as for the commit hooks;
    `purlis save` scans what it stages itself (ADR 0051);
  - "what the remote already has" is read from this clone's remote-tracking refs, which anything
    in the clone can write: a ref written there excludes its commits from the scan;
  - a merge commit's own resolution is not read, and a chat can make a merge without
    `pre-merge-commit` (`git commit-tree` with two parents);
  - commit and tag messages are not scanned, and content git shows as "Binary files differ" (a
    `binary` or `-diff` attribute, `core.bigFileThreshold`) is not read, here or at commit;
  - pushing by any route the Bash guard cannot read: `sh -c`, a script, an alias, a forge CLI's
    API calls that write content, or a git client that is not git.

| Where | What |
|---|---|
| `charter_core::githooks::PRE_PUSH` | the shim that keeps standard input for both hooks |
| `charter_core::diffscan::pushed` | the commits a push sends, scanned |
| `purlis git-hook pre-push` | the refusal |
| `charter_core::commitguard` | `git push --no-verify` and its prefixes, `git send-pack` |
