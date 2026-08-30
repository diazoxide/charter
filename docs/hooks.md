# The hooks

The CLI is one of charter's two artifacts. The other is a **Claude Code plugin**, and the
plugin is almost entirely hooks: the things that have to happen without anyone remembering
to ask for them.

Install both — a CLI with no plugin leaves every guard and every injection dead, while
looking completely installed. See [install.md](install.md).

The plugin ships **no Python**. Every hook is a `charter hook <name>` call against the CLI
on `PATH`, which is why a version skew between the two is worth shouting about and is the
one thing a hook is allowed to shout about.

## What fires, and when

| Event | Matcher | What it does |
| --- | --- | --- |
| `SessionStart` | — | reconcile workspace state, GC persona scratch, inject context, run `doctor`, refresh forge state |
| `UserPromptSubmit` | — | the commitment gate (below) |
| `PreToolUse` | `Bash` | four of the five guards (below) |
| `PreToolUse` | `Read\|Grep` | keeps a vault file from being read into context |
| `PreToolUse` | `Task\|Agent` | notes a dispatch about to happen |
| `PostToolUse` | `Write\|Edit\|MultiEdit` | the record-memory nudge |
| `PostToolUse` | `Skill` | tallies which skills a persona actually invokes |
| `PostToolUse` | `Task\|Agent` | tallies the dispatch |
| `Stop`, `SubagentStop` | — | autosave a LIVE workspace |

## The guards

Every one of them **denies**. None of them asks — charter holds no nudge on the Bash tool,
and that is a deliberate position, not an accident (see *What charter stopped asking*).

A denial from these is **the rule working, not a bug** — the single most common thing
mistaken for a defect. Each prints why, because a developer who reads the reason learns the
rule while one who reads a bare refusal files an issue.

- **Secret leak.** A charter invocation carrying `--reveal`, or a **known** file-reading
  program whose argument, as written, spells a path under `.charter/`. It is a name-based
  check on the argv it can see, and that is its ceiling: an interpreter (`python3 -c`,
  `node -e`), a program not on the list (`base64`, `cp`, `jq`, `cut`,
  `git show HEAD:<path>`), or a shell string (`sh -c 'cat .charter/vaults/db.json'`, which
  is one argument here and is not re-parsed) is not covered. Widening the list is not the
  fix — the missing name is always the next one, and false positives arrive immediately.
  "Argv" now means the real one. A wrapper (`env`, `sudo`, `command`, `xargs`, a `{ … }`
  group, a `then` branch) does not change what the program is — and where a wrapper opens a
  file *itself* (`xargs -a <file>`) that file counts as read, even though the program named
  on the line is something else. A **redirection** is neither the program nor an operand: it
  may sit in front of the command (`< <vault> cat`), and the target of an input redirection
  is a file the shell opens whatever the program does with it (`tee < <vault>`). A **command
  boundary is an operator the shell would interpret**, so a quoted or escaped one is an
  argument and not a boundary (`cat \) <vault>` reads the vault), and the `&` inside the
  redirection `>&` is not the control operator `&` (`cat 2>&1 <vault>` is one command);
  while a newline *is* a boundary exactly as `;` is — every line of a multi-line command is
  its own command, `bash <<'EOF'` bodies included — and `#` starts a comment only where a
  word starts. Position counts too: `{` and `}` are reserved words, so bash passes them as
  plain arguments anywhere but command position and `cat { <vault>` is one command that
  reads the vault. Beyond that: an unparseable quote does not hide the commands after it,
  an **unquoted** `$( … )` substitution is read both as the command it runs and as the word
  it becomes, and a relocation counts however it is spelled (`cd`, `pushd`, `env -C`,
  `sudo --chdir`). The **path** is normalised before the match — redundant separators,
  `.`/`..` segments and letter case — so `.charter//vaults/db.json`,
  `.charter/./vaults/db.json` and `.CHARTER/vaults/db.json` all answer the same as the
  plain form. Two things it still cannot know. A *different* path holding the same bytes: a
  vault registered outside `.charter/`, a file `charter secret cp` wrote to a path you
  named, or a symlink. And anything a **shell** does to the operand after the hook has
  answered — a glob (`cat .charter/vault?/db.json`), a variable (`V=…; cat $V`), a quoted
  substitution, brace or tilde expansion. The hook runs on the command line, never on what
  `sh` turns it into, so each of those is `cat` on the same inode and allowed. **What it
  does not catch is written down** — see *Where the secret-leak guard stops*, below, and
  [SECURITY.md](../SECURITY.md) for why that is the honest scope rather than a defect.
