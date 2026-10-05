# Smart close sends its prompt, and the record command closes the tab

**Accepted 2026-09-28**, by the operator's rulings on SI-8 (smart IDE).

Closing a chat throws away everything it knew that is not already in the plane: why it did what
it did, what it left half-done, where the next chat should start. The operator ruled that
closing can first have **the same chat** write a **session record** into its workspace, fold
what lasts into `workspace.md`, and then close. That is **Smart close**.

This PR is the core, the command line, the skill and the documents. The app's side is the next
PR, and its rules are recorded here so it has one place to build from.

## The decision

### A session record is a summary the chat writes, and purlis files

A **session record** is one Markdown file per record: `workspaces/<ws>/sessions/
<YYYYMMDD-HHMMSS>-<slug>.md` for a chat in a workspace, and `sessions/<…>.md` at the plane's root
for a chat at the plane root, which is in no workspace (SI-1). It is a summary and never the
transcript: at most 32 KiB, and its body is exactly five `## ` sections, in order — **Goal**,
**Done**, **Decisions**, **Open**, **How to resume** — each with something under it and nothing
before the first. A heading inside a code fence is text.

**The model writes the title and the body; purlis writes everything else.** The frontmatter is
purlis's alone, from facts it already holds:

| Key | From |
|---|---|
| `title`, `date` | the command's `--title`; the machine's clock |
| `chat`, `chat-name` | `$PURLIS_CHAT`, else `$PURLIS_SESSION_ID` (the same number, set in every app chat); the name the chat's tab shows, from the app's record (`reopen::shown_name`) |
| `persona` | the persona ladder, as every command reads it; `none` for none |
| `harness` | the program the app's record says the chat runs |
| `conversation` | `reopen::conversation_of` — see below |
| `workspace` | the workspace, or `plane root` |
| `piece` (one per line) | the piece the command runs in, and each `--piece <repo>/<piece>`, with the branch `git worktree list` reports; a piece git does not report is refused |

A fact purlis does not have is written `unknown`, never left out, so every record has every key
and a reader never has to guess whether a key's absence meant something.

**The conversation id is SI-8a's answer.** `reopen::conversation_of(root, number)` is the one
way to ask which conversation a chat is in now: the app keeps the chat's `resume` in
`reopen.json` current from its own hook reports (the id a Codex or opencode chat names, the one a
Claude Code chat moves to on `/clear`). A chat it holds no id for is written `unknown`.

`sessions/index.md` (newest first, one line per record: date, title, link) and `workspace.md`'s
`## Sessions` line (the count, the latest record, a link to the index) are **rebuilt from the
records** every time one is written. The records are the one source of truth; a hand edit to
either lasts until the next record. `## Sessions` is in the workspace.md template, before
`## Log`, saying there are none yet. **The skill, not purlis, folds** durable decisions, terms
and vision changes into `## Context & decisions`, `## Glossary` and `## Vision`: those are
judgments, and purlis's line is only a pointer.

### LIVE and LOCAL

A workspace's records follow the workspace: the LIVE block un-ignores `sessions` and
`sessions/**` beside `memory` and `todos`, `purlis workspace live` and `reinit` write it, and a
LOCAL workspace keeps them on disk. `meta_paths` lists `sessions` so going LOCAL untracks them as
it untracks memory.

**The plane root's records stay on this machine** (`/sessions/` in the plane's `.gitignore`,
written by `init` and added by `reinit`). A workspace is LOCAL until somebody makes it LIVE; the
plane root has no such switch, so the choice that publishes nothing by itself is the one that
fails toward no change. Committing them by default would push every plane-root chat's summary to
the plane's remote — public, for a plane like purlis-plane — without anyone having chosen it.
Deleting the line shares them.

Secrets: the skill keeps them out, and `check` refuses a record whose title or body matches the
credential shapes `purlis save` refuses a memory for (`secretshape::secret_kind`), naming the
kind and never the value. The save-time scan still guards whatever is committed.

### One writer, and it is what ends a Smart close

