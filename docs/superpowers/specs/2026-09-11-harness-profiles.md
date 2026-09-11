# Harness profiles — a chat starts on the harness you pick, launched the way you launch it

**Status:** agreed 2026-09-11, in a grill between the operator and the `steward` persona.
Decisions and the reasons behind each: `workspaces/harness-profiles/workspace.md` in the plane.
**Depends on:** #969 — `doctor` has to read the config folder a harness actually uses before
it can report one row per profile.

## The failure

Two, and one feature answers both.

- **One program per harness kind, launched one way.** `Harness.binary` is a class attribute
  and `launch_argv` returns `[binary, *extra]`; nothing overrides either. An operator with two
  Claude Code accounts — a work `CLAUDE_CONFIG_DIR` and a personal one — or a pinned older
  Codex has no way to tell charter so. A shell alias does not help: charter finds the binary
  with `shutil.which` and runs it with no shell, so an rc-file alias never resolves.
- **Opening charter starts a harness nobody asked for.** Bare `charter` launches
  `[harness] default`, and `+` launches the kind of the chat it was pressed from
  (`_same_harness_as`). On an empty workspace that is a harness session started before
  anybody said which one they wanted.

## Language

- **Harness profile** — a named way to launch one harness kind: its kind, its command, its
  environment. _Avoid:_ *alias* (a shell feature charter never sees), *account* (a profile
  need not be a different one).
- **Kind** — which harness program a profile launches, written as the word typed after
  `charter`: `claude`, `codex`, `opencode`. The registry's own names, and the value of
  `$CHARTER_HARNESS`, stay `claude-code`, `codex`, `opencode`.
- **Profile selector** — what a new chat's harness pane shows before any harness has run in
  it.

## A profile

```toml
# charter.local.toml — beside charter.toml, never committed
[harness]
default = "claude-work"                          # optional: the row the selector starts on

[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.claude-work" }

[harness.codex-pinned]
kind = "codex"
command = ["npx", "-y", "@openai/codex@0.140.0"]
```

- `kind` must be a registered kind. `command` is a list of arguments, never a shell string;
  its first element is resolved on PATH. A leading `~` is expanded in that first element and
  in every `env` value, because no shell is there to do it. `env` is optional.
- Every table directly under `[harness]` is a profile. `default` is the one other key, so no
  profile may be named `default`.
- A name follows the workspace-name alphabet — a dot in a workspace name broke tmux targets
  in #695 — and may not equal a charter command, a clash `cli._add_frame_parsers` already
  refuses for kinds.
- **Every registered kind is also a built-in profile** named after itself: `command = [kind]`,
  no `env`. A declared profile of the same name replaces it, which is how plain `claude` gets
  pinned. Built-ins cannot be hidden.
- **Refused by name with its reason, and the rest still load:** an unknown `kind`; a `command`
  that is not a non-empty list of strings; a reserved, illegal or clashing name; and an `env`
  name containing `KEY`, `TOKEN`, `SECRET` or `PASSWORD`. That last refusal names the
  harness's own login instead — `CLAUDE_CONFIG_DIR` + `/login`, `CODEX_HOME` +
  `codex login`, `XDG_DATA_HOME` + `opencode auth login`. A broken profile is refused alone
  rather than taking the file down, unlike `[[frame.component]]`: a missing panel is easy to
  miss, and a missing profile is a row that is not in the selector.

**Why no credentials.** A variable set on the harness process reaches the shell the model
runs. Measured on Claude Code — the grill's own shell saw `CLAUDE_CODE_MESSAGING_TOKEN` — and
on Codex, whose default `shell_environment_policy` passed a `*_TOKEN` probe straight through
(`codex sandbox`, 0.147.0). A vault reference would not change that: the key would only rest
somewhere else before it landed in the same shell. A wrapper script on PATH can still export
a key. Charter declines to hold one; it cannot prevent one.

## Where profiles live

- **Only in `charter.local.toml`.** Accounts and install paths belong to one person's
  machine, and a profile's command runs on a click with no harness permission prompt in
  between — so a command in the committed file could be changed by a merged PR or by a chat,
  and then run on every machine. A `[harness.<name>]` table in `charter.toml` is refused with
  a pointer to the local file. `charter.toml`'s `[harness]` keeps `default` and nothing else;
  the local `default` wins.
- **The local file accepts `[harness]` and nothing else**, refusing every other section by
  name. A full overlay would let an ignored file change plane policy — `[[forge]]` hosts
  steer the credential guard (`gitpolicy`) — with no trace in git, and "override" has no
  definite meaning for a list table. `[frame]`'s look keys are the likely next section; each
  is admitted on its own reason.
- **"Ignored" is guaranteed, not hoped for.** `charter init` writes `/charter.local.toml`
  into the plane's `.gitignore`, and `charter reinit` adds it to an existing plane. This
  amends ADR 0017, whose rule covers only a path charter creates that carries credentials:
  this file carries none, and its whole meaning is "not committed". If git tracks the file or
  would not ignore it, charter refuses its profiles and says why, and `doctor` warns. The
  check runs at launch, in the selector and in `doctor` — never on a config read, so no hook
  pays a git call per tool call.
