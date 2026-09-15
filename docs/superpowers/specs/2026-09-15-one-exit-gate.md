# One exit gate — a harness that ends keeps its tab, and every tab names one harness session

**Status:** agreed 2026-09-15, in a six-round grill between the operator and the `steward` persona.
Every recommendation was confirmed ("agree", "agree all", "yes good"), and the operator added the
session-name and tab-id ideas.

**Revised the same day** after an adversarial review of PR #1104 and the controller's rulings on this
spec's open questions (decisions file, *Rulings on the spec's open questions*).

**Sources.** Decisions, rulings, the delivery order and the evidence gathered are in
`workspaces/harness-profiles/refs/exit-gate-decisions.md` in the plane.

**Against:** `origin/main` `5ad755d` (0.62.0). Line anchors below are at that commit; re-anchor by
symbol name. #1100 has merged since (`00d69f2`, which adds `tmuxctl.nothing_listening`). #1103 is still
open and touches `_launch`, `launcher.py`, `chats.py` and the reopen path, so each task re-reads what has
merged before it starts.

**Closes:** #1097 (task 4) and #1101 (task 1).

## The failure

The operator asked for three things:

> force-disable Ctrl+C in harnesses; a top-right close button offering "close charter only"
> (default) and "close charter with all sessions"; one gate to exit, so a harness cannot be
> closed inside charter except by closing its tab.

What the code does at 0.62.0:

- **Every harness exit destroys its chat.**
  - `/exit`, a double Ctrl+C, Ctrl+D and a crash all end the pane.
  - `pane-died[1]` is `kill-window` (`commands_frame._pane_died_teardown_hook_argv`, `:1421-1459`).
  - The next launch's `state.reap` deletes the chat directory. The resume id goes with it, because
    `session.durable` lives in that directory (`state.py:799`), unless a recorded manifest still
    names it.
  - Inside the operator's own tmux the launcher closes the window itself (`_launch_in_operator_tmux`,
    `_close_window`).
  - Nothing respawns anything.
- **The IDE spec promised the opposite, and nobody built it.**
  - §4j says *"The chat becomes a dead tab and is kept"*
    (`docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md:469-474`).
  - §5 lists it as an open question (`:521-522`).
  - §6 stage 6 was never delivered (`:599-600`).