`purlis session record --title … [--piece …] [-w …]` with the body on standard input is the
**only** write path. It validates (`sessionrecord::check`: title one drawable line of at most 120
characters, body shape, size cap, no control or invisible character but a line feed or a tab, no
credential shape), writes the record whole (temp file and rename, the name taken under a lock on
the directory), rebuilds the index and the pointer, and then **tells the app**: one fire-and-forget
line on `$PURLIS_HOOK_SOCKET`,

```json
{"chat": 3, "session_saved": "/abs/plane/workspaces/alpha/sessions/20260928-140312-ship-it.md"}
```

`hookwire::SessionSaved`, the fourth kind of line beside a report, an ask and `StartedByHand`
(ADR 0062), and the same shape: one connection, one line, a 250 ms write deadline, every failure
dropped. It is read after the other three and requires `session_saved`, which none carries, so
no line an older `purlis` writes reads as one, and an app older than this reads the line as
nothing and drops the connection. `Listener::each_answering_noticing_and_saving` hands it to the
app; the existing `each_answering_and_noticing` drops it.

With no socket or no `$PURLIS_CHAT` (a chat started outside the app), or an app that did not
hear it, the record is written all the same and the command says the tab will not close by
itself. `purlis session list [-w]` and `purlis session show <file>` read them back.
`session` joins purlis's core words.

**Completion is signalled by the command, never by reading output.** The app learns the record
is saved from the line the command sends, exactly as it learns a chat's state from hooks: nothing
watches the terminal for a sentence the model printed (spec decision 3).

### Smart close sends its prompt

**This is the deliberate exception to ADR 0061.** A curation action types its prompt and never
sends it, because the prompt is the whole contract with the operator and nothing may run before
they have read it. Smart close is different in kind: the operator has already chosen what
happens — by clicking **Smart close** on a chat they are looking at — and the prompt names one
skill of purlis's own, on that chat, whose only write is a record the operator asked for. The
click is the consent, so the app **sends** the prompt. There is still no setting that makes a
curation action send its prompt; this exception is Smart close's alone.

The prompt is one line naming the skill in words, never a `/slash` command, so it works on every
harness that reaches purlis's skills (ADR 0063): *Use purlis's smart-close skill to write this
session's record and close the chat.* The skill ends with `purlis session record`, and so the
command that writes the record is the command that closes the tab.

## The app's side (the next PR)

- **When the prompt is sent.** On a chat that is **Waiting**, at once. On a **Running** chat it is
  queued until that chat's next `Stop`, then sent. On a chat that **needs you** (a permission or
  a question), Smart close is refused, with a sentence: the chat is asking the operator something,
  and the operator answers it first. A chat that was **never prompted** or whose state is
  **Unknown** has nothing to record: it is offered **Close** only.
- **How it ends.** On a `SessionSaved` whose `chat` is the chat being smart-closed, the app
  closes that tab as Close does. A `SessionSaved` for a chat that is not being smart-closed closes
  nothing.
- **When it does not end.** About five minutes after the prompt was sent with no record, the tab
  goes back to normal with one sentence saying no record arrived and the chat was left open.
  **Smart close never closes a chat without its record.**
- **While it runs.** The tab wears a closing look that reuses the `charter-breathe` animation,
  still under reduced motion through the motion tokens. Its menu offers **Cancel smart close**,
  and the operator typing into the chat cancels it too — the app knows the input is theirs, as it
  does for a curation prompt (ADR 0061, Q29). A cancelled Smart close leaves the chat open and
  running; a record it writes afterwards closes nothing.
- **Asking.** Closing a chat asks **Smart close** (primary), **Close** or **Cancel**. **Close**
  is the default when the chat has had at most one turn, where there is little to record.

## Getting a session back (SI-8d, added 2026-09-28)

The operator's ruling (Q9): *the user can always get old sessions back.*

- **A Sessions panel** per workspace, and one on the plane root's tab for the plane's own, lists
  `sessionrecord::list` newest first: the title, when, persona and harness, and `↻ resumable`
  where the record holds a conversation. A row opens the record as a read-only view tab
  (`{ view: "session", key: <plane-relative path> }`), rendered Markdown with no HTML.
