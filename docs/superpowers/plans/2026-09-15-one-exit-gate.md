# One exit gate — a harness that ends keeps its tab, and every tab names one harness session

> For agentic workers: REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `/exit`, a double Ctrl+C or a crash no longer destroys a chat, and charter has one way
out.
- **One way out:** `F10`, the identity row's `F10 close` and the same two `F2` rows open one menu.
  - *Close charter (keep chats running)* detaches the terminal that asked, and only that one.
  - *Close charter and stop all chats…* confirms, records and stops.
- **Every tab names exactly one harness session**, so a tab that ended can offer resume.
- **A chat id is never handed out twice**, so a new chat inherits nothing.
- **A tab can carry a title.**

**Architecture:**
- **Ids.** `state.new_chat_id` starts above a per-prefix high-water mark kept in
  `.charter/frame/chat-ids.json`, and above every trace an id leaves. The mark is written before the
  claim, and the `mkdir` claim stays the exclusion.
- **The link.**
  - `harness/base.Harness` gains the session members.
  - `frame/launcher.py` adds the session and resume arguments at the `exec`, from the chat's own
    record, so none of them crosses tmux.
  - Hooks record what Codex and opencode report. Only the chat's own harness may change its link.
- **An exit.** For a profile's harness, `pane-died[1]` stops being `kill-window` and runs
  `charter frame-ended` (`frame/ended.py`).
  - A clean exit respawns the proven dead pane into the selector with a resume row.
  - A crash leaves the dead pane and opens a drawer beside it.
  - The escape hatch keeps `kill-window`.
- **The gate.** `frame/gate.py` is the menu, in `tabmenu.py`'s shape.
  - `conf_text` binds `F10`.
  - Every bind and click route records the presser (`#{client_name}`).
  - The identity row gains a gate door on charter's own server.

**Tech Stack:**
- Python ≥ 3.11 stdlib, and stdlib `unittest`.
- tmux: 3.7c measured, the 3.2 floor at `~/.local/share/charter-testing/tmux-3.2`, and CI's image
  tmux (3.4).
- Claude Code, codex-cli and opencode at the versions this machine runs. Step 0 records them.

**Spec:** `docs/superpowers/specs/2026-09-15-one-exit-gate.md` (binding). The decisions, the rulings on
the spec's open questions and the evidence are in `workspaces/harness-profiles/refs/exit-gate-decisions.md`
in the plane.

**Line anchors** are against `origin/main` @ `5ad755d` (0.62.0). Open PRs #1100 and #1103 and every task
here move them, so re-anchor by symbol name, never by number.

## Global Constraints

Every task's requirements include this section.

**Carried over from the harness-profiles plan:**
- stdlib only, Python ≥ 3.11; no new runtime dependency.
- **Tests are stdlib `unittest`, and they fail first.** Every behavioural change lands with a test that
  is red without it. They use the isolation helpers CONTRIBUTING names:
  - `tests._isolation.PersonaIso`/`PlaneIso`;
  - `_planeguard`, including `RealTmuxReach`, `RealForgeReach` and `BackgroundGrandchild`;
  - `_envguard`, `_ttyguard`, `_gitguard`, `_tmuxreap`, `_claudeguard` and `_execguard`.
- `charter/frame/tmuxctl.py` is the only module that calls tmux (ADR 0018).
- Charter never parses the harness pane. It draws in a chat pane only where ADR 0018 allows, as task 2
  amends it.
- **No version bump and no tag.**
  - Each task's news entry is its own file, `docs/news/unreleased-<slug>.md`, with flat `key: value`
    frontmatter, unquoted values, and only the six keys CONTRIBUTING lists.
  - No test names it by filename (`test_news_gate.test_no_test_opens_an_entry_by_its_staged_name`).
- Docs move with the code, in the same PR.
- Comments explain why.
- Every state write goes through `config.write_for`, `config.replace_for`, `config.create_for`,
  `config.open_for`, `config.private_mkdir` or `config.claim_private_dir`.
- **A hook never breaks a turn.** New hook-reachable code is best-effort, in
  `hooks._record_harness_session`'s `except Exception: pass` shape.
- **A test that starts tmux names every socket** with `tests._tmuxreap.name("<slug>")` (lowercase
  letters, digits and single hyphens), and kills and unlinks it in cleanup.
- A new `mock.patch.dict(os.environ, …)` passes `clear=True` or states every value it depends on.
- **Every refusal follows CONTEXT.md's Prose rules:** it says the rule worked and names the fix in the
  same breath.
- **A real-tmux test that launches a harness puts a recorder first on the tmux client's `PATH`** under
  the kind's name, and hands the server `CHARTER_ROOT`.
  - That is how `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py` does it.
  - A declared profile is seeded with `tests._isolation.approve_profile` and checked with
    `assert_approved`.
- Kill only the PIDs you started. Never `pkill -f`.

**The operator's standing rules on this machine.** These replace the harness-profiles plan's *run the
whole suite before each PR* and *run `tools/sweep.py`*.
- **No full local suite and no local sweep.**
  - Locally a task runs only the modules it writes or changes:
    `( unset TMUX TMUX_PANE; python3 -m unittest tests.<module> … )`.
  - The whole suite runs on CI's four `test (3.x)` jobs, and the guard sweep on `sweep.yml`. Each PR
    reports both from the run's own summary.
  - A task that adds a spawn or an exec also runs `tests/test_plane_spawn_guard.py` locally.
- **Never touch the operator's tmux servers.**
  - Nothing a task runs addresses `-L charter`, the default socket, a `charter-plane-*` socket this
    plane's frames run on, or the server `$TMUX` names. `tests._planeguard.RealTmuxReach` refuses those
    for a test's child.
  - A measurement by hand uses a throwaway `-S /tmp/cp-t<N>-<slug>/s` socket, started `-f /dev/null`,
    run with `TMUX` and `TMUX_PANE` unset.
- **Every guard is pinned by a test that goes red without it.**
  - CI's sweep reports survivors for lines under `charter/`, and each gets a test or its line deleted.
  - A guard added under `tests/` gets a hand deletion check, and the PR names the class that went red.
- **Anything a chat can write is contained before charter shows it or hands it on** (ruling 35).
  - That covers a title, a brief's first line, a harness session id, a transcript path, a presser
    name, and every hook payload field.
  - Display goes through `contain.readable` or `contain.one_line`.
  - A value is held to its shape before it reaches an argv, and a path is only ever `stat`ed.
- **An ADR amendment ships with the code that first relies on it** (ruling 44): ADR 0024 in task 1,
  the ADR 0018 amendment in task 2.
- **Scratch files are namespaced per task** (`/tmp/cp-t1-…`, `<scratchpad>/t1/…`). Parallel agents on
  this machine share one scratchpad.
- **Commit messages and PR bodies go through a file** (`git commit -F`, `gh pr create --body-file`).
  The vault guard reads the whole tool call.

**Added for this feature:**
- **No new kill, respawn, split or detach acts on a record alone** (spec, *What proves a pane before
  charter acts on it*; the #933/#1103 rule).
  - Each is aimed at what ONE listing on the chat's recorded server proves, targeted by the pane or
    client id that listing reported:
    - `@charter_chat == <id>` and `@charter_plane == commands_frame._this_plane()`;
    - for a drawer, `@charter_drawer == <id>`;
    - for a detach, a presser attached to that chat's session.
  - A respawn never passes `-k`. A race two processes can both win is claimed with `config.create_for`.
- **A tmux server that did not answer proves nothing** (#1100). Only a server that
  `tmuxctl.nothing_listening(server)` confirms is gone counts as holding no panes. A timeout, or any
  other failure, refuses.
- **Plane-marker tests on every new tmux write.** Each task's write is tested against three things:
  - a pane of **another plane** carrying the same chat id;
  - an **unmarked** pane, with no `@charter_plane`;
  - a record naming a pane the listing puts under another chat.

  Each case must be red without its guard.
- **`layout.CARRIABLE` is unchanged, and nothing new crosses tmux but flags and closed-alphabet ids.**
  - A title, a harness session id and a transcript path never ride a `-e`, a launcher argv or
    `set-environment`.
  - The new words on a tmux argv are `--resume`, `--ended`, `--fresh`, `--gate`, and chat ids already
    held to `chats.ID_RE`, plus `#{client_name}`, which tmux expands for itself.
- **A tmux hook or bind action is a constant string** (`_pane_died_write_hook_argv`'s docstring,
  `commands_frame.py:1407-1412`). It holds formats tmux expands and variables the shell expands;
  charter interpolates nothing into it.
- **Charter restarts nothing by itself and never types into a harness** (ADR 0018 as task 2 amends
  it).
  - No path starts a harness after an exit without an operator's Enter.
  - On an ended tab, neither end of input nor Ctrl+C is ever read as a choice.
  - No `send-keys` reaches a chat pane.
  - Tasks 2 and 3 pin these by recording every `tmuxctl.run` argv.
- **Nothing reads a harness's pane to decide.**
  - The ended step decides from `state.exit_code` and the link.
  - `_pane_last_words` and `_capture_transcript` keep their two moments, and a quit captures no ended
    tab.
  - Resume existence is a `stat` of a path the harness named.
- **"Close charter" detaches the proven presser and nothing else** (decision 2). No route falls back
  to every client of a session.
- **Nothing parses a chat id beyond what `chats._order` already reads**, which is the ordinal after
  the last dot.
- **Open PRs #1100 and #1103 touch the same functions** (`_launch`, `launcher.py`, `chats.py`,
  `slots.py`, the reopen path). Before each task starts, `git fetch` and re-read whichever has merged,
  then re-anchor.

## Order and parallelism

**1 → 2 → 3 → 4, one task at a time, each its own reviewed PR** (spec, *Build order*).
- **Task 1** has no dependency.
- **Task 2** needs task 1 on `main`:
  - kept ids and restored records, for reopen;
  - the link and `leave.conversation_exists`, for the resume row;
  - `launcher.attempt(..., resume=)` and its `on_exec` wrapper, where `ended.reset` is called.
- **Task 3** needs tasks 1 and 2: it composes the name task 1's launcher passes, and draws it on
  task 2's resume row.
- **Task 4** needs task 2, whose confirmation lists ended tabs, and task 3, whose rows name titles.

**The spec's open questions, as ruled** (decisions file, *Rulings on the spec's open questions*):

| Open question | Status | Where it lands |
|---|---|---|
| 1. opencode's chat id on the hook path (#946) | ruled (A) | task 1: `state.chat_in_pane`, `hooks._record_reported_session` |
| 2. The facts decision 10 rests on | answered | task 1 Step 0: C1–C7 were run live; X1–X3 and O1–O2 were read from source |
| 3. Which terminal the button detaches | ruled: record the presser | task 4: the `MouseDown1Pane` bind and G3. An unidentifiable presser stops the task |
| 4. `pane-died` on Linux for an empty status and signal | open; a pre-merge gate on task 2 | task 2's Linux CI real-tmux case, run unskipped, with a raw-mode stand-in, covering exit 0 and `SIGKILL`; the controller rules before merge |
| 5. `charter frame -- <cmd>` | ruled: keep today's behaviour | task 2: the hook branch and its real-tmux test |

## Measured while writing this plan