- **Profiles are added by editing the file.** There is no `charter harness add`: a chat can
  run it as easily as it can edit the file, so it could never stand for the operator's
  approval, and all it would buy is typing. `charter harness list` shows every profile charter
  read, the file it came from, and why any was refused.

## What guards a launch

Each check runs before tmux wherever a terminal is attached — a refusal there is a `return`,
with nothing to tear down — and again in the pane immediately before the harness replaces
charter's process (*How a harness starts*).

1. **The file is ignored** (above).
2. **A new or changed command asks once.** Charter records each profile's `kind`, `command`
   and `env` as last launched, under `.charter/`. A profile with no record, or a different
   one, shows its command and asks `run this? [y/N]` before it runs. Built-ins never ask.
   Why: once the file is ignored an edit leaves no diff; nothing stops a chat editing plane
   config; and the command goes to tmux, not through a harness permission prompt. Codex
   trusts hooks by hash for the same reason. An open that nobody is at (*The selector*)
   refuses where it would have asked.
3. **The profile is wired.** Measured on claude 2.1.268, codex-cli 0.147.0 and opencode
   1.18.23, in throwaway folders with no login and no model tokens: a harness pointed at
   another config folder loads none of charter's wiring. Claude Code lists no charter plugin
   — not even "enabled but not installed" — in a directory whose settings enable it, because
   the `charter` marketplace is known only to `~/.claude/settings.json`. An empty `CODEX_HOME`
   holds no plugin, no hook trust and no `shell_environment_policy.set`. opencode under
   another `XDG_CONFIG_HOME` loads no shim, and its shells get no `CHARTER_HARNESS`. So a
   profile that is not wired **refuses to launch and prints the fix**, and no flag launches
   one unguarded.
   - **Wiring is detected by asking the harness under the profile's environment**, never
     inferred from variable names, because one account is reachable through variables that
     do and do not move the plugin: by its binary, Claude Code's
     `CLAUDE_SECURESTORAGE_CONFIG_DIR` moves only the login, and opencode's login follows
     `XDG_DATA_HOME` while its plugins follow `XDG_CONFIG_HOME`.
   - `charter init`, `charter reinit` and `charter harness install <profile>` run each kind's
     wiring under that profile's environment. Never at launch: a click would write a plugin
     into a second account unasked, and Codex ignores hooks nobody approved anyway.
   - `doctor` shows one row per profile.

## How a harness starts

**Every chat pane starts as charter, and charter replaces itself with the harness.** The
pane's first process is a charter launcher. It runs the checks above, then `exec`s the
profile's `command` — plus whatever the open adds, `--resume <id>` or
`Harness.first_message_argv` — with the profile's `env` applied. Three reasons, the first a
constraint:

- **`env` reaches the harness without `layout.CARRIABLE`.** That allow-list raises on every
  other name, and the chat-handoff plan's global constraints say nothing is added to it. The
  values also never pass through tmux's own argument parser, which has already cost this
  repo #957 and #961.
- **One place runs the checks for every open** — the CLI, the selector, `+`, reopen, a
  handoff.
- **`exec` keeps the pane's first process the harness.** The pid does not change, so the
  pane id charter records, `remain-on-exit` and the `pane-died` path see what they see today.

The plan measures that last claim before building on it: what `pane_current_command`,
`_pane_last_words` and the chat-status paths read from a pane whose first process was charter,
on tmux 3.7c and at the 3.2 floor, on charter's own server and in the operator's own tmux.

**A chat records its profile.** `CHARTER_HARNESS` stays the kind: hooks compare it to
`claude-code` for session ids, resume and the working spinner. The profile is a field of its
own, `CHARTER_HARNESS_PROFILE`, set by the launcher at `exec` rather than through tmux, and
recorded in the chat's state and in the reopen manifest. Wherever charter shows a chat's
harness today, it shows the profile.

## The selector

**No harness starts until someone picks a profile.**

- **Where it appears:** bare `charter` on a workspace with no running chat; `+`; the palette's
  new chat; a workspace tab whose workspace has no running chat. The window opens with the
  selector in its harness pane, and the harness starts in that pane once a profile is picked.
- **Where it does not.** `charter <profile>` names the profile. So does every open nobody is
  at: reopen, restoring a recorded plane, and a handoff — which takes the calling chat's
  profile, so the chat-handoff spec's "the harness is the calling chat's" becomes "the profile
  is". A reopened chat whose profile is gone is **skipped** with a line naming the profile,
  never given another: another profile may be another account, where the chat's resume id
  does not exist and its workspace's code was never meant to go.
- **It always shows**, even with one profile available. Skipping it would bring back the
  harness nobody picked on a machine with one harness; one profile costs one Enter.
- **The rows.** Declared profiles always; a built-in only when its program is installed. A
  profile that cannot start stays listed with its reason on the row — command not on PATH;
  not wired, with the fix; the file tracked by git — and Enter on it only shows the reason,
  which is the palette's rule for a name it cannot switch to (`docs/frame.md`). A new or
  changed profile shows its command on Enter and asks in place. The starting row is the
  profile of the chat `+` was pressed from, else `default`; a `default` naming a profile this
  machine lacks starts on nothing, and `doctor` warns.