- **A record is named by its plane-relative path**, and `sessionrecord::locate` is the one
  reading of it — the CLI's `session show`, the app's commands and the briefing all go through
  it, and it refuses every other spelling rather than normalising one.
- **Resume starts a NEW chat**, never the one that wrote the record: in the record's place, on
  a profile of its harness (the project's default first, then a declared one, then the
  built-in, because the record does not name the profile), with `Start::resume` set to its
  conversation so `start::ready` builds the harness's own resume words — the relaunch's builder,
  not a second one — and as its persona where the plane still has it. `Start::resuming` puts the
  record's path in `$PURLIS_RESUMING_RECORD`, and the briefing quotes the record from it, every
  line behind `> ` under a sentence saying it is data, up to 8,000 characters, in place of the
  last-session line.
- **When the conversation cannot be given, the chat is fresh and says why**: no id in the
  record, no harness in it, no profile here that runs it. A harness that cannot find the
  conversation cannot be asked in advance; it says so by its program failing **before it
  reported anything** — a state the board holds from hooks and the exit status, never from
  output. The window then closes that chat and asks again with `after_failure`, which starts the
  same record fresh, once, and says the harness could not bring the conversation back.

## What this costs

- **A record costs a turn.** Smart close spends one more turn of the model on the way out.
- **The title is the model's words in every later briefing.** Each session start names the
  place's last record in one line: *Last session: "<title>" — <path>*, the title quoted and
  said to be data, never a task. A title that reads as an order is still only read.
- **A plane-root record is not shared** unless the operator deletes the `.gitignore` line.
- **A record made from inside a piece** records that piece; one made from the workspace
  directory records only the pieces passed with `--piece`.

## What was rejected

- **A transcript, or a log.** It is large, holds everything the chat saw (secrets included), and
  is what a harness already keeps. A record is what the next chat needs, in five sections.
- **Frontmatter the model writes.** Then a record could claim a chat, a branch or a workspace it
  never had. The model gives two things; purlis gives the rest.
- **Reading the chat's output to know it is done.** Parsing harness output decides nothing
  (spec decision 3); the command that did the work says it is done.
- **purlis folding the record into `workspace.md`.** Deciding what is durable is judgment. The
  skill does it; purlis keeps one pointer line.
- **Committing plane-root records by default.** Above.
- **Reading the harness's "no conversation found" to fall back.** Output decides nothing (spec
  decision 3). The failed exit before any report is the signal, and its cost is said: a resumed
  chat whose program fails for another reason before its first report is also started again
  fresh, once.

## Finishing Smart close (SI-8e, amended 2026-09-28)

The operator's rulings of 2026-09-28, and what was measured to carry them out.

### The record command is pre-allowed on Claude Code, and on nothing else

_Amended 2026-10-03 by V79: purlis's five read-only MCP tools are pre-allowed beside it
(below)._

A Claude Code chat the app starts carries one permission rule in its session `--settings`:
`{"permissions": {"allow": ["Bash(purlis session record *)"]}}`
(`harness::SMART_CLOSE_ALLOW`). One `allow`, no `ask`, no `deny`, no mode. It is a session flag,
not a file: nothing is written into the plane's `.claude/settings.json` or into a workspace layer,
so the plane's own settings and the recorded fixtures don't change, and the layer's rule that a
grant never travels sideways (`layer::RESTRICTIVE`) still holds.

**Measured on Claude Code 2.1.283**, the real `claude` in a pseudo-terminal against a stand-in
Messages server (`ANTHROPIC_BASE_URL`, a dummy token), with a scratch `HOME` and
`CLAUDE_CONFIG_DIR` and a stand-in `purlis` on `PATH`. No real model call, and none of the
operator's settings read or written. Each measurement is one launch:

| Case | What happened |
|---|---|
| The ADR 0064 prompt as one bracketed paste and `\r`, in **one write** | submitted; the stand-in's main request held exactly one user text, *Use purlis's smart-close skill to write this session's record and close the chat.* |
| `--permission-mode default`, **no** allow rule, the skill's heredoc command | asked: *This command requires approval. Do you want to proceed?* (the red run) |
| the same, **with** the allow rule | ran in about a second, no prompt |
| a body holding backticks, `$(…)` and `$HOME` inside the quoted heredoc | ran, no prompt |
| the record command followed by `touch <file>` in one command | asked; the file was not made |
| a user-level `allow` (`Bash(echo *)`) and a project `deny` (`Bash(rm *)`) beside the session rule | both still in force: `echo` ran, `rm` was refused without a prompt. The session's `permissions` merge with the other sources, they don't replace them |

Claude Code gives `deny` and `ask` precedence over `allow`, so an operator's own `ask` or `deny`
for this command still wins.

**Codex (0.147.0) needs no rule, and has none to give.** Measured the same way against a
stand-in Responses server (a scratch `CODEX_HOME`, a trusted directory, the default approval and
sandbox): the paste and `\r` in one write submitted exactly the prompt, and the heredoc command
**ran without asking** inside the `workspace-write` sandbox. Codex has no per-command, per-session
approval. Its approval policy and sandbox are whole-session switches, and its execpolicy
`.rules` are files in `CODEX_HOME` or the project. So nothing is added (`harness` tests hold
that). **The same sandbox refused the command's connect to the app's hook socket**
(`Operation not permitted`), so on Codex the record is written but the tab doesn't close by
itself: #517, ruled and fixed below (SI-8f).

**opencode is a documented gap.** Its default lets `bash` run without asking. It can be told
`permission.bash` patterns for one session through `OPENCODE_CONFIG_CONTENT`, but there the more
specific pattern wins, so `"purlis session record *": "allow"` would override an operator's own
`"purlis *": "deny"`: broader than the ruling allows. The other way is a permission hook in the
shim, which is authority purlis arms nowhere (`plugin` tests). Nothing is added. A chat whose
operator configured opencode to ask will ask.

SI-8c's rule is unchanged: answering a question the chat asks mid-turn doesn't cancel a Smart
close.

### Amended 2026-10-03: purlis's five read-only MCP tools are pre-allowed too (V79, #1050)

The operator's ruling V79 widens the one-allow rule above, for Claude Code only. The same
session `--settings` now carries the Smart close allow and five more, one per read-only tool of
purlis's own MCP server (HP-7, `chattools::PRE_ALLOWED`):

`mcp__charter__todo_list`, `mcp__charter__memory_search`, `mcp__charter__session_record_list`,
`mcp__charter__session_record_read` and `mcp__charter__change_status`.

- **Each tool by its full name, never the server.** An `mcp__charter` rule would allow every
  tool purlis's server offers, including one added later.
- **The writes and `ask_operator` still prompt.** `todo_add`, `todo_done` and `memory_add` change
  the workspace. `ask_operator` is marked read-only, but it is a question for the operator and
  goes through the harness's prompt anyway. The list is written out, not derived from the
  read-only mark, so a new tool asks until someone rules it in.
- **Still only `allow`, still a session flag.** No `ask`, `deny` or mode is added, nothing is
  written to a settings file, and an operator's own `ask` or `deny` for any of these tools still
  wins, as measured above for the record command.

**A rule names a server by its name, so which server answers to `purlis` was measured.** Claude
Code also loads MCP servers from a project's `.mcp.json`, from the local and user scopes and from
the operator's own configuration, and a server registered there under the same name would answer
to the same tool names, pre-allows included. Measured on Claude Code 2.1.288, one launch per
case: the real `claude -p` with a scratch `HOME` and `CLAUDE_CONFIG_DIR`, a dummy token and an
unreachable API, reading the server list and the tool list it reports at start. Two stand-in
stdio servers both named `charter` offered different tools, one handed over with `--mcp-config`
the way purlis hands it, and one in the scope under test.

