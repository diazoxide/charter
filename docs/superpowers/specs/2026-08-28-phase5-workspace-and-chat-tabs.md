# Phase 5 — workspace tabs, chat tabs, and many harnesses in parallel

**Date:** 2026-08-28 · **Status:** specified, unimplemented
**Settles:** `docs/superpowers/specs/2026-08-25-agentic-ide-foundation.md` §4j and §6's Phase 5
**Overrides §4j where they disagree**, and they disagree in three places — each one because a
measurement on this machine said so. The three are marked **OVERRIDES §4j** in the text.

---

## 1. What this is, in the operator's own words

> I have an idea — I want to make our IDEA have a workspace selector, user can switch between
> workspaces but not lose open sessions. User opening a workspace — and in one workspace he can
> add chat sessions (tabs). We can use session name — e.g. claude has a session name, we can on
> creating a new session make some deterministic name e.g. `{workspace}-{some-hash}`. And user
> can on starting a session select harness. So in one IDEA session user can use many harnesses
> in parallel.

Recorded as §4j of the foundation spec, settled 2026-08-26.

### The sentence that makes the rest coherent

**Charter is not adding tabs. It is adding many chats. The tab bar is a readout.**

That distinction is load-bearing and it comes out of a measurement (§7.9): a 200-column row
holds about twelve chat names at charter's own minimum name width of 12 cells
(`slots._NAME_MIN_W`); an 80-column row holds four. A design in which the bar *is* the
mechanism breaks at 80 columns. A design in which the bar is a readout and the **palette** is
the mechanism works at every width, because Phase 2 already reaches every action in ≤2
keystrokes and does not care how wide the window is.

So: the chat bar and the workspace bar are two new components in the Phase 1 registry, they
join the existing degradation ladder, and when they cannot be drawn nothing is lost but the
reminder.

---

## 2. What is already true, and what is not

Facts checked in the tree at `27da88e`, not remembered.

**Already exists and is reused unchanged.**

- `state.frame_id(workspace, pid)` mints `{sanitised workspace}-{pid}`; `_UNSAFE =
  re.compile(r"[^A-Za-z0-9._-]")` is the alphabet.
- `state.record_harness_session(fid, sid)` / `state.harness_session(fid)` — charter already
  records which harness session belongs to which frame, in `.charter/frame/<fid>/session`, at
  mode 0600. The only process holding both ids at once is `statusline.py:2816`.
- `.charter/frame/<fid>/` already holds twelve files per frame: `version`, `exit`, `harness`,
  `session`, `server`, `workspace`, `density`, `chrome`, `hidden`, `identity`, `panes`,
  `respawn/`.
- `frame/choose.py` already renders a picker as rows over one `overlay.Surface`, with
  `OPEN_ID = "pick:{}"` and `NAME_ID = "{}:n{}"`.
- `palette.own_the_tty(surface, then=…)` is already an N-surface mechanism in one pane. The
  two-level tree is charter's restraint, not a structural limit.
- `_split_panels(...)` is the one funnel every panel pane comes out of, and
  `_apply_arrangement(fid, *, where, want, window=None)` is the one live pane-mutation path.
- `tmuxctl.chain(argvs)` collapses many tmux commands into one invocation. §7.7 shows this is
  worth 3.3× on the switch path.
- **The guest path already treats a frame as a window.** `_launch_in_operator_tmux` creates a
  `new-window -n <fid>`, and `_live_windows` reads liveness from `list-windows -a -F
  '#{window_name}'`. Phase 5 makes the private path work the way the guest path already does.

**Does not exist.**

- `grep -rn "tab" charter/frame/` is empty. There is no tab concept anywhere.
- `switch-client` is not used anywhere in the tree.
- `new-window` is used on the guest path only; nothing ever creates a second window on
  charter's private server.
- The `Harness` interface has `name`, `deficits`, `cli_name`, `binary`, `launch_argv(extra)`,
  `detect()` and the wiring/rule methods. There is **no session flag, no resume flag, no
  session-id flag, no cwd field and no auth field** anywhere under `charter/harness/`.

---

## 3. The seven questions, decided

### 3.1 What a chat tab is, concretely

**A chat is a charter frame. A frame is a tmux window. The window is where the chat is drawn;
the frame directory is what the chat is.**

Precisely: a chat is a directory `.charter/frame/<chat id>/` whose panes live in one tmux
window on the workspace's tmux session. Identity lives on disk. The window is a rendering.

This changes two words in charter's vocabulary and nothing else:

| today | Phase 5 |
|---|---|
| a frame is a tmux **session** on `-L charter` | a frame is a tmux **window** |
| a tmux session is a frame | a tmux session is a **workspace** |

Switching chats is `select-window`; switching workspaces is `switch-client -t`. Measured
(§7.6): both preserve every pane and neither reattaches or kills the client.

#### The adversary: *"tmux already has windows and sessions. What is charter adding that `tmux new-window` is not?"*

Five things, each measured or read out of the tree, and the last one is the answer on its own.

1. **`new-window` cannot say which harness with which identity.** Measured (§7.5): `new-window
   -e CHARTER_SESSION_ID=chat-A -e CHARTER_HARNESS=claude-code` works on 3.7c and on 3.2 and
   overrides a session-wide `set-environment`. Somebody still has to compose that argv, hold it
   to `layout.CARRIABLE` (which raises on any other name), and mint the id. That is charter.
2. **A tmux window name is not an identity, and it is not even one thing.** Measured (§7.3):
   `automatic-rename` is **on** by default and renamed my eight test windows to `zsh`, `tmux`,
   `bash` and `kernel_task`, following whatever ran in them. `allow-rename` is **off** by
   default, and with it on, `printf '\033kPWNED\033\\'` from inside the pane set the window
   name to `PWNED` — the harness's own output naming charter's tab. And `rename-window` with a
   newline is **refused on 3.7c** (`invalid window name`, rc=1) and **accepted on 3.2**, stored
   escaped as the four characters `a\nb`. Three mechanisms, two versions, four answers. The tab
   bar therefore reads charter's own record and never `#{window_name}`.
3. **The panels.** `new-window` gives one pane. A chat is a harness pane plus the panel panes
   `_split_panels` creates — sized by `layout`, styled per component through
   `instance.pane_bg_options`, with `remain-on-exit` set and two `pane-died` hooks armed before
   anything can die.
