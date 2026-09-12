# A harness profile belongs to one machine

`Harness.binary` is a class attribute and `launch_argv` returns `[binary, *extra]`. Nothing
overrides either, so charter has run one program per harness kind, one way, and an operator
with a work Claude Code account in one `CLAUDE_CONFIG_DIR` and a personal one in another has
had nowhere to say so. A shell alias is no answer: charter runs the binary with no shell —
`execvp` where there is no frame, tmux's own exec in a pane, `shutil.which` only as a
pre-check — so an rc-file alias never resolves.

A second failure sits beside it. Bare `charter` launched `[harness] default`, and `+`
launched the kind of the chat it was pressed from, so opening charter on an empty workspace
started a harness session before anybody had said which one they wanted.

A **harness profile** answers both: a name, a kind, a command and an environment, declared by
the operator, and a chat that starts on the one somebody picked. This record holds the
decisions that were expensive to reverse and the reason each rests on. The mechanics — every
key of the table, every refusal and its fix — are in
[`docs/control-plane.md`](../control-plane.md), and the grill that settled them, ruling by
ruling, is `workspaces/harness-profiles/workspace.md` in the plane this was built on.

Written while the feature lands in stages
(`docs/superpowers/plans/2026-09-11-harness-profiles.md`): the decisions below are settled,
and the code for each arrives with the task that owns it.

## The click is the reason for most of what follows

A profile's `command` runs on a click. Between pressing `+` and `os.execvpe` there is no
harness permission prompt, no tool call a guard could deny, nothing that shows a human the
words about to be run — the command goes to tmux, and tmux runs it. Everything in this
record that looks like suspicion of the operator's own file is that sentence, applied.

**So profiles live only in `charter.local.toml`, beside `charter.toml` and out of git.**
Accounts and install paths belong to one person's machine, and a command in the committed
file could be changed by a merged pull request, or by a chat writing the file it is running
in, and then run on every machine that pulls it. A `[harness.<name>]` table in `charter.toml`
is refused with a pointer to the local file. The committed `[harness]` keeps `default` and
nothing else changes there.

**The local file carries `[harness]` and nothing else**, refusing every other section by
name. A general overlay would let an ignored file change plane policy with no trace in git —
`[[forge]]` hosts steer the one-credential guard (`charter/gitpolicy.py`) — and "override"
has no definite meaning for a list table anyway. `[frame]`'s look keys are the likely next
section to be admitted, each on its own reason.

**"Ignored" is guaranteed rather than hoped for.** `charter init` writes
`/charter.local.toml` into a new plane's `.gitignore` and `charter reinit` backfills it —
which needed an amendment to [ADR 0017](0017-charter-ignores-what-carries-credentials.md),
whose rule covers a path charter creates that carries credentials, and this file carries
none. Charter does not then trust its own line: while git tracks the file, or would commit
it, the profiles in it are refused by name and `doctor` warns. A plane that is not a git
repository has nothing to commit to and passes; any other answer git cannot give — git
missing, a timeout, output that does not parse — refuses too, because an unknown is not a
pass ([ADR 0009](0009-errors-classify-they-do-not-guess.md)). The check is one
`git --no-optional-locks status`, so it takes no `index.lock` from a `charter save` running
beside it, and it runs where a person asked — a launch, the selector, `charter harness list`,
`charter harness install`, `charter doctor` — never on a config read, so no hook pays a git
call per tool call. One hook pays one per session: SessionStart runs `charter doctor`, so a
plane that has a `charter.local.toml` makes that single `git status` at each session start.
Named here rather than left to be found.

**A new or changed command asks once.** Charter records each profile's `kind`, `command` and
`env` as last launched, under `.charter/`; a profile with no record, or with a different one,
shows its command and asks `run this? [y/N]` before it runs. Built-ins never ask. Why an ask
and not a rule: once the file is ignored, an edit to it leaves no diff for a reviewer to
catch, and nothing stops a chat editing plane config. Codex trusts its hooks by hash for the
same reason. After a yes, the whole chain of checks runs again from the top before the
`exec`, so a yes never walks past a refusal standing behind it; an open nobody is at refuses
where it would have asked, rather than waiting on a question nobody can see.

## No credential goes in `env`

A variable set on the harness process reaches the shell the model runs. Measured on Claude
Code — the grill's own tool shell saw `CLAUDE_CODE_MESSAGING_TOKEN` — and on Codex, whose
default `shell_environment_policy` passed a `*_TOKEN` probe straight through (`codex
sandbox`, 0.147.0). So an `env` name containing `KEY`, `TOKEN`, `SECRET` or `PASSWORD` is
refused, and the refusal names the harness's own login in the config folder the profile
already moves: `CLAUDE_CONFIG_DIR` and `/login`, `CODEX_HOME` and `codex login`,
`XDG_DATA_HOME` and `opencode auth login`.