| The other `purlis` | Without `--mcp-config` | With purlis's `--mcp-config` |
|---|---|---|
| project `.mcp.json`, not approved | not loaded | the session's answers; the other is never started |
| project `.mcp.json`, approved by name | it answers | the session's answers; the other is never started |
| project `.mcp.json`, every project server enabled | it answers | the session's answers; the other is never started |
| local scope | it answers | the session's answers; the other is never started |
| user scope | it answers | the session's answers; the other is never started |

The session's `--mcp-config` server wins in every scope, and the same-named one is not started
at all. So the name stays `purlis`, without `--strict-mcp-config`, which would also drop the
operator's own servers from every chat. It is what 2.1.288 does, not a rule Claude Code
documents: one whose precedence changed would hand these five allows to another server, so it is
measured again with this ADR's other Claude Code measurements. A session-unique server name was
considered and not taken: it would change every tool's name in every chat for a case that
measured as closed.

Codex and opencode are not covered by this ruling, and nothing is added for them.

### The record names its profile and its directory, and Resume uses them

`profile:` (the app's `reopen.json` `profile` for the chat, by name) and `cwd:` (its `cwd`,
plane-relative, `.` for the plane root, `unknown` outside the plane) are purlis's frontmatter,
next to `harness:` and `workspace:`, never the model's. The CLI takes neither as a flag or from
the chat's environment. Read back, `profile:` is kept only as a profile name, and `cwd:` only
as plain relative components: no `..`, not absolute, no backslash, nothing undrawable.

`sessionresume::ready` tries the record's profile first when this machine still has it as a
profile of the record's harness, then the old order (default, declared, built-in). It starts in
the record's `cwd` when that is still a directory that, with every link followed, is inside the
record's place (the workspace's directory, or the plane). Anything else starts in the place's
own directory. Either fallback is one sentence in `Resumed::notes`, and the window shows it
beside *was resumed* (`OpenChat.guessed`). A record written before these keys reads as
`unknown` for both, resumes as it did before, and says what it guessed.

**What was measured about the directory, and it corrects the reason given for it.** Claude
Code 2.1.283's `--resume <id>` found a conversation from a subdirectory of where it was made,
from its parent, and from an unrelated directory. Wrong directories didn't stop the resume. So
the `cwd:` is not what makes Claude find the conversation. It is what puts the resumed chat back
where it was working: in the piece, for a record written from a piece.

### Losing the conversation, measured

| Harness | Launch | Result |
|---|---|---|
| Claude Code 2.1.283 | `claude --resume <unknown id> --name "steward 9"` | exit **1** after **0.6 s**, *No conversation found with session ID: …*, no model request, and **no `SessionStart` hook** (the same hook fired at an ordinary launch) |
| Codex 0.147.0 | `codex resume <unknown id>` | exit **1** after **0.8 s**, *No saved session found with ID …*, no model request (Codex fires `SessionStart` only inside a first turn) |

Both end with a failure before reporting anything, which is exactly what `lostOnResume` reads.
The window then closes that chat and asks again with `after_failure`, which starts the same record
fresh, once (the Sessions window tests hold that sequence). A known id resumed on both.

### A record's tab resumes its own record

The Resume button on a record's tab was the catalogue's `session.resume:<path>` row. The
catalogue offered that row only for the records of the place in front, so a tab showing another
place's record drew no Resume. Now the catalogue offers the row for every open record tab too,
one row per record, and the button still is that row.

## Smart close puts the chat into the background (SI-8f, amended 2026-09-28)

The operator's ruling: *Smart close should feel like putting the chat into the background.*

### The tab becomes a chip, and the front goes where Close would send it

On the Smart close click the tab shrinks to a **chip**: the chat's icon and the breathing amber
diamond (still under reduced motion), no name, and the tooltip `<name> — wrapping up`. It is
drawn at the left edge of the chat strip, before the pinned tabs, and it is fixed: it cannot be
dragged, and nothing is dropped on it (`tabs.tabsIn`, `reorder.ts`). ADR 0039 is amended for it:
the operator's click moved it, and nothing reorders on its own.