- **Five ways out, and none of them is one gate.**
  - Closing the terminal detaches.
  - `F2 → detach` runs `detach-client -s <chat id>`, which names nothing (#1097).
  - `F2 → charter: quit` and `charter frame-quit` record the plane and stop it, after a confirmation
    (`leave.confirm_rows`).
  - `F2 → chat: close` stops one chat for good.
  - `-` on the chat strip and a right press on a tab open the tab menu, whose close row is the same
    close.
- **A chat id is handed out again (#1101).**
  - `state.new_chat_id` claims the lowest free ordinal (`state.py:249-279`), and its own note says an
    ordinal *"is free the moment `reap` removes its directory"* (`state.py:2625-2627`).
  - Two files keyed by the id live outside that directory and survive the reap: `<id>.transcript`,
    and the entry in `reopen.json`.
  - So a new `default.1` is offered an unrelated chat's scrollback.
  - Closing it runs `_forget_transcript("default.1")` (`commands_frame.py:10538`, `:10556`), which
    deletes the earlier chat's record.
- **Only Claude Code has a session charter can name.**
  - `hooks._record_harness_session` returns for every other harness (`hooks.py:6013-6015`).
  - `leave.resumable_harness` answers `claude-code` alone (`leave.py:409-426`).
  - A Codex or opencode chat always reopens empty.

## Language

- **Exit gate** — the one menu that leaves charter. It has two rows: *Close charter (keep chats
  running)* and *Close charter and stop all chats…*. _Avoid:_ quit dialog, exit menu.
- **Ended tab** — a chat whose harness exited on its own, and whose tab stays and offers a choice.
  _Avoid:_ dead tab (the IDE spec's word, and "dead" is tmux's word for the pane).
- **Session link** — the one harness session a chat is tied to, as the harness names it. _Avoid:_
  mapping, guess, latest session.
- **Chat id** — `<workspace prefix>.<n>`, handed out once for the life of the plane. CONTEXT.md
  already says it is allocated and never parsed; *never handed out again* is new.
- **Title** — optional words for people, drawn wherever a chat is named to a person. It is never an
  identity. _Avoid:_ name, which is what Claude Code calls the session name charter composes from the
  title and the id.
- **Presser** — the tmux client (`#{client_name}`) whose key or click opened a surface. It is the
  only terminal *Close charter* may detach.

## The decisions

Each is the operator's confirmed answer, with its reason and the evidence it rests on.

1. **Protect the chat, not the key (Q1).**
   - Ctrl+C stays. In Claude Code one press cancels the turn and the second exits, and disabling it
     would take the interrupt away.
   - What changes is that no harness exit destroys a chat.
   - *Evidence:* ADR 0018's consequence that *"charter never gets between the harness and the
     terminal it is actually talking to"* (0018:96-98).
   - The operator's first ask, force-disabling Ctrl+C, is therefore **not** built.
2. **"Close charter" detaches THIS terminal only (Q2)**, on every route to it: `F10`, the
   identity-row button, and the `F2` row.
   - Every chat keeps running, and `charter` in the project reattaches.
   - It is exactly what closing the terminal does today (`docs/frame.md:1153-1160`).
   - It never detaches every client of a session.
3. **"Close charter and stop all chats" records, then stops, every chat in this plane (Q3)**, after a
   confirmation that lists them. It is today's `F2 → charter: quit` (`cmd_quit`,
   `commands_frame.py:10240`), and `charter reopen` restores.
4. **A harness that ends keeps its tab (Q4, Q11, Q19).** This amends ADR 0018. Today a harness exit
   is final: *"A pane whose harness later EXITS closes as it always did and never comes back to the
   selector"* (0018:333-337).
   - **Clean exit** (`/exit`, double Ctrl+C, Ctrl+D) → back to the profile selector, with
     **resume &lt;session name&gt;** preselected, then start fresh, then close tab (Esc).
   - **Crash** (non-zero exit) → the harness's last lines stay on screen, with the same three choices.
   - **No conversation yet** → only start fresh and close.
   - **Background or handed-off chats** keep their tab too, marked ended in the strip.
5. **Closing a tab is the one way to end a chat, and it ends it for good (Q5, Q20)** — today's
   `chat: close`. It confirms for a tab whose harness is running. An ended tab closes without asking.
6. **The gate lives on `F10` everywhere, plus a button at the right end of the identity row wherever
   that row is placed (Q6).**
   - The bars and the mouse are off by default (`docs/frame.md:774`, `[frame] mouse`), so the key is
     the primary route.
   - Both open one menu. *Close charter (keep chats running)* is preselected. *Close charter and stop
     all chats…* leads to the confirmation.
   - The house rule applies: *"A pointer opens the question; the keyboard answers it"*
     (`docs/frame.md:944-948`).
   - `F10` joins the reserved keys in `instance.component_arrangement` (`instance.py:2561-2562`).
7. **Inside the operator's own tmux, no key is bound and no button is drawn (Q7).**
   - Charter binds nothing there today (`_launch_in_operator_tmux`'s docstring,
     `commands_frame.py:3445-3453`).
   - The gate is reachable only as `F2` rows there.
8. **Prerequisite bug #1097.**
   - `F2 → detach` runs `detach-client -s <chat id>` (`builtin_actions._detach`, `:116-124`), but the
     session is named after the workspace. On tmux 3.7c `has-session -t default.1` answers
     `can't find pane: 1`.
   - The gate's default row IS this detach, so it detaches the presser it can prove, by name.
9. **Chat ids are never handed out again within a plane (Q8, Q15).**
   - `workspace.N` only grows, and the highest N is remembered on disk.
   - Existing chats keep their ids, and the counter starts above the highest number any trace still
     carries. A restored chat keeps its id.
   - *Why:* #1101. A leftover keyed by an id can be inherited only if the id comes back.
10. **Every tab is linked to exactly one harness session, never guessed (Q9, Q13, Q14).**
    - **Claude Code:** charter chooses the UUID and launches with `--session-id <uuid> --name
      <name>`, both verified in `claude --help`, so the link exists before the harness starts.
    - **Codex and opencode:** charter records the id each one reports.
      - The Codex hook payload carries `session_id` (`codex.py:9-16`).
      - opencode's plugin sets `CHARTER_SESSION_ID` from its `sessionID` (`opencode.py:307-309`).
      - Neither is recorded today.
      - They resume by id: `codex resume <id>`, `opencode -s <id>`.
    - **Resume is offered only when the conversation actually exists** (*The session link*, below,
      says how that is told).
11. **Optional tab titles (Q10, Q16, Q17, Q18).** The id stays immutable for linking, and the title is
    for people.
    - **Set from:** a rename row in the tab menu and in `F2` (a one-line input), an optional title at
      `+`, or the first line of a handoff's brief.
    - **Shown in:** the chat strip.
    - **Claude Code's session name** is `<title> · <id>`, or `<id>` when the chat is untitled.
    - **A rename** updates the tab at once and reaches Claude Code at its next start or resume
      (`--name` again). Charter never types `/rename` into a harness (ADR 0018).
    - *Today:* no title and no rename (`tabmenu.py:36-42`); the tab is the bare id
      (`slots._chats_strip`, `:4577-4603`).
12. **A quit records ended-but-open tabs (Q12)**, with their session link, so reopen brings them back
    ended and resumable.

## What proves a pane before charter acts on it

This rule covers every tmux write this feature adds that changes a pane: a respawn, a split, a kill,
a detach. No such write acts on a record alone.

- **One listing proves the target.** Each write is aimed at what ONE listing on the chat's recorded
  server (`state.frame_server`) proves, and it is targeted by the pane id that listing reported. For
  a pane, that means:
  - `@charter_chat == <id>`, and
  - `@charter_plane == _this_plane()` (`commands_frame.py:1315-1323`), and
  - for a drawer, `@charter_drawer == <id>`.

  A pane with no plane marker is not proven. A session an older charter created is left alone, not
  adopted.
- **A record only names what to look for.** A chat's recorded pane, drawer or server says where to
  look. It is never itself the target. That is the #933 and #1103 rule, which
  `launcher._close_the_cancelled_chat` (`launcher.py:686-704`) and `_chat_seats`' plane veto already
  keep.
- **tmux's own refusals stay in the way.** A respawn is `respawn-pane` without `-k`, so tmux still
  refuses to respawn a live pane.
- **A state change that two processes can race is claimed atomically.** `ended` is created with
  `config.create_for` (`O_EXCL`, `config.py:517-530`). So the `pane-died` hook and `_launch`'s late
  check cannot both present one exit.
- **Every such guard has a test that goes red without it.** Each is tested against another plane's
  pane and against an unmarked pane.

## Chat ids that are never handed out again

**The counter.** `state.new_chat_id(workspace)` keeps its signature and its `mkdir` claim. What
changes is where it starts, and what it writes first.

- **Where it starts.** It starts at `max(mark, traces) + 1`:
  - *mark* is the highest ordinal this plane has ever handed out for the id's prefix;
  - *traces* is the highest ordinal any leftover on disk still carries.
- **What it writes first.** Under a lock, for each candidate ordinal `n`, it first raises the mark to
  `n` and only then claims the directory with `config.claim_private_dir`.
  - A mark that could not be written ends the allocation. It answers `None` before any directory is
    made, and the launch says the id record could not be written.
  - So no id is ever handed out unrecorded.
  - A claim that then fails costs an ordinal the mark already covers, which is harmless.
- **The `mkdir` still decides who wins.** Two allocators may compute the same start, and one of them
  then gets `FileExistsError` and moves on, which is today's rule. `new_chat_id`'s own docstring says
  *"a scan is not a substitute"* (`state.py:223-226`): the scan only chooses where to start, never who
  owns a name.
- **The mark only goes up, under a lock.** The read-max-write runs under `fcntl.flock` on a lock file
  beside the mark, opened through `config.open_for` (`config.py:430`). A mark that went down would
  hand an id out again.
- **Keyed by the id's prefix, not the workspace name.**
  - The id is `state.workspace_prefix(ws)`, and that function maps every character outside
    `[A-Za-z0-9_-]` to `_` (`state.py:79-102`).
  - The workspace alphabet allows a dot, so `a.b` and `a_b` share the prefix `a_b`.
  - Two marks keyed by name would each hand out `a_b.3`.
- **Ordinals are bounded by what a strip can sort.**
  - An ordinal of more than `chats._MAX_ORDINAL_DIGITS` digits (5, `chats.py:355`) is ignored by the
    trace scan, never raised on.
  - `new_chat_id` refuses to hand out an ordinal past 99,999 for a prefix.
  - Handing out an id no scan could count would reopen #1101.
- **`_CHAT_ORDINAL_MAX` bounds the attempts from the start, not the ordinal.** Today it caps how high
  allocation may count (`state.py:132-141`), and a counter that never goes down would turn that cap
  into a lifetime limit per workspace.

**Where it lives.** One file per plane, `.charter/frame/chat-ids.json`, maps each prefix to its
highest ordinal. Its lock is `.charter/frame/chat-ids.lock`.

- **A file, never a directory.** Every scan of the frame root reads a directory as a chat:
  - `leave.plane_chats` (`leave.py:227-231`);
  - `chats._by_workspace` (`chats.py:261-337`);
  - `state.reap` (`state.py:2790`).

  A directory there would be listed as a tab, and removed by a reap of the legacy server.
- **Skipped by every scan already.** Like `reopen.json` (`reopen.py:53-57`) and `<id>.transcript`,
  the file is skipped by each of those scans, and `reopen.prune_transcripts` touches only
  `*.transcript`.
- **Scratch that survives being lost.** The mark lives under `NO_FORMAT_PROMISE` (`state.py:181-186`),
  which allows any release to change its shape. Losing it costs nothing, because the traces are
  scanned on every allocation, not only the first.
  - A lost mark falls back to the highest ordinal a leftover still carries.
  - An id that leaves no leftover has nothing to hand down.

**The traces** are the migration, and they are scanned on every allocation:

1. chat directories `<prefix>.<n>/` in the frame root;
2. `<prefix>.<n>.transcript` files beside them (`reopen.TRANSCRIPT_SUFFIX`);
3. every `chat` field in `reopen.json`, read raw, because `reopen.read` returns `None` for a version
   it does not speak (`reopen.py:324-325`), and that file still names ids a transcript carries;
4. `.charter/sessions/<prefix>.<n>.*` markers. They are removed only when a reap removes the directory
   (`state._forget_session`, `:2622-2683`), so a marker with no directory beside it is an id that
   still has something to hand down. The controller accepted this trace.

**`chat-turns/<chat>` is keyed by the id too, and is not a trace.**
- It is `inflight._turn_file`, under `STATE_DIR/chat-turns/` (`inflight.py:418-420`).
- It stands for a turn at most `inflight.TURN_STALE_SECONDS` (ten minutes, `inflight.py:415`), and
  it marks a spinner.
- It is never a record a new chat could be offered.

**A restored chat keeps its id.** `_reopen_one` hands `Reopening` the recorded id, and `_launch`
claims exactly that directory (`state.claim_chat_id`) instead of allocating one.

When the claim fails because the directory exists, `_launch` asks for proof before it gives up. It
runs ONE `list-panes -a` on that chat's recorded server (`_chat_pane_rows`, `commands_frame.py:9996`),
and chooses:

- **No row carries `@charter_chat == <id>` with `@charter_plane == _this_plane()`, and the
  directory's claim marker names no live launcher (`state._claiming_pid`):**
  - the directory is dead;
  - it is reaped (`state.reap_chat`, which is `rmtree` plus `_forget_session`);
  - the claim is made again.
- **The server does not answer:** that proves nothing (#1100, and the controller's second-review
  ruling).
  - **Gone.** Only a server that `tmuxctl.nothing_listening(server)` confirms is gone holds no panes
    (`tmuxctl.py:347` on `main` since `00d69f2`). That means its socket answers `ENOENT` or
    `ECONNREFUSED`. The directory is then reaped as above. `_plane_liveness` decides a reopen by the
    same test.
  - **Wedged.** A server that timed out, or failed any other way, may still be running the chat:
    #1100 measured a SIGSTOP'd server timing out while its chats were live. So the chat is refused
    with `KEPT_ID_TAKEN`, naming the directory and what clears it — a retry once that server answers,
    or removing the directory once the operator knows the server is gone.
  - **Only this chat.** Neither answer says anything about any other chat or server.
- **Otherwise:** the chat is refused with `KEPT_ID_TAKEN`. The sentence names the directory, and the
  exact command that clears it: the `tmux … kill-window -t <window id>` of the live window it found,
  then `charter reopen`, or the retry once a live launcher finishes.

Either way the chat stays in the manifest for a retry (`_consume`). With the id unchanged,
`_restore_recorded_chat`'s transcript rename is a no-op, which its equality test already allows
(`commands_frame.py:10798-10801`).

## The session link

**One link per chat, in the chat's own directory, and carried in the manifest.** The files that exist
today keep their meaning:

- `session`: the gauge's mapping, which `clear_shape` deletes;
- `session.durable`: the link.

Three files are added, and none of them is ever opened, shown or passed to a command:

- **`conversation`** — the path the harness itself named for this chat's conversation. It is only
  ever `stat`ed.
- **`harness.pid`** — the `CLAUDE_PID` of the report that adopted the link. Claude Code only.
- **`session.adopted`** — a claim, created with `O_EXCL` and cleared by every start, that this start
  has adopted its report.

`reopen.Chat` gains `conversation: str = ""`, defaulted like `profile` and `brief`, so a manifest from
0.62.0 still reads (`reopen._chat`).

**Measured, 2026-09-15.** Task 1's Step 0 took claude 2.1.272 live. codex-cli 0.147.0 and opencode
1.18.23 were read from their tagged source and not run. The readings are in the controller's
`exit-gate-t1-step0.md`, and the plan's Task 1 copies its table.

| Harness | Who chooses the id | Recorded when | Resumes with | The conversation exists when |
|---|---|---|---|---|
| Claude Code | charter: `--session-id <uuid> --name <name>` on every fresh start | by the launcher before the `exec`; adopted by the first SessionStart reporting that id (C1, and C3 for a resume) | `--resume <uuid> --name <name>`, which keeps the id and renames the session (C3) | the `transcript_path` a hook payload named is a file — on a fresh start only after the first prompt (C1, C2); on a resume it is already a file at SessionStart (C3) |
| Codex | Codex | at SessionStart, which Codex runs inside the **first turn**, not at launch (X1) | `codex resume <id>` (X2) | the `transcript_path` it named is a file; the rollout exists before the hook runs (X1) |
| opencode | opencode | at the chat's first tool hook: `input.sessionID`, shaped `ses_…` (O1) | `opencode -s <id>`, run in the chat's recorded directory, because opencode looks the id up in the working directory (O2) | a tool hook has reported the id — opencode names no transcript file (`opencode.py:318-326` builds the payload without one) |

- **Charter chooses only where the harness lets it.** A UUID is minted with `uuid.uuid4()` by the
  launcher at the `exec`, so every route that starts a harness gets one: the CLI, `+`, a tab, a
  reopen, a handoff, and a pick in an ended tab. `launcher.attempt`'s `on_exec` writes it to
  `session.durable` before `os.execvpe`.
- **The id and the name are added by the launcher at the `exec`, and never cross tmux.**
  - They ride no `-e` and no launcher argv. A title is free text, and tmux's argument parser has
    already cost this repo #957 and #961.
  - The launcher reads the chat's own record, which it may do only after `framed_chat()` has proved
    the pane (`launcher.py:520-550`).
  - An unframed launch has no chat and gets no link.
- **The operator's own session flag wins.** A launch whose arguments already carry the harness's
  session flag gets nothing added. For Claude Code those are `--session-id`, `--resume`, `-r`,
  `--continue`, `-c` and `--fork-session`. The link is then what the harness reports at SessionStart.
- **Only the chat's own harness may change its link** (controller's rulings on task 1's Step 0
  readings and on the second spec review, 2026-09-15). Every start clears the per-start adoption
  (`session.adopted`, `harness.pid`); nothing else does.
  - **Which harness sent a report comes from the report's own invocation, never from inherited
    environment.**
    - **The command cannot tell them apart.** Claude Code and Codex run the same command, the
      plugin's `charter hook sessionstart` (`hooks/hooks.json`); Codex installs the same plugin
      (`codex.py:84-97`).
    - **Neither can the environment.** A nested harness inherits every `CHARTER_*` variable,
      `CLAUDECODE`, `CLAUDE_PID` and `TMUX_PANE` (C5). `CHARTER_HARNESS=codex` is set only for Codex's
      shells (`codex.py:84-97`), so a `codex` run from a Claude chat's shell still carries
      `CHARTER_HARNESS=claude-code` and the outer `CLAUDE_PID`.
    - **A Claude Code report counts as the chat's own only when both of these hold** (decided by
      C7):
      1. **`CLAUDE_CODE_SESSION_ID == payload.session_id`.** C7 measured the variable equal to the
         payload's `session_id` in all 10 of charter's hooks, at startup and after `/clear`, where it
         changed to the new id. It rejects a nested non-Claude harness, which inherits the outer id
         but reports its own. That is inferred for `codex` and not measured.
      2. **The report's `CLAUDE_PID` relates to the chat** by adopt or follow, below.

      A nested `claude` fails both: it reports a new id from its own pid (C5).
    - **`os.getppid() == CLAUDE_PID` is not a proof** (C7).
      - Claude runs every hook as `/bin/sh -c '<command>'`.
      - The relation held for a lone command such as `charter hook sessionstart`.
      - A compound command such as `charter workspace _reconcile >/dev/null 2>&1` leaves one or two
        shells in between.
      - So it depends on the shell, and dash on Linux may differ.
    - **A Codex report** is one that is not a Claude Code report, and whose payload has exactly the keys
      Codex declares for SessionStart with `deny_unknown_fields`
      (`codex-rs/hooks/src/schema.rs:486-497`): `session_id`, `transcript_path`, `cwd`,
      `hook_event_name`, `model`, `permission_mode`, `source`.
    - **An opencode report** arrives only on the tool route, from the shim. The shim sets
      `CHARTER_HARNESS=opencode` in the call it makes (`opencode.py:331-332`), with the payload it
      builds (`opencode.py:318-326`).
    - **The kind must match.** The chat's recorded kind (`state.identity`) must be the harness that
      sent the report. A report from any other harness changes nothing.
  - **Claude Code — adopt, follow, ignore.** Each step needs a proven Claude Code report (above).
    - **Adopt.** The first SessionStart whose `session_id` equals the chat's recorded id adopts that
      report's `CLAUDE_PID` as the chat's harness pid, written beside the link as `harness.pid`.
      That covers a launch (C1) and a resume, which reports the same id with `source=resume` (C3).
      A start that chose no id — the operator passed their own session flag — adopts its first
      report's id and pid.
    - **Follow.** A later report with a different `session_id` moves the link only when its
      `CLAUDE_PID` equals the adopted pid. That is `/clear` (C6: `source=clear`, a new id, the same
      claude pid), so resume offers the conversation the operator is actually in. The pre-clear
      conversation is still resumable by its own id (C6); charter just stops offering it.
    - **Ignore.** A report is ignored, and never recorded, when it:
      - fails (1);
      - carries a `CLAUDE_PID` other than the adopted pid with an id other than the recorded one — a
        nested `claude` (C5);
      - lacks either variable after adoption.

      Before adoption, a report lacking either variable never adopts.
    - **The adopted pid is learned from the adopting report, never from `#{pane_pid}`.** A profile's
      command may be a wrapper that forks `claude`, and then the pane's pid is the wrapper's.
  - **Codex and opencode — the first report of a start.** Neither report carries a pid that tells a
    nested run from `/new`, so the first id reported after a fresh start is adopted (`session.adopted`,
    created with `O_EXCL`), and a later different id in the same start is ignored.
  - **A resumed Codex start keeps the link it resumed.** Read from source at `rust-v0.147.0` (X3,
    `core/src/session/session.rs:570-594`): a resumed start keeps the thread's id, and takes its
    session id from the resumed rollout's own `SessionMeta`, so a resumed root session reports the
    id it had.
    - The link stays the id `codex resume` was given, whichever id the resumed start reports.
    - A report naming another id is never adopted, and its `transcript_path` is recorded only when
      its id is the link.
    - That holds for either answer X3 could have given.
