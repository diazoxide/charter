# Harness profiles — a chat starts on the harness you pick, launched the way you launch it

> For agentic workers: REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** an operator declares harness profiles — a kind, a command, an environment — in a
gitignored `charter.local.toml`. Every chat pane starts as a charter launcher that checks the
profile and `exec`s it. A new chat shows a profile selector in its own pane before any harness
runs, so one workspace can run chats on different accounts and binaries side by side.

**Architecture:**
- `charter/profiles.py` reads and validates profiles at the config boundary
  (`config.PROFILES`). It does no git call and runs no subprocess.
- `charter/frame/launcher.py` is the one place a profile starts. `commands_frame._launch` puts
  `charter frame-launch --profile <name> -- <rest>` into tmux where the harness argv went. The
  launcher re-runs the guards in the pane, applies the profile's `env`, and `os.execvpe`s the
  command. The env never passes through tmux, and `layout.CARRIABLE` is untouched.
- The guards run in order, before tmux wherever a terminal is attached and again in the pane:
  1. the file is ignored (`profiles.ignored_refusal`);
  2. the command is on `PATH`;
  3. a new or changed command asks once (`charter/profiletrust.py`);
  4. the profile is wired (`charter/wiring.py`).
- The selector (`charter/frame/selector.py`) is the palette's `own_the_tty` surface, run by the
  same launcher before a profile is picked.