4. **The record.** Twelve files per frame that tmux has no place for, at modes charter chose
   rather than the umask (#505/#603).
5. **The switch is not `select-window`, and cannot be.** Measured (§7.4): a background window
   keeps its stale geometry. With the client resized 200×50 → 100×30, the active window followed
   and the background window stayed at 200×50 until `select-window`, which resized it *then*.
   On 3.7c that fires `window-resized` (2 events measured). On **tmux 3.2 — `tmuxctl.FLOOR` —
   `set-hook -w window-resized` returns `invalid option: window-resized`, rc=1.** The hook does
   not exist. So on the floor charter promises to run on, a bare `tmux select-window` leaves a
   chat's panels laid out for a window size that is no longer true, with nothing to repair it.
   Charter's switch must reassert the layout itself. **That is not a convenience over tmux; it
   is a correction of it.**

### 3.2 The deterministic name

The operator wrote `{workspace}-{some-hash}`. **The word doing the work in that sentence is
"deterministic", and it has two readings.** Reading it as *reproducible from inputs* produces a
design that collides and breaks reaping. Reading it as *charter chooses it, nobody is prompted*
produces a clean one. Phase 5 takes the second reading, and here is why the first fails.

**Decision: a chat's id is `{workspace}.{n}` — the sanitised workspace name, a dot, and an
ordinal. It is allocated, not computed. Nothing is hashed.** **OVERRIDES §4j**, which repeats
the operator's `{workspace}-{hash}`.

**What would be hashed, and why that is the problem.** The only inputs available at creation are
the workspace, the chosen harness, the wall clock and a sequence number. Hashing the clock or
the sequence produces a random-looking string carrying no more information than the counter it
hides. Hashing (workspace, harness) is *not unique by construction* — two Claude chats in the
same workspace hash the same. So the only hash that works is a hash of a counter, which is a
counter wearing a disguise.

**Collisions are silent and expensive.** A truncated hash collides at √(2^b): six hex characters
is ~4,000 chats before a coin-flip. That is comfortably beyond one person's use, and the failure
mode is what makes it unacceptable rather than the probability. Two chats sharing
`.charter/frame/<fid>/` share `session`, `panes`, `version` and `workspace` — one chat's token
gauge, pane map and repaint clock overwrite the other's, and nothing reports it. A counter
cannot collide because it is **allocated**, not computed.

**Allocation is a `mkdir`, not a scan.** The ordinal is the smallest positive integer whose
directory does not exist. Charter claims it with a bare `os.mkdir(d, 0o700)` followed by an
explicit `os.chmod(d, 0o700)` — *not* `config.private_mkdir`, which swallows `FileExistsError`
on a directory (#331) and is therefore idempotent, which is precisely wrong for an allocator.
`FileExistsError` is the claim failing; try `n+1`. Two racing allocators cannot both win.

**Why the dot, and why it is not cosmetic.** `state._launcher_pid(name)` does
`name.rpartition("-")` and requires a non-empty head and a digit tail; `is_live` and `reap` both
depend on it, and `reap` is the only thing that bounds `.charter/frame/`. An id ending
`-{ordinal}` would read `2` as a pid — and pid 2 is alive on every Unix, so every dead chat
would look live forever. `{workspace}.{n}` makes `_launcher_pid` return `None`, and **that
`None` is the version discriminator**: an id it can parse is an old `{workspace}-{pid}` frame
and keeps the pid liveness rule; an id it cannot parse is a chat and takes liveness from
`list-windows`, exactly as the guest path already does. No flag day, no migration, no new
field.

**It is legible.** `api.3` is a thing a person says out loud. `api-9f3a1c` is not, and the tab
bar is a readout.

#### What reaches tmux, and what does not

A chat has **two strings and they have different alphabets**, and conflating them is how the
`[frame] hotkey` class of bug happens.

| | alphabet | where it goes |
|---|---|---|
| **id** — `api.3` | `_UNSAFE`-sanitised workspace + `.` + digits. Closed. | `new-window -n`, `split-window` argv, `-e CHARTER_SESSION_ID=`, the directory name |
| **label** — whatever the operator or the harness calls it | open | the tab bar and the palette row, and **nowhere else** |

The label is `contain.one_line`'d **before width arithmetic** (#472), in `Surface.render` and in
the `chats` component, and never reaches a tmux argv or a tmux format string. That last clause
is not caution, it is measured: §7.3 shows `rename-window 'chat#{pane_pid}X'` stores the name
**already expanded** as `chat4327X`, and `#{E:@charter_chat}` expands a `#{...}` in a user
option's value (`#{pane_pid}` → `4327`). A name routed through a tmux format is a format
injection on the first hop.

A chat's palette row id is `chat:<chat id>`. The `:` puts it in `choose.py`'s picker namespace,
which `component._ID_RE` forbids for action ids and component ids — so a provider shipping an
action called `chat` cannot steal the keypress, and the row is correctly excluded from
`palette.matches`' id filter, the same way workspace and persona name rows already are.

#### The adversary: *"what happens when the operator renames a workspace?"*

Nothing, and that is a property rather than an accident. **The id is a name, not a pointer, and
it is never parsed for meaning.** `frame_workspace(fid)` reads the workspace from
`.charter/frame/<fid>/workspace`, name-checked through `workspace.valid_name`, and that file can
be repointed. A renamed workspace's chats keep ids spelling the old name and keep working; the
tab bar shows the workspace the file names, not the prefix of the id. Pin it with a test that
renames a workspace under two live chats and asserts both still resolve.

The one place a rename does cost something: allocation scans `.charter/frame/` for
`{new name}.*`, so chat 1 in the renamed workspace may be `newname.1` while a sibling is
`oldname.2`. That is ugly and harmless, and rewriting ids to fix it would break every
`$CHARTER_SESSION_ID` already exported into a live process. **Do not fix it.**

### 3.3 "Switch without losing sessions" — and what it costs

**What keeps a session alive is tmux, and it costs almost nothing. What keeps a *harness* alive
is the harness, and it costs 226 MB.** Everything below is measured on this machine; commands
are in §7.

Per chat, at rest:

| | measured | note |
|---|---|---|
| tmux window structure | **~4.5 KB**, **1 fd** | §7.1 — 7 extra windows moved the server 3,792 → 3,824 KB and 15 → 22 fds |
| tmux scrollback | **up to 127 MB** | §7.2 — one 200-column pane filling charter's shipped `history_limit = 50000` took the server 3,776 → 130,624 KB |
| charter panel process | **23.7 MB RSS / 15.4 MB phys_footprint** idle, **36.8 MB peak RSS** through a full paint | §7.8 — ×4 per chat as things stand |
| the harness | **226 MB mean, 660 MB max** | §7.8 — 19 live `claude` processes on this machine right now sum to 4.29 GB |

Eight workspaces with two chats each, extrapolated honestly and with the panels-per-chat model
§4j proposed:

- 64 panel processes → **1.5 GB resident / 986 MB dirty**
- 16 harnesses → **3.6 GB**
- tmux structure → ~4 MB. fds → ~94, against a soft limit of 1,048,576 and a
  `kern.maxfilesperproc` of 184,320. **File descriptors are not a constraint and will not
  become one.**
- scrollback → unbounded in practice, and it lives in **one process's heap**, not spread across
  processes the kernel can page independently.

**Total ≈ 5.1 GB before scrollback, and scrollback can exceed everything else combined.**

Two decisions fall straight out of that.

**Decision: a chat has three states, not two.**

- **live** — the tmux window exists, the harness is running, the record is current.
- **cold** — the record exists, the window does not, the harness is not running. Reopening is a
  relaunch (§3.4 says exactly what that does and does not restore).
- **gone** — reaped.

The distinction matters because "switching away does not lose the chat" and "switching away
keeps 226 MB and a token meter running" are two different promises and only the first is what
the operator asked for.

**Decision: charter caps live chats per plane, and refuses past the cap with a reason and a
name.** The cap is per plane, not per workspace, because the memory is the machine's, not the
workspace's. Recommended default **6** — six harnesses at the measured mean is ~1.4 GB plus
~370 MB of panels, which a laptop survives; sixteen is 5 GB, which it does not. The refusal
follows the palette rule from Phase 2: it appears **with its reason**, and it names the coldest
live chat so the operator has one thing to do rather than a lecture.

#### The adversary: *"eight live harnesses is eight times the memory and eight agents burning tokens while you look at one of them. Who pays for that, and does the operator know?"*

The operator pays, and **today charter does not tell them**. That is the actual defect this
question exposes, and it is not fixed by capping. So:

**Decision: the `chats` component shows cost, not just names.** Charter already records
`harness_session(fid)` and `slots.py:384` already draws a token-usage gauge from it for the
frame in front of you. Phase 5 draws that gauge for **every** chat in the workspace, on the one
row you are looking at. A chat burning tokens behind your back says so on the tab.

And running is distinguished from idle, because an idle chat costs memory and no tokens while a
running one costs both. Two sources, both existing: `charter/inflight.py`, which the `attention`
component already reads; and tmux's own per-window flags — measured (§7.3), a background window
with `monitor-activity on` sets `#{window_activity_flag}` on any output and
`#{window_bell_flag}` on a BEL, and both read back through one `display-message -p -F` over the
session's windows, the same shape as the existing `_measure_window`.

**Charter sets `monitor-activity on` per chat window and reads those flags. It does not touch
`bell-action`.** Measured defaults are `monitor-activity off`, `monitor-bell on`,
`activity-action other`, `bell-action any`. `bell-action any` means a background chat's bell
reaches the operator's terminal — which is arguably exactly right and arguably a nuisance, and
charter's conf is session-scoped (`set -t <session>`), so charter would be overriding a personal
preference for a benefit it cannot measure. Leave it alone and say so in `docs/frame.md`.

The activity flag is a **hint, not a truth**: "bytes moved" includes a spinner. The
`inflight` reading is the one charter states; the tmux flag is what it falls back to for a
harness that reports nothing.

#### The adversary: *"if a session survives switching away, what happens when it finishes, fails, or asks a question nobody is looking at?"*

Three answers, one per case, and the first two already have machinery.

- **It fails.** `remain-on-exit on` is already set on every charter pane, and two pane-scoped
  `pane-died` hooks are already armed on the harness pane before it can die. Today the teardown
  hook runs `kill-session`. **Under tabs it must run `kill-window`**, or one chat's harness dying
  takes the whole workspace with it, including the other chats — including chats mid-turn. This
  is the single most dangerous line in the port and it gets its own task and its own test.
- **It finishes.** The pane goes dead, `remain-on-exit` keeps it, `#{pane_dead}` reads true, and
  the chat's mark on the tab bar changes. Nothing is reaped while the window exists.
- **It asks a question.** This is the one with no machinery, and it is why the tab bar is a
  readout rather than decoration: a background chat has no panels of its own (§3.7), so its
  attention row is by definition not on screen. The active chat's `chats` row carries every
  chat's state. That is strictly better than the alternative, in which the row that would tell
  you is in the window you are not looking at.

### 3.4 Many harnesses in parallel — what charter owns

**Charter owns the container, the identity and the record. It owns nothing inside the harness.**
ADR 0018 — charter may run the harness but never draws it — is the same rule seen from the other
side.

**Charter owns:**

1. **The window**, and every pane in it except the harness pane.
2. **The identity carried in at creation**: `-e CHARTER_SESSION_ID=<chat id>`, `-e
   CHARTER_HARNESS=<name>`, and the rest of `layout.CARRIABLE` — `CHARTER_ROOT`,
   `CHARTER_WORKSPACE`, `CHARTER_PERSONA`, `PATH`. **Nothing else, ever**, because
   `layout._env_argv` raises on any other name and because a tmux `-e` becomes world-readable
   argv (already measured on this repo at 138 argv elements, 7,696 bytes, and two live
   service-account tokens — which is why `_frame_identity_env` exists).
3. **The record**, `.charter/frame/<chat id>/`, including `session` — the harness's own session
   id, as the harness's own hook reports it to charter. Charter writes down what it was told.
4. **The working directory** it starts the harness in. cwd is the frame launcher's business
   today and stays so; the `Harness` interface has no cwd field and does not need one.

**Charter does not own, and must not touch:**

1. **The harness's session store.** Charter never enumerates it. §4j settled this and the tree
   confirms why: Claude Code's id arrives as `$CLAUDE_CODE_SESSION_ID` and in the statusLine
   payload; opencode's plugin reads `input.sessionID` off `shell.env` **per invocation** because
   one server hosts many sessions and nothing may be cached; codex has only the payload
   `session_id`. Three stores, three shapes, and asking each "what sessions do you have" is
   harness-specific work that fights the point of being harness-agnostic.
2. **The harness's auth.** Every harness authenticates itself, entirely outside the abstraction.
   Two chats on the same harness share that harness's credentials. Charter cannot separate them
   and does not pretend to (§4).
3. **Anything drawn in the harness pane.** ADR 0018, enforced by construction: `_surface_argvs`
   never takes the harness pane as an argument.

#### The consequence nobody will like, stated up front

**A cold chat cannot be resumed. Reopening one starts a fresh harness session.**

Charter records the harness's session id and has nowhere to send it back: there is no resume
member on `Harness`. `state.clear_shape(fid)` already unlinks `session` at launch for exactly
this reason (#413) — a new frame claiming a recycled id must not inherit another chat's gauge.

Adding `resume_argv` to `Harness` means charter shipping a claim it can verify for one of three
harnesses. `harness/base.py`'s own docstring sets the bar — *"A fifth member needs the same kind
of argument, not just a use."* **Phase 5 does not add it.** It ships the honest behaviour: the
chat id, the workspace, the persona and the cwd all come back; the conversation does not, and
the message says so before the relaunch, not after. Resume is a Phase 6 question with a real
design cost, and a resume that silently starts a new conversation is worse than a sentence
admitting it will.

### 3.5 Where the state lives

`.charter/frame/<chat id>/`, in the shape that already exists, at the modes charter chose.

- Directories 0700 through `config._mkdir_0700`, which does an explicit `os.chmod` **after**
  `mkdir` because mkdir's mode is umask-masked (#470).
- Files 0600 through `config.write_for` → `_private_fd`: `os.open(..., O_CREAT,
  STATE_FILE_MODE)` then `os.fchmod`, deliberately **without** `O_TRUNC` because truncating
  first would empty the file at its old mode (#437/#505/#603). Every writer is
  temp-then-`os.replace`, and the temp file's mode is what settles the destination's.
- Anything new that writes must go through `write_for` or `tests/_statedirscan.py` fails the
  build — it follows a path through one level of indirection, cross-module, and through a
  function's return value (#581/#582).

**Two new files per chat, and no index.**

- `.charter/frame/<chat id>/label` — the display name. Open alphabet, contained at render.
- `.charter/frame/<chat id>/created` — the allocation stamp, so tab order and the ordinal
  allocator do not depend on directory mtime.

**Decision: there is no per-workspace index of chats. The directory is the list.** An index
would make "which chats does this workspace have" one read instead of an `os.scandir` over
~30 entries — and it would be a second source of truth for a fact the directory already holds,
free to disagree with it. That is the #411 shape: writing a pointer some readers may not read.
The scan is cheap and `gather` already caches a plane snapshot per frame.

**What stops one chat writing another's state.** `state.frame_dir(fid, create=True)` resolves
through `contain.child(_root(), fid)` and returns `None` for a hostile id — every Phase 5 writer
goes through it. Around that: `contain.write_refusal`/`writable` refuse writes outside the data
roots; `hooks._state_write_reason` denies a Write or Edit whose target resolves under
`config.STATE_DIR`; `toolgate._control_surface_paths` holds both the declared and the `realpath`
spelling of `STATE_DIR`, `VAULTS_DIR` and every registered vault file.

And the limit, said plainly because the assessment insists on it: **the chat id is an identity,
not a capability.** A process that can set `$CHARTER_SESSION_ID` to another chat's id writes
that chat's directory, because the env var *is* the identity. That is not a hole Phase 5 opens;
it is what identity means. These are guard rails against a mistake, not a boundary against an
attacker with shell access as your user (`SECURITY.md:43-46`).

### 3.6 The surface

**Decision: no new chrome for choosing. Two new components for showing. Everything stays within
Phase 2's ≤2-keystroke criterion.**

**Choosing reuses Phase 2 exactly.** `choose.py` gains `CHAT` as a third noun in `NOUNS`, with
`OPEN_ID`/`NAME_ID` unchanged; the palette grows two doorway rows, `pick:chat` and
`pick:harness`, beside `pick:workspace` and `pick:persona`. `F2` then a filter or an arrow and
Enter — two keystrokes, the same as switching workspace today. `palette.own_the_tty`'s `then=`
seam runs the second surface in the same pane without leaving raw mode; the chat picker is a
third row source over one `Surface`, so there is no new input loop, no new modal and no second
escape hatch.

`F12` keeps working per chat with no new mechanism, and this is the neat part: the hatch is a
**window** option, `@charter_hatch`, resolved by `run-shell -C '#{@charter_hatch}'` in the
presser's own window. One chat per window means one hatch per chat, for free. `F2` likewise
carries no frame identity in its bind text — it resolves `$CHARTER_SESSION_ID` at fire time —
and per-window `-e` (§7.5) is what makes that variable per-chat.

**No default key for next-chat or previous-chat.** `bind -n` is server-wide and would take a key
from every harness in every window. `[frame] component` already validates an operator-chosen
`key` per component through `instance.toggle_key` / `_HOTKEY_RE`, and
`instance.component_tables` already refuses a key that collides with `[frame] hotkey`. Same rule,
same validator, and the same deliberate absence of a default.

**Showing needs two components**, both in the Phase 1 registry, both `edge="top"`,
`size=Fixed(1)`:

- `workspaces` — the workspace bar.
- `chats` — the chat bar. It **hides itself when there is one chat**, showing only the "add
  chat" affordance (§4j). A bar earns its row only when there is something to choose between.

Both join `layout._DROP_ORDER`, **above `top`**: `("right", "repos", "chats", "workspaces",
"top")`. On a short window the bars are the first things to go, because the palette still
reaches every chat at every size and the bars are a readout.

**Degradation within the row, which is where the GUI metaphor is actually tested.** The chat bar
shows the active chat's label in full and the others as their state marks plus a count. It never
wraps, never scrolls, and never truncates a name below `slots._NAME_MIN_W` (12 cells) — below
that it shows marks only. `contain.one_line` runs before any of that arithmetic (#472).

#### The adversary: *"this is an IDE feature borrowed from GUI editors. Does it survive contact with a terminal, or is it a metaphor that breaks?"*

**The bar breaks. The feature does not, because the bar is not the feature.**

Measured against charter's own minimum: a 200-column row holds about twelve names at 12 cells
plus a mark; an 80-column row holds four; a 40-column pane holds one. A design that made the bar
the way you switch chats would work on the author's monitor and fail on a laptop in a split. So
the bar degrades to a status indicator and the palette — which is a **full pane** by §4k, sized
to whatever it is given, with no row cap since Phase 2 — is the mechanism. On a 40-column window
you cannot see the tabs and you can still reach all six chats in two keystrokes.

The metaphor that genuinely does not survive, and Phase 5 does not attempt it: **dragging a tab,
tearing one off into its own window, or moving a chat between workspaces.** §4j settled the
last one — a chat belongs to its workspace for life, because the harness's own context is its
cwd, its files and its history, and moving it makes all three about a different plane. A
conversation wanted elsewhere is a new chat.

### 3.7 What happens to the frame

§4j said: panels duplicate per chat, measure at 5 and 10, then decide. **The measurements are
in, and the decision changes.**

**Decision: panels do not duplicate per chat. Panels follow the active chat.** **OVERRIDES §4j.**

Three measurements force it:

1. **Cost.** 4 panel processes per chat × 8 chats = 32 processes at 23.7 MB resident /
   15.4 MB dirty = **758 MB resident**, all rendering the same `gather.json` at the same
   `state.version`, of which the operator can see four.
2. **Correctness.** A background window **does not resize** (§7.4, measured identically on 3.7c
   and 3.2). Background panels are not merely idle; they are rendering at a width that is wrong.
   That is exactly `panel._component_text`'s `width=slots._width()` guard — the one the deletion
   sweep found, where a constant width made a provider's output wrap and destroy the frame in a
   40-column pane.
3. **Repairability.** On tmux 3.2 there is no `window-resized` hook at all (`invalid option`,
   rc=1). The repair path charter would rely on does not exist at `tmuxctl.FLOOR`.

Under "panels follow the active chat" all three vanish at once, and the third is the elegant
part: **panels are created after the switch, in a window tmux has just resized, so they are born
at the right width.** There is nothing stale to repair, so the missing 3.2 hook stops mattering
for panels. (The harness pane in a background window is still stale-width and is resized by tmux
at `select-window`; how it redraws on SIGWINCH is the harness's business — ADR 0018.)

**The switch, mechanically**, and every step is an existing path:

1. `select-window -t <window id>` on the workspace's session.
2. `_apply_arrangement(old chat, want=[])` — tear down the previous chat's panels. This is the
   same funnel a density change or a component toggle already uses.
3. `_apply_arrangement(new chat, want=<arrangement>)` — split them into the now-active window
   via `_split_panels`, the one funnel every panel pane comes out of.
4. `state.bump(new chat)` so the panels paint. **Bump, not just write a pointer** — #411/#412.

**The cost, measured (§7.7):** `select-window` 4.4 ms; four `kill-pane` chained through
`tmuxctl.chain` 4.9 ms; four `split-window` chained 6.7 ms. **~16 ms of tmux work per switch**,
against 41.5 ms if each command were its own invocation — chaining is worth 3.3× here and is not
optional. The panels then paint about 90 ms later, which is one `charter panel … --once`
process start (`/usr/bin/time` real 0.08–0.09 s, of which `import charter.frame` is 26 ms warm).

Nothing is lost in the teardown, and this is a real property of the current design rather than a
hope: a panel is a pure renderer over `gather.json` plus `state.version`. `panel.run` claims a
pane, watches a version file, and paints. **No state lives in a panel process.**

#### Two alternatives, rejected in writing

- **Chats as panes in one window, the active one zoomed.** Tempting — `resize-pane -Z` is
  already in charter's command surface for the palette, and it would give one set of panels per
  *workspace*, cutting 128 processes to 32 at eight workspaces. It fails on the thing the frame
  exists for: tmux's zoom is a whole-window takeover, so a zoomed chat hides the panels. A frame
  whose panels disappear whenever you look at a chat is not a frame.
- **`join-pane` the panels into the target window on each switch.** Four `join-pane` calls that
  mutate the pane tree of two windows at once, against a `_relayout` that is explicitly *not*
  transactional. A failure halfway leaves the panels in neither window.

#### The adversary: *"you just made switching four times slower than `select-window`"*

Yes — 16 ms instead of 4.4 ms, both comfortably inside the band where a terminal reads as
instant, with a ~90 ms lag before the panels have text in them. In exchange: 758 MB of processes
not started, and no panel anywhere rendering at a width that is not its window's. If measurement
later shows the paint lag is visible, the fallback is to keep the *previous* chat's panels alive
for one switch — but that is a cache, and building it now, unmeasured, is §4d's layout engine
arriving through the back door for the third time.

---

## 4. Security

Priority one on this project, and the honest answer here is smaller than the feature looks.

### The property

**Two chats in one plane hold the same authority. Charter's only real separation is that each
chat's *record* is its own. Phase 5 must not claim more, and — because a tab looks like a
boundary — it must say so where an operator reads it.**

That is the property, not a list, and it follows from what charter is. Two harness processes run
as the same uid, under the same `CHARTER_ROOT`, with the same vaults, on the same filesystem.
Nothing in POSIX separates them without a sandbox charter does not have and has never claimed.
`SECURITY.md:43-46` already says the shape of it: *"Guard rails, not guarantees… a guard against
mistakes, not an attacker with shell access as your user."*

**So the deliverable here is a sentence in `docs/frame.md` and `SECURITY.md`, not a mechanism:**
*a chat tab is a container, not a boundary; two chats in one plane can read each other's files
and each other's vaults, and the second tab does not add that risk — it multiplies the number of
processes that hold it.* Writing a mechanism that half-separates them would be the exact defect
the security assessment's headline names: a claim that is false in a reachable configuration is
worse than no claim.

### What Phase 5 does give, and it is real

**Per-chat identity makes per-chat policy work with no new code.** `session.current()` reads
`$CHARTER_SESSION_ID`, which under tabs is the chat id, so every per-session pointer becomes
per-chat automatically: `.charter/sessions/<chat id>.persona`, `.workspace`, `.tools` and
`.gate`. Two chats can therefore run different personas with different tool ceilings, each
enforced in its own process by its own `toolgate.decide`.

**The narrowing property carries.** `toolgate.frozen_tools` intersects — `tools &= frozen` — so
narrowing takes effect at once and widening never does. A chat that starts under a narrow
persona cannot widen itself mid-session by switching persona.

### If two tabs run different personas, exactly what each may do

- Each chat's `toolgate.decide` reads `persona.resolve_active()` from **that chat's own**
  pointer, and grants exactly `persona.effective_tools(that persona) ∩ frozen_tools(that
  persona, that chat id)`.
- `effective_tools(name)` is that persona's own `tools:` ∪ the `tools:` of everything in its
  `borrows:` — or, when `borrows:` is absent, everything in its `uses:`. Per-persona, computed
  per name; `toolgate.snapshot()` writes `{persona: [tools]}` for **every** persona at
  SessionStart, keyed separately, so opting one persona in never alters another's.
- A chat under `release` may auto-approve the binaries `release`'s own charter names, and
  nothing `forge`'s charter names unless `release` borrows `forge`.
- **Neither may deny.** `toolgate.decide` returns `None` or an allow and never a denial
  (`toolgate.py:10-11`), and the leak guard runs before it (`hooks.py:1293` before `:1339`). A
  persona cannot restrict a chat below the plane's own guards; it can only widen it. An operator
  who reads a narrow persona as a sandbox has misread it, in one tab or in six.
- **The grant is per-persona and #575 closed four fail-opens there this week** — a miscased
  `Borrows:`, a miscased `Extends:`, and duplicate declarations of either, each of which handed
  back the wider legacy grant. `persona._borrows_unreadable` now asks the **whole lineage**, and
  `borrows_of` returns `[]` rather than `None` when it cannot read, failing narrow. Phase 5 adds
  no new grant path, which is the point: **every chat resolves its persona through the same
  ladder, so there is exactly one place for the fifth fail-open to be found.**

### What Phase 5 must not do

- **No new environment variable.** `layout.CARRIABLE` is a hard funnel and `-e` becomes
  world-readable argv. A per-chat secret, token or vault handle passed this way would be visible
  in `ps` to every user on the machine.
- **No index file naming chats outside the frame directory** (§3.5) — a second writable surface
  for no gain.
- **No claim that a chat isolates anything.**

### The standing rule

**Implement, PR, merge — never release alone.** No version bump, no stamping, no tag anywhere in
Phase 5. The release is the operator's, and autonomy stops at it.

---

## 5. One real session, end to end

Workspace `charter`, two chats, two harnesses, switch away, come back. This is the sequence, and
every named function is one that exists today unless marked **new**.

**1. Open the workspace.** `charter claude` in the operator's shell.
`cmd_launch` resolves the harness, `_choose_workspace` returns `charter`, and — **new** —
instead of `state.frame_id(ws, os.getpid())` it calls `state.new_chat_id("charter")`, which
`os.mkdir`s `.charter/frame/charter.1` at 0700 and returns `charter.1`.

`layout.session_argv` creates the tmux session **named after the workspace**, not the frame:

```
tmux -L charter -f <conf> new-session -d -s charter -x 200 -y 50 \
  -P -F '#{pane_id}' -e CHARTER_SESSION_ID=charter.1 -e CHARTER_HARNESS=claude-code \
  -e CHARTER_ROOT=… -e CHARTER_WORKSPACE=charter -e PATH=… -- claude
```

Then, as today: `source-file <conf>`, four `set-environment -t charter`, `arm_hatch_argv` on the
harness pane, two pane-scoped `pane-died` hooks — **whose teardown now runs `kill-window`, not
`kill-session`** — `_split_panels` for the four panels, `select-pane`, and `attach -t charter`.

**New, and one line:** `set -w -t <window> monitor-activity on`, and
`set -w -t <window> @charter_chat charter.1`. The second is why every later lookup can ask the
window what chat it is without parsing anything.

**2. Add a second chat, on a different harness.** `F2` → `pick:harness` → `codex`. Two
keystrokes plus a pick.

`builtin_actions._spawn` fires and reports (actions are fire-and-report, never blocking — §4g),
and the spawned process: allocates `charter.2` by `mkdir`; runs one chained invocation —

```
tmux -L charter new-window -d -t charter -n charter.2 \
  -e CHARTER_SESSION_ID=charter.2 -e CHARTER_HARNESS=codex -e CHARTER_WORKSPACE=charter … \
  -P -F '#{window_id} #{pane_id}' -- codex \
  ';' set -w -t <window id> @charter_chat charter.2 \
  ';' set -w -t <window id> monitor-activity on
```

— records `harness_pane`, `workspace` and `server`, arms the hatch and the two `pane-died` hooks
on the new harness pane, and hands off to the switch (step 3) to make it active.

The old chat's `chats` component repaints on the next `state.bump` and now draws two entries, so
the bar appears for the first time — it hid itself while there was one chat.

**3. Switch to chat 2.** The switch action, ~16 ms of tmux:

```
tmux -L charter select-window -t <window id of charter.2>
tmux -L charter kill-pane -t %5 ';' kill-pane -t %6 ';' kill-pane -t %7 ';' kill-pane -t %8
tmux -L charter split-window … ';' split-window … ';' split-window … ';' split-window …
```

then `state.record_panes(charter.2, …)` and `state.bump(charter.2)`. tmux resized
`charter.2`'s window at `select-window`, so the four new panels are born at the true width. The
first paint lands ~90 ms later.

**4. Switch away — to another workspace.** `F2` → `pick:workspace` → `api`. Measured (§7.6):
`switch-client -c <tty> -t api` moves the client without killing it and without killing a pane;
both `charter`'s windows and both harnesses stay alive with the same pids.

Charter's own bookkeeping on the way out: `state.bump` the chat now becoming active in `api`,
and leave `charter.1` and `charter.2` alone. **The panels of `charter.2` are torn down** — the
same `_apply_arrangement(want=[])` — because no chat in a background workspace has panels.

`charter.1` and `charter.2` are now what §3.3 calls **live**: harnesses running, tokens
possibly burning, no panels, one tmux window each holding one pane.

**5. Come back.** `F2` → `pick:workspace` → `charter`. `switch-client -t charter` puts the
client back on the session; tmux restores its last active window, `charter.2`. The switch action
splits `charter.2`'s panels into the freshly-resized window and bumps.

If the operator resized the terminal while away, this is the case that would have been broken by
a bare `select-window` on tmux 3.2 — no `window-resized` hook, stale geometry, panels laid out
for a window that no longer exists at that size. It is not broken here because the panels did
not exist to be stale: they are created after the resize, by the switch, on every tmux version.

**6. What the operator sees.** Row 0: the workspace bar, `charter` marked. Row 1: the chat bar —
`charter.2` in full with its harness and its token gauge, `charter.1` as a mark plus its gauge,
and if `charter.1` produced output while nobody was looking, its activity mark. Below that,
today's frame exactly: identity, the harness pane, the sidebar, repos, attention.

---

## 6. Decisions that need the operator

Each is answerable in one word. Each has a recommendation and the reasoning behind it.

**6.1 The live-chat cap. Recommend 6.**
Six harnesses at the measured mean of 226 MB is ~1.4 GB, plus ~370 MB of panels — a laptop
survives it. Sixteen is 5.1 GB before scrollback and it does not. The number is a default, not a
law: `[frame] max_chats`, validated at the config boundary, refused rather than clamped, the way
`FRAME_PANE_PAD_MAX` already is.

**6.2 A lower `history-limit` for chat windows. Recommend yes — 20,000.**
Charter ships `history_limit = 50000`, which is 25× tmux's default, and it is set per session,
so under tabs it applies to every chat in a workspace. Measured: one 200-column pane filling
50,000 lines took the shared tmux server from 3.8 MB to **130.6 MB**. That is one pane, in the
one process that holds every chat in every workspace on the machine. 20,000 lines is still 10×
tmux's default and caps the same pane near 50 MB.

**6.3 Reopening a cold chat starts a fresh harness session. Recommend yes, with a message.**
There is no resume member on `Harness` and adding one means a claim charter can verify for one
of three harnesses (§3.4). The alternative — refusing to reopen at all — is worse: it makes
"cold" mean "gone" and deletes the workspace, persona and cwd charter *can* restore. The message
goes before the relaunch, not after.

---

## 7. Measurements

Every command was run on this machine, 2026-08-28, macOS (Darwin 25.2.0), Apple silicon, Python
3.14.4. tmux 3.7c is `/opt/homebrew/bin/tmux`; tmux 3.2 is the binary built from source for the
Phase 3.5 work, at
`…/scratchpad/task7-a6e19896/tmux-3.2/tmux`. Every test server ran on its own `-L` socket and
none of them was charter's own `-L charter`.

Context for the 3.2 column: Phase 3.5 rebuilt 3.2 from the release tarball and re-ran the whole
of the visual design spec's §1 and §4 against it — **sixty-six of sixty-eight answers were
byte-identical to 3.7c**, and the two that were not were the measuring harness's own batching.
The divergences below are therefore worth reading as genuine, not as noise.

### 7.1 A tmux window costs 4.5 KB and one file descriptor

```
$ tmux -L p5spec -f /dev/null new-session -d -s w1 -x 200 -y 50 'sleep 600'
$ ps -o rss= -p $(pgrep -f "tmux -L p5spec" | head -1)   →  3792     (KB)
$ lsof -p <server> | wc -l                                →  15
$ for i in 2 3 4 5 6 7 8; do tmux -L p5spec new-window -t w1 -d 'sleep 600'; done
$ ps -o rss= -p <server>                                  →  3824     (KB)
$ lsof -p <server> | wc -l                                →  22
```

**7 windows: +32 KB, +7 fds.** ~4.5 KB and exactly one descriptor each.

At scale — 8 sessions × 4 windows × 5 panes = **160 panes** in one server:

```
$ tmux -L p5scale list-sessions | wc -l   →  8
$ tmux -L p5scale list-windows -a | wc -l →  32
$ tmux -L p5scale list-panes -a | wc -l   →  160
$ ps -o rss= -p <server>                  →  4656   (KB)
$ lsof -p <server> | wc -l                →  174
$ ulimit -n                               →  1048576
$ sysctl -n kern.maxfilesperproc          →  184320
```

**File descriptors are not a constraint at any plausible scale.**

### 7.2 Scrollback is the constraint, and it is charter's own default

`charter/instance.py:914` ships `"history_limit": (50000, "history-limit")`.

```
$ tmux -L p5hist -f /dev/null new-session -d -s h -x 200 -y 50 'sleep 900'
$ tmux -L p5hist set -t h history-limit 50000
$ ps -o rss= -p <server>                                     →    3776  (KB)
# one pane writes 51,000 lines of 199 columns
$ ps -o rss= -p <server>                                     →  130624  (KB)
$ tmux -L p5hist display -p -t h:0 '#{history_size}'         →   45951
# a second such pane, in the same window
$ ps -o rss= -p <server>                                     →  180624  (KB)
```

**One 200-column pane at charter's shipped history-limit: +127 MB, inside the single shared
`tmux -L charter` server process.** At tmux's own default of 2,000 lines the same fill costs
~2.2 MB per pane (measured separately: 8 windows × 3,000 lines took the server 3,824 →
21,648 KB).

### 7.3 Window names, activity flags, and format expansion

```
$ tmux -L p5spec show -g automatic-rename   →  automatic-rename on
$ tmux -L p5spec show -g allow-rename       →  allow-rename off
$ tmux -L p5spec show -g monitor-activity   →  monitor-activity off
$ tmux -L p5spec show -g monitor-bell       →  monitor-bell on
$ tmux -L p5spec show -g monitor-silence    →  monitor-silence 0
$ tmux -L p5spec show -g activity-action    →  activity-action other
$ tmux -L p5spec show -g bell-action        →  bell-action any
$ tmux -L p5spec show -g window-size        →  window-size latest
$ tmux -L p5spec show -g aggressive-resize  →  aggressive-resize off
```

Identical on 3.2, every line.

**`automatic-rename` renames windows out from under you.** The eight windows created in §7.1
listed as `zsh`, `tmux`, `tmux`, `kernel_task`, `tmux`, `tmux`, `tmux`, `tmux` — the name follows
whatever the pane's foreground process is.

**With `allow-rename on`, the pane's own output names the window:**

```
$ tmux -L p5spec set -w -t ws1:1 allow-rename on
$ tmux -L p5spec respawn-window -k -t ws1:1 "sh -c 'printf \"\\033kPWNED\\033\\\\\"; sleep 900'"
$ tmux -L p5spec display -p -t ws1:1 '#{window_name}'   →  PWNED
```

With the default `allow-rename off` the same sequence left the name as `bash`.

**A window name is contained differently on the two versions:**

| | 3.7c | 3.2 |
|---|---|---|
| `rename-window "$(printf 'a\nb')"` | `invalid window name: a\nb`, **rc=1** | **rc=0**, stored as the four characters `a\nb` |
| `rename-window "$(printf 'a\033[31mR')"` | `invalid window name`, **rc=1** | **rc=0**, stored as `a\033[31mR` (escaped, 10 chars) |

Two versions, two answers, neither of them "what charter asked for".

**Formats.** `rename-window 'chat#{pane_pid}X'` stores the name **already expanded** —
`chat4327X` on 3.7c, `chat49359X` on 3.2. A user option is *not* expanded on set but *is* under
`#{E:}`:

```
$ tmux -L p5spec set -w -t ws1:1 @evil '#{pane_pid}'
$ tmux -L p5spec display -p -t ws1:1 '#{@evil}'      →  #{pane_pid}
$ tmux -L p5spec display -p -t ws1:1 '#{E:@evil}'    →  4327
```

Same on 3.2 (`49359`). A user option also accepts a newline where `rename-window` refuses one, so
tmux's containment of the two is not the same containment.

**Activity and bell flags on a genuinely background window:**

```
$ tmux -L p5spec new-window -d -t ws1: "sh -c 'sleep 5; printf HELLO; sleep 3; printf \"\\a\"; sleep 900'"
$ tmux -L p5spec select-window -t ws1:0
$ tmux -L p5spec set -w -t ws1:1 monitor-activity on
# t+7
  w0 active=1 activity=0 bell=0
  w1 active=0 activity=1 bell=0
# t+11
  w1 active=0 activity=1 bell=1
```

Note in passing: `respawn-window -k -t <w>` **makes that window active**, which cost two attempts
before this measurement was valid.

### 7.4 A background window keeps stale geometry — on both versions

The client is a real attached tmux client in a pty whose size the harness changes with
`TIOCSWINSZ`.

**3.7c:**
```
client 200x50, window 1 active
  w0 active=0 200x50
  w1 active=1 200x50
client resized to 100x30
  w0 active=0 200x50     ← background: STALE
  w1 active=1 100x30
tmux -L p5spec select-window -t ws1:0
  w0 active=1 100x30     ← resized AT the switch
  w1 active=0 100x30
```

**3.2, identical:**
```
client 200x50, window 0 active
  w0 active=1 200x49
  w1 active=0 200x50
client resized to 100x30
  w0 active=1 100x29
  w1 active=0 200x50     ← background: STALE
select-window -t ws1:1
  w0 active=0 100x29
  w1 active=1 100x29
```

**And the divergence that changes a decision:**

```
# 3.7c — a window-resized hook on each window, appending to a log
$ tmux -L p5spec set-hook -w -t ws1:0 window-resized "run-shell 'date >> …'"
  client resize while w0 active                → 1 event
  select-window to the stale w1                → 2 events total

# 3.2
$ ./tmux-3.2/tmux -L p5v32 set-hook -w -t ws1:0 window-resized "run-shell 'true'"
  invalid option: window-resized
  rc=1
```

**`window-resized` does not exist at `tmuxctl.FLOOR`.** `tmuxctl.RESIZE_HOOK_FLOOR = (3, 3)`
already says so; this is what it means for tabs.

### 7.5 Per-window `-e` overrides a session-wide `set-environment`

```
$ tmux -L p5env-X -f /dev/null new-session -d -s s -x 100 -y 30 'sleep 60'
$ tmux -L p5env-X set-environment -t s CHARTER_SESSION_ID "session-wide"
$ tmux -L p5env-X new-window -d -t s -e CHARTER_SESSION_ID=chat-A -e CHARTER_HARNESS=claude-code \
      "sh -c 'printenv CHARTER_SESSION_ID > …; printenv CHARTER_HARNESS >> …; sleep 60'"
$ tmux -L p5env-X new-window -d -t s -e CHARTER_SESSION_ID=chat-B -e CHARTER_HARNESS=codex  …
```

| | 3.7c | 3.2 |
|---|---|---|
| window A | `chat-A claude-code` | `chat-A claude-code` |
| window B | `chat-B codex` | `chat-B codex` |

**This is the mechanism that makes identity per-chat**, and it is available at
`tmuxctl.PANE_ENV_FLOOR = (3, 0)`, below the floor charter warns at.

### 7.6 Switching loses nothing

```
$ tmux -L p5spec switch-client -c /dev/ttys058 -t ws2
  client session: ws1 → ws2
  attach process still alive: yes
$ tmux -L p5spec list-panes -a -F '#{session_name}:#{window_index} … dead=#{pane_dead} pid=#{pane_pid}'
  ws1:0 100x30 dead=0 pid=4152
  ws1:1 100x30 dead=0 pid=4327
  ws2:0 100x30 dead=0 pid=23812
  ws2:1 100x30 dead=0 pid=23815
$ tmux -L p5spec switch-client -c /dev/ttys058 -t ws1     → back, attach still alive
```

No pane died, no pid changed, the client was never reattached.

### 7.7 The switch is cheap, and chaining is worth 3.3×

100 iterations each, `subprocess.run` around the whole tmux invocation:

```
select-window       : min 4.1  median 4.4  p95 4.9  ms
switch-client       : min 4.1  median 4.4  p95 4.8  ms
list-windows -a     : min 3.9  median 4.3  p95 4.8  ms      ← the baseline: spawning tmux at all
```

**The switch itself is free; the round trip is the cost.** Twenty iterations of the panel
teardown and re-split, four panes each:

| | one invocation per command | chained through one invocation |
|---|---|---|
| 4 × `split-window` | median **22.2 ms** | median **6.7 ms** |
| 4 × `kill-pane` | median **19.3 ms** | median **4.9 ms** |

`tmuxctl.chain` already exists and returns `None` when the servers disagree. Phase 5's switch
must use it.

### 7.8 What a panel costs, and what a harness costs

```
$ python3 -X importtime -c "import charter.frame" 2>&1 | tail -1
  import time:  153 | 24147 | charter.frame          ← 26 ms cumulative warm, 48 ms cold
$ /usr/bin/time -l python3 -c "pass"                       → 15171584   max RSS  (15.2 MB)
$ /usr/bin/time -l python3 -c "import charter.frame.panel" → 24379392   (24.4 MB)
$ /usr/bin/time -l python3 -m charter panel identity --session testfid-1 --once
  0.09 real   36880384 max RSS  (36.8 MB)
```

Four component ids measured: `identity` 36.9 MB, `repos` 36.8, `personas` 37.1, `todos` 36.7,
each 0.08–0.09 s real.

Eight concurrent processes that have imported `charter.frame.panel` and built the registry:

```
$ ps -o rss= -p <8 pids> | awk '{s+=$1} END {print s}'   →  190848 KB  (186 MB)
$ footprint -f bytes -p <one>
    15401416 B   dirty
     4931584 B   clean
    phys_footprint: 15434184 B                            →  15.4 MB
```

**A panel is 23.7 MB resident / 15.4 MB dirty idle, and peaks at 36.8 MB through a paint.** The
brief's working figure of ~13 MB is in the right neighbourhood for the *dirty* number and
understates the resident one; the multiplication in §3.3 uses the larger.

The harness, sampled from what is actually running on this machine right now:

```
$ ps -eo pid=,rss=,comm= | awk '$3=="claude"{…}'
  claude processes: 19 | sum RSS: 4389216 KB = 4286 MiB | min: 48992 KB | max: 676192 KB | mean: 231011 KB
```

**19 live harnesses, 4.29 GB, mean 226 MB, max 660 MB.** A panel is noise. The harness is the
cost, by an order of magnitude, and no amount of panel sharing changes the shape of the bill.

### 7.9 What I could not measure

Stated rather than assumed, because a spec that hides its gaps is worse than one with fewer
claims.

- **Whether `#(shell)` command substitution executes through `#{E:@option}`.** I proved
  `#{E:}` expands nested `#{format}` references (`#{pane_pid}` → `4327`). I could not get a
  `#(touch …)` inside a user option to run — not through `display -p '#{E:@evil}'`, not through
  `#{E:#{E:@evil}}`, and not through `status-left` with `status on`, because I could not attach
  an interactive client with a rendering status line from a non-interactive shell. **The `#{}`
  expansion alone is enough to reject routing a name through a tmux format**, so the design does
  not depend on the unmeasured half — but do not repeat the stronger claim.
- **Panel cost under real repaint load.** Every panel figure is an idle process or a single
  `--once` paint. Nobody has run four panels against a plane bumping its version at speed.
  §4j's `_right` at 4.8 ms is that spec's number, not one re-measured here.
- **Anything above two chats, live.** Every scale figure above is `sleep` processes standing in
  for harnesses, plus a separately-measured real-harness distribution. Nobody has run six real
  agents in six tabs. **§4j's instruction to measure at 5 and 10 chats is not discharged by this
  document** and is a task in the plan.
- **Whether the tab bar looks right.** No screenshot was taken and nobody looked at a screen.
- **tmux 3.3 through 3.6.** Untested, here as in Phase 3.5. The two floors that matter
  (`RESIZE_HOOK_FLOOR`, `PANE_ENV_FLOOR`) are read from tmux's own CHANGES, and only their
  endpoints were run.

### 7.10 The switch, measured against §7.7's extrapolation (Stage 5b)

**Added 2026-08-30, when Task 4 was built. §7.7's ~16 ms is wrong by a factor of 22, and
the reason is that it counted the wrong commands.**

§7.7 timed `select-window` (4.4 ms), four `kill-pane` chained (4.9 ms) and four
`split-window` chained (6.7 ms) and added them up — nine tmux commands. Charter's one live
pane-mutation path (`commands_frame._apply_arrangement` → `_relayout`) issues **41**, and
the extra thirty-two are not incidental: `_disarm_panel_respawn` before each kill,
`_arm_panel_respawn` after each split, `_panel_mark_argv`, the pane surface and border
options, `_install_resize_hook`, and `_reassert_sizes` — which exists precisely because a
`kill-pane` or a `split-window -l` makes tmux redistribute every surviving pane.

Measured end to end: `cmd_chat` between two real chats of one workspace session, a real
attached client at 200x50, four panels on the chat being left, six switches alternating
direction.

```
tmux 3.7c   median 360.2 ms   min 282.2   max 600.3   41 invocations each
tmux 3.2    median 394.7 ms   min 330.2   max 428.4   41 invocations each
one tmux invocation, median: 6.18 ms (3.7c) / 6.70 ms (3.2)
```

41 × 6.2 ms is 254 ms of round trips; the rest is charter's own work between them.

**What this changes and what it does not.** §3.7's decision stands and is not close: 41
invocations once per switch against 758 MB of panel processes permanently rendering at
widths that are wrong is not a trade this reopens. What changes is the adversary's answer
in §3.7 — "16 ms instead of 4.4 ms, both comfortably inside the band where a terminal reads
as instant" is no longer true at 360 ms, and the honest statement is that a switch is
perceptible and roughly half a second before the panels have text in them.

**And it changes the plan's Task 4 step 4.** "One `tmuxctl.chain` per group … chaining is
worth 3.3× here and is not optional" was derived from the nine-command model. The eight
commands a chain would collapse are under a fifth of the forty-one, so chaining them buys
about 10 % of the switch. The rest is in the other thirty-three, which means the real
optimisation is collapsing the whole of `_relayout` into one invocation — a change to the
funnel a density change and every component's toggle key also go through, and not one to
make at the end of a stage. Stage 5b therefore ships the switch over the existing funnel,
unchained, and this is the measurement that says why.

### 7.11 `layout._DROP_ORDER` was read by nothing (Stage 5b)

**§3.6's instruction "Both join `layout._DROP_ORDER`, **above `top`**" does not, on its
own, do anything.** Checked on the tree at `7dcf09c`: `grep -rn _DROP_ORDER charter/`
returns the definition and one docstring mention. `layout.visible_slots` spells the same
order out by hand — `s != "right"`, then `s != "top"` — so adding two names to the tuple
would have left both bars surviving exactly the shortage that takes the identity row, which
is the wrong way round for a readout the palette makes redundant.

Stage 5b derives the row-edge half of the list (`layout._ROW_DROPS`) from `_DROP_ORDER`, so
the constant is now the mechanism and an entry deleted from it changes what a short
terminal draws.
---

## 8. What this spec deliberately does not do

- **No resume.** §3.4. A cold chat reopens as a fresh harness session, and says so.
- **No moving a chat between workspaces.** §4j settled it; §3.6 repeats why.
- **No per-chat sandbox, jail, uid or container.** §4. Charter would be claiming a boundary it
  does not have.
- **No chat-level cross-repo query.** "Show me the chats working on this change" needs Phase 4's
  change object and this phase's chats to both exist. Phase 5 makes it askable, not answerable —
  §4j says the same.
- **No shared-panel cache.** §3.7 rejects it as unmeasured, and would reject building it now for
  the same reason it rejected duplicating them.
- **No new ADR.** Phase 5 changes what a frame *is* without changing any decision an ADR
  records: 0015 (the boundary moves with the harness), 0018 (charter may run the harness but
  never draws it) and 0019 (the frame owns the surface) all hold verbatim under tabs, and 0018
  is what forbids the one shortcut this design might have taken.
