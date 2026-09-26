# The hooks

A hook is the harness calling charter at a moment nobody had to remember: a session
starting, a prompt, a tool call about to run, a turn ending. Every one of them is a
`charter hook <name>` call. What the app itself runs for a hook is its own binary, by its
absolute path inside the app bundle (see [install.md](install.md)). A hook a plane declares in
its own `.claude/settings.json` as a bare `charter hook …` is found on the chat's `PATH`, which
the app extends with its bundle's directory.

The app ships a Claude Code plugin in its bundle and loads it into each chat it starts, with
`claude --plugin-dir`, so the hooks arrive with the chat and there is nothing to install per
project or per machine. Its hooks name the app's `charter` through `$CHARTER_HOOK_BINARY`,
which the app sets in the chat's environment. A Codex chat is armed with the state hooks and
the Bash guard as `-c hooks.<Event>=…` flags. Both are for that session alone; see
[harnesses.md](harnesses.md#per-profile--armed-at-launch).

## What this version answers

`charter hook --list` prints every word the binary answers, with the event and tool matcher
each is wired to.

| Hook | What it does |
| --- | --- |
| `sessionstart`, `userpromptsubmit`, `notification`, `subagentstop`, `stop`, `sessionend` | reports what the session just did to the app that started it, which is how the window knows which chat needs you. Outside the app it has nobody to tell, except that `sessionstart` starts a background forge refresh |
| `sessionstart`, in a plane | also briefs the session, as `additionalContext`: the workspace gate (confirm a workspace before repo work, unless the session is locked to one or `$CHARTER_WORKSPACE` pins it), the persona it was started as and a digest of that persona's memory, memory not yet shared, the workspace's oldest open todos, the plane's other workspaces, and the piece the session stands in. It also freezes each persona's `tools:` for the persona tool gate below |
| `userpromptsubmit`, in a plane | also adds, as `additionalContext`, the commitment gate: a prompt that asks for work and leaves a real fork open (open-ended wording, a broad scope, something irreversible, a long many-part ask) is told to scout first, then ask the operator at the fork before building. Never on a question, on work with nothing to ask about, or on a slash command; never in an unattended run (`permission_mode: bypassPermissions`); and quiet for the three prompts after it fires. A report a handed-off chat sent back rides the same context |
| `pretooluse` on `Bash` | the guards below, then the persona tool gate |
| `pretooluse-read` on `Read`/`Grep` | the vault guard on those tools (*Vault read*, below) |
| `pretooluse-edit` on `Write`/`Edit`/`MultiEdit` | the state-directory guard (*A hand-written state file*, below) |
| `pretooluse-dispatch` on `Task`/`Agent` | asks before a persona that writes code is dispatched beside an agent already running (*A dispatch beside a running agent*, below) |
| `posttooluse` on `Write`/`Edit`/`MultiEdit`, in a plane | warns when a memory or ref just written looks like it holds a secret; says what the workspace flow expects on the first edit in a LIVE workspace's clone; and every twelfth change without a memory, reminds the session to record one |
| `posttooluse-skill`, `posttooluse-dispatch`, `posttooluse-message`, in a plane | log which skill the active persona used, which persona was dispatched, and a message that resumes one |
| any other `pretooluse…` or `posttooluse…` word | **blocks** the tool call, with the reason on stderr |
| any other word | exits 1 with the reason on stderr, and blocks nothing |

**A reporting hook never blocks.** A harness reads exit 2 as "block", and on `Stop` that
would make the session carry on instead of ending, so every failure on a reporting hook is a
silent exit 0 and the window keeps the last state it was told.

**A tool hook word this binary does not know blocks, and that is deliberate.** Refusing a
tool call is the only safe thing a program that checked nothing can do; answering `allow`
for a guard it has not got would be a hole that looks like a guard. Every word the plugin
wires is answered, so this only ever meets a word nobody has invented yet.

## The guards

These are what `charter hook pretooluse` answers on a `Bash` call, and the two guards on the
file tools beside it. Every one of them **denies**. None of them asks: charter holds no nudge
on the Bash tool.

The guards about the shell itself (the secret leak and the two substitution guards) run in
any directory. The ones whose subject is a control plane (one credential, the two plane-root
guards, the release floor and the handoff guard) run only inside one, because a denial about
a control plane that does not exist explains nothing to the person reading it.

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
  "Argv" now means the real one. A wrapper (`env`, `sudo`, `command`, `xargs`,
  `charter secret exec … --`, a `{ … }` group, a `then` branch) does not change what the
  program is — and where a wrapper opens a
  file *itself* (`xargs -a <file>`) that file counts as read, even though the program named
  on the line is something else. A **redirection** is neither the program nor an operand: it
  may sit in front of the command (`< <vault> cat`), and the target of an input redirection
  is a file the shell opens whatever the program does with it (`tee < <vault>`). A **command
  boundary is an operator the shell would interpret**, so a quoted or escaped one is an
  argument and not a boundary (`cat \) <vault>` reads the vault), and the `&` inside the
  redirection `>&` is not the control operator `&` (`cat 2>&1 <vault>` is one command);
  while a newline *is* a boundary exactly as `;` is — every line of a multi-line command is
  its own command. A **heredoc body** reaches the guard as those commands whenever something
  runs it: a shell that opens the `<<` (`bash <<'EOF'`), or one anywhere in the opener's
  pipeline (`cat x && bash <<'EOF'`, `cat <<'EOF' | bash`), so a vault read on any of its
  lines is denied wherever the reader on the line stands — it is the command that opens the
  `<<`, and its pipeline, that decide, not the first word. A shell inside a **loop or
  conditional** the body is piped into (`cat <<'EOF' | while read l; do eval "$l"; done`) counts,
  and an executor is still seen when it stands behind ANSI-C `$'…'` quoting. **The pipeline is followed across
  lines**: a trailing `|` continues onto the command after the heredoc body (`cat <<'EOF' |`
  … `EOF` … `bash`), and a backslash-newline splices before it, so the downstream shell is
  seen either way. **Newlines, quotes and comments are read as bash reads them across lines**:
  a `<<` inside a quoted string that spans lines opens no heredoc, a comment ending in `\` is not
  spliced into the next line, and — when the command cannot be tokenised at all — it is still cut
  into lines on the newlines bash makes a boundary (not one inside an unclosed quote, not one a
  live backslash spliced away) rather than collapsed on whitespace, so a read on the line after a
  `cd` is still seen. A **quoted** heredoc fed only to a
  reader (`cat <<'EOF'`) is stdin data: its body is dropped, so a document naming these paths
  is not refused as a read of them.
  That body ends where bash ends it: `<<'EO'F`, `<<"EO"F` and `<<E\OF` are all heredocs whose
  delimiter is `EOF`, because quotes are removed per character and the pieces join. So are
  `<<$'E\x4fF'` (ANSI-C quoting is decoded) and a `"…"` delimiter split by a backslash-newline,
  and `<<'A B'` is a heredoc whose delimiter holds a blank. Every guard reads a heredoc this one
  way — which `<<` opens one, where its body ends, whether it expands — so no two guards read
  the same heredoc differently. A body whose terminator never arrives is not dropped at all,
  since that is what a misread delimiter looks like. Nor is the body of a heredoc opened inside
  a `$( … )` or backticks that close on the same line (`x=$( cat <<'EOF' )`): bash 3.2 and zsh
  run the lines after it as commands where bash 5 reads them as the body, so they are read as
  commands a shell runs. The same goes for a `$"…"` delimiter, which bash ends at `EOF` and zsh
  at `$EOF`.
  A **commit message on stdin** is the same data on the same terms: the quoted body of
  `git commit -F -`, `-F-`, `--file=-` or `--file -`, git's global options before `commit`
  included, is dropped when no executor is in its pipeline, so a message describing charter's
  own layout is not refused. Any
  spelling of `--edit` keeps it visible, because git hands the message to the editor and
  `core.editor=sh` runs it; so does a redirection target spelled like the flag (`> -F-`), which
  leaves the heredoc to the editor's stdin. The recogniser is narrow on purpose, and each
  spelling it does not read keeps the body visible rather than hiding one: `git tag -F -` and
  other subcommands, an alias, a short cluster (`-aF -`), an abbreviation (`--fil=-`), `-F -`
  after `--`. What git does with the message after storing it is outside the guard, as a
  script file is: a `commit-msg` hook that runs the file it is handed runs the message.
  A **PR or issue body on stdin** is data on the same terms: the quoted body of
  `gh pr create`, `gh pr comment`, `gh issue create` or `gh issue comment` given
  `--body-file -`, `--body-file=-`, `-F -` or `-F-` is dropped when no executor is in its
  pipeline. Before that, one
  apostrophe in a body left the call unparseable, the body's words became operands of a
  `| tail -1` after it, and two neighbours such as "`~`." and "Charter" joined into `.Charter`
  and were refused as a vault read. `-e`, `--editor` and a short cluster holding `e` keep the
  body visible, as a redirection target spelled like the flag and `-F -` after `--` do; gh 2.83.2
  opens no editor on a body from stdin, but the refusal does not rest on that. `gh pr edit`,
  `gh release … -F -`, `gh api --input -` and `-dF -` are not read. A line holding an
  **escaped** backtick (`` --title "keeps its \`~/\`" ``) is attributed like any other; an
  unescaped backtick, including one behind an escaped backslash (`` \\` ``), is a substitution
  and leaves the line unattributed.
  A body file naming a vault is the opposite case: `-F -` is stdin data, but
  `gh pr create -F .charter/vaults/x.json` (and `--body-file`, `--notes-file`, `-T`/`--template`,
  and `gh api --input <path>` / `--field key=@<path>`) READS that file and uploads it to the
  forge — worse than printing it, because the value lands on the forge. `gh` is not a printer
  and so is not in the reader allowlist, but these flags name a file it opens, so their operand
  goes through the same vault check a reader's does. An ordinary body file
  (`-F notes.md`) and `gh api -f/--raw-field key=@path` (a literal string, not a file) stay
  allowed. A release ASSET named positionally (`gh release create v1 <path>`) is uploaded too
  and is not covered here — a separate finding, not this flag check.
  An *unquoted* body stays visible instead, because the shell expands it before the reader
  sees it and a `$( … )` in it would run. And `#` starts a comment only where a word starts. Position counts too: `{` and `}` are reserved words, so bash passes them as
  plain arguments anywhere but command position and `cat { <vault>` is one command that
  reads the vault. Beyond that: an unparseable quote does not hide the commands after it,
  an **unquoted** `$( … )` substitution is read both as the command it runs and as the word
  it becomes, and a relocation counts however it is spelled (`cd`, `pushd`, `env -C`,
  `sudo --chdir`). The **path** is normalised before the match — redundant separators,
  `.`/`..` segments and letter case — so `.charter//vaults/db.json`,
  `.charter/./vaults/db.json` and `.CHARTER/vaults/db.json` all answer the same as the
  plain form. Two things it still cannot know. A *different* path holding the same bytes: a
  vault registered outside `.charter/`, a copy of a vault at a path you named, or a symlink. And anything a **shell** does to the operand after the hook has
  answered — a glob (`cat .charter/vault?/db.json`), a variable (`V=…; cat $V`), a quoted
  substitution, brace or tilde expansion. The hook runs on the command line, never on what
  `sh` turns it into, so each of those is `cat` on the same inode and allowed. **What it
  does not catch is written down** — see *Where the secret-leak guard stops*, below, for
  why that is the honest scope rather than a defect.