- **Vault read.** The same invariant on the `Read`/`Grep` tools, which never reach the Bash
  matcher at all. It calls the **same predicate on the same operand and adds no step of its
  own**, so one operand gets one answer whichever route it arrives on — asserted directly, in
  both directions, by `tests/test_vault_path_spellings.py::TestTheTwoGuardsCannotDisagree`.
  That sentence used to read "same path pattern, so the same limits", which was false: this
  route carried a private trailing-slash retry that the Bash route did not, so
  `Grep(path=".charter/vaults")` was refused while `grep -rn TOKEN .charter/vaults` printed
  plaintext. The retry is gone and the pattern anchors `vaults` to a path segment instead.
  No shell is involved on this route, so of the limits above only the path ones apply — the
  shell-expansion and wrapper families do not arise here.
- **Plane-root branch move.** The plane is not a work tree (ADR 0008); a branch switch there
  is almost always meant for a clone. `--detach` counts — with an operand, without one, and
  with the plane's own default branch as the operand, which is the one spelling that used to
  slip past. What is always allowed is putting the root back **on** its default branch
  (`git checkout main`), which is a different thing from naming that branch:
  `git checkout --detach main` names it and leaves HEAD attached to nothing.
  Restoring a file does **not**: `git checkout` is two commands wearing one name, and which
  one you typed is settled by asking git whether the operand resolves as a revision or names
  a path it tracks — so `git checkout <path>` and `git checkout <tree-ish> -- <paths>` run
  here exactly as `git restore <path>` always has. Where a branch and a tracked file share a
  name the answer is genuinely ambiguous, git breaks the tie in favour of the ref, and the
  denial says *that* and names the two unambiguous spellings rather than assuming a branch.
  The options are read the same way round: the restore gate opens only when every option
  present is one charter can place as restore-only, because an option decides what its
  operand means — `git checkout --orphan README` creates a branch called `README`. An option
  charter cannot place is refused rather than assumed harmless, value forms included
  (`-bREADME`, `--orphan=README`), which costs a false denial on a restore-only flag nobody
  has added to the list yet; `git restore <path>` needs no flags and is always allowed.
  Aliases are followed before the guard stands aside — `co = checkout` makes `git co
  feature` the same branch move — including chains, aliases carrying their own options,
  `!git checkout`, and `git -c alias.co=checkout co …`. A `!`-alias that is not a plain
  `git …` is not read (refusing every shell alias here would refuse `s = !git status`), and
  neither is `--config-env`. **These routes all reach the same verdict** — the list is what
  is covered, not a claim that every route is: the cwd, a `cd` earlier in the same command,
  `git -C <path>` — absolute or relative, and relative means *relative to the shell*, which
  is the fix for `git -C ../../.. checkout <branch>` reaching the root from a clone — and
  the three options that name a repository without naming a directory to stand in:
  `--git-dir`, `--work-tree` and their `GIT_DIR` / `GIT_WORK_TREE` environment spellings,
  attached or separated, composing with `-C`
  ([#477](https://github.com/diazoxide/charter/issues/477)). The cwd is a subject of every
  git command, including one that names a `--work-tree` elsewhere: with no `--git-dir`, git
  discovers the repository from the cwd, so the refs that move are the cwd's. And *which
  repository a `--git-dir` belongs to* is asked of the filesystem rather than of the string,
  so `<plane>/.git`, `<plane>/.git/./` and `<plane>/.git/refs/..` are one question — a
  lexical parent made the last of those a live bypass for a round. What it still does not
  place is a `--git-dir` pointing at a **linked worktree's** git dir, whose HEAD is that
  worktree's and not the root's; that one is a missed denial, never a wrong one. A fourth
  spelling of the work tree is in no token at all: `core.worktree` in a repository's own
  `.git/config` makes the named directory that repository's working tree for every command,
  so `git checkout <branch>` typed in a workspace clone wrote into the plane root and the
  guard saw a plain checkout in a clone. The repository's config is read now — the one
  invocation-derived subject that costs a disk read, at 13–65 µs, stated in
  `charter/gitconfig.py` along with the routes it declines: `git -c core.worktree=…` on the
  command line (git ignores it, so it reaches nothing), `include`/`includeIf`, and the
  global and system configs
  ([#504](https://github.com/diazoxide/charter/issues/504)). A `-C`
  counts as git's
  change-directory global only **before the subcommand**, which is the only position git
  reads one in — so `git switch -C <branch>`, where `-C` is `switch`'s own `--force-create`,
  is the branch creation it is rather than a directory called `<branch>`
  ([#483](https://github.com/diazoxide/charter/issues/483)). Every one of those is a row in
  the guard's corpus (`tests/test_plane_root_checkout_is_two_commands.py`), crossed with the
  commands rather than listed beside them. The walk also carries the **environment a command
  line establishes for its later segments**, so `export GIT_DIR=<plane>/.git && git checkout
  <branch>` reaches the same denial the attached `GIT_DIR=… git …` does
  ([#496](https://github.com/diazoxide/charter/issues/496)) — as do `declare -x`/`typeset
  -x`, `GIT_DIR=…; export GIT_DIR`, and a bare assignment under `set -a`. A bare
  `GIT_DIR=…;` segment on its own is *not* one of them, because a shell exports nothing
  there. That environment only ever grows: `unset` and `export -n` are not modelled, since
  forgetting a variable is the direction that opens a door. What is still outside it is the
  same boundary `cd` has — a `$(…)`, a sourced file, and a `GIT_DIR` already in the
  session's environment before the hook ran, which the `PreToolUse` payload does not carry
  at all.
- **Plane-root history wipe.** A `git reset --hard` (or `--merge`/`--keep`) in the plane root
  that would take commits off the branch which no remote has a copy of — the command that
  destroyed eleven memory commits in one session. Only that: the unstage
  (`git reset HEAD -- <path>`), `--soft`/`--mixed`, a reset with no ref, and any reset over
  commits that are already pushed all run untouched. It clears itself — `charter save` lands
  the commits and the same command is allowed. It follows **aliases** exactly as the branch
  guard does — `wipe = reset --hard` makes `git wipe origin/main` the same command, and so
  does `git -c alias.z='reset --hard origin/main' z`
  ([#467](https://github.com/diazoxide/charter/issues/467)) — and shares every route above,
  so `git --git-dir=<plane>/.git reset --hard <ref>` from a clone is refused too.
- **One credential.** SSH to a forge, `GIT_SSH_COMMAND`, `-S`/`--gpg-sign`, and the
  `core.sshCommand` family that reaches the same transport by another road (`-c`,
  `--config-env`, `GIT_CONFIG_KEY_n`, and a `git config` write of it). This one *is*
  expressible as a pattern and stays in the hook anyway, so it can explain itself — see
  [git-policy.md](git-policy.md) and ADR 0014.
- **Release floor.** A run the harness reports as `bypassPermissions` may not create a tag,
  push tags, `gh release create` / `gh pr merge`, or **`charter change land`**.
  `bypassPermissions` means *stop asking me*, not *stop knowing things*, and a published
  version number can never be reused.

  `charter change land` is here because it merges one member of a cross-repo change into a
  repository, which is the same act `gh pr merge` is — and the project's line already runs
  between *opening* a request and *merging* one (`gh pr create` is deliberately not on this
  list). The split is attended versus unattended: attended, an agent may land one member,
  because that is the merge the standing rule already permits for a single repo. Every other
  `charter change` verb — `show`, `list`, `create`, `add`, `drop`, `push`, `revert` — is
  untouched in every mode. See [changes.md](changes.md) and
  [ADR 0020](adr/0020-there-is-no-cross-repo-merge-loop.md).

- **Forge body substitution.** A `gh`/`glab` command that publishes prose — `issue
  create|comment|edit`, `pr create|comment|edit|review`, `release create|edit`, `gist
  create|edit`, and glab's `issue`/`mr` `create|note|update`, `release create`,
  `snippet create` — may not carry a command substitution the shell would **run**.

  ```bash
  gh issue create --body "run `env` first"     # DENIED — the shell runs env, gh gets the output
  gh issue create --body 'run `env` first'     # allowed — one character, and it publishes the text
  ```

  Inside double quotes a backtick is command substitution, and a body is the one argument
  where that collides with markdown: writing a code span and writing a shell command are
  the same character. An agent filing an issue hit this and published sixty-four
  environment variables — vault tokens included — into a public body
  ([#703](https://github.com/diazoxide/charter/issues/703)). Nineteen other issues filed
  the same night used the same shape and were harmless, because the backticked text was not
  a runnable command; the pattern was wrong in all twenty.

  **The remedy the denial names is `--body-file`** — a path, or `-` with a **quoted**
  heredoc (`<<'BODY'`). An *unquoted* `<<BODY` expands exactly the same way and is denied
  too, which matters more than the `--body` case now: that is the spelling an agent
  following the rule and forgetting the quotes will write.

  **What it claims is the shape of the line, and nothing else.** The value is out of its
  reach in both directions: at `PreToolUse` the substitution has not run, and by
  `PostToolUse` the issue is already public. So this is a refusal of a shape, not a promise
  that a credential stays off a forge. Three limits follow from that
  and are stated rather than left to be found: a **`--body-file` whose file already holds
  the text** is not covered (nothing expands on that path, so there is no shape to see);
  **`git commit -m "… `x` …"`** is out of scope
  ([#711](https://github.com/diazoxide/charter/issues/711)); and the check is scoped to the
  **whole Bash call**, not to the
  body argument, so `cd "$(git rev-parse --show-toplevel)" && gh pr create --body-file b.md`
  is refused as well. Narrowing that means deciding which argument a substitution lands in,
  which means putting a shell inside the guard — the failure the leak guard already
  documents. Run the substitution in a **separate** Bash call instead; each is judged alone.

  The commit-message limit is worth one more sentence, because the obvious reason for it is
  wrong. It is **not** that a commit matters less. On the axis #703 turns on — *can this be
  undone* — a commit message is **worse** than an issue body: a body is replaced in one
  call, while a pushed commit needs a history rewrite, and a rewrite reaches neither forks
  nor existing clones nor the forge's caches. Visibility is not reversibility. It is out of
  scope because the commit surface has not been verified the way the `gh`/`glab` verbs
  were, and because it is dense with the character the guard keys on: **most commit
  messages on charter's own `main` carry a backtick** — 26 of 30 consecutive ones when
  this was measured, nineteen of them around text that reads as a command — and inside
  `-m "…"` every one of those is live. A guard that fires
  constantly on legitimate work is one that gets switched off, and then it covers nothing.

  Unlike the four guards above it, this one is **not gated on there being a control
  plane**. What it refuses is a fact about the shell rather than a policy this plane holds,
  and its remedy is plain `gh`/`glab` usage — the same reason the secret-leak guard is
  ungated.

A seventh path is not a guard but an allowance: a program the **active persona** declares in
`tools:` runs without a prompt while that persona is active, and only then. It approves the
**program**, so every argument rides along — which is why seven things are not smoothed
whatever `tools:` says: destructive subcommands (`kubectl delete`, `charter secret`,
`charter change`, `git clean`, …), **an argument that is itself a program**, interpreters
whose argument is the real command (`bash -c`, `python3 -c`, `env`, `xargs`, `npx`, …),
any command whose arguments **reach** a vault or charter's own state, any tool added to that line after the
session started — the gate answers within the set declared before this session could
rewrite it, and `persona.md` is a file the model can write — any command carrying a
character the shell would rewrite before the program sees it, and any command whose
**command word is not the file the declared name refers to**. Each of those is a fall back
to the normal prompt; this path never denies.

Two of those seven are one class read from opposite ends, and the difference between them
is the difference between a rule and a list. A command that runs a second program is not
one program with its arguments, and only one of the two was declared. Where that second
program is named as a **path**, charter asks the filesystem — is this a file this machine
would execute? — and declines. That is what covers `caffeinate -s ./evil`,
`arch -arm64 ./evil`, `flock /tmp/l ./evil`, `taskset 1 ./evil`, `runuser -u u -- ./evil`
and the wrapper written after this sentence, none of which charter has to have heard of.
Every one of those was an affirmative `allow` before, and for the first two the
agent-written `./evil` was observed executing.

The interpreter list is the other end, and **it is a list of names, and it is best-effort**.
It exists for the programs that take their command as *text* — `bash -c hostname`,
`awk 'BEGIN{system("id")}'`, `npx some-package` — where there is no file for the rule above
to stat and nothing in `argv` that separates the string from an ordinary argument. Round
four walked past it with `caffeinate`, which was simply not on it, while `nice`, `ionice`
and `timeout` were. Adding names is not what closed that; asking about the file is. Read
the list as a convenience over a residual, never as the boundary.

The residual it sits on, stated: an argument that is a **bare name** is resolved by the
program being run, through `PATH`, and charter does not treat it as a program. Doing so
means treating every argument `shutil.which` answers as a program — and `log`, `pr`, `ps`,
`top`, `apply` and `cp` are all binaries on an ordinary machine, so `git log`,
`gh pr list`, `docker ps`, `kubectl top` and `kubectl apply` would each become a prompt.
That is not one prompt; it is the end of the allowance. Exploiting it needs a program the
agent can already reach through `PATH`, which is the same precondition as the command-word
residual further down. `ssh host rm` is the same floor with the machine changed.

The command-word rule is newer than the rest and is the same mistake as the first version
of the one before it. `tools: gh` was matched against `os.path.basename`, so `./gh` — a shell
script the agent wrote a moment earlier — inherited the approval, as did `/tmp/gh`,
`bin/kubectl` and `PATH=<a directory the agent wrote> gh`. That is arbitrary unprompted
execution under any `tools:` line at all. A command word spelled as a path is now smoothed
only when it is, by inode, either the persona's own `personas/<n>/bin/<name>` script or
whatever a bare invocation of that name resolves to right now — so `/usr/bin/gh` and a
symlink to it are still `gh`, and a file of the agent's own is not. A **bare** name is
smoothed as before: it is the declaration exactly as written, and there is nothing else to
check it against. What that cannot see is whether the file `PATH` resolves to is the real
`gh` — an agent that can write into a directory already on `PATH` puts its program where
bash and charter both find it and both agree.

A leading `VAR=value` is refused for the same reason, all of them, not a list of the
variables that matter: `PATH=/tmp gh` picks the file as surely as a `/` does, and there is
no honestly inert subset to carve out — `KUBECONFIG=… kubectl …` was the shape the old
exemption existed for, and a kubeconfig may name an `exec` credential plugin, which is a
kubeconfig naming a program for kubectl to run. `KUBECONFIG=… kubectl get pods` is one
prompt now.

Destructive subcommands are read as **words** of `argv`, not as whole arguments:
`git -c alias.z=clean z -xfd` defines the alias and runs it in one command, and really does
delete every untracked file. A word ends where a word ends, so `kubectl describe pod
my-run-1` is not `kubectl run` and `port-forward` is still one verb — but
`git commit -m "clean up the tests"` is a prompt now, and that is the price.

*Reach* is decided about the file, not about the text of the command. Each argument is
resolved and compared by inode to where `charter` actually keeps its state — so
`@".charter"/vaults/x.json`, `.chart\er/…`, a symlink, a case-folded spelling, the bare
directory name, and a directory that merely *contains* the state directory
(`tar -cf /tmp/o.tar .` in the plane root) are all one answer. Because the question is asked
of `charter.config` rather than of a hardcoded name, a plane with `$CHARTER_HOME` set, a
plane still on the legacy `.edm/` directory, and a vault the registry points outside the
plane are covered too.

That comparison is only worth anything if the argument charter examines is the argument the
program receives, and the step in between is the shell. **A command is smoothed only when
every character in it is one the shell hands over unchanged** — letters, digits, and
`_ - . / : , = + @ %`, plus quotes, a backslash, spaces and tabs, and any non-ASCII
character. Anything else and the gate declines and you get the normal prompt. That is not a
list of dangerous characters — two of those have been written here and both were bypassed
by a character nobody had thought of. It is the inverse: a spelling this process cannot
evaluate is refused by default.

The concrete cost, so it is a decision rather than a surprise: `ls *`, `cat ~/notes.md`,
`git commit -m "fix #12"`, `kubectl get -o jsonpath={.items}` and any command naming an
executable file — `git add ./release.sh`, `cat ./deploy.sh` — are no longer smoothed —
one prompt each. Brace expansion (`.charte{r..r}`), ANSI-C quoting (`$'\x2echarter'`),
globs, `~`, `$VAR` and `$( )` all reach the same refusal, because each of them is the shell
rewriting a word before `argv` exists: `charter secre*` is `charter secret` the moment
something creates a file called `secret` beside it.

**What it still cannot see**, stated rather than implied: this gate reads `argv` and nothing
else. A program that takes its arguments from a *file* is outside it — `curl -K req.conf`
runs whatever `req.conf` says, including `--upload-file .charter/vaults/devops.json`, and
nothing charter owns appears in the command. That is not closed, and the reason is worth
being explicit about: closing it means listing `curl -K`, `wget -i`, `tar -T`, `xargs -a`,
`git -c include.path` and whatever comes next, and a list of the spellings somebody thought
of is the shape that has already been bypassed three times here. There is no property in
`argv` that separates a config file from any other argument. The same applies to a relative
path hidden behind an unrecognised flag prefix on a plane whose state directory is named
neither `.charter` nor `.edm`.

Two more sit on that same floor, found in the fourth round and disclosed here for the same
reason. A flag whose *value* is another program — `tar --use-compress-program=`,
`git -c core.pager=` — runs it, and in `argv` it looks like any other `--flag=value`. Half
of that one closed itself when the rule above arrived: where the value is a path to an
executable file, charter stats it and declines, without knowing the flag. What is left is a
value that is a bare name — `--use-compress-program=gzip` — on the residual described
above. And a verb can reach the command from a file: the gate reads every word of `argv`, so
`git -c alias.z=clean z` is seen, but a `z = clean` alias already in `.git/config` — which
takes a plain Edit and no Bash at all — makes `git z -xfd` a command whose every word is
innocent. Both fall back to a prompt the moment anything else in the command is unusual,
and both are one prompt away from being seen if you would rather not have the allowance at
all: take the tool off the `tools:` line.

None of that is a denial being dodged. This path is an *allowance*: the fallback is the
normal prompt, and every guard above — the Bash leak guard, the state-write guard — applies
to those commands exactly as before.

## Where the secret-leak guard stops

SECURITY.md gives charter's position and this section is the local, specific version of it:
**guard rails, not guarantees — a guard against mistakes, not an attacker with shell access
as your user.** The secret-leak guard is worth having because an agent reaching for a vault
file by name is a real and frequent event, and the guard catches those spellings reliably.
It is not a sandbox, and the list above is not a claim of completeness.

**It is defeated by deliberate obfuscation.** One example, so nobody has to guess where the
line is:

```bash
echo $(cat .charter/vaults/x.json)      # DENIED
echo "$(cat .charter/vaults/x.json)"    # ALLOWED — one pair of quotes, and it prints
```

Four rounds of adversarial review have now been run against this guard, and each round's fix
was defeated by the next spelling — `$( … )`, `env -C`, a quoted `)`, a bare `{`, a leading
fd digit. That pattern is the finding. Deciding what a shell will execute, without executing
it, is not winnable in a Python tokeniser, so the honest move is to say what is open:

- **a quoted command substitution** — the example above, and `` "`cat <vault>`" ``,
  `"$(<vault>)"`, `"$(charter secret get v k --reveal)"`. One command family is the
  exception, and it is an exception for a different reason rather than a fix for this one:
  a `gh`/`glab` command that publishes prose is refused whenever a live substitution stands
  on its line, so `gh issue create --body "$(cat <vault>)"` stops — see *Forge body
  substitution* above. That guard never looks inside the substitution and knows nothing
  about vaults; it refuses the shape. Everywhere else on this page, a quoted substitution
  is still open;
- **any expansion between the guard and `open()`** — globs (`.charter/vault?/x.json`,
  `.charter/*/x.json`), brace expansion (`.charter/{vaults,}/x.json`), `$'\x73'` quoting,
  and a path that arrives in a variable (`V=<vault>; cat $V`). The path check matches text,
  not resolved files;
- **a shell that runs a string** — `sh -c '…'`, `eval`;
- **a vault registered outside `.charter/`**, which the Bash guard does not look up (a
  registry read on every Bash call is a cost the hot path will not carry);
- **anything that reads the file without naming a known reader** — an editor, a language
  runtime, a copy followed by a read of the copy.

There is no second line of defence behind it: nothing scans Bash *output*. What actually
makes a vault not worth reading is the provider — `1password` and `reference` keep the value
in a system built for custody and resolve it on demand, so there is no plaintext on disk for
any of the above to print. That is the control; the hook is the guard rail.

## When a guard is wrong

Every guard is eventually wrong about something, and the response a design invites at that
moment is the response it gets. So this is written down rather than left to be discovered.

**There is no config key, environment variable or `charter guard` verb that lifts a
denial, and there will not be one.** That is the answer, not an omission:

- charter's guards exist because **committed data must not be able to reach a credential or
  make something run**. A switch charter read from `charter.toml` would be a switch a
  committed file could flip — a teammate's pull request turning off the guard that keeps a
  vault out of the transcript. An environment variable is no better: the agent writes the
  command line the variable would sit on.
- So an override charter can read is an override the *agent* controls, which is precisely
  the party the guards bound.

**The override is that you run the command yourself.** The guards are `PreToolUse` hooks on
the harness's tools — they govern what an agent does with your authority inside a session.
Your own shell is on the other side of that boundary and always was. Open a terminal and run
it. Nothing is being worked around: the rule never applied to you.

Four guards name a narrower move first, and it is usually the one you want:

- **Forge body substitution** — `--body-file <path>`, or `--body-file -` with a quoted
  heredoc. This one is rarely wrong about the shape and often wrong about the intent: the
  body you meant is exactly the body you get, and it is the shorter line to type anyway.
- **Release floor** — re-run the step **attended**. This is a mode, and it is yours to set.
- **One credential** — `charter git-policy --apply` configures every clone for the token
  transport, which is what most denials of it are actually asking for.
- **Plane-root history wipe** — `charter save`. The guard is measuring commits that exist
  nowhere else; push them and it stops firing, on that command and every other one.

**If a guard is wrong about you *every time*, that is not an override problem.** It means
charter is holding a policy your organisation does not — an org that mandates signed
commits, say. Switching the guard off locally hides that; the fix belongs in the rule.
[Open an issue](https://github.com/diazoxide/charter/issues).

**The thing that is not an override**, named here so nobody finds it by accident and
believes they found the switch: removing charter's hooks from `.claude/settings.json`, or
disabling the plugin. That takes out every guard, both injections and every tally together,
because one of them was wrong once. It is an uninstall.

## What charter stopped asking

charter used to nudge before a git write inside a workspace clone, suggesting a repo-rooted
session. It is gone, and the measurement is why: in one plane it asked **471 times in two
weeks and was approved 97 times out of 98** — while the persona tool-gate, the mechanism
whose whole job is to *remove* prompts, fired 16 times over the same window. Its trigger was
also charter's own prescribed workflow: `charter clone` puts every repo under `workspaces/`,
and the `working-in-a-clone` skill says *commit to the repo you are in*. The advice the
nudge carried still exists there, in prose, interrupting nobody.

The rule that came out of it, and the one a new nudge has to pass:

> **A prompt is worth its interruption only if it changes what happens.** If the evidence
> that it does cannot be collected, the prompt cannot be justified — and a declined `ask`
> produces no `PostToolUse`, so charter can never tell a decline from an interrupted turn.

An approved ask *is* countable, and every nudge charter still has records both halves. See
*What gets counted*.

Policy that *can* be written as a command pattern belongs in Claude Code's own
`permissions`, not here — `charter guard ask <pattern>` writes it there. Charter keeps only
what needs context the host cannot see. That line is ADR 0014.

## What gets injected

`SessionStart` puts a bounded amount of context in front of the session: the active
persona's role, a digest of memory (a digest, not the corpus — recall is a search, not a
preload), the workspace to confirm, and a warning when the plane's config has moved on
since the session began.

The `UserPromptSubmit` gate is narrower than it sounds. It fires when a prompt asks for
work *and* carries a real fork — open-ended, broad, destructive, or multi-part — and its
effect is to say: scout first, then ask, before dispatching or editing.

When the acting persona declares `routing: advise` or `require`, the same message leads
with the **roster** — who else exists, what each advertises, when each was last dispatched.
One message, not two: two blocks on one prompt is how a nudge becomes wallpaper. charter
never says which persona owns the prompt (ADR 0016); see `docs show personas`.

At `routing: require` a second, quieter hook joins it: `PreToolUse` on `Write|Edit|MultiEdit`
**asks** once when a turn edits after the roster fired and nothing was dispatched. It never
denies, and a dispatch — or the next prompt — clears it.

## What gets counted

Two tallies — tracked, not ignored, though charter never commits them for you (`[memory].share` defaults to `local`) — both counts-and-dates only, never prompt text, and both
parallel-writer safe with the host in the filename so two engineers never conflict:

- **Dispatches** (`personas/_dispatch/`) — was this persona ever actually used, or did its
  work quietly route to a generic agent? Surfaces in `charter persona stats` as the
  `⚑ never dispatched` flag and the persona-vs-generic ratio.
- **Routing advice** (`personas/_dispatch/`, as `{"ts", "event": "advice"}` rows) — how
  often the roster was shown. Paired with dispatches in `charter persona stats`, it is the
  number that can say the block is not working.
- **Skill invocations** (`personas/_skills/`) — a persona's declared `skills:` are preloaded
  into every dispatch of it, so declaring one is cheap to write and expensive to keep. This
  says whether the equipment was worth carrying.

Neither has a secret surface to scan, by construction.

## When a hook fails

Hooks swallow their exceptions. A tally that breaks a turn is worse than a tally that
misses a row, and none of this is load-bearing for the work itself.

**A denial is the exception, and it is load-bearing.** A guard refuses by printing one JSON
object on stdout, so a hook that cannot write has said nothing — and a `PreToolUse` hook
that says nothing is an *allow*. Deciding is still allowed to fail: a payload charter cannot
parse is an allow, as it always was. Refusing is not. When the verdict is deny and the write
fails, the process exits **2** with the reason on stderr, which is the harness's other
refusal channel — every other non-zero status is a non-blocking error and the tool call goes
ahead. That is why the number is 2 and not "any failure": `charter … | head` legitimately
exits 141, and 141 lets the call through (#438).

A write into a buffer is not a delivery, and that distinction is the whole of the fix: a
hook's stdout is a pipe, `print` to a pipe block-buffers, so a `print` into a *broken* pipe
returns cleanly and the error only surfaces when the interpreter flushes at exit — where it
is worth 120, another status that lets the call through. So the verdict is flushed inside
the guard, while "the harness did not get this" can still become a refusal.

The deliberate exception is version skew, in **both** directions — that is the failure shape
this project keeps paying for, so it is the one thing a hook says out loud, once, at session
start.

- **Plugin newer than the CLI** — the manifest dispatches `charter hook <name>` for a handler
  this CLI does not have, so it errors. `doctor` FAILs, which is what makes the SessionStart
  preflight print at all.
- **Plugin older than the CLI** — the manifest never *names* the newer handlers, so they
  simply do not run. Nothing errors; the tallies they would have written read as empty rather
  than absent, which is the more expensive kind of wrong. charter compares what the installed
  `hooks/hooks.json` invokes against what the CLI ships and names the handlers that are not
  being dispatched.

The second is measured by **handler sets, not version numbers**. A plugin one patch behind
that adds no handler behaves identically, so it says nothing — a row that fires on every
version lag is a row people learn to scroll past.

Seeing what actually happened: `charter trace` reports guard denials, tool approvals, secret
warnings, **credential hand-outs** and memory writes for the session.

The hand-outs are the three ways a value leaves charter's own process, one event each —
`secret-exec` (a child's environment or a temp file), `secret-cp` (a file on disk) and
`secret-reveal` (`secret get --reveal`, a terminal). Each row carries the vault, the key
**names**, and for `exec` the environment variable names and `argv[0]`. It never carries a
value, and never the rest of the command line: charter does not substitute a secret into
argv, but a caller may have typed one there. `secret get` without `--reveal` records
nothing, because nothing left — it prints a length and a digest.

So the question `charter trace` can now answer is *which command received the prod token*.
The one it cannot is what that command then did with it; a credential delivered to a
process is that process's from then on.