- **The launcher never infers an id** from a directory, a time or "the latest session". A report the
  rule above ignores changes nothing and is never recorded.
- **Asked of the registry, not spelled per module.** `Harness` gains the members below. Today a resume
  member is refused: `leave.resumable_harness` (`:419-423`) and `_reopen_one`
  (`commands_frame.py:11193-11197`) point at `harness/base.py`'s bar, *"`launch_argv` is
  `[self.binary, *extra]` with no subclass override anywhere in the registry, so the pass-through
  **is** the seam"*. That bar is now met, and for `first_message_argv`'s own reason: three harnesses
  need three spellings, so no single `extra` is right for all of them.
  - `chooses_session_id`
  - `names_its_transcript`
  - `session_flags`
  - `new_session_argv(sid, name)`
  - `resume_argv(sid, name)`
  - `reports_session_at`
- **Held to a shape before it is used.**
  - A session id matches `^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$`. The id reaches a harness argv, and one
    starting with `-` would be read as a flag.
  - A `transcript_path` must be absolute, carry no NUL, and fit `contain.PATH_DISPLAY_LIMIT`.
  - Anything else records nothing. Both arrive in a hook payload, which a chat's own shell can write
    (*Limits*).
- **opencode's report reaches the right chat (ruled, open question 1 (A)).**
  - The shim overwrites `CHARTER_SESSION_ID` in every hook subprocess (`opencode.py:332`, `:379`), so
    that variable does not name the chat.
  - The chat is resolved from `$TMUX_PANE` against the harness panes charter recorded, on the server
    `$TMUX` names (`state.chat_in_pane`).
  - More than one chat recorded on that pane resolves to none, so nothing is recorded.
  - The shim's bytes do not move.