**Nothing was run.** The plan's author read code only, and the facts it rests on are in the decisions
file:
- `claude --help`: `-n/--name`, `--session-id`, `-r/--resume`, `/rename`;
- `codex resume [SESSION_ID]`;
- `opencode -s/--session`, `session_rename` on Ctrl+R;
- measured on 3.7c: `has-session -t default.1` → `can't find pane: 1` (#1097).

**#729** removed `"#{client_name}"` from the hotkey bind because its one consumer, `display-message
-c`, was gone (`commands_frame.py:861-876`, PR #763). That reason does not forbid a new consumer, and
`frame-palette` still accepts the `client` positional (`cli.py:971`).

---

## File Structure

Created:

| Path | Task | Responsibility |
|---|---|---|
| `charter/frame/ended.py` | 2 | The ended step: `_proof` (one listing), `present`, `reset`, `drop_drawer`, the drawer's rows and `choose`, `draw`, `cmd_frame_ended`. |
| `charter/frame/rename.py` | 3 | The one-line title input (`Rename`), `normalized`, the rename doorway rows, `first_line_title`. |
| `charter/frame/gate.py` | 4 | `GATE_KEY`, the gate's two rows with the cursor pinned to detach, the stop doorway, `draw`. |
| `docs/adr/0024-a-chat-id-names-one-chat-and-one-harness-session.md` | 1 | The id and link decisions. |
| `docs/news/unreleased-a-chat-id-is-never-handed-out-again.md` | 1 | News. |
| `docs/news/unreleased-an-ended-harness-keeps-its-tab.md` | 2 | News. |
| `docs/news/unreleased-a-tab-can-carry-a-title.md` | 3 | News. |
| `docs/news/unreleased-one-gate-closes-charter.md` | 4 | News. |
| `tests/test_a_chat_id_is_never_handed_out_again.py` | 1 | Counter, traces, lock, digits bound, fail-closed mark, scans, kept ids and their proof, #1101's repro, the ADR pin. |
| `tests/test_a_tab_is_linked_to_one_harness_session.py` | 1 | Harness members, the launcher's session arguments, what hooks record, the nested-harness rule, `chat_in_pane`, resume existence, the record. |
| `tests/test_a_launch_hands_the_harness_its_session_on_a_real_server.py` | 1 | Real tmux: the id reaches the harness argv and not tmux's; a reopened chat keeps its id. |
| `tests/test_an_ended_harness_keeps_its_tab.py` | 2 | The hook and its escape-hatch branch, proof and plane guards, the claim, reset on every start, the selector after an exit, Esc against end of input, the drawer, marks, close without asking, quit and reopen, exit codes, the operator's tmux, the ADR pin. |
| `tests/test_an_ended_harness_keeps_its_tab_on_a_real_server.py` | 2 | Real tmux: clean exit, crash, signal death, the last chat, the escape hatch, the operator's tmux, another plane's pane. |
| `tests/test_a_tab_can_carry_a_title.py` | 3 | Storage and containment, where a title is shown and where it is not, rename, `+`, a handoff, `--name`, the record. |
| `tests/test_one_gate_closes_charter.py` | 4 | The binds carry the presser, reserved keys, the menu and its pinned cursor, the presser-only detach (#1097), the button, decision 7, the `F2` rows. |
| `tests/test_the_gate_detaches_a_real_client.py` | 4 | Real tmux over ptys: `F10` and `F2 → Close charter` detach only the presser; a real click records the presser and opens the gate. |

Modified:

| Path | Task | Change |
|---|---|---|
| `charter/frame/state.py` | 1, 2, 3 | Counter, traces, digits bound, lock through `config.open_for`, mark before claim, `claim_chat_id`, `reap_chat`, session id shape, `record_conversation`, `record_harness_pid`, `chat_in_pane` (1); `drawn`, the `ended` claim, `drawer` marks (2); `title` (3). |
| `charter/harness/base.py`, `claude_code.py`, `codex.py`, `opencode.py` | 1 | The session members and their overrides. |
| `charter/frame/launcher.py` | 1, 2, 3 | `session_argv`, `attempt(resume=)` and its `on_exec` wrapper, `argv(resume=)`, the harness pid record (1); `ended.reset` in that wrapper, `argv_select(ended=, fresh=)`, `resume_row`, Esc against end of input (2); `session_name` (3). |
| `charter/hooks.py` | 1 | `_record_harness_session` gated on the registry, with the adoption rule (adopt, follow, ignore by `CLAUDE_PID`; first report per start for Codex); `_record_reported_session` for opencode. |
| `charter/frame/leave.py` | 1, 2, 3, 4 | `resumable_harness`, `conversation_exists`, `Doomed.conversation` (1); `Doomed.ended`, `needs_confirming`, `CLOSE_NOW_ID`, the ended note (2); `Doomed.title` (3); `OPEN_QUIT`'s words (4). |
| `charter/frame/reopen.py` | 1, 2, 3 | `Chat.conversation` (1), `Chat.ended` (2), `Chat.title` (3). |
| `charter/commands_frame.py` | 1, 2, 3, 4 | Kept-id claim and its proof, `_resumes`, `_restore_recorded_chat`, `_reopen_one` (1); the ended hook and its escape-hatch branch, drawn mark, late check, operator-tmux loop, `cmd_close`, capture skip, reopen ended (2); `cmd_rename`, palette doorway, handoff title (3); `conf_text` binds carry the presser, `cmd_palette` reads it (4). |
| `charter/frame/overlay.py` | 2 | `Surface.left` records a key cancel against end of input. |
| `charter/frame/layout.py` | 2 | `respawn_argv(kill=True)`; the ended step passes `kill=False`. |
| `charter/frame/selector.py` | 2, 3 | Resume row, `Choice.resume`, `END_OF_INPUT`, the ended footer (2); title row (3). |
| `charter/frame/chats.py` | 2, 3 | `Chat.ended` (2); `Chat.title`, `label_of` (3). |
| `charter/frame/slots.py` | 2, 3, 4 | `ENDED_MARK` (2); strip labels (3); `GATE_BUTTON`, `_Doors` gate columns, no button in the operator's tmux (4). |
| `charter/frame/tabmenu.py` | 2, 3 | Close-now row on an ended tab (2); rename row (3). |
| `charter/frame/builtin_actions.py` | 4 | `_detach(fid, client)` presser-only (#1097); `build(..., client=)`. |
| `charter/frame/builtins.py` | 4 | `_strip_events` reads the recorded presser and opens the gate from a gate door. |
| `charter/frame/choose.py` | 3 | The chat picker names titles. |
| `charter/instance.py` | 4 | `F10` reserved; a hotkey equal to it refused. |
| `charter/cli.py` | 1, 2, 3, 4 | `frame-launch --resume` (1); `frame-ended`, `frame-launch --ended --fresh`, `frame-palette --ended` (2); `frame-rename`, `--title-row` (3); `frame-palette --gate` (4). |
| `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md` | 2 | The 2026-09-15 amendment. |
| `CONTEXT.md` | 1, 2, 3, 4 | **Chat** (1), **Ended tab** (2), **Title** (3), **Exit gate** and **Presser** (4). |
| `docs/frame.md`, `docs/harnesses.md`, `docs/control-plane.md` | 1–4 | Docs with each task. |
| `docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md` | 2 | Dated notes at §4j and §5. |

There is no new `docs/*.md` page, so `pyproject.toml`'s force-include set does not change
(`test_docs_show.TestPagesShip.test_nothing_else_is_force_included`).

---
## Task 1: A chat id is never handed out again, and every tab is linked to one harness session

**Depends on:** nothing. **Closes:** #1101.

**Nothing about ending a chat changes here.** `pane-died[1]` is still `kill-window`, and no surface
offers anything new except resume for Codex and opencode.

### Step 0 — taken 2026-09-15

The readings are in the controller's `exit-gate-t1-step0.md` (session scratchpad). They were taken on:
- claude 2.1.272, run live on the operator's account, with 4 model requests to Haiku 4.5;
- codex-cli 0.147.0, read from source at `rust-v0.147.0`, not run;
- opencode 1.18.23, read from source at `v1.18.23`, not run;
- tmux 3.7c, on a throwaway `-L cp-t1-*` server.

**No C-reading failed, so the stop rule did not fire.** Two readings were added after the second spec
review:
- **X3**, read from source;
- **C7**, measured live with no prompt.

The PR copies the table into its description.

| # | Reading | Verdict | What it settles |
|---|---|---|---|
| C1 | `claude --session-id <uuid> --name "t1 · beta.1"`, quit before any prompt | **pass** | SessionStart `session_id == <uuid>`, `source=startup`, `transcript_path` present. The file does **not** exist at SessionStart, nor after a quit with no prompt. So a chat with no prompt yet is not offered resume |
| C2 | the same, after one prompt | **pass** | the transcript file exists |
| C3 | `claude --resume <uuid> --name "t2 · beta.1"` | **pass** | the conversation comes back, with the **same** `session_id`, `source=resume`, and the new name written with no prompt. A fresh `claude --resume` picker lists `t2 · beta.1`. The in-session `/resume` hides the current session |
| C4 | `claude --resume <uuid> --session-id <other>` | **recorded** | refused (`--session-id can only be used with --continue or --resume if --fork-session is also specified`, exit 1). Charter never combines them |
| C5 | a nested `claude -p` from a chat's Bash tool | **recorded** | a new `session_id` and its own SessionStart. The environment is identical (`CLAUDECODE`, `TMUX_PANE`, `CHARTER_*` inherited). `CLAUDE_PID` differs: 44188 for the outer harness, 44929 for the nested one, each equal to its hook's `$PPID` |
| C6 | `/clear` | **recorded** | SessionStart with `source=clear`, a **new** `session_id` and the same `CLAUDE_PID`. `claude --resume <original uuid>` still brings back the pre-clear conversation |
| X1 | Codex SessionStart | **pass**, read from source | `session_id` and `transcript_path` (`codex-rs/hooks/src/schema.rs:486-497`); the rollout exists before the hook runs (`core/src/session/mod.rs:4074-4085`). SessionStart runs at the **first turn**, not at launch (`core/src/session/turn.rs:233`, `:457`) |
| X2 | `codex resume <id>` | **pass**, read from source | a UUID is looked up exactly (`tui/src/lib.rs:626-659`); a non-UUID falls through to a name lookup |
| O1 | opencode `tool.execute.before` input | **pass**, read from source | `sessionID`, shaped `ses_` + 12 hex + 14 base62, which fits `SESSION_ID_RE` |
| O2 | `opencode -s <id>` | **pass** (medium-high), read from source | validates the id with `directory: cwd` (`src/cli/tui/validate-session.ts:7-28`), so **resume must run in the chat's recorded directory** |
| X3 | a resumed Codex start: what `session_id` its SessionStart reports, and whether `codex resume` then accepts it | **pass**, read from source at `rust-v0.147.0`, not run | a resumed start keeps the resumed thread's id, and takes its session id from that rollout's own `SessionMeta` (`core/src/session/session.rs:570-594`); a root session reports the id it had, which `codex resume` accepts (X2). The link rule below holds for either answer |
| C7 | charter's real hook command forms, with no prompt: `os.getppid()` against `CLAUDE_PID`, and `CLAUDE_CODE_SESSION_ID` against the payload's `session_id`, at launch and after `/clear` | **recorded** (claude 2.1.272, 0 prompts; `exit-gate-t1-step0.md` section C7, raw evidence in `c7-evidence/`) | `CLAUDE_CODE_SESSION_ID == session_id` in all 10 hooks, at startup and after `/clear`, where it changed with the id and `CLAUDE_PID` did not. `getppid() == CLAUDE_PID` held for a lone command and failed for compound ones (Claude runs `/bin/sh -c`, which leaves one or two shells in between), so it is not a proof. A nested `!` child and a nested `codex` were not measured. Two `send-keys C-c` did not end Claude at startup, so no reading depends on a double Ctrl+C |

**What the readings changed in this task, by the controller's ruling:**
- **Claude Code's link adopts, follows `/clear`, and ignores a nested harness,** by
  `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID` together.
- **Codex and opencode adopt the first report of each start.**
- **A Codex chat has no link until its first turn.**
- **opencode resumes only in the chat's recorded directory.**
- **A resumed Codex start keeps the link it resumed** (X3).
- **The harness that sent a report is proven by the report's own invocation, never by inherited
  environment.** A Claude Code report needs `CLAUDE_CODE_SESSION_ID == session_id` (C7), and a
  `CLAUDE_PID` that adopts or follows.

### Files

- `charter/frame/state.py`:
  - **`new_chat_id` (`:193-279`).** Under the lock:
    - `start = highest_ordinal(prefix) + 1`;
    - for each candidate `n` (at most `_CHAT_ORDINAL_MAX` attempts, never past `99_999`), first
      `_raise_mark(prefix, n)`, then `claim_private_dir`;
    - a failed mark write → `None`, with no directory made.

    `_CHAT_ORDINAL_MAX`'s note (`:132-141`) becomes a bound on attempts.
  - **New beside it:**
    - `CHAT_IDS`, `CHAT_IDS_LOCK`, `ORDINAL_CEILING = 10 ** chats._MAX_ORDINAL_DIGITS - 1`;
    - `_locked()`, a context manager that opens the lock with `config.open_for(root / CHAT_IDS_LOCK,
      "a")` and takes `fcntl.flock`;
    - `_mark(prefix)`, `_raise_mark(prefix, n) -> bool`, `_traces(prefix)`, `highest_ordinal(prefix)`,
      `ordinal_of(chat)`;
    - `claim_chat_id(chat)`, `reap_chat(chat)`.
  - **`record_harness_session` (`:692-746`)** refuses a sid outside `SESSION_ID_RE`.
  - **After `kept_harness_session` (`:821-843`):**
    - `record_conversation`, `conversation`, `clear_conversation`;
    - `record_harness_pid`, `harness_pid`;
    - `adopt_report` (the `session.adopted` claim, with `config.create_for`) and `clear_adoption`.

    `clear_shape`'s tuple (`:1852`) lists none of them.
  - **`chat_in_pane(pane, server)`** after `harness_pane` (`:676`). Two or more matches answer `None`.
  - **The recycled-ordinal docstrings:**
    - `new_chat_id`'s rename paragraph (`:235-241`, kept);
    - `clear_exit` (`:591-596`), `clear_shape`, `clear_respawn`;
    - `_forget_session` (`:2625-2636`);
    - `reap`'s empty-directory note (`:2819-2823`).
- **`charter/harness/base.py`**, after `first_message_argv`:
  - `chooses_session_id`, `names_its_transcript`, `session_flags`, `reports_session_at`;
  - `reports_harness_pid`, `resume_needs_cwd`;
  - `new_session_argv`, `resume_argv`.
- **`charter/harness/claude_code.py`, `codex.py`, `opencode.py`** — the overrides (below).
- `charter/frame/launcher.py`:
  - **`argv` (`:192-204`)** takes `resume: bool = False`.
  - **`session_argv(p, fid, *, resume, rest)`** and `RESUME_GONE` are new.
  - **`attempt` (`:463-498`)** takes `resume: bool = False`:
    - the session words go before `rest`;
    - its `on_exec` wrapper (below) records the start;
    - the undo restores what the wrapper changed.
  - **`cmd_frame_launch` (`:772-813`)** passes `args.resume`.
- **`charter/cli.py`** — `frame-launch` (`:1195-1210`) gains `--resume`, `action="store_true"`.
- `charter/hooks.py`:
  - **`_record_harness_session` (`:5976-6027`)** asks `sender(data)` which harness sent the report, and
    never reads `$CHARTER_HARNESS` or trusts `$CLAUDE_PID` alone. It then calls
    `_record_harness_report(chat, h, data)`, which records `transcript_path` when
    `names_its_transcript`.
  - **`_record_reported_session(data)`** is new, called beside every `_turn_bump()`, for
    `reports_session_at == "tool"`.
- `charter/frame/leave.py`:
  - `Doomed.conversation: str = ""`, which `plan` fills.
  - `resumable_harness` (`:409-426`) asks the registry, and its docstring gives the reason the bar is
    met.
  - New `conversation_exists(harness, link, conversation)`, which `_resume_clause` (`:391-406`) and
    `summary` (`:292`) ask.
  - `_group`'s note (`:249-254`).
- `charter/frame/reopen.py` — `Chat.conversation: str = ""`, `_chat`'s key list (`:360-362`), and the
  notes at `:104-117`.
- `charter/commands_frame.py`:
  - **`_launch` (`:5880-5893`).** A reopen claims its kept id through `_claim_kept_id(restoring.chat)`
    (below), and an ordinary launch calls `new_chat_id`. The refusal sentence names both causes.
    The launcher argv sites pass `resume=getattr(args, "resume", False)`.
  - **`_resumes` (`:10724-10737`)** asks `leave.conversation_exists`.
  - **`_restore_recorded_chat` (`:10740-10805`)** writes `rec.resume` and `rec.conversation` into the
    claimed directory.
  - **`_reopen_one`:**
    - `rest = []`, `resume=_resumes(c)` (`:11243-11245`, `:11278-11280`);
    - for a harness with `resume_needs_cwd`, only when `_same_directory(where, c.cwd)`, and otherwise
      a sentence saying why it reopens empty.
  - **`_reopen_args` (`:11306-11319`)** takes `resume`.
  - **`_record_the_plane` (`:10368-10376`)** writes `conversation`.
  - **`KEPT_ID_TAKEN`** and `_claim_kept_id`.
  - **Notes** at `:6505-6509` and `:10808-10814`.
- **`charter/frame/chats.py`** — `_MAX_ORDINAL_DIGITS`' note (`:355`).
- **Tests:** the three Task 1 modules, and the existing cases below.
- **Docs:** `docs/frame.md`, `docs/harnesses.md`, `CONTEXT.md`, `docs/adr/0024-…`, news.

### Interfaces

**Consumes (on `main`):**
- `config.claim_private_dir`, `replace_for`, `create_for`, `open_for`, `write_for`, `SESSIONS_DIR`
- `state.workspace_prefix` (`:82`), `_root` (`:189`), `frame_dir` (`:313`), `_record_claim` (`:282`),
  `_claiming_pid` (`:2495`), `_launcher_is_alive` (`:2526`), `harness_pane` (`:676`), `frame_server`
  (`:875`), `identity` (`:1912`), `_forget_session` (`:2622`)
- `reopen.path` (`:196`), `reopen.TRANSCRIPT_SUFFIX` (`:71`)
- `commands_frame._chat_pane_rows` (`:9996`), `_this_plane` (`:1315`), `_same_directory` (`:11130`),
  `_restore_root` (`:11057`)
- `launcher.framed_chat` (`:520`); `harness.registry.get`; `tmuxctl.same_server`, `PANE_ID_RE`,
  `nothing_listening` (on `main` since #1100, `00d69f2`, `tmuxctl.py:347`);
  `contain.PATH_DISPLAY_LIMIT`; `chats._MAX_ORDINAL_DIGITS`

**Produces:**

```python
# charter/frame/state.py
CHAT_IDS = "chat-ids.json"      # {prefix: highest ordinal ever handed out}, a FILE in the frame root
CHAT_IDS_LOCK = "chat-ids.lock"
ORDINAL_CEILING = 99_999         # 10 ** chats._MAX_ORDINAL_DIGITS - 1
SESSION_ID_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,127}")

def new_chat_id(workspace: str) -> str | None: ...
    # None as today, and when the lock cannot be opened, the mark cannot be written, or the next
    # ordinal would pass ORDINAL_CEILING
def claim_chat_id(chat: str) -> bool: ...  # claim exactly *chat* (a restore), under the lock, mark first
def reap_chat(chat: str) -> None: ...      # rmtree the chat's directory, then _forget_session(chat)
def ordinal_of(chat: str) -> int | None: ...   # 1..ORDINAL_CEILING after the last dot, else None
def highest_ordinal(prefix: str) -> int: ...   # max(mark, traces); never raises
def record_harness_session(fid: str, sid: str) -> bool: ...   # refuses sid outside SESSION_ID_RE
def record_conversation(fid: str, path: str) -> bool: ...     # absolute, no NUL, <= PATH_DISPLAY_LIMIT
def conversation(fid: str) -> str | None: ...
def clear_conversation(fid: str) -> None: ...
def record_harness_pid(fid: str, pid: int) -> None: ...       # harness.pid; pid > 0
def harness_pid(fid: str) -> int | None: ...
def adopt_report(fid: str) -> bool: ...    # create_for(session.adopted); True only for the first caller
def clear_adoption(fid: str) -> None: ...  # removes session.adopted and harness.pid
def record_start(fid: str, *, resumed: bool) -> None: ...   # session.start: "resumed" or "fresh"
def resumed_start(fid: str) -> bool: ...
def chat_in_pane(pane: str, server: str) -> str | None: ...   # exactly one match, else None

# charter/harness/base.py — class Harness
chooses_session_id: bool = False
names_its_transcript: bool = False
reports_harness_pid: bool = False     # the hook environment carries the harness's own pid
resume_needs_cwd: bool = False        # the resume looks the id up in the working directory
session_flags: tuple[str, ...] = ()
reports_session_at: str = ""          # "sessionstart" | "tool" | ""
def new_session_argv(self, sid: str, name: str) -> list[str]: ...
def resume_argv(self, sid: str, name: str) -> list[str] | None: ...

# claude_code: chooses_session_id, names_its_transcript, reports_harness_pid = True;
#   reports_session_at = "sessionstart";
#   session_flags = ("--session-id", "--resume", "-r", "--continue", "-c", "--fork-session");
#   new_session_argv -> ["--session-id", sid, "--name", name];
#   resume_argv -> ["--resume", sid, "--name", name]
# codex:    names_its_transcript = True; reports_session_at = "sessionstart";
#   session_flags = ("resume", "fork"); resume_argv -> ["resume", sid]
# opencode: resume_needs_cwd = True; reports_session_at = "tool";
#   session_flags = ("-s", "--session", "-c", "--continue"); resume_argv -> ["-s", sid]

# charter/frame/launcher.py
RESUME_GONE = ("the conversation this chat was linked to cannot be resumed here — starting a fresh "
               "one on profile '{name}'.")
def argv(profile: str, rest: list[str], *, attended: bool, resume: bool = False) -> list[str]: ...
def session_argv(p, fid: str | None, *, resume: bool, rest: list[str]) -> tuple[list[str], str]: ...
def attempt(p, rest, *, fid, attended, resume: bool = False, on_exec=...) -> Refusal | None: ...

# charter/frame/leave.py
def resumable_harness(name: str) -> bool: ...
def conversation_exists(harness: str, link: str, conversation: str) -> bool: ...

# charter/commands_frame.py
KEPT_ID_TAKEN = ("charter reopen: {chat} is not reopened — {dir} belongs to a chat that is still "
                 "{what}, so opening it would give two chats one id. It stays recorded; {fix}, then "
                 "run charter reopen again.")
def _claim_kept_id(rec) -> tuple[str | None, str]: ...   # (the claimed id, or None and the sentence)

# charter/hooks.py
CODEX_SESSIONSTART_KEYS = frozenset({"session_id", "transcript_path", "cwd", "hook_event_name",
                                     "model", "permission_mode", "source"})
def sender(data: dict) -> "Harness | None": ...   # Claude Code: CLAUDE_CODE_SESSION_ID == session_id (C7); Codex: the exact key set
def _record_harness_report(chat: str, h, data: dict) -> None: ...
def _record_reported_session(data: dict) -> None: ...
```

### Behaviour

**The allocator.**
1. `prefix = workspace_prefix(workspace)`, then `config.private_mkdir(root)`. An `OSError` is `None`.
2. `with _locked(root):`. An `OSError` opening or locking is `None`.
3. `start = highest_ordinal(prefix) + 1`. For `n` in `range(start, min(start + _CHAT_ORDINAL_MAX,
   ORDINAL_CEILING + 1))`:
   - `if not _raise_mark(prefix, n): return None` — the id record could not be written, so nothing
     is handed out;
   - `claim_private_dir(root / f"{prefix}.{n}")`: `FileExistsError` → continue; any other `OSError` →
     `None`;
   - on success, `_record_claim(d)`, then return the name.

   Falling out of the loop is `None`.
4. **The `mkdir` stays the exclusion.** A racer not holding the lock, such as an older charter across
   the upgrade, still cannot share a name.

**`_raise_mark(prefix, n)`** reads `CHAT_IDS` (unreadable → `{}`), sets `max(existing, n)`, and
`config.replace_for`s it. It answers `False` on `OSError`, and is called only under the lock.

**`highest_ordinal(prefix)`** takes the max of these, and never raises:
- the mark (a non-`int` or negative value reads as 0);
- every trace whose digits parse and have at most `chats._MAX_ORDINAL_DIGITS` digits. Longer names are
  ignored:
  - `os.scandir(root)` entries named `{prefix}.{n}` or `{prefix}.{n}.transcript`;
  - every `chat` string reachable as `frames[*].chats[*].chat` in `reopen.json` read raw;
  - `SESSIONS_DIR` entries splitting as `[prefix, n, *family]`.

`prefix` holds no dot, so an old `{workspace}-{pid}` id never counts.

**`claim_chat_id(chat)`** requires `chats.is_chat(chat)`, `ordinal_of(chat)`, and a dot-free prefix.
Then, under the lock: `_raise_mark(prefix, n)`, `claim_private_dir`, `_record_claim`.
`FileExistsError`, `OSError` or a failed mark → `False`.

**`_claim_kept_id(rec)`** — the path that can succeed:
1. `state.claim_chat_id(rec.chat)` → `(rec.chat, "")`.
2. Otherwise, if `state._claiming_pid(dir)` is alive → `(None, KEPT_ID_TAKEN)`, with *what* = `being
   started`, *fix* = `wait for that launch`.
3. `rows = _chat_pane_rows(server)` for `server = state.frame_server(rec.chat) or
   tmuxctl.LEGACY_SOCKET`. When `rows is None`:
   - **gone** — `tmuxctl.nothing_listening(server)` → `rows = []`;
   - **anything else**, a timeout included (#1100 measured a SIGSTOP'd server timing out while its
     chats were live) → `(None, KEPT_ID_TAKEN)`, with *what* = `on a tmux server that did not answer`,
     *fix* = `run charter reopen again once that server answers, or remove <dir> if you know it is
     gone`. Nothing is reaped.
4. `live = [r for r in rows or () if r[0] == rec.chat and r[3] == _this_plane()]`.
   - **None live** → `state.reap_chat(rec.chat)`, then `claim_chat_id` again.
   - **Some live** → `(None, KEPT_ID_TAKEN)`, with *what* = `running`, *fix* =
     `shlex.join(tmuxctl.server_argv(server, "kill-window", "-t", live[0][1]))`, and *dir* the
     contained directory path.

A pane of another plane with that id, or one with no plane marker, is not this plane's.

**`launcher.session_argv(p, fid, *, resume, rest)`:**
1. `fid is None` → `([], "")`.
2. `h = registry.get(p.harness)`; `None` → `([], "")`.
3. Any word of `rest` in `h.session_flags` → `([], "")`.
4. `name = fid`. Task 3 replaces this line with `session_name(fid)`.
5. `resume`, `link := state.kept_harness_session(fid)`, and `(args := h.resume_argv(link, name))` →
   `(args, "")`.
6. `resume` otherwise → `util.err(f"charter: {RESUME_GONE…}")`, then fall through.
7. `h.chooses_session_id` → `sid = str(uuid.uuid4())`, `(h.new_session_argv(sid, name), sid)`.
8. Otherwise → `([], "")`.

**`attempt`'s `on_exec` wrapper — every start, and only a start, resets the adoption:**
- **Before:** read the previous link, conversation and harness pid.
- **Always:** `state.clear_adoption(fid)`, so the start adopts its own first report again, and
  `state.record_start(fid, resumed=resume)`, which a Codex report reads (X3).
- **A chosen id:** `record_harness_session(fid, chosen)` and `clear_conversation(fid)`.
- **A fresh start of a harness that chooses no id** (`not resume and not chosen`): the link and
  conversation are cleared, so an earlier start's conversation is not offered as this start's.
- **Then** the caller's `on_exec`.
- **The undo** restores the link, conversation, harness pid and adoption, then runs the caller's undo.

**Hooks.**

`_record_harness_session(data)`:
1. `chat = _chat_id()`, and the plane check.
2. `h = sender(data)`, and `h.reports_session_at == "sessionstart"`.
3. `state.identity(chat)["CHARTER_HARNESS"] == h.name`. `$CHARTER_HARNESS` is never read here, because
   a nested harness inherits it.
4. `_record_harness_report(chat, h, data)`.

**`sender(data)` — which harness sent this report, decided by the report's own invocation.** Claude
Code and Codex run the same plugin command (`hooks/hooks.json`, `codex.py:84-97`), so the command
cannot say.
- **Claude Code**, when `$CLAUDE_CODE_SESSION_ID` is set and equals `data["session_id"]`. C7 measured
  them equal in all 10 of charter's hooks, at startup and after `/clear`.
  - A nested non-Claude harness inherits the outer id and reports its own, so it fails. That is
    inferred for `codex` and not measured.
  - `os.getppid() == $CLAUDE_PID` is not used. Behind Claude's `/bin/sh -c`, a compound command keeps a
    shell in between (C7).
- **Codex**, when the report is not Claude Code's and `frozenset(data) == CODEX_SESSIONSTART_KEYS`.
  Codex declares that input with `deny_unknown_fields` (`codex-rs/hooks/src/schema.rs:486-497`).
- **`None`** otherwise, and nothing is recorded.

`_record_harness_report(chat, h, data)`:
- `sid` must be a `str` matching `SESSION_ID_RE`; otherwise return.
- `link = state.kept_harness_session(chat)`.
- **`h.reports_harness_pid` (Claude Code):**
  - `pid = int(os.environ.get("CLAUDE_PID", ""))` when it is digits > 0, else `None`.
  - `adopted = state.harness_pid(chat)`.
  - **Adopt:** `adopted is None and (sid == link or not link)` and `pid` → record the sid, the
    harness pid (`record_harness_pid`) and `adopt_report`.
  - **Follow:** `adopted is not None and pid == adopted and sid != link` → record the sid, and clear
    the conversation until this report's path is recorded.
  - **Anything else is ignored**, and never recorded:
    - a pid that differs, carrying an id that is not the recorded one;
    - a report with no `CLAUDE_PID` — before adoption it never adopts;
    - a report before adoption whose id is not the chosen one.
  - **The adopted pid is learned from the adopting report, never from `#{pane_pid}`.** Behind a wrapper
    profile the pane pid is the wrapper's.
- **Otherwise (Codex):**
  - **A fresh start:** `state.adopt_report(chat)` → record the sid. A `False` is ignored: this start
    already adopted.
  - **A resumed start** (`state.resumed_start(chat)`): the link stays the id it resumed with (X3).
    `adopt_report` is still claimed, and the report's `transcript_path` is recorded only when its sid
    equals the link.
- **A recorded report** with `h.names_its_transcript` also records `transcript_path` through
  `record_conversation`.

`_record_reported_session(data)`, for opencode (ruled OQ1 (A)):
1. `h.reports_session_at == "tool"`.
2. `pane` from `$TMUX_PANE`, held to `PANE_ID_RE`; `server` is `$TMUX`'s first comma field.
3. `chat = state.chat_in_pane(pane, server)`, whose kind is `h.name`.
4. `state.adopt_report(chat)` → record the sid.

`_chat_id()` is never read, because it holds opencode's own id (`opencode.py:332`).

Both functions are wrapped in `except Exception: pass`.

**`leave.conversation_exists(harness, link, conversation)`:**
```python
h = registry.get(harness)
if not link or h is None or h.resume_argv(link, "") is None:
    return False
if h.names_its_transcript:
    return bool(conversation) and os.path.isfile(conversation)
return True   # opencode: a tool hook reported the session, and it names no transcript
```

A Codex chat that has taken no turn has no link (X1), so it has no resume. A Claude Code chat with no
prompt has a link and no file (C1), so it has no resume either.

### Test cases (write first; each must fail before the code exists)

**`tests/test_a_chat_id_is_never_handed_out_again.py`**

Class `TheCounterOnlyGrows(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_reaped_ordinal_is_not_handed_out_again` | `beta.1`; `shutil.rmtree` it; the next is `beta.2`. Red at `5ad755d` |
| `test_the_mark_is_a_file_in_the_frame_root` | `json.loads((config.STATE_DIR / "frame" / "chat-ids.json").read_text()) == {"beta": 1}`, spelled literally |
| `test_one_mark_per_prefix_not_per_workspace_name` | `a.b` then `a_b` → `a_b.1`, `a_b.2` |
| `test_a_lost_mark_falls_back_to_the_traces` | allocate two; delete the mark and both directories, keep `beta.2.transcript` → `beta.3` |
| `test_an_unreadable_mark_still_counts_the_traces` | the mark holds `not json`, a directory `beta.7` exists → `beta.8` |
| `test_the_mark_never_goes_down` | `{"beta": 9}`, `_raise_mark("beta", 3)` → still 9 |
| `test_the_mark_is_written_before_the_directory` | `config.claim_private_dir` patched to assert `json.loads(chat-ids.json)["beta"] >= <its ordinal>` when called |
| `test_a_mark_that_cannot_be_written_hands_out_nothing` | `config.replace_for` raises `OSError` for `chat-ids.json` → `None`; no `beta.*` directory exists |
| `test_the_lock_is_opened_through_config` | `config.open_for` recorded → called with `root / "chat-ids.lock"` before any mark write |
| `test_the_attempt_bound_counts_from_the_start` | mark `{"beta": 20000}` → `beta.20001`. Red at `5ad755d` |
| `test_no_ordinal_past_the_ceiling_is_handed_out` | mark `{"beta": 99999}` → `None` |
| `test_a_taken_ordinal_is_still_skipped` (pin) | a directory `beta.1` exists → `beta.2` |

Class `EveryTraceCounts(PersonaIso, unittest.TestCase)`, with no `chat-ids.json`:
- A directory `beta.4` → `beta.5`.
- A transcript `beta.6.transcript` → `beta.7`.
- `reopen.json` naming `beta.9` → `beta.10`.
- `{"version": 99, "frames": [{"chats": [{"chat": "beta.12"}]}]}` → `beta.13`.
- `SESSIONS_DIR / "beta.3.workspace"` → `beta.4`.
- `test_names_that_are_not_this_prefix_do_not_count`: `betamax.9`, `beta-9`, `beta.x`, `beta.9x` →
  `beta.1`.
- `test_a_name_past_the_digit_bound_is_ignored_not_raised`: a directory `beta.123456` and
  `SESSIONS_DIR / "beta.9999999999999999999999.lock"` → `beta.1`, no exception.
- `test_chat_turns_is_not_a_trace` (pin): `STATE_DIR / "chat-turns" / "beta.8"` → `beta.1`.

Class `TwoAllocatorsNeverShareAnId(PersonaIso, unittest.TestCase)`:
- **Eight threads, five allocations each** → 40 distinct ids, and the mark is 40.
- **`test_the_mark_is_raised_while_the_lock_is_held`** — `fcntl.flock` and `config.replace_for`
  recorded → `LOCK_EX` before the replace and `LOCK_UN` after. Red when the lock is deleted.
- **A lock that cannot be opened** (`config.open_for` raises) → `None`, and no directory.

Class `NoScanReadsTheMarkAsAChat(PersonaIso, unittest.TestCase)` — neither file shows up anywhere:
- `leave.plane_chats()` and `chats._by_workspace()` hold neither `chat-ids.json` nor `chat-ids.lock`.
- `state.reap(set(), server=tmuxctl.LEGACY_SOCKET)` and `state.reap(set(), server="x")` leave both.
- `reopen.prune_transcripts(set())` leaves both.

Class `ARestoredChatKeepsItsId(PersonaIso, unittest.TestCase)` — `_chat_pane_rows` stood in per case:

| Case | Asserts |
|---|---|
| `test_claiming_a_recorded_id_makes_exactly_that_directory` | `claim_chat_id("beta.7")`; the `launcher` file holds this pid; the mark ≥ 7 |
| `test_a_name_that_is_not_a_chat_id_is_not_claimed` | subTest `"../x"`, `"beta-7"`, `"beta"`, `"beta.7.transcript"` → `False` |
| `test_a_reopen_launch_claims_the_recorded_id` | `_launch` with `reopening=Reopening(reopen.Chat(chat="beta.7", …))`, under `TheLaunchOpensWithoutMovingAnyone`'s patches (`tests/test_a_chat_opens_in_the_background_with_its_first_message.py:437-461`) → `reopening.fid == "beta.7"`, and the `new-window`/`new-session` argv names it |
| `test_a_surviving_directory_no_pane_proves_live_is_reaped_and_claimed` | directory `beta.7` holding `server`; rows `[]` → `_claim_kept_id` answers `"beta.7"`; the directory is new (no `server` file); `SESSIONS_DIR / "beta.7.workspace"` gone |
| `test_a_server_that_timed_out_refuses_and_leaves_the_directory` | rows `None`, `tmuxctl.nothing_listening` → `False` → `None`; the sentence holds the directory and `did not answer`; `beta.7`'s files are byte-identical and `state.reap_chat` is not called |
| `test_a_server_that_is_gone_allows_the_reap_for_that_chat_only` | rows `None`, `nothing_listening` → `True`; directories `beta.7` and `beta.8` → `beta.7` claimed; `beta.8` untouched |
| `test_a_server_that_answers_without_the_chat_allows_the_reap` | rows hold only `gamma.2`'s pane, of this plane → `beta.7` reaped and claimed |
| `test_another_planes_pane_is_not_this_chats_life` | a row `beta.7 @3 1 /other/plane %4` → reaped and claimed |
| `test_an_unmarked_pane_is_not_this_chats_life` | a row `beta.7 @3 1  %4` (empty plane) → reaped and claimed |
| `test_a_live_pane_of_this_plane_refuses_naming_the_directory_and_the_command` | a row `beta.7 @3 1 <this plane> %4` → `None`; the sentence holds the directory and `kill-window -t @3`; the directory untouched |
| `test_a_live_launcher_claim_refuses_without_asking_tmux` | `launcher` file holds this test's own pid → `None`; `_chat_pane_rows` not called; `"being started"` |
| `test_a_refused_reopen_stays_recorded` | after `_reopen_one` and `_consume`, the manifest still names `beta.7` |
| `test_an_ordinary_launch_never_takes_a_recorded_id` | the manifest names `beta.3`, no directories → `beta.4` |

Class `AClosedChatsLeftoversAreNeverInherited(PersonaIso, unittest.TestCase)` — #1101:
- **setUp:** `default.1` with an identity and a workspace, `default.1.transcript`, a `reopen.json`
  naming it, `record_closed`, `rmtree`.
- `test_the_next_chat_is_not_the_closed_chats_id` — `new_chat_id("default") == "default.2"`.
- `test_the_new_chat_is_not_offered_the_old_transcript` —
  `builtin_actions._has_transcript("default.2") is False`.
- `test_closing_the_new_chat_leaves_the_old_record` — `_forget_transcript("default.2")` leaves
  `default.1` in the manifest and its transcript file in place.

Class `TheRecordIsWrittenDown(unittest.TestCase)` — files read off the repository root:
- **One ADR:** exactly one `docs/adr/*-a-chat-id-names-one-chat-and-one-harness-session.md`, and its
  first line starts with `# `.
- **Its costs:** the text holds `CLAUDE_PID`, `/new`, `99,999`.
- **CONTEXT.md:** `**Chat**:` holds `handed out once`.

**`tests/test_a_tab_is_linked_to_one_harness_session.py`**

Class `EachHarnessSaysHowItsSessionIsNamed(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_claude_code` | `new_session_argv("u", "n") == ["--session-id", "u", "--name", "n"]`; `resume_argv("u", "n") == ["--resume", "u", "--name", "n"]`; `chooses_session_id`, `reports_harness_pid` |
| `test_codex` | `new_session_argv == []`; `resume_argv("s", "n") == ["resume", "s"]`; not `resume_needs_cwd` |
| `test_opencode` | `resume_argv("s", "n") == ["-s", "s"]`; `resume_needs_cwd`; `reports_session_at == "tool"` |
| `test_resumable_is_the_registrys_answer` | a stand-in entry whose `resume_argv` answers `None` → `False` |

Class `TheLauncherAddsTheSession(PersonaIso, unittest.TestCase)`:
- **Stand-ins:** `launcher.os.execvpe` patched; `framed_chat` → `beta.1`; approval and wiring stood in.
- **Setup:** `beta.1` records kind `claude-code`.

| Case | Asserts |
|---|---|
| `test_a_framed_claude_start_gets_a_fresh_uuid_and_its_id_as_name` | `cmd_frame_launch(profile="claude", rest=["--", "-p", "x"])` → `["claude", "--session-id", u, "--name", "beta.1", "-p", "x"]`, `uuid.UUID(u).version == 4`, the link is `u` inside the `execvpe` fake |
| `test_every_start_clears_the_adoption` | `harness.pid` and `session.adopted` recorded → both gone inside the fake, for a fresh start and for `--resume` |
| `test_an_unframed_start_gets_no_session` | `framed_chat` → `None` → `["claude", "-p", "x"]` |
| `test_the_operators_own_session_flag_wins` | rest `["--resume", "abc"]` → no `--session-id`, the link unchanged |
| `test_a_resume_hands_the_link_back` | link `u`, `--resume` → `["claude", "--resume", u, "--name", "beta.1"]` |
| `test_a_resume_with_no_link_starts_fresh_and_says_so` | `"cannot be resumed here"` on stderr, and `--session-id` |
| `test_a_fresh_codex_start_forgets_the_last_starts_link` | codex, link `s1` → inside the fake, no link and no conversation; `--resume` with `s1` → `["codex", "resume", "s1"]` |
| `test_an_exec_that_raises_restores_everything` | link, conversation, harness pid, adoption all restored |
| `test_the_session_never_crosses_tmux` | `launcher.argv("claude", [], attended=True, resume=True)` holds `--resume` and no UUID-shaped word; `_launch`'s recorded tmux argvs hold no `--session-id` |

Class `ALinkFollowsOnlyTheChatsOwnHarness(PersonaIso, unittest.TestCase)` — `os.environ` patched
`clear=True` with `CHARTER_SESSION_ID=beta.1`, `CHARTER_HARNESS` per case and `CLAUDE_PID` per case.
`CLAUDE_CODE_SESSION_ID` equals each Claude report's `session_id` unless the case says otherwise:

| Case | Asserts |
|---|---|
| `test_the_first_report_of_the_chosen_id_adopts_its_pid` | link `u`, report `session_id=u`, `CLAUDE_PID=100`, `transcript_path=/abs/t` → `harness_pid == 100`, conversation `/abs/t` (C1) |
| `test_a_resume_reporting_the_same_id_re_adopts` | adoption cleared by a start; report `u`, `CLAUDE_PID=200` → `harness_pid == 200` (C3) |
| `test_clear_from_the_adopted_pid_moves_the_link` | adopted 100; report `v`, `CLAUDE_PID=100`, `source=clear` → link `v` (C6) |
| `test_a_nested_harness_is_ignored` | adopted 100; report `w`, `CLAUDE_PID=300` → link `u`, conversation unchanged (C5) |
| `test_a_report_without_claude_pid_after_adoption_is_ignored` | adopted 100; no `CLAUDE_PID` → unchanged |
| `test_a_report_of_another_id_before_adoption_is_ignored` | link `u`, no adoption, report `w` → unchanged |
| `test_a_start_that_chose_no_id_adopts_its_first_report` | no link; report `x`, `CLAUDE_PID=100` → link `x`, pid 100 |
| `test_codex_adopts_the_first_report_of_a_start_only` | kind `codex`; report `s1` → link `s1`; report `s2` → still `s1`; a start clears adoption; report `s3` → `s3`. Red at `5ad755d` for the first report |
| `test_a_nested_non_claude_report_is_ignored` | identity `claude-code`, adopted pid 100, link `u`; env `CLAUDE_PID=100` and `CLAUDE_CODE_SESSION_ID=u`, both inherited; payload `session_id=z` → not a Claude Code report, the link stays `u`, nothing recorded (C7; inferred for `codex`) |
| `test_a_session_id_variable_unequal_to_the_payload_is_not_a_claude_report` | `CLAUDE_CODE_SESSION_ID=u`, payload `u2` → ignored; equal → adopted |
| `test_a_missing_session_id_variable_never_adopts_and_is_ignored_after_adoption` | no `CLAUDE_CODE_SESSION_ID` → no adoption; adopted pid 100 and a report without it → unchanged |
| `test_the_parent_pid_is_never_consulted` | `os.getppid` patched to raise → an adopting report still adopts (C7: not a proof) |
| `test_charter_harness_never_decides_the_sender` | env `CHARTER_HARNESS=codex`, a Claude report whose proof holds → Claude Code; env `CHARTER_HARNESS=claude-code`, a Codex-shaped payload and no proof → Codex |
| `test_a_codex_payload_with_an_unknown_key_is_no_report` | Codex's keys plus `turn_id` → `sender` is `None` |
| `test_a_resumed_codex_start_keeps_the_link_it_resumed` | a start recorded `resumed`, link `s1`: a report `s1` → link `s1`, transcript recorded; a report `s9` → link `s1`, transcript not recorded (X3, both answers) |
| `test_an_id_that_could_be_read_as_a_flag_is_refused` | `"-rf"`, `"a b"`, `"x" * 200`, `""` → nothing |
| `test_a_relative_or_nul_path_is_refused` | `"t.jsonl"`, `"/a\x00b"`, a 5000-byte path → no conversation |
| `test_opencode_is_recorded_from_its_tool_hook_by_its_pane` | `CHARTER_HARNESS=opencode`, `CHARTER_SESSION_ID=ses_abc…`, `TMUX_PANE=%4`, `TMUX=<srv>,1,0`; `beta.1` records pane `%4` on `<srv>`, kind `opencode` → link `ses_abc…`; a second report `ses_def…` → unchanged |
| `test_a_pane_two_chats_record_resolves_to_none` | `beta.1` and `beta.2` both record `%4` on `<srv>` → `chat_in_pane` → `None`, nothing recorded |
| `test_a_pane_charter_did_not_record_writes_nothing` | `%9` → nothing |
| `test_the_hook_never_raises` | `state.record_harness_session` raises → both return `None` |

Class `ResumeIsOfferedOnlyWhereTheConversationExists(PersonaIso, unittest.TestCase)`:
- **A named transcript that is a file resumes.** `conversation_exists`, `_resume_clause == RESUMES`,
  and `_resumes` are all true.
- **The C1 case: a link with no transcript file** → `NO_RESUME_YET`.
- **The X1 case: a Codex chat with no link** → not resumable, and says `NO_RESUME_YET`.
- **An opencode report is enough.**
- **`test_opencode_resumes_only_in_its_recorded_directory`:** `c.cwd` outside the workspace, so
  `_restore_root` moves it → `_reopen_args(...).resume is False`, and the warning says why.
- **The quit summary counts what can resume:** one of two.
- **A reopen hands `resume` to the launcher, not `rest`:** `rest == []`, `resume is True`.

Class `TheLinkTravelsThroughTheRecord(PersonaIso, unittest.TestCase)`:
- The literal key `"conversation"` in `reopen.json`.
- A record from 0.62 reads as `""`.
- A restored chat has its link and conversation before tmux (inside the `new-window` fake).

**`tests/test_a_launch_hands_the_harness_its_session_on_a_real_server.py`** — real tmux, with the
fixtures of `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py`:
- `test_the_harness_argv_carries_the_chosen_id_and_tmux_never_saw_it` — the recorder argv holds
  `--session-id <u> --name <fid>`, and `#{pane_start_command}` does not hold `u`.
- `test_a_reopened_chat_comes_back_under_its_own_id`:
  - two chats, then `_record_the_plane` and `_stop_chats`;
  - `state.reap`;
  - `_reopen_one` for each → `r.fid == c.chat`, and the second recorder argv holds `--resume <u>`.

**Existing tests this task changes.** CI's four jobs name the rest; each is changed in this PR.

*Premise gone — ordinals are no longer recycled:*
- `tests/test_a_reaped_chat_leaves_its_session_behind.py::ARecycledOrdinalStartsFromNothing` (`:96`,
  `:110`, `:130`) asserts the reaped ordinal comes back. It is restaged onto a reopen that keeps its
  id through `claim_chat_id`, which is the one way a directory's markers can meet the same id again.
  `_forget_session`'s guard stays pinned.
- `tests/test_a_chat_belongs_to_its_workspace_for_life.py::TheOtherDoorMovesNoChatEither::test_a_pointer_left_behind_by_a_reaped_chat_decides_nothing`
  (`:378`) — its docstring's `lowest free ordinal` premise; the planted relaunch becomes a kept id.
- `tests/test_a_chats_harness_session_outlives_its_gauge.py::ItDoesNotOUTLIVETheChatItself::test_a_recycled_ordinal_cannot_inherit_the_previous_chats_durable_id`
  (`:198`) — restaged on a kept id.
- `tests/test_a_reopen_says_what_it_cannot_bring_back.py::WhatAReopenPutsBack::test_the_manifest_outranks_a_stale_pointer_left_on_a_recycled_ordinal`
  (`:470`) — restaged on a kept id.
- `tests/test_the_branch_not_taken_is_still_a_promise.py::RestoringAChatsRecordsChecksTheNameItWasGiven::test_a_recycled_ordinal_moves_no_transcript_onto_itself`
  (`:873`) — the equal-name case is now every reopen; renamed.
- Kept as reopen-into-its-own-directory pins, docstrings reworded:
  - `tests/test_a_key_cycles_the_tab_strips_height.py::TheHeightIsRememberedForThisFrameAndNoLonger::test_a_new_frame_claiming_a_recycled_id_does_not_inherit_it` (`:172`);
  - `tests/test_the_repo_table_scrolls_and_selects.py::ASelectionBelongsToOneFrame::test_a_new_frame_claiming_a_recycled_id_does_not_inherit_a_selection` (`:1090`).
- `tests/test_frame_state.py`:
  - `ChatIdIsAllocated::test_giving_up_is_bounded_rather_than_a_spin` — with traces, a taken name no
    longer exhausts the loop. It is restaged with `config.claim_private_dir` always raising
    `FileExistsError`, and `_CHAT_ORDINAL_MAX` patched to 2.
  - `ChatIdIsAllocated::test_a_taken_ordinal_is_skipped_rather_than_adopted` stays as a pin.
  - Docstrings re-read for recycling prose: `AClaimSurvivesASiblingsReap` (`:346`), `ClearExit`
    (`:669`), `ClearRespawn` (`:809`), `TheFramesOwnWorkspace::test_a_relaunch_on_the_same_id_overwrites_rather_than_keeps`
    (`:1115`).
- `tests/test_frame_launcher.py::Launch::test_a_launch_claims_an_id_no_directory_already_holds`
  (`:4217`) — its docstring's claim-by-`mkdir` paragraph gains the mark.
- `tests/test_a_chat_records_where_it_was_started.py::TheLauncherSTailIsWhereAReopenDiffers::test_a_launch_whose_plane_was_quit_names_the_command_that_undoes_it`
  (`:356`) — a reopen now gets the SAME ids by design; re-read its gate.

*The link and resume:*
- `tests/test_a_chats_harness_session_outlives_its_gauge.py::AHookIsTheWriterNow` — the non-Claude
  case flips for Codex. `test_a_claude_code_hook_records_the_id_reopen_asks_with` (`:281`) sets
  `CLAUDE_PID` and a chosen link.
- `tests/test_a_reopen_says_what_it_cannot_bring_back.py` and
  `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py` — the opencode resume sentence.
- `tests/test_a_second_launch_focuses_instead_of_dragging.py::TheLaunchTakesTheDecision::test_a_focused_launch_claims_no_ordinal_and_makes_no_directory`
  — also asserts `chat-ids.json` is not written.
- `tests/test_a_quit_records_the_plane_before_it_kills.py::WhatIsOnDiskIsAFormatAndNotAnImplementationDetail`
  — gains the `conversation` key.
- `tests/test_the_launcher_becomes_the_profile.py`:
  - `ThePaneCommandResolvesTheProfileByName::test_the_pane_execs_the_profile_it_was_named` (`:549`) —
    the exec argv gains `--session-id <u> --name <fid>` before its rest;
  - `TheLauncherBecomesTheProfile::test_the_profile_and_kind_ride_the_exec_not_tmux` (`:136`) is
    unchanged, because its `rest` carries `--resume`;
  - `TheLauncherBecomesTheProfile::test_an_exec_that_fails_after_on_exec_undoes_it` (`:188`) — the
    wrapper's undo runs as well as the caller's.
- `tests/test_quit_and_reopen_on_a_real_tmux.py::ARealQuitStopsRealChats` — id assertions after a
  reopen.

*Grep before finishing:* `grep -rn "recycl\|lowest free\|fresh ordinal\|same NAME back\|same ids back\|must be recycled" tests/ charter/`,
in both directions. A hit about pids or pane ids is not this task's.

### Implementation steps

- [ ] Step 0 is taken: copy its table into the PR description, C7 included.
- [ ] Write the three new modules and the changed cases.
- [ ] Run `( unset TMUX TMUX_PANE; python3 -m unittest tests.test_a_chat_id_is_never_handed_out_again tests.test_a_tab_is_linked_to_one_harness_session tests.test_a_launch_hands_the_harness_its_session_on_a_real_server )`
      — expect failures; note the counts.
- [ ] `state.py`: lock through `config.open_for`, mark before claim, digits bound and ceiling,
      traces, `claim_chat_id`, `reap_chat`, the link and adoption records, `chat_in_pane`.
- [ ] Harness members and overrides; `leave.resumable_harness`, `conversation_exists`,
      `_resume_clause`, `summary`, `Doomed.conversation`; `reopen.Chat.conversation`.
- [ ] `launcher.session_argv`, the `on_exec` wrapper, `argv(resume=)`; `frame-launch --resume`.
- [ ] `hooks`: the registry gate, `_record_harness_report` (adopt, follow, ignore; first report per
      start), `_record_reported_session`.
- [ ] `_launch`'s `_claim_kept_id`; `_restore_recorded_chat`; `_reopen_one`/`_reopen_args` with
      `resume_needs_cwd`; `_record_the_plane`.
- [ ] The grep above; reword each hit or restage its test.
- [ ] Re-run every new and changed module, plus `tests.test_plane_spawn_guard`; green.
- [ ] ADR 0024, CONTEXT.md, docs, news. Commit, push, read CI's four `test (3.x)` jobs and the
      sweep summary, and add a test per survivor.

### Docs and news

- **`docs/frame.md`.**
  - `## Leaving` (`:1188-1193`): a reopened chat comes back under its own id, and a closed chat's id
    is never handed out again.
  - The quit example (`:1167-1176`): the opencode row reads `conversation resumes`.
  - `**Resume is Claude Code only…**` (`:1325-1334`) becomes the table below, in prose.
- **`docs/harnesses.md`** `## What each harness lets charter offer` (`:105`) gains the resume table,
  with Step 0's versions:

  | Harness | Link | Offered when | Resumes with | Limits |
  |---|---|---|---|---|
  | Claude Code 2.1.272 | the id charter hands it; follows `/clear`; ignores a nested `claude` | after the first prompt (the transcript exists) | `--resume <id> --name <title · id>` | the in-session `/resume` hides the current session; the name shows in a fresh `claude --resume` picker, and in `/resume` from every other session |
  | Codex 0.147.0 | the first id it reports in each start | after its first turn (it reports nothing before) | `codex resume <id>` | after `/new`, resume offers the start's first conversation; read from source, not run |
  | opencode 1.18.23 | the first id its tool hooks report in each start | after its first tool call | `opencode -s <id>`, in the chat's recorded directory | a new session inside opencode is not followed; a chat moved to another directory reopens empty; read from source, not run |

- **`CONTEXT.md`** **Chat** (`:83-88`): *handed out once* and *linked to exactly one harness session*.
- **`docs/adr/0024-…`**: the spec's draft.
- **News** `docs/news/unreleased-a-chat-id-is-never-handed-out-again.md`:

  ```
  ---
  version: unreleased
  headline: A new chat is never given a closed chat's id — so it no longer inherits that chat's transcript or deletes its quit record — and Codex and opencode chats now resume
  ---
  ```

  The body covers:
  - #1101;
  - ids only grow, and a reopened chat keeps its id;
  - one session per tab, and that `/clear` is followed;
  - the resume table's limits.

  No `adopt:`.

**Suggested PR title:** A chat id is never handed out again, and every tab is linked to the harness session it holds (#1101)

---

## Task 2: A harness that ends keeps its tab

**Depends on:** task 1 on `main`. **Ruled before code:** open question 5 — the escape hatch keeps today's
ending.

### Step 0 — Measure first. A failed reading stops this task.

**Where.** Throwaway `-S /tmp/cp-t2-<slug>/s` servers, started `-f /dev/null`, on tmux 3.7c and at the
3.2 floor. The 3.2 binary comes first on `PATH` from a directory holding a `tmux` symlink, the convention
`tests/test_a_real_click_on_a_real_tab_bar_switches.py:34` records.

Two arms:
- **charter's own layout:** `layout.session_argv` for a first chat and `chat_window_argv` for a second,
  with `_plane_option_argv`;
- **the operator's:** `layout.window_argv`, `_remain_on_exit_argv`, `_plane_option_argv(window=True)`
  and `layout.respawn_argv`.

A recorder stands in for the harness. It exits with `$EXIT_WITH`, or is killed from outside with `TERM`
or `KILL`. No reading or test depends on a double Ctrl+C ending a harness: C7 saw two `send-keys C-c`
leave Claude running at startup. A clean exit is `/exit`, or the stand-in's exit 0.

| # | Reading | Pass when |
|---|---|---|
| E1 | the write hook plus the new `pane-died[1]` constant action, with a stand-in `frame-ended` that appends `#{@charter_chat}` and its pid; the recorder exits 0, 3, `TERM`, `KILL` | the stand-in runs once per death with the id expanded; the exit file holds 0, 3, 1, 1; the window is still listed |
| E2 | from that stand-in: `respawn-pane` **without `-k`** on the dead pane | the pane id, window id, `@charter_chat`, both pane hooks and `remain-on-exit` are unchanged; the new pid is `#{pane_pid}`; `launcher.framed_chat()` in it answers the chat on both servers; `$CHARTER_SESSION_ID` in it is the chat |
| E2b | `respawn-pane` without `-k` on a **live** pane | tmux refuses it (`pane … still active`), rc ≠ 0, and the live process is untouched |
| E3 | `split-window -t <dead pane> -l 6 -P -F '#{pane_id}' <stand-in>`, then `set-option -p -t <that id> @charter_drawer <chat>` | the split succeeds; the dead pane's `#{pane_dead}` stays 1; one `list-panes -a -F '#{pane_id}\t#{pane_dead}\t#{@charter_chat}\t#{@charter_plane}\t#{@charter_drawer}'` shows the drawer's marker on the drawer alone, `@charter_chat` and `@charter_plane` on both panes |
| E4 | a client attached over a pty to a session whose only window's pane has ended and been respawned | `attach` has not returned after 3 s, and returns within 1 s of that window's `kill-window` |
| E5 | operator arm: `_wait_for_harness`, respawn, `_wait_for_harness` | the second wait waits on the new process |
| E6 | a respawn of a harness pane whose window has panels | every panel is unchanged, and `select-pane -t <harness pane>` still gives it focus |
| E7 | a pane whose stdin ends while a raw-mode Python stand-in reads it (the server killed, then the pane) | the stand-in's read answers end of input — not an Escape byte |

E1's `TERM` and `KILL` rows also run on CI's Linux runner, as
`AnExitOnARealServer.test_a_signal_death_ends_the_tab` (open question 4). **That case is a pre-merge
gate on this task** (controller's ruling):
- it runs unskipped;
- its stand-in harness enters raw mode before it exits or is killed;
- it covers both exit 0 and `SIGKILL`.

The PR quotes that run.

**Stop rule, written into the dispatch.** If any criterion fails on any version or arm, the implementer
stops before any code, writes the table of readings into the task issue, and reports to the controller.

### Files

- **Create** `charter/frame/ended.py` and the two Task 2 test modules.
- **`charter/frame/layout.py`** — `respawn_argv` (`:1227`) gains `kill: bool = True`. The ended step
  passes `False`. The operator's placeholder respawn keeps `-k`.
- **`charter/frame/overlay.py`**:
  - `LEFT_KEY`, `LEFT_EOF` and `Surface.left: str = ""`;
  - `run` (`:865-905`) sets `left = LEFT_KEY` before returning `None` for a `CANCEL`, and
    `left = LEFT_EOF` before returning `None` at end of input;
  - every existing caller's `None` is unchanged;
  - **Ctrl+C is its own key.** `decode` (`:687-690`) turns `\x03` into `Event(KEY, "ctrl-c")`, not
    `"escape"`.
  - `Surface.cancel_keys: tuple[str, ...] = ("escape", "ctrl-c")`, and `handle` (`:849`) cancels on
    `ev.name in self.cancel_keys`.
  - **Every surface that cancels on Ctrl+C today keeps doing so** through that default:
    - `palette.Palette` — the `F2` palette, `commands_frame._draw_palette`;
    - its pickers (`commands_frame._picker`);
    - its confirmation drawers (`leave.confirm_rows`, through `_as_a_drawer`);
    - the tab menu (`tabmenu.draw`);
    - the never-started `selector.Selector`, as #1103 left it.

    Tasks 3 and 4's `Rename` and `Gate` inherit it. This task's ended selector and crash drawer set
    `cancel_keys = ("escape",)`, so Ctrl+C does nothing there.
- **`charter/commands_frame.py`**:
  - **`_pane_died_ended_hook_argv`** is new, beside `_pane_died_teardown_hook_argv` (`:1421-1459`),
    which stays.
  - **`_launch` (`:6245-6263`):**
    - installs the ended hook when `p is not None`, else today's teardown hook;
    - `ENDED_HOOK_NOT_INSTALLED` replaces the refusal sentence (`:6362-6374`) for a profile chat;
    - after `_arm_panel_respawn` (`:6386`), a profile chat gets `state.record_drawn(fid)`, then
      `_query_pane_dead_status` again, and a dead pane → `record_exit` and `ended.present`.
  - **`_launch_in_operator_tmux`** — the wait after `_drop_panels` becomes a loop for a profile chat.
    The escape hatch and the path before `_draw_panels` are unchanged.
  - **`cmd_close` (`:10491-10564`)** also runs `state.clear_ended` and `ended.drop_drawer`.
  - **`_record_the_plane` (`:10337-10376`):**
    - `ended=c.ended`;
    - no `_capture_transcript` for an ended chat, which names its existing `<id>.transcript` the way
      `capture=False` does (`:10353-10360`).
  - **`_reopen_one` (`:11278-11290`)** sends an ended record to `_reopen_args(..., select=True,
    ended=True, start=p.name, profile=p.name)`.
  - **`_launch`'s selector branch** hands tmux `launcher.argv_select(start, ended=…)`. For an ended
    restore, before tmux, it runs `state.claim_ended(fid)`, `state.record_profile(fid, p.name)` and
    `state.record_picked_kind(fid, p.harness)`, and never `record_waiting`.
- **`charter/frame/launcher.py`**:
  - **`argv_select` (`:207-221`)** gains `ended: bool = False` (adds `--ended`) and `fresh: bool =
    False` (adds `--fresh`).
  - **`resume_row(fid)`** is new.
  - **`attempt`'s `on_exec` wrapper** (task 1) also calls `ended.reset(fid)`. Its undo calls
    `state.claim_ended(fid)` only when `reset` answered that it cleared a claim.
  - **`_select_in_pane` (`:707-769`)**, when `args.ended`:
    - passes `resume=None if args.fresh else resume_row(fid)`;
    - on `Choice(resume=True)` → `attempt(p_of_chat, [], …, resume=True)`;
    - on a real Esc (`selector.KEY_CANCEL`) → `commands_frame.cmd_close` in-process;
    - on `selector.END_OF_INPUT` → return `selector.CANCELLED_EXIT`, having written nothing.

    A pane that never started keeps `_close_the_cancelled_chat` for both.
  - **`cmd_frame_launch`** reads `args.ended` and `args.fresh`.
- **`charter/frame/selector.py`**:
  - `RESUME_ID`, `Resume`, `Choice.resume`;
  - `KEY_CANCEL` and `END_OF_INPUT`, two sentinels `pick` answers where it answered `None`, decided
    from `Selector.left`;
  - `resume=` on `rows` (`:200-285`), `opens_on` (`:324-336`) and `pick` (`:361-391`);
  - `FOOTER_ENDED`.

  `_select_in_pane` is `pick`'s only caller.
- **`charter/frame/state.py`**, beside `record_closed` (`:2234`):
  - `record_drawn`/`was_drawn`;
  - `claim_ended` (with `config.create_for`) / `is_ended` / `clear_ended`;
  - `record_drawer`/`drawer`.
- **`charter/frame/chats.py`** — `Chat.ended`; `roster` fills it.
- **`charter/frame/slots.py`** — `ENDED_MARK = "x"`. `_chats_strip` (`:4577-4603`) hands the ended set
  beside the working set (`working_chats`, `:4555`).
- **`charter/frame/tabmenu.py`** — `CLOSE_NOW_ID`, `close_now_title`; `catalogue`, `chose`, `opens`.
  The close row stays last.
- **`charter/frame/leave.py`**:
  - `Doomed.ended`, the ended note (`_ended`, `:380-388`);
  - `needs_confirming`, `CLOSE_NOW_ID`;
  - `open_rows` (`:463-503`) keeps its order, with *chat: close* last, and swaps the close doorway
    for `CLOSE_NOW_ID` when the palette's chat needs no confirming.
- **`charter/frame/reopen.py`** — `Chat.ended: bool = False`.
- **`charter/cli.py`** — `frame-ended` (`--chat`), added to `_core_commands` (`:865-871`);
  `frame-launch --ended --fresh`; `frame-palette --ended`.
- **Docs** — `docs/adr/0018-…` (the amendment); the IDE spec's §4j and §5 notes; docs and news below.

### Interfaces

**Consumes:**
- From task 1:
  - `leave.conversation_exists`;
  - `launcher.attempt(..., resume=)` and its `on_exec` wrapper;
  - `state.kept_harness_session`, `conversation`;
  - `reopen.Chat.conversation`;
  - `Harness.resume_needs_cwd`.
- On `main`:
  - `layout.respawn_argv`;
  - `commands_frame._frame_identity_env`, `_guest_harness_env`, `_query_pane_dead_status` (`:1742`),
    `_repaint_the_other_strips` (`:10661`), `_this_plane` (`:1315`), `_PLANE_OPTION`, `_CHAT_OPTION`;
  - `palette.Palette`, `own_the_tty`; `tabmenu.handback` (`:360`);
  - `tmuxctl.live_pane_by_pid` (`:549`), `PANE_ID_RE`; `config.create_for`.

**Produces:**

```python
# charter/frame/state.py
def record_drawn(fid: str) -> None: ...
def was_drawn(fid: str) -> bool: ...
def claim_ended(fid: str) -> bool: ...    # config.create_for(<dir>/ended); True only for the first caller
def is_ended(fid: str) -> bool: ...
def clear_ended(fid: str) -> None: ...
def record_drawer(fid: str, pane: str) -> None: ...   # held to tmuxctl.PANE_ID_RE
def drawer(fid: str) -> str | None: ...

# charter/frame/overlay.py
LEFT_KEY, LEFT_EOF = "key", "eof"
CTRL_C = "ctrl-c"   # decode's name for \x03; Surface.cancel_keys defaults to ("escape", CTRL_C)
# Surface.left: str = ""   — set by run() before it answers None

# charter/frame/layout.py
def respawn_argv(*, socket, harness_pane, env, cwd, harness_argv, kill: bool = True) -> list[str]: ...

# charter/frame/ended.py
CLEAN = 0
RESUME_ID, FRESH_ID, CLOSE_ID = "ended:resume", "ended:fresh", "ended:close"
DRAWER_OPTION = "@charter_drawer"
DRAWER_ROWS = 6
PROOF_FORMAT = "#{pane_id}\t#{pane_dead}\t#{@charter_chat}\t#{@charter_plane}\t#{@charter_drawer}"
class Proof(NamedTuple):
    harness: str               # the proven harness pane id, or ""
    harness_dead: bool
    drawers: tuple[str, ...]   # proven drawer pane ids
def proof(fid: str, *, socket: str) -> Proof | None: ...   # ONE list-panes; None when the server did not answer
def present(fid: str, *, socket: str) -> str: ...          # "selector" | "drawer" | ""; never raises
def reset(fid: str) -> bool: ...                            # clear ended, exit and a proven drawer; True when it cleared an ended claim
def drop_drawer(fid: str) -> None: ...                      # kill-pane only a proven drawer
def drawer_rows(fid: str) -> tuple[overlay.Row, ...]: ...
def choose(row, fid: str) -> None: ...
def draw(args) -> int: ...          # the drawer's own process; always 0
def cmd_frame_ended(args) -> int: ...   # always 0

# charter/frame/selector.py
RESUME_ID = "resume:"
FOOTER_ENDED = "  up/down move   enter start   esc close this tab"
KEY_CANCEL = object()      # a real Esc
END_OF_INPUT = object()    # the pane's input ended
class Resume(NamedTuple):
    title: str
    note: str
class Choice(NamedTuple):
    profile: str
    resume: bool = False
def rows(have, *, cwd, start=None, after=None, resume: Resume | None = None) -> tuple[overlay.Row, ...]: ...
def opens_on(listed, start, *, resume: Resume | None = None) -> str: ...
def pick(*, cwd, root, start=None, after=None, resume: Resume | None = None, fd=None, out=None): ...
    # -> Choice | KEY_CANCEL | END_OF_INPUT

# charter/frame/launcher.py
def argv_select(start: str | None, *, ended: bool = False, fresh: bool = False) -> list[str]: ...
def resume_row(fid: str | None) -> "selector.Resume | None": ...

# charter/frame/leave.py
CLOSE_NOW_ID = "leave:close:now"
def needs_confirming(chat: str) -> bool: ...
# Doomed / reopen.Chat / chats.Chat: ended: bool = False

# charter/frame/slots.py
ENDED_MARK = "x"
# charter/frame/tabmenu.py
CLOSE_NOW_ID = "tab:close-now"
def close_now_title(target: str) -> str: ...

# charter/commands_frame.py
def _pane_died_ended_hook_argv(*, socket: str, harness_pane: str) -> list[str]: ...
ENDED_HOOK_NOT_INSTALLED = (
    "charter frame: refusing to attach — the hook that answers this chat's harness ending failed to "
    "install, so an exit later would leave a dead tab charter could offer nothing on. The harness is "
    "still running, detached; reattach manually if you must: tmux -L {socket} attach -t {session}")
```

### Behaviour

**The hook.**
- `_pane_died_ended_hook_argv` returns `tmuxctl.server_argv(socket, "set-hook", "-p", "-t", pane,
  "pane-died[1]", action)`.
- The action is the constant `run-shell -b "\"$CHARTER_PY\" -m charter frame-ended --chat
  \"#{@charter_chat}\""`, quoted as the write hook is (`:1414-1416`).
- **The branch (ruled open question 5):** `_launch` installs it only when `p is not None`. The escape
  hatch keeps `_pane_died_teardown_hook_argv`, and so its `kill-window`, its exit code and its
  `attach` return, unchanged.

**`ended.proof(fid, *, socket)` — the one listing.**
- One `list-panes -a -F PROOF_FORMAT` on `socket`. Rows of any other width are dropped.
- `me = commands_frame._this_plane()`.
- **The harness** is the row whose pane is `chats.pane_of(fid)`, and whose chat is `fid`, plane is
  `me` and drawer is `""`. A record naming a pane the listing puts under another chat, another plane,
  or no plane proves nothing: `harness = ""`.
- **The drawers** are the rows whose drawer, chat and plane are `fid`, `fid` and `me`.
- rc ≠ 0 → `None`.

**`ended.present(fid, *, socket)`.** Each step answers `""` at the first thing that is not so.
1. `chats.ID_RE.fullmatch(fid)`, `state.profile(fid)`, `state.was_drawn(fid)`,
   `not state.was_closed(fid)`.
2. `pr = proof(fid, socket=socket)`; `pr is None or not pr.harness or not pr.harness_dead` → `""`.
3. `state.claim_ended(fid)`. `False` → `""`: this exit is already presented.
4. `state.bump(fid)`, then `commands_frame._repaint_the_other_strips(fid)`.
5. Present:
   - **`state.exit_code(fid) == CLEAN`** → one `layout.respawn_argv(kill=False, harness_pane=pr.harness,
     harness_argv=launcher.argv_select(state.profile(fid), ended=True), cwd=state.chat_cwd(fid) or
     config.ROOT, env=…)`. The env is `_frame_identity_env(state.identity(fid))`, or on an operator
     socket `_guest_harness_env` of it. Return `"selector"`.
   - **Otherwise** → `split-window -v -l DRAWER_ROWS -t <pr.harness> -P -F '#{pane_id}' -e
     CHARTER_SESSION_ID=<fid> -- <self_relaunch_argv("frame-palette", "--ended", "--chat", fid)>`.
     Then `set-option -p -t <reported id> @charter_drawer <fid>`, `state.record_drawer(fid, <id>)`,
     `select-pane -t <id>`. Return `"drawer"`.
6. The whole body is in `try`/`except Exception: return ""`. A refused respawn (tmux: still active)
   leaves `ended` claimed and returns `""`.

**`ended.reset(fid)`**, which the `on_exec` wrapper calls for every start:
- `state.clear_ended(fid)`, `state.clear_exit(fid)`, `drop_drawer(fid)`.
- It answers whether an `ended` claim existed, and the wrapper's undo claims `ended` again only then.

A resumed or fresh chat is then live, drawn, unclaimed, and presented again at its next exit.

**`ended.drop_drawer(fid)`:**
- `pr = proof(…)`, then one `kill-pane -t <id>` per `pr.drawers`, then `state.record_drawer` cleared.
- The recorded drawer is never itself a target.

**The drawer's own process (`ended.draw(args)`)** has `tabmenu.draw`'s shape: `handback`, a
`palette.Palette(catalogue=drawer_rows(fid), label=f"chat {fid} ended", mouse=True)`, `own_the_tty`,
then `choose`. Its `finally` does two things:
- closes its own pane, proven by `tmuxctl.live_pane_by_pid(server, os.getpid())`, which is
  `launcher._close_the_cancelled_chat`'s proof;
- runs `select-pane -t <proven harness pane>`.

It returns 0. A `None` row — Esc or end of input — chooses nothing, and the tab stays ended.

**`drawer_rows(fid)`:**
1. `ended:resume` — only when `leave.conversation_exists(kind, link, conversation)`, with
   `launcher.resume_row(fid)`'s title and note.
2. `ended:fresh` — `start fresh`.
3. `ended:close` — `close this tab`.

**`choose(row, fid)`.** Each of the first two acts on a fresh `proof`, and only when its harness pane is
dead:
- `ended:resume` → `respawn_argv(kill=False, harness_argv=launcher.argv(profile, [], attended=True,
  resume=True))`.
- `ended:fresh` → `respawn_argv(kill=False, harness_argv=launcher.argv_select(profile, ended=True,
  fresh=True))`. That is the selector, as decision 4 says, with no resume row. The drawer never starts
  a profile.
- `ended:close` → `builtin_actions._spawn(util.self_relaunch_argv("frame-close", fid, "--chat", fid),
  fid=fid)`.

The respawn's cwd is `state.chat_cwd(fid)`. For a `resume_needs_cwd` harness (opencode), that
directory is where the id is looked up (O2).

**`launcher.resume_row(fid)`.**
- `kind` is `state.identity(fid)["CHARTER_HARNESS"]`, or when that is empty,
  `resolve(state.profile(fid))[0].harness`.
- It answers `Resume(title=f"resume {name}", note=f"{kind} · session {link[:8]}")` when
  `leave.conversation_exists(kind, link, conversation)`, else `None`.
- `name` is `fid` here; task 3 composes it.

**The selector after an exit (`_select_in_pane` with `--ended`):**
- `rows(..., resume=…)` puts `Row(id=RESUME_ID, …)` first, and `opens_on` answers it.
- `Choice(resume=True)` → `attempt(p_of_chat, [], fid=fid, attended=True, resume=True, on_exec=lambda:
  _picked(fid, p))`. A profile row → `resume=False`.
- `KEY_CANCEL` → `commands_frame.cmd_close(SimpleNamespace(chat_id=fid, chat=fid))`, then
  `CANCELLED_EXIT`.
- **Ctrl+C never cancels here:** the ended `Selector` sets `cancel_keys = ("escape",)`, so `\x03` is
  a key that does nothing.
- `END_OF_INPUT` → `CANCELLED_EXIT`. No closed mark, no `_forget_transcript`, no manifest change.
- The footer is `FOOTER_ENDED`.

**`_launch` on charter's own server, for a profile chat.**
- The ended hook sits where the teardown hook was.
- After `_arm_panel_respawn`:
  1. `state.record_drawn(fid)`;
  2. `late = _query_pane_dead_status(socket, harness_pane)`;
  3. `late is not None` → `record_exit`, then `ended.present`.
- The claim makes the hook and this late check present one exit once.

**`_launch_in_operator_tmux`, for a profile chat**, after `_drop_panels`:
```python
while True:
    code = _wait_for_harness(socket, harness_pane)
    if code is None or state.was_closed(fid):
        break
    state.record_exit(fid, code)
    state.record_drawn(fid)
    if not ended.present(fid, socket=socket):
        break   # nothing offered: close as before
```
Everything before `_draw_panels`, and the escape hatch, keep today's `_close_window()`.

**Closing.**
- `leave.needs_confirming(chat)` is `not state.is_ended(chat) and not state.is_waiting(chat)`.
- `tabmenu.catalogue(target)` ends in `CLOSE_NOW_ID` when it is false, else in today's close row; the
  close row is last either way.
- `leave.open_rows(fid)` keeps `(quit, close)` and swaps the close row's id and title on the same rule.
  `_draw_palette` treats `CLOSE_NOW_ID` like `goes_through(row, CLOSE)`.

**Quit and reopen.**
- `leave.plan` fills `ended=state.is_ended(fid)`.
- `_record_the_plane` writes it, and captures no ended chat.
- `_reopen_one` sends an ended record through the selector branch as above. Before tmux, `_launch`
  writes:
  - `claim_ended`;
  - the profile and kind from the record;
  - task 1's link and conversation.

  So `resume_row` answers on the restored tab.

### Test cases (write first; each must fail before the code exists)

**`tests/test_an_ended_harness_keeps_its_tab.py`**

Class `TheHookAndItsBranch(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_index_one_runs_the_ended_step` | the argv ends `["pane-died[1]", action]` after `"%1"`; `"kill-window" not in action`; `"frame-ended"`, `"#{@charter_chat}"` in it; `action.startswith("run-shell -b ")` |
| `test_the_action_is_a_constant` | two sockets and panes → the same text |
| `test_a_profile_launch_installs_it_after_the_write_hook` | `_launch` with a profile → the `pane-died[1]` write follows the `pane-died` write, and holds `frame-ended` |
| `test_the_escape_hatch_keeps_kill_window` | `_launch` with `harness="frame", rest=["--", "true"]` → the `pane-died[1]` write is `kill-window`, and no `frame-ended` appears. Red without the branch |
| `test_the_escape_hatch_is_never_presented` | a chat with no profile, drawn, pane dead → `present` returns `""` |

Class `NothingActsOnARecordAlone(PersonaIso, unittest.TestCase)`:
- **Setup:** `tmuxctl.run` recorded and `list-panes` answered per case. `beta.1` records pane `%1`,
  server `s`, profile `claude`, drawn, exit 0. `P` is `commands_frame._this_plane()`.

| Case | Asserts |
|---|---|
| `test_a_proven_dead_pane_is_respawned_without_k` | row `%1\t1\tbeta.1\tP\t` → one `respawn-pane` on `%1`, and `"-k"` is not in it |
| `test_another_planes_pane_is_not_touched` | row `%1\t1\tbeta.1\t/other\t` → `""`, no write, not ended |
| `test_an_unmarked_pane_is_not_touched` | row `%1\t1\tbeta.1\t\t` → `""`, no write |
| `test_a_recorded_pane_listed_under_another_chat_is_not_touched` | row `%1\t1\tbeta.2\tP\t` → `""` |
| `test_a_live_pane_is_not_touched` | row `%1\t0\tbeta.1\tP\t` → `""` |
| `test_a_server_that_does_not_answer_touches_nothing` | rc 1 → `""` |
| `test_the_hook_and_the_late_check_present_once` | two `present` calls → one respawn; `claim_ended` answers `True` then `False` |
| `test_a_crash_opens_a_drawer_and_marks_it` | exit 3 → one `split-window -t %1`, then `set-option -p -t <id> @charter_drawer beta.1` |
| `test_drop_drawer_kills_only_a_proven_drawer` | drawer recorded `%7`; rows `%7\t0\tbeta.1\tP\tbeta.1` and `%8\t0\tbeta.1\tP\t` → exactly `kill-pane -t %7` |
| `test_drop_drawer_never_kills_the_recorded_pane_when_unproven` | drawer recorded `%7`; row `%7\t0\tbeta.2\tP\tbeta.2` → no `kill-pane` |
| `test_choose_respawns_only_a_proven_dead_pane` | row `%1\t0\tbeta.1\tP\t` (live again) → `choose(resume)` writes nothing |
| `test_it_reads_nothing_from_the_pane_and_sends_no_keys` | no argv is `capture-pane` or `send-keys` in any case above |
| `test_the_command_always_returns_zero` | `cmd_frame_ended(SimpleNamespace(chat="../x"))` → 0, no tmux call; `present` raising → 0 |

Class `EveryStartClearsTheEndedState(PersonaIso, unittest.TestCase)` — execvpe patched, `framed_chat` →
`beta.1`, `beta.1` ended with exit 3, drawer `%7` proven:

| Case | Asserts |
|---|---|
| `test_a_selector_pick_resets` | `_select_in_pane` picks `profile:claude` → inside the fake: not ended, `exit_code is None`, `kill-pane -t %7` recorded |
| `test_the_drawers_resume_resets` | `cmd_frame_launch(profile="claude", resume=True)` (what `choose(resume)` respawns into) → the same three |
| `test_a_reopen_start_resets` | `cmd_frame_launch(profile="claude")` for a restored chat → the same |
| `test_an_exec_that_raises_claims_ended_again` | execvpe raises → `is_ended` |
| `test_an_exec_that_raises_claims_nothing_a_start_did_not_clear` | a chat that was not ended; execvpe raises → not `is_ended` |
| `test_a_resumed_chat_is_live_confirms_on_close_and_is_presented_again` | after the resume: `needs_confirming("beta.1")`; `tabmenu.catalogue` ends in `tab:close`; a later dead row → `present` returns non-empty |
| `test_start_fresh_in_the_drawer_goes_to_the_selector` | `choose(fresh)` → the respawn command holds `frame-launch --select`, `--ended`, `--fresh`, and no `--profile`; `_select_in_pane(fresh=True)` has no `resume:` row |

Class `TheSelectorAfterAnExit(PersonaIso, unittest.TestCase)` — link `u`, conversation a file:

| Case | Asserts |
|---|---|
| `test_resume_is_first_and_preselected` | the first row id is `resume:`, and `selected().id == "resume:"` |
| `test_no_conversation_means_no_resume_row` | no file → the cursor is on `profile:claude` |
| `test_resume_execs_the_harness_on_its_link` | `["claude", "--resume", "u", "--name", "beta.1"]` |
| `test_a_profile_row_starts_fresh_on_a_new_link` | `--session-id v`, `v != "u"` |
| `test_a_real_escape_closes_the_tab_for_good` | `pick` → `KEY_CANCEL` → `cmd_close(chat_id="beta.1")`; `was_closed` |
| `test_end_of_input_leaves_the_tab_ended_and_open` | `pick` → `END_OF_INPUT` → `cmd_close` not called; `was_closed` false; `_forget_transcript` not called; the manifest still names `beta.1`; `is_ended` |
| `test_a_never_started_selector_keeps_todays_close_on_either` (pin) | not ended → `_close_the_cancelled_chat` on both, `was_closed` false |
| `test_the_surface_tells_escape_from_end_of_input` | `overlay.Surface.run` fed `b"\x1b"` → `left == LEFT_KEY`; fed `None` → `left == LEFT_EOF` |
| `test_ctrl_c_is_decoded_as_its_own_key` | `overlay.decode(b"\x03", final=True)` → `[Event(KEY, "ctrl-c")]` |
| `test_ctrl_c_on_the_ended_selector_does_nothing` | the ended selector driven with `b"\x03"`, then end of input → `cmd_close` not called, `was_closed` false, `_forget_transcript` not called, the manifest still names `beta.1`, and `is_ended` |
| `test_every_surface_that_cancelled_on_ctrl_c_still_does` | a subTest per surface — the `F2` `Palette`, a `leave.confirm_rows` palette, a `_picker` palette, `tabmenu.draw`'s palette, the never-started `Selector` — each fed `b"\x03"` → `run` answers `None` with `left == LEFT_KEY` |
| `test_the_never_started_selector_still_closes_on_ctrl_c` (pin, #1103) | not ended, `b"\x03"` → `_close_the_cancelled_chat` called |
| `test_ctrl_c_in_the_crash_drawer_chooses_nothing_and_keeps_it_open` | `ended.draw` fed `b"\x03"`, then Enter on `ended:fresh` → that row acts |
| `test_the_footer_says_escape_closes_this_tab` | the rendered last line holds `esc close this tab` |

Class `TheCrashDrawer(PersonaIso, unittest.TestCase)`:
- **Rows:** `drawer_rows` with a conversation → resume, fresh, close; without one → fresh, close.
- **Resume:** `choose(resume)` → the respawn command holds `frame-launch --profile claude --attended
  --resume`, with `-c <chat cwd>`.
- **Close:** `choose(close)` spawns `frame-close beta.1 --chat beta.1`.
- **`test_the_drawer_closes_its_own_pane_by_pid`:** `live_pane_by_pid` → `%7` → `kill-pane -t %7`,
  whether `own_the_tty` answered `None` or raised.
- **End of input** in the drawer chooses nothing: `is_ended` still.

Class `AnEndedTabIsMarked(PersonaIso, unittest.TestCase)`:
- The roster says ended.
- The strip draws `ENDED_MARK`, and `TABS.tab_at` still answers the chat.
- `ENDED_MARK` is one ASCII cell.
- A background chat ends the same way.

Class `ClosingAnEndedTabDoesNotAsk(PersonaIso, unittest.TestCase)`:
- `needs_confirming`: running → `True`; ended → `False`; waiting → `False`.
- The tab menu's last row is `tab:close-now` on an ended tab; `opens` → `None`; `chose` spawns
  `frame-close`.
- A running tab is still confirmed (pin).
- `leave.open_rows("beta.1")` is `(leave:quit, leave:close:now)` on an ended chat: the close row stays
  last.

Class `QuitAndReopenKeepEndedTabs(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_quit_records_an_ended_tab_as_ended` | `"ended": true` in `reopen.json` |
| `test_a_quit_captures_no_ended_tab` | `_capture_transcript` patched to raise → not called for an ended chat; an existing `beta.1.transcript` is named in the record |
| `test_the_confirmation_says_it_comes_back_ended` | `comes back ended; resume offered` / `nothing to resume` |
| `test_a_reopen_brings_an_ended_chat_back_at_the_ended_selector` | `select is True`, `ended is True`, `start == "claude"`, no `resume` |
| `test_a_restored_ended_tab_can_resume` | `_launch` for that record under the tmux fakes → inside `new-window`: `is_ended`, not `is_waiting`, `profile == "claude"`, `identity()["CHARTER_HARNESS"] == "claude-code"`, link `u`, `launcher.resume_row("beta.7")` is not `None`; then `_select_in_pane` picking resume execs `--resume u` |
| `test_resume_row_falls_back_to_the_profile_for_the_kind` | identity kind empty, profile `claude` → `resume_row` answers |
| `test_the_unattended_reopened_selector_survives_end_of_input` | the restored ended chat's `_select_in_pane` with `pick` → `END_OF_INPUT` → the manifest still names it, `was_closed` false, `is_ended` |
| `test_a_record_from_before_reads_as_not_ended` | no key → `False` |

Class `TheExitCodeArrivesWhenTheTabCloses(PersonaIso, unittest.TestCase)`:
- Exit 3 recorded, the chat not live after attach → rc 3.
- A detach is 0 and says so (pin).

Class `InsideTheOperatorsTmux(PersonaIso, unittest.TestCase)`:
- `test_a_profile_exit_after_the_draw_presents_and_keeps_waiting`: two waits, one `present`, no
  `kill-window` between them.
- `test_the_escape_hatch_still_closes_at_exit` (pin): the profile is `""` → `_close_window`.
- `test_an_exit_before_the_draw_is_still_an_early_death` (pin).

Class `TheRecordIsWrittenDown(unittest.TestCase)`:
- 0018 holds `a harness exit is no longer final`, `Charter restarts nothing by itself` and
  `only as a listing proves it`.
- CONTEXT.md holds `**Ended tab**:`.
- The IDE spec's §4j holds `Built 2026-09-15`.

**`tests/test_an_ended_harness_keeps_its_tab_on_a_real_server.py`** — real tmux, fixtures as
`tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py`. `selector.pick` and `ended.draw`
are stood in through a `PYTHONPATH` shim, as
`tests/test_a_new_chat_starts_at_the_profile_selector.py::TheSelectorOnARealServer` records.

Class `AnExitOnARealServer(PersonaIso, unittest.TestCase)`:
- **A clean exit keeps the window and respawns the same pane.** Within 5 s, the pane id is unchanged
  and the shim record holds `--ended`.
- **A crash keeps the dead pane and opens a marked drawer.** The listing shows `@charter_drawer` on
  the drawer alone.
- **`test_a_signal_death_ends_the_tab`** (open question 4, a pre-merge gate on this task).
  - It runs unskipped on CI's Linux runner.
  - Its stand-in harness puts its own terminal into raw mode (`tty.setraw(sys.stdin.fileno())`) before
    it exits or is killed, as a real harness does.
  - subTests cover `exit 0`, where the selector respawns, and `SIGKILL`, where a drawer opens within
    5 s.
  - **It fails with the listing wherever that does not happen, and is never skipped.**
- **`test_the_last_chat_ending_keeps_the_session`** replaces
  `ChatsAreWindowsOnOneWorkspaceSession::test_the_last_chats_teardown_still_ends_the_session`.
- **`test_the_escape_hatch_closes_its_window_and_returns_its_code`:**
  - `_launch(harness="frame", rest=["--", "sh", "-c", "sleep 1; exit 7"])`, with `tmuxctl.interact`
    stood in by a helper that attaches a real client over a pty
    (`tests/test_a_real_click_on_a_real_tab_bar_switches.py`'s helper) and returns when that client
    exits, killing it at 10 s;
  - it asserts `_launch` returns 7 and the window is gone;
  - without the branch, the ended hook keeps the window, the client never exits, and the case fails
    at its bound.
- **`test_another_planes_pane_with_the_same_id_is_never_respawned`:**
  - two throwaway planes on one `-S` server;
  - plane B's chat `default.1` exits 0 while plane A's `default.1` is recorded drawn;
  - `ended.present("default.1")` run as plane A → no respawn, and B's pane still dead.
- **`test_the_same_holds_in_a_tmux_you_already_had`:** an `-S` server, `$TMUX` patched,
  `_launch_in_operator_tmux` in a thread, and the recorder exits 0 → respawned; `kill-window` → the
  thread returns.

**Existing tests this task changes** (a floor):
- **`tests/test_frame_launcher.py`:**
  - `PaneDiedHooks::test_the_teardown_hook_is_a_constant_kill_window_at_index_1` — kept for the escape
    hatch, with a sibling case for the ended hook.
  - `Launch::test_the_write_hook_is_installed_before_the_teardown_hook` — the order holds for both
    hooks.
  - `Launch::test_refuses_to_attach_when_the_teardown_hook_fails_to_install` — `ENDED_HOOK_NOT_INSTALLED`
    for a profile chat.
  - `Launch::test_a_failed_teardown_after_an_early_death_is_reported` — unchanged, a pin.
  - `LaunchInsideTmux` — a profile chat's close-at-exit cases flip, and
    `test_a_harness_that_dies_at_once_is_never_switched_to` stays.
- **`tests/test_frame_tmux_integration.py`:**
  - `TmuxIntegration::test_the_write_hook_must_be_installed_before_the_teardown_hook` — kept.
  - `ChatsAreWindowsOnOneWorkspaceSession::test_the_last_chats_teardown_still_ends_the_session` —
    replaced.
- **`tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py::TheHarnessExitCodeTravelsAsItDid::test_a_harness_exit_code_travels_as_it_did`**
  — the window stays, and the chat is ended.
- **`tests/test_a_new_chat_starts_at_the_profile_selector.py`**, as #1103 leaves it — its Esc cases
  stay for a never-started pane, and `pick`'s `None` becomes `KEY_CANCEL`/`END_OF_INPUT`.
- **`tests/test_frame_overlay.py`**:
  - `run`'s cancel cases also assert `left`;
  - its `\x03` decode case expects `ctrl-c`;
  - a surface that cancels on it does so through `cancel_keys`.
- **`tests/test_a_right_click_on_a_tab_acts_on_that_tab.py`** — `TheMenuIsTheTwoRowsThatHaveATabToSitOn`
  and `CloseIsConfirmedAndItNamesTheTabYouClicked`.
- **`tests/test_a_reopen_says_what_it_cannot_bring_back.py`** and
  **`tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py`** — the `already ended on its own`
  literal.
- **`tests/test_a_quit_records_the_plane_before_it_kills.py`** — the capture cases keep a live chat's
  capture.
- **`tests/test_quit_and_reopen_on_a_real_tmux.py`** — gains `AnEndedTabComesBackEnded`.
- **`tests/test_a_background_chat_really_starts_on_its_brief.py`** — re-read its window-gone
  assertions.

### Implementation steps

- [ ] Step 0: E1–E7 on both versions and both arms; apply the stop rule.
- [ ] Write the two modules and the changed cases; run them locally — expect failures; note the counts.
- [ ] `overlay.left`; `layout.respawn_argv(kill=)`.
- [ ] `state` marks and the `ended` claim; `reopen`, `chats` and `leave` fields; `needs_confirming`,
      `CLOSE_NOW_ID`.
- [ ] `ended.py`: `proof`, `present`, `reset`, `drop_drawer`, `draw`, `choose`; the `cli` parsers.
- [ ] `commands_frame`: the hook branch, drawn mark and late check, operator-tmux loop, `cmd_close`,
      capture skip, reopen ended.
- [ ] `selector` resume row and the two sentinels; `launcher` `argv_select(ended=, fresh=)`,
      `resume_row`, reset in the `on_exec` wrapper, Esc against end of input.
- [ ] `tabmenu` and palette close-now; the strip's mark.
- [ ] `grep -rn "kill-window\|teardown hook\|teardown_hook\|already ended on its own\|closes as it always did\|dead tab" charter/ tests/ docs/frame.md`,
      both directions.
- [ ] Re-run every new and changed module; green.
- [ ] The ADR 0018 amendment, the IDE-spec notes, CONTEXT.md, docs, news.
- [ ] Commit, push; read the four `test (3.x)` jobs and quote the signal case's Linux result; read the
      sweep summary.
- [ ] If the signal case is red on Linux, stop before merge and take open question 4 to the controller.

### Docs and news

- **`docs/frame.md`.**
  - `## Inside a tmux you already have` (`:1070-1075`): a profile's tab stays and offers a choice, and
    `charter frame -- <cmd>` still closes at exit.
  - `## Leaving` gains `### When a harness ends`:
    - the two presentations and end of input;
    - that charter restarts nothing and types nothing;
    - that closing an ended tab does not ask;
    - that a quit does not capture an ended tab;
    - that a session charter did not mark is not presented.
  - `## Exit codes` (`:1394-1401`): the code arrives when the tab closes, and the escape hatch keeps
    its own.
  - `## When the command dies before the frame is drawn` (`:1486`): still the one case a profile chat's
    window closes on its own.
  - `## How a harness starts` (`:1517-1540`): the amendment in one sentence.
  - `### No harness starts until you pick a profile` (`:3206`): the selector after an exit, its resume
    row, Esc, `--fresh`.
  - The `-` paragraph (`:944-962`): an ended tab closes without the warning.
- **IDE spec.** Dated notes at §4j and §5: `Built 2026-09-15 by docs/superpowers/specs/2026-09-15-one-exit-gate.md`.
- **`CONTEXT.md`** **Ended tab**.
- **ADR 0018** — the amendment.
- **News** `docs/news/unreleased-an-ended-harness-keeps-its-tab.md`:

  ```
  ---
  version: unreleased
  headline: `/exit`, a double Ctrl+C or a crash no longer closes a chat — its tab stays and offers resume, a fresh start or close
  ---
  ```

  The body covers:
  - the failure;
  - the two presentations;
  - that closing a tab is the one way to end a chat;
  - `charter claude` returns when the tab closes, and `charter frame -- <cmd>` is unchanged;
  - the limits: ended tabs stay until closed; Ctrl+C is not disabled; an unmarked older session is
    not presented.

**Suggested PR title:** A harness that ends keeps its tab, and the tab offers resume, a fresh start or close

---

## Task 3: A tab can carry a title

**Depends on:** tasks 1 and 2 on `main`.

### Step 0 — none new

Task 1's C1 and C3 measured `--name "t1 · beta.1"` and `--name "t2 · beta.1"`:
- a title with spaces and ` · ` is accepted at a start and at a resume;
- the name is written with no prompt;
- it is listed by a fresh `claude --resume` picker.

This task re-reads that table.

### Where a title is shown, and where it is not

A title is shown **wherever a chat is named to a person**. That is decision 11's *"shown in: the
strip"*, extended as the controller ruled (decisions file, *Where titles show*):
- the chat strip, instead of the id;
- after the id in the tab menu's label, the quit and close confirmation rows (`leave.title`), the ended
  selector's resume row and drawer, and the chat picker (`choose`).

**Nowhere else.** A workspace tab's selector names profiles, not chats, so it shows no chat's title.
The selector's own title row, at `+` and at a workspace tab, is an input for the *new* chat's title,
and shows nothing about any other chat.

### Files

- **Create** `charter/frame/rename.py` and `tests/test_a_tab_can_carry_a_title.py`.
- **`charter/frame/state.py`** — beside `record_brief` (`:2099`): `TITLE_MAX`, `TITLE_FILE`,
  `record_title`, `title`.
- **`charter/frame/chats.py`** — `Chat.title: str = ""`; `roster` fills it; `label_of(chat)`.
- **`charter/frame/slots.py`** — `_chats_strip` (`:4577-4603`) hands a label per id as a sixth tuple
  element. `_workspaces_strip` (`:4702`) hands `{}`, and `BARS` (`:4706`) keeps one shape. The bar
  draws the label, and `_Tabs.publish` (`:3487`) is still handed ids.
- **`charter/frame/launcher.py`**:
  - `session_name(fid)`, which `session_argv` and `resume_row` use;
  - `argv_select(start, *, ended=False, fresh=False, titling=False)`, where `titling` adds
    `--title-row`;
  - `_select_in_pane` passes it on.
- **`charter/frame/tabmenu.py`**:
  - `RENAME_ID`, `rename_title`;
  - `catalogue` gives transcript, rename, then the close row, last;
  - `opens` answers `rename.Rename` for `RENAME_ID`;
  - `label` shows the title;
  - the module docstring's *"Exactly two rows"* and *"no rename anywhere in the frame"* (`:36-42`) are
    rewritten.
- **`charter/frame/selector.py`** — `TITLE_ID`, and `rows(..., titling=)` puts a title row **last**;
  `pick(..., titling=)` loops through `rename.Rename` in the pane.
- **`charter/commands_frame.py`**:
  - `cmd_rename(args)`;
  - `_palette_catalogue` (`:9140-9166`) inserts `rename.open_rows(fid)` before `leave.open_rows(fid)`,
    which stays last with *chat: close* last;
  - `_picker` (`:9417+`) opens `rename.opens`;
  - `_launch`'s `opening` branch (`:5977-5985`) records `rename.first_line_title(opening.brief)`;
  - `_restore_recorded_chat` records `rec.title`; `_record_the_plane` writes `title=`;
  - `cmd_new_chat` (`:11591+`) and `_open_workspace` (`:8510+`) launch with `titling=True`.
- **`charter/frame/leave.py`** — `Doomed.title`; `title()` (`:429-443`) is `f"{chat} · {title} ·
  {profile}"`, dropping the empty parts.
- **`charter/frame/reopen.py`** — `Chat.title: str = ""`; `_chat`'s key list.
- **`charter/frame/choose.py`** — the chat picker's row title uses `chats.label_of`.
- **`charter/frame/ended.py`** — the drawer's label names the title.
- **`charter/cli.py`** — `frame-rename` (`chat`, then `title` as `nargs=REMAINDER` after `--`), added
  to `_core_commands`; `frame-launch --title-row`.

### Interfaces

**Consumes:**
- From tasks 1–2: `launcher.session_argv`, `resume_row`, `argv_select`; `selector.pick`;
  `tabmenu.CLOSE_NOW_ID`; `reopen.Chat.ended`.
- On `main`: `palette.Palette`, `palette.typed` (`palette.py:160`), `contain.one_line`,
  `contain.readable`, `commands_frame._repaint_the_other_strips`, `_say_on_screen`,
  `builtin_actions._spawn`.

**Produces:**

```python
# charter/frame/state.py
TITLE_MAX = 60
TITLE_FILE = "title"
def record_title(fid: str, text: str) -> bool: ...
    # "" removes; refuses non-printable text or more than TITLE_MAX characters after
    # contain.one_line; True when the record changed
def title(fid: str) -> str | None: ...

# charter/frame/chats.py
class Chat(NamedTuple): ...; title: str = ""
def label_of(chat: str) -> str: ...   # contain.readable(title) when set, else the id

# charter/frame/rename.py
OPEN_ID = "rename:open"
GO_ID = "rename:go"
TOO_LONG = "a title is at most {max} characters — this one is {n}; nothing was renamed"
NOT_PRINTABLE = "a title is one line of printable text — nothing was renamed"
def normalized(text: str) -> tuple[str | None, str]: ...
def first_line_title(brief: str) -> str: ...
def open_rows(fid: str) -> tuple[overlay.Row, ...]: ...   # "chat: rename — give this tab a title"
def is_row(row) -> bool: ...
def opens(row, target: str) -> "Rename | None": ...
@dataclass
class Rename(palette.Palette):
    target: str = ""
def chose(row, target: str, *, fid: str, text: str) -> bool: ...   # spawns frame-rename <target> -- <text>

# charter/frame/launcher.py
def session_name(fid: str) -> str: ...   # f"{title} · {fid}" or fid
def argv_select(start, *, ended: bool = False, fresh: bool = False, titling: bool = False) -> list[str]: ...

# charter/frame/selector.py
TITLE_ID = "title:"
def rows(have, *, cwd, start=None, after=None, resume=None, titling: bool = False) -> tuple[overlay.Row, ...]: ...
def pick(*, cwd, root, start=None, after=None, resume=None, titling: bool = False, fd=None, out=None): ...

# charter/frame/tabmenu.py
RENAME_ID = "tab:rename"
def rename_title(target: str) -> str: ...

# charter/commands_frame.py
def cmd_rename(args) -> int: ...   # always 0; the outcome goes to _say_on_screen
```

### Behaviour

- **`record_title` is the one gate.** It applies `contain.one_line` and strips. It refuses any
  character that is not `str.isprintable()`, and anything over `TITLE_MAX`. Empty removes the file.
  It writes through `config.write_for`.
- **The strip.** `label_of` is drawn and measured with `tui.width`, and clipped by the bar's rules. The
  click map is published with the id.
- **Rename from the tab menu or `F2`.**
  1. The row is a doorway to `Rename` in the same pane, whose one row is `title: <typed>`.
  2. Enter runs `normalized`. A refusal stays in the footer. A title is spawned as `frame-rename
     <target> -- <title>` through `builtin_actions._spawn`: a `Popen` argv, no tmux.
  3. Esc renames nothing.
- **`cmd_rename(args)`.**
  1. The id is held to `chats.ID_RE` and must name a chat on this plane.
  2. `normalized`, then `record_title`.
  3. `bump`, then `_repaint_the_other_strips`.
  4. `_say_on_screen` says `renamed — Claude Code sees the new name the next time it starts or
     resumes` for a `claude-code` chat, and `renamed` otherwise.

  It sends no keys and respawns nothing.
- **At `+` and a workspace tab.**
  - The selector's `Row(id=TITLE_ID, title="title: (none) — Enter to name this chat")` is last, so
    `palette.aim` and `opens_on` never put the cursor on it.
  - Enter opens `Rename`, records through `state.record_title` (the pane is the chat's own, proven by
    `framed_chat`), and comes back.
  - It is not offered on an ended selector or a reopen.
- **A handoff.** `_launch` records `first_line_title(opening.brief)`: the first non-blank line, run
  through `contain.one_line`, cut at `TITLE_MAX` with `contain.readable`'s marker, never refused.
- **The name Claude Code sees.** `session_name(fid)` is read at every `exec`. Codex and opencode get
  none.
- **The record.** `leave.Doomed.title` → `_record_the_plane` → `_restore_recorded_chat`, before tmux.

### Test cases (write first; each must fail before the code exists)

**`tests/test_a_tab_can_carry_a_title.py`**

Class `TheTitleIsStoredAndContained(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_a_title_is_recorded_and_read` | `record_title("beta.1", "fix the widget")` → `title == "fix the widget"`; the file is `.charter/frame/beta.1/title`, spelled literally |
| `test_empty_removes_it` | → `None`, no file |
| `test_a_newline_becomes_one_line` | `"a\nb"` → no `\n` |
| `test_a_control_byte_is_refused` | `"a\x1b[2Kb"` → `False`, nothing written |
| `test_a_title_over_the_limit_is_refused_with_its_length` | 61 characters → `False`; `normalized` holds `this one is 61` |
| `test_a_brief_first_line_is_cut_not_refused` | at most 60 characters, ending in `contain.readable`'s marker |
| `test_a_drawn_title_is_escaped` | a hand-written file holding `\x1b` → `label_of` holds no ESC |

Class `WhereATitleIsShown(PersonaIso, unittest.TestCase)`:
- **The strip:** a titled tab draws its title and an untitled one its id; a click on the title
  resolves to the chat.
- **The tab menu's label:** `tabmenu.label("beta.1")` holds `fix it`.
- **The confirmation rows:** `leave.title(Doomed(..., title="fix it", profile="claude")) ==
  "beta.1 · fix it · claude"`.
- **The resume row:** `launcher.resume_row("beta.1").title == "resume fix it · beta.1"`.
- **The chat picker:** `choose`'s chat row holds `fix it`.
- **`test_a_workspace_tabs_selector_names_no_chat_title`:** a sibling chat titled `secret plan` →
  `selector.rows(..., titling=True)` holds that text in no row.
- **The workspaces strip is unchanged** (pin).

Class `RenameFromTheTabMenuAndF2(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_the_tab_menu_carries_rename_and_close_stays_last` | ids `["tab:transcript", "tab:rename", "tab:close"]`; on an ended tab the last is `tab:close-now` |
| `test_the_palette_carries_rename_before_the_leaving_rows` | `rename:open` before `leave:quit`; `leave:close` last |
| `test_the_rename_row_is_a_doorway` | `opens` → `Rename`; `chose` → `False` |
| `test_enter_spawns_frame_rename_with_the_text_on_argv_not_tmux` | `_spawn` argv ends `["frame-rename", "beta.1", "--", "fix it"]`; no `tmuxctl.run` |
| `test_the_command_records_bumps_and_says_when_claude_sees_it` | recorded; `beta.2`'s version moved; the notice holds `next time it starts or resumes` |
| `test_a_rename_never_touches_the_harness` | no tmux argv holds `send-keys` or `respawn-pane` |
| `test_a_name_that_is_not_a_chat_is_refused` | `chat_id="../x"` → nothing written |

Class `ATitleAtThePlus(PersonaIso, unittest.TestCase)`:
- The title row is last when titling.
- The cursor never opens on it.
- A title given there is recorded before the pick (inside the `execvpe` fake).
- `+` and a tab set `titling=True`; a reopen and an ended selector do not.

Class `AHandoffTitlesItsChat(PersonaIso, unittest.TestCase)`:
- The first line of the brief is the title.
- A brief carrying `\x1b]0;x\x07` leaves no ESC or BEL in the label.

Class `ClaudeIsNamedAtStartAndResume(PersonaIso, unittest.TestCase)`:
- A titled chat gets `--name "fix it · beta.1"`, and an untitled one `--name beta.1`.
- A resume after a rename carries the new name.
- Codex and opencode get no `--name`.

Class `TheTitleTravelsThroughTheRecord(PersonaIso, unittest.TestCase)`:
- The literal key `"title"`.
- Restored before tmux.
- A record from before reads as untitled.

**Existing tests this task changes** (a floor):
- `tests/test_a_right_click_on_a_tab_acts_on_that_tab.py::TheMenuIsTheTwoRowsThatHaveATabToSitOn` —
  three rows, and the class name and docstring rewritten.
- `tests/test_a_real_click_on_a_real_tab_bar_switches.py::ARealRightClickOnTheChatBarOpensARealMenu` —
  the row count.
- `tests/test_frame_bars.py` and `tests/test_a_tab_strip_grows_a_row_when_its_tabs_overflow.py` —
  widths from labels, and the sixth tuple element.
- `tests/test_frame_palette.py::TheActionsCharterOffersItself` — the catalogue's rows.
- `tests/test_a_new_chat_starts_at_the_profile_selector.py::TheRows` — no title row without `titling`.
- `tests/test_the_launcher_becomes_the_profile.py` and task 1's module — the `--name` value.
- `tests/test_a_reopen_says_what_it_cannot_bring_back.py` — `leave.title` literals.

### Implementation steps

- [ ] Re-read task 1's C1 and C3 readings.
- [ ] Tests first; run the new module and the changed ones — expect failures; note the counts.
- [ ] The `state` title; `chats.label_of`; the `reopen` and `leave` fields.
- [ ] `rename.py`; `cmd_rename`; the `cli` parsers.
- [ ] The strip's labels; `tabmenu`'s rename row; the palette doorway and `_picker`.
- [ ] The selector's title row; `argv_select(titling=)`; `cmd_new_chat` and `_open_workspace`.
- [ ] `session_name`; the handoff title; the restore; the drawer label.
- [ ] `grep -rn "Exactly two rows\|no rename\|two rows that have a tab" charter/ tests/`, both
      directions.
- [ ] Focused modules green; docs, news; commit, push; CI's four jobs; the sweep summary.

### Docs and news

- **`docs/frame.md`:**
  - the strip and `-` paragraphs (`:930-962`): titles, rename, the `+` title row, and where a title
    is shown;
  - `### A chat opened for you in the background` (`:726`): the brief's first line titles it;
  - `## Leaving`: a reopen restores titles.
- **`docs/harnesses.md`** — the session name, `<title> · <id>`, which Claude Code sees at its next start
  or resume and lists in a fresh `claude --resume` picker (C3). Codex and opencode keep their own.
- **`CONTEXT.md`** **Title**.
- **News** `docs/news/unreleased-a-tab-can-carry-a-title.md`:

  ```
  ---
  version: unreleased
  headline: A chat tab can carry a title — rename it from the tab menu or F2, name it at `+`, and a handoff titles its chat from the brief
  ---
  ```

**Suggested PR title:** A tab can carry a title, and Claude Code is started under it

---

## Task 4: One gate closes charter

**Depends on:** tasks 2 and 3 on `main`. **Closes:** #1097.

### Step 0 — Measure first. A failed G1, G2, G3, G4 or G6 stops this task.

**Where.** A throwaway `-S /tmp/cp-t4-<slug>/s` server, with two clients attached over two ptys
(`tests/test_a_real_click_on_a_real_tab_bar_switches.py::_ARealFrameWithBars`' pty helper, run by hand).
Both tmux 3.7c and the 3.2 floor.

| # | Reading | Pass when |
|---|---|---|
| G1 | `bind -n F2 run-shell 'echo "#{client_name}" >> <file>'`, and the same for `F10`; each key's bytes written to client A's pty | the file holds A's `#{client_name}` and not B's, on both versions |
| G2 | `detach-client -t <A>` | A's `attach` exits; B stays attached |
| G3 | the `MouseDown1Pane` bind with its panel branch `set-option -F -p -t = @charter_presser "#{client_name}" ; send-keys -M`; an SGR press on a panel pane in A's view, then in B's | after A's press the panel pane's `@charter_presser`, read back with `show-options -p -v`, is A's expanded name and never the text `#{client_name}`; after B's it is B's, in either order of last activity, three trials each. **If not, the task stops and goes to the controller** (the spec's open question 3 ruling) |
| G4 | `detach-client -s default.1` against session `default` with window `default.1` | fails with `can't find` — the #1097 reading, recorded |
| G5 | `conf_text(...)` with the `F10` line and the changed hotkey and click binds, `source-file`'d | `list-keys -T root` shows `F2`, `F10` and `MouseDown1Pane` with the presser; the escape hatch's line is still last |
| G6 | `list-clients -F '#{client_name}\t#{session_id}'` on the server, and `display-message -p -t <$N> '#{@charter_plane}'` | each attached client is listed with its session; a session created by `layout.session_argv` and `_plane_option_argv` answers its plane |

**Stop rule** as task 2's.

### Files

- **Create** `charter/frame/gate.py`, `tests/test_one_gate_closes_charter.py` and
  `tests/test_the_gate_detaches_a_real_client.py`.
- **`charter/commands_frame.py` — `conf_text` (`:1018-1035`).**
  - **The hotkey bind** passes the presser in `frame-palette`'s existing `client` positional:
    `frame-palette "#{client_name}" --chat "#{@charter_chat}"`. #729 left that positional accepted,
    and its reason for removing the value (a consumer gone) no longer holds.
  - **The gate's `F10` bind** follows the hotkey's.
  - **`tmuxctl.CLICK_KEY`'s panel branch** records the presser before forwarding:
    `'set-option -F -p -t = @charter_presser "#{client_name}" ; send-keys -M'`.
  - The escape hatch stays last.
- **`charter/commands_frame.py` — the palette.**
  - **`cmd_palette` (`:8948-9024`):** `gate.wanted(args)` → `gate.draw(args)`, beside
    `tabmenu.wanted`.
  - **`_open_palette` (`:9026-9070`):** forwards the presser (`args.client`, held to
    `gate.CLIENT_RE`) and `--gate`.
  - **`_draw_palette` (`:9168-9300`):** hands `builtin_actions.build(..., client=)` the presser.
- **`charter/frame/builtin_actions.py`.**
  - **`_detach(fid, client)` (`:116-124`)** is presser-only, proven by a listing.
    `NO_PRESSER_TO_DETACH`.
  - **`build(fid, *, current_density, current_chrome, client="")` (`:798`)** closes the presser over
    `_register_detach`'s `run`.
  - **The detach row's title** is `gate.DETACH_TITLE`, and it is listed refused with
    `NO_PRESSER_TO_DETACH` when `client` is empty. `_detachable` is unchanged.
- **`charter/frame/leave.py`** — `OPEN_QUIT` (`:459`) is `gate.STOP_TITLE`. The ids stay, and the order
  stays `(quit, close)`: *chat: close* last.
- **`charter/frame/slots.py`.**
  - **`_Doors` (`:320-399`)** gains `_gate`, `publish(columns, gate=())` and `opens_gate(col)`.
  - **`_top` (`:433-615`)** draws `GATE_BUTTON` at the right end and publishes its columns. It is
    dropped after the version and before the identity, and stays at `terse`.
  - **Inside the operator's own tmux** `_top` draws no button and publishes no gate column. That is
    `_bottom`'s rule (`:2805-2813`).
- **`charter/frame/builtins.py` — `_strip_events` (`:701-752`).**
  - Reads its own pane's `@charter_presser`, with one `display-message -p -t $TMUX_PANE`, held to
    `gate.CLIENT_RE`.
  - Spawns `frame-palette <presser> --gate` for a gate door, and `frame-palette <presser>` for a
    palette door.
- **`charter/instance.py`.**
  - `component_arrangement`'s `bound` (`:2561-2562`) gains `gate.GATE_KEY`.
  - The `[frame] hotkey` resolution refuses a value equal to it, falling back to `F2` as an unusable
    hotkey does. Read `frame_of` and `:2100` first.
- **`charter/cli.py`** — `frame-palette` (`:970-991`) gains `--gate` (`store_true`). The `client`
  positional (`:971`) is read now, not ignored.

### Interfaces

**Consumes:**
- From tasks 2–3: `leave.plan(...)` with `ended`, `leave.note`, `chats.label_of`.
- On `main`:
  - `commands_frame._pane_place` (`:7949`), `_plane_live`, `_plane_servers`, `_as_a_drawer`,
    `_close_palette` (`:9605`), `_say_on_screen`, `_this_plane`;
  - `tabmenu.handback`; `builtin_actions._spawn`, `_server`, `_detachable`;
  - `slots._door_columns` (`:402`); `palette.Palette`.

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
PRESSER_OPTION = "@charter_presser"
CLIENT_RE = re.compile(r"/dev/[A-Za-z0-9._/-]{1,64}")
NOT_HERE = ("the close menu is not drawn inside a tmux you already have — F2's rows carry it, and your "
            "own prefix key detaches")
def wanted(args) -> bool: ...
def presser_of(args) -> str: ...     # args.client held to CLIENT_RE, else ""
def catalogue(fid: str, *, client: str) -> tuple[overlay.Row, ...]: ...
@dataclass
class Gate(palette.Palette): ...      # _refilter pins the cursor to DETACH_ID, refused or not
def opens(row, fid: str, *, live) -> "palette.Palette | None": ...
def chose(row, fid: str, *, client: str) -> bool: ...
def draw(args) -> int: ...           # always 0

# charter/frame/builtin_actions.py
NO_PRESSER_TO_DETACH = ("charter cannot tell which terminal asked, so it detached nothing — press F10 "
                        "in the terminal you want to close, or close it")
NOT_ATTACHED_HERE = ("that terminal is not attached to this chat's session on this plane, so charter "
                     "detached nothing")
def _detach(fid: str, client: str) -> str: ...
def build(fid: str, *, current_density: str, current_chrome: str, client: str = "") -> ActionRegistry: ...

# charter/frame/slots.py
GATE_BUTTON = "F10 close"
class _Doors:
    def publish(self, columns, gate=()) -> None: ...
    def opens_gate(self, col: int) -> bool: ...
```

### Behaviour

**The binds.** Each is one line in `conf_text`, and each is a constant:
- **hotkey:** `bind -n {hotkey} run-shell '"$CHARTER_PY" -m charter frame-palette "#{client_name}"
  --chat "#{@charter_chat}"'`
- **gate:** `bind -n F10 run-shell '"$CHARTER_PY" -m charter frame-palette "#{client_name}" --gate
  --chat "#{@charter_chat}"'`
- **click:** `bind -n MouseDown1Pane if-shell -F -t = '#{@charter_panel}' 'set-option -F -p -t =
  @charter_presser "#{client_name}" ; send-keys -M' 'select-pane -t =; send-keys -M'`. The spelling is
  as G3 and G5 settle it.

**The menu (`gate.draw`).**
- **On an operator socket** it draws nothing, says `NOT_HERE` on the attention row, and returns 0.
- **Otherwise** it has `tabmenu.draw`'s shape: `handback`, `Gate(catalogue=catalogue(fid,
  client=presser_of(args)), label=LABEL, mouse=True)`, `own_the_tty` with `then=` answering `opens`
  through `_as_a_drawer`, then `chose`. `finally` runs `_close_palette`.

**`catalogue(fid, *, client)`:**
1. `Row(id=DETACH_ID, title=DETACH_TITLE)`, `refused=not client`, with `note=NO_PRESSER_TO_DETACH`
   when it is refused.
2. `Row(id=STOP_ID, title=STOP_TITLE)` — a doorway.

`Gate._refilter` puts the cursor on `DETACH_ID` in every state. It is never left to `palette.aim`,
which would open on the stop row when the detach row is refused.

**`opens`.** `STOP_ID` → `palette.Palette(catalogue=leave.confirm_rows(leave.plan(live=live,
focus=state.own_workspace(fid) or ""), verb=leave.QUIT), label=leave.QUIT, mouse=True)`.

**`chose`:**
- `DETACH_ID` → `builtin_actions._detach(fid, client)`.
- `leave.goes_through(row, QUIT)` → `_spawn(frame-quit --chat fid)`.
- A refused row → `False`, and its note is said.

**`_detach(fid, client)` — #1097, and decision 2.**
1. `client` must match `gate.CLIENT_RE`. Otherwise return `NO_PRESSER_TO_DETACH`, spawning nothing.
2. `server = _server(fid)`; `place = commands_frame._pane_place(server, state.harness_pane(fid))`.
   `None` → return `NOT_ATTACHED_HERE`.
3. One `list-clients -F '#{client_name}\t#{session_id}'` on `server`. The presser's row must name
   `place[0]`, and `display-message -p -t <place[0]> '#{@charter_plane}'` must equal `_this_plane()`.
   Otherwise → `NOT_ATTACHED_HERE`.
4. `_spawn(tmuxctl.server_argv(server, "detach-client", "-t", client))`.

It never targets a chat id, a session name or `-s`. No branch detaches every client.

**The pointer route.** `_strip_events` reads `@charter_presser` from its own pane and hands it on. An
empty or unshaped value reaches the gate as no presser, where the detach row is refused with its
reason.

**The button.** `_top` builds `f" {GATE_BUTTON} "` as the last cell on charter's own server, and never
on an operator socket.

**The `F2` rows.**
- `frame.detach`'s title is `DETACH_TITLE`, and its `run` detaches the hotkey's presser.
- `leave.OPEN_QUIT` is `STOP_TITLE`.
- The ids and positions stay: *chat: close* is still the palette's last row, and its stop row second
  to last. `palette.aim` lands on charter's first harmless row, as today.

**Reserved.** A component `key = "F10"` is refused. `[frame] hotkey = "F10"` resolves to `F2` with its
reason.

### Test cases (write first; each must fail before the code exists)

**`tests/test_one_gate_closes_charter.py`**

Class `TheBindsCarryThePresser(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_the_hotkey_bind_passes_client_name_again` | the `bind -n F2` line holds `frame-palette "#{client_name}" --chat "#{@charter_chat}"`. Red at `5ad755d` |
| `test_f10_opens_the_gate_with_the_presser` | a `bind -n F10` line holds `frame-palette "#{client_name}" --gate` |
| `test_a_click_on_a_panel_records_its_presser` | the `MouseDown1Pane` line's panel branch holds `set-option -F -p -t = @charter_presser "#{client_name}"` before `send-keys -M`; its other branch is unchanged |
| `test_every_bind_is_a_constant_and_the_hatch_stays_last` | two sessions → the same lines; the last non-empty line is `overlay.hatch_bind()` |
| `test_the_operators_tmux_gets_no_bind` (pin) | `_launch_in_operator_tmux` → no argv holds `bind` or `F10` |

Class `TheKeyIsReserved(PersonaIso, unittest.TestCase)`:
- A component with `key = "F10"` → `"already bound"`.
- `hotkey = "F10"` → `config.FRAME["hotkey"] == "F2"`, with its reason.

Class `TheMenu(PersonaIso, unittest.TestCase)`:

| Case | Asserts |
|---|---|
| `test_two_rows_detach_first` | `gate:detach`, `gate:stop`, with the titles spelled here |
| `test_the_cursor_opens_on_close_charter_when_it_can_run` | `Gate(...).selected().id == "gate:detach"` |
| `test_the_cursor_never_opens_on_stop_even_when_detach_is_refused` | `client=""` → `selected().id == "gate:detach"`, and Enter answers its note. Red with `palette.aim` |
| `test_stop_all_chats_opens_the_quit_confirmation_listing_every_chat` | two running and one ended → three chat rows, the ended one's note holding `comes back ended` |
| `test_the_confirming_row_runs_frame_quit` | spawns `frame-quit --chat beta.1` |
| `test_the_menu_is_not_drawn_inside_the_operators_tmux` | an operator socket → `own_the_tty` not called; the notice holds `F2's rows carry it` |
| `test_the_menu_always_returns_zero_and_gives_the_harness_back` | `own_the_tty` raises → 0, `_close_palette` recorded |

Class `CloseCharterDetachesOnlyThePresser(PersonaIso, unittest.TestCase)` — #1097:

| Case | Asserts |
|---|---|
| `test_a_proven_presser_is_detached_alone` | `_pane_place` → `("$4", "@9")`; `list-clients` → `/dev/ttys003\t$4`; plane `P` → `_spawn` argv `[..., "detach-client", "-t", "/dev/ttys003"]` |
| `test_no_presser_detaches_nothing` | `client=""` → no spawn; `NO_PRESSER_TO_DETACH` |
| `test_no_route_ever_detaches_a_whole_session` | across every case, no argv holds `-s` |
| `test_a_chat_id_is_never_the_target` | no argv holds `default.1`. Red at `5ad755d` |
| `test_a_presser_attached_to_another_session_is_not_detached` | `list-clients` → `/dev/ttys003\t$7` → no spawn, `NOT_ATTACHED_HERE` |
| `test_a_session_of_another_plane_is_not_detached` | the session's plane `/other` → no spawn |
| `test_an_unmarked_session_is_not_detached` | the session's plane `""` → no spawn |
| `test_a_client_name_outside_its_shape_is_not_used` | `"x; kill-server"` → no spawn |
| `test_the_f2_detach_row_detaches_the_hotkeys_presser` | `build(fid, …, client="/dev/ttys003")`, then `frame.detach`'s `run` → the `-t` argv |
| `test_the_f2_detach_row_is_refused_without_a_presser` | `build(..., client="")` → the offer is not available, and its reason is `NO_PRESSER_TO_DETACH` |
| `test_the_operators_tmux_row_is_refused_with_its_reason` (pin) | an operator socket → refused, with the prefix-key sentence |

Class `TheButton(PersonaIso, unittest.TestCase)`:
- **Right end:** `_top(fid)` at 120 columns ends in `F10 close `.
- **Doors:** its columns open the gate, not the palette; the workspace chip is the reverse.
- **Starved:** a starved row drops the version before the button; at `terse` the button stays.
- **A click reads the recorded presser:** `display-message` answers `/dev/ttys003` → `_spawn` argv
  holds `frame-palette /dev/ttys003 --gate`.
- **A presser that cannot be read is no presser:** it answers `""`, or `x;y` → the argv holds no
  client.
- **`test_no_button_inside_the_operators_tmux`** (decision 7): an operator socket → no `F10 close` on
  the row, and `DOORS.opens_gate` is false for every column.

Class `TheF2RowsAreTheGate(PersonaIso, unittest.TestCase)`:
- **The detach row:** its title is `Close charter (keep chats running)`, and it is still the first id.
- **The stop row:** `leave.open_rows("beta.1")[0].title == "Close charter and stop all chats…"`.
- **`test_close_is_still_the_last_row`:** `leave.open_rows("beta.1")[-1].id` is the close row, and
  `_palette_catalogue(...)[-1]` is it too.
- **`test_the_palette_never_opens_on_the_stop_row_inside_the_operators_tmux`:** an operator socket →
  the aimed row is not `leave:quit`.

**`tests/test_the_gate_detaches_a_real_client.py`** — real tmux over ptys, with the `_ARealFrameWithBars`
and `_ARealFrameWithStrips` bases:
- **Class `ThePresserOnARealServer(_ARealFrameWithBars, unittest.TestCase)`:**
  - `test_f10_then_enter_detaches_only_the_terminal_that_pressed_it` — two clients; `\x1b[21~` then
    `\r` to A's pty → A exits within 5 s, and `list-clients` still lists B.
  - `test_f2_close_charter_detaches_only_the_terminal_that_pressed_it` (#1097) — `F2`, type `close
    charter`, `\r` in A's pty → A exits, and B stays. Red at `5ad755d`, where nothing detaches.
- **Class `ARealClickRecordsItsPresser(_ARealFrameWithStrips, unittest.TestCase):`**
  - `test_a_press_on_f10_close_opens_the_gate_for_that_client` — an SGR press at the button's column
    in A's pty → within 5 s a pane whose start command holds `frame-palette <A's name> --gate`, and
    `show-options -p -v -t <panel> @charter_presser` reads A's name, not `#{client_name}`.

**Existing tests this task changes** (a floor):
- `tests/test_frame_palette.py::TheActionsCharterOffersItself` — the detach row's title and
  availability, which now depends on the presser; its id stays first.
- `tests/test_a_click_on_a_panel_stays_where_it_points.py` and `tests/test_component_toggle_keys.py`,
  each `…::test_the_escape_hatch_is_still_the_last_line` — pins with the new lines present.
- `tests/test_a_click_on_a_panel_stays_where_it_points.py` — its `MouseDown1Pane` bind assertions gain
  the presser branch; the click still does not move the keyboard.
- `tests/test_component_toggle_keys.py` — a component key `F10` is refused.
- `tests/test_frame_slots.py::TopRenderer` — the right end.
- `tests/test_a_real_click_opens_the_real_palette.py::ARealClickOnTheHotkeyHintOpensTheRealPalette` —
  the spawned argv holds the presser.
- `tests/test_frame_launcher.py` — `conf_text` hotkey-bind literal cases.
- `tests/test_what_a_quit_says_is_spelled_where_it_is_asserted.py` — `OPEN_QUIT`'s words.
- `tests/test_a_confirmation_is_a_drawer_not_a_window.py` — the gate's stop doorway is a drawer.
- `tests/test_the_palette_advertises_unmaking_what_it_makes.py` — the row titles.

### Implementation steps

- [ ] Step 0: G1–G6, with the stop rule. G3 decides whether the task continues.
- [ ] Tests first; run the modules and the changed ones — expect failures; note the counts.
- [ ] `conf_text`'s three binds; `cli`'s `--gate`.
- [ ] `builtin_actions._detach` (presser-only, proven) and `build(client=)`.
- [ ] `gate.py`; `cmd_palette`, `_open_palette`, `_draw_palette`.
- [ ] `_Doors`, `_top` (with the operator rule), `_strip_events`.
- [ ] `instance`'s reserved key; `leave.OPEN_QUIT`.
- [ ] `grep -rn "detach — leave the harness running\|charter: quit — stop every harness\|detach-client" charter/ tests/ docs/`,
      both directions.
- [ ] Focused modules green; docs, news; commit, push; CI's four jobs; the sweep summary.

### Docs and news

- **`docs/frame.md`:**
  - `## Leaving` (`:1151`) opens with the gate: `F10`, the button, the two rows, and that only the
    terminal that asked is detached. Closing the terminal still detaches it. `F2 → detach` works
    again.
  - The identity row paragraph (`:366-372`): the `F10 close` button opens the gate.
  - `## Inside a tmux you already have` (`:1094-1105`): no gate key and no button; the rows are in the
    palette.
  - `## Configuring it`: `F10` joins `F12` as a reserved key.
- **`docs/control-plane.md`** `[frame] hotkey`: `F10` is refused.
- **`CONTEXT.md`** **Exit gate** and **Presser**.
- **News** `docs/news/unreleased-one-gate-closes-charter.md`:

  ```
  ---
  version: unreleased
  headline: F10 closes charter — detaching only the terminal you pressed it in, or stopping every chat after a confirmation — and F2 → detach detaches again
  ---
  ```

  The body covers:
  - what was built, and that Ctrl+C is not disabled;
  - #1097;
  - the limits: F10 is taken from the harness on charter's server; no key or button inside your own
    tmux; the button needs `[frame] mouse`; a detach charter cannot attribute does not happen.

**Suggested PR title:** One gate closes charter: F10, the identity row's button and two F2 rows, each detaching only the terminal that asked (#1097)
