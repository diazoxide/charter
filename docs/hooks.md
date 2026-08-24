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

- **Secret leak.** A command whose argv would put a vault's contents into the transcript.
  Needs argv *and* the plane's vault paths, so it cannot be expressed as a static rule.
- **Vault read.** The same invariant on the `Read`/`Grep` tools, which never reach the Bash
  matcher at all.
- **Plane-root branch move.** The plane is not a work tree (ADR 0008); a branch switch there
  is almost always meant for a clone.
- **Plane-root history wipe.** A `git reset --hard` (or `--merge`/`--keep`) in the plane root
  that would take commits off the branch which no remote has a copy of — the command that
  destroyed eleven memory commits in one session. Only that: the unstage
  (`git reset HEAD -- <path>`), `--soft`/`--mixed`, a reset with no ref, and any reset over
  commits that are already pushed all run untouched. It clears itself — `charter save` lands
  the commits and the same command is allowed.
- **One credential.** SSH to a forge, `GIT_SSH_COMMAND`, `-S`/`--gpg-sign`, and the
  `core.sshCommand` family that reaches the same transport by another road (`-c`,
  `--config-env`, `GIT_CONFIG_KEY_n`, and a `git config` write of it). This one *is*
  expressible as a pattern and stays in the hook anyway, so it can explain itself — see
  [git-policy.md](git-policy.md) and ADR 0014.
- **Release floor.** A run the harness reports as `bypassPermissions` may not create a tag,
  push tags, or `gh release create` / `gh pr merge`. `bypassPermissions` means *stop asking
  me*, not *stop knowing things*, and a published version number can never be reused.

A seventh path is not a guard but an allowance: a program the **active persona** declares in
`tools:` runs without a prompt while that persona is active, and only then. It approves the
**program**, so every argument rides along — which is why six things are never smoothed
whatever `tools:` says: destructive subcommands (`kubectl delete`, `charter secret`,
`git clean`, …), interpreters and wrappers whose argument is the real command (`bash -c`,
`python3 -c`, `env`, `xargs`, `npx`, …), any command whose arguments **reach** a vault or
charter's own state, any tool added to that line after the session started — the gate
answers within the set declared before this session could rewrite it, and `persona.md` is a
file the model can write — any command carrying a character the shell would rewrite before
the program sees it, and any command whose **command word is not the file the declared name
refers to**. Each of those is a fall back to the normal prompt; this path never denies.

That last one is newer than the rest and is the same mistake as the first version of the
one before it. `tools: gh` was matched against `os.path.basename`, so `./gh` — a shell
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
`git commit -m "fix #12"` and `kubectl get -o jsonpath={.items}` are no longer smoothed —
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
`git -c core.pager=` — runs it, and in `argv` it looks like any other `--flag=value`. And a
verb can reach the command from a file: the gate reads every word of `argv`, so
`git -c alias.z=clean z` is seen, but a `z = clean` alias already in `.git/config` — which
takes a plain Edit and no Bash at all — makes `git z -xfd` a command whose every word is
innocent. Both fall back to a prompt the moment anything else in the command is unusual,
and both are one prompt away from being seen if you would rather not have the allowance at
all: take the tool off the `tools:` line.

None of that is a denial being dodged. This path is an *allowance*: the fallback is the
normal prompt, and every guard above — the Bash leak guard, the state-write guard — applies
to those commands exactly as before.

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

Three guards name a narrower move first, and it is usually the one you want:

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
warnings and memory writes for the session.