- **"The conversation exists" is a `stat`, asked when a surface is about to offer resume.**
  - Those surfaces are the ended selector, the crash drawer, the quit confirmation's note and
    `_reopen_one`.
  - A transcript deleted since is no longer offered.
  - Charter reads nothing inside the file.

**What reopen and quit say.** `leave._resume_clause` keeps its four sentences, which now depend on the
harness members instead of Claude Code alone.
- `NO_RESUME_HARNESS` (*"records no session id to resume from"*) is said only for a harness whose
  `resume_argv` is `None`.
- `NO_RESUME_YET` covers a link with no conversation yet.

## Titles

- **Stored** in `.charter/frame/<id>/title`, one line, through `state.record_title`. It is carried in
  the manifest as `title: str = ""`.
- **Contained on the way in.** A title goes through `contain.one_line`, holds only printable
  characters, and is at most `TITLE_MAX` = 60 characters.
  - The rename surface refuses a longer one and says how long it was.
  - A brief's first line is cut at 60 with `contain.readable`'s marker. The brief is text a model
    wrote, and the operator approved its whole message at the harness prompt (ADR 0021), not a tab
    label.
  - Every drawn title goes through `contain.readable` (ruling 35), because `title` is a file a chat
    can write.
- **Drawn wherever a chat is named to a person**, which is what decision 11's *"shown in: the strip"*
  extends to:
  - the chat strip, instead of the id;
  - after the id in the tab menu's label, the quit and close confirmation rows (`leave.title`), the
    ended selector's resume row, and the chat picker.

  Nowhere else. A workspace tab's selector names profiles, not chats, so it shows no chat's title.
  The strip's cell map (`slots._Tabs`) stays keyed by the id: a click resolves to the chat, never to
  its words.
- **Claude Code's session name** is `f"{title} · {id}"`, or `id`. The launcher composes it at every
  `exec`.
