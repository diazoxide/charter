# One exit gate — a harness that ends keeps its tab, and every tab names one harness session

> For agentic workers: REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/exit`, a double Ctrl+C or a crash no longer destroys a chat, and charter has one way
out.
- **One way out:** `F10`, the identity row's `F10 close`, and the same two `F2` rows open one
  menu. *Close charter (keep chats running)* detaches this terminal. *Close charter and stop all
  chats…* confirms, records and stops.
- **Every tab names exactly one harness session**, so a tab that ended can offer resume.
- **A chat id is never handed out twice**, so a new chat inherits nothing.
- **A tab can carry a title.**

**Architecture:**
- **Ids.** `state.new_chat_id` starts above a per-prefix high-water mark kept in
  `.charter/frame/chat-ids.json`, and above every trace an id leaves. The `mkdir` claim stays
  the exclusion.
- **The link.** `harness/base.Harness` gains the session members. `frame/launcher.py` adds the
  session and resume arguments at the `exec`, from the chat's own record, so none of them
  crosses tmux. Hooks record what Codex and opencode report.
- **An exit.** `pane-died[1]` stops being `kill-window` and runs `charter frame-ended`
  (`frame/ended.py`). A clean exit respawns the pane into the selector with a resume row. A
  crash leaves the dead pane and opens a drawer beside it.
- **The gate.** `frame/gate.py` is the menu, `tabmenu.py`'s shape. `conf_text` binds `F10`. The
  identity row gains a gate door.

**Tech Stack:**
- Python ≥ 3.11 stdlib, and stdlib `unittest`.
- tmux: 3.7c measured, the 3.2 floor at `~/.local/share/charter-testing/tmux-3.2`, and CI's image
  tmux (3.4).
- Claude Code, codex-cli and opencode at the versions this machine runs. Step 0 records them.

**Spec:** `docs/superpowers/specs/2026-09-15-one-exit-gate.md` (binding). The decisions and
their evidence: `workspaces/harness-profiles/refs/exit-gate-decisions.md` in the plane.

**Line anchors** are against `origin/main` @ `5ad755d` (0.62.0). Open PRs #1100 and #1103 and
every task here move them, so re-anchor by symbol name, never by number.

## Global Constraints

Every task's requirements include this section.

**Carried over from the harness-profiles plan:**
- stdlib only, Python ≥ 3.11; no new runtime dependency.
- **Tests are stdlib `unittest`, and they fail first.** Every behavioural change lands with a
  test that is red without it. They use the isolation helpers CONTRIBUTING names:
  - `tests._isolation.PersonaIso`/`PlaneIso`;
  - `_planeguard` (including `RealTmuxReach`, `RealForgeReach` and `BackgroundGrandchild`);
  - `_envguard`, `_ttyguard`, `_gitguard`, `_tmuxreap`, `_claudeguard` and `_execguard`.
- `charter/frame/tmuxctl.py` is the only module that calls tmux (ADR 0018).
- Charter never parses the harness pane. It draws in a chat pane only where ADR 0018 allows,
  as task 2 amends it.
- **No version bump and no tag.**
  - Each task's news entry is its own file, `docs/news/unreleased-<slug>.md`.
  - Its frontmatter is flat `key: value` with unquoted values, and only the six keys
    CONTRIBUTING lists.
  - No test names it by filename (`test_news_gate.test_no_test_opens_an_entry_by_its_staged_name`).
- Docs move with the code, in the same PR.
- Comments explain why.
- Every state write goes through `config.write_for` / `config.replace_for` /
  `config.private_mkdir` / `config.claim_private_dir`.
- **A hook never breaks a turn.** New hook-reachable code is best-effort, in
  `hooks._record_harness_session`'s `except Exception: pass` shape.
- **Real-tmux tests clean up after themselves.** Every socket is named with
  `tests._tmuxreap.name("<slug>")`: lowercase letters, digits and single hyphens, never
  `tmux-3.2`. It is killed and unlinked in cleanup.
- A new `mock.patch.dict(os.environ, …)` passes `clear=True` or states every value it depends
  on.
- **Every refusal follows CONTEXT.md's Prose rules:** it says the rule worked and names the fix
  in the same breath.
- **A real-tmux test that launches a harness** puts a recorder first on the tmux client's `PATH`
  under the kind's name and hands the server `CHARTER_ROOT`. That is how
  `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py` does it. A declared
  profile is seeded with `tests._isolation.approve_profile` and checked with `assert_approved`.
- Kill only the PIDs you started. Never `pkill -f`: sibling agents run the same suites.

**The operator's standing rules on this machine.** They replace the harness-profiles plan's
*run the whole suite before each PR* and *run `tools/sweep.py`*.
- **No full local suite and no local sweep.**
  - Locally a task runs only the modules it writes or changes:
    `( unset TMUX TMUX_PANE; python3 -m unittest tests.<module> … )`.
  - The whole suite runs on CI's four `test (3.x)` jobs, and the guard sweep on `sweep.yml`.
    Each PR reports both from the run's own summary.
  - `tests/test_plane_spawn_guard.py` scans the tree statically, so a task that adds a spawn or
    an exec runs that module locally too.
- **Never touch the operator's tmux servers.**
  - Nothing a task runs addresses `-L charter`, the default socket, a `charter-plane-*` socket
    this plane's frames run on, or the server `$TMUX` names.
  - `tests._planeguard.RealTmuxReach` refuses those for a test's child.
  - A measurement by hand uses a throwaway `-S /tmp/cp-t<N>-<slug>/s` socket started with
    `-f /dev/null`, run with `TMUX` and `TMUX_PANE` unset. A socket path under the session
    scratchpad passes tmux's 104-byte limit.
- **Every guard is pinned by a test that goes red without it.**
  - CI's sweep reports survivors for lines under `charter/`. Each gets a test or its line
    deleted, and the PR says which.
  - A guard added under `tests/` is never mutated by the sweep, so it gets a hand deletion
    check: delete the line in a scratch copy, run the covering class. The PR names the class
    that went red.
- **Anything a chat can write is contained before charter shows it or hands it on** (ruling 35).
  - That covers a title, a brief's first line, a harness session id, a transcript path, and
    every hook payload field.
  - Display goes through `contain.readable`/`contain.one_line`.
  - A value is held to its shape before it reaches an argv.
  - A path is only ever `stat`ed.
- **An ADR amendment ships with the code that first relies on it** (ruling 44): ADR 0024 in
  task 1, the ADR 0018 amendment in task 2.
- **Scratch files are namespaced per task** (`/tmp/cp-t1-…`, `<scratchpad>/t1/…`): parallel agents
  on this machine share one scratchpad and clobber each other's files.
- **Commit messages and PR bodies go through a file** (`git commit -F`, `gh pr create
  --body-file`): the vault guard reads the whole tool call.

**Added for this feature:**
- **`layout.CARRIABLE` is unchanged, and nothing new crosses tmux but flags and closed-alphabet
  ids.**
  - A title, a harness session id and a transcript path never ride a `-e`, a launcher argv or
    `set-environment`. The launcher reads them from the chat's record at the `exec`.
  - The new words on a tmux argv are `--resume`, `--ended`, `--gate` and chat ids already held
    to `chats.ID_RE`, plus `#{client_name}`, which tmux expands for itself.
- **A tmux hook or bind action is a constant string** (`_pane_died_write_hook_argv`'s docstring,
  `commands_frame.py:1407-1412`). It holds formats tmux expands and variables the shell expands;
  charter interpolates nothing into it.
- **Charter restarts nothing by itself and never types into a harness** (ADR 0018 as task 2
  amends it). No path starts a harness after an exit without an operator's Enter, and no
  `send-keys` reaches a chat pane. Task 2 and task 3 pin both by recording every `tmuxctl.run`
  argv.
- **Nothing reads a harness's pane to decide.**
  - The ended step decides from `state.exit_code` and the link.
  - `_pane_last_words` and `_capture_transcript` keep the two moments ADR 0018's 2026-09-01
    amendment gives them.
  - Resume existence is a `stat` of a path the harness named.
- **Nothing parses a chat id beyond what `chats._order` already reads**, which is the ordinal
  after the last dot.
- **Open PRs #1100 and #1103 touch the same functions** (`_launch`, `launcher.py`, `chats.py`,
  `slots.py`, the reopen path). Before each task starts, run `git fetch` and re-read whichever of
  them has merged, then re-anchor.

## Order and parallelism

**1 → 2 → 3 → 4, one task at a time, each its own reviewed PR** (spec, *Build order*).

- **Task 1** has no dependency.
- **Task 2** needs task 1 on `main`:
  - kept ids, for reopen;
  - the link and `leave.conversation_exists`, for the resume row;
  - `launcher.attempt(..., resume=)`.
- **Task 3** needs tasks 1 and 2: it composes the name task 1's launcher passes, and draws it on
  task 2's resume row.
- **Task 4** needs task 2, because its confirmation lists ended tabs, and task 3, because its rows
  name titles.

**The spec's open questions, and where each is answered before code:**

| Open question | Answered in | By | Default if nobody rules |
|---|---|---|---|
| 1. opencode's chat id on the hook path (#946) | task 1, before code | controller | (A): resolve by `$TMUX_PANE` against charter's pane records |
| 2. The five harness facts decision 10 rests on | task 1, Step 0 | the measurement | a harness whose reading fails gets `resume_argv → None` |
| 3. Which terminal the button's *Close charter* detaches | task 4, Step 0 (G3) | the measurement | detach every client of that workspace's session, and say so |
| 4. `pane-died` on Linux for an empty status and signal | task 2, CI's real-tmux run | controller, before task 2 merges | none — task 2 does not invent a fallback |
| 5. `charter frame -- <cmd>` keeps today's ending | task 2, before code | operator | yes |

## Measured while writing this plan