- **The surface** is the F2 palette's picker: type to filter, Enter to choose, Esc to cancel.
- **Cancel.** Esc closes that window having started nothing; if it was the workspace's only
  window, the session goes with it — the frame's rule for its last chat. #518 put the
  workspace picker before tmux because cancelling inside the frame meant tearing down "a
  launch that half happened". Here nothing has happened: no harness ran and no identity was
  recorded.
- **A waiting pane is not a chat.** It has a tab, so it can be left and come back to; it is
  not in the quit manifest and is never reopened; its kind and profile are recorded at the
  pick.
- **Opening a running workspace adds nothing.** Where nobody is attached, bare `charter` on a
  workspace whose chats are running attaches to them rather than opening another, as it
  already does where somebody is attached. `+` adds a chat. A launch that names what to run
  still runs it — `charter <profile>`, `charter frame -- <cmd>`.
- **The workspace prompt stays before tmux.** A workspace is a tmux session, so charter has to
  know which one before the frame exists; a profile belongs to one chat, so choosing it
  belongs in that chat's pane.
- `+` no longer stops on "this chat records no harness charter can launch and your plane
  declares no `[harness] default`": the selector answers that case.

## ADR 0018, amended

**Charter draws in a pane only while no harness has ever run in it.** The selector is
charter's, drawn before the pane's harness exists. Once a harness has run there, ADR 0018
holds unchanged, its two reading moments included. A pane whose harness exits closes as it
does today and never goes back to the selector.

In the operator's own tmux the launcher takes the moment the `cat` placeholder
(`layout.PLACEHOLDER`, `layout.window_argv`) holds today. On charter's own server the window's
first command becomes the launcher instead of the harness (`layout.chat_window_argv`,
`layout.session_argv`).

## What this changes elsewhere

- The phase-5 spec's "two chats on one harness share that harness's credentials. Charter
  cannot separate them and does not pretend to" stops being true: two profiles of one kind can
  hold two logins.
- `doctor`, dispatch's transcript lookup, persona skill lookup and `plugincache` read
  `~/.claude` or run the default `claude`. Each either follows the profile it is about, or
  says it answers for the default config folder. #969 is the first of them.
- `[harness] default` launches nothing any more, and bare `charter` no longer exits on a
  refused default.
- `charter harness list` lists profiles, and `charter harness install` accepts a profile name
  as well as a kind.

## Limits

- A pattern match on `env` names can refuse an innocent variable, and a wrapper script can
  still carry a key.
- Detecting wiring spends a harness subprocess per profile; the plan measures what the
  selector pays for it and whether that has to be cached.
- `claude plugin list --json` writes `.claude.json` into the config folder it runs against
  (measured), so asking a profile's harness is not write-free.
- opencode also reads `~/.opencode/` as a config folder whatever `XDG_CONFIG_HOME` says — a
  possible home for the shim that would survive a profile. Unmeasured.
- The proof below is one account in two Claude Code config folders, not two accounts.

## Done when

On this plane, one workspace runs these side by side, each chat started from the selector:

- the built-in `claude`;
- `claude-alt`, with `CLAUDE_CONFIG_DIR = "~/.claude-alt"`, logged in once to the same account;
- `codex`, replaced by a profile whose `command` is `~/.local/bin/codex`.

In each chat, a command charter should block shows charter's own refusal. `claude-alt`
refuses to launch before `charter harness install claude-alt`, and prints that command. After
`charter: quit` and reopening, each chat comes back on its own profile. The operator's one
step is `/login` inside `~/.claude-alt`.

## Build order

0. **#969 on `main`** — `doctor`'s plugin rows read the config folder the harness uses.
1. **Profiles are read.** The local file, validation and refusals, built-ins, the
   `.gitignore` guarantee, `charter harness list`. Nothing launches differently yet.
2. **A profile launches.** It opens with the launcher measurement above. `charter <profile>`,
   the launcher and its `exec`, `CHARTER_HARNESS_PROFILE` in state and in the manifest; `+`,
   tabs, reopen and handoff carry the profile; reopen skips a missing one.
3. **A new or changed command asks once.**
4. **A profile is wired or refuses:** detection, the fix, `charter harness install
   <profile>`, `init` and `reinit` per profile, `doctor` per profile.
5. **The selector**, carrying 3's and 4's states on its rows; attach-and-add-nothing;
   `default` as the starting row.
6. **The records:** ADR 0021 for the profile decisions, the 0017 and 0018 amendments,
   `CONTEXT.md`, the phase-5 spec's credentials line, and a review of the news entry.
7. **The proof above, on this plane.**

Each task moves the docs page for what it changes in its own PR, and task 1 starts
`docs/news/unreleased-harness-profiles.md`, which each later task extends.

**Order: 0 → 1 → 2 → {3, 4} → 5 → 6 → 7.** 3 and 4 touch different code; whichever merges
second rebases its docs.