- **A rename** writes the file, bumps every strip on the plane (`_repaint_the_other_strips`' shape),
  and does nothing else. Claude Code sees the new name at its next start or resume; Codex and opencode
  never see it.

## The four surfaces

### The gate

**`F10`, on charter's own server.** `conf_text` binds it after the palette's hotkey, before
`BAR_ROWS_KEY` and the toggles. The escape hatch stays the last line (`commands_frame.py:1018-1035`).

```
bind -n F10 run-shell '"$CHARTER_PY" -m charter frame-palette "#{client_name}" --gate --chat "#{@charter_chat}"'
```

- The action is a constant. tmux expands both formats at the keypress: the chat from the presser's
  window option, the presser from whoever pressed.
- The presser rides the palette's existing `client` positional (`cli.py:971`). #729 left that
  positional accepted and ignored.

**The presser is recorded on every route, and #729 does not stand in the way.** #729 removed
`"#{client_name}"` from the hotkey bind because its one consumer, `display-message -c`, was gone
(`commands_frame.py:861-876`). *Close charter* is a new consumer, so:

- **`F2`.** The palette's hotkey bind carries the presser again, in the same positional.
- **Pointer doors.** Every door on the identity and attention strips — the workspace chip, the
  persona head, `F2 palette` and the gate button — is reached through the `MouseDown1Pane` bind
  (`conf_text`, `tmuxctl.CLICK_KEY`).
  - The bind's panel branch records the clicking client on the panel pane before forwarding the
    click: `set-option -F -p -t = @charter_presser "#{client_name}"`. `-F` expands the format at the
    press, so the option holds the client's name and not the text `#{client_name}`.
  - The strip's handler reads its own pane's `@charter_presser` in one `display-message` and hands it
    on.
  - It never asks tmux for its last-active client.
- **Unmeasured until task 4.** Whether the recorded presser is the clicking client with two clients
  attached is task 4's Step 0 (G3). If it is not, task 4 stops and goes to the controller.

**The button** is `F10 close`, at the right end of the identity row, on whichever edge the arrangement
places `identity`.
- On a starved row it is dropped after the version and before the identity. At `terse` the version
  goes and the button stays.
- It is a door (`slots._Doors`) that opens the gate rather than the palette, so `_Doors` gains a
  second column set. Its docstring's argument against a mapping, *"whose values nothing branches
  on"*, stops holding once something branches on the value.
- With `[frame] mouse` off, which is the default, it is a label that teaches the key.
- **Inside the operator's own tmux it is not drawn and not published** (decision 7). That is
  `_bottom`'s rule for the `F2 palette` hint, which is dropped wherever charter binds no key
  (`slots.py:2805-2813`).

**The menu** is `frame/gate.py`, in `tabmenu.py`'s shape: a `palette.Palette`, `own_the_tty`,
full-pane and modal, with one key that always leaves.

```
charter · close · 2 to choose from
>   Close charter (keep chats running)
    Close charter and stop all chats…

  up/down move   enter choose   esc cancel   F12 back to the harness
```

- **Close charter (keep chats running)** detaches the presser and nothing else.
  1. The presser is held to tmux's client-name shape.
  2. One `list-clients -F '#{client_name}\t#{session_id}'` on the chat's recorded server proves it is
     attached to that chat's session. The session is `_pane_place(server, <pane>)[0]`, a `$N`
     (`commands_frame.py:7949-7995`). That session's `@charter_plane` must equal `_this_plane()`.
  3. Then `detach-client -t <presser>` runs.

  It never uses a chat id, a session name or `-s`, and it never falls back to every client. When no
  presser can be proven, the row is listed refused with its reason: *charter cannot tell which
  terminal asked, so it detached nothing — press `F10` in that terminal, or close it*. That is #1097.
- **Close charter and stop all chats…** is a doorway. Enter replaces the surface with
  `leave.confirm_rows(leave.plan(...), verb=QUIT)`, which is today's confirmation as a drawer
  (`_as_a_drawer`). It lists every chat, ended tabs included (decision 12), each with what it gets
  back. The keypress on its first row runs `charter frame-quit`.
- **The cursor opens on *Close charter* in every state, refused or not.** It is pinned by row id, not
  left to `palette.aim`, because `aim` opens on the first row that can run, and with the detach row
  refused that would be *stop all chats*. Enter on a refused row says its reason and does nothing.

**`F2` carries the same two rows, in the same words.**
- `frame.detach`'s title becomes the first row's words, and its action is the detach above, with the
  hotkey's presser.
- `leave.OPEN_QUIT` becomes the second row's words.
- Their ids and positions stay. The detach row stays near the top. `leave.open_rows` keeps the stop
  row second to last and *chat: close* last (`leave.py:463-503`), because its guard puts the
  destructive rows at the bottom.

**`F10` is reserved.**
- A `[[frame.component]]` `key = "F10"` is refused the way `F12` is, through
  `instance.component_arrangement`'s `bound` set.
- A `[frame] hotkey = "F10"` is refused, and the shipped `F2` binds instead.

**Inside the operator's own tmux** (decision 7):
- No key is bound: not `F10`, not the hotkey, not `F12` (`docs/frame.md:1094-1105`). No button is
  drawn.
- The two rows are in the palette, which is reached there through a pointer door when their tmux mouse
  is on.
- *Close charter* is listed refused with its reason, `builtin_actions._detachable`'s: the frame is a
  window in their session, and their own prefix key detaches.
- The palette's own cursor rule keeps it off the stop row, which stays second to last. The gate menu
  itself is never drawn there: `frame-palette --gate` on an operator socket draws nothing and says
  the rows are in `F2`.

### An ended tab

**What ends a harness:** its process exits in a chat pane the launcher `exec`'d, for any reason. That
includes `/exit`, a double Ctrl+C, Ctrl+D, a crash, and a signal from outside.

No test or measurement depends on a double Ctrl+C ending a harness. C7 saw two `send-keys C-c` leave
Claude running at startup, so task 2 uses `/exit`, or a stand-in harness.

**`charter frame -- <cmd>` is not a harness and keeps today's ending** (ruled, open question 5).
- A window started by the escape hatch records no profile (`_profile_name(None) == ""`,
  `commands_frame.py:5436-5443`).
- `_launch` installs today's `pane-died[1] kill-window` on it, and not the ended hook.
- `_launch_in_operator_tmux` closes its window at exit, as today.
- The window closes when the command exits, and `attach` returns with its exit code, as
  `docs/frame.md:1394-1401` promises a script.

**On charter's own server, for a profile's harness:**
1. `pane-died[0]` writes the exit code, unchanged (`_pane_died_write_hook_argv`).
2. `pane-died[1]` is no longer `kill-window`. It is the constant action
   `run-shell -b '"$CHARTER_PY" -m charter frame-ended --chat "#{@charter_chat}"'`, installed after
   `[0]`. The order is still the whole fix (`:1444-1456`): an unindexed `set-hook` replaces the array.
3. `charter frame-ended` (`frame/ended.py`) acts only when all of these hold:
   - the chat records a profile;
   - the chat is marked **drawn**, meaning the launch that opened it finished laying it out;
   - the chat is not closed;
   - one listing proves a dead pane for the chat, by *What proves a pane*;
   - `ended` is claimed now, with `O_EXCL`. A claim that already exists means another process has
     presented, or is presenting, this exit.

   Then it bumps every strip, and presents one of two things:
   - **Exit code 0 — a clean exit.** It respawns the proven pane, without `-k`, into
     `frame-launch --select --ended --start <profile> --attended`. The pane is charter's selector
     again; the pane id, the window, `@charter_chat` and the pane's hooks stay.
   - **Anything else — a crash, a signal, or an empty status read as `_UNKNOWN_DEATH_CODE`.** The
     dead pane is left exactly as tmux keeps it (`remain-on-exit`), so the harness's last lines and
     tmux's own `Pane is dead (status N)` stay on screen.
     - The choice opens in a drawer split beneath the proven pane: a pane running
       `frame-palette --ended`, the confirmation drawer's shape (#921, #927).
     - The drawer is marked `@charter_drawer <id>` the moment tmux reports its pane id.
     - The drawer's own process closes its own pane on the way out, proven by `#{pane_pid}`, as
       `_close_the_cancelled_chat` does. So a drawer whose marker never landed still goes.
4. **The choices are the same in both.**
   - **resume &lt;session name&gt;** — first and preselected, and present only when the
     conversation exists. It respawns the proven dead pane into `frame-launch --profile <p>
     --attended --resume`.
   - **start fresh** — in the selector, any profile row starts on a new link, with the cursor on the
     chat's own profile when resume is absent. In the drawer, this row respawns the proven dead pane
     into the selector with no resume row: `--select --ended --fresh --start <profile>`. That is
     decision 4 and its "back to the profile selector"; the drawer does not start a profile directly.
   - **close tab** — a real Esc keystroke in the selector, or a row in the drawer. It is `chat: close`
     without a confirmation (decision 5).

   The resume row's title is the session name, `<title> · <id>` or `<id>`. Its note is
   `<kind> · session <first 8 of the link>`.
5. **Every harness start clears the ended state, in one function.**
   - `ended.reset(fid)` clears `ended`, `exit` and the drawer, killing only a proven drawer pane.
   - It runs from `launcher.attempt`'s `on_exec`, the one place every start reaches: a selector pick
     (`_picked`), the drawer's resume, a fresh start from the ended selector, a reopen, and every
     ordinary launch.
   - `reset` answers whether it cleared an `ended` claim. Its undo claims `ended` again only when
     `reset` cleared one and the `exec` then raised.
   - A resumed chat is then drawn live again: it confirms on close, and its next exit is presented
     again.
6. **End of input is not Esc, and neither is Ctrl+C.**
   - `overlay.Surface.run` answers `None` for both today (`overlay.py:865-905`). It now also records
     which one it was (`left`: a key, or end of input).
   - **Ctrl+C decodes as its own key.**
     - Today `decode` turns `\x03` into `escape` (`overlay.py:687-690`), so a stray third Ctrl+C after
       a double-Ctrl+C `/exit` would close an ended tab for good. It becomes `ctrl-c`.
     - Every surface that cancels on Ctrl+C today keeps doing so, through an explicit mapping
       (`Surface.cancel_keys`): the `F2` palette with its pickers and confirmation drawers, the tab
       menu, and the never-started selector, as #1103 left it.
   - **On an ended selector and in the crash drawer, Ctrl+C does nothing.** No close, no mark, no
     manifest change.
   - **Only a real Esc keystroke in an ended selector closes the tab.**
   - **End of input leaves the tab ended and open.** A closed pty, a killed server or a machine that
     went down all end input. The launcher exits and writes no closed mark, calls no
     `_forget_transcript`, and drops no manifest entry. The pane's own death then reaches
     `frame-ended`, which finds `ended` already claimed and does nothing.
   - A pane that never started a harness keeps today's rule: its selector closes on Esc, on Ctrl+C
     and at end of input.

**The launch's own early-death path is unchanged (#384).**
- A harness dead before the chat is drawn is still reported and its window killed by `_launch`.
- `frame-ended` does nothing for a chat that is not marked drawn.
- After `_launch` writes the drawn mark, it asks `_query_pane_dead_status` once more, and runs the
  ended step itself if the pane died in between. The `O_EXCL` claim is what keeps that from ever
  presenting twice.

**The teardown-hook refusal stays, reworded.** `_launch` still refuses to attach when `[1]` did not
install. The reason is no longer an `attach` blocked forever, but a tab whose exit nothing would answer
(`:6362-6374`).

**`charter claude` returns when the tab closes or you detach, not when the harness exits.**
- `attach` returns when its session's last window goes.
- The launch then returns the exit code the chat recorded (`state.exit_code`), else 0. That is the
  same rule it has today, arriving later.
- A pipe, `--no-frame` and the escape hatch keep their own code (`docs/frame.md:1394-1401`).

**Inside the operator's own tmux** there are no hooks (`_launch_in_operator_tmux`'s docstring,
`:3495-3505`), and the launcher is awake for the life of the frame.
- When `_wait_for_harness` sees a drawn profile chat's harness die, the launcher runs the same ended
  step in-process and goes back to waiting.
- It returns when the window is gone.
- A harness that dies before the window is drawn keeps today's path.

**Background and handed-off chats** end through the same hook, whether or not anybody is looking.
- The strip marks the tab `x`, in the cell the working spinner uses, and draws the name dim. `x` is
  ASCII, by `slots._BAR_RULE`.
- The tab switches as any tab does, and the choice is waiting there.