Nothing was run. The plan's author read code only, and the facts it rests on are in the
decisions file:
- `claude --help`: `-n/--name`, `--session-id`, `-r/--resume`, `/rename`.
- `codex resume [SESSION_ID]`.
- `opencode -s/--session`, `session_rename` on Ctrl+R.
- Measured on 3.7c: `has-session -t default.1` → `can't find pane: 1` (#1097).

Every reading a task needs is its own Step 0.

---

## File Structure

Created:

| Path | Task | Responsibility |
|---|---|---|
| `charter/frame/ended.py` | 2 | The ended step: prove the dead pane, record `ended`, respawn into the selector or open the drawer; the drawer's rows and what each does; `cmd_frame_ended`. |
| `charter/frame/rename.py` | 3 | The one-line title input (`Rename`, a `palette.Palette` whose query is the value), `normalized`, the rename doorway rows, `first_line_title`. |
| `charter/frame/gate.py` | 4 | `GATE_KEY`, the gate's two rows, the stop doorway, the detach with the presser's client, `draw`. |
| `docs/adr/0024-a-chat-id-names-one-chat-and-one-harness-session.md` | 1 | The id and link decisions (the spec's draft, with Step 0's readings). |
| `docs/news/unreleased-a-chat-id-is-never-handed-out-again.md` | 1 | News. |
| `docs/news/unreleased-an-ended-harness-keeps-its-tab.md` | 2 | News. |
| `docs/news/unreleased-a-tab-can-carry-a-title.md` | 3 | News. |
| `docs/news/unreleased-one-gate-closes-charter.md` | 4 | News. |
| `tests/test_a_chat_id_is_never_handed_out_again.py` | 1 | Counter, traces, lock, scans, kept ids on reopen, #1101's repro, the ADR pin. |
| `tests/test_a_tab_is_linked_to_one_harness_session.py` | 1 | Harness members, the launcher's session arguments, what hooks record, resume existence, the record. |
| `tests/test_a_launch_hands_the_harness_its_session_on_a_real_server.py` | 1 | Real tmux: the id reaches the harness argv and not tmux's; a reopened chat keeps its id. |
| `tests/test_an_ended_harness_keeps_its_tab.py` | 2 | The hook, the ended step, the selector after an exit, the drawer, marks, close without asking, quit and reopen, exit codes, the operator's tmux, the ADR pin. |
| `tests/test_an_ended_harness_keeps_its_tab_on_a_real_server.py` | 2 | Real tmux: a clean exit, a crash, a signal death, the last chat, the operator's tmux. |
| `tests/test_a_tab_can_carry_a_title.py` | 3 | Storage and containment, the strip, rename from the tab menu and `F2`, `+`, a handoff, `--name`, the record. |
| `tests/test_one_gate_closes_charter.py` | 4 | The bind, reserved keys, the menu, the detach resolution (#1097), the button, the `F2` rows, the operator's tmux. |
| `tests/test_the_gate_detaches_a_real_client.py` | 4 | Real tmux over ptys: `F10` detaches only the presser; `F2 → detach` detaches again; a real click on the button opens the gate. |

Modified:

| Path | Task | Change |
|---|---|---|
| `charter/frame/state.py` | 1, 2, 3 | Counter, traces, lock, `claim_chat_id`, session id shape, `record_conversation`, `chat_in_pane` (1); `drawn`, `ended`, `drawer` marks (2); `title` (3). |
| `charter/harness/base.py`, `claude_code.py`, `codex.py`, `opencode.py` | 1 | The session members and their overrides. |
| `charter/frame/launcher.py` | 1, 2, 3 | `session_argv`, `attempt(resume=)`, `argv(resume=)` (1); `argv_select(ended=)`, `resume_row`, Esc closes an ended tab, `_picked` clears ended (2); `session_name` (3). |
| `charter/hooks.py` | 1 | `_record_harness_session` gated on the registry and recording `conversation`; `_record_reported_session` for opencode. |
| `charter/frame/leave.py` | 1, 2, 3, 4 | `resumable_harness` from the registry, `conversation_exists`, `Doomed.conversation`, resume clause and summary (1); `Doomed.ended`, `needs_confirming`, `CLOSE_NOW_ID`, the ended note (2); `Doomed.title`, `title()` (3); `OPEN_QUIT`'s words (4). |
| `charter/frame/reopen.py` | 1, 2, 3 | `Chat.conversation` (1), `Chat.ended` (2), `Chat.title` (3); the recycled-ordinal docstrings (1). |
| `charter/commands_frame.py` | 1, 2, 3, 4 | Restore claims the kept id, `_resumes`, `_restore_recorded_chat`, `_reopen_one`/`_reopen_args`, `_record_the_plane` (1); the ended hook, drawn mark, check-after-mark, operator-tmux loop, `cmd_close`, reopen ended (2); `cmd_rename`, palette doorway, handoff title, restore title (3); `conf_text`'s `F10`, `cmd_palette`/`_open_palette` `--gate` (4). |
| `charter/frame/selector.py` | 2, 3 | Resume row, `Choice.resume`, ended footer (2); title row (3). |
| `charter/frame/chats.py` | 2, 3 | `Chat.ended` (2); `Chat.title`, `label_of` (3). |
| `charter/frame/slots.py` | 2, 3, 4 | `ENDED_MARK` (2); strip labels (3); `GATE_BUTTON`, `_Doors` gate columns (4). |
| `charter/frame/tabmenu.py` | 2, 3 | Close-now row on an ended tab (2); rename row (3). |
| `charter/frame/builtin_actions.py` | 4 | `_detach(fid, client)` (#1097), the detach row's words. |
| `charter/frame/builtins.py` | 4 | `_strip_events` opens the gate from a gate door. |
| `charter/frame/choose.py` | 3 | The chat picker names titles. |
| `charter/instance.py` | 4 | `F10` reserved in `component_arrangement`; a hotkey equal to it refused. |
| `charter/cli.py` | 1, 2, 3, 4 | `frame-launch --resume` (1); `frame-ended`, `frame-launch --ended`, `frame-palette --ended` (2); `frame-rename` (3); `frame-palette --gate --client` (4). |
| `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md` | 2 | The 2026-09-15 amendment. |
| `CONTEXT.md` | 1, 2, 3, 4 | **Chat** (1), **Ended tab** (2), **Title** (3), **Exit gate** (4). |
| `docs/frame.md`, `docs/harnesses.md`, `docs/control-plane.md` | 1–4 | Docs with each task. |
| `docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md` | 2 | Dated notes at §4j and §5. |

There is no new `docs/*.md` page, so `pyproject.toml`'s force-include set does not change
(`test_docs_show.TestPagesShip.test_nothing_else_is_force_included`).

---

## Task 1: A chat id is never handed out again, and every tab is linked to one harness session

**Depends on:** nothing. **Closes:** #1101. **Nothing about ending a chat changes here.**
`pane-died[1]` is still `kill-window`, and no surface offers anything new except resume for Codex
and opencode.

### Step 0 — Measure first. A failed reading changes what a harness is offered, never the design.

**Where.** Throwaway directories under `/tmp/cp-t1-*`, with a SessionStart hook that dumps its
stdin to a file. Claude Code uses a throwaway `CLAUDE_CONFIG_DIR` logged in to the operator's
account. Codex uses a `CODEX_HOME` and opencode an `XDG_CONFIG_HOME` holding charter's wiring.
Each reading below spends one short prompt. The implementer asks the controller before spending
them and records the versions.

| # | Reading | Pass when |
|---|---|---|
| C1 | `claude --session-id <uuid4> --name "t1 · beta.1"`, then quit before any prompt | the SessionStart payload's `session_id == <uuid4>`; it carries `transcript_path`; record whether that path is a file at SessionStart |
| C2 | the same, after one prompt | `transcript_path` is a file |
| C3 | `claude --resume <uuid4> --name "t2 · beta.1"` | the conversation comes back; record the SessionStart `session_id` (same, or new) and whether `/resume`'s list shows `t2 · beta.1` |
| C4 | `claude --resume <uuid4> --session-id <other>` | recorded only — charter never combines them |
| X1 | `codex`, one prompt | SessionStart payload carries `session_id` and `transcript_path`; the path is a file after the prompt |
| X2 | `codex resume <X1's session_id>` | that conversation comes back |
| O1 | `opencode`, one prompt that runs one tool | the shim's `tool.execute.before` payload carries `session_id` (`input.sessionID`) |
| O2 | `opencode -s <O1's id>` | that conversation comes back |

**Stop rule, written into the dispatch.**
- A failed C-reading stops the task and goes to the controller. Claude Code's chosen id is the
  decision's ruling, and a failure changes the spec.
- A failed X- or O-reading does not stop the task:
  - that harness's `resume_argv` answers `None` in this task;
  - the table goes into the PR and into `docs/harnesses.md`;
  - the spec's open question 2 is updated.
- The design is never bent around a failed reading.

**Before code:** the controller's ruling on the spec's open question 1 (opencode's chat on the
hook path) is in the task issue. This task implements (A) unless it says (B).

### Files

- `charter/frame/state.py`:
  - `new_chat_id` (`:193-279`): start from `highest_ordinal(prefix) + 1`, under the lock; claim
    upward with `config.claim_private_dir` for at most `_CHAT_ORDINAL_MAX` attempts; `_record_claim`;
    raise the mark; release. `_CHAT_ORDINAL_MAX`'s note (`:132-141`) becomes a bound on attempts.
  - New beside it: `CHAT_IDS`, `CHAT_IDS_LOCK`, `_locked()` (a context manager over `fcntl.flock`),
    `_mark(prefix)`, `_raise_mark(prefix, n)`, `_traces(prefix)`, `highest_ordinal(prefix)`,
    `ordinal_of(chat)`, `claim_chat_id(chat)`.
  - `record_harness_session` (`:692-746`): refuses a sid outside `SESSION_ID_RE`.
  - After `kept_harness_session` (`:821-843`): `CONVERSATION_FILE`, `record_conversation`,
    `conversation`, `clear_conversation`. `clear_shape`'s tuple (`:1852`) does not list it: the
    conversation is the link's, not the gauge's.
  - `chat_in_pane(pane, server)` after `harness_pane` (`:676`).
  - Docstrings that call an ordinal recycled:
    - `new_chat_id`'s rename paragraph (`:235-241`, kept);
    - `clear_exit` (`:591-596`), `clear_shape` (`:1781+`), `clear_respawn` (`:2433+`);
    - `_forget_session` (`:2625-2636`);
    - `reap`'s empty-directory note (`:2819-2823`).
- `charter/harness/base.py` — after `first_message_argv` (`:284+`): `chooses_session_id`,
  `names_its_transcript`, `session_flags`, `reports_session_at`, `new_session_argv`, `resume_argv`.
- `charter/harness/claude_code.py` (`ClaudeCodeHarness`, `:244`), `codex.py` (`CodexHarness`,
  `:141`), `opencode.py` (`OpenCodeHarness`, `:688`) — the overrides.
- `charter/frame/launcher.py`:
  - `argv` (`:192-204`) takes `resume: bool = False` and adds `--resume` before `--`.
  - New `session_argv(p, fid, *, resume, rest)` and `RESUME_GONE`.
  - `attempt` (`:463-498`) takes `resume: bool = False`: the session arguments go before `rest`,
    and `on_exec` also records the chosen id and clears `conversation`; the undo restores both.
  - `cmd_frame_launch` (`:772-813`) passes `args.resume`.
- `charter/cli.py` — `frame-launch` (`:1195-1210`): `--resume`, `action="store_true"`.
- `charter/hooks.py`:
  - `_record_harness_session` (`:5976-6027`): the gate is the registry's
    `reports_session_at == "sessionstart"`, and the chat's recorded kind must equal
    `$CHARTER_HARNESS`; it also records `transcript_path` when `names_its_transcript`.
  - New `_record_reported_session(data)`, called beside every `_turn_bump()` call
    (`grep -n "_turn_bump()" charter/hooks.py`), for `reports_session_at == "tool"`.
- `charter/frame/leave.py`:
  - `Doomed.conversation: str = ""` (`:51-116`); `plan` fills it (`:181-204`).
  - `resumable_harness` (`:409-426`) asks `registry.get(name).resume_argv`; its docstring's refusal
    of a member is replaced by the reason the bar is now met.
  - New `conversation_exists(harness, link, conversation)`.
  - `_resume_clause` (`:391-406`) and `summary`'s `back` (`:292`) ask it.
  - `_group`'s note (`:249-254`).
- `charter/frame/reopen.py` — `Chat.conversation: str = ""` (`:92-160`); `_chat`'s key list
  (`:360-362`); `Chat.chat`/`Chat.workspace` notes (`:104-117`).
- `charter/commands_frame.py`:
  - `_launch` (`:5880-5893`): when `_reopening(args)` is not `None`, `fid = restoring.chat.chat`
    if `state.claim_chat_id(...)`, else `KEPT_ID_TAKEN` and return 1; otherwise `new_chat_id`.
    The launcher argv sites pass `resume=getattr(args, "resume", False)`.
  - `_resumes` (`:10724-10737`): `leave.conversation_exists(c.harness, c.resume, c.conversation)`.
  - `_restore_recorded_chat` (`:10740-10805`): writes `rec.resume` (`record_harness_session`) and
    `rec.conversation` into the claimed directory; its recycled-ordinal paragraph goes.
  - `_reopen_one` (`:11243-11245`, `:11278-11280`): `rest = []`, `resume=_resumes(c)`.
  - `_reopen_args` (`:11306-11319`): `resume`.
  - `_record_the_plane` (`:10368-10376`): `conversation=c.conversation`.
  - Notes at `:6505-6509` and `REOPEN_ON_A_LIVE_PLANE`'s comment (`:10808-10814`).
- `charter/frame/chats.py` — `_MAX_ORDINAL_DIGITS`' note (`:355`): ordinals only grow, and past
  99,999 a tab sorts last.
- Tests: the three Task 1 modules (File Structure) and the existing cases below.
- Docs: `docs/frame.md`, `docs/harnesses.md`, `CONTEXT.md`, `docs/adr/0024-…`, news.

### Interfaces

**Consumes (on `main`):**
- `config.claim_private_dir`, `config.replace_for`, `config.write_for`, `config.SESSIONS_DIR`
- `state.workspace_prefix` (`:82`), `state._root` (`:189`), `state.frame_dir` (`:313`),
  `state._record_claim` (`:282`), `state.harness_pane` (`:676`), `state.frame_server` (`:875`),
  `state.identity` (`:1912`)
- `reopen.path` (`:196`), `reopen.TRANSCRIPT_SUFFIX` (`:71`)
- `launcher.framed_chat` (`:520`), `launcher._handed_over` (`:578`), `launcher._picked` (`:616`)
- `harness.registry.get` (`harness/registry.py:31`)
- `tmuxctl.same_server`, `contain.PATH_DISPLAY_LIMIT`
- `hooks._chat_id` (`:5921`), `hooks._in_a_plane`

**Produces:**

```python
# charter/frame/state.py
CHAT_IDS = "chat-ids.json"      # {prefix: highest ordinal ever handed out}, a FILE in the frame root
CHAT_IDS_LOCK = "chat-ids.lock" # fcntl.flock'd around every read-max-write of CHAT_IDS
SESSION_ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,127}")
CONVERSATION_FILE = "conversation"

def new_chat_id(workspace: str) -> str | None: ...
    # unchanged signature; never an ordinal any trace or the mark still names; None as today,
    # and also when the lock cannot be opened
def claim_chat_id(chat: str) -> bool: ...
    # a restore claims exactly *chat*; False when it is not a chat id, its directory exists, or
    # the claim fails. Raises the mark on success.
def ordinal_of(chat: str) -> int | None: ...    # the decimal digits after the last dot, else None
def highest_ordinal(prefix: str) -> int: ...    # max(mark, traces); 0 for a prefix nothing names
def record_harness_session(fid: str, sid: str) -> bool: ...   # refuses sid outside SESSION_ID_RE
def record_conversation(fid: str, path: str) -> bool: ...
    # absolute, no NUL, len <= contain.PATH_DISPLAY_LIMIT; True when the record changed
def conversation(fid: str) -> str | None: ...
def clear_conversation(fid: str) -> None: ...
def chat_in_pane(pane: str, server: str) -> str | None: ...
    # the chat whose recorded harness pane is *pane* on a server tmuxctl.same_server calls *server*

# charter/harness/base.py — class Harness
chooses_session_id: bool = False
names_its_transcript: bool = False
session_flags: tuple[str, ...] = ()   # words in `rest` that mean the operator named a session
reports_session_at: str = ""          # "sessionstart" | "tool" | ""
def new_session_argv(self, sid: str, name: str) -> list[str]: ...     # [] by default
def resume_argv(self, sid: str, name: str) -> list[str] | None: ...   # None: charter cannot resume it

# claude_code.ClaudeCodeHarness
#   chooses_session_id = True; names_its_transcript = True; reports_session_at = "sessionstart"
#   session_flags = ("--session-id", "--resume", "-r", "--continue", "-c", "--fork-session")
#   new_session_argv -> ["--session-id", sid, "--name", name]
#   resume_argv      -> ["--resume", sid, "--name", name]
# codex.CodexHarness
#   names_its_transcript = True; reports_session_at = "sessionstart"
#   session_flags = ("resume", "fork"); resume_argv -> ["resume", sid]
# opencode.OpenCodeHarness
#   reports_session_at = "tool"; session_flags = ("-s", "--session", "-c", "--continue")
#   resume_argv -> ["-s", sid]

# charter/frame/launcher.py
RESUME_GONE = ("the conversation this chat was linked to cannot be resumed here — starting a "
               "fresh one on profile '{name}'.")
def argv(profile: str, rest: list[str], *, attended: bool, resume: bool = False) -> list[str]: ...
def session_argv(p: profiles.Profile, fid: str | None, *, resume: bool,
                 rest: list[str]) -> tuple[list[str], str]: ...
    # (the words to put before rest, the id on_exec records or "")
def attempt(p, rest, *, fid, attended, resume: bool = False, on_exec=...) -> Refusal | None: ...

# charter/frame/leave.py
def resumable_harness(name: str) -> bool: ...   # registry answer: resume_argv("x", "") is not None
def conversation_exists(harness: str, link: str, conversation: str) -> bool: ...
class Doomed(NamedTuple): ...; conversation: str = ""

# charter/frame/reopen.py
class Chat(NamedTuple): ...; conversation: str = ""

# charter/commands_frame.py
KEPT_ID_TAKEN = ("charter reopen: {chat} is not reopened — a directory for that id is still on "
                 "this plane, so opening it would give two chats one id. It stays recorded; "
                 "run charter reopen again once that chat has ended.")

# charter/hooks.py
def _record_reported_session(data: dict) -> None: ...   # opencode's route; best-effort
```

### Behaviour

**`new_chat_id(workspace)`.**
1. `prefix = workspace_prefix(workspace)`; `config.private_mkdir(root)`. An `OSError` is `None`,
   as today.
2. `with _locked(root):`. The context manager opens `root / CHAT_IDS_LOCK` with
   `os.open(…, O_RDWR | O_CREAT | O_NOFOLLOW, 0o600)` and takes `fcntl.flock(fd, LOCK_EX)`, the
   idiom of `hooks.py:8058`. An `OSError` opening or locking is `None`: an allocation that cannot
   keep the mark monotonic is refused, never done unguarded.
3. `start = highest_ordinal(prefix) + 1`. For `n` in `range(start, start + _CHAT_ORDINAL_MAX)`:
   - `claim_private_dir(root / f"{prefix}.{n}")`;
   - `FileExistsError` → continue; any other `OSError` → `None`;
   - on success, `_record_claim(d)` then `_raise_mark(prefix, n)`, return the name.
4. **The `mkdir` stays the exclusion, and the lock is only what keeps the mark from going down.**
   A racer that is not holding the lock — a charter older than this task, running across the
   upgrade — still cannot share a name, because the kernel picks one `mkdir`.

**`highest_ordinal(prefix)` — the mark and the traces, every time.**
- **The mark:** `json.loads(CHAT_IDS)[prefix]` when it is an `int` ≥ 0. Anything else reads 0 — an
  unreadable mark is not a reason to trust nothing, because the traces still count.
- **Traces**, each a name `f"{prefix}.{digits}"` whose digits parse:
  - every entry of `os.scandir(root)`, directory or file, matched as `{prefix}.{n}` or
    `{prefix}.{n}{TRANSCRIPT_SUFFIX}`. `prefix` holds no dot (`workspace_prefix` maps it to `_`),
    so `rpartition(".")` after removing the suffix answers it;
  - every `chat` string in `reopen.json` read raw — any `version`, any shape that
    `frames[*].chats[*].chat` reaches — ignoring what is not text;
  - every entry of `SESSIONS_DIR` whose name splits as `[prefix, digits, *family]` on `.`.
- **An old `{workspace}-{pid}` frame id carries no dot and never counts.**
- **Cost** is one `scandir` of the frame root and one of `SESSIONS_DIR`, and one JSON read, per
  allocation. It is paid on a chat open, never on a hook or a repaint.

**`_raise_mark(prefix, n)`.** Read `CHAT_IDS` (or `{}`), set `max(existing, n)`, and
`config.replace_for` the JSON with `sort_keys=True`. It is called only under the lock.

**`claim_chat_id(chat)`.** It requires all three of `chats.is_chat(chat)`,
`ordinal_of(chat) is not None`, and a prefix with no dot. Then, under the lock:
`claim_private_dir(root / chat)`, `_record_claim`, and `_raise_mark(prefix, ordinal)`.
`FileExistsError` or `OSError` → `False`, and nothing is written into an existing directory.

**`_launch` on a reopen.** It claims the recorded id before anything else that makes state:
- `_pin_workspace`, `frame_dir(create=True)` and the rest follow unchanged;
- a failed claim prints `KEPT_ID_TAKEN` and returns 1, so `_reopen_one` reports the chat not
  back and `_consume` keeps it;
- `restoring.fid` is the kept id;
- `_restore_recorded_chat` writes `rec.resume` and `rec.conversation` into the claimed directory
  before tmux, beside the persona and brief it already restores.

**`launcher.session_argv(p, fid, *, resume, rest)`.**
1. `fid is None` → `([], "")`. An unframed launch has no chat to link.
2. `h = registry.get(p.harness)`; `None` → `([], "")`.
3. Any word of `rest` in `h.session_flags` → `([], "")`. The operator named a session; the
   harness's SessionStart report is the link.
4. `name = fid`. Task 3 replaces this line with `session_name(fid)`.
5. `resume` and `link := state.kept_harness_session(fid)` and
   `(args := h.resume_argv(link, name)) is not None` → `(args, "")`.
6. `resume` otherwise → `util.err(f"charter: {RESUME_GONE…}")`, then fall through to a fresh start.
7. `h.chooses_session_id` → `sid = str(uuid.uuid4())`; `(h.new_session_argv(sid, name), sid)`.
8. Otherwise → `([], "")`.

**`attempt(…, resume=)`.**
- `words, chosen = session_argv(p, fid, resume=resume, rest=rest)`, and `rest = [*words, *rest]`.
  The words go before `rest`, so an operator's own `-p hi` stays last.
- The `on_exec` it runs is wrapped:
  - read the previous `kept_harness_session` and `conversation`;
  - when `chosen`, write it with `record_harness_session` and `clear_conversation`;
  - call the caller's `on_exec`.
- The undo it returns restores both previous values, then runs the caller's undo.

**Hooks.**
- **`_record_harness_session(data)`:**
  1. `chat = _chat_id()`, and the plane check, as today.
  2. `h = registry.get(os.environ.get("CHARTER_HARNESS", ""))`. `h is None` or
     `h.reports_session_at != "sessionstart"` → return.
  3. `state.identity(chat).get("CHARTER_HARNESS") != h.name` → return. A harness started by hand
     inside another chat's shell inherits that chat's id, and must not write its own session over
     the chat's link.
  4. `sid = data.get("session_id")`: a `str` recorded through `record_harness_session`, which
     holds it to `SESSION_ID_RE`; a change bumps.
  5. When `h.names_its_transcript`, `tp = data.get("transcript_path")`: a `str` recorded through
     `record_conversation`, which holds it to its shape.
- **`_record_reported_session(data)`** (ruling on open question 1, A):
  1. `h = registry.get(os.environ.get("CHARTER_HARNESS", ""))` with
     `h.reports_session_at == "tool"`.
  2. `pane = os.environ.get("TMUX_PANE", "")` held to `tmuxctl.PANE_ID_RE`.
  3. `server` is the first comma field of `$TMUX`.
  4. `chat = state.chat_in_pane(pane, server)`, whose recorded kind must be `h.name`.
  5. `record_harness_session(chat, data.get("session_id"))`; nothing is recorded as the
     conversation.

  `_chat_id()` is never read here: it holds opencode's own id (`opencode.py:332`).
- **Both are wrapped in `except Exception: pass`.**

**`leave.conversation_exists(harness, link, conversation)`:**
```python
h = registry.get(harness)
if not link or h is None or h.resume_argv(link, "") is None:
    return False
if h.names_its_transcript:
    return bool(conversation) and os.path.isfile(conversation)
return True   # opencode: a tool hook reported the session; it names no transcript
```

**What the quit row says.**
- `_resume_clause(c)`: `RESUMES` when `conversation_exists(c.harness, c.resume, c.conversation)`.
- Otherwise the three existing sentences in their existing order. `NO_RESUME_HARNESS` is reached
  only when `resumable_harness` is false, which after this task is only a Step 0 failure.
- `summary`'s `back` counts `conversation_exists`.

### Test cases (write first; each must fail before the code exists)

**`tests/test_a_chat_id_is_never_handed_out_again.py`**

Class `TheCounterOnlyGrows(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_reaped_ordinal_is_not_handed_out_again` | `new_chat_id("beta") == "beta.1"`; `shutil.rmtree` that directory (what a reap does); `new_chat_id("beta") == "beta.2"`. Red at `5ad755d`, which answers `beta.1` |
| `test_the_mark_is_a_file_in_the_frame_root` | after one allocation, `json.loads((config.STATE_DIR / "frame" / "chat-ids.json").read_text()) == {"beta": 1}`, the name spelled literally |
| `test_one_mark_per_prefix_not_per_workspace_name` | `new_chat_id("a.b")`, then `new_chat_id("a_b")` → `"a_b.1"`, `"a_b.2"` |
| `test_workspaces_do_not_share_a_count` | `alpha.1`, then `beta.1` |
| `test_a_lost_mark_falls_back_to_the_traces` | allocate `beta.1`, `beta.2`; delete `chat-ids.json` and both directories; leave `beta.2.transcript` → `beta.3` |
| `test_an_unreadable_mark_still_counts_the_traces` | `chat-ids.json` holds `not json`, a directory `beta.7` → `beta.8` |
| `test_the_mark_never_goes_down` | `{"beta": 9}`, `state._raise_mark("beta", 3)` under the lock → still 9 |
| `test_a_taken_ordinal_is_still_skipped` (pin: passes at `5ad755d`) | a directory `beta.1` and no mark → `beta.2` |
| `test_the_attempt_bound_counts_from_the_start` | mark `{"beta": 20000}` → `beta.20001`. Red at `5ad755d`, which answers `None` |

Class `EveryTraceCounts(PersonaIso, unittest.TestCase)` — the migration, with no `chat-ids.json`:

| Case | Asserts |
|---|---|
| `test_a_chat_directory` | directory `beta.4` → `beta.5` |
| `test_a_transcript` | file `beta.6.transcript` → `beta.7` |
| `test_a_quit_record` | `reopen.json` naming `beta.9` → `beta.10` |
| `test_a_quit_record_this_charter_cannot_read` | `{"version": 99, "frames": [{"chats": [{"chat": "beta.12"}]}]}` → `beta.13` |
| `test_a_session_marker` | `config.SESSIONS_DIR / "beta.3.workspace"` → `beta.4` |
| `test_names_that_are_not_this_prefix_do_not_count` | `betamax.9`, `beta-9`, `beta.x`, `beta.9x` → `beta.1` |

Class `TwoAllocatorsNeverShareAnId(PersonaIso, unittest.TestCase)`:
- `test_concurrent_allocations_are_distinct_and_the_mark_is_the_highest` — eight threads, five
  allocations each, with `concurrent.futures.ThreadPoolExecutor` → 40 distinct ids, and the mark
  is 40.
- `test_the_mark_is_raised_while_the_lock_is_held` — `fcntl.flock` and `config.replace_for` are
  recorded into one log → `LOCK_EX` comes before the replace of `chat-ids.json`, and `LOCK_UN`
  after it. **Red when the lock is deleted.**
- `test_a_lock_that_cannot_be_opened_allocates_nothing` — `os.open` raises for the lock path →
  `None`, and no chat directory exists.

Class `NoScanReadsTheMarkAsAChat(PersonaIso, unittest.TestCase)` — after one allocation and its
directory removed:
- `test_the_quit_scan_skips_both_files` — `leave.plane_chats()` holds neither `chat-ids.json` nor
  `chat-ids.lock`.
- `test_the_tab_scan_skips_both_files` — `chats._by_workspace()` holds neither.
- `test_a_reap_of_any_server_leaves_both` — `state.reap(set(), server=tmuxctl.LEGACY_SOCKET)` and
  `state.reap(set(), server="x")` → both files remain.
- `test_the_transcript_sweep_leaves_both` — `reopen.prune_transcripts(set())` → both remain.

Class `ARestoredChatKeepsItsId(PersonaIso, unittest.TestCase)`:
- `test_claiming_a_recorded_id_makes_exactly_that_directory` — `claim_chat_id("beta.7")` is
  `True`, the directory holds `launcher` with this pid, and the mark is ≥ 7.
- `test_a_directory_that_survived_is_not_adopted` — a directory `beta.7` holding `server` →
  `False`, and its files are byte-identical afterwards.
- `test_a_name_that_is_not_a_chat_id_is_not_claimed` — subTest over `"../x"`, `"beta-7"`,
  `"beta"`, `"beta.7.transcript"` → `False`.
- `test_a_reopen_launch_claims_the_recorded_id` — `_launch` with
  `reopening=Reopening(reopen.Chat(chat="beta.7", workspace="beta", …))`, under
  `TheLaunchOpensWithoutMovingAnyone`'s patches
  (`tests/test_a_chat_opens_in_the_background_with_its_first_message.py:437-461`) →
  `reopening.fid == "beta.7"`, and the recorded `new-window`/`new-session` argv names `beta.7`.
- `test_a_reopen_whose_id_is_taken_is_refused_and_stays_recorded` — a directory `beta.7` exists →
  `_reopen_one` returns `None`; stderr holds `"would give two chats one id"`; after `_consume` the
  manifest still names `beta.7`.
- `test_an_ordinary_launch_never_takes_a_recorded_id` — the manifest names `beta.3`, and no
  directories exist → an ordinary `_launch` allocates `beta.4`.

Class `AClosedChatsLeftoversAreNeverInherited(PersonaIso, unittest.TestCase)` — #1101.
- **setUp:** a chat `default.1` with an identity and a workspace record, `default.1.transcript`,
  and a `reopen.json` naming `default.1`. Then `state.record_closed("default.1")` and
  `shutil.rmtree` of its directory: a close followed by a reap.
- `test_the_next_chat_is_not_the_closed_chats_id` — `new_chat_id("default") == "default.2"`.
- `test_the_new_chat_is_not_offered_the_old_transcript` —
  `builtin_actions._has_transcript("default.2") is False`.
- `test_closing_the_new_chat_leaves_the_old_record` — `commands_frame._forget_transcript("default.2")`
  → `reopen_state.read().all_chats()` still names `default.1`, and `default.1.transcript` exists.

Class `TheRecordIsWrittenDown(unittest.TestCase)` — files read off
`Path(__file__).resolve().parents[1]`, never through `config`:
- `test_the_chat_id_adr_exists_once` — exactly one
  `docs/adr/*-a-chat-id-names-one-chat-and-one-harness-session.md`, whose first line starts with
  `# `. The slug is pinned, not the number.
- `test_the_adr_states_its_costs` — its text holds `forged`, `opencode` and `99,999`.
- `test_context_says_a_chat_id_is_handed_out_once` — CONTEXT.md's `**Chat**:` entry holds
  `handed out once`.

**`tests/test_a_tab_is_linked_to_one_harness_session.py`**

Class `EachHarnessSaysHowItsSessionIsNamed(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_claude_code_takes_a_chosen_id_and_a_name` | `new_session_argv("u", "n") == ["--session-id", "u", "--name", "n"]`; `resume_argv("u", "n") == ["--resume", "u", "--name", "n"]`; `chooses_session_id` |
| `test_codex_resumes_by_the_id_it_reported` | `new_session_argv("u", "n") == []`; `resume_argv("s", "n") == ["resume", "s"]` |
| `test_opencode_resumes_by_the_id_it_reported` | `resume_argv("s", "n") == ["-s", "s"]`; `reports_session_at == "tool"` |
| `test_every_launchable_harness_answers` | for each registered harness with a `cli_name`: `resume_argv` is a list or `None`; `reports_session_at in {"sessionstart", "tool", ""}` |
| `test_resumable_is_the_registrys_answer` | `leave.resumable_harness("codex")`; a stand-in registry entry whose `resume_argv` answers `None` → `False` |

Class `TheLauncherAddsTheSession(PersonaIso, unittest.TestCase)`.
- **Stand-ins:** `charter.frame.launcher.os.execvpe` patched; `launcher.framed_chat` → `"beta.1"`;
  `launcher._approval_refusal` and `wiring.wired_or_refusal` stood in.
- **Setup:** `beta.1` records kind `claude-code`.

| Case | Asserts |
|---|---|
| `test_a_framed_claude_start_gets_a_fresh_uuid_and_its_id_as_name` | `cmd_frame_launch(profile="claude", rest=["--", "-p", "x"])` → the exec argv is `["claude", "--session-id", u, "--name", "beta.1", "-p", "x"]`; `uuid.UUID(u).version == 4`; inside the `execvpe` fake, `state.kept_harness_session("beta.1") == u` |
| `test_an_unframed_start_gets_no_session` | `framed_chat` → `None` → `["claude", "-p", "x"]`, nothing recorded |
| `test_the_operators_own_session_flag_wins` | rest `["--resume", "abc"]` → no `--session-id` word; the link unchanged |
| `test_a_resume_hands_the_link_back` | link `u` recorded, `--resume` → `["claude", "--resume", u, "--name", "beta.1"]` |
| `test_a_resume_with_no_link_starts_fresh_and_says_so` | no link, `--resume` → `--session-id` argv; stderr holds `"cannot be resumed here"` |
| `test_codex_gets_nothing_fresh_and_resumes_by_its_report` | codex kind: fresh → `["codex"]`; link `s` + `--resume` → `["codex", "resume", "s"]` |
| `test_a_fresh_start_forgets_the_old_conversation` | `conversation` recorded → gone inside the `execvpe` fake |
| `test_an_exec_that_raises_restores_the_link_and_conversation` | link `old`, conversation `/c`; `execvpe` raises `OSError` → both restored |
| `test_the_session_never_crosses_tmux` | `launcher.argv("claude", [], attended=True, resume=True)` holds `"--resume"` and no word matching a UUID; a `_launch` with tmux recorded holds `--session-id` in no argv |

Class `WhatAHookRecords(PersonaIso, unittest.TestCase)` — `os.environ` patched `clear=True` with
`CHARTER_SESSION_ID=beta.1`, `CHARTER_ROOT=<plane>` and the kind named per case:

| Case | Asserts |
|---|---|
| `test_claude_code_records_its_id_and_its_transcript` | kind `claude-code`, payload `{"session_id": "u1", "transcript_path": "/abs/t.jsonl"}` → link `u1`, `state.conversation("beta.1") == "/abs/t.jsonl"` |
| `test_codex_records_what_it_reports_at_session_start` | kind `codex` → link and conversation recorded. Red at `5ad755d` (the gate returned) |
| `test_a_report_replaces_a_chosen_id` | link `chosen`, payload `session_id` `reported` → `reported` |
| `test_a_harness_started_inside_another_chats_shell_records_nothing` | identity kind `claude-code`, env `CHARTER_HARNESS=codex` → the link unchanged |
| `test_an_id_that_could_be_read_as_a_flag_is_refused` | subTest over `"-rf"`, `"a b"`, `"x" * 200`, `""` → nothing written |
| `test_a_relative_or_nul_path_is_refused` | subTest over `"t.jsonl"`, `"/a\x00b"`, `"/" + "a" * 5000` → no conversation |
| `test_opencode_is_recorded_from_its_tool_hook_by_its_pane` | env `CHARTER_HARNESS=opencode`, `CHARTER_SESSION_ID=ses_abc` (the shim's overwrite), `TMUX_PANE=%4`, `TMUX=<srv>,1,0`; `beta.1` records pane `%4`, server `<srv>`, kind `opencode` → `hooks._record_reported_session({"session_id": "ses_abc"})` → link `ses_abc` |
| `test_opencode_in_a_pane_charter_did_not_record_writes_nothing` | `TMUX_PANE=%9` → no link |
| `test_the_hook_never_raises` | `state.record_harness_session` raises `RuntimeError` → both functions return `None` |

Class `ResumeIsOfferedOnlyWhereTheConversationExists(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_named_transcript_that_is_a_file_resumes` | claude-code, link, `conversation` names a file in `self.tmp` → `conversation_exists` true, `leave._resume_clause(doomed) == leave.RESUMES`, `commands_frame._resumes(chat)` true |
| `test_a_named_transcript_that_is_gone_reopens_empty` | the file is missing → `NO_RESUME_YET` |
| `test_an_opencode_report_is_enough` | opencode, link, no conversation → resumes |
| `test_no_link_is_never_a_resume` | codex, conversation file present, no link → does not resume |
| `test_the_quit_summary_counts_what_can_resume` | two chats, one with a file and one without → `"1 of 2 can resume"` |
| `test_a_reopen_hands_resume_to_the_launcher_not_rest` | `_reopen_one` with `cmd_launch` recorded → `rest == []`, `resume is True` |

Class `TheLinkTravelsThroughTheRecord(PersonaIso, unittest.TestCase)`:
- `test_a_quit_records_the_conversation` — `_record_the_plane` with a `Doomed(...,
  conversation="/abs/t.jsonl")`, every field by keyword → the literal key `"conversation"` in
  `reopen.json`.
- `test_a_record_from_0_62_reads_as_no_conversation` — a chat without the key →
  `read().all_chats()[0].conversation == ""`.
- `test_a_restored_chat_has_its_link_before_tmux` — inside the `new-window` fake,
  `kept_harness_session("beta.7") == "u"` and `conversation("beta.7") == "/abs/t.jsonl"`.

**`tests/test_a_launch_hands_the_harness_its_session_on_a_real_server.py`** — real tmux.
- **Skip rules, sockets, `_spawn_gather` stand-in, recorder on the client's `PATH` as `claude`,
  and `CHARTER_ROOT`:** exactly as
  `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py`.
- Class `TheSessionReachesTheHarnessAndNotTmux(PersonaIso, unittest.TestCase)`.
- `test_the_harness_argv_carries_the_chosen_id_and_tmux_never_saw_it` — the recorder's argv holds
  `--session-id <u> --name <fid>`, and `display-message -p -t <pane> '#{pane_start_command}'` does
  not hold `u`.
- `test_a_reopened_chat_comes_back_under_its_own_id`:
  - two chats, then `_record_the_plane` and `_stop_chats` as `cmd_quit` runs them;
  - `state.reap`;
  - `_reopen_one` for each → `r.fid == c.chat` for both;
  - the recorder's second argv holds `--resume <u>`.

**Existing tests this task changes** (a floor: CI's four `test (3.x)` jobs name the rest, and each
is changed in this PR):
- `tests/test_frame_state.py::ChatIdIsAllocated` — every case that expects an ordinal back after
  its directory is removed now expects the next one.
  `test_a_taken_ordinal_is_skipped_rather_than_adopted` stays as a pin.
- `tests/test_frame_state.py::AClaimSurvivesASiblingsReap` — re-read. The claim-marker race stays;
  any assertion that the reaped name is claimed again flips.
- `tests/test_a_chats_harness_session_outlives_its_gauge.py::ItDoesNotOUTLIVETheChatItself::test_a_recycled_ordinal_cannot_inherit_the_previous_chats_durable_id`
  — no recycled ordinal exists. Restage it: *a reaped chat's id is never handed out, so its
  durable id has no heir*.
- `tests/test_a_chats_harness_session_outlives_its_gauge.py::AHookIsTheWriterNow` — the case
  asserting a non-Claude harness records nothing flips for Codex.
  `test_a_claude_code_hook_records_the_id_reopen_asks_with` stays.
- `tests/test_a_reopen_says_what_it_cannot_bring_back.py::WhatAReopenPutsBack::test_the_manifest_outranks_a_stale_pointer_left_on_a_recycled_ordinal`
  — restaged on a kept id. The resume-sentence cases for `opencode records no session id` flip.
- `tests/test_a_key_cycles_the_tab_strips_height.py::TheHeightIsRememberedForThisFrameAndNoLonger::test_a_new_frame_claiming_a_recycled_id_does_not_inherit_it`
  — kept as the reopen-into-a-surviving-directory pin, docstring reworded.
- `tests/test_a_second_launch_focuses_instead_of_dragging.py::TheLaunchTakesTheDecision::test_a_focused_launch_claims_no_ordinal_and_makes_no_directory`
  — also asserts `chat-ids.json` was not written.
- `tests/test_a_quit_records_the_plane_before_it_kills.py::WhatIsOnDiskIsAFormatAndNotAnImplementationDetail`
  — gains the `conversation` key.
- `tests/test_the_launcher_becomes_the_profile.py::TheLauncher::test_the_pane_command_resolves_the_profile_by_name`
  — the exec argv now holds `--session-id <u> --name beta.1` before `-p x`.
  `test_the_profile_and_kind_ride_the_exec_not_tmux` is unchanged, because its `rest` carries
  `--resume`.
- `tests/test_quit_and_reopen_on_a_real_tmux.py::ARealQuitStopsRealChats` — re-read for id
  assertions after reopen.
- `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py` — the opencode resume sentence.

### Implementation steps

- [ ] Step 0: C1–C4, X1–X2, O1–O2. Record the table; apply the stop rule.
- [ ] Confirm the controller's ruling on open question 1 is in the task issue.
- [ ] Write the three new modules and the changed cases.
- [ ] Run `( unset TMUX TMUX_PANE; python3 -m unittest tests.test_a_chat_id_is_never_handed_out_again tests.test_a_tab_is_linked_to_one_harness_session tests.test_a_launch_hands_the_harness_its_session_on_a_real_server )`
      — expect import errors and assertion failures; note the counts for the PR.
- [ ] `state.py`: the lock, mark, traces, `new_chat_id`, `claim_chat_id`, `ordinal_of`,
      `record_conversation`, `chat_in_pane`, `SESSION_ID_RE`.
- [ ] Harness members and overrides; `leave.resumable_harness`, `conversation_exists`,
      `_resume_clause`, `summary`, `Doomed.conversation`; `reopen.Chat.conversation`.
- [ ] `launcher.session_argv`, `attempt(resume=)`, `argv(resume=)`; `frame-launch --resume`.
- [ ] `hooks._record_harness_session`'s gate and conversation; `_record_reported_session` beside
      every `_turn_bump()`.
- [ ] `_launch`'s kept-id claim; `_restore_recorded_chat`; `_reopen_one`/`_reopen_args`;
      `_record_the_plane`.
- [ ] Run `grep -rn "recycl\|lowest free\|fresh ordinal\|same NAME back\|same ids back" charter/ tests/ docs/frame.md`
      and reword every hit or restage its test, in both directions (`assertIn` and `assertNotIn`).
- [ ] Re-run the three modules and every changed existing module; green.
- [ ] Hand deletion check: no guard is added under `tests/`; say so in the PR.
- [ ] ADR 0024, CONTEXT.md, docs, news. Commit; push; read the four `test (3.x)` jobs and the
      sweep summary; a test for each survivor.

### Docs and news

- **`docs/frame.md`.**
  - `## Leaving` (`:1188-1193`): a reopened chat comes back under the id it had, and a closed
    chat's id is never handed out again, so no new chat is offered an earlier chat's transcript.
  - The quit example (`:1167-1176`): the opencode row reads `conversation resumes`.
  - `**Resume is Claude Code only…**` (`:1325-1334`) becomes *resume, per harness*:
    - Claude Code by the id charter handed it;
    - Codex by the id its SessionStart reported;
    - opencode by the id its first tool call reported;
    - offered only when the conversation exists;
    - Step 0's failures named.
- **`docs/harnesses.md`** `## What each harness lets charter offer` (`:105`): a resume row per
  harness, with Step 0's versions and readings, and the session-name limit.
- **`CONTEXT.md`** **Chat** (`:83-88`): *handed out once for the life of the plane* and *linked to
  exactly one harness session*.
- **`docs/adr/0024-a-chat-id-names-one-chat-and-one-harness-session.md`**: the spec's draft, with
  Step 0's readings in *Consequences*.
- **News** `docs/news/unreleased-a-chat-id-is-never-handed-out-again.md`:

  ```
  ---
  version: unreleased
  headline: A new chat is never given a closed chat's id — so it no longer inherits that chat's transcript or deletes its quit record — and Codex and opencode chats now resume
  ---
  ```

  Body:
  - the #1101 failure in the operator's terms;
  - what now holds: ids only grow, a reopened chat keeps its id, every tab is linked to one
    session, resume per harness;
  - the limits: an opencode conversation that never ran a tool, and a forged hook.

  No `adopt:`, unless the ruling on open question 1 was (B), which moves the shim and needs
  `adopt: reinit`.

**Suggested PR title:** A chat id is never handed out again, and every tab is linked to the harness session it holds (#1101)

---

## Task 2: A harness that ends keeps its tab

**Depends on:** task 1 on `main`. The operator's answer to the spec's open question 5 (the escape
hatch) is in the task issue before code.

### Step 0 — Measure first. A failed reading stops this task.

**Where.** Throwaway `-S /tmp/cp-t2-<slug>/s` servers started `-f /dev/null`. On tmux 3.7c, and
at the 3.2 floor through a directory holding a `tmux` symlink first on `PATH`: the convention
`tests/test_a_real_click_on_a_real_tab_bar_switches.py:34` records. Both arms:
- **charter's own server layout:** `layout.session_argv` for a first chat, `chat_window_argv` for
  a second;
- **the operator's:** `layout.window_argv`, `_remain_on_exit_argv`, `layout.respawn_argv`.

A recorder stands in for the harness. It exits with `$EXIT_WITH`, or is killed from outside with
`TERM` or `KILL`.

| # | Reading | Pass when |
|---|---|---|
| E1 | `_pane_died_write_hook_argv` + the new `pane-died[1]` constant action, with a stand-in `frame-ended` that appends `#{@charter_chat}` and its pid to a file; recorder exits 0, 3, `TERM`, `KILL` | the stand-in runs once per death with the chat id expanded; the exit file holds 0, 3, 1, 1; the window is still listed |
| E2 | from that stand-in: `layout.respawn_argv(..., harness_argv=<a stand-in launcher>)` against the dead pane | the pane id, window id, `@charter_chat`, `show-hooks -p -t <pane>` (both hooks) and `remain-on-exit` are unchanged; the new process's pid is `#{pane_pid}`; `launcher.framed_chat()` run in it answers the chat on both servers; `$CHARTER_SESSION_ID` in it is the chat |
| E3 | `split-window -t <dead pane> -l 6 <stand-in drawer>` | the split succeeds; the harness pane's `#{pane_dead}` stays 1; after `kill-pane` of the drawer the dead pane is still listed, and by eye still shows its last lines |
| E4 | a client attached over a pty to a session whose only window's pane has ended and been respawned | `attach` has not returned after 3 s, and returns within 1 s of that window's `kill-window` |
| E5 | operator arm: `_wait_for_harness` on the recorder, then `respawn_argv` again, then `_wait_for_harness` | the first returns the code; the second waits on the new process and returns its code |
| E6 | a respawn of a harness pane whose window has panels split off it | every panel is unchanged, and `select-pane -t <harness pane>` still gives it focus |

E1's `TERM` and `KILL` rows also run on CI's Linux runner, as
`AnExitOnARealServer.test_a_signal_death_ends_the_tab` (the spec's open question 4). The PR quotes
that run.

**Stop rule, written into the dispatch.** If any criterion fails on any version or arm, the
implementer:
- stops before any code;
- writes the table of readings (version, arm, value) into the task issue;
- reports to the controller.

### Files

- Create `charter/frame/ended.py` and the two Task 2 test modules.
- `charter/commands_frame.py`:
  - `_pane_died_teardown_hook_argv` (`:1421-1459`) becomes `_pane_died_ended_hook_argv`. Its
    docstring keeps the array-replacement measurement and the install order.
  - `_launch` (`:6245-6263`):
    - the batch installs the ended hook where the teardown hook was;
    - the refusal (`:6362-6374`) becomes `ENDED_HOOK_NOT_INSTALLED`;
    - after `_arm_panel_respawn` (`:6386`): `state.record_drawn(fid)`, then
      `_query_pane_dead_status` once more; a dead pane → `ended.present(fid, socket=socket)`
      in-process.
  - `_launch_in_operator_tmux` (the wait at the end of the function, after `_drop_panels`):
    loop while the window stands. When `_wait_for_harness` returns a code for a drawn, unclosed
    chat, `state.record_exit`, then `ended.present(fid, socket=socket)`, then wait again. The
    early path before `_draw_panels` is unchanged.
  - `cmd_close` (`:10491-10564`): also `state.clear_ended`, `ended.drop_drawer`.
  - `_record_the_plane` (`:10368-10376`): `ended=c.ended`.
  - `_reopen_one` (`:11278-11290`): an ended record → `_reopen_args(..., select=True, ended=True,
    start=p.name)`, with no `resume`.
  - `_launch`'s selector branch: `launcher.argv_select(start, ended=...)`, and for an ended
    restore `state.record_ended(fid)` before tmux, never `record_waiting`.
- `charter/frame/launcher.py`:
  - `argv_select` (`:207-221`): `ended: bool = False` adds `--ended`.
  - `resume_row(fid)`.
  - `_select_in_pane` (`:707-769`) passes `resume=resume_row(fid)` when `args.ended`. A
    `Choice(resume=True)` → `attempt(p, [], fid=fid, attended=True, resume=True, on_exec=…)`.
    Esc with `args.ended` → `commands_frame.cmd_close(SimpleNamespace(chat_id=fid, chat=fid))`,
    never `_close_the_cancelled_chat`.
  - `_picked` (`:616-653`): `clear_ended`, `clear_exit`, `ended.drop_drawer`; the undo writes
    `record_ended` back.
  - `cmd_frame_launch`: `args.ended`.
- `charter/frame/selector.py`: `RESUME_ID`, `Resume`, `Choice.resume`, and `resume=` on `rows`
  (`:200-285`), `opens_on` (`:324-336`) and `pick` (`:361-391`). `FOOTER_ENDED` names Esc as
  closing the tab.
- `charter/frame/state.py` — beside `record_closed` (`:2234`): `record_drawn`/`was_drawn`,
  `record_ended`/`is_ended`/`clear_ended`, `record_drawer`/`drawer`. Each is one file in the chat's
  directory, in `_CLOSED_FILE`'s shape and promise.
- `charter/frame/chats.py` — `Chat.ended: bool = False` (`:72-90`); `roster` (`:425-451`) fills it.
- `charter/frame/slots.py` — `ENDED_MARK = "x"`. `_chats_strip` (`:4577-4603`) hands the ended set
  to the bar alongside the working set (`working_chats`, `:4555`), which draws the mark in the
  spinner's cell and the name dim.
- `charter/frame/tabmenu.py` — `CLOSE_NOW_ID`, `close_now_title`; `catalogue` (`:172-237`),
  `chose` (`:263-301`), `opens` (`:304-326`); the module docstring's row paragraph.
- `charter/frame/leave.py`:
  - `Doomed.ended`; `plan` fills it with `state.is_ended`.
  - `_ended` (`:380-388`) says `ended — comes back ended; resume offered` or `ended — comes back
    ended; nothing to resume`.
  - `needs_confirming`; `CLOSE_NOW_ID`.
  - `open_rows` (`:463-503`) offers the close row as an action when the palette's own chat does
    not need confirming; `is_row` and `goes_through` know it.
- `charter/frame/reopen.py` — `Chat.ended: bool = False`; `_chat` reads `raw.get("ended") is True`.
- `charter/cli.py`:
  - `frame-ended` (`--chat`), added to `_core_commands` (`:865-871`);
  - `frame-launch --ended` (`:1195-1210`);
  - `frame-palette --ended` (`:970-991`).
- `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md` — the amendment, appended.
- `docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md` — dated notes at §4j
  (`:469-474`) and §5 (`:521-522`).
- Docs and news below.

### Interfaces

**Consumes:**
- From task 1:
  - `leave.conversation_exists`;
  - `launcher.attempt(..., resume=)`, `argv(..., resume=)`;
  - `state.kept_harness_session`, `state.conversation`;
  - `reopen.Chat.conversation`.
- On `main`:
  - `layout.respawn_argv(*, socket, harness_pane, env, cwd, harness_argv)`;
  - `commands_frame._frame_identity_env`, `_guest_harness_env`, `_query_pane_dead_status`
    (`:1742`), `_repaint_the_other_strips` (`:10661`), `_as_a_drawer` (`:9358`),
    `_say_on_screen` (`:9713`);
  - `palette.Palette`, `palette.own_the_tty`; `overlay.Row`;
  - `tabmenu.handback` (`:360`);
  - `util.self_relaunch_argv`; `tmuxctl.PANE_ID_RE`.

**Produces:**

```python
# charter/frame/state.py
def record_drawn(fid: str) -> None: ...   # the launch finished laying the chat out
def was_drawn(fid: str) -> bool: ...
def record_ended(fid: str) -> None: ...
def is_ended(fid: str) -> bool: ...
def clear_ended(fid: str) -> None: ...
def record_drawer(fid: str, pane: str) -> None: ...   # refuses a pane outside tmuxctl.PANE_ID_RE
def drawer(fid: str) -> str | None: ...

# charter/frame/ended.py
CLEAN = 0
RESUME_ID, FRESH_ID, CLOSE_ID = "ended:resume", "ended:fresh", "ended:close"
DRAWER_ROWS = 6
def present(fid: str, *, socket: str) -> str: ...   # "selector" | "drawer" | "" (did nothing); never raises
def drawer_rows(fid: str) -> tuple[overlay.Row, ...]: ...
def choose(row, fid: str) -> None: ...
def drop_drawer(fid: str) -> None: ...
def draw(args) -> int: ...          # the drawer pane's own process; always 0
def cmd_frame_ended(args) -> int: ...   # always 0 — a run-shell child's non-zero prints into the pane

# charter/frame/selector.py
RESUME_ID = "resume:"
FOOTER_ENDED = "  up/down move   enter start   esc close this tab"
class Resume(NamedTuple):
    title: str    # "resume <session name>"
    note: str     # "<kind> · session <first 8 of the link>"
class Choice(NamedTuple):
    profile: str
    resume: bool = False
def rows(have, *, cwd, start=None, after=None, resume: Resume | None = None) -> tuple[overlay.Row, ...]: ...
def opens_on(listed, start, *, resume: Resume | None = None) -> str: ...
def pick(*, cwd, root, start=None, after=None, resume: Resume | None = None,
         fd=None, out=None) -> Choice | None: ...

# charter/frame/launcher.py
def argv_select(start: str | None, *, ended: bool = False) -> list[str]: ...
def resume_row(fid: str | None) -> "selector.Resume | None": ...

# charter/frame/leave.py
CLOSE_NOW_ID = "leave:close:now"
def needs_confirming(chat: str) -> bool: ...   # not state.is_ended(chat) and not state.is_waiting(chat)
class Doomed(NamedTuple): ...; ended: bool = False

# charter/frame/reopen.py / chats.py
class Chat(NamedTuple): ...; ended: bool = False

# charter/frame/slots.py
ENDED_MARK = "x"

# charter/frame/tabmenu.py
CLOSE_NOW_ID = "tab:close-now"
def close_now_title(target: str) -> str: ...   # "chat: close <target> — its harness has ended"

# charter/commands_frame.py
def _pane_died_ended_hook_argv(*, socket: str, harness_pane: str) -> list[str]: ...
ENDED_HOOK_NOT_INSTALLED = (
    "charter frame: refusing to attach — the hook that answers this chat's harness ending "
    "failed to install, so an exit later would leave a dead tab charter could offer nothing on. "
    "The harness is still running, detached; reattach manually if you must: "
    "tmux -L {socket} attach -t {session}")
```

### Behaviour

**The hook.** `_pane_died_ended_hook_argv` returns
`tmuxctl.server_argv(socket, "set-hook", "-p", "-t", pane, "pane-died[1]", action)`.
- **The action** is the constant
  `run-shell -b "\"$CHARTER_PY\" -m charter frame-ended --chat \"#{@charter_chat}\""`.
- **Its quoting** is the write hook's (`:1414-1416`): `\\"` and `\\$` escaped for tmux's own
  parse.
- **`-b`**, because the ended step spawns tmux commands, and a `run-shell` without `-b` holds
  tmux's command queue while it runs (`_panel_died_hook_argv`'s note, `:1535+`).

**`ended.present(fid, *, socket)`.** Every step returns `""` on the first thing that is not so.
1. `chats.ID_RE.fullmatch(fid)`.
2. `state.was_drawn(fid)`, `not state.is_ended(fid)`, `not state.was_closed(fid)`.
3. `pane = chats.pane_of(fid)`.
4. One `list-panes -a -F '#{pane_id}\t#{pane_dead}\t#{@charter_chat}'` on `socket`. Its row for
   `pane` must read `1` and `fid`. A live pane, a pane under another chat, or a server that does
   not answer: `""`.
5. `state.record_ended(fid)`, `state.bump(fid)`, `commands_frame._repaint_the_other_strips(fid)`.
6. Branch on the exit code:
   - **`state.exit_code(fid) == CLEAN`** → one `layout.respawn_argv`:
     - `harness_argv=launcher.argv_select(state.profile(fid) or None, ended=True)`;
     - `cwd=state.chat_cwd(fid) or config.ROOT`;
     - `env`: on charter's server
       `commands_frame._frame_identity_env(state.identity(fid))`; on an operator socket
       `_guest_harness_env` of the same.

     `respawn_argv` passes `-k`. Return `"selector"`.
   - **Otherwise** → `split-window -v -l DRAWER_ROWS -t <pane> -P -F '#{pane_id}' -e
     CHARTER_SESSION_ID=<fid> -- <util.self_relaunch_argv("frame-palette", "--ended", "--chat",
     fid)>`. Then `state.record_drawer(fid, <the reported pane>)`, then `select-pane -t <drawer>`.
     Return `"drawer"`.
7. The whole body is in `try`/`except Exception: return ""`. Nothing is ever sent to the harness
   pane but `respawn-pane`, and nothing reads it.

**`cmd_frame_ended(args)`:**
```python
fid = getattr(args, "chat", "") or ""
socket = state.frame_server(fid) or tmuxctl.LEGACY_SOCKET
ended.present(fid, socket=socket)
return 0
```

**The drawer (`ended.draw(args)`)** takes `tabmenu.draw`'s shape:
- `handback(os.environ)`, then a `palette.Palette(catalogue=drawer_rows(fid),
  label=f"chat {fid} ended", mouse=True)`, then `own_the_tty`, then `choose`.
- In `finally`: `drop_drawer(fid)`, and `select-pane -t <harness pane>`.
- It returns 0.

**`drawer_rows(fid)`:**
1. `ended:resume` — present only when `leave.conversation_exists(kind, link, conversation)`, with
   `launcher.resume_row(fid)`'s title and note.
2. `ended:fresh` — `start fresh on <profile>`.
3. `ended:close` — `close this tab`.

**`choose(row, fid)`:**
- `ended:resume` → one `respawn_argv` of the harness pane into
  `launcher.argv(profile, [], attended=True, resume=True)`.
- `ended:fresh` → the same, without `resume`.
- `ended:close` → `builtin_actions._spawn(util.self_relaunch_argv("frame-close", fid, "--chat",
  fid), fid=fid)`.
- `None` → nothing; the tab stays ended and the drawer goes.

**The selector after an exit (`_select_in_pane` with `--ended`):**
- `resume_row(fid)` returns `Resume(title=f"resume {name}", note=f"{kind} · session {link[:8]}")`
  when `conversation_exists`, else `None`. `name` is `fid` here, and task 3 composes it.
- `selector.rows(..., resume=…)` puts `Row(id=RESUME_ID, title=…, note=…)` first. `opens_on`
  answers the resume row when it is present, else the start profile as before.
- `Choice(resume=True)` → `attempt(p_of_chat, [], …, resume=True, on_exec=lambda: _picked(fid, p))`.
- A profile row → `attempt(p, [], …, resume=False, …)`, a fresh link (task 1).
- Esc → `commands_frame.cmd_close(...)` in-process. It returns `selector.CANCELLED_EXIT`.
- The footer is `FOOTER_ENDED`.

**`_launch` on charter's own server.**
- The ended hook sits where the teardown hook was, at `teardown_at`. The refusal to attach when
  it did not install stays, with `ENDED_HOOK_NOT_INSTALLED`.
- In the `code is None` branch, after `_arm_panel_respawn`:
  1. `state.record_drawn(fid)`;
  2. `late = _query_pane_dead_status(socket, harness_pane)`;
  3. `late is not None` → `state.record_exit(fid, late)`, then `ended.present(fid,
     socket=socket)`.
- **Nothing else in `_launch` changes.** When `attach` returns, `state.exit_code(fid)` is the code
  the chat last recorded, else 0. The tail's detach sentence stands.

**`_launch_in_operator_tmux`.** After `_drop_panels`, the single wait becomes a loop. It returns
when the window is gone, with `code` unchanged in meaning:
```python
while True:
    code = _wait_for_harness(socket, harness_pane)
    if code is None or state.was_closed(fid):
        break   # the window went, or the tab was closed
    state.record_exit(fid, code)
    state.record_drawn(fid)
    if not ended.present(fid, socket=socket):
        break   # nothing was offered: close as before
```
The early path, before `_draw_panels`, keeps its `_close_window()` and report.

**Closing.**
- `leave.needs_confirming(chat)` is `not state.is_ended(chat) and not state.is_waiting(chat)`.
- `tabmenu.catalogue(target)` ends in `Row(id=CLOSE_NOW_ID, title=close_now_title(target))` when
  it is false, else in today's close row.
- `chose` spawns `frame-close <target> --chat <fid>` for `CLOSE_NOW_ID`; `opens` answers `None`
  for it.
- `leave.open_rows(fid)` swaps its close doorway for `CLOSE_NOW_ID` on the same rule, and
  `_draw_palette`'s branch for `goes_through(row, CLOSE)` also takes `CLOSE_NOW_ID`.

**Quit and reopen.**
- `leave.plan` fills `ended=state.is_ended(fid)`, and `_record_the_plane` writes it.
- `_reopen_one`, for `c.ended`, builds `_reopen_args(c, harness_name=p.kind, profile=p.name,
  rest=[], reopening=r)` with `select=True, ended=True, start=p.name`.
- `_launch`'s selector branch, for `ended`, records `record_ended` instead of `record_waiting`, and
  hands tmux `argv_select(start, ended=True)`.

**What charter says about the exit code, in `docs/frame.md` only.** No code line changes in
`_launch`'s tail. `attach` returns later, and that is the behaviour change.

### Test cases (write first; each must fail before the code exists)

**`tests/test_an_ended_harness_keeps_its_tab.py`**

Class `TheHookNoLongerKillsTheWindow(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_index_one_runs_the_ended_step` | `_pane_died_ended_hook_argv(socket="s", harness_pane="%1")[-3:]` is `["pane-died[1]", action]` preceded by `"%1"`; `"kill-window" not in action`; `"frame-ended" in action`; `"#{@charter_chat}" in action`; `action.startswith("run-shell -b ")` |
| `test_the_action_is_a_constant` | two sockets and two panes → the same action text |
| `test_the_launch_installs_it_after_the_write_hook` | `_launch` with `tmuxctl.write_all` recorded → the `pane-died[1]` write's index is greater than the `pane-died` write's |
| `test_no_launch_installs_kill_window_on_pane_died` | no recorded argv holds both `pane-died` and `kill-window` |

Class `WhatTheEndedStepDoes(PersonaIso, unittest.TestCase)` — `tmuxctl.run` recorded; `list-panes`
answered per case; `beta.1` records pane `%1`, server `s`, profile `claude`, drawn:

| Case | Asserts |
|---|---|
| `test_a_clean_exit_respawns_the_pane_into_the_ended_selector` | exit 0, row `%1\t1\tbeta.1` → `present` returns `"selector"`; one `respawn-pane` argv on `%1` whose command ends `["frame-launch", "--select", "--attended", "--start", "claude", "--ended"]` (order as `argv_select` builds it); `state.is_ended("beta.1")` |
| `test_a_crash_leaves_the_dead_pane_and_opens_a_drawer` | exit 3 → `"drawer"`; no `respawn-pane`; one `split-window` with `-t %1` running `frame-palette --ended --chat beta.1`; `state.drawer("beta.1")` is the reported pane |
| `test_a_signal_death_is_a_crash` | exit `commands_frame._UNKNOWN_DEATH_CODE` → `"drawer"` |
| `test_a_chat_the_launch_has_not_drawn_is_left_to_the_launch` | no drawn mark → `""`, no tmux write, not ended |
| `test_a_live_pane_is_never_touched` | row `%1\t0\tbeta.1` → `""`, no write |
| `test_a_pane_listed_under_another_chat_is_not_acted_on` | row `%1\t1\tbeta.2` → `""` |
| `test_a_chat_already_ended_is_not_presented_twice` | ended mark → `""` |
| `test_a_closed_chat_is_not_presented` | closed mark → `""` |
| `test_it_reads_nothing_from_the_pane` | `commands_frame._pane_last_words` patched to raise; no recorded argv is `capture-pane` |
| `test_it_never_sends_keys` | across every case above, no recorded argv holds `send-keys` |
| `test_every_other_strip_is_woken` | `beta.2` exists → `state.version("beta.2")` moved |
| `test_the_command_always_returns_zero_and_refuses_an_unusable_id` | `cmd_frame_ended(SimpleNamespace(chat="../x"))` → 0, no tmux call; `present` raising → 0 |

Class `TheLaunchStillOwnsAnEarlyDeath(PersonaIso, unittest.TestCase)` — `_launch` under
`tests/test_frame_launcher.py::Launch`'s patches:

| Case | Asserts |
|---|---|
| `test_an_early_death_is_still_reported_and_its_window_killed` (pin: passes at `5ad755d`) | eager status 127 → the early-death sentence on stderr, `kill-window -t %1` recorded, no drawn mark |
| `test_the_drawn_mark_is_written_before_attach` | inside the `tmuxctl.interact` (attach) fake, `state.was_drawn(fid)` |
| `test_a_death_between_the_hooks_and_the_mark_is_answered_by_the_launch` | `_query_pane_dead_status` → `None`, then `0` → `ended.present` called once, with `fid` |
| `test_an_ended_hook_that_did_not_install_refuses_to_attach` | `write_all` answers rc 1 at the hook's index → no attach; stderr holds `"answers this chat's harness ending"` |

Class `TheSelectorAfterAnExit(PersonaIso, unittest.TestCase)` — `palette.own_the_tty` returns
queued rows, `framed_chat` → `beta.1`, `execvpe` patched, `pane.claim` stood in; `beta.1` is
ended, kind `claude-code`, profile `claude`, link `u`, conversation a file in `self.tmp`:

| Case | Asserts |
|---|---|
| `test_resume_is_the_first_row_and_the_cursor_starts_on_it` | the first `Selector`'s first row id is `resume:`, title `resume beta.1`; `Selector(...).selected().id == "resume:"` |
| `test_no_conversation_means_no_resume_row` | the conversation file removed → no `resume:` row; the cursor is on `profile:claude` |
| `test_resume_execs_the_harness_on_its_link` | resume picked → `execvpe` argv `["claude", "--resume", "u", "--name", "beta.1"]` |
| `test_a_profile_row_starts_fresh_on_a_new_link` | `profile:claude` picked → `--session-id v`, `v != "u"`; `state.conversation("beta.1") is None` inside the fake |
| `test_a_pick_clears_ended_exit_and_the_drawer` | a drawer `%7` recorded, exit 3 → inside the fake: not ended, `exit_code is None`; a `kill-pane -t %7` argv recorded |
| `test_escape_closes_the_tab_for_good` | `own_the_tty` → `None` with `ended=True` → `commands_frame.cmd_close` called with `chat_id="beta.1"`; `launcher._close_the_cancelled_chat` not called |
| `test_escape_at_a_selector_that_never_started_writes_no_mark` (pin) | `ended=False` → `_close_the_cancelled_chat` called, `state.was_closed("beta.1") is False` |
| `test_the_footer_says_escape_closes_this_tab` | the rendered last line holds `esc close this tab` |

Class `TheCrashDrawer(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_three_rows_with_resume_first` | link and conversation file → `[r.id for r in drawer_rows("beta.1")] == ["ended:resume", "ended:fresh", "ended:close"]` |
| `test_two_rows_with_no_conversation` | → `["ended:fresh", "ended:close"]` |
| `test_resume_respawns_the_harness_pane_into_the_launcher` | `choose(resume)` → one `respawn-pane` on `%1` whose command holds `frame-launch --profile claude --attended --resume` |
| `test_start_fresh_respawns_without_resume` | no `--resume` in it |
| `test_close_this_tab_does_not_ask` | `_spawn` recorded with `frame-close beta.1 --chat beta.1`; no `Palette` built |
| `test_the_drawer_goes_whatever_happens` | `own_the_tty` → `None`, then raising → a `kill-pane -t <drawer>` and a `select-pane -t %1` recorded both times |

Class `AnEndedTabIsMarked(PersonaIso, unittest.TestCase)`:
- `test_the_roster_says_ended` — `chats.roster("beta.2")` → the `beta.1` entry has `ended=True`.
- `test_the_strip_draws_the_mark_and_a_click_still_resolves_to_the_chat` — the chats bar
  rendered → `ENDED_MARK` is on the row, and `slots.TABS.tab_at(0, <a column of beta.1's name>)
  == "beta.1"`.
- `test_the_mark_is_one_ascii_cell` — `ENDED_MARK.isascii()` and `tui.width(ENDED_MARK) == 1`.
- `test_a_background_chat_ends_the_same_way` — `present` for a chat opened with `Opening` →
  `"selector"`, ended.

Class `ClosingAnEndedTabDoesNotAsk(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_needs_confirming` | subTest: running → `True`; ended → `False`; waiting → `False` |
| `test_the_tab_menus_close_row_is_an_action_on_an_ended_tab` | `tabmenu.catalogue("beta.1")[-1].id == "tab:close-now"`; `opens(...) is None`; `chose(...)` spawns `frame-close beta.1` |
| `test_a_running_tab_is_still_confirmed` (pin) | not ended → `tab:close`, and `opens` builds a `Palette` over `leave.confirm_rows` |
| `test_the_palettes_close_row_is_an_action_for_an_ended_chat` | `leave.open_rows("beta.1")[-1].id == "leave:close:now"`, and `leave.is_row` of it is true |

Class `QuitAndReopenKeepEndedTabs(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_quit_records_an_ended_tab_as_ended` | `_record_the_plane([Doomed(..., ended=True)], …)` → the literal `"ended": true` in `reopen.json` |
| `test_the_confirmation_says_it_comes_back_ended` | `leave.note(Doomed(..., ended=True, resume="u", conversation=<file>))` holds `comes back ended; resume offered`; without the file, `nothing to resume` |
| `test_the_recorder_writes_ended_too` | `record_the_plane_now("beta.1")` with `_plane_live` stood in → `ended` in the file |
| `test_a_reopen_brings_an_ended_chat_back_at_the_ended_selector` | `_reopen_one(Chat(..., ended=True))` with `cmd_launch` recorded → `select is True`, `ended is True`, `start == "claude"`, no `resume` |
| `test_a_restored_ended_chat_is_ended_before_tmux_and_not_waiting` | inside the `new-window` fake: `state.is_ended("beta.7")`, `not state.is_waiting("beta.7")`; the argv holds `--ended` |
| `test_a_record_from_before_reads_as_not_ended` | no key → `False` |

Class `TheExitCodeArrivesWhenTheTabCloses(PersonaIso, unittest.TestCase)`:
- `test_attach_returning_after_a_close_returns_the_recorded_code` — exit 3 recorded, the chat not
  in `live_after` → rc 3.
- `test_a_detach_is_still_zero_and_says_so` (pin) — the chat live after attach → rc 0 and the
  `detached` line.

Class `InsideTheOperatorsTmux(PersonaIso, unittest.TestCase)` — `_launch_in_operator_tmux` with
tmux recorded:
- `test_an_exit_after_the_draw_presents_and_keeps_waiting` — `_wait_for_harness` → 0, then `None`
  → `ended.present` called once; the function returns after the second wait; no `kill-window`
  between the two waits.
- `test_an_exit_before_the_draw_is_still_an_early_death` (pin) — `_pane_state` dead at the first
  ask → `_close_window` and the report.

Class `TheRecordIsWrittenDown(unittest.TestCase)`:
- `test_adr_0018_is_amended_for_an_exit` — 0018 holds `a harness exit is no longer final` and
  `Charter restarts nothing by itself`.
- `test_context_defines_an_ended_tab` — CONTEXT.md holds `**Ended tab**:` followed by an `_Avoid_:`
  line before the next `**`.
- `test_the_ide_spec_says_4j_is_built` — its §4j holds `Built 2026-09-15`.

**`tests/test_an_ended_harness_keeps_its_tab_on_a_real_server.py`** — real tmux.
- Skip rules and fixtures as `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py`.
- `selector.pick` and `ended.draw` are stood in through a `PYTHONPATH` shim that writes its argv
  to a file and exits 0 without drawing: the approach
  `tests/test_a_new_chat_starts_at_the_profile_selector.py::TheSelectorOnARealServer` records.
- Class `AnExitOnARealServer(PersonaIso, unittest.TestCase)`:
- `test_a_clean_exit_keeps_the_window_and_respawns_the_same_pane` (E2) — the recorder exits 0 →
  within 5 s: the window is still listed, `#{pane_id}` unchanged, the shim's record holds
  `--select` and `--ended`, and `state.is_ended(fid)`.
- `test_a_crash_keeps_the_dead_pane_and_opens_a_drawer` (E3) — exit 3 → the harness pane lists
  `#{pane_dead}` 1, and a second pane in the window has a start command holding
  `frame-palette --ended`.
- `test_a_signal_death_ends_the_tab` (E1, open question 4) — `os.kill(<recorder pid>,
  signal.SIGKILL)` → the drawer within 5 s. **It fails with the `list-panes` output on a
  platform where it does not happen. It is not skipped.**
- `test_the_last_chat_ending_keeps_the_session` — one chat, exit 0 → the session is still listed;
  `commands_frame.cmd_close` of it → the session is gone. It replaces
  `ChatsAreWindowsOnOneWorkspaceSession::test_the_last_chats_teardown_still_ends_the_session`.
- `test_the_same_holds_in_a_tmux_you_already_had` — an `-S` server with `$TMUX` patched;
  `_launch_in_operator_tmux` in a thread; the recorder exits 0 → the pane is respawned; a
  `kill-window` of it → the thread returns within 5 s.

**Existing tests this task changes** (a floor):
- `tests/test_frame_launcher.py::PaneDiedHooks::test_the_teardown_hook_is_a_constant_kill_window_at_index_1`
  — asserts the ended hook's constant action.
- `tests/test_frame_launcher.py::Launch::test_the_write_hook_is_installed_before_the_teardown_hook`
  — the same order, renamed for the ended hook.
- `tests/test_frame_launcher.py::Launch::test_refuses_to_attach_when_the_teardown_hook_fails_to_install`
  — `ENDED_HOOK_NOT_INSTALLED`'s words.
- `tests/test_frame_launcher.py::Launch::test_a_failed_teardown_after_an_early_death_is_reported`
  — unchanged; the early-death `kill-window` pin.
- `tests/test_frame_launcher.py::LaunchInsideTmux` — cases asserting the window closes after a
  drawn harness exits flip. `test_a_harness_that_dies_at_once_is_never_switched_to` stays.
- `tests/test_frame_tmux_integration.py::TmuxIntegration::test_the_write_hook_must_be_installed_before_the_teardown_hook`
  — the array-replacement measurement, kept for `[1]` and renamed.
- `tests/test_frame_tmux_integration.py::ChatsAreWindowsOnOneWorkspaceSession::test_the_last_chats_teardown_still_ends_the_session`
  — replaced (above).
- `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py::TheHarnessExitCodeTravelsAsItDid::test_a_harness_exit_code_travels_as_it_did`
  — *the window is gone* becomes *the window stays and the chat is ended*; the exit-code
  assertion stays.
- `tests/test_a_new_chat_starts_at_the_profile_selector.py`, as #1103 leaves it — its Esc cases
  stay for a pane that never started.
- `tests/test_a_right_click_on_a_tab_acts_on_that_tab.py::TheMenuIsTheTwoRowsThatHaveATabToSitOn`
  and `::CloseIsConfirmedAndItNamesTheTabYouClicked` — an ended tab's close row.
- `tests/test_a_reopen_says_what_it_cannot_bring_back.py` and
  `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py` — the `already ended on its own`
  literal.
- `tests/test_quit_and_reopen_on_a_real_tmux.py` — gains `AnEndedTabComesBackEnded`: quit with one
  ended tab, reopen → that window's shim record holds `--ended`.
- `tests/test_a_background_chat_really_starts_on_its_brief.py` — re-read for assertions that the
  window goes when the recorder exits.

### Implementation steps

- [ ] Step 0: E1–E6 on both versions and both arms; apply the stop rule.
- [ ] Confirm the operator's answer to open question 5 is in the task issue.
- [ ] Write the two modules and the changed cases; run them locally — expect failures; note the
      counts.
- [ ] `state` marks; `reopen.Chat.ended`; `chats.Chat.ended`; `leave.Doomed.ended`,
      `needs_confirming`, `CLOSE_NOW_ID`, the ended note.
- [ ] `ended.py`; the `cli` parsers.
- [ ] `commands_frame`: the ended hook, the drawn mark, the check after it, the refusal
      sentence, the operator-tmux loop, `cmd_close`, `_record_the_plane`, `_reopen_one`, the
      selector branch.
- [ ] `selector` resume row; `launcher` `--ended`, `resume_row`, Esc closes, `_picked` clears.
- [ ] `tabmenu` and palette close-now; the strip's mark.
- [ ] Run `grep -rn "kill-window\|teardown hook\|teardown_hook\|already ended on its own\|closes as it always did\|dead tab" charter/ tests/ docs/frame.md`
      in both directions; each hit reworded or restaged.
- [ ] Re-run the focused modules and every changed module; green.
- [ ] The ADR 0018 amendment, the IDE-spec notes, CONTEXT.md, docs, news.
- [ ] Commit; push. Read the four `test (3.x)` jobs, and quote the signal case's Linux result.
      Read the sweep summary and add a test per survivor.
- [ ] If the signal case is red on Linux, stop before merge and take open question 4 to the
      controller.

### Docs and news

- **`docs/frame.md`.**
  - `## Inside a tmux you already have` (`:1070-1075`): *"when the harness exits, charter closes
    the window"* becomes *the tab stays and offers a choice, and `charter claude` returns when
    you close the tab*.
  - `## Leaving` gains `### When a harness ends`, after its opening paragraphs:
    - a clean exit, a crash, no conversation yet, a background chat;
    - that charter restarts nothing by itself and types nothing into the harness;
    - that closing an ended tab does not ask.
  - `## Exit codes` (`:1394-1401`): the code now arrives when the tab closes or you detach.
  - `## When the command dies before the frame is drawn` (`:1486`): one sentence — this is still
    the one case a chat's window closes on its own.
  - `## How a harness starts` (`:1517-1540`): the amended ADR in one sentence.
  - `### No harness starts until you pick a profile` (`:3206`): the selector after an exit, its
    resume row, and that Esc closes the tab.
  - The `-` paragraph (`:944-962`): an ended tab closes without the warning.
- **IDE spec.** Dated notes at §4j and §5, not deletions: *Built 2026-09-15 by
  `docs/superpowers/specs/2026-09-15-one-exit-gate.md`*.
- **`CONTEXT.md`** **Ended tab**, in `### Chats`.
- **ADR 0018** — the amendment.
- **News** `docs/news/unreleased-an-ended-harness-keeps-its-tab.md`:

  ```
  ---
  version: unreleased
  headline: `/exit`, a double Ctrl+C or a crash no longer closes a chat — its tab stays and offers resume, a fresh start or close
  ---
  ```

  Body:
  - the failure: every exit killed the window and a later launch reaped the chat;
  - the two presentations, and resume;
  - that closing a tab is now the one way to end a chat;
  - `charter claude` returns when the tab closes;
  - the limits: ended tabs stay until closed, and Ctrl+C is not disabled.

**Suggested PR title:** A harness that ends keeps its tab, and the tab offers resume, a fresh start or close

---

## Task 3: A tab can carry a title

**Depends on:** tasks 1 and 2 on `main`.

### Step 0 — none new

`--name` is measured by task 1's C1 and C3, including a title with spaces and ` · `. This task
re-reads that table, and stops if C3 showed `--resume` refusing `--name`.

### Files

- Create `charter/frame/rename.py` and `tests/test_a_tab_can_carry_a_title.py`.
- `charter/frame/state.py` — beside `record_brief` (`:2099`): `TITLE_MAX`, `TITLE_FILE`,
  `record_title`, `title`.
- `charter/frame/chats.py` — `Chat.title: str = ""`; `roster` fills it; `label_of(chat)`.
- `charter/frame/slots.py` — `_chats_strip` (`:4577-4603`) hands a label per id. Today it returns
  a 5-tuple, and `_workspaces_strip` (`:4702`) and `BARS` (`:4706`) share the shape, so both gain
  a sixth element (`{}` for workspaces). The bar draws the label; `_Tabs.publish` (`:3487`) is
  still handed ids.
- `charter/frame/launcher.py` — `session_name(fid)`; `session_argv` and `resume_row` use it.
- `charter/frame/tabmenu.py`:
  - `RENAME_ID`, `rename_title`;
  - `catalogue` gives transcript, rename, close;
  - `opens` answers `rename.Rename` for `RENAME_ID`;
  - `label` shows the title;
  - the module docstring's *"Exactly two rows"* and *"no rename anywhere in the frame"*
    (`:36-42`) are rewritten.
- `charter/frame/selector.py` — `TITLE_ID` and a title row, last, when `titling=True`; `pick`
  loops through `rename.Rename` in the pane and back.
- `charter/frame/launcher.py` — `argv_select(start, *, ended=False, titling=False)` adds
  `--title-row`; `_select_in_pane` passes it on.
- `charter/commands_frame.py`:
  - `cmd_rename(args)`.
  - `_palette_catalogue` (`:9140-9166`) inserts `rename.open_rows(fid)` before
    `leave.open_rows(fid)`.
  - `_picker` (`:9417+`) opens `rename.opens`.
  - `_launch`'s `opening` branch (`:5977-5985`) records `rename.first_line_title(opening.brief)`.
  - `_restore_recorded_chat` records `rec.title`; `_record_the_plane` writes `title=`.
  - `cmd_new_chat` (`:11591+`) and `_open_workspace` (`:8510+`) launch with `titling=True`.
- `charter/frame/leave.py` — `Doomed.title`; `title()` (`:429-443`) shows `label · profile`.
- `charter/frame/reopen.py` — `Chat.title: str = ""`; `_chat`'s key list.
- `charter/frame/choose.py` — the chat picker's row title uses `chats.label_of`.
- `charter/cli.py` — `frame-rename` (`chat`, then `title` as `nargs=REMAINDER` after `--`), added
  to `_core_commands`; `frame-launch --title-row`.
- Docs and news below.

### Interfaces

**Consumes:**
- From tasks 1–2:
  - `launcher.session_argv`, `resume_row`, `argv_select(ended=)`;
  - `selector.pick(resume=)`, `tabmenu.CLOSE_NOW_ID`;
  - `reopen.Chat.ended`.
- On `main`: `palette.Palette`, `palette.typed` (`palette.py:160`), `contain.one_line`,
  `contain.readable`, `commands_frame._repaint_the_other_strips`, `_say_on_screen`,
  `builtin_actions._spawn`.

**Produces:**

```python
# charter/frame/state.py
TITLE_MAX = 60
TITLE_FILE = "title"
def record_title(fid: str, text: str) -> bool: ...
    # "" removes the title; refuses text that is not printable or is longer than TITLE_MAX after
    # contain.one_line; True when the record changed
def title(fid: str) -> str | None: ...

# charter/frame/chats.py
class Chat(NamedTuple): ...; title: str = ""
def label_of(chat: str) -> str: ...     # contain.readable(title) when set, else the id

# charter/frame/rename.py
OPEN_ID = "rename:open"
GO_ID = "rename:go"
TOO_LONG = "a title is at most {max} characters — this one is {n}; nothing was renamed"
NOT_PRINTABLE = "a title is one line of printable text — nothing was renamed"
def normalized(text: str) -> tuple[str | None, str]: ...   # (the title or None, why it was refused)
def first_line_title(brief: str) -> str: ...                # first non-blank line, one_line, cut at TITLE_MAX with contain.readable's marker
def open_rows(fid: str) -> tuple[overlay.Row, ...]: ...     # "chat: rename — give this tab a title"
def is_row(row) -> bool: ...
def opens(row, target: str) -> "Rename | None": ...
@dataclass
class Rename(palette.Palette):
    target: str = ""      # the one row is the typed text; Enter on GO_ID records it
def chose(row, target: str, *, fid: str, text: str) -> bool: ...   # spawns `frame-rename <target> -- <text>`

# charter/frame/launcher.py
def session_name(fid: str) -> str: ...  # f"{title} · {fid}" or fid; the title as recorded (printable, one line)

# charter/frame/tabmenu.py
RENAME_ID = "tab:rename"
def rename_title(target: str) -> str: ...

# charter/frame/selector.py
TITLE_ID = "title:"
def rows(have, *, cwd, start=None, after=None, resume=None,
         titling: bool = False) -> tuple[overlay.Row, ...]: ...   # the title row last when titling
def pick(*, cwd, root, start=None, after=None, resume=None, titling: bool = False,
         fd=None, out=None) -> Choice | None: ...                   # loops through Rename on TITLE_ID

# charter/frame/launcher.py
def argv_select(start: str | None, *, ended: bool = False, titling: bool = False) -> list[str]: ...
    # titling adds --title-row

# charter/commands_frame.py
def cmd_rename(args) -> int: ...   # always 0; the outcome goes to _say_on_screen
```

### Behaviour

- **`record_title` is the one gate.** It applies `contain.one_line`, strips, and refuses a
  character that is not `str.isprintable()` or a length over `TITLE_MAX`. Empty removes the
  file. It writes through `config.write_for`.
- **The strip.**
  - `label_of` is drawn in the tab, measured with `tui.width`, clipped by the bar's existing
    rules.
  - The click map is published with the id, so `TABS.tab_at` answers the chat.
  - A title wider than a tab is `tui.truncate`d like a long id is today.
- **Rename from the tab menu or `F2`.**
  1. The row opens `Rename` in the same pane: a doorway, `tabmenu.opens`' and `_picker`'s shape.
  2. Typing edits the one row, whose title is `title: <typed>`; `palette.typed` is the query.
  3. Enter runs `normalized`. A refusal goes to the footer, and the surface stays. A title is
     spawned as `frame-rename <target> -- <title>` through `builtin_actions._spawn`, a `Popen`
     argv with no tmux.
  4. Esc renames nothing.
- **`cmd_rename(args)`.**
  1. The id is held to `chats.ID_RE` and must name a chat on this plane.
  2. `normalized`, then `state.record_title`.
  3. `state.bump(target)` and `_repaint_the_other_strips(target)`.
  4. `_say_on_screen(fid, "renamed — Claude Code sees the new name the next time it starts or
     resumes")` when the kind is `claude-code`, and `"renamed"` otherwise.

  It sends no `send-keys`, respawns nothing, and touches no harness.
- **At `+` and a workspace tab.**
  - The selector carries `Row(id=TITLE_ID, title="title: (none) — Enter to name this chat")` as
    its **last** row, so `palette.aim` and `opens_on` never put the cursor on it.
  - Enter opens `Rename` in the pane, records through `state.record_title` directly (the pane is
    the chat's own, proven by `framed_chat`), and returns to the list with the row showing the
    title.
  - It is not offered on an ended selector or on a reopen.
- **A handoff.** `_launch` records `rename.first_line_title(opening.brief)` beside
  `state.record_brief`. `first_line_title` cuts with `contain.readable`'s marker and never
  refuses: a brief is not a tab label.
- **The name Claude Code sees.**
  - `launcher.session_name(fid)` is read at every `exec` by `session_argv`, for `--session-id …
    --name` and `--resume … --name`.
  - Codex and opencode get nothing, because their `new_session_argv` and `resume_argv` take no
    name.
- **The record.** `leave.Doomed.title` comes from `state.title`, `_record_the_plane` writes it,
  and `_restore_recorded_chat` records it before tmux.

### Test cases (write first; each must fail before the code exists)

**`tests/test_a_tab_can_carry_a_title.py`**

Class `TheTitleIsStoredAndContained(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_title_is_recorded_and_read` | `record_title("beta.1", "fix the widget")` → `title("beta.1") == "fix the widget"`; the file is `.charter/frame/beta.1/title`, spelled literally |
| `test_empty_removes_it` | → `title is None`, no file |
| `test_a_newline_becomes_one_line` | `"a\nb"` → the recorded text holds no `\n` |
| `test_a_control_byte_is_refused` | `"a\x1b[2Kb"` → `False`, nothing written |
| `test_a_title_over_the_limit_is_refused_with_its_length` | 61 characters → `False`; `rename.normalized` answers `(None, "…at most 60 characters — this one is 61…")` |
| `test_a_brief_first_line_is_cut_not_refused` | `first_line_title("\n\n" + "x" * 90 + "\nrest")` is at most 60 characters and ends with `contain.readable`'s marker |
| `test_a_drawn_title_is_escaped` | a title file written by hand holding `\x1b` → `chats.label_of` holds no ESC byte |

Class `TheStripDrawsTheTitle(PersonaIso, unittest.TestCase)`:
- `test_a_titled_tab_draws_its_title_and_an_untitled_one_its_id` — the chats bar rendered →
  `fix it` and `beta.2` are on the row.
- `test_a_click_on_a_title_resolves_to_the_chat` — `TABS.tab_at(0, <a column of "fix it">) ==
  "beta.1"`.
- `test_the_workspaces_strip_is_unchanged` (pin) — its render equals the render at `5ad755d` for
  the same plane.

Class `RenameFromTheTabMenuAndF2(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_the_tab_menu_carries_rename_between_transcript_and_close` | `[r.id for r in tabmenu.catalogue("beta.1")] == ["tab:transcript", "tab:rename", "tab:close"]` |
| `test_the_palette_carries_rename_before_the_leaving_rows` | `_palette_catalogue` → the `rename:open` row comes before `leave:quit` |
| `test_the_rename_row_is_a_doorway` | `tabmenu.opens(rename_row, …)` is a `rename.Rename`; `tabmenu.chose` of it is `False` |
| `test_enter_spawns_frame_rename_with_the_typed_text_on_argv_and_not_tmux` | `rename.chose(go_row, "beta.1", fid="beta.1", text="fix it")` → `_spawn` argv ends `["frame-rename", "beta.1", "--", "fix it"]`; no `tmuxctl.run` call |
| `test_the_command_records_bumps_and_says_when_claude_sees_it` | `cmd_rename(SimpleNamespace(chat_id="beta.1", title=["fix", "it"], chat="beta.1"))` → title recorded; `beta.2`'s version moved; the notice holds `next time it starts or resumes` for a `claude-code` chat |
| `test_a_rename_never_touches_the_harness` | no recorded tmux argv holds `send-keys` or `respawn-pane` |
| `test_a_name_that_is_not_a_chat_is_refused_before_anything` | `chat_id="../x"` → nothing written, the notice holds the refusal |

Class `ATitleAtThePlus(PersonaIso, unittest.TestCase)`:
- `test_the_selector_carries_a_title_row_last_when_titling` — `selector.rows(..., titling=True)[-1].id
  == "title:"`.
- `test_the_cursor_never_opens_on_it` — `Selector(rows).selected().id != "title:"` with a runnable
  profile row present.
- `test_a_title_given_at_the_selector_is_recorded_before_the_pick` — queued rows: `title:`, then
  the typed `fix it` on `Rename`, then `profile:claude` → inside the `execvpe` fake,
  `state.title("beta.1") == "fix it"`.
- `test_the_plus_and_a_tab_ask_for_the_title_row` — the `cmd_new_chat` and `_open_workspace`
  launch args hold `titling=True`, and a reopen's do not.

Class `AHandoffTitlesItsChat(PersonaIso, unittest.TestCase)`:
- `test_the_first_line_of_the_brief_is_the_title` — `open_in_background("beta", caller="alpha.1",
  first_message=…, brief="Fix the widget\n\nDetails…")` with `cmd_launch` running `_launch` under
  its patches → `state.title(<new fid>) == "Fix the widget"`.
- `test_a_brief_carrying_control_bytes_titles_nothing_unescaped` — `"\x1b]0;x\x07Fix"` → the
  strip's label holds no ESC or BEL byte.

Class `ClaudeIsNamedAtStartAndResume(PersonaIso, unittest.TestCase)` — task 1's launcher patches:
- `test_a_titled_chat_is_named_title_dot_id` — `--name` is followed by `fix it · beta.1`.
- `test_an_untitled_chat_is_named_its_id` — `--name beta.1`.
- `test_a_resume_after_a_rename_carries_the_new_name` — rename, then `--resume` →
  `--name "new · beta.1"`.
- `test_codex_and_opencode_are_given_no_name` — no `--name` in either argv.
- `test_the_resume_row_names_the_session` — `launcher.resume_row("beta.1").title ==
  "resume fix it · beta.1"`.

Class `TheTitleTravelsThroughTheRecord(PersonaIso, unittest.TestCase)`:
- `test_a_quit_records_the_title` — the literal key `"title"`.
- `test_a_reopen_restores_it_before_tmux` — inside the `new-window` fake, `state.title("beta.7")`.
- `test_a_record_from_before_reads_as_untitled` — no key → `""`.
- `test_the_quit_row_names_the_title` — `leave.title(Doomed(..., title="fix it", profile="claude"))
  == "beta.1 · fix it · claude"`.

**Existing tests this task changes** (a floor):
- `tests/test_a_right_click_on_a_tab_acts_on_that_tab.py::TheMenuIsTheTwoRowsThatHaveATabToSitOn`
  — three rows; the class name and its docstring's scope argument are rewritten.
- `tests/test_a_real_click_on_a_real_tab_bar_switches.py::ARealRightClickOnTheChatBarOpensARealMenu`
  — the row count.
- `tests/test_frame_bars.py` and `tests/test_a_tab_strip_grows_a_row_when_its_tabs_overflow.py` —
  widths measured from labels; the strip tuple's sixth element.
- `tests/test_frame_palette.py::TheActionsCharterOffersItself` — the catalogue's rows.
- `tests/test_a_new_chat_starts_at_the_profile_selector.py::TheRows` — no title row without
  `titling`.
- `tests/test_the_launcher_becomes_the_profile.py` and task 1's module — the `--name` value.
- `tests/test_a_reopen_says_what_it_cannot_bring_back.py` — `leave.title` literals.

### Implementation steps

- [ ] Re-read task 1's C1/C3 table; stop if `--resume` refuses `--name`.
- [ ] Tests first; run the module and the changed modules — expect failures; note the counts.
- [ ] `state` title; `chats.label_of`, `Chat.title`; `reopen`/`leave` title fields.
- [ ] `rename.py`; `cmd_rename`; the `cli` parsers.
- [ ] The strip's labels; `tabmenu`'s rename row; the palette doorway and `_picker`.
- [ ] The selector title row and `argv_select(titling=)`; `cmd_new_chat`/`_open_workspace`.
- [ ] `launcher.session_name` in `session_argv` and `resume_row`; the handoff's title; the restore.
- [ ] Run `grep -rn "Exactly two rows\|no rename\|two rows that have a tab" charter/ tests/` in both
      directions.
- [ ] Focused modules green. Docs, news, commit, push, CI's four jobs, the sweep summary.

### Docs and news

- **`docs/frame.md`.**
  - The chat strip and `-` paragraphs (`:930-962`): a tab shows its title, rename is on the tab
    menu and `F2`, and `+` offers a title row.
  - `### A chat opened for you in the background` (`:726`): the brief's first line titles it.
  - `## Leaving`: a reopen restores titles.
  - `### No harness starts until you pick a profile`: the title row.
- **`docs/harnesses.md`** — the session name: Claude Code sees `<title> · <id>` at its next start
  or resume; Codex and opencode keep their own names.
- **`CONTEXT.md`** **Title**, avoiding *name*.
- **News** `docs/news/unreleased-a-tab-can-carry-a-title.md`:

  ```
  ---
  version: unreleased
  headline: A chat tab can carry a title — rename it from the tab menu or F2, name it at `+`, and a handoff titles its chat from the brief
  ---
  ```

  Body: where titles come from; that Claude Code sees one at its next start or resume; that Codex
  and opencode keep their own names; and that charter never types into a harness.

**Suggested PR title:** A tab can carry a title, and Claude Code is started under it

---

## Task 4: One gate closes charter

**Depends on:** tasks 2 and 3 on `main`. **Closes:** #1097.

### Step 0 — Measure first. A failed G1, G2 or G4 stops this task.

**Where.** A throwaway `-S /tmp/cp-t4-<slug>/s` server with two clients attached over two ptys:
`tests/test_a_real_click_on_a_real_tab_bar_switches.py::_ARealFrameWithBars`' pty helper, run by
hand. Both tmux 3.7c and the 3.2 floor.

| # | Reading | Pass when |
|---|---|---|
| G1 | `bind -n F10 run-shell 'echo "#{client_name}" >> <file>'`; `\x1b[21~` written to client A's pty | the file holds A's `#{client_name}` and not B's, on both versions |
| G2 | `detach-client -t <A>` | A's `attach` exits; B's is still attached |
| G3 | an SGR press on a panel pane in A's view; the panel's child runs `display-message -p -t <$session id> '#{client_name}'` | record the answer, three trials in each order of last activity (open question 3). Not a stop criterion |
| G4 | `detach-client -s default.1` against a session `default` with a window `default.1`; then `detach-client -s <$N>` from `display-message -p -t <%pane> '#{session_id}'` | the first fails with `can't find`, the second detaches every client of that session |
| G5 | `conf_text(...)` with the `F10` line, `source-file`'d | `list-keys -T root` shows `F10` and `F2`, and the escape hatch's line is still the last one sourced |

**Stop rule** as task 2's. **G3's result chooses the pointer route** (Behaviour, below), and goes
into the PR and the spec's open question 3.

### Files

- Create `charter/frame/gate.py`, `tests/test_one_gate_closes_charter.py`, and
  `tests/test_the_gate_detaches_a_real_client.py`.
- `charter/commands_frame.py`:
  - `conf_text` (`:1018-1035`): the gate's bind directly after the hotkey's, before
    `BAR_ROWS_KEY`'s and the toggles; the escape hatch stays last.
  - `cmd_palette` (`:8948-9024`): `gate.wanted(args)` → `gate.draw(args)`, beside
    `tabmenu.wanted`.
  - `_open_palette` (`:9026-9070`): splices `gate.forward(args)` beside `tabmenu.forward(args)`.
  - `_draw_palette` (`:9168-9300`): a chosen `builtin_actions` detach row passes the palette's
    client when it has one.
- `charter/frame/builtin_actions.py`:
  - `_detach(fid, client="")` (`:116-124`) resolves as below; `NO_SESSION_TO_DETACH`.
  - `_register_detach` (`:139-146`): the title is `gate.DETACH_TITLE`; `available` and
    `reason_unavailable` are unchanged.
- `charter/frame/leave.py` — `OPEN_QUIT` (`:459`) is `gate.STOP_TITLE`; ids unchanged.
- `charter/frame/slots.py`:
  - `_Doors` (`:320-399`): `_gate` columns, `publish(columns, gate=())`, `opens_gate(col)`.
  - `_top` (`:433-615`): `GATE_BUTTON` at the right end. It is dropped whole after the version and
    before the identity, and its columns are published through `_door_columns`.
- `charter/frame/builtins.py` — `_strip_events` (`:701-752`): a gate door spawns
  `frame-palette --gate`.
- `charter/instance.py`:
  - `component_arrangement`'s `bound` (`:2561-2562`) gains `gate.GATE_KEY`.
  - The `[frame] hotkey` resolution (read `frame_of` and `:2100` first) refuses a value equal to
    `gate.GATE_KEY`, falling back to the shipped `F2` exactly as it does for a value
    `_HOTKEY_RE` refuses, with the reason where that path puts it.
- `charter/cli.py` — `frame-palette` (`:970-991`): `--gate` (`store_true`) and `--client`
  (`default=""`).
- Docs and news below.

### Interfaces

**Consumes:**
- From tasks 2–3: `leave.plan(...).chats[*].ended`, `leave.note`, `chats.label_of`.
- On `main`:
  - `commands_frame._pane_place` (`:7949`), `_plane_live`, `_plane_servers`, `_as_a_drawer`,
    `_close_palette` (`:9605`), `_say_on_screen`;
  - `tabmenu.handback`; `builtin_actions._spawn`, `_server`, `_detachable`;
  - `slots._door_columns` (`:402`); `instance.component_arrangement`.

**Produces:**

```python
# charter/frame/gate.py
GATE_KEY = "F10"
OPTION = "--gate"
LABEL = "charter · close"
DETACH_ID = "gate:detach"
STOP_ID = "gate:stop"
DETACH_TITLE = "Close charter (keep chats running)"
STOP_TITLE = "Close charter and stop all chats…"
CLIENT_RE = re.compile(r"/dev/[A-Za-z0-9._/-]{1,64}")
DETACHES_EVERY_CLIENT = "detaches every terminal on this workspace — charter cannot tell which one clicked"
def wanted(args) -> bool: ...
def client_of(args) -> str: ...            # args.client held to CLIENT_RE, else ""
def forward(args) -> tuple[str, ...]: ...  # ("--gate",) + ("--client", c) when c, else ()
def catalogue(fid: str, *, client: str) -> tuple[overlay.Row, ...]: ...
def opens(row, fid: str, *, live) -> "palette.Palette | None": ...
def chose(row, fid: str, *, client: str) -> bool: ...
def draw(args) -> int: ...                 # always 0

# charter/frame/builtin_actions.py
NO_SESSION_TO_DETACH = ("charter cannot find this chat's tmux session, so it detached nothing — "
                        "close the terminal to detach it")
def _detach(fid: str, client: str = "") -> str: ...

# charter/frame/slots.py
GATE_BUTTON = "F10 close"
class _Doors:
    def publish(self, columns, gate=()) -> None: ...
    def opens_gate(self, col: int) -> bool: ...
```

### Behaviour

**The bind.** One line, after the hotkey's:
`f"bind -n {gate.GATE_KEY} run-shell '\"${_CHARTER_PY_ENV}\" -m charter frame-palette --gate --chat \"#{{{_CHAT_OPTION}}}\" --client \"#{{client_name}}\"'"`.
It is a constant, and tmux expands both formats at the keypress (G1).

**The menu (`gate.draw`)** takes `tabmenu.draw`'s shape: `handback`, then
`palette.Palette(catalogue=catalogue(fid, client=client_of(args)), label=LABEL, mouse=True)`,
then `own_the_tty` with `then=` answering `opens` through `commands_frame._as_a_drawer`, then
`chose`. `finally` runs `_close_palette`.

**`catalogue(fid, *, client)`:**
1. `Row(id=DETACH_ID, title=DETACH_TITLE)`.
   - `note=DETACHES_EVERY_CLIENT` when `client == ""` and G3 did not pass.
   - `refused=True` with `_register_detach`'s own `reason_unavailable` when
     `not builtin_actions._detachable(fid)`: the operator's own tmux.
2. `Row(id=STOP_ID, title=STOP_TITLE)` — a doorway.

It is first-runnable-row aimed, so *Close charter (keep chats running)* is under the cursor
wherever it can run.

**`opens(row, fid, *, live)`.** `STOP_ID` → `palette.Palette(catalogue=leave.confirm_rows(
leave.plan(live=live, focus=state.own_workspace(fid) or ""), verb=leave.QUIT), label=leave.QUIT,
mouse=True)`. Anything else → `None`.

**`chose(row, fid, *, client)`:**
- `DETACH_ID` → `builtin_actions._detach(fid, client)`, and `True`.
- `leave.goes_through(row, leave.QUIT)` → `_spawn(util.self_relaunch_argv("frame-quit", "--chat",
  fid), fid=fid)`, and `True`.
- A refused row → `False`, and its note is said.

**`_detach(fid, client="")` — #1097.**
1. `server = _server(fid)`.
2. `client` matching `gate.CLIENT_RE` → `_spawn(tmuxctl.server_argv(server, "detach-client",
   "-t", client))`.
3. Otherwise `place = commands_frame._pane_place(server, state.harness_pane(fid))`:
   - `None` → return `NO_SESSION_TO_DETACH` and spawn nothing;
   - else `_spawn(tmuxctl.server_argv(server, "detach-client", "-s", place[0]))`, where
     `place[0]` is `$N`.
4. A chat id and a session name are never a target.

**The pointer route.**
- `_strip_events` spawns `frame-palette --gate` with no client for a gate door.
- **If G3 passed**, `gate.draw` resolves the client itself, with `tmuxctl.run(...,
  "display-message", "-p", "-t", <$N>, "#{client_name}")` held to `CLIENT_RE`, and the row carries
  no note.
- **If G3 did not pass**, the row detaches every client of the session and carries
  `DETACHES_EVERY_CLIENT`.

**The button.**
- `_top` builds `f" {GATE_BUTTON} "` as the last cell.
  - When the row fits, the right end is `charter <version> │ F10 close`.
  - When it does not, the version goes first, then the button, then the identity is truncated as
    today.
- Its columns are published as `gate=`; `opens_palette` never answers for them.
- At `terse` the version goes and the button stays.

**The `F2` rows.**
- `frame.detach`'s title is `DETACH_TITLE`, and its `run` passes the palette's client: `cmd_palette`
  reads `--client` when `_open_palette` forwarded one, and today's hotkey bind carries none. So the
  `F2` row resolves the session per step 3, unless G3 passed and the palette resolved it.
- `leave.OPEN_QUIT` is `STOP_TITLE`.
- Both keep their ids and positions.

**Reserved.**
- `instance.component_arrangement` refuses `key = "F10"` with its existing *already bound*
  sentence.
- `[frame] hotkey = "F10"` resolves to `F2`, and the reason is reported where an unusable hotkey's
  already is.

**Inside the operator's own tmux.**
- `_launch_in_operator_tmux` writes no bind: a pin that already holds.
- The palette's rows are the gate's. *Close charter* is refused there with its reason.
  *Close charter and stop all chats…* runs `frame-quit`.

### Test cases (write first; each must fail before the code exists)

**`tests/test_one_gate_closes_charter.py`**

Class `TheKeyIsBound(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_conf_text_binds_f10_to_the_gate_with_the_chat_and_the_client` | a line starts `bind -n F10 run-shell` and holds `frame-palette --gate`, `#{@charter_chat}` and `#{client_name}` |
| `test_the_bind_is_a_constant` | two sessions → the same line |
| `test_it_follows_the_hotkey_and_the_hatch_stays_last` | the `F10` line's index is greater than the hotkey's; the last non-empty line is `overlay.hatch_bind()` |
| `test_the_operators_tmux_gets_no_bind` (pin: passes at `5ad755d`) | `_launch_in_operator_tmux` with tmux recorded → no argv holds `bind` or `F10` |

Class `TheKeyIsReserved(PersonaIso, unittest.TestCase)`:
- `test_a_component_cannot_take_f10` — a `[[frame.component]]` with `key = "F10"` → refused with
  `"already bound"`.
- `test_a_hotkey_of_f10_falls_back_to_f2` — `[frame] hotkey = "F10"` → `config.FRAME["hotkey"] ==
  "F2"`, and the reason is reported.

Class `TheMenu(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_two_rows_detach_first` | `[r.id for r in gate.catalogue("beta.1", client="/dev/ttys003")] == ["gate:detach", "gate:stop"]`; titles are `DETACH_TITLE` and `STOP_TITLE`, spelled here |
| `test_the_cursor_opens_on_close_charter` | `palette.Palette(catalogue=…).selected().id == "gate:detach"` |
| `test_stop_all_chats_opens_the_quit_confirmation_listing_every_chat` | two running chats and one ended → `gate.opens(stop_row, …)` is a `Palette` whose rows are `leave.confirm_rows(…, verb=QUIT)`, three chat rows, the ended one's note holding `comes back ended` |
| `test_the_confirming_row_runs_frame_quit` | `gate.chose(leave_go_row, "beta.1", client="")` spawns `frame-quit --chat beta.1` |
| `test_stop_is_a_doorway_not_an_action` | `gate.chose(stop_row, …) is False`, no spawn |
| `test_the_menu_always_returns_zero_and_gives_the_harness_back` | `own_the_tty` raises → `draw` returns 0; `_close_palette` recorded |

Class `CloseCharterDetachesThisTerminal(PersonaIso, unittest.TestCase)` — #1097:

| Case | Asserts |
|---|---|
| `test_a_named_client_is_detached_alone` | `_detach("default.1", "/dev/ttys003")` → `_spawn` argv `[..., "detach-client", "-t", "/dev/ttys003"]` |
| `test_without_a_client_the_chats_session_id_is_the_target` | `_pane_place` → `("$4", "@9")` → `[..., "detach-client", "-s", "$4"]` |
| `test_a_chat_id_is_never_the_target` | across both cases, no argv holds `default.1`. Red at `5ad755d` |
| `test_a_session_that_cannot_be_found_detaches_nothing_and_says_so` | `_pane_place` → `None` → no spawn; the return is `NO_SESSION_TO_DETACH` |
| `test_a_client_name_outside_its_shape_is_not_used` | `client="x; kill-server"` → the `-s $4` route |
| `test_the_operators_tmux_row_is_refused_with_its_reason` (pin) | an operator socket → the detach row `refused`, with the prefix-key sentence |

Class `TheButton(PersonaIso, unittest.TestCase)`:
- `test_the_identity_row_ends_in_the_gate_button` — `_top(fid)` rendered at 120 columns ends in
  `F10 close ` (the right pad).
- `test_its_columns_open_the_gate_and_not_the_palette` — `DOORS.opens_gate(<a column of the
  button>)` and not `opens_palette` for it; the workspace chip's column is the reverse.
- `test_a_starved_row_drops_the_version_before_the_button` — at the width where both do not fit,
  `charter ` is absent and `F10 close` is present.
- `test_a_click_on_it_spawns_the_gate` — `builtins._strip_events(fid)` given a press at the
  button's column → `_spawn` argv holds `frame-palette --gate`.
- `test_terse_keeps_the_button` — `verbosity` → `terse` → `F10 close` present, `charter ` absent.

Class `TheF2RowsAreTheGate(PersonaIso, unittest.TestCase)`:
- `test_the_detach_row_says_close_charter` — `_offers()["frame.detach"].title == "Close charter
  (keep chats running)"`, and it is still the first id.
- `test_the_quit_doorway_says_stop_all_chats` — `leave.open_rows("beta.1")[0].title == "Close
  charter and stop all chats…"`, and `leave.verb_of` of it is `QUIT`.

**`tests/test_the_gate_detaches_a_real_client.py`** — real tmux over ptys.
- The `_ARealFrameWithBars` / `_ARealFrameWithStrips` base, skip rules and reaped sockets.
- Class `F10OnARealServer(_ARealFrameWithBars, unittest.TestCase)`:
  - `test_f10_then_enter_detaches_only_the_terminal_that_pressed_it` — two clients on one frame;
    `\x1b[21~` then `\r` written to A's pty → A's client process exits within 5 s, and
    `list-clients` still lists B.
  - `test_f2_detach_detaches_again` (#1097) — the palette's detach row run for chat `<ws>.1` →
    `list-clients -t <session>` is empty within 5 s. Red at `5ad755d`.
- Class `ARealClickOnTheGateButtonOpensTheGate(_ARealFrameWithStrips, unittest.TestCase)`:
  - `test_a_press_on_f10_close_opens_the_gate_menu` — an SGR press at the button's column →
    within 5 s a pane whose start command holds `frame-palette --gate`.

**Existing tests this task changes** (a floor):
- `tests/test_frame_palette.py::TheActionsCharterOffersItself` — the detach row's title; its id
  stays first.
- `tests/test_a_click_on_a_panel_stays_where_it_points.py` and
  `tests/test_component_toggle_keys.py`, each `…::test_the_escape_hatch_is_still_the_last_line`
  — pins, with the `F10` line present.
- `tests/test_component_toggle_keys.py` — a component key `F10` is refused.
- `tests/test_frame_slots.py::TopRenderer` — the right end.
- `tests/test_a_real_click_opens_the_real_palette.py::ARealClickOnTheHotkeyHintOpensTheRealPalette`
  — a pin that the hint still opens the palette.
- `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py` — `OPEN_QUIT`'s words.
- `tests/test_a_confirmation_is_a_drawer_not_a_window.py` — the gate's stop doorway is a drawer.
- `tests/test_the_palette_advertises_unmaking_what_it_makes.py` — the row titles.

### Implementation steps

- [ ] Step 0: G1–G5; the stop rule; G3's result recorded.
- [ ] Tests first; run the modules and the changed modules — expect failures; note the counts.
- [ ] `builtin_actions._detach` (#1097) and its row title.
- [ ] `gate.py`; `cli`'s `--gate`/`--client`; `cmd_palette`/`_open_palette`.
- [ ] `conf_text`'s bind; the reserved key in `instance`.
- [ ] The button: `_Doors`, `_top`, `_strip_events`.
- [ ] `leave.OPEN_QUIT`.
- [ ] Run `grep -rn "detach — leave the harness running\|charter: quit — stop every harness" charter/ tests/ docs/`
      in both directions.
- [ ] Focused modules green. Docs, news, commit, push, CI's four jobs, the sweep summary.

### Docs and news

- **`docs/frame.md`.**
  - `## Leaving: detach, close, quit — and reopen` (`:1151`) opens with the gate: `F10`, the
    button, the two rows, what each keeps. Closing the terminal still detaches. `F2 → detach`
    works again.
  - The identity row's paragraph (`:366-372`): the `F10 close` button opens the gate, not the
    palette.
  - `## Inside a tmux you already have` (`:1094-1105`): no gate key, and the rows are in the
    palette.
  - `## Configuring it`: `F10` joins `F12` as a key a component and `hotkey` may not take.
- **`docs/control-plane.md`** `[frame] hotkey`: `F10` is refused.
- **`CONTEXT.md`** **Exit gate**.
- **News** `docs/news/unreleased-one-gate-closes-charter.md`:

  ```
  ---
  version: unreleased
  headline: F10 closes charter — detaching this terminal with every chat still running, or stopping them all after a confirmation — and F2 → detach detaches again
  ---
  ```

  Body:
  - the operator's ask and what was built;
  - what was not built: Ctrl+C is not disabled, and why;
  - #1097;
  - the limits: F10 is taken from the harness on charter's server; no key inside your own tmux;
    the button needs `[frame] mouse`.

**Suggested PR title:** One gate closes charter: F10, the identity row's button and two F2 rows, and F2 → detach detaches again (#1097)