- **Vault read.** The same invariant on the `Read` and `Grep` tools, which never reach the
  Bash matcher (`charter hook pretooluse-read`). A `Read` or `Grep` naming a vault path is
  refused, and so is a `Grep` that would walk into the plane's state directory — one with no
  path searches the directory it stands in, which is the commonest spelling of that walk —
  unless its `glob` selects nothing in there. Like the leak guard it runs in any directory,
  because `$CHARTER_HOME` can put a real vault within reach of one that holds no
  `charter.toml`.
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
  attached or separated, composing with `-C`. The cwd is a subject of every
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
  charter's git-config reader along with the routes it declines: `git -c core.worktree=…` on the
  command line (git ignores it, so it reaches nothing), `include`/`includeIf`, and the
  global and system configs. A `-C`
  counts as git's
  change-directory global only **before the subcommand**, which is the only position git
  reads one in — so `git switch -C <branch>`, where `-C` is `switch`'s own `--force-create`,
  is the branch creation it is rather than a directory called `<branch>`. Every one of those is a row in
  the guard's corpus, crossed with the commands rather than listed beside them. The walk also carries the **environment a command
  line establishes for its later segments**, so `export GIT_DIR=<plane>/.git && git checkout
  <branch>` reaches the same denial the attached `GIT_DIR=… git …` does — as do `declare -x`/`typeset
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
  does `git -c alias.z='reset --hard origin/main' z` — and shares every route above,
  so `git --git-dir=<plane>/.git reset --hard <ref>` from a clone is refused too.
- **One credential.** SSH to a forge, `GIT_SSH_COMMAND`, `-S`/`--gpg-sign`, and the
  `core.sshCommand` family that reaches the same transport by another road (`-c`,
  `--config-env`, `GIT_CONFIG_KEY_n`, and a `git config` write of it). This one *is*
  expressible as a pattern and stays in the hook anyway, so it can explain itself — see
  [git-policy.md](git-policy.md) and ADR 0014.
- **Release floor.** A run the harness reports as `bypassPermissions` may not create a tag,
  push tags, or `gh release create` / `gh pr merge` (and glab's equivalents).
  `bypassPermissions` means *stop asking me*, not *stop knowing things*, and a published
  version number can never be reused. The line runs between *opening* a request and
  *merging* one: `gh pr create` is deliberately not on this list. The floor also refuses the
  spelling `charter change land`, a cross-repo landing command this version does not have yet.

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
  environment variables — vault tokens included — into a public body. Nineteen other issues filed
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
  **`git commit -m "… `x` …"`** is out of scope; and the check is scoped to the
  **whole Bash call**, not to the
  body argument, so `cd "$(git rev-parse --show-toplevel)" && gh pr create --body-file b.md`
  is refused as well. Narrowing that means deciding which argument a substitution lands in,
  which means putting a shell inside the guard — the failure the leak guard already
  documents. Run the substitution in a **separate** Bash call instead; each is judged alone.

  The commit-message limit is worth its own paragraphs, because the obvious reason for it is
  wrong and the real one took a measurement. It is **not** that a commit matters less. On
  the axis the forge case turns on — *can this be undone* — a commit message is **worse** than an
  issue body: a body is replaced in one call, while a pushed commit needs a history rewrite,
  and a rewrite reaches neither forks nor existing clones nor the forge's caches. Visibility
  is not reversibility.

  **The commit surface has since been enumerated**, against real `git … --help` output
  rather than recalled, so the gap this limit opened with is closed. These write a message: `commit`
  (`-m`, which repeats and concatenates; `-F`; `-t`; `-C`/`--reuse-message`;
  `-c`/`--reedit-message`; `--squash`; `--fixup`), `tag` (`-m`, `-F`), `merge` (`-m`, `-F`),
  `notes add|append|edit` (`-m`, `-F`, `-C`, `-c`) and `stash push|store` (`-m`). `revert`
  and `cherry-pick` have a `-m`, and it is `--mainline` — a parent number, not prose, which
  is the kind of thing a recalled list gets wrong.

  **It stays out anyway, and the reason is calibration.** Measured on `main`:

  - **29 of the last 30 commit messages carry a backtick** (160 of 200), and inside
    `-m "…"` every one of those is live;
  - **all 30 of the last 30 are multi-line**, which a single `-m "…"` cannot produce. The
    spelling that writes them is `-m "$(cat <<'EOF' … EOF)"` — and that line **is itself a
    live `$(`**, while the backticks inside the quoted heredoc are inert.

  So a liveness-keyed guard on `git commit` would refuse the exact form that makes those
  backticks harmless: its trigger would be the prescribed workflow, which is not a
  miscalibrated guard but an inverted one — and that is the argument which deleted the
  clone-commit nudge outright rather than narrowing it (see *What charter stopped asking*).
  Exempting `$(cat <<'QUOTED')` by name
  would mean the guard deciding which substitutions are *safe* rather than which are *live*,
  and a parser that gets that wrong fails open. `charter save <message>` writes a commit
  message too, so it is out for the same reason rather than by oversight. `git commit` is therefore a **stated limit, not an open
  question**.

  **What it does not reach, checked rather than assumed.** The guard matches a
  `(tool, noun, verb)` triple, so every route that publishes without spelling one is
  outside it — and the nearest of those is not exotic:

  ```bash
  gh api repos/o/r/issues -f body="`env`"    # ALLOWED — same publish, no noun and verb
  ```

  `gh api` is the REST escape hatch and it writes as well as reads; covering it means
  deciding which invocations write, from `--method` or from `-f`, and that is a surface
  nobody has verified the way the nineteen verbs were. A **user-defined alias**
  (`gh alias set ic 'issue create'`) is the same hole one step further, and so is a program
  name that arrives in a variable (`$GH issue create`). Below those sit the routes any
  guard on argv is blind to and `_leak_reason` already lists: `sh -c '…'`, `eval`, a `curl`
  straight at the API, an interpreter. None of that is a reason to distrust the denial you
  do get — it is why this is a guard against a mistake an agent makes while trying to do
  its job, and not a boundary.

  Unlike the four guards above it, this one is **not gated on there being a control
  plane**. What it refuses is a fact about the shell rather than a policy this plane holds,
  and its remedy is plain `gh`/`glab` usage — the same reason the secret-leak guard is
  ungated.

- **charter's own text substitution.** The same rule on charter's own text-taking commands. The guard above covers somebody
  else's tools; these persist prose that this plane commits and pushes, so the same defect
  reaches a public repository by an indirect route:

  ```bash
  charter persona remember "the marker is appended to `pending` each pass"   # DENIED
  charter persona remember "the marker is appended to \`pending\` each pass"  # allowed
  ```

  That is not a constructed example. It happened while a guard review's findings were being
  written up: zsh ran the word, printed `command not found`, spliced its empty output, and
  the saved memory read *"appending to  each pass"* with the word silently gone — into
  `personas/_shared/memory/`, which is committed and pushed.

  **The rows:** `persona remember`, `workspace remember|note|todo|vision` (and the `ws`
  alias). The list also holds the spellings of text-taking commands this version does not
  have yet — `persona log`, `worktree abandon` (`wt`), `change create|drop` and
  `report bug|gap` — so the rule is already in place when those commands arrive.
  The line for inclusion is the one that keeps `gh pr merge --body` out above: the free text
  has to be required or the primary operand, and what holds it has to be read back as prose.
  So `workspace create --vision` and `workspace snapshot --description` are outside it, where
  the prose is a secondary attribute of creating a thing.

  **The remedy is not the forge guard's, and that took measuring.** Of the 284 committed
  memory bodies on `main`, **221 contain an apostrophe** — and *all nine* of the
  backtick-carrying ones do — so "single-quote it", the obvious answer, fails on exactly the
  text this refuses. **Backslash-escape each backtick** instead: inside double quotes
  `` \` `` is a literal backtick and the apostrophes keep working. The memory commands have
  no file input, so the denial names no flag. And if you *meant* to interpolate a computed value, compute it in a **separate** Bash call
  and pass `"$VAR"` — a parameter expansion is not a substitution, and a shell does not
  re-expand a parameter's value.

  **Why this one is a guard where `git commit` is a limit** is the same question answered by
  two different corpora. 13 of those 284 bodies would meet this guard — **5%**, not the
  87% the issue predicted, and 254 of the 284 predate the working rule that warns against
  the shape, so that is charter's natural prose. The reason is structural: a commit message
  is rendered as markdown by a forge, so agents write code spans in them, while a memory
  body is read back by `charter recall` in a terminal, so they write *"the `$(` branch"* as
  words. And **283 of the 284 bodies are single-line**, so none came through the
  `"$(cat <<'EOF' …)"` spelling that makes a backtick inert — a live backtick in a
  single-line double-quoted operand is a command that was going to corrupt its own text.

  **What it does not reach**, on the same terms as the guard above: `python3 -m charter …`
  is covered, but a shell wrapper (`sh -c 'charter …'`), an alias, and a program name arriving in a variable are
  not — nor is `python3 -mcharter`, a fail-open hole named in the guard rather than closed
  with a short-option parser inside it. It reads the first two words after `charter`, which
  is exact only because charter's root parser has no option that takes a value — a test
  asserts that, so if one is ever added the guard is told rather than quietly
  under-reading. Ungated on there being a control plane, for the reason above.

- **A handoff the prompt cannot stand in front of.** A handoff's brief becomes a new chat's
  first message and runs with your authority, so its consent is your harness's own permission
  prompt: the `ask` rule for `charter handoff *` that `charter init` writes (see
  [handoff.md](handoff.md), *The prompt is the consent*). This guard refuses the handoffs it can
  recognise that the prompt would not stand in front of. Each refusal needs a fact no command
  pattern can see.

  | Refused | Why the prompt cannot cover it |
  | --- | --- |
  | a call from a **sub-agent**: the payload carries `agent_id` | You are talking to the parent chat, and what the sub-agent found goes back there anyway. Measured on Claude Code 2.1.268 and codex-cli 0.147.0: a sub-agent's Bash call carries `agent_id` and a main-conversation call does not. A harness nobody has measured is not read this way. |
  | an **unattended run**: `permission_mode: bypassPermissions` | Nobody is there to answer the prompt. The refusal names `charter ws todo` as the way to keep the work. |
  | a **spelling** of `charter handoff …` it can recognise as other than the exact one: a wrapper, a prefix, a path or `python3 -m charter`; a word quoted or escaped; a word that still reads `charter` or `handoff` once its quoting, expansion and glob characters are removed (`$'handoff'`, `${x:-handoff}`, `{handoff,}`), or that `handoff` matches as a glob (`hando?f`); a gap other than one ASCII space before or after `handoff`, a line continuation included | On Claude Code 2.1.268, `python3 -m charter handoff`, a path to charter, `charter 'handoff'`, `charter $'handoff'`, `charter {handoff,}` and `charter hando?f` ran with no prompt. A `FOO=1` prefix, an `env` wrapper, a quoted `charter`, two spaces and a tab were matched there and are refused anyway, so a model has one spelling to follow. The first two words are judged as written, never as a shell would rewrite them — which is also why a brace split inside a word (`{hand,}off`) and a parameter default split across one (`hand${x:-}off`) are not recognised; the first of those ran with no prompt too. An ANSI-C word is the exception, because the shared reader decodes it the way the shell does: `charter $'\x68andoff'` is `charter handoff` spelled another way, and is refused as one. |
  | a handoff **inside a string or a heredoc a shell runs**, one level deep: `eval`, or `sh`, `bash`, `zsh`, `dash`, `ksh` with `-c` (alone or in a cluster such as `-lc`) or reading a heredoc body (`bash <<'EOF'`) | The rule reads the outer command: on Claude Code 2.1.268, a handoff inside `eval '…'`, `bash -c '…'` or a `bash <<'EOF'` body ran with no prompt. The refusal says to run it directly. Which heredoc bodies a shell runs is the same answer the leak guard uses, so a brief is never one of them. |
  | a **stdin** other than one quoted heredoc on the handoff's own segment: an unquoted `<<BRIEF`, a pipe, `< file`, `<<<`, no heredoc, two heredocs, or a live `$(…)` anywhere in the call | The prompt has to show the exact text the new chat is sent. An unquoted heredoc expands before charter reads it, a file shows as a path, and with two heredocs bash hands the command only the last body (GNU bash 3.2.57). |

  **Text that only mentions a handoff is not one.** A heredoc body a reader takes (`cat > f
  <<'EOF'`), a quoted argument (`grep 'charter handoff' docs`), an `echo`'s words, and the
  later lines of a quoted string that spans lines (a `git commit -m '…'` message, a
  `python3 -c "…"` script) are data, and are not searched for a handoff. What a shell runs is:
  a `-c` string, `eval`'s words, and a heredoc fed to a shell. Where a multi-line quote closes
  partway along a line, the rest of that line is a command again and is judged as one.

  **Which heredoc bodies A7 searches.** A body is searched when its OWN opener is a shell or an
  interpreter (`bash`, `sh`, `python3`, `perl`, one of those behind `env`/`nohup`, or `ssh`,
  whose remote shell runs it); when charter cannot resolve the opener to a name
  (`${RUNNER} <<'EOF'`, decided at runtime, or `$(which bash) <<'EOF'`, a word out of a
  substitution); or when an executor stands downstream of the opener **in the same pipeline**,
  since `cat <<'A' | bash` is a script where `cat <<'A'; bash` is not. Any other opener hands its
  body on without running it, so the body is data: `git commit -F -`, `tee`, `mail`, `wc`, every
  reader. Each heredoc is judged by its own opener and its own pipeline, so
  `( cat <<'A' > notes.md; bash <<'B' )` searches only the second body — except that when two or
  more heredocs share one `$( … )` and any is a shell's, every body there is searched, because
  bash's ordering inside a substitution does not match the attribution. That reaches exactly
  that shape: a substitution holding ONE heredoc, or only readers, is not covered, and a shell
  can run the handoff in those. Downstream, only a program that can be NAMED counts — an
  unresolvable *opener* is a reason to search, an unresolvable or remote *downstream* member is
  not, so `cat <<'A' | ${RUNNER}`, `cat <<'A' | ssh host` and `( cat <<'A' |& bash )` are
  allowed while a shell runs the handoff. A `<<` inside quotes is
  not an opener at all, while a `<<` inside `$( … )` is one even inside quotes — the spelling
  `git commit -m "$(cat <<'EOF' … EOF)"` depends on that. Inside `"…"` a bare `$` is a literal,
  so `$'` opens nothing there and `grep -v "^$" f` is a filter rather than a quote.

  That scan does **not honour `#` comments**: a `'` or `"` inside one still opens a quote, so
  `echo #' && bash <<'ZZ'` reads the rest of the line as quoted, the real opener is never seen,
  and the handoff in that body runs with no prompt.

  An **ANSI-C word** (`$'don\'t'`) is read correctly by that scan, and the shared reader
  decodes it too, as the shell does. A7's own mis-reading of `$'` inside `"…"` erased real
  openers until it was fixed.

  **The brief is data to the secret-leak guard.** The body of a heredoc on the handoff's own
  segment is stdin charter sends on, never a command the shell runs, so it is skipped the way a
  reader's is. A brief that names `.charter/vaults/…` in prose, holds one apostrophe, or
  opens a line with a reader (`cat .charter/vaults/dev.json would print it, so never run
  that.`) is not refused as a read. Only that body: in `charter handoff beta && bash <<'EOF'` the body belongs
  to `bash` and is read as commands.

  Gated on a control plane, unlike the two substitution guards above: this is a policy about a
  plane's chats, not a fact about the shell. **What it does not reach:** Codex has no
  command-pattern permissions, so an attended Codex chat's handoff runs without a prompt, and
  `codex exec --approve-for-me` reports `permission_mode: default`, so it is not refused as
  unattended. `charter doctor` does not check the handoff gate yet, so nothing names that gap
  on a plane but this page.

  **What it does not see, on any harness.** It refuses the spellings of a handoff it can
  recognise, so a chat working in good faith keeps the prompt in front of its handoff; it reads a
  command's words and is not a shell. A handoff run by an interpreter (`python3 -c`, `node -e`, or
  `os.system` inside a `python3 - <<'PY'` body),
  through a variable, from a script file, behind an expansion that does not leave the word whole
  (`{hand,}off`, `hand${x:-}off`), or more than one string deep is not seen.
  Nor is a shell behind a **name charter cannot know**: `r() { bash; }; r <<'EOF'` defines a
  function and calls it, so the opener reads as `r` and its body is treated as data — the same
  class as an interpreter or a script file. The same rule costs the other direction, which is
  the price of the fail-safe: **when the word that NAMES THE PROGRAM is itself a variable or a
  substitution** charter cannot name the program and treats that body as something that could
  run, so a brief-shaped body is refused even when the program is an editor or a pager. An
  expansion elsewhere on the line — a redirect target, an argument — does not, in any of the
  three spellings: `( tee ${OUT} <<'EOF' )`, `( tee "$(mktemp)" <<'EOF' )` and
  `( tee "`mktemp`" <<'EOF' )` all name `tee` and are allowed. Measured on
  `( ${EDITOR} <<'EOF' )`, `( ${PAGER} <<'EOF' )`, `( ${GIT} commit -F - <<'EOF' )` and
  `( $(which tee) notes.md <<'EOF' )`. Those are the same shape as `( ${RUNNER} <<'EOF' )`,
  where the variable really is a shell, and the only thing that would separate them is the
  body's content — which A7 must not use, since a brief is indistinguishable from prose naming
  the feature. Spelling the program out avoids the prompt.
  The look inside `eval` and `sh -c` strings reads the call with reader heredoc bodies removed,
  so a body that is NOT a reader's — a `python3 - <<'PY'` or `tee` body, or a
  `git commit -F -` spelling the leak guard does not read as a message — holding a lone `'` (as
  in `don't`) leaves the call unparseable and the look is skipped, so a handoff in a later
  `eval '…'` or `bash -c '…'` is allowed; a `cat` body, a quoted `git commit -F -` message or a
  quoted `gh pr create --body-file -` body is stripped first and costs nothing. Claude Code says the same of its rule:
  a Bash rule "isn't a security boundary around the program"
  ([What a Bash rule doesn't match](https://code.claude.com/docs/en/permissions#bash-rule-limits)).

- **A hand-written state file.** A `Write`, `Edit` or `MultiEdit` into charter's state
  directory (`.charter/`, or `$CHARTER_HOME`), which holds the persona tool gate's frozen
  ceiling and the persona pointers (`charter hook pretooluse-edit`). The target is resolved
  through links before it is compared. Gated on a control plane.

## Two answers that are not denials

- **The persona tool gate.** When nothing above refused a `Bash` call, charter asks whether
  the active persona's `tools:` declares the program, and if it does answers `allow`, so the
  harness does not prompt. It never denies; the worst it can do is leave the prompt. It
  declines a command the shell would still rewrite (`$`, `~`, `*`, `;`, `|`, `>` and the
  like), an interpreter or wrapper (`bash`, `python`, `env`, `sudo`, `xargs`, `find`,
  `make`), an argument that names another program, a destructive subcommand, and anything
  that touches charter's control surface. The ceiling is frozen at `SessionStart`: a session
  that edits its own persona's `tools:` can narrow the grant mid-session, never widen it.
  Gated on a control plane.
- **A dispatch beside a running agent.** A `Task`/`Agent` call that sends out a persona
  declaring `dispatch-isolation: worktree` while another dispatched agent is still running is
  **asked** about, because the two share one working tree and their edits interleave. In an
  unattended run there is nobody to ask, so the note is given and the call goes ahead. Gated
  on a control plane.

## Where the secret-leak guard stops

charter's position is **guard rails, not guarantees — a guard against mistakes, not an attacker with shell access
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
it, is not winnable in a tokeniser, so the honest move is to say what is open:

- **a quoted command substitution** — the example above, and `` "`cat <vault>`" `` and
  `"$(<vault>)"`. Two command families are the
  exception, and they are an exception for a different reason rather than a fix for this
  one: a `gh`/`glab` command that publishes prose, and a charter command that persists it,
  are refused whenever a live substitution stands on the line — so
  `gh issue create --body "$(cat <vault>)"` and
  `charter persona remember "$(cat <vault>)"` both stop. See *Forge body substitution* and
  *charter's own text substitution* above. Neither guard looks inside the substitution or
  knows anything about vaults; they refuse the shape. Everywhere else on this page, a quoted
  substitution is still open;
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
makes a vault not worth reading is keeping the value in a system built for custody and
resolving it on demand, so there is no plaintext on disk for any of the above to print. That
is the control; the hook is the guard rail. `charter secret exec` hands a value to a command
without anyone reading it ([secrets.md](secrets.md)).

## A line that looks like a secret, in memory or a brief

A second guard reads text rather than commands. Three places ask it the same question:
`charter save` before it commits a memory or ref file, `charter handoff` before it sends a
brief, and the `PostToolUse` hook after a memory or ref is written, which warns rather than
refuses. Each answer is a **kind**, never the text it matched: an AgentMail key, a
JWT, a PEM private key, an AWS access key, or a **credential assignment**, which is a
`password`, `passwd`, `api_key`, `apikey`, `secret` or `token` followed by `:` or `=` and six
non-blank characters.

**Naming where a credential lives is not a credential assignment.** The brief refusal tells
you to do exactly that, and the rule used to refuse the answer whenever it was one word:
`api_key = vault:forge/api-token` and ``token: `charter secret get forge token` `` were both
refused as credentials. A value in
one of the four spellings charter uses is now let through, with at most a quote or backtick
on each side: `vault:<vault>/<key>`, `charter secret get <vault> <key>`, and the two URIs a
`reference` vault stores, `op://<vault>/<item>/<field>` and `vault://<path>#<field>`. Because that happens in the one classifier, every
place gives the same answer. **The
whole value, to the end of its line, has to be the reference, and its names have to look
like names**. No name may start with a prefix a credential issuer puts on its tokens (`ghp_`,
`github_pat_`, `glpat-`, `sk_live_`, `sk-`, `xoxb-`, `AIza`, `pypi-`, `npm_`, `hf_`, `AKIA`
and the rest of charter's list of credential prefixes). And all the names together — every vault, key,
item, field and path segment, counted without the scheme or the `/`, `#` and space between
them — come to **at most 32 characters**. The cap is on the total rather than on each name
because a secret can hold a `/`: capped per name, AWS's documented example secret key
written as a `vault://` path split into short segments and passed. So all of these are still
refused:

- a bare `forge/token`, because a secret can contain a slash;
- a token typed into any slot of a reference — `vault:forge/ghp_…`,
  `charter secret get forge <40 hex>`, `op://<token>/item/field` — by its prefix or by the
  length it adds, which is the accident this rule exists for;
- a secret of more than 32 name characters however it is split across names, including a
  `vault://` path of many short segments;
- a real value beside the reference, glued onto it, or in a second assignment on that line
  or the next;
- prose after the reference on the same line;
- any other spelling, including `$(charter secret get …)`, `op:/…`, an `op://` with more or
  fewer than three names, and a `vault://` with no `#<field>`.

The four other kinds are checked on the whole text whatever the assignment says:
`vault:forge/xAKIA…` is still an AWS access key. **The length and prefix rule has a ceiling
of its own:** a secret of at most 32 name characters, not counting the `/`, `#` or single
spaces that separate them, that starts with none of those prefixes reads as names and
passes — so a secret holding k separators passes at up to 32 + k characters, and
`vault:<16>/<16>` is a 33-character value that passes. And it costs the other way: an ordinary reference whose names add
up to more than 32 characters, `vault://secret/data/production/payments#stripe_api_key`, is
refused as a credential.

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

Some guards name a narrower move first, and it is usually the one you want:

- **Forge body substitution** — `--body-file <path>`, or `--body-file -` with a quoted
  heredoc. This one is rarely wrong about the shape and often wrong about the intent: the
  body you meant is exactly the body you get, and it is the shorter line to type anyway.
- **charter's own text substitution** — backslash-escape each backtick, which is one
  character and leaves the apostrophes in your prose working.
- **Release floor** — re-run the step **attended**. This is a mode, and it is yours to set.
- **One credential** — `charter git-policy --apply` configures every clone for the token
  transport, which is what most denials of it are actually asking for.
- **Plane-root history wipe** — `charter save`. The guard is measuring commits that exist
  nowhere else; push them and it stops firing, on that command and every other one.
- **A handoff the prompt cannot stand in front of** — spell it `charter handoff <workspace>
  <<'BRIEF'`, from the chat the operator is talking to, attended. Three of its four refusals
  have that as the fix, and the fourth (a sub-agent's call) is answered by returning what the
  sub-agent found to the parent chat, which can propose the handoff itself. Your own terminal
  is the override here too, and it does something different: outside the app, the command
  prints what to run rather than opening a chat ([handoff.md](handoff.md)).

**If a guard is wrong about you *every time*, that is not an override problem.** It means
charter is holding a policy your organisation does not — an org that mandates signed
commits, say. Switching the guard off locally hides that; the fix belongs in the rule.
[Open an issue](https://github.com/diazoxide/charter/issues).

**The thing that is not an override**, named here so nobody finds it by accident and
believes they found the switch: removing charter's hooks from `.claude/settings.json`, or
disabling the plugin. That takes out every guard, the briefing and every log together,
because one of them was wrong once. It is an uninstall.

## What is not injected, and not counted

`SessionStart` briefs the session (see *What this version answers*), but not with the brief
of a handed-off chat that reopened empty. `UserPromptSubmit` injects the commitment gate
and nothing else of its own: no persona roster (`routing:` is retired — personas reach the
harness as sub-agents), no placement advice, and no "control plane updated" note — a chat
running on instructions that changed after it started is marked on its tab in the window
instead. The dispatch and skill logs are kept; no hook keeps a tally of routing advice or
handoffs, and no trace of verdicts is written.

## When a hook fails

A reporting hook swallows its failures, for the reason above: nothing charter draws is worth
wedging a session over.

**A denial is the exception, and it is load-bearing.** A guard refuses by printing one JSON
object on stdout, so a hook that cannot write has said nothing, and a `PreToolUse` hook that
says nothing is an *allow*. Deciding is still allowed to fail: a payload charter cannot parse
is judged as an empty command, and no guard fires on that. Refusing is not allowed to fail.
When the verdict is deny and the write fails, the process exits **2** with the reason on
stderr, which is the harness's other refusal channel; every other non-zero status is a
non-blocking error and the tool call goes ahead. The verdict is flushed inside the guard, so
"the harness did not get this" can still become a refusal.

**There is no version skew to report.** The plugin, the hooks it declares and the binary
they call ship in one bundle and move together when the app updates.