**A reopened ended tab comes back as a window whose pane runs the ended selector.**
- Before tmux, `_launch` records the chat's profile and its harness kind from the manifest
  (`reopen.Chat.profile`, `reopen.Chat.harness`), beside the link and conversation task 1 restores.
  So `resume_row` answers on a restored tab exactly as on the one that ended. It also derives the
  kind from the recorded profile when the identity record holds none.
- It is the one selector an open nobody is at may reach. That does not contradict `argv_select`'s
  rule that *"a selector is a question, so the only open that may reach one is an open somebody is in
  front of"* (`launcher.py:207-221`): nothing starts until somebody switches to it and presses Enter.

### Closing a tab

- **Closing is still `cmd_close`**: the mark, dropping the transcript and the manifest entry, handing
  the client another chat, and killing the window (`commands_frame.py:10491-10564`). It now also:
  - drops `conversation`, `title` and a proven drawer;
  - leaves the harness's own conversation store untouched. Charter deletes nothing a harness wrote.
- **It confirms only when a harness is running.** `leave.needs_confirming(chat)` is true when the
  chat is neither ended nor still waiting at the selector.
  - The tab menu's close row (`tabmenu.catalogue`) and `F2 → chat: close` (`leave.open_rows`) are
    doorways only then.
  - On an ended tab the row is an action: `frame-close <id>` on Enter, and its title says the harness
    has ended. It stays the last row either way.
  - `-` still opens the tab menu; it does not close on the press.
- **A real Esc in an ended selector closes the tab for good.**
  - It runs `cmd_close` in-process, not `_close_the_cancelled_chat`, which closes a pane that never
    became a chat and must not write the closed mark (`launcher.py:656-704`).
  - An ended tab was a chat, and forgetting it is what close means.
  - End of input and Ctrl+C are not Esc (above).

### Quit, and reopening ended tabs

- **Quit records every open tab, ended ones included.**
  - `leave.plan` already keeps an ended chat, which is neither closed nor waiting.
  - `leave.Doomed` and `reopen.Chat` gain `ended: bool = False`.
  - The recorder (`record_the_plane_now`) writes the same field, so a machine that goes down keeps
    ended tabs too.
- **A quit does not capture an ended tab's pane.** That pane is charter's selector or a drawer, never
  the harness's screen. `_record_the_plane` skips `_capture_transcript` for an ended chat and names the
  `<id>.transcript` it already has, as `capture=False` does (`commands_frame.py:10353-10360`). The
  ended tab keeps its harness's last transcript.
- **The confirmation's note for an ended chat** replaces the old `already ended on its own (N)` clause
  (`leave._ended`) with `ended — comes back ended; resume offered` or `ended — comes back ended;
  nothing to resume`.
- **`charter reopen` brings an ended chat back ended.**
  - `_reopen_one` keeps its id (above).
  - It writes the chat's link, conversation, title, profile and kind into the claimed directory before
    tmux.
  - It launches the pane at `frame-launch --select --ended --start <profile>` and starts no harness.
- **Every other chat reopens as today.** Its conversation resumes when the link and the conversation
  exist, for every harness whose `resume_argv` answers: `frame-launch --profile <p> --resume`. `rest`
  no longer carries `--resume`.

## ADR 0018, amended — draft for task 2's PR

> **Amendment, 2026-09-15: a harness exit is no longer final — the pane that emptied offers a choice.**
>
> The 2026-09-12 selector amendment ended on *"A pane whose harness later EXITS closes as it always
> did and never comes back to the selector — going back would be charter drawing in a pane a harness
> has run in"* (0018:333-337). The operator ruled otherwise: no harness exit destroys a chat. Ctrl+C
> stays exactly as each harness defines it, because a chat is protected by what charter does after an
> exit and not by taking a key away — the consequence at 0018:96-98.
>
> **The distinguishing question moves from the past tense to the present.** It was *has a harness
> ever run in this pane, and is charter's own process still the one in it?* It is now: **is a harness
> process in this pane right now?** No, and the pane is charter's again — before the first `exec`, and
> after any exit. Yes, and the rule is unqualified: charter draws nothing there and reads it only at
> the two moments of the 2026-09-01 amendment. It is checkable from outside: tmux lists the pane
> `#{pane_dead}` before charter touches it, and `ended` is claimed before any respawn.
>
> **What charter puts there after an exit, and nothing else.**
> - A clean exit respawns the pane into the profile selector — a `frame/overlay.py` surface bounded
>   exactly as the selector already is, with one extra row, resume.
> - A crash leaves the dead pane untouched — tmux keeps the harness's last lines on its own screen —
>   and the choice opens in a drawer pane split beside it. Charter does not draw in the dead pane at
>   all.
> - `charter frame -- <cmd>` is not a harness: its window closes at exit as it always did.
>
> **Bounded, and each bound is checkable.**
> - **Charter restarts nothing by itself.** Every harness start after an exit is an operator's Enter
>   on a row. No timer, no retry, no automatic resume; an ended tab nobody switches to stays ended,
>   and neither end of input nor Ctrl+C is ever read as a choice.
> - **Charter never types into a harness.** No `send-keys`, no `/rename`, no `/resume`. A resume is an
>   argument at the `exec` (`--resume`, `resume`, `-s`), and a title reaches Claude Code as `--name`
>   at the next `exec`.
> - **Charter reads nothing to decide.** The presentation is chosen from the exit code the
>   `pane-died[0]` hook wrote and from the link charter recorded, never from what the harness printed.
>   The last lines stay because charter leaves that pane alone, not because it reads them.
> - **Charter acts on a pane only as a listing proves it.** Every respawn, split and kill targets the
>   pane id that one listing on the chat's own server reported under this chat and this plane; a
>   respawn never passes `-k`, so tmux itself refuses a live pane.
> - **The launch's own early death is unchanged.** A harness dead before its chat was drawn is
>   reported and its window closed, as #384 requires.
>
> *Recorded in the pull request whose code first relies on it (ruling 44).*

## ADR 0024 — draft for task 1's PR

`docs/adr/0024-a-chat-id-names-one-chat-and-one-harness-session.md`. The number is the next free one at
`5ad755d`, and the test pins the slug, not the number, as the profile ADR's test does.