A vault reference was considered and rejected on the same measurement: resolving
`vault:work/token` at launch puts the value on the harness process, which is where the model's
shell reads it — the key would only rest somewhere else first. **Charter declines to hold a
credential in a profile, and cannot prevent one**: a wrapper script named in `command` can
export whatever it likes, and charter sees a program name.

## Unwired refuses to launch, built-ins included

Measured on claude 2.1.268, codex-cli 0.147.0 and opencode 1.18.23, in throwaway folders with
no login: **a harness pointed at another config folder loads none of charter's wiring.**
Claude Code lists no charter plugin — not even "enabled but not installed" — even in a
directory whose `.claude/settings.json` enables it, because the `charter` marketplace is
known only to `~/.claude/settings.json`. An empty `CODEX_HOME` holds no plugin, no hook trust
and no `shell_environment_policy.set` line. opencode under another `XDG_CONFIG_HOME` loads no
shim, and its shells are handed no `CHARTER_HARNESS`.

A chat in that state looks guarded and is not. So a profile whose wiring charter cannot find
refuses to launch and prints the fix, and no flag launches one unguarded. **The refusal
covers the built-ins too** — `charter codex` where nobody wired Codex, `charter opencode`
where `init` never wrote the shim, both of which run today — because a chat that looks
guarded and is not is the same failure whichever profile started it.

Wiring is detected by **asking the harness under the profile's own environment**, never
inferred from which variables the profile sets: one account is reachable through variables
that do and do not move the plugin. Claude Code's `CLAUDE_SECURESTORAGE_CONFIG_DIR` moves the
login alone by its own binary's code, and opencode's login follows `XDG_DATA_HOME` while its
plugins follow `XDG_CONFIG_HOME`. Codex is the exception that cannot be asked — `codex plugin
list` answers the same for an empty home and a wired one — so charter reads that home's
`config.toml` instead, and counts it wired only with all three marks present.

A probe that cannot answer — a timeout, a non-zero exit, output that does not parse —
refuses the launch and names the probe to run by hand. Same rule as the git answer above, and
the same ADR 0009 behind it.

## `CHARTER_HARNESS` stays the kind, and the profile rides beside it

The obvious move is to put the profile's name in `CHARTER_HARNESS`, where every hook would
find it. It is wrong: hooks compare that variable to `claude-code` for session ids, resume
and the working spinner (`hooks._turn_begin`, `hooks._record_harness_session`), and a value
of `claude-work` would make each of them quietly answer "not Claude Code". `CHARTER_HARNESS`
keeps the registry's name for the kind. The profile is a field of its own —
`CHARTER_HARNESS_PROFILE` — set by the launcher at the `exec` rather than through tmux, and
recorded in the chat's state and in the reopen manifest. It never joins
`commands_frame._FRAME_IDENTITY`, because that would put it on a tmux `-e`.

**Every chat pane starts as a charter launcher that `exec`s the profile.** Three reasons, the
first a constraint: a profile's `env` reaches the harness without being added to
`layout.CARRIABLE`, which raises on every name it does not know, and without passing through
tmux's own argument parser, which has already cost this repo #957 and #961; one process runs
the checks for every open — the CLI, the selector, `+`, a reopen, a handoff; and `exec` keeps
the launcher's pid, so the harness is the process the launcher was, and the pane id charter
recorded, `remain-on-exit` and the `pane-died` path all see what they see today. What that
launcher may put on the screen before the `exec`, and why it has to hold the pane rather than
print and exit, is [ADR 0018](0018-charter-may-run-the-harness-but-never-draws-it.md)'s
2026-09-12 amendment.

## No harness starts until somebody picks a profile

Opening charter on a workspace with no running chat, and `+`, show a **profile selector** in
the chat's own pane; the harness starts there once a row is picked. It shows even when one
profile is available, because skipping it would bring back the harness nobody picked on a
one-harness machine, and one profile costs one Enter. Esc closes that window having started
nothing — which is what #518 could not offer when it put the workspace picker *before* tmux,
because cancelling there meant tearing down a launch that had half happened. Here nothing has
happened: no harness ran, and no identity was recorded.

The workspace prompt stays before tmux for the reason it was put there: a workspace *is* a
tmux session, so charter has to know which one before a frame exists. A profile belongs to
one chat, so choosing it belongs in that chat's pane.