**Tech Stack:** Python ≥ 3.11 stdlib; tmux (3.7c measured, 3.2 floor at
`~/.local/share/charter-testing/tmux-3.2`, CI's image tmux 3.4); Claude Code 2.1.268, codex-cli
0.147.0, opencode 1.18.23; stdlib `unittest`; `tools/sweep.py`.

**Spec:** `docs/superpowers/specs/2026-09-11-harness-profiles.md` (binding). Decisions and
reasons: `workspaces/harness-profiles/workspace.md` in the plane.

**Line anchors** are against `harness-profiles-spec` @ `83a5363`, which is `main` @ `a5aa860`
plus the spec commit. #969 and every task move them, so re-anchor by symbol name, never by
number.

## Global Constraints

Every task's requirements include this section.

**Carried over from the chat-handoff plan:**
- stdlib only, Python ≥ 3.11; no new runtime dependency.
- Tests are stdlib `unittest` and fail first. They use the isolation helpers CONTRIBUTING names:
  `tests._isolation.PersonaIso`/`PlaneIso`, `_planeguard`, `_envguard`, `_ttyguard`, `_gitguard`,
  `_tmuxreap`. They also use `_claudeguard`, which puts a fake `claude` answering `[]` first on
  `PATH` for the whole suite.
- `charter/frame/tmuxctl.py` is the only module that calls tmux (ADR 0018).
- Charter never parses the harness pane. It draws in a pane only while no harness has ever run
  in it (ADR 0018 as the spec amends it). Nothing before Task 5 draws in one at all.
- No version bump and no tag. The news entry is the one file
  `docs/news/unreleased-harness-profiles.md`: flat `key: value` frontmatter, unquoted values,
  only the six keys CONTRIBUTING lists. No test names it by filename
  (`test_news_gate.test_no_test_opens_an_entry_by_its_staged_name`).
- Docs move with the code, in the same PR.
- Comments explain why.
- Every state write goes through `config.write_for` / `config.replace_for` /
  `config.private_mkdir`.
- A hook never breaks a turn: new hook-reachable code is best-effort.
- A test that starts tmux names every socket with `tests._tmuxreap.name("<slug>")`: lowercase,
  digits and single hyphens, never `tmux-3.2`. It kills and unlinks the socket in cleanup. A
  new `mock.patch.dict(os.environ, …)` passes `clear=True` or states every value it depends
  on.
- Every refusal message follows CONTEXT.md's Prose rules: it says the rule worked and names the
  fix in the same breath.

**Added for this feature:**
- **Nothing is added to `layout.CARRIABLE`, and a profile's `env` never passes through tmux** —
  not on `-e`, not on the launcher's argv, not through `set-environment`. Only the profile's
  NAME travels, as an argument after `--`.
- **`CHARTER_HARNESS` stays the kind's registry name** (`claude-code`, `codex`, `opencode`).
  Hooks compare it to `claude-code` for session ids, resume and the working spinner
  (`hooks._turn_begin`, `hooks._record_harness_session`). The profile rides as
  `CHARTER_HARNESS_PROFILE`, which the launcher sets at `exec`. It never joins
  `commands_frame._FRAME_IDENTITY`, because that would put it on a tmux `-e`.
- **No committed file ever holds a profile.** `charter.toml`'s `[harness]` carries `default`
  and nothing else. Tests write `charter.local.toml` only under `config.ROOT` inside
  `PersonaIso`.
- **The gitignore check and the trust check never run on a config read.** They are absent from
  `config.derive`, `instance.harness_of`, `profiles.derive` and anything a `charter hook …`
  process runs. Every hook process runs `config.derive`, and `hooks/hooks.json` fires on Bash,
  Read, Grep, Write, Edit, Task, Skill and SendMessage. The checks run at launch, in the
  selector, in `charter harness list`, `charter harness install` and `doctor`.
- **No profile's command runs before that profile passes the ignored check and the trust
  check.** That covers a wiring probe, an install and a launch alike: Task 4's probe runs
  `[*command, "plugin", "list", "--json"]`, and before Task 3 nothing stands for the operator's
  approval of `command`.
- **Between merges, `main` never runs an unapproved declared command** (review B1). Task 2
  refuses every declared profile, a replacement of a built-in included; Task 3 lifts that
  refusal in the PR that adds the approval.
- **A launch never trusts a cached wiring answer** (review B2). Every launch probes fresh; the
  selector's cache is display-only, and an entry stamped in the future is stale.
- **No profile probe runs on a hook path**, SessionStart's preflight included (Ruling 11). A
  probe costs 215–750 ms per profile and writes into that profile's config folder. Probes run
  at launch, in the selector, in `charter harness install`, and in a `charter doctor` a person
  runs — never in `charter doctor --preflight`, which is what the SessionStart hook runs.
- **Every refusal, clamp or fallback has a test that goes red when that line is deleted.**
  Before each PR:
  - run `python3 tools/sweep.py` and report what it said in the PR description;
  - for a guard added under `tests/`, which the sweep never mutates, also report the hand
    deletion check (delete the line in a scratch copy, run the covering class).
- **Every piece of profile-derived text charter displays passes through `contain`** (ruling 35),
  the way `frame/picker.py` contains workspace names. That covers the ask prompt, selector rows
  and footer, `charter harness list`, doctor rows and refusal text. A profile's `command` and
  `env` come from a file a chat can write, and a `\r` or ESC in one could redraw the approval
  prompt to show a harmless command while another one runs. They are shown escaped
  (`contain.readable`), never interpreted.
- **A real-tmux test that launches a declared profile seeds its launch record with
  `tests._isolation.approve_profile` and checks it with `assert_approved`** (from Task 3; N6 nit).
  Without that, the attended pane asks `run this? [y/N]` and waits for the suite's 600 s
  watchdog.
- **Each task's first implementation step runs the whole suite against the change, in the
  background** (re-review N4), and records what breaks. Every task's list of existing tests it
  changes is a floor, not the whole set.
- Run the whole suite the way CI does, `python3 -m unittest discover -s tests`, before each PR.
  `tests/test_plane_spawn_guard.py` scans the whole tree statically, so a focused run cannot see
  a new `os.exec*`.
- Kill only the PIDs you started. Never `pkill -f`: sibling agents run the same sweep and the
  same tmux.

## Order and parallelism

**0 → 1 → 2 → 3 → 4 → 5 → 6 → 7, one task at a time** (Ruling 1). Task 4 cannot run beside
Task 3: detecting wiring runs the profile's own command, and only Task 3's launch record stands
for the operator's approval of it.

- **Task 0 is #969, open as PR #970**: branch `fix-969-doctor-config-folder`, commits `677faac`,
  `95c2a49` and `3624287`, under review, not merged. It is a prerequisite of Task 4 only and is not planned here.
- Tasks 1–3 do not touch doctor's plugin rows and do not wait for #970.
- Task 4 starts only once #970 is on `main`, and re-reads its merged diff first: review may
  rename what Task 4 consumes.
- Task 7 runs after Task 6 is on `main`, on this plane, with the operator.

## Measured while writing this plan

On this machine (darwin), 2026-09-11. Recorded so Task 4's measurement step starts from numbers,
not from nothing.

- **`claude plugin list --json`, 5 runs each.**
  - Empty throwaway `CLAUDE_CONFIG_DIR`: median **215 ms** (first 281 ms). Output `[]`. It
    wrote `.claude.json` and `backups/` into that folder.
  - Default config: median **281 ms**. 64 entries.
  - Entry keys: `enabled`, `id`, `installPath`, `installedAt`, `lastUpdated`, `mcpServers`,
    `projectPath`, `scope`, `version`.
  - `charter@charter` appears at scopes `project`/`local`/`user`, both enabled and disabled.
- **`opencode debug config`, 3 runs each.**
  - Empty throwaway `XDG_CONFIG_HOME`: median **749 ms**. The `plugin` key is present.
  - Default: median **720 ms**. `"plugin": ["file:///Users/aharon/.config/opencode/plugin/charter.ts"]`.
- **Codex needs no subprocess.** `~/.codex/config.toml` holds `[plugins."charter@charter"]`,
  `[shell_environment_policy]`, and one `[hooks.state."charter@charter:hooks/hooks.json:<event>:<i>:<j>"]`
  with a `trusted_hash` per hook.
- **tmux:** `/opt/homebrew/bin/tmux` 3.7c; `~/.local/share/charter-testing/tmux-3.2` answers
  `tmux-3.2 3.2`.
- **Doctor already spawns `claude` three times per run** (`plugin install`, `plugin files` ×2).
  The SessionStart preflight runs `charter doctor` under a 20 s hook timeout (`hooks/hooks.json:22`)
  and reads only its exit code.

---

## File Structure

Created:

| Path | Task | Responsibility |
|---|---|---|
| `charter/profiles.py` | 1 | `Profile`, `Refused`, `derive(root, cfg)`, the built-ins, `~` expansion, the name/kind/command/env refusals, `current()` (clashes with charter commands), `ignored_refusal(root)` (the git check). No subprocess except `ignored_refusal`. |
| `charter/frame/launcher.py` | 2 | `argv()` for tmux, `environment()`, `refusal()` (the ordered guards), `start()` (records, `os.execvpe`), `cmd_frame_launch`. |
| `charter/profiletrust.py` | 3 | The last-launched record under `.charter/`, `approval_needed`, `record_launched`, the in-terminal ask. |
| `charter/wiring.py` | 4 | `Wiring`, per-kind `detect`, the stamped cache under `.charter/cache/`, `install(profile, root)`, the fix sentence. |
| `charter/frame/selector.py` | 5 | Rows from profiles + trust + wiring, the `Selector` and `Confirm` surfaces, the pick loop. |
| `docs/adr/0022-a-harness-profile-belongs-to-one-machine.md` | 6 | The profile decisions (number: Ruling 2). |
| `docs/news/unreleased-harness-profiles.md` | 1 (extended by 2–5, reviewed in 6) | The news entry. |
| `tests/test_harness_profiles_are_read_from_the_local_file.py` | 1 | derive, refusals, built-ins, default, the charter.toml pointer. |
| `tests/test_the_local_file_stays_out_of_git.py` | 1 | init/reinit ignore line, `ignored_refusal`, the doctor row. |
| `tests/test_charter_harness_list_shows_profiles.py` | 1 | The list output. |
| `tests/test_charter_names_a_profile_on_the_command_line.py` | 2 | `charter <profile>` rewrite, `--profile`, bare charter's default. |
| `tests/test_a_profile_launch_is_refused_before_tmux.py` | 2 | `_launch`'s pre-tmux refusals and the bypass path. |
| `tests/test_the_launcher_becomes_the_profile.py` | 2 | `launcher.environment/argv/refusal/start`, nothing on tmux. |
| `tests/test_a_chat_carries_its_profile.py` | 2 | `+`, tab, handoff, reopen, manifest, displayed names. |
| `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py` | 2 | Real tmux: pane id, pid, dead status, hooks, last words, env. |
| `tests/test_a_new_or_changed_profile_asks_once.py` | 3 | The record, the ask, unattended refusals. |
| `tests/test_a_profile_is_wired_or_refuses.py` | 4 | detection per kind, cache, fix, install, init/reinit, doctor rows. |
| `tests/test_a_new_chat_starts_at_the_profile_selector.py` | 5 | rows, keys, cancel, waiting pane, attach-and-add-nothing. |
| `tests/test_the_profile_words_are_written_down.py` | 6 | CONTEXT.md terms, ADR presence, amendments. |

Modified:

| Path | Task | Change |
|---|---|---|
| `charter/config.py` | 1 | `d["PROFILES"] = profiles.derive(root, cfg)` before `d["HARNESS"]` (`:721`), which Task 2's `harness_of(cfg, profiles=…)` reads (review 14). |
| `tests/_planeguard.py` | 1 | `charter.local.toml` joins the refused-write markers (`:1765`); `"PROFILES"` joins `_GUARDED_SETTINGS` (`:368`). |
| `charter/commands.py` | 1, 4 | `LOCAL_PROFILES_IGNORE`, the baseline block and `_ensure_gitignore` check (`:1025-1073`), `_ensure_local_profiles_ignored` called by `cmd_reinit` (`:2494`) (1); per-profile wiring in `cmd_init`/`cmd_reinit` (4). |
| `charter/util.py` | 1 | `git_path_state(root, path)` beside `git_ignores` (`:417`), its timeout caught; `git_ignores` itself untouched (review 4). |
| `charter/commands_harness.py` | 1, 4 | `cmd_harness_list` shows profiles (1); `cmd_harness_install` accepts a profile (4). |
| `charter/doctor.py` | 1, 4 | `check_harness_profiles` row (1); per-profile rows and `check_names` splice (4). |
| `charter/cli.py` | 2, 5 | `_profile_launch` in `main`, `--profile` on each kind's parser, `frame-launch` parser and reserved word (2); bare `charter` → `frame --select` (5). |
| `charter/commands_frame.py` | 2, 3, 4, 5 | `_launch` resolves a profile and runs the launcher; `_same_profile_as`; `_reopen_one`; `background_refusal`/`open_in_background`; `early_death_message` display argv (2); asks (3); selector launch, attach-and-add-nothing, `+`/tab (5). |
| `charter/frame/state.py` | 2, 5 | `record_profile`/`profile` (2); `record_waiting`/`is_waiting`/`clear_waiting` and the identity update at the pick (5). |
| `charter/frame/reopen.py`, `charter/frame/leave.py`, `charter/frame/chats.py`, `charter/frame/choose.py` | 2, 5 | `Chat.profile`, `Doomed.profile`, `chats.profile_of`, displayed names (2); waiting panes left out of the plan (5). |
| `charter/plugincache.py`, `charter/harness/codex.py`, `charter/harness/opencode.py` | 4 | `env`/`command` threading and per-home detection. |
| `charter/frame/overlay.py` | 5 | `Surface.footer`. |
| `charter/instance.py` | 2 | `harness_of` stops being what bare `charter` reads; keeps shape validation of `charter.toml`'s `[harness]`. |
| `tests/_envguard.py`, `tests/test_no_test_reads_the_operators_shell.py` | 2 | `CHARTER_HARNESS_PROFILE` refused on read. |
| `tests/test_plane_spawn_guard.py` | 2 | `KNOWN["charter/frame/launcher.py:execvpe"]`. |
| `docs/control-plane.md`, `docs/harnesses.md`, `docs/frame.md`, `docs/install.md`, `README.md` | 1–5 | Docs with each task. |
| `docs/adr/0017-…`, `docs/adr/0018-…`, `CONTEXT.md`, `docs/superpowers/specs/2026-08-28-phase5-workspace-and-chat-tabs.md` | 6 | Amendments and terms. |

No new `docs/*.md` page. Profiles live in `docs/control-plane.md`, `docs/harnesses.md` and
`docs/frame.md`, so `pyproject.toml`'s force-include set does not change
(`test_docs_show.TestPagesShip.test_nothing_else_is_force_included`).

---

## Task 0: #969 — `doctor` reads the config folder Claude Code uses (PR #970; prerequisite, not planned here)

PR #970, branch `fix-969-doctor-config-folder`, commits `677faac`, `95c2a49` and `3624287`,
under review. Task 4 starts after it is on `main`.

**What it adds** (read with `git show 3624287:<path>`; `config_home` and reinit's `~/.claude` pin
are unchanged there; its worktree under `.worktrees/` is not
touched):
- `charter/harness/claude_code.py`: `config_home() -> Path`
  (`CLAUDE_CONFIG_DIR ?? ~/.claude`, NFC-normalised; an empty value is kept and names the
  working directory) and `global_config_file() -> Path` (a legacy `<home>/.config.json` if
  present, else `(CLAUDE_CONFIG_DIR || $HOME)/.claude.json`). Both read `os.environ` only.
- `charter/doctor.py`: `_settings_files`, `_plugin_declaring_guard` and `_claude_json` read
  through them; the `session root`, `plane-root guard` and `mcp` rows name the folder read.
- `charter/guardseen.py`: a sighting records `claude_config_dir`, and
  `last_claude_config_dir()` reads it back. `check_guard_wired` accepts a plugin sighting only
  for the folder in use.
- `charter/commands.py` (as of `95c2a49`): `_plugin_dispatches_guard` names `~/.claude` whatever
  the shell says, through `doctor._plugin_declaring_guard(root, folder=…)`. It decides a write
  into the committed `.claude/settings.json` that every config folder reads, so it must not
  follow one person's shell. Task 4 leaves it so.
- `charter/util.py` (`95c2a49`): `short_path` shows a path with a literal `~` segment absolute.
- `docs/install.md`: the rows answer for the folder in use, and `personas` still reads
  `~/.claude`.

**What Task 4 takes from it.** Task 4 resolves a Claude Code profile's folder with
`claude_code.config_home`, never a second resolver. It gives `config_home` and
`global_config_file` an optional `env: Mapping[str, str] | None = None` (default `os.environ`)
so the same rule runs over a profile's merged environment. If review renames either function,
Task 4 follows the merged name.

---
## Task 1: Profiles are read — the local file, its refusals, the built-ins, the ignore guarantee, `charter harness list`

**Depends on:** nothing. **Nothing launches differently:** `config.HARNESS`, `_bare_launch`,
`_same_harness_as` and `_reopen_one` are untouched in this task.

### Files

- Create `charter/profiles.py`.
- `charter/config.py:721` — **before** `d["HARNESS"]`: `d["PROFILES"] = _profiles.derive(root, cfg)`,
  with a `#:` comment on why it is derived here and carries no git call. Before, because Task 2's
  `instance.harness_of(cfg, profiles=…)` reads it (review 14).
- `charter/util.py:417` — `git_path_state(root, path, *, timeout: float = 5.0) -> str` directly
  after `git_ignores`. `git_ignores` is not touched: `commands_secrets.py:121` and
  `doctor.check_credential_paths` (`doctor.py:3063`) read its `None` as "not a repository", and a
  timeout added there would raise `util.ProcTimeout` into both (review 4).
- `charter/commands.py`:
  - `_GITIGNORE_BASELINE` (`:1025`): a block after the `/.claude/settings.local.json` block.
  - `_ensure_gitignore` (`:1050`): a whole-line check for `LOCAL_PROFILES_IGNORE`.
  - `LOCAL_PROFILES_IGNORE` and `_ensure_local_profiles_ignored(root)` beside
    `LOCAL_SETTINGS_IGNORE`/`_ensure_local_settings_ignored` (`:1619-1640`).
  - `cmd_reinit` (`:2494`): call it after `_create_baseline_dirs`, and report
    `".gitignore (/charter.local.toml)"` under `created`.
- `charter/commands_harness.py:16` — `cmd_harness_list` prints profiles first, then the kinds'
  ceilings it prints today.
- `charter/doctor.py` — `check_harness_profiles()` directly after `check_control_plane_config`
  in `_checks` (`:3196`); `"harness profiles"` after `"charter.toml"` in `_FIXED_CHECK_NAMES`
  (`:3239`).
- `tests/_planeguard.py:1765` — the markers tuple also covers `Path(r) / profiles.LOCAL_FILE`,
  in both spellings. `:368`: `_GUARDED_SETTINGS = ("UPDATE", "HARNESS", "PROFILES")`.
- Tests:
  - `tests/test_harness_profiles_are_read_from_the_local_file.py`
  - `tests/test_the_local_file_stays_out_of_git.py`
  - `tests/test_charter_harness_list_shows_profiles.py`
  - one case each in `tests/test_plane_write_guard.py::WritesAreRefused` and
    `tests/test_no_test_reads_the_operators_channel.py::WhatIsGuarded`
  - fixture line in `tests/test_init.py::TestGitignorePresenceCheckIsPrecise::test_the_real_anchor_is_still_recognised_as_present`,
    whose docstring lists every rule on purpose
- Docs: `docs/control-plane.md` (the config reference block `:93` and `## [harness].default`
  `:458`), `docs/harnesses.md` (`# Harnesses` intro and a new `## Profiles`),
  `docs/news/unreleased-harness-profiles.md` (new).

### Interfaces

**Consumes (on `main`):**
- `instance.load(root)` (`instance.py:105`), `instance.harness_of(cfg)` (`:2952`),
  `instance.launchable_harnesses()` (`:2920`)
- `harness.registry.all()`/`get()` (`harness/registry.py:26,31`); `Harness.name`, `cli_name`,
  `binary` (`harness/base.py:238-259`)
- `workspace.valid_name` (`workspace.py:195`), for the alphabet reference only (Ruling 5)
- `contain.readable`, `contain.one_line` (`contain.py`)
- `util.git_ignores` (`util.py:417`), `util.append_gitignore(root, lines, header)` (`:386`)
- `doctor.Result`/`OK`/`WARN` (`doctor.py:31,21`)

**Produces:**

```python
# charter/profiles.py
LOCAL_FILE = "charter.local.toml"
#: Words an env NAME may not contain, matched case-insensitively.
SECRET_WORDS = ("KEY", "TOKEN", "SECRET", "PASSWORD")
#: What a refused secret-shaped name is pointed at instead, by registry name.
LOGIN = {"claude-code": "set CLAUDE_CONFIG_DIR and run /login inside Claude Code",
         "codex": "set CODEX_HOME and run codex login",
         "opencode": "set XDG_DATA_HOME and run opencode auth login"}
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_-]*$")       # Ruling 5
BUILTIN = "built-in"

class Profile(NamedTuple):
    name: str                          # "claude-work"; a built-in is named after its kind
    kind: str                          # the word after `charter`: "claude" | "codex" | "opencode"
    harness: str                       # the registry NAME: "claude-code" | "codex" | "opencode"
    command: tuple[str, ...]           # as declared, before `~` expansion
    env: tuple[tuple[str, str], ...]   # as declared, sorted by name
    source: str                        # BUILTIN | LOCAL_FILE

class Refused(NamedTuple):
    name: str      # contained with contain.readable; "" for a whole-file refusal
    source: str    # LOCAL_FILE | "charter.toml"
    reason: str    # one sentence: the rule, then the fix

def builtins() -> dict[str, Profile]: ...
    # one per registered harness with a cli_name: Profile(h.cli_name, h.cli_name, h.name,
    #                                                     (h.binary,), (), BUILTIN)
def derive(root: Path, cfg: dict) -> dict: ...
    # {"profiles": {name: Profile}, "refused": tuple[Refused, ...],
    #  "default": str | None, "default_from": str | None, "default_refused": str | None}
    # Never raises; runs no subprocess.
def current() -> dict: ...
    # config.PROFILES with names that clash with a charter command, and commands whose first
    # word is charter, moved into "refused" (lazy `from . import cli, hooks`). Every surface
    # reads this, never config.PROFILES directly.
def expanded_command(p: Profile) -> list[str]: ...  # `~` expanded in element 0 only
def expanded_env(p: Profile) -> dict[str, str]: ...  # `~` expanded in every value
def display(p: Profile) -> str: ...                  # env NAME=value, then shlex.join(p.command), each piece through contain.readable:
                                                     # control bytes shown escaped, never interpreted (ruling 35)
def ignored_refusal(root: Path) -> str: ...
    # "" when the file is absent, the plane is not a git repository, or git ignores the file
    # and tracks it not; TRACKED, NOT_IGNORED or GIT_CANNOT_TELL otherwise. The ONLY function
    # here that runs git.

# charter/cli.py (Task 1 adds only this helper; Task 2 uses it on the launch path)
def command_words() -> frozenset[str]: ...
    # every subcommand name build_parser() registers, minus the registered kinds' cli_names;
    # built once per process (functools.cache)

# charter/util.py
NOT_A_REPO, TRACKED, IGNORED, COMMITTABLE, UNKNOWN_GIT = (
    "not-a-repo", "tracked", "ignored", "committable", "unknown")
def git_path_state(root, path, *, timeout: float = 5.0) -> str: ...
    # one of the five; never raises — util.ProcTimeout and OSError read as UNKNOWN_GIT

# charter/commands.py
LOCAL_PROFILES_IGNORE = "/charter.local.toml"
def _ensure_local_profiles_ignored(root: Path) -> bool: ...   # True iff it appended

# charter/doctor.py
def check_harness_profiles() -> Result: ...                   # name "harness profiles"
```

### Behaviour

**`derive(root, cfg)`.**
1. **Built-ins first.** `builtins()`, in `harness.all()` order. Today every registered kind has
   `binary == cli_name`, so a built-in's command is its kind word, which is the spec's
   `command = [kind]`.
2. **`charter.toml`'s `[harness]`** (`cfg.get("harness")`). Every key other than `default`:
   - a table → `Refused(name, "charter.toml", PROFILE_IN_COMMITTED.format(name=…))`;
   - a non-table is ignored, as today (review 13).
     `test_bare_charter_opens_the_frame.TheSectionIsReadAtTheConfigBoundary.test_a_section_with_no_default_key_is_not_a_refusal`
     pins that silence for its stated reason: no `doctor` warning on a plane that did nothing
     wrong.
   - `default` is kept as the candidate default, from `"charter.toml"`.
3. **The local file.** `root / LOCAL_FILE`, absent → nothing. Unreadable or not TOML → one
   `Refused("", LOCAL_FILE, LOCAL_UNREADABLE.format(why=…))`, and no declared profiles are read.
   Otherwise, for each top-level key other than `harness`:
   `Refused(key, LOCAL_FILE, LOCAL_SECTION.format(section=…))`. Then under `[harness]`:
   - `default` (a string) replaces the candidate, from `LOCAL_FILE`. The local one wins.
   - Every other key must be a table, else `Refused(key, …, NOT_A_TABLE)`. Checks, in this
     order, first failure wins, each refusing that one profile:
     1. the name fails `NAME_RE` → `ILLEGAL_NAME`;
     2. the name is `default` → `RESERVED_NAME`;
     3. `kind` is missing or not a registered `cli_name` → `UNKNOWN_KIND` naming the legal kinds
        (`instance.launchable_harnesses()`);
     4. `command` is not a non-empty list of non-empty strings → `BAD_COMMAND`;
     5. `env` is present and not a table of string → string → `BAD_ENV`;
     6. an `env` name whose upper-case form starts with `CHARTER_` → `CHARTER_ENV` (Ruling 14);
     7. an `env` name whose upper-case form contains any of `SECRET_WORDS` → `SECRET_ENV`,
        naming the variable and `LOGIN[harness]`;
     8. a key other than `kind`/`command`/`env` → `UNKNOWN_PROFILE_KEY`. Kept on purpose
        (review 13): a typo such as `enviroment` would otherwise drop `CLAUDE_CONFIG_DIR` and
        launch the default account without a word.
   - A declared profile named like a built-in replaces it; the source becomes `LOCAL_FILE`.
4. **The default.** If the candidate names a profile in the result, it is `default`.
   Otherwise `default = None` and `default_refused` holds the contained value. A value naming a
   refused profile is also refused.

`current()` moves two more kinds of declared profile into `refused`:
- a name found in `cli.command_words()`, with `CLASHING_NAME`;
- a profile whose command's first word is charter itself, with `CHARTER_COMMAND` (Ruling 14).
  The test is `hooks._is_charter(command[0], command[1:])`, which already knows `charter`,
  `edm` and `python -m charter` (`hooks._CHARTER_PROGS`), so there is no second list.

It is a separate pass because `config.derive` runs before `cli` and `hooks` can be imported,
and importing either there would be a cycle. A clashing name is refused by name here, never
left to raise: the kind clash in `cli._add_frame_parsers` raises `ValueError` at
`build_parser()` and takes every command down, which is right for a registry mistake CI sees
and wrong for one machine's file.

**Refusal constants in `charter/profiles.py`** (each fills `{}` fields; no value of `env` is
ever quoted except the NAME):

```python
PROFILE_IN_COMMITTED = ("[harness.{name}] is in charter.toml, which is committed — a profile's "
    "command runs on a click, so charter reads profiles only from charter.local.toml, which "
    "stays on this machine. Move the table there; charter.toml's [harness] keeps `default` alone.")
LOCAL_UNREADABLE = ("charter.local.toml could not be read ({why}), so no declared profile was "
    "loaded — the built-in profiles still are. Fix the file and run charter harness list.")
LOCAL_SECTION = ("[{section}] in charter.local.toml is not read — that file carries [harness] "
    "and nothing else, because an ignored file must not change plane policy with no trace in "
    "git. Put [{section}] in charter.toml.")
NOT_A_TABLE = ("[harness] {name} in charter.local.toml is not a table — a profile is "
    "[harness.{name}] with kind, command and optionally env.")
ILLEGAL_NAME = ("profile '{name}' is not a name charter accepts — letters, digits, '_' and '-', "
    "starting with a letter or digit. Rename the table.")
RESERVED_NAME = ("a profile cannot be named 'default' — [harness] default names which profile "
    "the selector starts on. Rename the table.")
UNKNOWN_KIND = ("profile '{name}' has kind {kind}, which is not a harness charter can launch — "
    "one of: {kinds}. Set kind to one of them.")
BAD_COMMAND = ("profile '{name}' has no usable command — command is a list of arguments, "
    "[\"claude\"], never a shell string, because no shell runs it. Write it as a list.")
BAD_ENV = ("profile '{name}' has an env that is not a table of text values — write "
    "env = {{ NAME = \"value\" }}.")
SECRET_ENV = ("profile '{name}' sets {var}, which is named like a credential — charter holds no "
    "credential in a profile, because anything set on the harness reaches the model's own "
    "shell. Log in inside that harness instead: {login}.")
CHARTER_ENV = ("profile '{name}' sets {var}, one of charter's own variables — the launcher sets "
    "those at exec, and a profile's value would tell every hook the wrong harness or plane. "
    "Remove it.")
CHARTER_COMMAND = ("profile '{name}' runs charter itself — a new chat on it would open the "
    "profile selector again, forever. Give it the harness's own command.")
UNKNOWN_PROFILE_KEY = ("profile '{name}' has {key}, which charter does not read — a profile is "
    "kind, command and env. Remove it.")
CLASHING_NAME = ("profile '{name}' is named like the charter command `charter {name}`, which "
    "would keep it from ever launching. Rename the table.")
TRACKED = ("git tracks charter.local.toml, so the profiles in it would reach every clone of "
    "this plane — charter refuses them until it is untracked: git rm --cached "
    "charter.local.toml, then charter reinit.")
NOT_IGNORED = ("git would commit charter.local.toml, so the profiles in it are refused until it "
    "is ignored — charter reinit adds /charter.local.toml to .gitignore.")
GIT_CANNOT_TELL = ("git could not say whether charter.local.toml is ignored ({why}), so the "
    "profiles in it are refused — an unknown is not a pass. Nothing was started. Check it by "
    "hand: git check-ignore -v charter.local.toml")
DEFAULT_REFUSED = ("[harness] default = \"{value}\" names no profile this machine has — one of: "
    "{names}.")
```

**`.gitignore`.**
- `_GITIGNORE_BASELINE` gains:

  ```
  # This machine's harness profiles (commands, config folders). Never committed: a profile's
  # command runs on a click, and a merged edit would run it on every machine.
  /charter.local.toml
  ```

- `_ensure_gitignore` adds `LOCAL_PROFILES_IGNORE` when it is not a whole line.
- `_ensure_local_profiles_ignored` has `_ensure_local_settings_ignored`'s shape, with the header
  `"added by charter reinit — harness profiles stay on this machine"`, and returns whether it
  appended.
- `cmd_reinit` calls it.

**`ignored_refusal(root)`.**
- `util.git_path_state(root, LOCAL_FILE)` decides (review 4) with **one** git call (re-review
  N8): `git --no-optional-locks -C <root> status --porcelain=v1 --ignored=matching --untracked-files=all -- <path>`,
  run with `LC_ALL=C` so git's message is English whatever `LANG` says.
  - `--no-optional-locks` (ruling 34): a plain `git status` refreshes the index and takes
    `index.lock` when it can. This check runs at launch, again in the pane, in the selector, in
    `harness list` and in doctor, so a concurrent `charter save` or agent commit would fail on
    the lock — #917's failure, which `doctor.check_index_lock` exists for. Measured: the flag
    gives the same `!!` answer. Measured on git 2.50.1
  in a throwaway repo:
  - rc 0, `?? <path>` → `COMMITTABLE` → `NOT_IGNORED`.
  - rc 0, `!! <path>` → `IGNORED` → `""`.
  - rc 0, empty output (the file exists), or any tracked status such as ` M` → `TRACKED`,
    whether or not it is also ignored. `--untracked-files=all` overrides an operator's
    `status.showUntrackedFiles=no`, which would otherwise hide `??`.
  - rc 128 with `fatal: not a git repository` → `NOT_A_REPO` → `""`: nothing to commit to, so
    nothing to refuse.
  - `UNKNOWN_GIT` — git missing, any other non-zero answer (rc 128 `dubious ownership`
    included), or a timeout (`util.ProcTimeout`, caught) → `GIT_CANNOT_TELL`, naming the reason.
- It never raises, so one slow git costs `doctor` its `harness profiles` row and nothing more:
  `doctor._checks` builds its rows in one list with no per-check guard.
- An absent file → `""` without running git: the check is about declared profiles, and a file
  that is not there declares none.

**`charter harness list`.** One line per profile, sorted with built-ins first in registry order,
then declared by name:

```
  NAME          KIND      COMMAND                               FROM
* claude        claude    claude                                built-in
  claude-work   claude    CLAUDE_CONFIG_DIR=~/.claude-work claude   charter.local.toml
```

- `*` marks `default`.
- Measured with `tui.column` (the rule in `commands_workspace.cmd_workspace_list`).
- Then a `refused:` block, one line per `Refused`: `  <name or file>: <reason>`.
- Then the ignored line: when `ignored_refusal(config.ROOT)` is non-empty, `util.warn` it.
  `harness list` is a command someone typed, so the git call is allowed.
- Then a blank line, and today's per-kind ceilings block, unchanged, so
  `tests/test_cmd_harness.py::HarnessList` keeps passing.

**`check_harness_profiles()`.** Returns `Result("harness profiles", …)`:
- **WARN**, `detail=profiles.ignored_refusal(config.ROOT)`, `hint="charter reinit"` — when that
  is non-empty.
- Else **WARN**, `detail=f"{n} refused: {names}"`, `hint=` the first reason — when
  `current()["refused"]` is non-empty.
- Else **WARN**, `detail=DEFAULT_REFUSED…`, `hint="the selector will start on no row until it names one"`
  — when `default_refused` is set.
- Else **OK**, `detail=f"{len(profiles)} profile(s): {names}"`, with an empty hint
  (`test_a_green_doctor_row_keeps_nothing_back`).

It reads the doctor's own `instance.load` result the way the `charter.toml` row does
(`doctor.py:331`), never `config.PROFILES`, because the row is about the file.

### Test cases (write first; each must fail before the code exists)

**`tests/test_harness_profiles_are_read_from_the_local_file.py`**

Class `TheLocalFileIsRead(PersonaIso, unittest.TestCase)`. Helper
`_local(text)` writes `config.ROOT / "charter.local.toml"`, then returns
`profiles.derive(config.ROOT, instance.load(config.ROOT))`. The filename is spelled literally,
not through `profiles.LOCAL_FILE` (a round trip cannot pin a name).

| Case | Asserts |
|---|---|
| `test_a_plane_with_no_local_file_has_exactly_the_built_ins` | `set(r["profiles"]) == {"claude", "codex", "opencode"}`, every `source == "built-in"`, `refused == ()` |
| `test_a_built_in_runs_its_kind_with_no_environment` | `r["profiles"]["claude"] == Profile("claude", "claude", "claude-code", ("claude",), (), "built-in")` |
| `test_a_declared_profile_is_read_with_its_kind_command_and_env` | the spec's `claude-work` example → `kind == "claude"`, `harness == "claude-code"`, `command == ("claude",)`, `env == (("CLAUDE_CONFIG_DIR", "~/.claude-work"),)`, `source == "charter.local.toml"` |
| `test_a_declared_profile_replaces_the_built_in_of_its_name` | `[harness.claude] kind="claude" command=["/opt/claude"]` → `r["profiles"]["claude"].command == ("/opt/claude",)` and source is the local file |
| `test_env_is_optional` | no `env` → `env == ()` |
| `test_the_local_default_wins_over_charter_toml` | charter.toml `default = "codex"`, local `default = "claude-work"` → `default == "claude-work"`, `default_from == "charter.local.toml"` |
| `test_a_default_naming_no_profile_is_refused_by_value` | local `default = "nope"` → `default is None`, `default_refused == "nope"` |

Class `ABrokenProfileIsRefusedAlone(PersonaIso, unittest.TestCase)`. Each case declares one broken
profile beside a good `ok` profile and asserts that `"ok" in r["profiles"]`, the broken name is not
in it, and the refusal's reason contains the given fragment:

| Case | Fragment |
|---|---|
| `test_an_unknown_kind_is_refused_naming_the_kinds` | `"claude, opencode, codex"` |
| `test_a_command_written_as_a_string_is_refused` | `"never a shell string"` |
| `test_an_empty_command_is_refused` | same |
| `test_a_command_holding_a_non_string_is_refused` | same |
| `test_a_profile_named_default_is_refused` | `"cannot be named 'default'"` |
| `test_a_name_with_a_dot_is_refused` | `"letters, digits"` |
| `test_an_env_that_is_not_text_is_refused` | `"table of text values"` |
| `test_an_unknown_profile_key_is_refused` | `"does not read"` |
| `test_an_env_name_starting_with_charter_is_refused` | `env = { CHARTER_HARNESS = "codex" }`, and subTest `charter_root` → `"one of charter's own variables"` |
| `test_a_command_that_is_charter_itself_is_refused_by_current` | subTest over `["charter"]`, `["/usr/local/bin/charter", "claude"]`, `["python3", "-m", "charter"]`, `["edm"]` → absent from `profiles.current()["profiles"]`, reason contains `"runs charter itself"`; `["charterize"]` is not refused |

- `test_a_secret_shaped_env_name_is_refused_for_each_word`:
  - subTest over `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, and lower-case `api_key` → refused.
  - The claude profile's reason contains `"CLAUDE_CONFIG_DIR and run /login"`.
  - A codex profile's contains `"CODEX_HOME and run codex login"`.
  - An opencode one's contains `"XDG_DATA_HOME and run opencode auth login"`.
  - The env VALUE (`"sk-live-123"`) is in no reason.
- `test_an_unreadable_local_file_keeps_the_built_ins`: `"[harness"` → built-ins present,
  exactly one refusal with `name == ""`.
- `test_a_section_other_than_harness_is_refused_by_name`: `[forge]` → refusal names `forge`,
  and `"charter.toml"` in reason.
- `test_each_refusal_says_a_different_thing`: the reasons above are pairwise distinct.

Class `CharterTomlCarriesNoProfile(PersonaIso, unittest.TestCase)`:
- `test_a_profile_table_in_charter_toml_is_refused_with_a_pointer`: `charter.toml` with
  `[harness.x]` → refusal source `"charter.toml"`, `"move the table" in reason.lower()`, and
  `"x" not in r["profiles"]`.
- `test_charter_toml_default_is_still_read`: `default = "codex"` → `default == "codex"`.
- `test_other_keys_in_charter_tomls_harness_are_still_ignored` (pin): `[harness] nothing = "here"`
  → no `Refused` whose source is `"charter.toml"`.

Class `NamesThatClashWithACommand(PersonaIso, unittest.TestCase)`:
- `test_a_profile_named_like_a_command_is_refused_by_current` — `[harness.doctor]` → not in
  `profiles.current()["profiles"]`; reason contains `"charter doctor"`.
- `test_the_command_words_hold_the_core_commands_and_not_the_kinds`:
  - `{"doctor", "workspace", "frame", "frame-new-chat"} <= cli.command_words()`
  - `"claude" not in cli.command_words()`

Class `TheConfigReadIsCheap(PersonaIso, unittest.TestCase)`:
- `test_deriving_profiles_runs_no_subprocess`: `mock.patch("subprocess.run", side_effect=AssertionError)`
  and `mock.patch("subprocess.Popen", side_effect=AssertionError)` around `config.use(config.ROOT)`
  with a local file present, even one that git would commit → no exception.
- `test_config_carries_the_profiles`: after `config.use`, `"claude-work" in config.PROFILES["profiles"]`.

Class `WhatIsGuarded` additions:
- `tests/test_no_test_reads_the_operators_channel.py::WhatIsGuarded::test_the_profiles_are_guarded_like_the_harness_default`
  — `"PROFILES" in _planeguard._GUARDED_SETTINGS`, `"PROFILES" in config.DERIVED`, and
  `config.derive(tmp)["PROFILES"]` is a `dict`.
- `tests/test_plane_write_guard.py::WritesAreRefused::test_the_planes_local_profiles_file_cannot_be_written`
  — the `test_the_planes_own_marker_cannot_be_rewritten` shape (`:391`), with `charter.local.toml`
  → the write raises the guard's refusal. **Hand deletion check required:** the sweep does not
  mutate `tests/`.

**`tests/test_the_local_file_stays_out_of_git.py`**

Class `InitAndReinitIgnoreIt(PersonaIso, unittest.TestCase)` — reuse `tests/test_init.py`'s init
fixture (read it first; do not build a second one):
- `test_a_fresh_plane_ignores_the_local_file`: after `cmd_init`, `"/charter.local.toml"` is a
  whole line of `.gitignore`.
- `test_an_existing_gitignore_gains_the_line_once`: an existing file with the other rules →
  one append; a second `_ensure_gitignore` → bytes unchanged.
- `test_a_rule_that_merely_contains_the_name_does_not_count`: `build/charter.local.toml.bak` →
  still appended.
- `test_reinit_adds_it_to_a_plane_made_before_it`: `.gitignore` without the line, `cmd_reinit`
  → line present and `"/charter.local.toml"` named in stdout.
- `test_reinit_leaves_a_plane_that_has_it_alone`: bytes unchanged. *Pin* — it passes at
  `a5aa860`, where reinit never touches `.gitignore`; it keeps the additive rule once reinit does.

Class `TheFileMustBeIgnoredToBeUsed(PersonaIso, unittest.TestCase)`:
- setUp: `git init` in `config.ROOT` with `tests._gitguard.environment()` in the child env.
- `test_an_absent_file_needs_no_git`: `mock.patch("charter.util.run", side_effect=AssertionError)`
  → `profiles.ignored_refusal(config.ROOT) == ""`.
- `test_an_ignored_untracked_file_is_fine`: `.gitignore` holds the line, file exists → `""`.
- `test_a_file_git_would_commit_is_refused`: no ignore line → `"charter reinit adds" in text`.
- `test_a_tracked_file_is_refused_even_when_ignored`: `git add -f` + commit → `"git rm --cached" in text`.
- `test_a_plane_that_is_not_a_repository_is_not_refused`: no `.git` → `""`.
- `test_git_saying_not_a_repository_is_the_only_pass`: `util.run` patched so the `status` call
  answers rc 128 with `fatal: not a git repository` → `""`; rc 128 with `fatal: detected dubious
  ownership` → `"could not say"`.
- `test_the_ignore_check_takes_no_index_lock`: the recorded argv begins
  `["git", "--no-optional-locks", "-C", <root>, "status"]` (ruling 34).
- `test_one_git_call_answers_every_state`: a real repo in `self.tmp`, with
  `_gitguard.environment()` in the child env — committable, ignored, tracked-clean and
  tracked-modified each give their state, with `util.run` called once per ask (re-review N8).
- `test_a_non_english_locale_still_reads_not_a_repository`: `LANG=de_DE.UTF-8` and
  `LC_ALL=de_DE.UTF-8` in `os.environ` (`clear=True` plus `PATH`) → the git call's env holds
  `LC_ALL == "C"`, and a plane that is not a repository reads `""` (re-review N8).
- `test_git_missing_is_not_a_pass`: `util.run` raises `FileNotFoundError` → `"could not say"`.
- `test_a_git_that_times_out_is_not_a_pass_and_does_not_raise`: `util.run` raises
  `util.ProcTimeout` → `"could not say"`, no exception.
- `test_an_unparseable_status_is_not_a_pass`: rc 0 with `?! <path>` → `"could not say"`.
- `test_the_existing_callers_see_git_ignores_unchanged` (pin: passes at `a5aa860`):
  `util.git_ignores` takes no `timeout` and still answers `None` for a non-zero `rev-parse`.

Class `DoctorWarns(PersonaIso, unittest.TestCase)`:
- `test_a_committable_local_file_is_a_warning_naming_reinit` → `WARN`, `hint == "charter reinit"`.
- `test_a_refused_profile_is_a_warning_naming_it` → `WARN`, the name in detail.
- `test_a_default_naming_no_profile_is_a_warning` → `WARN`, `"names no profile" in detail`.
- `test_a_clean_file_is_ok_with_no_hint` → `OK`, `hint == ""`.
- `test_a_git_that_hangs_costs_one_row` → `util.run` raising `util.ProcTimeout` for git → the
  `harness profiles` row is `WARN` with `"could not say"`, and `doctor.run_all()` returns every
  other row.
- `test_the_row_is_named_in_the_preflight` → `"harness profiles" in doctor.check_names()` and in
  `[r.name for r in doctor.run_all()]`, in order
  (`tests/test_a_table_column_is_measured_in_cells.py::TestDoctorSizesItsNameColumnFromTheChecks`
  stays green).

**`tests/test_charter_harness_list_shows_profiles.py`** — `PersonaIso`, stderr captured from
`commands_harness.cmd_harness_list(SimpleNamespace())`. `util.info` and `util.warn` both write to
stderr (review 11):
- `test_every_built_in_is_listed_as_built_in`: rows `claude`, `codex`, `opencode`, each ending
  `built-in`.
- `test_a_declared_profile_shows_its_command_env_and_file`: the row holds
  `CLAUDE_CONFIG_DIR=~/.claude-work claude` and `charter.local.toml`.
- `test_the_default_is_marked`: the `claude-work` row starts with `*`.
- `test_a_refused_profile_is_listed_with_its_reason`: `refused:` block holds the name and
  `"never a shell string"`.
- `test_a_committable_file_is_said`: no ignore line, in a git repo → stderr holds
  `"charter reinit adds"`.
- `test_a_command_with_control_bytes_is_listed_escaped`: `command = ["claude\r\x1b[2Kharmless"]`
  → stderr holds no `\r` and no ESC byte, and holds that element as `contain.readable` spells
  it (ruling 35).
- `test_the_kinds_ceilings_are_still_listed`: `"↳"` still in stderr after the profiles block.

### Implementation steps

- [ ] Write the three test modules and the two guard cases above.
- [ ] Run `python3 -m unittest tests.test_harness_profiles_are_read_from_the_local_file tests.test_the_local_file_stays_out_of_git tests.test_charter_harness_list_shows_profiles`
      — expect import errors and assertion failures.
- [ ] Add `profiles.py`, `config.PROFILES` (before `HARNESS`), `util.git_path_state`, the `.gitignore` changes,
      `harness list`, `check_harness_profiles`, `cli.command_words`, and the two `_planeguard`
      lines.
- [ ] Re-run the modules; then `python3 -m unittest discover -s tests`.
- [ ] Hand deletion check for both `_planeguard` lines: delete each in a scratch copy, run
      `tests.test_plane_write_guard` and `tests.test_no_test_reads_the_operators_channel` — each
      must go red.
- [ ] `python3 tools/sweep.py`; every survivor either gets a test or its line deleted. Report the
      output in the PR.
- [ ] Docs and news (below), then PR.

### Docs and news

- **`docs/control-plane.md`.**
  - The config reference block (`:93`) shows `[harness] default` only, with a comment that
    profiles live in `charter.local.toml`.
  - `## [harness].default` becomes `## [harness] — profiles, and the default`. It covers:
    - the local file and why only `[harness]` is read there;
    - the profile shape;
    - the refusals, each with its fix;
    - built-ins, and replacing one;
    - no credentials, with the three logins;
    - the ignore guarantee and what `doctor` says.
  - The bare-`charter` text stays as it is. Nothing launches differently yet, and this page says
    so: "Profiles are listed by `charter harness list`; launching one arrives in a later release."
- **`docs/harnesses.md`.** A new `## Profiles` after the intro: `charter harness list`'s
  columns, and that a profile is added by editing the file (no `charter harness add`, and why).
- **`docs/news/unreleased-harness-profiles.md`** (created):

  ```
  ---
  version: unreleased
  headline: Charter reads harness profiles from charter.local.toml, a file it keeps out of git
  adopt: reinit
  ---
  ```

  Body: one paragraph on the failure (two Claude Code accounts, a pinned Codex, no way to tell
  charter). What this release does: `charter harness list` shows every profile, the file it came
  from and why any was refused, and `charter reinit` adds `/charter.local.toml` to `.gitignore`.
  What it does not do yet: launch a profile. No `check:`, because no subcommand's exit code
  answers whether the ignore line is present.

**Suggested PR title:** Charter reads harness profiles from a file it keeps out of git, and lists them

---
## Task 2: A profile launches — `charter <profile>`, the launcher and its `exec`, the profile carried by `+`, tabs, reopen and handoff

**Depends on:** Task 1 (`profiles.current`, `Profile`, `ignored_refusal`, `expanded_*`,
`cli.command_words`).

### Step 0 — Measure first. A failed measurement stops this task.

**The claim.** Every chat pane's first process is a charter launcher that `exec`s the harness
with the profile's env. The claim says this leaves these as they are today:
- the pane id;
- `remain-on-exit`;
- the `pane-died` path;
- `pane_current_command` once the harness runs;
- `_pane_last_words`;
- the chat-status paths.

Nothing in `charter/` is written until every reading below is in.

**Where.** A throwaway script under a short `/tmp/cp-measure/` directory: a tmux socket path
under the session scratchpad exceeds tmux's 104-byte limit. Nothing is committed from it. The
readings go into the PR description and into `docs/frame.md` (*Docs*, below).

**Versions.**
- `/opt/homebrew/bin/tmux` 3.7c.
- The 3.2 floor: a directory holding a `tmux` symlink to
  `~/.local/share/charter-testing/tmux-3.2`, first on `PATH`. This is the convention
  `tests/test_a_real_click_on_a_real_tab_bar_switches.py:34` and
  `tests/test_frame_tmux_integration.py:6297` record. It is the only way charter's own
  `tmuxctl` calls reach the floor: `tmuxctl.server_argv` hard-codes `"tmux"` (`tmuxctl.py:398`),
  and `tmuxctl.FLOOR = (3, 2)` (`:57`) is the floor charter warns below and does not refuse
  below.
- If that binary is gone, the floor run is reported as not done. It is not skipped silently.

**Servers.** Run every reading below on both arms, on both versions and on both servers:
- **charter's own server:** a `-L` socket from `tests._tmuxreap.name("launcher-measure")`, with
  the window started by `layout.session_argv(...)` for the first chat and by
  `layout.chat_window_argv(...)` for a second;
- **an operator's tmux:** a `-S /tmp/cp-measure/op` server started with `-f /dev/null
  new-session -d`, with the chat built the way `_launch_in_operator_tmux` builds it —
  `layout.window_argv(...)`, `commands_frame._remain_on_exit_argv(...)`, then
  `layout.respawn_argv(...)`.

**The two arms.**
- **A (today):** the harness argv is a stand-in harness directly. `/bin/sleep 30` for the
  process readings; a recorder script that writes `sys.argv` and `os.environ` to a file and
  exits with `$EXIT_WITH` for the exit readings.
- **B (launcher):** the argv is `[sys.executable, "-P", "/tmp/cp-measure/launch.py", "--", *A's argv]`.
  `launch.py`:
  - sleeps 0.3 s, so a reading before the `exec` is possible;
  - sets `CLAUDE_CONFIG_DIR=/tmp/cp-measure/alt` and `CHARTER_HARNESS_PROFILE=alt` in a copy
    of `os.environ`;
  - calls `os.execvpe(argv[0], argv, env)`;
  - on `--refuse`, prints `charter: profile 'alt' is not wired — run charter harness install alt`
    and exits 3 instead of the exec.

**Readings.** Each is a stop criterion unless it says otherwise.

| # | Reading | Pass when |
|---|---|---|
| L1 | `display-message -p -t <pane> '#{pane_id} #{pane_pid}'` before B's exec (at 0.1 s) and after (at 1.0 s) | the pane id is the one `-P -F` reported, on both readings; B's pid is the same before and after the exec (on the operator arm, after the respawn) |
| L2 | recorder with `EXIT_WITH=7`; `_query_pane_dead_status(socket, pane)`; `_pane_died_write_hook_argv` + `_pane_died_teardown_hook_argv` installed as `_launch` installs them, then read the exit file and `list-windows` | A and B both answer 7 from the query; the write hook's file holds `7`; the teardown hook removed the window; on the operator arm `_wait_for_harness` returns 7 |
| L3 | `#{pane_current_command}` polled (the `_settled` loop, `tests/test_quit_and_reopen_on_a_real_tmux.py:327`) after the exec | B's settled value equals A's (`sleep`). The value before the exec (the interpreter's name) is recorded for the docs, not a criterion. Nothing in `charter/` reads this format; only tests do (Ruling 3) |
| L4 | B with `--refuse`; `_pane_last_words(socket, pane)` | returns a list containing the printed refusal line; `_query_pane_dead_status` answers 3 |
| L5 | inside the exec'd recorder: `$TMUX_PANE`; then `state.is_live(fid, pane=<that>)` after `state.record_server`/`record_harness_pane`; `_live_chats(socket)` after `_chat_option_argv` | `$TMUX_PANE` equals the recorded pane; `is_live` is True; `_live_chats` contains the chat id — identically in A and B |
| L6 | the recorder's env file; `tmux show-environment -g` and `show-environment -t <session>` | B's harness sees `CLAUDE_CONFIG_DIR=/tmp/cp-measure/alt` and `CHARTER_HARNESS_PROFILE=alt`; neither name appears in either `show-environment` output; no argv tmux was handed contains `/tmp/cp-measure/alt` |
| L7 | the frame's existing `-e` overlay (`_frame_identity_env`) on both arms | `CHARTER_HARNESS` and `CHARTER_SESSION_ID` reach B's harness exactly as they reach A's |
| L8 | B's `os.getpid()` before its `exec`, beside one `list-panes -a -F '#{pane_pid}\t#{pane_dead}\t#{pane_id}\t#{session_name}\t#{window_name}\t#{@charter_chat}'` (the plan author read `#{@charter_chat}` and `#{pane_dead}` through that format on 3.7c), on both servers — a first chat (`session_argv`), a second (`chat_window_argv`), and the operator's `respawn-pane` — plus a child B spawns | B's pid is the `#{pane_pid}` of the window named for its chat; the child's is not. The plan author pre-measured this on 3.7c and 3.2 with a Python stand-in: first command, second window and respawned pane were each equal to `#{pane_pid}`, and the child differed. Re-run with the real launcher (ruling 29, revised) |

**Stop rule, written into the dispatch.** If any criterion fails on any version or server, the
implementer:
- stops Task 2 before any code;
- writes the table of readings (version, server, arm, value) into the Task 2 issue;
- reports to the controller.

The design is not adjusted to route around a failed reading. The launcher is the spec's ruling,
and a failure changes the spec.

### Files

- Create:
  - `charter/frame/launcher.py`;
  - the five Task 2 test modules (File Structure).
- `charter/cli.py`:
  - `_wire` (`:692`): add `--profile NAME`, `help=argparse.SUPPRESS`.
  - `_OWN_VALUE_FLAGS` (`:1736`): add `"--profile"`.
  - `_add_frame_parsers`: register `frame-launch` beside `frame-new-chat`, `func=launcher.cmd_frame_launch`
    (args `--profile`, `--attended` — absent means unattended (review 3) — and `rest` as `nargs=REMAINDER`), and add the word to
    `_core_commands` (`:793`).
  - `main` (`:2001`): `argv = _profile_launch(argv)` directly after `_bare_launch`.
  - `_bare_launch` (`:1927-1954`): read `config.PROFILES["default"]` and `default_refused`
    instead of `config.HARNESS`.
- `charter/commands_frame.py`:
  - `_launch` (`:5090`): profile resolution after `h = …` (`:5100`); `argv` (`:5107`); the
    bypass branch (`:5124`); the `shutil.which` branch (`:5153`); `state.record_profile` after
    `record_workspace` (`:5401` and `:3435`); the launcher argv into `chat_window_argv` /
    `session_argv` (`:5510-5518`) and `_launch_in_operator_tmux` (its call at `:5231`; its respawn at `:3499`).
  - `early_death_message(argv, …)` callers (`:5719`, `:3530`) and
    `_say_why_the_harness_did_not_start` (`:5524`, `:3503`) get the display argv.
  - `_same_harness_as` (`:7649`) → `_same_profile_as`; `NO_HARNESS` (`:7645`) reworded;
    `PROFILE_GONE` added beside it.
  - `_open_workspace` (`:7820-7833`), `cmd_new_chat` (`:10515-10551`), `background_refusal`
    (`:10686`), `open_in_background` (`:10730-10753`).
  - `_reopen_one` (`:10131-10145`), `_reopen_args` (`:10191`), `_record_the_plane` (`:9393`).
- `charter/frame/tmuxctl.py` — `live_pane_by_pid(server, pid)` beside `operator_server`;
  `tmuxctl.py` stays the only module that calls tmux.
- `charter/frame/state.py:1950` — `record_profile`/`profile` after `chat_cwd`.
- `charter/frame/reopen.py:74-120,294-309` — `Chat.profile`; `_chat` reads it.
- `charter/frame/leave.py:51,134-188,418-422` — `Doomed.profile`; `plan` fills it; `title`
  shows it.
- `charter/frame/chats.py:377` — `profile_of(fid)` beside `harness_of`.
- `charter/frame/choose.py:341` — `_note` shows `chats.profile_of(name) or chats.harness_of(name)`.
- `charter/instance.py:2952` — `harness_of` keeps validating `charter.toml`'s `default` shape
  for `doctor`'s `charter.toml` row. It no longer refuses a value that names a declared profile:
  it takes `profiles=` and treats those names as legal.
- `tests/_envguard.py:206-209` — `_loud_names()` adds `"CHARTER_HARNESS_PROFILE"` explicitly,
  not through `_FRAME_IDENTITY`.
- `tests/test_plane_spawn_guard.py:878` — the `KNOWN` entry.
- Updated literals:
  - `tests/test_a_real_click_on_a_real_tab_bar_switches.py:1085` (`_NO_HARNESS`);
  - `tests/test_the_branch_not_taken_is_still_a_promise.py:597`;
  - `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py:326` (the reopen fallback
    this task removes).
  - `tests/test_bare_charter_opens_the_frame.py`: the classes that pin `config.HARNESS` reading
    move to `config.PROFILES`. Grep for every assertion keyed on the old sentence in both
    directions before rewording (`assertIn` and `assertNotIn`).
- Existing tests this task breaks, and how each changes (review 9):
  - `tests/test_frame_launcher.py::MainDeliversFrameRest` (a plain `TestCase`):
    `cli.main(["claude", "--no-frame", "-p", "hi"])` now reaches `profiles.current()` in
    `_launch`, which `_planeguard` refuses to read off the real plane, and a profile launch calls
    `launcher.start`, not `bypass`. It moves onto `PersonaIso`, patches
    `charter.frame.launcher.os.execvpe`, and asserts the exec'd argv `["claude", "-p", "hi"]`.
  - `tests/test_bare_charter_opens_the_frame.py::EveryOtherCommandIsUnchanged` (`_BareLaunchCase`,
    a plain `TestCase`): `test_an_unknown_command_is_still_an_unknown_command` makes
    `_profile_launch` read `config.PROFILES`, and
    `test_the_harness_flags_still_reach_the_typed_launcher` reaches `_launch`. The class moves
    onto `PersonaIso`.
  - Real-tmux tests that start a chat through `_launch` —
    `tests/test_a_background_chat_really_starts_on_its_brief.py` and
    `tests/test_a_workspace_tab_opens_what_it_names.py::ARealTabOpensARealWorkspace` — now hand
    tmux an argv holding `-m charter`. `_planeguard._cmd_launches_charter` refuses that unless
    the child's plane is the tmp one, so each states `CHARTER_ROOT` for the tmux client.
  - `tests/test_frame_launcher.py::MissingTmux` (a plain `TestCase`, `cmd_launch(harness="claude")`):
    `_launch` now reads `profiles.current()` before `tmuxctl.version()`, which `_planeguard`
    refuses off the real plane. It moves onto `PersonaIso` (re-review N4).
  - `tests/test_frame_launcher.py::MissingHarnessBinary`: its launch-level cases asserted that a
    missing binary reaches `bypass`'s 127 sentence. A profile launch now refuses before tmux
    with `NOT_ON_PATH`, still 127, and those cases assert that sentence. The case that calls
    `commands_frame.bypass([...])` directly is unchanged.
  - `tests/test_frame_launcher.py::BypassRouting`: a harness launch with no frame now routes to
    `launcher.start`, not `bypass`, and `charter frame -- <cmd>` still routes to `bypass`. Each
    case asserts the function its launch really reaches.
- Docs: `docs/frame.md`, `docs/control-plane.md`, `README.md:89-116`, `docs/install.md:296-305`.

### Interfaces

**Consumes:**
- From Task 1: `profiles.current()`, `Profile`, `expanded_command`, `expanded_env`, `display`,
  `ignored_refusal`, `cli.command_words()`.
- On `main`:
  - `commands_frame.bypass` (`:403`), `_frame_env` (`:2430`), `_frame_identity_env` (`:2506`),
    `_guest_harness_env` (`:2550`)
  - `layout.session_argv` (`layout.py:1301`), `chat_window_argv` (`:1357`), `window_argv`
    (`:1184`), `respawn_argv` (`:1227`)
  - `util.self_relaunch_argv` (`util.py:292`)
  - `state.identity` (`state.py:1803`), `record_workspace` (`:886`)
  - `leave.resumable_harness` (`leave.py:392`)
  - `reopen_state.read`/`write`
  - `Reopening`/`_reopening` (`:4812`), `Opening`/`_opening` (`:4853`)
  - `_consume` (`:9955`), which keeps a chat that did not come back in the manifest for a retry

**Produces:**

```python
# charter/frame/launcher.py
REFUSED_EXIT = 3        # a launcher that refused in the pane, after printing why
MISSING_EXIT = 127      # the command is not on PATH (the shell's own number, as `bypass`)

def argv(profile: str, rest: list[str], *, attended: bool) -> list[str]: ...
    # util.self_relaunch_argv("frame-launch", "--profile", profile,
    #                         *(("--attended",) if attended else ()), "--", *rest)
    # unattended unless told: a pane nobody asked for never waits on a question (review 3)
def environment(p: profiles.Profile, base: Mapping[str, str], *,
                framed: bool) -> dict[str, str]: ...
    # dict(base) — minus CHARTER_SESSION_ID when not framed, and nothing else (review 8) —
    # | expanded_env(p) | {"CHARTER_HARNESS": p.harness, "CHARTER_HARNESS_PROFILE": p.name}
KIND_IGNORED, KIND_PATH, KIND_NOT_YET, KIND_ASK, KIND_UNATTENDED, KIND_NOT_WIRED, KIND_CANNOT_TELL = (
    "ignored", "path", "not-yet", "ask", "unattended", "not-wired", "cannot-tell")
KIND_EXEC, KIND_RECORD = "exec", "record"       # exec raised after on_exec; a yes not recorded (Task 3)

class Refusal(NamedTuple):
    kind: str      # one of the KIND_* constants; every caller branches on this (re-review N2)
    text: str      # the sentence printed; never compared, NEEDS_ASKING's included
    exit: int      # what a CLI returns for it: MISSING_EXIT for KIND_PATH, else REFUSED_EXIT

def refusal(p: profiles.Profile, *, root: Path, attended: bool,
            env: Mapping[str, str], cwd: Path) -> Refusal | None: ...
    # None when it may start. Task 2's order: ignored_refusal (declared profiles only), the
    # command on PATH, then _approval_refusal. Task 3 swaps _approval_refusal's body; Task 4
    # appends its wiring check, probing from cwd (see Behaviour).
def _approval_refusal(p: profiles.Profile, *, attended: bool) -> Refusal | None: ...
    # Task 2: Refusal(KIND_NOT_YET, DECLARED_NOT_YET formatted, REFUSED_EXIT) when
    #         p.source != BUILTIN, else None (review B1)
    # Task 3: profiletrust.refusal(p, attended=attended)
def start(p: profiles.Profile, rest: list[str], *, fid: str | None,
          attended: bool) -> int: ...
    # r = attempt(p, rest, fid=fid, attended=attended); util.err(r.text); return r.exit
def attempt(p: profiles.Profile, rest: list[str], *, fid: str | None, attended: bool,
            on_exec: Callable[[], Callable[[], None]] = lambda: (lambda: None)) -> Refusal: ...
    # refusal() from the top. Task 3 adds: KIND_ASK with a terminal on both ends asks, and a
    # yes runs the whole chain again from the top (re-review N2). None → undo = on_exec(),
    # record the profile under fid, os.execvpe(cmd[0], cmd + rest, env). An OSError from
    # execvpe runs undo() and returns Refusal(KIND_EXEC, EXEC_FAILED…, code) (N7 nit). Returns
    # the final Refusal only when it did not exec — the selector goes back to its list with a
    # launch refusal (re-review N7), and exits on KIND_EXEC.
def framed_chat() -> str | None: ...
    # the chat this process claims ($CHARTER_SESSION_ID) only when tmux itself says os.getpid()
    # is the #{pane_pid} of a LIVE pane that belongs to that chat, on that chat's server
    # (state.frame_server(chat), written before the pane exists; else commands_frame.SOCKET):
    # - charter's own server: the pane's window is named for the chat;
    # - the operator's tmux (tmuxctl.is_operator_socket): its @charter_chat option is the chat —
    #   set before respawn-pane (commands_frame.py:3480 before :3499), and no pane output or
    #   operator hook renaming the window changes it (ruling 33).
    # Never $TMUX, never $TMUX_PANE, never charter's pane record (ruling 29, revised). None
    # otherwise, and when a chat was claimed but not proven it prints UNPROVEN_CHAT once.
    # The first thing cmd_frame_launch does (ruling 35).
UNPROVEN_CHAT = ("charter: this launch was given chat '{chat}' but is not that chat's pane, so "
                 "it runs with no frame and records nothing for '{chat}'.")

# charter/frame/tmuxctl.py
def live_pane_by_pid(server: str, pid: int) -> tuple[str, str, str, str] | None: ...
    # one list-panes -a -F
    #   '#{pane_pid}\t#{pane_dead}\t#{pane_id}\t#{session_name}\t#{window_name}\t#{@charter_chat}'
    # through server_argv(server); (pane_id, session_name, window_name, charter_chat) of the
    # row whose pane_pid is pid and whose pane_dead is 0 — a dead remain-on-exit pane keeps
    # its old pid, which the system may have reused (ruling 33) — or None, a server that does
    # not answer included
def cmd_frame_launch(args) -> int: ...
    # resolves args.profile via profiles.current(); a name it cannot resolve → util.err and
    # REFUSED_EXIT; fid = framed_chat(), and None makes it a launch with no frame

# charter/frame/state.py
def record_profile(fid: str, name: str) -> None: ...   # .charter/frame/<fid>/profile
def profile(fid: str) -> str | None: ...               # None for absent/unreadable/empty

# charter/frame/chats.py
def profile_of(fid: str) -> str: ...                   # state.profile(fid) or ""

# charter/frame/reopen.py
class Chat(NamedTuple):
    ...                  # existing eight fields unchanged, then:
    profile: str = ""

# charter/frame/leave.py
class Doomed(NamedTuple):
    ...                  # existing fields, then:
    profile: str = ""

# charter/commands_frame.py
NO_HARNESS = ("this chat records no profile this charter can launch, and this plane "
              "declares no `[harness] default`")
PROFILE_GONE = ("this chat ran on profile '{name}', which this plane no longer declares — "
                "declare it again in charter.local.toml, or open a chat on a profile you "
                "name: charter <profile>")
def _same_profile_as(fid: str) -> tuple["profiles.Profile | None", str]: ...
    # (profile, "") or (None, the refusal: PROFILE_GONE or NO_HARNESS)

# charter/cli.py
def _profile_launch(argv: list[str]) -> list[str]: ...
    # [p.kind, "--profile", p.name, *argv[1:]] for a declared, non-built-in profile named by
    # argv[0]; argv untouched otherwise, and config.PROFILES is not read when argv[0] is
    # empty, starts with "-", or is in command_words() or _frame_command_names()
```

### Behaviour

**`charter <profile>`.**
- `_profile_launch` runs in `main` right after `_bare_launch`. It rewrites a declared profile's
  name into its kind's launcher plus `--profile`. That is `_bare_launch`'s own shape: a rewrite,
  so every splitter and flag below runs on the tokens a typed command produces.
- It never registers a subparser per profile. `build_parser()` runs in every `charter hook …`
  process and in every plain `unittest.TestCase` that builds the parser, and reading
  `config.PROFILES` there meets `_planeguard`'s read refusal (`_GUARDED_SETTINGS`).
- A declared profile named like a built-in (`claude`) is not rewritten. `charter claude` already
  parses, and `_launch` resolves `args.profile or args.harness` to the declared profile.

**`_launch`, after `h = …` (`:5100`).**
1. `name = getattr(args, "profile", None) or args.harness`. When `h` is `None`
   (`charter frame -- <cmd>`) there is no profile and nothing below changes.
2. `read = profiles.current()`; `p = read["profiles"].get(name)`.
   - `p is None` and a `Refused` carries that name → `util.err(f"charter: {reason}")`, return 2.
   - `p is None` otherwise → `util.err(UNKNOWN_PROFILE.format(name=…, names=…))`, return 2.
   - `p.kind != args.harness` → `util.err(KIND_MISMATCH.format(…))`, return 2.
3. `attended = _reopening(args) is None and _opening(args) is None`.
   `display = profiles.expanded_command(p) + rest`.
   `argv = launcher.argv(p.name, rest, attended=attended)`.
4. **Bypass.** `args.no_frame or (not sys.stdout.isatty() and _wants_attach(args))` →
   `return launcher.start(p, rest, fid=None, attended=sys.stdin.isatty() and sys.stdout.isatty())`,
   in place of `bypass(argv)`. The `--no-frame` harness gets the same checks.
   - Its environment drops exactly one inherited name: `environment(..., framed=False)` removes
     `CHARTER_SESSION_ID` and sets `CHARTER_HARNESS` to the launched kind (review 8).
   - Why that one: the defect is the pair. `hooks._record_harness_session` acts on a
     `CHARTER_SESSION_ID` beside `CHARTER_HARNESS=claude-code`, so a bare harness typed inside a
     chat would write its own session id into that chat's state.
   - `CHARTER_ROOT`, `CHARTER_WORKSPACE` and `CHARTER_PERSONA` stay, because they are pins. A
     bare harness started from a chat's shell staying in that chat's plane and workspace is what
     its operator wants, and `CHARTER_PERSONA=forge charter claude --no-frame` means what it
     says. Dropping them would buy nothing against the pair.
5. **Pre-tmux refusal.** The old `if h and not shutil.which(h.binary): return bypass(argv)` is
   replaced by `r = launcher.refusal(p, root=config.ROOT, attended=attended,
   env=launcher.environment(p, os.environ, framed=True), cwd=Path.cwd())`. A `Refusal` →
   `util.err(r.text)`, return `r.exit` (`MISSING_EXIT` for `PATH`, `REFUSED_EXIT` otherwise).
   Nothing has been allocated yet, so this is a `return`. Nothing compares `r.text`
   (re-review N2). From Task 4, an `Opening` skips
   the wiring probe here: `background_refusal` probed once from the target workspace's
   directory, and the pane probes again before `exec` (re-review N8).
6. `state.record_profile(fid, p.name)` beside `state.record_workspace(fid, ws)` on both paths.
7. `display`, never `argv`, reaches `early_death_message` and `_say_why_the_harness_did_not_start`,
   so an early death names `claude --resume …`, not `python -P -m charter frame-launch`.
8. Unchanged: `_frame_env(fid, h)` still sets `CHARTER_HARNESS=h.name` on `-e`, and
   `_frame_identity_env`'s five names are the only `-e` names.
9. **In the operator's own tmux the `cat` placeholder stays** (Ruling 4).
   - `layout.window_argv` still creates the pane on `layout.PLACEHOLDER`.
   - `_remain_on_exit_argv` is still armed on it.
   - `layout.respawn_argv` carries `launcher.argv(...)` where it carried the harness argv.
   - A launcher in `cat`'s place could `exec` a harness that dies before `remain-on-exit`
     exists (#384).

```python
UNKNOWN_PROFILE = ("charter: no profile named '{name}' — have: {names}. Nothing was started. "
                   "charter harness list shows every profile and why any was refused.")
KIND_MISMATCH = ("charter: profile '{name}' is a {kind} profile, not a {asked} one — nothing "
                 "was started. Run it by its own name: charter {name}")
```

**The launcher.** `launcher.refusal(p, *, root, attended, env)`:
1. `p.source != BUILTIN` and `profiles.ignored_refusal(root)` non-empty →
   `f"charter: profile '{p.name}' is refused — {that}"`. A declared replacement of a built-in
   (`[harness.claude]`) has `source == LOCAL_FILE`, so `charter claude` refuses too, and never
   falls back to the built-in (Ruling 19).
2. `shutil.which(expanded_command(p)[0], path=env.get("PATH"))` is `None` → `NOT_ON_PATH`.

   ```python
   NOT_ON_PATH = ("charter: profile '{name}' runs {cmd}, which is not on PATH — nothing was "
                  "started. Install it, or give the profile a command with a full path in "
                  "charter.local.toml.")
   ```

3. `_approval_refusal(p, attended=attended)` non-empty → that (review B1).
   - In Task 2 it is `DECLARED_NOT_YET` for every profile whose `source != BUILTIN`, a declared
     replacement of a built-in included. So `main`, between Task 2's merge and Task 3's, never
     runs a command a chat could have written into `charter.local.toml` — through
     `charter <profile>`, `+`, a tab, reopen or a handoff.
   - Task 3 replaces the body with `profiletrust.refusal` and deletes the constant.
   - Task 4 appends `wiring.refusal` after it.
   - The order is the spec's: ignored, asked, wired. Asked comes before wired because the wiring
     probe runs the profile's command.

   ```python
   DECLARED_NOT_YET = ("charter: profile '{name}' comes from charter.local.toml, and this charter "
       "cannot yet ask before a declared command runs, so it runs none — nothing was started. "
       "Until it can, launch a built-in the file does not replace: {builtins}.")
   ```

`launcher.start`:
1. `env = environment(p, os.environ, framed=fid is not None)`.
2. `r = refusal(…)`. A `Refusal` → `attempt` returns it, and `start` prints `r.text` and returns
   `r.exit`. Task 3 adds the one exception: `KIND_ASK` with a terminal on both ends asks, and
   a yes goes back to step 2 — the whole chain again, never straight to `exec` (re-review N2).
3. `fid` set → `state.record_profile(fid, p.name)`.
4. `cmd = expanded_command(p)`; `os.execvpe(cmd[0], cmd + rest, env)`.
5. Any `OSError` from `execvpe` runs the undo that `on_exec` returned (N7 nit), then
   `attempt` returns `Refusal(KIND_EXEC, EXEC_FAILED.format(cmd=…, why=…), code)`.
   - `code` is 127 for `FileNotFoundError` and 126 for `PermissionError` — `bypass`'s pair —
     and `REFUSED_EXIT` for anything else.
   - The pane prints the reason. What `on_exec` recorded is undone, so a pane that is no longer
     running anything does not also claim to be a chat.

   ```python
   EXEC_FAILED = ("charter: profile '{name}' could not be started — {cmd} failed to run ({why}). "
                  "Nothing is running here; fix the command in charter.local.toml.")
   ```

`cmd_frame_launch(args)`:
- **Frame identity is proven by pid, never by what is inherited** (ruling 29, revised).
  - Why the environment cannot be proof: a model's own tool shell inherits the chat's `$TMUX`,
    `$TMUX_PANE` and `$CHARTER_SESSION_ID`. Measured in a real framed Claude Code chat, its Bash
    shell (pid 4707, ppid 53118) saw `TMUX_PANE=%3195`, whose `#{pane_pid}` was 53118. A
    `charter frame-launch --profile claude` run from that tool would pass an environment check
    and rewrite the chat's `profile` record, which reopen follows.
  - **It is the first thing `cmd_frame_launch` does** (ruling 35): before `pane.claim()`,
    before the selector draws, before any output. That keeps true the premise that the pane has
    printed nothing yet.
  - `fid = framed_chat()`. The claimed chat is `$CHARTER_SESSION_ID`. It counts only when
    `tmuxctl.live_pane_by_pid(server, os.getpid())` finds a live pane that belongs to it.
    - **On charter's own server, the window's name is the proof.** Charter's config is loaded
      there, and nothing charter runs emits a title escape before `exec`.
    - **In the operator's tmux, the `@charter_chat` window option is the proof** (ruling 33).
      An operator hook, a plugin, or `allow-rename on` output can rename a window, and
      `_CHAT_OPTION`'s note (`commands_frame.py:202-214`) records a window name as only a
      label. The option is set before `respawn-pane` (`:3480` before `:3499`), and pane output
      cannot change it.
    - Either way `#{pane_pid} == os.getpid()` on a pane whose `#{pane_dead}` is 0.
  - A launch that claims a chat and cannot prove it prints `UNPROVEN_CHAT` once, rather than
    downgrading in silence. Otherwise a chat whose window an operator's hook renamed would lose
    its session id and resume id with no word said.
  - `server` is `state.frame_server(chat)`, which `_launch` writes before the pane exists, else
    `commands_frame.SOCKET`. It is charter's own server by socket name on the private path, the
    operator's socket on theirs, and never `$TMUX`.
  - The answer comes from tmux, not from `state.harness_pane`, so nothing races `_launch`
    recording the pane, and nothing waits.
  - It runs before `exec`, while the launcher is the pane's first process. After `exec`,
    `#{pane_pid}` is the harness, and every child of the harness has another pid.
  - Why the pid is right on every path: every path names the window after the chat
    (`layout.session_argv`'s `-n chat`, `layout.chat_window_argv`'s `-n chat`,
    `layout.window_argv`'s `-n fid`). A multi-argument command is exec'd with no shell, so the
    launcher is the pane's process. `respawn-pane` makes the process it starts the pane's new
    `#{pane_pid}` (measurement L8).
  - Without the proof it is a launch with no frame: `fid=None`, `framed=False` drops
    `CHARTER_SESSION_ID` (review 8), and no chat record is written.
  - **Limit, stated in the docs:** a process that deliberately starts its own tmux pane, with a
    window named like a chat or an `@charter_chat` set to one, passes this check. It is a guard
    rail against a model's accidental misuse, not a boundary.
- `rest` loses a leading `--` the way `_launch` strips it (`:5105`).
- Every failure prints to the pane's stdout/stderr and exits non-zero. `_launch`'s eager
  `_query_pane_dead_status` then reports it with `_pane_last_words` (L4), so the operator reads
  charter's own sentence.

**`KNOWN` entry** (`tests/test_plane_spawn_guard.py`):

```python
"charter/frame/launcher.py:execvpe":
    "The launcher hands its pane — or, on `--no-frame`, this process — to a harness "
    "profile's command and replaces itself. A profile whose command's first word is charter "
    "is refused by `profiles.current` before any launch (Ruling 14), so what runs afterwards "
    "is a harness, not charter. A wrapper script that execs charter is the limit.",
```

**`_same_profile_as(fid)`**, which replaces `_same_harness_as` at all four callers:
1. `recorded = state.profile(fid)`. If set, return `(read["profiles"][recorded], "")` or
   `(None, PROFILE_GONE.format(name=recorded))`. A gone profile is never substituted, even
   by `default`.
2. Else a chat from before this change: the kind from `state.identity(fid)["CHARTER_HARNESS"]`
   → `harness.get(kind)` → its `cli_name`, looked up in `read["profiles"]` (a declared
   replacement of the built-in included) → `(p, "")`.
3. Else `read["default"]` names a profile → `(p, "")`.
4. Else `(None, NO_HARNESS)`.

Callers:
- `cmd_new_chat`: `_say_on_screen(fid, f"cannot open another chat: {why}")`.
- `_open_workspace`: `f"cannot open '{ws}': {why}"`.
- `background_refusal`: `f"cannot open a chat in '{ws}': {why}"`. With a profile in hand it also
  asks `launcher.refusal(p, root=config.ROOT, attended=False, env=launcher.environment(p, os.environ, framed=True))`
  with `cwd=` the target workspace's directory — `workspace.workspace_dir(ws)` when it exists,
  `config.ROOT` for one `--create` has not made yet, and never `_launch_root`, which creates
  it. It says `r.text` the same way, so a handoff to a declared profile refuses before anything
  is written (review B1, re-review N8). `UNMEASURED_FIRST_MESSAGE`
  keeps naming the kind: `first_message_argv` is a kind's.
- `open_in_background`: passes `harness=p.kind, profile=p.name`.

Each launch namespace gains `profile=p.name`; `_reopen_args` gains `profile=c.profile or None`.

**Reopen.** `_reopen_one(c)`:
1. `name = c.profile`, or, for a manifest written before this change, the `cli_name` of
   `harness.get(c.harness)`.
2. `p = profiles.current()["profiles"].get(name)`. `None` →
   `_report(quiet, util.warn, REOPEN_GONE.format(chat=c.chat, name=name or c.harness or "nothing"))`,
   return `None`. **The `[harness] default` fallback (`:10133-10145`) is deleted.** The spec
   says reopen never gives a chat another profile (Ruling 15).
3. Otherwise as today, with `_reopen_args(c, harness_name=p.kind, profile=p.name, …)`.

```python
REOPEN_GONE = ("charter reopen: {chat} ran on profile '{name}', which this plane no longer "
               "declares — not reopened, because another profile may be another account, "
               "where its conversation does not exist. Declare '{name}' again in "
               "charter.local.toml and run charter reopen to bring it back.")
```

The sentence is true: `_consume` leaves an unreopened chat in the manifest (`:9964-9985`).

**The record.**
- `leave.plan` fills `profile=state.profile(fid) or ""`.
- `_record_the_plane` passes `profile=c.profile` into `reopen_state.Chat`.
- `reopen._chat` adds `"profile"` to its text keys. A missing key reads as `""` and `VERSION`
  stays 1, the migration rule at `:297-302`.
- `leave.title(c)` is `f"{c.chat} · {c.profile or c.harness}"`.
- `choose._note` shows the profile first.
- The `_reopen_one` lines name `name`, not the kind.

**`CHARTER_HARNESS_PROFILE` in tests.**
- `_envguard` already scrubs it (prefix `CHARTER_`).
- `_loud_names()` adds it, so a test that reads it without declaring it is refused. The scrub
  alone would let a test pass inside a live frame that exports it.

### Test cases (write first; each must fail before the code exists)

**`tests/test_charter_names_a_profile_on_the_command_line.py`** — `PersonaIso`. A local file
declares `claude-work` (kind `claude`, command `["claude"]`, env `CLAUDE_CONFIG_DIR = "~/.cw"`),
`codex-pinned` (kind `codex`) and a gitignore'd git repo in `config.ROOT`.

| Case | Asserts |
|---|---|
| `test_a_declared_profile_name_becomes_its_kinds_launcher` | `cli._profile_launch(["claude-work", "-p", "hi"]) == ["claude", "--profile", "claude-work", "-p", "hi"]` |
| `test_a_built_in_name_is_left_as_typed` | `["claude"]` unchanged |
| `test_a_core_command_never_reads_the_profiles` | `mock.patch.object(config, "PROFILES", <object whose __getitem__ raises>)`; `_profile_launch(["doctor"])` and `(["frame-new-chat"])` unchanged, no raise |
| `test_an_unknown_word_is_left_for_argparse` | `["nope"]` unchanged |
| `test_the_profile_flag_is_charters_and_the_rest_is_the_harnesses` | `cli._split_frame_argv(["claude", "--profile", "claude-work", "-p", "hi"])` → rest `["-p", "hi"]` |
| `test_the_parser_carries_the_profile_to_the_launcher` | `build_parser().parse_args(["claude", "--profile", "claude-work"])` → `func is commands_frame.cmd_launch`, `profile == "claude-work"` |
| `test_bare_charter_on_a_terminal_runs_the_declared_default_profile` | local `default = "claude-work"`, `sys.stdout.isatty` → True → `cli._bare_launch([]) == (["claude-work"], None)`; then `_profile_launch` gives `["claude", "--profile", "claude-work"]` |
| `test_bare_charter_names_a_refused_default_and_exits_2` | local `default = "nope"` → rc 2, stderr contains `"nope"` and `"names no profile"` |
| `test_a_charter_toml_default_naming_a_local_profile_is_not_a_refusal` | `charter.toml` `default = "claude-work"`, profile declared locally → `instance.harness_of(cfg, profiles=…)["refused"] is None` and doctor's `charter.toml` row is `OK` |

**`tests/test_a_profile_launch_is_refused_before_tmux.py`** — class
`ALaunchNamesAProfile(PersonaIso, unittest.TestCase)`, using
`TheLaunchOpensWithoutMovingAnyone`'s patches
(`tests/test_a_chat_opens_in_the_background_with_its_first_message.py:437-461`).
`tmuxctl.run` is recorded; `mock.patch("charter.frame.launcher.shutil.which", return_value="/nowhere/claude")`;
git repo with the ignore line. `mock.patch("charter.frame.launcher._approval_refusal", return_value="")`
for every case that is not about approval; the three B1 cases run without it. Helper `_launch(**ns)` calls `commands_frame._launch(SimpleNamespace(harness="claude", rest=[], no_frame=False, workspace="beta", pick=False, size=(120, 40), **ns))`.
Refusals:

| Case | Asserts |
|---|---|
| `test_every_declared_profile_is_refused_until_approval_exists` | no stand-in; `profile="claude-work"` → rc 1, `"cannot yet ask before a declared command runs"` in stderr, no tmux call, no chat directory, `launcher.os.execvpe` not called. Red when the B1 refusal is deleted; Task 3 rewrites it (review B1) |
| `test_a_declared_replacement_of_a_built_in_is_refused_until_approval_exists` | no stand-in; `[harness.claude] command = ["/opt/claude"]`, `harness="claude"` → the same words, no tmux call |
| `test_a_built_in_the_file_does_not_replace_still_launches` | no stand-in; `harness="codex"` with `claude-work` declared → a `new-window` argv recorded |
| `test_an_unknown_profile_is_refused_and_nothing_is_allocated` | `profile="nope"` → rc 2, `"no profile named 'nope'"` in stderr, no `tmuxctl.run` call, `workspaces/beta` has no chat directory under `config.STATE_DIR / "frame"` |
| `test_a_refused_profile_says_its_own_reason` | a local profile with a string command → rc 2, `"never a shell string"` in stderr |
| `test_a_profile_of_another_kind_is_refused_by_its_name` | `harness="claude", profile="codex-pinned"` → `"is a codex profile"` and `"charter codex-pinned"` |
| `test_a_declared_profile_from_a_committable_file_is_refused` | remove the ignore line → rc 1, `"charter reinit adds"`, no tmux call |
| `test_a_built_in_is_not_refused_over_the_local_file` | same repo state, `profile=None, harness="claude"` → launches (a `new-window` argv was recorded) |
| `test_a_declared_replacement_of_a_built_in_in_a_committable_file_refuses_that_name` | `[harness.claude] command = ["/opt/claude"]`, no ignore line, `harness="claude"` → rc 1, `"charter reinit adds"` in stderr, no tmux call, and `launcher.os.execvpe` not called for `claude` or `/opt/claude` |
| `test_a_command_not_on_path_is_refused_by_profile_name` | `which` → `None` → rc 127, `"profile 'claude-work' runs claude, which is not on PATH"` |
| `test_each_refusal_says_something_different` | the stderr texts above are pairwise distinct |

The rest:

| Case | Asserts |
|---|---|
| `test_the_launch_hands_tmux_the_launcher_and_never_the_env` | `profile="claude-work"` → the recorded `new-window` argv ends with `[sys.executable, "-P", "-m", "charter", "frame-launch", "--profile", "claude-work", "--attended", "--"]`; `"~/.cw"` and the expanded `/…/.cw` appear in no recorded argv; every `-e` name is in the literal set `{"CHARTER_SESSION_ID", "CHARTER_HARNESS", "CHARTER_ROOT", "CHARTER_WORKSPACE", "CHARTER_PERSONA"}` (spelled here, not imported) |
| `test_carriable_is_unchanged` (pin: passes at `a5aa860`) | `layout.CARRIABLE == frozenset({"CHARTER_SESSION_ID", "CHARTER_HARNESS", "CHARTER_ROOT", "CHARTER_WORKSPACE", "CHARTER_PERSONA", "PATH"})` |
| `test_the_chat_records_its_profile_before_tmux_is_asked` | inside the `tmuxctl.run` fake for `new-window`: `state.profile("beta.1") == "claude-work"` |
| `test_the_harness_variable_stays_the_kind` | `state.identity("beta.1")["CHARTER_HARNESS"] == "claude-code"` |
| `test_a_reopen_launch_is_unattended` | `reopening=Reopening(chat)` → the argv holds no `"--attended"`; an ordinary launch's holds it |
| `test_a_background_open_launch_is_unattended` | `opening=Opening("fix it please")` → no `"--attended"` |
| `test_frame_launch_is_unattended_unless_told` | `build_parser().parse_args(["frame-launch", "--profile", "claude", "--"]).attended is False` (review 3) |
| `test_no_frame_execs_the_profile_with_its_env` | `no_frame=True`, `mock.patch("charter.frame.launcher.os.execvpe")` → called once with `("claude", ["claude"], env)`, `env["CLAUDE_CONFIG_DIR"] == os.path.expanduser("~/.cw")`, `env["CHARTER_HARNESS_PROFILE"] == "claude-work"`, `env["CHARTER_HARNESS"] == "claude-code"` |
| `test_no_frame_from_a_chat_does_not_carry_that_chats_session_id` | `os.environ` holds `CHARTER_SESSION_ID=alpha.1` and `CHARTER_HARNESS=codex` (`clear=True`); `harness="claude"`, `no_frame=True` → the exec env holds no `CHARTER_SESSION_ID`, and `CHARTER_HARNESS == "claude-code"` (review 8) |
| `test_a_persona_pin_typed_before_no_frame_still_reaches_the_harness` (pin: `bypass` carries it at `a5aa860`) | `os.environ` holds `CHARTER_PERSONA=forge`, `CHARTER_WORKSPACE=alpha`, `CHARTER_ROOT=<plane>` and `CHARTER_SESSION_ID=alpha.1` (`clear=True`); `harness="claude"`, `no_frame=True` → the exec env holds `CHARTER_PERSONA == "forge"`, `CHARTER_WORKSPACE == "alpha"` and `CHARTER_ROOT == <plane>` |
| `test_the_operator_window_still_starts_on_the_placeholder_and_respawns_the_launcher` | `tmuxctl.operator_server` → an `-S` socket that `is_operator_socket` accepts; `_launch_in_operator_tmux` with `tmuxctl.run` recorded → the `new-window` argv ends `["--", "cat"]`; a `set-option -p … remain-on-exit on` argv comes before the `respawn-pane` argv; the `respawn-pane` argv ends `["frame-launch", "--profile", "claude", "--attended", "--"]` (Ruling 4, review 10) |
| `test_an_early_death_names_the_profiles_command_not_the_launcher` | `_query_pane_dead_status` → 3, `_pane_last_words` → `["charter: profile …"]` → stderr contains `claude` and not `frame-launch` |
| `test_frame_escape_hatch_is_unchanged` (pin: passes at `a5aa860`) | `harness="frame", rest=["--", "true"]` → argv ends `["--", "true"]`, no `frame-launch` |

**`tests/test_the_launcher_becomes_the_profile.py`** — class `TheLauncher(PersonaIso, unittest.TestCase)`,
`os.execvpe` patched on `charter.frame.launcher.os`, and `launcher._approval_refusal` stood in
for every case but the B1 one:

| Case | Asserts |
|---|---|
| `test_the_pane_refuses_a_declared_profile_until_approval_exists` | no stand-in; `start(p, [], fid="beta.1", attended=True)` for `claude-work` → returns `REFUSED_EXIT`, execvpe not called (review B1) |
| `test_an_unframed_environment_drops_only_the_session_id` | `environment(p, {"CHARTER_SESSION_ID": "a.1", "CHARTER_ROOT": "/p", "CHARTER_PERSONA": "forge", "CHARTER_HARNESS": "codex"}, framed=False)` → no `CHARTER_SESSION_ID`; `CHARTER_ROOT == "/p"` and `CHARTER_PERSONA == "forge"`; `CHARTER_HARNESS == "claude-code"`. With `framed=True` the session id stays too (review 8) |
| `test_the_env_is_the_profiles_over_this_process` | `environment(p, {"PATH": "/x", "CLAUDE_CONFIG_DIR": "/old"}, framed=True)` → `PATH == "/x"`, `CLAUDE_CONFIG_DIR == expanduser("~/.cw")` |
| `test_a_tilde_is_expanded_in_the_first_word_and_every_value_only` | command `["~/bin/claude", "~/literal"]` → `expanded_command == [expanduser("~/bin/claude"), "~/literal"]` |
| `test_the_profile_and_kind_ride_the_exec_not_tmux` | `start(p, ["--resume", "s1"], fid="beta.1", attended=True)` → execvpe args `("claude", ["claude", "--resume", "s1"], env)` with both variables; `state.profile("beta.1") == "claude-work"` |
| `test_a_file_that_became_committable_since_tmux_is_refused_in_the_pane` | ignore line removed → `start` returns `REFUSED_EXIT`, execvpe not called, stderr contains `"charter reinit adds"` |
| `test_a_command_removed_since_tmux_is_refused_in_the_pane` | `which` → None → returns 127, not called |
| `test_the_pane_command_resolves_the_profile_by_name` | `cmd_frame_launch(SimpleNamespace(profile="claude-work", attended=True, rest=["--", "-p", "x"]))` with `CHARTER_SESSION_ID=beta.1` in a `clear=True` env and `tmuxctl.live_pane_by_pid` faked to answer `("%9", "beta", "beta.1")` for `os.getpid()` → execvpe argv `["claude", "-p", "x"]` |
| `test_a_process_that_is_not_the_panes_first_process_is_not_framed` | `alpha.1` recorded with server `commands_frame.SOCKET` and profile `codex`; env holds the chat's own `CHARTER_SESSION_ID=alpha.1`, `TMUX_PANE=%1` and `TMUX` (`clear=True`); `tmuxctl.run` answers `list-panes` with `53118\t0\t%1\talpha\talpha.1\talpha.1` (pid, dead, pane, session, window, `@charter_chat`); `os.getpid` → 4707 → `cmd_frame_launch(profile="claude", attended=False)` execs with no `CHARTER_SESSION_ID`, and `state.profile("alpha.1") == "codex"`. Red against the `$TMUX_PANE` version (ruling 29, revised) |
| `test_the_panes_own_first_process_is_framed` | same, `os.getpid` → 53118 → the env keeps `CHARTER_SESSION_ID=alpha.1`, and the record becomes `claude` |
| `test_a_forged_tmux_pane_naming_the_chats_pane_does_not_make_a_child_framed` | `TMUX_PANE=%1` and a `TMUX` naming another socket set by hand, `os.getpid` → 4707 → not framed; the recorded `list-panes` argv addresses `commands_frame.SOCKET` by name, never the socket in `$TMUX` |
| `test_a_pane_whose_window_is_not_named_for_the_chat_is_not_framed` | `os.getpid` → 53118, but the row's window is `alpha.2` → not framed, no record written |
| `test_a_renamed_window_on_the_operators_server_still_proves_the_chat_by_its_option` | `state.record_server("alpha.1", <an operator socket path>)`; the row is `53118\t0\t%1\top\tPWNED\talpha.1`; `os.getpid` → 53118 → framed (ruling 33) |
| `test_a_window_named_like_the_chat_on_the_operators_server_is_not_proof_without_the_option` | same server, the row is `53118\t0\t%1\top\talpha.1\t` → not framed |
| `test_a_dead_pane_whose_pid_matches_does_not_prove_the_chat` | the row is `53118\t1\t%1\talpha\talpha.1\talpha.1`, `os.getpid` → 53118 → not framed (ruling 33) |
| `test_a_claimed_chat_that_cannot_be_proven_says_so_in_one_line` | `os.getpid` → 4707 → stderr holds exactly one line, naming `alpha.1` and saying it runs with no frame (ruling 33) |
| `test_a_launch_that_claims_no_chat_says_nothing_about_one` | no `CHARTER_SESSION_ID` → no `UNPROVEN_CHAT` line |
| `test_the_frame_proof_runs_before_anything_is_printed` | `tmuxctl.live_pane_by_pid`, `util.err`, `util.info`, `sys.stdout.write` and `sys.stderr.write` recorded into one call log → `live_pane_by_pid` is its first entry (ruling 35) |
| `test_a_server_that_does_not_answer_is_no_frame` | `list-panes` rc 1 → not framed, nothing raised |
| `test_a_pane_asked_for_a_profile_that_is_gone_says_so` | `profile="gone"` → returns `REFUSED_EXIT`, `"no profile named 'gone'"` |
| `test_a_command_that_vanished_between_which_and_exec_is_127` | execvpe raises `FileNotFoundError` → 127 |
| `test_a_command_that_cannot_be_executed_is_126` | `PermissionError` → 126 |
| `test_an_exec_that_fails_after_on_exec_undoes_it` | `on_exec` returns a recording undo; `execvpe` raises `OSError(5, "Input/output error")` → the undo ran once, and the `Refusal` has kind `KIND_EXEC`, exit `REFUSED_EXIT`, and text naming the error (N7 nit) |

**`tests/test_a_chat_carries_its_profile.py`**

Class `ANewChatTakesThePressersProfile(PersonaIso, unittest.TestCase)` — `_a_chat`
(`tests/test_the_chat_bars_plus_makes_a_chat.py:46`) plus `state.record_profile`, and
`launcher._approval_refusal` stood in, because `background_refusal` now asks the launcher's
checks:
- `test_the_plus_launches_the_profile_this_chat_records`: `alpha.1` records `claude-work` →
  `cmd_launch` fake sees `harness == "claude"`, `profile == "claude-work"`.
- `test_a_chat_from_before_profiles_takes_the_built_in_of_its_kind`: no profile record, identity
  `codex` → `profile == "codex"`.
- `test_a_declared_replacement_of_the_built_in_is_what_that_chat_gets`: local `[harness.codex]`
  command `["/opt/codex"]` → `profile == "codex"`, and `profiles.current()["profiles"]["codex"].command == ("/opt/codex",)`.
- `test_a_chat_whose_profile_is_gone_is_refused_not_given_the_default`: record `gone`,
  `default = "claude"` → no launch, `state.notice("alpha.1")` contains `"no longer declares"`.
- `test_with_no_record_and_no_default_the_press_is_refused_by_name`: `NO_HARNESS` text in the
  notice, and it contains `"no profile"`.
- `test_a_workspace_tab_opens_the_same_profile`: `_open_workspace` fake → `profile == "claude-work"`.
- `test_a_handoff_takes_the_calling_chats_profile`: `open_in_background("beta", caller="alpha.1", first_message="fix the widget please")`
  with `cmd_launch` faked → `profile == "claude-work"`.
- `test_a_handoff_from_a_chat_whose_profile_is_gone_is_refused`: `background_refusal(...)`
  contains `"no longer declares"`.

Class `TheRecordCarriesTheProfile(PersonaIso, unittest.TestCase)`:
- `test_a_quit_records_each_chats_profile`: `_record_the_plane([...Doomed(..., profile="claude-work")], …)`,
  every field by keyword → `json.loads((config.STATE_DIR / "frame" / "reopen.json").read_text())`
  holds a chat with `"profile": "claude-work"`. The key and the file name are spelled literally:
  a round trip cannot pin a name.
- `test_a_manifest_written_before_profiles_reads_as_no_profile`: a `reopen.json` chat without
  the key → `reopen_state.read().all_chats()[0].profile == ""`.
- `test_a_profile_that_is_not_text_reads_as_no_profile`: `"profile": 3` → `""`.

Class `AReopenNeverSubstitutes(PersonaIso, unittest.TestCase)` — `cmd_launch` patched to record
args and set `reopening.fid`:
- `test_a_chat_comes_back_on_its_own_profile`: `Chat(..., harness="claude-code", profile="claude-work")`
  → launched with `profile == "claude-work"`.
- `test_a_chat_whose_profile_is_gone_is_skipped_with_its_name`: `profile="gone"` → returns
  `None`, no launch, the warning contains `"ran on profile 'gone'"` and `"charter reopen"`.
- `test_a_chat_whose_kind_is_unregistered_is_skipped_not_moved_to_the_default`:
  `harness="zzz", profile=""`, `default = "claude"` → returns `None`, no launch, and
  `"reopening it under"` appears nowhere in the captured output. This replaces
  `test_the_branch_not_taken_is_still_a_promise.py:597`.
- `test_a_skipped_chat_is_still_recorded_for_a_retry`: two chats, one gone → after `cmd_reopen`,
  `reopen_state.read().all_chats()` holds exactly the gone one.

Class `WhereTheHarnessWasShownTheProfileIs(PersonaIso, unittest.TestCase)`:
- `test_a_quit_row_names_the_profile`: `leave.title(Doomed(..., harness="claude-code", profile="claude-work"))
  == "alpha.1 · claude-work"`.
- `test_a_chat_row_from_before_profiles_still_names_its_harness`: `profile=""` → `"alpha.1 · claude-code"`.
- `test_the_chat_pickers_note_names_the_profile`: `choose._note(choose.CHAT, "alpha.1") == "claude-work"`.

Class `TheProfileVariableIsGuardedInTests(unittest.TestCase)` — in
`tests/test_no_test_reads_the_operators_shell.py::WhatIsGuarded`:
- `test_the_profile_variable_is_refused_on_read`: `"CHARTER_HARNESS_PROFILE" in _envguard._loud_names()`.
- `test_it_does_not_ride_the_frames_identity` (pin: passes at `a5aa860`): `"CHARTER_HARNESS_PROFILE" not in commands_frame._FRAME_IDENTITY`.

**`tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py`** — real tmux; `skipUnless(shutil.which("tmux"))`;
`setUp` skips below `tmuxctl.FLOOR`. One `_tmuxreap.name(...)` socket per case, killed and unlinked
in cleanup (the #713 race; memory: a shared socket fails 28/50 under load). `commands_frame.SOCKET`
is patched to it. `_spawn_gather` is stood in (a detached `frame-gather` is refused by
`_planeguard`). A recorder script is written in `self.tmp` (`#!{sys.executable}`, 0o755), and the
server env carries `CHARTER_ROOT=str(config.ROOT)`, `RECORD_DIR` and `_gitguard.environment()`.
The chat runs the built-in `claude`. A directory holding the recorder as `claude` goes first on
the `PATH` the tmux client is started with, ahead of `_claudeguard`'s fake. A declared profile
cannot launch in Task 2 (review B1), so the arrival of a declared `env` moves to Task 3's real-tmux
case, which seeds a launch record. The server env carries `CHARTER_ROOT`, so
`_planeguard._cmd_launches_charter` sees the tmp plane behind the `-m charter` argv.
- `test_the_pane_id_tmux_reported_is_the_harnesses_pane` (L1): after a real `_launch`
  (`attach=False`, `size=(120, 40)`, drawable slots forced empty),
  `state.harness_pane(fid)` equals the pane whose `#{pane_current_command}` settles on the
  recorder's interpreter name, and `display-message -p -t <pane> '#{pane_pid}'` equals the pid
  the recorder wrote.
- `test_a_harness_exit_code_travels_as_it_did` (L2): recorder exits 7 →
  `state.exit_code(fid) == 7` after the hook fires; the window is gone.
- `test_a_refusal_in_the_pane_is_what_the_operator_reads` (L4): the recorder removed from disk
  after the pre-tmux check (patch `launcher.refusal` to `""` for the pre-tmux call only) →
  `_launch` returns 127 and stderr contains `"not on PATH"`, read off the pane.
- `test_the_exec_env_reaches_the_harness_and_not_tmux` (L6): the recorder's env holds
  `CHARTER_HARNESS_PROFILE=claude`, and neither `show-environment -g` nor
  `show-environment -t <session>` holds that name.
- `test_only_the_panes_first_process_is_framed_on_a_real_server` (L8, ruling 29 revised): the
  window's first command is a Python stand-in that writes `launcher.framed_chat()` to a file,
  then spawns a child that writes its own answer with the same inherited environment. The
  stand-in answers the chat id and the child answers `None`. This runs on both servers,
  including the operator's `respawn-pane`.
- `test_the_harness_is_still_a_chat_to_every_status_reader` (L5): from the recorder's env,
  `TMUX_PANE == state.harness_pane(fid)` and `fid in commands_frame._live_chats(socket)`.
- `test_the_same_holds_in_a_tmux_you_already_had`: the operator arm. An `-S` server; `$TMUX`
  patched to name it; `tmuxctl.operator_server` real; L2 and L6 on
  `_launch_in_operator_tmux`'s path.

The measurement's L3 is not a test: nothing in `charter/` reads that format. Its readings are in
`docs/frame.md`. On CI this module runs on the image's tmux (3.4); by hand on 3.7c and on the
3.2 floor via `PATH`. The PR says which ran where.

**Updated existing tests:**
- `tests/test_bare_charter_opens_the_frame.py`:
  - `TheSectionReachesConfig` and `TheBareCommandLaunchesTheDeclaredHarness` read
    `config.PROFILES`;
  - `DoctorNamesASilentlyIgnoredDefault` keeps its row;
  - `TheSectionIsReadAtTheConfigBoundary`'s refusal cases stay on `instance.harness_of`.
- `tests/test_the_chat_bars_plus_makes_a_chat.py::ThePressRunsTheLauncherForThisWorkspace::test_the_launch_uses_the_harness_this_chat_records`
  asserts `profile` too.
- `tests/test_a_workspace_tab_opens_what_it_names.py::TheOpenUsesTheHarnessTheOperatorIsAlreadyIn`
  likewise.
- `tests/test_a_real_click_on_a_real_tab_bar_switches.py:1085`: `_NO_HARNESS = "records no profile this charter can launch"`.
- `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py:326`: the fallback line is
  gone; the case asserts `REOPEN_GONE`'s words instead.
- `tests/test_a_background_chat_really_starts_on_its_brief.py` (`_stand_ins`, `:179`) patches
  `ClaudeCodeHarness.binary` in-process. The pane now runs `frame-launch` in a child process that
  patch cannot reach, and a declared profile is refused in this task. So it puts its recorder
  first on the client's `PATH` as `claude` and hands the server `CHARTER_ROOT`; otherwise the pane
  runs `_claudeguard`'s fake `claude`.
- `tests/test_a_harness_argument_ending_in_a_semicolon_arrives_whole.py::ARealTmuxHandsTheHarnessEveryByte`
  is unaffected: it runs `layout.*_argv` through its own tmux binary, never through `_launch`,
  and never patches `binary` (review 11).

### Implementation steps

- [ ] Step 0 (the measurement). Record the table. Stop here on any failed criterion.
- [ ] Write the five new modules and the updated literals; run them — expect failures.
- [ ] `state.record_profile`/`profile`, `reopen.Chat.profile`, `leave.Doomed.profile`, `chats.profile_of`.
- [ ] `launcher.py` with `_approval_refusal` and `DECLARED_NOT_YET`, the `frame-launch` parser
      (`--attended`), `_profile_launch`, `--profile`, `_bare_launch` on `config.PROFILES`.
- [ ] `_launch` on both paths; `_same_profile_as` at its four callers; `_reopen_one` without the fallback.
- [ ] `_envguard._loud_names`, the `KNOWN` entry.
- [ ] `grep -rn "records no harness\|reopening it under\|this plane's default" tests/` — every hit
      reworded or deleted, in both directions.
- [ ] `python3 -m unittest discover -s tests` (the whole suite: the exec-family scan is static).
- [ ] Hand deletion check of the `_loud_names` line (red: `WhatIsGuarded`).
- [ ] `python3 tools/sweep.py`; report it.
- [ ] Docs and news; PR.

### Docs and news

- **`docs/frame.md`.**
  - Top (`:17-24`): `charter <profile>` beside `charter claude`.
  - A new `### How a harness starts` after `## When the command dies before the frame is drawn`
    (`:1315`):
    - the launcher and its `exec`;
    - that the pane id, `remain-on-exit`, the exit code and the early-death report are
      unchanged — the measured readings L1–L7, both versions, both servers;
    - that `#{pane_current_command}` names charter's interpreter until the harness starts;
    - that only the profile's name crosses tmux.
    - that `charter frame-launch` counts itself framed only when its pid is the `#{pane_pid}`
      of the window named for its chat on that chat's server, asked before `exec`. It is a
      guard rail against a model running it from its tool shell, not a boundary: a process
      that deliberately starts its own tmux pane in a window named like a chat passes.
  - `+` (`:808-813`, `:837-841`) and tabs (`:2803-2806`): "the profile this chat is running".
  - `## Leaving: detach, close, quit — and reopen` (`:1021`): a chat comes back on its own
    profile, and one whose profile is gone is skipped with its name, never given another.
- **`docs/control-plane.md`.** `default` names a profile. Bare `charter` runs a built-in default
  on a terminal, as it did the kind; a declared default is refused with the launcher's not-yet
  sentence. "Launching a declared profile arrives in a later release" stays until Task 3.
- **`README.md:89-116` and `docs/install.md:296-305`:** the `[harness] default` snippets say "a
  profile" and point at `charter.local.toml`.
- **News**, extending `docs/news/unreleased-harness-profiles.md`:
  - the headline stays Task 1's: a declared profile does not launch yet;
  - body: every chat now starts through charter's launcher, which records the built-in profile
    it runs; `+`, tabs, reopen and a handoff keep that profile; a reopened chat whose profile is
    gone is skipped by name rather than moved to another account; a declared profile is refused
    with a sentence saying approval is not built yet, so no command a chat could have written
    into `charter.local.toml` runs.
  - `adopt: reinit` stays.

**Suggested PR title:** A chat starts on the profile you name, and keeps it through `+`, tabs, handoff and reopen

---
## Task 3: A new or changed command asks once

**Depends on:** Task 2 (`launcher.refusal`'s order, `_approval_refusal`, `--attended`, `_same_profile_as`,
`_reopen_one`, `background_refusal`).

### Files

- Create `charter/profiletrust.py` and `tests/test_a_new_or_changed_profile_asks_once.py`.
- Task 2's tests, rewritten or seeded (review B1, review 9):
  - `tests/test_a_profile_launch_is_refused_before_tmux.py::test_every_declared_profile_is_refused_until_approval_exists`
    becomes `test_a_declared_profile_with_no_record_asks_before_it_runs`, and the replacement case
    becomes `test_a_declared_replacement_of_a_built_in_asks_before_it_runs`.
  - Task 2's `_approval_refusal` stand-ins in that module and in
    `tests/test_the_launcher_becomes_the_profile.py` and `tests/test_a_chat_carries_its_profile.py`
    become `profiletrust.record_launched(p)` in `setUp`, so the mechanics run through the real
    approval.
  - `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py` gains
    `test_a_declared_profiles_env_reaches_the_harness_and_not_tmux`: a declared recorder profile
    with `env = { CLAUDE_CONFIG_DIR = "<tmp>/alt" }` and a seeded record. The value reaches the
    recorder, and neither `show-environment` holds it.
  - Correction (re-review N6): `_launch` passes `--attended`, and a real pane has a terminal on
    both ends. So without the record that pane asks `run this? [y/N]` and waits until the
    suite's 600 s kill. Every real-tmux test that launches a declared profile seeds its record
    through one helper, `tests._isolation.approve_profile(case, name)`, and launches through
    `tests._isolation.assert_approved(name)` first. That fails before tmux starts when the
    record for the profile it launches is missing.
- `tests/_isolation.py` — `approve_profile(case, name) -> profiles.Profile` (resolves the name,
  calls `profiletrust.record_launched`) and `assert_approved(name)` (raises
  `AssertionError("no launch record for '<name>' — its pane would wait at run this? [y/N]")`
  unless `profiletrust.approval_needed` is `""`).
- `charter/frame/launcher.py`:
  - `_approval_refusal`'s body becomes `profiletrust.refusal(p, attended=attended)`, and
    `DECLARED_NOT_YET` is deleted (review B1).
  - `attempt` calls `profiletrust.ask_in_terminal` when the refusal's `kind` is `KIND_ASK` —
    compared by kind, never by text — and `profiletrust.can_ask(sys.stdin, sys.stdout)`
    (re-review N2).
  - After a yes it runs the whole chain again from the top before any `exec`: the ignore check,
    `PATH`, approval (now matching) and, from Task 4, wiring. This holds on every path,
    `--no-frame` and the pane included.
  - A yes whose record fails to write returns `Refusal(KIND_RECORD, RECORD_NOT_WRITTEN…,
    REFUSED_EXIT)` at once and never asks again. Re-running the chain would only find no record
    and ask a second time (N2b nit).
  - With a terminal missing on either side it returns that refusal: its text is printed and
    `REFUSED_EXIT` returned (review 3).
- `charter/commands_frame.py`:
  - `_launch`'s pre-tmux step 5: ask when `attended` and `profiletrust.can_ask(sys.stdin, sys.stdout)`. Defer to the pane
    only a `KIND_ASK` refusal, and only when attended with no terminal (`+`, a tab). Every other
    kind refuses before tmux, and `KIND_ASK` refuses when unattended (N2a nit).
  - `_reopen_one` checks before `cmd_launch` and skips with the reason.
  - `background_refusal` gains the check after `_same_profile_as`.
- Docs: `docs/control-plane.md` (`## [harness]` gains *A new or changed command asks once*),
  `docs/frame.md` (the refusals a `+`/reopen/handoff can meet).

### Interfaces

**Consumes:**
- From Task 1: `profiles.Profile`, `BUILTIN`, `display`.
- From Task 2: `launcher.refusal`/`start`, `attended`, `REFUSED_EXIT`.
- On `main`: `config.STATE_DIR`, `config.replace_for`, `config.private_mkdir`.

**Produces:**

```python
# charter/profiletrust.py
RECORD = "harness-profiles-launched.json"      # under config.STATE_DIR
DECLINED_EXIT = 130                             # the workspace picker's cancel code, `_PICKER_CANCELLED`

def fingerprint(p: profiles.Profile) -> dict: ...
    # {"kind": p.kind, "command": list(p.command), "env": dict(p.env)} — as DECLARED, before
    # `~` expansion: the file is what an edit changes, and HOME is not something a chat moves
def last_launched(name: str) -> dict | None: ...
def record_launched(p: profiles.Profile) -> str: ...     # "" once written; the OSError's text otherwise. Never raises
RECORD_NOT_WRITTEN = ("charter: you approved profile '{name}', but charter could not record that "
    "at {path} ({why}), so it will not start it — it would only ask you again. Nothing was "
    "started; fix that path and run it again.")
def approval_needed(p: profiles.Profile) -> str: ...     # "" | "new" | "changed"; built-ins ""
def refusal(p: profiles.Profile, *, attended: bool) -> "launcher.Refusal | None": ...
    # None when approved; Refusal(KIND_ASK, NEEDS_ASKING formatted, REFUSED_EXIT) when
    # attended — the caller asks if can_ask, and prints the text otherwise;
    # Refusal(KIND_UNATTENDED, UNATTENDED formatted, REFUSED_EXIT) when not. Callers branch
    # on kind, never on text (re-review N2)
def ask_in_terminal(p: profiles.Profile, *, stdin, stdout) -> bool: ...
    # prints the command and env, reads one line, True only for "y"/"yes"; records on True
NEEDS_ASKING = ("charter: profile '{name}' is {state} since it last ran, and charter asks "
    "before such a command runs — but there is no terminal here to ask in. Nothing was "
    "started. Run it where you can answer: charter {name}")
def can_ask(stdin, stdout) -> bool: ...     # both isatty(); a terminal on one side only is not one
```

### Behaviour

**The record.**
- One JSON object keyed by profile name, each value `fingerprint(p)`.
- It lives under `.charter/`, which `charter init` ignores. `config.private_mkdir` +
  `config.replace_for`.
- Unreadable or malformed reads as `{}`, so everything declared asks again. That fails toward
  asking, never toward running.

**`approval_needed(p)`.**
- `p.source == BUILTIN` → `""`. The spec says built-ins never ask. A declared profile named
  `claude` is not a built-in, so it asks.
- No record → `"new"`. A record `!=` fingerprint → `"changed"`. Else `""`.

**The ask** (`ask_in_terminal`), in the terminal the operator is at:

```
charter: profile 'claude-work' is new — it has not run on this machine before.
  command  claude
  env      CLAUDE_CONFIG_DIR=~/.claude-work
run this? [y/N]
```

- For `"changed"`, the first line reads `has changed since it last ran`, and a line
  `  was      <the recorded command and env>` follows.
- The command and env lines are `profiles.display(p)`, contained. A `\r`, ESC or other control
  byte is shown escaped, never interpreted, so a file a chat can write cannot redraw this
  prompt to show another command (ruling 35).
- Anything but `y`/`yes` → `util.info("charter: nothing started.")` and `DECLINED_EXIT`.
- End of input and `KeyboardInterrupt` both decline.

**Where it runs.**

| Open | Before tmux | In the pane |
|---|---|---|
| `charter <profile>` on a terminal | asks; a yes records; a no returns 130 with nothing allocated | the record matches, so it runs |
| `charter <profile> --no-frame` | `launcher.start` asks on its own terminal | — |
| an attended launch with no terminal on stdin or on stdout (`charter claude-work --no-frame > log`) | refuses with `NEEDS_ASKING` (review 3) | — |
| `+`, a workspace tab, the palette's new chat (no terminal at the press) | does not ask and does not refuse: the launch passes `--attended` | asks in the pane, whose stdin and stdout are its tty; a no exits 130 and the window closes as any exit does |
| reopen, a recorded plane's restore | `_reopen_one` skips: `REOPEN_UNAPPROVED` | unattended (no `--attended`), so it refuses if it ever gets there |
| a handoff | `background_refusal` refuses: `UNATTENDED.format(...)` | unattended, so it refuses |

```python
UNATTENDED = ("profile '{name}' is {state} since it last ran, and nobody is at this open to "
              "approve it — nothing was started. Run it once yourself so charter can ask: "
              "charter {name}")
REOPEN_UNAPPROVED = ("charter reopen: {chat} runs profile '{name}', which is {state} since it "
                     "last ran — not reopened, because a reopen has nobody to ask. Run "
                     "charter {name} once to approve it, then charter reopen.")
```

**Why it asks.** Put this in the module docstring:
- Once the file is ignored, an edit leaves no diff.
- Nothing stops a chat editing plane config.
- The command goes to tmux, not through a harness permission prompt.
- Codex trusts hooks by hash for the same reason.

**The limit, stated at full volume in the docstring and the docs.** A chat that can edit
`charter.local.toml` can also edit `.charter/harness-profiles-launched.json` (Ruling 13).
The ask catches a command the operator did not change themselves, unless whatever changed it also
forged the record.

### Test cases — `tests/test_a_new_or_changed_profile_asks_once.py`

Class `TheRecord(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_built_in_never_needs_approval` | `approval_needed(profiles.builtins()["claude"]) == ""` with no record |
| `test_a_declared_profile_with_no_record_is_new` | `== "new"` |
| `test_a_recorded_profile_is_approved` | after `record_launched(p)` → `""` |
| `test_a_changed_command_is_changed` | record, then re-declare `command = ["claude", "--x"]` → `"changed"` |
| `test_a_changed_env_value_is_changed` | same for `env` |
| `test_a_changed_kind_is_changed` | same for `kind` |
| `test_a_declared_replacement_of_a_built_in_asks` | `[harness.claude]` → `"new"` |
| `test_the_record_is_private_state_under_charter` | `(config.STATE_DIR / "harness-profiles-launched.json").stat().st_mode & 0o077 == 0`, and its JSON holds `{"claude-work": {"kind": "claude", "command": ["claude"], "env": {"CLAUDE_CONFIG_DIR": "~/.claude-work"}}}` — spelled literally |
| `test_an_unreadable_record_asks_again` | file holds `"{nope"` → `"new"` |

Class `TheAsk(PersonaIso, unittest.TestCase)`:
- `test_it_shows_the_command_and_the_env_before_asking`: `io.StringIO("y\n")` in → out contains
  `"command  claude"`, `"CLAUDE_CONFIG_DIR=~/.claude-work"`, and `"run this? [y/N]"` last.
- `test_yes_records_and_runs`: `True`; `approval_needed == ""`.
- `test_anything_else_declines_and_records_nothing`: subTest over `"\n"`, `"n\n"`, `"yess\n"`, `""`
  (EOF) → `False`; still `"new"`.
- `test_control_bytes_in_a_command_are_shown_escaped_in_the_prompt`:
  `command = ["rm\r\x1b[2Kclaude"]` → the prompt's output holds no `\r` and no ESC byte, and
  holds the escaped command (ruling 35).
- `test_a_changed_profile_shows_what_it_was`: out contains `"was "` and the old command.

Class `WhereItAsks(PersonaIso, unittest.TestCase)` — Task 2's `ALaunchNamesAProfile` patches,
`sys.stdin` replaced (the `_ttyguard` rule):

| Case | Asserts |
|---|---|
| `test_a_terminal_launch_asks_before_anything_is_allocated` | `isatty` True, stdin `"n\n"` → rc 130, no `tmuxctl.run`, no chat directory |
| `test_a_yes_at_the_terminal_launches_and_is_not_asked_again_in_the_pane` | stdin `"y\n"` → `new-window` recorded; `launcher.start` in-process with stdin that raises on read → exec called |
| `test_an_attended_launch_with_no_terminal_refuses_with_needs_asking` | `launcher.start(p, [], fid=None, attended=True)` with stdin a tty and stdout a pipe → `REFUSED_EXIT`, stderr contains `"no terminal here to ask in"`, stdin never read (review 3) |
| `test_an_attended_launch_with_a_terminal_on_both_asks` | stdin and stdout ttys, stdin `"y\n"` → exec called |
| `test_only_asking_is_deferred_to_the_pane` | attended, no terminal, and a `KIND_PATH` refusal (`which` → None) → refused before tmux, no `new-window` argv (N2a nit) |
| `test_a_record_that_cannot_be_written_refuses_and_never_asks_again` | `config.replace_for` raises `OSError(28, "No space left on device")`; stdin answers `"y\n"` once and raises on a second read → `REFUSED_EXIT`, stderr names the record's path and `No space left`, stdin read exactly once (N2b nit) |
| `test_a_press_with_no_terminal_defers_to_the_pane` | `attach=False`, `isatty` False → launches; no read of stdin |
| `test_the_pane_asks_and_a_no_exits_130` | `launcher.start(p, [], fid="beta.1", attended=True)` with a tty stdin answering `"n\n"` → 130, execvpe not called |
| `test_an_unattended_pane_refuses_rather_than_asks` | `attended=False` → `REFUSED_EXIT`, stderr contains `"nobody is at this open"` |
| `test_a_reopen_skips_an_unapproved_profile` | `_reopen_one(Chat(profile="claude-work"))` → `None`, warning contains `"a reopen has nobody to ask"` |
| `test_a_handoff_refuses_an_unapproved_profile` | caller records `claude-work` → `background_refusal(...)` contains `"charter claude-work"` |
| `test_a_built_in_never_asks_anywhere` | `harness="claude"`, stdin raising on read → launches |
| `test_each_refusal_says_something_different` | pairwise distinct |

### Implementation steps

- [ ] Tests first; run — red.
- [ ] `profiletrust.py`; the three call sites; the launcher insert.
- [ ] Whole suite; `python3 tools/sweep.py`; report it.
- [ ] Docs, news; PR.

### Docs and news

- **`docs/control-plane.md`** `### A new or changed command asks once`:
  - the prompt as printed;
  - where it asks and where it refuses (the table above, in prose);
  - that built-ins never ask;
  - the limit about `.charter/`.
- **`docs/frame.md`:** the `+`/tab paragraph notes a new profile asks in the chat's own pane.
  `## Leaving … reopen` notes a changed profile is skipped by a reopen until run once.
- **News:** the headline becomes `A chat starts on the harness profile you name — its own command and config folder, never through tmux`.
  The body gains: "`charter claude-work` runs that profile's command with its env. A profile
  whose command or environment is new or has changed since it last ran shows it and asks
  `run this? [y/N]` once; a reopen, a handoff, or a launch with no terminal to ask in refuses it
  instead."

**Suggested PR title:** A new or changed harness profile shows its command and asks before it runs

---
## Task 4: A profile is wired or refuses — detection, the fix, `charter harness install <profile>`, `init`/`reinit`, `doctor` per profile

**Depends on:**
- **PR #970 on `main`** (Task 0; branch `fix-969-doctor-config-folder`, commit `677faac`). This
  task does not start before it merges, and re-reads the merged diff first.
- Task 3 on `main`: detection and install run the profile's own command, which only an approved
  profile may do (Global Constraints; Ruling 1).

### Step 0 — Measure first. A failed D3 stops this task.

Use throwaway folders under `/tmp/cp-wiring/`: an alternate `CLAUDE_CONFIG_DIR`, an empty
`CODEX_HOME`, an alternate `XDG_CONFIG_HOME`. There is no login and no model tokens. Do not run
anything against `~/.claude`, `~/.codex` or `~/.config/opencode` except the default-config
timings.

**D1 — cost.**
- Five runs each of `claude plugin list --json` and `opencode debug config`, under an empty
  alternate folder and under the default one. Codex's detection reads a file, so D1 times the
  parse only.
- The plan author's readings (*Measured while writing this plan*) are the baseline: Claude Code
  215 / 281 ms, opencode 749 / 720 ms.
- Then time a `charter doctor` run by hand on this plane twice: before this task, and after it
  with three declared, approved profiles (`claude-alt`, `codex-alt`, `opencode-alt`). Time
  `charter doctor --preflight` too, which must not grow: it never probes.
- **Decision rule (Ruling 11)**, on the hand-run figure:
  - ≤ 2 s added → rows for every listed profile, probed concurrently;
  - otherwise rows for declared profiles only, and the PR says so.

**D2 — what flips each answer, and which scope wins.** The first half is for the cache stamp;
the second decides Claude Code's rule (re-review N3).
- **Scope precedence**, recorded from `claude plugin list --json` under the alternate folder, for
  a throwaway plane:
  - (a) a disable written only to `.claude/settings.local.json`
    (`"enabledPlugins": {"charter@charter": false}`), and, separately, one written only to the
    project `.claude/settings.json`, each beside an enabled user-scope install. Record whether
    `claude plugin list --json` reports a local or project entry at all, and whether any entry's
    `enabled` follows the settings file (ruling 36). Covering entries are install records —
    they carry `installedAt` — and a settings-file disable creates none;
  - (b) a project-scope enable beside a user-scope disable.
  - For each, record every entry's `scope`, `projectPath` and `enabled`.
- The plan author's reading of the default folder (*Measured while writing this plan*) found the
  pairs `(local, False)`, `(project, False)`, `(project, True)` and `(user, True)` side by side.
  That shows they coexist, not which one wins.
- If D2 cannot settle the order, the most-specific rule below stands, because it fails closed.
- Claude Code under the alternate folder, in turn:
  - install the plugin at project scope for a throwaway plane;
  - disable it (`claude plugin disable`);
  - enable it;
  - uninstall it.
- After each step, record which files' `(mtime_ns, size)` changed among:
  - `<config>/plugins/installed_plugins.json`
  - `<config>/plugins/known_marketplaces.json`
  - `<config>/settings.json`
  - `<config>/.claude.json`
  - the plane's `.claude/settings.json` and `.claude/settings.local.json`
- opencode: remove and restore `<xdg>/opencode/plugin/charter.ts`, then edit
  `<xdg>/opencode/opencode.json`.
- The stamp is the set of files whose change accompanied every flip, minus `.claude.json`: the
  probe itself writes that.

**D3 — install under a profile's environment reaches a workspace chat.**
- Under `CLAUDE_CONFIG_DIR=/tmp/cp-wiring/claude-alt`, from a throwaway plane root, run
  `claude plugin marketplace add diazoxide/charter`, then
  `claude plugin install charter@charter --scope project -y`.
- Then `claude plugin list --json` with cwd `<plane>/workspaces/w/` (after
  `workspace.ensure("w")` wrote the mirrored `.claude/settings.json`).
- **Pass:** an entry `charter@charter`, `enabled: true`, whose `plugincache.covers(entry, <cwd>)`
  holds, or a documented other rule that answers true for that cwd.
- **Fail — stop, record, report:** the install shape per profile changes, and so does what
  `charter init` has always done for the default folder.

**D4 — Codex under a profile's `CODEX_HOME`.** With `CODEX_HOME=/tmp/cp-wiring/codex-alt`,
record:
- `codex plugin --help` and which subcommand installs `charter@charter`;
- what it writes to `config.toml` (`[plugins."charter@charter"] enabled`) and where the plugin
  cache lands;
- after one approval of charter's hooks in a session, the `[hooks.state."charter@charter:hooks/hooks.json:…"]`
  keys and whether a `trusted_hash` can be recomputed by charter from the plugin's
  `hooks/hooks.json`.

If it cannot be recomputed, detection accepts "a trust entry exists for every hook key in the
installed plugin's `hooks.json`", and the docs state that limit.

**D5 — opencode.** With `XDG_CONFIG_HOME=/tmp/cp-wiring/oc-alt`:
- `OpenCodeHarness().wire(root)` with that environment applied in-process;
- then `opencode debug config` under it lists `plugin/charter.ts`;
- also whether `~/.opencode/plugin/` loads for any `XDG_CONFIG_HOME` (the spec's Limits; record,
  do not build on it).

Readings go into `docs/harnesses.md` and the PR.

### Files

- Create `charter/wiring.py` and `tests/test_a_profile_is_wired_or_refuses.py`.
- `charter/harness/claude_code.py` (as #970 leaves it): `config_home(env: Mapping[str, str] | None = None)`
  and `global_config_file(env=None)` read `env if env is not None else os.environ`. This is
  #970's resolver with a parameter, never a second resolver, and every existing caller is
  unchanged.
- `charter/cli.py:120` — `doctor`'s parser gains `--preflight`. `commands.cmd_doctor` passes it
  to `doctor.run_all(preflight=…)` and `doctor.check_names(preflight=…)`.
- `hooks/hooks.json` — the fourth SessionStart command becomes
  `out="$(charter doctor --preflight 2>&1)" || printf …`. Today it is the bare `charter doctor`
  a person types, so nothing in the process can tell the two apart, and a tty test would
  misread `charter doctor --json`. Codex trusts that hook by hash, so its users approve it once
  more; the news entry says so.
- `tests/_isolation.py` — `wired_as_today(case)` patches `charter.wiring.refusal` to `""` for a
  case whose subject is not wiring. In-process, `_claudeguard` makes `plugincache.available()`
  answer False, so detection reads `UNKNOWN_STATE` and refuses with `CANNOT_TELL`; in a child
  process its fake `claude` answers `[]`, which reads as unwired (review 11). Either way every
  launch test would refuse (Ruling 10). A real-tmux test cannot use the patch, because a pane is
  a child process, so its recorder answers `plugin list --json` with a covering, enabled entry.
- Existing tests Task 4's probes reach, and how each answers (review 9):
  - `tests/test_a_second_launch_focuses_instead_of_dragging.py` (`_launch` at `:343`, `_press`
    at `:672`, `:729`) and `tests/test_a_workspace_tab_opens_what_it_names.py`'s in-process
    launches (`:517`, `:532`) → `wired_as_today`.
  - `tests/test_frame_launcher.py::Launch` and `::LaunchInsideTmux`,
    `tests/test_charter_records_the_plane_as_it_changes.py::TheLauncherTakesTheDecision`, and
    `tests/test_a_chat_opens_in_the_background_with_its_first_message.py::TheLaunchOpensWithoutMovingAnyone`
    → `wired_as_today` (re-review N4).
  - `tests/test_a_background_chat_really_starts_on_its_brief.py`, whose recorder Task 2 put
    first on `PATH` as `claude` → the recorder answers `plugin list --json` with a covering,
    enabled entry.
  - `tests/test_a_workspace_tab_opens_what_it_names.py::ARealTabOpensARealWorkspace`, whose
    harness is a sleeping script on `PATH`, and
    `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py`'s recorder → answer
    `plugin list --json` with a wired entry and exit, before the script would sleep. Unchanged,
    the sleeping script would run into `LIST_TIMEOUT` and refuse.
  - Task 3's seeded records stay: an unapproved profile is never probed.
- `charter/plugincache.py`:
  - `_claude_json(args, cwd=None, timeout=LIST_TIMEOUT, *, env=None, command=("claude",))` (`:141`);
  - `_our_entries(*, env=None, command=…)`;
  - `installed_for(project, *, env=None, command=…)` (`:248`), unchanged in what it returns;
  - `covering_entries(project, *, env=None, command=…) -> list[dict] | UNKNOWN`, beside it;
  - `install_argvs(scope, source, *, command=("claude",))` (`:265`);
  - `_step(argv, cwd, *, env=None)` (`:293`);
  - `install(project, scope, *, env=None, command=…)` (`:304`);
  - `available(command=("claude",))` answers `shutil.which(command[0])`.
  - `tests/_claudeguard.py:69` becomes `plugincache.available = lambda *a, **k: False`. The
    zero-argument lambda would raise `TypeError` at the first `available(command=…)` (review 9).
  - Every default is today's value, so every existing caller is unchanged.
- `charter/harness/codex.py:55` — `config_path(env: Mapping[str, str] | None = None)` reads
  `(env or os.environ).get("CODEX_HOME")`. `install(env=None)` passes it through.
- `charter/harness/opencode.py:175` — `global_dir(env=None)` likewise. `OpenCodeHarness.wire(root, *, env=None)`
  passes it to `global_dir`, `refresh_shim`, `write_context`, `ensure_instructions` and
  `unvouched`.
- `charter/frame/launcher.py` — `refusal` appends `wiring.refusal(p, cwd=…)` last.
- `charter/commands_harness.py:34` — `cmd_harness_install` resolves a profile first, and a
  registry NAME second (Ruling 8).
- `charter/commands.py`:
  - `cmd_init` (`:2274`) and `cmd_reinit` (`:2494`): `_wire_profiles(root, *, install: bool)`
    after `_wire_harnesses`.
- `charter/doctor.py`:
  - `check_profile_wiring(*, preflight: bool = False) -> list[Result]` after
    `check_harness_profiles` in `_checks`. With `preflight` it returns no rows and calls
    nothing.
  - `check_names(preflight=False)` splices `f"profile {name}"` for each listed profile after
    `"harness profiles"` — the forge pair's shape (`check_names`, `_FORGE_NAMES_AT`) — and none
    with `preflight`. The name list and `run_all` cannot disagree.
- `charter/dispatch.py:332` (`_transcript_dir`) and `charter/persona.py:1448` (`_skill_roots`):
  docstrings say they answer for the default config folder (Ruling 16).
- Docs: `docs/harnesses.md` (`## Wiring, and when it happens` `:59`, the table `:33`),
  `docs/install.md` (the doctor rows), `docs/control-plane.md` (`### The profile is wired`).

### Interfaces

**Consumes:**
- From Task 1: `profiles.current`, `expanded_command`, `expanded_env`, `BUILTIN`.
- From Task 2: `launcher.environment`, `launcher.refusal`.
- From Task 3: `profiletrust.approval_needed`, `profiletrust.ask_in_terminal`.
- From #970 on `main`: `claude_code.config_home()` and `claude_code.global_config_file()`, which
  this task gives an `env` parameter. `guardseen.last_claude_config_dir()` and `doctor`'s
  `plane-root guard` row are #970's and stay as they are.
- On `main`:
  - `plugincache.covers(entry, project)` (`:238`), `UNKNOWN`, `PLUGIN_ID`, `MARKETPLACE_SOURCE`
  - `codex.install()` statuses (`:94-130`)
  - `OpenCodeHarness.wire(root)` (`opencode.py:885`), `SHIM_PATH` (`:202`)
  - `doctor.Result`, `_NOT_CHECKED_HINT` (`:90`)
  - `util.run(…, env=…)` (`util.py:144`), which overlays `os.environ`

**Produces:**

```python
# charter/wiring.py
WIRED, UNWIRED, UNKNOWN_STATE = "wired", "unwired", "unknown"
CACHE = "cache/harness-wiring.json"               # under config.STATE_DIR

class Wiring(NamedTuple):
    state: str        # WIRED | UNWIRED | UNKNOWN_STATE
    detail: str       # what was asked and what it answered: "claude plugin list: charter@charter enabled for <dir>"
    fix: str          # "" when wired; else the command to run

def detect(p: profiles.Profile, *, cwd: Path) -> Wiring: ...          # always asks; never cached
def cached(p: profiles.Profile, *, cwd: Path) -> Wiring | None: ...   # a remembered answer whose stamp still matches
def remember(p: profiles.Profile, *, cwd: Path, w: Wiring) -> None: ...
def refusal(p: profiles.Profile, *, cwd: Path) -> str: ...
    # "" when detect(...) is WIRED; NOT_WIRED / CANNOT_TELL otherwise. Always fresh.
def install(p: profiles.Profile, root: Path) -> list[tuple[str, str]]: ...
    # the kind's wiring under p's environment; (status, label) pairs like Harness.wire

# charter/doctor.py
def check_profile_wiring(*, preflight: bool = False) -> list[Result]: ...  # one per listed profile; none in the preflight
def run_all(*, preflight: bool = False) -> list[Result]: ...           # existing; gains the flag
def check_names(*, preflight: bool = False) -> list[str]: ...          # existing; gains the flag
```

### Behaviour

**Detection, per kind.** It runs only for a profile that passed the ignored check and
`profiletrust.approval_needed(p) == ""`. Otherwise `detect` is never called, and the caller says
the earlier reason. It applies to built-ins exactly as to declared profiles (Ruling 10).

- **Claude Code.**
  - `plugincache.covering_entries(cwd, env=launcher.environment(p, os.environ, framed=True), command=profiles.expanded_command(p))`
    with `cwd` = the chat's launch directory (`_launch_root(ws)`), or `config.ROOT` for doctor.
  - The detail names the folder asked about: `claude_code.config_home(env)` over the same
    merged environment. That is #970's rule, so doctor's `plane-root guard` row and this row
    name one folder for one environment.
  - It reads every covering entry, not the first (review 6). `installed_for` returns the first
    entry `covers` accepts, and this machine lists user, project and local entries, enabled and
    disabled, side by side.
  - **The most specific covering scope decides: local over project over user** (re-review N3).
    A chat that disables charter in `.claude/settings.local.json` is unwired whatever the user
    entry says, and a project enable over a user disable is wired.
  - `UNKNOWN` → `UNKNOWN_STATE`. No covering entry → `UNWIRED`. The most specific covering
    entry `enabled is True` → `WIRED`, otherwise `UNWIRED` with detail
    `"disabled at <scope> scope"`.
  - **Settings files decide too** (ruling 36), unless D2(a) shows the list's `enabled` follows
    a settings-file disable.
    - If it does not, or D2(a) cannot settle it, detection also reads
      `enabledPlugins["charter@charter"]` from the settings files covering the directory, most
      specific first: `<cwd>/.claude/settings.local.json`, `<cwd>/.claude/settings.json`, then
      `claude_code.config_home(env) / "settings.json"`.
    - The first file that names it decides, and `false` is `UNWIRED`, detail
      `"disabled in <file>"`.
    - An install entry being enabled never outranks a settings file that says `false`.
  - The profile's command is what runs (`[*command, "plugin", "list", "--json"]`), so a wrapper
    script or a pinned binary answers for itself.
- **Codex.**
  - `home = Path(env.get("CODEX_HOME") or Path.home() / ".codex")` from the merged environment;
    parse `home / "config.toml"`.
  - `WIRED` needs all three:
    1. `plugins."charter@charter".enabled is True`;
    2. `shell_environment_policy.set.CHARTER_HARNESS == "codex"`;
    3. D4's hook-trust rule.
  - Unreadable TOML → `UNKNOWN_STATE`.
  - When the missing mark is the policy line and `[shell_environment_policy]` already exists,
    `fix` is the line to add by hand, in `CODEX_POLICY_BY_HAND`'s words — never
    `charter harness install`, which would answer `present` and change nothing (review 7).
  - The detail names the home read. That this is the home charter can see — not one a wrapper
    script might export — is stated in the docs (Ruling 7).
- **opencode.**
  - `[*command, "debug", "config"]` with the merged env and `timeout=plugincache.LIST_TIMEOUT`.
  - Parse the JSON. `home = opencode.global_dir(env)`, the directory that holds `plugin/`.
    `WIRED` iff some `plugin` entry, read as a path with any `file://` prefix removed, equals
    `home / opencode.SHIM_PATH` — both `Path`s, compared resolved — and
    `opencode.shim_is_charters(home)` is True, a byte match (review 5), **and**
    `opencode.foreign_plugins(home)` is empty (re-review N1).
  - `unvouched` is not the test: it answers `()` when the shim is missing
    (`opencode.py:530`), so an entry naming a deleted file would read as wired.
  - Neither is `shim_is_charters` alone. `foreign_plugins` (`opencode.py:494-505`) and ADR 0015's
    amendment (`:205-217`) record the attack: a byte-perfect `plugin/charter.ts` beside
    `plugin/aaa_boot.ts` containing `Object.hasOwn = () => false` let a vault read through while
    the shim compared equal.
  - Bad JSON, non-zero exit or timeout → `UNKNOWN_STATE`.

**The fix sentence.**

```python
NOT_WIRED = ("charter: profile '{name}' is not wired — {detail}, so a chat on it would run "
             "without charter's guard. Nothing was started. Wire it: charter harness install "
             "{name}")
CANNOT_TELL = ("charter: charter could not ask {kind} whether profile '{name}' is wired "
               "({detail}), so it will not start it unguarded. Nothing was started. Run "
               "charter harness install {name}, or check by hand: {probe}")
```

- `{probe}` is the probe argv with the profile's env names prefixed (`CLAUDE_CONFIG_DIR=~/.claude-alt claude plugin list --json`).
- `CANNOT_TELL` refuses too (Ruling 12): no flag launches one unguarded.

**Where it runs.**
- **Launch.** `launcher.refusal`'s last check, with `cwd=Path.cwd()` — the directory `_launch`
  stands in before tmux, and the pane's own start directory after — before tmux on a terminal
  launch and again in the pane. Each is a fresh probe, and a cached answer never starts a chat
  (review B2, Ruling 13). A start pays the probe twice, about 0.4–1.5 s. The cache it could have
  reused is a file a chat can write, key, stamp and date ahead.
- **The selector** (Task 5) reads `cached` to draw its rows, and probes on a miss. That answer is
  display-only: picking a row runs `launcher.attempt`, which probes again.
- **A handoff probes twice** (re-review N8).
  - Once in `background_refusal`, from the target workspace's directory
    (`workspace.workspace_dir(ws)`, or `config.ROOT` before `--create` makes it), so
    `charter handoff` refuses visibly before it writes anything.
  - Then once in the pane before `exec`. `_launch` skips its own pre-tmux wiring probe for an
    `Opening`.
- **After a yes** the whole chain runs again from the top, wiring included (re-review N2): a
  yes never walks past a refusal behind it.
- **A `charter doctor` a person runs** probes, concurrently, and remembers each answer.
  `charter doctor --preflight`, which the SessionStart hook runs, never probes: no profile probe
  runs on a hook path (Ruling 11).

**The cache.**
- `config.STATE_DIR / "cache/harness-wiring.json"`, keyed by
  `sha256(json.dumps(profiletrust.fingerprint(p) | {"cwd": str(cwd)}, sort_keys=True))`.
- Each entry is the `Wiring`, `checked_at`, and the stamp: `{path: [mtime_ns, size]}`
  for D2's files.
- `cached` answers only when every stamp matches and `0 <= now - checked_at < 24 h`. An entry
  stamped in the future is stale: without the lower bound it would pass the age test forever
  (review B2).
- The docs state that a chat can write this file as it can the trust record (Ruling 13).
  That is why a launch never trusts it.

**`charter harness install <name>`.**
1. `p = profiles.current()["profiles"].get(name)`.
   - Absent, and `name` is a registry NAME (`claude-code`) → the built-in of that kind.
   - Absent otherwise → `UNKNOWN_PROFILE`, rc 2.
2. `profiles.ignored_refusal(config.ROOT)` for a declared profile → print it, rc 1.
3. `profiletrust.approval_needed(p)`:
   - on a terminal → `ask_in_terminal`; a no → rc 130;
   - not on a terminal → `UNATTENDED`, rc 1.
4. `wiring.install(p, config.ROOT)`:
   - **Claude Code** → `plugincache.install(config.ROOT, env=…, command=…)`.
   - **Codex** → `codex.install(env=merged)`.
     - `created` → print D4's measured Codex-side steps with `CODEX_HOME=<home>` prefixed,
       because Codex ignores hooks nobody approved.
     - `present` while the file lacks `set.CHARTER_HARNESS = "codex"` → refuse, rc 1, with
       `CODEX_POLICY_BY_HAND`. The table exists, and charter edits no TOML it did not write.
       `codex.install()` answers `present` for any `[shell_environment_policy]`
       (`codex.py:127`), so pointing back at this command would loop (review 7).

     ```python
     CODEX_POLICY_BY_HAND = ("charter harness install: {path} already has a "
         "[shell_environment_policy] table without charter's line, and charter does not edit "
         "TOML it did not write — nothing was changed. Add this line inside that table:\n"
         "  set = {{ CHARTER_HARNESS = \"codex\" }}\n"
         "or, if the table already has a `set`, add CHARTER_HARNESS = \"codex\" to it.")
     ```
   - **opencode** → `OpenCodeHarness().wire(config.ROOT, env=merged)`.
5. `wiring.detect(p, cwd=config.ROOT)` → `✓ profile 'x' is wired` or the fix again. rc 0 only
   when wired.

Today's `codex` NAME path keeps its messages (`tests/test_cmd_harness.py::HarnessInstall`).

**`init` / `reinit`** — `_wire_profiles(root, *, install)`, for each declared, approved profile
with an ignored file:
- `init` (`install=True`): `wiring.install(p, root)` for Claude Code and opencode profiles. A
  Codex profile is reported as "opt-in: charter harness install <name>", the rule
  `cmd_harness_install`'s docstring states for Codex today.
- `reinit` (`install=False`, Ruling 9):
  - opencode profiles: `wire` (files only);
  - Claude Code profiles: `detect`, reported as `profile 'x': not wired — charter harness install x`
    when it is not;
  - Codex: as `init`.
- Unapproved profiles: `profile 'x' is not approved yet — charter x`.

**`doctor`** — `check_profile_wiring(preflight=…)`:
- `preflight=True` returns `[]`, and `check_harness_profiles(preflight=True)` skips its git
  ignore check (re-review N8), so SessionStart's preflight spends no git call on profiles. It
  still reports refused profiles and a missing `default`, which cost one file read.
- Otherwise, every profile the selector would list (declared, plus built-ins whose program is on
  `PATH`), subject to D1's rule. Built-ins are probed and refused like any profile
  (Ruling 10).
- `concurrent.futures.ThreadPoolExecutor(max_workers=4)`; each probe keeps its own
  `LIST_TIMEOUT`.

| State | Row |
|---|---|
| wired | `Result("profile <name>", OK, detail=w.detail)` |
| unwired | `WARN`, `detail=w.detail`, `hint=f"charter harness install {name}"` |
| unknown | `WARN`, `hint=_NOT_CHECKED_HINT` |
| unapproved | `WARN`, `detail="not approved yet — charter asks before its command runs"`, `hint=f"charter {name}"`, no probe (Ruling 1) |
| file not ignored | no probe; the `harness profiles` row already warns |

A row's exception costs that row, not the preflight: the `plugin install` row's catch-all shape
(`doctor.py:2795`).

### Test cases — `tests/test_a_profile_is_wired_or_refuses.py`

Probes are faked at the seam `util.run`, or at `plugincache._claude_json` for Claude Code
(`tests/test_plugin_install.py:40-60`'s `rows()`/`spawns()` shapes). A shim directory is
prepended ahead of `_claudeguard`'s fake `claude`. No real harness runs in this module.

Class `ClaudeCodeIsAskedUnderTheProfilesEnvironment(PersonaIso, unittest.TestCase)`:
- `test_the_probe_runs_the_profiles_command_with_its_env`: `util.run` recorder → argv
  `["claude", "plugin", "list", "--json"]`, `env["CLAUDE_CONFIG_DIR"] == expanduser("~/.claude-alt")`,
  `cwd == <launch dir>`.
- `test_an_enabled_install_covering_the_directory_is_wired`: `[{"id": "charter@charter", "scope": "project", "projectPath": <dir>, "enabled": true}]`
  → `WIRED`.
- `test_no_entry_is_unwired`: `[]` → `UNWIRED`, `fix == "charter harness install claude-alt"`.
- `test_a_disabled_install_is_unwired`: `enabled: false` → `UNWIRED`, `"disabled" in detail`.
- `test_a_disabled_user_entry_listed_first_does_not_hide_an_enabled_project_entry`:
  `[{"id": "charter@charter", "scope": "user", "enabled": false}, {"id": "charter@charter", "scope": "project", "projectPath": <dir>, "enabled": true}]`
  → `WIRED` (review 6): the project entry is the more specific scope.
- `test_a_local_disable_over_a_user_enable_is_unwired`:
  `[{"id": "charter@charter", "scope": "user", "enabled": true}, {"id": "charter@charter", "scope": "local", "projectPath": <dir>, "enabled": false}]`
  → `UNWIRED`, `"disabled at local scope" in detail` (re-review N3).
- `test_a_project_enable_over_a_user_disable_is_wired`: the pair listed in the other order →
  `WIRED`, so the order of the list decides nothing.
- `test_a_disable_in_settings_local_json_over_an_enabled_user_install_is_unwired`: the list
  holds only an enabled user entry, and `<dir>/.claude/settings.local.json` holds
  `{"enabledPlugins": {"charter@charter": false}}` → `UNWIRED`,
  `"disabled in .claude/settings.local.json" in detail` (ruling 36).
- `test_a_disable_in_project_settings_json_over_an_enabled_user_install_is_unwired`: the same
  with `<dir>/.claude/settings.json` → `UNWIRED`.
- `test_an_unreadable_list_is_unknown_and_refuses`: rc 1 → `UNKNOWN_STATE`; `refusal(...)`
  contains `"could not ask"` and `"plugin list --json"`.
- `test_a_probe_that_times_out_refuses`: `util.run` raises `util.ProcTimeout` →
  `UNKNOWN_STATE`, `refusal(...)` contains `"could not ask"`, nothing raised (Ruling 12,
  review 10).
- `test_a_wrapper_scripts_answer_is_the_one_used`: `command = ["/opt/claude-wrap"]` → argv[0] is it.

Class `CodexIsReadAtTheProfilesHome(PersonaIso, unittest.TestCase)` — `tests/__init__.py` already
points `CODEX_HOME` at a sandbox; each case writes its own home under `self.tmp` and declares
`env = { CODEX_HOME = "<that>" }`:
- `test_all_three_marks_are_wired`: plugin enabled, `set = { CHARTER_HARNESS = "codex" }`, and a
  trust entry per hook key → `WIRED`; `detail` names the home.
- `test_an_empty_home_is_unwired`: → `UNWIRED`.
- `test_the_plugin_without_the_policy_line_is_unwired`: → `UNWIRED`, `"shell_environment_policy" in detail`.
- `test_untrusted_hooks_are_unwired`: → `UNWIRED`, `"approve" in fix.lower()`.
- `test_the_default_home_is_the_one_codex_reads_with_no_env`: no `CODEX_HOME` in the profile or
  the process (`mock.patch.dict(os.environ, {...}, clear=True)` with `HOME` stated) →
  `home/.codex/config.toml` is read.
- `test_a_malformed_config_is_unknown`: `"[x"` → `UNKNOWN_STATE`.

Class `OpencodeIsAskedUnderTheProfilesConfigHome(PersonaIso, unittest.TestCase)`:
- `test_the_shim_in_the_answer_is_wired`: `util.run` returns `{"plugin": ["file://<xdg>/opencode/plugin/charter.ts"]}`
  with the shim written → `WIRED`.
- `test_no_shim_is_unwired`: `{"plugin": []}` → `UNWIRED`.
- `test_a_plugin_entry_naming_a_missing_shim_is_unwired`: the entry names
  `<xdg>/opencode/plugin/charter.ts` and no file is there → `UNWIRED` (review 5).
- `test_a_byte_perfect_shim_beside_a_foreign_plugin_is_not_wired`: the shim written by
  `refresh_shim` (so `shim_is_charters` is True) and `plugin/aaa_boot.ts` holding
  `Object.hasOwn = () => false` beside it, the probe naming the shim → `UNWIRED`, and `detail`
  names `aaa_boot.ts`. This is the documented bypass (re-review N1).
- `test_an_opencode_probe_that_times_out_refuses`: `util.run` raises `util.ProcTimeout` for
  `debug config` → `UNKNOWN_STATE`, and `refusal(...)` contains `"could not ask"`
  (re-review N1).
- `test_a_shim_charter_cannot_vouch_for_is_unwired`: foreign shim content → `UNWIRED`.
- `test_bad_json_is_unknown`: → `UNKNOWN_STATE`.

Class `NothingUnapprovedIsRun(PersonaIso, unittest.TestCase)`:
- `test_an_unapproved_profile_is_never_probed`: `util.run` raises → `launcher.refusal(...)`
  returns Task 3's refusal and no probe ran.
- `test_doctor_does_not_probe_an_unapproved_profile`: its row is `WARN`, `hint == "charter claude-alt"`,
  `util.run` not called for it.
- `test_install_asks_first`: stdin `"n\n"` on a tty → rc 130, `plugincache.install` not called.
- `test_a_yes_on_an_unwired_profile_still_refuses_with_no_frame`: an unapproved declared
  profile, `detect` → `UNWIRED`, stdin and stdout ttys answering `"y\n"`; `charter <name> --no-frame`
  → the record is written, `execvpe` not called, rc 1, and stderr holds `"not wired"`
  (re-review N2).
- `test_a_yes_on_an_unwired_profile_still_refuses_in_the_pane`: the same through
  `cmd_frame_launch`, with `tmuxctl.live_pane_by_pid` faked to prove the frame for `os.getpid()` →
  no exec, `"not wired"`.
- `test_callers_branch_on_the_kind_not_the_text`: `NEEDS_ASKING` patched to a different
  sentence → the pane still asks (the kind decided).

Class `TheLaunchRefusesAnUnwiredProfile(PersonaIso, unittest.TestCase)` — Task 2's
`ALaunchNamesAProfile` patches, profile approved:
- `test_an_unwired_profile_is_refused_before_tmux_with_the_fix`: `detect` → `UNWIRED` → rc 1, no
  tmux call, stderr contains `"charter harness install claude-alt"`.
- `test_a_cached_wired_answer_never_starts_a_chat`: `remember(WIRED)` a moment ago, `detect` →
  `UNWIRED` → refused (review B2).
- `test_every_launch_probes_fresh_before_tmux_and_in_the_pane`: a terminal launch, then
  `launcher.start` in-process for the same chat → `detect` called twice.
- `test_a_reopen_skips_an_unwired_profile_by_name`: `_reopen_one` → `None`, warning names the fix.
- `test_a_built_in_that_is_not_wired_is_refused_too`: `harness="codex"`, no profile declared, a
  `CODEX_HOME` whose `config.toml` lacks the plugin mark → rc 1, `"charter harness install codex"`
  in stderr, no tmux call (Ruling 10).
- `test_a_built_in_claude_under_claudeguards_empty_answer_is_refused`: no stand-in, the suite's
  fake `claude` answering `[]` → refused. This pins why `wired_as_today` exists.

Class `TheCache(PersonaIso, unittest.TestCase)`:
- `test_a_stamp_that_still_matches_is_answered`: `remember`, `cached` → same `Wiring`.
- `test_a_touched_stamp_file_is_a_miss`: `os.utime` on the stamped file → `None`.
- `test_a_changed_profile_is_a_miss`: re-declared command → `None`.
- `test_an_old_entry_is_a_miss`: `checked_at` 25 h ago → `None`.
- `test_a_stamp_in_the_future_is_stale`: `checked_at` one hour ahead → `None` (review B2).
- `test_an_unreadable_cache_is_a_miss`: `"{nope"` → `None`.

Class `HarnessInstallTakesAProfile(PersonaIso, unittest.TestCase)`:
- `test_a_claude_profile_installs_under_its_env`: `plugincache.install` called with
  `env["CLAUDE_CONFIG_DIR"]` and `command`; rc 0 when the follow-up detect is `WIRED`.
- `test_an_opencode_profile_wires_its_own_config_home`: the shim lands under `<xdg>/opencode/plugin/`,
  and nothing lands under the sandbox default.
- `test_a_codex_profile_writes_its_policy_line_under_its_own_home`:
  `tomllib.loads((<home> / "config.toml").read_text())["shell_environment_policy"]["set"]["CHARTER_HARNESS"] == "codex"`,
  and the sandbox default home's `config.toml` is unchanged (review 10).
- `test_a_codex_profile_names_the_codex_side_steps`: the output contains `CODEX_HOME=` and D4's
  command.
- `test_an_existing_policy_table_without_charters_line_is_refused_with_the_line_to_add`: the
  home's `config.toml` holds `[shell_environment_policy]` with `inherit = "core"` → rc 1, the
  file's bytes unchanged, the output contains `set = { CHARTER_HARNESS = "codex" }`, and
  `"charter harness install"` is not in it (review 7).
- `test_the_codex_fix_for_that_table_is_the_line_not_the_command`: `detect` for that home →
  `UNWIRED`, `fix` contains `CHARTER_HARNESS = "codex"` and not `charter harness install`.
- `test_a_registry_name_still_works_as_today`: `name="codex"` (the NAME) → today's messages.
- `test_an_unknown_name_is_refused_with_the_profiles_named`: rc 2.
- `test_a_committable_local_file_refuses_install`: rc 1, `"charter reinit adds"`.

Class `InitAndReinitWireEachProfile(PersonaIso, unittest.TestCase)` — `tests/test_init.py`'s fixture:
- `test_init_installs_for_an_approved_claude_profile`: `wiring.install` called for it.
- `test_init_leaves_a_codex_profile_opt_in`: not called; output contains `"charter harness install codex-alt"`.
- `test_reinit_installs_nothing_and_names_what_is_missing`: `plugincache.install` not called;
  output contains `"not wired"`.
- `test_reinit_wires_an_opencode_profiles_shim`: an approved opencode profile with
  `env = { XDG_CONFIG_HOME = "<tmp>/oc" }` → `<tmp>/oc/opencode/plugin/charter.ts` exists and
  `opencode.shim_is_charters(Path("<tmp>/oc/opencode"))` (review 10).
- `test_neither_touches_an_unapproved_profile`: not called; output contains `"not approved yet"`.

Class `DoctorHasARowPerProfile(PersonaIso, unittest.TestCase)`:
- `test_every_listed_profile_has_a_row_in_order`: `check_names()` contains
  `["harness profiles", "profile claude", "profile claude-alt"]` in sequence, and equals the
  names `run_all()` produces
  (`tests/test_a_table_column_is_measured_in_cells.py::TestDoctorSizesItsNameColumnFromTheChecks`).
- `test_a_wired_row_is_ok_with_no_hint`.
- `test_an_unwired_row_names_the_install`.
- `test_a_probe_that_raises_costs_one_row`: `detect` raises for one → that row `WARN`, the others
  present, `len(run_all()) > 20`.
- `test_rows_are_probed_concurrently`: two probes each sleeping 0.3 s (patched) → the rows take
  < 0.55 s.
- `test_the_preflight_runs_no_git_for_profiles`: `util.run` raising for any git argv →
  `doctor.run_all(preflight=True)` raises nothing and holds a `harness profiles` row
  (re-review N8).
- `test_the_preflight_never_probes`: `mock.patch("charter.wiring.detect", side_effect=AssertionError)`
  and `util.run` raising for a harness argv → `doctor.run_all(preflight=True)` raises nothing
  and holds no `profile …` row.
- `test_the_names_agree_with_the_preflight`: `check_names(preflight=True)` equals the names
  `run_all(preflight=True)` produces, in order, with no `profile …` name.
- `test_a_hand_run_doctor_probes_every_approved_profile`: `run_all()` calls `detect` once per
  approved listed profile and never for an unapproved one.
- `test_the_session_start_hook_runs_the_preflight`: `hooks/hooks.json`, read off the repository
  root, has a SessionStart command containing `charter doctor --preflight`, and no command runs
  `charter doctor` without it.
- `test_the_parser_takes_the_preflight_flag`: `build_parser().parse_args(["doctor", "--preflight"]).preflight is True`.

### Implementation steps

- [ ] Step 0 (D1–D5); stop on D3.
- [ ] Tests first; red.
- [ ] `env`/`command` threading in `plugincache`, `codex`, `opencode` (existing tests stay green).
- [ ] `config_home(env=)` and `global_config_file(env=)` on #970's resolver.
- [ ] `wiring.py`; the launcher's last check; `harness install`; `_wire_profiles`; doctor rows;
      `--preflight` and the `hooks/hooks.json` command; `wired_as_today` and the tests it keeps
      green.
- [ ] The dispatch and persona docstrings (Ruling 16).
- [ ] Whole suite; `python3 tools/sweep.py`; report it.
- [ ] Docs, news; PR.

### Docs and news

- **`docs/harnesses.md`.**
  - `## Wiring, and when it happens` gains `### Per profile`:
    - what "wired" means for each kind, and the probe that answers it;
    - D1–D5's readings;
    - the fix;
    - what `init`, `reinit` and `harness install <profile>` each do;
    - that `claude plugin list --json` writes `.claude.json` and `backups/` into the folder it
      asks about;
    - that dispatch's transcript lookup and persona skill lookup answer for the default config
      folder (Ruling 16);
    - that Codex's home is the one charter can see, and one a wrapper script exports is not
      (Ruling 7);
    - that the launch record and the wiring cache under `.charter/` are as writable by a chat
      as `charter.local.toml`, and a launch never trusts the cache (Ruling 13).
  - The table at `:33` gains a row for the wiring check's reach on each kind.
- **`docs/install.md`:** the doctor rows `harness profiles` and `profile <name>`, and that the
  SessionStart preflight runs `charter doctor --preflight`, which probes no profile.
- **`docs/control-plane.md`** `### The profile is wired`: the refusal as printed, and that no
  flag launches one unguarded.
- **News body lines:**
  - "A profile whose config folder does not carry charter's plugin refuses to start and prints
    `charter harness install <profile>`, which wires it; `charter doctor` shows a row per
    profile."
  - "This includes the built-ins: `charter codex` on a plane where nobody wired Codex, and
    `charter opencode` where `init` never wrote its shim, now refuse where they used to start."
  - "Codex users approve charter's SessionStart preflight hook once more: its command gained
    `--preflight`."
  - No `check:`: doctor's exit code does not distinguish it (WARN exits 0).

**Suggested PR title:** A harness profile that does not carry charter's plugin refuses to start and names the command that wires it

---
## Task 5: The selector — no harness starts until someone picks a profile

**Depends on:** Tasks 2, 3 and 4. Its rows carry 3's and 4's states.

### Step 0 — Measure first

- **S1.** With Task 4's cache warm and cold, and three declared profiles plus the installed
  built-ins, time from the `frame-launch --select` process start to the first painted row.
  Record the numbers in `docs/frame.md`.
- **Stop rule:** if a cold open exceeds 2 s with probes concurrent, stop and report. The
  fallback — rows paint first and states fill in — is a design change for the controller, not
  a fix to improvise.
- **S2.** On tmux 3.7c and 3.2, both servers: a `frame-launch --select` pane that exits 130
  (Esc) as the session's only window. Confirm the session is gone, `_launch` returns 130, and
  the eager `_query_pane_dead_status` answered `None` while it waited.

### Files

- Create `charter/frame/selector.py` and `tests/test_a_new_chat_starts_at_the_profile_selector.py`.
- `charter/frame/overlay.py:655` — `Surface` gains `footer: str | None = None`. `render` (`:745`)
  prints it in place of the hard-coded line when set.
- `charter/frame/launcher.py`:
  - `cmd_frame_launch` gains `--select` and `--start NAME`.
  - `argv_select(start: str | None) -> list[str]`.
- `charter/frame/state.py` beside `record_closed` (`:1998`):
  - `record_waiting`/`is_waiting`/`clear_waiting`;
  - `record_picked_kind(fid, harness_name)`, which rewrites `identity`'s `CHARTER_HARNESS`
    through `record_identity`.
- `charter/frame/leave.py:163` — `plan` skips `state.is_waiting(fid)` beside `was_closed`. Tabs
  (`chats._by_workspace`, `chats.py:334`) do not.
- `charter/commands_frame.py`:
  - `_launch`: `args.select`, with no profile, `h = None`, argv from `launcher.argv_select`;
    `env["CHARTER_HARNESS"] = ""` right after `_frame_env(fid, None)`, which would otherwise carry
    the launching shell's kind onto the window and into its identity record (review 12);
    `state.record_waiting` before tmux; attach-and-add-nothing; suppress the early-death and
    recorded-plane lines for a waiting pane that exited 130.
  - `cmd_new_chat` and `_open_workspace` launch `select=True, start=…`, and `NO_HARNESS`'s stop
    goes.
  - The palette's `chat: new` reaches `cmd_new_chat` unchanged.
- `charter/cli.py`:
  - `_bare_launch` returns `["frame", "--select", *argv]` on a terminal, and no longer reads or
    refuses a default.
  - `frame`'s parser gains `--select`.
  - `_OWN_FLAGS` gains `"--select"`.
- Tests updated:
  - `tests/test_bare_charter_opens_the_frame.py` (bare charter now selects);
  - `tests/test_the_chat_bars_plus_makes_a_chat.py::ThePressSaysWhyWhenItWillNotMakeAChat::test_a_chat_recording_no_launchable_harness_is_refused_by_name`
    (the stop is gone: replace it with the selector opening);
  - `tests/test_a_workspace_tab_opens_what_it_names.py::TheOpenUsesTheHarnessTheOperatorIsAlreadyIn`
    (it opens the selector starting on that profile).
- Docs: `docs/frame.md`, `docs/control-plane.md`, `README.md`, `docs/install.md`.

### Interfaces

**Consumes:**
- From Tasks 1–4: `profiles.current`, `profiletrust.approval_needed`/`record_launched`,
  `wiring.cached`/`detect`/`remember`, `launcher.start`, `_same_profile_as`.
- On `main`:
  - `palette.own_the_tty(surface, *, fd=None, out=None, then=None)` (`palette.py:475`),
    `palette.Palette` (`:314`)
  - `overlay.Row(id, title, note, mark, refused)` (`overlay.py:435`), `overlay.CHOOSE`/`CANCEL`
  - `pane.claim`/`release` (`frame/pane.py:80`), `_workspace_to_focus` (`:2081`),
    `_plane_session`, `_focus_workspace` (`:4758`)

**Produces:**

```python
# charter/frame/selector.py
CANCELLED_EXIT = 130
FOOTER = "  up/down move   enter start   esc close this chat"

class Choice(NamedTuple):
    profile: str          # the picked profile's name
def rows(read: dict, *, cwd: Path, start: str | None) -> tuple[overlay.Row, ...]: ...
def pick(*, cwd: Path, start: str | None, tty_fd: int | None = None) -> Choice | None: ...
    # None on Esc / end of input; loops on a refused row, showing its reason in the footer

@dataclass
class Selector(palette.Palette): ...     # label "charter · which profile?", footer FOOTER
@dataclass
class Confirm(overlay.Surface): ...      # "run this? [y/N]": `y` → CHOOSE, anything else → CANCEL

# charter/frame/launcher.py
def argv_select(start: str | None) -> list[str]: ...
    # util.self_relaunch_argv("frame-launch", "--select", "--attended",
    #                         *(("--start", start) if start else ()))

# charter/frame/state.py
def record_waiting(fid: str) -> None: ...     # .charter/frame/<fid>/waiting
def is_waiting(fid: str) -> bool: ...
def clear_waiting(fid: str) -> None: ...
def record_picked_kind(fid: str, harness_name: str) -> None: ...
```

### Behaviour

**Where it appears.**
- **Bare `charter` on a terminal** → `charter frame --select`. `_launch` resolves the workspace
  as today: the picker stays before tmux (#518).
  - If `_plane_session(SOCKET, ws=ws)` has a seat **and** `not rest` → `_focus_workspace(*seat, ws=ws, picked=picked)`,
    whether or not a client is attached. That is attach-and-add-nothing, for `--select` launches
    only.
  - `charter <profile>` and `charter frame -- <cmd>` keep today's open-or-focus rule (Ruling 17).
  - Otherwise a new chat window whose first command is `launcher.argv_select(read["default"])`.
- **`+`** (`cmd_new_chat`) and **a workspace tab with no running chat** (`_open_workspace`):
  - `start = p.name` where `_same_profile_as(fid)` gave a profile, else `read["default"]`, else
    `None`.
  - The launch namespace is `SimpleNamespace(harness="frame", select=True, start=start, rest=[], no_frame=False, workspace=ws, pick=False, attach=False, size=…)`.
  - `NO_HARNESS` no longer stops a press.
- **Never** for `charter <profile>`, reopen, a restored plane, or a handoff: all of those name
  their profile.

**In the pane** (`cmd_frame_launch` with `--select`):
1. `held = pane.claim()`, then `try/finally pane.release(held)`: `cmd_palette`'s shape
   (`commands_frame.py:8166`).
2. `read = profiles.current()`; `rows(read, cwd=os.getcwd(), start=args.start)`.
   - **Listed:** every declared profile, and a built-in only when
     `shutil.which(expanded_command(p)[0])`.
   - **Refused declared profiles** are listed too, with their reason.
   - **Each row** is `Row(id=f"profile:{name}", title=name, note=…, mark=(name == start), refused=…)`.

   | State, first match | `refused` | Note |
   |---|---|---|
   | in `read["refused"]` | True | the reason |
   | local file tracked or not ignored (declared only) | True | `"charter.local.toml is tracked or not ignored — charter reinit"` |
   | command not on `PATH` | True | `f"not on PATH: {cmd[0]}"` |
   | `approval_needed` is `"new"` / `"changed"` | False | `"not approved yet (new) — Enter shows its command"` / `"not approved yet (changed) — Enter shows its command"`; no wiring probe runs for it (Ruling 1) |
   | `wiring.cached(...)` or, on a miss, `detect` (concurrent, then `remember`) is `UNWIRED`/`UNKNOWN_STATE` | True | `f"not wired — {w.fix}"`; display only, and a pick probes again (review B2) |
   | otherwise | False | `f"{p.kind} · {profiles.display(p)}"` |

3. **Cursor.** It opens on the `start` row when that row can run; otherwise where
   `palette.aim` puts it (the first runnable row). A `default` naming a profile this machine
   lacks marks no row (Ruling 18).
4. `palette.own_the_tty(Selector(...))` returns a row:
   - **Refused** → the surface re-opens with `footer = f"  {row.note}"`, the reason, and the
     cursor where it was.
   - **New or changed** → `own_the_tty(Confirm(heading=f"run this? {display(p)}"))`; `y` →
     `profiletrust.record_launched(p)`; anything else → back to the selector.
   - **Wired and approved** → `Choice(name)`.
   - This is the spec's "Enter on it only shows the reason". The palette closes on a refused
     Enter today (`commands_frame.py:8396-8456`), so the selector does not borrow that
     behaviour (Ruling 6).
5. **Esc / end of input** → `sys.exit(CANCELLED_EXIT)`, having started nothing.
   - The pane dies and `pane-died[1]` kills the window. If it was the workspace's only window,
     the session goes with it: the last-chat rule (`docs/frame.md:481-483`).
   - `state.is_waiting(fid)` is still true, so `leave.plan` never records it. `_launch` sees
     130 with `is_waiting` and prints neither `early_death_message` nor
     `_say_the_plane_is_recorded`.
6. **On a Choice:**
   - `launcher.attempt(p, [], fid=fid, attended=True, on_exec=…)`, which re-runs every guard
     fresh.
   - Only when it is about to `exec` does `on_exec` run: `state.clear_waiting(fid)`, then
     `state.record_picked_kind(fid, p.harness)` — so panels, `chats.harness_of`, `leave.plan`
     and `_same_profile_as` read the kind from the file — then `state.record_profile(fid, p.name)`.
     It returns the undo: `record_waiting`, the kind cleared, the profile record removed. If
     `execvpe` raises, `attempt` runs that undo, and the pane prints the reason and exits with
     the refusal's code. It is still waiting, so a quit never records it (N7 nit).
   - **A pick refused at launch returns to the selector** (re-review N7). `attempt` hands back
     the `Refusal`; the pane re-opens the selector with that row updated (`refused=True`, its
     note the refusal's text) and the reason in `footer`, and stays waiting. Only Esc closes the
     window.
   - The window's tmux `-e CHARTER_HARNESS=` was empty at creation; the exec environment sets it
     for the harness and every hook it runs.

**ADR 0018, amended in code comments and docs.** Charter draws in the pane only while no harness
has ever run in it. After the `exec`, nothing of charter's is in that pane. A harness that exits
closes the window as today and never returns to the selector.

**Operator's own tmux.**
- `_launch_in_operator_tmux` puts `argv_select` on `respawn_argv`, keeping the `cat` placeholder
  and `remain-on-exit` ahead of it (Ruling 4).
- `_wait_for_harness` waits through the selection and returns 130 on Esc.

**Refusals and texts.** Unchanged from Tasks 2–4. The selector adds only the footer, the
`Confirm` heading and:

```python
NOTHING_TO_PICK = ("charter: no profile can start here — every row above says why. Nothing "
                   "was started; fix one of them and open a chat again.")
```

`NOTHING_TO_PICK` is shown in the footer when every row is refused and Enter is pressed.

### Test cases — `tests/test_a_new_chat_starts_at_the_profile_selector.py`

Class `TheRows(PersonaIso, unittest.TestCase)` — `wiring.cached`/`detect` and `shutil.which`
patched per case:

| Case | Asserts |
|---|---|
| `test_a_declared_profile_is_always_listed` | even with `which` → None: its row is present, `refused` True, `"not on PATH" in note` |
| `test_a_built_in_is_listed_only_when_its_program_is_installed` | `which("codex")` → None → no `profile:codex` row |
| `test_a_refused_profile_is_listed_with_its_reason` | a string command → row refused, `"never a shell string"` |
| `test_a_committable_file_refuses_every_declared_row_and_no_built_in` | declared rows refused, built-in rows not |
| `test_an_unwired_profile_carries_its_fix` | `"charter harness install claude-alt"` in note |
| `test_a_new_profile_is_not_refused_and_says_it_is_not_approved_yet` | `refused` False, `"not approved yet (new) — Enter"` in note |
| `test_an_unapproved_profile_is_never_probed` | `detect` raises if called → no raise |
| `test_the_start_row_is_marked` | `start="claude-alt"` → only that row has `mark` |
| `test_a_default_naming_a_missing_profile_marks_nothing` | `start="gone"` → no mark |
| `test_a_default_naming_a_missing_profile_puts_the_cursor_where_palette_aim_does` | `start="gone"`, a refused first row → `Selector(rows).selected()` is the row `palette.aim` picks for those rows, the first that can run (Ruling 18, review 10) |
| `test_control_bytes_in_a_command_are_escaped_in_its_row` | `command = ["rm\r\x1b[2Kclaude"]`, approved and wired → the row's note and its rendered line hold no `\r` or ESC byte, and hold the escaped command (ruling 35) |
| `test_a_warm_cache_spawns_nothing` | `cached` → `WIRED` for all; `util.run` raises → no raise |

Class `ThePick(PersonaIso, unittest.TestCase)` — `palette.own_the_tty` patched to return queued rows:
- `test_enter_on_a_runnable_row_is_the_choice`: → `Choice("claude-alt")`.
- `test_enter_on_a_refused_row_keeps_the_selector_open_and_shows_why`: queue refused then
  runnable → the second `own_the_tty` call's surface has `footer` containing the refused note;
  the result is the runnable choice.
- `test_enter_on_a_new_profile_asks_in_place_and_yes_records_it`: queue new row, `Confirm` →
  CHOOSE → `approval_needed == ""`, `Choice`.
- `test_no_at_the_confirm_goes_back_to_the_list_and_records_nothing`.
- `test_escape_starts_nothing`: `own_the_tty` → None → `pick` returns None.
- `test_the_selector_shows_even_with_one_profile`: only the built-in `claude` is installed →
  `own_the_tty` is called once, with a one-row `Selector`; nothing is picked for the operator
  (review 10).
- `test_the_footer_does_not_promise_a_harness_to_go_back_to`: `"F12" not in Selector(...).render(...)[-1]`.

Class `ThePaneWaitsThenBecomesTheHarness(PersonaIso, unittest.TestCase)` — `cmd_frame_launch` with
`--select`, `pane.claim` stood in, `launcher.os.execvpe` patched, `CHARTER_SESSION_ID=beta.1`
(`clear=True`), `tmuxctl.live_pane_by_pid` faked to answer `beta.1`'s window for `os.getpid()`
(ruling 29, revised), `state.record_waiting("beta.1")`:
- `test_a_pick_records_kind_and_profile_then_execs`: → `identity("beta.1")["CHARTER_HARNESS"] == "codex"`,
  `state.profile == "codex-alt"`, `not is_waiting`, execvpe called.
- `test_a_pick_refused_at_launch_returns_to_the_selector`: `wiring.detect` → `UNWIRED` at the
  pick, though the row was cached wired → no exec and no `SystemExit`. The next `own_the_tty`
  call's `Selector` has that row `refused` with `"not wired"` in its note and the reason in
  `footer`, and `state.is_waiting("beta.1")` is still True (re-review N7).
- `test_an_exec_that_fails_after_the_pick_reverts_and_exits`: `execvpe` raises `OSError` →
  `SystemExit(REFUSED_EXIT)`, `state.is_waiting("beta.1")` is True again,
  `state.profile("beta.1") is None`, and `identity("beta.1")["CHARTER_HARNESS"] == ""` (N7 nit).
- `test_the_frame_proof_runs_before_the_pane_is_claimed_or_drawn`: `tmuxctl.live_pane_by_pid`,
  `pane.claim` and `palette.own_the_tty` recorded into one call log → `live_pane_by_pid` is its
  first entry (ruling 35).
- `test_only_escape_closes_the_selector_window`: a refused pick, then Esc → `SystemExit(130)`.
- `test_escape_exits_130_and_leaves_it_waiting`: → `SystemExit(130)`, `is_waiting` still True.

Class `AWaitingPaneIsNotAChat(PersonaIso, unittest.TestCase)`:
- `test_it_has_a_tab`: `record_waiting("beta.2")` → `"beta.2" in chats._by_workspace()["beta"]`.
- `test_a_quit_does_not_record_it`: `leave.plan(...)` has no `beta.2`.
- `test_it_is_never_reopened`: the manifest written by `_record_the_plane` has no `beta.2`.
- `test_a_cancel_says_nothing_about_a_recorded_plane`: `_launch` with the eager query `None` and
  a final exit code 130 on a waiting fid → stdout/stderr hold neither `"recorded"` nor the
  early-death sentence; rc 130.

Class `WhereItAppears(PersonaIso, unittest.TestCase)` — Task 2's `_launch` patches:

| Case | Asserts |
|---|---|
| `test_bare_charter_on_a_terminal_opens_the_selector_launch` | `cli._bare_launch([])` with `isatty` True → `(["frame", "--select"], None)`, even with no default |
| `test_bare_charter_piped_still_prints_usage` (pin: passes at `a5aa860`) | `isatty` False → `([], None)` |
| `test_a_refused_default_no_longer_stops_bare_charter` | `default = "nope"` → `(["frame", "--select"], None)` |
| `test_bare_charter_on_a_running_workspace_nobody_is_attached_to_attaches` | `_plane_session` → seat, `list-clients` empty → `_focus_workspace` called, no `new-window` |
| `test_a_named_profile_on_that_workspace_still_opens_a_chat` | `harness="claude", profile="claude-alt"`, same state → `new-window` recorded |
| `test_the_plus_opens_the_selector_starting_on_the_pressers_profile` | `cmd_new_chat` → launch args `select=True, start="claude-alt"` |
| `test_the_plus_with_no_profile_record_and_no_default_opens_the_selector_anyway` | `start is None`, no refusal on screen |
| `test_a_tab_opens_the_selector` | `_open_workspace` → `select=True` |
| `test_a_reopen_never_opens_the_selector` (pin: passes at `a5aa860`) | `_reopen_args(...)` has no `select` |
| `test_a_handoff_never_opens_the_selector` (pin: passes at `a5aa860`) | `open_in_background` launch args have no `select` |
| `test_the_selector_launch_records_the_chat_as_waiting_before_tmux` | inside the `new-window` fake: `state.is_waiting("beta.1")` |
| `test_the_window_starts_on_the_launcher_with_no_inherited_kind` | `os.environ` holds `CHARTER_HARNESS=codex`, the pressing chat's (`clear=True`) → the argv ends `["frame-launch", "--select", "--attended", "--start", "claude-alt"]`; the `-e CHARTER_HARNESS=` value is empty, and `state.identity("beta.1")["CHARTER_HARNESS"] == ""`. `_frame_env(fid, None)` alone would carry `codex` (review 12) |

Real tmux, in the same module: class `TheSelectorOnARealServer`. Skip rules as in Task 2's real
module. `palette.own_the_tty` cannot be driven without a client, so the pane runs
`frame-launch --select` with a stand-in `pick` returning a fixed choice or None (patched by
module path through a `PYTHONPATH` shim):
- `test_a_cancelled_only_window_takes_the_session_with_it` (S2).
- `test_a_pick_leaves_the_same_pane_running_the_harness` (L1 for the selector).

### Implementation steps

- [ ] S1, S2; stop on S1's rule.
- [ ] Tests first; red.
- [ ] `Surface.footer`; `selector.py`; `state` waiting and picked-kind; `leave.plan` skip.
- [ ] `_launch` selector path and attach-and-add-nothing; `cmd_new_chat`/`_open_workspace`;
      `_bare_launch`.
- [ ] Grep the suite for `records no harness`, `declares no \`[harness] default\``,
      `prints the usage` and `the planes default`, both directions; reword.
- [ ] Whole suite; `python3 tools/sweep.py`; report it.
- [ ] Docs, news; PR.

### Docs and news

- **`docs/frame.md`.**
  - Top (`:17-24`): bare `charter` opens the selector, or attaches to a workspace already
    running.
  - New `### No harness starts until you pick a profile`, beside `### Picking a workspace when
    the frame opens` (`:2951`):
    - the rows and their states;
    - Enter on a refused row shows the reason and stays;
    - new or changed asks in place;
    - Esc closes the chat, and the session when it was the last;
    - a waiting pane has a tab and is never recorded or reopened;
    - it always shows, even with one profile;
    - S1's numbers.
  - The open-or-focus paragraph (`:485-502`): bare `charter` attaches whether or not anyone is.
  - `+` (`:808-841`) and tabs (`:2803-2806`): the selector, starting on that chat's profile; the
    "no harness … no default" stop is gone.
  - ADR 0018's amendment in one sentence where the frame says charter never draws in the
    harness pane (`:13`).
- **`docs/control-plane.md`:** `default` chooses the row the selector starts on and launches
  nothing. A missing one starts on no row and `doctor` warns.
- **`README.md:89-116`, `docs/install.md:296-305`:** bare `charter` opens the selector. No
  `[harness] default` is needed to use it.
- **News:**
  - headline becomes `A new chat asks which harness profile to start — no harness runs until you pick one`;
  - body: where it appears and where it does not; attach-and-add-nothing; Esc; `default`
    preselects.

**Suggested PR title:** A new chat opens at a profile selector, and no harness starts until you pick one

---
## Task 6: The records — the ADR, the 0017 and 0018 amendments, `CONTEXT.md`, the phase-5 line, the news review

**Depends on:** Tasks 1–5 on `main`.

### Files

- `docs/adr/0022-a-harness-profile-belongs-to-one-machine.md` (new; number per Ruling 2).
- `docs/adr/0017-charter-ignores-what-carries-credentials.md` — a closing `## Amended: a file whose
  whole meaning is "not committed"` section.
- `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md` — a closing `## Amended:
  before any harness has run` section.
- `CONTEXT.md` — three entries in `### The plane`, directly after `**Harness**:` (`:17-21`).
- `docs/superpowers/specs/2026-08-28-phase5-workspace-and-chat-tabs.md:329-330` — the credentials
  sentence gets a dated note, not a deletion: that spec is the record of what was decided then.
- `docs/news/unreleased-harness-profiles.md` — reviewed against CONTEXT.md's Prose rules.
- `tests/test_the_profile_words_are_written_down.py` (new).

### Behaviour

**`CONTEXT.md`**, in the file's entry shape (`**Term**:` / definition / `_Avoid_:`):

- **Harness profile**: A named way to launch one harness kind — its kind, its command, its
  environment — declared in the plane's `charter.local.toml`, which charter keeps out of git.
  Every registered kind is also a profile named after itself.
  _Avoid_: alias, account
- **Kind**: Which harness program a profile launches, written as the word typed after `charter`:
  `claude`, `codex`, `opencode`. `$CHARTER_HARNESS` holds the registry's name for it
  (`claude-code`), and `$CHARTER_HARNESS_PROFILE` the profile's.
  _Avoid_: type, flavour
- **Profile selector**: What a new chat's pane shows before any harness has run in it; the harness
  starts in that pane once a profile is picked.
  _Avoid_: picker (that is the workspace prompt before tmux), menu

**The ADR**, in the repo's ADR shape: claim title, the failure, the decision, why, what it costs,
what was considered.

- **The failure:**
  - one program per kind, launched one way;
  - opening charter started a harness nobody picked.
- **The decision**, one line each, with the reason beside it (the spec's own reasons, and
  `workspace.md`'s):
  - profiles live only in `charter.local.toml`;
  - that file carries `[harness]` alone;
  - "ignored" is guaranteed by `init`/`reinit` and checked at launch;
  - no credential in `env`;
  - a new or changed command asks once;
  - an unwired profile refuses to launch;
  - every chat pane starts as a charter launcher that `exec`s the profile;
  - `CHARTER_HARNESS` stays the kind;
  - no harness starts until a profile is picked;
  - reopen never substitutes;
  - no `charter harness add`.
- **Costs, at full volume:**
  - a pattern can refuse an innocent env name;
  - a wrapper script can still carry a key;
  - the trust record and the wiring cache sit under `.charter/`, which a chat can write;
  - asking a harness is not write-free (`claude plugin list --json` writes `.claude.json`);
  - `doctor` spends a subprocess per profile;
  - `+` and bare `charter` cost one more keypress on a one-harness machine;
  - the proof is one account in two folders, not two accounts.
- **Considered and rejected:**
  - one `"kind: command args"` string (nowhere to put env, shell splitting);
  - a per-user `~/.config/charter` file (the operator's call);
  - a full local overlay (plane policy with no trace in git);
  - a vault reference in `env` (the key still lands in the model's shell);
  - setup at launch (a click writing a plugin into a second account);
  - the selector before tmux (a profile belongs to one chat).

**ADR 0017 amendment.**
- The rule covered only a path charter creates that carries credentials.
- `charter.local.toml` carries none, and `charter init` writes its ignore line anyway, because
  that file's whole meaning is "not committed".
- If git tracks it, or would not ignore it, charter refuses its profiles rather than trusting the
  operator to notice.

**ADR 0018 amendment.**
- Charter draws in a pane only while no harness has ever run in it.
- The selector is charter's, drawn before the pane's harness exists. Once a harness has run
  there, the ADR holds unchanged, its two reading moments included.
- On charter's own server the window's first command is the launcher. In an operator's tmux the
  launcher is what `respawn-pane` starts after the placeholder (Ruling 4).

**The phase-5 line.** Under "Two chats on the same harness share that harness's credentials.
Charter cannot separate them and does not pretend to (§4).", add:

> *Superseded 2026-09-11 by harness profiles (`docs/superpowers/specs/2026-09-11-harness-profiles.md`):
> two profiles of one kind can hold two logins.*

**News review.**
- The headline names the failure.
- Every claim is checkable: the file name, the commands, the refusal sentences.
- The limits sit in the same breath: no credentials; a wrapper can still carry one; `.charter/`
  is writable by a chat.
- `adopt: reinit` stays.

### Test cases — `tests/test_the_profile_words_are_written_down.py`

`unittest.TestCase`. It reads files off `Path(__file__).resolve().parents[1]`, never through
`config`.

- `test_context_defines_the_three_terms`: `CONTEXT.md` contains `**Harness profile**:`,
  `**Kind**:` and `**Profile selector**:`, each followed by an `_Avoid_:` line before the next
  `**`.
- `test_the_profile_adr_exists_once`: exactly one `docs/adr/*-a-harness-profile-belongs-to-one-machine.md`,
  first line starting `# `. The slug is pinned, not the number (Ruling 2).
- `test_adr_0017_is_amended_for_the_local_file`: the 0017 file contains `charter.local.toml`.
- `test_adr_0018_is_amended_for_the_selector`: the 0018 file contains `no harness has ever run`.
- `test_the_phase5_credentials_line_carries_its_supersession`: the phase-5 spec contains
  `Superseded 2026-09-11 by harness profiles`.

No guard is added. Say so in the PR, and that the sweep has nothing to charge.

### Implementation steps

- [ ] Test first; red.
- [ ] CONTEXT.md, ADR, amendments, phase-5 note, news review.
- [ ] Whole suite; `python3 tools/sweep.py` (expect nothing to sweep); report it.
- [ ] PR.

**Suggested PR title:** Harness profiles are written down: the ADR, the 0017 and 0018 amendments, and the words

---

## Task 7: The proof, on this plane

**Depends on:** Tasks 1–6 on `main`, and a charter built from `main`. This is a checklist the
controller runs with the operator. It is not an agent task, and nothing here is automated.

- **The one human-only step** is `/login` inside `~/.claude-alt`.
- **Everything else** the controller runs and reads back, pasting each command's output into the
  Task 7 issue.
- **A failed box** stops the proof and becomes an issue against the task that owns it.
- **Scope:** one account in two Claude Code config folders, not two accounts (spec, Limits).

### Setup

- [ ] `charter update` (or `uv tool install` from the merged `main`), then `charter version`.
      The controller records the version.
- [ ] `charter reinit` in the plane root. Then `git check-ignore -v charter.local.toml` names
      `.gitignore`, and `git ls-files charter.local.toml` prints nothing.
- [ ] Write `charter.local.toml`:

  ```toml
  [harness]
  default = "claude"

  [harness.claude-alt]
  kind = "claude"
  command = ["claude"]
  env = { CLAUDE_CONFIG_DIR = "~/.claude-alt" }

  [harness.codex]
  kind = "codex"
  command = ["~/.local/bin/codex"]
  ```

- [ ] `charter harness list` shows three rows: `claude` (built-in), `claude-alt`, and `codex`
      from `charter.local.toml`. No `refused:` block. `claude` is marked `*`.
- [ ] `charter doctor`: `harness profiles` OK; `profile claude-alt` WARN naming
      `charter harness install claude-alt` (or "not approved yet"); `profile codex` present.

### `claude-alt` refuses before it is wired, and prints the command

- [ ] In a fresh terminal, `charter claude-alt`. First it asks `run this? [y/N]` with
      `CLAUDE_CONFIG_DIR=~/.claude-alt claude` shown. Answer `y`.
- [ ] It then refuses with `profile 'claude-alt' is not wired … charter harness install claude-alt`.
      No tmux session was started: `tmux -L charter list-sessions` is unchanged.
- [ ] `charter harness install claude-alt`. Output ends `✓ profile 'claude-alt' is wired`.
- [ ] **Operator:** `CLAUDE_CONFIG_DIR=~/.claude-alt claude`, then `/login` to the same account,
      then quit it.

### One workspace, three chats, each started from the selector

- [ ] `charter workspace create proof --vision "harness profiles proof"`.
- [ ] `charter --workspace proof`. The selector shows `claude`, `claude-alt`, `codex`
      (+ `opencode` if installed), cursor on `claude`. Enter.
- [ ] `claude` starts. Inside it, `echo "$CHARTER_HARNESS $CHARTER_HARNESS_PROFILE"` prints
      `claude-code claude`.
- [ ] Press `+`. The selector starts on `claude` (the pressed-from chat's profile). Move to
      `claude-alt`, Enter. Inside, `echo "$CHARTER_HARNESS $CHARTER_HARNESS_PROFILE $CLAUDE_CONFIG_DIR"`
      prints `claude-code claude-alt /Users/aharon/.claude-alt`.
- [ ] Press `+`, choose `codex`. On the first launch it asks `run this?`: answer `y`. If it
      refuses as not wired, run `charter harness install codex` in another terminal, follow the
      Codex-side steps it prints (approve charter's hooks in Codex), and press `+` again. Inside,
      `echo "$CHARTER_HARNESS $CHARTER_HARNESS_PROFILE"` prints `codex codex`, and
      `ps -o command= -p $PPID` shows the parent is `/Users/aharon/.local/bin/codex`.
- [ ] `tmux -L charter show-environment -g | grep -c CLAUDE_CONFIG_DIR` prints `0`, and so does
      `-t proof`.
- [ ] The chat strip shows three tabs. `F2 → chat` lists them with notes `claude`, `claude-alt`,
      `codex`.

### Charter's own refusal shows in each chat

- [ ] In each of the three chats, ask the model to run `cat .charter/vaults/x.json`. Each shows
      charter's own refusal naming the vault guard (`hooks._READ_REASON`), not a harness error.

### Quit and reopen

- [ ] `F2 → charter: quit`, confirm. The shell prints the plane-recorded line naming `charter reopen`.
- [ ] `python3 -c 'import json,sys; m=json.load(open(".charter/frame/reopen.json")); print([(c["chat"], c["profile"]) for f in m["frames"] for c in f["chats"]])'`
      lists the three chats with profiles `claude`, `claude-alt`, `codex`.
- [ ] `charter reopen`. Three chats come back. In each, the same `echo` as above prints the same
      profile. Claude Code's chats resume their conversations.
- [ ] Negative: quit again. Rename `[harness.claude-alt]` to `[harness.claude-other]`, then
      `charter reopen`. It prints `ran on profile 'claude-alt', which this plane no longer
      declares — not reopened`, and reopens the other two. Rename it back; `charter reopen`
      brings `claude-alt` back.

### Close

- [ ] The controller pastes the checked list and outputs into the Task 7 issue, and closes the
      workspace's todos.

---
## Controller rulings

Ruled 2026-09-11 on this plan's 19 open questions. The authoritative text is in
`workspaces/harness-profiles/workspace.md`, *Controller rulings on the implementation plan*;
every task above is written to it. Each entry is the ruling, then the reason it rests on.

1. **Order is 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7, one task at a time.** A probe never runs a declared
   command whose Task 3 launch record does not match; that doctor or selector row says the
   profile is not approved yet, and no probe runs. *Reason:* detection runs the profile's own
   command (`[*command, "plugin", "list", "--json"]`, `[*command, "debug", "config"]`), and
   only Task 3's record stands for the operator's approval of it.
2. **The profile ADR is 0022, pinned by slug.** *Reason:* the chat-handoff plan's Task 6 takes
   0021 and pins that number in its own test.
3. **`pane_current_command` after the `exec` must equal a direct start's; before it, the value
   is recorded for the docs, not a stop criterion.** *Reason:* nothing in `charter/` reads it;
   tests and an operator's `automatic-rename` status bar do.
4. **In the operator's own tmux the `cat` placeholder stays, and the launcher is what
   `respawn-pane` starts.** *Reason:* the placeholder is what lets `remain-on-exit` be set before
   anything in the pane can exit (#384, `layout.window_argv`).
5. **Profile names are `^[A-Za-z0-9][A-Za-z0-9_-]*$`, no dots.** *Reason:* a dot broke tmux
   targets in #695. The workspace alphabet (`instance.WORKSPACE_NAME_RE`) still allows one, so
   the reason is kept and the parity claim dropped.
6. **The selector stays open on a refused row and shows the reason in its footer.** *Reason:* the
   palette closes on Enter over a refused row (`commands_frame._draw_palette`), and closing the
   selector would close the chat. `docs/frame.md` states the difference.
7. **Codex: charter writes the `shell_environment_policy` line under the profile's home and
   prints the Codex-side steps with `CODEX_HOME=` prefixed; "wired" needs all three marks.**
   *Reason:* `codex plugin list` answers the same for an empty home and a wired one, and nothing
   in charter writes `[plugins."charter@charter"]` or hook trust. The wrapper-script limit goes
   in `docs/harnesses.md`.
8. **`charter harness install <name>` resolves a profile first, then a registry name.**
   *Reason:* `cmd_harness_install` takes the registry name today, and profile-first keeps
   `charter harness install codex` working.
9. **`reinit` installs no software.** It wires the file-only kinds per profile and names
   `charter harness install <name>` for a missing Claude Code plugin. `init` and
   `harness install <profile>` install. Codex stays opt-in. *Reason:* `docs/harnesses.md`'s
   standing rule, which `_provision_harnesses` keeps by running only from `init` and
   `doctor --fix`.
10. **The wiring refusal applies to built-ins, and the news entry says so.** *Reason:* a chat
    that looks guarded and is not is the same failure whichever profile started it.
11. **No profile probe runs on any hook path, SessionStart included.** Only a `charter doctor` a
    person runs probes, concurrently, kept only if that adds ≤ 2 s on this plane; otherwise
    rows for declared profiles only. *Reason:* the SessionStart preflight runs `charter doctor`
    under a 20 s timeout at every session start, and a probe costs 215–750 ms and writes into
    the profile's config folder.
12. **A probe that cannot answer refuses the launch (`CANNOT_TELL`), naming the probe to run by
    hand.** *Reason:* an unknown is not a pass (ADR 0009), and no flag launches a profile
    unguarded.
13. **The trust record and the wiring cache are stated as writable by a chat, in the docs and the
    ADR; a launch always probes fresh; no new guard.** *Reason:* both sit under `.charter/`, which
    no guard covers, and a path pattern is host policy (ADR 0014).
14. **Two more refusals, in Task 1:** a `command` whose first word is charter itself, and an
    `env` name starting with `CHARTER_`. *Reason:* the first makes the selector open itself in a
    loop and would falsify the launcher's spawn-guard reason; the second would lie to every
    hook about the harness or the plane.
15. **Reopen's `[harness] default` fallback is deleted, and its two pinning tests are
    rewritten.** *Reason:* a reopened chat is never given another profile, and `_consume` keeps
    a skipped chat in the manifest for a retry.
16. **Dispatch's transcript lookup and persona skill lookup say they answer for the default
    config folder.** *Reason:* neither is about one chat or one profile. `plugincache` reads no
    folder: it runs `claude` and follows the environment Task 4 hands it.
17. **Attach-and-add-nothing applies to selector launches only.** *Reason:* `charter <profile>`
    keeps today's open-a-chat rule where nobody is attached (`_workspace_to_focus`), which a
    test pins on purpose.
18. **A `default` naming a missing profile marks no row, and the cursor goes where `palette.aim`
    puts it.** *Reason:* a cursor on nothing makes Enter do nothing, which `docs/frame.md`
    promises never happens.
19. **A declared replacement of a built-in, in a local file git would commit, refuses that
    name.** *Reason:* running the built-in in its place would run a command the operator
    replaced.

### Review rulings on the plan at `7f98a61`

An adversarial review found 2 blocking and 10 should-fix problems. Each was checked against
`a5aa860`, or `677faac`/`95c2a49` for #970, before it was applied.

- **B1.** Task 2 refuses every declared profile, a built-in's replacement included, with
  `DECLARED_NOT_YET`; Task 3 lifts it. *Reason:* between merges, `main` would otherwise run
  whatever a chat wrote into `charter.local.toml`.
- **B2.** No launch reuses a cached wiring answer; the selector's cache is display-only, and a
  stamp in the future is stale. *Reason:* a chat can write the cache, compute its key and stamp,
  and date an entry ahead.
- **3.** `frame-launch` is unattended unless `--attended`; asking needs a terminal on stdin and
  stdout; `NEEDS_ASKING` has text. *Reason:* a question with nobody at the terminal hangs the
  pane.
- **4.** The ignore check fails closed except on git's own "not a git repository";
  `util.git_path_state` catches its timeout; `util.git_ignores` is untouched. *Reason:*
  `git_ignores` reads every non-zero `rev-parse` as "not a repo", and `commands_secrets` and
  `check_credential_paths` depend on exactly that.
- **5.** opencode is wired only when the shim it names matches charter's bytes. *Reason:*
  `unvouched()` answers `()` for a missing shim.
- **6.** Claude Code detection reads every covering entry, not the first; N3 (ruling 28) then
  decides by the most specific scope. *Reason:* `installed_for` returns the first covering
  entry, and this machine lists disabled and enabled entries together.
- **7.** Where `[shell_environment_policy]` exists without charter's line, `harness install`
  refuses and prints the line. *Reason:* `codex.install()` answers `present` for any such table,
  so the old fix looped.
- **8.** A launch with no frame drops `CHARTER_SESSION_ID`, sets `CHARTER_HARNESS` to the
  launched kind, and keeps `CHARTER_ROOT`, `CHARTER_WORKSPACE` and `CHARTER_PERSONA`. *Reason:*
  the defect is the pair — an inherited session id beside the launched
  `CHARTER_HARNESS=claude-code` makes `hooks._record_harness_session` write into another chat's
  state — while the other three are pins an operator means.
- **9.** Each task names the existing tests it breaks and how they change.
- **10.** Failing tests for Ruling 4's argv, Ruling 18's cursor, a one-profile selector, the Codex
  file under its home, a probe timeout, and `reinit` wiring an opencode shim.
- **11.** References corrected: `ARealTmuxHandsTheHarnessEveryByte` never goes through `_launch`;
  `util.info` writes to stderr; in-process `_claudeguard` yields `UNKNOWN`, not `[]`.
- **12.** Tests that already pass at `a5aa860` are labelled pins; a selector window clears an
  inherited `CHARTER_HARNESS`.
- **13.** `UNKNOWN_PROFILE_KEY` stays, because a typo'd key would launch the wrong account. The
  refusal of other keys in `charter.toml`'s `[harness]` is dropped.
- **14.** `PROFILES` is derived before `HARNESS`.

### Re-review rulings on the plan at `35b1d09` (rulings 26–32)

- **N1 (26).** opencode is wired only when the entry names `home / SHIM_PATH`,
  `shim_is_charters` matches, and `foreign_plugins` is empty. *Reason:* the byte check alone
  passes the documented bypass, a byte-perfect `charter.ts` beside `plugin/aaa_boot.ts`
  (`opencode.py:494-505`, ADR 0015).
- **N2 (27).** Refusals are typed (`launcher.Refusal.kind`), and after a yes the whole chain runs
  again from the top before `exec`, on every path. *Reason:* a yes must never walk past a wiring
  refusal, and text comparison breaks the day a sentence is reworded.
- **N3 (28).** Claude Code is wired by the most specific covering scope, local over project over
  user; D2 records both pairs. *Reason:* "any enabled" fails open when a chat disables charter
  locally over a user enable. The most-specific rule fails closed if D2 cannot settle it.
- **N4 (31).** More existing tests are listed per task, and each task's first step runs the whole
  suite in the background. *Reason:* the lists were a floor that kept missing classes.
- **N5 (29, revised).** A launcher is framed only when tmux says its `os.getpid()` is the
  `#{pane_pid}` of a pane whose window is named for its chat, on that chat's server. It is read
  with one `list-panes -a`, before `exec`. `$TMUX_PANE`, `$CHARTER_SESSION_ID` and charter's
  pane record are never proof, and nothing waits. *Reason:* a model's tool shell inherits the
  chat's `$TMUX_PANE` and session id (measured: shell pid 4707 under `pane_pid` 53118). An
  environment check would let `charter frame-launch` run from it rewrite the chat's profile
  record, which reopen follows.
- **N6.** "Never hangs" was false. Declared-profile real-tmux tests seed records through
  `approve_profile` and fail fast through `assert_approved`. *Reason:* an attended pane with a
  tty asks and waits until the suite is killed.
- **N7 (30).** A pick refused at launch returns to the selector with its reason; only Esc
  closes. *Reason:* a cached "wired" that fails the fresh probe would otherwise close the chat.
- **N8 (32).** `git_path_state` is one `git status` call under `LC_ALL=C`; the preflight runs
  no profile git check; a handoff probes twice, from the target workspace's directory and in
  the pane. *Reason:* English-text matching breaks under a localised git, and every probe or git
  call on a hook path is paid per session.

### What the rulings cost in the code

None is impossible. Four carry a cost the tasks above now pay:

- **Ruling 11 needs the preflight to say it is one.**
  - The SessionStart hook runs `out="$(charter doctor 2>&1)" || printf …` (`hooks/hooks.json`,
    the fourth SessionStart command): the same words a person types, with no flag and no
    marker.
  - A tty test would misread `charter doctor --json` and `charter doctor | less` as the hook.
  - So Task 4 adds `--preflight` to doctor's parser and to that hook command. A
    `charter doctor --preflight` builds every profile row without probing.
  - Changing the hook's command changes its `trusted_hash` in Codex, so a Codex user approves
    that one hook once more.
- **Ruling 10 makes every real launch in the suite refuse under `_claudeguard`.**
  - In-process, `tests/_claudeguard.py` makes `plugincache.available()` answer False, which
    Task 4 reads as `UNKNOWN_STATE` and refuses. In a child process its fake `claude`, first on
    `PATH`, answers `[]` to `plugin … --json`, which reads as unwired.
  - In-process tests stand in `wiring.refusal`.
  - A pane is a child process no patch reaches, so a real-tmux test declares a profile whose
    `command` is its own recorder, which answers `plugin list --json` with a wired entry.
- **Building on #970's resolver changes its signature.** `claude_code.config_home()` and
  `global_config_file()` read `os.environ` and nothing else. Task 4 gives both an optional `env`
  mapping, defaulting to `os.environ`, so a profile's folder is resolved by the same rule and no
  second resolver exists.
- **Real-tmux tests that patch `ClaudeCodeHarness.binary` stop controlling the pane** (Task 2).
  The launcher resolves the profile in its own process, so those tests put their recorder first
  on the tmux client's `PATH` as `claude` instead.
- **Ruling 29's pid proof is a guard rail, not a boundary.** A process that deliberately starts
  its own tmux pane — with a window named like a chat, or an `@charter_chat` set to one —
  passes it. On the operator's tmux the proof is the option rather than the name (ruling 33),
  because a hook or `allow-rename on` output can rename a window there. The docs and the spec
  state it.

### Third re-review rulings on `ac9088c` (rulings 33–36) and nits

- **S1 (33).** Charter's own server proves the chat by window name, and the operator's tmux by
  `@charter_chat`; both need `#{pane_pid} == os.getpid()` on a live pane. An unproven claimed
  chat prints one line. *Reason:* `_CHAT_OPTION`'s note calls the name only a label, and a
  silent downgrade drops the session and resume ids with no word.
- **S2 (34).** The ignore check is `git --no-optional-locks status`. *Reason:* a plain status
  takes `index.lock`, and this check runs often enough to break a concurrent commit (#917).
- **S3 (35).** `framed_chat()` runs first in `cmd_frame_launch`, and every profile-derived text
  charter shows passes through `contain`. *Reason:* the name proof holds only before the pane
  prints anything, and a control byte in a chat-writable `command` could redraw the approval
  prompt.
- **S4 (36).** D2(a) measures settings-file disables, and detection reads `enabledPlugins` from
  the covering settings files when the list does not reflect them. *Reason:* covering entries
  are install records, and a `false` written to `settings.local.json` creates none.
- **Nits.**
  - N2a: only `KIND_ASK` defers to the pane.
  - N2b: a record that cannot be written refuses, and never asks again.
  - N6: `approve_profile` is a Global Constraint.
  - N7: an `execvpe` failure after `on_exec` undoes it and exits with the refusal.
- **Corrected report, not plan:** the N1 quote "`shim_is_charters` said True throughout" is
  verbatim at `opencode.py:509`. An earlier report called it paraphrased; the plan never said
  so.
