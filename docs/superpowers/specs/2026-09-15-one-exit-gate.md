# One exit gate — a harness that ends keeps its tab, and every tab names one harness session

**Status:** agreed 2026-09-15, in a six-round grill between the operator and the `steward`
persona. Every recommendation was confirmed ("agree", "agree all", "yes good"), and the
operator added the session-name and tab-id ideas. Decisions, the delivery order and the
evidence gathered: `workspaces/harness-profiles/refs/exit-gate-decisions.md` in the plane.
**Against:** `origin/main` `5ad755d` (0.62.0). Line anchors below are at that commit; re-anchor
by symbol name. Open PRs #1100 and #1103 touch `_launch`, `launcher.py`, `chats.py` and the
reopen path, so each task re-reads their merged diffs before it starts.
**Closes:** #1097 (task 4) and #1101 (task 1).

## The failure

The operator asked for three things:

> force-disable Ctrl+C in harnesses; a top-right close button offering "close charter only"
> (default) and "close charter with all sessions"; one gate to exit, so a harness cannot be
> closed inside charter except by closing its tab.

What the code does at 0.62.0:

- **Every harness exit destroys its chat.** `/exit`, a double Ctrl+C, Ctrl+D and a crash all
  end the pane. `pane-died[1]` is `kill-window` (`commands_frame._pane_died_teardown_hook_argv`,
  `:1421-1459`), and the next launch's `state.reap` deletes the chat directory. The resume id
  goes with it (`session.durable` lives in that directory, `state.py:799`), unless a recorded
  manifest still names it. Inside the operator's own tmux the launcher closes the window
  itself (`_launch_in_operator_tmux`, `_close_window`). Nothing respawns anything.
- **The IDE spec promised the opposite and nobody built it.** §4j says *"The chat becomes a
  dead tab and is kept"*
  (`docs/superpowers/specs/2026-08-30-charter-opens-like-an-ide.md:469-474`). §5 lists it as
  an open question (`:521-522`), and §6 stage 6 was never delivered (`:599-600`).