Every open that nobody is at names its profile instead: `charter <profile>`, a reopen, a
restored plane, a chat handed off by another chat. **A reopened chat whose profile is gone is
skipped by name, never given another.** Another profile may be another account, where that
chat's resume id does not exist and where its workspace's code was never meant to go. It stays
in the manifest, so declaring the profile again and running `charter reopen` brings it back.

**There is no `charter harness add`.** A chat can run a command as easily as it can edit a
file, so the command could never stand for the operator's approval of what it wrote — it
would buy typing and nothing else. `charter harness list` shows every profile charter read,
the file it came from, and why any was refused.

## Refused by name, and the name stays refused

A broken profile is refused alone and the rest still load, because a missing profile is a row
that is not in the selector — easy to miss in a way a missing panel is not, which is why
`[[frame.component]]` makes the opposite trade and refuses its whole arrangement.

Two refusals are worth recording with their reasons, because both look like fussiness:

- **A key inside a profile other than `kind`, `command` and `env` refuses that profile.** A
  typo such as `enviroment` would otherwise drop `CLAUDE_CONFIG_DIR` and launch the default
  account without a word — the failure the feature exists to prevent, arrived at by spelling.
- **A declared profile that replaces a built-in and is refused takes the name down with it**,
  rather than letting the built-in stand in. The operator said how `claude` runs on this
  machine; running the default in its place runs the command they replaced.

## What was considered and rejected

- **One `"kind: command args"` string.** It has nowhere to put environment variables, which
  is how a second account is usually selected in the first place, and it would make charter
  split a string the way a shell does. Every later option would have been more invented
  syntax.
- **A per-user `~/.config/charter` file.** The operator's call: profiles are per plane and
  per machine, and a plane's own directory is where its other configuration is.
- **A full overlay in the local file.** An ignored file that could restate `[[forge]]` would
  move plane policy with no trace in git.
- **A vault reference in `env`.** The value lands on the harness process either way.
- **Setup at launch.** A click would write a plugin into a second account unasked, and Codex
  ignores hooks nobody approved, so the click would not even work.
- **The selector before tmux, beside the workspace prompt.** A profile belongs to one chat,
  and there is no chat yet at that point.
- **A `charter harness add` command.** Above: it cannot be approval.

## Consequences, including what this costs

- **The launch record and the wiring cache live under `.charter/`, which is as writable by a
  chat as `charter.local.toml` is.** The ask catches a changed command only where whatever
  changed it did not also forge the record, and a launch therefore never trusts the cached
  wiring answer — it probes fresh, and the selector's cache draws rows and decides nothing.
  Charter adds no guard for either path: a path pattern is host policy
  ([ADR 0014](0014-policy-that-fits-a-pattern-belongs-to-the-host.md)).
- **A launcher's claim to be in a frame is a guard rail, not a boundary.** It proves the claim
  by being its pane's first process — its own pid equal to the `#{pane_pid}` of a live pane
  that belongs to the chat it names, read from tmux — because `$TMUX_PANE` and
  `$CHARTER_SESSION_ID` are inherited by a model's tool shell and prove nothing. Measured
  2026-09-11: a framed chat's Bash tool saw `TMUX_PANE=%3195`, the harness pane of its own
  chat, so a pane-id proof would have passed for a model running `charter frame-launch` by
  hand. A process that deliberately starts its own tmux pane in a window named like a chat
  still passes the pid proof. It stops an accident, not an attempt.
- **The `env` refusal is a pattern match, so it can refuse an innocent name** —
  `KEYBOARD_LAYOUT` contains `KEY` — and a wrapper script on `PATH` can still export a key
  charter never sees. This is a refusal to hold one, not a barrier against one.
- **Asking a harness is not free and not read-only.** A probe costs about 215–281 ms for
  `claude plugin list --json` and 720–750 ms for `opencode debug config`, and
  `claude plugin list --json` writes `.claude.json` into the config folder it runs against
  (measured). That is why no probe runs on a hook path, SessionStart's preflight included,
  and why `doctor` spends a subprocess per profile only when a person ran it.
- **A one-harness machine pays one more keypress** at every bare `charter` and every `+`.
  Bought deliberately: the alternative is the harness nobody picked, which is one of the two
  failures this record opens with.
- **`[harness] default` launches nothing any more.** It chooses the row the selector starts
  on, and a `default` naming a profile this machine lacks marks no row and makes `doctor`
  warn, rather than refusing a launch.
- **The proof this is checked against is one account in two Claude Code config folders, not
  two accounts.** Two accounts is what an operator will actually do, and nothing here has
  been run against it.
- **opencode also reads `~/.opencode/` whatever `XDG_CONFIG_HOME` says** — a possible home
  for a shim that would survive a profile switch, and it is unmeasured.