The front goes **exactly where Close would have sent it** — the tab before it on the strip, or
the first after it, or the workspace's empty state when it was the last chat. That is one
function, `tabs.frontWithout`, which `closeTab` and `sendToBackground` both call. A chip is never
where the front goes when another tab closes. The click is what moves it, so it moves at once,
before the core's first step arrives; a start the core refuses brings it back.

Clicking the chip brings the chat forward to watch it work, and it stays a chip while it wraps
up. **Cancel smart close** is still on its menu and in the palette, and typing into the chat
still cancels it. A tab with a second chat beside it in a split does not become a chip — it
would not close on the record — and keeps the wrapping-up mark.

### How it ends

- **Saved:** the chip goes, as before (`closeChat`, never `close_session`), and a quiet notice
  says *Session saved — <title>* with **Open record**, which opens the record's view tab (SI-8d).
  The `Closed` step now carries the record (`SmartClosing.record`: its plane-relative path and
  its title), read through `sessionrecord::saved`, which reads the line's path only as a record
  of this plane — a line naming anything else still closes the tab and names nothing.
- **No record in five minutes, the chat ended, or the start refused:** the tab comes back **in
  its old place**, because nothing ever moved it in `tabs.order` — the chip is drawn first, and
  that is all. The one-sentence notice stays, and the chat is also listed in the title bar's ✋
  needs-you menu with why (*smart close stopped — no session record arrived in five minutes*, *…
  it ended before it wrote its record*, *smart close did not start*). A chat already in the queue
  is one item that says why; one that is not is an item of its own, whose ✕ dismisses it. Going
  to it takes it off the list.
- **Cancelled:** the tab comes back in its old place, and nothing is listed.

### A Stop hook passes on the line the sandbox refused (#517)

**What was measured.** Codex 0.147.0's default `workspace-write` sandbox runs `purlis session
record` without asking and lets it write the record, and refuses its connect to the hook
socket. Its hooks run outside that sandbox. Measured again for this change with the plane
outside the sandbox's temporary roots (`exclude_slash_tmp` and `exclude_tmpdir_env_var`, since
the scratch plane lived under `/tmp`): from a chat in `workspaces/alpha`, a command could write
below `workspaces/alpha/` and could **not** write the plane's own `.charter/`. So the refused
line cannot wait in the plane's `.charter/sessions/`, the place first suggested for it; it waits
in the directory of the place the chat works, which is the chat's own.

**The rule.** When `purlis session record` had a socket and a chat to tell and the line did not
get through, it leaves a marker, `<place>/.charter/sessions/<chat>.saved`
(`docs/plane-format.md`): the chat, its conversation as the app recorded it, the record's path,
and when. The place is the one the command files a record in when it is given no `-w`, which is
where the hook looks too — one function, `Here::place`, answers both. The chat's next `Stop`
hook — the end of the same turn — takes the marker (`sessionrecord::relay::take`): it removes
it, whatever it holds, and sends the `SessionSaved` line only when the marker is this chat's,
this conversation's where it names one, and at most `relay::PASSED_ON_WITHIN` old. That is five
minutes, and the app's `GIVES_UP_AFTER` is now that constant, so the two waits are one number.

**Why this is still one writer and still a command's signal.** The line the hook sends is the
one the command would have sent, built from what the command wrote down. Nothing is read from
what the harness printed. **It cannot close a tab twice**: the app closes on a `SessionSaved`
only while that chat is being smart-closed (`smartclose::saved`), so the same line arriving a
second time closes nothing (`planes` tests send it twice). **It cannot pass on an earlier
session's line**: a chat's number is used again when a plane is opened again, so a marker from
another chat, another conversation, a future time, or more than five minutes ago is removed
unsent. On Claude Code the socket is reached, so no marker is left. If one is left anyway, the
Stop's line is a harmless duplicate.

**What would not have fixed it** is in #517: opening the sandbox's network, changing the
approval policy, or writing an execpolicy rule into the operator's `CODEX_HOME`.

**Verified live** against codex-cli 0.147.0 with a stand-in model server in a scratch
`CODEX_HOME`: the chat ran the real `purlis session record` in its sandbox, the connect was
refused and the marker left, and the `Stop` hook sent the line the tab closes on.