> # A chat id names one chat for the life of the plane, and one harness session
>
> **The failure.** `state.new_chat_id` handed out the lowest free ordinal, and a reap freed an ordinal
> with the directory. Two things keyed by the id live outside that directory — the captured transcript
> and the quit record — so a new `default.1` was offered an unrelated chat's scrollback, and closing
> it deleted the earlier chat's record (#1101). Separately, charter knew which conversation a chat
> held only for Claude Code, only after its first hook, and lost it at the reap.
>
> **The decision.**
> 1. **A chat id is handed out once per plane.** The allocator starts above the highest ordinal this
>    plane has handed out for the prefix, and above every ordinal a leftover still carries; the mark is
>    written before the claim, so no id leaves unrecorded, and the `mkdir` still decides who wins. A
>    restored chat keeps its id.
> 2. **A tab is linked to exactly one harness session, never guessed.** Charter chooses the id where
>    the harness takes one (Claude Code's `--session-id`), and records the id the harness reports where
>    it does not (Codex at SessionStart, opencode at its first tool hook). Only the chat's own harness
>    may change the link — a report from a harness nested inside the chat changes nothing.
> 3. **Resume is offered only when the conversation exists** — the transcript the harness named is a
>    file, or, for opencode, which names none, a hook has reported the session.
> 4. **A title is display; the id is identity.** Claude Code's session name is composed from both at
>    every `exec`.
>
> **Considered and rejected.**
> - *The lowest free ordinal, with every leftover deleted at the reap* — a leftover charter does not
>   yet know about is inherited the day it is written; never reusing the name needs no list.
> - *A hash id* — `new_chat_id`'s own docstring: a truncated hash collides silently.
> - *Ids renamed to titles* — every `$CHARTER_SESSION_ID` already exported into a live process would
>   name nothing (`state.py:235-241`).
> - *`--continue`, or "the newest session in this directory"* — two chats of one workspace share a
>   directory, so that is the guess this ADR exists to refuse.
> - *Any report carrying the chat's variables* — a `claude -p` in the chat's own shell carries them
>   too.
> - *Typing `/rename` or `/resume` into the harness* — ADR 0018.
> - *`codex resume <name>`* — Codex has no flag to set a name (openai/codex#14482), so a name charter
>   did not set cannot identify a session.
>
> **Measured, 2026-09-15** (claude 2.1.272 run live; codex-cli 0.147.0 and opencode 1.18.23 read from
> tagged source, not run; tmux 3.7c).
> - Claude Code reports the chosen id at SessionStart, but writes no transcript until the first prompt.
> - A resume reports the same id and renames the session.
> - A nested `claude` reports a new id from its own `CLAUDE_PID`.
> - `/clear` reports a new id from the same `CLAUDE_PID`.
> - Every Claude Code hook's `CLAUDE_CODE_SESSION_ID` equals its payload's `session_id`, at startup and
>   after `/clear`, while a hook's parent is Claude only for a lone command: Claude runs hooks under
>   `/bin/sh -c` (C7).
> - Codex reports at its first turn, not at launch.
> - `opencode -s` looks the id up in the working directory.
>
> **Consequences, including what they cost.**
> - One more file and a lock under `.charter/frame/`, and ordinals only grow: a prefix hands out at
>   most 99,999 ids in the life of a plane (`chats._MAX_ORDINAL_DIGITS`).
> - A Claude Code report counts only when `CLAUDE_CODE_SESSION_ID` equals its payload's id, and its
>   `CLAUDE_PID` either becomes the adopted pid through the recorded id or equals it. A parent-pid test
>   was measured unreliable behind `/bin/sh -c`, and a nested non-Claude harness is rejected by
>   inference, not by measurement.
> - The link is as trustworthy as the hook that writes it: a process that fakes the payload,
>   `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID` together can point a tab at another conversation.
> - Claude Code's link follows `/clear`, so resume offers the conversation the operator is in; the
>   pre-clear conversation is still in `claude --resume`'s own picker, but charter stops offering it.
> - Codex and opencode carry no pid a hook can compare, so each start adopts its first reported id:
>   after `/new` inside Codex, or a new session inside opencode, resume offers that start's first
>   conversation.
> - A Codex chat that has not taken a turn has no link, and a Claude Code chat with no prompt has no
>   transcript, so neither is offered resume.
> - An opencode conversation that never ran a tool reports no session, and one whose chat is reopened in
>   another directory reopens empty.
> - A rename reaches Claude Code only at its next start or resume, and Codex and opencode keep their
>   own session names.

## The bugs this closes

- **#1097** — `F2 → detach` names a chat id as the tmux session.
  - *Task 4:* `_detach` detaches the presser, proven attached to that chat's session on this plane,
    and never a name.
  - It gets a real-tmux test that attaches two clients, presses in one and sees only that one go,
    which the issue asks for.
- **#1101** — reused chat ids inherit transcripts and quit records.
  - *Task 1:* ids are never handed out again.
  - Its test closes `default.1` with a transcript and a manifest entry, then opens a chat, and asserts
    three things:
    - the new chat is not `default.1`;
    - it is not offered that transcript;
    - closing it leaves `default.1`'s record.

## What this changes elsewhere

- **The IDE spec's §4j becomes true, and §5's open question is answered.** Both get a dated note, not
  a deletion.
- **CONTEXT.md.**
  - The **Chat** entry gains *handed out once* and *linked to one harness session*.
  - **Ended tab** and **Exit gate** are added.
  - **Title** is added, avoiding *name*.
- **`docs/frame.md`** changes in these sections:
  - *Leaving* — the gate first, then close, quit and reopen;
  - *Exit codes*;
  - *Inside a tmux you already have* — its first paragraph says the window closes when the harness
    exits, which no longer holds for a profile's harness;
  - *How a harness starts* — the ADR sentence;
  - the selector section;
  - `+`, tabs and the identity row's buttons (`:366-372`).
- **`docs/harnesses.md`** *What each harness lets charter offer* gains a resume row per harness. The
  session-name limit goes beside it.
- **`hooks._record_harness_session`** is gated on the registry (`reports_session_at ==
  "sessionstart"`), not on `claude-code`, and on the adoption rule (*The session link*). An opencode
  route records from the tool hook. Both record `conversation` when the payload names a path.
- **`leave.resumable_harness`** asks the registry's `resume_argv`. The docstrings that argue against a
  resume member are rewritten, not left contradicting the code.
- **Docstrings that say an ordinal is recycled** are rewritten in task 1:
  - `state.new_chat_id`, `_forget_session`, `clear_exit`, `clear_shape` and `clear_respawn`;
  - `leave._group`;
  - `reopen.Chat.chat` and `reopen.Chat.workspace`;
  - `_restore_recorded_chat`, and `_launch`'s notes at `:6505-6509`.

## Limits

- **Ctrl+C is not disabled.** The operator's first ask is not built (decision 1). A double Ctrl+C still
  ends Claude Code. What changes is that the tab stays and offers resume.
- **A rename reaches Claude Code only at its next start or resume.** Codex and opencode keep the session
  names they chose. Neither has a flag charter could pass: openai/codex#14482 is open, and opencode's
  `--title` exists only on `opencode run`. Charter never types `/rename`.
- **`F10` is taken from every pane on charter's own server, the harness's included.** A root-table bind
  is server-wide (`conf_text`'s docstring), so a harness, or a program run in its pane, that wants F10
  no longer gets it there.
- **The button is unclickable on most planes.** `[frame] mouse` ships off, so the key is the gate.
- **Inside the operator's own tmux there is no gate key, no palette key and no button.**
  - The rows are reached through a pointer door only when their tmux mouse is on.
  - *Close charter* is refused there, because their prefix key detaches.
- **A detach nobody can attribute does not happen.** A route that cannot prove its presser refuses
  *Close charter* with its reason, and the terminal stays attached.
- **A session charter did not mark is left alone.** A pane carrying no `@charter_plane` gets no ended
  presentation, no drawer and no respawn. That covers two cases:
  - a workspace session an older charter created before #793 wrote the marker;
  - a plane whose state path holds a tab or a newline, which is left unmarked
    (`_plane_option_argv`).

  Its harness's exit leaves a dead pane with tmux's own `Pane is dead`, until the tab is closed.
- **The session link is as trustworthy as the hook that writes it.** A process that forges a payload,
  `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID` together can point a tab's resume at another conversation.
  A nested non-Claude harness is rejected because it reports an id other than the one it inherited,
  which is inferred for `codex` and not measured.
- **After `/clear`, resume offers the new conversation.** The pre-clear one is still in Claude Code's
  own `claude --resume` picker (C6), but charter stops offering it.
- **Codex and opencode follow no `/new`.** Neither report carries a pid a hook can compare, so each
  start adopts its first reported id. After `/new` inside Codex, or a new session inside opencode,
  resume offers that start's first conversation.
- **No resume is offered before a conversation exists.**
  - A Claude Code chat with no prompt has no transcript yet (C1).
  - A Codex chat reports nothing before its first turn (X1).
  - An opencode conversation that never ran a tool reports no session: opencode has no session-start
    event (`docs/frame.md:1252-1255`) and names no transcript.
- **An opencode chat resumes only in its recorded directory** (O2). One that a reopen moves into its
  workspace (#867) reopens empty and says why.
- **Claude Code's in-session `/resume` hides the session it is run in** (C3). A title charter composes
  is listed by a fresh `claude --resume` picker, and by `/resume` in every other session.
- **Resume is a `stat` at the moment of offering.** A transcript removed afterwards gets the harness's
  own error at resume.
- **Ended tabs stay until somebody closes them.** The IDE spec's *"the open set only ever grows"* (§5)
  now covers ended chats too.
- **A crashed tab's last lines are not captured by a quit.** They stay on screen until the tab is
  closed or quit. A reopened crashed tab comes back as the selector.
- **A terminal left attached shows the choice, not the shell.** `charter claude` returns when you
  detach or close the tab.
- **Ids only grow.** A prefix hands out at most 99,999 ids in the life of its plane.

## Open questions, and their rulings

1. **opencode's chat id on the hook path (#946).** **Ruled: (A).**
   - The shim sets `CHARTER_SESSION_ID` to opencode's own id in every hook subprocess
     (`opencode.py:332`, `:379`).
   - The chat is resolved from `$TMUX_PANE` against the harness panes charter recorded, on the server
     `$TMUX` names, and a pane two chats claim resolves to none.
   - The shim's bytes do not move. #946 stays its own issue.
2. **The facts decision 10 rests on.** **Answered 2026-09-15 by task 1's Step 0**
   (`exit-gate-t1-step0.md`). claude 2.1.272 was run live on the operator's account; codex-cli
   0.147.0 and opencode 1.18.23 were read from their tagged source, not run; tmux was 3.7c. No
   C-reading failed.
   - **C1 — launch.** `--session-id <uuid> --name …` reports that uuid at SessionStart
     (`source=startup`), with `transcript_path` present. The file does not exist at SessionStart, nor
     after a quit with no prompt.
   - **C2 — first prompt.** After it, the transcript is a file.
   - **C3 — resume.** `--resume <uuid> --name "t2 · beta.1"` brings the conversation back with the same
     `session_id` and `source=resume`. It writes the new name with no prompt. A fresh
     `claude --resume` picker lists `t2 · beta.1`. The in-session `/resume` hides the current session,
     so the operator's "findable via /resume" holds from every other session.
   - **C4 — combining flags.** `--resume` with `--session-id` is refused (`--session-id can only be
     used with --continue or --resume if --fork-session is also specified`, exit 1). Charter never
     combines them.
   - **C5 — a nested harness.** A nested `claude -p` reports its own SessionStart with a new
     `session_id`, and its hook descends from the outer claude. The environment cannot tell the two
     apart: `CLAUDECODE`, `TMUX_PANE` and `CHARTER_*` are all inherited. `CLAUDE_PID` does, and it
     equals each hook's `$PPID`.
   - **C6 — `/clear`.** It reports `source=clear`, a new `session_id` and the same `CLAUDE_PID`.
     `claude --resume <original uuid>` still brings back the pre-clear conversation.
   - **X1 — Codex.** SessionStart carries `session_id` and `transcript_path`, and the rollout exists
     before the hook runs (`codex-rs/hooks/src/schema.rs:486-497`, `core/src/session/mod.rs:4074-4085`).
     It runs at the first turn, not at launch (`core/src/session/turn.rs:233`, `:457`).
   - **X2 — Codex resume.** `codex resume <uuid>` looks that id up exactly (`tui/src/lib.rs:626-659`).
   - **O1 — opencode.** `tool.execute.before`'s input carries `sessionID`, shaped `ses_…`.
   - **O2 — opencode resume.** `opencode -s <id>` validates the id against the working directory
     (`src/cli/tui/validate-session.ts:7-28`), so resume must run in the chat's recorded directory.
   - **X3 — a resumed Codex start.** Read from source at `rust-v0.147.0`, not run.
     - It keeps the resumed thread's id, and reports the session id recorded in that rollout's own
       `SessionMeta` (`core/src/session/session.rs:570-594`).
     - For a root session that is the id it had, so `codex resume` accepts it again (X2).
     - The link rule is written to hold either way.
   - **C7 — which proof shows a Claude Code hook belongs to that Claude process.** Taken 2026-09-15
     with charter's real hook command forms, claude 2.1.272, no prompt (`exit-gate-t1-step0.md`,
     section C7, with raw evidence in `c7-evidence/`).
     - **The session-id variable is a proof.** `CLAUDE_CODE_SESSION_ID` equalled the payload's
       `session_id` in all 10 hooks, at startup and after `/clear`. After `/clear` it changed to the
       new id, while `CLAUDE_PID` stayed the same.
     - **The parent pid is not.** `os.getppid() == CLAUDE_PID` held only for a lone command, because
       Claude runs hooks as `/bin/sh -c` and a compound command keeps one or two shells in between.
     - **Not measured:** a nested `!` child, because `!` costs a model turn; and a nested `codex`,
       which is inferred.
     - **A note for task 2.** Two `send-keys C-c` did not end Claude at startup in that run. No test or
       measurement may depend on a double Ctrl+C exiting: use `/exit` or a stand-in harness.
   - **What they settled, by the controller's ruling:**
     - Claude Code's link adopts, follows `/clear` and ignores a nested harness, by
       `CLAUDE_CODE_SESSION_ID` and `CLAUDE_PID` together (C5, C6, C7).
     - Codex and opencode adopt the first report of each start.
     - A Codex chat has no link before its first turn.
     - opencode resumes only in its recorded directory.

     See *The session link*.
3. **Which terminal *Close charter* detaches from a pointer.** **Ruled: record the presser, never adopt
   tmux's last-active client.**
   - The `MouseDown1Pane` bind records `#{client_name}` on the panel pane.
   - The hotkey and `F10` binds carry it too.
   - Task 4's G3 measures that the recorded name is the clicking client with two clients attached.
   - If it is not, task 4 stops and goes to the controller. A detach-all never ships.
4. **`pane-died` on Linux for a pane whose status and signal both come back empty.** **Open, and a
   pre-merge gate on task 2** (controller's ruling).
   - *Evidence:* a cancelled selector pane on the CI runner was marked dead with both empty, and its
     window was still listed a minute later, with both hooks present
     (`launcher._close_the_cancelled_chat`'s docstring, `:660-669`). Whether a harness killed from
     outside can reach that state is unmeasured.
   - *Resolution:* task 2's real-tmux module runs a signal death on CI's runner. If the hook does not
     fire, the controller rules on a fallback before task 2 merges. The candidate is the chat strip's
     panel running `frame-ended` for a chat whose pane its listing reports dead, which reads
     `#{pane_dead}`, a state, not content. The task does not invent one.
   - *The gate, written into task 2:*
     - the Linux CI case runs unskipped;
     - its stand-in harness enters raw mode before it exits or is killed;
     - it covers both exit 0 and `SIGKILL`.
5. **Scope: `charter frame -- <cmd>`.** **Ruled: keep today's behaviour.** The window closes when the
   command exits, and the exit code is returned to the script, with no ended tab (*An ended tab*,
   above).

## Done when

On this plane, with the operator, after task 4 is on `main`:

- One workspace holds three chats, started from the selector on `claude`, `codex` and `opencode`.
- **A clean exit.** In the Claude Code chat, `/exit` leaves the tab, and the selector opens on
  **resume &lt;title · id&gt;**. Enter brings the conversation back, and `ps -o command= -p $PPID`
  inside it shows `--resume <uuid> --name`. A second `/exit` offers the choice again.
- **A nested harness.** `claude -p hi` run from that chat's shell does not change the tab's link.
- **`/clear` is followed.** `/clear` in that chat moves the link, and the next `/exit` offers to resume
  the post-clear conversation.
- **A crash.** `kill -9` of the Codex chat's harness from another terminal leaves the last lines, and
  the drawer offers resume. Resume brings the conversation back through `codex resume <id>`. *start
  fresh* opens the selector.
- **This terminal only.** With two terminals attached to one workspace, `F10` → *Close charter (keep
  chats running)* in one detaches only that one, and so does `F2` → the same row.
- **Stop all chats.** `F10` → *Close charter and stop all chats…* lists all three, and the ended one
  says so. `charter reopen` brings back every chat under its old id. The ended one comes back ended,
  and its resume row works.
- **Ids.** Close one chat and open a new one: its id is higher than every id before it, and
  `F2 → chat: previous transcript` is refused on it.
- **Titles.** Rename a tab. The strip shows the title at once, and the next Claude Code resume shows it
  in `/resume`'s list.
- **The escape hatch.** `charter frame -- sh -c 'exit 7'` closes its window and returns 7.

## Build order

1. **Never-reused ids and the session link** (decisions 9–10; closes #1101; ADR 0024).
2. **An ended harness keeps its tab** (decisions 4, 5, 12; the ADR 0018 amendment).
3. **Titles and rename** (decision 11).
4. **The exit gate** (decisions 2, 3, 6, 7; the #1097 fix).

**Order: 1 → 2 → 3 → 4, one at a time, each its own reviewed PR.**
- Task 2's resume row and reopen need task 1's link and kept ids.
- Task 3 composes the name task 1's launcher passes, and draws it on task 2's resume row.
- Task 4's confirmation lists task 2's ended tabs.

Each task moves the docs for what it changes in its own PR, ships its own news entry, and ships the ADR
text its code first relies on (ruling 44).