- **Five ways out, and none of them is one gate.**
  - Closing the terminal detaches.
  - `F2 → detach` runs `detach-client -s <chat id>`, which names nothing (#1097).
  - `F2 → charter: quit` and `charter frame-quit` record the plane and stop it, after a
    confirmation (`leave.confirm_rows`).
  - `F2 → chat: close` stops one chat for good.
  - `-` on the chat strip and a right press on a tab open the tab menu, whose close row is
    the same close.
- **A chat id is handed out again** (#1101). `state.new_chat_id` claims the lowest free
  ordinal (`state.py:249-279`), and its own note says an ordinal *"is free the moment `reap`
  removes its directory"* (`state.py:2625-2627`). Two files keyed by the id live outside that
  directory and survive the reap:
  - `<id>.transcript`;
  - the entry in `reopen.json`.
  So a new `default.1` is offered an unrelated chat's scrollback. Closing it runs
  `_forget_transcript("default.1")` (`commands_frame.py:10538`, `:10556`), which deletes the
  earlier chat's record.
- **Only Claude Code has a session charter can name.** `hooks._record_harness_session` returns
  for every other harness (`hooks.py:6013-6015`), and `leave.resumable_harness` answers
  `claude-code` alone (`leave.py:409-426`). A Codex or opencode chat always reopens empty.

## Language

- **Exit gate** — the one menu that leaves charter. It has two rows: *Close charter (keep chats
  running)* and *Close charter and stop all chats…*. _Avoid:_ quit dialog, exit menu.
- **Ended tab** — a chat whose harness exited on its own, and whose tab stays and offers a
  choice. _Avoid:_ dead tab (the IDE spec's word, and "dead" is tmux's word for the pane).
- **Session link** — the one harness session a chat is tied to, as the harness names it.
  _Avoid:_ mapping, guess, latest session.
- **Chat id** — `<workspace prefix>.<n>`, handed out once for the life of the plane. CONTEXT.md
  already says it is allocated and never parsed; *never handed out again* is new.
- **Title** — optional words for people, drawn on the tab. It is never an identity. _Avoid:_
  name, which is what Claude Code calls the session name charter composes from the title and
  the id.

## The decisions

Each is the operator's confirmed answer, with its reason and the evidence it rests on.

1. **Protect the chat, not the key (Q1).** Ctrl+C stays. In Claude Code one press cancels the
   turn and the second exits, and disabling it would take the interrupt away. What changes is
   that no harness exit destroys a chat. *Evidence:* ADR 0018's consequence that *"charter
   never gets between the harness and the terminal it is actually talking to"* (0018:96-98).
   The operator's first ask, force-disabling Ctrl+C, is therefore **not** built.
2. **"Close charter" detaches THIS terminal only (Q2).** Every chat keeps running, and
   `charter` in the project reattaches. It is exactly what closing the terminal does today
   (`docs/frame.md:1153-1160`).
3. **"Close charter and stop all chats" records, then stops, every chat in this plane (Q3)**,
   after a confirmation that lists them. It is today's `F2 → charter: quit` (`cmd_quit`,
   `commands_frame.py:10240`), and `charter reopen` restores.
4. **A harness that ends keeps its tab (Q4, Q11, Q19).** This amends ADR 0018. Today a harness
   exit is final: *"A pane whose harness later EXITS closes as it always did and never comes
   back to the selector"* (0018:333-337).
   - **Clean exit** (`/exit`, double Ctrl+C, Ctrl+D) → back to the profile selector, with
     **resume &lt;session name&gt;** preselected, then start fresh, then close tab (Esc).
   - **Crash** (non-zero exit) → the harness's last lines stay on screen, with the same three
     choices.
   - **No conversation yet** → only start fresh and close.
   - **Background or handed-off chats** keep their tab too, marked ended in the strip.
5. **Closing a tab is the one way to end a chat, and it ends it for good (Q5, Q20)** — today's
   `chat: close`. It confirms for a tab whose harness is running. An ended tab closes without
   asking.
6. **The gate lives on `F10` everywhere, plus a button at the right end of the identity row
   wherever that row is placed (Q6).** The bars and the mouse are off by default
   (`docs/frame.md:774`, `[frame] mouse`), so the key is the primary route. Both open one menu.
   *Close charter (keep chats running)* is preselected. *Close charter and stop all chats…*
   leads to the confirmation. The house rule applies: *"A pointer opens the question; the
   keyboard answers it"* (`docs/frame.md:944-948`). `F10` joins the reserved keys in
   `instance.component_arrangement` (`instance.py:2561-2562`).
7. **Inside the operator's own tmux, no key is bound (Q7).** Charter binds nothing there today
   (`_launch_in_operator_tmux`'s docstring, `commands_frame.py:3445-3453`), so the gate is
   reachable only as `F2` rows.
8. **Prerequisite bug #1097.** `F2 → detach` runs `detach-client -s <chat id>`
   (`builtin_actions._detach`, `:116-124`), but the session is named after the workspace. On
   tmux 3.7c `has-session -t default.1` answers `can't find pane: 1`. The gate's default row
   IS this detach, so it has to resolve the chat's real session: `commands_frame._pane_place`
   answers the session id `$N` (`:7949-7995`).
9. **Chat ids are never handed out again within a plane (Q8, Q15).** `workspace.N` only grows,
   and the highest N is remembered on disk. Existing chats keep their ids, and the counter
   starts above the highest number any trace still carries. A restored chat keeps its id.
   *Why:* #1101. A leftover keyed by an id can be inherited only if the id comes back.
10. **Every tab is linked to exactly one harness session, never guessed (Q9, Q13, Q14).**
    - **Claude Code:** charter chooses the UUID and launches with `--session-id <uuid>
      --name <name>`, both verified in `claude --help`, so the link exists before the
      harness starts.
    - **Codex and opencode:** charter records the id each one reports.
      - The Codex hook payload carries `session_id` (`codex.py:9-16`).
      - opencode's plugin sets `CHARTER_SESSION_ID` from its `sessionID` (`opencode.py:307-309`).
      - Neither is recorded today.
      - They resume by id: `codex resume <id>`, `opencode -s <id>`.
    - **Resume is offered only when the conversation actually exists** (*The session link*,
      below, says how that is told).
11. **Optional tab titles (Q10, Q16, Q17, Q18).** The id stays immutable for linking, and the
    title is for people.
    - **Set from:** a rename row in the tab menu and in `F2` (a one-line input), an optional
      title at `+`, or the first line of a handoff's brief.
    - **Shown in:** the chat strip.
    - **Claude Code's session name** is `<title> · <id>`, or `<id>` when the chat is untitled.
    - **A rename** updates the tab at once and reaches Claude Code at its next start or resume
      (`--name` again). Charter never types `/rename` into a harness (ADR 0018).
    - *Today:* no title and no rename (`tabmenu.py:36-42`); the tab is the bare id
      (`slots._chats_strip`, `:4577-4603`).
12. **A quit records ended-but-open tabs (Q12)**, with their session link, so reopen brings them
    back ended and resumable.

## Chat ids that are never handed out again

**The counter.** `state.new_chat_id(workspace)` keeps its signature and its `mkdir` claim. What
changes is where it starts. It starts at `max(mark, traces) + 1`:
- **mark** is the highest ordinal this plane has ever handed out for the id's prefix;
- **traces** is the highest ordinal any leftover on disk still carries.

It claims upward from there with the same `config.claim_private_dir`. After a claim it raises
the mark.

- **The `mkdir` is still what decides who wins.** Two allocators may compute the same start,
  and one of them then gets `FileExistsError` and moves on, which is today's rule. Its own
  docstring says *"a scan is not a substitute"* (`state.py:223-226`): here the scan only
  chooses where to start, never who owns a name.
- **The mark only goes up, under a lock.** The read-max-write runs under `fcntl.flock` on a lock
  file beside the mark (the idiom `hooks.py:8058` already uses). Two racers that claimed N+1
  and N+2 must not leave N+1 recorded.
- **Keyed by the id's prefix, not the workspace name.** The id is
  `state.workspace_prefix(ws)`, and that function maps every character outside
  `[A-Za-z0-9_-]` to `_` (`state.py:79-102`). The workspace alphabet allows a dot, so `a.b` and
  `a_b` share the prefix `a_b`. Two marks keyed by name would each hand out `a_b.3`.
- **`_CHAT_ORDINAL_MAX` bounds the loop from the start, not the ordinal.** Today it caps how
  high allocation may count (`state.py:132-141`), and a counter that never goes down would turn
  that cap into a lifetime limit per workspace.

**Where it lives.** One file per plane, `.charter/frame/chat-ids.json`, maps each prefix to its
highest ordinal. Its lock is `.charter/frame/chat-ids.lock`. It is written with
`config.replace_for` under the lock.
- **A file, never a directory.** Every scan of the frame root reads a directory as a chat:
  - `leave.plane_chats` (`leave.py:227-231`);
  - `chats._by_workspace` (`chats.py:261-337`);
  - `state.reap` (`state.py:2790`).
  A directory there would be listed as a tab, and removed by a reap of the legacy server. Like
  `reopen.json` (`reopen.py:53-57`) and `<id>.transcript`, the file is skipped by each of them,
  and `reopen.prune_transcripts` touches only `*.transcript`.
- **Scratch that survives being lost.** The mark lives under `NO_FORMAT_PROMISE`
  (`state.py:181-186`), which allows any release to change its shape. Losing the file costs
  nothing, because the traces are scanned on every allocation, not only the first. A lost mark
  falls back to the highest ordinal a leftover still carries. An id that leaves no leftover has
  nothing to hand down.

**The traces**, which are the migration and scanned every time:
1. chat directories `<prefix>.<n>/` in the frame root;
2. `<prefix>.<n>.transcript` files beside them (`reopen.TRANSCRIPT_SUFFIX`);
3. every `chat` field in `reopen.json`, read raw. `reopen.read` returns `None` for a version it
   does not speak (`reopen.py:324-325`), and that file still names ids a transcript carries;
4. `.charter/sessions/<prefix>.<n>.*` markers. They are removed only when a reap removes the
   directory (`state._forget_session`, `:2622-2683`), so a marker with no directory beside it
   is an id that still has something to hand down.

Items 1–3 are the decision's list, and 4 is the same rule applied to the one other place an
id-keyed file lives.

**A restored chat keeps its id.** `_reopen_one` hands `Reopening` the recorded id, and `_launch`
claims exactly that directory (`state.claim_chat_id`) instead of allocating one. When the claim
fails, the directory survived the launch's own reap. That means the directory is another
server's, a claim still in progress, or a live window. The chat is skipped with a sentence and
stays in the manifest for a retry (`_consume`). Nothing adopts a directory a live process may
own. With the id unchanged, `_restore_recorded_chat`'s transcript rename is a no-op, which its
equality test already allows (`commands_frame.py:10798-10801`).

## The session link

**One link per chat, in the chat's own directory, and carried in the manifest.** The files that
exist today keep their meaning:
- `session`: the gauge's mapping, which `clear_shape` deletes;
- `session.durable`: the link.

One file is added:
- **`conversation`** — the path the harness itself named for this chat's conversation. It is
  only ever `stat`ed, and never opened, shown or passed to a command.

`reopen.Chat` gains `conversation: str = ""`, defaulted like `profile` and `brief` so a manifest
from 0.62.0 still reads (`reopen._chat`).

| Harness | Who chooses the id | Recorded when | Resumes with | The conversation exists when |
|---|---|---|---|---|
| Claude Code | charter: `--session-id <uuid> --name <name>` on every fresh start | by the launcher just before the `exec`; the SessionStart payload's `session_id` confirms it | `--resume <uuid> --name <name>` | the `transcript_path` a hook payload named for this chat is a file |
| Codex | Codex | at the chat's SessionStart hook: `session_id`, `transcript_path` | `resume <id>` | the `transcript_path` it named is a file |
| opencode | opencode | at the chat's first tool hook: `input.sessionID` | `-s <id>` | a tool hook has reported the id — opencode names no transcript file (`opencode.py:318-326` builds the payload without one) |

- **Charter chooses only where the harness lets it.** A UUID is minted with `uuid.uuid4()` by
  the launcher at the `exec`, so every route that starts a harness gets one: the CLI, `+`, a
  tab, a reopen, a handoff, and a pick in an ended tab. `launcher.attempt`'s `on_exec` writes it
  to `session.durable` before `os.execvpe`.
- **The id and the name are added by the launcher at the `exec` and never cross tmux.** They
  ride no `-e` and no launcher argv. A title is free text, and tmux's argument parser has
  already cost this repo #957 and #961. The launcher reads the chat's own record, which it may
  do only after `framed_chat()` has proved the pane (`launcher.py:520-550`). An unframed launch
  has no chat and gets no link.
- **The operator's own session flag wins.** A launch whose arguments already carry the
  harness's session flag gets nothing added. For Claude Code those are `--session-id`,
  `--resume`, `-r`, `--continue`, `-c` and `--fork-session`. The link is then what the harness
  reports at SessionStart.
- **What the harness reports replaces what charter chose.** If a resume reports a different
  `session_id` than the one charter passed, the report is recorded. That is the harness naming
  its own session, which is not a guess. The launcher never infers an id from a directory, a
  time or "the latest session".
- **Asked of the registry, not spelled per module.** `Harness` gains the members below. Today a
  resume member is refused: `leave.resumable_harness` (`:419-423`) and `_reopen_one`
  (`commands_frame.py:11193-11197`) point at `harness/base.py`'s bar, *"`launch_argv` is
  `[self.binary, *extra]` with no subclass override anywhere in the registry, so the
  pass-through **is** the seam"*. That bar is now met, and for `first_message_argv`'s own reason:
  three harnesses need three spellings, so no single `extra` is right for all of them.
  - `chooses_session_id`
  - `names_its_transcript`
  - `session_flags`
  - `new_session_argv(sid, name)`
  - `resume_argv(sid, name)`
  - `reports_session_at`
- **Held to a shape before it is used.** A session id matches
  `^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$`. The id reaches a harness argv, and one starting with `-`
  would be read as a flag. A `transcript_path` must be absolute, carry no NUL, and fit
  `contain.PATH_DISPLAY_LIMIT`. Anything else records nothing. Both arrive in a hook payload,
  which a chat's own shell can write (*Limits*).
- **"The conversation exists" is a `stat`, asked when a surface is about to offer resume.**
  Those surfaces are the ended selector, the crash drawer, the quit confirmation's note and
  `_reopen_one`. A transcript deleted since is no longer offered. Charter reads nothing inside
  the file.

**What reopen and quit say.** `leave._resume_clause` keeps its four sentences, which now depend
on the harness members instead of Claude Code alone:
- `NO_RESUME_HARNESS` (*"records no session id to resume from"*) is said only for a harness
  whose `resume_argv` is `None`. No shipped harness is one after task 1.
- `NO_RESUME_YET` covers a link with no conversation yet.

## Titles

- **Stored** in `.charter/frame/<id>/title`, one line, through `state.record_title`. It is
  carried in the manifest as `title: str = ""`.
- **Contained on the way in.** A title goes through `contain.one_line` and is at most
  `TITLE_MAX` = 60 characters.
  - The rename surface refuses a longer one and says how long it was.
  - A brief's first line is cut at 60 with `contain.readable`'s marker. The brief is text a
    model wrote, and the operator approved its whole message at the harness prompt (ADR 0021),
    not a tab label.
  - Every drawn title goes through `contain.readable` (ruling 35), because `title` is a file a
    chat can write.
- **Drawn** in the chat strip instead of the id, and appended to the id wherever a chat is
  named for a decision: the quit and close rows (`leave.title`), the tab menu's label, and the
  chat picker. The strip's cell map (`slots._Tabs`) stays keyed by the id. A click resolves to
  the chat, never to its words.
- **Claude Code's session name** is `f"{title} · {id}"`, or `id`. The launcher composes it at
  every `exec`.
- **A rename** writes the file, bumps every strip on the plane (`_repaint_the_other_strips`'
  shape), and does nothing else. Claude Code sees the new name at its next start or resume.
  Codex and opencode never see it.

## The four surfaces

### The gate

- **`F10`, on charter's own server.** `conf_text` binds it beside the palette's hotkey and
  before the escape hatch, which stays the last line (`commands_frame.py:1018-1035`):
  `bind -n F10 run-shell '"$CHARTER_PY" -m charter frame-palette --gate --chat
  "#{@charter_chat}" --client "#{client_name}"'`. The action is a constant: tmux expands both
  formats at the keypress, the chat id from the window option and the client from whoever
  pressed.
- **The button** is `F10 close`, at the right end of the identity row, on whichever edge the
  arrangement places `identity`.
  - On a starved row it is dropped after the version and before the identity.
  - It is a door (`slots._Doors`) that opens the gate rather than the palette, so `_Doors`
    gains a second column set. Its docstring's argument against a mapping, *"whose values
    nothing branches on"*, stops holding once something branches on the value.
  - With `[frame] mouse` off, which is the default, it is a label that teaches the key.
- **The menu** is `frame/gate.py`, `tabmenu.py`'s shape: a `palette.Palette`, `own_the_tty`,
  full-pane and modal, with one key that always leaves.

  ```
  charter · close · 2 to choose from
  >   Close charter (keep chats running)
      Close charter and stop all chats…

    up/down move   enter choose   esc cancel   F12 back to the harness
  ```

  - **Close charter (keep chats running)** detaches the terminal that asked.
    - With a client name held to tmux's shape: `detach-client -t <client>`.
    - Without one, from the pointer route: the chat's session id through `_pane_place`, then
      `detach-client -s <$N>`. That detaches every client on that workspace's session, and the
      row's note says so (open question 3).
    - It never uses a chat id and never a session name. That is #1097.
  - **Close charter and stop all chats…** is a doorway. Enter replaces the surface with
    `leave.confirm_rows(leave.plan(...), verb=QUIT)`, which is today's confirmation as a
    drawer (`_as_a_drawer`). It lists every chat, ended tabs included (decision 12), each with
    what it gets back. The keypress on its first row runs `charter frame-quit`.
- **`F2` carries the same two rows, in the same words.**
  - `frame.detach`'s title becomes the first row's words, and its action is the fixed detach.
  - `leave.OPEN_QUIT` becomes the second row's words.
  - Their ids and positions stay: the detach row near the top, and the stop row last
    (`leave.open_rows`' guard, `leave.py:472-476`).
- **`F10` is reserved.**
  - A `[[frame.component]]` `key = "F10"` is refused the way `F12` is, through
    `instance.component_arrangement`'s `bound` set.
  - A `[frame] hotkey = "F10"` is refused, and the shipped `F2` binds instead.
- **Inside the operator's own tmux.**
  - No key is bound: not `F10`, not the hotkey, not `F12` (`docs/frame.md:1094-1105`).
  - The two rows are in the palette, which is reached there through a pointer door when their
    tmux mouse is on.
  - *Close charter* is listed refused with its reason, `builtin_actions._detachable`'s: the
    frame is a window in their session, and their own prefix key detaches.
  - *Close charter and stop all chats…* works as quit does there.

### An ended tab

**What ends a harness:** its process exits in a chat pane the launcher `exec`'d, for any reason.
That includes `/exit`, a double Ctrl+C, Ctrl+D, a crash, and a signal from outside.

**On charter's own server.**
1. `pane-died[0]` writes the exit code, unchanged (`_pane_died_write_hook_argv`).
2. `pane-died[1]` is no longer `kill-window`. It is the constant action
   `run-shell -b '"$CHARTER_PY" -m charter frame-ended --chat "#{@charter_chat}"'`,
   installed after `[0]`. The order is still the whole fix (`:1444-1456`): an unindexed
   `set-hook` replaces the array.
3. `charter frame-ended` (`frame/ended.py`) acts only when all of these hold, each asked of
   tmux or of the chat's own record, never of the environment:
   - the window carrying `@charter_chat` = the chat has a pane tmux lists as dead;
   - the chat is marked **drawn** — the launch that opened it finished laying it out;
   - it is not already ended.

   Then it writes `ended` and bumps every strip. What it presents depends on the code:
   - **Exit code 0 — a clean exit.** It respawns the harness pane:
     `respawn-pane -k -t <pane> <frame-launch --select --ended --start <profile> --attended>`.
     The pane is charter's selector again, and the pane id, the window, `@charter_chat` and
     the pane's hooks stay.
   - **Anything else — a crash, a signal, or an empty status read as `_UNKNOWN_DEATH_CODE`.**
     The dead pane is left exactly as tmux keeps it (`remain-on-exit`), so the harness's last
     lines and tmux's own `Pane is dead (status N)` stay on screen. The choice opens in a
     drawer split beneath it: a pane running `frame-palette --ended`, the confirmation
     drawer's shape (#921, #927).
4. **The choices are the same in both.**
   - **resume &lt;session name&gt;** — first and preselected, and present only when the
     conversation exists. It respawns the pane into the launcher with `--resume`.
   - **start fresh** — in the selector, any profile row starts on a new link, with the cursor
     on the chat's own profile when resume is absent. In the drawer, this row respawns the
     pane into the selector.
   - **close tab** — Esc in the selector, or a row in the drawer. It is `chat: close` without
     a confirmation (decision 5).

   The resume row's title is the session name, which is `<title> · <id>`, or `<id>`. Its note
   is `<kind> · session <first 8 of the link>`.
5. **A pick clears the ended state**: `ended`, `exit`, and the drawer's pane. A fresh start also
   drops `conversation` and writes the new link.

**The launch's own early-death path is unchanged** (#384). A harness dead before the chat is
drawn is still reported and its window killed by `_launch`. `frame-ended` does nothing for a chat
that is not marked drawn. After `_launch` writes the drawn mark, it asks `_query_pane_dead_status`
once more and runs the ended step itself if the pane died in between. Otherwise an exit landing
in that gap would leave a dead pane nothing acts on.

**The teardown-hook refusal stays**, reworded. `_launch` still refuses to attach when `[1]` did not
install. The reason is no longer an `attach` blocked forever. It is a tab whose exit nothing would
answer (`:6362-6374`).

**`charter claude` returns when the tab closes or you detach, not when the harness exits.**
`attach` returns when its session's last window goes. The launch then returns the exit code the
chat recorded (`state.exit_code`), else 0, the same rule it has today, arriving later. A pipe and
`--no-frame` bypass the frame and keep the harness's own code (`docs/frame.md:1394-1401`).

**Inside the operator's own tmux** there are no hooks (`_launch_in_operator_tmux`'s docstring,
`:3495-3505`), and the launcher is awake for the life of the frame. When `_wait_for_harness` sees a
drawn chat's harness die, the launcher runs the same ended step in-process and goes back to
waiting. It returns when the window is gone. A harness that dies before the window is drawn keeps
today's path.

**Background and handed-off chats** end through the same hook, whether or not anybody is looking.
The strip marks the tab `x`, the ASCII rule `slots._BAR_RULE` states, in the cell the working
spinner uses, and draws the name dim. The tab switches as any tab does. The choice is waiting
there.

**A reopened ended tab** (below) comes back as a window whose pane runs the ended selector. It is
the one selector an open nobody is at may reach. That does not contradict `argv_select`'s rule
that *"a selector is a question, so the only open that may reach one is an open somebody is in
front of"* (`launcher.py:207-221`): nothing starts until somebody switches to it and presses
Enter.

### Closing a tab

- **Closing is still `cmd_close`**: the mark, dropping the transcript and the manifest entry,
  handing the client another chat, and killing the window (`commands_frame.py:10491-10564`). It
  now also:
  - drops `conversation`, `title` and the drawer pane;
  - leaves the harness's own conversation store untouched — charter deletes nothing a harness
    wrote.
- **It confirms only when a harness is running.** `leave.needs_confirming(chat)` is true when the
  chat is neither ended nor still waiting at the selector.
  - The tab menu's close row (`tabmenu.catalogue`) and `F2 → chat: close` (`leave.open_rows`) are
    doorways only then.
  - On an ended tab the row is an action: `frame-close <id>` on Enter, and its title says the
    harness has ended.
  - `-` still opens the tab menu. It does not close on the press.
- **Esc in an ended selector closes the tab for good.** It runs `cmd_close` in-process, not
  `_close_the_cancelled_chat`. That function closes a pane that never became a chat and must not
  write the closed mark (`launcher.py:656-704`). An ended tab was a chat, and forgetting it is
  what close means.

### Quit, and reopening ended tabs

- **Quit records every open tab, ended ones included.** `leave.plan` already keeps an ended chat,
  which is neither closed nor waiting.
  - `leave.Doomed` and `reopen.Chat` gain `ended: bool = False`.
  - The recorder (`record_the_plane_now`) writes the same field, so a machine that goes down
    keeps ended tabs too.
- **The confirmation's note for an ended chat** replaces the old `already ended on its own (N)`
  clause (`leave._ended`) with `ended — comes back ended; resume offered` or `ended — comes back
  ended; nothing to resume`.
- **`charter reopen` brings an ended chat back ended.** `_reopen_one` keeps its id (above) and
  writes its link, conversation, title and profile into the claimed directory before tmux. It
  launches the pane at `frame-launch --select --ended --start <profile>` and starts no harness.
  Every other chat reopens as today. Its conversation resumes when the link and the conversation
  exist, for every harness whose `resume_argv` answers: `frame-launch --profile <p> --resume`.
  `rest` no longer carries `--resume`.

## ADR 0018, amended — draft for task 2's PR

> **Amendment, 2026-09-15: a harness exit is no longer final — the pane that emptied offers a
> choice.**
>
> The 2026-09-12 selector amendment ended on *"A pane whose harness later EXITS closes as it
> always did and never comes back to the selector — going back would be charter drawing in a
> pane a harness has run in"* (0018:333-337). The operator ruled otherwise: no harness exit
> destroys a chat. Ctrl+C stays exactly as each harness defines it, because a chat is protected
> by what charter does after an exit and not by taking a key away — the consequence at 0018:96-98.
>
> **The distinguishing question moves from the past tense to the present.** It was *has a harness
> ever run in this pane, and is charter's own process still the one in it?* It is now: **is a
> harness process in this pane right now?** No, and the pane is charter's again — before the
> first `exec`, and after any exit. Yes, and the rule is unqualified: charter draws nothing there
> and reads it only at the two moments of the 2026-09-01 amendment. It is checkable from outside:
> tmux lists the pane `#{pane_dead}` before charter touches it, and `state` records `ended`
> before any respawn.
>
> **What charter puts there after an exit, and nothing else.**
> - A clean exit respawns the pane into the profile selector, a `frame/overlay.py` surface
>   bounded exactly as the selector already is, with one extra row: resume.
> - A crash leaves the dead pane untouched — tmux keeps the harness's last lines on its own
>   screen — and the choice opens in a drawer pane split beside it. Charter does not draw in the
>   dead pane at all.
>
> **Bounded, and each bound is checkable.**
> - **Charter restarts nothing by itself.** Every harness start after an exit is an operator's
>   Enter on a row. No timer, no retry, no automatic resume, and an ended tab nobody switches to
>   stays ended.
> - **Charter never types into a harness.** No `send-keys`, no `/rename`, no `/resume`. A resume is
>   an argument at the `exec` (`--resume`, `resume`, `-s`), and a title reaches Claude Code as
>   `--name` at the next `exec`.
> - **Charter reads nothing to decide.** The presentation is chosen from the exit code the
>   `pane-died[0]` hook wrote and from the link charter recorded, never from what the harness
>   printed. The last lines stay because charter leaves that pane alone, not because it reads
>   them.
> - **The launch's own early death is unchanged.** A harness dead before its chat was drawn is
>   reported and its window closed, as #384 requires.
>
> *Recorded in the pull request whose code first relies on it (ruling 44).*

## ADR 0024 — draft for task 1's PR

`docs/adr/0024-a-chat-id-names-one-chat-and-one-harness-session.md`. The number is the next free
one at `5ad755d`, and the test pins the slug, not the number, as the profile ADR's test does.

> # A chat id names one chat for the life of the plane, and one harness session
>
> **The failure.** `state.new_chat_id` handed out the lowest free ordinal, and a reap freed an
> ordinal with the directory. Two things keyed by the id live outside that directory — the
> captured transcript and the quit record — so a new `default.1` was offered an unrelated chat's
> scrollback, and closing it deleted the earlier chat's record (#1101). Separately, charter knew
> which conversation a chat held only for Claude Code, only after its first hook, and lost it at
> the reap.
>
> **The decision.**
> 1. **A chat id is handed out once per plane.** The allocator starts above the highest ordinal
>    this plane has handed out for the prefix, and above every ordinal a leftover still carries;
>    the `mkdir` still decides who wins. A restored chat keeps its id.
> 2. **A tab is linked to exactly one harness session, never guessed.** Charter chooses the id
>    where the harness takes one (Claude Code's `--session-id`), and records the id the harness
>    reports where it does not (Codex at SessionStart, opencode at its first tool hook). A report
>    replaces a choice; nothing else does.
> 3. **Resume is offered only when the conversation exists** — the transcript the harness named
>    is a file, or, for opencode, which names none, a hook has reported the session.
> 4. **A title is display; the id is identity.** Claude Code's session name is composed from both
>    at every `exec`.
>
> **Considered and rejected.**
> - *The lowest free ordinal, with every leftover deleted at the reap* — a leftover charter does
>   not yet know about is inherited the day it is written; never reusing the name needs no list.
> - *A hash id* — `new_chat_id`'s own docstring: a truncated hash collides silently.
> - *Ids renamed to titles* — every `$CHARTER_SESSION_ID` already exported into a live process
>   would name nothing (`state.py:235-241`).
> - *`--continue`, or "the newest session in this directory"* — two chats of one workspace share
>   a directory, so that is the guess this ADR exists to refuse.
> - *Typing `/rename` or `/resume` into the harness* — ADR 0018.
> - *`codex resume <name>`* — Codex has no flag to set a name (openai/codex#14482), so a name
>   charter did not set cannot identify a session.
>
> **Consequences, including what they cost.**
> - One more file and a lock under `.charter/frame/`, and ordinals only grow: past 99,999 an id
>   sorts last in a strip (`chats._MAX_ORDINAL_DIGITS`).
> - The link is as trustworthy as the hook that writes it: a chat's own shell can run
>   `charter hook sessionstart` with a forged payload and point its own tab at another
>   conversation — which was already true of Claude Code's record.
> - An opencode conversation that never ran a tool reports no session and is not offered resume.
> - A rename reaches Claude Code only at its next start or resume, and Codex and opencode keep
>   their own session names.

## The bugs this closes

- **#1097** — `F2 → detach` names a chat id as the tmux session. *Task 4:* `_detach` resolves
  the session through `_pane_place` or names the pressing client, never a name. It gets a
  real-tmux test that attaches a client and sees it go, which the issue asks for.
- **#1101** — reused chat ids inherit transcripts and quit records. *Task 1:* ids are never handed
  out again. A test closes `default.1` with a transcript and a manifest entry, opens a chat, and
  asserts the new chat is not `default.1`, is not offered that transcript, and that closing it
  leaves `default.1`'s record.

## What this changes elsewhere

- **The IDE spec's §4j becomes true, and §5's open question is answered.** Both get a dated note,
  not a deletion.
- **CONTEXT.md.**
  - The **Chat** entry gains *handed out once* and *linked to one harness session*.
  - **Ended tab** and **Exit gate** are added.
  - **Title** is added, avoiding *name*.
- **`docs/frame.md`** changes in these sections:
  - *Leaving* — the gate first, then close, quit and reopen;
  - *Exit codes*;
  - *Inside a tmux you already have* — its first paragraph says the window closes when the
    harness exits, and it no longer does;
  - *How a harness starts* — the ADR sentence;
  - the selector section;
  - `+`, tabs and the identity row's buttons (`:366-372`).
- **`docs/harnesses.md`** *What each harness lets charter offer* gains a resume row per harness.
  The session-name limit goes beside it.
- **`hooks._record_harness_session`** is gated on the registry (`reports_session_at ==
  "sessionstart"`), not on `claude-code`. An opencode route records from the tool hook. Both
  record `conversation` when the payload names a path.
- **`leave.resumable_harness`** asks the registry's `resume_argv`. The docstrings that argue
  against a resume member are rewritten, not left contradicting the code.
- **Docstrings that say an ordinal is recycled** are rewritten in task 1:
  - `state.new_chat_id`, `_forget_session`, `clear_exit`, `clear_shape` and `clear_respawn`;
  - `leave._group`;
  - `reopen.Chat.chat` and `reopen.Chat.workspace`;
  - `_restore_recorded_chat`, and `_launch`'s notes at `:6505-6509`.

## Limits

- **Ctrl+C is not disabled.** The operator's first ask is not built (decision 1). A double Ctrl+C
  still ends Claude Code. What changes is that the tab stays and offers resume.
- **A rename reaches Claude Code only at its next start or resume.** Codex and opencode keep the
  session names they chose. Neither has a flag charter could pass (openai/codex#14482 is open,
  and opencode's `--title` exists only on `opencode run`), and charter never types `/rename`.
- **`F10` is taken from every pane on charter's own server, the harness's included.** A root-table
  bind is server-wide (`conf_text`'s docstring), so a harness or a program run in its pane that
  wants F10 no longer gets it there.
- **The button is unclickable on most planes.** `[frame] mouse` ships off, so the key is the gate.
- **Inside the operator's own tmux there is no gate key and no palette key.** The rows are reached
  through a pointer door only when their tmux mouse is on. *Close charter* is refused there,
  because their prefix key detaches.
- **The session link is as trustworthy as the hook that writes it.** A chat's own shell can run
  `charter hook sessionstart` with a forged payload and point its own tab's resume at another
  conversation. It needs no more than today's Claude Code record already accepts.
- **An opencode conversation that never ran a tool is not offered resume.** opencode has no
  session-start event (`docs/frame.md:1252-1255`) and names no transcript.
- **Resume is a `stat` at the moment of offering.** A transcript removed afterwards gets the
  harness's own error at resume.
- **Ended tabs stay until somebody closes them.** The IDE spec's *"the open set only ever grows"*
  (§5) now covers ended chats too.
- **A terminal left attached shows the choice, not the shell.** `charter claude` returns when you
  detach or close the tab.
- **Ids only grow.** Past 99,999 in one workspace, a tab sorts last.

## Open questions

Each has the evidence that makes it open and a recommendation. None changes a decision.

1. **opencode's chat id on the hook path is overwritten (#946).**
   - *Evidence:* the shim sets `CHARTER_SESSION_ID` to opencode's own `sessionID` in every hook
     subprocess (`opencode.py:332`, `:379`), and `hooks._chat_id` reads that variable
     (`hooks.py:5921-5937`). So inside an opencode chat a tool hook cannot say which chat it is
     in (`hooks.py:7205-7210` states the defect).
   - *Options:*
     - **(A)** resolve the chat from `$TMUX_PANE` against the pane charter recorded
       (`state.harness_pane`) on the server `$TMUX` names. That is the evidence
       `state.is_live` already stands on (`state.py:2594-2603`). The shim's bytes do not move.
       It costs a scan of at most 30 chat directories per tool call until the link matches.
     - **(B)** the shim passes the chat id it inherited at load in a variable of its own. That
       moves the shim's bytes, so `opencode.shim_is_charters` fails until a launch rewires it
       (ADR 0022's 2026-09-15 amendment), and it needs `adopt: reinit`.
   - *Recommendation:* A, and #946 stays its own issue.
2. **Five harness facts that decision 10 rests on are recorded but unmeasured.**
   - *Evidence:* the flags exist (`claude --help`, `codex resume [SESSION_ID]`, `opencode -s`,
     the decisions file); the payload fields exist (`codex.py:9-16` for Claude Code and Codex,
     `opencode.py:318-326` for opencode). What is not measured:
     1. when Claude Code first creates the file its `transcript_path` names: at SessionStart,
        or at the first prompt;
     2. whether `claude --resume <uuid>` reports the same `session_id` back;
     3. whether `--resume` and `--name` combine;
     4. whether Codex's SessionStart `session_id` is the id `codex resume` takes;
     5. whether opencode's `input.sessionID` is the id `opencode -s` takes.
   - *Resolution:* task 1's Step 0 measures all five on throwaway config folders. A harness whose
     reading fails is offered no resume, and the failure is written down. Nothing is guessed.
3. **Which terminal *Close charter* detaches when the button is clicked.**
   - *Evidence:* `F10`'s bind can carry `#{client_name}`, which tmux expands for the presser. The
     hotkey bind carried it until #729 (`commands_frame.py:855-876`). A click reaches a panel
     process that has no client (`builtins._strip_events` spawns the palette detached).
   - *Recommendation:* the pointer route asks tmux for `#{client_name}` with the chat's session
     as target and uses it only if task 4's Step 0 measures that answer to be the clicking client
     with two clients attached. Otherwise the row detaches every client of that workspace's
     session and says so in its note.
4. **`pane-died` on Linux for a pane whose status and signal both come back empty.**
   - *Evidence:* a cancelled selector pane on the CI runner was marked dead with both empty and
     its window was still listed a minute later, with both hooks present
     (`launcher._close_the_cancelled_chat`'s docstring, `:660-669`). Whether a harness killed
     from outside can reach that state is unmeasured.
   - *Resolution:* task 2's real-tmux module runs a signal death on CI's runner. If the hook does
     not fire, the controller rules on a fallback before task 2 merges. The candidate is the
     chat strip's panel running `frame-ended` for a chat whose pane its listing reports dead,
     which reads `#{pane_dead}`, a state and not content. The task does not invent one.
5. **Scope: `charter frame -- <cmd>`.**
   - *Evidence:* the escape hatch runs no profile, records no link, and promises a script its
     command's exit code (`docs/frame.md:1394-1401`, `:1511-1515`). Decision 4 speaks of *a
     harness*.
   - *Recommendation:* the escape hatch keeps today's ending (the window closes at exit and the
     code is returned), because it has nothing to resume and no profile to start fresh on. The
     operator confirms.

## Done when

On this plane, with the operator, after task 4 is on `main`:

- One workspace holds three chats, started from the selector on `claude`, `codex` and `opencode`.
- In the Claude Code chat, `/exit` leaves the tab, and the selector opens on
  **resume &lt;title · id&gt;**. Enter brings the conversation back, and
  `ps -o command= -p $PPID` inside it shows `--resume <uuid> --name`.
- A crash is `kill -9` of the Codex chat's harness from another terminal. The last lines stay,
  the drawer offers resume, and resume brings the conversation back through `codex resume <id>`.
- `F10` → *Close charter (keep chats running)* detaches only the terminal that pressed it, while
  a second terminal attached to the same workspace stays.
- `F10` → *Close charter and stop all chats…* lists all three, and the ended one says so.
  `charter reopen` brings back every chat under its old id, and the ended one comes back ended.
- Close one chat, open a new one, and its id is higher than every id before it.
  `F2 → chat: previous transcript` is refused on it.
- Rename a tab. The strip shows the title at once, and the next Claude Code resume shows it in
  `/resume`'s list.

## Build order

1. **Never-reused ids and the session link** (decisions 9–10; closes #1101; ADR 0024).
2. **An ended harness keeps its tab** (decisions 4, 5, 12; the ADR 0018 amendment).
3. **Titles and rename** (decision 11).
4. **The exit gate** (decisions 2, 3, 6, 7; the #1097 fix).

**Order: 1 → 2 → 3 → 4, one at a time, each its own reviewed PR.**
- Task 2's resume row and reopen need task 1's link and kept ids.
- Task 3 composes the name task 1's launcher passes and draws it on task 2's resume row.
- Task 4's confirmation lists task 2's ended tabs.

Each task moves the docs for what it changes in its own PR, ships its own news entry, and ships
the ADR text its code first relies on (ruling 44).
